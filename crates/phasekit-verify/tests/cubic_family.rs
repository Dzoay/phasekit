//! The conformance kit on the cubic family (PLAN.md M11.6; VERIFICATION.md §10): every vdW, SRK and PR fluid built by
//! `phasekit_cubic::with_cubics` through the same checks as any other family, in both datasets. ROT-213, ROT-214 (the
//! dimensional seams), ROT-220 (no cubic root is solved) and ROT-107 (R scales, it does not shift).

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::num::NonZeroUsize;

use phasekit_core::batch::{BatchRequest, ExecPolicy};
use phasekit_core::{
    Basis, DataSet, DomainError, Error, FlashOptions, Fluid, Gauge, HelmholtzModel, Input, Order, Pair, Phase, Prop,
    Registry, Var,
};
use phasekit_cubic::{Kind, R_CODATA, with_cubics};
use phasekit_verify::conformance::{Outcome, capability_matrix, fd_first_order, gauge_invariance, policy_equivalence};

const KINDS: [Kind; 3] = [Kind::Vdw, Kind::Srk, Kind::Pr];
const SOURCES: [&str; 3] = ["n-Propane", "Water", "CarbonDioxide"];

fn registry(data: DataSet) -> Registry {
    with_cubics(&Registry::from_embedded(data).unwrap(), &KINDS, data).unwrap()
}

/// Each kind's fluid of each source.
fn cubics(registry: &Registry) -> impl Iterator<Item = (String, &Fluid)> {
    KINDS.into_iter().flat_map(move |kind| {
        SOURCES.map(|name| {
            let name = format!("{}::{name}", kind.prefix());
            let fluid = registry.get(&name).unwrap();
            (name, fluid)
        })
    })
}

fn molar(pair: Pair, x: f64, y: f64) -> Input {
    Input::new(pair, x, y, Basis::Molar).unwrap()
}

/// The inputs of the two truths: supercritical (0.6 ρ_c, 1.2 T_c) and in the dome (ρ_c, 0.8 T_c), both DT, which every
/// kind flashes (vdW, without a saturation curve, through the core's VLE).
fn truth_inputs(fluid: &Fluid) -> [Input; 2] {
    let c = fluid.model().critical_point().unwrap();
    [molar(Pair::DT, 0.6 * c.rho, 1.2 * c.t), molar(Pair::DT, c.rho, 0.8 * c.t)]
}

/// Whether one cell of the matrix is as expected (`tests/common/capability.rs`'s table for a pure fluid): a pass; for a
/// quality pair, `Undefined` from the single-phase truth (it has no quality) or, for van der Waals, `Unsupported` (the
/// core declares the quality pairs only with a saturation curve, and vdW has no superancillary); and PT of the two-phase
/// truth, `Ambiguous` between its saturated sides.
fn expected(outcome: &Outcome, pair: Pair, vdw: bool, single_phase: bool) -> bool {
    let q_pair = matches!(pair.vars(), (Var::Q, _) | (_, Var::Q));
    match outcome {
        Outcome::Pass => true,
        Outcome::Unsupported => vdw && q_pair,
        Outcome::Refused(Error::Undefined { .. }) => single_phase && q_pair,
        Outcome::Refused(Error::Ambiguous { .. }) => !single_phase && pair == Pair::PT,
        _ => false,
    }
}

/// VERIFICATION.md §8.5 (ROT-213): all 19 pairs round-trip through both truths on every cubic fluid, in both datasets,
/// with the refusals [`expected`] lists.
#[test]
fn cubic_capability_matrix() {
    for data in [DataSet::Corrected, DataSet::Parity] {
        let registry = registry(data);
        for (name, fluid) in cubics(&registry) {
            let [single, two] = truth_inputs(fluid).map(|input| fluid.state(input).unwrap());
            assert!(two.quality().is_some_and(|q| 0.0 < q && q < 1.0), "{name}: {two:?}");
            let vdw = name.starts_with("VDW::");
            for (pair, [one_phase, dome]) in capability_matrix(fluid, [&single, &two]) {
                assert!(expected(&one_phase, pair, vdw, true), "{name} {data:?} {pair:?}: {one_phase:?}");
                assert!(expected(&dome, pair, vdw, false), "{name} {data:?} {pair:?} in the dome: {dome:?}");
            }
        }
    }
}

/// Map 15 §8: a reference-state gauge moves h and u by Δh, s by Δs, g and a by Δh − TΔs, and nothing else, on every
/// cubic fluid, single-phase and in the dome.
#[test]
fn cubic_gauge_invariance() {
    let registry = registry(DataSet::Corrected);
    let gauge = Gauge::new(1234.5, -6.75).unwrap();
    for (name, fluid) in cubics(&registry) {
        let result = gauge_invariance(fluid, gauge, &truth_inputs(fluid));
        assert!(result.is_ok(), "{name}: {result:?}");
    }
}

/// K17: DT, PT, QT and PQ batches of every cubic fluid under `Parallel` and `Simd` give `Reference`'s cells bit for bit.
#[test]
fn cubic_policy_equivalence() {
    let registry = registry(DataSet::Corrected);
    let outputs = [Prop::P, Prop::Hmolar, Prop::Cpmolar, Prop::Q];
    for (name, fluid) in cubics(&registry) {
        let c = fluid.model().critical_point().unwrap();
        let t: Vec<f64> = (0..37).map(|i| (0.6 + 0.02 * f64::from(i)) * c.t).collect();
        let rho: Vec<f64> = (0..37).map(|i| (0.05 + 0.05 * f64::from(i)) * c.rho).collect();
        let q: Vec<f64> = (0..37).map(|i| f64::from(i % 5) / 4.0).collect();
        let p: Vec<f64> = (0..37).map(|i| (0.05 + 0.025 * f64::from(i)) * c.p).collect();
        let requests = [
            BatchRequest::new(Pair::DT, Basis::Molar, &rho, &t, &outputs),
            BatchRequest::new(Pair::PT, Basis::Molar, &p, &t, &outputs),
            BatchRequest::new(Pair::QT, Basis::Molar, &q, &t, &outputs),
            BatchRequest::new(Pair::PQ, Basis::Molar, &p, &q, &outputs),
        ];
        for exec in [ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() }, ExecPolicy::Simd] {
            for req in &requests {
                let result = policy_equivalence(fluid, req, exec);
                assert!(result.is_ok(), "{name} {exec:?}: {result:?}");
            }
        }
    }
}

/// ROT-214: A10 and A01 of each cubic's residual against central differences in dimensional T and ρ.
#[test]
fn cubic_residual_matches_finite_differences() {
    let registry = registry(DataSet::Corrected);
    for (name, fluid) in cubics(&registry) {
        let c = fluid.model().critical_point().unwrap();
        let eos = fluid.model().helmholtz().unwrap();
        for (t, rho) in [(0.7 * c.t, 2.5 * c.rho), (c.t, c.rho), (1.5 * c.t, 0.1 * c.rho)] {
            let result = fd_first_order(eos, t, rho, 1e-6, 1e-7);
            assert!(result.is_ok(), "{name} at {t} K, {rho}: {result:?}");
        }
    }
}

/// The vapour and liquid spinodal densities of the isotherm `t` (where ∂p/∂ρ = 0, a double root of the cubic in Z), by
/// bisection on 1 + 2A01 + A02, below and above `rho_c`.
fn spinodals(eos: &dyn HelmholtzModel, t: f64, rho_c: f64) -> (f64, f64) {
    let dp = |rho: f64| {
        let d = eos.residual(t, rho, Order::Two);
        1.0 + 2.0 * d.get(0, 1).unwrap() + d.get(0, 2).unwrap()
    };
    let bisect = |mut lo: f64, mut hi: f64| {
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if (dp(mid) > 0.0) == (dp(lo) > 0.0) { lo = mid } else { hi = mid }
        }
        0.5 * (lo + hi)
    };
    (bisect(1e-3 * rho_c, rho_c), bisect(rho_c, 0.999 * eos.rho_max(t)))
}

/// ROT-220: phasekit-cubic solves no cubic for ρ; the core's PT flash finds it. Where the cubic in Z has a double root,
/// at the spinodal pressures of a subcritical isotherm, the flash returns a finite state of the stable phase; at the
/// critical point, a triple root where ∂p/∂ρ = 0, a finite state or the typed `MechanicallyUnstable`, never NaN.
#[test]
fn pt_near_a_double_root_is_finite() {
    let registry = registry(DataSet::Corrected);
    for (name, fluid) in cubics(&registry) {
        let c = fluid.model().critical_point().unwrap();
        let eos = fluid.model().helmholtz().unwrap();
        let t = 0.8 * c.t;
        let (vapour, liquid) = spinodals(eos, t, c.rho);
        let p_of = |rho: f64| rho * R_CODATA * t * (1.0 + eos.residual(t, rho, Order::One).get(0, 1).unwrap());
        for (p, want) in [(p_of(vapour), Phase::Liquid), (p_of(liquid).max(1.0), Phase::Gas)] {
            for scale in [1.0 - 1e-12, 1.0, 1.0 + 1e-12] {
                let state = fluid.flash(molar(Pair::PT, p * scale, t), &FlashOptions::default()).unwrap();
                assert!(state.rho(Basis::Molar).is_finite() && state.phase() == want, "{name} at {p} Pa: {state:?}");
            }
        }
        match fluid.flash(molar(Pair::PT, c.p, c.t), &FlashOptions::default()) {
            Ok(state) => assert!(state.rho(Basis::Molar).is_finite(), "{name}: {state:?}"),
            Err(e) => assert_eq!(e, Error::Domain(DomainError::MechanicallyUnstable), "{name} at the critical point"),
        }
    }
}

/// ROT-107 (user decision CB1): a cubic's R differs from its source's, and that scales its ideal-gas c_p⁰ by
/// R_CODATA/R_source at every T; it is not a reference-state shift, which would leave c_p⁰ alone and move h⁰ by the
/// same amount at every T, whereas h⁰'s difference here changes with T.
#[test]
fn r_scales_and_does_not_shift() {
    let embedded = Registry::from_embedded(DataSet::Corrected).unwrap();
    let registry = with_cubics(&embedded, &[Kind::Pr], DataSet::Corrected).unwrap();
    for name in ["Water", "CycloPropane", "n-Propane"] {
        let (source, cubic) = (embedded.get(name).unwrap(), registry.get(&format!("PR::{name}")).unwrap());
        let ratio = R_CODATA / source.model().helmholtz().unwrap().gas_constant();
        let at = |f: &Fluid, t: f64, prop: Prop| f.prop(&f.state(molar(Pair::DT, 1e-3, t)).unwrap(), prop).unwrap();
        let limits = cubic.model().limits().unwrap();
        let mut shifts = Vec::new();
        for x in [0.2, 0.5, 0.8] {
            let t = limits.t_min() + x * (limits.t_max() - limits.t_min());
            let cp = at(cubic, t, Prop::Cp0molar) / at(source, t, Prop::Cp0molar);
            assert!((cp / ratio - 1.0).abs() <= 4.0 * f64::EPSILON, "{name} at {t}: {cp} against {ratio}");
            shifts.push(at(cubic, t, Prop::HmolarIdealGas) - at(source, t, Prop::HmolarIdealGas));
        }
        assert!((shifts[2] - shifts[0]).abs() > 1e-9 * shifts[0].abs().max(1.0), "{name}: {shifts:?}");
    }
}
