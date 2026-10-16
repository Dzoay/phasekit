//! CoolProp's key tables (map 01 §4a-§4c): the output and input keys, the input-pair names and the phase names. Each
//! table lists every name once; a key also matches as its all-uppercase form, as CoolProp registers both (`DMASS`
//! works, `dmass` does not). Matching is ASCII byte for byte, so no locale enters it (map 01 R17, R18).

use phasekit_core::{Basis, Constant, DerivVar, Pair, Partial, Phase, Prop, ReferenceState, SecondPartial, Var};

/// What a `PropsSI` output key names: a kernel property, or the state's phase (CoolProp's `Phase`, its index as a
/// float).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Output {
    /// A row of the kernel's one output table (`Fluid::prop`).
    Prop(Prop),
    /// The phase, as CoolProp's index ([`phase_index`]).
    Phase,
}

/// CoolProp's 85 outputs in `parameter_info_list` order (`DataStructures.cpp:20-112`), then its 24 aliases
/// (`:130-153`).
const OUTPUTS: [(&str, Output); 109] = {
    use Constant as C;
    use Output::Prop as P;
    [
        ("T", P(Prop::T)),
        ("P", P(Prop::P)),
        ("Dmolar", P(Prop::Dmolar)),
        ("Hmolar", P(Prop::Hmolar)),
        ("Smolar", P(Prop::Smolar)),
        ("Umolar", P(Prop::Umolar)),
        ("Gmolar", P(Prop::Gmolar)),
        ("Helmholtzmolar", P(Prop::Helmholtzmolar)),
        ("Dmass", P(Prop::Dmass)),
        ("Hmass", P(Prop::Hmass)),
        ("Smass", P(Prop::Smass)),
        ("Umass", P(Prop::Umass)),
        ("Gmass", P(Prop::Gmass)),
        ("Helmholtzmass", P(Prop::Helmholtzmass)),
        ("Q", P(Prop::Q)),
        ("Qmass", P(Prop::Qmass)),
        ("Delta", P(Prop::Delta)),
        ("Tau", P(Prop::Tau)),
        ("Cpmolar", P(Prop::Cpmolar)),
        ("Cpmass", P(Prop::Cpmass)),
        ("Cvmolar", P(Prop::Cvmolar)),
        ("Cvmass", P(Prop::Cvmass)),
        ("Cp0molar", P(Prop::Cp0molar)),
        ("Cp0mass", P(Prop::Cp0mass)),
        ("Hmolar_residual", P(Prop::HmolarResidual)),
        ("Smolar_residual", P(Prop::SmolarResidual)),
        ("Gmolar_residual", P(Prop::GmolarResidual)),
        ("Hmolar_idealgas", P(Prop::HmolarIdealGas)),
        ("Smolar_idealgas", P(Prop::SmolarIdealGas)),
        ("Umolar_idealgas", P(Prop::UmolarIdealGas)),
        ("Hmass_idealgas", P(Prop::HmassIdealGas)),
        ("Smass_idealgas", P(Prop::SmassIdealGas)),
        ("Umass_idealgas", P(Prop::UmassIdealGas)),
        ("GWP20", P(Prop::Constant(C::Gwp20))),
        ("GWP100", P(Prop::Constant(C::Gwp100))),
        ("GWP500", P(Prop::Constant(C::Gwp500))),
        ("FH", P(Prop::Constant(C::Fh))),
        ("HH", P(Prop::Constant(C::Hh))),
        ("PH", P(Prop::Constant(C::Ph))),
        ("ODP", P(Prop::Constant(C::Odp))),
        ("Bvirial", P(Prop::Bvirial)),
        ("Cvirial", P(Prop::Cvirial)),
        ("dBvirial_dT", P(Prop::DBvirialDT)),
        ("dCvirial_dT", P(Prop::DCvirialDT)),
        ("gas_constant", P(Prop::Constant(C::GasConstant))),
        ("molar_mass", P(Prop::Constant(C::MolarMass))),
        ("acentric", P(Prop::Constant(C::Acentric))),
        ("dipole_moment", P(Prop::Constant(C::DipoleMoment))),
        ("rhomass_reducing", P(Prop::Constant(C::RhomassReducing))),
        ("rhomolar_reducing", P(Prop::Constant(C::RhomolarReducing))),
        ("rhomolar_critical", P(Prop::Constant(C::RhomolarCritical))),
        ("rhomass_critical", P(Prop::Constant(C::RhomassCritical))),
        ("T_reducing", P(Prop::Constant(C::TReducing))),
        ("T_critical", P(Prop::Constant(C::TCritical))),
        ("T_triple", P(Prop::Constant(C::TTriple))),
        ("T_max", P(Prop::Constant(C::TMax))),
        ("T_min", P(Prop::Constant(C::TMin))),
        ("P_min", P(Prop::Constant(C::PMin))),
        ("P_max", P(Prop::Constant(C::PMax))),
        ("p_critical", P(Prop::Constant(C::PCritical))),
        ("p_reducing", P(Prop::Constant(C::PReducing))),
        ("p_triple", P(Prop::Constant(C::PTriple))),
        ("fraction_min", P(Prop::Constant(C::FractionMin))),
        ("fraction_max", P(Prop::Constant(C::FractionMax))),
        ("T_freeze", P(Prop::Constant(C::TFreeze))),
        ("speed_of_sound", P(Prop::SpeedOfSound)),
        ("viscosity", P(Prop::Viscosity)),
        ("conductivity", P(Prop::Conductivity)),
        ("surface_tension", P(Prop::SurfaceTension)),
        ("Prandtl", P(Prop::Prandtl)),
        ("isothermal_compressibility", P(Prop::IsothermalCompressibility)),
        ("isobaric_expansion_coefficient", P(Prop::IsobaricExpansion)),
        ("isentropic_expansion_coefficient", P(Prop::IsentropicExpansion)),
        ("Z", P(Prop::Z)),
        ("fundamental_derivative_of_gas_dynamics", P(Prop::FundamentalDerivative)),
        ("PIP", P(Prop::Pip)),
        ("alphar", P(Prop::Alphar)),
        ("dalphar_dtau_constdelta", P(Prop::DalpharDtau)),
        ("dalphar_ddelta_consttau", P(Prop::DalpharDdelta)),
        ("alpha0", P(Prop::Alpha0)),
        ("dalpha0_dtau_constdelta", P(Prop::Dalpha0Dtau)),
        ("dalpha0_ddelta_consttau", P(Prop::Dalpha0Ddelta)),
        ("d2alpha0_ddelta2_consttau", P(Prop::D2alpha0Ddelta2)),
        ("d3alpha0_ddelta3_consttau", P(Prop::D3alpha0Ddelta3)),
        ("Phase", Output::Phase),
        ("D", P(Prop::Dmass)),
        ("H", P(Prop::Hmass)),
        ("M", P(Prop::Constant(C::MolarMass))),
        ("S", P(Prop::Smass)),
        ("U", P(Prop::Umass)),
        ("C", P(Prop::Cpmass)),
        ("O", P(Prop::Cvmass)),
        ("G", P(Prop::Gmass)),
        ("V", P(Prop::Viscosity)),
        ("L", P(Prop::Conductivity)),
        ("pcrit", P(Prop::Constant(C::PCritical))),
        ("Pcrit", P(Prop::Constant(C::PCritical))),
        ("Tcrit", P(Prop::Constant(C::TCritical))),
        ("Ttriple", P(Prop::Constant(C::TTriple))),
        ("ptriple", P(Prop::Constant(C::PTriple))),
        ("rhocrit", P(Prop::Constant(C::RhomassCritical))),
        ("Tmin", P(Prop::Constant(C::TMin))),
        ("Tmax", P(Prop::Constant(C::TMax))),
        ("pmax", P(Prop::Constant(C::PMax))),
        ("pmin", P(Prop::Constant(C::PMin))),
        ("molemass", P(Prop::Constant(C::MolarMass))),
        ("molarmass", P(Prop::Constant(C::MolarMass))),
        ("A", P(Prop::SpeedOfSound)),
        ("I", P(Prop::SurfaceTension)),
    ]
};

/// The number of CoolProp's own outputs at the head of the output table, before its aliases (map 01 §4a).
pub const OUTPUT_COUNT: usize = 85;

/// CoolProp's input keys: the outputs it flags "IO" that an input pair takes (Tau and Delta take none), and the aliases
/// of those; Qmass reaches the model as a mass quality (`Input::with_quality_basis`), the same number as Q for a pure or
/// pseudo-pure fluid, whose phases share one molar mass. `None`: basis-free.
const INPUTS: [(&str, Var, Option<Basis>); 16] = [
    ("T", Var::T, None),
    ("P", Var::P, None),
    ("Q", Var::Q, None),
    ("Qmass", Var::Q, None),
    ("Dmolar", Var::D, Some(Basis::Molar)),
    ("Dmass", Var::D, Some(Basis::Mass)),
    ("Hmolar", Var::H, Some(Basis::Molar)),
    ("Hmass", Var::H, Some(Basis::Mass)),
    ("Smolar", Var::S, Some(Basis::Molar)),
    ("Smass", Var::S, Some(Basis::Mass)),
    ("Umolar", Var::U, Some(Basis::Molar)),
    ("Umass", Var::U, Some(Basis::Mass)),
    ("D", Var::D, Some(Basis::Mass)),
    ("H", Var::H, Some(Basis::Mass)),
    ("S", Var::S, Some(Basis::Mass)),
    ("U", Var::U, Some(Basis::Mass)),
];

/// Whether `key` spells the table key `name`: `name` itself or its all-uppercase form, which CoolProp registers too
/// (`DataStructures.cpp:157-160`).
pub fn spells(key: &str, name: &str) -> bool {
    key == name || (key.eq_ignore_ascii_case(name) && !key.bytes().any(|b| b.is_ascii_lowercase()))
}

/// The output a CoolProp output key names: one of its 85 outputs or 24 aliases, or an uppercase form of one, or a
/// derivative string ([`parse_derivative`]).
pub fn parse_output(key: &str) -> Option<Output> {
    table(key).or_else(|| parse_derivative(key).map(Output::Prop))
}

/// One of the table's outputs.
fn table(key: &str) -> Option<Output> {
    OUTPUTS.iter().find(|(name, _)| spells(key, name)).map(|(_, output)| *output)
}

/// The first-order variable a table key names (`DerivVar`: T, P, ρ, h, s, u, g in both bases; map 01 §4a).
fn deriv_var(key: &str) -> Option<DerivVar> {
    Some(match table(key)? {
        Output::Prop(Prop::T) => DerivVar::T,
        Output::Prop(Prop::P) => DerivVar::P,
        Output::Prop(Prop::Dmolar) => DerivVar::Dmolar,
        Output::Prop(Prop::Dmass) => DerivVar::Dmass,
        Output::Prop(Prop::Hmolar) => DerivVar::Hmolar,
        Output::Prop(Prop::Hmass) => DerivVar::Hmass,
        Output::Prop(Prop::Smolar) => DerivVar::Smolar,
        Output::Prop(Prop::Smass) => DerivVar::Smass,
        Output::Prop(Prop::Umolar) => DerivVar::Umolar,
        Output::Prop(Prop::Umass) => DerivVar::Umass,
        Output::Prop(Prop::Gmolar) => DerivVar::Gmolar,
        Output::Prop(Prop::Gmass) => DerivVar::Gmass,
        _ => return None,
    })
}

/// `d(X)/d(Y)|Z`, the first partial (∂X/∂Y)_Z.
fn first(key: &str) -> Option<Partial> {
    let (of, rest) = key.strip_prefix("d(")?.split_once(")/d(")?;
    let (wrt, at) = rest.split_once(")|")?;
    Some(Partial { of: deriv_var(of)?, wrt: deriv_var(wrt)?, at: deriv_var(at)? })
}

/// A derivative string, strictly (ROT-167; map 01 R20, where CoolProp reads `x(P)/y(T)|Dmolar` as `d(P)/d(T)|Dmolar`):
/// `d(X)/d(Y)|Z`, the first partial (∂X/∂Y)_Z, or `d(d(X)/d(Y)|Z)/d(W)|V`, the second (∂/∂W (∂X/∂Y)_Z)_V, each of X to V
/// a table key of one of the 12 first-order variables, with nothing around them. CoolProp also differentiates c_v, c_p,
/// w, τ and δ, which user decision OD1 defers: such a string is an unknown output, as is `d(X)/d(Y)|sigma`.
pub fn parse_derivative(key: &str) -> Option<Prop> {
    let Some(rest) = key.strip_prefix("d(d(") else {
        return first(key).map(Prop::Partial);
    };
    // X)/d(Y)|Z)/d(W)|V: no key holds a parenthesis or a bar, so each split is the only one.
    let (of, rest) = rest.split_once(")/d(")?;
    let (wrt, rest) = rest.split_once(")|")?;
    let (at, rest) = rest.split_once(")/d(")?;
    let (wrt_2, at_2) = rest.split_once(")|")?;
    let first = Partial { of: deriv_var(of)?, wrt: deriv_var(wrt)?, at: deriv_var(at)? };
    Some(Prop::SecondPartial(SecondPartial { first, wrt: deriv_var(wrt_2)?, at: deriv_var(at_2)? }))
}

/// The output names in CoolProp's order, its 85 outputs first, then its aliases.
pub fn output_names() -> impl Iterator<Item = &'static str> {
    OUTPUTS.iter().map(|(name, _)| *name)
}

/// The variable and basis of a CoolProp input key (`None`: basis-free, T, P and Q).
pub fn parse_input(key: &str) -> Option<(Var, Option<Basis>)> {
    INPUTS.iter().find(|(name, ..)| spells(key, name)).map(|(_, var, basis)| (*var, *basis))
}

/// A variable's input key in `basis` ([`INPUTS`]' first spelling): `T`, `P`, `Q`, or `Dmolar`, `Hmass` and the like,
/// with its basis-freedom.
fn var_key(var: Var, basis: Basis) -> Option<(&'static str, bool)> {
    INPUTS.iter().find(|(_, v, b)| *v == var && b.is_none_or(|b| b == basis)).map(|(name, _, b)| (*name, b.is_none()))
}

/// A pair's name in `basis`, CoolProp's enum spelling (`HmassP_INPUTS`, `QT_INPUTS`): its two keys in the order of the
/// one pair table, then `_INPUTS`. Unlike CoolProp's name table (`DataStructures.cpp:513-520`, where QSmolar and QSmass
/// are both "QS_INPUTS", ROT-163), no two (pair, basis) with a basis-dependent variable share a name.
pub fn pair_name(pair: Pair, basis: Basis) -> Option<String> {
    let (x, y) = pair.vars();
    Some(format!("{}{}_INPUTS", var_key(x, basis)?.0, var_key(y, basis)?.0))
}

/// The pair and basis a [`pair_name`] names; the basis is `None` for a pair of basis-free variables (QT, PQ, PT).
pub fn parse_pair_name(name: &str) -> Option<(Pair, Option<Basis>)> {
    let mut named = Pair::ALL.iter().flat_map(|&pair| [(pair, Basis::Molar), (pair, Basis::Mass)]);
    let (pair, basis) = named.find(|&(pair, basis)| pair_name(pair, basis).is_some_and(|n| n == name))?;
    let (x, y) = pair.vars();
    let free = |v| var_key(v, basis).is_some_and(|(_, free)| free);
    Some((pair, if free(x) && free(y) { None } else { Some(basis) }))
}

/// The reference state a CoolProp name gives (`set_reference_stateS`, `CoolProp.cpp:969-1025`): IIR, ASHRAE, NBP, and
/// DEF or RESET for the model's own zero (`ReferenceState::Native`); exact names only.
pub fn parse_reference_state(name: &str) -> Option<ReferenceState> {
    Some(match name {
        "IIR" => ReferenceState::Iir,
        "ASHRAE" => ReferenceState::Ashrae,
        "NBP" => ReferenceState::Nbp,
        "DEF" | "RESET" => ReferenceState::Native,
        _ => return None,
    })
}

/// CoolProp's index of a phase (`enum phases`, `DataStructures.h:183-194`), which its `Phase` output returns as a float
/// (M10.6 checks the codes against the oracle). A solid, which CoolProp has no phase for, is its `unknown`.
pub fn phase_index(phase: Phase) -> u8 {
    match phase {
        Phase::Liquid => 0,
        Phase::Supercritical => 1,
        Phase::SupercriticalGas => 2,
        Phase::SupercriticalLiquid => 3,
        Phase::CriticalPoint => 4,
        Phase::Gas => 5,
        Phase::TwoPhase => 6,
        _ => 7,
    }
}

/// CoolProp's `PhaseSI` name of a phase (`phase_lookup_string`, `CoolProp.cpp:1168-1190`); a solid is `solid`.
pub fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Liquid => "liquid",
        Phase::Supercritical => "supercritical",
        Phase::SupercriticalGas => "supercritical_gas",
        Phase::SupercriticalLiquid => "supercritical_liquid",
        Phase::CriticalPoint => "critical_point",
        Phase::Gas => "gas",
        Phase::TwoPhase => "twophase",
        _ => "solid",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Map 01 §4a: the table lists CoolProp's 85 outputs then its 24 aliases, each name once, and no name or uppercase
    /// form names two outputs (CoolProp's map keeps the first of a collision; its `pcrit` and `Pcrit` share one).
    #[test]
    fn output_table_lists_each_name_once() {
        let mut keys: Vec<(String, Output)> =
            OUTPUTS.iter().flat_map(|(n, o)| [(n.to_string(), *o), (n.to_ascii_uppercase(), *o)]).collect();
        keys.sort_by(|a, b| a.0.cmp(&b.0));
        assert!(keys.windows(2).all(|w| w[0].0 != w[1].0 || w[0].1 == w[1].1), "a key names two outputs");
        let mut names: Vec<&str> = output_names().collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!((OUTPUTS.len(), names.len()), (OUTPUT_COUNT + 24, OUTPUT_COUNT + 24));
        assert_eq!(output_names().nth(OUTPUT_COUNT - 1), Some("Phase"));
        assert_eq!(output_names().nth(OUTPUT_COUNT), Some("D"));
    }

    /// Rot: ROT-167 (map 01 R20). CoolProp's derivative parser checks only for a '(' after the first character, so
    /// `x(P)/y(T)|Dmolar` reads as d(P)/d(T)|Dmolar, and its key table matches any case it registered loosely. Here a
    /// key is a table name or its exact uppercase form, or a derivative string exactly `d(X)/d(Y)|Z` or
    /// `d(d(X)/d(Y)|Z)/d(W)|V` over the 12 first-order variables, nothing else: lax spellings, lowercase forms, padding
    /// and a NUL are unknown outputs, as are derivatives of c_v, c_p, w, τ, δ (user decision OD1) and along the
    /// saturation curve; Tau, Delta and G, which no input pair takes, are not input keys.
    #[test]
    fn strict_grammar_rejects_lax_keys() {
        let lax = ["x(P)/y(T)|Dmolar", "zz(P)junk/q(T)more|Dmolar", "d(Phase)/d(T)|P", "dmass", "t", " T", "T ", "T\0"];
        for key in lax.into_iter().chain(["Hmass_IDEALGAS", "speed_of_Sound", "", "HEOS::T"]) {
            assert_eq!(parse_output(key), None, "{key:?}");
        }
        assert_eq!(parse_output("DMASS"), Some(Output::Prop(Prop::Dmass)));
        assert_eq!(parse_output("SPEED_OF_SOUND"), Some(Output::Prop(Prop::SpeedOfSound)));
        assert_eq!(parse_output("T_CRITICAL"), parse_output("T_critical"));
        for key in ["Tau", "Delta", "G", "Gmolar", "dmolar", "q", "PHASE"] {
            assert_eq!(parse_input(key), None, "{key:?}");
        }
        assert_eq!(parse_input("DMOLAR"), Some((Var::D, Some(Basis::Molar))));
        assert_eq!(parse_input("QMASS"), Some((Var::Q, None)));
        // M10.3: CoolProp's own invalid strings (`CoolProp-Tests.cpp:1806-1815`), and what it accepts beyond them.
        let invalid = ["d()/d(P)|T", "d(Dmolar)/d()|T", "d(Dmolar)/d(P)|", "d(XXXX)/d(P)|T", "d(Dmolar)d(P)|T"];
        let lax = ["D(P)/D(T)|Dmolar", "d(P) /d(T)|Dmolar", "d(P)/d(T)|Dmolar|T", "d(d(P)/d(T)|Dmolar)/d(T)"];
        let deferred = ["d(Cvmass)/d(T)|P", "d(P)/d(T)|sigma", "d(Tau)/d(T)|P", "d(Phase)/d(T)|P"];
        let others = ["d(Dmolar)/d(P)T", "d(Bvirial)/d(P)T", "d(Tcrit)/d(P)T"];
        for key in invalid.into_iter().chain(lax).chain(deferred).chain(others) {
            assert_eq!(parse_output(key), None, "{key}");
        }
        let (p, rho) = (DerivVar::P, DerivVar::Dmolar);
        let first = Partial { of: p, wrt: rho, at: DerivVar::T };
        assert_eq!(parse_output("d(P)/d(Dmolar)|T"), Some(Output::Prop(Prop::Partial(first))));
        assert_eq!(parse_output("d(P)/d(DMOLAR)|T"), parse_output("d(P)/d(Dmolar)|T"));
        let second = SecondPartial { first, wrt: rho, at: DerivVar::T };
        assert_eq!(parse_output("d(d(P)/d(Dmolar)|T)/d(Dmolar)|T"), Some(Output::Prop(Prop::SecondPartial(second))));
        let mass = Partial { of: DerivVar::Hmass, wrt: DerivVar::T, at: DerivVar::P };
        assert_eq!(parse_output("d(H)/d(T)|P"), Some(Output::Prop(Prop::Partial(mass))), "H is Hmass");
    }

    /// Map 01 R17, R18: CoolProp folds keys through the C library's locale-dependent `toupper`, under which a Turkish
    /// locale maps 'i' to 'İ' (U+0130). Here keys match byte for byte, with ASCII case only: the ASCII uppercase form of
    /// every key with an 'i' works, and a dotted capital İ or a dotless ı never matches.
    #[test]
    fn parsing_is_locale_free() {
        for name in output_names().filter(|n| n.contains('i')) {
            let upper = name.to_ascii_uppercase();
            assert_eq!(parse_output(&upper), parse_output(name), "{upper}");
            for turkish in [name.replace('i', "İ"), name.replace('i', "ı"), upper.replace('I', "İ")] {
                assert_eq!(parse_output(&turkish), None, "{turkish}");
            }
        }
        assert_eq!(parse_output("İ"), None);
        assert_eq!(parse_output("I"), Some(Output::Prop(Prop::SurfaceTension)));
    }

    /// CoolProp's five reference-state names, exactly, and nothing else (map 01 §3).
    #[test]
    fn reference_state_names_are_coolprops() {
        let names = [
            ("IIR", ReferenceState::Iir),
            ("ASHRAE", ReferenceState::Ashrae),
            ("NBP", ReferenceState::Nbp),
            ("DEF", ReferenceState::Native),
            ("RESET", ReferenceState::Native),
        ];
        for (name, reference) in names {
            assert_eq!(parse_reference_state(name), Some(reference), "{name}");
        }
        for name in ["iir", "Iir", " NBP", "NATIVE", "", "IIR\0"] {
            assert_eq!(parse_reference_state(name), None, "{name:?}");
        }
    }

    /// Map 01 §4c: every phase's CoolProp index (`enum phases`, which M10.6 checks against the oracle) and `PhaseSI`
    /// name; a solid, which CoolProp has no phase for, is its `unknown`, 7.
    #[test]
    fn phase_indices_and_names_are_coolprops() {
        let phases = [
            (Phase::Liquid, 0, "liquid"),
            (Phase::Supercritical, 1, "supercritical"),
            (Phase::SupercriticalGas, 2, "supercritical_gas"),
            (Phase::SupercriticalLiquid, 3, "supercritical_liquid"),
            (Phase::CriticalPoint, 4, "critical_point"),
            (Phase::Gas, 5, "gas"),
            (Phase::TwoPhase, 6, "twophase"),
            (Phase::Solid, 7, "solid"),
        ];
        for (phase, index, name) in phases {
            assert_eq!((phase_index(phase), phase_name(phase)), (index, name), "{phase:?}");
        }
    }

    /// Rot: ROT-163 (map 01 R2). CoolProp's name table gives QSmolar and QSmass both "QS_INPUTS" (and HQ, DQ the like),
    /// so its name round trip reads J/kg as J/mol. Here a pair's name is its keys in the one pair table's order: every
    /// (pair, basis) has one, it parses back to that pair, and to that basis wherever a variable has one; only the three
    /// basis-free pairs (QT, PQ, PT) share a name between the bases. CoolProp's colliding spellings name nothing.
    #[test]
    fn pair_names_round_trip_in_both_bases() {
        let mut names = Vec::new();
        for (&pair, basis) in Pair::ALL.iter().flat_map(|p| [(p, Basis::Molar), (p, Basis::Mass)]) {
            let name = pair_name(pair, basis).unwrap();
            let (x, y) = pair.vars();
            let free = [x, y].iter().all(|v| matches!(v, Var::T | Var::P | Var::Q));
            assert_eq!(parse_pair_name(&name), Some((pair, if free { None } else { Some(basis) })), "{name}");
            names.push(name);
        }
        names.sort();
        names.dedup();
        assert_eq!((Pair::ALL.len(), names.len()), (19, 2 * 19 - 3));
        assert_eq!(pair_name(Pair::HP, Basis::Mass).as_deref(), Some("HmassP_INPUTS"));
        assert_eq!(pair_name(Pair::QS, Basis::Molar).as_deref(), Some("QSmolar_INPUTS"));
        for collided in ["QS_INPUTS", "HQ_INPUTS", "DQ_INPUTS", "QSmolar", "hmassp_INPUTS", ""] {
            assert_eq!(parse_pair_name(collided), None, "{collided}");
        }
    }
}
