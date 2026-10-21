//! IAPWS water and ice through `phasekit-iapws` (PLAN.md M14; user decisions HA1, HA4): each release's check table,
//! within its printed digits (`Paper`), the values converted to the table's units.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::num::NonZeroUsize;

use phasekit_core::batch::{BatchRequest, ExecPolicy};
use phasekit_core::math;
use phasekit_core::{Basis, Error, Fluid, Gauge, Input, Pair, Phase, Prop, State, Var};
use phasekit_iapws::if97;
use phasekit_verify::conformance::{Outcome, capability_matrix, gauge_invariance, identities, policy_equivalence};
use phasekit_verify::register::{RowKey, exempt_row};
use phasekit_verify::{Cell, DIVERGENCES, Fixture, Kind, ToleranceClass, fixture};

/// The table at `path`, parsed.
fn table((path, text): (&'static str, &'static str)) -> Fixture<'static> {
    Fixture::parse(path, text).unwrap()
}

/// Each `(column, value)` of a row checked against `table`'s row `row`: the misses, named.
fn misses(table: &Fixture<'_>, row: usize, values: &[(&str, f64)]) -> Vec<String> {
    values.iter().filter_map(|&(column, got)| table.check(row, column, got).err().map(|e| format!("{e:?}"))).collect()
}

/// A state's v, h, u, s, c_p and w in Table 5's and Table 15's units: m³/kg, kJ/kg, kJ/(kg K), m/s.
fn printed_properties(state: &State) -> [(&'static str, f64); 6] {
    [
        ("v", 1.0 / state.rho(Basis::Mass)),
        ("h", state.h(Basis::Mass) / 1e3),
        ("u", state.u(Basis::Mass) / 1e3),
        ("s", state.s(Basis::Mass) / 1e3),
        ("cp", state.cp(Basis::Mass).unwrap_or(f64::NAN) / 1e3),
        ("w", state.speed_of_sound().unwrap_or(f64::NAN)),
    ]
}

/// Arbiter: IAPWS R7-97(2012) Table 5, region 1's v, h, u, s, c_p and w at (300 K, 3 MPa), (300 K, 80 MPa) and
/// (500 K, 3 MPa), through core's `bundle_from_gibbs` and `State::from_total` (PLAN.md M14.2).
#[test]
fn region_1_is_table_5() {
    let table = table(fixture!("paper/Water/IAPWS-R7-97-2012.5.csv"));
    let mut missed = Vec::new();
    for row in 0..table.rows().len() {
        let (t, p) = (table.value(row, "T").unwrap(), table.value(row, "p").unwrap() * 1e6);
        match if97::region1(t, p) {
            Ok(state) => missed.extend(misses(&table, row, &printed_properties(&state))),
            Err(e) => missed.push(format!("row {row}: {e}")),
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Arbiter: IAPWS R7-97(2012) Tables 35 and 36, the saturation pressure at 300, 500 and 600 K (Eq. (30)) and the
/// saturation temperature at 0.1, 1 and 10 MPa (Eq. (31)) (PLAN.md M14.2).
#[test]
fn the_saturation_line_is_tables_35_and_36() {
    let (pressures, temperatures) = (
        table(fixture!("paper/Water/IAPWS-R7-97-2012.35.csv")),
        table(fixture!("paper/Water/IAPWS-R7-97-2012.36.csv")),
    );
    let mut missed = Vec::new();
    for row in 0..pressures.rows().len() {
        let t = pressures.value(row, "T").unwrap();
        let p = if97::saturation_pressure(t).map_or(f64::NAN, |p| p / 1e6);
        missed.extend(misses(&pressures, row, &[("p", p)]));
    }
    for row in 0..temperatures.rows().len() {
        let p = temperatures.value(row, "p").unwrap() * 1e6;
        let t = if97::saturation_temperature(p).unwrap_or(f64::NAN);
        missed.extend(misses(&temperatures, row, &[("T", t)]));
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Each `if97` row's pair and inputs: PT (x = p, y = T), QT (x = Q, y = T) or PQ (x = p, y = Q).
fn if97_row<'a>(table: &Fixture<'a>, row: usize) -> (&'a str, f64, f64) {
    let pair = match table.rows()[row].cells.first() {
        Some(Cell::Text(pair)) => *pair,
        _ => "",
    };
    (pair, table.value(row, "x").unwrap(), table.value(row, "y").unwrap())
}

/// `got` against the row's `column` within `Prop`, 1e-12 of the larger of the value and `floor` (VERIFICATION.md §5:
/// R·T for an energy, R for an entropy or a heat capacity, in the row's mass units; 0 elsewhere): the miss, named.
fn prop_miss(table: &Fixture<'_>, row: usize, (column, got): (&str, f64), floor: f64) -> Option<String> {
    let want = table.value(row, column).unwrap();
    let bound = ToleranceClass::Prop.bound(want.abs().max(floor)).unwrap();
    let error = (got - want).abs();
    (error.is_nan() || error > bound).then(|| format!("row {row} {column}: {got} against {want}"))
}

/// Oracle: CoolProp 8.0.0's `IF97::Water`, `fixtures/coolprop-8.0.0/all/if97.csv` (PLAN.md M14.2). Region 1's PT rows,
/// those at or above Eq. (30)'s saturation pressure, give the oracle's ρ, h, s, u, c_p and w within `Prop`; the QT and
/// PQ rows give its saturation pressure and temperature. A row the oracle refuses is QT at 273.15 K, below its
/// saturation line's 273.16 K, where Eq. (30) is defined (§8.1: it extrapolates to 273.15 K). Region 2's rows and the
/// saturated states are M14.3's.
#[test]
fn region_1_and_the_saturation_line_match_the_oracle() {
    let table = table(fixture!("coolprop-8.0.0/all/if97.csv"));
    let (mut missed, mut counts) = (Vec::new(), [0; 4]);
    for row in 0..table.rows().len() {
        let (pair, x, y) = if97_row(&table, row);
        if table.printed(row, "status") != Some("ok") {
            assert_eq!((pair, y), ("QT", 273.15), "row {row}: the oracle refuses");
            counts[3] += 1;
            continue;
        }
        match pair {
            "PT" if x >= if97::saturation_pressure(y).unwrap() => {
                let state = if97::region1(y, x).unwrap();
                let (energy, entropy) = (if97::R * y, if97::R);
                let values = [
                    (("rhomass", state.rho(Basis::Mass)), 0.0),
                    (("hmass", state.h(Basis::Mass)), energy),
                    (("smass", state.s(Basis::Mass)), entropy),
                    (("umass", state.u(Basis::Mass)), energy),
                    (("cpmass", state.cp(Basis::Mass).unwrap()), entropy),
                    (("speed_sound", state.speed_of_sound().unwrap()), 0.0),
                ];
                missed.extend(values.into_iter().filter_map(|(value, floor)| prop_miss(&table, row, value, floor)));
                counts[0] += 1;
            }
            "QT" => {
                missed.extend(prop_miss(&table, row, ("p", if97::saturation_pressure(y).unwrap()), 0.0));
                counts[1] += 1;
            }
            "PQ" => {
                missed.extend(prop_miss(&table, row, ("T", if97::saturation_temperature(x).unwrap()), 0.0));
                counts[2] += 1;
            }
            _ => {}
        }
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!(counts, [58, 20, 12, 2], "region 1's PT rows, QT rows, PQ rows, refusals");
}

/// Arbiter: IAPWS R7-97(2012) Table 15, region 2's v, h, u, s, c_p and w at (300 K, 3.5 kPa), (700 K, 3.5 kPa) and
/// (700 K, 30 MPa) (PLAN.md M14.3): the region's function, which the `Fluid` uses only to 623.15 K.
#[test]
fn region_2_is_table_15() {
    let table = table(fixture!("paper/Water/IAPWS-R7-97-2012.15.csv"));
    let mut missed = Vec::new();
    for row in 0..table.rows().len() {
        let (t, p) = (table.value(row, "T").unwrap(), table.value(row, "p").unwrap() * 1e6);
        match if97::region2(t, p) {
            Ok(state) => missed.extend(misses(&table, row, &printed_properties(&state))),
            Err(e) => missed.push(format!("row {row}: {e}")),
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// IF97 water's `Fluid` (PLAN.md M14.3; user decision HA4).
fn water() -> Fluid {
    if97::water().unwrap()
}

/// The `if97` row's input for IF97 water's `Fluid`.
fn if97_input(pair: &str, x: f64, y: f64) -> Input {
    let pair = match pair {
        "PT" => Pair::PT,
        "QT" => Pair::QT,
        _ => Pair::PQ,
    };
    Input::new(pair, x, y, Basis::Mass).unwrap()
}

/// Oracle: CoolProp 8.0.0's `IF97::Water`, `fixtures/coolprop-8.0.0/all/if97.csv` (PLAN.md M14.3). Every row through
/// IF97 water's `Fluid`: T, p, ρ, h, s, u, c_p and w within `Prop` with §5's floors, region 2's PT rows and the
/// saturated sides (Q 0 and 1: c_p and w the saturated phase's) among them, DIV-0065's saturated liquid at 623.15 K at
/// its registered 5e-12 in w. The oracle refuses QT at 273.15 K, below its saturation line; the `Fluid` answers there
/// (two oracle-gap rows).
#[test]
fn if97_water_matches_the_oracle() {
    let (table, water) = (table(fixture!("coolprop-8.0.0/all/if97.csv")), water());
    let (mut missed, mut asserted, mut gaps, mut exempt) = (Vec::new(), 0, 0, 0);
    for row in 0..table.rows().len() {
        let (pair, x, y) = if97_row(&table, row);
        let state = water.state(if97_input(pair, x, y));
        if table.printed(row, "status") != Some("ok") {
            assert!(state.is_ok(), "row {row}: {state:?}");
            gaps += 1;
            continue;
        }
        let state = state.unwrap();
        let t = state.t();
        let (energy, entropy) = (if97::R * t, if97::R);
        let values = [
            (("T", t), 0.0),
            (("p", state.p()), 0.0),
            (("rhomass", state.rho(Basis::Mass)), 0.0),
            (("hmass", state.h(Basis::Mass)), energy),
            (("smass", state.s(Basis::Mass)), entropy),
            (("umass", state.u(Basis::Mass)), energy),
            (("cpmass", state.cp(Basis::Mass).unwrap_or(f64::NAN)), entropy),
            (("speed_sound", state.speed_of_sound().unwrap_or(f64::NAN)), 0.0),
        ];
        for ((column, got), floor) in values {
            let p = table.value(row, "p");
            let key = RowKey { t, input: Some(pair), two_phase: false, p, balanced: true, near_critical: false };
            if exempt_row(DIVERGENCES, "IF97 water", Kind::If97, column, key).is_some() {
                let want = table.value(row, column).unwrap();
                assert!((got / want - 1.0).abs() <= 5e-12, "DIV-0065: row {row} {column}: {got} against {want}");
                exempt += 1;
                continue;
            }
            missed.extend(prop_miss(&table, row, (column, got), floor));
        }
        asserted += 1;
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!((asserted, gaps, exempt), (120, 2, 2), "rows asserted, oracle gaps, cells exempt (w at Q 0 and 1)");
}

/// The conformance kit (VERIFICATION.md §10) on IF97 water (PLAN.md M14.3): its two truths, a liquid at (300 K, 1 MPa)
/// and the dome at (Q 0.4, 400 K), round-trip through PT, QT and PQ, the rest of the 19 pairs `Unsupported`; a
/// single phase has no quality, so its quality pairs are refused before the model is asked, and PT on the line is
/// `Ambiguous` between the two sides, neither of which is the dome.
#[test]
fn if97_water_capability_matrix() {
    let water = water();
    let single = water.state(if97_input("PT", 1e6, 300.0)).unwrap();
    let dome = water.state(if97_input("QT", 0.4, 400.0)).unwrap();
    for (pair, [one_phase, two_phase]) in capability_matrix(&water, [&single, &dome]) {
        let declared = matches!(pair, Pair::PT | Pair::QT | Pair::PQ);
        let q_pair = matches!(pair.vars(), (Var::Q, _) | (_, Var::Q));
        let one_ok = match &one_phase {
            Outcome::Pass => declared && !q_pair,
            Outcome::Unsupported => !declared && !q_pair,
            Outcome::Refused(Error::Undefined { .. }) => q_pair,
            _ => false,
        };
        let two_ok = match &two_phase {
            Outcome::Pass => declared && q_pair,
            Outcome::Unsupported => !declared,
            Outcome::Refused(Error::Ambiguous { .. }) => pair == Pair::PT,
            _ => false,
        };
        assert!(one_ok && two_ok, "{pair:?}: {one_phase:?}, {two_phase:?}");
    }
}

/// The states the kit's other checks run on: liquid and vapour from region 1 and 2 across the `Fluid`'s range, and
/// both saturated sides and the dome at 450 K.
fn kit_inputs() -> Vec<Input> {
    let mut inputs = Vec::new();
    for t in [273.16, 300.0, 400.0, 500.0, 600.0, 623.15] {
        inputs.extend([if97_input("PT", 1e3, t), if97_input("PT", 50e6, t)]);
    }
    inputs.extend([0.0, 0.5, 1.0].map(|q| if97_input("QT", q, 450.0)));
    inputs
}

/// The kit's identities (`Identity`) and gauge invariance on IF97 water (PLAN.md M14.3).
#[test]
fn if97_water_passes_the_kit() {
    let fluid = water();
    let states: Vec<State> = kit_inputs().into_iter().map(|input| fluid.state(input).unwrap()).collect();
    let checked = identities(&fluid, &states).unwrap();
    assert!(checked >= 12 * 10, "the first-order identities at every single-phase state: {checked}");
    let gauged = gauge_invariance(&fluid, Gauge::new(1234.5, -6.75).unwrap(), &kit_inputs());
    assert!(gauged.is_ok(), "{gauged:?}");
}

/// The kit's thread invariance on IF97 water (PLAN.md M14.3): eight threads sharing one fluid get a lone fluid's states
/// bit for bit.
#[cfg(not(target_family = "wasm"))] // wasip2 has no thread spawning
#[test]
fn if97_water_is_thread_invariant() {
    use phasekit_verify::conformance::thread_invariance;
    let threads = thread_invariance(water, &kit_inputs(), 8);
    assert!(threads.is_ok(), "{threads:?}");
}

/// K17: PT, QT and PQ batches of IF97 water under `Parallel` and `Simd` give `Reference`'s cells bit for bit, its
/// refusals (below 273.15 K, above 623.15 K or 100 MPa) among them (PLAN.md M14.3).
#[test]
fn if97_water_batches_agree_under_every_policy() {
    let water = water();
    let outputs = [Prop::P, Prop::Hmass, Prop::Cpmass, Prop::Q];
    let t: Vec<f64> = (0..37).map(|i| 270.0 + 10.0 * f64::from(i)).collect();
    let p: Vec<f64> = (0..37).map(|i| 1e3 * math::powi(1.4, i)).collect();
    let q: Vec<f64> = (0..37).map(|i| f64::from(i % 5) / 4.0).collect();
    let requests = [
        BatchRequest::new(Pair::PT, Basis::Mass, &p, &t, &outputs),
        BatchRequest::new(Pair::QT, Basis::Mass, &q, &t, &outputs),
        BatchRequest::new(Pair::PQ, Basis::Mass, &p, &q, &outputs),
    ];
    for exec in [ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() }, ExecPolicy::Simd] {
        for req in &requests {
            let result = policy_equivalence(&water, req, exec);
            assert!(result.is_ok(), "{exec:?}: {result:?}");
        }
    }
}

/// ROT-242, region 2's half: v8.0.0's IF97 refuses p below 611.213 Pa (its PT rows start at 1 kPa), though region 2
/// is defined for any p > 0 (R7-97(2012) §6.1); IF97 water's vapour at 300 K and 100 Pa is region 2's state, its
/// compressibility factor 1 − 5.8e-5, the second virial's share there.
#[test]
fn if97_water_vapour_goes_below_611_pa() {
    let state = water().state(if97_input("PT", 100.0, 300.0)).unwrap();
    assert_eq!(state.phase(), Phase::Gas);
    assert_eq!(state.rho(Basis::Mass).to_bits(), if97::region2(300.0, 100.0).unwrap().rho(Basis::Mass).to_bits());
    let z = state.z();
    assert!(-1e-4 < z - 1.0 && z < 1.0, "Z = {z}");
}
