//! L4 flash (VERIFICATION.md §3.5 `flash`, §8.2): pairs read off truth states, never density bands (map 12 §6.4),
//! against CoolProp's flash of the same inputs. Since M5.3 the DT pair, below the critical temperature included.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Registry};
use phasekit_verify::flash::FlashCheck;
use phasekit_verify::{Fixture, fixture};

/// The core subset's `flash` files (VERIFICATION.md §3.6).
const FLASH_CORE: [(&str, &str); 14] = [
    fixture!("coolprop-8.0.0/flash/Air.csv"),
    fixture!("coolprop-8.0.0/flash/Ammonia.csv"),
    fixture!("coolprop-8.0.0/flash/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/flash/HFE143m.csv"),
    fixture!("coolprop-8.0.0/flash/Helium.csv"),
    fixture!("coolprop-8.0.0/flash/Methanol.csv"),
    fixture!("coolprop-8.0.0/flash/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/flash/R1130(E).csv"),
    fixture!("coolprop-8.0.0/flash/R1234yf.csv"),
    fixture!("coolprop-8.0.0/flash/R1234ze(E).csv"),
    fixture!("coolprop-8.0.0/flash/R125.csv"),
    fixture!("coolprop-8.0.0/flash/R410A.csv"),
    fixture!("coolprop-8.0.0/flash/Water.csv"),
    fixture!("coolprop-8.0.0/flash/n-Heptane.csv"),
];

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/flash/<Fluid>.csv, the DT rows (PLAN.md M5.3): (ρ, T) read off 6 × 6
/// (p, T) and 4 × 4 (T, Q) truth states per core fluid, flashed with no phase imposed. The phase label, Q (class
/// `Flash`, 1e-8 absolute) and p, h, s, u (class `Flash`, 1e-9 of max(|v|, floor); for a single-phase state with `Term`
/// carried through the relation, user decision TC1) match; inside the dome the state is the lever rule on the
/// superancillary's densities (`State::from_split`). CoolProp has no QT at 0 < Q < 1 for the pseudo-pure Air and R410A,
/// so their (T, Q) cells are `dome` truths, ρ between their QT densities (M6.9): inside their dome the state is the
/// pure VLE of their EOS, as CoolProp's.
#[test]
fn dt_two_phase_matches_oracle() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = FlashCheck::default();
    for (path, text) in FLASH_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        check.dt_rows(&fixture, registry.get(name).unwrap(), &record);
    }
    assert_eq!(check.report(20), None);
    // 14 × 36 PT truths and 14 × 16 QT truths, the pseudo-pure Air's and R410A's `dome` truths (M6.9), of which two
    // of Air's are gas.
    assert_eq!((check.two_phase, check.compared), (12 * 16 + 30, 14 * 36 + 14 * 16));
}

/// VERIFICATION.md §3.2, §11.3: the `flash` files come from the pinned runner image, like every committed oracle
/// fixture.
#[test]
fn flash_fixtures_name_the_pinned_generator_environment() {
    let environment = |(path, text): (&str, &str)| {
        let generator = Fixture::parse(path, text).unwrap().header("generator").unwrap_or_default();
        generator.split(' ').filter(|field| !field.starts_with("sha256=")).collect::<Vec<_>>().join(" ")
    };
    let smoke = environment(fixture!("coolprop-8.0.0/facts/smoke.csv"));
    for file in FLASH_CORE {
        assert_eq!(environment(file), smoke, "{}", file.0);
    }
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/all/flash.csv (PLAN.md M6.9; D4, user decision 4; map 04 U4): the
/// six pseudo-pure fluids' `dome` rows, (ρ, T) between their QT densities at 4 × 4 cell centres of T and Q, flashed with
/// no phase imposed. Inside its dome CoolProp solves the pure-fluid VLE of the blend's EOS from the density ancillaries
/// (`HelmholtzEOSMixtureBackend.cpp:2447-2507`), as phasekit does: phase, Q, p, h, s and u as `dt_rows` checks them.
#[test]
fn pseudo_pure_in_dome_dt_follows_the_oracle_rule() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (path, text) = fixture!("coolprop-8.0.0/all/flash.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let mut check = FlashCheck::default();
    for name in ["Air", "R404A", "R407C", "R410A", "R507A", "SES36"] {
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let of = |row: usize| fixture.printed(row, "fluid") == Some(name);
        check.dt_rows_where(&fixture, name, (registry.get(name).unwrap(), &record), of);
    }
    assert_eq!(check.report(20), None);
    assert_eq!((check.compared, check.two_phase), (96, 92));
}
