//! The quantities humid air's refusals name, beside core's ([`phasekit_core::Quantity`]; PLAN.md M28.1).

use phasekit_core::{Basis, Dimension, Quantity};

/// The humidity ratio W, kg of water per kg of dry air.
pub const HUMIDITY_RATIO: &Quantity = &Quantity::new("humidity ratio", "W", Dimension::MassRatio);
/// The mole fraction of water ψ_w.
pub const WATER_MOLE_FRACTION: &Quantity = &Quantity::new("mole fraction of water", "ψ_w", Dimension::Fraction);
/// The relative humidity φ.
pub const RELATIVE_HUMIDITY: &Quantity = &Quantity::new("relative humidity", "φ", Dimension::Fraction);
/// A liquid or ice part's molar mass, which must be water's.
pub const PART_MOLAR_MASS: &Quantity =
    &Quantity::new("molar mass of the liquid or ice part", "M", Dimension::MolarMass);
/// The enthalpy per kilogram of dry air, h, which a moist-air state is solved for.
pub const ENTHALPY_PER_DRY_AIR: &Quantity =
    &Quantity::new("enthalpy per kilogram of dry air", "h", Dimension::Energy(Basis::Mass));
/// The entropy per kilogram of dry air, s.
pub const ENTROPY_PER_DRY_AIR: &Quantity =
    &Quantity::new("entropy per kilogram of dry air", "s", Dimension::Entropy(Basis::Mass));
/// The volume per kilogram of dry air, v.
pub const VOLUME_PER_DRY_AIR: &Quantity =
    &Quantity::new("volume per kilogram of dry air", "v", Dimension::SpecificVolume(Basis::Mass));
/// The wet-bulb (ice-bulb below the triple point) temperature, T_wb.
pub const WET_BULB_TEMPERATURE: &Quantity = &Quantity::new("wet-bulb temperature", "T_wb", Dimension::Temperature);
