//! `qt_superancillary` and `pq_superancillary` (PLAN.md M6.11; VERIFICATION.md §12): the QT and PQ flashes on the
//! superancillary plus h, the property a caller reads (user decision PF1), on the C++ baseline's saturation grid,
//! SplitMix64 seed 1, T ~ U[T_t + 0.05 (T_c − T_t), 0.98 T_c] and Q ~ U[0, 1], PQ at the superancillary's p of each T.
//! Target: ARCHITECTURE.md §7. `qt_superancillary_batch` and `pq_superancillary_batch` (PLAN.md M12.9) evaluate that
//! grid as one `batch::evaluate` call of QT or PQ with h, reported per state: `scalar` under `ExecPolicy::Reference`
//! (`scalar_libm` built with `--features phasekit-core/libm`), and `simd`, built with `--features simd`, under
//! `ExecPolicy::Simd`, the curve's Clenshaw sums and both phases' bundles in lanes at the CPU's widest level. Recorded by
//! `cargo xtask bench --record`.
//! Built with `--features phasekit-core/libm`, the cases are `qt_h_libm` and `pq_h_libm` (`backend::case`; PLAN.md
//! M23.1). In the `fast` builds the batches' cases are `scalar_fast` and `simd_fast`, as `dt.rs`'s, so that no build's
//! case shares another's id in one results file.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use std::time::{Duration, Instant};

    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::batch::{self, BatchRequest, ExecPolicy, Status};
    use phasekit_core::internal::FluidRecord;
    use phasekit_core::math::BACKEND;
    use phasekit_core::{Basis, DataSet, Fluid, Input, Pressure, Prop, Quality, Registry, Temperature};
    use phasekit_verify::SplitMix64;
    use phasekit_verify::backend::case;

    /// The bench fluids (VERIFICATION.md §12).
    const FLUIDS: [&str; 5] = ["Water", "Methane", "R134a", "n-Propane", "n-Heptane"];

    /// States per fluid, cycled through one per iteration.
    const STATES: usize = 1024;

    /// The baseline's saturation grid as QT and PQ inputs, PQ at the `Corrected` superancillary's p.
    fn grid(record: &FluidRecord) -> Option<(Vec<Input>, Vec<Input>)> {
        let mut record = record.clone();
        record.apply(DataSet::Corrected).ok()?;
        let (c, curve) = (record.critical?, record.superancillary_curve()?);
        let t_t = record.limits.t_triple().unwrap_or(record.limits.t_min());
        let mut rng = SplitMix64::new(1);
        let (mut qt, mut pq) = (Vec::with_capacity(STATES), Vec::with_capacity(STATES));
        for _ in 0..STATES {
            let (t, q) = (rng.uniform(t_t + 0.05 * (c.t - t_t), 0.98 * c.t), rng.uniform(0.0, 1.0));
            let (temperature, quality) = (Temperature::new(t).ok()?, Quality::new(q).ok()?);
            qt.push(Input::qt(quality, temperature));
            pq.push(Input::pq(Pressure::new(curve.at_t(t).ok()?.bubble.p).ok()?, quality));
        }
        Some((qt, pq))
    }

    /// Times `work` over `states`, one state per iteration.
    fn run<R>(c: &mut Criterion, id: (&str, &str, &str), states: &[Input], work: impl Fn(Input) -> R) {
        let (group, fluid, name) = id;
        let mut g = c.benchmark_group(group);
        g.bench_function(BenchmarkId::new(fluid, name), |b| {
            let mut k = 0;
            b.iter(|| {
                k = (k + 1) % states.len();
                std::hint::black_box(work(std::hint::black_box(states[k])))
            });
        });
        g.finish();
    }

    /// `states` as one batch of h per iteration under each policy it times, per state, in `group`: criterion's median is
    /// then comparable with `run`'s one state per iteration.
    fn run_batch(c: &mut Criterion, (group, name): (&str, &str), fluid: &Fluid, states: &[Input]) {
        let Some(pair) = states.first().map(Input::pair) else { return };
        let (x, y): (Vec<f64>, Vec<f64>) = states.iter().map(|input| input.values()).unzip();
        let outputs = [Prop::Hmolar];
        let (mut out, mut status) = (vec![0.0; states.len()], vec![Status::Ok; states.len()]);
        let per_state = u32::try_from(states.len()).unwrap_or(u32::MAX);
        let backend = case(phasekit_core::math::BACKEND, "scalar");
        let lanes = if phasekit_core::math::BACKEND == "fast" { "simd_fast" } else { "simd" };
        let simd = cfg!(feature = "simd").then_some((ExecPolicy::Simd, lanes));
        let mut g = c.benchmark_group(group);
        for (exec, case) in [(ExecPolicy::Reference, backend.as_str())].into_iter().chain(simd) {
            let req = BatchRequest::new(pair, Basis::Molar, &x, &y, &outputs).with_exec(exec);
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

    fn saturation(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        for name in FLUIDS {
            let (Ok(fluid), Ok(record)) = (registry.get(name), phasekit_core::internal::record(registry, name)) else {
                return;
            };
            let Some((qt, pq)) = grid(&record) else { return };
            let flash_h = |input| Fluid::state(fluid, input).map(|state| state.h(Basis::Molar));
            run(c, ("qt_superancillary", name, &case(BACKEND, "qt_h")), &qt, flash_h);
            run(c, ("pq_superancillary", name, &case(BACKEND, "pq_h")), &pq, flash_h);
            run_batch(c, ("qt_superancillary_batch", name), fluid, &qt);
            run_batch(c, ("pq_superancillary_batch", name), fluid, &pq);
        }
    }

    criterion_group!(benches, saturation);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
