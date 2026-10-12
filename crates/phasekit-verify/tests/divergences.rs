//! The divergence proofs (VERIFICATION.md §6.3): one `div_NNNN` per register entry whose proof is due, dispatched
//! from `DIVERGENCES`. A proof is added in the PR of the first milestone its entry lists; `MILESTONE` (the first open
//! milestone) decides what is due, so closing a milestone fails here until its proofs exist.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::{
    ConductivityModel, Edit, FluidRecord, IdealTerm, LennardJones, LjSource, ViscosityModel,
};
use phasekit_core::{
    Basis, DataSet, Density, DomainError, Error, FlashOptions, Input, Order, Phase, Pressure, Prop, Quality, Registry,
    Temperature, ThermoModel,
};
use phasekit_verify::arbiters::{k3_values, table_text};
use phasekit_verify::{
    Cell, DIVERGENCES, Fixture, MILESTONE, ToleranceClass, fixture, from_printed, missing_proofs, unregistered_proofs,
};

#[path = "common/transport_rows.rs"]
mod transport_rows;

/// Every proof function, by register id.
const PROOFS: &[(&str, fn())] = &[
    ("DIV-0001", div_0001),
    ("DIV-0003", div_0003),
    ("DIV-0004", div_0004),
    ("DIV-0005", div_0005),
    ("DIV-0006", div_0006),
    ("DIV-0007", div_0007),
    ("DIV-0008", div_0008),
    ("DIV-0011", div_0011),
    ("DIV-0012", div_0012),
    ("DIV-0014", div_0014),
    ("DIV-0015", div_0015),
    ("DIV-0016", div_0016),
    ("DIV-0017", div_0017),
    ("DIV-0018", div_0018),
    ("DIV-0019", div_0019),
    ("DIV-0020", div_0020),
    ("DIV-0021", div_0021),
    ("DIV-0022", div_0022),
    ("DIV-0023", div_0023),
    ("DIV-0024", div_0024),
    ("DIV-0025", div_0025),
    ("DIV-0026", div_0026),
    ("DIV-0027", div_0027),
    ("DIV-0028", div_0028),
    ("DIV-0029", div_0029),
];

/// The value of a `facts/register.csv` row (the oracle side of every entry, M1.13).
fn fact(name: &str) -> f64 {
    let (path, text) = fixture!("coolprop-8.0.0/facts/register.csv");
    let facts = Fixture::parse(path, text).unwrap();
    let row = facts.rows().iter().position(|r| r.cells.first() == Some(&Cell::Text(name))).unwrap();
    facts.value(row, "value").unwrap()
}

/// The embedded record of `fluid` under `set`.
fn record(fluid: &str, set: DataSet) -> FluidRecord {
    let mut record = phasekit_core::internal::record(Registry::embedded().unwrap(), fluid).unwrap();
    record.apply(set).unwrap();
    record
}

/// DIV-0004, part M5 (`SkipOracle`; map 10 R18): c_p and c_v inside the dome are `Undefined { prop, TwoPhase }`, a
/// DT state there as for any two-phase state, where CoolProp returns numbers (Water QT(0.5, 400 K): c_p 4056.47,
/// c_v 2913.73 J/kg/K, measured 2026-10-05). The oracle's two-phase viscosity, the M8 part, is in the register's facts.
fn div_0004() {
    let water = Registry::embedded().unwrap().get("Water").unwrap();
    let state = water.state(Input::dt(Density::molar(1_000.0).unwrap(), Temperature::new(400.0).unwrap())).unwrap();
    assert_eq!(state.phase(), Phase::TwoPhase);
    for prop in [Prop::Cpmolar, Prop::Cpmass, Prop::Cvmolar, Prop::Cvmass, Prop::SpeedOfSound] {
        let undefined = water.prop(&state, prop).unwrap_err();
        assert!(matches!(undefined, Error::Undefined { phase: Phase::TwoPhase, .. }), "{prop:?}: {undefined:?}");
    }
    assert!(state.h(Basis::Mass).is_finite());
    assert!(fact("div0004_eta_t500_q0.5") > 0.0, "the oracle answers a two-phase viscosity (part M8)");
}

/// DIV-0011 (`SkipOracle`; map 12 §6.3, R8; ROT-063): phasekit's third virial coefficient is the exact Taylor
/// coefficient (`zero_density`, proved against the δ-series in tests/virials.rs); the oracle's, from α^r_δδ at δ = 1e-12,
/// differs beyond `Prop` for propane and nitrogen at 300 K and water at 600 K (measured at M1.13: −6.7e-5, −7.1e-5,
/// +1.9e-5).
fn div_0011() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    for (label, fluid, t) in
        [("propane", "n-Propane", 300.0), ("nitrogen", "Nitrogen", 300.0), ("water", "Water", 600.0)]
    {
        let exact = registry.get(fluid).unwrap().model().helmholtz().unwrap().zero_density(t).unwrap().c;
        let oracle = fact(&format!("div0011_cvirial_{label}"));
        let off = (oracle - exact) / exact.abs();
        let beyond = phasekit_verify::ToleranceClass::Prop.bound(1.0).unwrap();
        assert!(off.abs() > beyond, "{fluid}: the oracle's C is off by only {off:e}");
    }
}

/// DIV-0012 (`SkipOracle`; map 12 §6.3, #3394; ROT-078): Water DT(55018.5 mol/m³, 250 K) lies below the 273.16 K
/// triple point and is refused; the oracle accepts it and returns a negative pressure, −5.93 Pa.
fn div_0012() {
    let water = Registry::embedded().unwrap().get("Water").unwrap();
    let input = Input::dt(Density::molar(55_018.5).unwrap(), Temperature::new(250.0).unwrap());
    let below = DomainError::BelowMinTemperature { t: 250.0, t_min: 273.16 };
    assert_eq!(water.state(input), Err(below.into()));
    let oracle = fact("div0012_p_t250_rho55018.5");
    assert_eq!(oracle.to_bits(), (-5.927_712_393_567_710_5_f64).to_bits(), "a negative pressure");
}

/// p, c_v and w of `record`'s compiled model at (T, ρ), ρ in mol/m³, from the order-2 bundle.
fn p_cv_w(record: &FluidRecord, t: f64, rho: f64) -> (f64, f64, f64) {
    let model = record.clone().compile().unwrap();
    let eos = model.eos();
    let total = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
    let r = eos.gas_constant();
    let state = phasekit_core::State::from_total(model.info().key(), t, rho, r, record.molar_mass, Phase::Gas, &total);
    let state = state.unwrap();
    (state.p(), state.cv(Basis::Molar).unwrap(), state.speed_of_sound().unwrap())
}

/// Half a unit in the last digit of a printed value; NaN, which fails every comparison, if it is not a number.
fn half_unit_of(printed: &str) -> f64 {
    match phasekit_verify::from_printed(printed) {
        Some(phasekit_verify::Tolerance::Absolute(half)) => half,
        _ => f64::NAN,
    }
}

/// A printed table value's half unit in the last digit, in the table's units.
fn half_unit(table: &Fixture<'_>, row: usize, column: &str) -> f64 {
    half_unit_of(table.printed(row, column).unwrap())
}

/// The relative bound a register entry records; NaN, which fails every comparison, if it records none.
fn registered_bound(id: &str) -> f64 {
    match DIVERGENCES.iter().find(|d| d.id == id).and_then(|d| d.tolerance) {
        Some(phasekit_verify::Tolerance::Relative(bound)) => bound,
        _ => f64::NAN,
    }
}

/// The relative bound entry `id` registers for the table `file`, NaN without one.
fn registered_table_bound(id: &str, file: &str) -> f64 {
    let entry = DIVERGENCES.iter().find(|d| d.id == id);
    match entry.and_then(|d| d.table_tolerances.iter().find(|(f, _)| *f == file)) {
        Some((_, phasekit_verify::Tolerance::Relative(bound))) => *bound,
        _ => f64::NAN,
    }
}

/// DIV-0001, parts M5 (`UsePaper`; map 13 §3; ROT-043): Thol & Lemmon 2016 Table 3 fits R-1234ze(E) with R = 8.3144621.
/// (1) `Corrected`, which ships that R, has p within the table's printed digits; (2) the oracle, with v8.0.0's
/// 8.314472, is still 1.0e-6 to 1.4e-6 above it; (3) `Parity` is the oracle within `Prop`. (Part M6: the rescaled
/// superancillary.)
fn div_0001() {
    let (path, text) = fixture!("paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.csv");
    let table = Fixture::parse(path, text).unwrap();
    let (parity, corrected) = (record("R1234ze(E)", DataSet::Parity), record("R1234ze(E)", DataSet::Corrected));
    for row in [0, 1, 2, 4, 5] {
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap() * 1e3);
        let (p_corrected, p_parity) = (p_cv_w(&corrected, t, rho).0, p_cv_w(&parity, t, rho).0);
        let printed = table.value(row, "p").unwrap() * 1e6;
        assert!((p_corrected - printed).abs() <= half_unit(&table, row, "p") * 1e6, "row {row}: {p_corrected}");
        let oracle = fact(&format!("div0001_p_row{row}"));
        let high = oracle / p_corrected - 1.0;
        assert!((0.95e-6..=1.45e-6).contains(&high), "row {row}: the oracle is {high:e} above Corrected");
        let prop = phasekit_verify::ToleranceClass::Prop.bound(oracle.abs()).unwrap();
        assert!((p_parity - oracle).abs() <= prop, "row {row}: Parity {p_parity} against the oracle {oracle}");
    }
    // Part M6: the superancillary rescaled by R′/R is the corrected EOS's saturation (tests/saturation.rs).
    assert_eq!(phasekit_verify::saturation::check_rescaling(&parity, &corrected), Ok(40));
}

/// DIV-0005, part M5 (`KeepOracle`; map 13 §3 item 4; ROT-044): NIST IR 8474 Table 3 does not reproduce with its own R
/// (`Inconsistent`, tests/arbiters.rs), so no patch ships: `Corrected` is `Parity`, model key and all. Helium's p, c_v
/// and w at the table's 6 states are then within the registered 5e-7 of it (class `Measured`) and equal to the
/// oracle's within `Prop`. Part M6 (PLAN.md M6.10, user decision H4): Table 4 (K3) is `Inconsistent` too; each of the
/// shipped model's p_σ, ρ′, ρ″, h′ and h″ is within its printed digits or within the registered 2e-6 of it (`Measured`),
/// and some cells need the latter.
fn div_0005() {
    let (parity, corrected) = (record("Helium", DataSet::Parity), record("Helium", DataSet::Corrected));
    assert_eq!(parity.model_key(), corrected.model_key());
    let measured = registered_bound("DIV-0005");
    let (path, text) = fixture!("paper/Helium/OrtizVega-JPCRD-2019.3.csv");
    let table = Fixture::parse(path, text).unwrap();
    let prop = |x: f64| phasekit_verify::ToleranceClass::Prop.bound(x.abs()).unwrap();
    for row in 0..table.rows().len() {
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap() * 1e3);
        let (p, cv, w) = p_cv_w(&parity, t, rho);
        for (column, got, unit) in [("p", p, 1e6), ("cv", cv, 1.0), ("w", w, 1.0)] {
            let printed = table.value(row, column).unwrap() * unit;
            assert!((got / printed - 1.0).abs() <= measured, "row {row} {column}: {got} against {printed}");
            let oracle = fact(&format!("div0005_{column}_row{row}"));
            assert!((got - oracle).abs() <= prop(oracle), "row {row} {column}: {got} against the oracle {oracle}");
        }
    }
    let file = "paper/Helium/OrtizVega-JPCRD-2019.4.csv";
    let (bound, table4) = (registered_table_bound("DIV-0005", file), table_text(file).unwrap());
    let table4 = Fixture::parse(file, table4).unwrap();
    let (curve, m) = (parity.superancillary_curve().unwrap(), parity.molar_mass);
    let model = parity.clone().compile().unwrap();
    let mut beyond = 0;
    for row in 0..table4.rows().len() {
        let seed = curve.at_t(table4.value(row, "T").unwrap()).unwrap();
        for (column, got) in k3_values(&table4, row, &model, m, (seed.bubble.rho, seed.dew.rho)).unwrap() {
            let want = table4.value(row, column).unwrap();
            let half = match from_printed(table4.printed(row, column).unwrap()) {
                Some(phasekit_verify::Tolerance::Absolute(half)) => half,
                other => unreachable!("row {row} {column}: printed digits {other:?}"),
            };
            beyond += usize::from((got - want).abs() > half);
            assert!((got - want).abs() <= half.max(bound * want.abs()), "Table 4 row {row} {column}: {got} vs {want}");
        }
    }
    assert!(beyond > 0, "Table 4 needs its measured bound");
}

/// DIV-0014 (`Investigate`; map 10 §8.4; ROT-132): R1224YDZ's p at (400 K, 8000 mol/m³) is 3.3 half-units of the
/// printed 21.17909 MPa away from Akasaka & Lemmon 2023 Table 7, for phasekit and the oracle alike: asserted at the
/// registered 8e-7 (class `Measured`), the oracle's value pinned. Resolution: re-check the paper's Table 7.
fn div_0014() {
    let r1224ydz = record("R1224YDZ", DataSet::Corrected);
    let p = p_cv_w(&r1224ydz, 400.0, 8000.0).0;
    let (measured, half) = (registered_bound("DIV-0014"), half_unit_of("21.17909"));
    let printed = 21.179_09e6;
    assert!((p / printed - 1.0).abs() <= measured, "{p}");
    assert!((p - printed).abs() > 3.0 * half * 1e6, "outside the printed digits by more than 3 half-units: {p}");
    let oracle = fact("div0014_p_t400_rho8000");
    assert_eq!(oracle.to_bits(), 21_179_073.530_740_43_f64.to_bits());
    assert!((p - oracle).abs() <= phasekit_verify::ToleranceClass::Prop.bound(oracle).unwrap(), "{p} against {oracle}");
}

/// DIV-0003, part M2 (`UsePaper`): Corrected's ρ_r is Span et al. 2000's 11183.9 mol/m³; the oracle's
/// 11183.901464580624 differs from it, and Parity is the oracle bit for bit. (Part M6: the rescaled superancillary.)
fn div_0003() {
    let oracle = fact("div0003_rhomolar_reducing");
    let (parity, corrected) = (record("Nitrogen", DataSet::Parity), record("Nitrogen", DataSet::Corrected));
    assert_eq!(corrected.eos.rho_reducing, 11_183.9);
    assert_ne!(corrected.eos.rho_reducing, oracle);
    assert_eq!(parity.eos.rho_reducing.to_bits(), oracle.to_bits());
    assert_eq!(corrected.applied, [Box::<str>::from("DIV-0003")]);
    // Part M6: the superancillary rescaled by ρ_r′/ρ_r is the corrected EOS's saturation (tests/saturation.rs).
    assert_eq!(phasekit_verify::saturation::check_rescaling(&parity, &corrected), Ok(40));
}

/// The `Investigate` pins of DIV-0006..0008 (map 12 §6.3, upstream 2acbbc82): until a paper is transcribed, Parity's
/// ρ_r (and M) equal the oracle's bit for bit and Corrected changes nothing.
fn pinned(fluid: &str, id: &str, molar_mass: bool) {
    let (parity, corrected) = (record(fluid, DataSet::Parity), record(fluid, DataSet::Corrected));
    assert_eq!(parity.eos.rho_reducing.to_bits(), fact(&format!("{id}_rhomolar_reducing")).to_bits(), "{fluid}");
    if molar_mass {
        assert_eq!(parity.molar_mass.to_bits(), fact(&format!("{id}_molar_mass")).to_bits(), "{fluid}");
    }
    assert_eq!(corrected, parity, "{fluid}");
}

fn div_0006() {
    pinned("Ethylene", "div0006", true);
}

fn div_0007() {
    pinned("OrthoHydrogen", "div0007", true);
}

fn div_0008() {
    pinned("n-Undecane", "div0008", false);
}

fn ids() -> Vec<&'static str> {
    PROOFS.iter().map(|(id, _)| *id).collect()
}

#[test]
fn every_due_proof_exists() {
    assert_eq!(missing_proofs(DIVERGENCES, MILESTONE, &ids()), Vec::<&str>::new());
    for (_, proof) in PROOFS {
        proof();
    }
}

#[test]
fn every_proof_names_a_registered_id() {
    assert_eq!(unregistered_proofs(DIVERGENCES, &ids()), Vec::<&str>::new());
}

/// DIV-0015 (`Investigate`, PLAN.md M4.3; map 02 §6, map 13 A4): R123's c_p⁰ blocks (Younglove & McLinden 1994, MBWR)
/// are written with Tc = 456.82 K while T_r is 456.831 K. Evaluated as stored, CoolProp's way and Parity's, c_p⁰ differs
/// from the T_r form by −1.33e-5 at 300 K (−1.46e-5 at 200 K, −1.06e-5 at 500 K); the oracle's α⁰ rows match Parity
/// (`alpha0_matches_oracle_for_136_fluids`). Corrected = Parity until the paper's c_p⁰ is checked; the paper is
/// paywalled (PLAN.md §6 P4), which is the action that resolves the entry.
fn div_0015() {
    let (parity, corrected) = (record("R123", DataSet::Parity), record("R123", DataSet::Corrected));
    assert_eq!(corrected, parity);
    let mut at_t_r = parity.clone();
    let t_r = at_t_r.eos.t_reducing;
    let mut blocks = 0;
    for term in &mut at_t_r.eos.ideal {
        if let IdealTerm::Cp0Power { tc, .. } = term {
            assert_eq!((*tc, t_r), (456.82, 456.831));
            (*tc, blocks) = (t_r, blocks + 1);
        }
    }
    assert_eq!(blocks, 4, "CP0Constant and the three CP0PolyT terms");
    let rho = parity.eos.rho_reducing;
    let cp0 =
        |r: &FluidRecord| 1.0 - r.clone().compile().unwrap().eos().ideal(300.0, rho, Order::Two).get(2, 0).unwrap();
    let shift = cp0(&parity) / cp0(&at_t_r) - 1.0;
    assert!((shift / -1.33e-5 - 1.0).abs() < 0.01, "{shift}");
}

/// DIV-0016, part M6 (`SkipOracle`; M6.4): at 213 K, PropyleneGlycol's triple point, phasekit's VLE (seeded with the
/// superancillary's densities, 4e-8 of ρ″ there) gives the saturation of the v8.0.0 EOS that CoolProp 8.0.0's own VLE
/// gives (the register's facts, `ENABLE_SUPERANCILLARIES` off), within `Prop`; and fastchebpure's multiprecision ρ″
/// there (its 2026.06.02-v2 check file: 1.2262649766288283e-07 mol/m³), still 0.7 % below it, is the exempt cell.
fn div_0016() {
    let record = record("PropyleneGlycol", DataSet::Parity);
    let sa = record.superancillary_curve().unwrap().at_t(213.0).unwrap();
    let fluid = record.compile().unwrap();
    let sat = phasekit_core::internal::vle_at_t(fluid.eos(), 213.0, (sa.bubble.rho, sa.dew.rho)).unwrap();
    let close = |got: f64, want: f64| (got / want - 1.0).abs() <= 1e-12;
    let (liquid, vapour) = (fact("div0016_rhomolar_liquid_t213"), fact("div0016_rhomolar_vapour_t213"));
    assert!(close(sat.bubble.rho, liquid) && close(sat.dew.rho, vapour), "{sat:?}");
    assert!(sat.dew.rho / 1.2262649766288283e-07 - 1.0 > 7e-3, "the exempt cell still differs: {sat:?}");
}

/// DIV-0017, part M6 (`SkipOracle`; M6.7, user decision CR1): the replacing check is the criticality conditions. For
/// DimethylCarbonate and Chlorine the model critical point satisfies K1 = K2 = 0 within `Flash`, and an exempt cell
/// still differs: DimethylCarbonate's critical density is 2.0e-6 from the oracle's, beyond `Flash`'s near-critical 1e-6.
fn div_0017() {
    let registry = Registry::embedded().unwrap();
    for name in ["DimethylCarbonate", "Chlorine"] {
        let fluid = registry.get(name).unwrap();
        let c = fluid.model().critical_point().unwrap();
        let eos = fluid.model().helmholtz().unwrap();
        let d = eos.ideal(c.t, c.rho, Order::Three) + eos.residual(c.t, c.rho, Order::Three);
        let a = |j| d.get(0, j).unwrap();
        let (k1, k2) = (2.0 * a(1) + a(2), 2.0 * a(1) + 4.0 * a(2) + a(3));
        assert!(k1.abs() <= 1e-9 && k2.abs() <= 1e-9, "{name}: K1 {k1}, K2 {k2}");
    }
    let (path, text) = fixture!("coolprop-8.0.0/all/crit.csv");
    let crit = Fixture::parse(path, text).unwrap();
    let r = (0..crit.rows().len()).find(|&r| crit.printed(r, "fluid") == Some("DimethylCarbonate")).unwrap();
    let ours = registry.get("DimethylCarbonate").unwrap().model().critical_point().unwrap().rho;
    assert!((ours / crit.value(r, "rhoc_num").unwrap() - 1.0).abs() > 1e-6, "the exempt cell still differs");
}

/// DIV-0018, part M6 (`SkipOracle`; M6.8, user decision PQ1): the replacing check is the QT row that gave a PQ row its
/// p. At R245fa's p at Θ = 1e-7 in all/sat.csv, phasekit's PQ T is that row's within `SaCoeff`, and the exempt cell
/// still differs: the oracle's PQ T misses it by more.
fn div_0018() {
    let (path, text) = fixture!("coolprop-8.0.0/all/sat.csv");
    let sat = Fixture::parse(path, text).unwrap();
    let last = |input| {
        let rows = 0..sat.rows().len();
        rows.rev().find(|&r| sat.printed(r, "fluid") == Some("R245fa") && sat.printed(r, "input") == Some(input))
    };
    let (qt, pq) = (last("T").unwrap(), last("p").unwrap());
    let (t, p) = (sat.value(qt, "T").unwrap(), sat.value(pq, "p").unwrap());
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let input = Input::pq(Pressure::new(p).unwrap(), Quality::new(1.0).unwrap());
    let ours = registry.get("R245fa").unwrap().state(input).unwrap().t();
    assert!((ours / t - 1.0).abs() <= 1e-14, "{ours} against {t}");
    assert!((sat.value(pq, "T").unwrap() / t - 1.0).abs() > 1e-14, "the exempt cell still differs");
}

/// DIV-0019, part M6 (`SkipOracle`; M6.9): inside SES36's dome at 206.2675 K, phasekit's DT state is the pure VLE of
/// its EOS, the same p at every density there, with g′ = g″ to rounding; the exempt cell still differs: CoolProp's p is
/// 1e-8 away and changes with Q.
fn div_0019() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let ses36 = registry.get("SES36").unwrap();
    let dt = |rho: f64| Input::dt(Density::molar(rho).unwrap(), Temperature::new(206.2675).unwrap());
    let states = [1.97058, 0.656961].map(|rho| ses36.state(dt(rho)).unwrap());
    assert!(states.iter().all(|s| s.phase() == Phase::TwoPhase) && states[0].p() == states[1].p());
    let oracle = fact("div0019_p_t206.2675_rho1.97058");
    assert!((states[0].p() / oracle - 1.0).abs() > 1e-9, "the exempt cell still differs: {}", states[0].p());
}

/// DIV-0020, part M7 (`SkipOracle`; M7.1): phasekit's PT of Nitrogen at the oracle row's 107.45 MPa and 1192.98 K
/// reproduces its input to rounding, and its h is the oracle's DT h at the oracle's ρ (the DT row read off the PT row)
/// within `Flash`; the exempt cell still differs: the oracle's PT reports a p 7.9e-9 away (flash/Nitrogen.csv).
fn div_0020() {
    let (path, text) = fixture!("coolprop-8.0.0/flash/Nitrogen.csv");
    let flash = Fixture::parse(path, text).unwrap();
    let row = (0..flash.rows().len())
        .find(|&r| {
            let near = |column, want: f64| (flash.value(r, column).unwrap() / want - 1.0).abs() < 1e-4;
            flash.printed(r, "pair") == Some("PT") && near("x1", 107.45e6) && near("x2", 1192.98)
        })
        .unwrap();
    let (p, t) = (flash.value(row, "x1").unwrap(), flash.value(row, "x2").unwrap());
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let state =
        registry.get("Nitrogen").unwrap().state(Input::pt(Pressure::new(p).unwrap(), Temperature::new(t).unwrap()));
    let state = state.unwrap();
    assert!((state.p() / p - 1.0).abs() <= 1e-15, "{} against {p}", state.p());
    let dt_h = flash.value(row + 1, "h").unwrap();
    assert!((state.h(Basis::Molar) / dt_h - 1.0).abs() <= 1e-9, "{} against {dt_h}", state.h(Basis::Molar));
    assert!((flash.value(row, "p").unwrap() / p - 1.0).abs() > 1e-9, "the exempt cell still differs");
}

/// The text of one of CoolProp's transport test tables (PLAN.md M8.3).
fn transport_text(file: &str) -> &'static str {
    transport_rows::ROWS.iter().find(|((path, _), _)| *path == file).map(|((_, text), _)| *text).unwrap()
}

/// `prop` of `fluid` at a CoolProp test row's T and `x`, the row's density (`x_kind` `Dmolar`, `Dmass` or `D`) or
/// quality (`Q`).
fn row_value(fluid: &str, table: &Fixture<'_>, row: usize, x: f64, prop: Prop) -> f64 {
    let fluid = Registry::embedded().unwrap().get(fluid).unwrap();
    let t = Temperature::new(table.value(row, "T").unwrap()).unwrap();
    let input = match table.printed(row, "x_kind").unwrap() {
        "Dmolar" => Input::dt(Density::molar(x).unwrap(), t),
        "Q" => Input::qt(Quality::new(x).unwrap(), t),
        _ => Input::dt(Density::mass(x).unwrap(), t),
    };
    fluid.prop(&fluid.state(input).unwrap(), prop).unwrap()
}

/// M8.3's CoolProp test table of `fluid` and `prop`, with its path.
fn transport_table(fluid: &str, prop: Prop) -> (&'static str, Fixture<'static>) {
    let file = format!("paper/{fluid}/CoolProp-Tests.{}.csv", if prop == Prop::Viscosity { "eta" } else { "lambda" });
    let ((path, text), _) = transport_rows::ROWS.iter().find(|((path, _), _)| *path == file).unwrap();
    (path, Fixture::parse(path, text).unwrap())
}

/// The rows of a CoolProp test table whose `prop` lies beyond half a unit of its printed digits; every row within the
/// larger of that half unit and the bound DIV entry `id` registers for the table (as DIV-0005's Table 4).
fn beyond_printed(id: &str, fluid: &str, prop: Prop) -> Vec<usize> {
    let (file, table) = transport_table(fluid, prop);
    let bound = registered_table_bound(id, file);
    let beyond: Vec<usize> = (0..table.rows().len())
        .filter(|&row| {
            let got = row_value(fluid, &table, row, table.value(row, "x").unwrap(), prop);
            let (want, half) = (table.value(row, "value").unwrap(), half_unit(&table, row, "value"));
            assert!((got - want).abs() <= half.max(bound * want.abs()), "{file} row {row}: {got} against {want}");
            (got - want).abs() > half
        })
        .collect();
    assert!(!beyond.is_empty(), "{file} needs its registered bound no more");
    beyond
}

/// DIV-0021 (`Investigate`; map 10 §8.1): two papers' check values are off their own correlations, which v8.0.0
/// transcribes exactly and phasekit evaluates as the oracle does (`viscosity_stages_match_contributions`).
/// Quiñones-Cisneros, Huber & Deiters 2012 (SF₆): Eq. (6) with Table 2's printed dᵢ (mPa s) and T_c = 318.7232 K is
/// phasekit's dilute-gas η to 1e-12, and Table 8's ρ = 0 rows (CoolProp's rows 169 and 172, at 1e-14 kg/m³) are 3.9e-5
/// and 3.1e-5 above it, beyond their printed digits. Huber & Laesecke 2006 (R125): the 300 K point (10.5969998 mol/L,
/// nine digits) is 9.3e-5 off, the 400 K point within its digits. Both tables within their registered bounds.
/// Resolution: the user accepts the correlations as printed (`KeepOracle`), or the authors' unprinted digits are found.
fn div_0021() {
    let d = [0.118561, -0.378103, 0.416428, -0.165295, 0.0245381];
    let file = "paper/SulfurHexafluoride/CoolProp-Tests.eta.csv";
    let table = Fixture::parse(file, transport_text(file)).unwrap();
    for (row, t) in [(0, 300.0), (3, 400.0)] {
        let tr = t / 318.7232;
        let eq6 =
            1e-3 * d.iter().enumerate().fold(0.0, |s, (i, d)| s + d * phasekit_core::math::powf(tr, i as f64 / 4.0));
        let x = table.value(row, "x").unwrap();
        let eta = row_value("SulfurHexafluoride", &table, row, x, Prop::Viscosity);
        assert!((eta / eq6 - 1.0).abs() <= 1e-12, "{t} K: {eta} against Eq. (6) {eq6}");
    }
    assert_eq!(beyond_printed("DIV-0021", "SulfurHexafluoride", Prop::Viscosity), [0, 1, 2, 3, 4, 5]);
    assert_eq!(beyond_printed("DIV-0021", "R125", Prop::Viscosity), [0]);
}

/// DIV-0022 (`Investigate`; map 10 §8.1): CoolProp's test rows print these densities to fewer digits than η needs.
/// Every row beyond its printed η digits is explained by its density's: the printed η lies within the η of the densities
/// half a printed unit either side, widened by η's own half unit; and each table within its registered bound.
/// Resolution: transcribe the papers' own tables (Fenghour 1995, Tanaka 1996, Huber 2004, Vogel 1998), and with them
/// decide whether a row's printed inputs carry their half unit into the `Paper` class.
fn div_0022() {
    for (fluid, rows) in [("Ammonia", &[1][..]), ("R123", &[0]), ("n-Octane", &[0]), ("n-Propane", &[0, 1, 2, 3])] {
        let file = format!("paper/{fluid}/CoolProp-Tests.eta.csv");
        let table = Fixture::parse(&file, transport_text(&file)).unwrap();
        assert_eq!(beyond_printed("DIV-0022", fluid, Prop::Viscosity), rows, "{fluid}");
        for &row in rows {
            let (rho, half_rho) = (table.value(row, "x").unwrap(), half_unit(&table, row, "x"));
            let (low, high) = (
                row_value(fluid, &table, row, rho - half_rho, Prop::Viscosity),
                row_value(fluid, &table, row, rho + half_rho, Prop::Viscosity),
            );
            let (want, half) = (table.value(row, "value").unwrap(), half_unit(&table, row, "value"));
            assert!(low.min(high) - half <= want && want <= low.max(high) + half, "{file} row {row}: {low}..{high}");
        }
    }
}

/// DIV-0023 (`Investigate`; map 10 §8.1): six conductivity check rows off their printed digits, within CoolProp's 1e-4:
/// Lemmon & Jacobsen 2004's points near the critical point (Air, Argon, Nitrogen, Oxygen), Assael et al. 2012's SF₆ at
/// 0.97 T_c, and Huber 2005's n-Nonane at a density printed to five digits; each table within its registered bound.
/// Resolution: transcribe the papers' own tables.
fn div_0023() {
    for (fluid, row) in
        [("Air", 5), ("Argon", 5), ("Nitrogen", 5), ("Oxygen", 5), ("SulfurHexafluoride", 4), ("n-Nonane", 0)]
    {
        assert_eq!(beyond_printed("DIV-0023", fluid, Prop::Conductivity), [row], "{fluid}");
    }
}

/// DIV-0024 (`Investigate`; map 05 R10, map 10 §8.1): Friend, Ely & Ingham 1989's methane check values, computed with
/// their own equation of state, against CoolProp's hard-coded model on Setzmann & Wagner's: the dilute row at 400 K,
/// the saturated liquid at 182 K and the liquid at 100 K are off their digits, the dilute row at 100 K is not.
/// Resolution: transcribe the paper's table, with its equation of state.
fn div_0024() {
    assert_eq!(beyond_printed("DIV-0024", "Methane", Prop::Conductivity), [1, 2, 3]);
}

/// DIV-0025 (`Investigate`; map 05 R6, map 10 §8.1): at Vassiliou et al. 2015's check point (460 K, 377.687 kg/m³)
/// n-Pentane's dilute and residual terms are the paper's to their digits (34.048 and 33.325 mW/(m K)), and the critical
/// term, 7.313 against the printed 3.927, reads n-Pentane's friction-theory η where the correlation was fitted with an
/// ECS one, 49.465 µPa s: the term scales as 1/η, and with that η it is 4.014. Resolution: the user decides which η the
/// term reads; n-Pentane's own η waits for Quiñones-Cisneros & Deiters 2006.
fn div_0025() {
    assert_eq!(beyond_printed("DIV-0025", "n-Pentane", Prop::Conductivity), [1]);
    let record = record("n-Pentane", DataSet::Corrected);
    let fluid = Registry::embedded().unwrap().get("n-Pentane").unwrap();
    let state = fluid.state(Input::dt(Density::mass(377.687).unwrap(), Temperature::new(460.0).unwrap())).unwrap();
    let lambda = phasekit_core::internal::conductivity_stages(&record, &state).unwrap();
    let eta = phasekit_core::internal::viscosity_stages(&record, &state).unwrap().total();
    let mw = |w: f64| w * 1e3;
    let printed = |got: f64, want: f64| (got - want).abs() <= 5e-4;
    assert!(printed(mw(lambda.dilute), 34.048) && printed(mw(lambda.residual), 33.325), "{lambda:?}");
    assert!(printed(mw(lambda.critical), 7.313) && printed(eta * 1e6, 27.152), "{lambda:?} {eta}");
    assert!(printed(mw(lambda.critical) * eta / 49.465e-6, 4.014), "{}", mw(lambda.critical) * eta / 49.465e-6);
}

/// DIV-0026 (`SkipOracle`; map 05 R2, M8.8): with q_D edited to CoolProp's default, 2e9 1/m, each of the five fluids'
/// λ is the oracle's within `TransportEcs` at every state of the all-fluid tier where the oracle converged, through the
/// registry's reference fluid; with its own q_D at least one of those exempt cells differs beyond that bound.
fn div_0026() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (path, text) = fixture!("coolprop-8.0.0/all/transport.csv");
    let table = Fixture::parse(path, text).unwrap();
    for name in ["R13", "R14", "R142b", "R218", "RC318"] {
        let fluid = registry.get(name).unwrap();
        let own = record(name, DataSet::Parity);
        let mut edited = own.clone();
        let transport = edited.transport.as_mut().unwrap();
        transport.apply(Edit::EcsQd(2e9)).unwrap();
        let ecs = transport.conductivity.as_ref().and_then(|c| match &c.model {
            ConductivityModel::Ecs(ecs) => Some(ecs),
            _ => None,
        });
        let references = [registry.get(&ecs.unwrap().reference).unwrap().clone()];
        let (mut rows, mut differs) = (0, 0);
        for row in (0..table.rows().len()).filter(|&r| table.printed(r, "fluid") == Some(name)) {
            if table.printed(row, "lambda_status") != Some("ok") {
                continue;
            }
            let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap());
            let input = Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
            let state = fluid.flash(input, &FlashOptions::new()).unwrap();
            let stages = phasekit_core::internal::transport_stages_with(&edited, &state, &references).unwrap().1;
            let want = table.value(row, "lambda").unwrap();
            let bound = ToleranceClass::TransportEcs.bound(want).unwrap();
            let got = stages.unwrap().total();
            assert!((got - want).abs() <= bound, "{name} row {row}: {got} against {want}");
            let lambda = fluid.prop(&state, Prop::Conductivity).unwrap();
            differs += usize::from((lambda - want).abs() > bound);
            rows += 1;
        }
        assert!(rows >= 4 && differs >= 1, "{name}: {rows} rows, {differs} differ");
    }
}

/// Mylona et al. 2014 Eq. (18), EthylBenzene's dilute-gas λ in mW/(m K) at `t`, from the paper's printed coefficients and
/// the T_c it adopts (617.12 K).
fn mylona_eq_18(t: f64) -> f64 {
    let tr = t / 617.12;
    let a = [-1.10708, 10.8026, -28.9015, 41.9227, 20.9133, -4.01492];
    let numerator = a.iter().rev().fold(0.0, |s, a| s * tr + a);
    numerator / (0.259475 - 0.343879 * tr + tr * tr)
}

/// DIV-0027 (`Investigate`; map 10 §8.1, M8.8): at 600 K and 1e-12 kg/m³ EthylBenzene's λ is Mylona et al.'s Eq. (18),
/// 41.1347 mW/(m K), 1.3e-4 below their Table 21's 41.14, which `CoolProp-Tests.cpp:555` takes; at 300 K the table and
/// the equation agree to the table's digits. Resolution: what Table 21 was computed with.
fn div_0027() {
    let fluid = Registry::embedded().unwrap().get("EthylBenzene").unwrap();
    let lambda = |t: f64| {
        let state = fluid.state(Input::dt(Density::mass(1e-12).unwrap(), Temperature::new(t).unwrap())).unwrap();
        fluid.prop(&state, Prop::Conductivity).unwrap() * 1e3
    };
    for t in [300.0, 600.0] {
        assert!((lambda(t) / mylona_eq_18(t) - 1.0).abs() < 1e-12, "{t} K: {} against {}", lambda(t), mylona_eq_18(t));
    }
    assert!((lambda(300.0) - 9.71).abs() <= 0.005, "within the table's digits at 300 K");
    let off = lambda(600.0) / 41.14 - 1.0;
    assert!((off + 1.3e-4).abs() <= 0.05e-4 && (lambda(600.0) - 41.1347).abs() <= 0.00005, "{off:e}");
}

/// DIV-0028 (`Investigate`; map 05 R4, M8.9): R1234yf's stored (ρ s_r)_c is 6.85e-3 from ρ_r R (τα^r_τ − α^r) at its
/// current EOS's reducing state, and with that derived value its saturated liquid's viscosity rises by 1.6e-2 at 250 K
/// and 7.8e-3 at 340 K; the vapour's moves by under 7e-4. Resolution: derive the constant from the EOS.
fn div_0028() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let record = phasekit_core::internal::record(&registry, "R1234yf").unwrap();
    let model = record.transport.as_ref().and_then(|t| t.viscosity.as_ref()).map(|v| &v.model);
    let Some(ViscosityModel::RhoSrCs(stored)) = model else { panic!("rhosr-CS") };
    let (t, rho) = (record.eos.t_reducing, record.eos.rho_reducing);
    let a = record.clone().compile().unwrap().eos().residual(t, rho, Order::One);
    let derived = rho * record.eos.gas_constant * (a.get(1, 0).unwrap() - a.get(0, 0).unwrap());
    assert!((derived / stored.rhosr_critical - 1.0 + 6.85e-3).abs() < 1e-5, "{derived}");
    let mut edited = record.clone();
    edited.transport.as_mut().unwrap().apply(Edit::RhosrCritical(derived)).unwrap();
    let fluid = registry.get("R1234yf").unwrap();
    let eta = |record: &FluidRecord, t: f64, q: f64| {
        let state = fluid.state(Input::qt(Quality::new(q).unwrap(), Temperature::new(t).unwrap())).unwrap();
        phasekit_core::internal::transport_stages_with(record, &state, &[]).unwrap().0.unwrap().total()
    };
    let off = |t: f64, q: f64| eta(&edited, t, q) / eta(&record, t, q) - 1.0;
    assert!((off(250.0, 0.0) - 1.56e-2).abs() < 5e-4 && (off(340.0, 0.0) - 7.8e-3).abs() < 5e-4);
    assert!([250.0, 300.0, 340.0].iter().all(|&t| off(t, 1.0).abs() < 7e-4));
}

/// DIV-0029 (`Investigate`; map 05 R17, M8.9): the four fluids' ECS conductivity reads Chung's Lennard-Jones estimate
/// from the EOS reducing state, as CoolProp does; the parameters in each file's unread second viscosity entry, which
/// ship with the same ECS model, lower the dilute-gas conductivity at 250 to 400 K by up to the cited amounts.
/// Resolution: the papers' own parameters (Huber 2003, Klein 1997, McLinden 2000).
fn div_0029() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let published = [
        ("R124", 5.501e-10, 275.8, "Propane", -0.1053),
        ("R22", 4.666e-10, 284.72, "R134a", -0.0834),
        ("R245fa", 5.529e-10, 329.72, "R134a", -0.1204),
        ("R32", 4.098e-10, 289.65, "Propane", -0.0567),
    ];
    for (name, sigma, epsilon_over_k, reference, cited) in published {
        let record = record(name, DataSet::Parity);
        let mut edited = record.clone();
        let conductivity = edited.transport.as_mut().unwrap().conductivity.as_mut().unwrap();
        let ConductivityModel::Ecs(ecs) = &mut conductivity.model else { panic!("{name}: ECS") };
        assert_eq!(ecs.lennard_jones, LjSource::chung(record.eos.t_reducing, record.eos.rho_reducing), "{name}");
        ecs.lennard_jones = LjSource::Model(LennardJones { sigma, epsilon_over_k });
        let (fluid, references) = (registry.get(name).unwrap(), [registry.get(reference).unwrap().clone()]);
        let worst = [250.0, 300.0, 350.0, 400.0].into_iter().fold(0.0_f64, |worst, t| {
            let state = fluid.state(Input::dt(Density::molar(1.0).unwrap(), Temperature::new(t).unwrap())).unwrap();
            let dilute = |r: &FluidRecord| {
                phasekit_core::internal::transport_stages_with(r, &state, &references).unwrap().1.unwrap().dilute
            };
            worst.min(dilute(&edited) / dilute(&record) - 1.0)
        });
        assert!((worst - cited).abs() < 5e-4, "{name}: {worst}");
    }
}
