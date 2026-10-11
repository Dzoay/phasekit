//! `ph_flash` (PLAN.md M7.3; VERIFICATION.md §12): the PH flash plus T, the C++ baseline's `ph` row, on its grid:
//! SplitMix64 seed 1, T ~ U[1.05·T_c, min(1.5·T_c, T_max)], ρ ~ U[0.1, 2]·ρ_c (published critical point), p and h from
//! each (T, ρ) before the clock. Target ≤ 15 µs (ARCHITECTURE.md §7); CoolProp 119-376 µs through Python, its C++
//! 37-255 µs. Recorded by `cargo xtask bench --record`.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::internal::FluidRecord;
    use phasekit_core::{Basis, Density, Enthalpy, Fluid, Input, Pressure, Registry, Temperature};
    use phasekit_verify::SplitMix64;

    /// The bench fluids (VERIFICATION.md §12).
    const FLUIDS: [&str; 5] = ["Water", "Methane", "R134a", "n-Propane", "n-Heptane"];

    /// States per fluid, cycled through one per iteration.
    const STATES: usize = 1024;

    /// The baseline's single-phase (T, ρ) grid as PH inputs, p and h the state's at each.
    fn single(fluid: &Fluid, record: &FluidRecord) -> Option<Vec<Input>> {
        let c = record.critical?;
        let (t_hi, mut rng) = ((1.5 * c.t).min(record.limits.t_max()), SplitMix64::new(1));
        (0..STATES)
            .map(|_| {
                let (rho, t) = (rng.uniform(0.1 * c.rho, 2.0 * c.rho), rng.uniform(1.05 * c.t, t_hi));
                let state = fluid.state(Input::dt(Density::molar(rho).ok()?, Temperature::new(t).ok()?)).ok()?;
                Some(Input::hp(Enthalpy::molar(state.h(Basis::Molar)).ok()?, Pressure::new(state.p()).ok()?))
            })
            .collect()
    }

    fn ph_flash(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        for name in FLUIDS {
            let (Ok(fluid), Ok(record)) = (registry.get(name), phasekit_core::internal::record(registry, name)) else {
                return;
            };
            let Some(states) = single(fluid, &record) else { return };
            let mut g = c.benchmark_group("ph_flash");
            g.bench_function(BenchmarkId::new(name, "ph_t"), |b| {
                let mut k = 0;
                b.iter(|| {
                    k = (k + 1) % states.len();
                    std::hint::black_box(fluid.state(std::hint::black_box(states[k])).map(|s| s.t()))
                });
            });
            g.finish();
        }
    }

    criterion_group!(benches, ph_flash);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
