//! Gaussian bell-shaped terms `n τ^t δ^d e^(−η(δ − ε)² − β(τ − γ)²)` (433 default terms in 78 fluids; map 02 §3.1).
//! Each side is a power times a Gaussian, so its scaled derivatives follow from Leibniz's rule: the power gives the
//! falling factorials of its exponent, the Gaussian the complete Bell polynomials of `z u'` and `z² u''` with
//! `u = −η(z − ε)²` (Hermite polynomials in `w = z − ε`). Working in `w` keeps sharp Gaussians (β up to 1888)
//! accurate near their centre, where a polynomial expanded in z would cancel (PLAN.md M3.4).

use super::GaussianTerm;
use super::power::Vars;
use crate::derivs::Derivs;
use crate::error::{Error, LoadError};
use crate::num::{Real, math};

/// Binomial coefficients `C(j, m)`, j ≤ 4.
const BINOMIAL: [[f64; 5]; 5] = [
    [1.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 1.0, 0.0, 0.0, 0.0],
    [1.0, 2.0, 1.0, 0.0, 0.0],
    [1.0, 3.0, 3.0, 1.0, 0.0],
    [1.0, 4.0, 6.0, 4.0, 1.0],
];

/// `[1, p, p(p−1), p(p−1)(p−2), p(p−1)(p−2)(p−3)]`: the scaled derivatives `z^k ∂^k z^p / z^p`.
pub(super) fn falling(p: f64) -> [f64; 5] {
    [1.0, p, p * (p - 1.0), p * (p - 1.0) * (p - 2.0), p * (p - 1.0) * (p - 2.0) * (p - 3.0)]
}

/// The scaled derivatives `z^j ∂^j (z^p e^u) / (z^p e^u)`, j ≤ ORD, from the falling factorials of p and
/// `h_k = z^k u⁽ᵏ⁾`: Leibniz's rule over the complete Bell polynomials `B_m(h_1, …, h_m) = z^m ∂^m e^u / e^u`.
#[inline(always)]
pub(super) fn leibniz<R: Real, const ORD: usize>(falling: &[f64; 5], h: [R; 4]) -> [R; 5] {
    let [h1, h2, h3, h4] = h;
    let h11 = h1 * h1;
    let one = R::from_f64(1.0);
    let bell = [
        one,
        h1,
        h11 + h2,
        h11 * h1 + h1 * h2 * 3.0 + h3,
        h11 * h11 + h11 * h2 * 6.0 + h2 * h2 * 3.0 + h1 * h3 * 4.0 + h4,
    ];
    let mut b = [one; 5];
    for (j, slot) in b.iter_mut().enumerate().take(ORD + 1).skip(1) {
        let mut sum = R::from_f64(falling[j]);
        for m in 1..=j {
            sum = sum + bell[m] * (BINOMIAL[j][m] * falling[j - m]);
        }
        *slot = sum;
    }
    b
}

/// `[z u', z² u'', 0, 0]` for `u = −η(z − ε)²`, and u itself.
#[inline(always)]
pub(super) fn gaussian_side<R: Real>(z: R, eta: f64, epsilon: f64) -> (R, [R; 4]) {
    let w = z + -epsilon;
    let zero = R::from_f64(0.0);
    (w * w * -eta, [w * z * (-2.0 * eta), z * z * (-2.0 * eta), zero, zero])
}

/// A structure-of-arrays block of Gaussian terms, compiled once at decode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GaussianBlock {
    n: Box<[f64]>,
    t: Box<[f64]>,
    d: Box<[u8]>,
    eta: Box<[f64]>,
    epsilon: Box<[f64]>,
    beta: Box<[f64]>,
    gamma: Box<[f64]>,
    ft: Box<[[f64; 5]]>,
    fd: Box<[[f64; 5]]>,
}

impl GaussianBlock {
    /// Validates and compiles terms: non-finite data and `d` beyond [`super::MAX_POW`] are a `LoadError`.
    pub(crate) fn new(terms: &[GaussianTerm]) -> Result<Self, Error> {
        for p in terms {
            if ![p.n, p.t, p.eta, p.epsilon, p.beta, p.gamma].iter().all(|x| x.is_finite()) {
                return Err(Error::Load(LoadError::Format("non-finite Gaussian term coefficient".into())));
            }
            if usize::from(p.d) > super::MAX_POW {
                return Err(Error::Load(LoadError::Format("Gaussian term exponent out of range".into())));
            }
        }
        let column = |f: fn(&GaussianTerm) -> f64| terms.iter().map(f).collect();
        Ok(Self {
            n: column(|p| p.n),
            t: column(|p| p.t),
            d: terms.iter().map(|p| p.d).collect(),
            eta: column(|p| p.eta),
            epsilon: column(|p| p.epsilon),
            beta: column(|p| p.beta),
            gamma: column(|p| p.gamma),
            ft: terms.iter().map(|p| falling(p.t)).collect(),
            fd: terms.iter().map(|p| falling(f64::from(p.d))).collect(),
        })
    }

    /// `A_ij += φ·B^τ_i·B^δ_j` for every term, `i + j ≤ ORD`.
    pub(crate) fn accumulate<R: Real, const ORD: usize>(&self, v: &Vars<R>, acc: &mut Derivs<R>) {
        let delta = v.delta_pow[1];
        for k in 0..self.n.len() {
            let (ut, ht) = gaussian_side(v.tau, self.beta[k], self.gamma[k]);
            let (ud, hd) = gaussian_side(delta, self.eta[k], self.epsilon[k]);
            let phi = (v.ln_tau * self.t[k] + ut + ud).exp() * v.delta_pow[usize::from(self.d[k])] * self.n[k];
            acc.add_separable::<ORD>(phi, &leibniz::<R, ORD>(&self.ft[k], ht), &leibniz::<R, ORD>(&self.fd[k], hd));
        }
    }

    /// Exact δ → 0 Taylor coefficients `[a_k(τ), τ·da_k/dτ]`, k = 0, 1, 2: `δ^d e^(−η(δ−ε)²) = e^(−ηε²) δ^d (1 + 2ηε δ +
    /// (2η²ε² − η) δ² + …)`; the τ-factor `f = n τ^t e^(−β(τ−γ)²)` has `τ f' = f·(t − 2βτ(τ − γ))`.
    pub(crate) fn zero_density_series(&self, tau: f64) -> [[f64; 2]; 3] {
        let mut out = [[0.0; 2]; 3];
        for k in 0..self.n.len() {
            let (eta, eps, beta, gamma) = (self.eta[k], self.epsilon[k], self.beta[k], self.gamma[k]);
            let w = tau - gamma;
            let f = self.n[k] * math::exp(self.t[k] * math::ln(tau) - beta * w * w - eta * eps * eps);
            let tf = f * (self.t[k] - 2.0 * beta * tau * w);
            let series = [1.0, 2.0 * eta * eps, 2.0 * eta * eta * eps * eps - eta];
            for (q, c) in series.into_iter().enumerate() {
                if let Some(slot) = out.get_mut(usize::from(self.d[k]) + q) {
                    slot[0] += c * f;
                    slot[1] += c * tf;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derivs::Order;
    use crate::num::Jet4;

    /// The paper formula, written independently of the block.
    fn paper<R: Real>(terms: &[GaussianTerm], tau: R, delta: R) -> R {
        let mut sum = R::from_f64(0.0);
        for p in terms {
            let (wt, wd) = (tau + -p.gamma, delta + -p.epsilon);
            let e = -(wd * wd * p.eta) - wt * wt * p.beta;
            sum = sum + tau.powf(p.t) * delta.powi(i32::from(p.d)) * e.exp() * p.n;
        }
        sum
    }

    /// IAPWS-95's three Gaussian terms (Wagner & Pruß 2002, Table 2: i = 52-54).
    fn water() -> Vec<GaussianTerm> {
        let term = |n, t, beta, gamma| GaussianTerm { n, t, d: 3, eta: 20.0, epsilon: 1.0, beta, gamma };
        vec![
            term(-31.306260323435, 0.0, 150.0, 1.21),
            term(31.546140237781, 1.0, 150.0, 1.21),
            term(-2521.3154341695, 4.0, 250.0, 1.25),
        ]
    }

    /// The block against `Jet4` AD of the paper formula, all 15 A_ij, near the Gaussians' centre and away from it.
    #[test]
    fn gaussian_terms_match_jet4_ad() {
        let block = GaussianBlock::new(&water()).unwrap();
        for (tau, delta) in [(1.22, 1.05), (1.6, 0.3), (0.9, 2.1)] {
            let mut acc = Derivs::zero(Order::Four);
            block.accumulate::<f64, 4>(&Vars::new(tau, delta), &mut acc);
            let ad = paper(&water(), Jet4::tau(tau), Jet4::delta(delta)).derivs(tau, delta);
            for n in 0..=4 {
                for i in 0..=n {
                    let (got, want) = (acc.get(i, n - i).unwrap(), ad.get(i, n - i).unwrap());
                    assert!(((got - want) / want).abs() < 1e-11, "({tau}, {delta}) A{i}{}: {got} vs {want}", n - i);
                }
            }
        }
    }

    /// δ → 0: the series agrees with the block's own values at δ = 1e-8 (d = 3: only a_3 onwards, so a_0..a_2 are 0);
    /// with d = 0 (Methane's terms) the series carries α^r itself.
    #[test]
    fn gaussian_zero_density_series() {
        assert_eq!(GaussianBlock::new(&water()).unwrap().zero_density_series(1.3), [[0.0; 2]; 3]);
        let d0 = [GaussianTerm { n: 0.7, t: 1.5, d: 0, eta: 3.0, epsilon: 0.5, beta: 2.0, gamma: 1.1 }];
        let block = GaussianBlock::new(&d0).unwrap();
        let (tau, delta) = (1.3, 1e-8);
        let [a0, a1, a2] = block.zero_density_series(tau);
        let mut acc = Derivs::zero(Order::One);
        block.accumulate::<f64, 1>(&Vars::new(tau, delta), &mut acc);
        let at = |k: usize| a0[k] + a1[k] * delta + a2[k] * delta * delta;
        assert!(((acc.get(0, 0).unwrap() - at(0)) / at(0)).abs() < 1e-14);
        assert!(((acc.get(1, 0).unwrap() - at(1)) / at(1)).abs() < 1e-14);
    }

    /// Leibniz by hand: with p = 0 and every h_k = 1 the factors are the Bell numbers 1, 1, 2, 5, 15; with h = 0 they
    /// are the falling factorials of p; p = 1, h = (1, 1, 1, 1): B_j + j·B_(j−1), i.e. 1, 2, 4, 11, 35.
    #[test]
    fn leibniz_by_hand() {
        let ones = [1.0; 4];
        assert_eq!(leibniz::<f64, 4>(&falling(0.0), ones), [1.0, 1.0, 2.0, 5.0, 15.0]);
        assert_eq!(leibniz::<f64, 4>(&falling(2.5), [0.0; 4]), falling(2.5));
        assert_eq!(leibniz::<f64, 4>(&falling(1.0), ones), [1.0, 2.0, 4.0, 11.0, 35.0]);
        assert_eq!(leibniz::<f64, 2>(&falling(0.0), ones), [1.0, 1.0, 2.0, 1.0, 1.0]);
    }

    /// The δ → 0 coefficients by hand for d = 0: `f·[1, 2ηε, 2η²ε² − η]` and `τ f' = f·(t − 2βτ(τ − γ))`.
    #[test]
    fn gaussian_zero_density_coefficients_by_hand() {
        let term = GaussianTerm { n: 0.7, t: 1.5, d: 0, eta: 3.0, epsilon: 0.4, beta: 2.0, gamma: 1.1 };
        let tau = 1.3;
        let f = 0.7 * math::exp(1.5 * math::ln(tau) - 2.0 * 0.2 * 0.2 - 3.0 * 0.16);
        let tf = f * (1.5 - 2.0 * 2.0 * tau * 0.2);
        let got = GaussianBlock::new(&[term]).unwrap().zero_density_series(tau);
        // 2ηε = 2.4, 2η²ε² − η = 2.88 − 3 = −0.12.
        let want = [[f, tf], [2.4 * f, 2.4 * tf], [-0.12 * f, -0.12 * tf]];
        for (g, w) in got.iter().flatten().zip(want.iter().flatten()) {
            assert!(((g - w) / w).abs() < 1e-14, "{got:?} vs {want:?}");
        }
    }

    /// Non-finite data and exponents past `MAX_POW` are refused at decode.
    #[test]
    fn bad_gaussian_terms_are_load_errors() {
        let mut terms = water();
        assert!(GaussianBlock::new(&terms).is_ok());
        terms[1].beta = f64::INFINITY;
        assert!(GaussianBlock::new(&terms).is_err());
        let mut terms = water();
        terms[0].d = 17;
        assert!(GaussianBlock::new(&terms).is_err());
    }
}
