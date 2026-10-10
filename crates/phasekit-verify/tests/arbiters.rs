//! The literature arbiters (VERIFICATION.md §4.3): each record's status is asserted against the committed files, and
//! from M4 on against the evaluation of its table with the paper's own constants.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{Basis, Order, Phase, PureFluid, Registry, State, ThermoModel, math};
use phasekit_verify::arbiters::{ArbiterPart, Role, k3_values, violations};
use phasekit_verify::{
    ARBITERS, ArbiterStatus, DIVERGENCES, Fixture, MILESTONE, Part, Tolerance, fixture, from_printed,
};

const MANIFEST: &str = include_str!("../fixtures/MANIFEST.sha256");

fn committed(file: &str) -> bool {
    MANIFEST.lines().any(|line| line.rsplit(' ').next() == Some(file))
}

#[test]
fn arbiter_statuses_are_asserted() {
    assert_eq!(violations(ARBITERS, &committed, MILESTONE), Vec::<String>::new());
    let allowed = |s: &ArbiterStatus| {
        matches!(
            s,
            ArbiterStatus::Expected | ArbiterStatus::Transcribed | ArbiterStatus::None | ArbiterStatus::Unpublished
        )
    };
    // The records evaluated so far, each by its own test in this file (PLAN.md M4.5 on; the paper constants of the
    // other α^r records at M5.7; the saturation tables at M6.10).
    let alpha_r =
        ["Water", "R227EA", "R365MFC", "R115", "R13I1", "R1234ze(E)", "Helium", "R1130(E)", "Tetrahydrofuran"];
    let saturation = [("Water", ArbiterPart::Saturation), ("Helium", ArbiterPart::Saturation)];
    let evaluated: Vec<_> = alpha_r.map(|fluid| (fluid, ArbiterPart::AlphaR)).into_iter().chain(saturation).collect();
    for a in ARBITERS {
        let is_evaluated = evaluated.contains(&(a.fluid, a.part));
        assert_eq!(!allowed(&a.status), is_evaluated, "{} {:?}: {:?}", a.fluid, a.part, a.status);
    }
    let water = ARBITERS.iter().find(|a| (a.fluid, a.part) == ("Water", ArbiterPart::AlphaR)).unwrap();
    assert_eq!(water.status, ArbiterStatus::SelfConsistent);
    assert!(ARBITERS.len() >= 20, "the core set of VERIFICATION.md §4.4");
}

#[test]
fn every_arbiter_cites_a_doi_or_report() {
    for arbiter in ARBITERS.iter().filter(|a| a.status != ArbiterStatus::Unpublished) {
        let source = arbiter.citation.doi_or_report.unwrap_or_default();
        let cited = source.starts_with("10.") || source.starts_with("IAPWS ") || source.starts_with("NIST IR ");
        assert!(cited, "{} {:?}: `{source}`", arbiter.fluid, arbiter.part);
    }
}

/// PLAN.md M1.12 and VERIFICATION.md §4.4: the 18 states of CoolProp's own EOS tests (map 10 §8.1) are the arbiters
/// of 13 fluids, one record each, traced to its paper.
#[test]
fn coolprop_test_rows_cover_thirteen_fluids() {
    let records = || ARBITERS.iter().filter(|a| a.citation.role == Role::CoolPropTests);
    let mut fluids: Vec<&str> = records().map(|a| a.fluid).collect();
    fluids.sort_unstable();
    assert_eq!(
        fluids,
        [
            "PropyleneGlycol",
            "R1123",
            "R1130(E)",
            "R1132(E)",
            "R1224YDZ",
            "R1233zd(E)",
            "R1234yf",
            "R1243zf",
            "Tetrahydrofuran",
            "VinylChloride",
            "n-Perfluorobutane",
            "n-Perfluorohexane",
            "n-Perfluoropentane",
        ]
    );
    assert_eq!(records().flat_map(|a| a.tables).filter_map(|t| t.rows).sum::<u16>(), 18, "18 states");
}

/// Water under both constant sets of VERIFICATION.md §4.3: the paper's (IAPWS R6-95(2018): R = 0.46151805 kJ/(kg K),
/// T_c = 647.096 K, ρ_c = 322 kg/m³; M is not printed, so the record's converts them) and the v8.0.0 ones, which
/// step 1 finds equal to rounding.
fn iapws95_models() -> [(&'static str, PureFluid, f64); 2] {
    let record = phasekit_core::internal::record(Registry::embedded().unwrap(), "Water").unwrap();
    let m = record.molar_mass;
    let e = &record.eos;
    // Equal within the printed digits: R = 0.46151805 kJ/(kg K) allows ±5e-6 J/(kg K); ρ_c = 322 kg/m³ ±0.5.
    assert!((e.gas_constant / m - 461.51805).abs() <= 5e-6, "R/M = {}", e.gas_constant / m);
    assert!((e.rho_reducing * m - 322.0).abs() <= 0.5 && e.t_reducing == 647.096);
    let mut paper = record.clone();
    (paper.eos.gas_constant, paper.eos.rho_reducing) = (461.51805 * m, 322.0 / m);
    [("paper", paper.compile().unwrap(), m), ("v8.0.0", record.compile().unwrap(), m)]
}

/// A printed value and the value under test agree within half a unit of the last printed digit (class `Paper`).
fn within_printed(table: &Fixture<'_>, row: usize, column: &str, got: f64) -> Result<(), String> {
    let printed = table.printed(row, column).unwrap();
    let Some(Tolerance::Absolute(tol)) = from_printed(printed) else { return Err(format!("{column}: `{printed}`")) };
    let want = table.value(row, column).unwrap();
    if (got - want).abs() <= tol { Ok(()) } else { Err(format!("row {row} {column}: {got} vs printed {printed}")) }
}

/// Arbiter: IAPWS R6-95(2018) Table 6 (Wagner & Pruß 2002, PLAN.md M4.5): φ⁰, φʳ and their first and second
/// derivatives at 500 K and 838.025 kg/m³, each within half a unit of its last printed digit, with the paper's constants
/// and with the v8.0.0 ones (no divergence).
#[test]
fn iapws95_table6_within_printed_digits() {
    let (path, text) = fixture!("paper/Water/IAPWS-R6-95-2018.6.csv");
    let table = Fixture::parse(path, text).unwrap();
    let mut failures = Vec::new();
    for (label, model, m) in iapws95_models() {
        let (t, rho) = (table.value(0, "T").unwrap(), table.value(0, "rho").unwrap() / m);
        let eos = model.eos();
        let (tau, delta) = (647.096 / t, rho * m / 322.0);
        for (prefix, a) in [("phi0", eos.ideal(t, rho, Order::Two)), ("phir", eos.residual(t, rho, Order::Two))] {
            let at =
                |i: usize, j: usize| a.get(i, j).unwrap() / (math::powi(tau, i as i32) * math::powi(delta, j as i32));
            let entries = [("", at(0, 0)), ("_d", at(0, 1)), ("_dd", at(0, 2)), ("_t", at(1, 0)), ("_tt", at(2, 0))];
            for (suffix, got) in entries.into_iter().chain([("_dt", at(1, 1))]) {
                if let Err(e) = within_printed(&table, 0, &format!("{prefix}{suffix}"), got) {
                    failures.push(format!("{label}: {e}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Arbiter: IAPWS R6-95(2018) Table 7 (PLAN.md M4.5): p, c_v, w and s at the 11 single-phase states, from the order-2
/// bundle at (T, ρ) through `State::from_total`, each within half a unit of its last printed digit, with both constant
/// sets. With Table 6 this makes the Water α arbiter `SelfConsistent`.
#[test]
fn iapws95_table7_within_printed_digits() {
    let (path, text) = fixture!("paper/Water/IAPWS-R6-95-2018.7.csv");
    let table = Fixture::parse(path, text).unwrap();
    assert_eq!(table.rows().len(), 11);
    let mut failures = Vec::new();
    for (label, model, m) in iapws95_models() {
        let eos = model.eos();
        for row in 0..table.rows().len() {
            let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap() / m);
            let total = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
            let phase = if t > 647.096 {
                Phase::Supercritical
            } else if rho * m > 322.0 {
                Phase::Liquid
            } else {
                Phase::Gas
            };
            let state = State::from_total(model.info().key(), t, rho, eos.gas_constant(), m, phase, &total).unwrap();
            let values = [
                ("p", state.p() / 1e6),
                ("cv", state.cv(Basis::Mass).unwrap() / 1e3),
                ("w", state.speed_of_sound().unwrap()),
                ("s", state.s(Basis::Mass) / 1e3),
            ];
            for (column, got) in values {
                if let Err(e) = within_printed(&table, row, column, got) {
                    failures.push(format!("{label}: {e}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The cells of a K3 (saturation) table outside their printed digits for `model` (class `Paper`), each row through
/// `phasekit_verify::arbiters::k3_values` seeded with `fluid`'s superancillary, and the largest relative residual of
/// those cells (the table's coarse digits dominate the residual of the cells within them).
fn k3_check(table: &Fixture<'_>, fluid: &str, model: &PureFluid, m: f64) -> (Vec<String>, f64) {
    let record = phasekit_core::internal::record(Registry::embedded().unwrap(), fluid).unwrap();
    let curve = record.superancillary_curve().unwrap();
    let (mut failures, mut worst) = (Vec::new(), 0.0_f64);
    for row in 0..table.rows().len() {
        let seed = curve.at_t(table.value(row, "T").unwrap()).unwrap();
        for (column, got) in k3_values(table, row, model, m, (seed.bubble.rho, seed.dew.rho)).unwrap() {
            if let Err(e) = within_printed(table, row, column, got) {
                let want = table.value(row, column).unwrap();
                worst = worst.max((got - want).abs() / want.abs());
                failures.push(e);
            }
        }
    }
    (failures, worst)
}

/// Arbiter: IAPWS R6-95(2018) Table 8 (PLAN.md M6.10): p_σ, ρ′, ρ″, h′, h″, s′ and s″ at 275, 450 and 625 K, from the
/// pure VLE of the EOS at T (p_σ its vapour side's, the better conditioned) and the EOS at its densities, each within
/// half a unit of its last printed digit, with both constant sets (the paper's and the v8.0.0 ones). With Tables 6 and
/// 7 this makes Water's saturation arbiter `SelfConsistent`.
#[test]
fn iapws95_saturation_table_within_printed_digits() {
    let (path, text) = fixture!("paper/Water/IAPWS-R6-95-2018.8.csv");
    let table = Fixture::parse(path, text).unwrap();
    assert_eq!((table.rows().len(), table.columns().len()), (3, 8));
    for (label, model, m) in iapws95_models() {
        let (failures, _) = k3_check(&table, "Water", &model, m);
        assert!(failures.is_empty(), "{label}: {failures:#?}");
    }
}

/// VERIFICATION.md §4.3 step 2 for NIST IR 8474 Table 4 (PLAN.md M6.10, user decision H4): with its own Table 1
/// constants 12 cells miss their printed digits, 11 h″ by up to 1.7e-6 and p_σ at 5.1 K by 3.5e-6 (the worst cell beyond
/// its digits, rounded up to two significant digits), so Helium's saturation arbiter is `Inconsistent` and does not
/// arbitrate; DIV-0005 holds the shipped model to it at `Measured`.
#[test]
fn ir8474_saturation_table_is_inconsistent() {
    let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
    let (path, text) = fixture!("paper/Helium/OrtizVega-JPCRD-2019.4.csv");
    let table = Fixture::parse(path, text).unwrap();
    let (model, m) = paper_model_of(&registry, "Helium");
    let (failures, worst) = k3_check(&table, "Helium", &model, m);
    assert_eq!(failures.len(), 12, "{failures:#?}");
    let exponent = (math::ln(worst) / math::ln(10.0)).floor() as i32 - 1;
    let digits = (worst / math::powi(10.0, exponent)).ceil();
    let residual: f64 = format!("{digits}e{exponent}").parse().unwrap();
    let record = ARBITERS.iter().find(|a| (a.fluid, a.part) == ("Helium", ArbiterPart::Saturation)).unwrap();
    assert_eq!(record.status, ArbiterStatus::Inconsistent { residual });
}

/// A printed constant in molar SI with half a unit of its last printed digit, both through the unit's factor; a
/// per-mass gas constant or density converts with the record's molar mass `m` (IAPWS-95 prints no M). `None` for a
/// unit not listed here.
fn printed_si(text: &str, m: f64) -> Option<(f64, f64)> {
    let (number, unit) = text.split_once(' ')?;
    let Some(Tolerance::Absolute(half)) = from_printed(number) else { return None };
    let factor = match unit {
        "K" | "J/(mol K)" => 1.0,
        "kJ/(kg K)" => 1e3 * m,
        "g/mol" => 1e-3,
        "kg/m3" => 1.0 / m,
        "mol/dm3" => 1e3,
        _ => return None,
    };
    Some((number.parse::<f64>().ok()? * factor, half * factor))
}

/// PLAN.md M4.7 (map 13 A3 and R1; VERIFICATION.md §4.3 step 1): R, M, T_r and ρ_r as v8.0.0 stores them (`Parity`)
/// against every paper's printed constants, within half a unit of the last printed digit. A mismatch must be
/// registered, a `GasConstant` (R) or `Reducing` (M, T_r, ρ_r) divergence of that fluid, and the registered ones are
/// pinned: R1234ze(E)'s R (DIV-0001; Thol 2016 prints 8.3144621, v8.0.0 stores 8.314472) and Helium's (DIV-0005; NIST
/// IR 8474 prints 8.314472, v8.0.0 stores 8.3144598). Map 13 R1's other 14 candidates have no transcribed paper
/// value yet, so none is listed.
#[test]
fn stored_constants_match_their_arbiter_records() {
    let registry = Registry::embedded().unwrap();
    let (mut audited, mut registered, mut unregistered) = (Vec::new(), Vec::new(), Vec::new());
    for a in ARBITERS {
        let Some(c) = a.constants else { continue };
        let record = phasekit_core::internal::record(registry, a.fluid).unwrap();
        let (e, m) = (&record.eos, record.molar_mass);
        let mut rows = vec![
            ("R", Part::GasConstant, e.gas_constant, c.r),
            ("T_r", Part::Reducing, e.t_reducing, c.t_reducing),
            ("rho_r", Part::Reducing, e.rho_reducing, c.rho_reducing),
        ];
        rows.extend(c.molar_mass.map(|printed| ("M", Part::Reducing, m, printed)));
        for (name, part, stored, printed) in rows {
            let (paper, half) = printed_si(printed, m).expect(printed);
            audited.push((a.fluid, name));
            if (stored - paper).abs() <= half {
                continue;
            }
            match DIVERGENCES.iter().find(|d| d.fluids.contains(&a.fluid) && d.part == part) {
                Some(d) => registered.push((a.fluid, name, d.id)),
                None => unregistered.push(format!("{} {name}: stored {stored}, printed {printed}", a.fluid)),
            }
        }
    }
    assert_eq!(unregistered, Vec::<String>::new());
    registered.dedup();
    assert_eq!(registered, [("R1234ze(E)", "R", "DIV-0001"), ("Helium", "R", "DIV-0005")]);
    audited.sort_unstable();
    audited.dedup();
    assert_eq!(audited.len(), 9 * 4 - 1, "9 fluids, M printed for all but Water");
}

/// `record` with a paper's printed constants in place of its own: R, M where printed, T_r and ρ_r.
fn paper_model(
    record: &phasekit_core::internal::FluidRecord,
    c: &phasekit_verify::arbiters::Constants,
) -> (PureFluid, f64) {
    let mut paper = record.clone();
    let m = c.molar_mass.map_or(record.molar_mass, |text| printed_si(text, record.molar_mass).unwrap().0);
    paper.molar_mass = m;
    paper.eos.gas_constant = printed_si(c.r, m).unwrap().0;
    paper.eos.t_reducing = printed_si(c.t_reducing, m).unwrap().0;
    paper.eos.rho_reducing = printed_si(c.rho_reducing, m).unwrap().0;
    (paper.compile().unwrap(), m)
}

/// A K2 table's p, c_v, c_p and w at `row` from `model` (molar mass `m`), in the table's units (mol/dm³ and MPa, or
/// mol/m³ and Pa). A row at ρ = 0 is the ideal-gas limit, from the ideal part alone.
fn k2_values(table: &Fixture<'_>, row: usize, model: &PureFluid, m: f64) -> [(&'static str, f64); 4] {
    let (rho_factor, p_factor) =
        if table.header("units").unwrap().contains("mol/dm3") { (1e3, 1e-6) } else { (1.0, 1.0) };
    let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap() * rho_factor);
    let eos = model.eos();
    let r = eos.gas_constant();
    if rho == 0.0 {
        let cv = -r * eos.ideal(t, 1.0, Order::Two).get(2, 0).unwrap();
        let cp = cv + r;
        return [("p", 0.0), ("cv", cv), ("cp", cp), ("w", math::sqrt(cp / cv * r * t / m))];
    }
    let total = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
    let state = State::from_total(model.info().key(), t, rho, r, m, Phase::Gas, &total).unwrap();
    let molar = Basis::Molar;
    [
        ("p", state.p() * p_factor),
        ("cv", state.cv(molar).unwrap()),
        ("cp", state.cp(molar).unwrap()),
        ("w", state.speed_of_sound().unwrap()),
    ]
}

/// The cells of a K2 table outside their printed digits for `model` (class `Paper`), except the `exempt` (row, column)
/// cells, and the largest relative residual of all cells.
fn k2_check(table: &Fixture<'_>, model: &PureFluid, m: f64, exempt: &[(usize, &str)]) -> (Vec<String>, f64) {
    let (mut failures, mut worst) = (Vec::new(), 0.0_f64);
    for row in 0..table.rows().len() {
        for (column, got) in k2_values(table, row, model, m) {
            let Some(want) = table.value(row, column) else { continue }; // a blank the paper leaves
            if want != 0.0 {
                worst = worst.max((got - want).abs() / want.abs());
            }
            if !exempt.contains(&(row, column))
                && let Err(e) = within_printed(table, row, column, got)
            {
                failures.push(e);
            }
        }
    }
    (failures, worst)
}

/// The arbiter record of `fluid`'s α^r (or the CoolProp tests' row for it) and its first table, parsed.
fn arbiter_table(fluid: &str) -> (&'static phasekit_verify::Arbiter, Fixture<'static>) {
    let a = ARBITERS.iter().find(|a| a.fluid == fluid && a.part == ArbiterPart::AlphaR).unwrap();
    let file = a.tables[0].file;
    let text = phasekit_verify::arbiters::table_text(file).unwrap();
    (a, Fixture::parse(file, text).unwrap())
}

/// `fluid`'s model with its α^r record's printed constants, and the record.
fn paper_model_of(registry: &Registry, fluid: &str) -> (PureFluid, f64) {
    let (a, _) = arbiter_table(fluid);
    let record = phasekit_core::internal::record(registry, fluid).unwrap();
    paper_model(&record, &a.constants.unwrap())
}

/// `fluid` as phasekit ships it (`Corrected`), and its molar mass.
fn shipped_model(registry: &Registry, fluid: &str) -> (PureFluid, f64) {
    let mut record = phasekit_core::internal::record(registry, fluid).unwrap();
    record.apply(phasekit_core::DataSet::Corrected).unwrap();
    let m = record.molar_mass;
    (record.compile().unwrap(), m)
}

/// Arbiter: Lemmon et al. 2016 Table 7 (PLAN.md M5.7; map 13 §3): p, c_v, c_p and w of R-227ea, R-365mfc, R-115 and
/// R-13I1 at 3 states each, the ideal-gas limit among them, within half a unit of their last printed digit, with the
/// paper's constants and as shipped (the oracle is within 4.3e-7 of them).
#[test]
fn lemmon2016_table7_within_printed_digits() {
    let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
    let (mut failures, mut states) = (Vec::new(), 0);
    for fluid in ["R227EA", "R365MFC", "R115", "R13I1"] {
        let (_, table) = arbiter_table(fluid);
        states += table.rows().len();
        for (label, (model, m)) in
            [("paper", paper_model_of(&registry, fluid)), ("shipped", shipped_model(&registry, fluid))]
        {
            failures.extend(k2_check(&table, &model, m, &[]).0.into_iter().map(|e| format!("{fluid} {label}: {e}")));
        }
    }
    assert_eq!(states, 12);
    assert_eq!(failures, Vec::<String>::new());
}

/// VERIFICATION.md §4.3 steps 2 and 4 (PLAN.md M5.7): every α^r record with printed constants is evaluated with them,
/// all its rows within `Paper` making it `SelfConsistent`, else `Inconsistent` with the largest relative residual
/// (rounded up to two significant digits). Water's is M4.5's test. NIST IR 8474 Table 3 misses with its own R by up
/// to 1.4e-6 (map 13 §3 item 4); Tetrahydrofuran's printed (rounded) constants miss one cell by 1.4e-7, though its
/// stored ones, equal to them within their digits, reproduce the table.
#[test]
fn paper_constants_decide_the_arbiter_status() {
    let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
    let records =
        ARBITERS.iter().filter(|a| a.part == ArbiterPart::AlphaR && a.constants.is_some() && a.fluid != "Water");
    let mut evaluated = Vec::new();
    for a in records {
        let (_, table) = arbiter_table(a.fluid);
        let (model, m) = paper_model_of(&registry, a.fluid);
        let (failures, worst) = k2_check(&table, &model, m, &[]);
        let expected = if failures.is_empty() {
            ArbiterStatus::SelfConsistent
        } else {
            // Two significant digits, rounded up, parsed from the decimal so the value is the one written in ARBITERS.
            let exponent = math::ln(worst) / math::ln(10.0);
            let exponent = exponent.floor() as i32 - 1;
            let digits = (worst / math::powi(10.0, exponent)).ceil();
            ArbiterStatus::Inconsistent { residual: format!("{digits}e{exponent}").parse().unwrap() }
        };
        assert_eq!(a.status, expected, "{} ({} cells off)", a.fluid, failures.len());
        evaluated.push(a.fluid);
    }
    assert_eq!(
        evaluated,
        ["R227EA", "R365MFC", "R115", "R13I1", "R1234ze(E)", "Helium", "R1130(E)", "Tetrahydrofuran"]
    );
}

/// Arbiter: the 18 states of CoolProp's own EOS tests (PLAN.md M1.12, M5.7; map 10 §8.1), one or more per fluid of 13,
/// each traced to its paper: p, c_v, c_p and w of the shipped model within half a unit of their last printed digit,
/// except R1224YDZ's p (DIV-0014: 3.3 half-units off, as is the oracle).
#[test]
fn coolprop_test_rows_within_printed_digits() {
    let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
    let (mut failures, mut states) = (Vec::new(), 0);
    for a in ARBITERS.iter().filter(|a| a.citation.role == Role::CoolPropTests) {
        let (_, table) = arbiter_table(a.fluid);
        let (model, m) = shipped_model(&registry, a.fluid);
        let exempt: &[(usize, &str)] = if a.fluid == "R1224YDZ" { &[(0, "p")] } else { &[] };
        failures.extend(k2_check(&table, &model, m, exempt).0.into_iter().map(|e| format!("{}: {e}", a.fluid)));
        states += table.rows().len();
    }
    assert_eq!(states, 18);
    assert_eq!(failures, Vec::<String>::new());
}
