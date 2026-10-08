//! The conformance kit every model family inherits (Extensible graft): a new family dev-depends on this
//! crate and runs these checks unchanged. Identity-based, so they need no oracle.

use phasekit_core::batch::{self, BatchRequest, ExecPolicy, Status};
use phasekit_core::{Basis, Error, Fluid, Gauge, HelmholtzModel, Input, Order, State};

use crate::ToleranceClass;

/// A failed check: which quantity, at which point, got vs expected.
#[derive(Clone, Debug, PartialEq)]
pub struct Mismatch {
    /// What was compared.
    pub what: &'static str,
    /// Index or point.
    pub at: usize,
    /// Value under test.
    pub got: f64,
    /// Expected value.
    pub want: f64,
}

/// Analytic `A10`, `A01` of the residual against central differences in the DIMENSIONAL variables
/// (`A10 = −T ∂α/∂T`, `A01 = ρ ∂α/∂ρ`), so a family's reducing choices cannot hide an error.
pub fn fd_first_order<M: HelmholtzModel + ?Sized>(
    m: &M,
    t: f64,
    rho: f64,
    rel_step: f64,
    tol: f64,
) -> Result<(), Mismatch> {
    let a = |t, rho| m.residual(t, rho, Order::One).get(0, 0).unwrap_or(f64::NAN);
    let d = m.residual(t, rho, Order::One);
    let (ht, hr) = (t * rel_step, rho * rel_step);
    let fd10 = -t * (a(t + ht, rho) - a(t - ht, rho)) / (2.0 * ht);
    let fd01 = rho * (a(t, rho + hr) - a(t, rho - hr)) / (2.0 * hr);
    for (what, got, want) in [("A10", d.get(1, 0), fd10), ("A01", d.get(0, 1), fd01)] {
        let got = got.unwrap_or(f64::NAN);
        let within = (got - want).abs() <= tol * want.abs().max(1e-300); // false for NaN
        if !within {
            return Err(Mismatch { what, at: 0, got, want });
        }
    }
    Ok(())
}

/// Map 15 §8 at every input: the handle in `gauge` and the handle as given flash the same state, except that h and u
/// move by exactly Δh, s by Δs and g and a by Δh − TΔs (the difference of the two gauges); see [`gauged_pair`].
pub fn gauge_invariance(fluid: &Fluid, gauge: Gauge, inputs: &[Input]) -> Result<(), Mismatch> {
    pairs_hold(fluid, gauge, inputs, gauged_pair)
}

/// [`gauge_invariance`] with the pair check `check`, a parameter so that a failing pair's index can be tested: with a
/// correct kernel [`gauged_pair`] never fails here.
fn pairs_hold(
    fluid: &Fluid,
    gauge: Gauge,
    inputs: &[Input],
    check: impl Fn(&State, &State, f64, f64) -> Result<(), Mismatch>,
) -> Result<(), Mismatch> {
    let shifted = fluid.with_gauge(gauge);
    let (dh, ds) = (gauge.dh() - fluid.gauge().dh(), gauge.ds() - fluid.gauge().ds());
    for (i, input) in inputs.iter().enumerate() {
        match (fluid.state(*input), shifted.state(*input)) {
            (Ok(a), Ok(b)) => check(&a, &b, dh, ds).map_err(|m| Mismatch { at: i, ..m })?,
            _ => return Err(Mismatch { what: "flash", at: i, got: f64::NAN, want: 0.0 }),
        }
    }
    Ok(())
}

/// One state `a` and the same state `b` in a gauge that differs by (Δh, Δs) (map 15 §8): T, p, ρ, cv, cp and w
/// (values or refusals) bit for bit (class `Exact`); h and u shifted by Δh, s by Δs, g and a by Δh − TΔs, within class
/// `Identity` (1e-12 of the largest of the two values and the shift).
pub fn gauged_pair(a: &State, b: &State, dh: f64, ds: f64) -> Result<(), Mismatch> {
    let m = Basis::Molar;
    let bits = |x: Result<f64, Error>| x.map(f64::to_bits);
    let same = [
        ("T", bits(Ok(a.t())), bits(Ok(b.t()))),
        ("p", bits(Ok(a.p())), bits(Ok(b.p()))),
        ("rho", bits(Ok(a.rho(m))), bits(Ok(b.rho(m)))),
        ("cv", bits(a.cv(m)), bits(b.cv(m))),
        ("cp", bits(a.cp(m)), bits(b.cp(m))),
        ("w", bits(a.speed_of_sound()), bits(b.speed_of_sound())),
    ];
    for (what, x, y) in same {
        if x != y {
            let value = |r: Result<u64, Error>| r.map_or(f64::NAN, f64::from_bits);
            return Err(Mismatch { what, at: 0, got: value(y), want: value(x) });
        }
    }
    let t = a.t();
    let shifted = [
        ("h", a.h(m), b.h(m), dh),
        ("u", a.u(m), b.u(m), dh),
        ("s", a.s(m), b.s(m), ds),
        ("g", a.g(m), b.g(m), dh - t * ds),
        ("a", a.a(m), b.a(m), dh - t * ds),
    ];
    for (what, x, y, shift) in shifted {
        let largest = x.abs().max(y.abs()).max(shift.abs());
        let bound = ToleranceClass::Identity.bound(largest).unwrap_or(0.0);
        let miss = (y - x - shift).abs();
        if miss.is_nan() || miss > bound {
            return Err(Mismatch { what, at: 0, got: y - x, want: shift });
        }
    }
    Ok(())
}

/// Every executor must reproduce `Reference` bitwise, values and statuses (K17).
pub fn policy_equivalence(fluid: &Fluid, req: &BatchRequest<'_>, candidate: ExecPolicy) -> Result<(), Mismatch> {
    let shape = Mismatch { what: "batch shape", at: 0, got: f64::NAN, want: 0.0 };
    let cells = req.cells().ok_or(shape.clone())?;
    let run = |exec| -> Result<(Vec<f64>, Vec<Status>), Error> {
        let (mut out, mut status) = (vec![0.0; cells], vec![Status::Ok; cells]);
        batch::evaluate(fluid, &req.with_exec(exec), &mut out, &mut status)?;
        Ok((out, status))
    };
    let (want, want_s) = run(ExecPolicy::Reference).map_err(|_| shape.clone())?;
    let (got, got_s) = run(candidate).map_err(|_| shape)?;
    let mut diffs = got.iter().zip(&want).zip(got_s.iter().zip(&want_s)).enumerate();
    match diffs.find(|(_, ((g, w), (gs, ws)))| g.to_bits() != w.to_bits() || gs != ws) {
        Some((at, ((&got, &want), _))) => Err(Mismatch { what: "cell", at, got, want }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use phasekit_core::internal::FluidRecord;
    use phasekit_core::{Density, Enthalpy, Entropy, FlashOptions, Phase, ReferenceState, Temperature};

    use super::*;

    /// The toy fluid's liquid at (T, 5000 mol/m³) in the handle's gauge.
    fn liquid(fluid: &Fluid, t: f64) -> State {
        let input = Input::dt(Density::molar(5_000.0).unwrap(), Temperature::new(t).unwrap());
        fluid.flash(input, &FlashOptions::new().with_phase(Phase::Liquid)).unwrap()
    }

    /// Map 15 §8: the same state in two gauges passes with the right shifts and fails, by name, with a wrong Δh or Δs
    /// or against another state; the handle-level check runs it at every input.
    #[test]
    fn gauged_pairs_shift_exactly_and_change_nothing_else() {
        let fluid = Fluid::new(Arc::new(FluidRecord::synthetic("X").unwrap().compile().unwrap()));
        let gauge = Gauge::new(1_500.0, 4.0).unwrap();
        let (a, b) = (liquid(&fluid, 300.0), liquid(&fluid.with_gauge(gauge), 300.0));
        assert_eq!(gauged_pair(&a, &b, 1_500.0, 4.0), Ok(()));
        let (got, m) = (b.h(Basis::Molar) - a.h(Basis::Molar), Mismatch { what: "h", at: 0, got: 0.0, want: 1_500.5 });
        assert_eq!(gauged_pair(&a, &b, 1_500.5, 4.0), Err(Mismatch { got, ..m }), "the difference and the shift");
        assert_eq!(gauged_pair(&a, &b, 1_500.0, 4.001).map_err(|m| m.what), Err("s"));
        assert_eq!(gauged_pair(&a, &liquid(&fluid, 301.0), 0.0, 0.0).map_err(|m| m.what), Err("T"));
        let inputs = [Input::dt(Density::molar(5_000.0).unwrap(), Temperature::new(300.0).unwrap())];
        assert_eq!(gauge_invariance(&fluid, gauge, &inputs).map_err(|m| m.what), Err("flash"));
    }

    /// A quantity that is exactly 0 in both gauges with no shift has a bound of 0 and a miss of 0, which is within it:
    /// Water at a custom reference state's anchor (supercritical), whose h is set to 0 there.
    #[test]
    fn a_zero_miss_is_within_a_zero_bound() {
        let registry = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let at = Input::dt(Density::molar(5_000.0).unwrap(), Temperature::new(700.0).unwrap());
        let zero = (Enthalpy::molar(0.0).unwrap(), Entropy::molar(0.0).unwrap());
        let reference = ReferenceState::Custom { at, h: zero.0, s: zero.1 };
        let state = registry.get("Water").unwrap().with_reference(reference).unwrap().state(at).unwrap();
        assert_eq!(state.h(Basis::Molar), 0.0, "exactly 0 at the anchor");
        assert_eq!(gauged_pair(&state, &state, 0.0, 0.0), Ok(()));
    }

    /// The index of the first input whose pair fails reaches the mismatch: Water above its critical temperature at
    /// three inputs, and a check that fails from the second pair on.
    #[test]
    fn a_failing_pair_names_its_input() {
        let registry = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let gauge = Gauge::new(1_500.0, 4.0).unwrap();
        let dt = |t| Input::dt(Density::molar(5_000.0).unwrap(), Temperature::new(t).unwrap());
        let inputs = [dt(700.0), dt(710.0), dt(720.0)];
        let late = |a: &State, _: &State, _: f64, _: f64| {
            if a.t() > 705.0 { Err(Mismatch { what: "late", at: 0, got: a.t(), want: 0.0 }) } else { Ok(()) }
        };
        assert_eq!(pairs_hold(water, gauge, &inputs, late).map_err(|m| (m.what, m.at)), Err(("late", 1)));
        assert_eq!(pairs_hold(water, gauge, &inputs, gauged_pair), Ok(()));
    }
}
