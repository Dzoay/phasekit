//! The `libm` backend's known differences (user decision LB1, issue #170). The oracle computed its fixtures with
//! glibc's transcendentals; the `libm` crate's differ from them in the last bits, and a few outputs amplify that past
//! their class. Each is listed here with the cells it covers. On `libm` a test counts those cells instead of failing
//! and asserts the count, so a new difference still fails and a fixed one shows; on std they are asserted as before.
//! The functions take the backend (`phasekit_core::math::BACKEND`), so both answers are tested on either build.

/// `(test, fluid, column or pair, cells)`: the cells of `test` that the `libm` backend moves, logged in issue #170.
/// Ammonia's η and its initial-density term at 8 states (both fixture tiers) and Methanol's η at one fall outside
/// `transport_direct`, 1.4e-12 relative at most. Water's HP round trip of its liquid at its lowest temperature, which
/// returned `BelowMinTemperature` on the `libm` crate's `exp`, round-trips on core's own (PLAN.md M23.2).
pub const LIBM_KNOWN: [(&str, &str, &str, usize); 3] = [
    ("viscosity_stages_match_contributions", "Ammonia", "eta", 8),
    ("viscosity_stages_match_contributions", "Ammonia", "eta_initial_density", 8),
    ("hardcoded_models_match_oracle", "Methanol", "eta", 1),
];

/// Whether, on `backend`, a mismatch of `test` in `fluid`'s `column` (or pair) is one of [`LIBM_KNOWN`]: never on std's
/// maths.
pub fn known(backend: &str, test: &str, fluid: &str, column: &str) -> bool {
    backend == "libm" && LIBM_KNOWN.iter().any(|&(t, f, c, _)| (t, f, c) == (test, fluid, column))
}

/// A single-state bench's case on `backend` (PLAN.md M23.1; VERIFICATION.md §12): `case` on std's maths, `<case>_libm`
/// on the `libm` crate's, so that one results file holds both builds' rows of D12's rule.
pub fn case(backend: &str, case: &str) -> String {
    if backend == "libm" { format!("{case}_libm") } else { case.to_string() }
}

/// The cells of `test` that [`LIBM_KNOWN`] lists on `backend`: none on std's maths.
pub fn known_cells(backend: &str, test: &str) -> usize {
    let cells = LIBM_KNOWN.iter().filter(|&&(t, ..)| t == test).map(|&(.., cells)| cells).sum();
    if backend == "libm" { cells } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list applies on `libm` only, to the named test, fluid and column, and sums each test's cells.
    #[test]
    fn known_differences_apply_on_libm_only() {
        let test = "viscosity_stages_match_contributions";
        assert!(known("libm", test, "Ammonia", "eta_initial_density"));
        assert!(known("libm", "hardcoded_models_match_oracle", "Methanol", "eta"));
        assert!(!known("std", test, "Ammonia", "eta_initial_density"));
        assert!(!known("libm", test, "Ammonia", "eta_dilute"));
        assert!(!known("libm", test, "Methanol", "eta"));
        assert!(!known("libm", "hardcoded_models_match_oracle", "Ammonia", "eta"));
        assert_eq!((known_cells("libm", test), known_cells("std", test)), (16, 0));
        assert_eq!(known_cells("libm", "hardcoded_models_match_oracle"), 1);
        assert_eq!(known_cells("libm", "conductivity_stages_match_contributions"), 0);
    }

    /// PLAN.md M23.1: a single-state bench's case names the `libm` backend and leaves std's as it was.
    #[test]
    fn bench_cases_name_their_backend() {
        assert_eq!((case("std", "pt_h"), case("libm", "pt_h")), ("pt_h".to_string(), "pt_h_libm".to_string()));
    }
}
