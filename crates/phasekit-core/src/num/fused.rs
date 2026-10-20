//! The fused forms of the `fast` feature (PLAN.md M23.7; user decision IM4), compiled only with it: Arm's FMA forms of
//! the in-house functions (optimized-routines' `HAVE_FAST_FMA` branches of `log.c` and `pow.c`, and `exp`'s
//! polynomial) and the jets' fused multiply-add. Each a·b + c here is [`math::mul_add`](super::math), one rounding.
//! `gates mutants` builds without `fast`, so this file is left out of it (`.cargo/mutants.toml`); CI's `fast` job runs
//! its tests.

use super::math::mul_add;

/// z·(1/c) − 1, rounded once: `log`'s and `pow`'s r with FMA, which needs neither `log`'s second table nor `pow`'s
/// split of z.
#[inline(always)]
pub(crate) fn ratio(z: f64, invc: f64) -> f64 {
    mul_add(z, invc, -1.0)
}

/// A0·r² as sq + lo: sq = r·(A0·r) and lo its rounding error, `pow`'s `ar2` and `lo3` with FMA.
#[inline(always)]
pub(crate) fn square(r: f64, a0: f64) -> (f64, f64) {
    let ar = a0 * r;
    let sq = r * ar;
    (sq, mul_add(ar, r, -sq))
}

/// y·(hi + lo) as ehi + elo: ehi = y·hi, and elo y·lo plus ehi's rounding error, `pow`'s with FMA.
#[inline(always)]
pub(crate) fn product(y: f64, hi: f64, lo: f64) -> (f64, f64) {
    let ehi = y * hi;
    (ehi, y * lo + mul_add(y, hi, -ehi))
}

/// `exp`'s tail + r + r²·(c2 + r·c3) + r⁴·(c4 + r·c5) in its order, each a·b + c fused.
#[inline(always)]
pub(crate) fn exp_poly(tail: f64, r: f64, [c2, c3, c4, c5]: [f64; 4]) -> f64 {
    let r2 = r * r;
    mul_add(r2 * r2, mul_add(r, c5, c4), mul_add(r2, mul_add(r, c3, c2), tail + r))
}

/// a·b + c on a jet's coefficients: each product of `triples` (slot k of the product from a's slot i and b's slot j,
/// the value part first) fused onto c's slot k in turn.
#[inline(always)]
pub(crate) fn jet_mad<const N: usize>(
    a: &[f64; N],
    b: &[f64; N],
    mut c: [f64; N],
    triples: &[(u8, u8, u8)],
) -> [f64; N] {
    for &(k, i, j) in triples {
        let factors = (a.get(usize::from(i)), b.get(usize::from(j)));
        if let ((Some(&ai), Some(&bj)), Some(ck)) = (factors, c.get_mut(usize::from(k))) {
            *ck = mul_add(ai, bj, *ck);
        }
    }
    c
}
