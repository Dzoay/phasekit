//! The quantities a mixture's refusals name, beside core's ([`phasekit_core::Quantity`]; PLAN.md M28.1).

use phasekit_core::{Argument, Dimension, Quantity};

/// The sum of a composition's mole fractions, which must be 1.
pub const MOLE_FRACTION_SUM: &Quantity = &Quantity::new("sum of the mole fractions", "Σx", Dimension::Dimensionless);
/// The vapour's molar share of a two-phase state, β.
pub const VAPOUR_FRACTION: &Quantity = &Quantity::new("vapour fraction", "β", Dimension::Fraction);
/// The pressure a phase envelope's trace starts at.
pub const START_PRESSURE: &Quantity = &Quantity::new("starting pressure", "p_start", Dimension::Pressure);
/// The highest pressure a phase envelope's trace may reach.
pub const HIGHEST_PRESSURE: &Quantity = &Quantity::new("highest pressure", "p_stop", Dimension::Pressure);
/// A binary pair's reducing parameters, β_T, γ_T, β_v and γ_v, each by its own symbol (PLAN.md M29.5: β alone was also
/// the vapour fraction's).
pub const BETA_T: &Quantity = &Quantity::new("binary parameter β_T", "β_T", Dimension::Dimensionless);
/// γ_T.
pub const GAMMA_T: &Quantity = &Quantity::new("binary parameter γ_T", "γ_T", Dimension::Dimensionless);
/// β_v.
pub const BETA_V: &Quantity = &Quantity::new("binary parameter β_v", "β_v", Dimension::Dimensionless);
/// γ_v.
pub const GAMMA_V: &Quantity = &Quantity::new("binary parameter γ_v", "γ_v", Dimension::Dimensionless);
/// A departure function's weight F.
pub const DEPARTURE_WEIGHT: &Quantity = &Quantity::new("departure function's F", "F", Dimension::Dimensionless);

/// The order of the composition derivatives asked for.
pub const DERIVATIVE_ORDER: &Argument = &Argument::new("order of composition derivatives", "0 to 3");
/// A departure function's component, by its index.
pub const DEPARTURE_COMPONENT: &Argument = &Argument::new("departure function's component", "a component of the model");
/// The number of mole fractions given.
pub const MOLE_FRACTIONS: &Argument = &Argument::new("number of mole fractions", "one for each component");
/// The number of non-zero mole fractions given to a mixture.
pub const NON_ZERO_FRACTIONS: &Argument =
    &Argument::new("number of non-zero mole fractions", "one for each component, none zero");
/// The number of distinct components of a model.
pub const DISTINCT_COMPONENTS: &Argument = &Argument::new("number of distinct components", "at least two, none twice");
/// A binary pair's component, by its index.
pub const PAIR_COMPONENT: &Argument = &Argument::new("binary pair's component", "two distinct components of the model");
/// A GERG-2008 component, by its index.
pub const GERG_COMPONENT: &Argument = &Argument::new("GERG-2008 component", "an index below 21");
/// The phase envelope's unknowns.
pub const ENVELOPE_UNKNOWNS: &Argument =
    &Argument::new("number of the envelope's unknowns", "one for each component and two");
