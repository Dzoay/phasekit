//! Ice Ih, IAPWS R10-06(2009), as a `Fluid` (PLAN.md M14.4, M14.5; user decision HA1): its Gibbs energy through
//! core's `bundle_from_gibbs`, every PT state `Phase::Solid`, on IAPWS-95's zero of u and s (the release's "IAPWS-95"
//! s₀), from 0 K to 273.16 K and to 210 MPa (§5); and its sublimation states, QT and PQ along IAPWS R14-08(2011)
//! Eq. (6) from 50 K to the triple point, the solid against the borrowed water's vapour through `State::from_split`.
//! Named in no compat string (HA5).

use std::sync::Arc;

use phasekit_core::density::{self, Branch};
use phasekit_core::{
    Capabilities, Constant, DataSet, DataTerms, DomainError, Error, FlashOptions, Fluid, FluidInfo, GibbsDerivs,
    Limits, ModelKey, NativeInput, Order, Pair, Phase, Prop, Quality, Source, State, ThermoModel, bundle_from_gibbs,
};

mod gibbs;
mod sublimation;

/// The highest pressure of ice Ih's equation, Pa (§5).
const P_MAX: f64 = 210e6;

/// The lowest temperature `Limits` states, K: the release holds from 0 K, exclusive, and `Limits` needs a positive
/// one.
const T_MIN: f64 = f64::MIN_POSITIVE;

/// What a water without a Helmholtz model lacks: a gas constant to share.
const NO_GAS_CONSTANT: Error = Error::NoModel { prop: Prop::Constant(Constant::GasConstant) };

/// Ice Ih as a [`Fluid`], on `water`'s molar mass and gas constant: g₀₀ the 2009 revision's under
/// `DataSet::Corrected` and v8.0.0's 2006 value under `Parity` (DIV-0066, ROT-247). `water` must be a Helmholtz model.
pub fn ice(water: &Fluid, data_set: DataSet) -> Result<Fluid, Error> {
    Ok(Fluid::new(model(water, data_set)?))
}

/// [`ice`]'s model, for a registry (`Registry::with_model`).
pub fn model(water: &Fluid, data_set: DataSet) -> Result<Arc<dyn ThermoModel>, Error> {
    let r = water.model().helmholtz().ok_or(NO_GAS_CONSTANT)?.gas_constant();
    let molar_mass = water.model().info().molar_mass();
    let (g00, key) = match data_set {
        DataSet::Parity => (gibbs::G00_2006, ModelKey::from_content(b"IAPWS R10-06 ice Ih, the 2006 g00")),
        _ => (gibbs::G00_2009, ModelKey::from_content(b"IAPWS R10-06(2009) ice Ih")),
    };
    let source = Source::new("IAPWS-R10-06(2009)", None, DataTerms::Published);
    let info = FluidInfo::new("Ice Ih", molar_mass, source, key)?;
    let limits = Limits::new(T_MIN, gibbs::T_T, P_MAX)?;
    Ok(Arc::new(Ice { info, limits, r, g00, water: water.clone() }))
}

/// Ice Ih's sublimation pressure at `t` (K), Pa: R14-08(2011) Eq. (6), from 50 K to 273.16 K.
pub fn sublimation_pressure(t: f64) -> Result<f64, Error> {
    sublimation::pressure(t)
}

/// Ice Ih's sublimation temperature at `p` (Pa), K: Eq. (6) inverted, from its pressure at 50 K to 611.657 Pa.
pub fn sublimation_temperature(p: f64) -> Result<f64, Error> {
    sublimation::temperature(p)
}

#[derive(Debug)]
struct Ice {
    info: FluidInfo,
    limits: Limits,
    /// The borrowed water's gas constant, J/(mol K): the bundle's scale, shared with the vapour side.
    r: f64,
    g00: f64,
    /// The borrowed water: its vapour, the gas side of a sublimation state, evaluated through its model, never the
    /// gauged `Fluid` (ROT-104's rule).
    water: Fluid,
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

    /// The borrowed water's vapour at (T, p), on this model's key and gas constant so that `from_split` takes it.
    fn vapour(&self, t: f64, p: f64) -> Result<State, Error> {
        let eos = self.water.model().helmholtz().ok_or(NO_GAS_CONSTANT)?;
        let rho = density::solve(eos, t, p, Branch::Vapour, None)?;
        let total = eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two);
        let bundle = total.bundle().ok_or(Error::NoModel { prop: Prop::Cpmolar })?;
        State::from_total(self.info.key(), t, rho, self.r, self.info.molar_mass(), Phase::Gas, &bundle)
    }

    /// The sublimation state at Q `q` (the vapour's fraction) on (T, p_subl): the solid as the denser phase.
    fn split(&self, q: f64, t: f64, p: f64) -> Result<State, Error> {
        State::from_split(self.solid(t, p)?, self.vapour(t, p)?, Quality::new(q)?, t, p)
    }
}

impl ThermoModel for Ice {
    fn info(&self) -> &FluidInfo {
        &self.info
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::none().with(Pair::PT).with(Pair::QT).with(Pair::PQ)
    }

    fn flash(&self, input: NativeInput, _opts: &FlashOptions) -> Result<State, Error> {
        match (input.pair(), input.values()) {
            (Pair::PT, (p, t)) => self.solid(t, p),
            (Pair::QT, (q, t)) => self.split(q, t, sublimation::pressure(t)?),
            (Pair::PQ, (p, q)) => self.split(q, sublimation::temperature(p)?, p),
            (pair, _) => Err(Error::Unsupported { pair }),
        }
    }

    fn limits(&self) -> Option<&Limits> {
        Some(&self.limits)
    }
}

#[cfg(test)]
mod tests;
