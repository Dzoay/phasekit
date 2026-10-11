//! `pt_flash` (PLAN.md M7.2; VERIFICATION.md §12): the PT flash plus h, the C++ baseline's `pt` row, on its grid:
//! SplitMix64 seed 1, T ~ U[1.05·T_c, min(1.5·T_c, T_max)], ρ ~ U[0.1, 2]·ρ_c (published critical point), p from
//! each (T, ρ) before the clock. Target ≤ 3 µs (ARCHITECTURE.md §7); CoolProp 19-27 µs through Python, its C++
//! 3.4-25 µs. Recorded by `cargo xtask bench --record`.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::internal::FluidRecord;
    use phasekit_core::{Basis, Density, Fluid, Input, Pressure, Registry, Temperature};
    use phasekit_verify::SplitMix64;

    /// The bench fluids (VERIFICATION.md §12).
    const FLUIDS: [&str; 5] = ["Water", "Methane", "R134a", "n-Propane", "n-Heptane"];

    /// States per fluid, cycled through one per iteration.
    const STATES: usize = 1024;

    /// The baseline's single-phase (T, ρ) grid as PT inputs, p the state's at each.
    fn single(fluid: &Fluid, record: &FluidRecord) -> Option<Vec<Input>> {
        let c = record.critical?;
        let (t_hi, mut rng) = ((1.5 * c.t).min(record.limits.t_max()), SplitMix64::new(1));
        (0..STATES)
            .map(|_| {
                let (rho, t) = (rng.uniform(0.1 * c.rho, 2.0 * c.rho), rng.uniform(1.05 * c.t, t_hi));
                let t = Temperature::new(t).ok()?;
                let p = fluid.state(Input::dt(Density::molar(rho).ok()?, t)).ok()?.p();
                Some(Input::pt(Pressure::new(p).ok()?, t))
            })
            .collect()
    }

    fn pt_flash(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        for name in FLUIDS {
            let (Ok(fluid), Ok(record)) = (registry.get(name), phasekit_core::internal::record(registry, name)) else {
                return;
            };
            let Some(states) = single(fluid, &record) else { return };
            let mut g = c.benchmark_group("pt_flash");
            g.bench_function(BenchmarkId::new(name, "pt_h"), |b| {
                let mut k = 0;
                b.iter(|| {
                    k = (k + 1) % states.len();
                    std::hint::black_box(fluid.state(std::hint::black_box(states[k])).map(|s| s.h(Basis::Molar)))
                });
            });
            g.finish();
        }
    }

    criterion_group!(benches, pt_flash);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
