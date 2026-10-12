//! Saturation curves: an open trait, so a family brings its own (superancillary, ancillary, a fitted curve)
//! without a core edit, and every curve says how far it can be trusted (Extensible graft).

use core::fmt;

use crate::error::Error;

/// How the flash may use a curve's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SatAccuracy {
    /// Accurate to the EOS (a fresh or exactly rescaled superancillary, ~1e-14): used as the answer.
    Exact,
    /// A starting point only (ancillaries, or a superancillary made stale by a data correction): the flash
    /// polishes it with a VLE solve and records `Strategy::Vle` (M6; refused as `Unsupported` before).
    Guess,
    /// The curve defines saturation (pseudo-pure bubble/dew ancillaries): used as the answer by definition.
    Definition,
}

/// One side of a saturation state: T, p and the coexisting molar density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SatSide {
    /// Temperature, K.
    pub t: f64,
    /// Pressure, Pa.
    pub p: f64,
    /// Molar density, mol/m³.
    pub rho: f64,
}

/// Bubble (saturated liquid) and dew (saturated vapour) points at one T (`at_t`) or one p (`at_p`). For a
/// pure fluid both share T and p. For the six pseudo-pure v0.1 fluids (Air, R404A, R407C, R410A, R507A,
/// SES36) they differ: CoolProp 8.0.0 gives R410A at 280 K p = 990480.5 Pa (Q = 0) vs 987288.1 Pa (Q = 1)
/// (map 04 §1, map 03 §3.1; E3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SatPair {
    /// The saturated-liquid side.
    pub bubble: SatSide,
    /// The saturated-vapour side.
    pub dew: SatSide,
}

impl SatPair {
    /// True when bubble and dew share T and p (every pure fluid).
    pub fn is_pure(&self) -> bool {
        self.bubble.t == self.dew.t && self.bubble.p == self.dew.p
    }
}

/// A vapour-liquid saturation curve of one pure or pseudo-pure fluid. Implementations are immutable and
/// lock-free: CoolProp's superancillary inverse built under a mutex on first use (map 11 F7) becomes
/// datagen output.
pub trait SaturationCurve: Send + Sync + fmt::Debug {
    /// Trust level of this curve's answers.
    fn accuracy(&self) -> SatAccuracy;
    /// Fitted temperature range. The flash never evaluates the curve outside it, under any `DomainPolicy`
    /// (D6; CoolProp extrapolates its superancillary below the triple point, map 03 §6).
    fn t_range(&self) -> (f64, f64);
    /// Saturation at temperature `t`.
    fn at_t(&self, t: f64) -> Result<SatPair, Error>;
    /// Saturation at pressure `p`.
    fn at_p(&self, p: f64) -> Result<SatPair, Error>;
}
