//! Each error's stable code, and the rule an invalid value breaks (user decision EM11): what a facade, an agent or a
//! solver reads instead of the message. The codes are snake_case and never change meaning; a new variant adds one.

use super::{DomainError, Error};
use crate::quantity::{Dimension, Quantity};

/// The rule an [`Error::InvalidInput`]'s value breaks: the first that it does, in this order, so that a message never
/// names a rule the value keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Rule {
    /// NaN.
    NotANumber,
    /// ±∞.
    NotFinite,
    /// A temperature at or below 0 K.
    NotAboveAbsoluteZero,
    /// A value of a dimension that must be positive (a pressure, a density, …) at or below zero.
    NotPositive,
    /// A fraction outside [0, 1].
    OutsideUnitInterval,
    /// None of the above: the model refuses it, or no state reaches it.
    OutOfRange,
}

impl Rule {
    pub(crate) fn of(quantity: &Quantity, value: f64) -> Self {
        let dimension = quantity.dimension();
        if value.is_nan() {
            Rule::NotANumber
        } else if value.is_infinite() {
            Rule::NotFinite
        } else if dimension == Dimension::Temperature && value <= 0.0 {
            Rule::NotAboveAbsoluteZero
        } else if dimension.is_positive() && value <= 0.0 {
            Rule::NotPositive
        } else if dimension == Dimension::Fraction && !(0.0..=1.0).contains(&value) {
            Rule::OutsideUnitInterval
        } else {
            Rule::OutOfRange
        }
    }

    /// The rule's stable code: `not_a_number`, `not_positive`, `out_of_range`, …
    pub const fn code(self) -> &'static str {
        match self {
            Rule::NotANumber => "not_a_number",
            Rule::NotFinite => "not_finite",
            Rule::NotAboveAbsoluteZero => "not_above_absolute_zero",
            Rule::NotPositive => "not_positive",
            Rule::OutsideUnitInterval => "outside_unit_interval",
            Rule::OutOfRange => "out_of_range",
        }
    }
}

impl Error {
    /// The error's stable code: `invalid_input`, `below_min_temperature`, `ambiguous`, …; a domain error's is its own.
    pub const fn code(&self) -> &'static str {
        match self {
            Error::InvalidInput { .. } => "invalid_input",
            Error::InvalidArgument { .. } => "invalid_argument",
            Error::Unsupported { .. } => "unsupported",
            Error::Domain(e) => e.code(),
            Error::NoConvergence { .. } => "no_convergence",
            Error::Ambiguous { .. } => "ambiguous",
            Error::Undefined { .. } => "undefined",
            Error::NoModel { .. } => "no_model",
            Error::ForeignState => "foreign_state",
            Error::InvalidState { .. } => "invalid_state",
            Error::Load(_) => "load",
            Error::Shape { .. } => "shape",
        }
    }

    /// The rule an [`Error::InvalidInput`]'s value breaks; `None` for any other error.
    pub fn rule(&self) -> Option<Rule> {
        match self {
            Error::InvalidInput { quantity, value } => Some(Rule::of(quantity, *value)),
            _ => None,
        }
    }
}

impl DomainError {
    /// The domain error's stable code: `below_min_temperature`, `mechanically_unstable`, …
    pub const fn code(&self) -> &'static str {
        match self {
            DomainError::BelowMinTemperature { .. } => "below_min_temperature",
            DomainError::AboveMaxTemperature { .. } => "above_max_temperature",
            DomainError::BelowMinPressure { .. } => "below_min_pressure",
            DomainError::AboveMaxPressure { .. } => "above_max_pressure",
            DomainError::BelowMeltingLine { .. } => "below_melting_line",
            DomainError::MechanicallyUnstable => "mechanically_unstable",
        }
    }
}
