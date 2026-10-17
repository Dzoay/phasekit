//! A mixture's DT (PLAN.md M13.10; ROT-190): R32/R125 at 50/50 and 250 K, on single-phase states (the compressed liquid
//! and the gas either side of the dome) and inside it (the bulk densities at β = 0.1 ... 0.9). `direct`: the DT flash with the
//! phase hint, `Strategy::Direct`'s state at (T, ρ); `stable`: without the hint, which tests the homogeneous state's
//! stability at its own pressure first; `dome`: without the hint inside the dome, where the test fails and the flash
//! finds the isochoric split. ARCHITECTURE.md §7 sets no target; M13.15 records it on the reference machine.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::{Basis, DataSet, FlashOptions, Fluid, Input, Pair, Phase, Registry};

    /// The single-phase states (ρ, phase), from PT flashes at 250 K either side of the dome, and the densities inside it,
    /// of the two phases at β = 0.1 ... 0.9 there.
    fn states(fluid: &Fluid) -> (Vec<(f64, Phase)>, Vec<f64>) {
        let single = [1e5, 2e5, 1e6, 2e6]
            .iter()
            .filter_map(|p| Input::new(Pair::PT, *p, 250.0, Basis::Molar).ok())
            .filter_map(|input| fluid.flash(input, &FlashOptions::new()).ok())
            .filter(|state| state.quality().is_none())
            .map(|state| (state.rho(Basis::Molar), state.phase()))
            .collect();
        let mixture = (fluid.model() as &dyn std::any::Any).downcast_ref::<phasekit_mix::Mixture>();
        let dome = (1..10)
            .filter_map(|k| mixture?.coexisting(0.1 * f64::from(k), phasekit_mix::Given::T(250.0)).ok())
            .map(|c| 1.0 / ((1.0 - c.beta) / c.rho_liquid + c.beta / c.rho_vapour))
            .collect();
        (single, dome)
    }

    fn mixture_dt(c: &mut Criterion) {
        let Ok(registry) = Registry::from_embedded(DataSet::Parity) else { return };
        let Ok(fluid) = phasekit_mix::mixture(&registry, &[("R32", 0.5), ("R125", 0.5)], DataSet::Parity) else {
            return;
        };
        let (single, dome) = states(&fluid);
        let dt = |rho: f64, opts: &FlashOptions| {
            Input::new(Pair::DT, rho, 250.0, Basis::Molar).and_then(|input| fluid.flash(input, opts))
        };
        let mut g = c.benchmark_group("mixture_dt");
        g.bench_function(BenchmarkId::new("R32&R125", "direct"), |b| {
            let mut k = 0;
            b.iter(|| {
                k = (k + 1) % single.len();
                let (rho, phase) = single[k];
                std::hint::black_box(dt(rho, &FlashOptions::new().with_phase(phase)).map(|s| s.p()))
            });
        });
        for (case, states) in [("stable", single.iter().map(|(rho, _)| *rho).collect::<Vec<_>>()), ("dome", dome)] {
            g.bench_function(BenchmarkId::new("R32&R125", case), |b| {
                let mut k = 0;
                b.iter(|| {
                    k = (k + 1) % states.len();
                    std::hint::black_box(dt(states[k], &FlashOptions::new()).map(|s| s.p()))
                });
            });
        }
        g.finish();
    }

    criterion_group!(benches, mixture_dt);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
