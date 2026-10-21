//! Saturated moist air at (T, p) (PLAN.md M18.2): the saturation pressure of pure water over the condensed phase,
//! liquid at T ≥ 273.16 K and ice below (one predicate, ROT-263), and the enhancement factor f of RP-1485 Eq. (3.25),
//! which v8.0.0's `f_factor` writes out (map 07 §3.3; the report is not in hand, user decision HA2). Under `Corrected`
//! every part is the one RP-1485 takes (ASHRAE's LibHuAirProp guide §1.3): IF97's saturation pressure, its liquid's
//! molar volume and compressibility, Henry's constant from IAPWS G7-04; under `Parity` v8.0.0's (map 07 H3): IAPWS-95's
//! saturation pressure and liquid volume in f, IAPWS-95's compressibility at IF97's density, IF97's p_ws in Henry's
//! constant with its own fractions of dry air.

use phasekit_core::{Basis, DataSet, Error, Fluid, Input, Pressure, Quality, State, Strategy, Temperature, math};

use crate::henry;
use crate::virials::Virials;

/// The triple point, K: liquid water at and above it, ice below.
pub(crate) const T_TRIPLE: f64 = 273.16;

/// Saturated moist air at one (T, p).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Saturation {
    /// The saturation pressure of pure water over its condensed phase at T, Pa: what ψ, φ and the dew point take.
    pub p_ws: f64,
    /// The enhancement factor: the water vapour's partial pressure at saturation over [`p_ws`](Self::p_ws).
    pub f: f64,
    /// The mole fraction of water in saturated moist air, f·p_ws/p.
    pub psi_ws: f64,
    /// The condensed phase's molar volume, m³/mol.
    pub v_condensed: f64,
    /// The condensed phase's isothermal compressibility, 1/Pa; 0 where p_ws exceeds p (RP-1485 Eq. (3.25)'s note).
    pub kappa_t: f64,
    /// Henry's-law constant of dry air in water, 1/Pa; 0 over ice and where p_ws exceeds p.
    pub beta_h: f64,
}

/// The condensed phase's inputs to Eq. (3.25) at (T, p): p_ws for f, its molar volume, κ_T and β_H.
struct Condensed {
    p_ws: f64,
    v: f64,
    kappa_t: f64,
    beta_h: f64,
}

/// `part`'s state at Q = `q` on its saturation line at T.
fn saturated(part: &Fluid, q: f64, t: f64) -> Result<State, Error> {
    part.state(Input::qt(Quality::new(q)?, Temperature::new(t)?))
}

/// `part`'s state at (T, p).
fn at(part: &Fluid, t: f64, p: f64) -> Result<State, Error> {
    part.state(Input::pt(Pressure::new(p)?, Temperature::new(t)?))
}

/// The saturation pressure of pure water at `t` over the condensed phase: the liquid part's (IF97) at and above the
/// triple point, the ice part's sublimation below.
pub(crate) fn p_ws(liquid: &Fluid, ice: &Fluid, t: f64) -> Result<f64, Error> {
    if t >= T_TRIPLE { Ok(saturated(liquid, 0.0, t)?.p()) } else { Ok(saturated(ice, 1.0, t)?.p()) }
}

/// The parts of Eq. (3.25) the condensed phase gives, under `data`.
fn condensed(parts: &crate::Parts, data: DataSet, t: f64, p: f64, p_ws: f64) -> Result<Condensed, Error> {
    if t < T_TRIPLE {
        let solid = at(&parts.ice, t, p)?;
        let kappa_t = if p_ws > p { 0.0 } else { solid.isothermal_compressibility()? };
        return Ok(Condensed { p_ws, v: 1.0 / solid.rho(Basis::Molar), kappa_t, beta_h: 0.0 });
    }
    let (p_f, v) = match data {
        DataSet::Parity => {
            let liquid = saturated(&parts.water, 0.0, t)?;
            (liquid.p(), 1.0 / liquid.rho(Basis::Molar))
        }
        _ => (p_ws, 1.0 / saturated(&parts.liquid, 0.0, t)?.rho(Basis::Molar)),
    };
    if p_ws > p {
        return Ok(Condensed { p_ws: p_f, v, kappa_t: 0.0, beta_h: 0.0 });
    }
    let liquid = at(&parts.liquid, t, p)?;
    let kappa_t = match data {
        DataSet::Parity => {
            let density = phasekit_core::Density::molar(liquid.rho(Basis::Molar))?;
            parts.water.state(Input::dt(density, Temperature::new(t)?))?.isothermal_compressibility()?
        }
        _ => liquid.isothermal_compressibility()?,
    };
    Ok(Condensed { p_ws: p_f, v, kappa_t, beta_h: henry::beta_h(data, t, p_ws)? })
}

/// Eq. (3.25)'s right-hand side at ψ_ws for the condensed phase `c`, the virials `b` and RT, as v8.0.0's `f_factor`
/// writes its eight lines.
#[allow(clippy::similar_names, reason = "the equation's own names")]
fn rhs(c: &Condensed, b: &Virials, p: f64, rt: f64, psi: f64) -> f64 {
    let (x, (p_ws, v)) = (1.0 - psi, (c.p_ws, c.v));
    let (k, bh) = (c.kappa_t, c.beta_h);
    let (baa, baw, bww) = (b.b_aa.value, b.b_aw.value, b.b_ww.value);
    let (caaa, caaw, caww, cwww) = (b.c_aaa.value, b.c_aaw.value, b.c_aww.value, b.c_www.value);
    let (q, q2) = (p / rt, p * p / (rt * rt));
    let line1 = ((1.0 + k * p_ws) * (p - p_ws) - k * (p * p - p_ws * p_ws) / 2.0) / rt * v + math::ln_1p(-bh * x * p);
    let line2 = x * x * q * baa - 2.0 * x * x * q * baw - (p - p_ws - x * x * p) / rt * bww;
    let line3 = x * x * x * q2 * caaa + 3.0 * x * x * (1.0 - 2.0 * x) * q2 / 2.0 * caaw;
    let line4 =
        -3.0 * x * x * psi * q2 * caww - ((3.0 - 2.0 * psi) * psi * psi * p * p - p_ws * p_ws) / (2.0 * rt * rt) * cwww;
    let line5 = -(x * x * (-2.0 + 3.0 * psi) * psi) * q2 * baa * bww;
    let line6 = -(2.0 * x * x * x * (-1.0 + 3.0 * psi)) * q2 * baa * baw;
    let line7 = 6.0 * x * x * psi * psi * q2 * bww * baw - 3.0 * x * x * x * x * q2 / 2.0 * baa * baa;
    let line8 = -(2.0 * x * x * psi * (-2.0 + 3.0 * psi)) * q2 * baw * baw
        - (p_ws * p_ws - (4.0 - 3.0 * psi) * psi * psi * psi * p * p) / (2.0 * rt * rt) * bww * bww;
    line1 + line2 + line3 + line4 + line5 + line6 + line7 + line8
}

/// The secant's iterations: from f = 1, Eq. (3.25) settles in under ten.
const ITERATIONS: u16 = 50;

/// f from ln f = RHS(f·p_ws/p) by the secant from 1 and 1 + 1e-6, as v8.0.0's `f_factor` starts it, to 4 ulp; at
/// least 1, as v8.0.0 clamps it (`NoConvergence` if it has not settled, ROT-265: v8.0.0 stops silently at 100).
fn enhancement(c: &Condensed, b: &Virials, p: f64, rt: f64) -> Result<f64, Error> {
    let residual = |f: f64| math::ln(f) - rhs(c, b, p, rt, f * c.p_ws / p);
    let (mut x1, mut x2) = (1.0, 1.0 + 1e-6);
    let mut y1 = residual(x1);
    for _ in 0..ITERATIONS {
        let y2 = residual(x2);
        let x3 = x2 - y2 * (x2 - x1) / (y2 - y1);
        if (x3 - x2).abs() <= 4.0 * f64::EPSILON * x2 || y2 == 0.0 {
            return Ok(x3.max(1.0));
        }
        (x1, y1, x2) = (x2, y2, x3);
    }
    Err(Error::NoConvergence { strategy: Strategy::External, iterations: ITERATIONS })
}

/// Saturated moist air at (T, p) over `parts`, under `data` (PLAN.md M18.2).
pub(crate) fn saturation(air: &crate::HumidAir, t: f64, p: f64) -> Result<Saturation, Error> {
    let parts = air.parts();
    let p_ws = p_ws(&parts.liquid, &parts.ice, t)?;
    let c = condensed(parts, air.data_set(), t, p, p_ws)?;
    let r = match air.data_set() {
        DataSet::Parity => 8.314_371,
        _ => crate::constants::R,
    };
    let f = enhancement(&c, &air.virials(t)?, p, r * t)?;
    Ok(Saturation { p_ws, f, psi_ws: f * p_ws / p, v_condensed: c.v, kappa_t: c.kappa_t, beta_h: c.beta_h })
}
