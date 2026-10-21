//! IAPWS-IF97, the industrial formulation for water and steam (IAPWS R7-97(2012)), below 623.15 K: region 1 (the
//! liquid), region 2 (the vapour) and region 4 (the saturation line), and IF97 water as a `Fluid` over them (PLAN.md
//! M14.2, M14.3).

use phasekit_core::{DomainError, Error, ModelKey, Phase, State, bundle_from_gibbs};

mod region1;
mod region2;
mod region4;
mod water;

pub use water::water;

/// IF97's specific gas constant, J/(kg K) (R7-97(2012) Eq. (1)).
pub const R: f64 = 461.526;

/// The molar mass the states carry, kg/mol: IAPWS-95's, since IF97 prints none. Only the molar basis depends on it.
pub const MOLAR_MASS: f64 = 0.018_015_268;

/// The key of every IF97 state (`State::key`).
pub const KEY: ModelKey = ModelKey::from_content(b"IAPWS-IF97 R7-97(2012) water, regions 1, 2 and 4");

/// The critical point IF97 states, K and Pa (Eqs. (2) and (3)).
const T_C: f64 = 647.096;
const P_C: f64 = 22.064e6;

/// Region 1's bounds: 273.15 K ≤ T ≤ 623.15 K, p ≤ 100 MPa (§5's range of validity).
const T_MIN: f64 = 273.15;
const T_MAX_1: f64 = 623.15;
const P_MAX: f64 = 100e6;

/// Region 2's upper temperature, K (§5): its function is valid there, the `Fluid` stops at [`T_MAX_1`].
const T_MAX_2: f64 = 1073.15;

/// The state of region 1, the liquid, at `t` (K) and `p` (Pa), from Eq. (7) through core's `bundle_from_gibbs`:
/// 273.15 K ≤ T ≤ 623.15 K and 0 < p ≤ 100 MPa. Which region a (T, p) belongs to is the model's choice, not this
/// function's: below the saturation pressure it still evaluates Eq. (7).
pub fn region1(t: f64, p: f64) -> Result<State, Error> {
    let (t, p) = (positive("T", t)?, positive("p", p)?);
    within_t(t, T_MIN, T_MAX_1)?;
    if p > P_MAX {
        return Err(DomainError::AboveMaxPressure { p, p_max: P_MAX }.into());
    }
    let r = R * MOLAR_MASS;
    let (rho, bundle) = bundle_from_gibbs(r, t, p, &region1::evaluate(t, p))?;
    let phase = if p > P_C { Phase::SupercriticalLiquid } else { Phase::Liquid };
    State::from_total(KEY, t, rho, r, MOLAR_MASS, phase, &bundle)
}

/// The state of region 2, the vapour, at `t` (K) and `p` (Pa), from Eq. (15) through core's `bundle_from_gibbs`:
/// 273.15 K ≤ T ≤ 1073.15 K and 0 < p ≤ 100 MPa; as [`region1`], the region a (T, p) belongs to is the model's
/// choice (above 623.15 K the B23 line bounds it, M27's).
pub fn region2(t: f64, p: f64) -> Result<State, Error> {
    let (t, p) = (positive("T", t)?, positive("p", p)?);
    within_t(t, T_MIN, T_MAX_2)?;
    if p > P_MAX {
        return Err(DomainError::AboveMaxPressure { p, p_max: P_MAX }.into());
    }
    let r = R * MOLAR_MASS;
    let (rho, bundle) = bundle_from_gibbs(r, t, p, &region2::evaluate(t, p))?;
    let phase = match (t > T_C, p > P_C) {
        (false, _) => Phase::Gas,
        (true, false) => Phase::SupercriticalGas,
        (true, true) => Phase::Supercritical,
    };
    State::from_total(KEY, t, rho, r, MOLAR_MASS, phase, &bundle)
}

/// The saturation pressure at `t` (K), Pa: Eq. (30), from 273.15 K to the critical temperature, 647.096 K.
pub fn saturation_pressure(t: f64) -> Result<f64, Error> {
    let t = positive("T", t)?;
    within_t(t, T_MIN, T_C)?;
    Ok(region4::pressure(t))
}

/// The saturation temperature at `p` (Pa), K: Eq. (31), from Eq. (30)'s pressure at 273.15 K, 611.2127 Pa (the
/// release's 611.213 Pa, rounded), to the critical pressure, 22.064 MPa.
pub fn saturation_temperature(p: f64) -> Result<f64, Error> {
    let p = positive("p", p)?;
    let p_min = region4::pressure(T_MIN);
    if p < p_min {
        return Err(DomainError::BelowMinPressure { p, p_min }.into());
    }
    if p > P_C {
        return Err(DomainError::AboveMaxPressure { p, p_max: P_C }.into());
    }
    Ok(region4::temperature(p))
}

/// `value` if it is finite and positive, else the typed refusal naming `quantity`.
fn positive(quantity: &'static str, value: f64) -> Result<f64, Error> {
    if value.is_finite() && value > 0.0 { Ok(value) } else { Err(Error::InvalidInput { quantity, value }) }
}

/// `t` within [`t_min`, `t_max`], or the domain error naming the bound it crosses.
fn within_t(t: f64, t_min: f64, t_max: f64) -> Result<(), Error> {
    if t < t_min {
        return Err(DomainError::BelowMinTemperature { t, t_min }.into());
    }
    if t > t_max {
        return Err(DomainError::AboveMaxTemperature { t, t_max }.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
