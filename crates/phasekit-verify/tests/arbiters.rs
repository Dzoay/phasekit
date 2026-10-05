//! The literature arbiters (VERIFICATION.md §4.3): each record's status is asserted against the committed files, and
//! from M4 on against the evaluation of its table with the paper's own constants.

use phasekit_verify::arbiters::{Role, violations};
use phasekit_verify::{ARBITERS, ArbiterStatus, MILESTONE};

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
    assert!(ARBITERS.iter().all(|a| allowed(&a.status)), "at M1 nothing is evaluated");
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
