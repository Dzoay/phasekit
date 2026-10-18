//! The mixture benches (PLAN.md M13.15; ROT-190, ROT-195), on R32/R125 at 50/50 and, for α^r, a GERG-2008 natural gas
//! of five components. ARCHITECTURE.md §7 sets no mixture target; `cargo xtask bench --record` records them on the
//! reference machine.
//!
//! - `mixture_alphar` (workload 1 at the mixture's composition): the order-2 α^r bundle through
//!   `HelmholtzModel::residual`, on the pure fluids' grid with the mixture's critical point: SplitMix64 seed 1,
//!   T ~ U[1.05·T_c, 1.5·T_c], ρ ~ U[0.1, 2]·ρ_c. Only the pairs with a departure function evaluate one (ROT-195).
//! - `mixture_dt` (PLAN.md M13.10; ROT-190): DT at 250 K on single-phase states (the compressed liquid and the gas
//!   either side of the dome) and inside it (the bulk densities at β = 0.1 ... 0.9). `direct`: the DT flash with the
//!   phase hint, `Strategy::Direct`'s state at (T, ρ); `stable`: without the hint, which tests the homogeneous state's
//!   stability at its own pressure first; `dome`: without the hint inside the dome, where the test fails and the flash
//!   finds the isochoric split.
//! - `mixture_pt` (PLAN.md M13.8): PT at 250 K on the same single-phase states (`single`: the stability test, then one
//!   root) and between the bubble and dew pressures there (`dome`: the test, then the split).
//! - `mixture_qt` (PLAN.md M13.9-M13.10): QT from 230 to 270 K at Q = 0 (`bubble`), 1 (`dew`) and 0.5 (`interior`).

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::{Basis, DataSet, Derivs, FlashOptions, Fluid, Input, Order, Pair, Phase, Registry};
    use phasekit_mix::{Given, Mixture, Side};
    use phasekit_verify::SplitMix64;

    /// The bench mixture's label.
    const LABEL: &str = "R32&R125";

    /// The natural gas's label and composition.
    const GAS: (&str, [(&str, f64); 5]) = (
        "NaturalGas5",
        [("Methane", 0.85), ("Ethane", 0.07), ("n-Propane", 0.03), ("Nitrogen", 0.03), ("CarbonDioxide", 0.02)],
    );

    /// The temperature of the DT and PT states.
    const T: f64 = 250.0;

    /// The bench mixture, R32/R125 at 50/50 on the `Parity` library.
    fn r32_r125() -> Option<Fluid> {
        let registry = Registry::from_embedded(DataSet::Parity).ok()?;
        phasekit_mix::mixture(&registry, &[("R32", 0.5), ("R125", 0.5)], DataSet::Parity).ok()
    }

    /// The mixture model behind `fluid`.
    fn mixture(fluid: &Fluid) -> Option<&Mixture> {
        (fluid.model() as &dyn std::any::Any).downcast_ref::<Mixture>()
    }

    /// The single-phase states (ρ, phase, p) at `T`, from PT flashes either side of the dome, and the densities inside
    /// it, of the two phases at β = 0.1 ... 0.9 there.
    fn states(fluid: &Fluid) -> (Vec<(f64, Phase, f64)>, Vec<f64>) {
        let single = [1e5, 2e5, 1e6, 2e6]
            .iter()
            .filter_map(|p| Input::new(Pair::PT, *p, T, Basis::Molar).ok())
            .filter_map(|input| fluid.flash(input, &FlashOptions::new()).ok())
            .filter(|state| state.quality().is_none())
            .map(|state| (state.rho(Basis::Molar), state.phase(), state.p()))
            .collect();
        let dome = (1..10)
            .filter_map(|k| mixture(fluid)?.coexisting(0.1 * f64::from(k), Given::T(T)).ok())
            .map(|c| 1.0 / ((1.0 - c.beta) / c.rho_liquid + c.beta / c.rho_vapour))
            .collect();
        (single, dome)
    }

    /// Times `flash` over `cases`, one per iteration, as the group's `LABEL`/`case`.
    fn cycle<X: Copy, R>(
        g: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
        case: &str,
        cases: &[X],
        flash: impl Fn(X) -> R,
    ) {
        if cases.is_empty() {
            return;
        }
        g.bench_function(BenchmarkId::new(LABEL, case), |b| {
            let mut k = 0;
            b.iter(|| {
                k = (k + 1) % cases.len();
                std::hint::black_box(flash(cases[k]))
            });
        });
    }

    fn mixture_alphar(c: &mut Criterion) {
        let Ok(registry) = Registry::from_embedded(DataSet::Parity) else { return };
        let gas = phasekit_mix::gerg2008(&registry, &GAS.1);
        let mut g = c.benchmark_group("mixture_alphar");
        for (label, fluid) in [(LABEL, r32_r125()), (GAS.0, gas.ok())] {
            let Some(fluid) = fluid else { continue };
            let (Some(eos), Some(crit)) = (fluid.model().helmholtz(), fluid.model().critical_point()) else { continue };
            let mut rng = SplitMix64::new(1);
            let grid: Vec<(f64, f64)> = (0..1024)
                .map(|_| (rng.uniform(1.05 * crit.t, 1.5 * crit.t), rng.uniform(0.1 * crit.rho, 2.0 * crit.rho)))
                .collect();
            g.bench_function(BenchmarkId::new(label, "residual"), |b| {
                let mut k = 0;
                b.iter(|| {
                    k = (k + 1) % grid.len();
                    let (t, rho) = grid[k];
                    std::hint::black_box::<Derivs>(eos.residual(t, rho, Order::Two))
                });
            });
        }
        g.finish();
    }

    fn mixture_dt(c: &mut Criterion) {
        let Some(fluid) = r32_r125() else { return };
        let (single, dome) = states(&fluid);
        let dt = |rho: f64, opts: &FlashOptions| {
            Input::new(Pair::DT, rho, T, Basis::Molar).and_then(|input| fluid.flash(input, opts)).map(|s| s.p())
        };
        let mut g = c.benchmark_group("mixture_dt");
        cycle(&mut g, "direct", &single, |(rho, phase, _)| dt(rho, &FlashOptions::new().with_phase(phase)));
        let stable: Vec<f64> = single.iter().map(|(rho, ..)| *rho).collect();
        cycle(&mut g, "stable", &stable, |rho| dt(rho, &FlashOptions::new()));
        cycle(&mut g, "dome", &dome, |rho| dt(rho, &FlashOptions::new()));
        g.finish();
    }

    fn mixture_pt(c: &mut Criterion) {
        let Some(fluid) = r32_r125() else { return };
        let Some(mix) = mixture(&fluid) else { return };
        let single: Vec<f64> = states(&fluid).0.iter().map(|(.., p)| *p).collect();
        let sides = [Side::Bubble, Side::Dew].map(|side| mix.saturation(side, Given::T(T)).map(|s| s.p));
        let dome: Vec<f64> = match sides {
            [Ok(bubble), Ok(dew)] => (1..10).map(|k| dew + 0.1 * f64::from(k) * (bubble - dew)).collect(),
            _ => Vec::new(),
        };
        let pt = |p: f64| {
            Input::new(Pair::PT, p, T, Basis::Molar).and_then(|input| fluid.state(input)).map(|s| s.h(Basis::Molar))
        };
        let mut g = c.benchmark_group("mixture_pt");
        cycle(&mut g, "single", &single, pt);
        cycle(&mut g, "dome", &dome, pt);
        g.finish();
    }

    fn mixture_qt(c: &mut Criterion) {
        let Some(fluid) = r32_r125() else { return };
        let temperatures: Vec<f64> = (0..9).map(|k| 230.0 + 5.0 * f64::from(k)).collect();
        let mut g = c.benchmark_group("mixture_qt");
        for (case, q) in [("bubble", 0.0), ("dew", 1.0), ("interior", 0.5)] {
            cycle(&mut g, case, &temperatures, |t| {
                Input::new(Pair::QT, q, t, Basis::Molar).and_then(|input| fluid.state(input)).map(|s| s.p())
            });
        }
        g.finish();
    }

    criterion_group!(benches, mixture_alphar, mixture_dt, mixture_pt, mixture_qt);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
