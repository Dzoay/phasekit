//! The divergence proofs (VERIFICATION.md §6.3): one `div_NNNN` per register entry whose proof is due, dispatched
//! from `DIVERGENCES`. A proof is added in the PR of the first milestone its entry lists; `MILESTONE` (the first open
//! milestone) decides what is due, so closing a milestone fails here until its proofs exist.

use phasekit_verify::{DIVERGENCES, MILESTONE, missing_proofs, unregistered_proofs};

/// Every proof function, by register id. None is due yet: the seeds' first proof milestones are M2 and later.
const PROOFS: &[(&str, fn())] = &[];

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
