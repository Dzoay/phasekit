//! GaoB terms `n τ^t δ^d e^(−η(δ − ε)² + 1/(β(τ − γ)² + b))` (Gao et al. 2020; Ammonia, the only fluid, 2 terms).
//! `η` has the paper's sign (datagen flips CoolProp's stored −η; map 02 §3.1), so the δ-side is a Gaussian side. The
//! τ-side is `τ^t e^v` with `v = 1/q`, `q = β(τ − γ)² + b > 0` (b > 0 in the data: no pole); its derivatives come
//! in closed form from `q' = 2β(τ − γ)`, `q'' = 2β`, `q''' = 0`.

use super::GaoBTerm;
use super::gaussian::TauSide;
use crate::num::{Real, math};

/// `(v, [τ v', τ² v'', τ³ v''', τ⁴ v''''])` for `v = 1/(β(τ − γ)² + b)`: with `r = 1/q`, `v' = −q' r²`,
/// `v'' = (2q'² r − q'') r²`, `v''' = 6q'(q'' − q'² r) r³`, `v'''' = (6q''² − 36q'² q'' r + 24q'⁴ r²) r³`.
#[inline(always)]
#[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
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

/// The τ-side of a GaoB term, `v = 1/(β(τ − γ)² + b)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GaoBTau {
    beta: f64,
    gamma: f64,
    b: f64,
}

impl TauSide for GaoBTau {
    type Term = GaoBTerm;
    const NAME: &'static str = "GaoB";

    fn split(p: &GaoBTerm) -> ([f64; 4], u8, Self) {
        ([p.n, p.t, p.eta, p.epsilon], p.d, Self { beta: p.beta, gamma: p.gamma, b: p.b })
    }

    fn finite(&self) -> bool {
        self.beta.is_finite() && self.gamma.is_finite() && self.b.is_finite()
    }

    /// A pole: `b ≤ 0` or `β < 0` would let `β(τ − γ)² + b` reach 0.
    fn refused(&self) -> Option<&'static str> {
        if self.b > 0.0 && self.beta >= 0.0 { None } else { Some("GaoB term with a pole: needs b > 0 and β ≥ 0") }
    }

    #[inline(always)]
    fn jet<R: Real>(&self, tau: R) -> (R, [R; 4]) {
        gao_b_tau_side(tau, self.beta, self.gamma, self.b)
    }

    /// `τ f' = f·(t + τ v')`.
    fn series(&self, t: f64, tau: f64) -> (f64, f64) {
        let (v, h) = gao_b_tau_side(tau, self.beta, self.gamma, self.b);
        (t * math::ln(tau) + v, t + h[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derivs::{Derivs, Order};
    use crate::helmholtz::GaussianBlock;
    use crate::helmholtz::power::{Vars, ad_misses, sum_of};
    use crate::num::Jet4;

    type Block = GaussianBlock<GaoBTau>;

    /// The paper formula (Gao et al. 2020), written independently of the block.
    fn paper<R: Real>(terms: &[GaoBTerm], tau: R, delta: R) -> R {
        sum_of(terms, |p| {
            let (wt, wd) = (tau + -p.gamma, delta + -p.epsilon);
            let e = R::from_f64(1.0) / (wt * wt * p.beta + p.b) - wd * wd * p.eta;
            tau.powf(p.t) * delta.powi(i32::from(p.d)) * e.exp() * p.n
        })
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
        let block = Block::new(&ammonia()).unwrap();
        for (tau, delta) in [(1.2, 0.45), (0.7, 2.3), (2.4, 1e-3)] {
            let mut acc = Derivs::zero(Order::Four);
            block.accumulate::<f64, 4>(&Vars::new(tau, delta), &mut acc);
            let ad = paper(&ammonia(), Jet4::tau(tau), Jet4::delta(delta)).derivs(tau, delta);
            let misses = ad_misses(&acc, &ad, 1e-12);
            assert!(misses.is_empty(), "({tau}, {delta}) {misses:?}");
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
        let got = Block::new(terms).unwrap().zero_density_series(tau);
        let want = [[0.0, 0.0], [f, tf], [a2 * f, a2 * tf]];
        for (g, w) in got.iter().flatten().zip(want.iter().flatten()) {
            assert!((g - w).abs() <= 1e-14 * w.abs(), "{got:?} vs {want:?}");
        }
    }

    /// Non-finite data, on either side, exponents past `MAX_POW` and a possible pole are refused at decode; β = 0 is
    /// no pole.
    #[test]
    fn bad_gao_b_terms_are_load_errors() {
        assert!(Block::new(&ammonia()).is_ok());
        let broken: [fn(&mut GaoBTerm); 5] =
            [|p| p.b = 0.0, |p| p.beta = -0.1, |p| p.n = f64::NAN, |p| p.gamma = f64::INFINITY, |p| p.d = 17];
        for edit in broken {
            let mut terms = ammonia();
            edit(&mut terms[1]);
            assert!(Block::new(&terms).is_err(), "{terms:?}");
        }
        let mut flat = ammonia();
        flat[1].beta = 0.0;
        assert!(Block::new(&flat).is_ok());
    }
}
