//! The quantities a mixture's refusals name, beside core's ([`phasekit_core::Quantity`]; PLAN.md M28.1).

use phasekit_core::{Dimension, Quantity};

/// The sum of a composition's mole fractions, which must be 1.
pub const MOLE_FRACTION_SUM: &Quantity = &Quantity::new("sum of the mole fractions", "Σx", Dimension::Dimensionless);
/// The vapour's molar share of a two-phase state, β.
pub const VAPOUR_FRACTION: &Quantity = &Quantity::new("vapour fraction", "β", Dimension::Fraction);
/// The pressure a phase envelope's trace starts at.
pub const START_PRESSURE: &Quantity = &Quantity::new("starting pressure", "p_start", Dimension::Pressure);
/// The highest pressure a phase envelope's trace may reach.
pub const HIGHEST_PRESSURE: &Quantity = &Quantity::new("highest pressure", "p_max", Dimension::Pressure);
/// A binary pair's reducing parameter, β or γ.
pub const BINARY_PARAMETER: &Quantity = &Quantity::new("binary parameter", "β", Dimension::Dimensionless);
/// A departure function's weight F.
pub const DEPARTURE_WEIGHT: &Quantity = &Quantity::new("departure function's F", "F", Dimension::Dimensionless);
