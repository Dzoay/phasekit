//! `phasekit-compat`: the PropsSI-style migration API (D11). One strict, locale-free grammar shared by the C,
//! WASM and Python facades (map 01 R17/R18); legacy policies (fill with +inf, HAPropsSI raise-first) live
//! here and never in the kernel (map 14). Works over ANY [`Registry`], so third-party families and
//! re-gauged fluids (`Registry::with_reference`, E10) are reachable by name with no core edit.
//!
//! Model cache (map 01 Q8, decided): none. The registry IS the cache: at most one decoded model per named
//! fluid, lookups allocation-free, memory released by dropping the registry value. Mixture strings
//! (`A[0.5]&B[0.5]`, post-0.1) resolve to handles registered with `Registry::with_model`, never to a
//! hidden LRU.

// PLAN.md M24.3 (issue #219): no implicit panic, an index out of bounds or an integer overflow, in the library's code
// (`gates lints` checks this line); its tests are exempt.
#![cfg_attr(not(test), deny(clippy::indexing_slicing, clippy::arithmetic_side_effects))]

use core::fmt;

use phasekit_core::batch::{self, BatchRequest, BatchSummary, ExecPolicy, Status};
use phasekit_core::{Basis, Error, FlashOptions, Fluid, Input, LoadError, Pair, Prop, Registry, Rule, State};

#[cfg(feature = "fault-injection")]
pub mod fault;
mod humid_air;
mod keys;

#[cfg(feature = "embedded")]
pub use humid_air::ha_props_si;
pub use humid_air::{ha_names, ha_output_at, ha_props_si_in, ha_props_si_vec_in, ha_state_in, humid_air_in};

pub use keys::{
    OUTPUT_COUNT, Output, output_names, pair_name, parse_derivative, parse_input, parse_output, parse_pair_name,
    parse_reference_state, phase_index, phase_name, spells,
};

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
    /// A fluid string outside the grammar: empty, padded, a mixture (`&`), fractions (`[…]`), backend options (`?…`)
    /// or an incompressible concentration (`%`) (map 01 R17, R18).
    InvalidFluid(String),
    /// `Props1SI` of an output that depends on the state.
    NotAConstant(String),
    /// A batch output that is not a property: `Phase`, which is `PhaseSI`'s.
    NotAProperty(String),
    /// The kernel refused the call.
    Core(Error),
}

impl CompatError {
    /// The refusal's stable code: core's for a kernel error (`invalid_input`, `below_min_temperature`, …), else
    /// compat's own (`unknown_output`, `invalid_pair`, …) (user decision EM12).
    pub const fn code(&self) -> &'static str {
        match self {
            CompatError::UnknownOutput(_) => "unknown_output",
            CompatError::UnknownInput(_) => "unknown_input",
            CompatError::InvalidPair(..) => "invalid_pair",
            CompatError::InvalidFluid(_) => "invalid_fluid",
            CompatError::NotAConstant(_) => "not_a_constant",
            CompatError::NotAProperty(_) => "not_a_property",
            CompatError::Core(e) => e.code(),
        }
    }

    /// The rule an invalid input's value breaks, core's; `None` for any other refusal.
    pub fn rule(&self) -> Option<Rule> {
        match self {
            CompatError::Core(e) => e.rule(),
            _ => None,
        }
    }
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
            CompatError::InvalidFluid(s) => write!(f, "`{s}` is not a fluid name"),
            CompatError::NotAConstant(k) => write!(f, "`{k}` depends on the state"),
            CompatError::NotAProperty(k) => write!(f, "`{k}` is not a property; a batch's outputs are"),
            CompatError::Core(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CompatError {}

/// The fluid a fluid string names in `registry`: a name it holds, with `HEOS::`, the built-in backend, as an optional
/// prefix (any other prefix is part of a registered name, `PR::Water` from a cubic crate); else the whole string as
/// the registry's resolvers read it ([`Registry::resolve`]), phasekit-mix's mixture names where they are registered
/// (`HEOS::R32[0.7]&R125[0.3]`, `R410A.mix`; PLAN.md M13.13), whose grammar is theirs. Backend options (`?…`,
/// ROT-172) and incompressible concentrations (`%`) are refused by name, as are an empty or a padded string: nothing is
/// parsed leniently (ROT-166). A name nothing reads is the registry's refusal of it without the prefix.
pub fn fluid_in(registry: &Registry, fluid: &str) -> Result<Fluid, CompatError> {
    let name = fluid.strip_prefix("HEOS::").unwrap_or(fluid);
    let outside = name.is_empty() || name.trim() != name || name.contains(['?', '%']);
    if outside {
        return Err(CompatError::InvalidFluid(fluid.into()));
    }
    let missing = match registry.get(name) {
        Ok(held) => return Ok(held.clone()),
        Err(missing) => missing,
    };
    match registry.resolve(fluid) {
        Err(Error::Load(LoadError::UnknownName(unread))) if *unread == *fluid => Err(missing.into()),
        resolved => Ok(resolved?),
    }
}

/// A `PropsSI` call's plan: everything that is checked before any state is flashed (CoolProp's plan-time errors,
/// map 14 §3.2), so a vector call refuses a bad key or fluid whole.
struct Plan {
    output: Output,
    keys: Keys,
    fluid: Fluid,
}

impl Plan {
    fn new(registry: &Registry, output: &str, name1: &str, name2: &str, fluid: &str) -> Result<Self, CompatError> {
        let output = parse_output(output).ok_or_else(|| CompatError::UnknownOutput(output.into()))?;
        Ok(Plan { output, keys: pair_of(name1, name2)?, fluid: fluid_in(registry, fluid)? })
    }

    /// The state at the two input values, in the order of the call's keys.
    fn state(&self, value1: f64, value2: f64, flash: &FlashOptions) -> Result<State, Error> {
        self.fluid.flash(self.keys.input(value1, value2)?, flash)
    }

    /// The output at the two input values.
    fn eval(&self, value1: f64, value2: f64, flash: &FlashOptions) -> Result<f64, Error> {
        value(&self.fluid, &self.state(value1, value2, flash)?, self.output)
    }
}

/// An output's value at a state of `fluid`.
fn value(fluid: &Fluid, state: &State, output: Output) -> Result<f64, Error> {
    match output {
        Output::Prop(prop) => fluid.prop(state, prop),
        Output::Phase => Ok(f64::from(phase_index(state.phase()))),
    }
}

/// The value of the output key `output` at a state of `fluid` (the browser package's states).
pub fn output_at(fluid: &Fluid, state: &State, output: &str) -> Result<f64, CompatError> {
    let parsed = parse_output(output).ok_or_else(|| CompatError::UnknownOutput(output.into()))?;
    Ok(value(fluid, state, parsed)?)
}

/// The state of `fluid` at the values of two input keys, in either order as `PropsSI`'s (the browser package's flash).
pub fn flash_si(
    fluid: &Fluid,
    (name1, value1): (&str, f64),
    (name2, value2): (&str, f64),
    flash: &FlashOptions,
) -> Result<State, CompatError> {
    Ok(fluid.flash(pair_of(name1, name2)?.input(value1, value2)?, flash)?)
}

/// A batch cell's code, which the C batch (`PK_*`) and the browser batch share (map 11 F15, F23): 0 for success, 1 for
/// a flagged extrapolation, then the kernel's failures in `Status` order, 10 for any other. Pinned and append-only, as
/// `Status`'s discriminants are not an ABI.
pub fn status_code(status: Status) -> u8 {
    match status {
        Status::Ok => 0,
        Status::Extrapolated => 1,
        Status::InvalidInput => 2,
        Status::Unsupported => 3,
        Status::Domain => 4,
        Status::NoConvergence => 5,
        Status::Ambiguous => 6,
        Status::Undefined => 7,
        Status::NoModel => 8,
        Status::Load => 9,
        _ => 10,
    }
}

/// What two input keys name: the pair, whether they come in its reverse order, their basis (molar if neither says) and
/// the quality's (mass for `Qmass`, which reaches the model as a mass quality: a mixture's phases differ in molar mass).
struct Keys {
    pair: Pair,
    swapped: bool,
    basis: Basis,
    quality_basis: Basis,
}

impl Keys {
    /// The input at the two values, in the order of the call's keys.
    fn input(&self, value1: f64, value2: f64) -> Result<Input, Error> {
        let (x, y) = if self.swapped { (value2, value1) } else { (value1, value2) };
        Ok(Input::new(self.pair, x, y, self.basis)?.with_quality_basis(self.quality_basis))
    }
}

/// The [`Keys`] of two input keys.
fn pair_of(name1: &str, name2: &str) -> Result<Keys, CompatError> {
    let (v1, b1) = parse_input(name1).ok_or_else(|| CompatError::UnknownInput(name1.into()))?;
    let (v2, b2) = parse_input(name2).ok_or_else(|| CompatError::UnknownInput(name2.into()))?;
    let invalid = || CompatError::InvalidPair(name1.into(), name2.into());
    let basis = match (b1, b2) {
        (Some(a), Some(b)) if a != b => return Err(invalid()),
        (a, b) => a.or(b).unwrap_or(Basis::Molar),
    };
    let (pair, swapped) = Pair::from_vars(v1, v2).ok_or_else(invalid)?;
    let mass = [name1, name2].iter().any(|key| keys::spells(key, "Qmass"));
    Ok(Keys { pair, swapped, basis, quality_basis: if mass { Basis::Mass } else { Basis::Molar } })
}

/// `PropsSI(output, name1, value1, name2, value2, fluid)` over any registry. No echo shortcut (map 01 R7):
/// every input passes the kernel's validation gate, a state-independent output too.
pub fn props_si_in(
    registry: &Registry,
    output: &str,
    name1: &str,
    value1: f64,
    name2: &str,
    value2: f64,
    fluid: &str,
) -> Result<f64, CompatError> {
    props_si_with(registry, &FlashOptions::new(), output, name1, value1, name2, value2, fluid)
}

/// [`props_si_in`] under the flash options `flash` (the Tier A shim's `set_config_*` keys that have an equivalent, user
/// decision P6).
#[allow(clippy::too_many_arguments, reason = "PropsSI's six arguments and the options")]
pub fn props_si_with(
    registry: &Registry,
    flash: &FlashOptions,
    output: &str,
    name1: &str,
    value1: f64,
    name2: &str,
    value2: f64,
    fluid: &str,
) -> Result<f64, CompatError> {
    Ok(Plan::new(registry, output, name1, name2, fluid)?.eval(value1, value2, flash)?)
}

/// `PropsSI` over vectors (map 14 §3.1-§3.2, the Python contract): the keys and the fluid are checked first, for the
/// whole call; a length-1 operand broadcasts to the other's length, any other length mismatch is `Error::Shape`; each
/// point is the scalar call's value, bit for bit. A failed point follows `fill`: the first error is returned, or the
/// point is +inf or NaN; under +inf or NaN a call whose every point fails returns the first point's error, as Python's
/// raises (CoolProp then reports "No outputs were able to be calculated" and loses the cause).
#[allow(clippy::too_many_arguments, reason = "PropsSI's six arguments and the fill policy")]
pub fn props_si_vec_in(
    registry: &Registry,
    output: &str,
    name1: &str,
    values1: &[f64],
    name2: &str,
    values2: &[f64],
    fluid: &str,
    fill: FillPolicy,
) -> Result<Vec<f64>, CompatError> {
    let plan = Plan::new(registry, output, name1, name2, fluid)?;
    let n = values1.len().max(values2.len());
    if [values1.len(), values2.len()].into_iter().any(|len| len != 1 && len != n) {
        return Err(Error::Shape { expected: n, found: values1.len().min(values2.len()) }.into());
    }
    let (mut out, mut first) = (Vec::with_capacity(n), None);
    for i in 0..n {
        let (Some(x), Some(y)) = (broadcast(values1, i), broadcast(values2, i)) else { break };
        match (plan.eval(x, y, &FlashOptions::new()), fill) {
            (Ok(value), _) => out.push(value),
            (Err(e), FillPolicy::Error) => return Err(e.into()),
            (Err(e), FillPolicy::Inf | FillPolicy::Nan) => {
                first.get_or_insert(e);
                out.push(if fill == FillPolicy::Inf { f64::INFINITY } else { f64::NAN });
            }
        }
    }
    match first {
        Some(e) if out.iter().all(|v| !v.is_finite()) => Err(e.into()),
        _ => Ok(out),
    }
}

/// A batch over the strings on `fluid`, the one batch of the C and browser facades (map 11 U4, F23): points `i` of the
/// pair the input keys `name1` and `name2` name, in either order as `PropsSI`'s, at `values1[i]` and `values2[i]`;
/// output `k` of point `i`, of the output key `outputs[k]`, into `out[i * M + k]` and its status into `status[i * M + k]`.
/// It is [`batch::evaluate`], whose cells are the scalar path's bit for bit, with the keys read first, for the whole
/// call: an unknown key, or `Phase`, which is not a property, refuses it before any cell is written.
pub fn batch_si(
    fluid: &Fluid,
    outputs: &[&str],
    input1: (&str, &[f64]),
    input2: (&str, &[f64]),
    out: &mut [f64],
    status: &mut [Status],
) -> Result<BatchSummary, CompatError> {
    batch_si_with(fluid, ExecPolicy::Reference, outputs, input1, input2, out, status)
}

/// [`batch_si`] under the executor `exec`; every policy gives the same cells, bit for bit (K17). The browser package
/// runs `ExecPolicy::Simd` (user decision SL8).
pub fn batch_si_with(
    fluid: &Fluid,
    exec: ExecPolicy,
    outputs: &[&str],
    (name1, values1): (&str, &[f64]),
    (name2, values2): (&str, &[f64]),
    out: &mut [f64],
    status: &mut [Status],
) -> Result<BatchSummary, CompatError> {
    let property = |key: &&str| match parse_output(key) {
        Some(Output::Prop(prop)) => Ok(prop),
        Some(Output::Phase) => Err(CompatError::NotAProperty((*key).into())),
        None => Err(CompatError::UnknownOutput((*key).into())),
    };
    let props = outputs.iter().map(property).collect::<Result<Vec<_>, _>>()?;
    let Keys { pair, swapped, basis, quality_basis } = pair_of(name1, name2)?;
    let (x, y) = if swapped { (values2, values1) } else { (values1, values2) };
    let request = BatchRequest::new(pair, basis, x, y, &props).with_exec(exec).with_quality_basis(quality_basis);
    Ok(batch::evaluate(fluid, &request, out, status)?)
}

/// Point `i` of a vector operand, a length-1 one at every point.
fn broadcast(values: &[f64], i: usize) -> Option<f64> {
    if values.len() == 1 { values.first().copied() } else { values.get(i).copied() }
}

/// `Props1SI(output, fluid)` over any registry: a state-independent output (CoolProp's trivial outputs, map 01 §4a).
/// Unlike CoolProp's, the arguments come in this order only.
pub fn props1_si_in(registry: &Registry, output: &str, fluid: &str) -> Result<f64, CompatError> {
    match parse_output(output).ok_or_else(|| CompatError::UnknownOutput(output.into()))? {
        Output::Prop(Prop::Constant(c)) => Ok(fluid_in(registry, fluid)?.constant(c)?),
        _ => Err(CompatError::NotAConstant(output.into())),
    }
}

/// `PhaseSI(name1, value1, name2, value2, fluid)` over any registry: CoolProp's name of the state's phase
/// ([`phase_name`]). Where CoolProp returns "unknown" for a failed state, this returns the error.
pub fn phase_si_in(
    registry: &Registry,
    name1: &str,
    value1: f64,
    name2: &str,
    value2: f64,
    fluid: &str,
) -> Result<&'static str, CompatError> {
    phase_si_with(registry, &FlashOptions::new(), name1, value1, name2, value2, fluid)
}

/// [`phase_si_in`] under the flash options `flash`, as [`props_si_with`].
pub fn phase_si_with(
    registry: &Registry,
    flash: &FlashOptions,
    name1: &str,
    value1: f64,
    name2: &str,
    value2: f64,
    fluid: &str,
) -> Result<&'static str, CompatError> {
    Ok(phase_name(Plan::new(registry, "Phase", name1, name2, fluid)?.state(value1, value2, flash)?.phase()))
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

/// `Props1SI` over the embedded registry.
#[cfg(feature = "embedded")]
pub fn props1_si(output: &str, fluid: &str) -> Result<f64, CompatError> {
    props1_si_in(Registry::embedded()?, output, fluid)
}

/// `PhaseSI` over the embedded registry.
#[cfg(feature = "embedded")]
pub fn phase_si(name1: &str, value1: f64, name2: &str, value2: f64, fluid: &str) -> Result<&'static str, CompatError> {
    phase_si_in(Registry::embedded()?, name1, value1, name2, value2, fluid)
}

#[cfg(all(test, feature = "embedded"))]
mod tests {
    use super::*;
    use phasekit_core::{Quantity, Var};

    /// User decision EM12: a refusal's stable code, core's for a kernel error and compat's own for a string; an invalid
    /// input's rule, core's.
    #[test]
    fn every_refusal_has_its_code() {
        let zero = Error::InvalidInput { quantity: Quantity::TEMPERATURE, value: 0.0 };
        let cases = [
            (CompatError::UnknownOutput("Y".into()), "unknown_output", None),
            (CompatError::UnknownInput("Y".into()), "unknown_input", None),
            (CompatError::InvalidPair("T".into(), "T".into()), "invalid_pair", None),
            (CompatError::InvalidFluid("A&B".into()), "invalid_fluid", None),
            (CompatError::NotAConstant("Hmass".into()), "not_a_constant", None),
            (CompatError::NotAProperty("Phase".into()), "not_a_property", None),
            (CompatError::Core(zero), "invalid_input", Some(Rule::NotAboveAbsoluteZero)),
            (CompatError::Core(Error::ForeignState), "foreign_state", None),
        ];
        for (error, code, rule) in cases {
            assert_eq!((error.code(), error.rule()), (code, rule), "{error:?}");
        }
    }

    /// ROT-165 (map 01 R7): CoolProp returns an output equal to one of its inputs before any check, so
    /// `PropsSI("T", "T", -5, "P", 101325, "Water")` echoes −5 K. Here every input passes the kernel's gate: an
    /// impossible input is an error whatever is asked for, and a valid one is answered by the flash.
    #[test]
    fn output_equal_to_an_input_is_not_echoed() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let refused = props_si_in(&registry, "T", "T", -5.0, "P", 101_325.0, "Water");
        assert!(
            matches!(refused, Err(CompatError::Core(Error::InvalidInput { quantity: Quantity::TEMPERATURE, .. }))),
            "{refused:?}"
        );
        let q = props_si_in(&registry, "Q", "Q", 7.0, "T", 300.0, "Water");
        assert!(matches!(q, Err(CompatError::Core(Error::InvalidInput { .. }))), "{q:?}");
        let echoed = props_si_in(&registry, "T", "T", 700.0, "Dmolar", 10.0, "Water").unwrap();
        assert_eq!(echoed, 700.0);
        let rho = props_si_in(&registry, "Dmolar", "Dmolar", 10.0, "T", 700.0, "HEOS::Water").unwrap();
        assert!((rho / 10.0 - 1.0).abs() < 1e-15, "{rho}");
    }

    /// E10 (PLAN.md M10.1): a registry re-gauged by `with_reference` serves its reference state through the strings
    /// under every name of the fluid, the `HEOS::` prefix included, bit for bit what the kernel's re-gauged handle
    /// gives; `Native` on top of it restores the EOS's own values, and the registry it was made from is unchanged.
    #[test]
    fn registry_with_reference_reaches_strings() {
        use phasekit_core::{Quality, ReferenceState, Temperature};
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let native = registry.get("n-Propane").unwrap();
        let liquid = Input::qt(Quality::new(0.0).unwrap(), Temperature::new(273.15).unwrap());
        let strings =
            |reg: &Registry, output, fluid| props_si_in(reg, output, "T", 273.15, "Q", 0.0, fluid).map(f64::to_bits);
        let iir = registry.with_reference("propane", ReferenceState::Iir).unwrap();
        let kernel = native.with_reference(ReferenceState::Iir).unwrap().state(liquid).unwrap();
        for fluid in ["n-Propane", "R290", "HEOS::propane", "74-98-6"] {
            assert_eq!(strings(&iir, "H", fluid), Ok(kernel.h(Basis::Mass).to_bits()), "{fluid}");
            assert_eq!(strings(&iir, "S", fluid), Ok(kernel.s(Basis::Mass).to_bits()), "{fluid}");
        }
        let state = native.state(liquid).unwrap();
        let back = iir.with_reference("n-Propane", ReferenceState::Native).unwrap();
        for reg in [&registry, &back] {
            assert_eq!(strings(reg, "H", "n-Propane"), Ok(state.h(Basis::Mass).to_bits()));
            assert_eq!(strings(reg, "S", "n-Propane"), Ok(state.s(Basis::Mass).to_bits()));
        }
    }

    /// Rot: ROT-164 (map 01 R2, R3). CoolProp's `generate_update_pair` has no branch for Q with S or H, so 8 of its 43
    /// pairs cannot be reached from PropsSI. Here every one of the 19 pairs reaches its kernel flash by name, in both
    /// bases and in either order of the keys, bit for bit the kernel's own: from a two-phase Water state at 400 K for the
    /// pairs with Q, a gas at 700 K for the others.
    #[test]
    fn all_19_pairs_by_name_in_both_bases() {
        use phasekit_core::{Density, Quality, Temperature};
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let wet = water.state(Input::qt(Quality::new(0.3).unwrap(), Temperature::new(400.0).unwrap())).unwrap();
        let gas = water.state(Input::dt(Density::molar(10.0).unwrap(), Temperature::new(700.0).unwrap())).unwrap();
        let (mut reached, mut names) = (0, Vec::new());
        for (&pair, basis) in Pair::ALL.iter().flat_map(|p| [(p, Basis::Molar), (p, Basis::Mass)]) {
            let (x, y) = pair.vars();
            let state = if x == Var::Q || y == Var::Q { &wet } else { &gas };
            let key = |v: Var| {
                let names = ["T", "P", "Q", "Dmolar", "Dmass", "Hmolar", "Hmass", "Smolar", "Smass", "Umolar", "Umass"];
                *names
                    .iter()
                    .find(|n| parse_input(n).is_some_and(|(var, b)| var == v && b.is_none_or(|b| b == basis)))
                    .unwrap()
            };
            let value = |v: Var| match v {
                Var::T => state.t(),
                Var::P => state.p(),
                Var::Q => state.quality().unwrap(),
                Var::D => state.rho(basis),
                Var::H => state.h(basis),
                Var::S => state.s(basis),
                _ => state.u(basis),
            };
            let kernel = water.state(Input::new(pair, value(x), value(y), basis).unwrap()).unwrap().t().to_bits();
            for ((k1, v1), (k2, v2)) in
                [((key(x), value(x)), (key(y), value(y))), ((key(y), value(y)), (key(x), value(x)))]
            {
                let got = props_si_in(&registry, "T", k1, v1, k2, v2, "Water").map(f64::to_bits);
                assert_eq!(got, Ok(kernel), "{pair:?} {basis:?} by {k1}, {k2}");
                reached += 1;
            }
            names.push((key(x), key(y)));
        }
        assert_eq!(reached, 19 * 2 * 2);
        assert!(names.contains(&("Q", "Smass")) && names.contains(&("Hmolar", "Q")), "the pairs CoolProp cannot reach");
    }

    /// Rot: ROT-166 (map 01 R17). CoolProp parses a fluid string differently per entry point and leniently (`MEG-abc%`
    /// is 0 %). Here one grammar: a registered name, alias or CAS number, `HEOS::` optional; backend options, a
    /// concentration, an empty or a padded string is refused by name before any lookup; any other prefix is part of a
    /// name the registry does not have; and a mixture or fractions go whole to the registry's resolvers, whose grammar
    /// is theirs (phasekit-mix's, PLAN.md M13.13): a registry without one holds no such name.
    #[test]
    fn fluid_string_grammar_is_strict() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let call = |fluid| props_si_in(&registry, "P", "T", 700.0, "Dmolar", 10.0, fluid);
        let water = call("Water").unwrap();
        for fluid in ["Water", "water", "WATER", "HEOS::Water", "R718", "7732-18-5", "HEOS::h2o"] {
            assert_eq!(call(fluid), Ok(water), "{fluid}");
        }
        let syntax = ["", "HEOS::", " Water", "Water ", "MEG-20%", "INCOMP::MEG-20%", "Water?{}", "Water[1.0]?"];
        for fluid in syntax {
            assert_eq!(call(fluid), Err(CompatError::InvalidFluid(fluid.into())), "{fluid:?}");
        }
        let prefixed = ["REFPROP::Water", "INCOMP::MEG", "Water-SRK", "SRK::Water", "HEOS::HEOS::Water", "Waterx"];
        let mixed = ["Water&Ethanol", "Water[1.0]", "HEOS::Water[1.0]", "Water[0,5]"];
        for fluid in prefixed.into_iter().chain(mixed) {
            let unknown =
                matches!(call(fluid), Err(CompatError::Core(Error::Load(phasekit_core::LoadError::UnknownName(_)))));
            assert!(unknown, "{fluid}: {:?}", call(fluid));
        }
        let unprefixed = Err(CompatError::Core(Error::Load(phasekit_core::LoadError::UnknownName("Waterx".into()))));
        assert_eq!(call("HEOS::Waterx"), unprefixed);
    }

    /// Rot: ROT-168 (map 14 R9). CoolProp's scalar PropsSI splits an output on '&', then demands one value, and fails
    /// with "error was  :". Here several outputs are a list of calls (or a batch): a joined key is an unknown output.
    #[test]
    fn ampersand_output_is_an_unknown_output() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        for key in ["D&H", "H&S", "T&P", "&", "Hmass&"] {
            let got = props_si_in(&registry, key, "T", 700.0, "Dmolar", 10.0, "Water");
            assert_eq!(got, Err(CompatError::UnknownOutput(key.into())), "{key}");
        }
    }

    /// Rot: ROT-172 (map 01 R18). CoolProp's factory splits a fluid on '&' before it strips a `?{json}` options suffix,
    /// and PropsSI refuses a JSON array in it. Here there are no string backend options (typed `FlashOptions`, D7): any
    /// suffix is refused whole, and a tabular backend's name is a name nothing holds, before a fluid is decoded.
    #[test]
    fn backend_options_suffix_is_refused() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let call = |fluid| props_si_in(&registry, "P", "T", 700.0, "Dmolar", 10.0, fluid);
        let suffixed = [
            r#"Water?{"critical_patch":{"mode":"a&b"}}"#,
            r#"Water?{"bbox":[600,650,1e7,3e7]}"#,
            "Water?@options.json",
            "HEOS::Water?",
        ];
        for fluid in suffixed {
            assert_eq!(call(fluid), Err(CompatError::InvalidFluid(fluid.into())), "{fluid}");
        }
        let tabular = "SVDSBTL&HEOS::Water";
        assert_eq!(
            call(tabular),
            Err(CompatError::Core(Error::Load(phasekit_core::LoadError::UnknownName(tabular.into()))))
        );
        assert_eq!(registry.loaded().count(), 0);
    }

    /// Rot: ROT-057 (map 09 R17). CoolProp's environmental metadata carry sentinels (ODP −1 for 100 fluids, GWP100 −1
    /// for 59) and come from a restricted table. Here none ships (D14): for every fluid, the seven numbers are `NoModel`
    /// through Props1SI and PropsSI alike, never a number.
    #[test]
    fn compat_metadata_has_no_sentinels() {
        use phasekit_core::Constant;
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let metadata = [
            ("ODP", Constant::Odp),
            ("GWP20", Constant::Gwp20),
            ("GWP100", Constant::Gwp100),
            ("GWP500", Constant::Gwp500),
            ("FH", Constant::Fh),
            ("HH", Constant::Hh),
            ("PH", Constant::Ph),
        ];
        let mut fluids = 0;
        for name in phasekit_data::FLUIDS.iter().map(|f| f.name) {
            for (key, constant) in metadata {
                let no_model = Err(CompatError::Core(Error::NoModel { prop: Prop::Constant(constant) }));
                assert_eq!(props1_si_in(&registry, key, name), no_model, "{name} {key}");
            }
            fluids += 1;
        }
        let water = props_si_in(&registry, "ODP", "T", 700.0, "Dmolar", 10.0, "Water");
        assert_eq!(water, Err(CompatError::Core(Error::NoModel { prop: Prop::Constant(Constant::Odp) })));
        assert_eq!(fluids, 136);
    }

    /// Rot: ROT-170 (map 14 U1, §3.1-§3.2: the Python contract). A vector call checks its keys and fluid first, for the
    /// whole call; a length-1 operand broadcasts; each point is the scalar call's, bit for bit; a failed point is +inf
    /// under `Inf` (CoolProp's), NaN under `Nan`, the call's error under `Error`; every point failing is the first one's
    /// error; a state-independent output broadcasts its constant; an empty call is empty.
    #[test]
    fn fill_policies_match_the_python_contract() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let vec =
            |output, t: &[f64], p: &[f64], fill| props_si_vec_in(&registry, output, "T", t, "P", p, "Water", fill);
        let scalar = |t| props_si_in(&registry, "D", "T", t, "P", 101_325.0, "Water");
        let (cold, hot) = (scalar(300.0).unwrap(), scalar(400.0).unwrap());
        let bad = scalar(-5.0).unwrap_err();
        let p = [101_325.0];
        let bits = |v: Vec<f64>| v.into_iter().map(f64::to_bits).collect::<Vec<_>>();
        let inf = vec("D", &[300.0, -5.0, 400.0], &p, FillPolicy::Inf).map(bits);
        assert_eq!(inf, Ok(bits(vec![cold, f64::INFINITY, hot])));
        let nan = vec("D", &[300.0, f64::NAN, 400.0], &p, FillPolicy::Nan).unwrap();
        assert!(nan[0] == cold && nan[1].is_nan() && nan[2] == hot, "{nan:?}");
        assert_eq!(vec("D", &[300.0, -5.0, 400.0], &p, FillPolicy::Error), Err(bad.clone()));
        assert_eq!(vec("D", &[-5.0, -6.0], &p, FillPolicy::Inf), Err(bad));
        assert_eq!(vec("D", &[300.0], &[101_325.0, 101_325.0], FillPolicy::Inf), Ok(vec![cold, cold]));
        let shape = Err(CompatError::Core(Error::Shape { expected: 3, found: 2 }));
        assert_eq!(vec("D", &[300.0, 400.0, 500.0], &[1e5, 2e5], FillPolicy::Inf), shape);
        let tc = props1_si_in(&registry, "Tcrit", "Water").unwrap();
        assert_eq!(vec("Tcrit", &[300.0, 400.0], &p, FillPolicy::Inf), Ok(vec![tc, tc]));
        assert_eq!(vec("D", &[], &[], FillPolicy::Inf), Ok(vec![]));
        assert_eq!(vec("D&H", &[300.0], &p, FillPolicy::Inf), Err(CompatError::UnknownOutput("D&H".into())));
        assert_eq!(props1_si_in(&registry, "T", "Water"), Err(CompatError::NotAConstant("T".into())));
    }

    /// ROT-151 (map 12 R18: a `de_DE` locale breaks CoolProp's number parsing). compat parses no number: a fluid string
    /// with fractions, in either decimal punctuation, goes whole to the registry's resolvers (a registry without one
    /// holds no such name; phasekit-mix's reads a decimal point only, `tests/mixture_names.rs`), and a key padded, in
    /// another case or with a separator is unknown. Values cross as `f64` only, so no locale reaches them.
    #[test]
    fn malformed_numbers_and_keys_are_refused() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let call = |output, name1, fluid| props_si_in(&registry, output, name1, 300.0, "P", 1e5, fluid);
        for fluid in ["Water[0.5]", "Water[0,5]", "R32[0.5]&R125[0.5]", "R32[0,5]&R125[0,5]"] {
            let unknown = Err(CompatError::Core(Error::Load(phasekit_core::LoadError::UnknownName(fluid.into()))));
            assert_eq!(call("Hmass", "T", fluid), unknown, "{fluid}");
        }
        assert!(matches!(call("Hmass", "T", "Water 1,0"), Err(CompatError::Core(Error::Load(_)))), "no such fluid");
        for key in ["Hmass ", " Hmass", "hmass", "Hmass,", "H,mass", "1,5"] {
            assert_eq!(call(key, "T", "Water"), Err(CompatError::UnknownOutput(key.into())), "{key}");
            assert_eq!(call("Hmass", key, "Water"), Err(CompatError::UnknownInput(key.into())), "{key}");
        }
    }

    /// M10.8: the browser package's typed flash and its outputs are `props_si_in`'s, bit for bit or the same refusal:
    /// `flash_si` reads the keys as `PropsSI` reads them (either order, the basis from the keys) and takes the flash
    /// options; `output_at` reads any output key at the state, `Phase` too.
    #[test]
    fn flash_si_and_output_at_are_props_si() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let options = FlashOptions::new();
        for ((n1, v1), (n2, v2)) in
            [(("T", 300.0), ("P", 1e5)), (("P", 1e5), ("T", 300.0)), (("Hmass", 2e6), ("P", 1e5))]
        {
            let state = flash_si(water, (n1, v1), (n2, v2), &options).unwrap();
            for key in ["Dmass", "Hmolar", "Q", "Phase"] {
                let want = props_si_in(&registry, key, n1, v1, n2, v2, "Water").map(f64::to_bits);
                assert_eq!(output_at(water, &state, key).map(f64::to_bits), want, "{n1} {n2} {key}");
            }
            assert_eq!(output_at(water, &state, "X"), Err(CompatError::UnknownOutput("X".into())));
        }
        let refused = flash_si(water, ("T", 300.0), ("T", 300.0), &options).unwrap_err();
        assert_eq!(refused, CompatError::InvalidPair("T".into(), "T".into()));
        let extrapolate = options.with_domain(phasekit_core::DomainPolicy::Extrapolate);
        let supercooled = |flash| flash_si(water, ("T", 260.0), ("P", 1e5), flash).map(|s| s.t());
        assert!(supercooled(&options).is_err() && supercooled(&extrapolate) == Ok(260.0));
    }

    /// The string grammar: unknown keys and pairs whose bases disagree are refused by name, before the kernel.
    #[test]
    fn keys_and_pairs_are_checked() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let call = |output, name1, name2| props_si_in(&registry, output, name1, 700.0, name2, 10.0, "Water");
        assert_eq!(call("X", "T", "Dmolar"), Err(CompatError::UnknownOutput("X".into())));
        assert_eq!(call("P", "Y", "Dmolar"), Err(CompatError::UnknownInput("Y".into())));
        assert_eq!(call("P", "T", "Z"), Err(CompatError::UnknownInput("Z".into())));
        assert_eq!(call("P", "Hmass", "Dmolar"), Err(CompatError::InvalidPair("Hmass".into(), "Dmolar".into())));
        assert_eq!(call("P", "T", "T"), Err(CompatError::InvalidPair("T".into(), "T".into())));
        assert_eq!(format!("{}", CompatError::UnknownOutput("X".into())), "unknown output key `X`");
        assert_eq!(props_si("P", "T", 700.0, "Dmolar", 10.0, "Water"), call("P", "T", "Dmolar"));
        // The embedded registry's Props1SI and PhaseSI are the registry forms' over it.
        let embedded = Registry::embedded().unwrap();
        assert_eq!(props1_si("Tcrit", "Water"), props1_si_in(embedded, "Tcrit", "Water"));
        assert!(props1_si("Tcrit", "Water").is_ok_and(|t| (t - 647.096).abs() < 1e-3));
        assert_eq!(phase_si("T", 700.0, "Dmolar", 10.0, "Water"), Ok("supercritical_gas"));
        assert_eq!(phase_si("T", 300.0, "P", 101_325.0, "Water"), Ok("liquid"));
    }

    /// M10.5: `batch_si` is `props_si_in` cell for cell, bit for bit or the same refusal as its status, the keys read as
    /// `PropsSI` reads them: `T` before `P` reverses the pair's order, and `Hmass` makes the basis mass. A bad key, an
    /// output that is not a property or a short buffer refuses the whole call, with no cell written.
    #[test]
    fn batch_si_is_props_si_cell_for_cell() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let outputs = ["Hmass", "Dmolar", "Q", "viscosity"];
        let failed = |(n1, v1): (&str, &[f64; 3]), (n2, v2): (&str, &[f64; 3])| {
            let (mut out, mut status) = ([0.0; 12], [Status::Ok; 12]);
            let summary = batch_si(water, &outputs, (n1, v1), (n2, v2), &mut out, &mut status).unwrap();
            for (i, k) in (0..3).flat_map(|i| (0..4).map(move |k| (i, k))) {
                let want = match props_si_in(&registry, outputs[k], n1, v1[i], n2, v2[i], "Water") {
                    Ok(v) => (v.to_bits(), Status::Ok),
                    Err(CompatError::Core(e)) => (f64::NAN.to_bits(), Status::from(&e)),
                    Err(_) => (u64::MAX, Status::Other), // a key the call refuses whole: never here
                };
                assert_eq!((out[i * 4 + k].to_bits(), status[i * 4 + k]), want, "{n1} {n2} point {i} {}", outputs[k]);
            }
            summary.failed_cells
        };
        let (t, p, h) = ([300.0, 700.0, -1.0], [1e5, 1e6, 1e5], [1e5, 3e6, 2e6]);
        assert!((4..12).contains(&failed(("T", &t), ("P", &p))), "T ≤ 0 fails its point, and only Q elsewhere");
        assert!(failed(("P", &p), ("Hmass", &h)) < 12);
        let (mut out, mut status) = ([7.0; 3], [Status::Ok; 3]);
        let mut call = |outputs: &[&str], n2| batch_si(water, outputs, ("T", &t), (n2, &p), &mut out, &mut status);
        assert_eq!(call(&["Phase"], "P"), Err(CompatError::NotAProperty("Phase".into())));
        assert_eq!(call(&["Hmass", "X"], "P"), Err(CompatError::UnknownOutput("X".into())));
        assert_eq!(call(&["Hmass"], "T"), Err(CompatError::InvalidPair("T".into(), "T".into())));
        assert_eq!(call(&["Hmass", "P"], "P"), Err(CompatError::Core(Error::Shape { expected: 6, found: 3 })));
        assert_eq!(out, [7.0; 3], "a refused call writes no cell");
        assert_eq!(
            format!("{}", CompatError::NotAProperty("Phase".into())),
            "`Phase` is not a property; a batch's outputs are"
        );
    }

    /// K17, M12.10: `batch_si_with` under every policy writes `batch_si`'s cells and statuses bit for bit, and refuses
    /// what it refuses: n-Propane's DT, QT and PQ over 21 points, single-phase, two-phase and failing ones.
    #[test]
    fn batch_si_with_every_policy_is_batch_si() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let propane = registry.get("n-Propane").unwrap();
        let t: Vec<f64> = (0..21).map(|i| 150.0 + 12.0 * f64::from(i)).collect();
        let d: Vec<f64> = core::iter::successors(Some(0.5), |d| Some(d * 1.4)).take(21).collect();
        let q: Vec<f64> = (0..21).map(|i| f64::from(i % 4) / 3.0).collect();
        let p: Vec<f64> = (0..21).map(|i| 2.5e5 * f64::from(i - 1)).collect();
        let parallel = ExecPolicy::Parallel { chunk: core::num::NonZeroUsize::MIN };
        let (t, d, q, p) = (&t[..], &d[..], &q[..], &p[..]);
        for (input1, input2) in [(("T", t), ("Dmass", d)), (("T", t), ("Q", q)), (("P", p), ("Q", q))] {
            let outputs = ["Hmass", "Cpmass", "P", "T"];
            let (mut want, mut cells) = (vec![0.0; 84], vec![Status::Ok; 84]);
            let summary = batch_si(propane, &outputs, input1, input2, &mut want, &mut cells).unwrap();
            assert!(summary.failed_cells > 0 && summary.failed_cells < 84, "{input1:?}: {summary:?}");
            for exec in [ExecPolicy::Reference, parallel, ExecPolicy::Simd] {
                let (mut out, mut status) = (vec![0.0; 84], vec![Status::Ok; 84]);
                let got = batch_si_with(propane, exec, &outputs, input1, input2, &mut out, &mut status).unwrap();
                let bits = |values: &[f64]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
                assert_eq!((bits(&out), status, got), (bits(&want), cells.clone(), summary), "{exec:?} {input1:?}");
            }
        }
        let (mut out, mut status) = ([7.0; 4], [Status::Ok; 4]);
        let refused = batch_si_with(propane, ExecPolicy::Simd, &["X"], ("T", t), ("Q", q), &mut out, &mut status);
        assert_eq!((refused, out), (Err(CompatError::UnknownOutput("X".into())), [7.0; 4]));
    }

    /// Every key of the grammar reaches its kernel name, CoolProp's spellings and one-letter aliases (map 01 §4a): an
    /// output key through `props_si_in` is `Fluid::prop` of the same DT state, bit for bit or the same refusal; an
    /// input key builds its own pair (refused by name where the kernel has no flash for it yet) and basis.
    #[test]
    fn every_key_reaches_its_kernel_name() {
        use phasekit_core::{Density, Enthalpy, Pressure, Quality, Temperature};
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let water = registry.get("Water").unwrap();
        let dt = |rho| Input::dt(rho, Temperature::new(700.0).unwrap());
        let state = water.state(dt(Density::molar(10.0).unwrap())).unwrap();
        let outputs = [
            ("T", Prop::T),
            ("P", Prop::P),
            ("Q", Prop::Q),
            ("D", Prop::Dmass),
            ("Dmass", Prop::Dmass),
            ("Dmolar", Prop::Dmolar),
            ("H", Prop::Hmass),
            ("Hmass", Prop::Hmass),
            ("Hmolar", Prop::Hmolar),
            ("S", Prop::Smass),
            ("Smass", Prop::Smass),
            ("Smolar", Prop::Smolar),
            ("U", Prop::Umass),
            ("Umass", Prop::Umass),
            ("Umolar", Prop::Umolar),
            ("C", Prop::Cpmass),
            ("Cpmass", Prop::Cpmass),
            ("Cpmolar", Prop::Cpmolar),
            ("O", Prop::Cvmass),
            ("Cvmass", Prop::Cvmass),
            ("Cvmolar", Prop::Cvmolar),
            ("A", Prop::SpeedOfSound),
            ("speed_of_sound", Prop::SpeedOfSound),
            ("Z", Prop::Z),
            ("Cp0molar", Prop::Cp0molar),
            ("Cp0mass", Prop::Cp0mass),
            ("M", Prop::Constant(phasekit_core::Constant::MolarMass)),
            ("molar_mass", Prop::Constant(phasekit_core::Constant::MolarMass)),
            ("V", Prop::Viscosity),
            ("viscosity", Prop::Viscosity),
            ("L", Prop::Conductivity),
            ("conductivity", Prop::Conductivity),
            ("I", Prop::SurfaceTension),
            ("surface_tension", Prop::SurfaceTension),
        ];
        for (key, prop) in outputs {
            let got = props_si_in(&registry, key, "T", 700.0, "Dmolar", 10.0, "Water").map(f64::to_bits);
            assert_eq!(got, water.prop(&state, prop).map(f64::to_bits).map_err(CompatError::Core), "{key}");
        }
        // A density key's basis reaches the flash.
        let mass = water.state(dt(Density::mass(0.18).unwrap())).unwrap().p();
        for key in ["D", "Dmass"] {
            assert_eq!(props_si_in(&registry, "P", "T", 700.0, key, 0.18, "Water"), Ok(mass), "{key}");
        }
        // Q with T reaches QT (M6.8): at 400 K Water's saturation pressure, above Tc a domain error.
        let quality = Quality::new(0.5).unwrap();
        let at_400 = water.state(Input::qt(quality, Temperature::new(400.0).unwrap())).unwrap().p();
        assert_eq!(props_si_in(&registry, "P", "T", 400.0, "Q", 0.5, "Water"), Ok(at_400));
        let above = props_si_in(&registry, "P", "T", 700.0, "Q", 0.5, "Water");
        assert!(matches!(above, Err(CompatError::Core(Error::Domain(_)))), "{above:?}");
        // P with T reaches PT (M7.1).
        let pt = water.state(Input::pt(Pressure::new(0.5).unwrap(), Temperature::new(700.0).unwrap())).unwrap().p();
        assert_eq!(props_si_in(&registry, "P", "T", 700.0, "P", 0.5, "Water"), Ok(pt));
        // The other inputs with T reach their pair's flash (M7.5) in the key's basis; CoolProp's H, S and U are mass.
        let (mass, molar) = (Basis::Mass, Basis::Molar);
        let others = [("H", Var::H, mass), ("Hmass", Var::H, mass), ("Hmolar", Var::H, molar)];
        let others =
            others.into_iter().chain([("S", Var::S, mass), ("Smass", Var::S, mass), ("Smolar", Var::S, molar)]);
        for (key, var, basis) in others.chain([("U", Var::U, mass), ("Umass", Var::U, mass), ("Umolar", Var::U, molar)])
        {
            let (pair, swapped) = Pair::from_vars(Var::T, var).unwrap();
            let (x, y) = if swapped { (0.5, 700.0) } else { (700.0, 0.5) };
            let kernel = Input::new(pair, x, y, basis).and_then(|input| water.state(input)).map(|s| s.p());
            assert_eq!(
                props_si_in(&registry, "P", "T", 700.0, key, 0.5, "Water"),
                kernel.map_err(CompatError::Core),
                "{key}"
            );
        }
        // Two keys of one basis pass the basis check, to the kernel's DH (M7.4); two of different bases do not.
        let same = props_si_in(&registry, "P", "Dmolar", 10.0, "Hmolar", 1.0, "Water");
        let dh = Input::dh(Density::molar(10.0).unwrap(), Enthalpy::molar(1.0).unwrap());
        assert_eq!(same, water.state(dh).map(|s| s.p()).map_err(CompatError::Core));
        let mixed = props_si_in(&registry, "P", "Dmass", 0.18, "Hmolar", 1.0, "Water");
        assert_eq!(mixed, Err(CompatError::InvalidPair("Dmass".into(), "Hmolar".into())));
    }
}
