//! The ideal-gas parts of humid air's enthalpy and entropy (PLAN.md M18.3), molar: h⁰ = R·T(1 + τα⁰_τ), c_p⁰ =
//! R(1 − τ²α⁰_ττ) and s⁰ = R(τα⁰_τ − α⁰), from a part's α⁰, which is reducing-invariant, plus an offset that places
//! the part on its data set's zero ([`Zeros`]). Dry air's comes from the air part's α⁰ at its own molar volume, with
//! Lemmon's R. Water vapour's comes from the water part's α⁰ at the ideal gas's density at the total pressure (v8.0.0's
//! fix of RP-1485's typo, map 07 §3.3). Under `Corrected`, above the triple point, it comes from IF97 region 2 instead,
//! as RP-1485 takes it (ASHRAE's LibHuAirProp guide §1.3): the IF97 part's vapour at a vanishing pressure.

use phasekit_core::{
    Basis, Constant, DataSet, Error, Fluid, HelmholtzModel, Input, Order, Pressure, Prop, Temperature, math,
};

use crate::Parts;
use crate::saturation::T_TRIPLE;

/// Lemmon's R for dry air, J/(mol K): v8.0.0 takes it for dry air's ideal part and molar volume (map 07 H4).
pub(crate) const R_AIR: f64 = 8.314_51;

/// v8.0.0's R for water vapour's h (RP-1485's) and for its s (IAPWS-95's), J/(mol K) (map 07 H4).
const R_WATER_H: f64 = 8.314_472;
const R_WATER_S: f64 = 8.314_371;

/// Where v8.0.0 anchors its ideal parts, K, and the pressure of water vapour's entropy there, Pa.
const T_REF: f64 = 473.15;
const P_REF: f64 = 101_325.0;

/// v8.0.0's values at [`T_REF`] (`HumidAirProp.cpp:281-326`, map 15 §2.1), to the digits an f64 keeps: water vapour's
/// h (J/mol) and s at [`P_REF`] (J/(mol K)); dry air's h and its s at the molar volume `V_REF_AIR` (m³/mol).
const H_REF_WATER: f64 = 51_885.582_451_893_446;
const S_REF_WATER: f64 = 141.182_978_958_403_03;
const H_REF_AIR: f64 = 13_782.240_592_933_37;
const S_REF_AIR: f64 = 212.223_652_837_593_1;
const V_REF_AIR: f64 = 0.038_837_605_637_863_17;

/// v8.0.0's legacy constants on top of them (`:1083, 1101, 1110`), and its s̄₀ (`MolarEntropy`), which the mixture adds
/// and so each part does.
const LEGACY_H_WATER: f64 = -0.011_023_038_06;
const LEGACY_H_AIR: f64 = -7_914.149_298;
const LEGACY_S_AIR: f64 = -196.137_581_5;
const S_0: f64 = 0.023_664_274_95;

/// The pressure at which the IF97 part's vapour stands for its ideal gas, Pa: its residual h is ~1e-8 J/mol there.
const P_IDEAL: f64 = 1e-6;

/// What each part's ideal h and s add to place it on its data set's zero, J/mol and J/(mol K).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Zeros {
    pub(crate) h_water: f64,
    pub(crate) s_water: f64,
    pub(crate) h_air: f64,
    pub(crate) s_air: f64,
}

/// A part's ideal-gas molar enthalpy, its T-derivative and its molar entropy.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Part {
    pub(crate) h: f64,
    pub(crate) cp: f64,
    pub(crate) s: f64,
}

/// `fluid`'s Helmholtz model.
fn helmholtz(fluid: &Fluid) -> Result<&dyn HelmholtzModel, Error> {
    fluid.model().helmholtz().ok_or(Error::NoModel { prop: Prop::Constant(Constant::GasConstant) })
}

/// The ideal part of `eos` at `t` and molar density `rho` with gas constant `r_h` in h and c_p and `r_s` in s, before
/// any offset.
fn from_alpha0(eos: &dyn HelmholtzModel, [r_h, r_s]: [f64; 2], t: f64, rho: f64) -> Part {
    let d = eos.ideal(t, rho, Order::Two);
    let at = |i, j| d.get(i, j).unwrap_or(f64::NAN);
    Part { h: r_h * t * (1.0 + at(1, 0)), cp: r_h * (1.0 - at(2, 0)), s: r_s * (at(1, 0) - at(0, 0)) }
}

/// Water vapour's gas constants in h and s: v8.0.0's under `Parity`, the water part's own otherwise.
fn water_r(eos: &dyn HelmholtzModel, data: DataSet) -> [f64; 2] {
    match data {
        DataSet::Parity => [R_WATER_H, R_WATER_S],
        _ => [eos.gas_constant(); 2],
    }
}

/// Water vapour's ideal part from the water part's α⁰ at (T, p), before its offset.
fn water_95(water: &Fluid, data: DataSet, t: f64, p: f64) -> Result<Part, Error> {
    let eos = helmholtz(water)?;
    let r = water_r(eos, data);
    Ok(from_alpha0(eos, r, t, p / (r[1] * t)))
}

/// Water vapour's ideal part from the IF97 part's vapour at [`P_IDEAL`], its s carried to p with IF97's R.
pub(crate) fn water_if97(liquid: &Fluid, t: f64, p: f64) -> Result<Part, Error> {
    let state = liquid.state(Input::pt(Pressure::new(P_IDEAL)?, Temperature::new(t)?))?;
    Ok(Part {
        h: state.h(Basis::Molar),
        cp: state.cp(Basis::Molar)?,
        s: state.s(Basis::Molar) + state.gas_constant() * math::ln(P_IDEAL / p),
    })
}

/// Water vapour's ideal part at (T, p) under `data`, on its zero.
pub(crate) fn water(parts: &Parts, data: DataSet, zeros: &Zeros, t: f64, p: f64) -> Result<Part, Error> {
    let part = match data {
        DataSet::Parity => water_95(&parts.water, data, t, p)?,
        _ if t < T_TRIPLE => water_95(&parts.water, data, t, p)?,
        _ => water_if97(&parts.liquid, t, p)?,
    };
    Ok(Part { h: part.h + zeros.h_water, s: part.s + zeros.s_water, ..part })
}

/// Dry air's ideal part at `t` and its molar volume `v_air`, on its zero.
pub(crate) fn air(air: &Fluid, zeros: &Zeros, t: f64, v_air: f64) -> Result<Part, Error> {
    let part = from_alpha0(helmholtz(air)?, [R_AIR; 2], t, 1.0 / v_air);
    Ok(Part { h: part.h + zeros.h_air, s: part.s + zeros.s_air, ..part })
}

/// v8.0.0's zeros (`ensure_ref_offsets`): each part's value at [`T_REF`] less the α⁰ formula's there, plus the legacy
/// constant and s̄₀.
pub(crate) fn parity_zeros(parts: &Parts) -> Result<Zeros, Error> {
    let water = water_95(&parts.water, DataSet::Parity, T_REF, P_REF)?;
    let air = from_alpha0(helmholtz(&parts.air)?, [R_AIR; 2], T_REF, 1.0 / V_REF_AIR);
    Ok(Zeros {
        h_water: LEGACY_H_WATER + H_REF_WATER - water.h,
        s_water: S_0 + S_REF_WATER - water.s,
        h_air: LEGACY_H_AIR + H_REF_AIR - air.h,
        s_air: S_0 + LEGACY_S_AIR + S_REF_AIR - air.s,
    })
}

/// RP-1485's zeros (`Corrected`) from dry air's molar h and s at 273.15 K and 101.325 kPa before any offset, `dry`:
/// dry air's at zero there; water vapour's on its parts' own zero, the liquid's u and s at the triple point, its s plus
/// s̄₀, which the authors' tables add to the mixture and so to each part (measured: at 90 °C, without it on the water,
/// Table 2's s_s misses by 1.9 J/(kg K)).
pub(crate) fn corrected_zeros([h, s]: [f64; 2]) -> Zeros {
    Zeros { h_water: 0.0, s_water: S_0, h_air: -h, s_air: -s }
}
