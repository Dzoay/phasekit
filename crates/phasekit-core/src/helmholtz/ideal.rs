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
    /// `n·ln(1 − e^(−θτ))`, θ > 0 (CoolProp stores θ with a flipped sign; datagen normalises it). CoolProp's
    /// `PlanckEinsteinFunctionT` arrives here with θ = v/T_crit, computed as CoolProp computes it.
    PlanckEinstein {
        /// Coefficient.
        n: f64,
        /// Characteristic reduced temperature.
        theta: f64,
    },
    /// `n·ln(c + d·e^(θτ))`, θ as CoolProp stores it (Air: θ = +87.31, c = 2/3; and the two hyperbolic terms of an
    /// Aly-Lee c_p⁰, converted at datagen as CoolProp converts them, map 02 §3.2). Evaluated from M4.2.
    PlanckEinsteinGeneralized {
        /// Coefficient.
        n: f64,
        /// Exponent factor.
        theta: f64,
        /// Constant inside the logarithm.
        c: f64,
        /// Factor of the exponential.
        d: f64,
    },
    /// The α⁰ part of `c_p⁰/R = c·T^t` integrated from the reference temperature `t0`, written in τ with the
    /// block's own `tc` in place of T_r, as CoolProp evaluates it (`Helmholtz.cpp:1188-1258`). `tc` is kept as
    /// stored: R123's is 456.82 K against T_r = 456.831 K (map 02 §6). CP0Constant is the `t = 0` case, and an Aly-Lee
    /// block's constant arrives here too. Evaluated from M4.2.
    Cp0Power {
        /// Coefficient of `T^t` in `c_p⁰/R`.
        c: f64,
        /// Exponent.
        t: f64,
        /// The block's critical temperature, K.
        tc: f64,
        /// Reference temperature, K.
        t0: f64,
    },
    /// `a1 + a2·τ`: the offset that puts the fluid in its default reference state (CoolProp's
    /// `EnthalpyEntropyOffset`; map 02 §3.2). Evaluated from M4.2.
    Offset {
        /// Constant.
        a1: f64,
        /// τ coefficient.
        a2: f64,
        /// The reference state the offset realises.
        reference: OffsetReference,
    },
}

/// The reference state an [`IdealTerm::Offset`] realises, as CoolProp tags it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OffsetReference {
    /// IIR: h = 200 kJ/kg, s = 1 kJ/(kg K) for saturated liquid at 0 °C.
    Iir,
    /// NBP: h = s = 0 for saturated liquid at 1 atm.
    Nbp,
    /// Another convention the fluid's paper uses (`OTH`).
    Other,
    /// A custom anchor (`CUSTOM`).
    Custom,
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
        let pending = terms.iter().find_map(|term| match term {
            IdealTerm::PlanckEinsteinGeneralized { .. } => Some("PlanckEinsteinGeneralized"),
            IdealTerm::Cp0Power { .. } => Some("Cp0Power"),
            IdealTerm::Offset { .. } => Some("Offset"),
            _ => None,
        });
        if let Some(kind) = pending {
            return Err(Error::Load(LoadError::Format(format!("ideal-gas {kind} terms land at M4.2").into())));
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
                // Refused by `new` until M4.2.
                IdealTerm::PlanckEinsteinGeneralized { .. } | IdealTerm::Cp0Power { .. } | IdealTerm::Offset { .. } => {
                    [0.0; 5]
                }
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
    use crate::num::{Jet4, Real};

    /// Kinds without an evaluator are refused when the ideal gas is built, never evaluated as zero (M4.2 lands them).
    #[test]
    fn kinds_without_an_evaluator_are_refused() {
        let lead = IdealTerm::Lead { a1: 1.0, a2: 2.0 };
        assert!(IdealGas::new(300.0, 1e4, vec![lead]).is_ok());
        let pending = [
            (IdealTerm::PlanckEinsteinGeneralized { n: 1.0, theta: 2.0, c: 1.0, d: -1.0 }, "PlanckEinsteinGeneralized"),
            (IdealTerm::Cp0Power { c: 1.0, t: 0.0, tc: 300.0, t0: 298.15 }, "Cp0Power"),
            (IdealTerm::Offset { a1: 1.0, a2: 2.0, reference: OffsetReference::Iir }, "Offset"),
        ];
        for (term, kind) in pending {
            let err = IdealGas::new(300.0, 1e4, vec![lead, term]).unwrap_err().to_string();
            assert!(err.contains(&format!("ideal-gas {kind} terms land at M4.2")), "{err}");
        }
    }

    /// The closed form against `Jet4` AD of `n ln(−expm1(−θτ))`, all four τ-orders.
    #[test]
    fn planck_einstein_matches_ad() {
        let (n, theta, tau) = (1.3, 2.7, 0.9);
        let x = Jet4::tau(tau);
        let ad = (((-(x * theta)).expm1() * -1.0).ln() * n).derivs(tau, 1.0);
        let got = planck_einstein(n, theta, tau);
        for (k, g) in got.iter().enumerate() {
            let w = ad.get(k, 0).unwrap();
            assert!(((g - w) / w).abs() < 1e-14, "order {k}: {g} vs {w}");
        }
    }
}
