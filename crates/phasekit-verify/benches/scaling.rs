//! `scaling_same_and_different_fluids` (PLAN.md M9.2; VERIFICATION.md §12; ARCHITECTURE.md §7 thread scaling, target ≥
//! 0.9·N): throughput of DT + h + c_p at 1, 2, 4, the physical cores and the hardware threads, relative to one thread.
//! The method is the C++ baseline's scaling rows (`coolprop-8.0.0-<machine>.scaling.csv`), so the speedups compare:
//! 2000 states per thread on each bench fluid's (T, ρ) grid (SplitMix64 seed 1, T ~ U[1.05·T_c, min(1.5·T_c, T_max)], ρ
//! ~ U[0.1, 2]·ρ_c), threads unpinned. Where the baseline times one run through the states in each of 5 passes, this
//! runs through them 10 times in each of 7 passes after a warm-up, and times from the workers' own clocks. A row is one
//! fluid shared by every thread (`&Fluid` from one registry: the same fluid) or `mixed`, every thread taking the five
//! fluids in turn (different fluids); a last `control` row shares and reads nothing, so its speedup is the machine's
//! ceiling. Speedup = threads × the one-thread time per state over the N-thread time per state. It asserts nothing (K19
//! targets are estimates): `cargo xtask bench --record scaling` passes `--scaling <physical cores>` and records the
//! rows it prints; a plain `cargo bench` skips it.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a bench program: it prints its rows, which `cargo xtask bench` records"
)]

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

    /// Distinct states per thread: the C++ baseline's 2000.
    const PER_THREAD: usize = 2000;

    /// Times each thread runs through its states in a pass: 10-90 ms of work, so the threads' wake-up skew (tens of µs)
    /// stays well under a percent.
    const REPEAT: usize = 10;

    /// Timed passes per row, after an untimed one; the row is their median.
    const PASSES: usize = 7;

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

    /// A row's work for state `i`, shared by its threads.
    type Work<'a> = Box<dyn Fn(usize) -> f64 + Sync + 'a>;

    /// The control row's work for state `i`: a chain of exp and ln on the thread's own value, about a microsecond with
    /// no data shared or read from memory. Its speedup is the machine's own (turbo and power limits), the ceiling for
    /// every other row.
    fn control(i: usize) -> f64 {
        (0..40).fold(i as f64 * 1e-3, |y, _| phasekit_core::math::ln(phasekit_core::math::exp(y) + 1.0) - 0.5)
    }

    /// The median wall time per state, in ns, of `threads` threads each running `work` over states 0, 1, ….
    fn row(threads: usize, work: &(dyn Fn(usize) -> f64 + Sync)) -> f64 {
        pass(threads, work);
        let mut walls: Vec<f64> = (0..PASSES).map(|_| pass(threads, work)).collect();
        walls.sort_by(f64::total_cmp);
        walls[PASSES / 2] / (PER_THREAD * REPEAT) as f64
    }

    /// One pass: the wall time in ns from the first worker's start to the last worker's end. Each worker reads the
    /// clock itself once the barrier releases them all, so a descheduled spawning thread (more threads than cores)
    /// cannot shorten it.
    fn pass(threads: usize, work: &(dyn Fn(usize) -> f64 + Sync)) -> f64 {
        let start = Barrier::new(threads);
        let spans: Vec<(Instant, Instant)> = std::thread::scope(|s| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    s.spawn(|| {
                        start.wait();
                        let begin = Instant::now();
                        black_box((0..PER_THREAD * REPEAT).map(|i| work(black_box(i))).sum::<f64>());
                        (begin, Instant::now())
                    })
                })
                .collect();
            workers.into_iter().filter_map(|w| w.join().ok()).collect()
        });
        let (first, last) = (spans.iter().map(|s| s.0).min(), spans.iter().map(|s| s.1).max());
        first.zip(last).map_or(f64::NAN, |(first, last)| (last - first).as_secs_f64() * 1e9)
    }

    /// DT + h + c_p of `states[i]` on its fluid, cycling through the states.
    fn dt_h_cp(states: &[(&Fluid, Input)], i: usize) -> f64 {
        states.get(i % states.len().max(1)).map_or(0.0, |(fluid, input)| {
            fluid.state(*input).map_or(0.0, |s| s.h(Basis::Molar) + s.cp(Basis::Molar).unwrap_or(0.0))
        })
    }

    pub fn main() {
        let args: Vec<String> = std::env::args().collect();
        let scaling = args.iter().position(|a| a == "--scaling").and_then(|k| args.get(k + 1)?.parse::<usize>().ok());
        let Some(physical) = scaling.filter(|n| *n > 0) else {
            return println!("scaling: skipped (`cargo xtask bench --record scaling` runs it)");
        };
        let Ok(registry) = Registry::embedded() else { return eprintln!("scaling: no embedded registry") };
        let mut work = Vec::new();
        for name in FLUIDS {
            let (Ok(fluid), Some(states)) = (registry.get(name), grid(registry, name)) else {
                return eprintln!("scaling: no grid for {name}");
            };
            work.push((name, states.into_iter().map(|input| (fluid, input)).collect::<Vec<_>>()));
        }
        let mixed: Vec<_> = (0..PER_THREAD).filter_map(|i| work.get(i % FLUIDS.len())?.1.get(i).copied()).collect();
        work.push(("mixed", mixed));
        let hardware = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let mut counts = vec![1, 2, 4, physical, hardware];
        counts.retain(|n| *n <= hardware);
        counts.sort_unstable();
        counts.dedup();
        println!("# cores: {physical} physical, {hardware} hardware threads");
        println!("# columns: mode,threads,states_per_thread,median_ns_per_state,speedup");
        let mut rows: Vec<(&str, Work<'_>)> =
            work.iter().map(|(mode, states)| (*mode, Box::new(|i| dt_h_cp(states, i)) as Work<'_>)).collect();
        rows.push(("control", Box::new(control)));
        for (mode, work) in &rows {
            let one = row(1, work.as_ref());
            for &threads in &counts {
                let ns = if threads == 1 { one } else { row(threads, work.as_ref()) };
                let speedup = threads as f64 * one / ns;
                println!("{mode},{threads},{},{ns:.1},{speedup:.3}", PER_THREAD * REPEAT);
            }
        }
    }
}

fn main() {
    #[cfg(not(target_family = "wasm"))]
    bench::main();
}
