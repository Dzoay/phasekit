//! L2 Helmholtz models: the family seam (D3) and the built-in CoolProp multiparameter family.

mod eos;
mod ideal;
mod power;
mod tau_exp;
mod terms;

use core::fmt;

pub(crate) use eos::{MultiParameterEos, ResidualBlock};
pub(crate) use ideal::IdealGas;
pub use ideal::{IdealTerm, OffsetReference};
pub(crate) use power::PowerBlock;
pub use power::{MAX_POW, PowerTerm};
pub(crate) use tau_exp::TauExpBlock;
pub use terms::{DoubleExponentialTerm, GaoBTerm, GaussianTerm, Lemmon2005Term, NonAnalyticTerm};

use crate::derivs::{Derivs, Order, Virials};

/// The seam for Helmholtz-explicit model families: CoolProp multiparameter (built in), cubic, PC-SAFT,
/// GERG and mixtures at fixed composition (later crates). Object-safe; one virtual call per evaluation,
/// never per term. The contract is in (T, ρ) and returns reducing-INVARIANT bundles, so no family has to
/// invent a reducing state (map 06 C10, D4) and no τ/δ crosses a model boundary (map 06 C1).
pub trait HelmholtzModel: Send + Sync + fmt::Debug {
    /// Molar gas constant of THIS model, J/(mol K) (map 12 R7: CoolProp mixes 8 distinct values).
    fn gas_constant(&self) -> f64;

    /// Residual bundle α^r at (T in K, ρ in mol/m³), unchecked: metastable and unstable states are allowed.
    /// Requires T > 0 and ρ ≥ 0; must be finite at ρ = 0.
    fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs;

    /// Ideal-gas bundle α⁰ at (T, ρ), evaluated in the model's own reducing variables. Its δ-part is always
    /// `ln δ`, so `residual + Derivs::IDEAL_DELTA` gives the mechanical derivatives without calling this.
    fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs;

    /// Upper molar-density bound for root bracketing at `t`: covolume, packing limit or a datagen bound.
    /// Comes from the model, never from invented metadata (map 06 C10).
    fn rho_max(&self, t: f64) -> f64;

    /// Exact virial coefficients from the δ → 0 Taylor coefficients of α^r (E4). `None` when the family
    /// has no closed form: virial outputs are then refused, never approximated at a small δ (map 12 R8).
    fn zero_density(&self, _t: f64) -> Option<Virials> {
        None
    }
}
