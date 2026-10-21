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
//! - `mixture_alphar_batch` (PLAN.md M22.1): workload 1 as one `HelmholtzModel::residual_batch` call over 1024 states,
//!   reported per state, for M22's bench mixtures (`phasekit_verify::mixtures`). The grid is workload 1's with the
//!   mixture's reducing state in place of its critical point: SplitMix64 seed 1, T ~ U[1.05·T_r, 1.5·T_r], ρ ~ U[0.1,
//!   2]·ρ_r. `scalar` runs under `ExecPolicy::Reference`; `simd`, built with `--features simd`, under
//!   `ExecPolicy::Simd`, where the built-in components run core's lanes.
//! - `mixture_alphar_parts` (PLAN.md M22.1): where that time goes, per state on the same grid. `residual`: the order-2
//!   bundle point by point, the path before M22.1. `reducing`: the reducing state at z, which the point pays each time
//!   and the batch once. `components` and `components_simd`: every component's own `residual_batch` over its scaled
//!   states. `no_departures` and `no_departures_simd`: the batch with every F set to 0, so the departures' share is the
//!   batch's time less this one's.
//! - `mixture_flash` (PLAN.md M22.5): flashes whose liquid and vapour at one T a mixture evaluates side by side, on
//!   R32/R125 rebuilt under each policy (`Mixture::with_exec`): `pt` between the bubble and dew pressures at 250 K
//!   (the stability test, then the split's substitution and Newton steps), `qt` from 230 to 270 K and `pq` at those
//!   temperatures' bubble pressures, each at Q = 0, 0.5 and 1 (the saturation solver's Newton steps). `pt`, `qt` and
//!   `pq` under `ExecPolicy::Reference`, one phase after the other; `<case>_simd`, built with `--features simd`, under
//!   `ExecPolicy::Simd`, the phases' composition derivatives in core's lanes.
//! - `mixture_dt_batch` (PLAN.md M22.4): DT with h and c_p under the gas hint (`Strategy::Direct`) as one
//!   `batch::evaluate` call on the same grid, reported per state; `scalar` under `ExecPolicy::Reference`, `simd` under
//!   `ExecPolicy::Simd`, where the mixture's group hook evaluates the bundles side by side.
//!
//! Built without `simd`, each `simd` case is a `lanes` case (`<case>_lanes`): `ExecPolicy::Simd`'s two lanes at the
//! target's baseline, the default build's lanes, whose measurement decides whether a kernel ships (user decision ML6).
//! In the `libm` build, and so in the `simd` build, a case that is not `simd` is named `<case>_libm`
//! (`backend::case`). In the `fast` builds every case carries `_fast`, the `simd` ones too (`<case>_simd_fast`, as
//! `alphar.rs`'s `simd_fast`), so that no build's case shares another's id in one results file.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::batch::{self, BatchRequest, ExecPolicy, Status};
    use phasekit_core::math::BACKEND;
    use phasekit_core::{
        Basis, DataSet, Derivs, FlashOptions, Fluid, HelmholtzModel, Input, Order, Pair, Phase, Prop, Registry,
    };
    use phasekit_mix::{Binary, Given, Mixture, MultiFluid, Side};
    use phasekit_verify::SplitMix64;
    use phasekit_verify::backend::case;
    use phasekit_verify::mixtures::{BENCH_MIXTURES, BenchMixture, MixtureModel};

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

    /// R32/R125, its flashes' phases under `exec`.
    fn r32_r125_under(exec: ExecPolicy) -> Option<Fluid> {
        let fluid = r32_r125()?;
        let mix = mixture(&fluid)?;
        let rebuilt = Mixture::new(mix.model().clone(), mix.composition()).ok()?;
        Some(Fluid::new(Arc::new(rebuilt.with_exec(exec))))
    }

    fn mixture_flash(c: &mut Criterion) {
        let Some(fluid) = r32_r125() else { return };
        let Some(mix) = mixture(&fluid) else { return };
        let bubble = |t: f64| mix.saturation(Side::Bubble, Given::T(t)).map(|s| s.p);
        let pt: Vec<(Pair, f64, f64)> = match (bubble(T), mix.saturation(Side::Dew, Given::T(T)).map(|s| s.p)) {
            (Ok(bubble), Ok(dew)) => {
                (1..10).map(|k| (Pair::PT, dew + 0.1 * f64::from(k) * (bubble - dew), T)).collect()
            }
            _ => Vec::new(),
        };
        let temperatures: Vec<f64> = (0..9).map(|k| 230.0 + 5.0 * f64::from(k)).collect();
        let qualities = [0.0, 0.5, 1.0];
        let qt: Vec<(Pair, f64, f64)> =
            qualities.iter().flat_map(|q| temperatures.iter().map(|t| (Pair::QT, *q, *t))).collect();
        let pressures: Vec<f64> = temperatures.iter().filter_map(|t| bubble(*t).ok()).collect();
        let pq: Vec<(Pair, f64, f64)> =
            qualities.iter().flat_map(|q| pressures.iter().map(|p| (Pair::PQ, *p, *q))).collect();
        let flash = |fluid: &Fluid, (pair, x, y): (Pair, f64, f64)| {
            Input::new(pair, x, y, Basis::Molar).and_then(|input| fluid.state(input)).map(|s| s.h(Basis::Molar))
        };
        let mut g = c.benchmark_group("mixture_flash");
        for (name, states) in [("pt", &pt), ("qt", &qt), ("pq", &pq)] {
            for (exec, case) in policies(name) {
                let Some(fluid) = r32_r125_under(exec) else { continue };
                cycle(&mut g, &case, states, |state| flash(&fluid, state));
            }
        }
        g.finish();
    }

    /// States of the batch benches' grid.
    const STATES: usize = 1024;

    /// M22's bench mixtures, each with its label, on the `Parity` library.
    fn bench_mixtures() -> Vec<(&'static str, Fluid)> {
        let Ok(registry) = Registry::from_embedded(DataSet::Parity) else { return Vec::new() };
        let build = |m: &BenchMixture| match m.model {
            MixtureModel::Library => phasekit_mix::mixture(&registry, m.components, DataSet::Parity),
            MixtureModel::Gerg2008 => phasekit_mix::gerg2008(&registry, m.components),
        };
        BENCH_MIXTURES.iter().filter_map(|m| Some((m.label, build(m).ok()?))).collect()
    }

    /// The batch benches' grid of `eos`: workload 1's at its reducing state, as columns of T and ρ.
    fn reduced_grid(eos: &dyn HelmholtzModel) -> Option<(Vec<f64>, Vec<f64>)> {
        let r = eos.reducing()?;
        let mut rng = SplitMix64::new(1);
        Some((0..STATES).map(|_| (rng.uniform(1.05 * r.t, 1.5 * r.t), rng.uniform(0.1 * r.rho, 2.0 * r.rho))).unzip())
    }

    /// Times `run` over the grid's `states` once per iteration, reported per state, as the group's `label`/`case`.
    fn per_state(
        g: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
        (label, case): (&str, &str),
        states: usize,
        mut run: impl FnMut(),
    ) {
        let states = u32::try_from(states).unwrap_or(u32::MAX);
        g.bench_function(BenchmarkId::new(label, case), |b| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                for _ in 0..iters {
                    run();
                }
                start.elapsed().checked_div(states).unwrap_or(Duration::ZERO)
            });
        });
    }

    /// The policies a batch bench runs under: `Reference` as `case` on this build's maths, and `Simd` as `<case>_simd`
    /// (`simd` for the batch itself) in the `simd` build, else as `<case>_lanes` (`lanes`), two lanes at the target's
    /// baseline, on this build's maths (user decision ML6); `_fast` on either in a `fast` build.
    fn policies(case_name: &str) -> Vec<(ExecPolicy, String)> {
        let lanes = if cfg!(feature = "simd") { "simd" } else { "lanes" };
        let simd = if case_name == "scalar" { lanes.to_string() } else { format!("{case_name}_{lanes}") };
        let simd = if cfg!(feature = "simd") && BACKEND != "fast" { simd } else { case(BACKEND, &simd) };
        vec![(ExecPolicy::Reference, case(BACKEND, case_name)), (ExecPolicy::Simd, simd)]
    }

    /// `eos`'s order-2 `residual_batch` over the grid under each policy, as `label`/`case_name`.
    fn batch(
        g: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
        (label, case_name): (&str, &str),
        eos: &dyn HelmholtzModel,
        (t, rho): (&[f64], &[f64]),
    ) {
        let mut out = vec![Derivs::zero(Order::Two); t.len()];
        for (exec, case) in policies(case_name) {
            per_state(g, (label, &case), t.len(), || {
                let batch = eos.residual_batch(t, rho, Order::Two, &mut out, exec);
                std::hint::black_box((batch.is_ok(), &mut out));
            });
        }
    }

    fn mixture_alphar_batch(c: &mut Criterion) {
        let mut g = c.benchmark_group("mixture_alphar_batch");
        for (label, fluid) in bench_mixtures() {
            let Some(eos) = fluid.model().helmholtz() else { continue };
            let Some((t, rho)) = reduced_grid(eos) else { continue };
            batch(&mut g, (label, "scalar"), eos, (&t, &rho));
        }
        g.finish();
    }

    /// `mix` with every departure function's F set to 0, so that no pair evaluates one.
    fn without_departures(mix: &Mixture) -> Option<Mixture> {
        let n = mix.composition().len();
        let mut model: MultiFluid = mix.model().clone();
        for (i, j) in (0..n).flat_map(|i| (i + 1..n).map(move |j| (i, j))) {
            let binary = model.binary(i, j).filter(|b| b.f != 0.0);
            if let Some(binary) = binary {
                model = model.with_binary(i, j, Binary { f: 0.0, ..binary }).ok()?;
            }
        }
        Mixture::new(model, mix.composition()).ok()
    }

    /// Each component's own model and the grid scaled to its state: (T_r,i/τ, δ ρ_r,i), τ and δ the mixture's.
    fn component_grids<'a>(
        mix: &'a Mixture,
        (t, rho): (&[f64], &[f64]),
    ) -> Vec<(&'a dyn HelmholtzModel, Vec<f64>, Vec<f64>)> {
        let (t_r, rho_r) = mix.model().reducing_state(mix.composition());
        let scaled = |eos: &'a dyn HelmholtzModel| {
            let r = eos.reducing()?;
            let t_i = t.iter().map(|t| r.t / (t_r / t)).collect();
            Some((eos, t_i, rho.iter().map(|rho| rho / rho_r * r.rho).collect()))
        };
        mix.model().fluids().filter_map(|f| scaled(f.model().helmholtz()?)).collect()
    }

    fn mixture_alphar_parts(c: &mut Criterion) {
        let mut g = c.benchmark_group("mixture_alphar_parts");
        for (label, fluid) in bench_mixtures() {
            let (Some(mix), Some(eos)) = (mixture(&fluid), fluid.model().helmholtz()) else { continue };
            let Some((t, rho)) = reduced_grid(eos) else { continue };
            per_state(&mut g, (label, &case(BACKEND, "residual")), t.len(), || {
                for (t, rho) in t.iter().zip(&rho) {
                    std::hint::black_box(eos.residual(std::hint::black_box(*t), *rho, Order::Two));
                }
            });
            per_state(&mut g, (label, &case(BACKEND, "reducing")), t.len(), || {
                for _ in 0..t.len() {
                    std::hint::black_box(mix.model().reducing_state(std::hint::black_box(mix.composition())));
                }
            });
            components(&mut g, label, mix, (&t, &rho));
            if let Some(plain) = without_departures(mix) {
                batch(&mut g, (label, "no_departures"), &plain, (&t, &rho));
            }
        }
        g.finish();
    }

    /// Every component's own order-2 `residual_batch` over its scaled grid, under each policy.
    fn components(
        g: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
        label: &str,
        mix: &Mixture,
        grid: (&[f64], &[f64]),
    ) {
        let grids = component_grids(mix, grid);
        let mut out = vec![Derivs::zero(Order::Two); grid.0.len()];
        for (exec, case) in policies("components") {
            per_state(g, (label, &case), grid.0.len(), || {
                for (eos, t, rho) in &grids {
                    let batch = eos.residual_batch(t, rho, Order::Two, &mut out, exec);
                    std::hint::black_box((batch.is_ok(), &mut out));
                }
            });
        }
    }

    fn mixture_dt_batch(c: &mut Criterion) {
        let outputs = [Prop::Hmolar, Prop::Cpmolar];
        let opts = FlashOptions::new().with_phase(Phase::Gas);
        let mut g = c.benchmark_group("mixture_dt_batch");
        for (label, fluid) in bench_mixtures() {
            let Some(eos) = fluid.model().helmholtz() else { continue };
            let Some((t, rho)) = reduced_grid(eos) else { continue };
            let cells = t.len() * outputs.len();
            let (mut out, mut status) = (vec![0.0; cells], vec![Status::Ok; cells]);
            for (exec, case) in policies("scalar") {
                let req =
                    BatchRequest::new(Pair::DT, Basis::Molar, &rho, &t, &outputs).with_flash(opts).with_exec(exec);
                per_state(&mut g, (label, &case), t.len(), || {
                    let batch = batch::evaluate(&fluid, &req, &mut out, &mut status);
                    std::hint::black_box((batch.is_ok(), &mut out));
                });
            }
        }
        g.finish();
    }

    criterion_group!(
        benches,
        mixture_alphar,
        mixture_alphar_batch,
        mixture_alphar_parts,
        mixture_dt_batch,
        mixture_flash,
        mixture_dt,
        mixture_pt,
        mixture_qt
    );
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
