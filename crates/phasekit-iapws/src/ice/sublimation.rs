//! Ice Ih's sublimation pressure, IAPWS R14-08(2011) Eq. (6): ln(p/p_t) = θ⁻¹ Σ aᵢ θ^bᵢ, θ = T/T_t, from 50 K to the
//! triple point, and its inverse by Newton in θ, the equation's θ-derivative closed-form.

use phasekit_core::{DomainError, Error, Strategy};

use super::gibbs::T_T;

/// p_t, Pa (Eq. (6), the "normal" triple point).
const P_T: f64 = 611.657;

/// The lowest temperature of Eq. (6), K.
pub(super) const T_MIN: f64 = 50.0;

/// (aᵢ, bᵢ) of Eq. (6).
const TERMS: [(f64, f64); 3] =
    [(-0.212_144_006e2, 0.333_333_333e-2), (0.273_203_819e2, 0.120_666_667e1), (-0.610_598_130e1, 0.170_333_333e1)];

/// ln(p_subl/p_t) at θ and its θ-derivative.
fn ln_pi(theta: f64) -> (f64, f64) {
    TERMS.iter().fold((0.0, 0.0), |(f, df), &(a, b)| {
        let x = a * theta.powf(b - 1.0);
        (f + x, df + (b - 1.0) * x / theta)
    })
}

/// `t` within [50 K, T_t].
fn within(t: f64) -> Result<(), Error> {
    if t < T_MIN {
        return Err(DomainError::BelowMinTemperature { t, t_min: T_MIN }.into());
    }
    if t > T_T {
        return Err(DomainError::AboveMaxTemperature { t, t_max: T_T }.into());
    }
    Ok(())
}

/// p_subl at `t` (K), Pa.
pub(super) fn pressure(t: f64) -> Result<f64, Error> {
    within(t)?;
    Ok(P_T * ln_pi(t / T_T).0.exp())
}

/// The iterations Newton may take: from θ = 1, Eq. (6)'s ln p is smooth and increasing in θ, and converges in under 10
/// over [50 K, T_t].
const ITERATIONS: u16 = 40;

/// T_subl at `p` (Pa), K: Newton in θ on Eq. (6) from the triple point, kept inside [50 K, T_t]; `NoConvergence` if it
/// has not settled to 4 ulp of θ.
pub(super) fn temperature(p: f64) -> Result<f64, Error> {
    let (p_min, target) = (pressure(T_MIN)?, (p / P_T).ln());
    if p < p_min {
        return Err(DomainError::BelowMinPressure { p, p_min }.into());
    }
    if p > P_T {
        return Err(DomainError::AboveMaxPressure { p, p_max: P_T }.into());
    }
    let mut theta = 1.0_f64;
    for _ in 0..ITERATIONS {
        let (f, df) = ln_pi(theta);
        let next = (theta - (f - target) / df).clamp(T_MIN / T_T, 1.0);
        if (next - theta).abs() <= 4.0 * f64::EPSILON * theta {
            return Ok(next * T_T);
        }
        theta = next;
    }
    Err(Error::NoConvergence { strategy: Strategy::External, iterations: ITERATIONS })
}
