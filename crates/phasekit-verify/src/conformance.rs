//! The conformance kit every model family inherits (Extensible graft): a new family dev-depends on this
//! crate and runs these checks unchanged. Identity-based, so they need no oracle.

use phasekit_core::batch::{self, BatchRequest, BatchSummary, ExecPolicy, Status};
use phasekit_core::{
    Basis, DerivVar, Derivs, Error, FlashOptions, Fluid, Gauge, HelmholtzModel, Input, Order, Pair, Partial, Prop,
    RootPolicy, SecondPartial, State, Var,
};

use crate::flash::{coordinates, disagreement, window_of};
use crate::{ToleranceClass, Window};

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

/// VERIFICATION.md §8.1 (PLAN.md M20.2): the L4 identities in class `Identity` (1e-12 of the largest term, 1e-8 in the
/// near-critical window of the model's critical point) at each single-phase state of `fluid` in `states`, each side
/// computed from the bundle along different algebra (a first partial's terms are the two summands of its Jacobian
/// ratio). First order: h = u + p/ρ, g = h − Ts, a = u − Ts, μ = a + p/ρ = g (a pure fluid), Z = p/(ρRT), (∂g/∂T)_p =
/// −s and, where c_p and c_v are defined, c_p − c_v = T(∂p/∂T)²_ρ/(ρ²(∂p/∂ρ)_T), (∂h/∂T)_p = c_p and (∂s/∂T)_p = c_p/T;
/// w² = (∂p/∂ρ)_s (mass density), or (∂p/∂ρ)_s ≤ 0 where w is refused. From the model's order-3 bundle, where it gives
/// one (M7.10): the mixed second partial of p, Maxwell's (∂s/∂ρ)_T differentiated in T, (∂c_v/∂ρ)_T =
/// −T(∂²p/∂T²)_ρ/ρ² and (∂c_p/∂p)_T = −T(∂²v/∂T²)_p from the second partials of s against those of p and v,
/// β/κ_T = (∂p/∂T)_ρ, and where c_p, c_v and w are defined c_p − c_v = Tβ²/(ρκ_T) and κ_T·ρ·w² = c_p/c_v. A state with
/// a quality is skipped. How many held, or the first that did not, at its state's index.
pub fn identities(fluid: &Fluid, states: &[State]) -> Result<usize, Mismatch> {
    let mut checked = 0;
    for (at, state) in states.iter().enumerate().filter(|(_, s)| s.quality().is_none()) {
        let mut ledger = Ledger::new(fluid, state, at);
        first_order_identities(&mut ledger, fluid, state)?;
        order_three_identities(&mut ledger, fluid, state)?;
        checked += ledger.held;
    }
    Ok(checked)
}

/// One state's checks in class `Identity`: its index, its window and how many held.
struct Ledger {
    at: usize,
    window: Window,
    held: usize,
}

impl Ledger {
    /// The ledger of `state`, the `at`-th, in the window of `fluid`'s critical point (regular without one).
    fn new(fluid: &Fluid, state: &State, at: usize) -> Ledger {
        let (t, rho) = (state.t(), state.rho(Basis::Molar));
        let window = fluid.model().critical_point().map_or(Window::Regular, |c| Window::at(t, rho, c.t, c.rho));
        Ledger { at, window, held: 0 }
    }

    /// `lhs = rhs` within `Identity` of the largest of the two and of `terms`, the magnitudes of its summands. An
    /// infinite side or term fails: its bound would be infinite too.
    fn hold(&mut self, what: &'static str, (lhs, rhs): (f64, f64), terms: &[f64]) -> Result<(), Mismatch> {
        let largest = terms.iter().fold(lhs.abs().max(rhs.abs()), |m, x| m.max(x.abs()));
        let bound = ToleranceClass::Identity.bound_in(largest, self.window).unwrap_or(0.0);
        if (lhs - rhs).abs() <= bound && bound.is_finite() {
            self.held += 1;
            Ok(())
        } else {
            Err(Mismatch { what, at: self.at, got: lhs, want: rhs }) // NaN included
        }
    }
}

/// [`identities`]' first-order list at one single-phase `state`.
fn first_order_identities(ledger: &mut Ledger, fluid: &Fluid, state: &State) -> Result<(), Mismatch> {
    use DerivVar::{Dmass, Gmolar, P, Smolar, T};
    let molar = Basis::Molar;
    let (t, p, rho, r) = (state.t(), state.p(), state.rho(molar), state.gas_constant());
    let (h, s, u, g, a) = (state.h(molar), state.s(molar), state.u(molar), state.g(molar), state.a(molar));
    let d = |of, wrt, at| state.partial(Partial { of, wrt, at }).unwrap_or(f64::NAN);
    let (pv, ts) = (p / rho, t * s);
    ledger.hold("h = u + p/ρ", (h, u + pv), &[u, pv])?;
    ledger.hold("g = h − Ts", (g, h - ts), &[h, ts])?;
    ledger.hold("a = u − Ts", (a, u - ts), &[u, ts])?;
    ledger.hold("μ = a + p/ρ = g", (a + pv, g), &[a, pv])?;
    ledger.hold("Z = p/(ρRT)", (state.z(), p / (rho * r * t)), &[])?;
    ledger.hold("(∂g/∂T)_p = −s", (d(Gmolar, T, P), -s), &along_p(state, Gmolar))?;
    caloric_identities(ledger, state)?;
    let dpds = d(P, Dmass, Smolar);
    let terms = along_s(state, fluid.info().molar_mass());
    match state.speed_of_sound() {
        Ok(w) => ledger.hold("w² = (∂p/∂ρ)_s", (w * w, dpds), &terms),
        Err(_) => ledger.hold("refused w: (∂p/∂ρ)_s ≤ 0", (dpds.max(0.0), 0.0), &terms),
    }
}

/// The two terms of (∂x/∂T)_p = x_T − x_ρ·p_T/p_ρ at `state`.
fn along_p(state: &State, x: DerivVar) -> [f64; 2] {
    use DerivVar::{Dmolar, P, T};
    let d = |of, wrt, at| state.partial(Partial { of, wrt, at }).unwrap_or(f64::NAN);
    [d(x, T, Dmolar), d(x, Dmolar, T) * d(P, T, Dmolar) / d(P, Dmolar, T)]
}

/// The two terms of (∂p/∂ρ)_s = (p_ρ − p_T·s_ρ/s_T)/M in mass density at `state`, `m` the molar mass.
fn along_s(state: &State, m: f64) -> [f64; 2] {
    use DerivVar::{Dmolar, P, Smolar, T};
    let d = |of, wrt, at| state.partial(Partial { of, wrt, at }).unwrap_or(f64::NAN);
    [d(P, Dmolar, T) / m, d(P, T, Dmolar) * d(Smolar, Dmolar, T) / d(Smolar, T, Dmolar) / m]
}

/// c_p − c_v = T(∂p/∂T)²_ρ/(ρ²(∂p/∂ρ)_T), (∂h/∂T)_p = c_p and (∂s/∂T)_p = c_p/T, where c_p and c_v are defined.
fn caloric_identities(ledger: &mut Ledger, state: &State) -> Result<(), Mismatch> {
    use DerivVar::{Dmolar, Hmolar, P, Smolar, T};
    let molar = Basis::Molar;
    let (Ok(cv), Ok(cp)) = (state.cv(molar), state.cp(molar)) else { return Ok(()) };
    let (t, rho) = (state.t(), state.rho(molar));
    let d = |of, wrt, at| state.partial(Partial { of, wrt, at }).unwrap_or(f64::NAN);
    let p_t = d(P, T, Dmolar);
    let rhs = t * p_t * p_t / (rho * rho * d(P, Dmolar, T));
    ledger.hold("cp − cv = T(∂p/∂T)²_ρ/(ρ²(∂p/∂ρ)_T)", (cp - cv, rhs), &[cp, cv])?;
    ledger.hold("(∂h/∂T)_p = cp", (d(Hmolar, T, P), cp), &along_p(state, Hmolar))?;
    ledger.hold("(∂s/∂T)_p = cp/T", (d(Smolar, T, P), cp / t), &along_p(state, Smolar))
}

/// [`identities`]' order-3 list at one single-phase `state`, where `fluid`'s model gives the order-3 bundle.
fn order_three_identities(ledger: &mut Ledger, fluid: &Fluid, state: &State) -> Result<(), Mismatch> {
    use DerivVar::{Dmolar, P, Smolar, T};
    let molar = Basis::Molar;
    let second = |of, wrt, at, wrt2, at2| {
        let p = SecondPartial { first: Partial { of, wrt, at }, wrt: wrt2, at: at2 };
        fluid.prop(state, Prop::SecondPartial(p))
    };
    let Ok(p_tt) = second(P, T, Dmolar, T, Dmolar) else { return Ok(()) };
    let d2 = |of, wrt, at, wrt2, at2| second(of, wrt, at, wrt2, at2).unwrap_or(f64::NAN);
    let d = |of, wrt, at| state.partial(Partial { of, wrt, at }).unwrap_or(f64::NAN);
    let (t, rho) = (state.t(), state.rho(molar));
    ledger.hold("∂²p/∂T∂ρ = ∂²p/∂ρ∂T", (d2(P, T, Dmolar, Dmolar, T), d2(P, Dmolar, T, T, Dmolar)), &[])?;
    ledger.hold("∂²s/∂ρ∂T = −(∂²p/∂T²)_ρ/ρ²", (d2(Smolar, Dmolar, T, T, Dmolar), -p_tt / (rho * rho)), &[])?;
    let cv_rho = t * d2(Smolar, T, Dmolar, Dmolar, T);
    ledger.hold("(∂cv/∂ρ)_T = −T(∂²p/∂T²)_ρ/ρ²", (cv_rho, -t * p_tt / (rho * rho)), &[])?;
    let (rho_t, rho_tt) = (d(Dmolar, T, P), d2(Dmolar, T, P, T, P));
    let v_tt = [-rho_tt / (rho * rho), 2.0 * rho_t * rho_t / (rho * rho * rho)];
    let cp_p = t * d2(Smolar, T, P, P, T);
    let rhs = v_tt.map(|v| -t * v);
    ledger.hold("(∂cp/∂p)_T = −T(∂²v/∂T²)_p", (cp_p, rhs[0] + rhs[1]), &rhs)?;
    let (kappa, beta) = (state.isothermal_compressibility(), state.isobaric_expansion());
    let (kappa, beta) = (kappa.unwrap_or(f64::NAN), beta.unwrap_or(f64::NAN));
    ledger.hold("β/κ_T = (∂p/∂T)_ρ", (beta / kappa, d(P, T, Dmolar)), &[])?;
    let (Ok(cv), Ok(cp)) = (state.cv(molar), state.cp(molar)) else { return Ok(()) };
    ledger.hold("cp − cv = Tβ²/(ρκ_T)", (cp - cv, t * beta * beta / (rho * kappa)), &[cp, cv])?;
    let Ok(w) = state.speed_of_sound() else { return Ok(()) };
    ledger.hold("κ_T·ρ·w² = cp/cv", (kappa * state.rho(Basis::Mass) * w * w, cp / cv), &[])
}

/// VERIFICATION.md §10 (PLAN.md M20.2): `ThermoModel::derivs`' ideal and residual parts add up to the total each
/// single-phase state of `states` carries, so a family whose parts disagree with its flash fails (`derivs`' own total
/// is their sum by construction, `PointDerivs::split`). In class `Identity`, of the larger of the two parts' terms:
/// the six order-2 sums against the state through their relations, p = ρRT·A01, (∂p/∂ρ)_T = RT(2A01 + A02),
/// (∂p/∂T)_ρ = ρR(A01 − A11) and c_v = −R·A20 from the summed entries, and h = h⁰ + h^r and s = s⁰ + s^r through
/// `Fluid::prop`, which reads A10 and A00 of each part. A state with a quality, or whose model gives no parts, is
/// skipped. How many held, or the first that did not, at its state's index.
pub fn part_additivity(fluid: &Fluid, states: &[State]) -> Result<usize, Mismatch> {
    let mut checked = 0;
    for (at, state) in states.iter().enumerate().filter(|(_, s)| s.quality().is_none()) {
        let Some(parts) = fluid.model().derivs(state, Order::Two) else { continue };
        let (Some(ideal), Some(residual)) = (parts.ideal(), parts.residual()) else { continue };
        let mut ledger = Ledger::new(fluid, state, at);
        order_two_sums(&mut ledger, state, [&ideal, &residual])?;
        caloric_sums(&mut ledger, fluid, state)?;
        checked += ledger.held;
    }
    Ok(checked)
}

/// p, (∂p/∂ρ)_T, (∂p/∂T)_ρ and c_v at `state` against the relations on the sum of the two parts' entries.
fn order_two_sums(ledger: &mut Ledger, state: &State, parts: [&Derivs; 2]) -> Result<(), Mismatch> {
    use DerivVar::{Dmolar, P, T};
    let (r, t, rho) = (state.gas_constant(), state.t(), state.rho(Basis::Molar));
    let a = |part: &Derivs, i, j| part.get(i, j).unwrap_or(f64::NAN);
    // Each part's term of a relation linear in the A_ij.
    let by_part = |f: &dyn Fn(&Derivs) -> f64| parts.map(f);
    let d = |of, wrt, at| state.partial(Partial { of, wrt, at }).unwrap_or(f64::NAN);
    let p = by_part(&|x| rho * r * t * a(x, 0, 1));
    ledger.hold("p = ρRT·A01", (state.p(), p[0] + p[1]), &p)?;
    let p_rho = by_part(&|x| r * t * (2.0 * a(x, 0, 1) + a(x, 0, 2)));
    ledger.hold("(∂p/∂ρ)_T = RT(2A01 + A02)", (d(P, Dmolar, T), p_rho[0] + p_rho[1]), &p_rho)?;
    let p_t = by_part(&|x| rho * r * (a(x, 0, 1) - a(x, 1, 1)));
    ledger.hold("(∂p/∂T)_ρ = ρR(A01 − A11)", (d(P, T, Dmolar), p_t[0] + p_t[1]), &p_t)?;
    let cv = by_part(&|x| -r * a(x, 2, 0));
    ledger.hold("cv = −R·A20", (state.cv(Basis::Molar).unwrap_or(f64::NAN), cv[0] + cv[1]), &cv)
}

/// h = h⁰ + h^r and s = s⁰ + s^r at `state`, each part's from `Fluid::prop`.
fn caloric_sums(ledger: &mut Ledger, fluid: &Fluid, state: &State) -> Result<(), Mismatch> {
    let prop = |p| fluid.prop(state, p).unwrap_or(f64::NAN);
    let h = [prop(Prop::HmolarIdealGas), prop(Prop::HmolarResidual)];
    ledger.hold("h = h⁰ + h^r", (state.h(Basis::Molar), h[0] + h[1]), &h)?;
    let s = [prop(Prop::SmolarIdealGas), prop(Prop::SmolarResidual)];
    ledger.hold("s = s⁰ + s^r", (state.s(Basis::Molar), s[0] + s[1]), &s)
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

    /// A miss within `Identity`'s 1e-12 of the largest side or term holds and counts; a larger miss, a NaN or an
    /// infinite side or term fails by name at the ledger's state, so a term that divides by a zero entry cannot widen
    /// its own bound to infinity. Water's liquid at 1 bar and 300 K, in the regular window.
    #[test]
    fn a_ledger_holds_within_its_class_and_never_with_an_infinity() {
        let registry = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let state = water.state(Input::new(Pair::PT, 1e5, 300.0, Basis::Molar).unwrap()).unwrap();
        let mut ledger = Ledger::new(water, &state, 3);
        assert_eq!(ledger.hold("x", (1.0, 1.0 + 1e-13), &[]), Ok(()));
        assert_eq!(ledger.hold("x", (1.0, 1.0 + 1e-11), &[-100.0]), Ok(()), "the largest term's bound");
        let fails = [(1.0 + 1e-11, 0.0), (f64::INFINITY, 0.0), (1.0, f64::INFINITY), (f64::NAN, 0.0)];
        for (rhs, term) in fails {
            let got = ledger.hold("y", (1.0, rhs), &[term]).map_err(|m| (m.what, m.at));
            assert_eq!(got, Err(("y", 3)), "{rhs} against 1 with a term of {term}");
        }
        assert_eq!(ledger.held, 2);
    }

    /// A first partial's terms are the two summands of its Jacobian ratio, so the first less the second is the partial
    /// in class `Identity`: (∂x/∂T)_p of g, h and s and (∂p/∂ρ)_s in mass density, at Water's liquid at 1 bar, 300 K.
    #[test]
    fn a_partials_two_terms_make_it_up() {
        use DerivVar::{Dmass, Gmolar, Hmolar, P, Smolar, T};
        let registry = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let state = water.state(Input::new(Pair::PT, 1e5, 300.0, Basis::Molar).unwrap()).unwrap();
        let partial = |of, wrt, at| state.partial(Partial { of, wrt, at }).unwrap();
        let mut ledger = Ledger::new(water, &state, 0);
        for x in [Gmolar, Hmolar, Smolar] {
            let terms = along_p(&state, x);
            assert_eq!(ledger.hold("(∂x/∂T)_p", (terms[0] - terms[1], partial(x, T, P)), &terms), Ok(()), "{x:?}");
        }
        let terms = along_s(&state, water.info().molar_mass());
        assert_eq!(ledger.hold("(∂p/∂ρ)_s", (terms[0] - terms[1], partial(P, Dmass, Smolar)), &terms), Ok(()));
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
