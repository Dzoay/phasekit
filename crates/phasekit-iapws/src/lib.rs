//! `phasekit-iapws`: what humid air takes from the IAPWS formulations, on phasekit-core's public seams (PLAN.md M14;
//! user decisions HA1, HA4, HA5): IAPWS-IF97's water below 623.15 K, regions 1, 2 and 4 (IAPWS R7-97(2012)), and ice
//! Ih (IAPWS R10-06(2009)) with its sublimation curve (IAPWS R14-08(2011)). Each is a Gibbs function turned into
//! core's bundle by `bundle_from_gibbs`, so every property relation is core's. Neither is named in compat's strings yet
//! (HA5): v8.0.0's `IF97::Water` spans all five regions, which M27 adds.
//!
//! It depends on phasekit-core's public API alone (`cargo xtask gates deps`). Its arbiters are the releases' own check
//! tables, which phasekit-verify's `tests/iapws.rs` reads; the IAPWS asks that its releases be attributed when they
//! are used, so every coefficient table here names its release and table.

// PLAN.md M24 (issue #219): no implicit panic, an index out of bounds or an integer overflow, in the library's code
// (`gates lints` checks this line); its tests are exempt. Humid air reaches this crate.
#![cfg_attr(not(test), deny(clippy::indexing_slicing, clippy::arithmetic_side_effects))]

pub mod ice;
pub mod if97;
