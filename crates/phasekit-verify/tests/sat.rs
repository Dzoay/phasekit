//! L3 saturation (map 10 §8.5): the superancillary evaluated from the blobs against the oracle's own evaluation of the
//! same coefficients and against the multiprecision check points (VERIFICATION.md §3.5, §5), and its exact rescaling
//! under a gas-constant or reducing-density correction (VERIFICATION.md §7.3).

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Registry, SatAccuracy, SaturationCurve};
use phasekit_verify::{Cell, CheckError, Fixture, ToleranceClass, fixture};

/// The superancillary of `name` from the embedded blob, under `data_set` and the hash gate.
fn curve(registry: &Registry, name: &str, data_set: DataSet) -> Box<dyn SaturationCurve> {
    let mut record = phasekit_core::internal::record(registry, name).unwrap();
    record.apply(data_set).unwrap();
    record.superancillary_curve().unwrap()
}

/// The text in `column` of `row`.
fn label<'a>(fixture: &Fixture<'a>, row: usize, column: &str) -> &'a str {
    let i = fixture.columns().iter().position(|c| *c == column).unwrap();
    match fixture.rows()[row].cells[i] {
        Cell::Text(text) => text,
        Cell::Num(_) | Cell::Blank => "",
    }
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/all/sat.csv, the `sa` rows (PLAN.md M5.2; map 03 §8): p, ρ′ and ρ″
/// at 8 temperatures per fluid of the 130 with a superancillary, Θ = 1 − T/Tc log-spaced from 1e-7 to the triple
/// point, against `CP.SuperAncillary(json).eval_sat` on the same coefficients. Class `SaCoeff`. Water's curve spans
/// exactly its fitted range, triple point to numerical critical point.
#[test]
fn superancillary_matches_oracle_eval_sat() {
    let (path, text) = fixture!("coolprop-8.0.0/all/sat.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut failures, mut checked, mut headroom, mut fluids) = (Vec::new(), 0, 0.0_f64, Vec::new());
    for row in 0..fixture.rows().len() {
        assert_eq!((label(&fixture, row, "input"), label(&fixture, row, "status")), ("sa", "ok"), "row {row}");
        let name = label(&fixture, row, "fluid");
        if fluids.last() != Some(&name) {
            fluids.push(name);
        }
        let sat = curve(&registry, name, DataSet::Parity).at_t(fixture.value(row, "T").unwrap()).unwrap();
        for (column, got) in [("p", sat.bubble.p), ("rhoL", sat.bubble.rho), ("rhoV", sat.dew.rho)] {
            checked += 1;
            let scale = fixture.value(row, column).unwrap().abs();
            match fixture.check_scaled(row, column, got, scale) {
                Ok(ratio) => headroom = headroom.max(ratio),
                Err(CheckError::Mismatch(m)) => failures.push(m.to_string()),
                Err(e) => failures.push(format!("{name}, row {row}, {column}: {e:?}")),
            }
        }
    }
    assert_eq!((fluids.len(), checked), (130, 130 * 8 * 3));
    assert_eq!(failures, Vec::<String>::new(), "headroom of the rest {headroom:.3}");
    assert_eq!(curve(&registry, "Water", DataSet::Parity).t_range(), (273.16, 647.095_999_999_987_3));
}

/// Arbiter: the multiprecision check points of CoolProp's fluid files, fixtures/mp/check-points.csv (PLAN.md M1.17,
/// M5.2): 3 per fluid of the 130, at Θ = 0.5, 0.3, 0.1. Class `SaFit`: within 4 × the point's own SA/mp misfit
/// (floor 1e-14), CoolProp's acceptance rule (map 10 §3). Three points sit an ulp or two below their fit's triple point
/// and are evaluated at it.
#[test]
fn superancillary_matches_the_check_points() {
    let (path, text) = fixture!("mp/check-points.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut failures, mut moved) = (Vec::new(), Vec::new());
    for row in 0..fixture.rows().len() {
        let name = label(&fixture, row, "fluid");
        let curve = curve(&registry, name, DataSet::Parity);
        // fastchebpure moved points below the triple point up to it; three came out 1-2 ulp below the fit, which the
        // curve refuses (D6), so they are evaluated at the triple point itself.
        let (t, t_min) = (fixture.value(row, "T").unwrap(), curve.t_range().0);
        if t < t_min {
            assert!(t_min - t <= 4.0 * f64::EPSILON * t_min, "{name}: {t} K is not at the triple point {t_min} K");
            moved.push(name);
        }
        let sat = curve.at_t(t.max(t_min)).unwrap();
        for (column, ratio, got) in [
            ("p", "p_sa_mp", sat.bubble.p),
            ("rhoL", "rhoL_sa_mp", sat.bubble.rho),
            ("rhoV", "rhoV_sa_mp", sat.dew.rho),
        ] {
            let bound = ToleranceClass::sa_fit(fixture.value(row, ratio).unwrap(), fixture.value(row, column).unwrap());
            if let Err(e) = fixture.check_bound(row, column, got, bound) {
                failures.push(format!("{name}: {e:?}"));
            }
        }
    }
    assert_eq!(fixture.rows().len(), 390);
    assert_eq!(moved, ["Nitrogen", "R114", "RC318"]);
    assert_eq!(failures, Vec::<String>::new());
}

/// VERIFICATION.md §7.3 (E14): a correction of R alone (R1234ze(E), DIV-0001) or of ρ_r alone (Nitrogen, DIV-0003)
/// keeps the superancillary `Exact`. Saturation is invariant in (τ, δ), so `Corrected` is `Parity` with ρ′ and ρ″ times
/// ρ_r′/ρ_r and p times (R′/R)(ρ_r′/ρ_r), bit for bit.
#[test]
fn rescaled_curves_apply_exact_factors() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let factors = [("R1234ze(E)", 8.314_462_1 / 8.314_472, 1.0), ("Nitrogen", 1.0, 11_183.9 / 11_183.901_464_580_624)];
    for (name, r, rho) in factors {
        let (parity, corrected) = (curve(&registry, name, DataSet::Parity), curve(&registry, name, DataSet::Corrected));
        assert_eq!((parity.accuracy(), corrected.accuracy()), (SatAccuracy::Exact, SatAccuracy::Exact), "{name}");
        assert_eq!(parity.t_range(), corrected.t_range());
        let (lo, hi) = parity.t_range();
        for k in 0..=20 {
            let t = lo + (hi - lo) * f64::from(k) / 20.0;
            let (p, c) = (parity.at_t(t).unwrap(), corrected.at_t(t).unwrap());
            assert_eq!(c.bubble.p.to_bits(), (p.bubble.p * (rho * r)).to_bits(), "{name} p at {t} K");
            assert_eq!(c.bubble.rho.to_bits(), (p.bubble.rho * rho).to_bits(), "{name} ρ′ at {t} K");
            assert_eq!(c.dew.rho.to_bits(), (p.dew.rho * rho).to_bits(), "{name} ρ″ at {t} K");
        }
    }
}

/// VERIFICATION.md §3.2, §11.3: `all/sat.csv` comes from the pinned runner image, like every committed oracle fixture.
#[test]
fn sat_fixtures_name_the_pinned_generator_environment() {
    let environment = |(path, text): (&str, &str)| {
        let generator = Fixture::parse(path, text).unwrap().header("generator").unwrap_or_default();
        generator.split(' ').filter(|field| !field.starts_with("sha256=")).collect::<Vec<_>>().join(" ")
    };
    let smoke = environment(fixture!("coolprop-8.0.0/facts/smoke.csv"));
    assert_eq!(environment(fixture!("coolprop-8.0.0/all/sat.csv")), smoke);
}
