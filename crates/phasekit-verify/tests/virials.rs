//! Exact virial coefficients (PLAN.md M5.6; ROT-063; map 02 §6, map 12 §6.3): B, C and their T-derivatives are the Taylor
//! coefficients of α^r at δ = 0 (`HelmholtzModel::zero_density`), not CoolProp's evaluation at δ = 1e-12.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, HelmholtzModel, Order, Prop, Registry, Virials};
use phasekit_verify::{DIVERGENCES, Fixture, Tolerance, ToleranceClass, fixture, majorant};

/// The virials of `eos` at T by Richardson extrapolation of α^r's δ-derivatives to δ = 0, independent of
/// `zero_density`: with α^r = Σ a_k δ^k, A01/δ → a1, A02/δ² → 2a2, A11/δ → τa1′ and A12/δ² → 2τa2′, each the
/// combination 2f(δ) − f(2δ) whose error is O(δ²), at δ = 1e-7.
fn extrapolated(eos: &dyn HelmholtzModel, t: f64, rho_r: f64) -> Virials {
    let delta = 1e-7;
    let at = |d: f64| eos.residual(t, d * rho_r, Order::Three);
    let (near, far) = (at(delta), at(2.0 * delta));
    let limit = |i, j| {
        let power = if j == 1 { delta } else { delta * delta };
        let scale = if j == 1 { 2.0 } else { 4.0 };
        2.0 * near.get(i, j).unwrap() / power - far.get(i, j).unwrap() / (scale * power)
    };
    let (a1, two_a2, ta1, two_ta2) = (limit(0, 1), limit(0, 2), limit(1, 1), limit(1, 2));
    let r2 = rho_r * rho_r;
    Virials { b: a1 / rho_r, c: two_a2 / r2, db_dt: -ta1 / (t * rho_r), dc_dt: -two_ta2 / (t * r2) }
}

/// ROT-063, class `Identity`: for all 136 fluids at T = 0.8, 1 and 1.5 T_r, B, C, dB/dT and dC/dT from `zero_density`
/// equal the δ → 0 extrapolation of the same α^r, within 1e-12 of the scale of their terms (each entry's `Term`
/// majorant at δ = 1e-7, divided as the entry is); the `Prop` outputs are those values.
#[test]
fn virials_equal_the_delta_series_for_every_fluid() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    for f in phasekit_data::FLUIDS {
        let fluid = registry.get(f.name).unwrap();
        let record = phasekit_core::internal::record(&registry, f.name).unwrap();
        let (eos, e) = (fluid.model().helmholtz().unwrap(), &record.eos);
        for factor in [0.8, 1.0, 1.5] {
            let t = factor * e.t_reducing;
            let exact = eos.zero_density(t).unwrap();
            let series = extrapolated(eos, t, e.rho_reducing);
            let (tau, delta, r2) = (e.t_reducing / t, 1e-7, e.rho_reducing * e.rho_reducing);
            let term = |i, j| majorant::eos(e, tau, delta, i, j);
            let entries = [
                ("B", exact.b, series.b, term(0, 1) / delta / e.rho_reducing),
                ("C", exact.c, series.c, term(0, 2) / (delta * delta) / r2),
                ("dB/dT", exact.db_dt, series.db_dt, term(1, 1) / delta / (t * e.rho_reducing)),
                ("dC/dT", exact.dc_dt, series.dc_dt, term(1, 2) / (delta * delta) / (t * r2)),
            ];
            for (what, got, want, scale) in entries {
                let bound = ToleranceClass::Identity.bound(scale.max(want.abs())).unwrap();
                assert!((got - want).abs() <= bound, "{} {what} at {t} K: {got:e} against {want:e}", f.name);
            }
            let state = fluid.state(phasekit_core::Input::dt(
                phasekit_core::Density::molar(1.0).unwrap(),
                phasekit_core::Temperature::new(t.clamp(record.limits.t_min(), record.limits.t_max())).unwrap(),
            ));
            if let Ok(state) = state {
                let v = eos.zero_density(state.t()).unwrap();
                let props =
                    [Prop::Bvirial, Prop::Cvirial, Prop::DBvirialDT, Prop::DCvirialDT].map(|p| fluid.prop(&state, p));
                assert_eq!(props, [Ok(v.b), Ok(v.c), Ok(v.db_dt), Ok(v.dc_dt)], "{}", f.name);
            }
        }
    }
}

/// Oracle: CoolProp 8.0.0, the `eos` fixtures' `Bvirial` and `dBvirial_dT` (core subset and all-fluid tier; PLAN.md
/// M5.6). Class `Measured` at DIV-0011's registered bound, relative 9e-8: the oracle takes B from α^r's δ-derivative at
/// δ = 1e-12, divided by δ, which loses up to 1.5e-8 of B and 8.6e-8 of dB/dT (Methanol, measured at M5.6, where the
/// series evaluated straight from the fluid file is ours to the last digit). C and dC/dT are DIV-0011's.
#[test]
fn bvirial_matches_oracle_within_the_registered_bound() {
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0011").unwrap();
    let Some(Tolerance::Relative(bound)) = entry.tolerance else { panic!("DIV-0011 registers a relative bound") };
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut files: Vec<(&str, &str)> = phasekit_verify::eos::CORE_FILES.to_vec();
    files.push(fixture!("coolprop-8.0.0/all/eos.csv"));
    let (mut checked, mut worst) = (0, 0.0_f64);
    for (path, text) in files {
        let fixture = Fixture::parse(path, text).unwrap();
        let single = fixture.header("fluid").and_then(|f| f.split(' ').next()).filter(|_| !path.contains("/all/"));
        for row in 0..fixture.rows().len() {
            let name = match single {
                Some(name) => name,
                None => match fixture.rows()[row].cells[0] {
                    phasekit_verify::Cell::Text(name) => name,
                    _ => panic!("{path}: row {row} has no fluid"),
                },
            };
            let eos = registry.get(name).unwrap().model().helmholtz().unwrap();
            let v = eos.zero_density(fixture.value(row, "T").unwrap()).unwrap();
            for (column, got) in [("Bvirial", v.b), ("dBvirial_dT", v.db_dt)] {
                let want = fixture.value(row, column).unwrap();
                let error = (got - want).abs() / want.abs();
                assert!(error <= bound, "{path} row {row} {column}: {got:e} against {want:e} ({error:e})");
                worst = worst.max(error);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, (14 * 500 + 136 * 8) * 2);
    assert!(worst > 1e-8, "the oracle's δ = 1e-12 evaluation shows: {worst:e}");
}
