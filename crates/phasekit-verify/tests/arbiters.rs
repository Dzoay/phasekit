//! The literature arbiters (VERIFICATION.md §4.3): each record's status is asserted against the committed files, and
//! from M4 on against the evaluation of its table with the paper's own constants.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{Basis, Order, Phase, PureFluid, Registry, State, ThermoModel, math};
use phasekit_verify::arbiters::{ArbiterPart, Role, violations};
use phasekit_verify::{ARBITERS, ArbiterStatus, Fixture, MILESTONE, Tolerance, fixture, from_printed};

const MANIFEST: &str = include_str!("../fixtures/MANIFEST.sha256");

fn committed(file: &str) -> bool {
    MANIFEST.lines().any(|line| line.rsplit(' ').next() == Some(file))
}

#[test]
fn arbiter_statuses_are_asserted() {
    assert_eq!(violations(ARBITERS, &committed, MILESTONE), Vec::<String>::new());
    let allowed = |s: &ArbiterStatus| {
        matches!(
            s,
            ArbiterStatus::Expected | ArbiterStatus::Transcribed | ArbiterStatus::None | ArbiterStatus::Unpublished
        )
    };
    // The records evaluated so far, each by its own test in this file (PLAN.md M4.5 on).
    let evaluated = [("Water", ArbiterPart::AlphaR)];
    for a in ARBITERS {
        let is_evaluated = evaluated.contains(&(a.fluid, a.part));
        assert_eq!(!allowed(&a.status), is_evaluated, "{} {:?}: {:?}", a.fluid, a.part, a.status);
    }
    let water = ARBITERS.iter().find(|a| (a.fluid, a.part) == ("Water", ArbiterPart::AlphaR)).unwrap();
    assert_eq!(water.status, ArbiterStatus::SelfConsistent);
    assert!(ARBITERS.len() >= 20, "the core set of VERIFICATION.md §4.4");
}

#[test]
fn every_arbiter_cites_a_doi_or_report() {
    for arbiter in ARBITERS.iter().filter(|a| a.status != ArbiterStatus::Unpublished) {
        let source = arbiter.citation.doi_or_report.unwrap_or_default();
        let cited = source.starts_with("10.") || source.starts_with("IAPWS ") || source.starts_with("NIST IR ");
        assert!(cited, "{} {:?}: `{source}`", arbiter.fluid, arbiter.part);
    }
}

/// PLAN.md M1.12 and VERIFICATION.md §4.4: the 18 states of CoolProp's own EOS tests (map 10 §8.1) are the arbiters
/// of 13 fluids, one record each, traced to its paper.
#[test]
fn coolprop_test_rows_cover_thirteen_fluids() {
    let records = || ARBITERS.iter().filter(|a| a.citation.role == Role::CoolPropTests);
    let mut fluids: Vec<&str> = records().map(|a| a.fluid).collect();
    fluids.sort_unstable();
    assert_eq!(
        fluids,
        [
            "PropyleneGlycol",
            "R1123",
            "R1130(E)",
            "R1132(E)",
            "R1224YDZ",
            "R1233zd(E)",
            "R1234yf",
            "R1243zf",
            "Tetrahydrofuran",
            "VinylChloride",
            "n-Perfluorobutane",
            "n-Perfluorohexane",
            "n-Perfluoropentane",
        ]
    );
    assert_eq!(records().flat_map(|a| a.tables).filter_map(|t| t.rows).sum::<u16>(), 18, "18 states");
}

/// Water under both constant sets of VERIFICATION.md §4.3: the paper's (IAPWS R6-95(2018): R = 0.46151805 kJ/(kg K),
/// T_c = 647.096 K, ρ_c = 322 kg/m³; M is not printed, so the record's converts them) and the v8.0.0 ones, which
/// step 1 finds equal to rounding.
fn iapws95_models() -> [(&'static str, PureFluid, f64); 2] {
    let record = phasekit_core::internal::record(Registry::embedded().unwrap(), "Water").unwrap();
    let m = record.molar_mass;
    let e = &record.eos;
    // Equal within the printed digits: R = 0.46151805 kJ/(kg K) allows ±5e-6 J/(kg K); ρ_c = 322 kg/m³ ±0.5.
    assert!((e.gas_constant / m - 461.51805).abs() <= 5e-6, "R/M = {}", e.gas_constant / m);
    assert!((e.rho_reducing * m - 322.0).abs() <= 0.5 && e.t_reducing == 647.096);
    let mut paper = record.clone();
    (paper.eos.gas_constant, paper.eos.rho_reducing) = (461.51805 * m, 322.0 / m);
    [("paper", paper.compile().unwrap(), m), ("v8.0.0", record.compile().unwrap(), m)]
}

/// A printed value and the value under test agree within half a unit of the last printed digit (class `Paper`).
fn within_printed(table: &Fixture<'_>, row: usize, column: &str, got: f64) -> Result<(), String> {
    let printed = table.printed(row, column).unwrap();
    let Some(Tolerance::Absolute(tol)) = from_printed(printed) else { return Err(format!("{column}: `{printed}`")) };
    let want = table.value(row, column).unwrap();
    if (got - want).abs() <= tol { Ok(()) } else { Err(format!("row {row} {column}: {got} vs printed {printed}")) }
}

/// Arbiter: IAPWS R6-95(2018) Table 6 (Wagner & Pruß 2002, PLAN.md M4.5): φ⁰, φʳ and their first and second
/// derivatives at 500 K and 838.025 kg/m³, each within half a unit of its last printed digit, with the paper's constants
/// and with the v8.0.0 ones (no divergence).
#[test]
fn iapws95_table6_within_printed_digits() {
    let (path, text) = fixture!("paper/Water/IAPWS-R6-95-2018.6.csv");
    let table = Fixture::parse(path, text).unwrap();
    let mut failures = Vec::new();
    for (label, model, m) in iapws95_models() {
        let (t, rho) = (table.value(0, "T").unwrap(), table.value(0, "rho").unwrap() / m);
        let eos = model.eos();
        let (tau, delta) = (647.096 / t, rho * m / 322.0);
        for (prefix, a) in [("phi0", eos.ideal(t, rho, Order::Two)), ("phir", eos.residual(t, rho, Order::Two))] {
            let at =
                |i: usize, j: usize| a.get(i, j).unwrap() / (math::powi(tau, i as i32) * math::powi(delta, j as i32));
            let entries = [("", at(0, 0)), ("_d", at(0, 1)), ("_dd", at(0, 2)), ("_t", at(1, 0)), ("_tt", at(2, 0))];
            for (suffix, got) in entries.into_iter().chain([("_dt", at(1, 1))]) {
                if let Err(e) = within_printed(&table, 0, &format!("{prefix}{suffix}"), got) {
                    failures.push(format!("{label}: {e}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Arbiter: IAPWS R6-95(2018) Table 7 (PLAN.md M4.5): p, c_v, w and s at the 11 single-phase states, from the order-2
/// bundle at (T, ρ) through `State::from_total`, each within half a unit of its last printed digit, with both constant
/// sets. With Table 6 this makes the Water α arbiter `SelfConsistent`.
#[test]
fn iapws95_table7_within_printed_digits() {
    let (path, text) = fixture!("paper/Water/IAPWS-R6-95-2018.7.csv");
    let table = Fixture::parse(path, text).unwrap();
    assert_eq!(table.rows().len(), 11);
    let mut failures = Vec::new();
    for (label, model, m) in iapws95_models() {
        let eos = model.eos();
        for row in 0..table.rows().len() {
            let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap() / m);
            let total = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
            let phase = if t > 647.096 {
                Phase::Supercritical
            } else if rho * m > 322.0 {
                Phase::Liquid
            } else {
                Phase::Gas
            };
            let state = State::from_total(model.info().key(), t, rho, eos.gas_constant(), m, phase, &total).unwrap();
            let values = [
                ("p", state.p() / 1e6),
                ("cv", state.cv(Basis::Mass).unwrap() / 1e3),
                ("w", state.speed_of_sound().unwrap()),
                ("s", state.s(Basis::Mass) / 1e3),
            ];
            for (column, got) in values {
                if let Err(e) = within_printed(&table, row, column, got) {
                    failures.push(format!("{label}: {e}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
