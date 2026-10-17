//! `phasekit-mix`: multi-fluid Helmholtz mixtures as a model family on phasekit-core's public seams (D4; PLAN.md M13).
//! A mixture at a fixed composition is a `HelmholtzModel` with its own `ThermoModel`: Kunz and Wagner's form, the model
//! of CoolProp's HEOS mixtures, over CoolProp 8.0.0's library of binary pairs and departure functions, and GERG-2008.
//!
//! Its milestone holds phasekit-core unchanged from `m13-start` (`cargo xtask gates core-frozen`), after M13.1 added
//! the seams a mixture needs (user decision MX2).

mod composition;
mod corrections;
mod critical;
// The predefined mixtures are built from PLAN.md M13.13; until then only the kit reads their fractions.
#[cfg_attr(not(test), expect(dead_code, reason = "the predefined mixtures are built from PLAN.md M13.13"))]
mod data;
mod departure;
mod envelope;
mod equilibrium;
#[rustfmt::skip] // datagen's output, one table row a line (`gates datagen` compares it byte for byte)
mod generated;
mod gerg;
mod illinois;
mod mixture;
mod multifluid;
mod reducing;
mod saturation;
mod taylor;

pub use composition::{Composition, MixtureModel, Var};
pub use critical::{CriticalState, Criticality};
pub use envelope::{Envelope, EnvelopeEnd, EnvelopeOptions};
pub use gerg::{PREFIX as GERG_PREFIX, R_GERG, with_gerg2008};
pub use mixture::{Equilibrium, Mixture, R_CODATA, gerg2008, mixture};
pub use multifluid::MultiFluid;
pub use saturation::{Given, Saturation, Side};

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

    /// The stable density root of `model` at (T, p) as the PT flash selects it: of the roots on the two branches, those
    /// their branch reaches with the pressure rising, the one of least Gibbs energy (PLAN.md M13.8).
    pub fn stable_root(model: &dyn phasekit_core::HelmholtzModel, t: f64, p: f64) -> Result<f64, phasekit_core::Error> {
        crate::equilibrium::stable_root(model, t, p)
    }

    /// The bubble or dew point of `mixture` on `side` at `given` from `seed` (T, p, ln K) alone: M13.9's Newton without
    /// its own seeds, which checks a point another implementation found (PLAN.md M13.12's oracle envelopes).
    pub fn saturation_from(
        mixture: &crate::Mixture,
        side: crate::Side,
        given: crate::Given,
        seed: (f64, f64, Vec<f64>),
    ) -> Result<crate::Saturation, phasekit_core::Error> {
        crate::saturation::saturation_from(mixture, side, given, seed)
    }

    /// v8.0.0's predefined mixture `name` (without `.mix`): its components and mole fractions, as its library gives
    /// them (PLAN.md M13.9's bubble and dew points; M13.13 registers them).
    pub fn predefined(name: &str) -> Option<(&'static [&'static str], &'static [f64])> {
        crate::generated::PREDEFINED.iter().find(|m| m.name == name).map(|m| (m.components, m.mole_fractions))
    }
}

#[cfg(test)]
mod tests;
