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
pub(crate) enum Wick {
    Liquid,
    Ice,
}

/// The condensed phase's h at (`t`, `p`), J per kilogram of water.
pub(crate) fn condensed_h(air: &HumidAir, wick: Wick, t: f64, p: f64) -> Result<f64, Error> {
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

/// Saturated air at a bulb temperature with its wick, what a stream's balance there takes: its h and W per kilogram of
/// dry air, and the condensed water's h.
pub(crate) struct Bulb {
    h_s: f64,
    w_s: f64,
    h_c: f64,
}

impl Bulb {
    /// The bulb at `twb` and `p` with `wick`.
    fn with(air: &HumidAir, p: f64, twb: f64, wick: Wick) -> Result<Bulb, Error> {
        let saturated = air.moist_air(twb, p, air.saturation(twb, p)?.psi_ws)?;
        let (h_s, w_s) = (saturated.h(Per::KgDryAir), saturated.humidity_ratio());
        Ok(Bulb { h_s, w_s, h_c: condensed_h(air, wick, twb, p)? })
    }

    /// The bulb at `twb` and `p`, its wick liquid at and above the triple point and ice below: what the solves for a
    /// given wet bulb take (`given`), so that each step costs a state, not a wet bulb.
    pub(crate) fn new(air: &HumidAir, p: f64, twb: f64) -> Result<Bulb, Error> {
        Bulb::with(air, p, twb, if twb < T_TRIPLE { Wick::Ice } else { Wick::Liquid })
    }

    /// The balance of a stream of humidity ratio `w` and enthalpy `h` (J per kilogram of dry air): the right-hand side
    /// less the left, zero where the bulb's temperature is the stream's wet bulb.
    pub(crate) fn balance(&self, (w, h): (f64, f64)) -> f64 {
        self.h_s + (w - self.w_s) * self.h_c - h
    }
}

/// The balance at a bulb temperature `twb` for a stream of humidity ratio `w` and enthalpy `h` (J per kilogram of dry
/// air), with `wick`: the right-hand side less the left. It rises with `twb` on either side of the triple point.
fn balance(air: &HumidAir, (p, w, h): (f64, f64, f64), wick: Wick, twb: f64) -> Result<f64, Error> {
    Ok(Bulb::with(air, p, twb, wick)?.balance((w, h)))
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
    // A balance of exactly zero at the bottom is a root, which `bracketed` has returned already.
    // equivalent mutant: replace > with >=
    if f(lo)? > 0.0 {
        return Err(phasekit_core::DomainError::BelowMinTemperature { t: lo, t_min: lo }.into());
    }
    Err(Error::InvalidState { reason: "a wet bulb needs air no wetter than saturated" })
}

/// The bracket's top: the stream's own T, where water would not boil there; else the dew point of ever wetter air,
/// ψ_w halfway to 1 each step, until the balance turns positive (near pure vapour at 10 MPa the virial equation has no
/// state, so the top cannot simply be water's boiling point).
pub(crate) fn top(air: &HumidAir, state: &MoistAir, stream: (f64, f64, f64)) -> Result<f64, Error> {
    let (t, p) = (state.t(), state.p());
    // ψ_ws is exactly 1 only at water's boiling point itself, which no stream's T reaches to the bit.
    // equivalent mutant: replace < with <=
    if air.saturation(t, p)?.psi_ws < 1.0 {
        return Ok(t);
    }
    let mut psi = state.mole_fraction();
    for _ in 0..TOP_STEPS {
        psi = 0.5 * (1.0 + psi);
        let twb = air.dew_point(p, psi)?;
        // A dew point falls on 273.16 K to the bit for no input but by construction.
        // equivalent mutant: replace < with <=
        let wick = if twb < T_TRIPLE { Wick::Ice } else { Wick::Liquid };
        // A balance of exactly zero is the wet bulb itself, which the bracket takes as its top or finds below the next.
        // equivalent mutant: replace > with >=
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
