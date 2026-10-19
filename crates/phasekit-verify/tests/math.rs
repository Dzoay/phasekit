//! The maths backend against the reference values of `fixtures/math/` (PLAN.md M23.1; VERIFICATION.md §5, class
//! `Faithful`): core's `math::exp`, `ln`, `powf` and `expm1` on this build's backend, std's by default and the `libm`
//! crate's with `--features phasekit-core/libm`, each value's error in ulps by `phasekit_verify::ulp`. The worst error
//! of each fixture's sets is this target's report, read with `--no-capture`; core's in-house functions (M23.2-M23.5)
//! are measured against the same values.

use phasekit_core::math;
use phasekit_verify::{Cell, Fixture, FixtureError, Row, fixture, ulp};

/// The reference values: the generator's sets (`edge`, `eos`, `binade`) and Arm's directed cases.
const FIXTURES: [(&str, &str); 8] = [
    fixture!("math/exp.csv"),
    fixture!("math/ln.csv"),
    fixture!("math/pow.csv"),
    fixture!("math/expm1.csv"),
    fixture!("math/arm/exp.csv"),
    fixture!("math/arm/ln.csv"),
    fixture!("math/arm/pow.csv"),
    fixture!("math/arm/expm1.csv"),
];

/// The backend's function a fixture is named for, at its inputs.
fn backend(name: &str, x: &[f64]) -> f64 {
    let (a, b) = (x.first().copied().unwrap_or(f64::NAN), x.get(1).copied().unwrap_or(f64::NAN));
    match name.rsplit('/').next() {
        Some("exp.csv") => math::exp(a),
        Some("ln.csv") => math::ln(a),
        Some("pow.csv") => math::powf(a, b),
        Some("expm1.csv") => math::expm1(a),
        _ => f64::NAN,
    }
}

/// The worst error over one set of a fixture: (rows, error, the inputs where it is).
#[derive(Debug, Default)]
struct Worst {
    rows: usize,
    error: f64,
    at: Vec<f64>,
}

impl Worst {
    fn add(&mut self, error: f64, x: Vec<f64>) {
        self.rows += 1;
        if error > self.error || self.at.is_empty() {
            (self.error, self.at) = (error, x);
        }
    }
}

/// A row's inputs, want, tail and set (`arm` in Arm's files, which have no `set` column); NaN for a cell that is not
/// a number, which the error then is.
fn values(row: &Row<'static>, inputs: usize) -> (Vec<f64>, f64, f64, &'static str) {
    let num = |i: usize| match row.cells.get(i) {
        Some(Cell::Num(v)) => *v,
        _ => f64::NAN,
    };
    let set = match row.cells.get(inputs + 2) {
        Some(Cell::Text(set)) => *set,
        _ => "arm",
    };
    ((0..inputs).map(num).collect(), num(inputs), num(inputs + 1), set)
}

/// The worst error of each set of a fixture on this build's backend, in the order the sets first appear.
fn worst_by_set(name: &'static str, text: &'static str) -> Result<Vec<(&'static str, Worst)>, FixtureError> {
    let fixture = Fixture::parse(name, text)?;
    let inputs = if name.ends_with("pow.csv") { 2 } else { 1 };
    let mut sets: Vec<(&'static str, Worst)> = Vec::new();
    for row in fixture.rows() {
        let (x, want, tail, set) = values(row, inputs);
        let error = ulp::error(backend(name, &x), want, tail);
        assert!(!error.is_nan(), "{name}:{}: no error measured at {x:?}", row.line);
        if sets.last().is_none_or(|(s, _)| *s != set) {
            sets.push((set, Worst::default()));
        }
        if let Some((_, worst)) = sets.last_mut() {
            worst.add(error, x);
        }
    }
    Ok(sets)
}

/// PLAN.md M23.1: every reference value is measured on this build's backend and the worst error of each set reported
/// (the report is printed; it asserts no bound, since std's maths is each platform's own). Asserted: each fixture has
/// its sets, about 4096 values for each function from the generator, and every error is a number.
#[test]
fn backend_errors_are_reported() {
    let os = if cfg!(target_family = "wasm") { "wasi" } else { std::env::consts::OS }; // empty under wasip2
    let target = format!("{}-{os}", std::env::consts::ARCH);
    let mut report = Vec::new();
    for (name, text) in FIXTURES {
        let sets = worst_by_set(name, text).unwrap();
        let rows: usize = sets.iter().map(|(_, w)| w.rows).sum();
        let want: &[&str] = if name.contains("/arm/") { &["arm"] } else { &["edge", "eos", "binade"] };
        assert_eq!(sets.iter().map(|(s, _)| *s).collect::<Vec<_>>(), want, "{name}: each set once, in order");
        assert!(name.contains("/arm/") || (3900..4300).contains(&rows), "{name}: {rows} values");
        report
            .extend(sets.iter().map(|(set, w)| {
                format!("{name:18} {set:6} {:5} rows  worst {:.4} ulp at {:?}", w.rows, w.error, w.at)
            }));
    }
    #[allow(clippy::print_stdout, reason = "the report is this test's record, read with --no-capture")]
    {
        println!("\nmath={} target={target}\n{}", math::BACKEND, report.join("\n"));
    }
}
