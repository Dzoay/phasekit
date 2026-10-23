//! What a caller can do about each error (user decision EM15): stable codes, which each facade writes in its own API's
//! terms, since Rust's `FlashOptions`, compat's imposed-phase strings and the browser package's options objects name
//! the same remedy differently. Core holds the codes, not the advice.

use super::{DomainError, Error, LoadError};
use crate::state::Phase;

/// A remedy for a refusal, by what the caller changes. The codes are snake_case and never change meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Remedy {
    /// Pick one of the roots: `RootPolicy::Nearest`, or a phase hint.
    ChooseRoot,
    /// Allow the state outside the validity domain: `DomainPolicy::Extrapolate`, the state flagged `Extrapolated`.
    Extrapolate,
    /// Start the solver elsewhere: `FlashOptions::with_guess`.
    WarmStart,
    /// Give the state by another input pair.
    OtherPair,
    /// Read each saturated phase's property, or the quality, in place of the two-phase state's.
    SaturatedPhases,
    /// Give the state by a pair the model declares (`Capabilities`).
    DeclaredPair,
    /// Use a name the registry holds: the nearest, by [`Registry::nearest`](crate::Registry::nearest).
    NearestName,
}

impl Remedy {
    /// The remedy's stable code: `choose_root`, `extrapolate`, …
    pub const fn code(self) -> &'static str {
        match self {
            Remedy::ChooseRoot => "choose_root",
            Remedy::Extrapolate => "extrapolate",
            Remedy::WarmStart => "warm_start",
            Remedy::OtherPair => "other_pair",
            Remedy::SaturatedPhases => "saturated_phases",
            Remedy::DeclaredPair => "declared_pair",
            Remedy::NearestName => "nearest_name",
        }
    }
}

impl Error {
    /// What the caller can do, most direct first; none for an error the caller cannot change by its inputs.
    pub const fn remedies(&self) -> &'static [Remedy] {
        match self {
            Error::Unreachable { .. } => &[Remedy::OtherPair],
            Error::Unsupported { .. } => &[Remedy::DeclaredPair],
            Error::Domain(DomainError::MechanicallyUnstable) => &[],
            Error::Domain(_) => &[Remedy::Extrapolate],
            Error::NoConvergence { .. } => &[Remedy::WarmStart, Remedy::OtherPair],
            Error::Ambiguous { .. } => &[Remedy::ChooseRoot],
            Error::Undefined { phase: Phase::TwoPhase, .. } => &[Remedy::SaturatedPhases],
            Error::Load(LoadError::UnknownName(_)) => &[Remedy::NearestName],
            _ => &[],
        }
    }
}
