//! The multi-fluid mixtures of `phasekit-mix` (PLAN.md M13) against Bell et al.'s printed check tables (M13.3).
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Order, Registry};
use phasekit_mix::MultiFluid;
use phasekit_verify::{Fixture, fixture};

/// The library's name of a fluid the tables spell as REFPROP does; `NEWR1234YF` is the R1234yf equation both tables use
/// (Table XIII's pure row is v8.0.0's to all printed digits; Table XI's predates it, DIV-0050).
fn library_name(printed: &str) -> &str {
    match printed {
        "NEWR1234YF" => "R1234yf",
        "R1234ZEE" => "R1234ze(E)",
        other => other,
    }
}

/// A row's (T_r, ρ_r, α^r) at its (T, ρ): a pure fluid's from its own model, a pair's from the multi-fluid model at
/// (z₁, 1 − z₁).
fn evaluate(registry: &Registry, data: DataSet, table: &Fixture<'_>, row: usize) -> (f64, f64, f64) {
    let fluids = table.printed(row, "fluids").unwrap();
    let (t, rho, z1) =
        (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap(), table.value(row, "z1").unwrap());
    let names: Vec<&str> = fluids.split('&').map(library_name).collect();
    if let [one] = names[..] {
        let eos = registry.get(one).unwrap().model().helmholtz().unwrap();
        let reducing = eos.reducing().unwrap();
        return (reducing.t, reducing.rho, eos.residual(t, rho, Order::One).get(0, 0).unwrap());
    }
    let mix = MultiFluid::new(registry, &names, data).unwrap();
    let x = [z1, 1.0 - z1];
    let (t_r, rho_r) = mix.reducing_state(&x);
    (t_r, rho_r, mix.residual(&x, t, rho, Order::One).unwrap().get(0, 0).unwrap())
}

/// Four ulp, relative.
const ULP4: f64 = 4.0 * f64::EPSILON;

/// `got` against a cell: within its printed digits or `rounding` of |got|, whichever is larger.
fn within(table: &Fixture<'_>, row: usize, column: &str, got: f64, rounding: f64) -> Result<(), String> {
    let printed = phasekit_verify::from_printed(table.printed(row, column).unwrap());
    let Some(phasekit_verify::Tolerance::Absolute(half)) = printed else { return Err(format!("{printed:?}")) };
    table.check_bound(row, column, got, half.max(rounding * got.abs())).map(|_| ()).map_err(|e| format!("{e:?}"))
}

/// Every row's T_r, ρ_r and α^r, or the cells that miss. The tables print what teqp or CoolProp computed, to 13 and 14
/// decimals, which is below what two double-precision evaluations agree to: each cell holds within its printed digits
/// (class `Paper`) or, if that is tighter, within 4 ulp for T_r and ρ_r (a few roundings of the reducing sums) and
/// within `Term`'s 1e-13 for α^r taken at |α^r|, which never exceeds Σ_k |φ_k|, so the bound is stricter than `Term`.
fn misses(table: &Fixture<'_>, rows: impl Iterator<Item = usize>, data: DataSet) -> Vec<String> {
    // See `within`.
    let registry = Registry::from_embedded(data).unwrap();
    let mut misses = Vec::new();
    for row in rows {
        let (t_r, rho_r, alphar) = evaluate(&registry, data, table, row);
        for (column, got, rounding) in [("Tred", t_r, ULP4), ("rhored", rho_r, ULP4), ("alphar", alphar, 1e-13)] {
            if let Err(e) = within(table, row, column, got, rounding) {
                misses.push(format!("{} {column}: {e}", table.printed(row, "fluids").unwrap()));
            }
        }
    }
    misses
}

/// Bell et al. 2023 Table XIII (teqp 0.14.1): its six pure fluids and five pairs at z₁ = 0.4, (τ, δ) ≈ (0.8, 0.8): the
/// reducing state and α^r of each within its printed digits.
#[test]
fn alphar_matches_bell_2023_table_xiii() {
    let (path, text) = fixture!("paper/mixtures/Bell-JPCRD-2023.XIII.csv");
    let table = Fixture::parse(path, text).unwrap();
    assert_eq!(table.rows().len(), 11);
    let misses = misses(&table, 0..11, DataSet::Corrected);
    assert!(misses.is_empty(), "{misses:#?}");
}

/// Bell et al. 2022 Table XI (CoolProp 6.4.2dev) arbitrates the paper's three final models, R1234yf/R134a,
/// R1234yf/R1234ze(E) (DIV-0013's) and R134a/R1234ze(E), which v8.0.0 carries; its interim R125/R1234yf,
/// R1234ze(E)/R227ea and R1234yf/R152a are superseded by Bell et al. 2023, whose Table XIII arbitrates them (R1234ze(E)/
/// R227ea's T_r moved by 0.78 K). The rows without R1234yf hold within their digits; the R1234yf rows used a
/// pre-publication R1234yf equation, so their α^r is DIV-0050's proof and only their reducing states are checked here.
#[test]
fn alphar_matches_bell_2022_table_xi() {
    let (path, text) = fixture!("paper/mixtures/Bell-JPCRD-2022.XI.csv");
    let table = Fixture::parse(path, text).unwrap();
    assert_eq!(table.rows().len(), 12);
    let interim = ["R125&NEWR1234YF", "NEWR1234YF&R152A", "R1234ZEE&R227EA"];
    let fluids = |row: usize| table.printed(row, "fluids").unwrap();
    let final_models = (0..12).filter(|&row| !interim.contains(&fluids(row)));
    let (with_r1234yf, without): (Vec<usize>, Vec<usize>) =
        final_models.partition(|&row| fluids(row).contains("NEWR1234YF"));
    assert_eq!((with_r1234yf.len(), without.len()), (3, 6));
    let misses = misses(&table, without.into_iter(), DataSet::Corrected);
    assert!(misses.is_empty(), "{misses:#?}");
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    for row in with_r1234yf {
        let (t_r, rho_r, _) = evaluate(&registry, DataSet::Corrected, &table, row);
        within(&table, row, "Tred", t_r, ULP4).unwrap();
        within(&table, row, "rhored", rho_r, ULP4).unwrap();
    }
}

/// The oracle's columns and the A_ij each holds.
const ALPHAR_COLUMNS: [(&str, usize, usize); 10] = [
    ("alphar", 0, 0),
    ("A10", 1, 0),
    ("A01", 0, 1),
    ("A20", 2, 0),
    ("A11", 1, 1),
    ("A02", 0, 2),
    ("A30", 3, 0),
    ("A21", 2, 1),
    ("A12", 1, 2),
    ("A03", 0, 3),
];

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/all/mix_alphar.csv (PLAN.md M13.3). Every binary pair of v8.0.0's
/// library at z₁ = 0.4 and two states of its reducing state, on `Parity`: the reducing state and α^r with its A_ij to
/// order 3 within `Prop`: 1e-12 of the larger of |A_ij| and 1, or `Term` (1e-13 of Σ_k |φ_k|, the components' terms'
/// majorants weighted by x_i) if that is larger, as VERIFICATION.md §5 carries it (CO₂/R124's A12 at δ = 1.6 cancels to
/// 1.4e-12 of its value). The 48 pairs the oracle cannot build, which name a fluid outside v8.0.0's library, phasekit
/// refuses too. The predefined mixtures' rows wait for M13.13, which builds them.
#[test]
fn mix_alphar_matches_the_oracle() {
    let (path, text) = fixture!("coolprop-8.0.0/all/mix_alphar.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut compared, mut refused, mut misses) = (0, 0, Vec::new());
    for row in 0..table.rows().len() {
        let label = table.printed(row, "mixture").unwrap();
        if label.ends_with(".mix") {
            continue;
        }
        let names: Vec<&str> = label.split('&').collect();
        let built = MultiFluid::new(&registry, &names, DataSet::Parity);
        if table.printed(row, "status") != Some("ok") {
            assert!(built.is_err(), "{label}: the oracle refuses it, phasekit builds it");
            refused += 1;
            continue;
        }
        let mix = built.unwrap();
        let z1 = table.value(row, "z1").unwrap();
        let (x, t, rho) = ([z1, 1.0 - z1], table.value(row, "T").unwrap(), table.value(row, "rhomolar").unwrap());
        let (t_r, rho_r) = mix.reducing_state(&x);
        let d = mix.residual(&x, t, rho, Order::Three).unwrap();
        for (column, got) in [("Tr", t_r), ("rhor", rho_r)] {
            if let Err(e) = table.check_scaled(row, column, got, got.abs()) {
                misses.push(format!("{label} row {row} {column}: {e:?}"));
            }
        }
        let records: Vec<_> = names.iter().map(|n| phasekit_core::internal::record(&registry, n).unwrap()).collect();
        let (tau, delta) = (t_r / t, rho / rho_r);
        for (column, i, j) in ALPHAR_COLUMNS {
            let majorant: f64 = records
                .iter()
                .zip(&x)
                .map(|(r, xi)| xi * phasekit_verify::majorant::eos(&r.eos, tau, delta, i, j))
                .sum();
            let scale = table.value(row, column).unwrap().abs().max(1.0).max(0.1 * majorant);
            if let Err(e) = table.check_scaled(row, column, d.get(i, j).unwrap(), scale) {
                misses.push(format!("{label} row {row} {column}: {e:?}"));
            }
        }
        compared += 1;
    }
    assert!(misses.is_empty(), "{} misses: {:#?}", misses.len(), &misses[..misses.len().min(20)]);
    assert_eq!((compared, refused), (1680, 96));
}
