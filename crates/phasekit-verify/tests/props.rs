//! The full PropsSI table (PLAN.md M10.2; map 01 §4a): CoolProp's 85 outputs through the compat strings, against the
//! oracle's `props` fixtures.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::BTreeSet;

use phasekit_compat::{OUTPUT_COUNT, output_names};
use phasekit_core::{DataSet, Registry};
use phasekit_verify::{Fixture, fixture};

#[path = "common/props_rows.rs"]
mod props_rows;
#[path = "common/sweep.rs"]
mod sweep;

use props_rows::PropsCheck;

/// The oracle's `props` files of the core subset: 10 states each (5 DT, 5 QT) and the 28 trivial outputs.
const PROPS_FIXTURES: [(&str, &str); 14] = [
    fixture!("coolprop-8.0.0/props/Air.csv"),
    fixture!("coolprop-8.0.0/props/Ammonia.csv"),
    fixture!("coolprop-8.0.0/props/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/props/HFE143m.csv"),
    fixture!("coolprop-8.0.0/props/Helium.csv"),
    fixture!("coolprop-8.0.0/props/Methanol.csv"),
    fixture!("coolprop-8.0.0/props/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/props/R1130(E).csv"),
    fixture!("coolprop-8.0.0/props/R1234yf.csv"),
    fixture!("coolprop-8.0.0/props/R1234ze(E).csv"),
    fixture!("coolprop-8.0.0/props/R125.csv"),
    fixture!("coolprop-8.0.0/props/R410A.csv"),
    fixture!("coolprop-8.0.0/props/Water.csv"),
    fixture!("coolprop-8.0.0/props/n-Heptane.csv"),
];

/// The outputs no Helmholtz fluid has, which both sides refuse: a dipole moment and an incompressible solution's
/// fraction limits and freezing point.
const NEITHER: [&str; 4] = ["dipole_moment", "fraction_min", "fraction_max", "T_freeze"];

/// The outputs the register exempts at every row: the environmental metadata phasekit does not ship (DIV-0038),
/// CoolProp's unreachable `rhomass_reducing` (DIV-0039) and its third virial coefficient (DIV-0011).
const EXEMPT: [&str; 10] =
    ["GWP20", "GWP100", "GWP500", "FH", "HH", "PH", "ODP", "rhomass_reducing", "Cvirial", "dCvirial_dT"];

/// VERIFICATION.md §3.2, §11.3: the `props` files come from the pinned runner image, like every committed oracle
/// fixture.
#[test]
fn props_fixtures_name_the_pinned_generator_environment() {
    assert_eq!(phasekit_verify::on_the_pinned_image(PROPS_FIXTURES), Ok(14));
}

/// The core subset's `props` files, checked.
fn checked() -> PropsCheck {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = PropsCheck::default();
    for (path, text) in PROPS_FIXTURES {
        let table = Fixture::parse(path, text).unwrap();
        check.file(&registry, path.rsplit('/').next().unwrap().trim_end_matches(".csv"), &table);
    }
    let shown = check.failures.iter().take(20).collect::<Vec<_>>();
    let (total, headroom) = (check.failures.len(), check.headroom);
    assert_eq!(shown, Vec::<&String>::new(), "{total} failures; the rest within {headroom:.3} of their bounds");
    check
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/props/<Fluid>.csv (PLAN.md M10.2, M10.3; map 01 §4a), under
/// `Parity`: each of CoolProp's 85 outputs at 10 states per core fluid and its 28 trivial outputs through `Props1SI`, by
/// name. Each cell equals the oracle's within its class (`Prop` at max(|value|, floor), or `Term` carried through its
/// relation; `Flash` for what a state inside the dome takes from its saturation split; the `crit` kind's bounds for the
/// numerical critical point; DIV-0011's for B), or both sides refuse it, or a register entry exempts it. All 85 are
/// accounted for: 71 compared, the 4 no Helmholtz fluid has refused by both, 10 exempt at every row.
#[test]
fn all_85_outputs_match_oracle_or_a_typed_error() {
    let check = checked();
    let outputs: BTreeSet<&str> = output_names().take(OUTPUT_COUNT).collect();
    let compared: BTreeSet<&str> = check.compared.keys().map(String::as_str).filter(|o| outputs.contains(o)).collect();
    let refused_only: BTreeSet<&str> =
        check.refused.keys().map(String::as_str).filter(|o| outputs.contains(o) && !compared.contains(o)).collect();
    let exempt_only: BTreeSet<&str> =
        outputs.iter().copied().filter(|o| !compared.contains(o) && !refused_only.contains(o)).collect();
    assert_eq!(refused_only, NEITHER.into_iter().collect());
    assert_eq!(exempt_only, EXEMPT.into_iter().collect());
    assert_eq!((outputs.len(), compared.len()), (85, 85 - NEITHER.len() - EXEMPT.len()));
}

/// Oracle: CoolProp 8.0.0, the `props` files' derivative strings (PLAN.md M10.3; `CoolProp-Tests.cpp:1744, 1829`):
/// the `eos` kind's 12 first partials `d(X)/d(Y)|Z` and 10 second partials `d(d(X)/d(Y)|Z)/d(W)|V`, each of X to V one
/// of the 12 first-order variables, parsed from the string as CoolProp parses it, at every DT state of the 14 core
/// fluids that has one phase (inside the dome they are DIV-0004's), in class `Prop` at OD2's floors or with `Term`
/// carried through the relation, a first partial's Jacobian ratio, a second's to order 3.
#[test]
fn first_and_second_derivative_strings_match_oracle() {
    let check = checked();
    let derivatives: Vec<(&String, &usize)> = check.compared.iter().filter(|(o, _)| o.starts_with("d(")).collect();
    let second = derivatives.iter().filter(|(o, _)| o.starts_with("d(d(")).count();
    assert_eq!((derivatives.len() - second, second), (12, 10), "{derivatives:?}");
    assert!(derivatives.iter().all(|(_, n)| **n >= 14 * 3), "{derivatives:?}");
}

/// Oracle: CoolProp 8.0.0, the `props` files' 8 α outputs (PLAN.md M10.3; ARCHITECTURE.md §12): α^r, ∂α^r/∂τ, ∂α^r/∂δ,
/// α⁰, ∂α⁰/∂τ, ∂α⁰/∂δ, ∂²α⁰/∂δ² and ∂³α⁰/∂δ³, τ- and δ-scaled at the published reducing state, at every one-phase state
/// of the 14 core fluids (inside the dome they are DIV-0004's), in class `Prop`, or `Term` at the entry's scale over
/// τ^i δ^j, the `term` kind's class.
#[test]
fn alpha_term_outputs_match_oracle() {
    let check = checked();
    let alpha = [
        "alphar",
        "dalphar_dtau_constdelta",
        "dalphar_ddelta_consttau",
        "alpha0",
        "dalpha0_dtau_constdelta",
        "dalpha0_ddelta_consttau",
        "d2alpha0_ddelta2_consttau",
        "d3alpha0_ddelta3_consttau",
    ];
    for output in alpha {
        assert!(check.compared.get(output).is_some_and(|n| *n >= 14 * 5), "{output}: {:?}", check.compared.get(output));
    }
}
