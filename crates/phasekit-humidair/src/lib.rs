//! `phasekit-humidair`: humid air as ASHRAE RP-1485 models it (Herrmann, Kretzschmar and Gatley 2009), a virial
//! mixture of dry air and water vapour, on phasekit-core's public API alone (PLAN.md M18; user decisions HA1-HA6). It
//! takes its four parts as `Fluid`s at construction (map 07 H3, ROT-263): IAPWS-95 water (its virials and ideal gas),
//! Lemmon's dry air, IF97 water (the saturation line and the liquid above 0 °C) and ice Ih (the sublimation curve and
//! the solid below). A state takes three inputs, one a humidity, which no core `Pair` names, so `HumidAir` is its own
//! type, not a `ThermoModel` (ARCHITECTURE.md §11).
//!
//! Its arbiters are the authors' printed tables and the IAPWS releases (HA2): G11-15's and G8-10's virials, G7-04's
//! Henry's constants. Where an equation is RP-1485's own, whose report is not in hand, it is taken as v8.0.0's
//! `HumidAirProp.cpp` writes it, and the tables arbitrate the result.

// PLAN.md M24 (issue #219): no implicit panic, an index out of bounds or an integer overflow, in the library's code
// (`gates lints` checks this line); its tests are exempt. The C ABI and the browser package reach this crate.
#![cfg_attr(not(test), deny(clippy::indexing_slicing, clippy::arithmetic_side_effects))]

pub mod constants;
mod dew_point;
mod given;
mod henry;
mod humid_air;
mod humidity;
mod ideal;
mod moist_air;
mod root;
mod saturation;
mod virials;
mod wet_bulb;

pub use given::Given;
pub use henry::{Gas, henry_constant};
pub use humid_air::{HumidAir, Parts};
pub use humidity::Humidity;
pub use moist_air::{MoistAir, Per};
pub use saturation::Saturation;
pub use virials::{Coefficient, Virials};

#[cfg(test)]
mod tests;
