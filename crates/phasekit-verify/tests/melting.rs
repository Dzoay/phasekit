//! Melting curves (PLAN.md M8.11; map 02 §3.7, §6): against the oracle's `melt` fixtures, and HeavyWater's against
//! Herrig et al. 2018's check values. Water's against IAPWS R14-08 is DIV-0002's proof (`tests/divergences.rs`).

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::BTreeSet;

use phasekit_core::{DataSet, Error, Registry};
use phasekit_verify::arbiters::table_text;
use phasekit_verify::{Fixture, fixture};

/// The oracle's melting files: the all-fluid tier (4 rows per segment) and the core subset's (50 rows per segment).
const MELT_FIXTURES: [(&str, &str); 7] = [
    fixture!("coolprop-8.0.0/all/melt.csv"),
    fixture!("coolprop-8.0.0/melt/Air.csv"),
    fixture!("coolprop-8.0.0/melt/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/melt/Helium.csv"),
    fixture!("coolprop-8.0.0/melt/Methanol.csv"),
    fixture!("coolprop-8.0.0/melt/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/melt/Water.csv"),
];

/// Whether `got`, a melting curve's value or its `Ambiguous` pressures, each times `scale` (the table's unit), holds
/// `table`'s `column` at `row` in its class (an oracle row's `Prop` at its own magnitude: each form is evaluated in
/// CoolProp's operation order, so even a Simon fit's p₀ + a((T/T₀)^c − 1) near T₀ agrees): one of the pressures where
/// two branches hold T.
fn holds(table: &Fixture<'_>, row: usize, column: &str, got: Result<f64, Error>, scale: f64) -> Result<(), String> {
    let check = |value: f64| match table.value(row, column) {
        Some(want) if table.printed(row, "status").is_some() => {
            table.check_scaled(row, column, value, want.abs()).map(drop)
        }
        _ => table.check(row, column, value),
    };
    match got {
        Ok(value) => check(value * scale).map_err(|e| format!("{e:?}")),
        Err(Error::Ambiguous { roots }) => {
            let any = roots.as_slice().iter().any(|value| check(value * scale).is_ok());
            if any { Ok(()) } else { Err(format!("none of {roots:?}")) }
        }
        Err(e) => Err(format!("{e}")),
    }
}

/// PLAN.md M8.11 (VERIFICATION.md §3.5), class `Prop`: under `Parity` every melting curve against CoolProp 8.0.0, the
/// 30 fluids' 39 segments at 2 temperatures and 2 pressures each (the all-fluid tier) and the core subset's six at 25 and
/// 25 per segment: p_melt(T), one of its pressures where two branches hold T (CoolProp's is the first segment in file
/// order: ice Ih's on Water's ice III and V ranges), and T_melt(p). Every row lies inside its segment, where the oracle
/// answers; no row is refused.
#[test]
fn melting_matches_oracle() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut rows, mut fluids, mut failures) = (0, BTreeSet::new(), Vec::new());
    for (path, text) in MELT_FIXTURES {
        let table = Fixture::parse(path, text).unwrap();
        let file_fluid = path.rsplit('/').next().unwrap().trim_end_matches(".csv");
        for row in 0..table.rows().len() {
            let name = table.printed(row, "fluid").unwrap_or(file_fluid);
            let model = registry.get(name).unwrap().model();
            let (t, p) = (table.value(row, "T").unwrap(), table.value(row, "p").unwrap());
            let checked = match (table.printed(row, "status"), table.printed(row, "input")) {
                (Some("ok"), Some("T")) => holds(&table, row, "p", model.melting_pressure(t), 1.0),
                (Some("ok"), _) => holds(&table, row, "T", model.melting_temperature(p), 1.0),
                (status, _) => Err(format!("the oracle refuses this row: {status:?}")),
            };
            failures.extend(checked.err().map(|e| format!("{path}:{row} {name} T {t} p {p}: {e}")));
            fluids.insert(name);
            rows += 1;
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!((rows, fluids.len()), (39 * 4 + (1 + 1 + 1 + 1 + 1 + 4) * 50, 30));
}

/// Herrig et al. 2018 (HeavyWater's EOS paper, Sec. 3.4; map 10 §8.1), class `Paper`: its four melting check values,
/// on ice Ih, III, V and VI, are HeavyWater's p_melt to their nine printed digits (on ice Ih's and ice V's ranges, one
/// of the curve's two pressures at that T), under both data sets: no correction touches it.
#[test]
fn heavy_water_melting_herrig() {
    let path = "paper/HeavyWater/Herrig-JPCRD-2019.S3-4.csv";
    let table = Fixture::parse(path, table_text(path).unwrap()).unwrap();
    assert_eq!(table.rows().len(), 4);
    for set in [DataSet::Parity, DataSet::Corrected] {
        let registry = Registry::from_embedded(set).unwrap();
        let model = registry.get("HeavyWater").unwrap().model();
        for row in 0..4 {
            let t = table.value(row, "T").unwrap();
            assert_eq!(holds(&table, row, "p", model.melting_pressure(t), 1e-6), Ok(()), "{set:?} row {row} at {t} K");
        }
    }
}
