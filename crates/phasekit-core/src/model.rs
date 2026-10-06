//! L3 package contracts: model identity and metadata, and [`ThermoModel`], the currency of the registry, the
//! batch driver and every facade.

use core::fmt;

use crate::derivs::{Order, PointDerivs};
use crate::error::{DomainError, Error};
use crate::flash::FlashOptions;
use crate::helmholtz::HelmholtzModel;
use crate::input::{Capabilities, NativeInput};
use crate::prop::Prop;
use crate::state::State;

/// Content hash of a compiled model (FNV-1a 64 over its canonical bytes, map 09 hash practice): for data
/// fluids, the whole canonical EOS section (α^r, α⁰, R, reducing state) plus M and name (E14). Stored in
/// every `State`, so a state is refused by any other model, and later used as the cache key of surrogates
/// (map 08, map 15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModelKey(u64);

impl ModelKey {
    /// FNV-1a 64 of `bytes`.
    pub const fn from_content(bytes: &[u8]) -> Self {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut i = 0;
        while i < bytes.len() {
            h ^= bytes[i] as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
            i += 1;
        }
        Self(h)
    }
    /// The raw hash.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Licence and publication status of a model's data (D14).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataTerms {
    /// Published in the open literature.
    Published,
    /// Unpublished (Propylene, SES36, Neon: oracle-only fixtures, map 13 R7).
    Unpublished,
    /// Redistribution needs clearance; never in default features (map 09).
    Restricted,
}

/// Provenance of a model or dataset (materials S8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// BibTeX key in the project bibliography.
    pub bibkey: Box<str>,
    /// DOI, if any.
    pub doi: Option<Box<str>>,
    /// Licence / publication status.
    pub terms: DataTerms,
}

/// Where a critical point comes from: the paper's metadata, or the model itself (they differ by up to
/// 2.3 K in 25 fluids, map 02; CoolProp switched between them by a global flag, map 03 §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CriticalOrigin {
    /// Published metadata.
    Published,
    /// Solved from (or exact for) the model.
    Model,
}

/// A critical point and its origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CriticalPoint {
    /// Temperature, K.
    pub t: f64,
    /// Pressure, Pa.
    pub p: f64,
    /// Molar density, mol/m³.
    pub rho: f64,
    /// Origin.
    pub origin: CriticalOrigin,
}

/// Identity and metadata: names, molar mass, published critical point, provenance and content key.
#[derive(Clone, Debug, PartialEq)]
pub struct FluidInfo {
    name: Box<str>,
    aliases: Box<[Box<str>]>,
    molar_mass: f64,
    published_critical: Option<CriticalPoint>,
    source: Source,
    key: ModelKey,
}

impl FluidInfo {
    /// Validates the molar mass (kg/mol).
    pub fn new(name: &str, molar_mass: f64, source: Source, key: ModelKey) -> Result<Self, Error> {
        if !(molar_mass.is_finite() && molar_mass > 0.0) {
            return Err(Error::InvalidInput { quantity: "molar mass", value: molar_mass });
        }
        Ok(Self { name: name.into(), aliases: Box::new([]), molar_mass, published_critical: None, source, key })
    }
    /// Adds aliases (and CAS numbers); the registry matches them case-insensitively.
    pub fn with_aliases(mut self, aliases: &[&str]) -> Self {
        self.aliases = aliases.iter().map(|a| Box::from(*a)).collect();
        self
    }
    /// Records the paper's critical point (metadata, never used for phase decisions).
    pub fn with_published_critical(mut self, c: CriticalPoint) -> Self {
        self.published_critical = Some(c);
        self
    }
    /// Canonical name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Aliases.
    pub fn aliases(&self) -> impl Iterator<Item = &str> {
        self.aliases.iter().map(|a| &**a)
    }
    /// Molar mass, kg/mol.
    pub fn molar_mass(&self) -> f64 {
        self.molar_mass
    }
    /// Published critical point, if recorded.
    pub fn published_critical(&self) -> Option<CriticalPoint> {
        self.published_critical
    }
    /// Provenance.
    pub fn source(&self) -> &Source {
        &self.source
    }
    /// Content key.
    pub fn key(&self) -> ModelKey {
        self.key
    }
}

/// Validity domain. Private fields so it can grow holes and melting curves without a break (materials S4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    t_min: f64,
    t_max: f64,
    p_max: f64,
    t_triple: Option<f64>,
}

impl Limits {
    /// Validates `t_min < t_max` and `p_max > 0`.
    pub fn new(t_min: f64, t_max: f64, p_max: f64) -> Result<Self, Error> {
        if !(t_min > 0.0 && t_max > t_min && p_max > 0.0) {
            return Err(Error::InvalidInput { quantity: "limits", value: t_min });
        }
        Ok(Self { t_min, t_max, p_max, t_triple: None })
    }
    /// Records the triple-point temperature, which is NOT `t_min` (map 09 R8: CoolProp conflates them).
    pub fn with_t_triple(mut self, t: f64) -> Self {
        self.t_triple = Some(t);
        self
    }
    /// The model's lower temperature limit, K (CoolProp's T_min: the saturation minimum, kept apart from the triple
    /// point, map 09 R8).
    pub fn t_min(&self) -> f64 {
        self.t_min
    }
    /// The model's upper temperature limit, K.
    pub fn t_max(&self) -> f64 {
        self.t_max
    }
    /// The model's upper pressure limit, Pa.
    pub fn p_max(&self) -> f64 {
        self.p_max
    }
    /// Triple-point temperature, if known.
    pub fn t_triple(&self) -> Option<f64> {
        self.t_triple
    }
    /// Checks a temperature. The lower bound is the higher of `t_min` and the triple point: below the triple
    /// point is refused unless the caller opts into `DomainPolicy::Extrapolate` (D6).
    pub fn check_t(&self, t: f64) -> Result<(), DomainError> {
        let t_min = self.t_triple.map_or(self.t_min, |tt| tt.max(self.t_min));
        if t < t_min {
            Err(DomainError::BelowMinTemperature { t, t_min })
        } else if t > self.t_max {
            Err(DomainError::AboveMaxTemperature { t, t_max: self.t_max })
        } else {
            Ok(())
        }
    }
    /// Checks a pressure.
    pub fn check_p(&self, p: f64) -> Result<(), DomainError> {
        if p > self.p_max { Err(DomainError::AboveMaxPressure { p, p_max: self.p_max }) } else { Ok(()) }
    }
}

/// Anything that turns an input into a [`State`]: what the registry stores, what the batch driver and every
/// facade call (D3, D10). Helmholtz fluids implement it through [`crate::PureFluid`]; a Gibbs solid, IF97 or
/// an INCOMP correlation implements it directly in its own crate. The model never sees a reference state or
/// a mass basis: [`crate::Fluid`] hands it a [`NativeInput`] (map 15).
pub trait ThermoModel: Send + Sync + fmt::Debug {
    /// Identity, molar mass, provenance and content key.
    fn info(&self) -> &FluidInfo;

    /// Declared input pairs (map 01 R21). The handle refuses others with `Error::Unsupported`.
    fn capabilities(&self) -> Capabilities;

    /// The flash: a pure function of the model and its arguments. Returns a new `State` or a typed error.
    fn flash(&self, input: NativeInput, opts: &FlashOptions) -> Result<State, Error>;

    /// The model's own critical point (used for phase labels), if it has one.
    fn critical_point(&self) -> Option<CriticalPoint> {
        None
    }

    /// Raw Helmholtz access for Helmholtz-explicit models: verification against CoolProp's `alphar`
    /// outputs, borrowed ideal gases, exact virials (`zero_density`).
    fn helmholtz(&self) -> Option<&dyn HelmholtzModel> {
        None
    }

    /// Derivatives beyond the stored order-2 bundle at a single-phase state this model produced (E1): order
    /// 3-4 and the ideal/residual split. The default evaluates [`ThermoModel::helmholtz`]; a Gibbs family
    /// overrides it with its own order-3 transform. `None`: not available (no Helmholtz part and no
    /// override, or a two-phase state); outputs that need it report `NoModel`.
    fn derivs(&self, state: &State, order: Order) -> Option<PointDerivs> {
        let (t, rho) = state.single_t_rho()?;
        let h = self.helmholtz()?;
        Some(PointDerivs::split(h.ideal(t, rho, order), h.residual(t, rho, order)))
    }

    /// Dynamic viscosity, Pa s, at a state this model produced.
    fn viscosity(&self, _state: &State) -> Result<f64, Error> {
        Err(Error::NoModel { prop: Prop::Viscosity })
    }

    /// Thermal conductivity, W/(m K), at a state this model produced.
    fn conductivity(&self, _state: &State) -> Result<f64, Error> {
        Err(Error::NoModel { prop: Prop::Conductivity })
    }

    /// Surface tension, N/m, of the saturated liquid at `t`.
    fn surface_tension(&self, _t: f64) -> Result<f64, Error> {
        Err(Error::NoModel { prop: Prop::SurfaceTension })
    }
}
