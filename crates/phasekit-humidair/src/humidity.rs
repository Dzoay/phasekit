//! The humidity of a moist-air state, as a caller gives it, and its mole fraction of water ψ_w (PLAN.md M18.2), as
//! RP-1485 relates them (v8.0.0's `MoleFractionWater` and `RelativeHumidity`, map 07 §3.3): W = εψ/(1 − ψ),
//! ε = M_w/M_a; φ = ψ p/(f p_ws); at the dew or frost point T_dp, ψ = f(T_dp, p) p_ws(T_dp)/p.

use phasekit_core::{DataSet, DomainError, Error};

use crate::HumidAir;
use crate::constants::{M_AIR, M_WATER};

/// A moist-air state's humidity.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Humidity {
    /// The humidity ratio W, kg of water per kg of dry air.
    Ratio(f64),
    /// The mole fraction of water ψ_w.
    MoleFraction(f64),
    /// The relative humidity φ, 0 to 1.
    Relative(f64),
    /// The dew point (over liquid water) or frost point (over ice, below the triple point), K.
    DewPoint(f64),
}

/// ε = M_w/M_a under `data`: v8.0.0 rounds it to 0.621945 (`HumidAirProp.cpp:110`), 5.1e-7 below the ratio of the
/// molar masses it takes elsewhere (DIV-0068).
pub(crate) fn epsilon(data: DataSet) -> f64 {
    match data {
        DataSet::Parity => 0.621_945,
        _ => M_WATER / M_AIR,
    }
}

/// The humidity ratio of mole fraction ψ under `data`: εψ/(1 − ψ).
pub(crate) fn ratio(data: DataSet, psi: f64) -> f64 {
    epsilon(data) * psi / (1.0 - psi)
}

/// `value` within [0, `max`], else the refusal naming `quantity`.
fn checked(quantity: &'static str, value: f64, max: f64) -> Result<f64, Error> {
    if (0.0..=max).contains(&value) { Ok(value) } else { Err(Error::InvalidInput { quantity, value }) }
}

impl HumidAir {
    /// ψ_w at `t` (K) and `p` (Pa) for `humidity`: W ≥ 0, 0 ≤ ψ_w < 1, 0 ≤ φ ≤ 1, a dew point no warmer than T.
    pub fn mole_fraction(&self, t: f64, p: f64, humidity: Humidity) -> Result<f64, Error> {
        match humidity {
            Humidity::Ratio(w) => Ok(checked("humidity ratio", w, f64::INFINITY)? / (epsilon(self.data_set()) + w)),
            Humidity::MoleFraction(psi) => checked("mole fraction of water", psi, 1.0 - f64::EPSILON),
            Humidity::Relative(phi) => Ok(checked("relative humidity", phi, 1.0)? * self.saturation(t, p)?.psi_ws),
            Humidity::DewPoint(t_dp) => {
                if t_dp > t {
                    return Err(DomainError::AboveMaxTemperature { t: t_dp, t_max: t }.into());
                }
                Ok(self.saturation(t_dp, p)?.psi_ws)
            }
        }
    }

    /// The relative humidity of moist air at `t` (K) and `p` (Pa) with mole fraction of water `psi`: ψ/ψ_ws.
    pub fn relative_humidity(&self, t: f64, p: f64, psi: f64) -> Result<f64, Error> {
        Ok(psi / self.saturation(t, p)?.psi_ws)
    }

    /// The humidity ratio of moist air with mole fraction of water `psi`, kg/kg: εψ/(1 − ψ).
    pub fn humidity_ratio(&self, psi: f64) -> f64 {
        ratio(self.data_set(), psi)
    }
}
