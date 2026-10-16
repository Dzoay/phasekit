//! The shapes of the mixture library that `cargo xtask datagen` writes into [`crate::generated`] from CoolProp 8.0.0's
//! `dev/mixtures` (PLAN.md M13.2): binary pairs keyed by their sorted CAS pair, departure functions by kind, the
//! predefined mixtures, and every library fluid's CAS number.

/// A binary pair's reducing parameters, oriented to its sorted CAS pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Reducing {
    /// GERG's β_T, γ_T, β_v, γ_v.
    Gerg([f64; 4]),
    /// Lemmon's ξ (K) and ζ (m³/mol), converted to γ_T and γ_v (β = 1) from the components' reducing states when a
    /// model is built (`ReducingFunctions.h:695-706`).
    Lemmon([f64; 2]),
}

/// One binary pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PairData {
    /// The two CAS numbers, sorted as strings: the pair's key.
    pub cas: [&'static str; 2],
    /// The library's names for them, in the same order.
    pub names: [&'static str; 2],
    pub reducing: Reducing,
    /// The departure function's weight.
    pub f: f64,
    /// The departure function's index in [`crate::generated::DEPARTURES`], where F ≠ 0.
    pub departure: Option<usize>,
    /// The library's BibTeX key.
    pub source: &'static str,
}

/// How a departure function's terms are evaluated, as v8.0.0 evaluates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// n δ^d τ^t exp(−δ^l), no exponential where l = 0 (`ExcessHEFunction.h:182-196`).
    Exponential,
    /// `power` terms n δ^d τ^t, then n δ^d τ^t exp(−η(δ − ε)² − β(δ − γ)) (`ExcessHEFunction.h:104-134`).
    Gerg2008,
    /// `power` terms n δ^d τ^t exp(−δ^l), then n δ^d τ^t exp(−η(δ − ε)² − β(τ − γ)²) (`ExcessHEFunction.h:143-172`).
    GaussianExponential,
}

/// One term; the fields its kind does not read are 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Term {
    pub n: f64,
    pub t: f64,
    pub d: f64,
    pub l: f64,
    pub eta: f64,
    pub epsilon: f64,
    pub beta: f64,
    pub gamma: f64,
}

/// One departure function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DepartureData {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub source: &'static str,
    pub kind: Kind,
    /// The leading terms without a Gaussian; every term of an Exponential function.
    pub power: usize,
    pub terms: &'static [Term],
}

/// One predefined mixture: v8.0.0's components (the library's spelling) and mole fractions, and ASHRAE 34's mass
/// fractions in the same order where its list defines the blend (ROT-199).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PredefinedData {
    pub name: &'static str,
    pub components: &'static [&'static str],
    pub mole_fractions: &'static [f64],
    pub mass_fractions: Option<&'static [f64]>,
}
