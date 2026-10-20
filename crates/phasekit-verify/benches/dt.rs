//! `properties_at_t_rho` and `dt_flash` (PLAN.md M5.11; VERIFICATION.md §12 workload 2). `properties_at_t_rho`: the DT
//! flash plus h and c_p, the C++ baseline's `dt_h_cp` row, on its grid: SplitMix64 seed 1, T ~ U[1.05·T_c, min(1.5·T_c,
//! T_max)], ρ ~ U[0.1, 2]·ρ_c (published critical point). Target ≤ 0.5 µs (ARCHITECTURE.md §7); CoolProp 1.5-10.7 µs
//! through Python, its C++ 0.76-3.2 µs. `dt_flash`: the flash alone on that grid (`single`), and inside the dome
//! (`dome`) on the baseline's saturation grid, T ~ U[T_t + 0.05 (T_c − T_t), 0.98 T_c] and Q ~ U[0, 1], ρ from the
//! superancillary's ρ′ and ρ″ by the lever rule: the phase rule, the curve and two bundles. `properties_at_t_rho_batch`
//! (PLAN.md M12.8) evaluates `properties_at_t_rho`'s grid as one `batch::evaluate` call of DT with h and c_p, reported
//! per state: `scalar` under `ExecPolicy::Reference` (`scalar_libm` built with `--features phasekit-core/libm`), and
//! `simd`, built with `--features simd`, under `ExecPolicy::Simd`, the total bundles in lanes at the CPU's widest level.
//! Recorded by `cargo xtask bench --record`.
//! Built with `--features phasekit-core/libm`, `properties_at_t_rho`'s case is `dt_h_cp_libm` (`backend::case`;
//! PLAN.md M23.1).

#[cfg(not(target_family = "wasm"))]
mod bench {
    use std::time::{Duration, Instant};

    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::batch::{self, BatchRequest, ExecPolicy, Status};
    use phasekit_core::internal::FluidRecord;
    use phasekit_core::math::BACKEND;
    use phasekit_core::{Basis, DataSet, Density, Fluid, Input, Pair, Prop, Registry, Temperature};
    use phasekit_verify::SplitMix64;
    use phasekit_verify::backend::case;

    /// The bench fluids (VERIFICATION.md §12).
    const FLUIDS: [&str; 5] = ["Water", "Methane", "R134a", "n-Propane", "n-Heptane"];

    /// States per fluid, cycled through one per iteration.
    const STATES: usize = 1024;

    /// The baseline's single-phase (T, ρ) grid.
    fn single(record: &FluidRecord) -> Option<Vec<Input>> {
        let c = record.critical?;
        let (t_hi, mut rng) = ((1.5 * c.t).min(record.limits.t_max()), SplitMix64::new(1));
        (0..STATES).map(|_| dt(rng.uniform(0.1 * c.rho, 2.0 * c.rho), rng.uniform(1.05 * c.t, t_hi))).collect()
    }

    /// The baseline's saturation grid, as (T, ρ) inside the dome of the `Corrected` superancillary.
    fn dome(record: &FluidRecord) -> Option<Vec<Input>> {
        let mut record = record.clone();
        record.apply(DataSet::Corrected).ok()?;
        let (c, curve) = (record.critical?, record.superancillary_curve()?);
        let t_t = record.limits.t_triple().unwrap_or(record.limits.t_min());
        let mut rng = SplitMix64::new(1);
        let mut states = Vec::with_capacity(STATES);
        for _ in 0..STATES {
            let (t, q) = (rng.uniform(t_t + 0.05 * (c.t - t_t), 0.98 * c.t), rng.uniform(0.0, 1.0));
            let sat = curve.at_t(t).ok()?;
            states.push(dt(1.0 / ((1.0 - q) / sat.bubble.rho + q / sat.dew.rho), t)?);
        }
        Some(states)
    }

    fn dt(rho: f64, t: f64) -> Option<Input> {
        Some(Input::dt(Density::molar(rho).ok()?, Temperature::new(t).ok()?))
    }

    /// Times `work` over `states`, one state per iteration.
    fn run<R>(c: &mut Criterion, id: (&str, &str, &str), states: &[Input], work: impl Fn(Input) -> R) {
        let (group, fluid, case) = id;
        let mut g = c.benchmark_group(group);
        g.bench_function(BenchmarkId::new(fluid, case), |b| {
            let mut k = 0;
            b.iter(|| {
                k = (k + 1) % states.len();
                std::hint::black_box(work(std::hint::black_box(states[k])))
            });
        });
        g.finish();
    }

    /// `states` as one DT batch of h and c_p per iteration under each policy it times, per state: criterion's median is
    /// then comparable with `run`'s one state per iteration.
    fn run_batch(c: &mut Criterion, name: &str, fluid: &Fluid, states: &[Input]) {
        let (rho, t): (Vec<f64>, Vec<f64>) = states.iter().map(|input| input.values()).unzip();
        let outputs = [Prop::Hmolar, Prop::Cpmolar];
        let (mut out, mut status) = (vec![0.0; 2 * states.len()], vec![Status::Ok; 2 * states.len()]);
        let per_state = u32::try_from(states.len()).unwrap_or(u32::MAX);
        let backend = case(BACKEND, "scalar");
        let lanes = if BACKEND == "fast" { "simd_fast" } else { "simd" };
        let simd = cfg!(feature = "simd").then_some((ExecPolicy::Simd, lanes));
        let mut g = c.benchmark_group("properties_at_t_rho_batch");
        for (exec, case) in [(ExecPolicy::Reference, backend.as_str())].into_iter().chain(simd) {
            let req = BatchRequest::new(Pair::DT, Basis::Molar, &rho, &t, &outputs).with_exec(exec);
            g.bench_function(BenchmarkId::new(name, case), |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();
                    for _ in 0..iters {
                        let batch = batch::evaluate(fluid, &req, &mut out, &mut status);
                        std::hint::black_box((batch.is_ok(), &mut out));
                    }
                    start.elapsed().checked_div(per_state).unwrap_or(Duration::ZERO)
                });
            });
        }
        g.finish();
    }

    fn properties_at_t_rho(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        for name in FLUIDS {
            let (Ok(fluid), Ok(record)) = (registry.get(name), phasekit_core::internal::record(registry, name)) else {
                return;
            };
            let Some(states) = single(&record) else { return };
            let h_cp = |input| fluid.state(input).map(|s| (s.h(Basis::Molar), s.cp(Basis::Molar)));
            run(c, ("properties_at_t_rho", name, &case(BACKEND, "dt_h_cp")), &states, h_cp);
            run_batch(c, name, fluid, &states);
        }
    }

    fn dt_flash(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        for name in FLUIDS {
            let (Ok(fluid), Ok(record)) = (registry.get(name), phasekit_core::internal::record(registry, name)) else {
                return;
            };
            let flash = |input| Fluid::state(fluid, input);
            let (Some(single), Some(dome)) = (single(&record), dome(&record)) else { return };
            run(c, ("dt_flash", name, "single"), &single, flash);
            run(c, ("dt_flash", name, "dome"), &dome, flash);
        }
    }

    criterion_group!(benches, properties_at_t_rho, dt_flash);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
