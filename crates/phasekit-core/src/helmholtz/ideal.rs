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
    /// Validates the reducing constants and every term's numbers: finite, and the positive temperatures a c_p⁰ term
    /// divides by.
    pub(crate) fn new(t_r: f64, rho_r: f64, terms: Vec<IdealTerm>) -> Result<Self, Error> {
        let bad = |m: &str| Error::Load(LoadError::Format(m.into()));
        if !(t_r > 0.0 && rho_r > 0.0) {
            return Err(bad("ideal-gas reducing constants must be > 0"));
        }
        for term in &terms {
            let (numbers, positive): (&[f64], &[f64]) = match term {
                IdealTerm::Lead { a1, a2 } | IdealTerm::Offset { a1, a2, .. } => (&[*a1, *a2], &[]),
                IdealTerm::LogTau { a } => (&[*a], &[]),
                IdealTerm::Power { n, t } => (&[*n, *t], &[]),
                IdealTerm::PlanckEinstein { n, theta } => (&[*n, *theta], &[]),
                IdealTerm::PlanckEinsteinGeneralized { n, theta, c, d } => (&[*n, *theta, *c, *d], &[]),
                IdealTerm::Cp0Power { c, t, tc, t0 } => (&[*c, *t], &[*tc, *t0]),
            };
            if !numbers.iter().chain(positive).all(|x| x.is_finite()) || !positive.iter().all(|x| *x > 0.0) {
                return Err(bad("ideal-gas term: non-finite number or non-positive temperature"));
            }
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
                IdealTerm::PlanckEinsteinGeneralized { n, theta, c, d } => {
                    planck_einstein_generalized(n, theta, c, d, tau)
                }
                IdealTerm::Cp0Power { c, t: e, tc, t0 } => cp0_power(c, e, tc, t0, tau),
                IdealTerm::Offset { a1, a2, .. } => [a1 + a2 * tau, a2 * tau, 0.0, 0.0, 0.0],
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

/// `τ^k d^k/dτ^k` of `n·ln(c + d e^(θτ))`, k = 0..4. With `x = θτ` and the logistic `s = d e^x / (c + d e^x)`:
/// f′ = nθs, f″ = nθ²s(1−s), f‴ = nθ³s(1−s)(1−2s), f⁗ = nθ⁴s(1−s)(1−6s+6s²) (CoolProp's closed forms,
/// `Helmholtz.cpp:1152-1176`, rearranged). For x > 0 the value is `n(x + ln(d + c e^(−x)))` and s is `d/(c e^(−x) + d)`,
/// so no exponential overflows (Air's θ = 87.31 would overflow e^(θτ) below 16.3 K; map 02 §3.2). For x ≤ 0,
/// `c + d e^x = (c + d) + d·expm1(x)` keeps the Aly-Lee form `ln(1 − e^x)` accurate for small |x|.
fn planck_einstein_generalized(n: f64, theta: f64, c: f64, d: f64, tau: f64) -> [f64; 5] {
    let x = theta * tau;
    // s and its complement 1 − s are each computed from their own expression: near s = 1 (Air above θτ ≈ 36) the
    // subtraction 1 − s would keep none of the tail's digits.
    let (value, s, sc) = if x > 0.0 {
        let e = math::exp(-x);
        let den = c * e + d;
        (x + math::ln(d + c * e), d / den, c * e / den)
    } else {
        let g = (c + d) + d * math::expm1(x);
        (math::ln(g), d * math::exp(x) / g, c / g)
    };
    let s1 = s * sc;
    let (x2, x3) = (x * x, x * x * x);
    [n * value, n * x * s, n * x2 * s1, n * x3 * s1 * (sc - s), n * x3 * x * s1 * (1.0 - 6.0 * s1)]
}

/// `τ^k d^k/dτ^k` of the α⁰ part of `c_p⁰/R = c·T^t`, integrated from `t0` (CoolProp's CP0PolyT and CP0Constant,
/// `Helmholtz.cpp:1180-1258`), written in τ with the block's own `tc` and τ₀ = tc/t0. With P = −c tc^t τ^(−t)/(t(t+1)):
/// `f = P − c t0^(t+1) τ/(tc(t+1)) + c t0^t/t` and `τ^k d^k P/dτ^k = (−t)_k P`; t = 0 and t = −1 are the logarithmic
/// limits `c(1 − τ/τ₀ + ln(τ/τ₀))` and `(c/tc)(τ ln(τ₀/τ) + τ − τ₀)`. The fourth derivative of the t = −1 form is
/// −2c/(τ³ tc), where CoolProp writes −3c (map 02 §6, ROT-060).
fn cp0_power(c: f64, t: f64, tc: f64, t0: f64, tau: f64) -> [f64; 5] {
    let tau0 = tc / t0;
    if t == 0.0 {
        return [c * (1.0 - tau / tau0 + math::ln(tau / tau0)), c * (1.0 - tau / tau0), -c, 2.0 * c, -6.0 * c];
    }
    if t == -1.0 {
        let k = c * tau / tc;
        return [k * math::ln(tau0 / tau) + c / tc * (tau - tau0), k * math::ln(tau0 / tau), -k, k, -2.0 * k];
    }
    let p = -c * math::powf(tc, t) * math::powf(tau, -t) / (t * (t + 1.0));
    let linear = c * math::powf(t0, t + 1.0) * tau / (tc * (t + 1.0));
    let falling = |k: usize| (0..k).fold(1.0, |acc, m| acc * (-t - m as f64));
    [p - linear + c * math::powf(t0, t) / t, -t * p - linear, falling(2) * p, falling(3) * p, falling(4) * p]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::{Jet4, Real};

    /// Every kind has an evaluator since M4.2; non-finite numbers and the non-positive temperatures a c_p⁰ term divides
    /// by are refused when the ideal gas is built, never evaluated.
    #[test]
    fn bad_ideal_terms_are_load_errors() {
        let lead = IdealTerm::Lead { a1: 1.0, a2: 2.0 };
        let good = [
            IdealTerm::PlanckEinsteinGeneralized { n: 1.0, theta: 2.0, c: 1.0, d: -1.0 },
            IdealTerm::Cp0Power { c: 1.0, t: 0.0, tc: 300.0, t0: 298.15 },
            IdealTerm::Offset { a1: 1.0, a2: 2.0, reference: OffsetReference::Iir },
        ];
        assert!(IdealGas::new(300.0, 1e4, [&[lead][..], &good].concat()).is_ok());
        let bad = [
            IdealTerm::Cp0Power { c: 1.0, t: 0.0, tc: 300.0, t0: 0.0 },
            IdealTerm::Cp0Power { c: 1.0, t: 0.0, tc: -1.0, t0: 298.15 },
            IdealTerm::PlanckEinsteinGeneralized { n: f64::NAN, theta: 2.0, c: 1.0, d: -1.0 },
            IdealTerm::Offset { a1: f64::INFINITY, a2: 2.0, reference: OffsetReference::Nbp },
            IdealTerm::Power { n: 1.0, t: f64::NAN },
        ];
        for term in bad {
            let err = IdealGas::new(300.0, 1e4, vec![lead, term]).unwrap_err().to_string();
            assert!(err.contains("non-finite number or non-positive temperature"), "{term:?}: {err}");
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

    /// α⁰ of `c_p⁰/R = c·T^t` from its definition, in T = tc/τ: `(1/T)∫_{t0}^{T} c T′^t dT′ − ∫_{t0}^{T} c T′^(t−1) dT′`,
    /// integrated by hand for t = 0, t = −1 and the general t.
    fn cp0_integral(c: f64, t: f64, tc: f64, t0: f64, tau: Jet4) -> Jet4 {
        let temp = Jet4::constant(tc) / tau;
        let inv = Jet4::constant(1.0) / temp;
        let log = (temp * (1.0 / t0)).ln();
        if t == 0.0 {
            (Jet4::constant(1.0) - inv * t0) * c - log * c
        } else if t == -1.0 {
            inv * log * c - (Jet4::constant(1.0 / t0) - inv) * c
        } else {
            let first = (temp.powf(t + 1.0) + -math::powf(t0, t + 1.0)) * inv * (c / (t + 1.0));
            first - (temp.powf(t) + -math::powf(t0, t)) * (c / t)
        }
    }

    /// The closed forms of `cp0_power` against `Jet4` AD of the integral that defines them, all four τ-orders, for the
    /// exponents in the data (CP0Constant is t = 0; n-Undecane has t = −2 and −1; HFE143m 1, 2, 3).
    #[test]
    fn cp0_power_matches_ad() {
        let (c, tc, t0) = (0.7, 400.0, 298.15);
        for t in [0.0, -1.0, -2.0, 1.0, 1.5, 3.0] {
            for tau in [0.6, 1.3] {
                let got = cp0_power(c, t, tc, t0, tau);
                let ad = cp0_integral(c, t, tc, t0, Jet4::tau(tau)).derivs(tau, 1.0);
                for (k, g) in got.iter().enumerate() {
                    let w = ad.get(k, 0).unwrap();
                    assert!((g - w).abs() <= 1e-12 * w.abs().max(1.0), "t = {t}, τ = {tau}, order {k}: {g} vs {w}");
                }
            }
        }
    }

    /// ROT-060 (map 02 §6): the fourth τ-derivative of the t = −1 term is −2c/(τ³ tc); CoolProp writes −3c.
    #[test]
    fn cp0polyt_t_minus_one_fourth_derivative_is_minus_2c() {
        let (c, tc, t0, tau) = (0.7, 400.0, 298.15, 1.3);
        let fourth = cp0_power(c, -1.0, tc, t0, tau)[4] / math::powi(tau, 4);
        assert!((fourth / (-2.0 * c / (tau * tau * tau * tc)) - 1.0).abs() < 1e-15, "{fourth}");
    }

    /// `n ln(c + d e^(θτ))` against `Jet4` AD of the formula as written, for the forms in the data: an Aly-Lee B term
    /// (c = 1, d = −1, θ < 0), an Aly-Lee D term (c = d = 1), and Air's (θ = 87.31, c = 2/3, d = 1; Lemmon et al. 2000),
    /// within 1e-13 of the order's scale |n|(1 + |θτ|)^k (at τ = 0.5 Air's s(1 − s) ≈ 1e-19: the logistic form keeps
    /// it, the AD of the logarithm loses it in a difference of two terms near θ²τ²).
    #[test]
    fn planck_einstein_generalized_matches_ad() {
        let forms = [(13.7266, -2.0 * 169.789 / 540.13, 1.0, -1.0), (-30.4707, -2.0 * 836.195 / 540.13, 1.0, 1.0)];
        let forms = [&forms[..], &[(-0.197938904, 87.31279, 2.0 / 3.0, 1.0)]].concat();
        for (n, theta, c, d) in forms {
            for tau in [0.5, 1.0, 2.0] {
                let got = planck_einstein_generalized(n, theta, c, d, tau);
                let ad = (((Jet4::tau(tau) * theta).exp() * d + c).ln() * n).derivs(tau, 1.0);
                for (k, g) in got.iter().enumerate() {
                    let w = ad.get(k, 0).unwrap();
                    // Scale |n|(1 + |θτ|)^k: the AD of ln(c + d e^x) cancels terms of that size once s(1 − s) ≪ 1.
                    let scale = n.abs() * math::powi(1.0 + (theta * tau).abs(), k as i32);
                    assert!((g - w).abs() <= 1e-13 * scale, "θ = {theta}, τ = {tau}, order {k}: {g} vs {w}");
                }
            }
        }
    }

    /// Map 02 §3.2: Air's generalized term would overflow e^(θτ) below 16.3 K (θτ > 709); written as n(x + ln(d +
    /// c e^(−x))) it stays finite at 10 K and 1 K, where it tends to nθτ with every higher derivative vanishing.
    #[test]
    fn air_generalized_term_refuses_overflow() {
        let (n, theta, c, d, t_r) = (-0.197938904, 87.31279, 2.0 / 3.0, 1.0, 132.6312);
        for temp in [10.0, 1.0] {
            let tau = t_r / temp;
            assert!(math::exp(theta * tau).is_infinite(), "the naive form overflows at {temp} K");
            let got = planck_einstein_generalized(n, theta, c, d, tau);
            assert!(got.iter().all(|v| v.is_finite()), "{temp} K: {got:?}");
            let x = theta * tau;
            assert!((got[0] / (n * x) - 1.0).abs() < 1e-15 && (got[1] / (n * x) - 1.0).abs() < 1e-15);
            assert!(got[2..].iter().all(|v| v.abs() < 1e-300), "{temp} K: {got:?}");
        }
    }

    /// Air at θτ ≈ 36: 1 − s ≈ 1.3e-16 is below an ulp of 1, so the tail terms must come from its own expression.
    /// CoolProp's exp-based closed forms (`Helmholtz.cpp:1152-1176`; e^(θτ) ≈ 5e15 is far from overflow here) keep it:
    /// τ²f″ = nθ²τ² c d e/(c + d e)² and τ³f‴ = nθ³τ³ c d (c − d e) e/(c + d e)³ agree within 1e-14.
    #[test]
    fn generalized_term_keeps_its_tail() {
        let (n, theta, c, d, tau) = (-0.197938904, 87.31279, 2.0 / 3.0, 1.0, 0.4145642920804462);
        let got = planck_einstein_generalized(n, theta, c, d, tau);
        let (x, e) = (theta * tau, math::exp(theta * tau));
        let para = c + d * e;
        let second = n * x * x * c * d * e / (para * para);
        let third = n * x * x * x * c * d * (c - d * e) * e / (para * para * para);
        assert!((got[2] / second - 1.0).abs() < 1e-14, "{} vs {second}", got[2]);
        assert!((got[3] / third - 1.0).abs() < 1e-14, "{} vs {third}", got[3]);
    }
}
