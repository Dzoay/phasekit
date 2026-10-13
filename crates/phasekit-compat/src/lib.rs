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

use phasekit_core::{Basis, Error, FlashOptions, Fluid, Input, Pair, Prop, Registry, State};

mod keys;

pub use keys::{
    OUTPUT_COUNT, Output, output_names, pair_name, parse_input, parse_output, parse_pair_name, phase_index, phase_name,
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
            CompatError::InvalidFluid(s) => write!(f, "`{s}` is not a fluid name"),
            CompatError::NotAConstant(k) => write!(f, "`{k}` depends on the state"),
            CompatError::Core(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CompatError {}

/// The fluid a fluid string names in `registry`. The grammar is one name, with `HEOS::`, the built-in backend, as an
/// optional prefix; any other prefix is part of a registered name (`PR::Water` from a cubic crate). Mixtures (`&`),
/// fractions (`[…]`), backend options (`?…`, ROT-172) and incompressible concentrations (`%`) are refused by name, as
/// are an empty or a padded string: nothing is parsed leniently (ROT-166).
pub fn fluid_in<'r>(registry: &'r Registry, fluid: &str) -> Result<&'r Fluid, CompatError> {
    let name = fluid.strip_prefix("HEOS::").unwrap_or(fluid);
    let outside = name.is_empty() || name.trim() != name || name.contains(['&', '[', ']', '?', '%']);
    if outside {
        return Err(CompatError::InvalidFluid(fluid.into()));
    }
    Ok(registry.get(name)?)
}

/// A `PropsSI` call's plan: everything that is checked before any state is flashed (CoolProp's plan-time errors,
/// map 14 §3.2), so a vector call refuses a bad key or fluid whole.
struct Plan<'r> {
    output: Output,
    pair: Pair,
    swapped: bool,
    basis: Basis,
    fluid: &'r Fluid,
}

impl<'r> Plan<'r> {
    fn new(registry: &'r Registry, output: &str, name1: &str, name2: &str, fluid: &str) -> Result<Self, CompatError> {
        let output = parse_output(output).ok_or_else(|| CompatError::UnknownOutput(output.into()))?;
        let (v1, b1) = parse_input(name1).ok_or_else(|| CompatError::UnknownInput(name1.into()))?;
        let (v2, b2) = parse_input(name2).ok_or_else(|| CompatError::UnknownInput(name2.into()))?;
        let invalid = || CompatError::InvalidPair(name1.into(), name2.into());
        let basis = match (b1, b2) {
            (Some(a), Some(b)) if a != b => return Err(invalid()),
            (a, b) => a.or(b).unwrap_or(Basis::Molar),
        };
        let (pair, swapped) = Pair::from_vars(v1, v2).ok_or_else(invalid)?;
        Ok(Plan { output, pair, swapped, basis, fluid: fluid_in(registry, fluid)? })
    }

    /// The state at the two input values, in the order of the call's keys.
    fn state(&self, value1: f64, value2: f64) -> Result<State, Error> {
        let (x, y) = if self.swapped { (value2, value1) } else { (value1, value2) };
        self.fluid.flash(Input::new(self.pair, x, y, self.basis)?, &FlashOptions::default())
    }

    /// The output at the two input values.
    fn eval(&self, value1: f64, value2: f64) -> Result<f64, Error> {
        let state = self.state(value1, value2)?;
        match self.output {
            Output::Prop(prop) => self.fluid.prop(&state, prop),
            Output::Phase => Ok(f64::from(phase_index(state.phase()))),
        }
    }
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
    Ok(Plan::new(registry, output, name1, name2, fluid)?.eval(value1, value2)?)
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
        match (plan.eval(x, y), fill) {
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
    Ok(phase_name(Plan::new(registry, "Phase", name1, name2, fluid)?.state(value1, value2)?.phase()))
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
    use phasekit_core::Var;

    /// ROT-165 (map 01 R7): CoolProp returns an output equal to one of its inputs before any check, so
    /// `PropsSI("T", "T", -5, "P", 101325, "Water")` echoes −5 K. Here every input passes the kernel's gate: an
    /// impossible input is an error whatever is asked for, and a valid one is answered by the flash.
    #[test]
    fn output_equal_to_an_input_is_not_echoed() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let refused = props_si_in(&registry, "T", "T", -5.0, "P", 101_325.0, "Water");
        assert!(
            matches!(refused, Err(CompatError::Core(Error::InvalidInput { quantity: "temperature", .. }))),
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
    /// is 0 %). Here one grammar: a registered name, alias or CAS number, `HEOS::` optional; a mixture, fractions,
    /// backend options, a concentration, an empty or a padded string is refused by name before any lookup, and any
    /// other prefix is part of a name the registry does not have.
    #[test]
    fn fluid_string_grammar_is_strict() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let call = |fluid| props_si_in(&registry, "P", "T", 700.0, "Dmolar", 10.0, fluid);
        let water = call("Water").unwrap();
        for fluid in ["Water", "water", "WATER", "HEOS::Water", "R718", "7732-18-5", "HEOS::h2o"] {
            assert_eq!(call(fluid), Ok(water), "{fluid}");
        }
        let syntax = ["", "HEOS::", " Water", "Water ", "Water&Ethanol", "Water[1.0]", "HEOS::Water[1.0]", "MEG-20%"];
        for fluid in syntax.into_iter().chain(["INCOMP::MEG-20%", "Water?{}", "Water[0,5]"]) {
            assert_eq!(call(fluid), Err(CompatError::InvalidFluid(fluid.into())), "{fluid:?}");
        }
        for fluid in ["REFPROP::Water", "INCOMP::MEG", "Water-SRK", "SRK::Water", "HEOS::HEOS::Water", "Waterx"] {
            let unknown =
                matches!(call(fluid), Err(CompatError::Core(Error::Load(phasekit_core::LoadError::UnknownName(_)))));
            assert!(unknown, "{fluid}: {:?}", call(fluid));
        }
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
    /// suffix, and a tabular backend's name, is refused whole, before a fluid is decoded.
    #[test]
    fn backend_options_suffix_is_refused() {
        let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let suffixed = [
            r#"Water?{"critical_patch":{"mode":"a&b"}}"#,
            r#"Water?{"bbox":[600,650,1e7,3e7]}"#,
            "Water?@options.json",
            "SVDSBTL&HEOS::Water",
            "HEOS::Water?",
        ];
        for fluid in suffixed {
            let got = props_si_in(&registry, "P", "T", 700.0, "Dmolar", 10.0, fluid);
            assert_eq!(got, Err(CompatError::InvalidFluid(fluid.into())), "{fluid}");
        }
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
