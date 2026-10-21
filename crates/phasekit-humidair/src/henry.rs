//! Henry's constant of dry air in water (PLAN.md M18.2): each gas's from IAPWS G7-04 Eq. (3) over the solvent's vapour
//! pressure, and dry air's β_H as v8.0.0's `HenryConstant` composes them (map 07 §3.3), 1/(1.01325·k_H,air) with
//! 1/k_H,air = Σ xᵢ/k_H,i. Under `Corrected` the vapour pressure is G7-04's own, Wagner and Pruss's Eq. (4), and the
//! fractions Lemmon's (`AIR_FRACTIONS`); under `Parity` v8.0.0's: IF97's p_ws and 0.7812, 0.2095 and 0.0093.

use phasekit_core::{DataSet, DomainError, Error, math};

use crate::constants::AIR_FRACTIONS;

/// A gas of dry air.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Gas {
    /// Nitrogen.
    N2,
    /// Oxygen.
    O2,
    /// Argon.
    Ar,
}

/// T_c and p_c of H₂O, K and Pa (G7-04 §2, IAPWS's).
const T_C: f64 = 647.096;
const P_C: f64 = 22.064e6;

/// G7-04 Table 1's aᵢ and bᵢ for H₂O (Wagner and Pruss 1993).
const VAPOUR: [(f64, f64); 6] = [
    (-7.859_517_83, 1.0),
    (1.844_082_59, 1.5),
    (-11.786_649_7, 3.0),
    (22.680_741_1, 3.5),
    (-15.961_871_9, 4.0),
    (1.801_225_02, 7.5),
];

/// G7-04 Table 2's A, B and C.
const fn abc(gas: Gas) -> [f64; 3] {
    match gas {
        Gas::N2 => [-9.675_78, 4.721_62, 11.705_85],
        Gas::O2 => [-9.448_33, 4.438_22, 11.420_05],
        Gas::Ar => [-8.409_54, 4.295_87, 10.527_79],
    }
}

/// v8.0.0's fractions of N₂, O₂ and Ar in its Henry's constant (`HumidAirProp.cpp:813`).
const PARITY_FRACTIONS: [f64; 3] = [0.7812, 0.2095, 0.0093];

/// T_R = T/T_c and τ = 1 − T_R, refused above T_c, where Eq. (3) has no τ^0.355.
fn reduced(t: f64) -> Result<(f64, f64), Error> {
    if t > T_C {
        return Err(DomainError::AboveMaxTemperature { t, t_max: T_C }.into());
    }
    Ok((t / T_C, 1.0 - t / T_C))
}

/// ln(k_H/p₁*), G7-04 Eq. (3).
fn ln_ratio(gas: Gas, t_r: f64, tau: f64) -> f64 {
    let [a, b, c] = abc(gas);
    a / t_r + b * math::powf(tau, 0.355) / t_r + c * math::powf(t_r, -0.41) * math::exp(tau)
}

/// The vapour pressure of H₂O at `t` (K), Pa: G7-04 Eq. (4), Wagner and Pruss's.
fn vapour_pressure(t_r: f64, tau: f64) -> f64 {
    P_C * math::exp(VAPOUR.iter().map(|&(a, b)| a * math::powf(tau, b)).sum::<f64>() / t_r)
}

/// Henry's constant of `gas` in H₂O at `t` (K), Pa: G7-04 Eq. (3) over Eq. (4)'s vapour pressure, to T_c.
pub fn henry_constant(gas: Gas, t: f64) -> Result<f64, Error> {
    let (t_r, tau) = reduced(t)?;
    Ok(vapour_pressure(t_r, tau) * math::exp(ln_ratio(gas, t_r, tau)))
}

/// Dry air's β_H at `t` under `data`, 1/Pa; `p_ws` is IF97's saturation pressure there, `Parity`'s p₁*.
pub(crate) fn beta_h(data: DataSet, t: f64, p_ws: f64) -> Result<f64, Error> {
    let (t_r, tau) = reduced(t)?;
    let (p1, fractions) = match data {
        DataSet::Parity => (p_ws, PARITY_FRACTIONS),
        _ => (vapour_pressure(t_r, tau), AIR_FRACTIONS),
    };
    let inverse: f64 = [Gas::N2, Gas::O2, Gas::Ar]
        .into_iter()
        .zip(fractions)
        .map(|(gas, x)| x / (p1 * math::exp(ln_ratio(gas, t_r, tau))))
        .sum();
    Ok(inverse / 1.01325)
}
