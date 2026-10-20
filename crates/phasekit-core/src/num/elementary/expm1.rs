//! `expm1` (PLAN.md M23.5): e^x − 1 on M23.2's `exp`, its reduction and 128-entry table. With exp(x) ≈ scale +
//! scale·tmp, e^x − 1 = (scale − 1) + scale·tmp, scale − 1 summed exactly (its rounding error kept), so the one
//! rounding that matters is the last. That is the main path, from 2⁻⁵ to 512 in magnitude, where scale − 1 is at
//! least 2⁻⁵ and tmp's rounding stays under a tenth of an ulp. Below 2⁻⁵ it is Taylor's polynomial to x⁸; from 512,
//! `exp` less 1, which is `exp` or −1; x itself below 2⁻⁵⁴ and at NaN. Each case is computed beside the main path and
//! selected, so a group of lanes never branches on a lane. It replaced M12.8's port of the `libm` crate's `expm1`,
//! being more accurate (0.54 ulp at worst on M23.1's values, the port 0.72) and faster.

#[cfg(any(test, feature = "libm"))]
use crate::num::Lanes;

use super::{exp, two_sum};

/// |x|'s bits at the main path's bounds, 2⁻⁵ and 512, and at 2⁻⁵⁴, below which e^x − 1 rounds to x.
const NEAR: u64 = 0x3fa0_0000_0000_0000;
const LARGE: u64 = 0x4080_0000_0000_0000;
const TINY: u64 = 0x3c90_0000_0000_0000;
/// Taylor's coefficients of (e^x − 1 − x)/x², 1/n! for n = 2 to 8. Below 2⁻⁵ the next term is under 2⁻⁵⁸ of x.
const T2: f64 = 1.0 / 2.0;
const T3: f64 = 1.0 / 6.0;
const T4: f64 = 1.0 / 24.0;
const T5: f64 = 1.0 / 120.0;
const T6: f64 = 1.0 / 720.0;
const T7: f64 = 1.0 / 5040.0;
const T8: f64 = 1.0 / 40320.0;

/// |x|'s bits.
#[inline(always)]
fn abs_bits(x: f64) -> u64 {
    x.to_bits() & 0x7fff_ffff_ffff_ffff
}

/// Whether `x` takes the main path: 2⁻⁵ ≤ |x| < 512.
#[inline(always)]
fn plain(x: f64) -> bool {
    // Equivalent (M23.5): on the main path `expm1_any` is `by_table`'s value bit for bit, so taking it at every `x`
    // changes only the speed.
    // equivalent mutant: replace plain -> bool with false
    (NEAR..LARGE).contains(&abs_bits(x))
}

/// e^x − 1 on the main path: (scale − 1) + scale·tmp from `exp`'s reduction, scale − 1 as hi + lo exactly.
#[inline(always)]
fn by_table(x: f64) -> f64 {
    // Equivalent (M23.5): as at `exp`'s tail, +0 would change r only at r = −0, which only x = −0 gives, where this
    // value is not taken.
    // equivalent mutant: delete -
    let (tmp, sbits, _) = exp::reduce(x, -0.0, 0);
    let scale = f64::from_bits(sbits);
    let (hi, lo) = two_sum(scale, -1.0);
    hi + (lo + scale * tmp)
}

/// e^x − 1 below 2⁻⁵ in magnitude: x + x²·(1/2 + x/6 + … + x⁶/8!).
#[inline(always)]
fn near(x: f64) -> f64 {
    let p = T6 + x * (T7 + x * T8);
    let p = T4 + x * (T5 + x * p);
    let p = T2 + x * (T3 + x * p);
    x + x * x * p
}

/// e^x − 1 at any `x`: the main path's value, or the case `x` falls in, each computed and selected. On the main path
/// it is [`by_table`]'s value bit for bit.
#[inline(always)]
fn expm1_any(x: f64) -> f64 {
    let ax = abs_bits(x);
    // Equivalent (M23.5): at |x| = 2⁻⁵ Taylor's value and the table's are the same bits
    // (`the_paths_agree_where_they_meet`).
    // equivalent mutant: replace < with <=
    let r = if ax < NEAR { near(x) } else { by_table(x) };
    // Equivalent (M23.5): at |x| = 2⁻⁵⁴ Taylor's value is x, x²/2 being under half an ulp of x.
    // equivalent mutant: replace < with <=
    let r = if ax < TINY { x } else { r };
    // Equivalent (M23.5): as at `exp`'s tail; this value is taken only from |x| = 512, where r is never −0.
    // equivalent mutant: delete -
    let r = if ax >= LARGE { exp::exp_signed(x, -0.0, 0) - 1.0 } else { r };
    if x.is_nan() { x } else { r }
}

/// e^x − 1, under 1 ulp: on `exp`'s table (PLAN.md M23.5).
pub fn expm1(x: f64) -> f64 {
    if plain(x) { by_table(x) } else { expm1_any(x) }
}

/// [`expm1`] on every lane: the main path where every lane takes it, else every case on every lane.
#[cfg(any(test, feature = "libm"))]
#[inline(always)]
pub(crate) fn expm1_lanes<const W: usize>(x: Lanes<W>) -> Lanes<W> {
    if x.0.iter().all(|&x| plain(x)) { Lanes(x.0.map(by_table)) } else { Lanes(x.0.map(expm1_any)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{LaneKernel, run_lanes};
    use crate::num::lanes::tests::{Rng, canonical};

    /// [`expm1`] as a lane kernel: the scalar function at a point, [`expm1_lanes`] across lanes.
    struct Expm1;

    impl LaneKernel for Expm1 {
        type Out = u64;
        fn point(&self, x: f64, _: f64) -> u64 {
            canonical(expm1(x))
        }
        fn lanes<const W: usize>(&self, x: Lanes<W>, _: Lanes<W>) -> [u64; W] {
            expm1_lanes(x).0.map(canonical)
        }
    }

    /// The edges of VERIFICATION.md §9.2 for `expm1`, each with its neighbours one ulp either side and both signs: ±0,
    /// the paths' bounds (2⁻⁵⁴, 2⁻⁵ and 512), ln 2 (where scale − 1 starts to round), the overflow threshold 709.78…,
    /// subnormals, NaN, ∞ and `f64::MAX`, and arguments of α⁰'s range.
    fn edges() -> Vec<f64> {
        let bits = [TINY, NEAR, LARGE, 0x4086_2e42_fefa_39ef];
        let mut edges = vec![0.0, f64::NAN, f64::INFINITY, f64::MAX, 1e-300, 5e-324, f64::MIN_POSITIVE];
        edges.extend(bits.map(f64::from_bits));
        edges.extend([core::f64::consts::LN_2, 0.5, 1.0, 3.0, 30.0, 100.0, 700.0, 745.2, 1000.0]);
        let signed: Vec<f64> = edges.iter().flat_map(|&e| [e, -e]).collect();
        signed.iter().flat_map(|&e| [e, e.next_up(), e.next_down()]).collect()
    }

    /// The test inputs: 30 000 each over the range α⁰'s Planck-Einstein terms give it (θτ and −θτ, −60 to 60), over
    /// [−750, 750], near 0 (±0.05) and over every bit pattern; then each edge alone among seven ordinary lanes, and the
    /// edges side by side.
    fn inputs() -> Vec<f64> {
        let mut rng = Rng(16);
        let mut x: Vec<f64> = (0..30_000).map(|_| rng.uniform(-60.0, 60.0)).collect();
        x.extend((0..30_000).map(|_| rng.uniform(-750.0, 750.0)));
        x.extend((0..30_000).map(|_| rng.uniform(-0.05, 0.05)));
        x.extend((0..30_000).map(|_| f64::from_bits(rng.next())));
        for e in edges() {
            x.push(e);
            x.extend((0..7).map(|_| rng.uniform(-60.0, 60.0)));
        }
        x.extend(edges());
        x
    }

    /// `Expm1` over `x` in groups of W, against [`expm1`] at each point (NaN canonical, VERIFICATION.md §9.4).
    fn assert_expm1(x: &[f64], run: impl Fn(&[f64], &mut [u64]), what: &str) {
        let mut got = vec![0; x.len()];
        run(x, &mut got);
        for (x, got) in x.iter().zip(got) {
            let want = canonical(expm1(*x));
            let (g, w) = (f64::from_bits(got), f64::from_bits(want));
            assert!(got == want, "{what}: expm1({x:e}) = {g:e}, scalar {w:e}");
        }
    }

    /// PLAN.md M23.5 (VERIFICATION.md §9.2, class `Exact`): [`expm1_lanes`] is [`expm1`] bit for bit (NaN canonical),
    /// in every build, on 2, 4 and 8 lanes and with the `simd` feature at every dispatch level of this CPU, on random
    /// inputs over α⁰'s argument range, near 0 and over all of `f64`, and on the edges, alone and side by side.
    #[test]
    fn lane_expm1_is_scalar_bitwise() {
        let x = inputs();
        assert_expm1(&x, |x, out| run_lanes::<_, 2>(&Expm1, x, x, out), "2 lanes");
        assert_expm1(&x, |x, out| run_lanes::<_, 4>(&Expm1, x, x, out), "4 lanes");
        assert_expm1(&x, |x, out| run_lanes::<_, 8>(&Expm1, x, x, out), "8 lanes");
        #[cfg(feature = "simd")]
        for level in crate::exec::tests::levels() {
            assert_expm1(&x, |x, out| crate::exec::run_simd(level, &Expm1, x, x, out), &format!("{level:?}"));
        }
        assert!(x.len() > 120_000 && x.iter().any(|x| x.is_nan()));
    }

    /// Where the paths meet, at |x| = 2⁻⁵, Taylor's polynomial and the table agree to 1 ulp over 2000 x either side and
    /// of both signs, and at ±2⁻⁵ itself they are the same bits. The polynomial's last term counts there (x⁷/8! is
    /// about 3 ulp of x near 2⁻⁵), and M23.1's values reach that range at only a few points.
    #[test]
    fn the_paths_agree_where_they_meet() {
        let mut rng = Rng(17);
        for _ in 0..2000 {
            let x = rng.uniform(0.028, 0.035) * if rng.next().is_multiple_of(2) { 1.0 } else { -1.0 };
            let (a, b) = (near(x), by_table(x));
            let ulps = (a.to_bits() as i64).wrapping_sub(b.to_bits() as i64).unsigned_abs();
            assert!(ulps <= 1, "expm1({x:e}): Taylor {a:e}, table {b:e}, {ulps} ulp apart");
        }
        let edge = f64::from_bits(NEAR);
        assert_eq!([near(edge), near(-edge)].map(f64::to_bits), [by_table(edge), by_table(-edge)].map(f64::to_bits));
    }
}
