//! Ice Ih, IAPWS R10-06(2009), as a `Fluid` (PLAN.md M14.4; user decision HA1): its Gibbs energy through core's
//! `bundle_from_gibbs`, every state `Phase::Solid`, on IAPWS-95's zero of u and s (the release's "IAPWS-95" s₀), from
//! 0 K to 273.16 K and to 210 MPa (§5). It borrows a water `Fluid` for its molar mass and gas constant, and for the
//! vapour side of its sublimation states (M14.5). Named in no compat string (HA5).

use std::sync::Arc;

use phasekit_core::{
    Capabilities, Constant, DataSet, DataTerms, DomainError, Error, FlashOptions, Fluid, FluidInfo, GibbsDerivs,
    Limits, ModelKey, NativeInput, Pair, Phase, Prop, Source, State, ThermoModel, bundle_from_gibbs,
};

mod gibbs;

/// The highest pressure of ice Ih's equation, Pa (§5).
const P_MAX: f64 = 210e6;

/// The lowest temperature `Limits` states, K: the release holds from 0 K, exclusive, and `Limits` needs a positive
/// one.
const T_MIN: f64 = f64::MIN_POSITIVE;

/// Ice Ih as a [`Fluid`], on `water`'s molar mass and gas constant: g₀₀ the 2009 revision's under
/// `DataSet::Corrected` and v8.0.0's 2006 value under `Parity` (DIV-0066, ROT-247). `water` must be a Helmholtz model.
pub fn ice(water: &Fluid, data_set: DataSet) -> Result<Fluid, Error> {
    let r =
        water.model().helmholtz().ok_or(Error::NoModel { prop: Prop::Constant(Constant::GasConstant) })?.gas_constant();
    let molar_mass = water.model().info().molar_mass();
    let (g00, key) = match data_set {
        DataSet::Parity => (gibbs::G00_2006, ModelKey::from_content(b"IAPWS R10-06 ice Ih, the 2006 g00")),
        _ => (gibbs::G00_2009, ModelKey::from_content(b"IAPWS R10-06(2009) ice Ih")),
    };
    let info =
        FluidInfo::new("Ice Ih", molar_mass, Source::new("IAPWS-R10-06(2009)", None, DataTerms::Published), key)?;
    let limits = Limits::new(T_MIN, gibbs::T_T, P_MAX)?;
    Ok(Fluid::new(Arc::new(Ice { info, limits, r, g00 })))
}

#[derive(Debug)]
struct Ice {
    info: FluidInfo,
    limits: Limits,
    /// The borrowed water's gas constant, J/(mol K): the bundle's scale, shared with the vapour side (M14.5).
    r: f64,
    g00: f64,
}

impl Ice {
    /// The solid at (T, p): Eq. (1)'s specific Gibbs energy times the molar mass, through `bundle_from_gibbs`.
    /// T and p arrive positive and finite: core builds a `NativeInput` only from a validated `Input`.
    fn solid(&self, t: f64, p: f64) -> Result<State, Error> {
        if t > gibbs::T_T {
            return Err(DomainError::AboveMaxTemperature { t, t_max: gibbs::T_T }.into());
        }
        if p > P_MAX {
            return Err(DomainError::AboveMaxPressure { p, p_max: P_MAX }.into());
        }
        let m = self.info.molar_mass();
        let [g, g_t, g_p, g_tt, g_tp, g_pp] = gibbs::evaluate(t, p, self.g00).map(|x| x * m);
        let molar = GibbsDerivs { g, g_t, g_p, g_tt, g_tp, g_pp };
        let (rho, bundle) = bundle_from_gibbs(self.r, t, p, &molar)?;
        State::from_total(self.info.key(), t, rho, self.r, m, Phase::Solid, &bundle)
    }
}

impl ThermoModel for Ice {
    fn info(&self) -> &FluidInfo {
        &self.info
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::none().with(Pair::PT)
    }

    fn flash(&self, input: NativeInput, _opts: &FlashOptions) -> Result<State, Error> {
        match (input.pair(), input.values()) {
            (Pair::PT, (p, t)) => self.solid(t, p),
            (pair, _) => Err(Error::Unsupported { pair }),
        }
    }

    fn limits(&self) -> Option<&Limits> {
        Some(&self.limits)
    }
}

#[cfg(test)]
mod tests;
