//! Gaussian bell-shaped terms `n τ^t δ^d e^(−η(δ − ε)² − β(τ − γ)²)` (433 default terms in 78 fluids; map 02 §3.1).
//! Each side is a power times a Gaussian, so its scaled derivatives follow from Leibniz's rule: the power gives the
//! falling factorials of its exponent, the Gaussian the complete Bell polynomials of `z u'` and `z² u''` with
//! `u = −η(z − ε)²` (Hermite polynomials in `w = z − ε`). Working in `w` keeps sharp Gaussians (β up to 1888)
//! accurate near their centre, where a polynomial expanded in z would cancel (PLAN.md M3.4).

use core::fmt;

use super::GaussianTerm;
use super::power::{Vars, falling};
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

/// The scaled derivatives `z^j ∂^j (z^p e^u) / (z^p e^u)`, j ≤ ORD, from the falling factorials of p and
/// `h_k = z^k u⁽ᵏ⁾`: Leibniz's rule over the complete Bell polynomials `B_m(h_1, …, h_m) = z^m ∂^m e^u / e^u`.
#[inline(always)]
#[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
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
    for ((j, slot), (&fj, row)) in
        b.iter_mut().enumerate().zip(falling.iter().zip(&BINOMIAL)).take(ORD.saturating_add(1)).skip(1)
    {
        let mut sum = R::from_f64(fj);
        // m = 1..=j: B_m C(j, m) (p)_(j−m), the falling factorials read from j down.
        let terms = bell.iter().zip(row).zip(falling.get(..=j).unwrap_or_default().iter().rev());
        for ((&bm, &c), &f) in terms.take(j.saturating_add(1)).skip(1) {
            sum = sum + bm * (c * f);
        }
        *slot = sum;
    }
    b
}

/// `[z u', z² u'', 0, 0]` for `u = −η(z − ε)²`, and u itself.
#[inline(always)]
#[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
pub(super) fn gaussian_side<R: Real>(z: R, eta: f64, epsilon: f64) -> (R, [R; 4]) {
    let w = z + -epsilon;
    let zero = R::from_f64(0.0);
    (w * w * -eta, [w * z * (-2.0 * eta), z * z * (-2.0 * eta), zero, zero])
}

/// The δ → 0 Taylor coefficients of `e^(−η(δ − ε)² + ηε²)`: `[1, 2ηε, 2η²ε² − η]`.
pub(super) fn delta_series(eta: f64, epsilon: f64) -> [f64; 3] {
    [1.0, 2.0 * eta * epsilon, 2.0 * eta * eta * epsilon * epsilon - eta]
}

/// The τ-side `τ^t e^u` of a term `n τ^t δ^d e^(u(τ) − η(δ − ε)²)` whose δ-side is Gaussian: all a family adds to
/// [`GaussianBlock`] (Gaussian: `u = −β(τ − γ)²`; GaoB: `u = 1/(β(τ − γ)² + b)`).
pub(crate) trait TauSide: Copy + fmt::Debug + PartialEq {
    /// The family's term data (`terms.rs`).
    type Term;
    /// The family's name in a `LoadError`.
    const NAME: &'static str;
    /// `([n, t, η, ε], d, τ-side)` of a term.
    fn split(term: &Self::Term) -> ([f64; 4], u8, Self);
    /// Every coefficient of the τ-side finite.
    fn finite(&self) -> bool;
    /// What the family refuses in finite data, as the `LoadError`'s text.
    fn refused(&self) -> Option<&'static str> {
        None
    }
    /// `(u, [τ u', τ² u'', τ³ u''', τ⁴ u''''])` at τ.
    fn jet<R: Real>(&self, tau: R) -> (R, [R; 4]);
    /// `(t ln τ + u, t + τ u', τ u' + τ² u'')` at a real τ, as the paper writes them: the δ → 0 series' τ-factor
    /// `f = n e^(t ln τ + u)`, s = `τ f' / f` and `τ ds/dτ`, so that `τ² f'' / f = s² − s + τ ds/dτ`.
    fn series(&self, t: f64, tau: f64) -> (f64, f64, f64);
}

/// The τ-side of a Gaussian term, `u = −β(τ − γ)²`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GaussianTau {
    beta: f64,
    gamma: f64,
}

impl TauSide for GaussianTau {
    type Term = GaussianTerm;
    const NAME: &'static str = "Gaussian";

    fn split(p: &GaussianTerm) -> ([f64; 4], u8, Self) {
        ([p.n, p.t, p.eta, p.epsilon], p.d, Self { beta: p.beta, gamma: p.gamma })
    }

    fn finite(&self) -> bool {
        self.beta.is_finite() && self.gamma.is_finite()
    }

    #[inline(always)]
    fn jet<R: Real>(&self, tau: R) -> (R, [R; 4]) {
        gaussian_side(tau, self.beta, self.gamma)
    }

    /// `τ f' = f·(t − 2βτ(τ − γ))`; τu' + τ²u'' = −2βτ(τ − γ) − 2βτ².
    fn series(&self, t: f64, tau: f64) -> (f64, f64, f64) {
        let w = tau - self.gamma;
        let tu = -2.0 * self.beta * tau * w;
        (t * math::ln(tau) - self.beta * w * w, t + tu, tu - 2.0 * self.beta * tau * tau)
    }
}

/// A structure-of-arrays block of terms with a Gaussian δ-side and the τ-side `S`, compiled once at decode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GaussianBlock<S> {
    n: Box<[f64]>,
    t: Box<[f64]>,
    d: Box<[u8]>,
    eta: Box<[f64]>,
    epsilon: Box<[f64]>,
    side: Box<[S]>,
    ft: Box<[[f64; 5]]>,
    fd: Box<[[f64; 5]]>,
}

impl<S: TauSide> GaussianBlock<S> {
    /// Validates and compiles terms: non-finite data, `d` beyond [`super::MAX_POW`] and what the family refuses
    /// ([`TauSide::refused`]) are a `LoadError`.
    pub(crate) fn new(terms: &[S::Term]) -> Result<Self, Error> {
        let bad = |m: &str| Error::Load(LoadError::Format(m.into()));
        let parts: Vec<([f64; 4], u8, S)> = terms.iter().map(S::split).collect();
        for (shared, d, side) in &parts {
            if !(shared.iter().all(|x| x.is_finite()) && side.finite()) {
                return Err(bad(&format!("non-finite {} term coefficient", S::NAME)));
            }
            if usize::from(*d) > super::MAX_POW {
                return Err(bad(&format!("{} term exponent out of range", S::NAME)));
            }
            if let Some(why) = side.refused() {
                return Err(bad(why));
            }
        }
        let column = |i: usize| parts.iter().map(|(shared, _, _)| shared.get(i).copied().unwrap_or(f64::NAN)).collect();
        Ok(Self {
            n: column(0),
            t: column(1),
            d: parts.iter().map(|(_, d, _)| *d).collect(),
            eta: column(2),
            epsilon: column(3),
            side: parts.iter().map(|(_, _, side)| *side).collect(),
            ft: parts.iter().map(|(shared, _, _)| falling(shared[1])).collect(),
            fd: parts.iter().map(|(_, d, _)| falling(f64::from(*d))).collect(),
        })
    }

    /// `A_ij += φ·B^τ_i·B^δ_j` for every term, `i + j ≤ ORD`.
    #[inline(always)]
    #[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
    pub(crate) fn accumulate<R: Real, const ORD: usize>(&self, v: &Vars<R>, acc: &mut Derivs<R>) {
        let delta = v.delta_pow[1];
        let terms = self.n.iter().zip(&self.t).zip(&self.d).zip(&self.eta).zip(&self.epsilon);
        for (((((&n, &t), &d), &eta), &epsilon), (side, (ft, fd))) in
            terms.zip(self.side.iter().zip(self.ft.iter().zip(&self.fd)))
        {
            let (ut, ht) = side.jet(v.tau);
            let (ud, hd) = gaussian_side(delta, eta, epsilon);
            let power = v.delta_pow.get(usize::from(d)).copied().unwrap_or_else(|| R::from_f64(f64::NAN));
            let phi = (v.ln_tau * t + ut + ud).exp() * power * n;
            acc.add_separable::<ORD>(phi, &leibniz::<R, ORD>(ft, ht), &leibniz::<R, ORD>(fd, hd));
        }
    }

    /// Exact δ → 0 Taylor coefficients `[a_k(τ), τ·da_k/dτ, τ²·d²a_k/dτ²]`, k = 0, 1, 2: `δ^d e^(−η(δ−ε)²) = e^(−ηε²)
    /// δ^d (1 + 2ηε δ + (2η²ε² − η) δ² + …)`, times the τ-factor `f = n τ^t e^u`, `τ f'` and `τ² f''`
    /// ([`TauSide::series`]).
    pub(crate) fn zero_density_series(&self, tau: f64) -> [[f64; 3]; 3] {
        let mut out = [[0.0; 3]; 3];
        let terms = self.n.iter().zip(&self.t).zip(&self.d).zip(&self.eta).zip(&self.epsilon).zip(self.side.iter());
        for (((((&n, &t), &d), &eta), &eps), side) in terms {
            let (log_f, s, ts) = side.series(t, tau);
            let f = n * math::exp(log_f - eta * eps * eps);
            let (tf, ttf) = (f * s, f * (s * s - s + ts));
            for (q, c) in delta_series(eta, eps).into_iter().enumerate() {
                if let Some(slot) = out.get_mut(usize::from(d).saturating_add(q)) {
                    slot[0] += c * f;
                    slot[1] += c * tf;
                    slot[2] += c * ttf;
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
    use crate::helmholtz::power::{ad_misses, sum_of};
    use crate::num::Jet4;

    type Block = GaussianBlock<GaussianTau>;

    /// The paper formula, written independently of the block.
    fn paper<R: Real>(terms: &[GaussianTerm], tau: R, delta: R) -> R {
        sum_of(terms, |p| {
            let (wt, wd) = (tau + -p.gamma, delta + -p.epsilon);
            let e = -(wd * wd * p.eta) - wt * wt * p.beta;
            tau.powf(p.t) * delta.powi(i32::from(p.d)) * e.exp() * p.n
        })
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
        let block = Block::new(&water()).unwrap();
        for (tau, delta) in [(1.22, 1.05), (1.6, 0.3), (0.9, 2.1)] {
            let mut acc = Derivs::zero(Order::Four);
            block.accumulate::<f64, 4>(&Vars::new(tau, delta), &mut acc);
            let ad = paper(&water(), Jet4::tau(tau), Jet4::delta(delta)).derivs(tau, delta);
            let misses = ad_misses(&acc, &ad, 1e-11);
            assert!(misses.is_empty(), "({tau}, {delta}) {misses:?}");
        }
    }

    /// δ → 0: the series agrees with the block's own values at δ = 1e-8 (d = 3: only a_3 onwards, so a_0..a_2 are 0);
    /// with d = 0 (Methane's terms) the series carries α^r itself.
    #[test]
    fn gaussian_zero_density_series() {
        assert_eq!(Block::new(&water()).unwrap().zero_density_series(1.3), [[0.0; 3]; 3]);
        let d0 = [GaussianTerm { n: 0.7, t: 1.5, d: 0, eta: 3.0, epsilon: 0.5, beta: 2.0, gamma: 1.1 }];
        let block = Block::new(&d0).unwrap();
        let (tau, delta) = (1.3, 1e-8);
        let [a0, a1, a2] = block.zero_density_series(tau);
        let mut acc = Derivs::zero(Order::Two);
        block.accumulate::<f64, 2>(&Vars::new(tau, delta), &mut acc);
        let at = |k: usize| a0[k] + a1[k] * delta + a2[k] * delta * delta;
        for (k, i) in [(0, 0), (1, 1), (2, 2)] {
            assert!(((acc.get(i, 0).unwrap() - at(k)) / at(k)).abs() < 1e-14, "A{i}0");
        }
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

    /// The δ → 0 coefficients by hand for d = 0: `f·[1, 2ηε, 2η²ε² − η]`, `τ f' = f·s` with s = t − 2βτ(τ − γ), and
    /// `τ² f'' = f·(s² − s − 2βτ(τ − γ) − 2βτ²)`.
    #[test]
    fn gaussian_zero_density_coefficients_by_hand() {
        let term = GaussianTerm { n: 0.7, t: 1.5, d: 0, eta: 3.0, epsilon: 0.4, beta: 2.0, gamma: 1.1 };
        let tau = 1.3;
        let f = 0.7 * math::exp(1.5 * math::ln(tau) - 2.0 * 0.2 * 0.2 - 3.0 * 0.16);
        let (tu, s) = (-2.0 * 2.0 * tau * 0.2, 1.5 - 2.0 * 2.0 * tau * 0.2);
        let (tf, ttf) = (f * s, f * (s * s - s + tu - 2.0 * 2.0 * tau * tau));
        let got = Block::new(&[term]).unwrap().zero_density_series(tau);
        // 2ηε = 2.4, 2η²ε² − η = 2.88 − 3 = −0.12.
        let want = [[f, tf, ttf], [2.4 * f, 2.4 * tf, 2.4 * ttf], [-0.12 * f, -0.12 * tf, -0.12 * ttf]];
        for (g, w) in got.iter().flatten().zip(want.iter().flatten()) {
            assert!(((g - w) / w).abs() < 1e-14, "{got:?} vs {want:?}");
        }
    }

    /// Non-finite data and exponents past `MAX_POW` are refused at decode; `d = MAX_POW` is not.
    #[test]
    fn bad_gaussian_terms_are_load_errors() {
        let mut terms = water();
        assert!(Block::new(&terms).is_ok());
        terms[1].beta = f64::INFINITY;
        assert!(Block::new(&terms).is_err());
        let mut terms = water();
        terms[0].d = 17;
        assert!(Block::new(&terms).is_err());
        terms[0].d = 16;
        assert!(Block::new(&terms).is_ok());
    }
}
