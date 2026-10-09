//! `qt_superancillary` and `pq_superancillary` (PLAN.md M6.11; VERIFICATION.md §12): the QT and PQ flashes on the
//! superancillary, on the C++ baseline's saturation grid, SplitMix64 seed 1, T ~ U[T_t + 0.05 (T_c − T_t), 0.98 T_c] and
//! Q ~ U[0, 1], PQ at the superancillary's p of each T. Target ≤ 0.1 µs (ARCHITECTURE.md §7); CoolProp 0.45 / 0.64 µs.
//! Recorded by `cargo xtask bench --record`.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::internal::FluidRecord;
    use phasekit_core::{DataSet, Fluid, Input, Pressure, Quality, Registry, Temperature};
    use phasekit_verify::SplitMix64;

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
    fn run<R>(c: &mut Criterion, id: (&str, &str), states: &[Input], work: impl Fn(Input) -> R) {
        let (group, fluid) = id;
        let mut g = c.benchmark_group(group);
        g.bench_function(BenchmarkId::new(fluid, "flash"), |b| {
            let mut k = 0;
            b.iter(|| {
                k = (k + 1) % states.len();
                std::hint::black_box(work(std::hint::black_box(states[k])))
            });
        });
        g.finish();
    }

    fn saturation(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        for name in FLUIDS {
            let (Ok(fluid), Ok(record)) = (registry.get(name), phasekit_core::internal::record(registry, name)) else {
                return;
            };
            let Some((qt, pq)) = grid(&record) else { return };
            let flash = |input| Fluid::state(fluid, input);
            run(c, ("qt_superancillary", name), &qt, flash);
            run(c, ("pq_superancillary", name), &pq, flash);
        }
    }

    criterion_group!(benches, saturation);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
