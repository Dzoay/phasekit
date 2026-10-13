//! `pt_viscosity_conductivity` (PLAN.md M8.12; VERIFICATION.md §12): the PT flash, then η and λ, on the C++ baseline's
//! single-phase grid (SplitMix64 seed 1, T ~ U[1.05·T_c, min(1.5·T_c, T_max)], ρ ~ U[0.1, 2]·ρ_c, p from each (T, ρ)
//! before the clock), so near-critical states run the critical terms. No ARCHITECTURE.md §7 target; CoolProp takes
//! 34 µs for Water through Python (map 10 §8.2). Recorded by `cargo xtask bench --record`.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::internal::FluidRecord;
    use phasekit_core::{Density, Fluid, Input, Pressure, Prop, Registry, Temperature};
    use phasekit_verify::SplitMix64;

    /// The bench fluids (VERIFICATION.md §12), each with η and λ: IAPWS (Water), hard-coded (Methane) and staged.
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

    fn pt_viscosity_conductivity(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        for name in FLUIDS {
            let (Ok(fluid), Ok(record)) = (registry.get(name), phasekit_core::internal::record(registry, name)) else {
                return;
            };
            let Some(states) = single(fluid, &record) else { return };
            // The transport set materialises on the first call, outside the clock (ARCHITECTURE.md §8).
            let first = fluid.state(states[0]).map(|s| fluid.prop(&s, Prop::Viscosity));
            assert!(first.as_ref().is_ok_and(|eta| eta.is_ok()), "{name}: {first:?}");
            let mut g = c.benchmark_group("pt_viscosity_conductivity");
            g.bench_function(BenchmarkId::new(name, "pt_eta_lambda"), |b| {
                let mut k = 0;
                b.iter(|| {
                    k = (k + 1) % states.len();
                    let state = fluid.state(std::hint::black_box(states[k]));
                    let transport = |s| (fluid.prop(&s, Prop::Viscosity), fluid.prop(&s, Prop::Conductivity));
                    std::hint::black_box(state.map(transport))
                });
            });
            g.finish();
        }
    }

    criterion_group!(benches, pt_viscosity_conductivity);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
