//! `scaling_same_and_different_fluids` (PLAN.md M9.2; VERIFICATION.md §12; ARCHITECTURE.md §7 thread scaling, target ≥
//! 0.9·N): throughput of DT + h + c_p at 1, 2, 4, the physical cores and the hardware threads, relative to one thread.
//! The method is the C++ baseline's scaling rows (`coolprop-8.0.0-<machine>.scaling.csv`), so the speedups compare:
//! 2000 states per thread on each bench fluid's (T, ρ) grid (SplitMix64 seed 1, T ~ U[1.05·T_c, min(1.5·T_c, T_max)], ρ
//! ~ U[0.1, 2]·ρ_c), threads unpinned, the median wall time over 5 passes. A row is one fluid shared by every thread
//! (`&Fluid` from one registry: the same fluid) or `mixed`, every thread taking the five fluids in turn (different
//! fluids). Speedup = threads × the one-thread time per state over the N-thread time per state. It asserts nothing (K19
//! targets are estimates): `cargo xtask bench --record scaling` sets `PHASEKIT_BENCH_SCALING` and records the rows it
//! prints; a plain `cargo bench` skips it.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use std::hint::black_box;
    use std::num::NonZeroUsize;
    use std::sync::Barrier;
    use std::time::Instant;

    use phasekit_core::{Basis, Density, Fluid, Input, Registry, Temperature};
    use phasekit_verify::SplitMix64;

    /// The bench fluids (VERIFICATION.md §12).
    const FLUIDS: [&str; 5] = ["Water", "Methane", "R134a", "n-Propane", "n-Heptane"];

    /// States per thread and pass.
    const PER_THREAD: usize = 2000;

    /// Passes per row; the row is their median.
    const PASSES: usize = 5;

    /// The baseline's single-phase (T, ρ) grid of a fluid, `PER_THREAD` states.
    fn grid(registry: &Registry, name: &str) -> Option<Vec<Input>> {
        let record = phasekit_core::internal::record(registry, name).ok()?;
        let c = record.critical?;
        let (t_hi, mut rng) = ((1.5 * c.t).min(record.limits.t_max()), SplitMix64::new(1));
        let mut dt = || {
            let rho = Density::molar(rng.uniform(0.1 * c.rho, 2.0 * c.rho)).ok()?;
            Some(Input::dt(rho, Temperature::new(rng.uniform(1.05 * c.t, t_hi)).ok()?))
        };
        (0..PER_THREAD).map(|_| dt()).collect()
    }

    /// Physical cores (distinct `physical id`, `core id` pairs in /proc/cpuinfo), else the hardware threads.
    fn physical_cores(threads: usize) -> usize {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let mut cores = Vec::new();
        let (mut package, mut core) = (None, None);
        for line in cpuinfo.lines().chain([""]) {
            let value = |key: &str| line.strip_prefix(key).and_then(|rest| rest.split(':').nth(1)).map(str::trim);
            if let Some(v) = value("physical id") {
                package = Some(v.to_string());
            } else if let Some(v) = value("core id") {
                core = Some(v.to_string());
            } else if line.trim().is_empty() {
                if let (Some(p), Some(c)) = (package.take(), core.take()) {
                    if !cores.contains(&(p.clone(), c.clone())) {
                        cores.push((p, c));
                    }
                }
            }
        }
        if cores.is_empty() { threads } else { cores.len() }
    }

    /// The median wall time per state, in ns, of `threads` threads each running DT + h + c_p over its states: thread
    /// `t`'s state `i` is `work[i]`'s input on `work[i]`'s fluid.
    fn row(threads: usize, work: &[(&Fluid, Input)]) -> f64 {
        let mut walls: Vec<f64> = (0..PASSES)
            .map(|_| {
                let start = Barrier::new(threads + 1);
                std::thread::scope(|s| {
                    for _ in 0..threads {
                        s.spawn(|| {
                            start.wait();
                            let mut sum = 0.0;
                            for (fluid, input) in work {
                                if let Ok(state) = fluid.state(black_box(*input)) {
                                    sum += state.h(Basis::Molar) + state.cp(Basis::Molar).unwrap_or(0.0);
                                }
                            }
                            black_box(sum)
                        });
                    }
                    start.wait();
                    Instant::now()
                })
                .elapsed()
                .as_secs_f64()
                    * 1e9
            })
            .collect();
        walls.sort_by(f64::total_cmp);
        walls[PASSES / 2] / PER_THREAD as f64
    }

    pub fn main() {
        if std::env::var_os("PHASEKIT_BENCH_SCALING").is_none() {
            println!("scaling: skipped (`cargo xtask bench --record scaling` runs it)");
            return;
        }
        let Ok(registry) = Registry::embedded() else { return eprintln!("scaling: no embedded registry") };
        let mut work = Vec::new();
        for name in FLUIDS {
            let (Ok(fluid), Some(states)) = (registry.get(name), grid(registry, name)) else {
                return eprintln!("scaling: no grid for {name}");
            };
            work.push((name, states.into_iter().map(|input| (fluid, input)).collect::<Vec<_>>()));
        }
        let mixed: Vec<_> = (0..PER_THREAD).map(|i| work[i % FLUIDS.len()].1[i]).collect();
        work.push(("mixed", mixed));
        let hardware = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let physical = physical_cores(hardware);
        let mut counts = vec![1, 2, 4, physical, hardware];
        counts.retain(|n| *n <= hardware);
        counts.sort_unstable();
        counts.dedup();
        println!("# cores: {physical} physical, {hardware} hardware threads");
        println!("# columns: mode,threads,states_per_thread,median_ns_per_state,speedup");
        for (mode, states) in &work {
            let one = row(1, states);
            for &threads in &counts {
                let ns = if threads == 1 { one } else { row(threads, states) };
                let speedup = threads as f64 * one / ns;
                println!("{mode},{threads},{PER_THREAD},{ns:.1},{speedup:.3}");
            }
        }
    }
}

fn main() {
    #[cfg(not(target_family = "wasm"))]
    bench::main();
}
