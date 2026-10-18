//! The Illinois regula falsi (PLAN.md M13.10): the isochoric split's searches, in β between the bubble and dew points
//! and in ln p over the PT flash.

use phasekit_core::Error;

/// A bracket's end: where it lies, its value as the Illinois method weighs it, its own value, and what the evaluation
/// there gave.
#[derive(Clone, Debug)]
pub(crate) struct End<E> {
    pub(crate) x: f64,
    weight: f64,
    pub(crate) value: f64,
    pub(crate) at: E,
}

impl<E> End<E> {
    /// The end at `x` of an evaluation's (value, result).
    pub(crate) fn new(x: f64, (value, at): (f64, E)) -> End<E> {
        End { x, weight: value, value, at }
    }
}

/// The root of `f` between `a` and `b`, a.x < b.x, whose values differ in sign, by the Illinois regula falsi: the end
/// kept while the other is replaced twice running has its weight halved. It ends where the bracket is within `width`, or
/// at a value of 0, after at most `iterations`, on the end of smaller |value|.
pub(crate) fn illinois<E>(
    a: End<E>,
    b: End<E>,
    f: &mut impl FnMut(f64) -> Result<(f64, E), Error>,
    (width, iterations): (f64, usize),
) -> Result<End<E>, Error> {
    let mut ends = [a, b];
    // The end the last step replaced: false for a, true for b.
    let mut replaced = None;
    for _ in 0..iterations {
        let [a, b] = &ends;
        if b.x - a.x <= width {
            break;
        }
        let x = (a.x * b.weight - b.x * a.weight) / (b.weight - a.weight);
        let at = End::new(x, f(x)?);
        if at.value == 0.0 {
            return Ok(at);
        }
        let replace_b = at.value.signum() != a.value.signum();
        let [a, b] = &mut ends;
        let (this, other) = if replace_b { (b, a) } else { (a, b) };
        if replaced == Some(replace_b) {
            other.weight /= 2.0;
        }
        *this = at;
        replaced = Some(replace_b);
    }
    let [a, b] = ends;
    Ok(if a.value.abs() <= b.value.abs() { a } else { b })
}

#[cfg(test)]
mod tests {
    use phasekit_core::math;

    use super::*;

    /// The method's run on `f` over [0, 1]: the end it returns and the evaluations it spent.
    fn run(f: fn(f64) -> f64, (width, iterations): (f64, usize)) -> (End<()>, usize) {
        let mut calls = 0;
        let mut counted = |x: f64| -> Result<(f64, ()), Error> {
            calls += 1;
            Ok((f(x), ()))
        };
        let (a, b) = (End::new(0.0, (f(0.0), ())), End::new(1.0, (f(1.0), ())));
        let found = illinois(a, b, &mut counted, (width, iterations)).unwrap_or_else(|e| panic!("{e:?}"));
        (found, calls)
    }

    /// The Illinois method finds a root of a curve its plain form creeps toward from one side, either way up, halving the
    /// end it keeps: exp(4x) − 2 to 1e-14 in 11 evaluations and its mirror 2 − exp(4(1 − x)) in 12; a line's root at once,
    /// a value of exactly 0. It stops on the bracket's width (here none: [0, 1] is 1 wide) or its iterations (one), then
    /// on the end of smaller |value|.
    #[test]
    fn the_illinois_method_finds_curved_roots_either_way_up() {
        let (found, calls) = run(|x| math::exp(4.0 * x) - 2.0, (1e-14, 100));
        assert!((found.x - math::ln(2.0) / 4.0).abs() <= 1e-14 && found.value.abs() <= 1e-13 && calls == 11, "{calls}");
        let (found, calls) = run(|x| 2.0 - math::exp(4.0 * (1.0 - x)), (1e-14, 100));
        assert!((found.x - (1.0 - math::ln(2.0) / 4.0)).abs() <= 1e-14 && calls == 12, "{} {calls}", found.x);
        let (found, calls) = run(|x| x - 0.25, (1e-14, 100));
        assert!(found.x == 0.25 && found.value == 0.0 && calls == 1, "{} {calls}", found.x);
        let (found, calls) = run(|x| math::exp(4.0 * x) - 2.0, (1.0, 100));
        assert!(calls == 0 && found.x == 0.0, "{} {calls}", found.x);
        let (found, calls) = run(|x| 2.0 - math::exp(4.0 * x), (1.0, 100));
        assert!(calls == 0 && found.x == 0.0 && found.value == 1.0, "{} {calls}", found.x);
        let (found, calls) = run(|x| math::exp(4.0 * x) - 2.0, (1e-14, 1));
        assert!(calls == 1 && found.x > 0.0 && found.x < 0.1, "{} {calls}", found.x);
    }
}
