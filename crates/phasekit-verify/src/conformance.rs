//! The conformance kit every model family inherits (Extensible graft): a new family dev-depends on this
//! crate and runs these checks unchanged. Identity-based, so they need no oracle.

use phasekit_core::batch::{self, BatchRequest, BatchSummary, ExecPolicy, Status};
use phasekit_core::{
    Basis, Error, FlashOptions, Fluid, Gauge, HelmholtzModel, Input, Order, Pair, Prop, RootPolicy, State, Var,
};

use crate::ToleranceClass;
use crate::flash::{coordinates, disagreement, window_of};

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

/// ROT-212 (PLAN.md M11.7): T (∂s/∂T)_p = c_p at each single-phase state `inputs` flash to, with (∂s/∂T)_p by central
/// differences of the entropy flashed along the isobar (step 1e-5 T), within 1e-7 of c_p. The derivative comes from the
/// states, not from the bundle's relations, so an ideal part whose T-derivatives lack its reducing state's chain rule
/// fails it by percent, as v8.0.0's cubics do (map 06 C1, DIV-0010: 91.35 against 93.89 J/(mol K) for PR n-Propane at
/// 400 K and 1 bar). Two-phase states are skipped; a failed flash is a mismatch.
pub fn entropy_identity(fluid: &Fluid, inputs: &[Input]) -> Result<(), Mismatch> {
    let failed = |at: usize| Mismatch { what: "flash", at, got: f64::NAN, want: 0.0 };
    for (at, &input) in inputs.iter().enumerate() {
        let state = fluid.state(input).map_err(|_| failed(at))?;
        if state.quality().is_some() {
            continue;
        }
        let (t, p, h) = (state.t(), state.p(), 1e-5 * state.t());
        let s = |t: f64| {
            let input = Input::new(Pair::PT, p, t, Basis::Molar).map_err(|_| failed(at))?;
            fluid.state(input).map(|s| s.s(Basis::Molar)).map_err(|_| failed(at))
        };
        let got = t * (s(t + h)? - s(t - h)?) / (2.0 * h);
        let want = state.cp(Basis::Molar).map_err(|_| failed(at))?;
        let within = (got - want).abs() <= 1e-7 * want.abs(); // false for NaN
        if !within {
            return Err(Mismatch { what: "T(ds/dT)p", at, got, want });
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

/// Every executor must reproduce `Reference` bitwise, values and statuses (K17), and so its summary.
pub fn policy_equivalence(fluid: &Fluid, req: &BatchRequest<'_>, candidate: ExecPolicy) -> Result<(), Mismatch> {
    let shape = Mismatch { what: "batch shape", at: 0, got: f64::NAN, want: 0.0 };
    let cells = req.cells().ok_or(shape.clone())?;
    let run = |exec| -> Result<(Vec<f64>, Vec<Status>, BatchSummary), Error> {
        let (mut out, mut status) = (vec![0.0; cells], vec![Status::Ok; cells]);
        let summary = batch::evaluate(fluid, &req.with_exec(exec), &mut out, &mut status)?;
        Ok((out, status, summary))
    };
    let (want, want_s, want_summary) = run(ExecPolicy::Reference).map_err(|_| shape.clone())?;
    let (got, got_s, got_summary) = run(candidate).map_err(|_| shape)?;
    let mut diffs = got.iter().zip(&want).zip(got_s.iter().zip(&want_s)).enumerate();
    if let Some((at, ((&got, &want), _))) = diffs.find(|(_, ((g, w), (gs, ws)))| g.to_bits() != w.to_bits() || gs != ws)
    {
        return Err(Mismatch { what: "cell", at, got, want });
    }
    if got_summary != want_summary {
        let count = |s: BatchSummary| s.failed_cells as f64;
        return Err(Mismatch { what: "failed cells", at: 0, got: count(got_summary), want: count(want_summary) });
    }
    Ok(())
}

/// VERIFICATION.md §10 (Threads): `threads` threads share one fluid fresh from `make`, released together into its first
/// use (so into whatever it computes once, as a mixture's critical points), and each flashes `inputs` to the bits of p,
/// h and Q, or of the refusal, that a lone fluid from `make` gives on the calling thread (class `Exact`). The first
/// input that differs on any thread is the mismatch, its p (NaN for a refusal); a thread that panics is `"panic"`.
#[cfg(not(target_family = "wasm"))] // wasip2 has no thread spawning
pub fn thread_invariance(make: impl Fn() -> Fluid, inputs: &[Input], threads: usize) -> Result<(), Mismatch> {
    let m = Basis::Molar;
    let bits = |fluid: &Fluid| -> Vec<Result<[u64; 3], Error>> {
        let of = |s: State| [s.p(), s.h(m), s.quality().unwrap_or(f64::NAN)].map(f64::to_bits);
        inputs.iter().map(|&input| fluid.state(input).map(of)).collect()
    };
    let want = bits(&make());
    let shared = make();
    let barrier = std::sync::Barrier::new(threads);
    let got: Vec<_> = std::thread::scope(|s| {
        let racing: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    barrier.wait();
                    bits(&shared)
                })
            })
            .collect();
        racing.into_iter().map(std::thread::ScopedJoinHandle::join).collect()
    });
    let p = |cell: &Result<[u64; 3], Error>| cell.as_ref().map_or(f64::NAN, |b| f64::from_bits(b[0]));
    for thread in got {
        let thread = thread.map_err(|_| Mismatch { what: "panic", at: 0, got: f64::NAN, want: 0.0 })?;
        if let Some(at) = thread.iter().zip(&want).position(|(g, w)| g != w) {
            return Err(Mismatch { what: "thread", at, got: p(&thread[at]), want: p(&want[at]) });
        }
    }
    Ok(())
}

/// One cell of the capability matrix (VERIFICATION.md §8.5): a pair's inputs read off a truth state, flashed with the
/// default options.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The truth within class `Flash`: the state returned, or under `Ambiguous` one of its roots, each taken by
    /// `RootPolicy::Nearest`.
    Pass,
    /// Another state: its first coordinate that differs from the truth's (phase, Q, T or ρ).
    Fail(Mismatch),
    /// The model does not declare the pair.
    Unsupported,
    /// A typed refusal: the flash's, an `Ambiguous` none of whose roots is the truth, or the truth's own where it does
    /// not define the pair's inputs (a single phase has no quality).
    Refused(Error),
}

/// VERIFICATION.md §8.5 (ROT-081, ROT-129): every pair of [`Pair::ALL`] round-tripped through a single-phase truth and a
/// two-phase truth, `truth` in that order. Each cell reads the pair's inputs off the truth (molar), flashes them, and
/// compares the state's phase, T, ρ and quality with the truth's in class `Flash` (its near-critical bounds in the
/// window of the published critical point), so a round trip asserts the state it recovers, not only that one came back.
pub fn capability_matrix(fluid: &Fluid, truth: [&State; 2]) -> Vec<(Pair, [Outcome; 2])> {
    Pair::ALL.iter().map(|&pair| (pair, truth.map(|truth| round_trip(fluid, pair, truth)))).collect()
}

/// One cell of [`capability_matrix`].
fn round_trip(fluid: &Fluid, pair: Pair, truth: &State) -> Outcome {
    let input = match input_of(pair, truth) {
        Ok(input) => input,
        Err(e) => return Outcome::Refused(e),
    };
    let check =
        |back: &State| match disagreement(coordinates(back), coordinates(truth), window_of(fluid, truth), (0.0, 0.0)) {
            Some((what, got, want)) => Err(Mismatch { what, at: 0, got, want }),
            None => Ok(()),
        };
    match fluid.state(input) {
        Ok(back) => check(&back).map_or_else(Outcome::Fail, |()| Outcome::Pass),
        Err(Error::Unsupported { .. }) => Outcome::Unsupported,
        Err(Error::Ambiguous { roots }) => {
            let nearest = |root: f64| FlashOptions::new().with_roots(RootPolicy::Nearest(root));
            let found = roots
                .as_slice()
                .iter()
                .any(|&root| fluid.flash(input, &nearest(root)).is_ok_and(|s| check(&s).is_ok()));
            if found { Outcome::Pass } else { Outcome::Refused(Error::Ambiguous { roots }) }
        }
        Err(e) => Outcome::Refused(e),
    }
}

/// The input of `pair` read off `state`, molar; a single phase has no quality (`Undefined`).
pub fn input_of(pair: Pair, state: &State) -> Result<Input, Error> {
    let m = Basis::Molar;
    let value = |var: Var| match var {
        Var::T => Ok(state.t()),
        Var::P => Ok(state.p()),
        Var::Q => state.quality().ok_or(Error::Undefined { prop: Prop::Q, phase: state.phase() }),
        Var::D => Ok(state.rho(m)),
        Var::H => Ok(state.h(m)),
        Var::S => Ok(state.s(m)),
        Var::U => Ok(state.u(m)),
        _ => Err(Error::Unsupported { pair }),
    };
    let (x, y) = pair.vars();
    Input::new(pair, value(x)?, value(y)?, m)
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

    /// The toy fluid, refusing every flash off the thread that built it, so that a lone fluid and threads disagree.
    #[cfg(not(target_family = "wasm"))]
    #[derive(Debug)]
    struct HomeOnly(Fluid, std::thread::ThreadId);

    #[cfg(not(target_family = "wasm"))]
    impl phasekit_core::ThermoModel for HomeOnly {
        fn info(&self) -> &phasekit_core::FluidInfo {
            self.0.model().info()
        }
        fn capabilities(&self) -> phasekit_core::Capabilities {
            self.0.model().capabilities()
        }
        fn flash(&self, input: phasekit_core::NativeInput, opts: &FlashOptions) -> Result<State, Error> {
            if std::thread::current().id() == self.1 {
                self.0.model().flash(input, opts)
            } else {
                Err(Error::Unsupported { pair: input.pair() })
            }
        }
    }

    /// VERIFICATION.md §10 (Threads): Water's states and refusal (above its p_max) on 4 threads sharing one handle are
    /// a lone handle's; a fluid that refuses off its own thread fails at its first input, naming the input and both p.
    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn threads_give_the_lone_fluids_bits() {
        let registry = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let pt = |p: f64, t: f64| Input::new(Pair::PT, p, t, Basis::Molar).unwrap();
        let inputs = [pt(1e5, 300.0), pt(1e5, 500.0), pt(1e10, 300.0)];
        assert!(water.state(inputs[2]).is_err());
        assert_eq!(thread_invariance(|| water.clone(), &inputs, 4), Ok(()));
        let home = || Fluid::new(Arc::new(HomeOnly(water.clone(), std::thread::current().id())));
        let mismatch = thread_invariance(home, &inputs, 4).unwrap_err();
        let want = water.state(inputs[0]).unwrap().p();
        assert!(mismatch.got.is_nan() && mismatch.want == want, "{mismatch:?}");
        assert_eq!((mismatch.what, mismatch.at), ("thread", 0));
        assert_eq!(thread_invariance(home, &inputs, 0), Ok(()), "no thread, nothing to compare");
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
