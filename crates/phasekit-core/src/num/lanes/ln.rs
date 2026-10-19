// origin: FreeBSD /usr/src/lib/msun/src/e_log.c, through the libm crate 0.2.16 (MIT); see the crate's NOTICE.
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//
// Developed at SunSoft, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this
// software is freely granted, provided that this notice
// is preserved.
// ====================================================

//! `ln` on every lane (PLAN.md M12.6): the `libm` crate's `log` (0.2.16, `src/math/log.rs` from FreeBSD's `e_log.c`),
//! operation for operation, so each lane is the `libm` path's bits. Built only with `libm`, as `exp` is.
//!
//! libm reduces x into [√2/2, √2) with integer operations on its high word; here they run on all 64 bits, which
//! leaves the low word as libm does. Every lane takes every path and the result is selected: k becomes a float through
//! the low bits of 2⁵² + e instead of an `as f64` of an integer. A group of positive normal numbers skips the
//! subnormal scaling and the special cases.

use super::Lanes;

// libm's constants by the words its comments give.
const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
const LN2_LO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
const LG1: f64 = f64::from_bits(0x3fe5_5555_5555_5593);
const LG2: f64 = f64::from_bits(0x3fd9_9999_9997_fa04);
const LG3: f64 = f64::from_bits(0x3fd2_4924_9422_9359);
const LG4: f64 = f64::from_bits(0x3fcc_71c5_1d8e_78af);
const LG5: f64 = f64::from_bits(0x3fc7_4664_96cb_03de);
const LG6: f64 = f64::from_bits(0x3fc3_9a09_d078_c69f);
const LG7: f64 = f64::from_bits(0x3fc2_f112_df3e_5244);
/// 2⁵⁴, libm's scale for subnormal x.
const X1P54: f64 = f64::from_bits(0x4350_0000_0000_0000);
/// 2⁵², whose low bits hold an integer below 2⁵².
const TWO52: f64 = f64::from_bits(0x4330_0000_0000_0000);

/// libm's reduction and polynomial for positive normal x, with `k0` added to its exponent (−54 after scaling a
/// subnormal x by 2⁵⁴, else 0).
#[inline(always)]
fn reduced(x: f64, k0: f64) -> f64 {
    let ui = x.to_bits();
    // hx += 0x3ff00000 - 0x3fe6a09e; k += (hx >> 20) - 0x3ff; hx = (hx & 0x000fffff) + 0x3fe6a09e
    // libm's uint32 arithmetic: no carry reaches bit 32, so wrapping and plain addition agree.
    let hx = (ui >> 32).wrapping_add(0x3ff0_0000 - 0x3fe6_a09e);
    let dk = (f64::from_bits(TWO52.to_bits() | (hx >> 20)) - TWO52 - 1023.0) + k0;
    let hx = (hx & 0x000f_ffff).wrapping_add(0x3fe6_a09e);
    let x = f64::from_bits((hx << 32) | (ui & 0xffff_ffff));
    let f = x - 1.0;
    let hfsq = 0.5 * f * f;
    let s = f / (2.0 + f);
    let z = s * s;
    let w = z * z;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    let r = t2 + t1;
    s * (hfsq + r) + dk * LN2_LO - hfsq + f + dk * LN2_HI
}

/// Whether x is a positive normal number, where libm's `log` takes its main path (at 1 it returns 0, which the main
/// path gives too).
#[inline(always)]
fn plain(x: f64) -> bool {
    (0x0010_0000_0000_0000..0x7ff0_0000_0000_0000).contains(&x.to_bits())
}

/// libm's `log` at any lane: the main path, with x scaled by 2⁵⁴ below the smallest normal, and libm's special cases:
/// −∞ at ±0, NaN below 0, x itself at +∞ and NaN, 0 at 1. They exclude each other, so the order of the selects does not
/// matter.
#[inline(always)]
fn ln_any(x: f64) -> f64 {
    let ui = x.to_bits();
    let (scaled, k0) = if ui < 0x0010_0000_0000_0000 { (x * X1P54, -54.0) } else { (x, 0.0) };
    let r = reduced(scaled, k0);
    let r = if ui == 0x3ff0_0000_0000_0000 { 0. } else { r };
    let r = if (0x7ff0_0000_0000_0000..0x8000_0000_0000_0000).contains(&ui) { x } else { r };
    #[expect(clippy::eq_op, reason = "libm's NaN for a negative x, operation for operation")]
    let r = if x.is_sign_negative() { (x - x) / 0.0 } else { r };
    if ui << 1 == 0 { -1. / (x * x) } else { r }
}

/// `ln` of every lane.
#[inline(always)]
pub(super) fn ln<const W: usize>(x: Lanes<W>) -> Lanes<W> {
    if x.0.iter().all(|&x| plain(x)) { x.map(|x| reduced(x, 0.0)) } else { x.map(ln_any) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{LaneKernel, run_lanes};
    use crate::num::lanes::tests::{Rng, canonical};
    use crate::num::{Real, math};

    /// `ln` as a lane kernel: `math::ln` (the `libm` crate's `log`) at a point, `Lanes::ln` across lanes.
    struct Ln;

    impl LaneKernel for Ln {
        type Out = u64;
        fn point(&self, x: f64, _: f64) -> u64 {
            canonical(math::ln(x))
        }
        fn lanes<const W: usize>(&self, x: Lanes<W>, _: Lanes<W>) -> [u64; W] {
            x.ln().0.map(canonical)
        }
    }

    /// The edges of VERIFICATION.md §9.2 for `ln`, each with its neighbours one ulp either side: ±0, negative inputs
    /// (−1, −`f64::MIN_POSITIVE`, −∞), the subnormals and the smallest normal (libm scales below it), 1, the √2/2
    /// reduction boundary (libm's high word 0x3fe6a09e) and √2, `f64::MAX`, +∞ and NaN.
    fn edges() -> Vec<f64> {
        let bits = [0x3fe6_a09e_0000_0000_u64, 0x3fe6_a09e_667f_3bcd, 0x3ff6_a09e_0000_0000, 0x3ff6_a09e_667f_3bcd];
        let mut edges = vec![0.0, -0.0, 1.0, -1.0, f64::MIN_POSITIVE, -f64::MIN_POSITIVE, 5e-324, 1e-310, 2.0, 0.5];
        edges.extend(bits.map(f64::from_bits));
        edges.extend([f64::MAX, f64::INFINITY, f64::NEG_INFINITY, f64::NAN]);
        edges
            .iter()
            .flat_map(|&e| [e, f64::from_bits(e.to_bits() + 1), f64::from_bits(e.to_bits().wrapping_sub(1))])
            .collect()
    }

    /// The test inputs, in an order that gives `ln` both kinds of group: 30 000 each over τ's range in α^r (10⁻³ to 20,
    /// whole groups of positive normal numbers), over [0, 10⁶] and over every bit pattern; then each edge alone among
    /// seven ordinary lanes, and the edges side by side.
    fn inputs() -> Vec<f64> {
        let mut rng = Rng(16);
        let mut x: Vec<f64> = (0..30_000).map(|_| rng.uniform(1e-3, 20.0)).collect();
        x.extend((0..30_000).map(|_| rng.uniform(0.0, 1e6)));
        x.extend((0..30_000).map(|_| f64::from_bits(rng.next())));
        for e in edges() {
            x.push(e);
            x.extend((0..7).map(|_| rng.uniform(1e-3, 20.0)));
        }
        x.extend(edges());
        x
    }

    /// `Ln` over `x` in groups of W, against `math::ln` at each point, NaN canonical (VERIFICATION.md §9.4).
    fn assert_ln(x: &[f64], run: impl Fn(&[f64], &mut [u64]), what: &str) {
        let mut got = vec![0; x.len()];
        run(x, &mut got);
        for (x, got) in x.iter().zip(got) {
            let want = canonical(math::ln(*x));
            assert!(got == want, "{what}: ln({x:e}) = {:e}, libm's {:e}", f64::from_bits(got), f64::from_bits(want));
        }
    }

    /// PLAN.md M12.6 (VERIFICATION.md §9.2, class `Exact`): `Lanes::ln` is the `libm` crate's `log` bit for bit (NaN
    /// canonical), on 2, 4 and 8 lanes, and with the `simd` feature at every dispatch level of this CPU, on random
    /// inputs and on the edges.
    #[test]
    fn lane_ln_is_libm_bitwise() {
        let x = inputs();
        assert_ln(&x, |x, out| run_lanes::<_, 2>(&Ln, x, x, out), "2 lanes");
        assert_ln(&x, |x, out| run_lanes::<_, 4>(&Ln, x, x, out), "4 lanes");
        assert_ln(&x, |x, out| run_lanes::<_, 8>(&Ln, x, x, out), "8 lanes");
        #[cfg(feature = "simd")]
        for level in crate::exec::tests::levels() {
            assert_ln(&x, |x, out| crate::exec::run_simd(level, &Ln, x, x, out), &format!("{level:?}"));
        }
        assert!(x.len() > 90_000 && x.iter().any(|x| x.is_nan()));
    }
}
