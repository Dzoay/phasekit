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
    assert_eq!(environment(fixture!("mp/check-points.csv")), smoke);
}

/// PLAN.md M1.17 (map 09 §8, map 10 §8.1): the superancillary check points of CoolProp's fluid files, 3 for each of
/// the 130 fluids with a superancillary, picked from fastchebpure's dense grid at Θ = (T_c − T)/T_c = 0.5, 0.3, 0.1 (a
/// point below the triple point moves up to it: CarbonDioxide's and SulfurHexafluoride's first two coincide); every
/// value finite and positive, ρ′ > ρ″, p not falling with T, and the SA/mp ratios within 1e-8 of 1.
#[test]
fn check_points_are_390_and_well_formed() {
    let (path, text) = fixture!("mp/check-points.csv");
    let points = Fixture::parse(path, text).unwrap();
    assert_eq!(points.kind(), "checkpoints");
    assert_eq!(points.provenance(), Provenance::MultiPrecision { source: "coolprop-json" });
    let columns = ["fluid", "Tc", "T", "p", "rhoL", "rhoV", "p_sa_mp", "rhoL_sa_mp", "rhoV_sa_mp"];
    assert_eq!((points.columns(), points.rows().len()), (&columns[..], 390));
    let value = |row: usize, column: &str| points.value(row, column).unwrap_or(f64::NAN);
    let near = |theta: f64, target: f64| (theta - target).abs() <= 3e-3;
    let mut fluids = Vec::new();
    for first in (0..390).step_by(3) {
        let Some(Cell::Text(fluid)) = points.rows()[first].cells.first() else { panic!("row {first}: no fluid") };
        let rows = first..first + 3;
        assert!(rows.clone().all(|row| points.rows()[row].cells.first() == Some(&Cell::Text(fluid))), "{fluid}");
        assert!(rows.clone().all(|row| value(row, "Tc") == value(first, "Tc")), "{fluid}: one Tc");
        let theta = |k: usize| 1.0 - value(first + k, "T") / value(first + k, "Tc");
        let (t0, t1, t2) = (theta(0), theta(1), theta(2));
        let clamped = t1 == t0 && t0 < 0.3;
        assert!(
            near(t2, 0.1) && (near(t1, 0.3) || clamped) && (near(t0, 0.5) || t0 < 0.5) && t0 >= t1,
            "{fluid}: Θ {t0} {t1} {t2}"
        );
        for row in rows {
            let [p, l, v] = ["p", "rhoL", "rhoV"].map(|c| value(row, c));
            assert!(p > 0.0 && v > 0.0 && l > v && p.is_finite() && l.is_finite(), "{fluid} row {row}");
            let ratios = ["p_sa_mp", "rhoL_sa_mp", "rhoV_sa_mp"].map(|c| value(row, c));
            assert!(ratios.iter().all(|r| (r - 1.0).abs() < 1e-8), "{fluid} row {row}: {ratios:?}");
        }
        assert!(value(first, "p") <= value(first + 1, "p") && value(first + 1, "p") < value(first + 2, "p"), "{fluid}");
        fluids.push(*fluid);
    }
    let mut unique = fluids.clone();
    unique.dedup();
    assert_eq!((fluids.len(), unique.len()), (130, 130), "130 fluids, each once");
    assert!(fluids.windows(2).all(|w| w[0] < w[1]), "sorted by name");
}
