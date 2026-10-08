//! `alphar_order2` (PLAN.md M3.8; VERIFICATION.md §12 workload 1): the order-2 α^r bundle at (T, ρ) of the bench
//! fluids, through `HelmholtzModel::residual` (one virtual call, one `match` per block, D3), on the C++ baseline's grid:
//! SplitMix64 seed 1, T ~ U[1.05·T_c, min(1.5·T_c, T_max)], ρ ~ U[0.1, 2]·ρ_c (published critical point). Target
//! ≤ 0.3 µs for 16-20 terms (ARCHITECTURE.md §7); the baseline's C++ CoolProp: 0.37 µs (n-Heptane, 12 terms) to
//! 1.2 µs (Methane, 40), 2.7 µs (Water, 56; recorded from M5.11). `overhead` evaluates a residual part
//! with no terms: the fixed cost of a call (virtual dispatch, the per-block `match`, the per-state τ and δ tables),
//! which bounds the dispatch share. Recorded by `cargo xtask bench --record`.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::internal::EosRecord;
    use phasekit_core::{HelmholtzModel, Order, Registry};
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
        g.bench_function(BenchmarkId::new(label, "residual"), |b| {
            let mut k = 0;
            b.iter(|| {
                k = (k + 1) % states.len();
                let (t, rho) = states[k];
                std::hint::black_box(model.residual(std::hint::black_box(t), std::hint::black_box(rho), Order::Two))
            });
        });
        g.finish();
    }

    fn alphar_order2(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        for name in FLUIDS {
            let Ok(record) = phasekit_core::internal::record(registry, name) else { return };
            let eos = term::residual_part(&record.eos);
            let (Ok(model), Some(states)) = (term::residual_model(&record, &eos), grid(&record)) else { return };
            run(c, "alphar_order2", name, model.eos(), &states);
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
