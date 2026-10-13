//! L3 packages and handles: [`PureFluid`] (the Helmholtz package, a `ThermoModel`), [`Fluid`] (the cheap
//! handle every caller holds) and [`Gauge`] (reference states as values, never global mutation).

use std::sync::{Arc, LazyLock, OnceLock};

use crate::data::Calorics;
use crate::derivs::{Derivs, Order};
use crate::error::{Error, LoadError};
use crate::flash::{self, FlashOptions};
use crate::helmholtz::HelmholtzModel;
use crate::input::{Capabilities, Input, NativeInput, Pair, Var};
use crate::melting::MeltingCurve;
use crate::model::{CriticalOrigin, CriticalPoint, FluidInfo, Limits, ThermoModel};
use crate::prop::Prop;
use crate::saturation::SatAccuracy;
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
const IMPLEMENTED: Capabilities = Capabilities::none()
    .with(Pair::DT)
    .with(Pair::PT)
    .with(Pair::QT)
    .with(Pair::PQ)
    .with(Pair::HP)
    .with(Pair::PS)
    .with(Pair::PU)
    .with(Pair::DH)
    .with(Pair::DS)
    .with(Pair::DU)
    .with(Pair::HT)
    .with(Pair::ST)
    .with(Pair::TU)
    .with(Pair::DP)
    .with(Pair::QS)
    .with(Pair::HQ)
    .with(Pair::DQ)
    .with(Pair::HS)
    .with(Pair::SU);

/// The package of a pure or pseudo-pure Helmholtz fluid: identity, EOS (any family), limits, critical point
/// and parts materialised on first use (saturation curve, transport). Immutable; shared through `Arc`.
#[derive(Debug)]
pub struct PureFluid {
    info: FluidInfo,
    eos: Box<dyn HelmholtzModel>,
    limits: Limits,
    critical: Option<CriticalPoint>,
    /// The model's own critical point, found on first use ([`PureFluid::model_critical`]).
    model_critical: OnceLock<Option<CriticalPoint>>,
    saturation: Lazy<Option<Box<dyn SaturationCurve>>>,
    /// The saturated caloric curves the Q-pairs find their roots on (M7.7), decoded on first use.
    calorics: Lazy<Option<Calorics>>,
    transport: Lazy<TransportSet>,
    /// The melting curve (M8.11), its segments' pressure ranges computed once; part of the model, so a correction or a
    /// second registry never meets another model's curve (ROT-036).
    melting: Option<MeltingCurve>,
}

/// Assembles a [`PureFluid`] around any [`HelmholtzModel`]: the decoder uses it, and so does a third-party
/// family (no core edit; `tests/new_family.rs`). Required parts are arguments of
/// [`PureFluid::builder`], so `build` cannot fail (S-09).
pub struct PureFluidBuilder {
    fluid: PureFluid,
}

impl PureFluidBuilder {
    /// The fluid's critical point. A published one (origin `Published`) seeds the model's own, found on first use; one
    /// with origin `Model` is the model's own as given.
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
    /// The saturated caloric curves (M5.2a), eagerly available.
    pub(crate) fn calorics(mut self, calorics: Calorics) -> Self {
        self.fluid.calorics = ready(Some(calorics));
        self
    }

    /// The saturated caloric curves, decoded on first use by a Q-pair.
    pub(crate) fn lazy_calorics(
        mut self,
        init: impl FnOnce() -> Result<Option<Calorics>, LoadError> + Send + 'static,
    ) -> Self {
        self.fluid.calorics = LazyLock::new(Box::new(init));
        self
    }

    /// The melting curve, from the record's segments.
    pub(crate) fn melting(mut self, curve: MeltingCurve) -> Self {
        self.fluid.melting = Some(curve);
        self
    }

    /// Transport, eagerly available (a whole record's).
    pub(crate) fn transport(mut self, set: TransportSet) -> Self {
        self.fluid.transport = ready(set);
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
            model_critical: OnceLock::new(),
            saturation: ready(None),
            calorics: ready(None),
            transport: ready(TransportSet::default()), // no models: properties report `NoModel`
            melting: None,
        };
        PureFluidBuilder { fluid }
    }
    /// The model's own critical point, found (see [`ThermoModel::critical_point`]).
    fn model_critical(&self) -> Option<CriticalPoint> {
        let given = self.critical?;
        if given.origin == CriticalOrigin::Model {
            return Some(given);
        }
        let model = |t, p, rho| CriticalPoint { t, p, rho, origin: CriticalOrigin::Model };
        // Without a curve, or with a pseudo-pure fluid's definition, the published point stands, as CoolProp has it.
        let curve = self.saturation().ok().flatten().filter(|c| c.accuracy() != SatAccuracy::Definition);
        let Some(curve) = curve else { return Some(given) };
        let top = curve.at_t(curve.t_range().1).ok();
        let (t0, rho0) = top.map_or((given.t, given.rho), |top| (top.bubble.t, top.bubble.rho));
        if let Ok((t, rho, p)) = crate::crit::numerical(&*self.eos, t0, rho0) {
            return Some(model(t, p, rho));
        }
        // Where the solve cannot run, an exact curve's top stands: non-analytic terms are singular at τ = δ = 1 (Water's
        // and CarbonDioxide's published points; their superancillary tops lie just off it, and solve).
        match (curve.accuracy(), top) {
            (SatAccuracy::Exact, Some(top)) => Some(model(top.bubble.t, top.bubble.p, top.bubble.rho)),
            _ => Some(given),
        }
    }
    /// The EOS.
    pub fn eos(&self) -> &dyn HelmholtzModel {
        &*self.eos
    }
    /// Validity domain.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }
    /// The saturated caloric curves, decoding them on first use; `None` without fresh ones.
    pub(crate) fn calorics(&self) -> Result<Option<&Calorics>, Error> {
        Ok(LazyLock::force(&self.calorics).as_ref().map_err(|e| Error::Load(e.clone()))?.as_ref())
    }

    /// The saturation curve, decoding it on first use. `None`: no curve, so the generic VLE (M6) decides.
    pub fn saturation(&self) -> Result<Option<&dyn SaturationCurve>, Error> {
        Ok(LazyLock::force(&self.saturation).as_ref().map_err(|e| Error::Load(e.clone()))?.as_deref())
    }
    /// The melting curve, if the model has one.
    pub(crate) fn melting_curve(&self) -> Option<&MeltingCurve> {
        self.melting.as_ref()
    }
    /// The transport set, materialising it (and its ECS reference fluids) on first use.
    pub(crate) fn transport(&self) -> Result<&TransportSet, Error> {
        LazyLock::force(&self.transport).as_ref().map_err(|e| Error::Load(e.clone()))
    }

    /// What the transport forms read at `state`.
    fn transport_inputs<'a>(&'a self, state: &'a State) -> crate::transport::TransportInputs<'a> {
        crate::transport::TransportInputs::new(state, self.info.molar_mass(), &*self.eos).with_model(self)
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
    /// The model's own critical point (map 03 §6; ROT-084): one given with origin `Model`; else, for a fluid with a
    /// saturation curve, the EOS's own, solved from the curve's top ([`crate::crit`]; CoolProp's with superancillaries
    /// on, within `Flash`), or the exact curve's top where the solve cannot run (non-analytic terms at τ = δ = 1); else
    /// the published one, with that origin, as CoolProp has it (the pseudo-pure fluids). The published point stays in
    /// [`FluidInfo::published_critical`].
    fn critical_point(&self) -> Option<CriticalPoint> {
        *self.model_critical.get_or_init(|| self.model_critical())
    }
    fn helmholtz(&self) -> Option<&dyn HelmholtzModel> {
        Some(&*self.eos)
    }
    fn viscosity(&self, state: &State) -> Result<f64, Error> {
        self.transport()?.viscosity(&self.transport_inputs(state))
    }
    fn conductivity(&self, state: &State) -> Result<f64, Error> {
        self.transport()?.conductivity(&self.transport_inputs(state))
    }
    fn melting_pressure(&self, t: f64) -> Result<f64, Error> {
        let curve = self.melting.as_ref().ok_or(Error::NoModel { prop: Prop::MeltingPressure })?;
        curve.pressure(Temperature::new(t)?.get())
    }
    fn melting_temperature(&self, p: f64) -> Result<f64, Error> {
        let curve = self.melting.as_ref().ok_or(Error::NoModel { prop: Prop::MeltingTemperature })?;
        curve.temperature(Pressure::new(p)?.get())
    }
    /// σ(T) over its fit's range, up to the model's own critical temperature ([`SurfaceTension`]'s rule).
    fn surface_tension(&self, t: f64) -> Result<f64, Error> {
        let fit = self.transport()?.surface_tension.as_ref().ok_or(Error::NoModel { prop: Prop::SurfaceTension })?;
        let t_c = self.critical_point().map_or(f64::INFINITY, |c| c.t);
        fit.eval(t, self.limits.t_min(), t_c)
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
            Prop::Gmolar => state.g(molar),
            Prop::Gmass => state.g(mass),
            Prop::Helmholtzmolar => state.a(molar),
            Prop::Helmholtzmass => state.a(mass),
            Prop::Cvmolar => state.cv(molar)?,
            Prop::Cvmass => state.cv(mass)?,
            Prop::Cpmolar => state.cp(molar)?,
            Prop::Cpmass => state.cp(mass)?,
            Prop::SpeedOfSound => state.speed_of_sound()?,
            Prop::Z => state.z(),
            Prop::Cp0molar | Prop::Cp0mass => {
                // cp⁰ = R·(1 − A20⁰): the model's ideal part (E1) at one phase. A function of T alone, so in the dome
                // too (SQ3), as CoolProp: there the Helmholtz part's at the state's T, A20⁰ being free of ρ.
                let ideal = match state.one_phase() {
                    Some(one) => self.model.derivs(&one, Order::Two).and_then(|d| d.ideal()),
                    None => self.model.helmholtz().map(|h| h.ideal(state.t(), state.rho(molar), Order::Two)),
                };
                let a20 = ideal.and_then(|d| d.get(2, 0)).ok_or(Error::NoModel { prop })?;
                let cp0 = state.gas_constant() * (1.0 - a20);
                if prop == Prop::Cp0mass { cp0 / m } else { cp0 }
            }
            Prop::HmolarResidual | Prop::SmolarResidual | Prop::GmolarResidual => {
                let one = one_phase(state, prop)?;
                let residual = self.model.derivs(&one, Order::One).and_then(|d| d.residual());
                let entry = |i, j| residual.and_then(|d| d.get(i, j)).ok_or(Error::NoModel { prop });
                let (a00, a10, a01) = (entry(0, 0)?, entry(1, 0)?, entry(0, 1)?);
                let (r, t) = (one.gas_constant(), one.t());
                match prop {
                    Prop::HmolarResidual => r * t * (a10 + a01),
                    Prop::SmolarResidual => r * (a10 - a00),
                    _ => r * t * (a00 + a01),
                }
            }
            Prop::Bvirial | Prop::Cvirial | Prop::DBvirialDT | Prop::DCvirialDT => {
                let v =
                    self.model.helmholtz().and_then(|h| h.zero_density(state.t())).ok_or(Error::NoModel { prop })?;
                match prop {
                    Prop::Bvirial => v.b,
                    Prop::Cvirial => v.c,
                    Prop::DBvirialDT => v.db_dt,
                    _ => v.dc_dt,
                }
            }
            Prop::MolarMass => m,
            Prop::Viscosity => self.model.viscosity(state)?,
            Prop::Conductivity => self.model.conductivity(state)?,
            Prop::SurfaceTension => self.model.surface_tension(state.t())?,
            Prop::MeltingPressure => self.model.melting_pressure(state.t())?,
            Prop::MeltingTemperature => self.model.melting_temperature(state.p())?,
            Prop::Partial(p) => state.partial(p)?,
            Prop::SecondPartial(p) => state.second_partial(p, &self.order_three(state, prop)?)?,
            Prop::FundamentalDerivative => state.fundamental_derivative(&self.order_three(state, prop)?)?,
            Prop::IsothermalCompressibility => state.isothermal_compressibility()?,
            Prop::IsobaricExpansion => state.isobaric_expansion()?,
        })
    }

    /// The model's order-3 total bundle at `state` as one phase (E1), `NoModel` from a model without one.
    fn order_three(&self, state: &State, prop: Prop) -> Result<Derivs, Error> {
        self.model.derivs(&one_phase(state, prop)?, Order::Three).map(|d| d.total()).ok_or(Error::NoModel { prop })
    }
}

/// `state` as the one phase `prop` reads from the model's `derivs`: a single phase's own, at Q = 0 and 1 a saturated
/// phase (SQ2, SQ3), undefined strictly inside the dome.
fn one_phase(state: &State, prop: Prop) -> Result<State, Error> {
    state.one_phase().ok_or(Error::Undefined { prop, phase: state.phase() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::FluidRecord;
    use crate::prop::{DerivVar, Partial, SecondPartial};
    use crate::state::Phase;
    use crate::units::Density;

    /// ROT-006 (map 01 R13, map 14 R11): a flash takes no prior state and mutates nothing it is given, so after a
    /// failed flash the earlier state reads exactly as before, and the handle flashes the same point to the same bits
    /// (CoolProp's backend is left torn: `T_` and `get_T()` disagree after a failed update).
    #[test]
    fn failed_flash_leaves_the_previous_state_untouched() {
        let fluid = Fluid::new(Arc::new(FluidRecord::synthetic("X").unwrap().compile().unwrap()));
        let liquid = FlashOptions::new().with_phase(Phase::Liquid);
        let dt = |t| Input::dt(Density::molar(5_000.0).unwrap(), Temperature::new(t).unwrap());
        let read =
            |s: &State| [Prop::T, Prop::P, Prop::Hmolar, Prop::Cpmolar].map(|p| fluid.prop(s, p).map(f64::to_bits));
        let state = fluid.flash(dt(300.0), &liquid).unwrap();
        let before = (state, read(&state));
        // Above the 420 K limit, and an (h, s) colder than the toy's liquid at its lowest temperature.
        assert!(matches!(fluid.flash(dt(500.0), &liquid), Err(Error::Domain(_))));
        let hs = Input::hs(Enthalpy::molar(1e4).unwrap(), Entropy::molar(10.0).unwrap());
        assert!(matches!(fluid.flash(hs, &liquid), Err(Error::Domain(_))));
        assert_eq!((state, read(&state)), before);
        let again = fluid.flash(dt(300.0), &liquid).unwrap();
        assert_eq!((again, read(&again)), before, "the handle carries nothing over from the failure");
    }

    /// M7.10: a second partial is undefined where either derivative holds the quantity it differentiates by (h, s, u
    /// or g in two bases, on either side: the Jacobian vanishes only up to rounding), and exactly 0 where the first
    /// partial holds its own variable constant.
    #[test]
    fn degenerate_second_partials() {
        use DerivVar::{Dmolar, Gmass, Gmolar, Hmass, Hmolar, P, Smass, Smolar, T, Umass, Umolar};
        let fluid = Fluid::new(Arc::new(FluidRecord::synthetic("X").unwrap().compile().unwrap()));
        let gas = FlashOptions::new().with_phase(Phase::Gas);
        let input = Input::dt(Density::molar(500.0).unwrap(), Temperature::new(350.0).unwrap());
        let state = fluid.flash(input, &gas).unwrap();
        let second = |first: (DerivVar, DerivVar, DerivVar), wrt, at| {
            let p = SecondPartial { first: Partial { of: first.0, wrt: first.1, at: first.2 }, wrt, at };
            fluid.prop(&state, Prop::SecondPartial(p))
        };
        let pairs = [(Hmolar, Hmass), (Smolar, Smass), (Umolar, Umass), (Gmolar, Gmass)];
        let cases = pairs.into_iter().flat_map(|(a, b)| [((P, a, b), T, Dmolar), ((P, T, Dmolar), a, b)]);
        for (first, wrt, at) in cases {
            let prop = Prop::SecondPartial(SecondPartial {
                first: Partial { of: first.0, wrt: first.1, at: first.2 },
                wrt,
                at,
            });
            assert_eq!(second(first, wrt, at), Err(Error::Undefined { prop, phase: Phase::Gas }));
        }
        assert_eq!(second((Umolar, T, Umass), T, Dmolar), Ok(0.0));
    }

    /// M7.10: where one quantity in two bases makes a Jacobian vanish only up to rounding, the check before the ratio
    /// refuses it: Water's dilute gas at 300 K and 0.1 mol/m³, where the ratio alone gives finite nonsense (about
    /// 1e16) for h, s, u and g in two bases on either side.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn degenerate_second_partials_of_a_dilute_gas() {
        use DerivVar::{Dmolar, Gmass, Gmolar, Hmass, Hmolar, P, Smass, Smolar, T, Umass, Umolar};
        let registry = crate::Registry::from_embedded(crate::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let input = Input::dt(Density::molar(0.1).unwrap(), Temperature::new(300.0).unwrap());
        let state = water.flash(input, &FlashOptions::new().with_phase(Phase::Gas)).unwrap();
        let pairs = [(Hmolar, Hmass), (Smolar, Smass), (Umolar, Umass), (Gmolar, Gmass)];
        for (a, b) in pairs {
            for (first, wrt, at) in
                [(Partial { of: P, wrt: a, at: b }, T, Dmolar), (Partial { of: P, wrt: T, at: Dmolar }, a, b)]
            {
                let prop = Prop::SecondPartial(SecondPartial { first, wrt, at });
                assert_eq!(water.prop(&state, prop), Err(Error::Undefined { prop, phase: Phase::Gas }));
            }
        }
    }

    /// M7.10: at Water's critical point the order-3 entries have no limit (NaN), so the second partials and the
    /// fundamental derivative are undefined there; κ_T and β, of order 2, are not finite either way.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn order_three_outputs_are_undefined_at_the_critical_point() {
        use DerivVar::{Dmolar, P, T};
        let registry = crate::Registry::from_embedded(crate::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let c = water.model().critical_point().unwrap();
        let input = Input::dt(Density::molar(c.rho).unwrap(), Temperature::new(c.t).unwrap());
        let state = water.state(input).unwrap();
        let first = Partial { of: P, wrt: T, at: Dmolar };
        let p = Prop::SecondPartial(SecondPartial { first, wrt: T, at: Dmolar });
        let phase = state.phase();
        assert_eq!(water.prop(&state, p), Err(Error::Undefined { prop: p, phase }));
        let gamma = water.prop(&state, Prop::FundamentalDerivative);
        assert!(gamma.is_err(), "{gamma:?}");
    }

    /// SQ2, SQ3 (ROT-010, DIV-0004): at Q = 0 and 1 the partials, κ_T, β, the second partials, Γ, c_p⁰ and the
    /// residual h, s and g are the saturated phase's, as CoolProp's are: on Water's 500 K isotherm and Air's 101 325 Pa
    /// isobar (whose dew point is at its own T, E3) each is, bit for bit, the single phase's at that phase's T and ρ.
    /// Strictly inside the dome they stay `Undefined`, but for c_p⁰, a function of T alone: the dilute gas's at the
    /// state's T, Air's between its bubble and dew temperatures.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn saturated_derivs_outputs_are_the_saturated_phase_s() {
        use DerivVar::{Dmolar, Hmass, P, Smolar, T};
        let registry = crate::Registry::embedded().unwrap();
        let (water, air) = (registry.get("Water").unwrap(), registry.get("Air").unwrap());
        let first = Partial { of: Hmass, wrt: P, at: T };
        let cp0 = [Prop::Cp0molar, Prop::Cp0mass];
        let refused = [
            Prop::Partial(first),
            Prop::IsothermalCompressibility,
            Prop::IsobaricExpansion,
            Prop::SecondPartial(SecondPartial { first: Partial { of: P, wrt: Dmolar, at: T }, wrt: T, at: Dmolar }),
            Prop::SecondPartial(SecondPartial { first, wrt: Smolar, at: P }),
            Prop::FundamentalDerivative,
            Prop::HmolarResidual,
            Prop::SmolarResidual,
            Prop::GmolarResidual,
        ];
        let outputs = [&cp0[..], &refused].concat();
        let sides: [(&Fluid, fn(Quality) -> Input); 2] = [
            (water, |q| Input::qt(q, Temperature::new(500.0).unwrap())),
            (air, |q| Input::pq(Pressure::new(101_325.0).unwrap(), q)),
        ];
        let same = |fluid: &Fluid, a: &State, b: &State, props: &[Prop]| {
            for &prop in props {
                let (got, want) = (fluid.prop(a, prop).unwrap(), fluid.prop(b, prop).unwrap());
                assert_eq!(got.to_bits(), want.to_bits(), "{a:?} {prop:?}: {got}, the single phase's {want}");
            }
        };
        for (fluid, input) in sides {
            let split = |q| fluid.state(input(Quality::new(q).unwrap())).unwrap();
            for (q, phase) in [(0.0, Phase::Liquid), (1.0, Phase::Gas)] {
                let side = split(q).one_phase().unwrap();
                let at =
                    Input::dt(Density::molar(side.rho(Basis::Molar)).unwrap(), Temperature::new(side.t()).unwrap());
                same(fluid, &split(q), &fluid.flash(at, &FlashOptions::new().with_phase(phase)).unwrap(), &outputs);
            }
            let wet = split(0.5);
            for prop in refused {
                assert_eq!(fluid.prop(&wet, prop), Err(Error::Undefined { prop, phase: Phase::TwoPhase }));
            }
            let dilute = fluid.state(Input::dt(Density::molar(1.0).unwrap(), Temperature::new(wet.t()).unwrap()));
            same(fluid, &wet, &dilute.unwrap(), &cp0);
        }
    }

    /// M7.10: a second partial along T at constant ρ, or along ρ at constant T, is the central difference of its
    /// first partial at T(1 ± 1e-5) or ρ(1 ± 1e-5), within 1e-6 of the larger of it and the first partial over T or
    /// ρ, for first partials of p, h, s, u and g in both bases; and the mixed second partial of p is symmetric.
    #[test]
    fn second_partials_are_the_first_partials_differentiated() {
        use DerivVar::{Dmass, Dmolar, Gmass, Hmolar, P, Smass, Smolar, T, Umolar};
        let fluid = Fluid::new(Arc::new(FluidRecord::synthetic("X").unwrap().compile().unwrap()));
        let gas = FlashOptions::new().with_phase(Phase::Gas);
        let state = |rho: f64, t: f64| {
            fluid.flash(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap()), &gas).unwrap()
        };
        let first = |s: &State, p: Partial| fluid.prop(s, Prop::Partial(p)).unwrap();
        let (rho, t, h) = (500.0, 350.0, 1e-5);
        let here = state(rho, t);
        let firsts = [(P, T, Dmolar), (Hmolar, Dmolar, T), (Smolar, P, T), (Umolar, T, P), (Gmass, Smass, Dmass)];
        for (of, wrt, at) in firsts {
            let p = Partial { of, wrt, at };
            let central = |up: State, down: State, x: f64| (first(&up, p) - first(&down, p)) / (2.0 * h * x);
            let along = [
                (T, Dmolar, central(state(rho, t * (1.0 + h)), state(rho, t * (1.0 - h)), t), t),
                (Dmolar, T, central(state(rho * (1.0 + h), t), state(rho * (1.0 - h), t), rho), rho),
            ];
            for (wrt2, at2, want, x) in along {
                let got =
                    fluid.prop(&here, Prop::SecondPartial(SecondPartial { first: p, wrt: wrt2, at: at2 })).unwrap();
                let scale = got.abs().max(first(&here, p).abs() / x);
                assert!((got - want).abs() <= 1e-6 * scale, "{p:?} by {wrt2:?} at {at2:?}: {got} vs {want}");
            }
        }
        let mixed = |of_wrt: DerivVar, then: DerivVar| {
            let first = Partial { of: P, wrt: of_wrt, at: then };
            fluid.prop(&here, Prop::SecondPartial(SecondPartial { first, wrt: then, at: of_wrt })).unwrap()
        };
        let (tr, rt) = (mixed(T, Dmolar), mixed(Dmolar, T));
        assert!((tr - rt).abs() <= 1e-12 * tr.abs(), "{tr} vs {rt}");
    }

    /// Rot: ROT-036 (map 02 §5 row 7, map 11 F8). CoolProp keys its melting data by fluid name in one process-global
    /// cache, built under a mutex with the first caller's EOS. Here the melting curve is part of the model: in one
    /// process the `Parity` and `Corrected` registries' Water, the same name, each answer their own ice VI T_melt at
    /// 1356.76 MPa (320.965 and 320.000 K, DIV-0002), in either order and (off wasm) from four threads at once, and a
    /// record edited after both carries its own curve.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn melting_caloric_data_lives_in_the_model() {
        use crate::{DataSet, Registry};
        let t_melt =
            |set| Registry::from_embedded(set).unwrap().get("Water").unwrap().model().melting_temperature(1356.76e6);
        let (corrected, parity) = (t_melt(DataSet::Corrected).unwrap(), t_melt(DataSet::Parity).unwrap());
        assert!((corrected - 320.0).abs() < 1e-3 && (parity - 320.965).abs() < 1e-3, "{corrected} {parity}");
        #[cfg(not(target_family = "wasm"))]
        {
            let both = std::thread::scope(|scope| {
                let threads: Vec<_> = [DataSet::Parity, DataSet::Corrected, DataSet::Parity, DataSet::Corrected]
                    .map(|set| scope.spawn(move || t_melt(set)))
                    .into_iter()
                    .collect();
                threads.into_iter().map(|t| t.join().unwrap().unwrap()).collect::<Vec<_>>()
            });
            assert_eq!(both, [parity, corrected, parity, corrected]);
        }
        let mut record = Registry::embedded().unwrap().record("Water").unwrap();
        record.melting[3].p0 = 640e6;
        let edited = Fluid::new(Arc::new(record.compile().unwrap()));
        assert!(edited.model().melting_temperature(1356.76e6).is_ok_and(|t| t < corrected - 0.5));
    }
}
