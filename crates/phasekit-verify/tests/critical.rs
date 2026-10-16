//! A mixture's critical points at fixed composition (PLAN.md M13.7): Heidemann and Khalil's conditions, teqp's
//! critical locus of GERG-2008's methane/ethane, and the oracle's `all_critical_points`.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Fluid, Registry};
use phasekit_verify::register::exempt_at;
use phasekit_verify::{DIVERGENCES, Fixture, Kind, Tolerance, ToleranceClass, fixture};

#[path = "common/mix_crit.rs"]
mod mix_crit;
use mix_crit::{binary, mixture, offsets, oracle_points};

fn registry() -> Registry {
    Registry::from_embedded(DataSet::Corrected).unwrap()
}

/// Every critical point found for the library's Methane/Ethane, Methane/n-Propane, R32/R125 and CO₂/Water and
/// GERG-2008's Methane/Ethane at 50/50 (the stability limit's inner branches and negative pressures among them) meets
/// det(D Q D) = 0 and the cubic form = 0 within `Identity` of each sum's largest term.
#[test]
fn critical_points_satisfy_heidemann_khalil() {
    let registry = registry();
    let pairs = [("Methane", "Ethane"), ("Methane", "n-Propane"), ("R32", "R125"), ("CarbonDioxide", "Water")];
    let mut fluids: Vec<(String, Fluid)> = pairs
        .iter()
        .map(|&(a, b)| {
            let fluid = phasekit_mix::mixture(&registry, &[(a, 0.5), (b, 0.5)], DataSet::Corrected).unwrap();
            (format!("{a}/{b}"), fluid)
        })
        .collect();
    let gerg = phasekit_mix::gerg2008(&registry, &[("Methane", 0.5), ("Ethane", 0.5)]).unwrap();
    fluids.push(("GERG2008 Methane/Ethane".into(), gerg));
    let mut points = 0;
    for (name, fluid) in &fluids {
        let mix = mixture(fluid);
        assert!(!mix.critical_points().is_empty(), "{name}: no critical point");
        for c in mix.critical_points() {
            let k = mix.criticality(c.t, c.rho).unwrap();
            let (det, cubic) =
                (ToleranceClass::Identity.bound(k.det_scale), ToleranceClass::Identity.bound(k.cubic_scale));
            assert!(k.det.abs() <= det.unwrap() && k.cubic.abs() <= cubic.unwrap(), "{name} at {} K: {k:?}", c.t);
            points += 1;
        }
    }
    assert!(points >= fluids.len());
}

/// teqp 0.23.2's critical locus of GERG-2008's methane/ethane (`trace_critical_arclength_binary`, polished;
/// differential, never an arbiter): at each of its compositions phasekit's GERG-2008 mixture has its vapour-liquid
/// critical point at the locus's T, ρ and p within `Flash`. Bell and Jäger 2017 arbitrates once in hand.
#[test]
fn gerg_methane_ethane_critical_locus_matches_teqp() {
    let (path, text) = fixture!("teqp-0.23.2/gerg/critical.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = registry();
    let mut misses = Vec::new();
    for row in 0..table.rows().len() {
        let z1 = table.value(row, "z1").unwrap();
        let fluid = phasekit_mix::gerg2008(&registry, &[("Methane", z1), ("Ethane", 1.0 - z1)]).unwrap();
        let c = fluid.model().critical_point().unwrap();
        for (column, got) in [("T", c.t), ("rhomolar", c.rho), ("p", c.p)] {
            if let Err(e) = table.check_scaled(row, column, got, got.abs()) {
                misses.push(format!("z1 = {z1} {column}: {e:?}"));
            }
        }
    }
    assert!(misses.is_empty(), "{misses:#?}");
    assert!(table.rows().len() >= 10);
}

/// The misses of phasekit's critical points against the oracle's point `o`, within the relative bound `measured`: the
/// nearest point's T, ρ and p, and where `o` is the oracle's first point and stable, `critical_point`'s T.
fn compare(fluid: &Fluid, o: &mix_crit::OraclePoint, measured: f64) -> Vec<String> {
    let mut misses = Vec::new();
    match offsets(mixture(fluid).critical_points(), o) {
        Some(off) if off.iter().all(|x| *x <= measured) => {}
        off => misses.push(format!("{} at {}: the oracle's {} K: {off:?}", o.label, o.z1, o.t)),
    }
    let c = fluid.model().critical_point();
    if o.index == 0.0 && o.stable && !c.is_some_and(|c| ((c.t - o.t) / o.t).abs() <= measured) {
        misses.push(format!("{} at {}: critical_point {c:?} against {} K", o.label, o.z1, o.t));
    }
    misses
}

/// Oracle: CoolProp 8.0.0's `all_critical_points` (fixtures/coolprop-8.0.0/all/mix_crit.csv) for ten library binaries
/// at three compositions each, on `Parity`. Each point the oracle reports inside the mixture's range is one of
/// phasekit's, T, ρ and p within DIV-0052's measured bound (v8.0.0's L* weights no ideal term), but DIV-0056's three,
/// where L*'s first row vanishes and no critical point is. The points below the components' T_min (map 04 #14:
/// Methane/n-Propane's "stable" 70 K, 162 MPa) are counted and skipped, and the oracle's `stable` flags are not read.
/// The vapour-liquid one, `critical_point`, is the oracle's first point where it is stable.
#[test]
fn critical_points_match_the_oracle() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0052").unwrap();
    let measured = match entry.table_tolerances.iter().find(|(file, _)| *file == "coolprop-8.0.0/all/mix_crit.csv") {
        Some((_, Tolerance::Relative(x))) => *x,
        _ => f64::NAN,
    };
    let (mut compared, mut out_of_range, mut spurious, mut misses) = (0, 0, 0, Vec::new());
    let mut fluid: Option<(String, f64, Fluid)> = None;
    for o in oracle_points() {
        if fluid.as_ref().is_none_or(|(label, z1, _)| (label, *z1) != (&o.label, o.z1)) {
            fluid = Some((o.label.clone(), o.z1, binary(&registry, &o.label, o.z1)));
        }
        let Some((_, _, fluid)) = &fluid else { continue };
        if o.t < fluid.model().limits().unwrap().t_min() {
            out_of_range += 1;
            continue;
        }
        if exempt_at(DIVERGENCES, &o.label, Kind::MixCrit, "*", o.t) == Some("DIV-0056") {
            spurious += 1;
            continue;
        }
        misses.extend(compare(fluid, &o, measured));
        compared += 1;
    }
    assert!(misses.is_empty(), "{} misses: {:#?}", misses.len(), misses);
    assert!(compared >= 39 && out_of_range >= 16 && spurious == 3, "{compared}, {out_of_range}, {spurious}");
}
