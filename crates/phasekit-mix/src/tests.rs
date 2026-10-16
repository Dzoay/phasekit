//! The generated library's invariants (PLAN.md M13.2): what the model will rely on when it looks a pair up.

use crate::data::{Kind, Reducing};
use crate::generated::{CAS, DEPARTURES, PAIRS, PREDEFINED};

/// v8.0.0's 888 pairs, sorted by their CAS pair (each sorted as strings) with none twice, so a binary search finds
/// one; a pair carries a departure function exactly where F ≠ 0, and it exists; six pairs are Lemmon's ξ/ζ.
#[test]
fn pairs_are_keyed_by_their_sorted_cas_pair() {
    assert_eq!(PAIRS.len(), 888);
    assert!(PAIRS.iter().all(|p| p.cas[0] < p.cas[1]));
    assert!(PAIRS.windows(2).all(|w| w[0].cas < w[1].cas), "sorted, none twice");
    for p in &PAIRS {
        assert_eq!(p.departure.is_some(), p.f != 0.0, "{:?}", p.names);
        assert!(p.departure.is_none_or(|i| i < DEPARTURES.len()), "{:?}", p.names);
    }
    assert_eq!(PAIRS.iter().filter(|p| p.departure.is_some()).count(), 40);
    assert_eq!(PAIRS.iter().filter(|p| matches!(p.reducing, Reducing::Lemmon(_))).count(), 6);
}

/// The 28 departure functions by kind: 16 Exponential, 9 GERG-2008 and 3 Gaussian + Exponential, each `power` within
/// its terms (every term for Exponential).
#[test]
fn departure_functions_come_in_three_kinds() {
    let count = |kind| DEPARTURES.iter().filter(|d| d.kind == kind).count();
    assert_eq!([Kind::Exponential, Kind::Gerg2008, Kind::GaussianExponential].map(count), [16, 9, 3]);
    for d in &DEPARTURES {
        assert!(d.power <= d.terms.len() && (d.kind != Kind::Exponential || d.power == d.terms.len()), "{}", d.name);
    }
}

/// The 147 predefined mixtures by name, each one's mole fractions summing to 1, and ASHRAE 34's mass fractions, where
/// present, for the same components; every library fluid's CAS number by name.
#[test]
fn predefined_mixtures_and_cas_numbers_are_by_name() {
    assert_eq!(PREDEFINED.len(), 147);
    assert!(PREDEFINED.windows(2).all(|w| w[0].name < w[1].name));
    for m in &PREDEFINED {
        let sum: f64 = m.mole_fractions.iter().sum();
        assert!(m.components.len() == m.mole_fractions.len() && (sum - 1.0).abs() < 1e-10, "{}", m.name);
        assert!(m.mass_fractions.is_none_or(|w| w.len() == m.components.len()), "{}", m.name);
    }
    assert_eq!(CAS.len(), 136);
    assert!(CAS.windows(2).all(|w| w[0].0 < w[1].0));
}
