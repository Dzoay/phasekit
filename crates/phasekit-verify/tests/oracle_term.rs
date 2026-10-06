//! L3 oracle fixtures, kind `term` (VERIFICATION.md §3.5): each residual block of the core subset, isolated in
//! CoolProp with `add_fluids_as_JSON` (map 10 §8.2), against the same block compiled from the `Parity` record.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::{EosRecord, FluidRecord, PowerTerm};
use phasekit_core::{Order, PureFluid, Registry, math};
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

/// `record` with `power` as its whole residual part (and no ideal part, which `residual` never reads), compiled.
fn power_only(record: &FluidRecord, power: &[PowerTerm]) -> PureFluid {
    let e = &record.eos;
    let mut eos = EosRecord::new(e.gas_constant, e.t_reducing, e.rho_reducing, e.rho_max);
    eos.power = power.to_vec();
    FluidRecord::new(&record.name, record.molar_mass, record.source.clone(), eos, record.limits).compile().unwrap()
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/term/<Fluid>.csv (map 10 §8.5 L1). Every `ResidualHelmholtzPower`
/// block of the core subset (12 blocks; R125 and Methanol have none), all 15 `A_ij` to order 4 at 100 (τ, δ) with δ
/// log-spaced down to 1e-8, evaluated at the oracle's (T, ρ), so τ and δ are bitwise the oracle's. Power and
/// Exponential blocks both become power terms at datagen, in file order, so a block's terms are the next `terms` of
/// the record's power list. Class `Term`, scale [`majorant::power`] (measured headroom 0.03).
#[test]
fn power_blocks_match_oracle_term_fixtures() {
    let registry = Registry::embedded().unwrap();
    let (mut failures, mut checked, mut headroom) = (Vec::new(), 0, 0.0_f64);
    for (path, text) in TERM_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let record = phasekit_core::internal::record(registry, name).unwrap();
        let mut offset = 0;
        for block in blocks(&fixture) {
            if !matches!(block.kind, "Power" | "Exponential") {
                continue;
            }
            let terms = &record.eos.power[offset..offset + block.terms];
            offset += block.terms;
            if block.kind != "Power" {
                continue;
            }
            let model = power_only(&record, terms);
            for &row in &block.rows {
                let (t, rho) = (fixture.value(row, "T").unwrap(), fixture.value(row, "rhomolar").unwrap());
                assert_eq!(fixture.check(row, "tau", record.eos.t_reducing / t), Ok(()), "{path}: τ as the oracle's");
                assert_eq!(fixture.check(row, "delta", rho / record.eos.rho_reducing), Ok(()), "{path}: δ");
                let (tau, delta) = (fixture.value(row, "tau").unwrap(), fixture.value(row, "delta").unwrap());
                let got = model.eos().residual(t, rho, Order::Four);
                for (column, i, j) in COLUMNS {
                    // The oracle's unscaled derivative is A_ij / (τ^i δ^j); so is the scale.
                    let factor = math::powi(tau, i as i32) * math::powi(delta, j as i32);
                    let scale: f64 = terms.iter().map(|term| majorant::power(term, tau, delta, i, j)).sum();
                    checked += 1;
                    match fixture.check_scaled(row, column, got.get(i, j).unwrap() / factor, scale / factor) {
                        Ok(ratio) => headroom = headroom.max(ratio),
                        Err(CheckError::Mismatch(m)) => failures.push(m.to_string()),
                        Err(e) => failures.push(format!("{path}: row {row}, {column}: {e:?}")),
                    }
                }
            }
        }
        assert_eq!(offset, record.eos.power.len(), "{path}: the Power and Exponential blocks are the power list");
    }
    assert_eq!(checked, 12 * 100 * 15);
    let shown = failures.iter().take(20).cloned().collect::<Vec<_>>().join("\n");
    assert!(
        failures.is_empty(),
        "{} of {checked} entries outside Term (headroom {headroom:.3}):\n{shown}",
        failures.len()
    );
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
