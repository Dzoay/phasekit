//! `exp` on every lane (PLAN.md M12.5): the `libm` crate's (0.2.16, `src/math/exp.rs` from FreeBSD's `e_exp.c`, and
//! its generic `scalbn`), operation for operation, so each lane is the `libm` path's bits. Built only with `libm`:
//! std's `exp` is the platform's, which no lane code can reproduce.
//!
//! libm branches on the high word of |x|. Here every lane takes every path and the result is selected in libm's order,
//! so the lanes vectorise: k is truncated in floating point and read back from the low bits of 1.5·2⁵² + k (an `as i32`
//! stays scalar), and nothing is looked up in a table. A group whose lanes all lie in 2⁻²⁸ < |x| < 708.39… skips the
//! special cases and the prescale.

use super::Lanes;

// libm's constants by the words its comments give (its decimals carry more digits than an f64 holds).
pub(super) const LN2HI: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
pub(super) const LN2LO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
pub(super) const INVLN2: f64 = f64::from_bits(0x3ff7_1547_652b_82fe);
const P1: f64 = f64::from_bits(0x3fc5_5555_5555_553e);
const P2: f64 = f64::from_bits(0xbf66_c16c_16be_bd93);
const P3: f64 = f64::from_bits(0x3f11_566a_af25_de2c);
const P4: f64 = f64::from_bits(0xbebb_bd41_c5d2_6bf1);
const P5: f64 = f64::from_bits(0x3e66_3769_72be_a4d0);
/// libm's overflow threshold, 709.782712893383973096.
pub(super) const OVERFLOW: f64 = f64::from_bits(0x4086_2e42_fefa_39ef);
/// libm's underflow-to-zero threshold, −745.13321910194110842.
const UNDERFLOW: f64 = f64::from_bits(0xc087_4910_d52d_3051);

/// |x|'s bits. libm compares the high word of |x| with `h`; on all 64 bits that is `> h·2³² + 2³² − 1` for its `>`
/// and `>= h·2³²` for its `>=`.
#[inline(always)]
pub(super) fn abs_bits(x: f64) -> u64 {
    x.to_bits() & 0x7fff_ffff_ffff_ffff
}

/// 2ⁿ for n in [−1022, 1023].
#[inline(always)]
pub(super) fn pow2(n: i64) -> f64 {
    f64::from_bits((n.wrapping_add(1023) as u64).wrapping_shl(52))
}

/// libm's argument reduction and polynomial: `(y, k)` with exp(x) = y·2ᵏ for 2⁻²⁸ < |x| ≤ 709.78…; any other lane
/// gets a value its special case replaces.
#[inline(always)]
fn reduced(x: f64) -> (f64, i64) {
    let ax = abs_bits(x);
    let half = if x.is_sign_negative() { -0.5 } else { 0.5 };
    // k: `(INVLN2 * x + HALF[sign]) as i32` from |x| ≥ 1.5 ln 2, `1 - sign - sign` above 0.5 ln 2, else 0.
    let k = if ax >= 0x3ff0_a2b2_0000_0000 {
        (INVLN2 * x + half).trunc()
    } else if ax > 0x3fd6_2e42_ffff_ffff {
        half + half
    } else {
        0.0
    };
    let hi = x - k * LN2HI;
    let lo = k * LN2LO;
    let r = hi - lo;
    let rr = r * r;
    let c = r - rr * (P1 + rr * (P2 + rr * (P3 + rr * (P4 + rr * P5))));
    let y = 1. + (r * c / (2. - c) - lo + hi);
    (y, ((k + 6_755_399_441_055_744.0).to_bits() as i64).wrapping_sub(0x4338_0000_0000_0000))
}

/// Whether libm takes its main path at `x` with k in [−1022, 1022]: 2⁻²⁸ < |x| < 708.39….
#[inline(always)]
fn plain(x: f64) -> bool {
    (0x3e30_0001_0000_0000..0x4086_232b_0000_0000).contains(&abs_bits(x))
}

/// libm's `exp` at a lane in its main path with no prescale: y·2ᵏ (y itself at k = 0, as y·1 is y).
#[inline(always)]
fn exp_plain(x: f64) -> f64 {
    let (y, k) = reduced(x);
    y * pow2(k)
}

/// libm's `exp` at any lane: the main path with `scalbn`'s single prescale (by 2¹⁰²³ above k = 1023, by 2⁻⁹⁶⁹ below
/// k = −1022, all exp's k need), and the special cases: 1 + x for |x| ≤ 2⁻²⁸, 0 below −745.13…, overflow above
/// 709.78…, NaN itself.
#[inline(always)]
fn exp_any(x: f64) -> f64 {
    let (y, k) = reduced(x);
    let (pre, n) = if k > 1023 {
        (pow2(1023), k.wrapping_sub(1023))
    } else if k < -1022 {
        (pow2(-969), k.wrapping_add(969))
    } else {
        (1.0, k)
    };
    // libm's cases exclude each other, so selecting in any order gives its result.
    let r = if abs_bits(x) < 0x3e30_0001_0000_0000 { 1. + x } else { (y * pre) * pow2(n) };
    let r = if x < UNDERFLOW { 0. } else { r };
    let r = if x > OVERFLOW { x * pow2(1023) } else { r };
    if x.is_nan() { x } else { r }
}

/// `exp` of every lane.
#[inline(always)]
pub(super) fn exp<const W: usize>(x: Lanes<W>) -> Lanes<W> {
    if x.0.iter().all(|&x| plain(x)) { x.map(exp_plain) } else { x.map(exp_any) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{LaneKernel, run_lanes};
    use crate::num::lanes::tests::Rng;
    use crate::num::{Real, math};

    /// `exp` as a lane kernel: `math::exp` (the `libm` crate's) at a point, `Lanes::exp` across lanes.
    struct Exp;

    impl LaneKernel for Exp {
        type Out = u64;
        fn point(&self, x: f64, _: f64) -> u64 {
            math::exp(x).to_bits()
        }
        fn lanes<const W: usize>(&self, x: Lanes<W>, _: Lanes<W>) -> [u64; W] {
            x.exp().0.map(f64::to_bits)
        }
    }

    /// The edges of VERIFICATION.md §9.2 for `exp`, each with its neighbours one ulp either side: ±0, |x| around 2⁻²⁸,
    /// the reduction boundaries 0.5 ln 2 and 1.5 ln 2 (libm's high-word thresholds), the special-case threshold
    /// 708.39…, overflow at 709.78…, underflow to 0 at −745.13…, subnormal results between, NaN, ±∞ and ±`f64::MAX`.
    fn edges() -> Vec<f64> {
        let bits = [0x3e30_0000_ffff_ffff_u64, 0x3fd6_2e42_ffff_ffff, 0x3ff0_a2b2_0000_0000, 0x4086_232b_0000_0000];
        let mut edges = vec![0.0, f64::NAN, f64::INFINITY, f64::MAX, f64::from_bits(0x3e30_0000_0000_0000)];
        edges.extend(bits.map(f64::from_bits));
        edges.extend([0.5, 1.5].map(|m| m * core::f64::consts::LN_2));
        edges.extend([f64::from_bits(0x4086_232b_dd7a_bcd2), OVERFLOW, -UNDERFLOW, 1e-300, 5e-324]);
        edges.extend([-708.5, -720.0, -740.0, -745.0, -1000.0, 1000.0]);
        let signed: Vec<f64> = edges.iter().flat_map(|&e| [e, -e]).collect();
        signed
            .iter()
            .flat_map(|&e| [e, f64::from_bits(e.to_bits() + 1), f64::from_bits(e.to_bits().wrapping_sub(1))])
            .collect()
    }

    /// The test inputs, in an order that gives `exp` both kinds of group: 30 000 each of α^r's argument range (−60 to 5,
    /// whole groups inside 2⁻²⁸ < |x| < 708.39…), of [−750, 750] and of every bit pattern; then each edge alone among
    /// seven ordinary lanes, and the edges side by side.
    fn inputs() -> Vec<f64> {
        let mut rng = Rng(15);
        let mut x: Vec<f64> = (0..30_000).map(|_| rng.uniform(-60.0, 5.0)).collect();
        x.extend((0..30_000).map(|_| rng.uniform(-750.0, 750.0)));
        x.extend((0..30_000).map(|_| f64::from_bits(rng.next())));
        for e in edges() {
            x.push(e);
            x.extend((0..7).map(|_| rng.uniform(-60.0, 5.0)));
        }
        x.extend(edges());
        x
    }

    /// `Exp` over `x` in groups of W, against `math::exp` at each point.
    fn assert_exp(x: &[f64], run: impl Fn(&[f64], &mut [u64]), what: &str) {
        let mut got = vec![0; x.len()];
        run(x, &mut got);
        for (x, got) in x.iter().zip(got) {
            let want = math::exp(*x).to_bits();
            assert!(got == want, "{what}: exp({x:e}) = {:e}, libm's {:e}", f64::from_bits(got), f64::from_bits(want));
        }
    }

    /// PLAN.md M12.5 (VERIFICATION.md §9.2, class `Exact`): `Lanes::exp` is the `libm` crate's `exp` bit for bit, on 2,
    /// 4 and 8 lanes, and with the `simd` feature at every dispatch level of this CPU, on random inputs over α^r's
    /// argument range and over all of `f64`, and on the edges.
    #[test]
    fn lane_exp_is_libm_bitwise() {
        let x = inputs();
        assert_exp(&x, |x, out| run_lanes::<_, 2>(&Exp, x, x, out), "2 lanes");
        assert_exp(&x, |x, out| run_lanes::<_, 4>(&Exp, x, x, out), "4 lanes");
        assert_exp(&x, |x, out| run_lanes::<_, 8>(&Exp, x, x, out), "8 lanes");
        #[cfg(feature = "simd")]
        for level in crate::exec::tests::levels() {
            assert_exp(&x, |x, out| crate::exec::run_simd(level, &Exp, x, x, out), &format!("{level:?}"));
        }
        assert!(x.len() > 90_000 && x.iter().any(|x| x.is_nan()));
    }
}
