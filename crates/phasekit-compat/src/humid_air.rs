//! `HAPropsSI` (PLAN.md M18.6): v8.0.0's humid-air string API (map 07 §2.3) over `phasekit-humidair`, its keys and
//! aliases as v8.0.0's `Name2Type` spells them, byte for byte. The pressure is one of the three inputs; the other two
//! are T, a humidity (W, ψ_w, φ or T_dp), h, s or v per kilogram of dry air, or T_wb, in the pairs `HumidAir::state`
//! takes. Every output comes from the state, one equal to an input too: v8.0.0 returns such an input as given,
//! unchecked (map 07 H10, ROT-165's rule; DIV-0074). Inputs and states out of v8.0.0's range are refused as its
//! `check_bounds` refuses them: T, T_dp and T_wb from 130 K to 350 °C, W to 10, ψ_w to 0.94145.

use phasekit_core::{DataSet, DomainError, Error, Quantity, Registry};
use phasekit_humidair::quantities::{HUMIDITY_RATIO, WATER_MOLE_FRACTION};
use phasekit_humidair::{Given, HumidAir, Humidity, MoistAir, Parts, Per};

use crate::{CompatError, FillPolicy, broadcast};

/// A humid-air key, as v8.0.0's `Name2Type` reads its names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    T,
    P,
    W,
    Psi,
    Rh,
    Tdp,
    Twb,
    H,
    Hha,
    U,
    Uha,
    S,
    Sha,
    V,
    Vha,
    Mu,
    K,
    Cp,
    CpHa,
    Cv,
    CvHa,
    Pw,
    Kappa,
    Sound,
    Z,
}

/// v8.0.0's names, every alias of a key (`HumidAirProp.cpp:1482-1536`).
const NAMES: [(&str, Key); 48] = [
    ("Omega", Key::W),
    ("HumRat", Key::W),
    ("W", Key::W),
    ("psi_w", Key::Psi),
    ("Y", Key::Psi),
    ("Tdp", Key::Tdp),
    ("T_dp", Key::Tdp),
    ("DewPoint", Key::Tdp),
    ("D", Key::Tdp),
    ("Twb", Key::Twb),
    ("T_wb", Key::Twb),
    ("WetBulb", Key::Twb),
    ("B", Key::Twb),
    ("Enthalpy", Key::H),
    ("H", Key::H),
    ("Hda", Key::H),
    ("Hha", Key::Hha),
    ("InternalEnergy", Key::U),
    ("U", Key::U),
    ("Uda", Key::U),
    ("Uha", Key::Uha),
    ("Entropy", Key::S),
    ("S", Key::S),
    ("Sda", Key::S),
    ("Sha", Key::Sha),
    ("RH", Key::Rh),
    ("RelHum", Key::Rh),
    ("R", Key::Rh),
    ("Tdb", Key::T),
    ("T_db", Key::T),
    ("T", Key::T),
    ("P", Key::P),
    ("V", Key::V),
    ("Vda", Key::V),
    ("Vha", Key::Vha),
    ("mu", Key::Mu),
    ("Visc", Key::Mu),
    ("M", Key::Mu),
    ("k", Key::K),
    ("Conductivity", Key::K),
    ("K", Key::K),
    ("C", Key::Cp),
    ("cp", Key::Cp),
    ("Cha", Key::CpHa),
    ("cp_ha", Key::CpHa),
    ("CV", Key::Cv),
    ("CVha", Key::CvHa),
    ("cv_ha", Key::CvHa),
];

/// The keys without an alias, after [`NAMES`].
const PLAIN: [(&str, Key); 4] =
    [("P_w", Key::Pw), ("isentropic_exponent", Key::Kappa), ("speed_of_sound", Key::Sound), ("Z", Key::Z)];

/// v8.0.0's range (`check_bounds`): T, T_dp and T_wb from −143.15 to 350 °C, W to 10, ψ_w to 0.94145.
const T_RANGE: (f64, f64) = (130.0, 623.15);
const W_MAX: f64 = 10.0;
const PSI_MAX: f64 = 0.941_45;

/// The key `name` spells.
fn key(name: &str) -> Option<Key> {
    NAMES.iter().chain(&PLAIN).find(|(spelled, _)| *spelled == name).map(|(_, key)| *key)
}

/// Every name `HAPropsSI` reads, aliases included, in v8.0.0's order.
pub fn ha_names() -> impl Iterator<Item = &'static str> {
    NAMES.iter().chain(&PLAIN).map(|(name, _)| *name)
}

/// `value` of a temperature key within v8.0.0's range.
fn temperature(value: f64) -> Result<f64, Error> {
    let (t_min, t_max) = T_RANGE;
    if value.is_nan() || value < t_min {
        return Err(DomainError::BelowMinTemperature { t: value, t_min }.into());
    }
    if value > t_max {
        return Err(DomainError::AboveMaxTemperature { t: value, t_max }.into());
    }
    Ok(value)
}

/// `value` within [0, `max`], else the refusal naming `quantity`.
fn at_most(quantity: &'static Quantity, value: f64, max: f64) -> Result<f64, Error> {
    if (0.0..=max).contains(&value) { Ok(value) } else { Err(Error::InvalidInput { quantity, value }) }
}

/// The input `name` with `value` as a [`Given`]; a key `HumidAir::state` does not take is `UnknownInput`.
fn given(name: &str, value: f64) -> Result<Given, CompatError> {
    let unknown = || CompatError::UnknownInput(name.into());
    Ok(match key(name).ok_or_else(unknown)? {
        Key::T => Given::Temperature(temperature(value)?),
        Key::W => Given::Humidity(Humidity::Ratio(at_most(HUMIDITY_RATIO, value, W_MAX)?)),
        Key::Psi => Given::Humidity(Humidity::MoleFraction(at_most(WATER_MOLE_FRACTION, value, PSI_MAX)?)),
        Key::Rh => Given::Humidity(Humidity::Relative(value)),
        Key::Tdp => Given::Humidity(Humidity::DewPoint(temperature(value)?)),
        Key::Twb => Given::WetBulb(temperature(value)?),
        Key::H => Given::Enthalpy(value),
        Key::S => Given::Entropy(value),
        Key::V => Given::Volume(value),
        _ => return Err(unknown()),
    })
}

/// An input: its name and its value.
type Named<'a> = (&'a str, f64);

/// The pressure among the three inputs and the other two, in the call's order.
fn split(inputs: [Named<'_>; 3]) -> Result<(f64, [Named<'_>; 2]), CompatError> {
    let refused = CompatError::Core(Error::InvalidState { reason: "HAPropsSI needs the pressure among its inputs" });
    let at = inputs.iter().position(|(name, _)| key(name) == Some(Key::P)).ok_or(refused)?;
    let [a, b, c] = inputs;
    let (p, others) = match at {
        0 => (a, [b, c]),
        1 => (b, [a, c]),
        _ => (c, [a, b]),
    };
    Ok((p.1, others))
}

/// The state the three inputs name.
fn state(air: &HumidAir, inputs: [Named<'_>; 3]) -> Result<MoistAir, CompatError> {
    let (p, [(name1, value1), (name2, value2)]) = split(inputs)?;
    let state = air.state(p, given(name1, value1)?, given(name2, value2)?)?;
    at_most(WATER_MOLE_FRACTION, state.mole_fraction(), PSI_MAX)?;
    Ok(state)
}

/// `key`'s value at `state`.
fn output(air: &HumidAir, state: &MoistAir, key: Key) -> Result<f64, Error> {
    let (t, p, psi) = (state.t(), state.p(), state.mole_fraction());
    let (da, ha) = (Per::KgDryAir, Per::KgHumidAir);
    Ok(match key {
        Key::T => t,
        Key::P => p,
        Key::W => state.humidity_ratio(),
        Key::Psi => psi,
        Key::Rh => air.relative_humidity(t, p, psi)?,
        Key::Tdp => air.dew_point(p, psi)?,
        Key::Twb => air.wet_bulb(state)?,
        Key::H => state.h(da),
        Key::Hha => state.h(ha),
        Key::U => state.u(da),
        Key::Uha => state.u(ha),
        Key::S => state.s(da),
        Key::Sha => state.s(ha),
        Key::V => state.v(da),
        Key::Vha => state.v(ha),
        Key::Mu => air.viscosity(state)?,
        Key::K => air.conductivity(state)?,
        Key::Cp => state.cp(da),
        Key::CpHa => state.cp(ha),
        Key::Cv => state.cv(da),
        Key::CvHa => state.cv(ha),
        Key::Pw => state.partial_pressure(),
        Key::Kappa => state.isentropic_exponent(),
        Key::Sound => state.speed_of_sound(),
        Key::Z => state.z(),
    })
}

/// Humid air over `registry`'s `Water` and `Air` and `phasekit-iapws`'s IF97 water and ice, under `data`, as
/// `HAPropsSI` takes it (PLAN.md M18.6): `data` is the registry's own, which a registry does not report.
pub fn humid_air_in(registry: &Registry, data: DataSet) -> Result<HumidAir, CompatError> {
    let water = registry.get("Water")?.clone();
    let air = registry.get("Air")?.clone();
    let (liquid, ice) = (phasekit_iapws::if97::water()?, phasekit_iapws::ice::ice(&water, data)?);
    Ok(HumidAir::new(Parts { water, air, liquid, ice }, data)?)
}

/// The state at `p` and the two named inputs, as `HAPropsSI` reads them (the browser package's `HumidAir.state`).
pub fn ha_state_in(air: &HumidAir, p: f64, input1: (&str, f64), input2: (&str, f64)) -> Result<MoistAir, CompatError> {
    state(air, [("P", p), input1, input2])
}

/// The output `HAPropsSI` names `output_name`, at `state` of `air`.
pub fn ha_output_at(air: &HumidAir, state: &MoistAir, output_name: &str) -> Result<f64, CompatError> {
    let out = key(output_name).ok_or_else(|| CompatError::UnknownOutput(output_name.into()))?;
    Ok(output(air, state, out)?)
}

/// `HAPropsSI(output, name1, value1, name2, value2, name3, value3)` over `air` (see the module's notes).
#[allow(clippy::too_many_arguments, reason = "HAPropsSI's seven arguments")]
pub fn ha_props_si_in(
    air: &HumidAir,
    output_name: &str,
    name1: &str,
    value1: f64,
    name2: &str,
    value2: f64,
    name3: &str,
    value3: f64,
) -> Result<f64, CompatError> {
    let out = key(output_name).ok_or_else(|| CompatError::UnknownOutput(output_name.into()))?;
    let state = state(air, [(name1, value1), (name2, value2), (name3, value3)])?;
    Ok(output(air, &state, out)?)
}

/// `HAPropsSI` over vectors, as [`crate::props_si_vec_in`] is `PropsSI`'s: the keys are checked first, for the whole
/// call; a length-1 operand broadcasts; any other length mismatch is `Error::Shape`; each point is the scalar call's
/// value, bit for bit, and a failed point follows `fill`.
#[allow(clippy::too_many_arguments, reason = "HAPropsSI's seven arguments and the fill policy")]
pub fn ha_props_si_vec_in(
    air: &HumidAir,
    output_name: &str,
    (name1, values1): (&str, &[f64]),
    (name2, values2): (&str, &[f64]),
    (name3, values3): (&str, &[f64]),
    fill: FillPolicy,
) -> Result<Vec<f64>, CompatError> {
    key(output_name).ok_or_else(|| CompatError::UnknownOutput(output_name.into()))?;
    for name in [name1, name2, name3] {
        key(name).ok_or_else(|| CompatError::UnknownInput(name.into()))?;
    }
    let lengths = [values1.len(), values2.len(), values3.len()];
    let n = lengths.into_iter().max().unwrap_or(0);
    if let Some(&found) = lengths.iter().find(|&&len| len != 1 && len != n) {
        return Err(Error::Shape { expected: n, found }.into());
    }
    let (mut out, mut first) = (Vec::with_capacity(n), None);
    for i in 0..n {
        let (Some(x), Some(y), Some(z)) = (broadcast(values1, i), broadcast(values2, i), broadcast(values3, i)) else {
            break;
        };
        match (ha_props_si_in(air, output_name, name1, x, name2, y, name3, z), fill) {
            (Ok(value), _) => out.push(value),
            (Err(e), FillPolicy::Error) => return Err(e),
            (Err(e), FillPolicy::Inf | FillPolicy::Nan) => {
                first.get_or_insert(e);
                out.push(if fill == FillPolicy::Inf { f64::INFINITY } else { f64::NAN });
            }
        }
    }
    match first {
        Some(e) if out.iter().all(|v| !v.is_finite()) => Err(e),
        _ => Ok(out),
    }
}

/// `HAPropsSI` over the embedded registry under `Corrected`, its humid air built once.
#[cfg(feature = "embedded")]
#[allow(clippy::too_many_arguments, reason = "HAPropsSI's seven arguments")]
pub fn ha_props_si(
    output_name: &str,
    name1: &str,
    value1: f64,
    name2: &str,
    value2: f64,
    name3: &str,
    value3: f64,
) -> Result<f64, CompatError> {
    static AIR: std::sync::LazyLock<Result<HumidAir, CompatError>> =
        std::sync::LazyLock::new(|| humid_air_in(Registry::embedded()?, DataSet::Corrected));
    let air = AIR.as_ref().map_err(Clone::clone)?;
    ha_props_si_in(air, output_name, name1, value1, name2, value2, name3, value3)
}
