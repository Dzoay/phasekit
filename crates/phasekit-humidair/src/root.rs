//! A root between two points whose values bracket it (PLAN.md M18.4): the Illinois method, as phasekit-cubic's
//! superancillary inverse has it (each family crate stands on phasekit-core alone, and the core is frozen through M18);
//! and a range narrowed to where its states exist.

use phasekit_core::{Error, Strategy};

/// The steps the bracket may take: the Illinois method gains about 1.4 digits a step, so 200 reaches adjacent doubles
/// from any bracket in range.
const ITERATIONS: u16 = 200;

/// The halvings [`narrowed`] may take: 64 reach adjacent doubles over any range in T or ψ_w.
const EDGE_STEPS: u16 = 64;

/// `range` narrowed, at the end where `exists` fails, to the last point where it holds, by bisection; `None` where it
/// holds at neither end. At high p the virial equation has no state at low T or high ψ_w: its branch ends there.
pub(crate) fn narrowed(exists: impl Fn(f64) -> bool, (lo, hi): (f64, f64)) -> Option<(f64, f64)> {
    let (lo_ok, hi_ok) = (exists(lo), exists(hi));
    if lo_ok == hi_ok {
        return lo_ok.then_some((lo, hi));
    }
    let (mut good, mut bad) = if hi_ok { (hi, lo) } else { (lo, hi) };
    for _ in 0..EDGE_STEPS {
        let mid = 0.5 * (good + bad);
        if mid == good || mid == bad {
            break;
        }
        if exists(mid) { good = mid } else { bad = mid }
    }
    Some(if hi_ok { (good, hi) } else { (lo, good) })
}

/// A root of `f` between `lo` and `hi`, whose values must differ in sign (`None` if they do not), by regula falsi
/// halving the stale end's value (Illinois) to adjacent doubles, bisecting where a step leaves the bracket.
pub(crate) fn bracketed(
    mut f: impl FnMut(f64) -> Result<f64, Error>,
    (lo, hi): (f64, f64),
) -> Result<Option<f64>, Error> {
    let mut bracket = Bracket { a: (lo, f(lo)?), b: (hi, f(hi)?), side: 0 };
    if (bracket.a.1 > 0.0) == (bracket.b.1 > 0.0) && bracket.a.1 != 0.0 && bracket.b.1 != 0.0 {
        return Ok(None);
    }
    for _ in 0..ITERATIONS {
        if let Some(root) = bracket.root() {
            return Ok(Some(root));
        }
        let c = bracket.trial();
        bracket.update(c, f(c)?);
    }
    Err(Error::NoConvergence { strategy: Strategy::External, iterations: ITERATIONS })
}

/// [`bracketed`]'s state: the ends with their values, and which end moved last (−1 b, 1 a, 0 neither).
struct Bracket {
    a: (f64, f64),
    b: (f64, f64),
    side: i8,
}

impl Bracket {
    /// The root, once an end's value is zero or the ends are adjacent doubles (the end with the smaller value).
    fn root(&self) -> Option<f64> {
        let ((a, fa), (b, fb)) = (self.a, self.b);
        let c = self.trial();
        match () {
            () if fa == 0.0 => Some(a),
            () if fb == 0.0 => Some(b),
            () if c == a || c == b => Some(if fa.abs() <= fb.abs() { a } else { b }),
            () => None,
        }
    }

    /// The false-position point, or the midpoint where that leaves the open bracket.
    fn trial(&self) -> f64 {
        let ((a, fa), (b, fb)) = (self.a, self.b);
        let c = (a * fb - b * fa) / (fb - fa);
        if a.min(b) < c && c < a.max(b) { c } else { 0.5 * (a + b) }
    }

    /// Replaces the end whose value has `fc`'s sign by (c, fc), halving the other end's value when the same end moved
    /// twice running (Illinois).
    fn update(&mut self, c: f64, fc: f64) {
        if (fc > 0.0) == (self.b.1 > 0.0) {
            self.b = (c, fc);
            if self.side == -1 {
                self.a.1 *= 0.5;
            }
            self.side = -1;
        } else {
            self.a = (c, fc);
            if self.side == 1 {
                self.b.1 *= 0.5;
            }
            self.side = 1;
        }
    }
}
