//! Core's own elementary functions (PLAN.md M23; user decisions IM1-IM5): each one scalar function under 1 ulp, with a
//! main path free of branches and divisions, a test for whether a value takes it, and selects for the special cases,
//! so that its lane form maps the same function across [`Lanes`](super::Lanes) and each lane is the scalar path's bits
//! by construction (K17). `expm1`, the `libm` crate's ported (M12.8, M23.5), has no main path: it computes every path,
//! one division among them, and selects. With the `libm` feature `math` and `Lanes` run them; `internal` exposes the
//! scalar functions in every build, for the tests against the reference values (`phasekit-verify`'s `fixtures/math/`).

mod exp;
mod expm1;
mod ln;
mod pow;

pub use exp::exp;
#[cfg(feature = "libm")]
pub(crate) use exp::exp_lanes;
pub use expm1::expm1;
#[cfg(feature = "libm")]
pub(crate) use expm1::expm1_lanes;
pub use ln::ln;
#[cfg(feature = "libm")]
pub(crate) use ln::ln_lanes;
pub use pow::pow;
#[cfg(feature = "libm")]
pub(crate) use pow::pow_lanes;

/// a + b as hi + lo exactly, hi the rounded sum and lo its rounding error (Dekker's Fast2Sum), for |a| ≥ |b|.
#[inline(always)]
fn fast_two_sum(a: f64, b: f64) -> (f64, f64) {
    let hi = a + b;
    (hi, a - hi + b)
}

/// a + b as hi + lo exactly, hi the rounded sum and lo its rounding error, whichever of a and b is larger (Knuth's
/// TwoSum).
#[inline(always)]
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let hi = a + b;
    let back = hi - a;
    (hi, (a - (hi - back)) + (b - back))
}

/// 2²⁷, the scale of [`split`].
const TWO27: f64 = f64::from_bits(0x41a0_0000_0000_0000);

/// r as rhi + rlo exactly, rhi r to about 26 bits, so that rhi² is exact: r·2²⁷ added and taken away (Arm's split in
/// `log` without FMA).
#[inline(always)]
fn split(r: f64) -> (f64, f64) {
    let w = r * TWO27;
    let rhi = r + w - w;
    (rhi, r - rhi)
}

#[cfg(test)]
mod tests {
    use super::{fast_two_sum, split, two_sum};
    use crate::num::lanes::tests::Rng;

    /// [`fast_two_sum`] keeps what the rounded sum drops: 1 + (¼ + 2⁻⁵⁴) rounds to 1.25 and leaves 2⁻⁵⁴, and the same
    /// negated. `ln_is_faithful` cannot see a sum that loses it: on `ln`'s main path `w − r` for `w + r` moves about one
    /// result in a thousand by 1 ulp, all still under 1 ulp at M23.1's values (M23.3's mutants).
    #[test]
    fn fast_two_sum_is_exact() {
        let tiny = f64::from_bits(0x3c90_0000_0000_0000);
        assert_eq!(fast_two_sum(1.0, 0.25 + tiny), (1.25, tiny));
        assert_eq!(fast_two_sum(-1.0, -0.25 - tiny), (-1.25, -tiny));
    }

    /// [`two_sum`] is exact in either order: it is [`fast_two_sum`] with the larger first, over 10 000 random pairs
    /// across 40 binades of either sign, and keeps 2⁻⁶⁰ beside 1 with the small one first, which Fast2Sum would lose.
    #[test]
    fn two_sum_is_exact() {
        let mut rng = Rng(5);
        for _ in 0..10_000 {
            let scale = f64::from_bits((1003 + rng.next() % 41) << 52);
            let (a, b) = (rng.uniform(-1.0, 1.0) * scale, rng.uniform(-1.0, 1.0));
            let (big, small) = if a.abs() >= b.abs() { (a, b) } else { (b, a) };
            assert_eq!(two_sum(a, b), fast_two_sum(big, small), "{a:e} + {b:e}");
            assert_eq!(two_sum(b, a), fast_two_sum(big, small), "{b:e} + {a:e}");
        }
        let tiny = f64::from_bits(0x3c30_0000_0000_0000);
        assert_eq!((two_sum(tiny, 1.0), two_sum(-tiny, -1.0)), ((1.0, tiny), (-1.0, -tiny)));
    }

    /// [`split`] cuts r at 26 bits: 2⁻⁴ + 2⁻²⁷ + 2⁻³⁴ (2⁻²⁷ its 24th bit, 2⁻³⁴ its 31st) is 2⁻⁴ + 2⁻²⁷ and 2⁻³⁴. Cut
    /// elsewhere, rhi² rounds, and `ln` near 1 keeps that rounding, under a tenth of an ulp, which `ln_is_faithful`
    /// cannot see (M23.3's mutants).
    #[test]
    fn split_keeps_26_bits() {
        let (b27, b34) = (f64::from_bits(0x3e40_0000_0000_0000), f64::from_bits(0x3dd0_0000_0000_0000));
        assert_eq!(split(0.0625 + b27 + b34), (0.0625 + b27, b34));
    }
}
