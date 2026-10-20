//! `alphar_order2` (PLAN.md M3.8; VERIFICATION.md §12 workload 1): the order-2 α^r bundle at (T, ρ) of the bench
//! fluids, through `HelmholtzModel::residual` (one virtual call, one `match` per block, D3), on the C++ baseline's grid:
//! SplitMix64 seed 1, T ~ U[1.05·T_c, min(1.5·T_c, T_max)], ρ ~ U[0.1, 2]·ρ_c (published critical point). Target
//! ≤ 0.3 µs for 16-20 terms (ARCHITECTURE.md §7); the baseline's C++ CoolProp: 0.37 µs (n-Heptane, 12 terms) to
//! 1.2 µs (Methane, 40), 2.7 µs (Water, 56; recorded from M5.11). `overhead` evaluates a residual part
//! with no terms: the fixed cost of a call (virtual dispatch, the per-block `match`, the per-state τ and δ tables),
//! which bounds the dispatch share. `alphar_batch` (PLAN.md M12.1) evaluates the same grid as one
//! `HelmholtzModel::residual_batch` call under `ExecPolicy::Reference`, reported per state: `scalar` with std's maths,
//! `scalar_libm` built with `--features phasekit-core/libm`; M12's SIMD gate is measured against `scalar` (user decision
//! SL3). `simd` (PLAN.md M12.4), built with `--features simd`, times the same call under `ExecPolicy::Simd` with core's
//! `simd` feature: the lanes at the CPU's widest level. Recorded by `cargo xtask bench --record`.
//! Built with `--features phasekit-core/libm`, `alphar_order2`'s is `residual_libm` (`backend::case`; PLAN.md M23.1).

#[cfg(not(target_family = "wasm"))]
mod bench {
    use std::time::{Duration, Instant};

    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::batch::ExecPolicy;
    use phasekit_core::internal::EosRecord;
    use phasekit_core::math::BACKEND;
    use phasekit_core::{Derivs, HelmholtzModel, Order, Registry};
    use phasekit_verify::backend::case;
    use phasekit_verify::{SplitMix64, term};

    /// The bench fluids (VERIFICATION.md §12).
    const FLUIDS: [&str; 5] = ["Water", "Methane", "R134a", "n-Propane", "n-Heptane"];

    /// States per fluid, cycled through one per iteration.
    const STATES: usize = 1024;

    /// The baseline's (T, ρ) grid for `record`.
    fn grid(record: &phasekit_core::internal::FluidRecord) -> Option<Vec<(f64, f64)>> {
        let c = record.critical?;
        let (t_hi, mut rng) = ((1.5 * c.t).min(record.limits.t_max()), SplitMix64::new(1));
        Some((0..STATES).map(|_| (rng.uniform(1.05 * c.t, t_hi), rng.uniform(0.1 * c.rho, 2.0 * c.rho))).collect())
    }

    fn run(c: &mut Criterion, group: &str, label: &str, model: &dyn HelmholtzModel, states: &[(f64, f64)]) {
        let mut g = c.benchmark_group(group);
        g.bench_function(BenchmarkId::new(label, case(BACKEND, "residual")), |b| {
            let mut k = 0;
            b.iter(|| {
                k = (k + 1) % states.len();
                let (t, rho) = states[k];
                std::hint::black_box(model.residual(std::hint::black_box(t), std::hint::black_box(rho), Order::Two))
            });
        });
        g.finish();
    }

    /// `states` as one `residual_batch` call per iteration under each policy it times, per state: criterion's median
    /// is then comparable with `run`'s one state per iteration.
    fn run_batch(c: &mut Criterion, label: &str, model: &dyn HelmholtzModel, states: &[(f64, f64)]) {
        let (t, rho): (Vec<f64>, Vec<f64>) = states.iter().copied().unzip();
        let mut out = vec![Derivs::zero(Order::Two); states.len()];
        let per_state = u32::try_from(states.len()).unwrap_or(u32::MAX);
        let backend = case(BACKEND, "scalar");
        let lanes = if BACKEND == "fast" { "simd_fast" } else { "simd" };
        let simd = cfg!(feature = "simd").then_some((ExecPolicy::Simd, lanes));
        let mut g = c.benchmark_group("alphar_batch");
        for (exec, case) in [(ExecPolicy::Reference, backend.as_str())].into_iter().chain(simd) {
            g.bench_function(BenchmarkId::new(label, case), |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();
                    for _ in 0..iters {
                        let batch = model.residual_batch(&t, &rho, Order::Two, &mut out, exec);
                        std::hint::black_box((batch.is_ok(), &mut out));
                    }
                    start.elapsed().checked_div(per_state).unwrap_or(Duration::ZERO)
                });
            });
        }
        g.finish();
    }

    fn alphar_order2(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        for name in FLUIDS {
            let Ok(record) = phasekit_core::internal::record(registry, name) else { return };
            let eos = term::residual_part(&record.eos);
            let (Ok(model), Some(states)) = (term::residual_model(&record, &eos), grid(&record)) else { return };
            run(c, "alphar_order2", name, model.eos(), &states);
            run_batch(c, name, model.eos(), &states);
        }
        // The fixed cost: Methane's constants and grid, no terms.
        let Ok(record) = phasekit_core::internal::record(registry, "Methane") else { return };
        let e = &record.eos;
        let empty = EosRecord::new(e.gas_constant, e.t_reducing, e.rho_reducing, e.rho_max);
        let (Ok(model), Some(states)) = (term::residual_model(&record, &empty), grid(&record)) else { return };
        run(c, "alphar_order2", "overhead", model.eos(), &states);
    }

    criterion_group!(benches, alphar_order2);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
