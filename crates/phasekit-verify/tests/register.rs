//! The divergence register is consistent with the corrections shipped in the data (VERIFICATION.md §7.2). From M2.7
//! the test decodes every embedded record and collects its patches; `seed_corrections()`, the rows
//! `data/corrections.csv` will ship, stand in for them until then.

use phasekit_core::internal::{Edit, Patch};
use phasekit_verify::{DIVERGENCES, Fix, Policy, Provenance, RegisterError, check_register, seed_corrections};

fn patch(id: &str, edit: Edit) -> Patch {
    Patch { divergence: id.into(), edit }
}

#[test]
fn corrections_cite_use_paper_entries() {
    assert_eq!(check_register(DIVERGENCES, &seed_corrections()), Ok(()));
    let skip = [patch("DIV-0004", Edit::GasConstant(8.314))];
    assert_eq!(check_register(DIVERGENCES, &skip), Err(RegisterError::NotUsePaper("DIV-0004".into())));
    let unknown = [patch("DIV-9999", Edit::GasConstant(8.314))];
    assert_eq!(check_register(DIVERGENCES, &unknown), Err(RegisterError::UnknownId("DIV-9999".into())));
}

/// DIV-0005 (Helium) is an accepted divergence: CoolProp's R is kept, so no correction may cite it and
/// Corrected equals Parity for helium (docs/design/04-user-decisions.md, question 5).
#[test]
fn accepted_divergences_ship_no_patch() {
    let helium = DIVERGENCES.iter().find(|d| d.id == "DIV-0005").unwrap();
    assert_eq!((helium.fluids, helium.policy, helium.fix), (&["Helium"][..], Policy::KeepOracle, Fix::None));
    let table_1_r = [patch("DIV-0005", Edit::GasConstant(8.314_472))];
    assert_eq!(check_register(DIVERGENCES, &table_1_r), Err(RegisterError::NotUsePaper("DIV-0005".into())));
}

#[test]
fn use_paper_entries_name_an_arbiter() {
    for d in DIVERGENCES.iter().filter(|d| d.policy == Policy::UsePaper) {
        assert!(d.arbiter.is_some(), "{}", d.id);
    }
    assert!(Provenance::Iapws { release: "R14-08" }.is_arbiter());
    assert!(!Provenance::Oracle { version: "8.0.0" }.is_arbiter());
    assert!(!Provenance::SelfReferential.is_arbiter());
}

/// VERIFICATION.md §7.2: every `UsePaper` entry with `fix: Data` is implemented by at least one patch, and no patch
/// cites an entry whose fix is not data.
#[test]
fn use_paper_data_fixes_are_cited_by_a_patch() {
    let without_0002: Vec<Patch> = seed_corrections().into_iter().filter(|p| &*p.divergence != "DIV-0002").collect();
    assert_eq!(check_register(DIVERGENCES, &without_0002), Err(RegisterError::UncitedDataFix("DIV-0002")));
    let mut code = DIVERGENCES.to_vec();
    if let Some(entry) = code.iter_mut().find(|d| d.id == "DIV-0003") {
        entry.fix = Fix::Code("phasekit_core::data");
    }
    assert_eq!(check_register(&code, &seed_corrections()), Err(RegisterError::NotDataFix("DIV-0003".into())));
    // A UsePaper entry fixed in code (the M11 cubic gas constant) needs no patch.
    let without_0003: Vec<Patch> = seed_corrections().into_iter().filter(|p| &*p.divergence != "DIV-0003").collect();
    assert_eq!(check_register(&code, &without_0003), Ok(()));
}

/// Every entry's evidence cites a map item ("map NN"), so each divergence traces to its measurement.
#[test]
fn every_entry_cites_a_map_id() {
    let mut uncited = DIVERGENCES.to_vec();
    if let Some(entry) = uncited.iter_mut().find(|d| d.id == "DIV-0007") {
        entry.evidence = "measured once, somewhere";
    }
    assert_eq!(check_register(&uncited, &seed_corrections()), Err(RegisterError::NoMapCitation("DIV-0007")));
    assert!(DIVERGENCES.iter().all(|d| d.proof.iter().all(|m| (2..=19).contains(m))), "proof milestones are M2-M19");
}
