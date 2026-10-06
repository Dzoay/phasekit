//! GaoB terms `n τ^t δ^d e^(−η(δ − ε)² + 1/(β(τ − γ)² + b))` (Gao et al. 2020; Ammonia, the only fluid, 2 terms).
//! `η` has the paper's sign (datagen flips CoolProp's stored −η; map 02 §3.1), so the δ-side is a Gaussian side. The
//! τ-side is `τ^t e^v` with `v = 1/q`, `q = β(τ − γ)² + b > 0` (b > 0 in the data: no pole); its derivatives come
//! in closed form from `q' = 2β(τ − γ)`, `q'' = 2β`, `q''' = 0`.

use super::gaussian::{delta_series, falling, gaussian_side, leibniz};
use super::power::Vars;
use super::{GaoBTerm, MAX_POW};
use crate::derivs::Derivs;
use crate::error::{Error, LoadError};
use crate::num::{Real, math};

/// `(v, [τ v', τ² v'', τ³ v''', τ⁴ v''''])` for `v = 1/(β(τ − γ)² + b)`: with `r = 1/q`, `v' = −q' r²`,
/// `v'' = (2q'² r − q'') r²`, `v''' = 6q'(q'' − q'² r) r³`, `v'''' = (6q''² − 36q'² q'' r + 24q'⁴ r²) r³`.
#[inline(always)]
fn gao_b_tau_side<R: Real>(tau: R, beta: f64, gamma: f64, b: f64) -> (R, [R; 4]) {
    let w = tau + -gamma;
    let r = R::from_f64(1.0) / (w * w * beta + b);
    let (q1, q2) = (w * (2.0 * beta), 2.0 * beta);
    let (r2, q11) = (r * r, q1 * q1);
    let r3 = r2 * r;
    let v1 = -(q1 * r2);
    let v2 = (q11 * r * 2.0 + -q2) * r2;
    let v3 = q1 * (q11 * r * -6.0 + 6.0 * q2) * r3;
    let v4 = (q11 * q11 * r2 * 24.0 - q11 * r * (36.0 * q2) + 6.0 * q2 * q2) * r3;
    let tau2 = tau * tau;
    (r, [tau * v1, tau2 * v2, tau2 * tau * v3, tau2 * tau2 * v4])
}

/// A structure-of-arrays block of GaoB terms, compiled once at decode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GaoBBlock {
    n: Box<[f64]>,
    t: Box<[f64]>,
    d: Box<[u8]>,
    eta: Box<[f64]>,
    epsilon: Box<[f64]>,
    beta: Box<[f64]>,
    gamma: Box<[f64]>,
    b: Box<[f64]>,
    ft: Box<[[f64; 5]]>,
    fd: Box<[[f64; 5]]>,
}

impl GaoBBlock {
    /// Validates and compiles terms: non-finite data, `d` beyond [`MAX_POW`] and a pole (`b ≤ 0` or `β < 0`, which
    /// would let `β(τ − γ)² + b` reach 0) are a `LoadError`.
    pub(crate) fn new(terms: &[GaoBTerm]) -> Result<Self, Error> {
        let bad = |m: &str| Error::Load(LoadError::Format(m.into()));
        for p in terms {
            if ![p.n, p.t, p.eta, p.epsilon, p.beta, p.gamma, p.b].iter().all(|x| x.is_finite()) {
                return Err(bad("non-finite GaoB term coefficient"));
            }
            if usize::from(p.d) > MAX_POW {
                return Err(bad("GaoB term exponent out of range"));
            }
            if !(p.b > 0.0 && p.beta >= 0.0) {
                return Err(bad("GaoB term with a pole: needs b > 0 and β ≥ 0"));
            }
        }
        let column = |f: fn(&GaoBTerm) -> f64| terms.iter().map(f).collect();
        Ok(Self {
            n: column(|p| p.n),
            t: column(|p| p.t),
            d: terms.iter().map(|p| p.d).collect(),
            eta: column(|p| p.eta),
            epsilon: column(|p| p.epsilon),
            beta: column(|p| p.beta),
            gamma: column(|p| p.gamma),
            b: column(|p| p.b),
            ft: terms.iter().map(|p| falling(p.t)).collect(),
            fd: terms.iter().map(|p| falling(f64::from(p.d))).collect(),
        })
    }

    /// `A_ij += φ·B^τ_i·B^δ_j` for every term, `i + j ≤ ORD`.
    pub(crate) fn accumulate<R: Real, const ORD: usize>(&self, v: &Vars<R>, acc: &mut Derivs<R>) {
        let delta = v.delta_pow[1];
        for k in 0..self.n.len() {
            let (vt, ht) = gao_b_tau_side(v.tau, self.beta[k], self.gamma[k], self.b[k]);
            let (ud, hd) = gaussian_side(delta, self.eta[k], self.epsilon[k]);
            let phi = (v.ln_tau * self.t[k] + vt + ud).exp() * v.delta_pow[usize::from(self.d[k])] * self.n[k];
            acc.add_separable::<ORD>(phi, &leibniz::<R, ORD>(&self.ft[k], ht), &leibniz::<R, ORD>(&self.fd[k], hd));
        }
    }

    /// Exact δ → 0 Taylor coefficients as for Gaussian terms; the τ-factor `f = n τ^t e^v` has `τ f' = f·(t + τ v')`.
    pub(crate) fn zero_density_series(&self, tau: f64) -> [[f64; 2]; 3] {
        let mut out = [[0.0; 2]; 3];
        for k in 0..self.n.len() {
            let (eta, eps) = (self.eta[k], self.epsilon[k]);
            let (v, h) = gao_b_tau_side(tau, self.beta[k], self.gamma[k], self.b[k]);
            let f = self.n[k] * math::exp(self.t[k] * math::ln(tau) + v - eta * eps * eps);
            let tf = f * (self.t[k] + h[0]);
            for (q, c) in delta_series(eta, eps).into_iter().enumerate() {
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

    /// The paper formula (Gao et al. 2020), written independently of the block.
    fn paper<R: Real>(terms: &[GaoBTerm], tau: R, delta: R) -> R {
        let mut sum = R::from_f64(0.0);
        for p in terms {
            let (wt, wd) = (tau + -p.gamma, delta + -p.epsilon);
            let e = R::from_f64(1.0) / (wt * wt * p.beta + p.b) - wd * wd * p.eta;
            sum = sum + tau.powf(p.t) * delta.powi(i32::from(p.d)) * e.exp() * p.n;
        }
        sum
    }

    /// Ammonia's two GaoB terms (Gao et al. 2020) with η in the paper's sign.
    fn ammonia() -> Vec<GaoBTerm> {
        let term = |n, t, eta, epsilon, beta, gamma, b| GaoBTerm { n, t, d: 1, eta, epsilon, beta, gamma, b };
        vec![
            term(-1.6909858, 4.3315, 2.8452, 0.4478, 0.3696, 1.108, 1.244),
            term(0.93739074, 4.015, 2.8342, 0.44689, 0.2962, 1.313, 0.6826),
        ]
    }

    /// The block against `Jet4` AD of the paper formula, all 15 A_ij, on both sides of the τ-centres.
    #[test]
    fn gao_b_terms_match_jet4_ad() {
        let block = GaoBBlock::new(&ammonia()).unwrap();
        for (tau, delta) in [(1.2, 0.45), (0.7, 2.3), (2.4, 1e-3)] {
            let mut acc = Derivs::zero(Order::Four);
            block.accumulate::<f64, 4>(&Vars::new(tau, delta), &mut acc);
            let ad = paper(&ammonia(), Jet4::tau(tau), Jet4::delta(delta)).derivs(tau, delta);
            for n in 0..=4 {
                for i in 0..=n {
                    let (got, want) = (acc.get(i, n - i).unwrap(), ad.get(i, n - i).unwrap());
                    assert!(((got - want) / want).abs() < 1e-12, "({tau}, {delta}) A{i}{}: {got} vs {want}", n - i);
                }
            }
        }
    }

    /// The δ → 0 coefficients by hand for d = 1: `a_1 = f`, `a_2 = 2ηε f`, with `f = n τ^t e^(v − ηε²)` and
    /// `τ f' = f·(t + τ v')`, `v' = −2β(τ − γ)/q²`.
    #[test]
    fn gao_b_zero_density_coefficients_by_hand() {
        let terms = &ammonia()[..1];
        let tau = 1.3;
        let (w, q) = (tau - 1.108, 0.3696 * (tau - 1.108) * (tau - 1.108) + 1.244);
        let f = -1.6909858 * math::exp(4.3315 * math::ln(tau) + 1.0 / q - 2.8452 * 0.4478 * 0.4478);
        let tf = f * (4.3315 - tau * 2.0 * 0.3696 * w / (q * q));
        let a2 = 2.0 * 2.8452 * 0.4478;
        let got = GaoBBlock::new(terms).unwrap().zero_density_series(tau);
        let want = [[0.0, 0.0], [f, tf], [a2 * f, a2 * tf]];
        for (g, w) in got.iter().flatten().zip(want.iter().flatten()) {
            assert!((g - w).abs() <= 1e-14 * w.abs(), "{got:?} vs {want:?}");
        }
    }

    /// Non-finite data, exponents past `MAX_POW` and a possible pole are refused at decode.
    #[test]
    fn bad_gao_b_terms_are_load_errors() {
        assert!(GaoBBlock::new(&ammonia()).is_ok());
        let broken: [fn(&mut GaoBTerm); 4] = [|p| p.b = 0.0, |p| p.beta = -0.1, |p| p.n = f64::NAN, |p| p.d = 17];
        for edit in broken {
            let mut terms = ammonia();
            edit(&mut terms[1]);
            assert!(GaoBBlock::new(&terms).is_err(), "{terms:?}");
        }
    }
}
