//! The power-law term family, fully implemented: how derivatives are obtained (D2).
//!
//! A separable term is `n·f(τ)·g(δ)` with `f = τ^t` and `g = δ^d·e^(−x)`, `x = c·δ^l`. Its scaled
//! derivatives `A_ij = τ^i δ^j ∂^(i+j)α/∂τ^i∂δ^j` are `α · B^τ_i · B^δ_j`:
//! - `B^τ_i` are the falling factorials of `t`, precomputed at decode (`bt`);
//! - `B^δ_j` are EXACT polynomials in `x` whose integer coefficients depend only on `(d, l)`, precomputed at
//!   decode (`bd`). Evaluating them by Horner has no cancellation as δ → 0, unlike the log-jet recurrence it
//!   replaces (E4: `p₂ + p₁² − p₁` lost 2.2e-5 relative at δ = 1e-12 for d = 1, l = 1).
//!
//! One `exp` per term, about 10 multiply-adds for the δ-factors, no per-term derivative formula.

use crate::derivs::Derivs;
use crate::error::{Error, LoadError};
use crate::num::Real;

/// Largest integer exponent `d` or `l` of a power term (v8.0.0 data: d ≤ 15, l ≤ 6; datagen rejects more).
pub const MAX_POW: usize = 16;

/// Per-state variables shared by every block of one evaluation: one `ln` and a δ^k table by
/// multiplication. δ^d comes from the table, not from `exp(d·ln δ)`, so the 57 MBWR terms with d = 0
/// (map 02 §3) stay finite at δ = 0 (kernel-performance §4).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Vars<R> {
    pub(super) tau: R,
    pub(super) ln_tau: R,
    pub(super) delta_pow: [R; MAX_POW + 1],
}

impl<R: Real> Vars<R> {
    /// Precomputes the per-state table.
    pub(crate) fn new(tau: R, delta: R) -> Self {
        let mut delta_pow = [R::from_f64(1.0); MAX_POW + 1];
        for k in 1..=MAX_POW {
            delta_pow[k] = delta_pow[k - 1] * delta;
        }
        Self { tau, ln_tau: tau.ln(), delta_pow }
    }
}

/// One term `n τ^t δ^d exp(−c δ^l)` as printed in the paper. `l = 0` requires `c = 0` (map 02: "0 means
/// absent"); CoolProp's Power and Exponential kinds both normalise to this.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
#[allow(missing_docs)] // the symbols of the formula above
pub struct PowerTerm {
    pub n: f64,
    pub t: f64,
    pub d: u8,
    pub l: u8,
    pub c: f64,
}

impl PowerTerm {
    /// A term as printed.
    pub const fn new(n: f64, t: f64, d: u8, l: u8, c: f64) -> Self {
        Self { n, t, d, l, c }
    }
}

/// Polynomial in x truncated at degree 4 (enough for order-4 δ-factors).
pub(super) type Poly = [f64; 5];

fn poly_mul(a: Poly, b: Poly) -> Poly {
    let mut r = [0.0; 5];
    for (i, ai) in a.iter().enumerate() {
        for (j, bj) in b.iter().enumerate().take(5 - i) {
            r[i + j] += ai * bj;
        }
    }
    r
}

fn poly_sum(terms: &[(f64, Poly)]) -> Poly {
    let mut r = [0.0; 5];
    for (k, p) in terms {
        for (ri, pi) in r.iter_mut().zip(p) {
            *ri += k * pi;
        }
    }
    r
}

/// Coefficients `C[j][k]` of `B_j(x) = Σ_k C[j][k]·x^k`, the scaled derivatives `z^j g^(j)/g` of `g = z^p e^(−x)`,
/// `x = c·z^q`, as polynomials in x. With `u = ln z`: `D_u ln g = p − q·x` and `D_u^k ln g = −q^k·x` (k ≥ 2);
/// complete Bell polynomials give `D_u^k g / g`, Stirling numbers of the first kind turn them into `z^k g^(k) / g`.
/// For the δ-side of a power term (p = d ≤ 15, q = l ≤ 6) every coefficient is an integer below 2^53, so exact in
/// f64; the τ-side of Lemmon2005 and DoubleExponential terms has real p = t and q = m.
pub(super) fn exp_poly(p: f64, q: f64) -> [Poly; 5] {
    let p1 = [p, -q, 0.0, 0.0, 0.0];
    let pk = |k: i32| [0.0, -crate::num::math::powi(q, k), 0.0, 0.0, 0.0];
    let (p2, p3, p4) = (pk(2), pk(3), pk(4));
    let p11 = poly_mul(p1, p1);
    let y1 = p1;
    let y2 = poly_sum(&[(1.0, p2), (1.0, p11)]);
    let y3 = poly_sum(&[(1.0, p3), (3.0, poly_mul(p2, p1)), (1.0, poly_mul(p11, p1))]);
    let y4 = poly_sum(&[
        (1.0, p4),
        (4.0, poly_mul(p3, p1)),
        (3.0, poly_mul(p2, p2)),
        (6.0, poly_mul(p2, p11)),
        (1.0, poly_mul(p11, p11)),
    ]);
    [
        [1.0, 0.0, 0.0, 0.0, 0.0],
        y1,
        poly_sum(&[(1.0, y2), (-1.0, y1)]),
        poly_sum(&[(1.0, y3), (-3.0, y2), (2.0, y1)]),
        poly_sum(&[(1.0, y4), (-6.0, y3), (11.0, y2), (-6.0, y1)]),
    ]
}

/// `[1, B_1(x), …, B_ORD(x)]` from the polynomials of [`exp_poly`], by Horner: exact coefficients, no cancellation
/// as x → 0.
#[inline(always)]
pub(super) fn horner<R: Real, const ORD: usize>(polys: &[Poly; 5], x: R) -> [R; 5] {
    let mut b = [R::from_f64(1.0); 5];
    for (j, (b, poly)) in b.iter_mut().zip(polys).enumerate().take(ORD + 1).skip(1) {
        *b = poly[..j].iter().rev().fold(R::from_f64(poly[j]), |acc, &ck| acc * x + ck);
    }
    b
}

/// A structure-of-arrays block of power terms, compiled once at decode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PowerBlock {
    n: Box<[f64]>,
    t: Box<[f64]>,
    d: Box<[u8]>,
    l: Box<[u8]>,
    c: Box<[f64]>,
    bt: Box<[[f64; 5]]>,
    bd: Box<[[Poly; 5]]>,
}

impl PowerBlock {
    /// Validates and compiles terms (unknown or out-of-range data is a `LoadError`, never skipped).
    pub(crate) fn new(terms: &[PowerTerm]) -> Result<Self, Error> {
        let bad = |m: &str| Error::Load(LoadError::Format(m.into()));
        for p in terms {
            if !(p.n.is_finite() && p.t.is_finite() && p.c.is_finite()) {
                return Err(bad("non-finite power-term coefficient"));
            }
            if usize::from(p.d.max(p.l)) > MAX_POW || (p.l == 0 && p.c != 0.0) {
                return Err(bad("power-term exponent out of range"));
            }
        }
        let falling =
            |t: f64| [1.0, t, t * (t - 1.0), t * (t - 1.0) * (t - 2.0), t * (t - 1.0) * (t - 2.0) * (t - 3.0)];
        Ok(Self {
            n: terms.iter().map(|p| p.n).collect(),
            t: terms.iter().map(|p| p.t).collect(),
            d: terms.iter().map(|p| p.d).collect(),
            l: terms.iter().map(|p| p.l).collect(),
            c: terms.iter().map(|p| p.c).collect(),
            bt: terms.iter().map(|p| falling(p.t)).collect(),
            bd: terms.iter().map(|p| exp_poly(f64::from(p.d), f64::from(p.l))).collect(),
        })
    }

    /// The contribution of term `k` at one state: `φ = n τ^t δ^d e^(−cδ^l)` and its δ-side factors.
    #[inline(always)]
    fn term<R: Real, const ORD: usize>(&self, k: usize, v: &Vars<R>) -> (R, [R; 5]) {
        let x = v.delta_pow[usize::from(self.l[k])] * self.c[k]; // c δ^l (0 for polynomial terms)
        let phi = (v.ln_tau * self.t[k] - x).exp() * v.delta_pow[usize::from(self.d[k])] * self.n[k];
        (phi, horner::<R, ORD>(&self.bd[k], x))
    }

    /// Hot path for one state: one `exp` per term, then `A_ij += φ·B^τ_i·B^δ_j` for `i + j ≤ ORD`.
    /// Generic over `R`, so `Jet4` (and lanes, if the SIMD gate fires) run this same code.
    pub(crate) fn accumulate<R: Real, const ORD: usize>(&self, v: &Vars<R>, acc: &mut Derivs<R>) {
        for k in 0..self.n.len() {
            let (phi, bd) = self.term::<R, ORD>(k, v);
            acc.add_outer::<ORD>(phi, &self.bt[k], &bd);
        }
    }

    /// Exact δ → 0 Taylor coefficients `[a_k(τ), τ·da_k/dτ]` of this block's α^r for k = 0, 1, 2, from
    /// `δ^d e^(−cδ^l) = Σ_m (−c)^m δ^(d+ml) / m!` (E4).
    pub(crate) fn zero_density_series(&self, tau: f64) -> [[f64; 2]; 3] {
        let mut a = [[0.0; 2]; 3];
        for k in 0..self.n.len() {
            let f = self.n[k] * crate::num::math::powf(tau, self.t[k]);
            let (d, l) = (usize::from(self.d[k]), usize::from(self.l[k]));
            let mut coef = 1.0; // (−c)^m / m!
            for m in 0..3u8 {
                if m > 0 && l == 0 {
                    break;
                }
                if let Some(slot) = a.get_mut(d + usize::from(m) * l) {
                    slot[0] += coef * f;
                    slot[1] += coef * f * self.t[k];
                }
                coef *= -self.c[k] / f64::from(m + 1);
            }
        }
        a
    }

    /// The paper formula (test oracle): with `R = Jet4` it yields exact derivatives to check the jets.
    #[cfg(test)]
    pub(crate) fn value<R: Real>(&self, tau: R, delta: R) -> R {
        let mut sum = R::from_f64(0.0);
        for k in 0..self.n.len() {
            let damping = (-(delta.powi(i32::from(self.l[k])) * self.c[k])).exp();
            sum = sum + tau.powf(self.t[k]) * delta.powi(i32::from(self.d[k])) * damping * self.n[k];
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derivs::Order;
    use crate::num::Jet4;
    use crate::num::math;

    fn rel(a: f64, b: f64) -> f64 {
        ((a - b) / b).abs()
    }

    /// n = 0.5, t = 1.5, d = 2, l = 1, c = 1 at τ = 2, δ = 0.5. By hand: α = 0.5·2^1.5·0.25·e^(−0.5);
    /// τ-factors are the falling factorials of 1.5; δ-factors from g = δ²e^(−δ): [1, 1.5, 0.25, −1.625, 2.0625].
    #[test]
    fn power_term_matches_hand_derivation() {
        let block = PowerBlock::new(&[PowerTerm::new(0.5, 1.5, 2, 1, 1.0)]).unwrap();
        let mut acc = Derivs::zero(Order::Four);
        block.accumulate::<f64, 4>(&Vars::new(2.0, 0.5), &mut acc);
        let alpha = 0.214_440_971_240_176_72;
        let bt = [1.0, 1.5, 0.75, -0.375, 0.5625];
        let bd = [1.0, 1.5, 0.25, -1.625, 2.0625];
        for n in 0..=4 {
            for i in 0..=n {
                let want = alpha * bt[i] * bd[n - i];
                assert!(rel(acc.get(i, n - i).unwrap(), want) < 1e-15, "A{i}{}", n - i);
            }
        }
    }

    /// g = δe^(−δ) at δ = 1: A00 = 1/e, A01 = 0, A02 = −1/e, A03 = 2/e, A04 = −3/e.
    #[test]
    fn delta_exp_delta_closed_form() {
        let block = PowerBlock::new(&[PowerTerm::new(1.0, 0.0, 1, 1, 1.0)]).unwrap();
        let mut acc = Derivs::zero(Order::Four);
        block.accumulate::<f64, 4>(&Vars::new(1.3, 1.0), &mut acc);
        let e = math::exp(-1.0);
        for (j, want) in [e, 0.0, -e, 2.0 * e, -3.0 * e].into_iter().enumerate() {
            assert!((acc.get(0, j).unwrap() - want).abs() < 1e-15, "A0{j}");
        }
    }

    /// E4: near δ = 0 the δ-factors keep full relative accuracy. For g = δ e^(−δ^l) (d = 1, c = 1) by hand:
    /// l = 1: B₂ = −2x + x² with x = δ; l = 2: B₂ = −6x + 4x² with x = δ².
    #[test]
    fn delta_factors_are_cancellation_free_near_zero_density() {
        for delta in [1e-4, 1e-8, 1e-12] {
            for (l, x) in [(1u8, delta), (2, delta * delta)] {
                let b2 = if l == 1 { -2.0 * x + x * x } else { -6.0 * x + 4.0 * x * x };
                let block = PowerBlock::new(&[PowerTerm::new(1.0, 0.0, 1, l, 1.0)]).unwrap();
                let mut acc = Derivs::zero(Order::Two);
                block.accumulate::<f64, 2>(&Vars::new(1.0, delta), &mut acc);
                let got = acc.get(0, 2).unwrap() / acc.get(0, 0).unwrap();
                assert!(rel(got, b2) < 1e-15, "δ = {delta}, l = {l}: {got} vs {b2}");
            }
        }
    }

    /// The jets agree with `Jet4` AD of the paper formula on all 15 A_ij (tests/terms.rs checks both against
    /// num-dual on real data).
    #[test]
    fn jets_match_jet4_ad() {
        let block = PowerBlock::new(&[
            PowerTerm::new(0.5, 1.5, 2, 1, 1.0),
            PowerTerm::new(-0.7, 0.25, 1, 0, 0.0),
            PowerTerm::new(0.3, 3.0, 4, 2, 1.0),
        ])
        .unwrap();
        let (tau, delta) = (1.7, 0.8);
        let mut acc = Derivs::zero(Order::Four);
        block.accumulate::<f64, 4>(&Vars::new(tau, delta), &mut acc);
        let ad = block.value(Jet4::tau(tau), Jet4::delta(delta)).derivs(tau, delta);
        for n in 0..=4 {
            for i in 0..=n {
                assert!(rel(acc.get(i, n - i).unwrap(), ad.get(i, n - i).unwrap()) < 1e-14, "A{i}{}", n - i);
            }
        }
    }

    /// The fast path is generic: run on `Jet4`, its value parts are bitwise those of `f64`.
    #[test]
    fn generic_fast_path_is_bitwise_scalar() {
        let block = PowerBlock::new(&[PowerTerm::new(0.5, 1.5, 2, 1, 1.0)]).unwrap();
        let mut a = Derivs::zero(Order::Four);
        let mut h = Derivs::<Jet4>::zero(Order::Four);
        block.accumulate::<f64, 4>(&Vars::new(2.0, 0.5), &mut a);
        block.accumulate::<Jet4, 4>(&Vars::new(Jet4::tau(2.0), Jet4::delta(0.5)), &mut h);
        for n in 0..=4 {
            for i in 0..=n {
                assert_eq!(a.get(i, n - i).unwrap().to_bits(), h.get(i, n - i).unwrap().value().to_bits());
            }
        }
    }

    /// An MBWR pair n τ^t (1 − e^(−δ²)) stored as two d = 0 terms: finite, and exactly zero, at δ = 0.
    #[test]
    fn finite_at_zero_density() {
        let block =
            PowerBlock::new(&[PowerTerm::new(1.37, 3.0, 0, 0, 0.0), PowerTerm::new(-1.37, 3.0, 0, 2, 1.0)]).unwrap();
        let mut acc = Derivs::zero(Order::Four);
        block.accumulate::<f64, 4>(&Vars::new(1.2, 0.0), &mut acc);
        for n in 0..=4 {
            for i in 0..=n {
                assert_eq!(acc.get(i, n - i), Some(0.0));
            }
        }
    }

    /// E4: δ-series by hand. n τ^t δ e^(−δ) = n τ^t (δ − δ² + …); the MBWR pair n τ^t (1 − e^(−δ)) with
    /// d = 0, l = 1 gives n τ^t (δ − δ²/2 + …).
    #[test]
    fn zero_density_series_is_exact() {
        let (n, t, tau) = (0.7, 1.5, 1.3);
        let f = n * math::powf(tau, t);
        let block = PowerBlock::new(&[PowerTerm::new(n, t, 1, 1, 1.0)]).unwrap();
        assert_eq!(block.zero_density_series(tau), [[0.0, 0.0], [f, f * t], [-f, -f * t]]);
        let mbwr = PowerBlock::new(&[PowerTerm::new(n, t, 0, 0, 0.0), PowerTerm::new(-n, t, 0, 1, 1.0)]).unwrap();
        let s = mbwr.zero_density_series(tau);
        assert_eq!((s[0][0], s[1][0], s[2][0]), (0.0, f, -0.5 * f));
    }
}
