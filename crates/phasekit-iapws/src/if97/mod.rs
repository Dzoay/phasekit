//! IAPWS-IF97, the industrial formulation for water and steam (IAPWS R7-97(2012)): region 1 (the liquid), region 2
//! (the vapour), region 3 (near the critical point), region 4 (the saturation line), region 5 (the high-temperature
//! steam) and the B23 line between regions 2 and 3, and IF97 water as a `Fluid` below 623.15 K (PLAN.md M14.2, M14.3,
//! M27.1, M27.2).

use phasekit_core::{Basis, DomainError, Error, ModelKey, Phase, Quantity, State, bundle_from_gibbs, math};

mod region1;
mod region2;
mod region3;
mod region4;
mod region5;
mod steam;
mod water;

pub use water::water;

/// IF97's specific gas constant, J/(kg K) (R7-97(2012) Eq. (1)).
pub const R: f64 = 461.526;

/// The molar mass the states carry, kg/mol: IAPWS-95's, since IF97 prints none. Only the molar basis depends on it.
pub const MOLAR_MASS: f64 = 0.018_015_268;

/// The key of every IF97 state (`State::key`).
pub const KEY: ModelKey = ModelKey::from_content(b"IAPWS-IF97 R7-97(2012) water, regions 1, 2 and 4");

/// The critical point IF97 states, K, Pa and kg/m³ (Eqs. (2) to (4)).
const T_C: f64 = 647.096;
const P_C: f64 = 22.064e6;
const RHO_C: f64 = 322.0;

/// Region 1's bounds: 273.15 K ≤ T ≤ 623.15 K, p ≤ 100 MPa (§5's range of validity).
const T_MIN: f64 = 273.15;
const T_MAX_1: f64 = 623.15;
const P_MAX: f64 = 100e6;

/// Region 2's upper temperature, K (§5): its function is valid there, the `Fluid` stops at [`T_MAX_1`]. Region 5
/// starts there.
const T_MAX_2: f64 = 1073.15;

/// Region 5's upper temperature, K, and pressure, Pa (§9).
const T_MAX_5: f64 = 2273.15;
const P_MAX_5: f64 = 50e6;

/// The B23 line's ends: 623.15 K at [`B23_P_MIN`] to 863.15 K at 100 MPa (§4).
const B23_T_MAX: f64 = 863.15;

/// Table 1: n₁ to n₅ of the B23-equation, Eqs. (5) and (6), with p* = 1 MPa and T* = 1 K.
const B23: [f64; 5] = [
    0.348_051_856_289_69e3,
    -0.116_718_598_799_75e1,
    0.101_929_700_393_26e-2,
    0.572_544_598_627_46e3,
    0.139_188_397_788_70e2,
];

/// The state of region 1, the liquid, at `t` (K) and `p` (Pa), from Eq. (7) through core's `bundle_from_gibbs`:
/// 273.15 K ≤ T ≤ 623.15 K and 0 < p ≤ 100 MPa. Which region a (T, p) belongs to is the model's choice, not this
/// function's: below the saturation pressure it still evaluates Eq. (7).
pub fn region1(t: f64, p: f64) -> Result<State, Error> {
    let (t, p) = (positive(Quantity::TEMPERATURE, t)?, positive(Quantity::PRESSURE, p)?);
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
    let (t, p) = (positive(Quantity::TEMPERATURE, t)?, positive(Quantity::PRESSURE, p)?);
    within_t(t, T_MIN, T_MAX_2)?;
    if p > P_MAX {
        return Err(DomainError::AboveMaxPressure { p, p_max: P_MAX }.into());
    }
    let r = R * MOLAR_MASS;
    let (rho, bundle) = bundle_from_gibbs(r, t, p, &region2::REGION2.evaluate(t, p))?;
    let phase = match (t > T_C, p > P_C) {
        (false, _) => Phase::Gas,
        (true, false) => Phase::SupercriticalGas,
        (true, true) => Phase::Supercritical,
    };
    State::from_total(KEY, t, rho, r, MOLAR_MASS, phase, &bundle)
}

/// The state of region 3 at density `rho` (kg/m³) and `t` (K), from Eq. (28): φ's bundle, through
/// `State::from_total`. Its temperatures are region 3's, 623.15 K to 863.15 K, where the B23 line meets 100 MPa; its
/// pressure, between the B23 line and 100 MPa (§7), is the `Fluid`'s to check, since a density alone does not know it.
/// The phase is labelled by T and p against the critical point, and below both by ρ against ρ_c.
pub fn region3(rho: f64, t: f64) -> Result<State, Error> {
    let (rho, t) = (positive(Quantity::density(Basis::Mass), rho)?, positive(Quantity::TEMPERATURE, t)?);
    within_t(t, T_MAX_1, B23_T_MAX)?;
    let bundle = region3::evaluate(rho, t);
    let p = rho * R * t * bundle.a01;
    let phase = match (t > T_C, p > P_C) {
        (true, true) => Phase::Supercritical,
        (true, false) => Phase::SupercriticalGas,
        (false, true) => Phase::SupercriticalLiquid,
        (false, false) if rho > RHO_C => Phase::Liquid,
        (false, false) => Phase::Gas,
    };
    State::from_total(KEY, t, rho / MOLAR_MASS, R * MOLAR_MASS, MOLAR_MASS, phase, &bundle)
}

/// The state of region 5, the high-temperature steam, at `t` (K) and `p` (Pa), from Eq. (32) through core's
/// `bundle_from_gibbs`: 1073.15 K ≤ T ≤ 2273.15 K and 0 < p ≤ 50 MPa, below the 611.213 Pa v8.0.0 stops at too
/// (ROT-242).
pub fn region5(t: f64, p: f64) -> Result<State, Error> {
    let (t, p) = (positive(Quantity::TEMPERATURE, t)?, positive(Quantity::PRESSURE, p)?);
    within_t(t, T_MAX_2, T_MAX_5)?;
    if p > P_MAX_5 {
        return Err(DomainError::AboveMaxPressure { p, p_max: P_MAX_5 }.into());
    }
    let r = R * MOLAR_MASS;
    let (rho, bundle) = bundle_from_gibbs(r, t, p, &region5::REGION5.evaluate(t, p))?;
    let phase = if p > P_C { Phase::Supercritical } else { Phase::SupercriticalGas };
    State::from_total(KEY, t, rho, r, MOLAR_MASS, phase, &bundle)
}

/// The pressure of the B23 line, the boundary between regions 2 and 3, at `t` (K), Pa: Eq. (5), from 623.15 K to
/// 863.15 K.
pub fn b23_pressure(t: f64) -> Result<f64, Error> {
    let t = positive(Quantity::TEMPERATURE, t)?;
    within_t(t, T_MAX_1, B23_T_MAX)?;
    Ok(b23_p(t))
}

/// The temperature of the B23 line at `p` (Pa), K: Eq. (6), from Eq. (5)'s pressure at 623.15 K, 16.5292 MPa, to
/// 100 MPa.
pub fn b23_temperature(p: f64) -> Result<f64, Error> {
    let p = positive(Quantity::PRESSURE, p)?;
    let p_min = b23_p(T_MAX_1);
    if p < p_min {
        return Err(DomainError::BelowMinPressure { p, p_min }.into());
    }
    if p > P_MAX {
        return Err(DomainError::AboveMaxPressure { p, p_max: P_MAX }.into());
    }
    let [_, _, n3, n4, n5] = B23;
    Ok(n4 + math::sqrt((p / 1e6 - n5) / n3))
}

/// Eq. (5): π = n₁ + n₂θ + n₃θ², in Pa.
fn b23_p(t: f64) -> f64 {
    let [n1, n2, n3, _, _] = B23;
    (n1 + n2 * t + n3 * t * t) * 1e6
}

/// The saturation pressure at `t` (K), Pa: Eq. (30), from 273.15 K to the critical temperature, 647.096 K.
pub fn saturation_pressure(t: f64) -> Result<f64, Error> {
    let t = positive(Quantity::TEMPERATURE, t)?;
    within_t(t, T_MIN, T_C)?;
    Ok(region4::pressure(t))
}

/// The saturation temperature at `p` (Pa), K: Eq. (31), from Eq. (30)'s pressure at 273.15 K, 611.2127 Pa (the
/// release's 611.213 Pa, rounded), to the critical pressure, 22.064 MPa.
pub fn saturation_temperature(p: f64) -> Result<f64, Error> {
    let p = positive(Quantity::PRESSURE, p)?;
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
fn positive(quantity: &'static Quantity, value: f64) -> Result<f64, Error> {
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
