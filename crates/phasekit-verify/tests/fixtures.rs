//! Committed oracle fixtures read back (VERIFICATION.md §3): the M1 gate "fixture round trip bit-exact".

use phasekit_verify::{Cell, Fixture, Provenance, fixture};

/// The `facts` row named `name`: its status and value.
fn fact<'a>(fixture: &Fixture<'a>, name: &str) -> Option<(&'a str, f64)> {
    let row = fixture.rows().iter().position(|row| row.cells.first() == Some(&Cell::Text(name)))?;
    match fixture.rows()[row].cells.get(2)? {
        Cell::Text(status) => Some((status, fixture.value(row, "value")?)),
        Cell::Num(_) => None,
    }
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/facts/smoke.csv. The values map 11 §8 and map 01 §8 measured,
/// read back bit for bit.
#[test]
fn oracle_smoke_round_trip() {
    let (path, text) = fixture!("coolprop-8.0.0/facts/smoke.csv");
    let smoke = Fixture::parse(path, text).unwrap();
    assert_eq!((smoke.kind(), smoke.provenance()), ("facts", Provenance::Oracle { version: "8.0.0" }));
    assert_eq!(fact(&smoke, "r134a_h_t300_q1"), Some(("ok", 413265.6843372975)));
    assert_eq!(fact(&smoke, "water_tcrit"), Some(("ok", 647.0959999999873)));
    assert_eq!(fact(&smoke, "water_t_reducing"), Some(("ok", 647.096)));
    let lock = phasekit_verify::OracleLock::parse(phasekit_verify::ORACLE_LOCK).unwrap();
    let oracle = smoke.header("oracle").unwrap();
    assert!(oracle.contains(lock.get("git").unwrap()) && oracle.contains(lock.get("so_sha256").unwrap()));
}
