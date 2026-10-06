//! `phasekit-verify`: the verification kit (D13), reusable by any model crate, plus the conformance corpus in
//! `tests/`. Zero dependencies beyond `phasekit-core`. The full design is docs/VERIFICATION.md (forthcoming).

pub mod arbiters;
pub mod conformance;
pub mod fixture;
pub mod lock;
pub mod majorant;
pub mod register;
pub mod sample;
pub mod sha256;
pub mod term;
pub mod tolerance;

pub use arbiters::{ARBITERS, Arbiter, ArbiterPart, ArbiterStatus};
pub use conformance::{Mismatch, fd_first_order, gauge_invariance, policy_equivalence};
pub use fixture::{Cell, CheckError, ColumnRole, Fixture, FixtureError, FixtureMismatch, Kind, Row};
pub use lock::{ORACLE_LOCK, OracleLock};
pub use register::{
    DIVERGENCES, DivStatus, Divergence, Exempt, Fix, Part, Policy, RegisterError, Rows, check_register, missing_proofs,
    unregistered_proofs,
};
pub use sample::SplitMix64;
pub use sha256::{sha256, sha256_hex};
pub use tolerance::{Provenance, Tolerance, ToleranceClass, from_printed};

/// The first milestone that is not closed (PLAN.md §0.2). The step that closes milestone n sets it to n + 1, which
/// arms the fail-closed checks for everything due by n: `cargo xtask gates rot` (VERIFICATION.md §11.2) and, from M1.6,
/// `tests/divergences.rs` (VERIFICATION.md §6.3).
pub const MILESTONE: u8 = 3;

/// `(path, text)` of a committed fixture, `path` relative to `crates/phasekit-verify/fixtures/` (VERIFICATION.md §3.4).
/// The text is compiled in, so the corpus runs on wasip2; `cargo xtask gates fixtures` checks that every path exists
/// and that every committed fixture is read by at least one test besides the manifest's own list.
#[macro_export]
macro_rules! fixture {
    ($path:literal) => {
        ($path, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/", $path)))
    };
}
