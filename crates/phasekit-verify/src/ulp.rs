//! Errors in units in the last place (PLAN.md M23.1; VERIFICATION.md §5, class `Faithful`): the measure core's in-house
//! `exp`, `ln`, `pow` and `expm1` are held to (user decision IM3), in the convention of Arm's optimized-routines
//! (`math/test/ulp.h`), in which its error bounds are stated. A reference value is its correctly rounded double `want`
//! and the residual `tail` = (exact − want)/ulp(want), as `scripts/math/reference.py` writes them to `fixtures/math/`.

/// Class `Faithful`'s bound (VERIFICATION.md §5): an error under 1 ulp.
pub const FAITHFUL: f64 = 1.0;

/// The spacing of doubles in `x`'s binade: 2⁻¹⁰⁷⁴ in the subnormals and at ±0, 2⁹⁷¹ at `f64::MAX`; infinite for an
/// infinite or NaN `x`.
pub fn ulp(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::INFINITY;
    }
    // 2^(e − 1075) for the biased exponent e (1 in the subnormals): a normal double down to e = 53, a subnormal below.
    let biased = (x.to_bits() >> 52) & 0x7ff;
    f64::from_bits(if biased > 52 { (biased - 52) << 52 } else { 1 << (biased.max(1) - 1) })
}

/// |got − exact| in ulps of `want`, the exact value being `want` + `tail`·ulp(`want`): 0 where `got` is `want` and the
/// tail is 0, `tail`'s magnitude where it is `want`, and infinite where one of `got` and `want` is NaN or infinite and
/// the other is not the same. Every NaN is the same.
pub fn error(got: f64, want: f64, tail: f64) -> f64 {
    if got.is_nan() || want.is_nan() {
        return if got.is_nan() && want.is_nan() { 0.0 } else { f64::INFINITY };
    }
    // An infinite `got` against a finite `want` is an infinite error by the arithmetic below.
    if want.is_infinite() {
        return if got == want { 0.0 } else { f64::INFINITY };
    }
    // got − want is exact where the two lie within a factor of 2 of each other (Sterbenz) or among the subnormals, and
    // dividing it by ulp(want), a power of two, is exact too.
    ((got - want) / ulp(want) - tail).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_UP: f64 = 1.0 + f64::EPSILON;
    const ONE_DOWN: f64 = 1.0 - f64::EPSILON / 2.0;
    const TINY: f64 = 5e-324;

    /// PLAN.md M23.1: the measure on cases worked by hand. At the reference the error is 0, or the tail's magnitude; at
    /// a midpoint (tail ½) both neighbours err by ½; a neighbour of an exact value by 1. Across a binade the unit is
    /// want's: 1 − 2⁻⁵³ is ½ ulp below 1 (whose ulp is 2⁻⁵²) and 1 is a whole ulp above 1 − 2⁻⁵³ (whose ulp is 2⁻⁵³).
    /// In the subnormals the unit is 2⁻¹⁰⁷⁴, at 0 too. NaN against NaN is no error; NaN against a number, or ∞ against
    /// `f64::MAX`, is an infinite one.
    #[test]
    fn ulp_error_is_exact() {
        assert_eq!(
            [ulp(1.0), ulp(-1.5), ulp(ONE_DOWN), ulp(2.0)],
            [f64::EPSILON, f64::EPSILON, f64::EPSILON / 2.0, 2.0 * f64::EPSILON]
        );
        assert_eq!([ulp(0.0), ulp(-0.0), ulp(TINY), ulp(f64::MIN_POSITIVE)], [TINY; 4]);
        assert_eq!(ulp(f64::MAX), f64::MAX - f64::MAX.next_down());
        // Either side of where the ulp itself turns subnormal: 2⁻⁹⁷⁰'s is 2⁻¹⁰²², 2⁻⁹⁷¹'s is 2⁻¹⁰²³.
        let (above, below) = (f64::from_bits(0x0350_0000_0000_0000), f64::from_bits(0x0340_0000_0000_0000));
        assert_eq!([ulp(above), ulp(below)], [f64::MIN_POSITIVE, f64::MIN_POSITIVE / 2.0]);
        assert!(ulp(f64::INFINITY).is_infinite() && ulp(f64::NAN).is_infinite());

        assert_eq!([error(1.0, 1.0, 0.0), error(TINY, TINY, 0.0), error(-3.5, -3.5, 0.0)], [0.0; 3]);
        assert_eq!([error(1.0, 1.0, 0.3), error(1.0, 1.0, -0.3)], [0.3, 0.3]);
        // 1 + 2⁻⁵³ rounds to 1 (ties to even): both neighbours err by ½.
        assert_eq!([error(1.0, 1.0, 0.5), error(ONE_UP, 1.0, 0.5)], [0.5, 0.5]);
        assert_eq!(
            [error(ONE_UP, 1.0, 0.0), error(-ONE_UP, -1.0, 0.0), error(f64::MAX.next_down(), f64::MAX, 0.0)],
            [1.0; 3]
        );
        assert_eq!([error(ONE_DOWN, 1.0, 0.0), error(1.0, ONE_DOWN, 0.0)], [0.5, 1.0]);
        // Exact 2 − ¼·2⁻⁵¹, rounded to 2: 2 − 2⁻⁵² errs by ¼ of 2's ulp.
        assert_eq!(error(2.0f64.next_down(), 2.0, -0.25), 0.25);
        assert_eq!(
            [error(2.0 * TINY, TINY, 0.0), error(f64::MIN_POSITIVE, f64::MIN_POSITIVE.next_down(), 0.0)],
            [1.0; 2]
        );
        assert_eq!(
            [error(TINY, 0.0, 0.25), error(0.0, 0.0, 0.25), error(-TINY, 0.0, 0.0), error(-0.0, 0.0, 0.0)],
            [0.75, 0.25, 1.0, 0.0]
        );
        assert_eq!([error(f64::NAN, f64::NAN, 0.0), error(f64::INFINITY, f64::INFINITY, 0.0)], [0.0; 2]);
        for (got, want) in [
            (f64::NAN, 1.0),
            (1.0, f64::NAN),
            (f64::MAX, f64::INFINITY),
            (f64::INFINITY, f64::MAX),
            (f64::NEG_INFINITY, f64::INFINITY),
        ] {
            assert_eq!(error(got, want, 0.0), f64::INFINITY, "{got} against {want}");
        }
    }
}
