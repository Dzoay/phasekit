//! The ideal-gas part α⁰: a closed set of term kinds, evaluated in the fluid's own reducing variables.

use crate::derivs::{Derivs, Order};
use crate::error::{Error, LoadError};
use crate::num::math;

/// Ideal-gas term kinds. CoolProp's 10 JSON kinds normalise to these at datagen (map 02 §9); an unknown kind
/// is a `LoadError`, never silently skipped (map 10 R8, map 09 R7).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum IdealTerm {
    /// `ln δ + a1 + a2·τ` (the `ln δ` part is [`Derivs::IDEAL_DELTA`]).
    Lead {
        /// Constant.
        a1: f64,
        /// τ coefficient.
        a2: f64,
    },
    /// `a·ln τ`.
    LogTau {
        /// Coefficient.
        a: f64,
    },
    /// `n·τ^t`.
    Power {
        /// Coefficient.
        n: f64,
        /// Exponent.
        t: f64,
    },
    /// `n·ln(1 − e^(−θτ))`, θ > 0 (CoolProp stores θ with a flipped sign; datagen normalises it).
    PlanckEinstein {
        /// Coefficient.
        n: f64,
        /// Characteristic reduced temperature.
        theta: f64,
    },
}

/// α⁰ of one fluid with its own reducing constants. Families that borrow a canonical ideal gas reach it
/// through `Fluid::model().helmholtz()?.ideal(t, rho, order)` in (T, ρ), so its τ never meets another
/// model's τ (map 15 X6, map 06 C1).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IdealGas {
    t_r: f64,
    rho_r: f64,
    terms: Box<[IdealTerm]>,
}

impl IdealGas {
    /// Validates the reducing constants.
    pub(crate) fn new(t_r: f64, rho_r: f64, terms: Vec<IdealTerm>) -> Result<Self, Error> {
        if !(t_r > 0.0 && rho_r > 0.0) {
            return Err(Error::Load(LoadError::Format("ideal-gas reducing constants must be > 0".into())));
        }
        Ok(Self { t_r, rho_r, terms: terms.into_boxed_slice() })
    }

    /// α⁰ bundle at (T, ρ): `ln δ` plus a τ-jet; no mixed derivatives exist.
    pub(crate) fn eval(&self, t: f64, rho: f64, order: Order) -> Derivs {
        let tau = self.t_r / t;
        let mut a = [0.0; 5]; // τ^k d^k/dτ^k of the τ-part, k = 0..4
        for term in self.terms.iter() {
            let add: [f64; 5] = match *term {
                IdealTerm::Lead { a1, a2 } => [a1 + a2 * tau, a2 * tau, 0.0, 0.0, 0.0],
                IdealTerm::LogTau { a } => [a * math::ln(tau), a, -a, 2.0 * a, -6.0 * a],
                IdealTerm::Power { n, t: e } => {
                    let v = n * math::powf(tau, e);
                    [
                        v,
                        v * e,
                        v * e * (e - 1.0),
                        v * e * (e - 1.0) * (e - 2.0),
                        v * e * (e - 1.0) * (e - 2.0) * (e - 3.0),
                    ]
                }
                IdealTerm::PlanckEinstein { n, theta } => planck_einstein(n, theta, tau),
            };
            a.iter_mut().zip(add).for_each(|(x, y)| *x += y);
        }
        let ln_delta = math::ln(rho / self.rho_r);
        Derivs::from_fn(order, |i, j| match (i, j) {
            (0, 0) => a[0] + ln_delta,
            (i, 0) => a[i],
            (0, j) => Derivs::IDEAL_DELTA.get(0, j).unwrap_or(0.0),
            _ => 0.0,
        })
    }
}

/// `τ^k d^k/dτ^k` of `n·ln(1 − e^(−θτ))`, k = 0..4, in closed form. With `w = 1/expm1(θτ)`:
/// f' = nθw, f'' = −nθ²w(1+w), f''' = nθ³w(1+w)(1+2w), f'''' = −nθ⁴w(1+w)(1+6w+6w²);
/// f = n·ln(−expm1(−θτ)) stays accurate for small θτ (map 02 §9).
fn planck_einstein(n: f64, theta: f64, tau: f64) -> [f64; 5] {
    let x = theta * tau;
    let w = 1.0 / math::expm1(x);
    let w1 = w * (1.0 + w);
    let (x2, x3) = (x * x, x * x * x);
    [
        n * math::ln(-math::expm1(-x)),
        n * x * w,
        -n * x2 * w1,
        n * x3 * w1 * (1.0 + 2.0 * w),
        -n * x3 * x * w1 * (1.0 + 6.0 * w + 6.0 * w * w),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::Real;
    use crate::num::hyperdual::HyperDual;

    /// The closed form against hyper-dual AD: orders 1-2 of f, order 3 as f''' = (f')'', order 4 as
    /// f'''' = (f'')'' (each differentiated twice by AD).
    #[test]
    fn planck_einstein_matches_ad() {
        let (n, theta, tau) = (1.3, 2.7, 0.9);
        let x = HyperDual::var(tau, true, true);
        let f = (-(-(x * theta)).expm1()).ln() * n;
        let w = |x: HyperDual| HyperDual::from_f64(1.0) / (x * theta).expm1();
        let f1 = w(x) * (n * theta);
        let f2 = w(x) * (w(x) + 1.0) * (-n * theta * theta);
        let got = planck_einstein(n, theta, tau);
        let want = [f.re, tau * f.e1, tau * tau * f.e12, tau * tau * tau * f1.e12, tau * tau * tau * tau * f2.e12];
        for (k, (g, w)) in got.iter().zip(want).enumerate() {
            assert!(((g - w) / w).abs() < 1e-14, "order {k}: {g} vs {w}");
        }
    }
}
