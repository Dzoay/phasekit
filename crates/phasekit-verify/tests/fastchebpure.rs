//! fastchebpure's dense multiprecision saturation files (PLAN.md M6.1; VERIFICATION.md §3.2, §3.7; user decision 3b):
//! the committed core-subset conversions are the pinned release's, and they agree with the 390 check points CoolProp
//! ships in its fluid files.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_verify::{Cell, Fixture, fixture, sha256_hex};

/// The core subset's fluids with a v8.0.0 superancillary (VERIFICATION.md §3.6; Air and R410A are pseudo-pure).
const FILES: [(&str, &str); 12] = [
    fixture!("mp/fastchebpure-2026.06.02-v2/Ammonia.csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/CarbonDioxide.csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/HFE143m.csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/Helium.csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/Methanol.csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/Nitrogen.csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/R1130(E).csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/R1234yf.csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/R1234ze(E).csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/R125.csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/Water.csv"),
    fixture!("mp/fastchebpure-2026.06.02-v2/n-Heptane.csv"),
];

/// The pin: tag, URL, the zip's sha256 and one sha256 per converted file (`cargo xtask fetch-fastchebpure`).
const LOCK: &str = include_str!("../fixtures/mp/fastchebpure.lock");

/// The text in `column` of `row`.
fn label<'a>(fixture: &Fixture<'a>, row: usize, column: &str) -> &'a str {
    let i = fixture.columns().iter().position(|c| *c == column).unwrap();
    match fixture.rows()[row].cells[i] {
        Cell::Text(text) => text,
        Cell::Num(_) | Cell::Blank => "",
    }
}

/// Arbiter: fastchebpure 2026.06.02-v2 (NIST; Bell 2024, JPCRD 53), the release CoolProp v8.0.0 pins
/// (`Web/scripts/fluid_properties.Superancillary.py:15-18`). Each committed file is the one the lock names, by sha256;
/// it says which release and which EOS it comes from (the hash `cargo xtask fetch-fastchebpure` checked against the
/// fluid's v8.0.0 `source_eos_hash`; tested in xtask, which reads the fluid files); and its rows at the temperatures
/// of `mp/check-points.csv` are those check points bit for bit, ratios included: the 36 points of these 12 fluids
/// come from the same multiprecision run.
#[test]
fn fastchebpure_files_match_the_v8_eos() {
    let (path, text) = fixture!("mp/check-points.csv");
    let check_points = Fixture::parse(path, text).unwrap();
    let mut matched = 0;
    for (path, text) in FILES {
        let name = path.rsplit('/').next().unwrap();
        let line = LOCK.lines().find(|l| l.split_whitespace().nth(1) == Some(name)).unwrap_or_default();
        assert_eq!(line.split_whitespace().nth(2), Some(sha256_hex(text.as_bytes()).as_str()), "{path}");
        let dense = Fixture::parse(path, text).unwrap();
        assert_eq!(dense.header("source"), Some("mp:fastchebpure@2026.06.02-v2"), "{path}");
        assert_eq!(dense.columns(), check_points.columns(), "{path}: the layout of mp/check-points.csv");
        let fluid = dense.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        assert!(dense.header("fluid").is_some_and(|f| f.contains(" source_eos_hash=")), "{path}");
        assert!(dense.rows().len() > 1_000, "{path}: {} rows", dense.rows().len());
        for point in (0..check_points.rows().len()).filter(|&r| label(&check_points, r, "fluid") == fluid) {
            let t = check_points.value(point, "T").unwrap();
            let row = (0..dense.rows().len()).find(|&r| dense.value(r, "T") == Some(t));
            let row = row.unwrap_or_else(|| panic!("{path}: no dense row at the check point T = {t} K"));
            for column in ["Tc", "p", "rhoL", "rhoV", "p_sa_mp", "rhoL_sa_mp", "rhoV_sa_mp"] {
                let (got, want) = (dense.value(row, column).unwrap(), check_points.value(point, column).unwrap());
                assert_eq!(got.to_bits(), want.to_bits(), "{path} at {t} K, {column}: {got} against {want}");
            }
            matched += 1;
        }
    }
    assert_eq!(matched, 12 * 3);
    assert!(LOCK.lines().any(|l| l.starts_with("zip_sha256 ")), "{LOCK}");
}
