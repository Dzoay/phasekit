//! The flash routines' tests, one module per input-pair family; the fixtures they share are here.

use std::sync::Arc;

use super::isotherm::tx_nodes;
#[cfg(feature = "fluids-all")]
use super::phase::generic_vle;
use super::phase::{band, stable, strip_liquid};
#[cfg(feature = "fluids-all")]
use super::quality::QX_CELLS;
use super::quality::{polish, qx_nodes};
use super::search::nearest;
use super::*;
use crate::batch::{self, BatchRequest, Status};
use crate::data::FluidRecord;
use crate::fluid::Fluid;
use crate::input::Input;
use crate::model::Limits;
use crate::num::math;
#[cfg(feature = "fluids-all")]
use crate::prop::{DerivVar, Partial, SecondPartial};
use crate::saturation::{SatAccuracy, SatSide, SaturationCurve};
#[cfg(feature = "fluids-all")]
use crate::test_support::product;
use crate::units::{Basis, Density, Pressure, Temperature};
#[cfg(feature = "fluids-all")]
use crate::units::{Enthalpy, Entropy, InternalEnergy};

mod dp;
mod dt;
#[cfg(feature = "fluids-all")]
mod dx;
#[cfg(feature = "fluids-all")]
mod every_pair;
mod pt;
mod px;
mod qt_pq;
mod qx;
mod tx;

/// A curve fitted over 200-380 K that fails the test if it is ever evaluated outside that range.
#[derive(Debug)]
struct Fit;

impl SaturationCurve for Fit {
    fn accuracy(&self) -> SatAccuracy {
        SatAccuracy::Exact
    }
    fn t_range(&self) -> (f64, f64) {
        (200.0, 380.0)
    }
    fn at_t(&self, t: f64) -> Result<SatPair, Error> {
        assert!((200.0..=380.0).contains(&t), "fit evaluated outside its range at {t} K");
        let side = |rho| SatSide { t, p: 1e9, rho }; // the top sets the critical pressure: above every toy state
        Ok(SatPair { bubble: side(4_000.0), dew: side(50.0) })
    }
    fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
        Err(Error::Unsupported { pair: Pair::PQ })
    }
}

/// `name` from a private registry over the embedded data (the process-wide one stays untouched: a registry test
/// checks that nothing in it is loaded).
#[cfg(feature = "fluids-all")]
fn embedded(name: &str) -> Fluid {
    crate::Registry::from_embedded(crate::DataSet::Corrected).unwrap().get(name).unwrap().clone()
}

/// The decoded record of `name`, from a private registry.
#[cfg(feature = "fluids-all")]
fn record_of(name: &str) -> FluidRecord {
    crate::internal::record(&crate::Registry::from_embedded(crate::DataSet::Corrected).unwrap(), name).unwrap()
}

#[cfg(feature = "fluids-all")]
fn dt(rho: f64, t: f64) -> Input {
    Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap())
}

#[cfg(feature = "fluids-all")]
fn pt(p: f64, t: f64) -> Input {
    Input::pt(Pressure::new(p).unwrap(), Temperature::new(t).unwrap())
}

/// Water's pressure at (T, ρ) with the phase imposed: no phase rule, no acceptance gate.
#[cfg(feature = "fluids-all")]
fn imposed(rho: f64, t: f64) -> State {
    embedded("Water").flash(dt(rho, t), &FlashOptions::new().with_phase(Phase::Gas)).unwrap()
}

/// QT at quality `q` and temperature `t`.
fn q_t(q: f64, t: f64) -> Input {
    Input::qt(Quality::new(q).unwrap(), Temperature::new(t).unwrap())
}

/// PQ at pressure `p` and quality `q`.
fn p_q(p: f64, q: f64) -> Input {
    Input::pq(Pressure::new(p).unwrap(), Quality::new(q).unwrap())
}
