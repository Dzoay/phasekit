//! Batch on real fluids (PLAN.md M5.8, M9.1, M12, M22; VERIFICATION.md §9.1, §9.2; ARCHITECTURE.md §7): the batch
//! driver is the scalar path cell by cell, bit for bit, into point-major buffers, every executor and thread count gives
//! the same bits, and so do a mixture's batch hooks and a family's kernels over `Real`.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::num::NonZeroUsize;

use phasekit_core::batch::{self, BatchRequest, ExecPolicy, RealKernel, Status};
use phasekit_core::{
    Basis, DataSet, Density, Derivs, DomainPolicy, Error, FlashOptions, Fluid, HelmholtzModel, Input, Order, Pair,
    Phase, Pressure, Prop, Quality, Real, Registry, RootPolicy, Temperature, math,
};
use phasekit_verify::mixtures::{BENCH_MIXTURES, BenchMixture, MixtureModel};
use phasekit_verify::{SplitMix64, policy_equivalence};

/// Outputs that cover single-phase and two-phase answers and refusals (cp, w and Q are undefined somewhere).
const OUTPUTS: [Prop; 8] =
    [Prop::T, Prop::P, Prop::Dmolar, Prop::Hmolar, Prop::Smolar, Prop::Cpmolar, Prop::SpeedOfSound, Prop::Q];

/// `n` DT points of a fluid between its triple point and 1.5 T_c, ρ log-spaced over six decades below its saturated
/// liquid at the triple point: gas, liquid, supercritical and two-phase states.
fn points(registry: &Registry, name: &str, n: usize, seed: u64) -> (Vec<f64>, Vec<f64>) {
    let record = phasekit_core::internal::record(registry, name).unwrap();
    let curve = record.superancillary_curve().unwrap();
    let (t_lo, t_c) = curve.t_range();
    let rho_l = curve.at_t(t_lo).unwrap().bubble.rho;
    let mut rng = SplitMix64::new(seed);
    let mut sample =
        |_| (rng.log_uniform(1e-6 * rho_l, rho_l), rng.uniform(t_lo, (1.5 * t_c).min(record.limits.t_max())));
    (0..n).map(&mut sample).unzip()
}

/// VERIFICATION.md §9.1, class `Exact`: 10,000 DT points of each of Water, CarbonDioxide and R1234yf through
/// `batch::evaluate`, every cell bitwise the scalar flash and `Fluid::prop` of that point, and every status the scalar
/// result's (ROT-011, ROT-014: one code path, so batch and scalar labels and values never disagree).
#[test]
fn batch_equals_scalar_bitwise() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    for (seed, name) in [(1, "Water"), (2, "CarbonDioxide"), (3, "R1234yf")] {
        let (two_phase, refused) = batch_against_scalar(&registry, name, seed);
        assert!(two_phase > 100 && refused > 0, "{name}: {two_phase} two-phase points, {refused} refused cells");
    }
}

/// One fluid of [`batch_equals_scalar_bitwise`]: its 10,000 points (from `seed`) through `batch::evaluate`, each cell
/// asserted bitwise the scalar flash's and `Fluid::prop`'s, with its status; returns the two-phase points and the
/// refused cells.
fn batch_against_scalar(registry: &Registry, name: &str, seed: u64) -> (usize, usize) {
    let fluid = registry.get(name).unwrap();
    let (x, y) = points(registry, name, 10_000, seed);
    let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &OUTPUTS);
    let (mut out, mut status) = (vec![0.0; x.len() * OUTPUTS.len()], vec![Status::Other; x.len() * OUTPUTS.len()]);
    batch::evaluate(fluid, &req, &mut out, &mut status).unwrap();
    let (mut two_phase, mut refused) = (0, 0);
    for (i, (rho, t)) in x.iter().zip(&y).enumerate() {
        let state = fluid.state(Input::dt(Density::molar(*rho).unwrap(), Temperature::new(*t).unwrap()));
        two_phase += usize::from(state.as_ref().is_ok_and(|s| s.quality().is_some()));
        for (k, prop) in OUTPUTS.iter().enumerate() {
            let scalar = state.as_ref().map_err(Clone::clone).and_then(|s| fluid.prop(s, *prop));
            let (cell, cell_status) = (out[i * OUTPUTS.len() + k], status[i * OUTPUTS.len() + k]);
            match scalar {
                Ok(v) => {
                    assert_eq!((cell.to_bits(), cell_status), (v.to_bits(), Status::Ok), "{name} point {i} {prop:?}")
                }
                Err(e) => {
                    assert!(cell.is_nan() && cell_status == Status::from(&e), "{name} point {i} {prop:?}: {e:?}");
                    refused += 1;
                }
            }
        }
    }
    (two_phase, refused)
}

/// User decision 11 (ARCHITECTURE.md §7): `out[i·M + k]` is output k of point i, and `status` has the same layout.
#[test]
fn out_is_point_major() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let water = registry.get("Water").unwrap();
    let (x, y) = ([10.0, 55_400.0, 1_000.0], [700.0, 300.0, 400.0]);
    let outputs = [Prop::T, Prop::Dmolar, Prop::Q];
    let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &outputs);
    let (mut out, mut status) = ([0.0; 9], [Status::Other; 9]);
    batch::evaluate(water, &req, &mut out, &mut status).unwrap();
    assert_eq!([out[0], out[3], out[6]], y, "column 0 is T");
    for (i, rho) in x.iter().enumerate() {
        assert!((out[i * 3 + 1] / rho - 1.0).abs() < 1e-15, "column 1 is ρ: {}", out[i * 3 + 1]);
    }
    assert_eq!([status[2], status[5]], [Status::Undefined, Status::Undefined], "single phases have no Q");
    assert!(status[8] == Status::Ok && out[8] > 0.0 && out[8] < 1.0, "the third point is two-phase: {}", out[8]);
}

/// PLAN.md M12.8 (VERIFICATION.md §9.2, class `Exact`): DT batches of every embedded fluid under `ExecPolicy::Simd`,
/// whose total bundles the built-in EOS evaluates in lanes, are `Reference`'s, every cell and status bit for bit
/// (`policy_equivalence`, K17). The points are [`dt_grid`]'s, in-dome states and refusals included, under the default
/// options, `Extrapolate` and an imposed gas; prefixes of 0 to 17 points and the whole set take every remainder of the
/// lanes and of the driver's groups.
#[test]
fn dt_batch_policy_equivalence() {
    let registry = Registry::embedded().unwrap();
    let options = [
        FlashOptions::new(),
        FlashOptions::new().with_domain(DomainPolicy::Extrapolate),
        FlashOptions::new().with_phase(Phase::Gas),
    ];
    let (mut fluids, mut dome) = (0, 0);
    for name in registry.names() {
        let fluid = registry.get(name).unwrap();
        let (x, y, in_dome) = dt_grid(registry, fluid, name);
        for opts in options {
            for n in (0..=17).chain([x.len()]) {
                let req = BatchRequest::new(Pair::DT, Basis::Molar, &x[..n], &y[..n], &OUTPUTS).with_flash(opts);
                let equal = policy_equivalence(fluid, &req, ExecPolicy::Simd);
                assert!(equal.is_ok(), "{name} {opts:?} n = {n}: {equal:?}");
            }
        }
        (fluids, dome) = (fluids + 1, dome + in_dome);
    }
    assert!(fluids >= 130 && dome >= 16 * fluids, "{fluids} fluids, {dome} in-dome points");
}

/// PLAN.md M12.9 (VERIFICATION.md §9.2, class `Exact`): QT and PQ batches of every embedded fluid under
/// `ExecPolicy::Simd`, whose saturations the superancillary and whose phases' bundles the built-in EOS evaluate in
/// lanes, are `Reference`'s, every cell and status bit for bit (`policy_equivalence`, K17). The points are
/// [`sat_grid`]'s, at Q = 0, 1 and inside the dome side by side (bundles of order 2 and 1 in one group) and refusals,
/// under the default options, `Extrapolate`, an imposed two-phase, an imposed gas (every point `Undefined`) and
/// `Nearest`; prefixes of 0 to 17 points and the whole set, past one group of 64, take every remainder.
#[test]
fn sat_batch_policy_equivalence() {
    let registry = Registry::embedded().unwrap();
    let options = [
        FlashOptions::new(),
        FlashOptions::new().with_domain(DomainPolicy::Extrapolate),
        FlashOptions::new().with_phase(Phase::TwoPhase),
        FlashOptions::new().with_phase(Phase::Gas),
        FlashOptions::new().with_roots(RootPolicy::Nearest(300.0)),
    ];
    let (mut fluids, mut answered) = (0, 0);
    for name in registry.names() {
        let fluid = registry.get(name).unwrap();
        let ((q, t), (p, pq)) = sat_grid(registry, fluid, name);
        for opts in options {
            for n in (0..=17).chain([t.len()]) {
                let qt = BatchRequest::new(Pair::QT, Basis::Molar, &q[..n], &t[..n], &OUTPUTS).with_flash(opts);
                let equal = policy_equivalence(fluid, &qt, ExecPolicy::Simd);
                assert!(equal.is_ok(), "{name} QT {opts:?} n = {n}: {equal:?}");
                let pq = BatchRequest::new(Pair::PQ, Basis::Molar, &p[..n], &pq[..n], &OUTPUTS).with_flash(opts);
                let equal = policy_equivalence(fluid, &pq, ExecPolicy::Simd);
                assert!(equal.is_ok(), "{name} PQ {opts:?} n = {n}: {equal:?}");
            }
        }
        let state = |pair, x: f64, y: f64| Input::new(pair, x, y, Basis::Molar).and_then(|input| fluid.state(input));
        let both = |k: usize| state(Pair::QT, q[k], t[k]).is_ok() && state(Pair::PQ, p[k], pq[k]).is_ok();
        (fluids, answered) = (fluids + 1, answered + (0..t.len()).filter(|k| both(*k)).count());
    }
    assert!(fluids >= 130 && answered >= 40 * fluids, "{fluids} fluids, {answered} points answered as QT and PQ");
}

/// A batch's two input columns, x and y.
type Columns = (Vec<f64>, Vec<f64>);

/// A fluid's QT points (Q, T) and PQ points (p, Q) on the `sat` grid (VERIFICATION.md §9.4, SplitMix64 seed 1): 72 T
/// with Θ = 1 − T/T_c ~ logU[1e-7, 1 − T_low/T_c], T_low = max(T_min, T_triple), Q 0, 1 and U(0, 1) in turn, each PQ
/// point at its QT state's p (NaN where QT refuses it). Then refusals and ends: T_low and T_c, 1 mK below T_low and above
/// T_c, a NaN T, Q 1.5 and NaN; p at T_low's and 1 % above it (where p(T) dips, two roots), 1 % above p_c, 1e-3 of
/// T_low's and NaN.
fn sat_grid(registry: &Registry, fluid: &Fluid, name: &str) -> (Columns, Columns) {
    let record = phasekit_core::internal::record(registry, name).unwrap();
    let limits = &record.limits;
    let t_low = limits.t_min().max(limits.t_triple().unwrap_or(limits.t_min()));
    let crit = record.critical.unwrap();
    let p_of = |q: f64, t: f64| Input::new(Pair::QT, q, t, Basis::Molar).and_then(|i| fluid.state(i)).map(|s| s.p());
    let mut rng = SplitMix64::new(1);
    let theta_max = 1.0 - t_low / crit.t;
    let (mut q, mut t): (Vec<f64>, Vec<f64>) = (0..72)
        .map(|k| match k % 3 {
            2 => (rng.uniform(0.0, 1.0), crit.t * (1.0 - rng.log_uniform(1e-7, theta_max))),
            side => (f64::from(side), crit.t * (1.0 - rng.log_uniform(1e-7, theta_max))),
        })
        .unzip();
    let (mut p, mut pq): (Vec<f64>, Vec<f64>) =
        q.iter().zip(&t).map(|(q, t)| (p_of(*q, *t).unwrap_or(f64::NAN), *q)).unzip();
    q.extend([0.0, 1.0, 0.5, 0.5, 0.5, 1.5, f64::NAN]);
    t.extend([t_low, crit.t, t_low - 1e-3, crit.t + 1e-3, f64::NAN, crit.t * 0.9, crit.t * 0.9]);
    let p_low = p_of(0.0, t_low).unwrap_or(1.0);
    p.extend([p_low, 1.01 * p_low, 1.01 * crit.p, 1e-3 * p_low, f64::NAN, p_low, p_low]);
    pq.extend([0.0, 0.5, 1.0, 0.5, 0.5, 1.5, f64::NAN]);
    ((q, t), (p, pq))
}

/// A fluid's DT points (ρ, T) on the `eos` and `flash` grids, and how many lie in the dome. The `eos` grid's 64 points
/// (VERIFICATION.md §9.4, SplitMix64 seed 1): T ~ U[T_low, T_max] and ρ ~ logU[1e-6·ρ_L, ρ_L], ρ_L the saturated liquid
/// at T_low = max(T_min, T_triple) (3ρ_c where QT has none). The `flash` grid's truths read off as DT (§3.5): 6 × 6
/// (p, T) at cell centres of T in [T_low, T_max] and ln p in [ln max(p_sat(T_low), 1 Pa), ln p_max], and 4 × 4 (T, Q)
/// at cell centres of T in [T_low, T_c] and Q in (0, 1), inside the dome: a pseudo-pure fluid's from its QT densities at
/// Q = 0 and 1 by the lever rule, as it has no QT inside the dome (§3.5's `dome` truths). A truth the flash refuses gives
/// a NaN ρ. Then a negative ρ and a NaN T.
fn dt_grid(registry: &Registry, fluid: &Fluid, name: &str) -> (Vec<f64>, Vec<f64>, usize) {
    let record = phasekit_core::internal::record(registry, name).unwrap();
    let limits = &record.limits;
    let (t_low, t_max) = (limits.t_min().max(limits.t_triple().unwrap_or(limits.t_min())), limits.t_max());
    let crit = record.critical.unwrap();
    let qt = |q: f64, t: f64| Some(Input::qt(Quality::new(q).ok()?, Temperature::new(t).ok()?));
    let state = |input: Option<Input>| input.and_then(|input| fluid.state(input).ok());
    let liquid = state(qt(0.0, t_low));
    let (rho_l, p_low) = liquid.map_or((3.0 * crit.rho, 1.0), |s| (s.rho(Basis::Molar), s.p().max(1.0)));
    let mut rng = SplitMix64::new(1);
    let (mut x, mut y): (Vec<f64>, Vec<f64>) =
        (0..64).map(|_| (rng.log_uniform(1e-6 * rho_l, rho_l), rng.uniform(t_low, t_max))).unzip();
    let centre = |k: u32, n: u32| (f64::from(k) + 0.5) / f64::from(n);
    let mut truth = |input: Option<Input>, t: f64| {
        let rho = state(input).map(|s| s.rho(Basis::Molar));
        x.push(rho.unwrap_or(f64::NAN));
        y.push(t);
        rho.is_some()
    };
    for i in 0..6 {
        let t = t_low + (t_max - t_low) * centre(i, 6);
        for j in 0..6 {
            let p = math::exp(math::ln(p_low) + (math::ln(limits.p_max()) - math::ln(p_low)) * centre(j, 6));
            truth(Pressure::new(p).ok().zip(Temperature::new(t).ok()).map(|(p, t)| Input::pt(p, t)), t);
        }
    }
    let mut in_dome = 0;
    for i in 0..4 {
        let t = t_low + (crit.t - t_low) * centre(i, 4);
        let sides = state(qt(0.0, t)).zip(state(qt(1.0, t))).map(|(l, v)| (l.rho(Basis::Molar), v.rho(Basis::Molar)));
        for j in 0..4 {
            let q = centre(j, 4);
            let lever = sides.map(|(l, v)| 1.0 / (q / v + (1.0 - q) / l));
            let rho = state(qt(q, t)).map(|s| s.rho(Basis::Molar)).or(lever);
            in_dome += usize::from(rho.is_some());
            x.push(rho.unwrap_or(f64::NAN));
            y.push(t);
        }
    }
    x.extend([-1.0, rho_l]);
    y.extend([t_low, f64::NAN]);
    (x, y, in_dome)
}

/// Executors and threads (PLAN.md M9.1). Thread tests: wasm has no rayon (D17).
#[cfg(not(target_family = "wasm"))]
mod parallel {
    use std::num::NonZeroUsize;

    use phasekit_core::Var;
    use phasekit_core::batch::ExecPolicy;

    use super::*;

    /// Points per pair in [`policy_equivalence_parallel_is_bitwise`]: 19 pairs make 99 997 points, about 10⁵, and 5 263
    /// is a multiple of none of the chunk sizes, so every batch ends on a short chunk (VERIFICATION.md §9.1).
    const POINTS_PER_PAIR: usize = 5_263;

    /// Inputs of every pair read off `n` DT truths of a fluid (the [`points`] sample): T, p, ρ, h, s and u off the
    /// scalar state, Q its quality or, for a single phase, a uniform draw, so the Q-pairs land in the dome too. A truth
    /// the scalar flash refuses gives NaN inputs, which the batch refuses cell by cell.
    fn pair_inputs(registry: &Registry, name: &str, n: usize, seed: u64) -> Vec<(Pair, Vec<f64>, Vec<f64>)> {
        let fluid = registry.get(name).unwrap();
        let (rho, t) = points(registry, name, n, seed);
        let mut rng = SplitMix64::new(seed ^ 0x9e37_79b9_7f4a_7c15);
        let truths: Vec<[f64; 7]> = rho
            .iter()
            .zip(&t)
            .map(|(rho, t)| {
                let q = rng.next_f64();
                match fluid.state(Input::dt(Density::molar(*rho).unwrap(), Temperature::new(*t).unwrap())) {
                    Ok(s) => {
                        let m = Basis::Molar;
                        [s.t(), s.p(), s.rho(m), s.h(m), s.s(m), s.u(m), s.quality().unwrap_or(q)]
                    }
                    Err(_) => [f64::NAN; 7],
                }
            })
            .collect();
        let column = |var: Var| -> Vec<f64> {
            let k = [Var::T, Var::P, Var::D, Var::H, Var::S, Var::U, Var::Q].iter().position(|v| *v == var).unwrap();
            truths.iter().map(|truth| truth[k]).collect()
        };
        Pair::ALL.iter().map(|&pair| (pair, column(pair.vars().0), column(pair.vars().1))).collect()
    }

    /// Every cell's bits and status of `req` under `exec`.
    fn run(fluid: &Fluid, req: &BatchRequest<'_>, exec: ExecPolicy) -> (Vec<u64>, Vec<Status>) {
        let cells = req.cells().unwrap();
        let (mut out, mut status) = (vec![0.0; cells], vec![Status::Other; cells]);
        batch::evaluate(fluid, &req.with_exec(exec), &mut out, &mut status).unwrap();
        (out.iter().map(|v| v.to_bits()).collect(), status)
    }

    fn parallel(chunk: usize) -> ExecPolicy {
        ExecPolicy::Parallel { chunk: NonZeroUsize::new(chunk).unwrap() }
    }

    /// VERIFICATION.md §9.1, class `Exact` (ROT-135; K6, K17): every pair, 5 263 points each (about 10⁵ in all) of
    /// R1234yf, through `Parallel` at chunks of 16, 64 and 1 024 points on rayon's global pool and through `Simd`
    /// (flashes have no lane kernel: the scalar path, PLAN.md M12.3), every cell and status bitwise the `Reference`
    /// path's: single-phase, two-phase, near-critical and refused cells alike.
    #[test]
    fn policy_equivalence_parallel_is_bitwise() {
        let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
        let fluid = registry.get("R1234yf").unwrap();
        let (mut points, mut refused) = (0, 0);
        for (pair, x, y) in pair_inputs(&registry, "R1234yf", POINTS_PER_PAIR, 7) {
            let req = BatchRequest::new(pair, Basis::Molar, &x, &y, &OUTPUTS);
            let reference = run(fluid, &req, ExecPolicy::Reference);
            for exec in [parallel(16), parallel(64), parallel(1024), ExecPolicy::Simd] {
                assert!(run(fluid, &req, exec) == reference, "{pair:?} under {exec:?}");
            }
            (points, refused) = (points + x.len(), refused + reference.1.iter().filter(|s| **s != Status::Ok).count());
        }
        assert!(points == Pair::ALL.len() * POINTS_PER_PAIR && refused > 0, "{points} points, {refused} refused");
    }

    /// VERIFICATION.md §9.1, class `Exact` (K6): PT and HP batches of n-Propane and R1234yf under `Parallel` on rayon
    /// pools of 1 to `available_parallelism` threads (at least 4), every cell and status bitwise the `Reference`
    /// path's: fixed chunks make the bits independent of the thread count.
    #[test]
    fn one_to_n_threads_bitwise() {
        let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
        let n = std::thread::available_parallelism().map_or(4, NonZeroUsize::get).max(4);
        let mut pools = 0;
        for (seed, name) in [(11, "n-Propane"), (12, "R1234yf")] {
            let fluid = registry.get(name).unwrap();
            for (pair, x, y) in pair_inputs(&registry, name, 1_000, seed) {
                if !matches!(pair, Pair::PT | Pair::HP) {
                    continue;
                }
                let req = BatchRequest::new(pair, Basis::Molar, &x, &y, &OUTPUTS);
                let reference = run(fluid, &req, ExecPolicy::Reference);
                for threads in 1..=n {
                    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
                    assert!(pool.install(|| run(fluid, &req, parallel(16))) == reference, "{name} {pair:?}, {threads}");
                    pools += 1;
                }
            }
        }
        assert_eq!(pools, 4 * n);
    }

    /// A kernel over `Real` whose outputs are its inputs on a rayon worker and NaN on any other thread.
    struct OnThePool;

    impl RealKernel for OnThePool {
        fn eval<R: Real>(&self, inputs: &[R], outputs: &mut [R]) {
            let on_the_pool = rayon::current_thread_index().is_some();
            for (out, x) in outputs.iter_mut().zip(inputs) {
                *out = if on_the_pool { *x } else { R::from_f64(f64::NAN) };
            }
        }
    }

    /// PLAN.md M22.2 (K6): `batch::run_kernel` places its chunks as the batch driver does. A `Parallel` batch of
    /// several chunks runs them on rayon's workers, and `Reference` and `Simd` run on the calling thread, where
    /// [`OnThePool`] writes NaN.
    #[test]
    fn real_kernel_chunks_run_on_the_rayon_pool() {
        let x: Vec<f64> = (1..=64).map(f64::from).collect();
        let run = |exec| {
            let mut y = vec![0.0; x.len()];
            batch::run_kernel(&OnThePool, &[&x], &mut [&mut y], exec).unwrap();
            y
        };
        assert_eq!(run(parallel(16)), x, "four chunks of 16 off the pool");
        for exec in [ExecPolicy::Reference, ExecPolicy::Simd] {
            assert!(run(exec).iter().all(|y| y.is_nan()), "{exec:?} ran on the pool");
        }
    }
}

/// PLAN.md M12.1 and M12.8 (VERIFICATION.md §9.1, class `Exact`): `HelmholtzModel::residual_batch` is `residual` and
/// `ideal_batch` is `ideal` at every point, every entry bit for bit, for every embedded fluid at every order, under
/// `Reference` and `Parallel` (16-point chunks, a short last one included). The points span the fluid's T range and ρ up
/// to `rho_max`, with ρ = 0, T_max and NaN inputs; unequal lengths are a `Shape` error.
#[test]
fn helmholtz_batches_are_per_point() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let parallel = ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() };
    let mut rng = SplitMix64::new(12);
    let mut fluids = 0;
    for name in registry.names() {
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let (fluid, (t_min, t_max)) =
            (record.clone().compile().unwrap(), (record.limits.t_min(), record.limits.t_max()));
        let eos = fluid.eos();
        let (mut t, mut rho): (Vec<f64>, Vec<f64>) = (0..40)
            .map(|_| {
                let t = rng.uniform(t_min, t_max);
                (t, rng.uniform(0.0, eos.rho_max(t)))
            })
            .unzip();
        t.extend([t_min, t_max, f64::NAN, t_max]);
        rho.extend([0.0, eos.rho_max(t_max), 1.0, f64::NAN]);
        for order in [Order::One, Order::Two, Order::Three, Order::Four] {
            for (part, exec) in
                [Part::Residual, Part::Ideal].into_iter().flat_map(|p| [(p, ExecPolicy::Reference), (p, parallel)])
            {
                assert_batch_is_per_point(name, eos, part, (&t, &rho), order, exec);
            }
        }
        let (mut short, mut out) =
            (vec![Derivs::zero(Order::Two); t.len() - 1], vec![Derivs::zero(Order::Two); t.len()]);
        for shape in [
            eos.residual_batch(&t, &rho, Order::Two, &mut short, ExecPolicy::Reference),
            eos.residual_batch(&t, &rho[1..], Order::Two, &mut out, parallel),
            eos.ideal_batch(&t, &rho, Order::Two, &mut short, ExecPolicy::Simd),
            eos.ideal_batch(&t[1..], &rho, Order::Two, &mut out, ExecPolicy::Reference),
        ] {
            assert!(matches!(shape, Err(Error::Shape { .. })), "{name}: {shape:?}");
        }
        assert!(eos.residual_batch(&[], &[], Order::Two, &mut [], parallel).is_ok(), "{name}: empty batch");
        assert!(eos.ideal_batch(&[], &[], Order::Two, &mut [], ExecPolicy::Simd).is_ok(), "{name}: empty α⁰ batch");
        fluids += 1;
    }
    assert!(fluids >= 130, "{fluids} fluids");
}

/// PLAN.md M12.3 and M12.8 (VERIFICATION.md §9.2, class `Exact`): `residual_batch` and `ideal_batch` under
/// `ExecPolicy::Simd` are `residual` and `ideal` at every point, as under `Reference`, for every embedded fluid at every
/// order. Batches of 0 to 17 points take every remainder of 2, 4 and 8 lanes, and the whole set adds ρ = 0, T_max and
/// NaN inputs.
#[test]
fn helmholtz_batches_policy_equivalence() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let mut rng = SplitMix64::new(13);
    let mut fluids = 0;
    for name in registry.names() {
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let (fluid, (t_min, t_max)) =
            (record.clone().compile().unwrap(), (record.limits.t_min(), record.limits.t_max()));
        let eos = fluid.eos();
        let (mut t, mut rho): (Vec<f64>, Vec<f64>) = (0..20)
            .map(|_| {
                let t = rng.uniform(t_min, t_max);
                (t, rng.uniform(0.0, eos.rho_max(t)))
            })
            .unzip();
        t.extend([t_min, t_max, f64::NAN, t_max]);
        rho.extend([0.0, eos.rho_max(t_max), 1.0, f64::NAN]);
        let orders = [Order::One, Order::Two, Order::Three, Order::Four];
        for (order, part) in orders.into_iter().flat_map(|o| [(o, Part::Residual), (o, Part::Ideal)]) {
            for n in (0..=17).chain([t.len()]) {
                assert_batch_is_per_point(name, eos, part, (&t[..n], &rho[..n]), order, ExecPolicy::Simd);
            }
        }
        fluids += 1;
    }
    assert!(fluids >= 130, "{fluids} fluids");
}

/// M22's bench mixtures (`phasekit_verify::mixtures`), each as a fluid on the `Parity` library.
fn bench_mixtures() -> Vec<(&'static str, Fluid)> {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let build = |m: &BenchMixture| match m.model {
        MixtureModel::Library => phasekit_mix::mixture(&registry, m.components, DataSet::Parity),
        MixtureModel::Gerg2008 => phasekit_mix::gerg2008(&registry, m.components),
    };
    BENCH_MIXTURES.iter().map(|m| (m.label, build(m).unwrap())).collect()
}

/// PLAN.md M22.1 (VERIFICATION.md §9.1, class `Exact`): a mixture's `residual_batch` and `ideal_batch` are its
/// `residual` and `ideal` at every point, every entry bit for bit, for M22's bench mixtures at every order under
/// `Reference`, `Parallel` (16-point chunks) and `Simd`. The points lie between half and twice each mixture's reducing
/// temperature and up to 2.5 times its reducing density, with ρ = 0 and NaN inputs. Under `Simd`, batches of 0 to 17
/// points take every remainder of the components' lanes, and the whole set of 130 spans three of the batch's blocks of
/// 64. Unequal lengths are a `Shape` error.
#[test]
fn mixture_batch_is_residual_per_point() {
    let mut rng = SplitMix64::new(14);
    let mut mixtures = 0;
    for (label, fluid) in bench_mixtures() {
        let eos = fluid.model().helmholtz().unwrap();
        let r = eos.reducing().unwrap();
        let (mut t, mut rho): (Vec<f64>, Vec<f64>) =
            (0..126).map(|_| (rng.uniform(0.5, 2.0) * r.t, rng.uniform(0.0, 2.5) * r.rho)).unzip();
        t.extend([r.t, 2.0 * r.t, f64::NAN, r.t]);
        rho.extend([0.0, r.rho, r.rho, f64::NAN]);
        assert_batches_are_per_point(label, eos, (&t, &rho));
        assert_shapes_are_refused(label, eos, (&t, &rho));
        mixtures += 1;
    }
    assert_eq!(mixtures, BENCH_MIXTURES.len());
}

/// [`mixture_batch_is_residual_per_point`]'s checks of one model: both parts at every order, the whole set under
/// `Reference` and `Parallel`, and under `Simd` its prefixes of 0 to 17 points and the whole set.
fn assert_batches_are_per_point(name: &str, eos: &dyn HelmholtzModel, (t, rho): (&[f64], &[f64])) {
    let parallel = ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() };
    let orders = [Order::One, Order::Two, Order::Three, Order::Four];
    for (order, part) in orders.into_iter().flat_map(|o| [(o, Part::Residual), (o, Part::Ideal)]) {
        for exec in [ExecPolicy::Reference, parallel] {
            assert_batch_is_per_point(name, eos, part, (t, rho), order, exec);
        }
        for n in (0..=17).chain([t.len()]) {
            assert_batch_is_per_point(name, eos, part, (&t[..n], &rho[..n]), order, ExecPolicy::Simd);
        }
    }
}

/// A Helmholtz model's batch hooks refuse unequal lengths with a `Shape` error naming the batch's lengths (not a
/// component's or a block's) under every policy, and run an empty batch.
fn assert_shapes_are_refused(name: &str, eos: &dyn HelmholtzModel, (t, rho): (&[f64], &[f64])) {
    let parallel = ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() };
    let (mut short, mut out) = (vec![Derivs::zero(Order::Two); t.len() - 1], vec![Derivs::zero(Order::Two); t.len()]);
    let (n, m) = (t.len(), t.len() - 1);
    for (shape, (expected, found)) in [
        (eos.residual_batch(t, rho, Order::Two, &mut short, ExecPolicy::Reference), (n, m)),
        (eos.residual_batch(t, &rho[1..], Order::Two, &mut out, ExecPolicy::Simd), (n, m)),
        (eos.ideal_batch(t, rho, Order::Two, &mut short, ExecPolicy::Simd), (n, m)),
        (eos.ideal_batch(&t[1..], rho, Order::Two, &mut out, parallel), (m, n)),
    ] {
        assert_eq!(shape, Err(Error::Shape { expected, found }), "{name}");
    }
    assert!(eos.residual_batch(&[], &[], Order::Two, &mut [], parallel).is_ok(), "{name}: empty batch");
    assert!(eos.ideal_batch(&[], &[], Order::Two, &mut [], ExecPolicy::Simd).is_ok(), "{name}: empty α⁰ batch");
}

/// A kernel over `Real` that uses every method and operator of `Real`, its counts set at run time: output j combines
/// inputs j and j + 1 (cyclically), scaled by j + 1. The inputs are positive, for `ln` and `sqrt`.
struct EveryMethod;

impl RealKernel for EveryMethod {
    #[inline(always)]
    fn eval<R: Real>(&self, inputs: &[R], outputs: &mut [R]) {
        let input = |k: usize| inputs.get(k % inputs.len().max(1)).copied().unwrap_or(R::from_f64(1.0));
        for (j, out) in outputs.iter_mut().enumerate() {
            let (a, b) = (input(j), input(j + 1));
            let growth = a.exp() + b.expm1() * a.ln() - b.ln_1p() / a.sqrt();
            let powers = a.powi(3) + b.powf(1.7);
            let hyperbolic = (a * 0.1).sinh() * (b * 0.1).cosh() + (-a).atan() - (-b).abs();
            *out = (growth + powers + hyperbolic + R::from_f64(0.5)) * (j as f64 + 1.0) + 0.25;
        }
    }
}

/// PLAN.md M22.2 (VERIFICATION.md §9.2, class `Exact`; user decision ML3): a family's kernel over `Real` runs through
/// `batch::run_kernel` bitwise its `f64` evaluation point by point, under `Reference`, `Parallel` (16-point chunks) and
/// `Simd`, through the public path only. The kernel uses every `Real` method, at counts set at run time (1 in and 1 out,
/// 2 in and 3 out, 3 in and 15 out), over batches of 1 to 65 points, so a short last group at every lane width. Unequal
/// columns are a `Shape` error, and an empty batch runs.
#[test]
fn real_kernels_are_scalar_bitwise() {
    let mut rng = SplitMix64::new(15);
    let columns: Vec<Vec<f64>> = (0..3).map(|_| (0..65).map(|_| rng.uniform(0.1, 3.0)).collect()).collect();
    let parallel = ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() };
    for (inputs, outputs) in [(1, 1), (2, 3), (3, 15)] {
        for n in 1..=65 {
            let ins: Vec<&[f64]> = columns[..inputs].iter().map(|c| &c[..n]).collect();
            let want = kernel_per_point(&ins, outputs, n);
            for exec in [ExecPolicy::Reference, parallel, ExecPolicy::Simd] {
                let mut got = vec![vec![f64::NAN; n]; outputs];
                let mut outs: Vec<&mut [f64]> = got.iter_mut().map(Vec::as_mut_slice).collect();
                let ran = batch::run_kernel(&EveryMethod, &ins, &mut outs, exec);
                assert!(ran.is_ok(), "{inputs} in, {outputs} out, {n} points, {exec:?}: {ran:?}");
                assert_eq!(column_bits(&got), column_bits(&want), "{inputs} in, {outputs} out, {n} points, {exec:?}");
            }
        }
    }
    assert_kernel_shapes_are_refused(&columns);
}

/// `EveryMethod` on `f64` at each of `n` points of the input columns `ins`: `outputs` columns.
fn kernel_per_point(ins: &[&[f64]], outputs: usize, n: usize) -> Vec<Vec<f64>> {
    let mut want = vec![vec![0.0; n]; outputs];
    for k in 0..n {
        let x: Vec<f64> = ins.iter().map(|c| c[k]).collect();
        let mut y = vec![0.0; outputs];
        EveryMethod.eval(&x, &mut y);
        for (column, value) in want.iter_mut().zip(y) {
            column[k] = value;
        }
    }
    want
}

/// Every value's bits, NaN canonical (VERIFICATION.md §9.4).
fn column_bits(columns: &[Vec<f64>]) -> Vec<Vec<u64>> {
    let canonical = |v: &f64| if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() };
    columns.iter().map(|c| c.iter().map(canonical).collect()).collect()
}

/// `batch::run_kernel` refuses an input or output column of another length with a `Shape` error under every policy,
/// and runs an empty batch.
fn assert_kernel_shapes_are_refused(columns: &[Vec<f64>]) {
    let parallel = ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() };
    for exec in [ExecPolicy::Reference, parallel, ExecPolicy::Simd] {
        let mut out = vec![0.0; 65];
        let short_input = batch::run_kernel(&EveryMethod, &[&columns[0], &columns[1][1..]], &mut [&mut out], exec);
        assert!(matches!(short_input, Err(Error::Shape { expected: 65, found: 64 })), "{exec:?}: {short_input:?}");
        let mut short = vec![0.0; 64];
        let short_output = batch::run_kernel(&EveryMethod, &[&columns[0]], &mut [&mut out, &mut short], exec);
        assert!(matches!(short_output, Err(Error::Shape { expected: 65, found: 64 })), "{exec:?}: {short_output:?}");
        let empty = batch::run_kernel(&EveryMethod, &[&[]], &mut [&mut []], exec);
        assert!(empty.is_ok(), "{exec:?}: empty batch {empty:?}");
    }
}

/// The part of α a Helmholtz batch hook evaluates.
#[derive(Clone, Copy, Debug)]
enum Part {
    /// `residual_batch` against `residual`.
    Residual,
    /// `ideal_batch` against `ideal`.
    Ideal,
}

/// One fluid, part, order and policy of [`helmholtz_batches_are_per_point`]: every entry of every point bit for bit,
/// NaN canonical as in the cross-target hash (VERIFICATION.md §9.4), since Rust leaves NaN bit patterns unspecified.
fn assert_batch_is_per_point(
    name: &str,
    eos: &dyn HelmholtzModel,
    part: Part,
    (t, rho): (&[f64], &[f64]),
    order: Order,
    exec: ExecPolicy,
) {
    let mut out = vec![Derivs::zero(order); t.len()];
    let called = match part {
        Part::Residual => eos.residual_batch(t, rho, order, &mut out, exec),
        Part::Ideal => eos.ideal_batch(t, rho, order, &mut out, exec),
    };
    assert!(called.is_ok(), "{name} {part:?} {order:?} {exec:?}, {} points: {called:?}", t.len());
    let bits = |d: &Derivs| {
        (0..=4)
            .flat_map(|n| (0..=n).map(move |i| (i, n - i)))
            .map(|(i, j)| d.get(i, j).map(|v| if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() }))
            .collect::<Vec<_>>()
    };
    for (i, got) in out.iter().enumerate() {
        let want = match part {
            Part::Residual => eos.residual(t[i], rho[i], order),
            Part::Ideal => eos.ideal(t[i], rho[i], order),
        };
        assert_eq!(bits(got), bits(&want), "{name} {part:?} {order:?} {exec:?} point {i}: T {} ρ {}", t[i], rho[i]);
    }
}
