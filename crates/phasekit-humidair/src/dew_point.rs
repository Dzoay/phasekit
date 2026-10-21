//! The dew or frost point (PLAN.md M18.4): the T at which saturated moist air at p holds the given ψ_w,
//! f(T, p)·p_ws(T) = ψ_w·p, over liquid water at and above the triple point and over ice below (v8.0.0's
//! `DewpointTemperature`, map 07 §3.3). Dry air has none (ROT-272: v8.0.0's guard is inverted and returns 149.4 K for
//! W = 0).
//!
//! f jumps at the triple point, where the condensed phase turns from liquid (with dissolved air, Henry's term) to ice,
//! so ln(f·p_ws) does too. The dew point is the liquid's root where one exists, at or above the triple point, else
//! the ice's below it; where the jump itself crosses zero, the triple point. The root is bracketed from the saturation
//! temperature at the partial pressure p_w = ψ_w·p, where f ≥ 1 puts it above (ROT-266: v8.0.0 starts its secant at
//! T_sat(p) − 1), down to two Newton steps below it, the slope of ln p_ws from Clapeyron's equation (or to the
//! triple point, or 130 K, where those do not reach), and closed by the Illinois method: f varies with T too much at
//! 10 MPa for Newton alone.

use phasekit_core::{Basis, DomainError, Error, Fluid, Input, Pressure, Quality, math};

use crate::moist_air::{P_MAX, P_MIN, T_MIN};
use crate::saturation::{T_TRIPLE, p_ws, saturated};
use crate::{HumidAir, root};

/// `part`'s temperature on its saturation line at `p`: the liquid's boiling or the ice's sublimation temperature.
fn saturation_temperature(part: &Fluid, p: f64) -> Result<f64, Error> {
    Ok(part.state(Input::pq(Pressure::new(p)?, Quality::new(0.0)?))?.t())
}

/// d(ln p_ws)/dT at `t` on `part`'s saturation line, from Clapeyron's equation over its two saturated phases:
/// (s_v − s_c)/((v_v − v_c)·p_ws).
fn slope(part: &Fluid, t: f64) -> Result<f64, Error> {
    let (condensed, vapour) = (saturated(part, 0.0, t)?, saturated(part, 1.0, t)?);
    let ds = vapour.s(Basis::Molar) - condensed.s(Basis::Molar);
    let dv = 1.0 / vapour.rho(Basis::Molar) - 1.0 / condensed.rho(Basis::Molar);
    Ok(ds / dv / vapour.p())
}

/// The dew or frost point of moist air at `p` with mole fraction of water `psi` (see the module's notes).
pub(crate) fn dew_point(air: &HumidAir, p: f64, psi: f64) -> Result<f64, Error> {
    if p.is_nan() || p < P_MIN {
        return Err(DomainError::BelowMinPressure { p, p_min: P_MIN }.into());
    }
    if p > P_MAX {
        return Err(DomainError::AboveMaxPressure { p, p_max: P_MAX }.into());
    }
    if !(0.0..1.0).contains(&psi) {
        return Err(Error::InvalidInput { quantity: "mole fraction of water", value: psi });
    }
    let parts = air.parts();
    let (p_w, p_min) = (psi * p, p_ws(&parts.liquid, &parts.ice, T_MIN)?);
    if p_w < p_min {
        return Err(DomainError::BelowMinPressure { p: p_w, p_min }.into());
    }
    let ln_ratio = |t: f64| -> Result<f64, Error> {
        let sat = air.saturation(t, p)?;
        Ok(math::ln(sat.f * sat.p_ws / p_w))
    };
    let (part, floor, top) = if ln_ratio(T_TRIPLE)? <= 0.0 {
        (&parts.liquid, T_TRIPLE, saturation_temperature(&parts.liquid, p_w)?)
    } else if p_w < p_ws(&parts.liquid, &parts.ice, T_TRIPLE)? {
        (&parts.ice, T_MIN, saturation_temperature(&parts.ice, p_w)?)
    } else {
        (&parts.ice, T_MIN, T_TRIPLE)
    };
    // f is at least 1, so ln(f·p_ws/p_w) ≥ 0 at the top but for IF97's Eqs. (30) and (31), which invert each other to
    // 1e-11: where f is 1 (nearly pure vapour), the top can fall a hair short, and a Newton step puts it past the root.
    let (slope_top, ratio) = (slope(part, top)?, ln_ratio(top)?);
    let top = if ratio < 0.0 { top - 2.0 * ratio / slope_top } else { top };
    let guess = (top - 2.0 * ln_ratio(top)? / slope_top).max(floor);
    let foot = if ln_ratio(guess)? < 0.0 { guess } else { floor };
    root::bracketed(ln_ratio, (foot, top))?
        .ok_or(Error::Domain(DomainError::BelowMinTemperature { t: T_MIN, t_min: T_MIN }))
}

impl HumidAir {
    /// The dew point (over liquid water) or frost point (over ice, below the triple point) of moist air at `p` (Pa)
    /// with mole fraction of water `psi`, K; it does not depend on the air's own temperature. `BelowMinPressure` names
    /// the water's partial pressure where it is below the saturation pressure at 130 K, the range's foot, dry air
    /// included (ROT-272).
    pub fn dew_point(&self, p: f64, psi: f64) -> Result<f64, Error> {
        dew_point(self, p, psi)
    }
}
