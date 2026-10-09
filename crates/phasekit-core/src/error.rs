//! One error type for the whole kernel (D12). Hand-written, `#[non_exhaustive]`, `Clone` (load failures are
//! cached in the registry), no `thiserror` (dependencies R10). Bindings map variants exhaustively (map 14).

use core::fmt;

use crate::input::Pair;
use crate::prop::Prop;
use crate::state::{Phase, Strategy};

/// Every failure the kernel can report. No panics on user input; no sentinels (map 12 R3).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// A quantity failed validation: NaN, infinite, non-positive T/p/ρ, or Q outside [0, 1] (map 12 R9).
    InvalidInput {
        /// Which quantity.
        quantity: &'static str,
        /// The rejected value.
        value: f64,
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
    /// The input pair has several physical solutions; they are returned as data (map 03 §6).
    Ambiguous {
        /// The candidate roots (temperatures or densities, depending on the pair).
        roots: Roots,
    },
    /// The property is undefined in this phase: cp, cv, w and transport inside the dome (map 02 §6,
    /// map 10 R18), quality outside it (no −1 sentinel, map 01 §4c).
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
        /// Requested temperature, K.
        t: f64,
        /// Limit, K.
        t_min: f64,
    },
    /// T above the model's maximum temperature.
    AboveMaxTemperature {
        /// Requested temperature, K.
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
    /// The state lies in the solid region of a fluid model.
    BelowMeltingLine,
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

/// Up to four roots of an ambiguous flash, as data. Two are equal when their roots are (the unused slots are NaN).
#[derive(Clone, Copy, Debug)]
pub struct Roots {
    values: [f64; 4],
    len: u8,
}

impl PartialEq for Roots {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Roots {
    /// Collects up to four roots; extra roots are dropped (a flash never has more on one isotherm).
    pub fn new(roots: &[f64]) -> Self {
        let mut values = [f64::NAN; 4];
        let len = roots.len().min(4);
        values[..len].copy_from_slice(&roots[..len]);
        Self { values, len: len as u8 }
    }

    /// The roots, in ascending order of the solved variable.
    pub fn as_slice(&self) -> &[f64] {
        &self.values[..usize::from(self.len)]
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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidInput { quantity, value } => write!(f, "invalid {quantity}: {value}"),
            Error::Unsupported { pair } => write!(f, "input pair {pair:?} is not supported by this model"),
            Error::Domain(e) => write!(f, "outside the validity domain: {e:?}"),
            Error::NoConvergence { strategy, iterations } => {
                write!(f, "{strategy:?} did not converge in {iterations} iterations")
            }
            Error::Ambiguous { roots } => write!(f, "several solutions: {:?}", roots.as_slice()),
            Error::Undefined { prop, phase } => write!(f, "{prop:?} is undefined in phase {phase:?}"),
            Error::NoModel { prop } => write!(f, "no model for {prop:?}"),
            Error::ForeignState => f.write_str("the state was produced by a different model"),
            Error::InvalidState { reason } => write!(f, "inconsistent state: {reason}"),
            Error::Load(e) => write!(f, "load error: {e}"),
            Error::Shape { expected, found } => write!(f, "buffer length {found}, expected {expected}"),
        }
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
