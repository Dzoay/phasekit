//! `phasekit-mix`: multi-fluid Helmholtz mixtures as a model family on phasekit-core's public seams (D4; PLAN.md M13).
//! A mixture at a fixed composition is a `HelmholtzModel` with its own `ThermoModel`: Kunz and Wagner's form, the model
//! of CoolProp's HEOS mixtures, over CoolProp 8.0.0's library of binary pairs and departure functions, and GERG-2008.
//!
//! Its milestone holds phasekit-core unchanged from `m13-start` (`cargo xtask gates core-frozen`), after M13.1 added
//! the seams a mixture needs (user decision MX2).

mod composition;
mod corrections;
mod critical;
// The predefined mixtures are read from PLAN.md M13.13; until then only the tests read them.
#[cfg_attr(not(test), expect(dead_code, reason = "the predefined mixtures are built from PLAN.md M13.13"))]
mod data;
mod departure;
#[cfg_attr(not(test), expect(dead_code, reason = "the predefined mixtures are built from PLAN.md M13.13"))]
#[rustfmt::skip] // datagen's output, one table row a line (`gates datagen` compares it byte for byte)
mod generated;
mod gerg;
mod mixture;
mod multifluid;
mod reducing;
mod taylor;

pub use composition::{Composition, MixtureModel, Var};
pub use critical::{CriticalState, Criticality};
pub use gerg::{PREFIX as GERG_PREFIX, R_GERG, with_gerg2008};
pub use mixture::{Mixture, R_CODATA, gerg2008, mixture};
pub use multifluid::MultiFluid;

/// What `cargo xtask` and the verification kit read of the crate's data; not a stable API.
#[doc(hidden)]
pub mod internal {
    /// GERG-2008's transcription keyed as AGA8's `SetupGERG` assigns it: its array name, indices from 1 and the
    /// value, for `cargo xtask`'s `gerg_2008_is_aga8s` (PLAN.md M13.5).
    pub fn gerg_2008_aga8() -> Vec<(&'static str, Vec<usize>, f64)> {
        crate::gerg::aga8_entries()
    }

    /// Each GERG-2008 component's ideal-gas n₁ and n₂ in AGA8's form (its `n0i[i][1]`, `n0i[i][2]`), as computed for
    /// h = s = 0 at 298.15 K and 101.325 kPa, in AGA8's order: AGA8 prints them to ten digits.
    pub fn gerg_2008_n1_n2() -> Vec<[f64; 2]> {
        crate::gerg::aga8_n1_n2()
    }

    /// `mixture`'s critical points with every R_i/R taken as 1: v8.0.0's L* = δ_ij/x_i + n ∂²(n α^r)/∂n_i ∂n_j and
    /// its M*, whose ideal part is unweighted where its α⁰ weights each ln x_i (DIV-0052, PLAN.md M13.7).
    pub fn critical_points_with_unit_weights(mixture: &crate::Mixture) -> Vec<crate::CriticalState> {
        mixture.critical_points_weighted(&vec![1.0; mixture.composition().len()])
    }
}

#[cfg(test)]
mod tests;
