//! `lookup_by_name` (PLAN.md M2.9; VERIFICATION.md §12 workload 7, the name half of a by-name call): a hot
//! `Registry::get` of an already decoded fluid by its canonical name, an alias in another case, and its CAS number.
//! Target ≤ 50 ns (ARCHITECTURE.md §7); the C++ baseline's by-name `PropsSI` rebuilds a backend instead (76.5 µs).
//! Recorded by `cargo xtask bench --record`.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::Registry;

    /// R134a: a bench fluid (VERIFICATION.md §12) that compiles today.
    const KEYS: [(&str, &str); 3] = [("name", "R134a"), ("alias", "r134A"), ("cas", "811-97-2")];

    fn lookup_by_name(c: &mut Criterion) {
        let Ok(registry) = Registry::embedded() else { return };
        if registry.get("R134a").is_err() {
            return;
        }
        let mut group = c.benchmark_group("lookup_by_name");
        for (label, key) in KEYS {
            let id = BenchmarkId::new("R134a", label);
            group.bench_function(id, |b| b.iter(|| std::hint::black_box(registry.get(std::hint::black_box(key)))));
        }
        group.finish();
    }

    criterion_group!(benches, lookup_by_name);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
