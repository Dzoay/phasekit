//! The `libm` backend's known differences (user decision LB1, issue #170). The oracle computed its fixtures with
//! glibc's transcendentals; the `libm` crate's differ from them in the last bits, and a few outputs amplify that past
//! their class. Each is listed here with the cells it covers. On `libm` a test counts those cells instead of failing
//! and asserts the count, so a new difference still fails and a fixed one shows; on std they are asserted as before.
//! The functions take the backend (`phasekit_core::math::BACKEND`), so both answers are tested on either build.

/// A known difference: `(test, fluid, column or pair, cells)`, the cells of `test` that a backend moves.
pub type Known = (&'static str, &'static str, &'static str, usize);

/// The `libm` backend's known differences, logged in issue #170: none since PLAN.md M23.4. M9.3's list held Ammonia's
/// η and its initial-density term at 8 states (both fixture tiers) and Methanol's η at one, outside
/// `transport_direct` by 1.4e-12 relative at most, which the `libm` crate's `pow` moved and core's own does not
/// (M23.4), and Water's HP round trip of its liquid at its lowest temperature, which returned `BelowMinTemperature` on
/// the `libm` crate's `exp` and round-trips on core's own (M23.2).
pub const LIBM_KNOWN: [Known; 0] = [];

/// Whether, on `backend`, a mismatch of `test` in `fluid`'s `column` (or pair) is one of [`LIBM_KNOWN`]: never on std's
/// maths.
pub fn known(backend: &str, test: &str, fluid: &str, column: &str) -> bool {
    // Equivalent (PLAN.md M23.4): `LIBM_KNOWN` is empty, so no backend has a known difference.
    // equivalent mutant: replace known -> bool with false
    // equivalent mutant: replace == with !=
    backend == "libm" && listed(&LIBM_KNOWN, test, fluid, column)
}

/// Whether `list` names `test`, `fluid` and `column`.
fn listed(list: &[Known], test: &str, fluid: &str, column: &str) -> bool {
    list.iter().any(|&(t, f, c, _)| (t, f, c) == (test, fluid, column))
}

/// A single-state bench's case on `backend` (PLAN.md M23.1, M23.7; VERIFICATION.md §12): `case` on std's maths,
/// `<case>_<backend>` on another (`_libm`, `_fast`), so that one results file holds every build's rows.
pub fn case(backend: &str, case: &str) -> String {
    if backend == "std" { case.to_string() } else { format!("{case}_{backend}") }
}

/// The cells of `test` that [`LIBM_KNOWN`] lists on `backend`: none on std's maths.
pub fn known_cells(backend: &str, test: &str) -> usize {
    // Equivalent (PLAN.md M23.4): `LIBM_KNOWN` is empty, so every backend's count is 0.
    // equivalent mutant: replace known_cells -> usize with 0
    // equivalent mutant: replace == with !=
    if backend == "libm" { cells(&LIBM_KNOWN, test) } else { 0 }
}

/// The cells of `test` that `list` names, summed.
fn cells(list: &[Known], test: &str) -> usize {
    list.iter().filter(|&&(t, ..)| t == test).map(|&(.., cells)| cells).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M9.3's list, before PLAN.md M23.4 emptied it.
    const M9_3: [Known; 3] = [
        ("viscosity_stages_match_contributions", "Ammonia", "eta", 8),
        ("viscosity_stages_match_contributions", "Ammonia", "eta_initial_density", 8),
        ("hardcoded_models_match_oracle", "Methanol", "eta", 1),
    ];

    /// A list names the test, fluid and column it lists, and sums each test's cells: M9.3's.
    #[test]
    fn a_list_names_its_cells() {
        let test = "viscosity_stages_match_contributions";
        assert!(listed(&M9_3, test, "Ammonia", "eta_initial_density"));
        assert!(listed(&M9_3, "hardcoded_models_match_oracle", "Methanol", "eta"));
        assert!(!listed(&M9_3, test, "Ammonia", "eta_dilute"));
        assert!(!listed(&M9_3, test, "Methanol", "eta"));
        assert!(!listed(&M9_3, "hardcoded_models_match_oracle", "Ammonia", "eta"));
        assert_eq!((cells(&M9_3, test), cells(&M9_3, "hardcoded_models_match_oracle")), (16, 1));
        assert_eq!(cells(&M9_3, "conductivity_stages_match_contributions"), 0);
    }

    /// PLAN.md M23.4: no difference is known on either backend, M9.3's cells included.
    #[test]
    fn no_difference_is_known() {
        for backend in ["libm", "std"] {
            assert!(!known(backend, "viscosity_stages_match_contributions", "Ammonia", "eta"));
            assert_eq!(known_cells(backend, "viscosity_stages_match_contributions"), 0);
        }
    }

    /// PLAN.md M23.1: a single-state bench's case names the `libm` backend and leaves std's as it was.
    #[test]
    fn bench_cases_name_their_backend() {
        assert_eq!((case("std", "pt_h"), case("libm", "pt_h")), ("pt_h".to_string(), "pt_h_libm".to_string()));
        assert_eq!(case("fast", "pt_h"), "pt_h_fast");
    }
}
