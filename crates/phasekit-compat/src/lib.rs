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

#[cfg(all(test, feature = "embedded"))]
mod tests {
    use super::*;

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
        use phasekit_core::{Density, Pressure, Quality, Temperature};
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
            ("M", Prop::MolarMass),
            ("molar_mass", Prop::MolarMass),
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
        // The other inputs with T: their pair, which the kernel refuses by name until its flash lands (M7).
        let others = [("H", Var::H), ("Hmass", Var::H), ("Hmolar", Var::H)];
        let others = others.into_iter().chain([("S", Var::S), ("Smass", Var::S), ("Smolar", Var::S)]);
        for (key, var) in others.chain([("U", Var::U), ("Umass", Var::U), ("Umolar", Var::U)]) {
            let (pair, _) = Pair::from_vars(Var::T, var).unwrap();
            let refused = props_si_in(&registry, "P", "T", 700.0, key, 0.5, "Water");
            assert_eq!(refused, Err(CompatError::Core(Error::Unsupported { pair })), "{key}");
        }
        // Two keys of one basis pass the basis check; two of different bases do not.
        let same = props_si_in(&registry, "P", "Dmolar", 10.0, "Hmolar", 1.0, "Water");
        assert_eq!(same, Err(CompatError::Core(Error::Unsupported { pair: Pair::DH })));
        let mixed = props_si_in(&registry, "P", "Dmass", 0.18, "Hmolar", 1.0, "Water");
        assert_eq!(mixed, Err(CompatError::InvalidPair("Dmass".into(), "Hmolar".into())));
    }
}
