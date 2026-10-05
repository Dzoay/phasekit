//! Committed oracle fixtures read back (VERIFICATION.md §3): the M1 gate "fixture round trip bit-exact".

use phasekit_verify::{Cell, Fixture, Provenance, fixture};

/// The `facts` row named `name`: its status and value.
fn fact<'a>(fixture: &Fixture<'a>, name: &str) -> Option<(&'a str, f64)> {
    let row = fixture.rows().iter().position(|row| row.cells.first() == Some(&Cell::Text(name)))?;
    match fixture.rows()[row].cells.get(2)? {
        Cell::Text(status) => Some((status, fixture.value(row, "value")?)),
        Cell::Num(_) | Cell::Blank => None,
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

/// VERIFICATION.md §3.2, §11.3 (PLAN.md M1.16): oracle fixtures are generated in one image, pinned by digest in the
/// lock, and every committed oracle fixture names the same generator environment (Python and libc), so a file made
/// elsewhere cannot slip in beside the others.
#[test]
fn oracle_lock_pins_the_runner_image() {
    let lock = phasekit_verify::OracleLock::parse(phasekit_verify::ORACLE_LOCK).unwrap();
    let image = lock.get("runner_image").unwrap_or_default();
    let digest = image.rsplit_once("@sha256:").map_or("", |(_, digest)| digest);
    assert!(digest.len() == 64 && digest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')), "`{image}`");
    let environment = |(path, text): (&str, &str)| {
        let generator = Fixture::parse(path, text).unwrap().header("generator").unwrap_or_default();
        generator.split(' ').filter(|field| !field.starts_with("sha256=")).collect::<Vec<_>>().join(" ")
    };
    let smoke = environment(fixture!("coolprop-8.0.0/facts/smoke.csv"));
    assert!(smoke.contains(" python=3.12.") && smoke.contains(" libc=glibc-"), "{smoke}");
    assert_eq!(environment(fixture!("coolprop-8.0.0/facts/register.csv")), smoke);
}
