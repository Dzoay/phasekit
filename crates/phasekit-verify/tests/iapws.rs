//! IAPWS water and ice through `phasekit-iapws` (PLAN.md M14; user decisions HA1, HA4): each release's check table,
//! within its printed digits (`Paper`), the values converted to the table's units.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{Basis, State};
use phasekit_iapws::if97;
use phasekit_verify::{Cell, Fixture, ToleranceClass, fixture};

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
