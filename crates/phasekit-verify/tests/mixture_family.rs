//! The conformance kit on the mixture family (PLAN.md M13.15; VERIFICATION.md §10): R410A.mix, Air.mix, a GERG-2008
//! natural gas and a ternary, each read by its name through `phasekit_mix::with_mixtures`, in both data sets, through
//! the checks every family runs: the capability matrix (ROT-293, ROT-009), gauge invariance, a composition that trims
//! to one component as the pure fluid under any reference state (map 04 §8, map 15 §8), the executors' bits (K17), the
//! threads' bits, the residual's first derivatives and T (∂s/∂T)_p = c_p (ROT-212).

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::num::NonZeroUsize;

use phasekit_core::batch::{BatchRequest, ExecPolicy};
use phasekit_core::{Basis, DataSet, Error, Fluid, Gauge, Input, Pair, Prop, ReferenceState, Registry, State, Var};
use phasekit_mix::with_mixtures;
use phasekit_verify::conformance::{
    Mismatch, Outcome, capability_matrix, entropy_identity, fd_first_order, gauge_invariance, policy_equivalence,
};

/// A mixture of the family: its name, the prefix its compositions take, and its components.
struct Member {
    name: &'static str,
    prefix: &'static str,
    components: &'static [&'static str],
}

const R410A: Member = Member { name: "R410A.mix", prefix: "HEOS::", components: &["R32", "R125"] };
const AIR: Member = Member { name: "Air.mix", prefix: "HEOS::", components: &["Nitrogen", "Argon", "Oxygen"] };
const GAS: Member = Member {
    name: "GERG2008::Methane[0.85]&Ethane[0.07]&n-Propane[0.03]&Nitrogen[0.03]&CarbonDioxide[0.02]",
    prefix: "GERG2008::",
    components: &["Methane", "Ethane", "n-Propane", "Nitrogen", "CarbonDioxide"],
};
const TERNARY: Member =
    Member { name: "HEOS::R32[0.4]&R125[0.2]&R134a[0.4]", prefix: "HEOS::", components: &["R32", "R125", "R134a"] };

const FAMILY: [&Member; 4] = [&R410A, &AIR, &GAS, &TERNARY];
const DATA_SETS: [DataSet; 2] = [DataSet::Corrected, DataSet::Parity];

/// `data`'s embedded fluids, with the mixture names read.
fn registry(data: DataSet) -> Registry {
    with_mixtures(&Registry::from_embedded(data).unwrap(), data)
}

/// The fluid `name` resolves to in `registry`.
fn fluid(registry: &Registry, name: &str) -> Fluid {
    registry.resolve(name).map_err(|e| (name.to_string(), e)).unwrap()
}

fn molar(pair: Pair, x: f64, y: f64) -> Input {
    Input::new(pair, x, y, Basis::Molar).unwrap()
}

/// The inputs of the two truths, DT states of `fluid`: one phase at (0.6 ρ_c, 1.2 T_c) and the dome at (ρ_c, 0.8 T_c), c
/// the model's critical point (`ThermoModel::critical_point`).
fn truth_inputs(fluid: &Fluid) -> [Input; 2] {
    let c = fluid.model().critical_point().unwrap();
    [molar(Pair::DT, 0.6 * c.rho, 1.2 * c.t), molar(Pair::DT, c.rho, 0.8 * c.t)]
}

/// Whether one cell of the matrix is as expected: a pass, or for a quality pair `Undefined` from the single-phase truth,
/// which has no quality. A mixture's PT is one state in the dome too, so it passes there (a pure fluid's is
/// `Ambiguous`).
fn expected(outcome: &Outcome, pair: Pair, single_phase: bool) -> bool {
    let q_pair = matches!(pair.vars(), (Var::Q, _) | (_, Var::Q));
    match outcome {
        Outcome::Pass => true,
        Outcome::Refused(Error::Undefined { .. }) => single_phase && q_pair,
        _ => false,
    }
}

/// VERIFICATION.md §8.5 and ROT-293 (v8.0.0 lacks DP, DQ, HQ and QS for mixtures): all 19 pairs round-trip through both
/// truths of `member` in `data` within `Flash`, the phase label included (ROT-009); the cells that do not are listed.
fn capability(member: &Member, data: DataSet) {
    let fluid = fluid(&registry(data), member.name);
    let [single, two] = truth_inputs(&fluid).map(|input| fluid.state(input).unwrap());
    assert!(single.quality().is_none(), "{single:?}");
    assert!(two.quality().is_some_and(|q| 0.0 < q && q < 1.0), "{two:?}");
    let mut wrong = Vec::new();
    for (pair, [one_phase, dome]) in capability_matrix(&fluid, [&single, &two]) {
        if !expected(&one_phase, pair, true) {
            wrong.push((pair, "one phase", one_phase));
        }
        if !expected(&dome, pair, false) {
            wrong.push((pair, "dome", dome));
        }
    }
    assert!(wrong.is_empty(), "{} {data:?}: {wrong:#?}", member.name);
}

#[test]
fn r410a_capability_matrix() {
    for data in DATA_SETS {
        capability(&R410A, data);
    }
    assert_eq!(R410A.components.len(), 2);
}

#[test]
fn air_capability_matrix() {
    for data in DATA_SETS {
        capability(&AIR, data);
    }
    assert_eq!(AIR.components.len(), 3);
}

/// The natural gas's matrix, one data set per test: each takes minutes in a debug build.
#[test]
fn natural_gas_capability_matrix_corrected() {
    capability(&GAS, DataSet::Corrected);
    assert_eq!(GAS.components.len(), 5);
}

#[test]
fn natural_gas_capability_matrix_parity() {
    capability(&GAS, DataSet::Parity);
    assert_eq!(GAS.components.len(), 5);
}

#[test]
fn ternary_capability_matrix() {
    for data in DATA_SETS {
        capability(&TERNARY, data);
    }
    assert_eq!(TERNARY.components.len(), 3);
}

/// Map 15 §8: a gauge moves h and u by Δh, s by Δs, g and a by Δh − TΔs, and nothing else, on every mixture of the
/// family in both data sets, at its two truths.
#[test]
fn mixture_gauge_invariance() {
    let gauge = Gauge::new(1234.5, -6.75).unwrap();
    for data in DATA_SETS {
        let registry = registry(data);
        for member in FAMILY {
            let fluid = fluid(&registry, member.name);
            let result = gauge_invariance(&fluid, gauge, &truth_inputs(&fluid));
            assert!(result.is_ok(), "{} {data:?}: {result:?}", member.name);
        }
    }
}

/// Every value of the state `a` and of the state `b`, bit for bit (values or refusals).
fn same_bits(a: &State, b: &State) -> bool {
    let m = Basis::Molar;
    let bits = |s: &State| {
        let values = [Ok(s.t()), Ok(s.p()), Ok(s.rho(m)), Ok(s.h(m)), Ok(s.s(m)), Ok(s.u(m)), s.cv(m), s.cp(m)];
        (values.map(|v| v.map(f64::to_bits)), s.speed_of_sound().map(f64::to_bits), s.quality().map(f64::to_bits))
    };
    bits(a) == bits(b) && a.phase() == b.phase()
}

/// `member`'s composition at x = [1, 0, ...] in `registry`, and its first component's pure fluid: the registry's, or
/// for a GERG-2008 component GERG-2008's (`GERG2008::Methane`).
fn trimmed_and_pure(registry: &Registry, member: &Member) -> (Fluid, Fluid) {
    let (first, rest) = member.components.split_first().unwrap();
    let zeros: String = rest.iter().map(|c| format!("&{c}[0]")).collect();
    let trimmed = fluid(registry, &format!("{}{first}[1]{zeros}", member.prefix));
    let gerg = member.prefix == phasekit_mix::GERG_PREFIX;
    let pure = if gerg { fluid(registry, &format!("{}{first}", member.prefix)) } else { fluid(registry, first) };
    (trimmed, pure)
}

/// How many of IIR, ASHRAE and NBP `member`'s trimmed composition and its pure fluid take in `data`: each taken alike,
/// their states at the pure fluid's two truths bit for bit and `gauge_invariance` from the trimmed fluid's native gauge
/// to the pure fluid's at that reference state, and each refused alike.
fn references_taken(member: &Member, data: DataSet) -> usize {
    let (trimmed, pure) = trimmed_and_pure(&registry(data), member);
    let inputs = truth_inputs(&pure);
    let mut taken = 0;
    for reference in [ReferenceState::Iir, ReferenceState::Ashrae, ReferenceState::Nbp] {
        let label = format!("{} at x = [1, 0, ...] {data:?} {reference:?}", member.name);
        let (a, b) = match (pure.with_reference(reference), trimmed.with_reference(reference)) {
            (Ok(a), Ok(b)) => (a, b),
            (a, b) => {
                assert_eq!(a.err(), b.err(), "{label}");
                continue;
            }
        };
        let states = inputs.map(|input| (a.state(input).unwrap(), b.state(input).unwrap()));
        assert!(states.iter().all(|(a, b)| same_bits(a, b)), "{label}: {states:?}");
        let result = gauge_invariance(&trimmed, a.gauge(), &inputs);
        assert!(result.is_ok(), "{label}: {result:?}");
        taken += 1;
    }
    taken
}

/// Map 04 §8 and map 15 §8: each mixture's composition at x = [1, 0, ...] trims to its first component's pure fluid
/// (ROT-192), which keeps its values under IIR, ASHRAE and NBP ([`references_taken`]): R32 takes all three; nitrogen
/// and methane only NBP, since IIR (0 °C) and ASHRAE (−40 °C) anchor above their critical temperatures.
#[test]
fn one_component_is_the_pure_fluid_under_any_gauge() {
    for data in DATA_SETS {
        for (member, want) in [(&R410A, 3), (&AIR, 1), (&GAS, 1), (&TERNARY, 3)] {
            assert_eq!(references_taken(member, data), want, "{} {data:?}", member.name);
        }
    }
}

/// K17 on `fluid`: DT, PT, QT and PQ batches, single-phase and two-phase cells, under `Parallel` (chunks of two) and
/// `Simd` give `Reference`'s cells and statuses bit for bit; the batches that do not are listed.
fn policy_mismatches(fluid: &Fluid) -> Vec<(ExecPolicy, Pair, Mismatch)> {
    let outputs = [Prop::P, Prop::Hmolar, Prop::Cpmolar, Prop::Q];
    let c = fluid.model().critical_point().unwrap();
    let t: Vec<f64> = [0.7, 0.8, 0.9, 1.2].iter().map(|f| f * c.t).collect();
    let rho: Vec<f64> = [2.0, 1.0, 0.1, 0.6].iter().map(|f| f * c.rho).collect();
    let p: Vec<f64> = [2.0, 0.3, 0.01, 0.5].iter().map(|f| f * c.p).collect();
    let q = [0.0, 0.5, 1.0, 0.25];
    let requests = [
        (Pair::DT, &rho[..], &t[..]),
        (Pair::PT, &p[..], &t[..]),
        (Pair::QT, &q[..], &t[..]),
        (Pair::PQ, &p[..], &q[..]),
    ];
    let mut wrong = Vec::new();
    for exec in [ExecPolicy::Parallel { chunk: NonZeroUsize::new(2).unwrap() }, ExecPolicy::Simd] {
        for (pair, x, y) in requests {
            let result = policy_equivalence(fluid, &BatchRequest::new(pair, Basis::Molar, x, y, &outputs), exec);
            wrong.extend(result.err().map(|m| (exec, pair, m)));
        }
    }
    wrong
}

/// K17 ([`policy_mismatches`]) on every mixture of the family, in both data sets.
#[test]
fn mixture_policy_equivalence() {
    for data in DATA_SETS {
        let registry = registry(data);
        for member in FAMILY {
            let wrong = policy_mismatches(&fluid(&registry, member.name));
            assert!(wrong.is_empty(), "{} {data:?}: {wrong:?}", member.name);
        }
    }
}

/// The bits of the order-2 α^r bundle of `fluid`'s model at each (T, ρ) of `at`.
#[cfg(not(target_family = "wasm"))]
fn residual_bits(fluid: &Fluid, at: &[(f64, f64)]) -> Vec<Vec<u64>> {
    let eos = fluid.model().helmholtz().unwrap();
    let blocks = [(0, 0), (1, 0), (0, 1), (2, 0), (1, 1), (0, 2)];
    let bundle = |&(t, rho): &(f64, f64)| {
        let d = eos.residual(t, rho, phasekit_core::Order::Two);
        blocks.iter().map(|&(i, j)| d.get(i, j).unwrap().to_bits()).collect()
    };
    at.iter().map(bundle).collect()
}

/// VERIFICATION.md §10 (Threads), on every mixture of the family in both data sets, at its two truths and a PT state:
/// 16 threads racing to resolve its name through one registry, its resolver and its cache (user decision MX9), each get
/// a model whose α^r has a lone model's bits; and the kit's `thread_invariance`, 16 threads sharing one fresh fluid and
/// so racing into its one critical-point search (ROT-187), each get a lone fluid's states bit for bit.
#[cfg(not(target_family = "wasm"))] // wasip2 has no thread spawning
#[test]
fn mixture_thread_invariance() {
    use phasekit_verify::conformance::thread_invariance;
    for data in DATA_SETS {
        for member in FAMILY {
            let alone = fluid(&registry(data), member.name);
            let c = alone.model().critical_point().unwrap();
            let [single, dome] = truth_inputs(&alone);
            let inputs = [single, dome, molar(Pair::PT, 0.5 * c.p, 0.9 * c.t)];
            let at: Vec<(f64, f64)> =
                inputs.iter().filter_map(|&i| alone.state(i).ok()).map(|s| (s.t(), s.rho(Basis::Molar))).collect();
            let want = residual_bits(&alone, &at);
            let shared = registry(data);
            let barrier = std::sync::Barrier::new(16);
            std::thread::scope(|s| {
                let racing: Vec<_> = (0..16)
                    .map(|_| {
                        s.spawn(|| {
                            barrier.wait();
                            residual_bits(&fluid(&shared, member.name), &at)
                        })
                    })
                    .collect();
                for thread in racing {
                    assert_eq!(thread.join().unwrap(), want, "{} {data:?}: resolved", member.name);
                }
            });
            let result = thread_invariance(|| fluid(&registry(data), member.name), &inputs, 16);
            assert!(result.is_ok(), "{} {data:?}: {result:?}", member.name);
        }
    }
}

/// A10 and A01 of each mixture's residual, at its composition, against central differences in dimensional T and ρ
/// (class `Fd`), dense, at the critical point and dilute.
#[test]
fn mixture_residual_matches_finite_differences() {
    for data in DATA_SETS {
        let registry = registry(data);
        for member in FAMILY {
            let fluid = fluid(&registry, member.name);
            let c = fluid.model().critical_point().unwrap();
            let eos = fluid.model().helmholtz().unwrap();
            for (t, rho) in [(0.7 * c.t, 2.5 * c.rho), (c.t, c.rho), (1.5 * c.t, 0.1 * c.rho)] {
                let result = fd_first_order(eos, t, rho, 1e-6, 1e-7);
                assert!(result.is_ok(), "{} {data:?} at {t} K, {rho}: {result:?}", member.name);
            }
        }
    }
}

/// ROT-212: T (∂s/∂T)_p = c_p on every mixture of the family, in both data sets, from entropies flashed along the
/// isobar: supercritical, dense supercritical (at 1.3 T_c or, where the components' range ends below it, as R32's at
/// 435 K does, at 0.98 T_max), a dilute gas and a compressed liquid.
#[test]
fn mixture_entropy_identity() {
    for data in DATA_SETS {
        let registry = registry(data);
        for member in FAMILY {
            let fluid = fluid(&registry, member.name);
            let c = fluid.model().critical_point().unwrap();
            let t_max = fluid.model().limits().unwrap().t_max();
            let inputs = [
                molar(Pair::PT, 0.1 * c.p, 1.1 * c.t),
                molar(Pair::PT, 2.0 * c.p, (1.3 * c.t).min(0.98 * t_max)),
                molar(Pair::PT, 1e4, 0.9 * c.t),
                molar(Pair::PT, 2.0 * c.p, 0.7 * c.t),
            ];
            let result = entropy_identity(&fluid, &inputs);
            assert!(result.is_ok(), "{} {data:?}: {result:?}", member.name);
        }
    }
}
