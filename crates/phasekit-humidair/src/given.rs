//! A moist-air state from its pressure and two more inputs (PLAN.md M18.4), as v8.0.0's `HAPropsSI` takes them (map 07
//! §2.3): T and a humidity directly; T and h, s, v or T_wb by a bracketed solve in the humidity; a humidity and h, s,
//! v or T_wb by one in T. A humidity that fixes ψ_w at any T (W, ψ_w or the dew point) also takes φ by a solve in T,
//! as v8.0.0 has done since its #2670, except for dry air, whose φ is 0 at every T. Every other pair is refused.
//!
//! The solve in the humidity runs over ψ_w, from dry air to saturation at T (W, its monotone image, is unbounded where
//! water would boil at p); the one in T from the dew point of ψ_w, or 130 K, to 350 °C, where φ is given, to water's
//! boiling point at p. Each output rises with its unknown there, so a target no state on the range reaches is
//! refused, naming it.

use phasekit_core::{Error, Limit, Quantity};

use crate::moist_air::{T_MAX, T_MIN};
use crate::quantities::{
    DEW_POINT, ENTHALPY_PER_DRY_AIR, ENTROPY_PER_DRY_AIR, HUMIDITY_RATIO, RELATIVE_HUMIDITY, VOLUME_PER_DRY_AIR,
    WATER_MOLE_FRACTION, WET_BULB_TEMPERATURE,
};
use crate::wet_bulb::Bulb;
use crate::{HumidAir, Humidity, MoistAir, Per, root};

/// One of a moist-air state's inputs besides its pressure, with h, s and v per kilogram of dry air, as `HAPropsSI`'s H,
/// S and V are.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Given {
    /// The dry-bulb temperature, K.
    Temperature(f64),
    /// A humidity: W, ψ_w, φ or the dew point.
    Humidity(Humidity),
    /// The enthalpy, J per kilogram of dry air.
    Enthalpy(f64),
    /// The entropy, J/K per kilogram of dry air.
    Entropy(f64),
    /// The volume, m³ per kilogram of dry air.
    Volume(f64),
    /// The wet-bulb (ice-bulb below the triple point) temperature, K.
    WetBulb(f64),
}

/// The mole fraction of water at the top of a solve in it: where water would boil at p, nearly pure vapour.
const PSI_TOP: f64 = 1.0 - 1e-6;

/// How close a solved state's wet bulb must come to the one asked for: a target in the triple point's gap is solved to
/// the jump instead, a gap of 0.1 to 1 K away.
const WET_BULB_MATCH: f64 = 1e-6;

/// The refusal of a pair that fixes no state.
const NO_STATE: Error = Error::InvalidState { reason: "a moist-air state needs T or a humidity, and one more input" };

/// `target`'s value at `state`: h, s, v, T_wb or φ.
fn output(air: &HumidAir, state: &MoistAir, target: Given) -> Result<f64, Error> {
    match target {
        Given::Enthalpy(_) => Ok(state.h(Per::KgDryAir)),
        Given::Entropy(_) => Ok(state.s(Per::KgDryAir)),
        Given::Volume(_) => Ok(state.v(Per::KgDryAir)),
        Given::WetBulb(_) => air.wet_bulb(state),
        Given::Humidity(Humidity::Relative(_)) => air.relative_humidity(state.t(), state.p(), state.mole_fraction()),
        _ => Err(NO_STATE),
    }
}

/// `target`'s value as given, and its quantity for a refusal.
fn wanted(target: Given) -> Result<(f64, &'static Quantity), Error> {
    match target {
        Given::Enthalpy(h) => Ok((h, ENTHALPY_PER_DRY_AIR)),
        Given::Entropy(s) => Ok((s, ENTROPY_PER_DRY_AIR)),
        Given::Volume(v) => Ok((v, VOLUME_PER_DRY_AIR)),
        Given::WetBulb(t) => Ok((t, WET_BULB_TEMPERATURE)),
        Given::Humidity(Humidity::Relative(phi)) => Ok((phi, RELATIVE_HUMIDITY)),
        _ => Err(NO_STATE),
    }
}

/// A humidity as a refusal names the given input.
const fn given_humidity(humidity: Humidity) -> (&'static Quantity, f64) {
    match humidity {
        Humidity::Ratio(w) => (HUMIDITY_RATIO, w),
        Humidity::MoleFraction(psi) => (WATER_MOLE_FRACTION, psi),
        Humidity::Relative(phi) => (RELATIVE_HUMIDITY, phi),
        Humidity::DewPoint(t) => (DEW_POINT, t),
    }
}

/// `value` of `quantity` beyond both ends of a solve's range, whose residuals (the target there less `value`) are
/// `ends`, as `Unreachable` at the end nearer it (PLAN.md M29.2; user decision EM16).
fn unreachable(
    (quantity, value): (&'static Quantity, f64),
    ends: (f64, f64),
    given: (&'static Quantity, f64),
) -> Error {
    let r = if ends.0.abs() <= ends.1.abs() { ends.0 } else { ends.1 };
    let limit = if r < 0.0 { Limit::Max } else { Limit::Min };
    Error::Unreachable { quantity, value, limit, bound: value + r, given }
}

/// `range` narrowed to where `value` has one (`root::narrowed`): at high p the virial equation has no state at low T or
/// high ψ_w, and near pure vapour at 10 MPa Eq. (3.25)'s f, which a wet bulb takes, does not settle. Where neither end
/// has one, the lower end's refusal.
fn existing(value: impl Fn(f64) -> Result<f64, Error>, range: (f64, f64)) -> Result<(f64, f64), Error> {
    match root::narrowed(|x| value(x).is_ok(), range) {
        Some(range) => Ok(range),
        None => value(range.0).map(|_| range),
    }
}

/// The state among `state(x)` for x over `range` at `p` whose `target` is the one given, or the refusal naming it: a
/// target beyond both ends of the range `Unreachable` at the `given` input other than p. A wet bulb is solved on its
/// balance at the given bulb temperature, a state a step (`wet_bulb::Bulb`), then checked: a target in the triple
/// point's gap, or one whose balance has its root on the other wick's side, is refused.
fn solve(
    air: &HumidAir,
    (p, state): (f64, impl Fn(f64) -> Result<MoistAir, Error>),
    (range, given): ((f64, f64), (&'static Quantity, f64)),
    target: Given,
) -> Result<MoistAir, Error> {
    let (value, quantity) = wanted(target)?;
    let bulb = match target {
        Given::WetBulb(twb) => Some(Bulb::new(air, p, twb)?),
        _ => None,
    };
    let residual = |x: f64| {
        let s = state(x)?;
        match &bulb {
            Some(bulb) => Ok(bulb.balance((s.humidity_ratio(), s.h(Per::KgDryAir)))),
            None => Ok(output(air, &s, target)? - value),
        }
    };
    let range = existing(residual, range)?;
    let matches = |s: &MoistAir| output(air, s, target).is_ok_and(|v| (v - value).abs() <= WET_BULB_MATCH);
    let Some(x) = root::bracketed(residual, range)? else {
        // A wet bulb at an end of the range (dry air, saturation) balances there only to its rounding.
        if bulb.is_none() {
            return Err(unreachable((quantity, value), (residual(range.0)?, residual(range.1)?), given));
        }
        let ends = [range.0, range.1].into_iter().filter_map(|x| state(x).ok());
        return ends.into_iter().find(matches).ok_or(Error::InvalidInput { quantity, value });
    };
    let solved = state(x)?;
    if bulb.is_some() && !matches(&solved) {
        return Err(Error::InvalidState { reason: "no state has this wet bulb: it is in the triple point's gap" });
    }
    Ok(solved)
}

/// At T and p, the state whose `target` is the one given, by a solve in ψ_w from dry air to saturation.
fn in_humidity(air: &HumidAir, t: f64, p: f64, target: Given) -> Result<MoistAir, Error> {
    let top = air.saturation(t, p)?.psi_ws.min(PSI_TOP);
    solve(air, (p, |psi| air.moist_air(t, p, psi)), ((0.0, top), (Quantity::TEMPERATURE, t)), target)
}

/// At p, with `humidity`, the state whose `target` is the one given, by a solve in T.
fn in_temperature(air: &HumidAir, p: f64, humidity: Humidity, target: Given) -> Result<MoistAir, Error> {
    if let Humidity::Relative(phi) = humidity {
        if matches!(target, Given::Humidity(_)) {
            return Err(NO_STATE);
        }
        let top = if phi > 0.0 { air.dew_point(p, PSI_TOP)? } else { T_MAX };
        let state = |t: f64| air.moist_air(t, p, air.mole_fraction(t, p, humidity)?);
        return solve(air, (p, state), ((T_MIN, top), given_humidity(humidity)), target);
    }
    if matches!(target, Given::Humidity(h) if !matches!(h, Humidity::Relative(_))) {
        return Err(NO_STATE);
    }
    // W, ψ_w and the dew point each fix ψ_w at any T at or above the dew point.
    let psi = air.mole_fraction(T_MAX, p, humidity)?;
    if psi == 0.0 && matches!(target, Given::Humidity(_)) {
        return Err(Error::InvalidState { reason: "dry air's relative humidity is 0 at every temperature" });
    }
    let bottom = if psi > 0.0 { air.dew_point(p, psi)?.max(T_MIN) } else { T_MIN };
    solve(air, (p, |t| air.moist_air(t, p, psi)), ((bottom, T_MAX), given_humidity(humidity)), target)
}

impl HumidAir {
    /// The moist-air state at `p` (Pa) and the inputs `a` and `b`, in either order (see the module's notes).
    pub fn state(&self, p: f64, a: Given, b: Given) -> Result<MoistAir, Error> {
        match (a, b) {
            (Given::Temperature(t), Given::Humidity(h)) | (Given::Humidity(h), Given::Temperature(t)) => {
                self.moist_air(t, p, self.mole_fraction(t, p, h)?)
            }
            (Given::Temperature(t), target) | (target, Given::Temperature(t)) => in_humidity(self, t, p, target),
            (relative @ Given::Humidity(Humidity::Relative(_)), Given::Humidity(h)) => {
                in_temperature(self, p, h, relative)
            }
            (Given::Humidity(h), target) | (target, Given::Humidity(h)) => in_temperature(self, p, h, target),
            _ => Err(NO_STATE),
        }
    }
}
