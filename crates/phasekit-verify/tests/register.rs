//! The divergence register is consistent with the corrections shipped in the data (VERIFICATION.md §7.2): the tests
//! decode every embedded record and collect its patches (PLAN.md M2.7).

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::{Edit, FluidRecord, Patch};
use phasekit_core::{DataSet, Registry};
use phasekit_verify::{
    Cell, DIVERGENCES, Fix, Fixture, Policy, Provenance, RegisterError, Tolerance, check_register, fixture,
    from_printed,
};

/// Every embedded fluid's record, as decoded (uncorrected).
fn records() -> Vec<FluidRecord> {
    let registry = Registry::embedded().unwrap();
    phasekit_data::FLUIDS.iter().map(|f| phasekit_core::internal::record(registry, f.name).unwrap()).collect()
}

/// The patches the embedded data ships.
fn seed_corrections() -> Vec<Patch> {
    records().into_iter().flat_map(|r| r.corrections).collect()
}

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

/// The `facts` row named `name`: its status and value (NaN for a failed call).
fn fact<'a>(facts: &Fixture<'a>, name: &str) -> Option<(&'a str, f64)> {
    let row = facts.rows().iter().position(|row| row.cells.first() == Some(&Cell::Text(name)))?;
    match facts.rows()[row].cells.get(2)? {
        Cell::Text(status) => Some((status, facts.value(row, "value")?)),
        Cell::Num(_) | Cell::Blank => None,
    }
}

/// `value` is within half a unit in the last digit of `figure`, the value a map section or the register cites.
fn cites(value: f64, figure: &str) -> bool {
    match (from_printed(figure), figure.parse::<f64>()) {
        (Some(Tolerance::Absolute(half)), Ok(want)) => (value - want).abs() <= half,
        _ => false,
    }
}

/// The relative differences of the oracle from a printed table's column at the given rows, as (min, max).
fn residuals(table: &Fixture, column: &str, scale: f64, oracle: impl Fn(usize) -> f64, rows: &[usize]) -> (f64, f64) {
    let rel = |row: usize| table.value(row, column).map(|printed| (oracle(row) / scale - printed) / printed);
    let rel: Vec<f64> = rows.iter().map(|&row| rel(row).unwrap_or(f64::NAN)).collect();
    (rel.iter().copied().fold(f64::INFINITY, f64::min), rel.iter().copied().fold(f64::NEG_INFINITY, f64::max))
}

/// PLAN.md M1.13: every seed entry's cited CoolProp 8.0.0 behaviour reproduces from `facts/register.csv`, which the
/// oracle generates (map 12 §6.3, map 10 §8.4, map 13 §3); a fact that stops reproducing is fixed in the register.
#[test]
fn register_cites_reproducible_oracle_facts() {
    let (path, text) = fixture!("coolprop-8.0.0/facts/register.csv");
    let facts = Fixture::parse(path, text).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((facts.kind(), facts.provenance()), ("facts", Provenance::Oracle { version: "8.0.0" }));
    let ok = |name: &str| match fact(&facts, name) {
        Some(("ok", value)) => value,
        other => panic!("fact `{name}`: {other:?}"),
    };
    let paper =
        |(path, text): (&'static str, &'static str)| Fixture::parse(path, text).unwrap_or_else(|e| panic!("{e}"));
    let mut checked = Vec::new();

    // DIV-0001: the stored R is 8.314472, and the oracle's p is +1.0e-6..+1.4e-6 above Thol's Table 3 at the five
    // states with ρ > 0 (map 13 R1).
    assert_eq!(ok("div0001_gas_constant"), 8.314_472);
    let thol = paper(fixture!("paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.csv"));
    let (lo, hi) = residuals(&thol, "p", 1e6, |row| ok(&format!("div0001_p_row{row}")), &[0, 1, 2, 4, 5]);
    assert!(cites(lo, "1.0e-6") && cites(hi, "1.4e-6"), "DIV-0001: {lo:e}..{hi:e}");
    checked.push("DIV-0001");

    // DIV-0002: ice VI with p0 = 623.4 MPa: T(1356.76 MPa) = 320.965 K, p(320 K) = 1337.45 MPa (map 10 R10).
    assert!(cites(ok("div0002_melting_t_1356.76mpa"), "320.965"));
    assert!(cites(ok("div0002_melting_p_320k") / 1e6, "1337.45"));
    checked.push("DIV-0002");

    // DIV-0003: Nitrogen's reducing density (map 12 §6.3).
    assert_eq!(ok("div0003_rhomolar_reducing"), 11183.901464580624);
    checked.push("DIV-0003");

    // DIV-0004: a two-phase viscosity, returned without error (map 12 §6.3).
    assert!(cites(ok("div0004_eta_t500_q0.5"), "1.6048e-5"));
    checked.push("DIV-0004");

    // DIV-0005: the stored R is 8.3144598, and the oracle's p, c_v and w are −4.8e-7..−0.9e-7 off NIST IR 8474
    // Table 3 at all six states (map 13 R3).
    assert_eq!(ok("div0005_gas_constant"), 8.314_459_8);
    let helium = paper(fixture!("paper/Helium/OrtizVega-JPCRD-2019.3.csv"));
    let rows = [0, 1, 2, 3, 4, 5];
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (column, scale) in [("p", 1e6), ("cv", 1.0), ("w", 1.0)] {
        let (l, h) = residuals(&helium, column, scale, |row| ok(&format!("div0005_{column}_row{row}")), &rows);
        (lo, hi) = (lo.min(l), hi.max(h));
    }
    assert!(cites(lo, "-4.8e-7") && cites(hi, "-0.9e-7"), "DIV-0005: {lo:e}..{hi:e}");
    checked.push("DIV-0005");

    // DIV-0006..0008: the v8.0.0 reducing densities (and molar masses) corrected upstream in 2acbbc82.
    assert_eq!((ok("div0006_rhomolar_reducing"), ok("div0006_molar_mass")), (7636.76598074554, 0.02805376));
    assert_eq!((ok("div0007_rhomolar_reducing"), ok("div0007_molar_mass")), (15444.54031369981, 0.00201594));
    assert_eq!(ok("div0008_rhomolar_reducing"), 1514.916863638556);
    checked.extend(["DIV-0006", "DIV-0007", "DIV-0008"]);

    // DIV-0009: R1233zd(E) has no viscosity model in v8.0.0.
    assert_eq!(fact(&facts, "div0009_eta_t300_p101325").map(|(status, _)| status), Some("err:notimpl"));
    checked.push("DIV-0009");

    // DIV-0010: PR propane at 400 K, 1 bar: T (∂s/∂T)_p = 91.35 against c_p = 93.89 J/(mol K).
    let ds = ok("div0010_smolar_t400.01") - ok("div0010_smolar_t399.99");
    assert!(cites(400.0 * ds / (400.01 - 399.99), "91.35") && cites(ok("div0010_cpmolar_t400"), "93.89"));
    checked.push("DIV-0010");

    // DIV-0011: C from δ = 1e-12 against ∂²αʳ/∂δ² extrapolated to ρ = 0 through 1..4 mol/m³ (cubic Lagrange
    // weights 4, −6, 4, −1): −6.7e-5, −7.1e-5, +1.9e-5 for Propane 300 K, N₂ 300 K, Water 600 K.
    for (fluid, figure) in [("propane", "-6.7e-5"), ("nitrogen", "-7.1e-5"), ("water", "1.9e-5")] {
        let d2 = |rho: u8| ok(&format!("div0011_d2alphar_ddelta2_{fluid}_rho{rho}"));
        let at_zero = 4.0 * d2(1) - 6.0 * d2(2) + 4.0 * d2(3) - d2(4);
        let rho_r = ok(&format!("div0011_rhomolar_reducing_{fluid}"));
        let exact = at_zero / (rho_r * rho_r);
        let error = (ok(&format!("div0011_cvirial_{fluid}")) - exact) / exact;
        assert!(cites(error, figure), "DIV-0011 {fluid}: {error:e}");
    }
    checked.push("DIV-0011");

    // DIV-0012: Water below Tmin, (55018.5 mol/m³, 250 K), gives a pressure without error.
    assert_eq!(fact(&facts, "div0012_p_t250_rho55018.5"), Some(("ok", -5.9277123935677105)));
    checked.push("DIV-0012");

    // DIV-0013: R1234yf + R1234ze(E), z₁ = 0.4, 469 K, 3399 mol/m³: αʳ = −0.464679 against Bell's −0.460595.
    assert!(cites(ok("div0013_alphar_t469_rho3399"), "-0.464679"));
    checked.push("DIV-0013");

    // DIV-0014: R1224YDZ p(400 K, 8000 mol/m³) = 21.1790735 MPa, 3.3 half units below the printed 21.17909.
    let p = ok("div0014_p_t400_rho8000");
    let printed = paper(fixture!("paper/R1224YDZ/Akasaka-IJT-2023-R1224ydZ.7.csv"));
    let half_units = (printed.value(0, "p").unwrap_or(f64::NAN) - p) / 5.0;
    assert!(cites(p / 1e6, "21.1790735") && cites(half_units, "3.3"), "DIV-0014: {half_units}");
    checked.push("DIV-0014");

    let ids: Vec<&str> = DIVERGENCES.iter().map(|d| d.id).collect();
    assert_eq!(checked, ids, "every register entry has its facts checked");
    for row in facts.rows() {
        let Some(Cell::Text(name)) = row.cells.first() else { panic!("line {}: no name", row.line) };
        let id = name.get(3..7).map(|n| format!("DIV-{n}"));
        assert!(name.starts_with("div") && id.is_some_and(|id| ids.contains(&id.as_str())), "fact `{name}`");
    }
}

/// PLAN.md M2.7: the embedded data ships exactly the three `UsePaper` data corrections, and the register accepts them.
#[test]
fn check_register_accepts_the_shipped_patches() {
    let shipped = seed_corrections();
    let ids: Vec<&str> = shipped.iter().map(|p| &*p.divergence).collect();
    assert_eq!(ids, ["DIV-0003", "DIV-0001", "DIV-0002"]); // Nitrogen, R1234ze(E), Water: index order
    assert_eq!(check_register(DIVERGENCES, &shipped), Ok(()));
    assert_eq!(shipped[2].edit, Edit::MeltingP0 { segment: 3, p0: 632.4e6 });
}

/// VERIFICATION.md §7.2 on real data: for all 136 fluids, Corrected differs from Parity by exactly the shipped edits
/// (undoing them gives Parity back), `applied` lists them, and the model key changes if and only if a patch applies.
#[test]
fn parity_and_corrected_differ_by_exactly_the_patches() {
    let mut changed = Vec::new();
    for record in records() {
        let mut parity = record.clone();
        parity.apply(DataSet::Parity).unwrap();
        let mut corrected = record.clone();
        corrected.apply(DataSet::Corrected).unwrap();
        assert_eq!(parity, record, "{}: Parity is the v8.0.0 data", record.name);
        let ids: Vec<Box<str>> = record.corrections.iter().map(|p| p.divergence.clone()).collect();
        assert_eq!(corrected.applied, ids, "{}", record.name);
        assert_eq!(parity.model_key() == corrected.model_key(), ids.is_empty(), "{}", record.name);
        let mut undone = corrected.clone();
        for patch in &record.corrections {
            match patch.edit {
                Edit::GasConstant(_) => undone.eos.gas_constant = parity.eos.gas_constant,
                Edit::ReducingDensity(_) => undone.eos.rho_reducing = parity.eos.rho_reducing,
                Edit::MolarMass(_) => undone.molar_mass = parity.molar_mass,
                Edit::MeltingP0 { segment, .. } => {
                    let s = usize::from(segment);
                    undone.melting[s].p0 = parity.melting[s].p0;
                }
                _ => panic!("an edit kind this test does not know"),
            }
        }
        undone.applied.clear();
        assert_eq!(undone, parity, "{}", record.name);
        if !ids.is_empty() {
            changed.push(record.name.clone());
        }
    }
    assert_eq!(changed, ["Nitrogen", "R1234ze(E)", "Water"]);
}

/// DIV-0005 (`KeepOracle`, user decision 5): Helium ships no patch, Corrected is Parity, and no patch may cite the entry.
#[test]
fn helium_ships_no_patch() {
    let helium = records().into_iter().find(|r| r.name == "Helium").unwrap();
    assert!(helium.corrections.is_empty());
    let (mut parity, mut corrected) = (helium.clone(), helium);
    parity.apply(DataSet::Parity).unwrap();
    corrected.apply(DataSet::Corrected).unwrap();
    assert_eq!(parity, corrected);
    let citing = [patch("DIV-0005", Edit::GasConstant(8.314_472))];
    assert_eq!(check_register(DIVERGENCES, &citing), Err(RegisterError::NotUsePaper("DIV-0005".into())));
}
