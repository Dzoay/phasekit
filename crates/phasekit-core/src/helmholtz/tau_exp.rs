//! Separable terms with an exponential on both sides, `n τ^t e^(−aτ^m) δ^d e^(−cδ^l)`: Lemmon2005 (Lemmon & Jacobsen
//! 2005; R125) and DoubleExponential (de Reuck & Craven 1993; Methanol, where a = g_t < 0). Each side's scaled
//! derivatives are exact polynomials of its exponential's argument ([`exp_poly`], E4), combined by an outer product
//! as for power terms; the τ-side costs one more `exp` per term, and only where a ≠ 0.

use super::power::{Poly, Vars, exp_poly, exp_series, horner};
use super::{DoubleExponentialTerm, Lemmon2005Term, MAX_POW};
use crate::derivs::Derivs;
use crate::error::{Error, LoadError};
use crate::num::{Real, math};

/// A structure-of-arrays block of τ-exponential terms, compiled once at decode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TauExpBlock {
    n: Box<[f64]>,
    t: Box<[f64]>,
    a: Box<[f64]>,
    m: Box<[f64]>,
    d: Box<[u8]>,
    l: Box<[u8]>,
    c: Box<[f64]>,
    bt: Box<[[Poly; 5]]>,
    bd: Box<[[Poly; 5]]>,
}

/// One term as `(n, t, a, m, d, c, l)`.
type Term = (f64, f64, f64, f64, u8, f64, u8);

impl TauExpBlock {
    /// Lemmon2005 terms (`l = 0` or `m = 0`: that exponential is absent) followed by DoubleExponential terms.
    /// Non-finite coefficients and exponents beyond [`MAX_POW`] are a `LoadError`.
    pub(crate) fn new(lemmon: &[Lemmon2005Term], double: &[DoubleExponentialTerm]) -> Result<Self, Error> {
        let present = |e: f64| if e == 0.0 { 0.0 } else { 1.0 };
        let terms: Vec<Term> = lemmon
            .iter()
            .map(|p| (p.n, p.t, present(p.m), p.m, p.d, present(f64::from(p.l)), p.l))
            .chain(double.iter().map(|p| (p.n, p.t, p.gt, p.lt, p.d, p.gd, p.ld)))
            .collect();
        for &(n, t, a, m, d, c, l) in &terms {
            if ![n, t, a, m, c].iter().all(|x| x.is_finite()) {
                return Err(Error::Load(LoadError::Format("non-finite τ-exponential term coefficient".into())));
            }
            if usize::from(d.max(l)) > MAX_POW {
                return Err(Error::Load(LoadError::Format("τ-exponential term exponent out of range".into())));
            }
        }
        let column = |f: fn(&Term) -> f64| terms.iter().map(f).collect();
        Ok(Self {
            n: column(|p| p.0),
            t: column(|p| p.1),
            a: column(|p| p.2),
            m: column(|p| p.3),
            d: terms.iter().map(|p| p.4).collect(),
            c: column(|p| p.5),
            l: terms.iter().map(|p| p.6).collect(),
            bt: terms.iter().map(|p| exp_poly(p.1, p.3)).collect(),
            bd: terms.iter().map(|p| exp_poly(f64::from(p.4), f64::from(p.6))).collect(),
        })
    }

    /// `A_ij += φ·B^τ_i(y)·B^δ_j(x)` for every term, `y = aτ^m`, `x = cδ^l`, `i + j ≤ ORD`.
    #[inline(always)]
    #[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
    pub(crate) fn accumulate<R: Real, const ORD: usize>(&self, v: &Vars<R>, acc: &mut Derivs<R>) {
        let pow = |e: u8| v.delta_pow.get(usize::from(e)).copied().unwrap_or_else(|| R::from_f64(f64::NAN));
        let terms = self.n.iter().zip(&self.t).zip(&self.a).zip(&self.m).zip(&self.d).zip(&self.l).zip(&self.c);
        for (((((((&n, &t), &a), &m), &d), &l), &c), (bt, bd)) in terms.zip(self.bt.iter().zip(&self.bd)) {
            let y = if a == 0.0 { R::from_f64(0.0) } else { (v.ln_tau * m).exp() * a };
            let x = pow(l) * c;
            let phi = (v.ln_tau * t - y - x).exp() * pow(d) * n;
            acc.add_separable::<ORD>(phi, &horner::<R, ORD>(bt, y), &horner::<R, ORD>(bd, x));
        }
    }

    /// Exact δ → 0 Taylor coefficients `[a_k(τ), τ·da_k/dτ, τ²·d²a_k/dτ²]`, k = 0, 1, 2, as for power terms; the
    /// τ-factor `f = n τ^t e^(−y)`, y = aτ^m, has `τ f' = f·s` with s = t − m y, and `τ² f'' = f·(s² − s − m² y)`.
    pub(crate) fn zero_density_series(&self, tau: f64) -> [[f64; 3]; 3] {
        let mut out = [[0.0; 3]; 3];
        let terms = self.n.iter().zip(&self.t).zip(&self.a).zip(&self.m).zip(&self.d).zip(&self.l).zip(&self.c);
        for ((((((&n, &t), &a), &m), &d), &l), &c) in terms {
            let y = if a == 0.0 { 0.0 } else { a * math::powf(tau, m) };
            let f = n * math::powf(tau, t) * math::exp(-y);
            let s = t - m * y;
            let (tf, ttf) = (f * s, f * (s * s - s - m * m * y));
            exp_series(&mut out, (d, l, c), |slot, coef| {
                slot[0] += coef * f;
                slot[1] += coef * tf;
                slot[2] += coef * ttf;
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derivs::Order;
    use crate::helmholtz::power::ad_misses;
    use crate::num::Jet4;

    /// The paper formulas, written independently of the block: Lemmon2005 with "0 means absent", DoubleExponential.
    fn paper<R: Real>(lemmon: &[Lemmon2005Term], double: &[DoubleExponentialTerm], tau: R, delta: R) -> R {
        let mut sum = R::from_f64(0.0);
        for p in lemmon {
            let mut e = R::from_f64(0.0);
            if p.l > 0 {
                e = e - delta.powi(i32::from(p.l));
            }
            if p.m > 0.0 {
                e = e - tau.powf(p.m);
            }
            sum = sum + tau.powf(p.t) * delta.powi(i32::from(p.d)) * e.exp() * p.n;
        }
        for p in double {
            let e = -(delta.powi(i32::from(p.ld)) * p.gd) - tau.powf(p.lt) * p.gt;
            sum = sum + tau.powf(p.t) * delta.powi(i32::from(p.d)) * e.exp() * p.n;
        }
        sum
    }

    fn toy() -> (Vec<Lemmon2005Term>, Vec<DoubleExponentialTerm>) {
        let lemmon = vec![
            Lemmon2005Term { n: 0.7, t: 1.25, d: 2, l: 0, m: 0.0 },
            Lemmon2005Term { n: -0.4, t: 2.5, d: 1, l: 2, m: 0.0 },
            Lemmon2005Term { n: 0.3, t: 4.5, d: 3, l: 3, m: 1.7 },
        ];
        let double = vec![DoubleExponentialTerm { n: 0.2, t: 0.5, d: 3, gd: 2.5, ld: 2, gt: -3.9, lt: 1.0 }];
        (lemmon, double)
    }

    /// The block against `Jet4` AD of the paper formulas, all 15 A_ij (tests/terms.rs checks real data against
    /// num-dual).
    #[test]
    fn tau_exponential_terms_match_jet4_ad() {
        let (lemmon, double) = toy();
        let block = TauExpBlock::new(&lemmon, &double).unwrap();
        let (tau, delta) = (1.3, 0.7);
        let mut acc = Derivs::zero(Order::Four);
        block.accumulate::<f64, 4>(&Vars::new(tau, delta), &mut acc);
        let ad = paper(&lemmon, &double, Jet4::tau(tau), Jet4::delta(delta)).derivs(tau, delta);
        let misses = ad_misses(&acc, &ad, 1e-13);
        assert!(misses.is_empty(), "{misses:?}");
    }

    /// δ → 0: the series agrees with the block's own values at δ = 1e-8 (A_00 ≈ a_1 δ + a_2 δ², and its τ-derivative;
    /// the omitted δ³ terms are 1e-14 relative there).
    #[test]
    fn tau_exponential_zero_density_series() {
        let (lemmon, double) = toy();
        let block = TauExpBlock::new(&lemmon, &double).unwrap();
        let (tau, delta) = (1.3, 1e-8);
        let [a0, a1, a2] = block.zero_density_series(tau);
        let mut acc = Derivs::zero(Order::Two);
        block.accumulate::<f64, 2>(&Vars::new(tau, delta), &mut acc);
        for (k, i) in [(0, 0), (1, 1), (2, 2)] {
            let series = a0[k] + a1[k] * delta + a2[k] * delta * delta;
            assert!(((acc.get(i, 0).unwrap() - series) / series).abs() < 1e-10, "A{i}0");
        }
    }

    /// Non-finite data and exponents past `MAX_POW` are refused at decode.
    #[test]
    fn bad_tau_exponential_terms_are_load_errors() {
        let (lemmon, double) = toy();
        let mut nan = double.clone();
        nan[0].gt = f64::NAN;
        assert!(TauExpBlock::new(&lemmon, &nan).is_err());
        let mut far = lemmon.clone();
        far[0].d = 17;
        assert!(TauExpBlock::new(&far, &double).is_err());
        assert!(TauExpBlock::new(&lemmon, &double).is_ok());
    }
}
