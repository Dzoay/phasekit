//! `HumidAir` and the four parts it takes at construction (PLAN.md M18.1): options by value, no global switch and no
//! thread-local copy of a part (ROT-267, ROT-268; v8.0.0 keeps three backends per calling thread and process-wide
//! model flags, map 07 H8, §5).

use phasekit_core::{Constant, DataSet, Error, Fluid, Input, Prop, Quality, Temperature, Virials as PureVirials};

use crate::constants::M_WATER;
use crate::ideal::{self, Zeros};
use crate::moist_air::{self, MoistAir, Per};
use crate::saturation::{self, Saturation};
use crate::virials::{self, Coefficient, Virials};

/// Humid air's four parts.
#[derive(Clone, Debug)]
pub struct Parts {
    /// IAPWS-95 water: the vapour's virials and ideal gas (core's `Water`).
    pub water: Fluid,
    /// Lemmon's dry air: its virials and ideal gas (core's `Air`).
    pub air: Fluid,
    /// IF97 water: the saturation line and the liquid at 0 °C and above (`phasekit_iapws::if97::water`).
    pub liquid: Fluid,
    /// Ice Ih: the sublimation curve and the solid below 0 °C (`phasekit_iapws::ice::ice`).
    pub ice: Fluid,
}

/// RP-1485's humid air over its [`Parts`], under a data set: `Corrected` RP-1485 as published, `Parity` v8.0.0's
/// choices (PLAN.md M18; user decision 2).
#[derive(Clone, Debug)]
pub struct HumidAir {
    parts: Parts,
    data_set: DataSet,
    pub(crate) zeros: Zeros,
}

/// Each part's role in a refusal: which one a construction error names.
const MOLAR_MASS: &str = "molar mass of the liquid or ice part";

/// Where `Corrected` puts dry air's h and s at zero, K and Pa (RP-1485's reference state).
const T_ZERO: f64 = 273.15;
const P_ZERO: f64 = 101_325.0;

impl HumidAir {
    /// Humid air over `parts`, each checked: water and air need exact virials (a Helmholtz model with a closed-form
    /// `zero_density`); the liquid and ice, water's molar mass, and a saturation state each, the liquid's at 300 K
    /// and the ice's at 250 K (the refusal is the part's own).
    pub fn new(parts: Parts, data_set: DataSet) -> Result<HumidAir, Error> {
        for part in [&parts.water, &parts.air] {
            pure_virials(part, 300.0)?;
        }
        for (part, t) in [(&parts.liquid, 300.0), (&parts.ice, 250.0)] {
            let m = part.model().info().molar_mass();
            if m != M_WATER {
                return Err(Error::InvalidInput { quantity: MOLAR_MASS, value: m });
            }
            part.state(Input::qt(Quality::new(0.0)?, Temperature::new(t)?))?;
        }
        let mut air = HumidAir { parts, data_set, zeros: Zeros::default() };
        air.zeros = match data_set {
            DataSet::Parity => ideal::parity_zeros(&air.parts)?,
            _ => {
                let dry = moist_air::state(&air, &air.zeros, T_ZERO, P_ZERO, 0.0)?;
                ideal::corrected_zeros([dry.h(Per::MolHumidAir), dry.s(Per::MolHumidAir)])
            }
        };
        Ok(air)
    }

    /// The parts.
    pub fn parts(&self) -> &Parts {
        &self.parts
    }

    /// The data set.
    pub fn data_set(&self) -> DataSet {
        self.data_set
    }

    /// The seven virial coefficients at `t` (K) with their first two T-derivatives.
    pub fn virials(&self, t: f64) -> Result<Virials, Error> {
        let (air, water) = (pure_virials(&self.parts.air, t)?, pure_virials(&self.parts.water, t)?);
        let b = |v: &PureVirials| Coefficient { value: v.b, d_dt: v.db_dt, d2_dt2: v.d2b_dt2 };
        let c = |v: &PureVirials| Coefficient { value: v.c, d_dt: v.dc_dt, d2_dt2: v.d2c_dt2 };
        Ok(Virials {
            b_aa: b(&air),
            c_aaa: c(&air),
            b_ww: b(&water),
            c_www: c(&water),
            b_aw: virials::b_aw(t),
            c_aaw: virials::c_aaw(t),
            c_aww: virials::c_aww(t),
        })
    }
}

impl HumidAir {
    /// Saturated moist air at `t` (K) and `p` (Pa): water's saturation pressure over the condensed phase, the
    /// enhancement factor and what Eq. (3.25) took (PLAN.md M18.2).
    pub fn saturation(&self, t: f64, p: f64) -> Result<Saturation, Error> {
        saturation::saturation(self, t, p)
    }

    /// The humidity ratio of saturated moist air at `t` (K) and `p` (Pa), kg of water per kg of dry air:
    /// W_s = ε ψ_ws/(1 − ψ_ws), ε = M_w/M_a.
    pub fn saturated_humidity_ratio(&self, t: f64, p: f64) -> Result<f64, Error> {
        Ok(crate::humidity::ratio(self.data_set, self.saturation(t, p)?.psi_ws))
    }

    /// The moist-air state at `t` (K), `p` (Pa) and mole fraction of water `psi` (PLAN.md M18.3), within v8.0.0's
    /// range: 130 to 623.15 K, 10 Pa to 10 MPa.
    pub fn moist_air(&self, t: f64, p: f64, psi: f64) -> Result<MoistAir, Error> {
        moist_air::state(self, &self.zeros, t, p, psi)
    }
}

/// A part's exact virials at `t`, or the refusal that it has none.
fn pure_virials(part: &Fluid, t: f64) -> Result<PureVirials, Error> {
    let missing = || Error::NoModel { prop: Prop::Constant(Constant::GasConstant) };
    let eos = part.model().helmholtz().ok_or_else(missing)?;
    eos.zero_density(t).ok_or(Error::NoModel { prop: Prop::Bvirial })
}
