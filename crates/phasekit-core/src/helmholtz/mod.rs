//! L2 Helmholtz models: the family seam (D3) and the built-in CoolProp multiparameter family.

mod eos;
mod gao_b;
mod gaussian;
mod ideal;
mod non_analytic;
mod power;
mod tau_exp;
mod terms;

use core::fmt;

pub(crate) use eos::{MultiParameterEos, ResidualBlock};
pub(crate) use gao_b::GaoBTau;
pub(crate) use gaussian::{GaussianBlock, GaussianTau};
pub(crate) use ideal::IdealGas;
pub use ideal::{IdealTerm, OffsetReference};
pub(crate) use non_analytic::NonAnalyticBlock;
pub(crate) use power::PowerBlock;
pub use power::{MAX_POW, PowerTerm};
pub(crate) use tau_exp::TauExpBlock;
pub use terms::{DoubleExponentialTerm, GaoBTerm, GaussianTerm, Lemmon2005Term, NonAnalyticTerm};

use crate::derivs::{Derivs, Order, Virials};
use crate::error::Error;
use crate::exec::ExecPolicy;
use crate::model::Reducing;

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

    /// [`residual`](Self::residual) at every point `(t[i], rho[i])` into `out[i]`, under `exec` (D9): the bits of
    /// `residual` at that point whatever the policy. Unequal lengths are [`Error::Shape`]. The default runs
    /// `residual` point by point (PLAN.md M12.1).
    fn residual_batch(
        &self,
        t: &[f64],
        rho: &[f64],
        order: Order,
        out: &mut [Derivs],
        exec: ExecPolicy,
    ) -> Result<(), Error> {
        crate::exec::points(exec, t, rho, out, |t, rho| self.residual(t, rho, order))
    }

    /// Ideal-gas bundle α⁰ at (T, ρ), evaluated in the model's own reducing variables. Its δ-part is always
    /// `ln δ`, so `residual + Derivs::IDEAL_DELTA` gives the mechanical derivatives without calling this.
    fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs;

    /// [`ideal`](Self::ideal) at every point `(t[i], rho[i])` into `out[i]`, under `exec` (D9): the bits of `ideal` at
    /// that point whatever the policy. Unequal lengths are [`Error::Shape`]. The default runs `ideal` point by point
    /// (PLAN.md M12.8).
    fn ideal_batch(
        &self,
        t: &[f64],
        rho: &[f64],
        order: Order,
        out: &mut [Derivs],
        exec: ExecPolicy,
    ) -> Result<(), Error> {
        crate::exec::points(exec, t, rho, out, |t, rho| self.ideal(t, rho, order))
    }

    /// Upper molar-density bound for root bracketing at `t`: covolume, packing limit or a datagen bound.
    /// Comes from the model, never from invented metadata (map 06 C10).
    fn rho_max(&self, t: f64) -> f64;

    /// Exact virial coefficients from the δ → 0 Taylor coefficients of α^r (E4). `None` when the family
    /// has no closed form: virial outputs are then refused, never approximated at a small δ (map 12 R8).
    fn zero_density(&self, _t: f64) -> Option<Virials> {
        None
    }

    /// The state this model reduces by, τ = T_r/T and δ = ρ/ρ_r, where it has one: what a family that composes models
    /// needs to evaluate one at a τ and δ of its own (a multi-fluid mixture evaluates each component at the mixture's,
    /// through `residual(T_r/τ, δ·ρ_r)`; PLAN.md M13.1). The contract stays in (T, ρ); `None` by default.
    fn reducing(&self) -> Option<Reducing> {
        None
    }
}
