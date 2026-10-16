//! `phasekit-mix`: multi-fluid Helmholtz mixtures as a model family on phasekit-core's public seams (D4; PLAN.md M13).
//! A mixture at a fixed composition is a `HelmholtzModel` with its own `ThermoModel`: Kunz and Wagner's form, the model
//! of CoolProp's HEOS mixtures, over CoolProp 8.0.0's library of binary pairs and departure functions, and GERG-2008.
//!
//! Its milestone holds phasekit-core unchanged from `m13-start` (`cargo xtask gates core-frozen`), after M13.1 added
//! the seams a mixture needs (user decision MX2).

mod corrections;
// The predefined mixtures are read from PLAN.md M13.13; until then only the tests read them.
#[cfg_attr(not(test), expect(dead_code, reason = "the predefined mixtures are built from PLAN.md M13.13"))]
mod data;
mod departure;
#[cfg_attr(not(test), expect(dead_code, reason = "the predefined mixtures are built from PLAN.md M13.13"))]
#[rustfmt::skip] // datagen's output, one table row a line (`gates datagen` compares it byte for byte)
mod generated;
mod multifluid;
mod reducing;

pub use multifluid::MultiFluid;

#[cfg(test)]
mod tests;
