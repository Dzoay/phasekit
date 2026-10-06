//! L3 oracle fixtures, kind `term` (VERIFICATION.md §3.5): each residual block of the core subset, isolated in
//! CoolProp with `add_fluids_as_JSON` (map 10 §8.2), against the same block compiled from the `Parity` record.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::EosRecord;
use phasekit_core::{Order, Registry};
use phasekit_verify::term::{self, TermCheck};
use phasekit_verify::{Cell, Fixture, fixture};

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

/// Every block of `kind` in the core subset against the same block compiled alone from the `Parity` record. Returns
/// the entries checked; panics with the first 20 failures.
fn check_kind(kind: &str) -> usize {
    let registry = Registry::embedded().unwrap();
    let mut check = TermCheck::default();
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
            if block.kind == kind {
                let eos = one_block(e, kind, range);
                check.rows(&fixture, &block.rows, term::residual_model(&record, &eos).unwrap().eos(), &eos);
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
    assert_eq!(check.report(20), None);
    check.checked
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

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/term/Ammonia.csv: the only GaoB block (block 2, 2 terms; Gao et
/// al. 2020, η in the paper's sign), scale [`majorant::gao_b`].
#[test]
fn gao_b_matches_oracle_term_fixtures() {
    assert_eq!(check_kind("GaoB"), 100 * 15);
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/all/term.csv, the all-fluid tier (VERIFICATION.md §3.6): α^r and
/// its 14 derivatives of every fluid without NonAnalytic terms (134 of 136; Water and CarbonDioxide wait for M4.1) at
/// 4 (τ, δ) each, phase imposed, against the whole residual part compiled from the `Parity` record:
/// `MultiParameterEos::residual` over all its blocks, one `match` per block (D3). Class `Term`, scale
/// [`majorant::eos`]. Re-evaluating each fluid's first state after its last gives the same bits (ROT-030: no state
/// carried between calls).
#[test]
fn alphar_totals_match_oracle_for_134_fluids() {
    let (path, text) = fixture!("coolprop-8.0.0/all/term.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let registry = Registry::embedded().unwrap();
    let mut by_fluid: Vec<(&str, Vec<usize>)> = Vec::new();
    for (row, cells) in fixture.rows().iter().enumerate() {
        let (Cell::Text(fluid), Cell::Text("all")) = (cells.cells[0], cells.cells[1]) else { panic!("{path}: {row}") };
        match by_fluid.last_mut() {
            Some((name, rows)) if *name == fluid => rows.push(row),
            _ => by_fluid.push((fluid, vec![row])),
        }
    }
    let (mut check, mut skipped) = (TermCheck::default(), Vec::new());
    for (name, rows) in &by_fluid {
        let record = phasekit_core::internal::record(registry, name).unwrap();
        if !record.eos.non_analytic.is_empty() {
            skipped.push(*name);
            continue;
        }
        let eos = term::residual_part(&record.eos);
        let model = term::residual_model(&record, &eos).unwrap();
        check.rows(&fixture, rows, model.eos(), &eos);
        let (t, rho) = (fixture.value(rows[0], "T").unwrap(), fixture.value(rows[0], "rhomolar").unwrap());
        let first = model.eos().residual(t, rho, Order::Four);
        check.rows(&fixture, &rows[1..], model.eos(), &eos);
        assert_eq!(model.eos().residual(t, rho, Order::Four), first, "{name}: residual is a pure function");
    }
    assert_eq!((by_fluid.len(), skipped), (136, vec!["CarbonDioxide", "Water"]));
    assert_eq!(check.checked, 134 * 4 * 15 + 134 * 3 * 15);
    assert_eq!(check.report(20), None);
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
    for file in TERM_CORE.into_iter().chain([fixture!("coolprop-8.0.0/all/term.csv")]) {
        assert_eq!(environment(file), smoke, "{}", file.0);
    }
}
