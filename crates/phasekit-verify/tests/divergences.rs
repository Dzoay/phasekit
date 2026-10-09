//! The divergence proofs (VERIFICATION.md §6.3): one `div_NNNN` per register entry whose proof is due, dispatched
//! from `DIVERGENCES`. A proof is added in the PR of the first milestone its entry lists; `MILESTONE` (the first open
//! milestone) decides what is due, so closing a milestone fails here until its proofs exist.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::{FluidRecord, IdealTerm};
use phasekit_core::{
    Basis, DataSet, Density, DomainError, Error, Input, Order, Phase, Pressure, Prop, Quality, Registry, Temperature,
    ThermoModel,
};
use phasekit_verify::{Cell, DIVERGENCES, Fixture, MILESTONE, fixture, missing_proofs, unregistered_proofs};

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
/// oracle's within `Prop`.
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
