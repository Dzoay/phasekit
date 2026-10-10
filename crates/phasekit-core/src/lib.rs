//! `phasekit-core`: the kernel of phasekit, a Rust successor to CoolProp (name: D16). One crate, private layered
//! modules whose dependencies point down (D1); this file re-exports the whole public API (S-03):
//!
//! | Layer | Modules | Test seam |
//! |---|---|---|
//! | L6 exec | `batch` | every `ExecPolicy` equals `Reference` |
//! | L5 transport | `transport` | stage-by-stage paper rows |
//! | L4 solve | `flash`, `saturation`, `state` | round trips, capability matrix, `SolvePath` |
//! | L3 package | `model`, `fluid`, `data`, `registry` | Parity vs Corrected, lazy-load counts |
//! | L2 model | `helmholtz` | jets vs AD, FD, paper α tables |
//! | L1 relations | `relations`, `derivs` | identities on synthetic bundles |
//! | L0 numerics | `num` (`math`), `units`, `input`, `prop`, `error` | unit tests |
//!
//! ```
//! # fn main() -> Result<(), phasekit_core::Error> {
//! use phasekit_core::{Basis, Density, Error, FlashOptions, Input, LoadError, Registry, Temperature};
//! let input = Input::dt(Density::mass(996.55)?, Temperature::new(300.0)?);
//! match Registry::embedded()?.get("Water") {
//!     // &Fluid: decoded once, then an atomic load per lookup.
//!     Ok(water) => match water.flash(input, &FlashOptions::default()) {
//!         Ok(state) => {
//!             let _ = (state.h(Basis::Mass), state.path());
//!         }
//!         // Until the DT flash lands (M5) it is a typed "not yet", never a wrong value.
//!         Err(Error::Unsupported { .. }) => {}
//!         Err(e) => return Err(e),
//!     },
//!     // A blob that cannot be decoded or compiled is a typed load error, never a partial model.
//!     Err(Error::Load(LoadError::Format(_))) => {}
//!     Err(e) => return Err(e),
//! }
//! # Ok(()) }
//! ```

mod blob;
mod crit;
mod data;
mod density;
mod derivs;
mod error;
mod flash;
mod fluid;
mod helmholtz;
mod input;
mod model;
mod num;
mod prop;
mod registry;
mod relations;
mod roots;
mod saturation;
mod state;
mod transport;
mod units;
mod vle;

pub mod batch;
pub use num::math;

pub use data::{Blob, DataSet, DataSource, FluidId, Pack};
pub use derivs::{Bundle, Derivs, Order, PointDerivs, Virials};
pub use error::{DomainError, Error, LoadError, Roots};
pub use flash::{DomainPolicy, FlashOptions, RootPolicy};
pub use fluid::{Fluid, Gauge, PureFluid, PureFluidBuilder, ReferenceState};
pub use helmholtz::HelmholtzModel;
pub use input::{Capabilities, Input, NativeInput, Pair, Var};
pub use model::{
    Citation, CitationRole, CriticalOrigin, CriticalPoint, DataTerms, FluidInfo, Limits, ModelKey, Source, ThermoModel,
};
pub use num::{Jet4, Real};
pub use prop::{DerivVar, Partial, Prop};
pub use registry::Registry;
pub use relations::{GibbsDerivs, bundle_from_gibbs};
pub use saturation::{SatAccuracy, SatPair, SatSide, SaturationCurve};
pub use state::{Phase, SolvePath, State, Strategy};
pub use units::{Basis, Density, Enthalpy, Entropy, InternalEnergy, Pressure, Quality, Temperature};

/// Semver-exempt items for `phasekit-verify` and `phasekit-xtask` only (datagen, arbitration, the divergence
/// register). Not API: these records grow every milestone (M4 ancillaries, M6 superancillary, M8 transport).
#[doc(hidden)]
pub mod internal {
    pub use crate::data::{
        Ancillary, CaloricCurves, CaloricFreshness, CaloricStamp, CaloricView, Edit, Environmental, EosRecord,
        FluidRecord, MeltingSegment, Patch, PseudoPure, SaFreshness, SaStamp, Superancillary,
    };
    pub use crate::helmholtz::{
        DoubleExponentialTerm, GaoBTerm, GaussianTerm, IdealTerm, Lemmon2005Term, MAX_POW, NonAnalyticTerm,
        OffsetReference, PowerTerm,
    };
    pub use crate::saturation::{chebyshev_derivative, clenshaw, piece};

    /// Saturation of a Helmholtz model at `t` by the pure-fluid VLE from seed densities `(ρ′, ρ″)` (PLAN.md M6.3).
    pub fn vle_at_t(eos: &dyn crate::HelmholtzModel, t: f64, seed: (f64, f64)) -> Result<crate::SatPair, crate::Error> {
        crate::vle::at_t(eos, t, seed)
    }

    /// Saturation of a Helmholtz model at the pressure `p` by the pure-fluid VLE from the seed `(T, ρ′, ρ″)`.
    pub fn vle_at_p(
        eos: &dyn crate::HelmholtzModel,
        p: f64,
        seed: (f64, f64, f64),
    ) -> Result<crate::SatPair, crate::Error> {
        crate::vle::at_p(eos, p, seed)
    }

    /// The density of a Helmholtz model at (T, p) on the liquid (`liquid`) or vapour branch, from a seed (PLAN.md M6.9).
    pub fn density_at_t_p(
        eos: &dyn crate::HelmholtzModel,
        t: f64,
        p: f64,
        seed: f64,
        liquid: bool,
    ) -> Result<f64, crate::Error> {
        crate::density::at_t_p(eos, t, p, seed, liquid).map(|(rho, _)| rho)
    }

    /// The liquid density of a Helmholtz model at (T, p) above `lo`, a liquid density below p (PLAN.md M7.1: datagen's
    /// `rho_max`, ρ(T_min, p_max)).
    pub fn liquid_density_above(eos: &dyn crate::HelmholtzModel, t: f64, p: f64, lo: f64) -> Result<f64, crate::Error> {
        crate::density::liquid_above(eos, t, p, lo)
    }

    /// The numerical critical point (T, ρ, p) of a Helmholtz model near a guess (PLAN.md M6.7).
    pub fn numerical_critical_point(
        eos: &dyn crate::HelmholtzModel,
        t: f64,
        rho: f64,
    ) -> Result<(f64, f64, f64), crate::Error> {
        crate::crit::numerical(eos, t, rho)
    }

    /// The decoded, uncorrected record of a data-backed fluid.
    pub fn record(registry: &crate::Registry, name: &str) -> Result<FluidRecord, crate::Error> {
        registry.record(name)
    }

    /// One fluid of a pack: names (canonical first), the canonical names of its references, its v1 blob.
    pub use crate::blob::PackFluid;

    /// The v1 pack of `fluids`, which [`crate::Pack::new`] reads (tooling and tests; a browser fetches one).
    pub fn pack(fluids: &[PackFluid]) -> Vec<u8> {
        crate::blob::pack(fluids)
    }

    /// The sections of blob format v1: (id, name, the step that fills a section the decoder cannot read yet). The
    /// readable dump (`cargo xtask fluid`) covers every section the decoder reads (PLAN.md §2.5, M2.9a).
    pub const BLOB_SECTIONS: &[(u32, &str, Option<&str>)] = &crate::blob::SECTIONS;

    /// A model key read back from its stored value (the readable dump's superancillary stamp).
    pub const fn model_key(raw: u64) -> crate::ModelKey {
        crate::ModelKey::from_raw(raw)
    }
}

// D8 by construction: shared types are Send + Sync + 'static, and `State` stays a small `Copy` value
// (no `Arc`, no cache). Adding a `Cell`, `Rc` or non-'static borrow to any of them breaks the build.
const _: () = {
    const fn shared<T: ?Sized + Send + Sync + 'static>() {}
    shared::<Registry>();
    shared::<Fluid>();
    shared::<PureFluid>();
    shared::<helmholtz::MultiParameterEos>();
    shared::<dyn ThermoModel>();
    shared::<dyn HelmholtzModel>();
    shared::<dyn SaturationCurve>();
    shared::<SatPair>();
    shared::<dyn DataSource>();
    shared::<State>();
    shared::<FlashOptions>();
    shared::<Error>();
    assert!(core::mem::size_of::<State>() <= 256);
    assert!(core::mem::size_of::<Error>() <= 48);
};
