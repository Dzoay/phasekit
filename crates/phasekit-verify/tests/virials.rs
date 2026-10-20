//! Exact virial coefficients (PLAN.md M5.6; ROT-063; map 02 §6, map 12 §6.3): B, C and their T-derivatives are the Taylor
//! coefficients of α^r at δ = 0 (`HelmholtzModel::zero_density`), not CoolProp's evaluation at δ = 1e-12.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, HelmholtzModel, Order, Prop, Registry, Virials};
use phasekit_cubic::Kind;
use phasekit_verify::{DIVERGENCES, Fixture, Tolerance, ToleranceClass, fixture, majorant};

/// The virials of `eos` at T by Richardson extrapolation of α^r's δ-derivatives to δ = 0, independent of
/// `zero_density`: with α^r = Σ a_k δ^k, A01/δ → a1, A02/δ² → 2a2, A11/δ → τa1′, A12/δ² → 2τa2′, A21/δ → τ²a1″ and
/// A22/δ² → 2τ²a2″, each the combination (8f(δ) − 6f(2δ) + f(4δ))/3 whose error is O(δ³), at δ = 1e-7 (the
/// two-point 2f(δ) − f(2δ), O(δ²), leaves 3.6e-12 of Ethane's d²B/dT², measured at M14.1);
/// T²d²/dT² = τ²d²/dτ² + 2τd/dτ.
fn extrapolated(eos: &dyn HelmholtzModel, t: f64, rho_r: f64) -> Virials {
    let delta = 1e-7;
    let at = |d: f64| eos.residual(t, d * rho_r, Order::Four);
    let points = [(8.0, at(delta), 1.0), (-6.0, at(2.0 * delta), 2.0), (1.0, at(4.0 * delta), 4.0)];
    let limit = |i, j| {
        let power = |k: f64| if j == 1 { k * delta } else { k * k * delta * delta };
        let combined: f64 = points.iter().map(|(w, d, k)| w * d.get(i, j).unwrap() / power(*k)).sum();
        combined / 3.0
    };
    let (a1, two_a2, ta1, two_ta2) = (limit(0, 1), limit(0, 2), limit(1, 1), limit(1, 2));
    let (tta1, two_tta2) = (limit(2, 1), limit(2, 2));
    let (r2, t2) = (rho_r * rho_r, t * t);
    Virials {
        b: a1 / rho_r,
        c: two_a2 / r2,
        db_dt: -ta1 / (t * rho_r),
        dc_dt: -two_ta2 / (t * r2),
        d2b_dt2: (tta1 + 2.0 * ta1) / (t2 * rho_r),
        d2c_dt2: (two_tta2 + 2.0 * two_ta2) / (t2 * r2),
    }
}

/// ROT-063, class `Identity`: for all 136 fluids at T = 0.8, 1 and 1.5 T_r, B, C and their first two T-derivatives
/// (the second since M14.1, user decision HA6) from `zero_density`
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
                (
                    "d²B/dT²",
                    exact.d2b_dt2,
                    series.d2b_dt2,
                    (term(2, 1) + 2.0 * term(1, 1)) / delta / (t * t * e.rho_reducing),
                ),
                (
                    "d²C/dT²",
                    exact.d2c_dt2,
                    series.d2c_dt2,
                    (term(2, 2) + 2.0 * term(1, 2)) / (delta * delta) / (t * t * r2),
                ),
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

/// The second T-derivatives of `eos`'s virials at T against central differences of its exact first derivatives at
/// the relative step 1e-5, class `Fd`, each scaled by the larger of the two sides and the first derivative over T (a
/// second derivative that crosses zero is compared at the scale of its terms): the misses, named.
fn second_derivative_misses(name: &str, eos: &dyn HelmholtzModel, t: f64) -> Vec<String> {
    let h = 1e-5 * t;
    let (v, up, down) =
        (eos.zero_density(t).unwrap(), eos.zero_density(t + h).unwrap(), eos.zero_density(t - h).unwrap());
    let pairs = [
        ("d²B/dT²", v.d2b_dt2, (up.db_dt - down.db_dt) / (2.0 * h), v.db_dt),
        ("d²C/dT²", v.d2c_dt2, (up.dc_dt - down.dc_dt) / (2.0 * h), v.dc_dt),
    ];
    let mut misses = Vec::new();
    for (what, got, fd, first) in pairs {
        let bound = ToleranceClass::Fd.bound(got.abs().max(fd.abs()).max(first.abs() / t)).unwrap();
        let error = (got - fd).abs();
        if error.is_nan() || error > bound {
            misses.push(format!("{name} {what} at {t} K: {got:e} against {fd:e}"));
        }
    }
    misses
}

/// PLAN.md M14.1 (user decision HA6), class `Fd`: d²B/dT² and d²C/dT² from `zero_density` against central differences
/// of the exact dB/dT and dC/dT, for all 136 fluids at T = 0.8, 1 and 1.5 T_r, the van der Waals, SRK and PR fluids
/// of n-Propane, Water and CO₂, and four mixtures (R410A.mix, Air.mix, a GERG-2008 natural gas, a ternary), at 0.8, 1
/// and 1.5 times their reducing temperatures.
#[test]
fn second_virial_derivatives_match_central_differences() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let cubics = phasekit_cubic::with_cubics(&registry, &[Kind::Vdw, Kind::Srk, Kind::Pr], DataSet::Parity).unwrap();
    let mixtures = phasekit_mix::with_mixtures(&registry, DataSet::Parity);
    let mut names: Vec<String> = phasekit_data::FLUIDS.iter().map(|f| f.name.to_string()).collect();
    for kind in [Kind::Vdw, Kind::Srk, Kind::Pr] {
        names.extend(["n-Propane", "Water", "CarbonDioxide"].map(|n| format!("{}::{n}", kind.prefix())));
    }
    names.extend(MIXTURES.map(String::from));
    let mut misses = Vec::new();
    for name in &names {
        let fluid = cubics.get(name).cloned().or_else(|_| mixtures.resolve(name)).unwrap();
        let eos = fluid.model().helmholtz().unwrap();
        let t_r = eos.reducing().map_or_else(|| fluid.model().critical_point().unwrap().t, |r| r.t);
        for factor in [0.8, 1.0, 1.5] {
            misses.extend(second_derivative_misses(name, eos, factor * t_r));
        }
    }
    assert_eq!(misses, Vec::<String>::new());
}

/// The mixtures [`second_virial_derivatives_match_central_differences`] checks (`tests/mixture_family.rs`'s).
const MIXTURES: [&str; 4] = [
    "R410A.mix",
    "Air.mix",
    "GERG2008::Methane[0.85]&Ethane[0.07]&n-Propane[0.03]&Nitrogen[0.03]&CarbonDioxide[0.02]",
    "HEOS::R32[0.4]&R125[0.2]&R134a[0.4]",
];

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
        for row in 0..fixture.rows().len() {
            for error in bvirial_errors(&registry, (path, &fixture), row, bound) {
                worst = worst.max(error);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, (14 * 500 + 136 * 8) * 2);
    assert!(worst > 1e-8, "the oracle's δ = 1e-12 evaluation shows: {worst:e}");
}

/// The relative errors of B and dB/dT at row `row` of the `eos` file at `path`, each asserted within `bound`: the
/// row's fluid is the file's, or for the all-fluid tier the row's first cell.
fn bvirial_errors(registry: &Registry, (path, fixture): (&str, &Fixture<'_>), row: usize, bound: f64) -> [f64; 2] {
    let single = fixture.header("fluid").and_then(|f| f.split(' ').next()).filter(|_| !path.contains("/all/"));
    let of_row = || match fixture.rows()[row].cells[0] {
        phasekit_verify::Cell::Text(name) => Some(name),
        _ => None,
    };
    let name = single.or_else(of_row).ok_or_else(|| format!("{path}: row {row} has no fluid")).unwrap();
    let eos = registry.get(name).unwrap().model().helmholtz().unwrap();
    let v = eos.zero_density(fixture.value(row, "T").unwrap()).unwrap();
    [("Bvirial", v.b), ("dBvirial_dT", v.db_dt)].map(|(column, got)| {
        let want = fixture.value(row, column).unwrap();
        let error = (got - want).abs() / want.abs();
        assert!(error <= bound, "{path} row {row} {column}: {got:e} against {want:e} ({error:e})");
        error
    })
}
