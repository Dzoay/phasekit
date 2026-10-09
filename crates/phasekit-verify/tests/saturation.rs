//! The superancillary and the VLE against fastchebpure's dense multiprecision saturation files (PLAN.md M6.4; map 10
//! §8.1, §8.3; user decisions SV1, NC1, DP1): the core subset's committed files on every PR, every fluid's in the
//! nightly sweep.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Registry};
use phasekit_verify::saturation::SaturationCheck;
use phasekit_verify::{Fixture, fixture};

/// The core subset's fluids with a v8.0.0 superancillary, their committed files (tests/fastchebpure.rs).
const FILES: [(&str, &str, &str); 12] = [
    (
        "Ammonia",
        fixture!("mp/fastchebpure-2026.06.02-v2/Ammonia.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/Ammonia.csv").1,
    ),
    (
        "CarbonDioxide",
        fixture!("mp/fastchebpure-2026.06.02-v2/CarbonDioxide.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/CarbonDioxide.csv").1,
    ),
    (
        "HFE143m",
        fixture!("mp/fastchebpure-2026.06.02-v2/HFE143m.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/HFE143m.csv").1,
    ),
    (
        "Helium",
        fixture!("mp/fastchebpure-2026.06.02-v2/Helium.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/Helium.csv").1,
    ),
    (
        "Methanol",
        fixture!("mp/fastchebpure-2026.06.02-v2/Methanol.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/Methanol.csv").1,
    ),
    (
        "Nitrogen",
        fixture!("mp/fastchebpure-2026.06.02-v2/Nitrogen.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/Nitrogen.csv").1,
    ),
    (
        "R1130(E)",
        fixture!("mp/fastchebpure-2026.06.02-v2/R1130(E).csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/R1130(E).csv").1,
    ),
    (
        "R1234yf",
        fixture!("mp/fastchebpure-2026.06.02-v2/R1234yf.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/R1234yf.csv").1,
    ),
    (
        "R1234ze(E)",
        fixture!("mp/fastchebpure-2026.06.02-v2/R1234ze(E).csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/R1234ze(E).csv").1,
    ),
    (
        "R125",
        fixture!("mp/fastchebpure-2026.06.02-v2/R125.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/R125.csv").1,
    ),
    (
        "Water",
        fixture!("mp/fastchebpure-2026.06.02-v2/Water.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/Water.csv").1,
    ),
    (
        "n-Heptane",
        fixture!("mp/fastchebpure-2026.06.02-v2/n-Heptane.csv").0,
        fixture!("mp/fastchebpure-2026.06.02-v2/n-Heptane.csv").1,
    ),
];

/// The core subset's rows below Θ = 1e-8 (Θ = 0 at the SA's own end excluded).
const NEAR_CRITICAL: usize = 116;

/// The nightly set's root, fixed at compile time (clippy bans run-time environment reads, D17).
const FULL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures-full/mp/fastchebpure-2026.06.02-v2");

/// Checks one fluid's file.
fn check(check: &mut SaturationCheck, registry: &Registry, name: &str, path: &str, text: &str) {
    let fixture = Fixture::parse(path, text).unwrap();
    let record = phasekit_core::internal::record(registry, name).unwrap();
    let fluid = record.clone().compile().unwrap();
    check.rows(&fixture, name, &record, fluid.eos());
}

/// Arbiter: fastchebpure 2026.06.02-v2 (NIST; Bell 2024, JPCRD 53), the dense multiprecision saturation of the core
/// subset's 12 fluids with a superancillary, 18 875 rows down to Θ = 0. Each row's superancillary is within class
/// `SaFit` of the multiprecision p, ρ′ and ρ″ (4 × the misfit the file records); the VLE at the row's T, seeded with
/// the superancillary's densities, is within `SatMp`, max(1e-11, 4·ε·Θ^−1.5) in ρ and max(1e-11, 4·ε/Θ) in p (NC1),
/// p derived from the row's (T, ρ″) (DP1); below Θ = 1e-8 it is not compared (`SatMp`'s bound exceeds 1e-3 of ρ
/// there). Its measured constants, against ε·Θ^−1.5·μ and ε·μ/Θ (NC2), stay at 0.25 and 0.27 (here at most 0.5).
#[test]
fn superancillary_and_vle_match_the_dense_multiprecision_files() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut saturation = SaturationCheck::default();
    for (name, path, text) in FILES {
        check(&mut saturation, &registry, name, path, text);
    }
    assert_eq!(saturation.report(20), None);
    assert_eq!(saturation.superancillary + saturation.beyond, 18_875);
    assert_eq!(saturation.vle + saturation.near_critical + saturation.unsplit, saturation.superancillary);
    assert_eq!((saturation.near_critical, saturation.unsplit), (NEAR_CRITICAL, 0), "{saturation:?}");
    let [density, pressure] = saturation.conditioning;
    assert!(density <= 0.5 && pressure <= 0.5, "measured conditioning {density:.2}, {pressure:.2}");
}

/// The same over every fluid with a superancillary, from the nightly's `cargo xtask fetch-fastchebpure --all`
/// (PLAN.md M6.4, the M6 exit gate's nightly part; 204 050 rows): the measured constants at most 0.39 and 0.38 (here
/// 1); Chlorine's last row unsplit (Θ = 2.45e-7, the file's ρ′ = ρ″ between rows with a 5 % split); PropyleneGlycol's
/// p and ρ″ from 213 K to 227.6028 K exempt (DIV-0016, 52 cells). A missing file fails the sweep, never skips it.
#[test]
#[ignore = "nightly: reads every fluid's fastchebpure file (cargo xtask fetch-fastchebpure --all)"]
#[allow(clippy::disallowed_methods, reason = "the nightly sweep reads the full set, too big to compile in (§3.6)")]
fn superancillary_and_vle_match_every_dense_multiprecision_file() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut saturation = SaturationCheck::default();
    let mut fluids = 0;
    for f in phasekit_data::FLUIDS {
        let record = phasekit_core::internal::record(&registry, f.name).unwrap();
        if record.superancillary_curve().is_none() {
            continue;
        }
        let path = format!("{FULL}/{}.csv", f.name);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}")).unwrap();
        check(&mut saturation, &registry, f.name, &path, &text);
        fluids += 1;
    }
    assert_eq!(fluids, 130);
    assert_eq!(saturation.report(20), None);
    assert_eq!((saturation.unsplit, saturation.exempt), (1, 52), "Chlorine's last row; DIV-0016");
    let [density, pressure] = saturation.conditioning;
    assert!(density <= 1.0 && pressure <= 1.0, "measured conditioning {density:.2}, {pressure:.2}");
}
