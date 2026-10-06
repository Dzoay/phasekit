//! L3 oracle fixtures, kind `term` (VERIFICATION.md §3.5): each residual block of the core subset, isolated in
//! CoolProp with `add_fluids_as_JSON` (map 10 §8.2), against the same block compiled from the `Parity` record.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::{EosRecord, FluidRecord};
use phasekit_core::{Order, Registry, math};
use phasekit_verify::{Cell, CheckError, Fixture, fixture, majorant};

/// The core subset's `term` files (VERIFICATION.md §3.6).
const TERM_CORE: [(&str, &str); 14] = [
    fixture!("coolprop-8.0.0/term/Air.csv"),
    fixture!("coolprop-8.0.0/term/Ammonia.csv"),
    fixture!("coolprop-8.0.0/term/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/term/HFE143m.csv"),
    fixture!("coolprop-8.0.0/term/Helium.csv"),
    fixture!("coolprop-8.0.0/term/Methanol.csv"),
    fixture!("coolprop-8.0.0/term/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/term/R1130(E).csv"),
    fixture!("coolprop-8.0.0/term/R1234yf.csv"),
    fixture!("coolprop-8.0.0/term/R1234ze(E).csv"),
    fixture!("coolprop-8.0.0/term/R125.csv"),
    fixture!("coolprop-8.0.0/term/R410A.csv"),
    fixture!("coolprop-8.0.0/term/Water.csv"),
    fixture!("coolprop-8.0.0/term/n-Heptane.csv"),
];

/// The 15 output columns, each with the (i, j) of the `A_ij` it is unscaled.
const COLUMNS: [(&str, usize, usize); 15] = [
    ("alphar", 0, 0),
    ("dalphar_dtau", 1, 0),
    ("dalphar_ddelta", 0, 1),
    ("d2alphar_dtau2", 2, 0),
    ("d2alphar_ddelta_dtau", 1, 1),
    ("d2alphar_ddelta2", 0, 2),
    ("d3alphar_dtau3", 3, 0),
    ("d3alphar_ddelta_dtau2", 2, 1),
    ("d3alphar_ddelta2_dtau", 1, 2),
    ("d3alphar_ddelta3", 0, 3),
    ("d4alphar_dtau4", 4, 0),
    ("d4alphar_ddelta_dtau3", 3, 1),
    ("d4alphar_ddelta2_dtau2", 2, 2),
    ("d4alphar_ddelta3_dtau", 1, 3),
    ("d4alphar_ddelta4", 0, 4),
];

/// One oracle block: its kind (`ResidualHelmholtz` dropped), its term count and its rows, contiguous in the file.
struct Block<'a> {
    kind: &'a str,
    terms: usize,
    rows: Vec<usize>,
}

/// The fixture's blocks in `EOS[0].alphar` order; every row's status is `ok`.
fn blocks<'a>(fixture: &Fixture<'a>) -> Vec<Block<'a>> {
    let mut blocks: Vec<(&str, Block<'a>)> = Vec::new();
    for (row, cells) in fixture.rows().iter().enumerate() {
        let label = |column: usize| match cells.cells[column] {
            Cell::Text(text) => text,
            Cell::Num(_) | Cell::Blank => "",
        };
        assert_eq!(label(7), "ok", "line {}: the oracle failed", cells.line);
        match blocks.last_mut() {
            Some((index, block)) if *index == label(0) => block.rows.push(row),
            _ => {
                let terms = fixture.value(row, "terms").unwrap();
                let block = Block { kind: label(1), terms: terms as usize, rows: vec![row] };
                blocks.push((label(0), block));
            }
        }
    }
    blocks.into_iter().map(|(_, block)| block).collect()
}

/// The record list a block kind's terms go to at datagen: Power and Exponential blocks share the power list, in file
/// order (map 02 §9), so a block's terms are the next `terms` of its list.
fn list(kind: &str) -> &'static str {
    match kind {
        "Power" | "Exponential" => "power",
        "Lemmon2005" => "lemmon2005",
        "DoubleExponential" => "double_exponential",
        "Gaussian" => "gaussian",
        "GaoB" => "gao_b",
        _ => "non_analytic",
    }
}

/// `e`'s constants with only the terms `range` of `kind`'s list as the residual part (and no ideal part, which
/// `residual` never reads).
fn one_block(e: &EosRecord, kind: &str, range: std::ops::Range<usize>) -> EosRecord {
    let mut eos = EosRecord::new(e.gas_constant, e.t_reducing, e.rho_reducing, e.rho_max);
    match list(kind) {
        "power" => eos.power = e.power[range].to_vec(),
        "lemmon2005" => eos.lemmon2005 = e.lemmon2005[range].to_vec(),
        "double_exponential" => eos.double_exponential = e.double_exponential[range].to_vec(),
        "gaussian" => eos.gaussian = e.gaussian[range].to_vec(),
        "gao_b" => eos.gao_b = e.gao_b[range].to_vec(),
        _ => eos.non_analytic = e.non_analytic[range].to_vec(),
    }
    eos
}

/// The `Term` scale of entry (i, j) of a one-block record: the sum of its terms' [`majorant`]s.
fn scale(e: &EosRecord, tau: f64, delta: f64, i: usize, j: usize) -> f64 {
    let power: f64 = e.power.iter().map(|t| majorant::power(t, tau, delta, i, j)).sum();
    let lemmon: f64 = e.lemmon2005.iter().map(|t| majorant::lemmon2005(t, tau, delta, i, j)).sum();
    let double: f64 = e.double_exponential.iter().map(|t| majorant::double_exponential(t, tau, delta, i, j)).sum();
    let gaussian: f64 = e.gaussian.iter().map(|t| majorant::gaussian(t, tau, delta, i, j)).sum();
    power + lemmon + double + gaussian
}

/// Every block of `kind` in the core subset against the same block compiled alone from the `Parity` record: all 15
/// `A_ij` at the oracle's (T, ρ), so τ and δ are bitwise the oracle's, class `Term`. Returns the entries checked;
/// panics with the first 20 failures.
fn check_kind(kind: &str) -> usize {
    let registry = Registry::embedded().unwrap();
    let (mut failures, mut checked, mut headroom) = (Vec::new(), 0, 0.0_f64);
    for (path, text) in TERM_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let record = phasekit_core::internal::record(registry, name).unwrap();
        let e = &record.eos;
        let mut offsets = std::collections::BTreeMap::new();
        for block in blocks(&fixture) {
            let offset = offsets.entry(list(block.kind)).or_insert(0);
            let range = *offset..*offset + block.terms;
            *offset += block.terms;
            if block.kind != kind {
                continue;
            }
            let eos = one_block(e, kind, range);
            let model = FluidRecord::new(name, record.molar_mass, record.source.clone(), eos.clone(), record.limits);
            let model = model.compile().unwrap();
            for &row in &block.rows {
                let (t, rho) = (fixture.value(row, "T").unwrap(), fixture.value(row, "rhomolar").unwrap());
                assert_eq!(fixture.check(row, "tau", e.t_reducing / t), Ok(()), "{path}: τ as the oracle's");
                assert_eq!(fixture.check(row, "delta", rho / e.rho_reducing), Ok(()), "{path}: δ");
                let (tau, delta) = (fixture.value(row, "tau").unwrap(), fixture.value(row, "delta").unwrap());
                let got = model.eos().residual(t, rho, Order::Four);
                for (column, i, j) in COLUMNS {
                    // The oracle's unscaled derivative is A_ij / (τ^i δ^j); so is the scale.
                    let factor = math::powi(tau, i as i32) * math::powi(delta, j as i32);
                    let scale = scale(&eos, tau, delta, i, j);
                    checked += 1;
                    match fixture.check_scaled(row, column, got.get(i, j).unwrap() / factor, scale / factor) {
                        Ok(ratio) => headroom = headroom.max(ratio),
                        Err(CheckError::Mismatch(m)) => failures.push(m.to_string()),
                        Err(e) => failures.push(format!("{path}: row {row}, {column}: {e:?}")),
                    }
                }
            }
        }
        let lengths = [
            ("power", e.power.len()),
            ("lemmon2005", e.lemmon2005.len()),
            ("double_exponential", e.double_exponential.len()),
            ("gaussian", e.gaussian.len()),
            ("gao_b", e.gao_b.len()),
            ("non_analytic", e.non_analytic.len()),
        ];
        for (list, len) in lengths {
            assert_eq!(offsets.get(list).copied().unwrap_or(0), len, "{path}: the blocks are the {list} list");
        }
    }
    let shown = failures.iter().take(20).cloned().collect::<Vec<_>>().join("\n");
    assert!(
        failures.is_empty(),
        "{} of {checked} entries outside Term (headroom {headroom:.3}):\n{shown}",
        failures.len()
    );
    checked
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/term/<Fluid>.csv (map 10 §8.5 L1). Every `ResidualHelmholtzPower`
/// block of the core subset (12 blocks; R125 and Methanol have none), 100 (τ, δ) each with δ log-spaced down to
/// 1e-8, scale [`majorant::power`] (measured headroom 0.03).
#[test]
fn power_blocks_match_oracle_term_fixtures() {
    assert_eq!(check_kind("Power"), 12 * 100 * 15);
}

/// Oracle: CoolProp 8.0.0, term fixtures of R1130(E) (Exponential with g ≠ 1, block 1) and Methanol (block 0); map 02
/// §3.1, map 13 §8. They compile to power terms with c = g.
#[test]
fn exponential_matches_oracle_term_fixtures() {
    assert_eq!(check_kind("Exponential"), 2 * 100 * 15);
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/term/R125.csv: the only Lemmon2005 block (18 terms; 15 with
/// m = 0 and 5 with l = 0, "0 means absent").
#[test]
fn lemmon2005_matches_oracle_term_fixtures() {
    assert_eq!(check_kind("Lemmon2005"), 100 * 15);
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/term/Methanol.csv: the only DoubleExponential block (8 terms,
/// g_t < 0: e^(+|g_t|τ)).
#[test]
fn double_exponential_matches_oracle_term_fixtures() {
    assert_eq!(check_kind("DoubleExponential"), 100 * 15);
}

/// Oracle: CoolProp 8.0.0, term fixtures: the Gaussian blocks of the core subset (Ammonia 10 terms, CarbonDioxide 5,
/// Helium 11, Nitrogen 4, R1130(E) 4, R1234yf 7, R1234ze(E) 6, Water 3; map 02 §3.1), scale [`majorant::gaussian`].
#[test]
fn gaussian_matches_oracle_term_fixtures() {
    assert_eq!(check_kind("Gaussian"), 8 * 100 * 15);
}

/// VERIFICATION.md §3.2, §11.3: the `term` files come from the pinned runner image, like every committed oracle
/// fixture (`oracle_lock_pins_the_runner_image` in tests/fixtures.rs checks the others).
#[test]
fn term_fixtures_name_the_pinned_generator_environment() {
    let environment = |(path, text): (&str, &str)| {
        let generator = Fixture::parse(path, text).unwrap().header("generator").unwrap_or_default();
        generator.split(' ').filter(|field| !field.starts_with("sha256=")).collect::<Vec<_>>().join(" ")
    };
    let smoke = environment(fixture!("coolprop-8.0.0/facts/smoke.csv"));
    for file in TERM_CORE {
        assert_eq!(environment(file), smoke, "{}", file.0);
    }
}
