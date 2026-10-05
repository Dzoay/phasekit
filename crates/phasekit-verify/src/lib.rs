//! `phasekit-verify`: the verification kit (D13), reusable by any model crate, plus the conformance corpus in
//! `tests/`. Zero dependencies beyond `phasekit-core`. The full design is docs/VERIFICATION.md (forthcoming).

pub mod conformance;
pub mod register;
pub mod tolerance;

pub use conformance::{Mismatch, fd_first_order, gauge_invariance, policy_equivalence};
pub use register::{DIVERGENCES, Divergence, Policy, RegisterError, check_register};
pub use tolerance::{Provenance, Tolerance, ToleranceClass, from_printed};
