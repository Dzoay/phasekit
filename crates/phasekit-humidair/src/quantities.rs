//! The quantities humid air's refusals name, beside core's ([`phasekit_core::Quantity`]; PLAN.md M28.1).

use phasekit_core::{Dimension, Quantity};

/// The humidity ratio W, kg of water per kg of dry air.
pub const HUMIDITY_RATIO: &Quantity = &Quantity::new("humidity ratio", "W", Dimension::MassRatio);
/// The mole fraction of water ψ_w.
pub const WATER_MOLE_FRACTION: &Quantity = &Quantity::new("mole fraction of water", "ψ_w", Dimension::Fraction);
/// The relative humidity φ.
pub const RELATIVE_HUMIDITY: &Quantity = &Quantity::new("relative humidity", "φ", Dimension::Fraction);
/// A liquid or ice part's molar mass, which must be water's.
pub const PART_MOLAR_MASS: &Quantity =
    &Quantity::new("molar mass of the liquid or ice part", "M", Dimension::MolarMass);
