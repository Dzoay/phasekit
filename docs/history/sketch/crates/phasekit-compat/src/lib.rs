//! `phasekit-compat`: the PropsSI-style migration API (D11). One strict, locale-free grammar shared by the C,
//! WASM and Python facades (map 01 R17/R18); legacy policies (fill with +inf, HAPropsSI raise-first) live
//! here and never in the kernel (map 14). Works over ANY [`Registry`], so third-party families and
//! re-gauged fluids (`Registry::with_reference`, E10) are reachable by name with no core edit.
//!
//! Model cache (map 01 Q8, decided): none. The registry IS the cache: at most one decoded model per named
//! fluid, lookups allocation-free, memory released by dropping the registry value. Mixture strings
//! (`A[0.5]&B[0.5]`, post-0.1) resolve to handles registered with `Registry::with_model`, never to a
//! hidden LRU.

use core::fmt;

use phasekit_core::{Basis, Error, FlashOptions, Input, Pair, Prop, Registry, Var};

/// How a vector call reports a failed point (map 14: PropsSI fills +inf, HAPropsSI raises on the first).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum FillPolicy {
    /// Return the first error (the native behaviour).
    #[default]
    Error,
    /// Fill +inf (CoolProp `PropsSI` vector parity).
    Inf,
    /// Fill NaN.
    Nan,
}

/// String-level failures, or a kernel error.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CompatError {
    /// Unknown output key.
    UnknownOutput(String),
    /// Unknown input key.
    UnknownInput(String),
    /// The two input keys do not form a physical pair, or their bases disagree.
    InvalidPair(String, String),
    /// The kernel refused the call.
    Core(Error),
}

impl From<Error> for CompatError {
    fn from(e: Error) -> Self {
        CompatError::Core(e)
    }
}

impl fmt::Display for CompatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompatError::UnknownOutput(k) => write!(f, "unknown output key `{k}`"),
            CompatError::UnknownInput(k) => write!(f, "unknown input key `{k}`"),
            CompatError::InvalidPair(a, b) => write!(f, "`{a}` and `{b}` do not form an input pair"),
            CompatError::Core(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CompatError {}

/// CoolProp input keys: (variable, basis). `None` basis = basis-free (T, P, Q).
pub fn parse_input(key: &str) -> Option<(Var, Option<Basis>)> {
    use Basis::{Mass, Molar};
    Some(match key {
        "T" => (Var::T, None),
        "P" => (Var::P, None),
        "Q" => (Var::Q, None),
        "D" | "Dmass" => (Var::D, Some(Mass)),
        "Dmolar" => (Var::D, Some(Molar)),
        "H" | "Hmass" => (Var::H, Some(Mass)),
        "Hmolar" => (Var::H, Some(Molar)),
        "S" | "Smass" => (Var::S, Some(Mass)),
        "Smolar" => (Var::S, Some(Molar)),
        "U" | "Umass" => (Var::U, Some(Mass)),
        "Umolar" => (Var::U, Some(Molar)),
        _ => return None,
    })
}

/// CoolProp output keys (subset in the sketch; the full table and the strict `d(X)/d(Y)|Z` grammar are
/// generated from map 01 §4a at M10, gated against the oracle's 85 outputs).
pub fn parse_output(key: &str) -> Option<Prop> {
    Some(match key {
        "T" => Prop::T,
        "P" => Prop::P,
        "Q" => Prop::Q,
        "D" | "Dmass" => Prop::Dmass,
        "Dmolar" => Prop::Dmolar,
        "H" | "Hmass" => Prop::Hmass,
        "Hmolar" => Prop::Hmolar,
        "S" | "Smass" => Prop::Smass,
        "Smolar" => Prop::Smolar,
        "U" | "Umass" => Prop::Umass,
        "Umolar" => Prop::Umolar,
        "C" | "Cpmass" => Prop::Cpmass,
        "Cpmolar" => Prop::Cpmolar,
        "O" | "Cvmass" => Prop::Cvmass,
        "Cvmolar" => Prop::Cvmolar,
        "A" | "speed_of_sound" => Prop::SpeedOfSound,
        "Z" => Prop::Z,
        "Cp0molar" => Prop::Cp0molar,
        "Cp0mass" => Prop::Cp0mass,
        "M" | "molar_mass" => Prop::MolarMass,
        "V" | "viscosity" => Prop::Viscosity,
        "L" | "conductivity" => Prop::Conductivity,
        "I" | "surface_tension" => Prop::SurfaceTension,
        _ => return None,
    })
}

/// `PropsSI(output, name1, value1, name2, value2, fluid)` over any registry. No echo shortcut (map 01 R7):
/// every input passes the kernel's validation gate. `HEOS::` is the built-in backend prefix; any other
/// prefix is part of a registered name (e.g. `PR::Water` from a cubic crate).
pub fn props_si_in(
    registry: &Registry,
    output: &str,
    name1: &str,
    value1: f64,
    name2: &str,
    value2: f64,
    fluid: &str,
) -> Result<f64, CompatError> {
    let prop = parse_output(output).ok_or_else(|| CompatError::UnknownOutput(output.into()))?;
    let (v1, b1) = parse_input(name1).ok_or_else(|| CompatError::UnknownInput(name1.into()))?;
    let (v2, b2) = parse_input(name2).ok_or_else(|| CompatError::UnknownInput(name2.into()))?;
    let invalid = || CompatError::InvalidPair(name1.into(), name2.into());
    let basis = match (b1, b2) {
        (Some(a), Some(b)) if a != b => return Err(invalid()),
        (a, b) => a.or(b).unwrap_or(Basis::Molar),
    };
    let (pair, swapped): (Pair, bool) = Pair::from_vars(v1, v2).ok_or_else(invalid)?;
    let (x, y) = if swapped { (value2, value1) } else { (value1, value2) };
    let handle = registry.get(fluid.strip_prefix("HEOS::").unwrap_or(fluid))?;
    let state = handle.flash(Input::new(pair, x, y, basis)?, &FlashOptions::default())?;
    Ok(handle.prop(&state, prop)?)
}

/// `PropsSI` over the embedded registry.
#[cfg(feature = "embedded")]
pub fn props_si(
    output: &str,
    name1: &str,
    value1: f64,
    name2: &str,
    value2: f64,
    fluid: &str,
) -> Result<f64, CompatError> {
    props_si_in(Registry::embedded()?, output, name1, value1, name2, value2, fluid)
}
