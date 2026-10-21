//! The wet-bulb temperature (PLAN.md M18.4), the ice-bulb below the triple point: adiabatic saturation, the T_wb at
//! which saturated moist air, having taken up the stream's water as condensed water at T_wb, carries the stream's
//! enthalpy (v8.0.0's `WetBulbSolver`, map 07 §3.3), per kilogram of dry air:
//! h(T, p, W) = h_s(T_wb, p) + (W − W_s(T_wb, p))·h_c(T_wb, p),
//! h_c the condensed phase's h per kilogram of water at (T_wb, p) from its part: under `Corrected` the IF97 liquid and
//! the ice, under `Parity` v8.0.0's, IAPWS-95 at IF97's density and the ice. `HumidAir::new` keeps every part in its
//! model's own gauge, so no caller's reference state reaches h_c (ROT-104).
//!
//! The balance jumps at the triple point, where the wick turns from ice to liquid: by W − W_s times the heat of fusion.
//! The wet bulb is the liquid's root where one exists, else the ice's. A stream wetter than saturated air at the
//! triple point always has the liquid's (its wet bulb lies between its dew point and its own T, both above the triple
//! point), so the ice's is only sought where W < W_s(273.16 K), and there the jump is downwards and the ice's root
//! exists. As such a stream cools, T_wb falls to the triple point and then jumps to the ice's root below it: the
//! wet-bulb temperatures between are the triple point's gap, which no state reaches (v8.0.0's #2906), and an input in
//! it is refused (`given`); at 1 atm and W = 0.002 the gap is about a third of a kelvin wide.

use phasekit_core::{Basis, DataSet, Density, Error, Input, Temperature};

use crate::moist_air::T_MIN;
use crate::saturation::{T_TRIPLE, at};
use crate::{HumidAir, MoistAir, Per, root};

/// The steps [`top`] may take towards pure water vapour, each halving 1 − ψ_w.
const TOP_STEPS: u16 = 40;

/// The condensed water at the bulb.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wick {
    Liquid,
    Ice,
}

/// The condensed phase's h at (`t`, `p`), J per kilogram of water.
fn condensed_h(air: &HumidAir, wick: Wick, t: f64, p: f64) -> Result<f64, Error> {
    let parts = air.parts();
    if wick == Wick::Ice {
        return Ok(at(&parts.ice, t, p)?.h(Basis::Mass));
    }
    let liquid = at(&parts.liquid, t, p)?;
    match air.data_set() {
        DataSet::Parity => {
            let density = Density::molar(liquid.rho(Basis::Molar))?;
            Ok(parts.water.state(Input::dt(density, Temperature::new(t)?))?.h(Basis::Mass))
        }
        _ => Ok(liquid.h(Basis::Mass)),
    }
}

/// The balance at a bulb temperature `twb` for a stream of humidity ratio `w` and enthalpy `h` (J per kilogram of dry
/// air), with `wick`: the right-hand side less the left. It rises with `twb` on either side of the triple point.
fn balance(air: &HumidAir, (p, w, h): (f64, f64, f64), wick: Wick, twb: f64) -> Result<f64, Error> {
    let saturated = air.moist_air(twb, p, air.saturation(twb, p)?.psi_ws)?;
    let w_s = saturated.humidity_ratio();
    Ok(saturated.h(Per::KgDryAir) + (w - w_s) * condensed_h(air, wick, twb, p)? - h)
}

/// The balance's root over `range` with `wick`, narrowed first to where saturated air at p has a state (at high p
/// the virial equation has none at low T); `BelowMinTemperature` where even the lowest is too warm, an `InvalidState`
/// where the top is too cold (air wetter than saturated, which RP-1485 does not model).
fn solved(air: &HumidAir, stream: (f64, f64, f64), wick: Wick, range: (f64, f64)) -> Result<f64, Error> {
    let f = |twb: f64| balance(air, stream, wick, twb);
    let Some((lo, hi)) = root::narrowed(|twb| f(twb).is_ok(), range) else {
        return f(range.0);
    };
    if let Some(twb) = root::bracketed(f, (lo, hi))? {
        return Ok(twb);
    }
    if f(lo)? > 0.0 {
        return Err(phasekit_core::DomainError::BelowMinTemperature { t: lo, t_min: lo }.into());
    }
    Err(Error::InvalidState { reason: "a wet bulb needs air no wetter than saturated" })
}

/// The bracket's top: the stream's own T, where water would not boil there; else the dew point of ever wetter air,
/// ψ_w halfway to 1 each step, until the balance turns positive (near pure vapour at 10 MPa the virial equation has no
/// state, so the top cannot simply be water's boiling point).
fn top(air: &HumidAir, state: &MoistAir, stream: (f64, f64, f64)) -> Result<f64, Error> {
    let (t, p) = (state.t(), state.p());
    if air.saturation(t, p)?.psi_ws < 1.0 {
        return Ok(t);
    }
    let mut psi = state.mole_fraction();
    for _ in 0..TOP_STEPS {
        psi = 0.5 * (1.0 + psi);
        let twb = air.dew_point(p, psi)?;
        let wick = if twb < T_TRIPLE { Wick::Ice } else { Wick::Liquid };
        if balance(air, stream, wick, twb)? > 0.0 {
            return Ok(twb);
        }
    }
    Err(Error::NoConvergence { strategy: phasekit_core::Strategy::External, iterations: TOP_STEPS })
}

/// The wet-bulb temperature of `state` (see the module's notes).
pub(crate) fn wet_bulb(air: &HumidAir, state: &MoistAir) -> Result<f64, Error> {
    let p = state.p();
    let stream = (p, state.humidity_ratio(), state.h(Per::KgDryAir));
    let hi = top(air, state, stream)?;
    if hi <= T_TRIPLE {
        return solved(air, stream, Wick::Ice, (T_MIN, hi));
    }
    if balance(air, stream, Wick::Liquid, T_TRIPLE)? <= 0.0 {
        return solved(air, stream, Wick::Liquid, (T_TRIPLE, hi));
    }
    solved(air, stream, Wick::Ice, (T_MIN, T_TRIPLE))
}

impl HumidAir {
    /// The wet-bulb temperature of `state`, K: the ice-bulb below the triple point (see the module's notes).
    pub fn wet_bulb(&self, state: &MoistAir) -> Result<f64, Error> {
        wet_bulb(self, state)
    }
}
