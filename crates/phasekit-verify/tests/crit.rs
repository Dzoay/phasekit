//! Critical points (PLAN.md M6.7; map 01 §8, map 02 §6, map 03 §6; ROT-084): the published point and the model's own,
//! exposed apart, the model's satisfying the criticality conditions and matching the oracle's computed columns.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{CriticalOrigin, DataSet, Order, Registry};
use phasekit_verify::fixture::Kind;
use phasekit_verify::register::exempt_at;
use phasekit_verify::{DIVERGENCES, Fixture, ToleranceClass, fixture};

/// The oracle's crit rows, one per fluid: CoolProp 8.0.0's published point (superancillaries off) and its computed one
/// (on), fixtures/coolprop-8.0.0/all/crit.csv.
fn crit() -> Fixture<'static> {
    let (path, text) = fixture!("coolprop-8.0.0/all/crit.csv");
    Fixture::parse(path, text).unwrap()
}

/// The row of `name` in the crit rows.
fn row(crit: &Fixture<'_>, name: &str) -> usize {
    (0..crit.rows().len()).find(|&r| crit.printed(r, "fluid") == Some(name)).unwrap()
}

/// K1 = 2A01 + A02 and K2 = 2A01 + 4A02 + A03 of the total α: ∂p/∂ρ over RT and ∂²p/∂ρ² over RT/ρ.
fn conditions(eos: &dyn phasekit_core::HelmholtzModel, t: f64, rho: f64) -> (f64, f64) {
    let d = eos.ideal(t, rho, Order::Three) + eos.residual(t, rho, Order::Three);
    let a = |j| d.get(0, j).unwrap();
    (2.0 * a(1) + a(2), 2.0 * a(1) + 4.0 * a(2) + a(3))
}

/// Map 01 §8: Water's published critical point is IAPWS-95's 647.096 K exactly, kept apart from the model's own,
/// which is the oracle's computed 647.0959999999873 K within `Flash` (solved from the superancillary's top, which lies
/// just off τ = δ = 1, where the non-analytic terms are singular).
#[test]
fn published_critical_point_is_exact() {
    let water = Registry::embedded().unwrap().get("Water").unwrap();
    let published = water.info().published_critical().unwrap();
    assert_eq!((published.t, published.origin), (647.096, CriticalOrigin::Published));
    let model = water.model().critical_point().unwrap();
    assert_eq!(model.origin, CriticalOrigin::Model);
    let flash = ToleranceClass::Flash.bound(647.0959999999873).unwrap();
    assert!((model.t - 647.0959999999873).abs() <= flash && model.t != 647.096, "{model:?}");
}

/// Map 03 §6: the numerical critical point, Newton on K1 = K2 = 0 from the published one, satisfies both conditions
/// within `Flash` (1e-9, of scales of 1) for every fluid but Water and CarbonDioxide (their non-analytic terms make
/// the third and fourth derivatives singular at τ = δ = 1, their published point); for the fluids with a
/// superancillary it is the oracle's computed point within `Flash`'s near-critical bounds, but for the cells DIV-0017
/// exempts (user decision CR1: DimethylCarbonate's flat and Chlorine's degenerate critical isotherm, where the
/// oracle's is the less converged).
#[test]
fn numerical_critical_point_satisfies_the_criticality_conditions() {
    let (crit, registry) = (crit(), Registry::from_embedded(DataSet::Parity).unwrap());
    let mut solved = 0;
    for r in 0..crit.rows().len() {
        let name = crit.printed(r, "fluid").unwrap();
        if ["Water", "CarbonDioxide"].contains(&name) {
            continue;
        }
        let v = |c: &str| crit.value(r, c).unwrap();
        let fluid = registry.get(name).unwrap();
        let eos = fluid.model().helmholtz().unwrap();
        let (t, rho, p) = phasekit_core::internal::numerical_critical_point(eos, v("Tc_pub"), v("rhoc_pub")).unwrap();
        let (k1, k2) = conditions(eos, t, rho);
        assert!(k1.abs() <= 1e-9 && k2.abs() <= 1e-9, "{name}: K1 {k1}, K2 {k2}");
        let pseudo_pure = v("Tc_num") == v("Tc_pub") && v("rhoc_num") == v("rhoc_pub");
        if !pseudo_pure {
            for (got, column, bound) in [(t, "Tc_num", 1e-8), (rho, "rhoc_num", 1e-6), (p, "pc_num", 1e-9)] {
                if exempt_at(DIVERGENCES, name, Kind::Crit, column, t).is_some() {
                    continue;
                }
                let bound = bound * v(column);
                assert!((got - v(column)).abs() <= bound, "{name} {column}: {got} against {}", v(column));
            }
        }
        solved += 1;
    }
    assert_eq!(solved, 134);
}

/// Map 02 §6 (ROT-084: CoolProp's `T_critical()` follows a global flag): R13's published critical point, 301.88 K,
/// and its model's own, 303.05 K (the oracle's computed point), are exposed apart and both labelled.
#[test]
fn both_critical_points_are_exposed_distinctly() {
    let (crit, r13) = (crit(), Registry::embedded().unwrap().get("R13").unwrap());
    let r = row(&crit, "R13");
    let published = r13.info().published_critical().unwrap();
    let model = r13.model().critical_point().unwrap();
    assert_eq!(
        (published.t, published.origin, model.origin),
        (301.88, CriticalOrigin::Published, CriticalOrigin::Model)
    );
    let tc = crit.value(r, "Tc_num").unwrap();
    assert!((model.t - tc).abs() <= ToleranceClass::Flash.bound(tc).unwrap() && (model.t - 303.05).abs() < 0.01);
}

/// Oracle: the crit kind's computed columns (VERIFICATION.md §3.5, class `Flash`, near-critical bounds: ρ 1e-6, T 1e-8)
/// and the criticality conditions (user decision CR1). Every model critical point the EOS solved, all 130 with a
/// superancillary, satisfies K1 = K2 = 0 within `Flash` (Water's and CarbonDioxide's too: their superancillary tops lie
/// just off τ = δ = 1, where their non-analytic terms are singular; the pseudo-pure fluids' is the published point, as
/// CoolProp's). And each is CoolProp 8.0.0's computed point within `Flash`'s near-critical bounds, for all 136, but for
/// the 6 cells DIV-0017 exempts.
#[test]
fn model_critical_points_match_the_oracle() {
    let (crit, registry) = (crit(), Registry::from_embedded(DataSet::Parity).unwrap());
    let (mut failures, mut exempt, mut solved) = (Vec::new(), 0, 0);
    for r in 0..crit.rows().len() {
        let name = crit.printed(r, "fluid").unwrap();
        let fluid = registry.get(name).unwrap();
        let model = fluid.model().critical_point().unwrap();
        if model.origin == CriticalOrigin::Model {
            let (k1, k2) = conditions(fluid.model().helmholtz().unwrap(), model.t, model.rho);
            if k1.abs().max(k2.abs()) > 1e-9 {
                failures.push(format!("{name}: K1 {k1}, K2 {k2}"));
            }
            solved += 1;
        }
        for (got, column, bound) in
            [(model.t, "Tc_num", 1e-8), (model.rho, "rhoc_num", 1e-6), (model.p, "pc_num", 1e-9)]
        {
            let want = crit.value(r, column).unwrap();
            if exempt_at(DIVERGENCES, name, Kind::Crit, column, model.t).is_some() {
                exempt += 1;
            } else if (got - want).abs() > bound * want.abs() {
                failures.push(format!("{name} {column}: {got} against {want}"));
            }
        }
    }
    assert_eq!(failures, Vec::<String>::new());
    assert_eq!((solved, exempt), (130, 6));
}
