//! `math_per_call` (PLAN.md M23.1; VERIFICATION.md §12): core's `math::exp`, `ln`, `powf` and `expm1` per call, on the
//! EOS arguments of `fixtures/math/` (each function's `eos` set: exp's −60 to 5, ln's τ and δ, pow's bases and
//! exponents from the fluids' terms, expm1's −θτ). `throughput` makes the calls independently, as the term sums do;
//! `chain` feeds each result into the next argument (times a zero the compiler cannot see), so a call waits for the one
//! before it, as in a Newton step. The case is the function on std's maths, `<f>_libm` built with
//! `--features phasekit-core/libm` (`backend::case`): M23's baseline, which the in-house functions are measured against
//! (M23.2-M23.6). Recorded by `cargo xtask bench --record`.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use std::time::{Duration, Instant};

    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::math::{self, BACKEND};
    use phasekit_verify::backend::case;
    use phasekit_verify::{Cell, Fixture, fixture};

    /// Each function, its reference values and its arguments' count.
    const FUNCTIONS: [(&str, (&str, &str), usize); 4] = [
        ("exp", fixture!("math/exp.csv"), 1),
        ("ln", fixture!("math/ln.csv"), 1),
        ("pow", fixture!("math/pow.csv"), 2),
        ("expm1", fixture!("math/expm1.csv"), 1),
    ];

    /// `name` at (x, y); y is unused but by `pow`.
    fn call(name: &str, x: f64, y: f64) -> f64 {
        match name {
            "exp" => math::exp(x),
            "ln" => math::ln(x),
            "pow" => math::powf(x, y),
            _ => math::expm1(x),
        }
    }

    /// The arguments of the fixture's `eos` rows.
    fn arguments((name, text): (&'static str, &'static str), count: usize) -> Vec<(f64, f64)> {
        let Ok(fixture) = Fixture::parse(name, text) else { return Vec::new() };
        let num = |cell: Option<&Cell>| match cell {
            Some(Cell::Num(v)) => *v,
            _ => f64::NAN,
        };
        let eos = |row: &&phasekit_verify::Row| row.cells.last() == Some(&Cell::Text("eos"));
        fixture
            .rows()
            .iter()
            .filter(eos)
            .map(|row| (num(row.cells.first()), if count == 2 { num(row.cells.get(1)) } else { 0.0 }))
            .collect()
    }

    /// Each call of `name` over `args` into `out`, independently.
    fn calls(name: &str, args: &[(f64, f64)], out: &mut [f64]) {
        for (o, &(x, y)) in out.iter_mut().zip(args) {
            *o = call(name, x, y);
        }
    }

    /// The calls of `name` over `args`, each argument plus the last result times `zero`: one chain of dependent calls.
    fn chained(name: &str, args: &[(f64, f64)], zero: f64) -> f64 {
        args.iter().fold(0.0, |last, &(x, y)| call(name, x + last * zero, y))
    }

    fn math_per_call(c: &mut Criterion) {
        let mut g = c.benchmark_group("math_per_call");
        for (name, fixture, count) in FUNCTIONS {
            let args = arguments(fixture, count);
            let per_call = u32::try_from(args.len()).unwrap_or(u32::MAX);
            let mut out = vec![0.0; args.len()];
            g.bench_function(BenchmarkId::new("throughput", case(BACKEND, name)), |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();
                    for _ in 0..iters {
                        calls(name, std::hint::black_box(&args), &mut out);
                        std::hint::black_box(&mut out);
                    }
                    start.elapsed().checked_div(per_call).unwrap_or(Duration::ZERO)
                });
            });
            g.bench_function(BenchmarkId::new("chain", case(BACKEND, name)), |b| {
                b.iter_custom(|iters| {
                    let zero = std::hint::black_box(0.0);
                    let start = Instant::now();
                    for _ in 0..iters {
                        std::hint::black_box(chained(name, std::hint::black_box(&args), zero));
                    }
                    start.elapsed().checked_div(per_call).unwrap_or(Duration::ZERO)
                });
            });
        }
        g.finish();
    }

    criterion_group!(benches, math_per_call);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);

#[cfg(target_family = "wasm")]
fn main() {}
