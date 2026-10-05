//! L3 packages and handles: [`PureFluid`] (the Helmholtz package, a `ThermoModel`), [`Fluid`] (the cheap
//! handle every caller holds) and [`Gauge`] (reference states as values, never global mutation).

use std::sync::{Arc, LazyLock};

use crate::derivs::Order;
use crate::error::{Error, LoadError};
use crate::flash::{self, FlashOptions};
use crate::helmholtz::HelmholtzModel;
use crate::input::{Capabilities, Input, NativeInput, Pair, Var};
use crate::model::{CriticalPoint, FluidInfo, Limits, ThermoModel};
use crate::prop::Prop;
use crate::saturation::SaturationCurve;
use crate::state::State;
use crate::transport::TransportSet;
use crate::units::{Basis, Enthalpy, Entropy, Pressure, Quality, Temperature};

/// Reference-state offsets, molar (J/mol, J/(mol K)). Applied at the boundary by [`Fluid`], never inside α⁰,
/// so it works identically for every family, Helmholtz or Gibbs (map 15 X3, X7, X12).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gauge {
    dh: f64,
    ds: f64,
}

impl Gauge {
    /// The model's own zero.
    pub const NATIVE: Gauge = Gauge { dh: 0.0, ds: 0.0 };
    /// Validates finite offsets.
    pub fn new(dh: f64, ds: f64) -> Result<Self, Error> {
        if dh.is_finite() && ds.is_finite() {
            Ok(Self { dh, ds })
        } else {
            Err(Error::InvalidInput { quantity: "gauge", value: if dh.is_finite() { ds } else { dh } })
        }
    }
    /// Enthalpy (and internal-energy) offset, J/mol.
    pub const fn dh(self) -> f64 {
        self.dh
    }
    /// Entropy offset, J/(mol K).
    pub const fn ds(self) -> f64 {
        self.ds
    }
    /// Shifts a molar input value from this gauge to the model's native one.
    pub(crate) fn to_native(self, var: Var, v: f64) -> f64 {
        match var {
            Var::H | Var::U => v - self.dh,
            Var::S => v - self.ds,
            Var::T | Var::P | Var::Q | Var::D => v,
        }
    }
}

/// CoolProp's reference states (map 01 §3), plus a custom anchor usable by ANY model (a solid included).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum ReferenceState {
    /// h = 200 kJ/kg, s = 1 kJ/(kg K) for saturated liquid at 0 °C.
    Iir,
    /// h = s = 0 for saturated liquid at −40 °C.
    Ashrae,
    /// h = s = 0 for saturated liquid at 1 atm.
    Nbp,
    /// The given h and s at the state the anchor input defines.
    Custom {
        /// State at which the values hold (in the handle's current gauge).
        at: Input,
        /// Enthalpy there.
        h: Enthalpy,
        /// Entropy there.
        s: Entropy,
    },
}

/// A part materialised on first use, at most once, lock-free afterwards; std's `LazyLock` drops the
/// initialiser (and the blob and reference handles it captured) after the first call (S-08).
pub(crate) type Lazy<T> = LazyLock<Result<T, LoadError>, Box<dyn FnOnce() -> Result<T, LoadError> + Send>>;

fn ready<T: Send + 'static>(value: T) -> Lazy<T> {
    LazyLock::new(Box::new(move || Ok(value)))
}

/// Input pairs the core flash implements at this milestone. Grows M5 → M7 so every milestone is green
/// (Verification-first): undeclared pairs are refused with `Unsupported`.
const IMPLEMENTED: Capabilities = Capabilities::none().with(Pair::DT);

/// The package of a pure or pseudo-pure Helmholtz fluid: identity, EOS (any family), limits, critical point
/// and parts materialised on first use (saturation curve, transport). Immutable; shared through `Arc`.
#[derive(Debug)]
pub struct PureFluid {
    info: FluidInfo,
    eos: Box<dyn HelmholtzModel>,
    limits: Limits,
    critical: Option<CriticalPoint>,
    saturation: Lazy<Option<Box<dyn SaturationCurve>>>,
    transport: Lazy<TransportSet>,
}

/// Assembles a [`PureFluid`] around any [`HelmholtzModel`]: the decoder uses it, and so does a third-party
/// family (no core edit; `tests/new_family.rs`). Required parts are arguments of
/// [`PureFluid::builder`], so `build` cannot fail (S-09).
pub struct PureFluidBuilder {
    fluid: PureFluid,
}

impl PureFluidBuilder {
    /// The model's own critical point (used for phase labels).
    pub fn critical(mut self, c: CriticalPoint) -> Self {
        self.fluid.critical = Some(c);
        self
    }
    /// An eagerly available saturation curve.
    pub fn saturation(mut self, curve: impl SaturationCurve + 'static) -> Self {
        let curve: Box<dyn SaturationCurve> = Box::new(curve);
        self.fluid.saturation = ready(Some(curve));
        self
    }
    /// A saturation curve decoded on first use (the superancillary is 89.8 % of the v8.0.0 data, map 09).
    pub fn lazy_saturation(
        mut self,
        init: impl FnOnce() -> Result<Option<Box<dyn SaturationCurve>>, LoadError> + Send + 'static,
    ) -> Self {
        self.fluid.saturation = LazyLock::new(Box::new(init));
        self
    }
    /// Transport materialised on first use; the decoder's closure holds the blob and strong handles to its
    /// ECS reference fluids (E5). Crate-private: CoolProp's transport forms are closed data (§11).
    pub(crate) fn lazy_transport(
        mut self,
        init: impl FnOnce() -> Result<TransportSet, LoadError> + Send + 'static,
    ) -> Self {
        self.fluid.transport = LazyLock::new(Box::new(init));
        self
    }
    /// Finishes the package.
    pub fn build(self) -> PureFluid {
        self.fluid
    }
}

impl PureFluid {
    /// Starts a package around `eos` with its validity domain.
    pub fn builder(info: FluidInfo, eos: impl HelmholtzModel + 'static, limits: Limits) -> PureFluidBuilder {
        let fluid = PureFluid {
            info,
            eos: Box::new(eos),
            limits,
            critical: None,
            saturation: ready(None),
            transport: ready(TransportSet::default()), // no models: properties report `NoModel`
        };
        PureFluidBuilder { fluid }
    }
    /// The EOS.
    pub fn eos(&self) -> &dyn HelmholtzModel {
        &*self.eos
    }
    /// Validity domain.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }
    /// The saturation curve, decoding it on first use. `None`: no curve, so the generic VLE (M6) decides.
    pub fn saturation(&self) -> Result<Option<&dyn SaturationCurve>, Error> {
        Ok(LazyLock::force(&self.saturation).as_ref().map_err(|e| Error::Load(e.clone()))?.as_deref())
    }
    /// The transport set, materialising it (and its ECS reference fluids) on first use.
    pub(crate) fn transport(&self) -> Result<&TransportSet, Error> {
        LazyLock::force(&self.transport).as_ref().map_err(|e| Error::Load(e.clone()))
    }
}

impl ThermoModel for PureFluid {
    fn info(&self) -> &FluidInfo {
        &self.info
    }
    fn capabilities(&self) -> Capabilities {
        IMPLEMENTED
    }
    fn flash(&self, input: NativeInput, opts: &FlashOptions) -> Result<State, Error> {
        flash::flash(self, input, opts)
    }
    fn critical_point(&self) -> Option<CriticalPoint> {
        self.critical
    }
    fn helmholtz(&self) -> Option<&dyn HelmholtzModel> {
        Some(&*self.eos)
    }
    fn viscosity(&self, _state: &State) -> Result<f64, Error> {
        // Materialises the transport set and its ECS reference fluid; evaluating the forms lands at M8.
        self.transport()?;
        Err(Error::NoModel { prop: Prop::Viscosity })
    }
    fn surface_tension(&self, t: f64) -> Result<f64, Error> {
        self.transport()?.surface_tension(t)
    }
}

/// The handle callers hold: one model plus the reference state it reports in. Cloning is one `Arc`
/// increment; the registry hands out `&Fluid`, so per-request lookups do no refcount traffic (K1). A
/// `Fluid` is self-contained: its lazy parts hold strong handles to what they need (S-05).
#[derive(Clone, Debug)]
pub struct Fluid {
    model: Arc<dyn ThermoModel>,
    gauge: Gauge,
}

impl Fluid {
    /// Wraps a model at its native gauge.
    pub fn new(model: Arc<dyn ThermoModel>) -> Self {
        Self { model, gauge: Gauge::NATIVE }
    }
    /// The model.
    pub fn model(&self) -> &dyn ThermoModel {
        &*self.model
    }
    /// Shorthand for `model().info()`.
    pub fn info(&self) -> &FluidInfo {
        self.model.info()
    }
    /// The reference-state offsets of this handle.
    pub fn gauge(&self) -> Gauge {
        self.gauge
    }
    /// The same model reporting in another gauge: a new value, O(1).
    pub fn with_gauge(&self, gauge: Gauge) -> Fluid {
        Fluid { model: Arc::clone(&self.model), gauge }
    }
    /// The same model in a named reference state. It flashes the anchor state, so it works for any family
    /// that declares the anchor's pair (IIR and ASHRAE need QT, NBP needs PQ; `Unsupported` otherwise).
    pub fn with_reference(&self, reference: ReferenceState) -> Result<Fluid, Error> {
        let m = self.info().molar_mass();
        let liquid = Quality::new(0.0)?;
        let (at, h, s) = match reference {
            ReferenceState::Iir => (Input::qt(liquid, Temperature::new(273.15)?), 200e3 * m, 1e3 * m),
            ReferenceState::Ashrae => (Input::qt(liquid, Temperature::new(233.15)?), 0.0, 0.0),
            ReferenceState::Nbp => (Input::pq(Pressure::new(101_325.0)?, liquid), 0.0, 0.0),
            ReferenceState::Custom { at, h, s } => (at, h.to_molar(m), s.to_molar(m)),
        };
        let native = self.model.flash(self.native(at)?, &FlashOptions::default())?;
        Ok(self.with_gauge(Gauge::new(h - native.h(Basis::Molar), s - native.s(Basis::Molar))?))
    }

    /// The one normalisation: capability check, basis to molar, this handle's gauge to native (S-04).
    fn native(&self, input: Input) -> Result<NativeInput, Error> {
        let pair = input.pair();
        if !self.model.capabilities().contains(pair) {
            return Err(Error::Unsupported { pair });
        }
        Ok(input.to_native(self.info().molar_mass(), self.gauge))
    }

    /// Flash in this handle's gauge: the input is normalised once, the model flashes in its native gauge,
    /// and the result is stamped with this handle's gauge.
    pub fn flash(&self, input: Input, opts: &FlashOptions) -> Result<State, Error> {
        Ok(self.model.flash(self.native(input)?, opts)?.with_gauge(self.gauge))
    }

    /// Flash with default options.
    pub fn state(&self, input: Input) -> Result<State, Error> {
        self.flash(input, &FlashOptions::default())
    }

    /// The one `Prop` table of the library (batch, strings, C, JS). Refuses states of other models.
    pub fn prop(&self, state: &State, prop: Prop) -> Result<f64, Error> {
        if state.key() != self.info().key() {
            return Err(Error::ForeignState);
        }
        let (molar, mass) = (Basis::Molar, Basis::Mass);
        let m = self.info().molar_mass();
        Ok(match prop {
            Prop::T => state.t(),
            Prop::P => state.p(),
            Prop::Q => state.quality().ok_or(Error::Undefined { prop, phase: state.phase() })?,
            Prop::Dmolar => state.rho(molar),
            Prop::Dmass => state.rho(mass),
            Prop::Hmolar => state.h(molar),
            Prop::Hmass => state.h(mass),
            Prop::Smolar => state.s(molar),
            Prop::Smass => state.s(mass),
            Prop::Umolar => state.u(molar),
            Prop::Umass => state.u(mass),
            Prop::Cvmolar => state.cv(molar)?,
            Prop::Cvmass => state.cv(mass)?,
            Prop::Cpmolar => state.cp(molar)?,
            Prop::Cpmass => state.cp(mass)?,
            Prop::SpeedOfSound => state.speed_of_sound()?,
            Prop::Z => state.z(),
            Prop::Cp0molar | Prop::Cp0mass => {
                // cp⁰ = R·(1 − A20⁰): needs the model's ideal part (E1).
                let ideal = self.model.derivs(state, Order::Two).and_then(|d| d.ideal());
                let a20 = ideal.and_then(|d| d.get(2, 0)).ok_or(Error::NoModel { prop })?;
                let cp0 = state.gas_constant() * (1.0 - a20);
                if prop == Prop::Cp0mass { cp0 / m } else { cp0 }
            }
            Prop::MolarMass => m,
            Prop::Viscosity => self.model.viscosity(state)?,
            Prop::Conductivity => self.model.conductivity(state)?,
            Prop::SurfaceTension => self.model.surface_tension(state.t())?,
            Prop::Partial(p) => state.partial(p)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::FluidRecord;
    use crate::state::Phase;
    use crate::units::Density;

    /// ROT-006 (map 01 R13, map 14 R11): a flash takes no prior state and mutates nothing it is given, so after a
    /// failed flash the earlier state reads exactly as before, and the handle flashes the same point to the same bits
    /// (CoolProp's backend is left torn: `T_` and `get_T()` disagree after a failed update).
    #[test]
    fn failed_flash_leaves_the_previous_state_untouched() {
        let fluid = Fluid::new(Arc::new(FluidRecord::toy("X").unwrap().compile().unwrap()));
        let liquid = FlashOptions::new().with_phase(Phase::Liquid);
        let dt = |t| Input::dt(Density::molar(5_000.0).unwrap(), Temperature::new(t).unwrap());
        let read =
            |s: &State| [Prop::T, Prop::P, Prop::Hmolar, Prop::Cpmolar].map(|p| fluid.prop(s, p).map(f64::to_bits));
        let state = fluid.flash(dt(300.0), &liquid).unwrap();
        let before = (state, read(&state));
        // Above the 420 K limit, and a pair the toy model does not declare.
        assert!(matches!(fluid.flash(dt(500.0), &liquid), Err(Error::Domain(_))));
        let pt = Input::pt(Pressure::new(1e6).unwrap(), Temperature::new(300.0).unwrap());
        assert_eq!(fluid.flash(pt, &liquid), Err(Error::Unsupported { pair: Pair::PT }));
        assert_eq!((state, read(&state)), before);
        let again = fluid.flash(dt(300.0), &liquid).unwrap();
        assert_eq!((again, read(&again)), before, "the handle carries nothing over from the failure");
    }
}
