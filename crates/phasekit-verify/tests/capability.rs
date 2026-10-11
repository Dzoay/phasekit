//! VERIFICATION.md §8.5 (PLAN.md M7.9; ROT-007, ROT-009, ROT-081, ROT-086, ROT-129): the 19 × 2 capability matrix.
//! Every pair's inputs are read off truth states built in (p, T) for a single phase and in (T, Q), (p, Q) or (T, ρ) in
//! the dome, never in density bands (map 03 §8), flashed with the default options, and compared with the truth in class
//! `Flash` (`phasekit_verify::capability_matrix`). The expected outcome of every cell is a table: all 19 pairs are
//! declared since M7.8, so a cell either recovers its truth or is refused by the documented error.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Registry};
use phasekit_verify::eos::CORE_FILES;

#[path = "common/capability.rs"]
mod kit;

use kit::{CALLS, mismatches};

/// The name of a core fixture's fluid.
fn name_of(path: &str) -> &str {
    path.rsplit('/').next().and_then(|f| f.strip_suffix(".csv")).unwrap()
}

/// PLAN.md M7.9: the core subset's 14 fluids, each pair × eight truths ([`COLUMNS`]), every cell as its table says.
#[test]
fn capability_matrix_19x2() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut bad, mut compared) = (Vec::new(), 0);
    for (path, _) in CORE_FILES {
        let (fluid_bad, n) = mismatches(&registry, name_of(path));
        bad.extend(fluid_bad);
        compared += n;
    }
    assert!(bad.is_empty(), "{} of {compared} cells:\n{}", bad.len(), bad.join("\n"));
    assert_eq!(compared, 14 * 19 * 2 * CALLS.len());
}
