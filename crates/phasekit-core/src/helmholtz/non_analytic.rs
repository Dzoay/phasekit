//! Non-analytic terms `n Δ^b δ ψ` (Span & Wagner 1996, Wagner & Pruß 2002; CarbonDioxide 3 terms, Water 2) with
//! `Δ = θ² + B[(δ − 1)²]^a`, `θ = (1 − τ) + A[(δ − 1)²]^(1/(2β))` and `ψ = e^(−C(δ − 1)² − D(τ − 1)²)`. They are not
//! separable, so each term is evaluated once on [`Jet4`] (D2). `[(δ − 1)²]^p` is written `|δ − 1|^(2p)`: no division by
//! δ − 1, no 0/0 form and no nudge away from δ = 1 or τ = 1, unlike CoolProp's 10ε offset (map 02 §6, ROT-065). At
//! τ = δ = 1, the critical point, the block returns its limits there (`NonAnalyticBlock::critical`, E17).

use super::NonAnalyticTerm;
use crate::derivs::{Derivs, Order};
use crate::error::{Error, LoadError};
use crate::num::{Jet4, Real};

/// One term's `f = n Δ^b ψ` at (τ, δ), as jets: α = δ·f is the paper formula.
fn f(p: &NonAnalyticTerm, tau: Jet4, delta: Jet4) -> Jet4 {
    let (w, u) = (delta + -1.0, tau + -1.0);
    let aw = w.abs();
    let theta = Jet4::constant(1.0) - tau + aw.powf(1.0 / p.beta) * p.big_a;
    let big_delta = theta * theta + aw.powf(2.0 * p.a) * p.big_b;
    let psi = (-(w * w * p.big_c) - u * u * p.big_d).exp();
    big_delta.powf(p.b) * psi * p.n
}

/// A block of non-analytic terms, validated at decode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NonAnalyticBlock {
    terms: Box<[NonAnalyticTerm]>,
}

impl NonAnalyticBlock {
    /// Validates terms: every coefficient finite; β and a positive (the exponents of the paper's form; a zero β would
    /// divide by zero); 1/2 < b < 1, the form's purpose (first derivatives finite and c_v divergent at the critical
    /// point; Water's and CarbonDioxide's b lie in [0.85, 0.95]); and B > 0, so that Δ vanishes only at τ = δ = 1.
    pub(crate) fn new(terms: &[NonAnalyticTerm]) -> Result<Self, Error> {
        for p in terms {
            let all = [p.n, p.a, p.b, p.beta, p.big_a, p.big_b, p.big_c, p.big_d];
            let form = p.beta > 0.0 && p.a > 0.0 && p.b > 0.5 && p.b < 1.0 && p.big_b > 0.0;
            if !all.iter().all(|x| x.is_finite()) || !form {
                return Err(Error::Load(LoadError::Format(
                    "non-analytic term: a non-finite coefficient or an exponent outside the form".into(),
                )));
            }
        }
        Ok(Self { terms: terms.into() })
    }

    /// `A_ij += τ^i δ^j ∂^(i+j)α/∂τ^i∂δ^j` of every term, from one `Jet4` evaluation at (τ, δ); at τ = δ = 1, the
    /// limits of [`Self::critical`].
    pub(crate) fn accumulate(&self, tau: f64, delta: f64, acc: &mut Derivs) {
        if tau == 1.0 && delta == 1.0 {
            *acc = *acc + self.critical();
            return;
        }
        let (t, d) = (Jet4::tau(tau), Jet4::delta(delta));
        let sum = self.terms.iter().fold(Jet4::constant(0.0), |sum, p| sum + f(p, t, d));
        *acc = *acc + (sum * d).derivs(tau, delta);
    }

    /// The block at τ = δ = 1, where every term's Δ vanishes: Δ^b has no second τ-derivative there, and the terms'
    /// jets would meet as ∞ − ∞. The limits along the critical isochore δ = 1, where every Δ = (1 − τ)²: α, its first
    /// derivatives, A11 and A02 tend to 0 (b > 1/2), and A20 to ±∞ with the sign of Σn over the terms of least b, whose
    /// Δ^(b−1) diverges fastest. For Water's and CarbonDioxide's terms these are the limits from every direction, and
    /// A20 → −∞ (c_v → +∞). Orders 3 and 4 have no single limit (A30 changes sign with 1 − τ) and are NaN. No nudge
    /// off the point (ROT-065).
    fn critical(&self) -> Derivs {
        let least = self.terms.iter().map(|p| p.b).fold(f64::INFINITY, f64::min);
        let n: f64 = self.terms.iter().filter(|p| p.b == least).map(|p| p.n).sum();
        Derivs::from_fn(Order::Four, |i, j| match (i, j) {
            (2, 0) => n * f64::INFINITY,
            _ if i + j <= 2 => 0.0,
            _ => f64::NAN,
        })
    }

    /// Exact δ → 0 Taylor coefficients `[a_k(τ), τ·da_k/dτ]`, k = 0, 1, 2: α = δ·f(τ, δ) with `f = n Δ^b ψ` analytic at
    /// δ = 0 (there |δ − 1| = 1), so `a_1 = f(τ, 0)` and `a_2 = ∂f/∂δ(τ, 0)`.
    pub(crate) fn zero_density_series(&self, tau: f64) -> [[f64; 2]; 3] {
        let (t, d) = (Jet4::tau(tau), Jet4::delta(0.0));
        let g = self.terms.iter().fold(Jet4::constant(0.0), |sum, p| sum + f(p, t, d));
        let at = |i: usize, j: usize| g.derivative(i, j).unwrap_or(f64::NAN);
        [[0.0, 0.0], [at(0, 0), tau * at(1, 0)], [at(0, 1), tau * at(1, 1)]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// IAPWS-95's two non-analytic terms (Wagner & Pruß 2002, Table 2: i = 55, 56).
    fn water() -> Vec<NonAnalyticTerm> {
        let term =
            |n, b, big_c, big_d| NonAnalyticTerm { n, a: 3.5, b, beta: 0.3, big_a: 0.32, big_b: 0.2, big_c, big_d };
        vec![term(-0.14874640856724, 0.85, 28.0, 700.0), term(0.31806110878444, 0.95, 32.0, 800.0)]
    }

    /// δ → 0: the series agrees with the block's own α and τ∂α/∂τ at δ = 1e-10, where the omitted δ³ terms are below
    /// 1e-16 relative (ψ = e^(−C(δ−1)²) with C = 28 makes f''/2f ≈ 1500); a_0 is zero (α = δ·f).
    #[test]
    fn non_analytic_zero_density_series_matches_the_block() {
        let block = NonAnalyticBlock::new(&water()).unwrap();
        let (tau, delta) = (0.95, 1e-10);
        let [a0, a1, a2] = block.zero_density_series(tau);
        assert_eq!(a0, [0.0, 0.0]);
        let mut acc = Derivs::zero(Order::One);
        block.accumulate(tau, delta, &mut acc);
        for (k, got) in [(0, acc.get(0, 0).unwrap()), (1, acc.get(1, 0).unwrap())] {
            let series = a1[k] * delta + a2[k] * delta * delta;
            assert!(((got - series) / series).abs() < 1e-14, "{k}: {got} vs {series}");
        }
    }

    /// Non-finite data and exponents outside the form (β = 0 would divide by zero; b outside (1/2, 1); B = 0 would let
    /// Δ vanish off the critical point) are refused at decode.
    #[test]
    fn bad_non_analytic_terms_are_load_errors() {
        assert!(NonAnalyticBlock::new(&water()).is_ok());
        let broken: [fn(&mut NonAnalyticTerm); 7] = [
            |p| p.beta = 0.0,
            |p| p.a = 0.0,
            |p| p.b = 0.5,
            |p| p.b = 1.0,
            |p| p.big_b = 0.0,
            |p| p.big_d = f64::INFINITY,
            |p| p.n = f64::NAN,
        ];
        for edit in broken {
            let mut terms = water();
            edit(&mut terms[1]);
            assert!(NonAnalyticBlock::new(&terms).is_err(), "{terms:?}");
        }
    }
}
