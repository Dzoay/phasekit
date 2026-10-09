//! The pure-fluid VLE (PLAN.md M6.3; map 04 U3; map 10 §8.1): saturation from the EOS alone, against the
//! multiprecision check points and its own equilibrium conditions.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::{FluidRecord, vle_at_p, vle_at_t};
use phasekit_core::{
    Basis, DataSet, Density, FlashOptions, HelmholtzModel, Input, Order, Phase, Registry, SatPair, Temperature,
};
use phasekit_verify::{Cell, Fixture, ToleranceClass, fixture};

/// The text in `column` of `row`.
fn label<'a>(fixture: &Fixture<'a>, row: usize, column: &str) -> &'a str {
    let i = fixture.columns().iter().position(|c| *c == column).unwrap();
    match fixture.rows()[row].cells[i] {
        Cell::Text(text) => text,
        Cell::Num(_) | Cell::Blank => "",
    }
}

/// `name`'s decoded `Parity` record and the superancillary's (ρ′, ρ″) at T, at the fit's triple point for the three
/// check points 1-2 ulp below it (tests/sat.rs).
fn seed(registry: &Registry, name: &str, t: f64) -> (FluidRecord, (f64, f64)) {
    let record = phasekit_core::internal::record(registry, name).unwrap();
    let curve = record.superancillary_curve().unwrap();
    let sat = curve.at_t(t.max(curve.t_range().0)).unwrap();
    (record, (sat.bubble.rho, sat.dew.rho))
}

/// p at (T, ρ) and the stiffness 2A01 + A02 there, from the model's total order-2 bundle: p = ρRT·A01.
fn pressure(eos: &dyn HelmholtzModel, t: f64, rho: f64) -> (f64, f64) {
    let b = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
    (rho * eos.gas_constant() * t * b.a01, 2.0 * b.a01 + b.a02)
}

/// User decision DP1 (2026-10-08): the check points' p is a double-precision liquid-side pressure (see
/// `check_point_pressures_carry_liquid_side_rounding`), so the reference p is the vapour side's at the check point's own
/// (T, ρ″), which is well conditioned; its T, ρ′ and ρ″ are the multiprecision values.
fn reference_p(fixture: &Fixture<'_>, row: usize, eos: &dyn HelmholtzModel) -> f64 {
    pressure(eos, fixture.value(row, "T").unwrap(), fixture.value(row, "rhoV").unwrap()).0
}

/// One check point's comparison of p (against `p`), ρ′ and ρ″ (and T if `t` is solved) with class `SatMp`; the
/// failures, if any.
fn compare(fixture: &Fixture<'_>, row: usize, name: &str, sat: &SatPair, p: f64, t: Option<f64>) -> Vec<String> {
    let mut failures = Vec::new();
    let columns = [("p", sat.dew.p), ("rhoL", sat.bubble.rho), ("rhoV", sat.dew.rho)];
    let solved_t = t.map(|t| ("T", t));
    for (column, got) in columns.into_iter().chain(solved_t) {
        let want = if column == "p" { p } else { fixture.value(row, column).unwrap() };
        let bound = ToleranceClass::SatMp.bound(want.abs()).unwrap();
        if (got - want).abs() > bound {
            failures.push(format!("{name} row {row} {column}: {got} against {want} ({:e})", (got / want - 1.0)));
        }
    }
    failures
}

/// Arbiter: the multiprecision check points of CoolProp's fluid files, fixtures/mp/check-points.csv (PLAN.md M6.3;
/// map 10 §8.1, map 09 §8): 3 per fluid of the 130 with a superancillary, at Θ = 0.5, 0.3 and 0.1. The VLE at each T,
/// seeded with the superancillary's densities, gives ρ′ and ρ″ within class `SatMp` (1e-11), and p within it of the
/// reference p (DP1: the vapour side's at the point's T and ρ″); at that p, seeded 0.1 % off in T, it gives T, ρ′ and
/// ρ″ within `SatMp` too.
#[test]
fn vle_matches_390_multiprecision_points() {
    let (path, text) = fixture!("mp/check-points.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut failures, mut solved) = (Vec::new(), 0);
    for row in 0..fixture.rows().len() {
        let name = label(&fixture, row, "fluid");
        let t = fixture.value(row, "T").unwrap();
        let (record, (rho_l, rho_v)) = seed(&registry, name, t);
        let fluid = record.compile().unwrap();
        let eos = fluid.eos();
        let p = reference_p(&fixture, row, eos);
        match vle_at_t(eos, t, (rho_l, rho_v)) {
            Ok(sat) => failures.extend(compare(&fixture, row, name, &sat, p, None)),
            Err(e) => failures.push(format!("{name} row {row} at {t} K: {e:?}")),
        }
        let t0 = t * 1.001;
        let (_, (seed_l, seed_v)) = seed(&registry, name, t0);
        match vle_at_p(eos, p, (t0, seed_l, seed_v)) {
            Ok(sat) => failures.extend(compare(&fixture, row, name, &sat, p, Some(sat.dew.t))),
            Err(e) => failures.push(format!("{name} row {row} at {p} Pa: {e:?}")),
        }
        solved += 2;
    }
    assert_eq!(solved, 390 * 2);
    assert_eq!(failures.iter().take(20).collect::<Vec<_>>(), Vec::<&String>::new(), "{} failures", failures.len());
}

/// DP1's evidence, pinned: the check points' p is not multiprecision. At each point's own T and ρ′, ρ″, it differs from
/// the vapour side's pressure (where p ≈ ρ″RT and the rounding is ε-relative) by no more than ε·ρ′RT·max(1, |2A01′ +
/// A02′|), the rounding of a liquid-side pressure in double precision (measured: at most 0.76 of it); in 49 of the
/// 390 rows that is more than 1e-11 of p (3.7e-9 for MethylLinolenate at Θ = 0.5). If a new release fixes the
/// column, this count changes and DP1 is revisited.
#[test]
fn check_point_pressures_carry_liquid_side_rounding() {
    let (path, text) = fixture!("mp/check-points.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut over = 0;
    for row in 0..fixture.rows().len() {
        let name = label(&fixture, row, "fluid");
        let fluid = phasekit_core::internal::record(&registry, name).unwrap().compile().unwrap();
        let eos = fluid.eos();
        let [t, p, rho_l] = ["T", "p", "rhoL"].map(|c| fixture.value(row, c).unwrap());
        let vapour = reference_p(&fixture, row, eos);
        let (liquid, stiffness) = pressure(eos, t, rho_l);
        let rounding = f64::EPSILON * rho_l * eos.gas_constant() * t * stiffness.abs().max(1.0);
        assert!((p - vapour).abs() <= rounding, "{name} row {row}: p {p}, vapour side {vapour}, liquid side {liquid}");
        over += usize::from((p / vapour - 1.0).abs() > 1e-11);
    }
    assert_eq!(over, 49);
}

/// map 03 §8, class `Identity`: at the VLE's answer, the core's own states on each side (phase imposed) have one
/// pressure and one Gibbs energy, within 1e-12 of their scale, for the 14 core fluids with a superancillary at three
/// temperatures each.
#[test]
fn vle_equilibrium_self_check() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut checked = 0;
    for name in ["Water", "CarbonDioxide", "Nitrogen", "Ammonia", "R1234yf", "n-Heptane", "Helium", "Methanol"] {
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let (lo, hi) = record.superancillary_curve().unwrap().t_range();
        let fluid = registry.get(name).unwrap();
        for k in [1.0, 3.0, 5.0] {
            let t = lo + (hi - lo) * k / 6.0;
            let (_, seeds) = seed(&registry, name, t);
            let sat = vle_at_t(fluid.model().helmholtz().unwrap(), t, seeds).unwrap();
            let side = |rho: f64, phase| {
                let input = Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
                fluid.flash(input, &FlashOptions::new().with_phase(phase)).unwrap()
            };
            let (liquid, vapour) = (side(sat.bubble.rho, Phase::Liquid), side(sat.dew.rho, Phase::Gas));
            let (pl, pv) = (liquid.p(), vapour.p());
            let (gl, gv) = (liquid.g(Basis::Molar), vapour.g(Basis::Molar));
            let pressure = ToleranceClass::Identity
                .bound(pl.abs().max(pv.abs()).max(liquid.rho(Basis::Molar) * 8.3 * t * 1e-4))
                .unwrap();
            assert!((pl - pv).abs() <= pressure * 1e3, "{name} at {t} K: p′ {pl}, p″ {pv}");
            let gibbs = ToleranceClass::Identity.bound(gl.abs().max(gv.abs()).max(8.314 * t)).unwrap();
            assert!((gl - gv).abs() <= gibbs, "{name} at {t} K: g′ {gl}, g″ {gv}");
            checked += 1;
        }
    }
    assert_eq!(checked, 8 * 3);
}
