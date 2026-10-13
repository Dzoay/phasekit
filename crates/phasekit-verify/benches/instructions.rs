//! Instruction counts of the scalar gate workloads (PLAN.md M9.6; VERIFICATION.md §12 enforcement item 1): gungraun
//! runs each workload once under Callgrind, its setup excluded. The workloads are the §7 rows on the bench fluids, each
//! over 16 states of the criterion benches' grids: the order-2 α^r bundle, properties at (T, ρ) (DT + h + c_p), QT
//! and PQ on the superancillary, PT + h and PH + T single phase, and a lookup by name. `cargo xtask gates perf` builds
//! this for `x86_64-unknown-linux-musl` with `--features phasekit-core/libm`, a static binary with no glibc, so the
//! counts do not depend on the machine's maths library or loader (glibc picks its exp by CPU and version), and fails on
//! a count more than 5 % over `benches/baseline/instructions.csv`.
//! Linux only: elsewhere `main` is empty, so G6 still builds every target.

#[cfg(target_os = "linux")]
mod bench {
    use std::hint::black_box;

    use gungraun::prelude::*;
    use phasekit_core::{
        Basis, DataSet, Density, Enthalpy, Fluid, HelmholtzModel, Input, Order, Pressure, Quality, Registry,
        Temperature,
    };
    use phasekit_verify::SplitMix64;

    /// States per workload: enough to cover the grid's regions, few enough to run under Callgrind in a second.
    const STATES: usize = 16;

    fn fluid(name: &str) -> &'static Fluid {
        Registry::embedded().and_then(|r| r.get(name)).unwrap_or_else(|_| std::process::abort())
    }

    /// The criterion benches' single-phase (T, ρ) grid (VERIFICATION.md §12): SplitMix64 seed 1, T ~ U[1.05·T_c,
    /// min(1.5·T_c, T_max)], ρ ~ U[0.1, 2]·ρ_c.
    fn single(name: &str) -> Vec<(f64, f64)> {
        let registry = Registry::embedded().unwrap_or_else(|_| std::process::abort());
        let record = phasekit_core::internal::record(registry, name).unwrap_or_else(|_| std::process::abort());
        let Some(c) = record.critical else { std::process::abort() };
        let (t_hi, mut rng) = ((1.5 * c.t).min(record.limits.t_max()), SplitMix64::new(1));
        (0..STATES).map(|_| (rng.uniform(0.1 * c.rho, 2.0 * c.rho), rng.uniform(1.05 * c.t, t_hi))).collect()
    }

    fn dt(rho: f64, t: f64) -> Input {
        match (Density::molar(rho), Temperature::new(t)) {
            (Ok(d), Ok(t)) => Input::dt(d, t),
            _ => std::process::abort(),
        }
    }

    fn alphar_setup(name: &str) -> (&'static dyn HelmholtzModel, Vec<(f64, f64)>) {
        let Some(model) = fluid(name).model().helmholtz() else { std::process::abort() };
        (model, single(name))
    }

    /// Runs every state's flash once outside the measurement, so the fluid's lazily decoded parts (its superancillary,
    /// caloric curves, melting line) are in place and the count is the steady-state cost (criterion's warm-up).
    fn warmed(work: (&'static Fluid, Vec<Input>)) -> (&'static Fluid, Vec<Input>) {
        for input in &work.1 {
            black_box(work.0.state(*input).ok());
        }
        work
    }

    fn dt_setup(name: &str) -> (&'static Fluid, Vec<Input>) {
        warmed((fluid(name), single(name).into_iter().map(|(rho, t)| dt(rho, t)).collect()))
    }

    /// The criterion benches' saturation grid: T ~ U[T_t + 0.05 (T_c − T_t), 0.98 T_c], Q ~ U[0, 1]; QT inputs, or PQ
    /// at the `Corrected` superancillary's p of each T.
    fn saturation(name: &str, pq: bool) -> (&'static Fluid, Vec<Input>) {
        let registry = Registry::embedded().unwrap_or_else(|_| std::process::abort());
        let mut record = phasekit_core::internal::record(registry, name).unwrap_or_else(|_| std::process::abort());
        if record.apply(DataSet::Corrected).is_err() {
            std::process::abort();
        }
        let (Some(c), Some(curve)) = (record.critical, record.superancillary_curve()) else { std::process::abort() };
        let t_t = record.limits.t_triple().unwrap_or(record.limits.t_min());
        let mut rng = SplitMix64::new(1);
        let mut input = || -> Option<Input> {
            let (t, q) = (rng.uniform(t_t + 0.05 * (c.t - t_t), 0.98 * c.t), rng.uniform(0.0, 1.0));
            let quality = Quality::new(q).ok()?;
            if pq {
                return Some(Input::pq(Pressure::new(curve.at_t(t).ok()?.bubble.p).ok()?, quality));
            }
            Some(Input::qt(quality, Temperature::new(t).ok()?))
        };
        let states = (0..STATES).map(|_| input()).collect::<Option<Vec<_>>>();
        (fluid(name), states.unwrap_or_else(|| std::process::abort()))
    }

    fn qt_setup(name: &str) -> (&'static Fluid, Vec<Input>) {
        warmed(saturation(name, false))
    }

    fn pq_setup(name: &str) -> (&'static Fluid, Vec<Input>) {
        warmed(saturation(name, true))
    }

    /// PT or PH inputs read off the single-phase grid's DT states.
    fn single_inputs(name: &str, ph: bool) -> (&'static Fluid, Vec<Input>) {
        let fluid = fluid(name);
        let input = |(rho, t): (f64, f64)| -> Option<Input> {
            let s = fluid.state(dt(rho, t)).ok()?;
            let p = Pressure::new(s.p()).ok()?;
            if ph {
                return Some(Input::hp(Enthalpy::molar(s.h(Basis::Molar)).ok()?, p));
            }
            Some(Input::pt(p, Temperature::new(t).ok()?))
        };
        let states = single(name).into_iter().map(input).collect::<Option<Vec<_>>>();
        (fluid, states.unwrap_or_else(|| std::process::abort()))
    }

    fn pt_setup(name: &str) -> (&'static Fluid, Vec<Input>) {
        warmed(single_inputs(name, false))
    }

    fn ph_setup(name: &str) -> (&'static Fluid, Vec<Input>) {
        warmed(single_inputs(name, true))
    }

    #[library_benchmark]
    #[bench::water(args = ("Water",), setup = alphar_setup)]
    #[bench::methane(args = ("Methane",), setup = alphar_setup)]
    #[bench::r134a(args = ("R134a",), setup = alphar_setup)]
    #[bench::n_propane(args = ("n-Propane",), setup = alphar_setup)]
    #[bench::n_heptane(args = ("n-Heptane",), setup = alphar_setup)]
    fn alphar_order2(work: (&'static dyn HelmholtzModel, Vec<(f64, f64)>)) -> f64 {
        let (model, states) = work;
        states.iter().filter_map(|&(rho, t)| black_box(model.residual(black_box(t), rho, Order::Two)).get(0, 0)).sum()
    }

    #[library_benchmark]
    #[bench::water(args = ("Water",), setup = dt_setup)]
    #[bench::methane(args = ("Methane",), setup = dt_setup)]
    #[bench::r134a(args = ("R134a",), setup = dt_setup)]
    #[bench::n_propane(args = ("n-Propane",), setup = dt_setup)]
    #[bench::n_heptane(args = ("n-Heptane",), setup = dt_setup)]
    fn properties_at_t_rho(work: (&'static Fluid, Vec<Input>)) -> f64 {
        let (fluid, states) = work;
        let h_cp = |input| fluid.state(input).map(|s| s.h(Basis::Molar) + s.cp(Basis::Molar).unwrap_or(0.0));
        states.into_iter().filter_map(|input| black_box(h_cp(black_box(input))).ok()).sum()
    }

    #[library_benchmark]
    #[bench::water(args = ("Water",), setup = qt_setup)]
    #[bench::methane(args = ("Methane",), setup = qt_setup)]
    #[bench::r134a(args = ("R134a",), setup = qt_setup)]
    #[bench::n_propane(args = ("n-Propane",), setup = qt_setup)]
    #[bench::n_heptane(args = ("n-Heptane",), setup = qt_setup)]
    fn qt_superancillary(work: (&'static Fluid, Vec<Input>)) -> f64 {
        let (fluid, states) = work;
        states.into_iter().filter_map(|input| black_box(fluid.state(black_box(input))).ok()).map(|s| s.t()).sum()
    }

    #[library_benchmark]
    #[bench::water(args = ("Water",), setup = pq_setup)]
    #[bench::methane(args = ("Methane",), setup = pq_setup)]
    #[bench::r134a(args = ("R134a",), setup = pq_setup)]
    #[bench::n_propane(args = ("n-Propane",), setup = pq_setup)]
    #[bench::n_heptane(args = ("n-Heptane",), setup = pq_setup)]
    fn pq_superancillary(work: (&'static Fluid, Vec<Input>)) -> f64 {
        let (fluid, states) = work;
        states.into_iter().filter_map(|input| black_box(fluid.state(black_box(input))).ok()).map(|s| s.t()).sum()
    }

    #[library_benchmark]
    #[bench::water(args = ("Water",), setup = pt_setup)]
    #[bench::methane(args = ("Methane",), setup = pt_setup)]
    #[bench::r134a(args = ("R134a",), setup = pt_setup)]
    #[bench::n_propane(args = ("n-Propane",), setup = pt_setup)]
    #[bench::n_heptane(args = ("n-Heptane",), setup = pt_setup)]
    fn pt_flash(work: (&'static Fluid, Vec<Input>)) -> f64 {
        let (fluid, states) = work;
        let h = |input| fluid.state(input).map(|s| s.h(Basis::Molar));
        states.into_iter().filter_map(|input| black_box(h(black_box(input))).ok()).sum()
    }

    #[library_benchmark]
    #[bench::water(args = ("Water",), setup = ph_setup)]
    #[bench::methane(args = ("Methane",), setup = ph_setup)]
    #[bench::r134a(args = ("R134a",), setup = ph_setup)]
    #[bench::n_propane(args = ("n-Propane",), setup = ph_setup)]
    #[bench::n_heptane(args = ("n-Heptane",), setup = ph_setup)]
    fn ph_flash(work: (&'static Fluid, Vec<Input>)) -> f64 {
        let (fluid, states) = work;
        states.into_iter().filter_map(|input| black_box(fluid.state(black_box(input))).ok()).map(|s| s.t()).sum()
    }

    /// The criterion bench's keys: R134a's name, an alias and its CAS number.
    const KEYS: [&str; 3] = ["R134a", "r134A", "811-97-2"];

    /// The registry, with R134a decoded and every key looked up once outside the measurement.
    fn registry() -> &'static Registry {
        let registry = Registry::embedded().unwrap_or_else(|_| std::process::abort());
        KEYS.iter().for_each(|key| drop(black_box(registry.get(key))));
        registry
    }

    #[library_benchmark]
    #[bench::name(setup = registry)]
    fn lookup_by_name(registry: &'static Registry) -> usize {
        KEYS.iter().filter(|key| black_box(registry.get(black_box(key))).is_ok()).count()
    }

    library_benchmark_group!(
        name = kernel,
        benchmarks = [
            alphar_order2,
            properties_at_t_rho,
            qt_superancillary,
            pq_superancillary,
            pt_flash,
            ph_flash,
            lookup_by_name
        ]
    );
}

#[cfg(target_os = "linux")]
use bench::kernel;

#[cfg(target_os = "linux")]
gungraun::main!(library_benchmark_groups = kernel);

#[cfg(not(target_os = "linux"))]
fn main() {}
