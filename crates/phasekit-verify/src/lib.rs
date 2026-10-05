//! `phasekit-verify`: the verification kit (D13), reusable by any model crate, plus the conformance corpus in
//! `tests/`. Zero dependencies beyond `phasekit-core`. The full design is docs/VERIFICATION.md (forthcoming).

pub mod conformance;
pub mod fixture;
pub mod register;
pub mod sample;
pub mod tolerance;

pub use conformance::{Mismatch, fd_first_order, gauge_invariance, policy_equivalence};
pub use fixture::{Cell, CheckError, ColumnRole, Fixture, FixtureError, FixtureMismatch, Row};
pub use register::{DIVERGENCES, Divergence, Policy, RegisterError, check_register};
pub use sample::SplitMix64;
pub use tolerance::{Provenance, Tolerance, ToleranceClass, from_printed};

/// The first milestone that is not closed (PLAN.md §0.2). The step that closes milestone n sets it to n + 1, which
/// arms the fail-closed checks for everything due by n: `cargo xtask gates rot` (VERIFICATION.md §11.2) and, from M1.6,
/// `tests/divergences.rs` (VERIFICATION.md §6.3).
pub const MILESTONE: u8 = 1;
