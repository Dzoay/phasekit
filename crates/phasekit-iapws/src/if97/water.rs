//! IF97 water as a `Fluid` (PLAN.md M14.3; user decision HA4): region 1 where p ≥ p_s(T), region 2 below, both from
//! 273.15 to 623.15 K and to 100 MPa, and the two-phase states along region 4 through `State::from_split`. Named in no
//! compat string (HA5).

use std::sync::Arc;

use phasekit_core::{
    Capabilities, DataTerms, DomainError, Error, FlashOptions, Fluid, FluidInfo, Limits, NativeInput, Pair, Quality,
    RootPolicy, Roots, Source, State, ThermoModel,
};

use super::{KEY, MOLAR_MASS, P_MAX, T_MAX_1, T_MIN, region1, region2, saturation_pressure, saturation_temperature};

/// IF97 water as a [`Fluid`]: PT, QT and PQ, every other pair `Unsupported`.
pub fn water() -> Result<Fluid, Error> {
    let source = Source::new("IAPWS-R7-97(2012)", None, DataTerms::Published);
    let info = FluidInfo::new("IF97 water", MOLAR_MASS, source, KEY)?;
    let limits = Limits::new(T_MIN, T_MAX_1, P_MAX)?.with_t_triple(273.16);
    Ok(Fluid::new(Arc::new(Water { info, limits })))
}

#[derive(Debug)]
struct Water {
    info: FluidInfo,
    limits: Limits,
}

impl Water {
    /// The state at (T, p), its region by Eq. (30): on the line itself, `Ambiguous` between the two sides' densities
    /// unless `roots` picks one (`RootPolicy::Nearest`, or `Stable`, where the two coexist and the liquid is taken).
    fn pt(t: f64, p: f64, roots: RootPolicy) -> Result<State, Error> {
        within(t)?;
        let p_s = saturation_pressure(t)?;
        if p > p_s {
            return region1(t, p);
        }
        if p < p_s {
            return region2(t, p);
        }
        let (liquid, vapour) = (region1(t, p)?, region2(t, p)?);
        let rho = |s: &State| s.rho(phasekit_core::Basis::Molar);
        match roots {
            RootPolicy::Nearest(target) if (rho(&vapour) - target).abs() < (rho(&liquid) - target).abs() => Ok(vapour),
            RootPolicy::Nearest(_) | RootPolicy::Stable => Ok(liquid),
            _ => Err(Error::Ambiguous { roots: Roots::new(&[rho(&liquid), rho(&vapour)]) }),
        }
    }

    /// The two-phase state at Q `q` along region 4 at (T, p): region 1's liquid and region 2's vapour there.
    fn split(q: f64, t: f64, p: f64) -> Result<State, Error> {
        State::from_split(region1(t, p)?, region2(t, p)?, Quality::new(q)?, t, p)
    }
}

/// `t` within the `Fluid`'s 273.15 to 623.15 K.
fn within(t: f64) -> Result<(), Error> {
    if t < T_MIN {
        return Err(DomainError::BelowMinTemperature { t, t_min: T_MIN }.into());
    }
    if t > T_MAX_1 {
        return Err(DomainError::AboveMaxTemperature { t, t_max: T_MAX_1 }.into());
    }
    Ok(())
}

impl ThermoModel for Water {
    fn info(&self) -> &FluidInfo {
        &self.info
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::none().with(Pair::PT).with(Pair::QT).with(Pair::PQ)
    }

    fn flash(&self, input: NativeInput, opts: &FlashOptions) -> Result<State, Error> {
        match (input.pair(), input.values()) {
            (Pair::PT, (p, t)) => Self::pt(t, p, opts.roots()),
            (Pair::QT, (q, t)) => {
                within(t)?;
                Self::split(q, t, saturation_pressure(t)?)
            }
            (Pair::PQ, (p, q)) => {
                let t = saturation_temperature(p)?;
                if t > T_MAX_1 {
                    let p_max = saturation_pressure(T_MAX_1)?;
                    return Err(DomainError::AboveMaxPressure { p, p_max }.into());
                }
                Self::split(q, t, p)
            }
            (pair, _) => Err(Error::Unsupported { pair }),
        }
    }

    fn limits(&self) -> Option<&Limits> {
        Some(&self.limits)
    }
}
