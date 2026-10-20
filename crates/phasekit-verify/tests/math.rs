//! The maths backend against the reference values of `fixtures/math/` (PLAN.md M23.1; VERIFICATION.md §5, class
//! `Faithful`): core's `math::exp`, `ln`, `powf` and `expm1` on this build's backend, std's by default and the `libm`
//! crate's with `--features phasekit-core/libm`, each value's error in ulps by `phasekit_verify::ulp`. The worst error
//! of each fixture's sets is this target's report, read with `--no-capture`. Core's own functions (M23.2-M23.5), in
//! every build through `phasekit_core::internal`, must be faithful on the same values and Arm's results at Arm's cases.

use phasekit_core::{internal, math};
use phasekit_verify::{Cell, Fixture, FixtureError, Row, fixture, ulp};

/// The reference values: the generator's sets (`edge`, `eos`, `binade`, and exp's `subnormal`) and Arm's directed
/// cases.
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

/// The number of inputs of the function a fixture is named for.
fn inputs(name: &str) -> usize {
    if name.ends_with("pow.csv") { 2 } else { 1 }
}

/// The sets of a fixture, in order: Arm's cases, or the generator's three, with exp's `subnormal` after them.
fn sets_of(name: &str) -> &'static [&'static str] {
    match name {
        "math/exp.csv" => &["edge", "eos", "binade", "subnormal"],
        _ if name.contains("/arm/") => &["arm"],
        _ => &["edge", "eos", "binade"],
    }
}

/// The worst error of `f` over each set of a fixture, in the order the sets first appear.
fn worst_by_set(
    name: &'static str,
    text: &'static str,
    f: impl Fn(&[f64]) -> f64,
) -> Result<Vec<(&'static str, Worst)>, FixtureError> {
    let fixture = Fixture::parse(name, text)?;
    let mut sets: Vec<(&'static str, Worst)> = Vec::new();
    for row in fixture.rows() {
        let (x, want, tail, set) = values(row, inputs(name));
        let error = ulp::error(f(&x), want, tail);
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
/// its sets, about 4096 values for each function in the generator's three (exp's `subnormal` besides), and every error
/// is a number.
#[test]
fn backend_errors_are_reported() {
    let os = if cfg!(target_family = "wasm") { "wasi" } else { std::env::consts::OS }; // empty under wasip2
    let target = format!("{}-{os}", std::env::consts::ARCH);
    let mut report = Vec::new();
    for (name, text) in FIXTURES {
        let sets = worst_by_set(name, text, |x| backend(name, x)).unwrap();
        let rows: usize = sets.iter().filter(|(s, _)| *s != "subnormal").map(|(_, w)| w.rows).sum();
        assert_eq!(sets.iter().map(|(s, _)| *s).collect::<Vec<_>>(), sets_of(name), "{name}: each set once, in order");
        assert!(name.contains("/arm/") || (3900..4300).contains(&rows), "{name}: {rows} values");
        report
            .extend(sets.iter().map(|(set, w)| {
                format!("{name:18} {set:9} {:5} rows  worst {:.4} ulp at {:?}", w.rows, w.error, w.at)
            }));
    }
    #[allow(clippy::print_stdout, reason = "the report is this test's record, read with --no-capture")]
    {
        println!("\nmath={} target={target}\n{}", math::BACKEND, report.join("\n"));
    }
}

/// The rows of a fixture where `f` is not the reference value bit for bit (NaN canonical), as `x → got, want` and the
/// reference's tail.
fn not_bitwise(
    name: &'static str,
    text: &'static str,
    f: impl Fn(&[f64]) -> f64,
) -> Result<Vec<(String, f64)>, FixtureError> {
    let canonical = |v: f64| if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() };
    let fixture = Fixture::parse(name, text)?;
    let differ = |row: &Row<'static>| {
        let (x, want, tail, _) = values(row, inputs(name));
        let got = f(&x);
        let what = || (format!("{name}:{}: {x:?} → {got:e}, want {want:e}", row.line), tail);
        (canonical(got) != canonical(want)).then(what)
    };
    Ok(fixture.rows().iter().filter_map(differ).collect())
}

/// The rows of [`not_bitwise`], a line each.
fn lines(differ: &[(String, f64)]) -> String {
    differ.iter().map(|(what, _)| what.as_str()).collect::<Vec<_>>().join("\n")
}

/// Core's own `function` (`f`) against M23.1's values: its worst error over both of its fixtures, and Arm's cases where
/// it is not Arm's result bit for bit. Its worst error per set is printed.
fn own(function: &str, f: impl Fn(&[f64]) -> f64 + Copy) -> Result<(Worst, Vec<(String, f64)>), FixtureError> {
    let ours = FIXTURES.iter().filter(|(name, _)| name.ends_with(&format!("/{function}.csv")));
    let (mut worst, mut report) = (Worst::default(), Vec::new());
    for &(name, text) in ours {
        for (set, w) in worst_by_set(name, text, f)? {
            report.push(format!("{name:18} {set:9} {:5} rows  worst {:.4} ulp at {:?}", w.rows, w.error, w.at));
            worst.add(w.error, w.at);
        }
    }
    let (name, text) = fixture_named(&format!("math/arm/{function}.csv"));
    let differ = not_bitwise(name, text, f)?;
    #[allow(clippy::print_stdout, reason = "the worst errors are this test's record, read with --no-capture")]
    {
        println!("\nphasekit's own {function}\n{}", report.join("\n"));
    }
    Ok((worst, differ))
}

/// The fixture of [`FIXTURES`] at `path`.
fn fixture_named(path: &str) -> (&'static str, &'static str) {
    FIXTURES.into_iter().find(|(name, _)| *name == path).unwrap_or(("", ""))
}

/// PLAN.md M23.2 (VERIFICATION.md §5, class `Faithful`; user decision IM3): core's own `exp`, Arm's without FMA, is
/// under 1 ulp at every value of `fixtures/math/exp.csv` and `math/arm/exp.csv`, and Arm's result bit for bit at
/// Arm's cases (NaN canonical), in every build.
#[test]
fn exp_is_faithful() {
    let (worst, differ) = own("exp", |x| internal::exp(x[0])).unwrap();
    assert!(worst.error < ulp::FAITHFUL, "{} ulp at {:?}", worst.error, worst.at);
    assert!(differ.is_empty(), "not Arm's result:\n{}", lines(&differ));
}

/// PLAN.md M23.2: at subnormal results core's `exp` rounds once, as Arm's `specialcase` does: under 0.51 ulp at the
/// `subnormal` set of `fixtures/math/exp.csv`. Before that rounding its value errs by at most 0.011 of a double's ulp
/// (Arm's 0.511 less the rounding's 0.5), at most 0.0055 of the subnormals' spacing. Rounded to a double first and then
/// to that spacing, it would err by up to 0.76 ulp, which `exp_is_faithful`'s bound of 1 ulp lets through.
#[test]
fn exp_rounds_once_in_the_subnormals() {
    let (name, text) = fixture_named("math/exp.csv");
    let sets = worst_by_set(name, text, |x| internal::exp(x[0])).unwrap();
    let subnormal = sets.iter().find(|(set, _)| *set == "subnormal").map(|(_, worst)| worst);
    assert!(subnormal.is_some_and(|w| w.rows == 512 && w.error < 0.51), "{subnormal:?}");
}

/// PLAN.md M23.3 (VERIFICATION.md §5, class `Faithful`; user decision IM3): core's own `ln`, Arm's `log` without FMA,
/// is under 1 ulp at every value of `fixtures/math/ln.csv` and `math/arm/ln.csv`, and Arm's result bit for bit at
/// Arm's cases (NaN canonical), in every build.
#[test]
fn ln_is_faithful() {
    let (worst, differ) = own("ln", |x| internal::ln(x[0])).unwrap();
    assert!(worst.error < ulp::FAITHFUL, "{} ulp at {:?}", worst.error, worst.at);
    assert!(differ.is_empty(), "not Arm's result:\n{}", lines(&differ));
}

/// PLAN.md M23.4 (VERIFICATION.md §5, class `Faithful`; user decision IM3): core's own `pow`, Arm's without FMA, is
/// under 1 ulp at every pair of `fixtures/math/pow.csv` (C99 Annex F's cases among its edges) and `math/arm/pow.csv`,
/// in every build, and Arm's result bit for bit at Arm's cases (NaN canonical, signed zeros kept) but its near-ties.
/// There, at nine of them, the exact value lies at the midpoint of two doubles to four digits (a tail of ±½), and Arm's
/// own `pow` without FMA rounds to the other neighbour as core's does, ½ ulp off either way: Arm's results are the
/// correctly rounded ones, which its FMA build gives. With the `fast` feature (PLAN.md M23.7) core's FMA forms round
/// four of the nine as Arm does, and five stay near-ties the other way.
#[test]
fn pow_is_faithful() {
    let (worst, differ) = own("pow", |x| internal::pow(x[0], x[1])).unwrap();
    assert!(worst.error < ulp::FAITHFUL, "{} ulp at {:?}", worst.error, worst.at);
    let (ties, differ): (Vec<_>, Vec<_>) = differ.into_iter().partition(|(_, tail)| tail.abs() == 0.5);
    assert!(differ.is_empty(), "not Arm's result:\n{}", lines(&differ));
    let want = if math::BACKEND == "fast" { 5 } else { 9 };
    assert!(ties.len() == want, "Arm's near-ties rounded the other way:\n{}", lines(&ties));
}

/// PLAN.md M23.5 (VERIFICATION.md §5, class `Faithful`; user decision IM3): core's own `expm1`, M12.8's port of the
/// `libm` crate's (FreeBSD's `s_expm1.c`), is under 1 ulp at every value of `fixtures/math/expm1.csv` and
/// `math/arm/expm1.csv`, and Arm's result bit for bit at Arm's cases (NaN canonical), in every build.
#[test]
fn expm1_is_faithful() {
    let (worst, differ) = own("expm1", |x| internal::expm1(x[0])).unwrap();
    assert!(worst.error < ulp::FAITHFUL, "{} ulp at {:?}", worst.error, worst.at);
    assert!(differ.is_empty(), "not Arm's result:\n{}", lines(&differ));
}
