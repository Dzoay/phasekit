//! The divergence proofs (VERIFICATION.md §6.3): one `div_NNNN` per register entry whose proof is due, dispatched
//! from `DIVERGENCES`. A proof is added in the PR of the first milestone its entry lists; `MILESTONE` (the first open
//! milestone) decides what is due, so closing a milestone fails here until its proofs exist.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::FluidRecord;
use phasekit_core::{DataSet, Registry};
use phasekit_verify::{Cell, DIVERGENCES, Fixture, MILESTONE, fixture, missing_proofs, unregistered_proofs};

/// Every proof function, by register id.
const PROOFS: &[(&str, fn())] =
    &[("DIV-0003", div_0003), ("DIV-0006", div_0006), ("DIV-0007", div_0007), ("DIV-0008", div_0008)];

/// The value of a `facts/register.csv` row (the oracle side of every entry, M1.13).
fn fact(name: &str) -> f64 {
    let (path, text) = fixture!("coolprop-8.0.0/facts/register.csv");
    let facts = Fixture::parse(path, text).unwrap();
    let row = facts.rows().iter().position(|r| r.cells.first() == Some(&Cell::Text(name))).unwrap();
    facts.value(row, "value").unwrap()
}

/// The embedded record of `fluid` under `set`.
fn record(fluid: &str, set: DataSet) -> FluidRecord {
    let mut record = phasekit_core::internal::record(Registry::embedded().unwrap(), fluid).unwrap();
    record.apply(set).unwrap();
    record
}

/// DIV-0003, part M2 (`UsePaper`): Corrected's ρ_r is Span et al. 2000's 11183.9 mol/m³; the oracle's
/// 11183.901464580624 differs from it, and Parity is the oracle bit for bit. (Part M6: the rescaled superancillary.)
fn div_0003() {
    let oracle = fact("div0003_rhomolar_reducing");
    let (parity, corrected) = (record("Nitrogen", DataSet::Parity), record("Nitrogen", DataSet::Corrected));
    assert_eq!(corrected.eos.rho_reducing, 11_183.9);
    assert_ne!(corrected.eos.rho_reducing, oracle);
    assert_eq!(parity.eos.rho_reducing.to_bits(), oracle.to_bits());
    assert_eq!(corrected.applied, [Box::<str>::from("DIV-0003")]);
}

/// The `Investigate` pins of DIV-0006..0008 (map 12 §6.3, upstream 2acbbc82): until a paper is transcribed, Parity's
/// ρ_r (and M) equal the oracle's bit for bit and Corrected changes nothing.
fn pinned(fluid: &str, id: &str, molar_mass: bool) {
    let (parity, corrected) = (record(fluid, DataSet::Parity), record(fluid, DataSet::Corrected));
    assert_eq!(parity.eos.rho_reducing.to_bits(), fact(&format!("{id}_rhomolar_reducing")).to_bits(), "{fluid}");
    if molar_mass {
        assert_eq!(parity.molar_mass.to_bits(), fact(&format!("{id}_molar_mass")).to_bits(), "{fluid}");
    }
    assert_eq!(corrected, parity, "{fluid}");
}

fn div_0006() {
    pinned("Ethylene", "div0006", true);
}

fn div_0007() {
    pinned("OrthoHydrogen", "div0007", true);
}

fn div_0008() {
    pinned("n-Undecane", "div0008", false);
}

fn ids() -> Vec<&'static str> {
    PROOFS.iter().map(|(id, _)| *id).collect()
}

#[test]
fn every_due_proof_exists() {
    assert_eq!(missing_proofs(DIVERGENCES, MILESTONE, &ids()), Vec::<&str>::new());
    for (_, proof) in PROOFS {
        proof();
    }
}

#[test]
fn every_proof_names_a_registered_id() {
    assert_eq!(unregistered_proofs(DIVERGENCES, &ids()), Vec::<&str>::new());
}
