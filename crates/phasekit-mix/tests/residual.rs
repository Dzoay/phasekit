//! The multi-fluid residual (PLAN.md M13.3) under component swaps, with departure functions only where F ≠ 0 (ROT-195),
//! and its refusals; its printed check tables are `phasekit-verify`'s `tests/mixture.rs`.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Order, Registry};
use phasekit_mix::MultiFluid;

/// A pair's model does not depend on the order its components are named in: swapping them and their fractions gives
/// the same reducing state within 1e-15 and A_ij within 1e-13 (the sums run in the other order), for Lemmon's ξ/ζ form (R32/R125),
/// GERG's β/γ with and without a departure function (Methane/Ethane, Methane/n-Butane) and a Gaussian + Exponential one
/// (Helium/Neon).
#[test]
fn swapping_components_changes_nothing() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    for (a, b) in [("R32", "R125"), ("Methane", "Ethane"), ("Methane", "n-Butane"), ("Helium", "Neon")] {
        let ab = MultiFluid::new(&registry, &[a, b], DataSet::Corrected).unwrap();
        let ba = MultiFluid::new(&registry, &[b, a], DataSet::Corrected).unwrap();
        let (x, x_swapped) = ([0.3, 0.7], [0.7, 0.3]);
        let (t_r, rho_r) = ab.reducing_state(&x);
        let (t_r2, rho_r2) = ba.reducing_state(&x_swapped);
        assert!(((t_r2 - t_r) / t_r).abs() <= 1e-15 && ((rho_r2 - rho_r) / rho_r).abs() <= 1e-15, "{a}&{b}");
        let (t, rho) = (t_r / 0.9, 0.7 * rho_r);
        let (d, d2) =
            (ab.residual(&x, t, rho, Order::Four).unwrap(), ba.residual(&x_swapped, t, rho, Order::Four).unwrap());
        for (i, j) in (0..=4).flat_map(|n| (0..=n).map(move |i| (i, n - i))) {
            let (p, q) = (d.get(i, j).unwrap(), d2.get(i, j).unwrap());
            assert!((p - q).abs() <= 1e-13 * p.abs().max(1.0), "{a}&{b} A{i}{j}: {p} against {q}");
        }
    }
}

/// ROT-195: v8.0.0 evaluates a dummy zero departure for every pair with F = 0; here only the pairs with a departure
/// function carry one. Of the six pairs of methane, nitrogen, CO₂ and ethane, five have one (GERG-2008); of
/// R32, R125 and R134a's three, all three (Lemmon 2004); hexane and heptane's one pair has none.
#[test]
fn only_pairs_with_a_departure_are_evaluated() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let count = |names: &[&str]| MultiFluid::new(&registry, names, DataSet::Corrected).unwrap().departures();
    let four = MultiFluid::new(&registry, &["Methane", "Nitrogen", "CarbonDioxide", "Ethane"], DataSet::Corrected);
    assert_eq!(four.as_ref().map(|m| (m.len(), m.is_empty())).unwrap(), (4, false));
    assert_eq!(count(&["Methane", "Nitrogen", "CarbonDioxide", "Ethane"]), 5);
    assert_eq!(count(&["R32", "R125", "R134a"]), 3);
    assert_eq!(count(&["n-Hexane", "n-Heptane"]), 0);
}

/// Components at x = 0 contribute nothing: a model of four at (0.5, 0.5, 0, 0) is the model of the first two, its
/// reducing state and A_ij within 1e-15 and 1e-13 (two absent components' pair term is 0, not 0/0), for the library's
/// model and GERG-2008's.
#[test]
fn absent_components_contribute_nothing() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let names = ["Methane", "Ethane", "Nitrogen", "CarbonDioxide"];
    let models = [
        (
            MultiFluid::new(&registry, &names, DataSet::Corrected),
            MultiFluid::new(&registry, &names[..2], DataSet::Corrected),
        ),
        (MultiFluid::gerg2008(&registry, &names), MultiFluid::gerg2008(&registry, &names[..2])),
    ];
    for (four, two) in models.map(|(a, b)| (a.unwrap(), b.unwrap())) {
        let (x4, x2) = ([0.5, 0.5, 0.0, 0.0], [0.5, 0.5]);
        let ((t4, r4), (t2, r2)) = (four.reducing_state(&x4), two.reducing_state(&x2));
        assert!(((t4 - t2) / t2).abs() <= 1e-15 && ((r4 - r2) / r2).abs() <= 1e-15, "{t4} {r4} against {t2} {r2}");
        let (d4, d2) =
            (four.residual(&x4, 250.0, 5e3, Order::Two).unwrap(), two.residual(&x2, 250.0, 5e3, Order::Two).unwrap());
        for (i, j) in [(0, 0), (1, 0), (0, 1), (2, 0), (1, 1), (0, 2)] {
            let (p, q) = (d4.get(i, j).unwrap(), d2.get(i, j).unwrap());
            assert!((p - q).abs() <= 1e-13 * q.abs().max(1.0), "A{i}{j}: {p} against {q}");
        }
    }
}

/// A pair the library does not have, a component outside it, one component or the same one twice are refused.
#[test]
fn missing_pairs_and_components_are_refused() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let refused = |names: &[&str]| MultiFluid::new(&registry, names, DataSet::Corrected).unwrap_err().to_string();
    assert!(refused(&["R124", "R22"]).contains("binary pair"), "{}", refused(&["R124", "R22"]));
    assert!(refused(&["Water", "nonsense"]).contains("nonsense"));
    assert!(refused(&["Water"]).contains("components"));
    assert!(refused(&["Water", "H2O"]).contains("components"));
}
