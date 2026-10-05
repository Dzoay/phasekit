//! The divergence register is consistent with the corrections shipped in the data. The real test decodes
//! every embedded record (`Registry::record`) and collects its patches; the seed rows of
//! `data/corrections.csv` stand in for them here.

use phasekit_core::internal::{Edit, Patch};
use phasekit_verify::{DIVERGENCES, Policy, Provenance, RegisterError, check_register};

fn patch(id: &str, edit: Edit) -> Patch {
    Patch { divergence: id.into(), edit }
}

#[test]
fn corrections_cite_use_paper_entries() {
    let seed = [
        patch("DIV-0001", Edit::GasConstant(8.314_462_1)),
        patch("DIV-0002", Edit::MeltingP0 { segment: 2, p0: 632.4e6 }),
        patch("DIV-0003", Edit::ReducingDensity(11_183.9)),
    ];
    assert_eq!(check_register(DIVERGENCES, &seed), Ok(()));
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
    assert_eq!((helium.fluid, helium.policy), ("Helium", Policy::KeepOracle));
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
