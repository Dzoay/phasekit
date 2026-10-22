//! One error type for the whole kernel (D12). Hand-written, `#[non_exhaustive]`, `Clone` (load failures are
//! cached in the registry), no `thiserror` (dependencies R10). Bindings map variants exhaustively (map 14).

use core::fmt;

use crate::input::Pair;
use crate::prop::Prop;
use crate::quantity::{Quantity, Si, Style, Units};
use crate::state::{Phase, Strategy};

mod code;
mod message;
mod words;

pub use code::Rule;
pub use message::Message;

/// Every failure the kernel can report. No panics on user input; no sentinels (map 12 R3).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// A quantity failed validation: NaN, infinite, non-positive T/p/ρ, or Q outside [0, 1] (map 12 R9), or no state
    /// reaches it.
    InvalidInput {
        /// Which quantity.
        quantity: &'static Quantity,
        /// The rejected value, SI.
        value: f64,
    },
    /// An argument that is not a physical value was refused: a count, an index, a plane, a mixture (user decision EM3).
    InvalidArgument {
        /// What was refused, as a sentence names it ("number of mole fractions").
        argument: &'static str,
        /// The refused count or index, if it has one.
        value: Option<usize>,
    },
    /// The model does not declare this input pair (declared capability, map 01 R21).
    Unsupported {
        /// The refused pair.
        pair: Pair,
    },
    /// Outside the model's validity domain (checked per call unless `DomainPolicy::Extrapolate`).
    Domain(DomainError),
    /// A solver exhausted its iterations or lost its bracket (map 03 §6).
    NoConvergence {
        /// The strategy that failed.
        strategy: Strategy,
        /// Iterations spent.
        iterations: u16,
    },
    /// The input pair has several physical solutions, or a curve several values (two melting pressures at one T on
    /// Water's curve); they are returned as data (map 03 §6).
    Ambiguous {
        /// The candidate roots (temperatures or densities, depending on the pair).
        roots: Roots,
    },
    /// The property is undefined in this phase: cp, cv, w and transport inside the dome (map 02 §6,
    /// map 10 R18), quality outside it (no −1 sentinel, map 01 §4c), and a correlation where it has no value
    /// (helium's λ exactly on its scaled equation's critical isochore, DIV-0035).
    Undefined {
        /// The refused property.
        prop: Prop,
        /// The phase of the state.
        phase: Phase,
    },
    /// The model has no correlation for this property.
    NoModel {
        /// The missing property.
        prop: Prop,
    },
    /// The state was produced by a different model (its `ModelKey` differs).
    ForeignState,
    /// A family built an inconsistent state through `State::from_total` / `from_split` (non-finite
    /// first-order bundle, NaN, a single point labelled `TwoPhase`, phases with different R or M).
    InvalidState {
        /// What is inconsistent.
        reason: &'static str,
    },
    /// A name, blob or lazily loaded part could not be loaded.
    Load(LoadError),
    /// Batch buffers have inconsistent lengths.
    Shape {
        /// Required length.
        expected: usize,
        /// Supplied length.
        found: usize,
    },
}

/// Validity-domain failures (materials S4: `BelowMeltingLine` exists from day one).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum DomainError {
    /// T below the model's minimum temperature (or its triple point, if higher).
    BelowMinTemperature {
        /// Requested or solved temperature, K; where a P+X input lies below every state at its p, the limit itself.
        t: f64,
        /// Limit, K.
        t_min: f64,
    },
    /// T above the model's maximum temperature.
    AboveMaxTemperature {
        /// Requested or solved temperature, K; where a P+X input lies beyond 1.5·Tmax, that bound.
        t: f64,
        /// Limit, K.
        t_max: f64,
    },
    /// p below the lowest pressure of a saturation curve (its triple point's, or a dip's, M6.8).
    BelowMinPressure {
        /// Requested pressure, Pa.
        p: f64,
        /// Limit, Pa.
        p_min: f64,
    },
    /// p above the model's maximum pressure, or above the highest pressure of a saturation curve (its critical point's).
    AboveMaxPressure {
        /// Requested or computed pressure, Pa.
        p: f64,
        /// Limit, Pa.
        p_max: f64,
    },
    /// The state lies in the solid region of a fluid model: T below the melting temperature at its pressure.
    BelowMeltingLine {
        /// The state's temperature, K.
        t: f64,
        /// The melting curve's temperature at the state's pressure, K.
        t_melt: f64,
    },
    /// dp/dρ ≤ 0 or cv ≤ 0 where a stable state was required.
    MechanicallyUnstable,
}

/// Loading failures. A failed decode is cached by the registry (data are immutable, so no retry storms);
/// `NotEmbedded` and `UnknownName` are answered without touching any cell, so a later layer can still
/// supply that name (E6).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LoadError {
    /// No fluid answers to this name, alias or CAS number.
    UnknownName(Box<str>),
    /// A name is already taken in this registry.
    DuplicateName(Box<str>),
    /// The fluid is in the index but its Cargo feature is off.
    NotEmbedded {
        /// Canonical name.
        name: Box<str>,
        /// Feature that embeds it.
        feature: &'static str,
    },
    /// Malformed or unsupported data (unknown term kind, bad version, failed validation).
    Format(Box<str>),
    /// A lazily materialised part (saturation, transport) is absent.
    MissingPart(&'static str),
    /// A fluid declares a reference (an ECS reference fluid) that no layer of the registry provides.
    /// Reported when the registry layer is built, never at first use (E5, map 05 R7).
    MissingReference {
        /// The fluid that declares the reference.
        fluid: Box<str>,
        /// The missing reference.
        reference: Box<str>,
    },
    /// The references declared by a data source form a cycle (refused when the layer is built: a cycle
    /// would deadlock on re-entrant lazy initialisation).
    ReferenceCycle(Box<str>),
}

/// Up to four roots of an ambiguous flash, as data, with the quantity they are values of. Two are equal when their
/// quantities and roots are; the unused slots are NaN, which no root is.
#[derive(Clone, Copy, Debug)]
pub struct Roots {
    quantity: &'static Quantity,
    values: [f64; 4],
}

impl PartialEq for Roots {
    fn eq(&self, other: &Self) -> bool {
        self.quantity == other.quantity && self.as_slice() == other.as_slice()
    }
}

impl Roots {
    /// Collects up to four roots of `quantity`, SI; extra roots are dropped (a flash never has more on one isotherm).
    pub fn new(quantity: &'static Quantity, roots: &[f64]) -> Self {
        let mut values = [f64::NAN; 4];
        for (value, root) in values.iter_mut().zip(roots) {
            *value = *root;
        }
        Self { quantity, values }
    }

    /// What the roots are values of.
    pub const fn quantity(&self) -> &'static Quantity {
        self.quantity
    }

    /// The roots, in ascending order of the solved variable.
    pub fn as_slice(&self) -> &[f64] {
        let len = self.values.iter().take_while(|v| !v.is_nan()).count();
        self.values.get(..len).unwrap_or_default()
    }
}

impl From<DomainError> for Error {
    fn from(e: DomainError) -> Self {
        Error::Domain(e)
    }
}

impl From<LoadError> for Error {
    fn from(e: LoadError) -> Self {
        Error::Load(e)
    }
}

impl Error {
    /// The message in `units` and `style` (user decisions EM1, EM2): `{}` is `display(&Si, Style::Long)`, `{:#}`
    /// `display(&Si, Style::Short)`.
    pub fn display<'a>(&'a self, units: &'a dyn Units, style: Style) -> Message<'a> {
        Message::new(self, units, style)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let style = if f.alternate() { Style::Short } else { Style::Long };
        self.display(&Si, style).fmt(f)
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::UnknownName(n) => write!(f, "unknown fluid name `{n}`"),
            LoadError::DuplicateName(n) => write!(f, "name `{n}` is already registered"),
            LoadError::NotEmbedded { name, feature } => write!(f, "{name} is not embedded; enable `{feature}`"),
            LoadError::Format(m) => write!(f, "bad data: {m}"),
            LoadError::MissingPart(p) => write!(f, "missing part: {p}"),
            LoadError::MissingReference { fluid, reference } => {
                write!(f, "{fluid} needs `{reference}`, which no layer of this registry provides")
            }
            LoadError::ReferenceCycle(n) => write!(f, "the references of `{n}` form a cycle"),
        }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests;
