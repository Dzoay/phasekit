// origin: FreeBSD /usr/src/lib/msun/src/s_expm1.c, through the libm crate 0.2.16 (MIT); see the crate's NOTICE.
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//
// Developed at SunPro, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this
// software is freely granted, provided that this notice
// is preserved.
// ====================================================

//! `expm1` on every lane (PLAN.md M12.8): the `libm` crate's (0.2.16, `src/math/expm1.rs` from FreeBSD's `s_expm1.c`),
//! operation for operation, so each lane is the `libm` path's bits. Built only with `libm`, as `exp` is.
//!
//! libm branches on the high word of |x| and then on k. Here every lane takes every path and the result is selected,
//! so the lanes vectorise, as in `exp`: k is truncated in floating point, and read back from the low bits of 1.5·2⁵² + k
//! only for 2ᵏ and 2⁻ᵏ. libm's reduction at k = ±1 (x ∓ ln2_hi, ±ln2_lo) is the general one's at k = ±1 exactly, and
//! at k = 0 the general one leaves x as it is, so one reduction serves every lane.

use super::Lanes;
use super::exp::{INVLN2, LN2HI, LN2LO, OVERFLOW, abs_bits, pow2};

// libm's constants by the words its comments give (its decimals carry more digits than an f64 holds); the scaled Qs of
// R(2z) with z = x²/2.
const Q1: f64 = f64::from_bits(0xbfa1_1111_1111_10f4);
const Q2: f64 = f64::from_bits(0x3f5a_01a0_19fe_5585);
const Q3: f64 = f64::from_bits(0xbf14_ce19_9eaa_dbb7);
const Q4: f64 = f64::from_bits(0x3ed0_cfca_86e6_5239);
const Q5: f64 = f64::from_bits(0xbe8a_fdb7_6e09_c32d);

/// libm's argument reduction: k as a float, and the reduced x = r with its correction c, x = k ln 2 + r + c. k is
/// `(INVLN2 * x + ±0.5) as i32` from |x| ≥ 1.5 ln 2, ±1 above 0.5 ln 2, else 0.
#[inline(always)]
fn reduced(x: f64) -> (f64, f64, f64) {
    let ax = abs_bits(x);
    let half = if x.is_sign_negative() { -0.5 } else { 0.5 };
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
    (k, r, (hi - r) - lo)
}

/// libm's 2ᵏ(r − e + 1) − 1 for k other than 0 and ±1: the scaled form from k = 57 and below k = −1, else the forms
/// for k below 20 and from 20.
#[inline(always)]
fn by_power(k: f64, r: f64, e: f64) -> f64 {
    let n = ((k + 6_755_399_441_055_744.0).to_bits() as i64).wrapping_sub(0x4338_0000_0000_0000);
    let (twopk, uf) = (pow2(n), pow2(n.wrapping_neg()));
    let y = r - e + 1.0;
    let far = (if k == 1024.0 { y * 2.0 * pow2(1023) } else { y * twopk }) - 1.0;
    let near = if k < 20.0 { (r - e + (1.0 - uf)) * twopk } else { (r - (e + uf) + 1.0) * twopk };
    if k < 0.0 || k > 56.0 { far } else { near }
}

/// libm's `expm1` from the reduction: the polynomial in r, then exp(x) − 1 ≈ 2ᵏ(r − e + 1) − 1 in libm's form for k.
#[inline(always)]
fn by_k(k: f64, r: f64, c: f64) -> f64 {
    let hfx = 0.5 * r;
    let hxs = r * hfx;
    let r1 = 1.0 + hxs * (Q1 + hxs * (Q2 + hxs * (Q3 + hxs * (Q4 + hxs * Q5))));
    let t = 3.0 - r1 * hfx;
    let e0 = hxs * ((r1 - t) / (6.0 - r * t));
    let e = r * (e0 - c) - c - hxs;
    let at_1 = if r < -0.25 { -2.0 * (e - (r + 0.5)) } else { 1.0 + 2.0 * (r - e) };
    let out = if k == 1.0 { at_1 } else { by_power(k, r, e) };
    let out = if k == -1.0 { 0.5 * (r - e) - 0.5 } else { out };
    if k == 0.0 { r - (r * e0 - hxs) } else { out }
}

/// libm's `expm1` at one lane, every path computed and selected in turn. libm's cases exclude each other, so the order
/// of the selects does not matter. The special cases: x itself below 2⁻⁵⁴ and at NaN, −1 from |x| ≥ 56 ln 2 for a
/// negative x, overflow above 709.78….
#[inline(always)]
fn expm1_lane(x: f64) -> f64 {
    let ax = abs_bits(x);
    let (k, r, c) = reduced(x);
    let out = if ax < 0x3c90_0000_0000_0000 { x } else { by_k(k, r, c) };
    let out = if ax >= 0x4043_687a_0000_0000 && x.is_sign_negative() { -1.0 } else { out };
    let out = if x > OVERFLOW { x * pow2(1023) } else { out };
    if x.is_nan() { x } else { out }
}

/// `expm1` of every lane.
#[inline(always)]
pub(super) fn expm1<const W: usize>(x: Lanes<W>) -> Lanes<W> {
    x.map(expm1_lane)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{LaneKernel, run_lanes};
    use crate::num::lanes::exp::{LN2HI, LN2LO, OVERFLOW};
    use crate::num::lanes::tests::{Rng, canonical};
    use crate::num::{Real, math};

    /// `expm1` as a lane kernel: `math::expm1` (the `libm` crate's) at a point, `Lanes::expm1` across lanes.
    struct Expm1;

    impl LaneKernel for Expm1 {
        type Out = u64;
        fn point(&self, x: f64, _: f64) -> u64 {
            canonical(math::expm1(x))
        }
        fn lanes<const W: usize>(&self, x: Lanes<W>, _: Lanes<W>) -> [u64; W] {
            x.expm1().0.map(canonical)
        }
    }

    /// The edges of VERIFICATION.md §9.2 for `expm1`, each with its neighbours one ulp either side and both signs: ±0,
    /// libm's high-word thresholds (|x| at 2⁻⁵⁴, 0.5 ln 2, 1.5 ln 2 and 56 ln 2, where a negative x saturates at −1), the
    /// k = 1 path's switch at a reduced x of −0.25 (the one x that reduces to it, ln2_hi − 1/4 + ln2_lo), where k becomes
    /// 20 and 57 ((k − 1/2) ln 2), k = 1024 just below the overflow threshold 709.78…, which is an edge too, subnormals,
    /// NaN, ∞ and `f64::MAX`. And two x with k = 20 where libm's forms for k below 20 and from 20 differ in the last bit
    /// (found by search, about one in two million there).
    fn edges() -> Vec<f64> {
        let bits = [0x3c90_0000_0000_0000_u64, 0x3fd6_2e42_ffff_ffff, 0x3ff0_a2b2_0000_0000, 0x4043_687a_0000_0000];
        let mut edges = vec![0.0, f64::NAN, f64::INFINITY, f64::MAX, 1e-300, 5e-324, f64::MIN_POSITIVE];
        edges.extend(bits.map(f64::from_bits));
        edges.extend([LN2HI - 0.25 + LN2LO, 19.5 * LN2HI, 56.5 * LN2HI, 709.5, 709.7, OVERFLOW]);
        edges.extend([13.575_261_892_990_213, 13.729_412_542_578_002]);
        edges.extend([0.5, 1.0, 3.0, 30.0, 100.0, 700.0, 1000.0]);
        let signed: Vec<f64> = edges.iter().flat_map(|&e| [e, -e]).collect();
        signed
            .iter()
            .flat_map(|&e| [e, f64::from_bits(e.to_bits() + 1), f64::from_bits(e.to_bits().wrapping_sub(1))])
            .collect()
    }

    /// The test inputs: 30 000 each over the range α⁰'s Planck-Einstein terms give it (θτ and −θτ, −60 to 60), over
    /// [−750, 750] and over every bit pattern; then each edge alone among seven ordinary lanes, and the edges side by
    /// side.
    fn inputs() -> Vec<f64> {
        let mut rng = Rng(16);
        let mut x: Vec<f64> = (0..30_000).map(|_| rng.uniform(-60.0, 60.0)).collect();
        x.extend((0..30_000).map(|_| rng.uniform(-750.0, 750.0)));
        x.extend((0..30_000).map(|_| f64::from_bits(rng.next())));
        for e in edges() {
            x.push(e);
            x.extend((0..7).map(|_| rng.uniform(-60.0, 60.0)));
        }
        x.extend(edges());
        x
    }

    /// `Expm1` over `x` in groups of W, against `math::expm1` at each point (NaN canonical, VERIFICATION.md §9.4).
    fn assert_expm1(x: &[f64], run: impl Fn(&[f64], &mut [u64]), what: &str) {
        let mut got = vec![0; x.len()];
        run(x, &mut got);
        for (x, got) in x.iter().zip(got) {
            let want = canonical(math::expm1(*x));
            let (g, w) = (f64::from_bits(got), f64::from_bits(want));
            assert!(got == want, "{what}: expm1({x:e}) = {g:e}, libm's {w:e}");
        }
    }

    /// PLAN.md M12.8 (VERIFICATION.md §9.2, class `Exact`): `Lanes::expm1` is the `libm` crate's `expm1` bit for bit, on
    /// 2, 4 and 8 lanes, and with the `simd` feature at every dispatch level of this CPU, on random inputs over α⁰'s
    /// argument range and over all of `f64`, and on the edges.
    #[test]
    fn lane_expm1_is_libm_bitwise() {
        let x = inputs();
        assert_expm1(&x, |x, out| run_lanes::<_, 2>(&Expm1, x, x, out), "2 lanes");
        assert_expm1(&x, |x, out| run_lanes::<_, 4>(&Expm1, x, x, out), "4 lanes");
        assert_expm1(&x, |x, out| run_lanes::<_, 8>(&Expm1, x, x, out), "8 lanes");
        #[cfg(feature = "simd")]
        for level in crate::exec::tests::levels() {
            assert_expm1(&x, |x, out| crate::exec::run_simd(level, &Expm1, x, x, out), &format!("{level:?}"));
        }
        assert!(x.len() > 90_000 && x.iter().any(|x| x.is_nan()));
    }
}
