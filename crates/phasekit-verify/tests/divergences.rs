//! The divergence proofs (VERIFICATION.md §6.3): one `div_NNNN` per register entry whose proof is due, dispatched
//! from `DIVERGENCES`. A proof is added in the PR of the first milestone its entry lists; `MILESTONE` (the first open
//! milestone) decides what is due, so closing a milestone fails here until its proofs exist.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::{
    ConductivityModel, Edit, FluidRecord, IdealTerm, LennardJones, LjSource, ViscosityModel,
};
use phasekit_core::{
    Basis, DataSet, Density, DomainError, Error, FlashOptions, Input, Order, Phase, Pressure, Prop, Quality, Registry,
    Temperature, ThermoModel, math,
};
use phasekit_verify::arbiters::{k3_values, table_text};
use phasekit_verify::{
    Cell, DIVERGENCES, Fixture, MILESTONE, ToleranceClass, fixture, from_printed, missing_proofs, unregistered_proofs,
};

#[path = "common/mix_crit.rs"]
mod mix_crit;
#[path = "common/mix_env.rs"]
mod mix_env;
#[path = "common/mix_flash.rs"]
mod mix_flash;
#[path = "common/mix_sat.rs"]
mod mix_sat;
#[path = "common/transport_rows.rs"]
mod transport_rows;

/// Every proof function, by register id.
const PROOFS: &[(&str, fn())] = &[
    ("DIV-0001", div_0001),
    ("DIV-0002", div_0002),
    ("DIV-0003", div_0003),
    ("DIV-0004", div_0004),
    ("DIV-0005", div_0005),
    ("DIV-0006", div_0006),
    ("DIV-0007", div_0007),
    ("DIV-0008", div_0008),
    ("DIV-0009", div_0009),
    ("DIV-0010", div_0010),
    ("DIV-0011", div_0011),
    ("DIV-0012", div_0012),
    ("DIV-0013", div_0013),
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
    ("DIV-0030", div_0030),
    ("DIV-0031", div_0031),
    ("DIV-0032", div_0032),
    ("DIV-0033", div_0033),
    ("DIV-0034", div_0034),
    ("DIV-0035", div_0035),
    ("DIV-0036", div_0036),
    ("DIV-0037", div_0037),
    ("DIV-0038", div_0038),
    ("DIV-0039", div_0039),
    ("DIV-0040", div_0040),
    ("DIV-0041", div_0041),
    ("DIV-0042", div_0042),
    ("DIV-0043", div_0043),
    ("DIV-0044", div_0044),
    ("DIV-0045", div_0045),
    ("DIV-0046", div_0046),
    ("DIV-0047", div_0047),
    ("DIV-0048", div_0048),
    ("DIV-0049", div_0049),
    ("DIV-0050", div_0050),
    ("DIV-0051", div_0051),
    ("DIV-0052", div_0052),
    ("DIV-0053", div_0053),
    ("DIV-0054", div_0054),
    ("DIV-0055", div_0055),
    ("DIV-0056", div_0056),
    ("DIV-0057", div_0057),
    ("DIV-0058", div_0058),
    ("DIV-0059", div_0059),
    ("DIV-0060", div_0060),
    ("DIV-0061", div_0061),
    ("DIV-0062", div_0062),
    ("DIV-0063", div_0063),
    ("DIV-0064", div_0064),
    ("DIV-0065", div_0065),
    ("DIV-0066", div_0066),
    ("DIV-0067", div_0067),
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

/// Whether `model`'s p_melt at row `row` of a printed melting table holds its p, MPa, to its printed digits: one of the
/// curve's pressures where two branches hold the row's T.
fn melting_row_holds(table: &Fixture<'_>, row: usize, model: &dyn ThermoModel) -> bool {
    let t = table.value(row, "T").unwrap();
    let mpa = match model.melting_pressure(t) {
        Ok(p) => vec![p / 1e6],
        Err(Error::Ambiguous { roots }) => roots.as_slice().iter().map(|p| p / 1e6).collect(),
        Err(_) => Vec::new(),
    };
    mpa.into_iter().any(|p| table.check(row, "p", p).is_ok())
}

/// DIV-0002 (`UsePaper`; map 10 R10, ROT-051): v8.0.0 gives Water's ice VI melting p₀ as 623.4 MPa, IAPWS R14-08 as
/// 632.4 MPa. (1) Under `Corrected` R14-08's Table 3 rows on ice Ih, III, V and VI are the curve's to their printed
/// digits (one of its two pressures where two branches hold T); its ice VII row at 550 K lies beyond the curve's 355 K
/// and is refused, and its last row is the sublimation line. (2) The oracle misses the ice VI row: p_melt(320 K) =
/// 1337.45 MPa against 1356.76, T_melt(1356.76 MPa) = 320.965 K. (3) `Parity` is the oracle's there within `Prop`, as on
/// every `melt` row (`tests/melting.rs::melting_matches_oracle`).
fn div_0002() {
    let path = "paper/Water/IAPWS-R14-08-2011.3.csv";
    let table = Fixture::parse(path, table_text(path).unwrap()).unwrap();
    let (corrected, parity) = (Registry::embedded().unwrap(), Registry::from_embedded(DataSet::Parity).unwrap());
    let (corrected, parity) = (corrected.get("Water").unwrap().model(), parity.get("Water").unwrap().model());
    for row in 0..4 {
        assert!(melting_row_holds(&table, row, corrected), "row {row}");
    }
    let beyond = DomainError::AboveMaxTemperature { t: 550.0, t_max: 355.0 };
    assert_eq!(corrected.melting_pressure(550.0), Err(beyond.into()));
    assert!(!melting_row_holds(&table, 3, parity), "the oracle's data misses ice VI");
    let (p_320, t_1356) = (fact("div0002_melting_p_320k"), fact("div0002_melting_t_1356.76mpa"));
    assert!((p_320 / 1e6 - 1356.76).abs() > 10.0 && (t_1356 - 320.0).abs() > 0.9, "{p_320} {t_1356}");
    let same = |a: f64, b: f64| (a / b - 1.0).abs() < 1e-12;
    assert!(same(parity.melting_pressure(320.0).unwrap(), p_320));
    assert!(same(parity.melting_temperature(1356.76e6).unwrap(), t_1356));
}

/// DIV-0004 (`SkipOracle`; map 10 R18). Part M5: c_p and c_v inside the dome are `Undefined { prop, TwoPhase }`, a
/// DT state there as for any two-phase state, where CoolProp returns numbers (Water QT(0.5, 400 K): c_p 4056.47,
/// c_v 2913.73 J/kg/K, measured 2026-10-05). Part M8 (M8.10): so are η and λ, where the oracle evaluates its
/// correlations at the mixture's density: its Water η at QT(0.5, 500 K), 1.6048e-5 Pa s, lies below the saturated
/// vapour's, which phasekit gives at Q = 1, the single phase's. SQ1: at Q = 0 and 1 c_p, c_v and w are the saturated
/// phase's too, the oracle's, whose c_p at QT(0.5, 500 K) is no phase's. SQ2: so are the partials, κ_T, β and Γ, where
/// the oracle's κ_T for CO₂ at QT(0.5, 280 K) is negative. SQ3: so are the residual h, s and g, where the oracle's
/// Water h^r at QT(0.5, 500 K) is not its phases' lever rule.
fn div_0004() {
    let water = Registry::embedded().unwrap().get("Water").unwrap();
    let state = water.state(Input::dt(Density::molar(1_000.0).unwrap(), Temperature::new(400.0).unwrap())).unwrap();
    assert_eq!(state.phase(), Phase::TwoPhase);
    for prop in [Prop::Cpmolar, Prop::Cpmass, Prop::Cvmolar, Prop::Cvmass, Prop::SpeedOfSound] {
        let undefined = water.prop(&state, prop).unwrap_err();
        assert!(matches!(undefined, Error::Undefined { phase: Phase::TwoPhase, .. }), "{prop:?}: {undefined:?}");
    }
    assert!(state.h(Basis::Mass).is_finite());
    let at = |q: f64| water.state(Input::qt(Quality::new(q).unwrap(), Temperature::new(500.0).unwrap())).unwrap();
    for prop in [Prop::Viscosity, Prop::Conductivity] {
        let undefined = Err(Error::Undefined { prop, phase: Phase::TwoPhase });
        assert_eq!((water.prop(&state, prop), water.prop(&at(0.5), prop)), (undefined.clone(), undefined));
    }
    let (oracle, vapour) = (fact("div0004_eta_t500_q0.5"), water.prop(&at(1.0), Prop::Viscosity).unwrap());
    assert!(oracle < vapour, "the oracle's two-phase η, {oracle}, below the saturated vapour's, {vapour}");
    saturated_outputs_are_the_oracle_s();
    let (oracle, liquid) = (fact("div0004_cpmass_water_t500_q0.5"), water.prop(&at(0.0), Prop::Cpmass).unwrap());
    assert!(oracle > 7.0 * liquid, "the oracle's two-phase c_p, {oracle}, is no phase's (the liquid's {liquid})");
    let co2 = Registry::embedded().unwrap().get("CarbonDioxide").unwrap();
    let wet = co2.state(Input::qt(Quality::new(0.5).unwrap(), Temperature::new(280.0).unwrap())).unwrap();
    let kappa = Prop::IsothermalCompressibility;
    assert_eq!(co2.prop(&wet, kappa), Err(Error::Undefined { prop: kappa, phase: Phase::TwoPhase }));
    let oracle = fact("div0004_kappa_t_co2_t280_q0.5");
    assert!(oracle < 0.0, "the oracle's two-phase κ_T, {oracle} 1/Pa, is negative, no phase's");
    let h_r = Prop::HmolarResidual;
    assert_eq!(water.prop(&at(0.5), h_r), Err(Error::Undefined { prop: h_r, phase: Phase::TwoPhase }));
    let lever = (water.prop(&at(0.0), h_r).unwrap() + water.prop(&at(1.0), h_r).unwrap()) / 2.0;
    let oracle = fact("div0004_hmolar_residual_water_t500_q0.5");
    assert!(
        (oracle / lever - 1.0).abs() > 0.5,
        "the oracle's two-phase h^r, {oracle}, is not the lever rule's {lever}"
    );
}

/// SQ1-SQ3: at Q = 0 and 1, c_p, c_v, w, a first and a second partial, κ_T, β, Γ, c_p⁰ and the residual h, s and g
/// are the saturated phase's, the oracle's within `Prop` (its floors, VERIFICATION.md §5): Water at 500 K, and Air at
/// 101325 Pa, whose dew point is 2.8 K above its bubble point. c_p⁰, a function of T alone, is the oracle's in the dome
/// too, Air's at PQ(101325 Pa, 0.5) between those temperatures.
fn saturated_outputs_are_the_oracle_s() {
    use phasekit_core::DerivVar::{Dmolar, Hmass, P, T};
    use phasekit_core::{Partial, SecondPartial};
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (water, air) = (registry.get("Water").unwrap(), registry.get("Air").unwrap());
    let curvature = SecondPartial { first: Partial { of: P, wrt: Dmolar, at: T }, wrt: T, at: Dmolar };
    let outputs = [
        ("cpmass", Prop::Cpmass),
        ("cvmass", Prop::Cvmass),
        ("w", Prop::SpeedOfSound),
        ("dhmass_dp_t", Prop::Partial(Partial { of: Hmass, wrt: P, at: T })),
        ("d2p_drho_t_dt_rho", Prop::SecondPartial(curvature)),
        ("kappa_t", Prop::IsothermalCompressibility),
        ("beta", Prop::IsobaricExpansion),
        ("gamma", Prop::FundamentalDerivative),
        ("cp0mass", Prop::Cp0mass),
        ("hmolar_residual", Prop::HmolarResidual),
        ("smolar_residual", Prop::SmolarResidual),
        ("gmolar_residual", Prop::GmolarResidual),
    ];
    for q in [0.0, 1.0] {
        let quality = Quality::new(q).unwrap();
        let sides = [
            ("water_t500", water, Input::qt(quality, Temperature::new(500.0).unwrap())),
            ("air_p101325", air, Input::pq(Pressure::new(101_325.0).unwrap(), quality)),
        ];
        for (label, fluid, input) in sides {
            let state = fluid.state(input).unwrap();
            for (name, prop) in outputs {
                within_prop(fluid, &state, prop, &format!("div0004_{name}_{label}_q{q}"));
            }
        }
    }
    let wet = air.state(Input::pq(Pressure::new(101_325.0).unwrap(), Quality::new(0.5).unwrap())).unwrap();
    within_prop(air, &wet, Prop::Cp0mass, "div0004_cp0mass_air_p101325_q0.5");
}

/// `prop` at `state` is the oracle's `fact` within `Prop` (its floor, [`prop_floor`]).
fn within_prop(fluid: &phasekit_core::Fluid, state: &phasekit_core::State, prop: Prop, fact_name: &str) {
    let (got, oracle) = (fluid.prop(state, prop).unwrap(), fact(fact_name));
    let bound = ToleranceClass::Prop.bound(oracle.abs().max(prop_floor(fluid, state, prop))).unwrap();
    assert!((got - oracle).abs() <= bound, "{fact_name}: {got} against the oracle's {oracle}");
}

/// The `Prop` floor of `prop` at `state` (VERIFICATION.md §5): a partial's |X|/|Y|, a second partial's |X|/(|Y|·|W|),
/// κ_T's 1/p, β's 1/T, Γ's 1, a molar energy's RT, c_p⁰'s and an entropy's R (per mass R/M), and none for the others.
fn prop_floor(fluid: &phasekit_core::Fluid, state: &phasekit_core::State, prop: Prop) -> f64 {
    use phasekit_verify::eos::{Floor, partial_floor, scale, second_partial_floor, variable};
    let value = |v| variable(fluid, state, v);
    let floor = |f| scale(f, 0.0, state.gas_constant(), state.t(), state.rho(Basis::Molar), state.p());
    match prop {
        Prop::HmolarResidual | Prop::GmolarResidual => floor(Floor::Energy),
        Prop::SmolarResidual => floor(Floor::Entropy),
        Prop::Cp0mass => floor(Floor::Entropy) / fluid.info().molar_mass(),
        Prop::Partial(p) => partial_floor(value(p.of), value(p.wrt)),
        Prop::SecondPartial(p) => second_partial_floor(value(p.first.of), value(p.first.wrt), value(p.wrt)),
        Prop::IsothermalCompressibility => 1.0 / state.p(),
        Prop::IsobaricExpansion => 1.0 / state.t(),
        Prop::FundamentalDerivative => 1.0,
        _ => 0.0,
    }
}

/// DIV-0009 (`Investigate`; map 12 §6.3, ROT-120): v8.0.0's R1233zd(E) has no viscosity model (the oracle raises
/// "Viscosity model is not available"; v7.2.0 answered 6.3689e-4 Pa s), and phasekit's η is `NoModel`, never a value
/// borrowed from elsewhere. Resolution: a model through the register (upstream 14da1f0d restored and refit it).
fn div_0009() {
    let fluid = Registry::embedded().unwrap().get("R1233zd(E)").unwrap();
    let state = fluid.state(Input::pt(Pressure::new(101_325.0).unwrap(), Temperature::new(300.0).unwrap())).unwrap();
    assert_eq!(fluid.prop(&state, Prop::Viscosity), Err(Error::NoModel { prop: Prop::Viscosity }));
    assert!(fact("div0009_eta_t300_p101325").is_nan(), "the oracle has none either");
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
/// quality (`Q`); inside the dome, the single phase's there ([`transport_rows::dome_side`]).
fn row_value(fluid: &str, table: &Fixture<'_>, row: usize, x: f64, prop: Prop) -> f64 {
    let fluid = Registry::embedded().unwrap().get(fluid).unwrap();
    let t = Temperature::new(table.value(row, "T").unwrap()).unwrap();
    let input = match table.printed(row, "x_kind").unwrap() {
        "Dmolar" => Input::dt(Density::molar(x).unwrap(), t),
        "Q" => Input::qt(Quality::new(x).unwrap(), t),
        _ => Input::dt(Density::mass(x).unwrap(), t),
    };
    let state = fluid.state(input).unwrap();
    let state = match transport_rows::dome_side(fluid, &state) {
        Some(phase) => fluid.flash(input, &FlashOptions::new().with_phase(phase)).unwrap(),
        None => state,
    };
    fluid.prop(&state, prop).unwrap()
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
    let stored = model.and_then(|m| if let ViscosityModel::RhoSrCs(r) = m { Some(r) } else { None }).unwrap();
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
        let ecs = if let ConductivityModel::Ecs(ecs) = &mut conductivity.model { Some(ecs) } else { None };
        let ecs = ecs.unwrap();
        let estimate = LjSource::chung(record.eos.t_reducing, record.eos.rho_reducing).parameters();
        let is_chungs = matches!(ecs.lennard_jones, LjSource::Chung(lj) if transport_rows::same_lj(lj, estimate));
        assert!(is_chungs, "{name}: {:?}, not Chung's {estimate:?}", ecs.lennard_jones);
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

/// `prop` of `fluid` at `t` and the mass density `rho` as a table row is: `Extrapolate`, since it may lie beyond the
/// EOS's range, and inside the dome the single phase's there ([`transport_rows::dome_side`]).
fn at_mass(fluid: &str, t: f64, rho: f64, prop: Prop) -> f64 {
    let fluid = Registry::embedded().unwrap().get(fluid).unwrap();
    let input = Input::dt(Density::mass(rho).unwrap(), Temperature::new(t).unwrap());
    let options = FlashOptions::new().with_domain(phasekit_core::DomainPolicy::Extrapolate);
    let state = fluid.flash(input, &options).unwrap();
    let options = transport_rows::dome_side(fluid, &state).map_or(options, |phase| options.with_phase(phase));
    fluid.prop(&fluid.flash(input, &options).unwrap(), prop).unwrap()
}

/// `prop` of `fluid` at `t` and the molar density `rho` as a table row is, inside the dome the single phase's there
/// ([`transport_rows::dome_side`]).
fn at_molar(fluid: &str, t: f64, rho: f64, prop: Prop) -> f64 {
    let fluid = Registry::embedded().unwrap().get(fluid).unwrap();
    let input = Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
    let state = fluid.state(input).unwrap();
    let options =
        transport_rows::dome_side(fluid, &state).map_or(FlashOptions::new(), |p| FlashOptions::new().with_phase(p));
    fluid.prop(&fluid.flash(input, &options).unwrap(), prop).unwrap()
}

/// DIV-0030 (`SkipOracle`; map 05 R3, M8.9), the replacing check: Ammonia's λ is finite and continuous across Tufeu's
/// T_c, 405.4 K, at a gas, a near-critical and a liquid density, and equals CoolProp's term outside the floored band;
/// the exempt cell still differs: the oracle's 6.26 W/(m K) at 405.3999 K and 50 kg/m³. Below the EOS's T_c the
/// near-critical density is in the dome, where λ is `Undefined` (M8.10), so the term is checked at the single phase there.
fn div_0030() {
    let ammonia = Registry::embedded().unwrap().get("Ammonia").unwrap();
    let dome = ammonia.state(Input::dt(Density::mass(235.0).unwrap(), Temperature::new(405.4).unwrap())).unwrap();
    let undefined = Error::Undefined { prop: Prop::Conductivity, phase: Phase::TwoPhase };
    assert_eq!(ammonia.prop(&dome, Prop::Conductivity), Err(undefined));
    for rho in [50.0, 235.0, 600.0] {
        let lambda = |t: f64| at_mass("Ammonia", t, rho, Prop::Conductivity);
        let band = [405.2401, 405.3, 405.3999, 405.4, 405.4001, 405.5, 405.5599].map(lambda);
        assert!(band.iter().all(|l| l.is_finite() && *l > 0.0), "{rho}: {band:?}");
        // No step: points 0.1 K apart or closer differ by under 1e-3, where CoolProp's term diverges.
        let jumps = band.windows(2).map(|w| (w[1] / w[0] - 1.0).abs()).fold(0.0, f64::max);
        assert!(jumps < 1e-3, "{rho}: {band:?}");
    }
    let lambda = at_mass("Ammonia", 405.3999, 50.0, Prop::Conductivity);
    assert!(fact("div0030_lambda_ammonia_t405.3999_rho50") / lambda > 10.0, "the exempt cell still differs: {lambda}");
}

/// DIV-0031 (`Investigate`; map 10 §8.1, M8.9): at the papers' own temperatures and densities, the correlations as
/// transcribed miss their tables by the cited amounts, the 630 K methanol row by far the most, where the table's own
/// densities are inconsistent. Resolution: with the papers' authors.
fn div_0031() {
    let rows = [
        ("Benzene", 300.0, 875.0, 608.52e-6, 1.77e-5),
        ("n-Heptane", 250.0, 720.0, 725.69e-6, -1.04e-4),
        ("n-Heptane", 550.0, 500.0, 95.105e-6, -2.71e-5),
        ("Methanol", 300.0, 788.41, 0.5422e-3, -1.30e-4),
        ("Methanol", 630.0, 888.50, 0.2405e-3, -1.46e-2),
    ];
    for (fluid, t, rho, printed, cited) in rows {
        let off = at_mass(fluid, t, rho, Prop::Viscosity) / printed - 1.0;
        assert!((off - cited).abs() < 0.01 * cited.abs(), "{fluid} at {t} K: {off:e}");
    }
}

/// DIV-0032 (`Investigate`; map 05 R6, M8.9): the critical term scales as 1/η, and with the viscosity each paper
/// printed, 40.842, 36.170 and 15.253 µPa s, the conductivity is the paper's to its digits; the dilute and residual
/// terms are Vassiliou et al. 2015's printed 37.042 and 24.018, 35.883 and 14.321 mW/(m K), within a unit of their last
/// digit (cyclopentane's Δλ is 24.0175). Resolution: the
/// viscosity the term reads (map 05 R6's declared viscosity).
fn div_0032() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let rows = [
        ("Cyclopentane", 512.0, 400.0, 40.842e-6, 69.698e-3, Some((37.042e-3, 24.018e-3))),
        ("Isopentane", 460.0, 329.914, 36.170e-6, 59.649e-3, Some((35.883e-3, 14.321e-3))),
        ("n-Heptane", 535.0, 100.0, 15.253e-6, 51.655e-3, None),
    ];
    for (name, t, rho, eta_paper, printed, stages) in rows {
        let record = record(name, DataSet::Parity);
        let state =
            registry.get(name).unwrap().state(Input::dt(Density::mass(rho).unwrap(), Temperature::new(t).unwrap()));
        let (eta, lambda) = phasekit_core::internal::transport_stages_with(&record, &state.unwrap(), &[]).unwrap();
        let (eta, lambda) = (eta.unwrap().total(), lambda.unwrap());
        let with_paper_eta = lambda.dilute + lambda.residual + lambda.critical * eta / eta_paper;
        assert!((with_paper_eta - printed).abs() <= 0.5e-6, "{name}: {with_paper_eta}");
        assert!((lambda.total() - printed).abs() > 0.5e-6, "{name}: the row itself differs");
        if let Some((dilute, residual)) = stages {
            // Within a unit of the last printed digit: cyclopentane's Δλ is 24.0175 against 24.018.
            assert!(
                (lambda.dilute - dilute).abs() <= 1e-6 && (lambda.residual - residual).abs() <= 1e-6,
                "{name}: {lambda:?}"
            );
        }
    }
}

/// DIV-0033 (`Investigate`; map 10 §8.1, M8.9): Friend et al.'s 100 K and 430 K viscosity rows, at the table's rounded
/// densities, are as far off as cited, and within their printed values at a density inside half a printed unit of
/// 0.01 mol/dm³; the 440 K and 310 K conductivity points are off by the cited amounts. Resolution: the paper's g9 under
/// Corrected, and Friend's critical-term constants.
fn div_0033() {
    let molar = |t: f64, rho: f64, prop: Prop| at_molar("Ethane", t, rho, prop);
    let eta = [(100.0, 21330.0, 878.68e-6, 1.6e-3), (430.0, 12780.0, 58.70e-6, 3.8e-4)];
    for (t, rho, printed, cited) in eta {
        let off = molar(t, rho, Prop::Viscosity) / printed - 1.0;
        assert!((off - cited).abs() < 0.1 * cited, "{t} K: {off:e}");
        let (lo, hi) = (molar(t, rho - 5.0, Prop::Viscosity), molar(t, rho + 5.0, Prop::Viscosity));
        assert!(lo.min(hi) <= printed && printed <= lo.max(hi), "{t} K: {printed} outside [{lo}, {hi}]");
    }
    let lambda = [(440.0, 1520.0, 45.9e-3, -4.35e-3), (310.0, 4130.0, 45.4e-3, 3.30e-3)];
    for (t, rho, printed, cited) in lambda {
        let off = molar(t, rho, Prop::Conductivity) / printed - 1.0;
        assert!((off - cited).abs() < 0.05 * cited.abs(), "{t} K: {off:e}");
    }
}

/// DIV-0034 (`Investigate`; map 10 §8.1, M8.9): R23's rows lie within their values at the density plus or minus half a
/// printed unit; CO₂'s and R123's are off by the cited amounts. Resolution: the papers' own tables.
fn div_0034() {
    let r23 = [
        (180.0, 21097.0, Prop::Viscosity, 353.88e-6, 0.005e-6),
        (420.0, 7564.0, Prop::Viscosity, 39.459e-6, 0.0005e-6),
        (180.0, 21097.0, Prop::Conductivity, 143.19e-3, 0.005e-3),
        (420.0, 7564.0, Prop::Conductivity, 50.19e-3, 0.005e-3),
    ];
    for (t, rho, prop, printed, half) in r23 {
        let (lo, hi) = (at_molar("R23", t, rho - 0.5, prop), at_molar("R23", t, rho + 0.5, prop));
        assert!(lo.min(hi) - half <= printed && printed <= lo.max(hi) + half, "R23 {prop:?} at {t} K");
    }
    let co2 = at_mass("CarbonDioxide", 300.0, 1400.0, Prop::Viscosity) / 0.50594e-3 - 1.0;
    let r123 = at_mass("R123", 180.0, 0.002873, Prop::Conductivity) / 2.473e-3 - 1.0;
    assert!((co2 - 1.05e-5).abs() < 5e-7 && (r123 + 9.56e-4).abs() < 5e-6, "{co2:e} {r123:e}");
}

/// DIV-0035 (`SkipOracle`; map 05 §3.8 row 4, M8.9; HE1). REFPROP's code for helium's critical conductivity term, which
/// CoolProp copies, mistypes the scaled equation it reads; phasekit evaluates it as Levelt Sengers, Greer and Sengers
/// 1976 print it. The replacing check: NIST TN 1334's 54 single-phase rows of 5-6.5 K on its 0.2-0.4 MPa isobars, Hands
/// and Arp's correlation on TN 1334's own EOS and η, within the table's measured bound (1.07 % at 5.3 K, 55.47 kg/m³).
/// Where the oracle's λ is NaN, in both lobes of its negative blend, phasekit's is finite. The exempt band is the
/// term's, 3.5-12 K, since ρ_c enters its damping too.
fn div_0035() {
    let fluid = Registry::embedded().unwrap().get("Helium").unwrap();
    let lambda = |t: f64, rho: Density| {
        let state = fluid.state(Input::dt(rho, Temperature::new(t).unwrap())).unwrap();
        fluid.prop(&state, Prop::Conductivity).unwrap()
    };
    let lobes = [(5.5, 17000.0, "t5.5_rho17000"), (5.5, 18000.0, "t5.5_rho18000"), (5.9, 15000.0, "t5.9_rho15000")];
    for (t, rho, name) in lobes {
        assert!(fact(&format!("div0035_lambda_helium_{name}")).is_nan(), "{name}: the oracle's λ");
        let got = lambda(t, Density::molar(rho).unwrap());
        assert!(got.is_finite() && got > 0.0, "{name}: {got}");
    }
    let file = "paper/Helium/Arp-NIST-TN1334-1998.B.csv";
    let (path, text) = fixture!("paper/Helium/Arp-NIST-TN1334-1998.B.csv");
    let table = Fixture::parse(path, text).unwrap();
    let worst = (0..table.rows().len())
        .map(|row| {
            let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap());
            lambda(t, Density::mass(rho).unwrap()) / table.value(row, "lambda").unwrap() - 1.0
        })
        .fold(0.0_f64, |worst, off| worst.max(off.abs()));
    assert_eq!(table.rows().len(), 54, "{file}");
    assert!(worst <= registered_table_bound("DIV-0035", file), "{file}: {worst:e}");
    assert!(worst > 1e-2, "the bound is measured: {worst:e}");
    probes_differ_where_the_term_is_on();
}

/// DIV-0035's oracle side, at the helium probe states of `transport/probes.csv`: where the critical term is on, 3.5 < T
/// < 12 K, the seven inside the blend's region, W = (ΔT*/0.2)² + (Δρ*/0.25)² ≤ 1, move from the oracle's λ by 0.9-25 %
/// (the scaled equation) and the four outside it by 1e-6 to 2e-4 (ρ_c in the damping); at 3.5 and 12 K, where it is
/// off, λ is the oracle's in `TransportDirect`.
fn probes_differ_where_the_term_is_on() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let fluid = registry.get("Helium").unwrap();
    let (path, text) = fixture!("coolprop-8.0.0/transport/probes.csv");
    let table = Fixture::parse(path, text).unwrap();
    let mut counts = [0; 3];
    for row in (0..table.rows().len()).filter(|&r| table.printed(r, "fluid") == Some("Helium")) {
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap());
        let state = fluid.state(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap())).unwrap();
        let (got, oracle) = (fluid.prop(&state, Prop::Conductivity).unwrap(), table.value(row, "lambda").unwrap());
        let off = (got / oracle - 1.0).abs();
        let (dt, drho) = ((1.0 - t / 5.18992).abs(), (1.0 - state.rho(Basis::Mass) / 69.58).abs());
        let blended = (dt / 0.2) * (dt / 0.2) + (drho / 0.25) * (drho / 0.25) <= 1.0;
        let class = match (3.5 < t && t < 12.0, blended) {
            (true, true) => (0, off > 5e-3),
            (true, false) => (1, off > 1e-6 && off < 1e-3),
            (false, _) => (2, (got - oracle).abs() <= ToleranceClass::TransportDirect.bound(oracle).unwrap()),
        };
        assert!(class.1, "{t} K, {rho} mol/m³: {got} against the oracle's {oracle}");
        counts[class.0] += 1;
    }
    assert_eq!(counts, [7, 4, 2]);
}

/// DIV-0036 (`Investigate`; map 02 §6 row 5): Hydrogen's and OrthoHydrogen's melting line, one Simon fit cited to
/// Datchi et al. 2000 and applied from 13.957 K, is the oracle's to `Prop`: 23.6 MPa at Hydrogen's triple point, over
/// 3000 times its triple pressure, and 44.9 MPa at 20 K, twice ParaHydrogen's 22.7 MPa; OrthoHydrogen's copy starts
/// below its own triple point. Resolution: Datchi 2000's validity range, and a curve for the low temperatures.
fn div_0036() {
    let registry = &Registry::from_embedded(DataSet::Parity).unwrap();
    let p = |name: &str, t: f64| registry.get(name).unwrap().model().melting_pressure(t).unwrap();
    let same = |a: f64, b: f64| (a / b - 1.0).abs() < 1e-12;
    assert!(same(p("Hydrogen", 13.957), fact("div0036_melting_p_hydrogen_t13.957")));
    assert!(same(p("Hydrogen", 20.0), fact("div0036_melting_p_hydrogen_t20")));
    assert!(same(p("ParaHydrogen", 20.0), fact("div0036_melting_p_parahydrogen_t20")));
    assert!(same(p("OrthoHydrogen", 14.008), fact("div0036_melting_p_orthohydrogen_t14.008")));
    assert!(p("Hydrogen", 13.957) > 3000.0 * fact("div0036_ptriple_hydrogen"));
    assert!(p("Hydrogen", 20.0) > 1.9 * p("ParaHydrogen", 20.0));
    let ortho = phasekit_core::internal::record(registry, "OrthoHydrogen").unwrap();
    assert!(ortho.melting[0].t_min < ortho.limits.t_triple().unwrap(), "{:?}", ortho.limits);
}

/// `PropsSI` through the compat strings over the embedded `Parity` registry, at DT(ρ mol/m³, T) or QT(Q, T).
fn props(output: &str, pair: (&str, f64, f64), fluid: &str) -> Result<f64, phasekit_compat::CompatError> {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    match pair {
        ("DT", rho, t) => phasekit_compat::props_si_in(&registry, output, "T", t, "Dmolar", rho, fluid),
        (_, q, t) => phasekit_compat::props_si_in(&registry, output, "Q", q, "T", t, fluid),
    }
}

/// DIV-0037 (`SkipOracle`; ROT-013, M10.2): a single phase has no quality. Water's gas at DT(10 mol/m³, 400 K) is the
/// oracle's −1 and here `Undefined`; R407C's gas at DT(405.54 mol/m³, 291.68 K) has the oracle's Qmass 1, here
/// `Undefined` too.
fn div_0037() {
    let undefined = |prop| Err(phasekit_compat::CompatError::Core(Error::Undefined { prop, phase: Phase::Gas }));
    assert_eq!(props("Q", ("DT", 10.0, 400.0), "Water"), undefined(Prop::Q));
    assert_eq!(fact("div0037_q_water_t400_rho10"), -1.0);
    let r407c = props("Qmass", ("DT", 405.544_225_835_316, 291.675_685_187_633_23), "R407C");
    assert_eq!(r407c, undefined(Prop::Qmass));
    assert_eq!(fact("div0037_qmass_r407c_t291.68"), 1.0);
}

/// DIV-0038 (`SkipOracle`; map 09 §9, D14, M10.2): the shipped records carry no environmental metadata, so R134a's
/// GWP100 is `NoModel` here and the oracle's 1430 (restricted data) is not asserted.
fn div_0038() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    assert_eq!(phasekit_core::internal::record(&registry, "R134a").unwrap().environmental, None);
    let gwp = registry.get("R134a").unwrap().constant(phasekit_core::Constant::Gwp100);
    assert_eq!(gwp, Err(Error::NoModel { prop: Prop::Constant(phasekit_core::Constant::Gwp100) }));
    assert_eq!(fact("div0038_gwp100_r134a"), 1430.0);
}

/// DIV-0039 (`SkipOracle`; map 01 R5, ROT-008, M10.2): Water's `rhomass_reducing` is ρ_r·M, 322 kg/m³, here; the oracle
/// refuses it.
fn div_0039() {
    use phasekit_core::Constant::{MolarMass, RhomassReducing, RhomolarReducing};
    let water = Registry::from_embedded(DataSet::Parity).unwrap().get("Water").unwrap().clone();
    let (mass, molar, m) =
        (water.constant(RhomassReducing), water.constant(RhomolarReducing), water.constant(MolarMass));
    assert_eq!(mass, Ok(molar.unwrap() * m.unwrap()));
    assert!(mass.clone().is_ok_and(|rho| (rho - 322.0).abs() < 1e-9), "{mass:?}");
    assert!(fact("div0039_rhomass_reducing_water").is_nan(), "refused by the oracle");
}

/// DIV-0040 (`SkipOracle`; map 09 R8, M10.2): Ammonia's EOS states its triple point as 195.49 K and its lowest
/// temperature as 195.495 K; phasekit's T_triple is the first and its T_min the second, the oracle's T_triple the second.
fn div_0040() {
    use phasekit_core::Constant::{TMin, TTriple};
    let ammonia = Registry::from_embedded(DataSet::Parity).unwrap().get("Ammonia").unwrap().clone();
    assert_eq!((ammonia.constant(TTriple), ammonia.constant(TMin)), (Ok(195.49), Ok(195.495)));
    assert_eq!(fact("div0040_t_triple_ammonia"), 195.495);
}

/// DIV-0041 (`SkipOracle`; map 05 §1, M10.2): σ is a function of T, so R125's gas at DT(532.3 mol/m³, 318.0 K) has its
/// saturated interface's σ at 318.0 K here; the oracle refuses σ at a single-phase state.
fn div_0041() {
    let (rho, t) = (532.315_573_405_610_8, 318.007_804_226_615_75);
    assert_eq!(props("Phase", ("DT", rho, t), "R125").unwrap(), 5.0, "a gas");
    let r125 = Registry::from_embedded(DataSet::Parity).unwrap().get("R125").unwrap().clone();
    assert_eq!(props("surface_tension", ("DT", rho, t), "R125"), Ok(r125.model().surface_tension(t).unwrap()));
    assert!(fact("div0041_sigma_r125_t318.0").is_nan(), "refused by the oracle");
}

/// DIV-0042 (`SkipOracle`; map 01 R7, ROT-165, M10.2): Air's QT inside its dome is refused (user decisions PP1, PP2),
/// so its T_critical there is refused too; the oracle answers the trivial output, 132.5306 K, while refusing its h.
fn div_0042() {
    let refused = props("T_critical", ("QT", 0.5, 100.0), "Air");
    assert_eq!(refused, props("Hmolar", ("QT", 0.5, 100.0), "Air"));
    assert!(matches!(refused, Err(phasekit_compat::CompatError::Core(Error::InvalidInput { .. }))), "{refused:?}");
    assert_eq!(fact("div0042_tcrit_air_q0.5_t100"), 132.5306);
    assert!(fact("div0042_h_air_q0.5_t100").is_nan(), "the oracle refuses the state's h");
}

/// DIV-0043 (`SkipOracle`; map 13 §4, M10.2): Air's saturated liquid at 80 K has g = h − Ts here; the oracle refuses g
/// there (its α⁰ at δ = −inf) while its h matches this one's.
fn div_0043() {
    let at = ("QT", 0.0, 80.0);
    let [g, h, s] = ["Gmolar", "Hmolar", "Smolar"].map(|o| props(o, at, "Air").unwrap());
    assert!((g - (h - 80.0 * s)).abs() <= 1e-9 * h.abs().max(80.0 * s.abs()), "{g} {h} {s}");
    assert!(fact("div0043_g_air_q0_t80").is_nan(), "refused by the oracle");
    let oracle = fact("div0043_h_air_q0_t80");
    assert!((h - oracle).abs() <= ToleranceClass::Prop.bound(oracle.abs().max(8.314 * 80.0)).unwrap(), "{h} {oracle}");
}

/// DIV-0044 (`SkipOracle`; map 02 §3.4, M10.2): Z is p/(ρRT) of the state here. Inside R1234yf's dome at DT(803.48
/// mol/m³, 203.94 K) that is 0.0086, where the oracle's 1 + δα^r_δ at the mixture's density is −0.164; at
/// PropyleneGlycol's saturated vapour at 225.99 K, 1.47 from its superancillary's p and ρ″, where the oracle's EOS at ρ″
/// gives 1.0.
fn div_0044() {
    let z = props("Z", ("DT", 803.484_455_766_067_1, 203.940_704_580_085_17), "R1234yf").unwrap();
    assert!((z - 0.0086).abs() < 1e-4 && fact("div0044_z_r1234yf_t203.94") < -0.16, "{z}");
    let z = props("Z", ("QT", 1.0, 225.991_054_973_623_68), "PropyleneGlycol").unwrap();
    assert!((z - 1.47).abs() < 0.01, "{z}");
    assert!((fact("div0044_z_propyleneglycol_q1_t225.99") - 1.0).abs() < 1e-12);
}

/// DIV-0045 (`Investigate`; M8.8, M10.2): at Propylene's saturated vapour at 104.87 K the two ECS solves' η part by
/// 1.8e-5 relative, at R14's at 215.07 K their λ by 23 %, beyond `TransportEcs`.
fn div_0045() {
    let ecs = ToleranceClass::TransportEcs.bound(1.0).unwrap();
    let eta = props("viscosity", ("QT", 1.0, 104.867_614_862_097_13), "Propylene").unwrap();
    let off = eta / fact("div0045_eta_propylene_q1_t104.87") - 1.0;
    assert!(off.abs() > 1e3 * ecs && (off.abs() - 1.8e-5).abs() < 1e-6, "{off:e}");
    let lambda = props("conductivity", ("QT", 1.0, 215.068_075_567_540_23), "R14").unwrap();
    let off = lambda / fact("div0045_lambda_r14_q1_t215.07") - 1.0;
    assert!((off.abs() - 0.23).abs() < 0.01, "{off}");
}

/// The PR or SRK `kind` of `fluid` from a registry of `data` with the cubics added.
fn cubic(kind: phasekit_cubic::Kind, fluid: &str, data: DataSet) -> phasekit_core::Fluid {
    let registry = Registry::from_embedded(data).unwrap();
    let cubics = phasekit_cubic::with_cubics(&registry, &[kind], data).unwrap();
    cubics.get(&format!("{}::{fluid}", kind.prefix())).unwrap().clone()
}

/// A molar (p, T) input.
fn pt(p: f64, t: f64) -> Input {
    Input::new(phasekit_core::Pair::PT, p, t, Basis::Molar).unwrap()
}

/// DIV-0010 (`SkipOracle`, PLAN.md M11.7; map 06 C1, map 12 §6.3): the replacing check, the kit's `entropy_identity`,
/// holds for phasekit's PR and SRK n-Propane at 400 K and 1 bar, T (∂s/∂T)_p = c_p within 1e-7; v8.0.0's own facts there
/// still break it: 400 (s(400.01) − s(399.99))/0.02 = 91.35 J/(mol K) against c_p = 93.89.
fn div_0010() {
    for kind in [phasekit_cubic::Kind::Pr, phasekit_cubic::Kind::Srk] {
        let fluid = cubic(kind, "n-Propane", DataSet::Corrected);
        assert_eq!(phasekit_verify::entropy_identity(&fluid, &[pt(1e5, 400.0)]), Ok(()), "{kind:?}");
    }
    let oracle = 400.0 * (fact("div0010_smolar_t400.01") - fact("div0010_smolar_t399.99")) / 0.02;
    let cp = fact("div0010_cpmolar_t400");
    assert!((oracle - 91.35).abs() < 0.005 && (cp - 93.89).abs() < 0.005, "{oracle} against {cp}");
}

/// DIV-0046 (`KeepOracle`, user decision CB1): (1) a cubic's ideal part is its source's `Derivs` bit for bit and its
/// c_p⁰ the source's times R_CODATA/R_source, with one R in both datasets; (2) phasekit's PR c_p⁰ is the oracle's for
/// n-Propane, Water and CycloPropane at the facts' states within 1e-14; (3) the source's own c_p⁰ (its paper's α⁰)
/// differs from the cubic's by the measured ratio, within the entry's bound and only there.
fn div_0046() {
    let bound = registered_bound("DIV-0046");
    for (name, t, key) in [
        ("n-Propane", 300.0, "n-propane_t300"),
        ("Water", 400.0, "water_t400"),
        ("CycloPropane", 300.0, "cyclopropane_t300"),
    ] {
        let (pr, parity) = (
            cubic(phasekit_cubic::Kind::Pr, name, DataSet::Corrected),
            cubic(phasekit_cubic::Kind::Pr, name, DataSet::Parity),
        );
        let source = Registry::embedded().unwrap().get(name).unwrap();
        let gas_constant = |f: &phasekit_core::Fluid| f.model().helmholtz().unwrap().gas_constant();
        assert_eq!((gas_constant(&pr), gas_constant(&parity)), (phasekit_cubic::R_CODATA, phasekit_cubic::R_CODATA));
        let cp0 = |f: &phasekit_core::Fluid| f.prop(&f.state(pt(1e4, t)).unwrap(), Prop::Cp0molar).unwrap();
        let ratio = phasekit_cubic::R_CODATA / gas_constant(source);
        assert!((cp0(&pr) / cp0(source) / ratio - 1.0).abs() <= 4.0 * f64::EPSILON, "{name}");
        let oracle = fact(&format!("div0046_cp0molar_pr_{key}"));
        assert!((cp0(&parity) / oracle - 1.0).abs() <= 1e-14, "{name}: {} against {oracle}", cp0(&parity));
        let paper = (cp0(&pr) / cp0(source) - 1.0).abs();
        assert!(paper <= bound && paper > 1e-7, "{name}: {paper:e} against {bound:e}");
    }
}

/// DIV-0047 (`UsePaper`, PLAN.md M11.2): (1) `Corrected`'s SRK meets the exact critical conditions at n-Propane's
/// critical point, Z = 1/3 and (∂p/∂ρ)_T = 0 within 1e-14; (2) the oracle misses them: Z = 1/3 (1 + 2.08e-9),
/// (∂p/∂ρ)_T/RT = 1.46e-9; (3) `Parity`'s SRK, with v8.0.0's literals, gives the oracle's Z within 1e-14.
fn div_0047() {
    let z_at = |data: DataSet, kind: phasekit_cubic::Kind| {
        let fluid = cubic(kind, "n-Propane", data);
        let eos = fluid.model().helmholtz().unwrap();
        let rho = 4_251_200.0 / (kind.z_c() * phasekit_cubic::R_CODATA * 369.89);
        let d = eos.residual(369.89, rho, Order::Two);
        (1.0 + d.get(0, 1).unwrap(), 1.0 + 2.0 * d.get(0, 1).unwrap() + d.get(0, 2).unwrap())
    };
    let (z, dp) = z_at(DataSet::Corrected, phasekit_cubic::Kind::Srk);
    assert!((z * 3.0 - 1.0).abs() <= 1e-14 && dp.abs() <= 1e-14, "{z} {dp:e}");
    let oracle = fact("div0047_z_srk_n-propane");
    assert!((oracle * 3.0 - 1.0 - 2.08e-9).abs() < 5e-12, "{oracle}");
    let (parity, _) = z_at(DataSet::Parity, phasekit_cubic::Kind::Srk);
    assert!((parity / oracle - 1.0).abs() <= 1e-14, "{parity} against {oracle}");
}

/// DIV-0048 (`SkipOracle`, PLAN.md M11.4; ROT-216): the replacing check, phasekit's PR Neon and D5 take T_c, p_c and ω
/// from their records; v8.0.0's library still holds its drifted copies (Neon 44.4918 K, D5 619.15 K).
fn div_0048() {
    for (name, tc_library) in [("Neon", 44.4918), ("D5", 619.15)] {
        let pr = cubic(phasekit_cubic::Kind::Pr, name, DataSet::Corrected);
        let record = Registry::embedded().unwrap().get(name).unwrap().info().published_critical().unwrap();
        assert_eq!(pr.model().critical_point().unwrap().t, record.t, "{name}");
        let oracle = fact(&format!("div0048_tcrit_pr_{}", name.to_lowercase()));
        assert!((oracle - tc_library).abs() < 1e-9 && (oracle - record.t).abs() > 0.05, "{name}: {oracle}");
    }
}

/// DIV-0049 (`SkipOracle`, PLAN.md M11.4; map 15 X6): the replacing check, phasekit's PR R40 and R134a take their HEOS
/// fluid's α⁰, so their c_p⁰ is the source's times the R ratio; v8.0.0's still differs: R40's h 2.38e4 J/mol below its
/// HEOS fluid's, R134a's c_p⁰ by −5.19e-5 (at 500 K, above R134a's 455 K T_max, so evaluated under `Extrapolate`).
fn div_0049() {
    for (name, t) in [("R40", 500.0), ("R134a", 500.0)] {
        let (pr, source) = (
            cubic(phasekit_cubic::Kind::Pr, name, DataSet::Corrected),
            Registry::embedded().unwrap().get(name).unwrap(),
        );
        let extrapolate = FlashOptions::new().with_domain(phasekit_core::DomainPolicy::Extrapolate);
        let cp0 =
            |f: &phasekit_core::Fluid| f.prop(&f.flash(pt(1e4, t), &extrapolate).unwrap(), Prop::Cp0molar).unwrap();
        let ratio = phasekit_cubic::R_CODATA / source.model().helmholtz().unwrap().gas_constant();
        assert!((cp0(&pr) / cp0(source) / ratio - 1.0).abs() <= 4.0 * f64::EPSILON, "{name}");
    }
    let h_offset = fact("div0049_hmolar_pr_r40_t500") - fact("div0049_hmolar_heos_r40_t500");
    let cp0_offset = fact("div0049_cp0molar_pr_r134a_t500") / fact("div0049_cp0molar_heos_r134a_t500") - 1.0;
    assert!((h_offset + 2.38e4).abs() < 50.0 && (cp0_offset + 5.19e-5).abs() < 5e-8, "{h_offset} {cp0_offset:e}");
}

/// A Bell et al. table row's (T_r, α^r) on `data` at its (T, ρ, z₁): the pair's from the multi-fluid model, a pure
/// fluid's from its own (`NEWR1234YF` is R1234yf, `R1234ZEE` R1234ze(E)).
fn bell_row(table: &Fixture<'_>, row: usize, data: DataSet) -> (f64, f64) {
    fn name(n: &str) -> &str {
        match n {
            "NEWR1234YF" => "R1234yf",
            "R1234ZEE" => "R1234ze(E)",
            other => other,
        }
    }
    let registry = Registry::from_embedded(data).unwrap();
    let names: Vec<&str> = table.printed(row, "fluids").unwrap().split('&').map(name).collect();
    let (t, rho, z1) =
        (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap(), table.value(row, "z1").unwrap());
    if let [one] = names[..] {
        let eos = registry.get(one).unwrap().model().helmholtz().unwrap();
        return (eos.reducing().unwrap().t, eos.residual(t, rho, Order::One).get(0, 0).unwrap());
    }
    let mix = phasekit_mix::MultiFluid::new(&registry, &names, data).unwrap();
    let x = [z1, 1.0 - z1];
    (mix.reducing_state(&x).0, mix.residual(&x, t, rho, Order::One).unwrap().get(0, 0).unwrap())
}

/// DIV-0013 (`UsePaper`, PLAN.md M13.3; ROT-207): Bell et al. 2022 Table III gives R1234yf/R1234ze(E) γ_T = 0.993309;
/// v8.0.0's library has 0.999309. (1) `Corrected`'s T_r at Table XI's row (z₁ = 0.4) is the table's, 375.3687082354176
/// K, within its printed digits, and its α^r within DIV-0050's bound of the table's (whose R1234yf equation predates
/// v8.0.0's); (2) the oracle's α^r there is −0.4646790, 0.89 % from the table; (3) `Parity`, with v8.0.0's γ_T, gives
/// the oracle's within 1e-12 and T_r 376.449 K.
fn div_0013() {
    let (path, text) = fixture!("paper/mixtures/Bell-JPCRD-2022.XI.csv");
    let table = Fixture::parse(path, text).unwrap();
    let row = (0..table.rows().len()).find(|&r| table.printed(r, "fluids") == Some("NEWR1234YF&R1234ZEE")).unwrap();
    let (t_r, alphar) = bell_row(&table, row, DataSet::Corrected);
    // Within the printed 13 decimals, or 4 ulp where those are finer than the reducing sums' rounding.
    let bound = half_unit(&table, row, "Tred").max(4.0 * f64::EPSILON * t_r);
    table.check_bound(row, "Tred", t_r, bound).unwrap();
    let printed = table.value(row, "alphar").unwrap();
    assert!((alphar / printed - 1.0).abs() <= registered_bound("DIV-0050"), "{alphar} against {printed}");
    let oracle = fact("div0013_alphar_t469_rho3399");
    assert!((oracle / printed - 1.0 - 0.00887).abs() < 1e-4, "the oracle {oracle} against the table {printed}");
    let (t_r, parity) = bell_row(&table, row, DataSet::Parity);
    assert!((parity / oracle - 1.0).abs() <= 1e-12 && (t_r - 376.449).abs() < 1e-3, "{parity} {t_r}");
}

/// DIV-0050 (`KeepOracle`, PLAN.md M13.3): Bell et al. 2022 Table XI used a pre-publication R1234yf equation. Its pure
/// R1234yf row's α^r is 1.48e-8 from v8.0.0's (which Table XIII's row matches within its printed digits), and its two
/// final pairs with R1234yf hold within the registered 2e-8 (its interim R125/R1234yf and R1234yf/R152a are not
/// v8.0.0's models; Table XIII arbitrates them).
fn div_0050() {
    let bound = registered_bound("DIV-0050");
    let (path, text) = fixture!("paper/mixtures/Bell-JPCRD-2022.XI.csv");
    let table = Fixture::parse(path, text).unwrap();
    let interim = ["R125&NEWR1234YF", "NEWR1234YF&R152A"];
    let fluids = |r: usize| table.printed(r, "fluids").unwrap();
    let rows = (0..table.rows().len()).filter(|&r| fluids(r).contains("NEWR1234YF") && !interim.contains(&fluids(r)));
    let mut worst: f64 = 0.0;
    for row in rows {
        let (_, alphar) = bell_row(&table, row, DataSet::Corrected);
        worst = worst.max((alphar / table.value(row, "alphar").unwrap() - 1.0).abs());
    }
    assert!((1e-8..=bound).contains(&worst), "{worst:e}");
    let (path, text) = fixture!("paper/mixtures/Bell-JPCRD-2023.XIII.csv");
    let newer = Fixture::parse(path, text).unwrap();
    let row = (0..newer.rows().len()).find(|&r| newer.printed(r, "fluids") == Some("NEWR1234YF")).unwrap();
    let alphar = bell_row(&newer, row, DataSet::Corrected).1;
    newer.check_bound(row, "alphar", alphar, half_unit(&newer, row, "alphar").max(1e-13 * alphar.abs())).unwrap();
}

/// DIV-0051 (`KeepOracle`, PLAN.md M13.4; user decision MX3): Tkaczuk et al. 2020 Table 8 prints p with Σ x_i R_i.
/// (1) phasekit's p, with R_CODATA, is the table's times R_CODATA/Σ x_i R_i, within 1e-12 (the same α^r); (2) the
/// oracle's p (v8.0.0, R_CODATA) is phasekit's within `Prop`: +3.4e-7 and −2.68e-6 from the table, as measured.
fn div_0051() {
    let (path, text) = fixture!("paper/mixtures/Tkaczuk-JPCRD-2020.8.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    for (row, label) in [(0, "he_ne"), (1, "he_ar"), (2, "ne_ar")] {
        let names: Vec<&str> = table.printed(row, "fluids").unwrap().split('&').collect();
        let fluid = phasekit_mix::mixture(&registry, &[(names[0], 0.5), (names[1], 0.5)], DataSet::Corrected).unwrap();
        let input = Input::new(phasekit_core::Pair::DT, 10_000.0, 200.0, Basis::Molar).unwrap();
        let p = fluid.flash(input, &FlashOptions::new().with_phase(Phase::Gas)).unwrap().p();
        let r: f64 =
            names.iter().map(|n| 0.5 * registry.get(n).unwrap().model().helmholtz().unwrap().gas_constant()).sum();
        let printed = table.value(row, "p").unwrap();
        assert!((p / printed - phasekit_mix::R_CODATA / r).abs() <= 1e-12, "{label}: {p} against {printed}");
        let oracle = fact(&format!("div0051_p_{label}"));
        assert!((oracle - p).abs() <= ToleranceClass::Prop.bound(oracle).unwrap(), "{label}: {oracle} against {p}");
    }
}

/// DIV-0052 (`SkipOracle`, PLAN.md M13.4, M13.6): (1) phasekit's mixture entropy is its pressure's, (∂s/∂ρ)_T =
/// −(∂p/∂T)_ρ/ρ² by central differences (class `Fd`), for Methane/Ethane; (2) the oracle's α⁰ there is Σ x_i (R_i/R)
/// [α⁰_i + ln x_i] of phasekit's own components within 1e-12, each ln δ_i weighted by R_i/R, and so differs from
/// phasekit's by (1 − Σ x_i R_i/R) ln ρ, within 1e-12; (3) the oracle's chemical potentials are not its own Gibbs
/// energy: for EBENZENE/PXYLENE Σ x_i μ_i is more than 0.04 J/mol from its h − Ts (phasekit's meet theirs,
/// `tests/composition.rs`).
fn div_0052() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let x = [0.4, 0.6];
    let fluid = phasekit_mix::mixture(&registry, &[("Methane", x[0]), ("Ethane", x[1])], DataSet::Parity).unwrap();
    let options = FlashOptions::new().with_phase(Phase::Gas);
    let at = |t: f64, rho: f64| {
        fluid.flash(Input::new(phasekit_core::Pair::DT, rho, t, Basis::Molar).unwrap(), &options).unwrap()
    };
    let (t, rho) = (300.0, 3_000.0);
    let (h_t, h_rho) = (1e-5 * t, 1e-5 * rho);
    let ds_drho = (at(t, rho + h_rho).s(Basis::Molar) - at(t, rho - h_rho).s(Basis::Molar)) / (2.0 * h_rho);
    let dp_dt = (at(t + h_t, rho).p() - at(t - h_t, rho).p()) / (2.0 * h_t);
    assert!((ds_drho / (-dp_dt / (rho * rho)) - 1.0).abs() <= 1e-7, "{ds_drho} against {}", -dp_dt / (rho * rho));
    let (path, text) = fixture!("coolprop-8.0.0/all/mix_eos.csv");
    let table = Fixture::parse(path, text).unwrap();
    let canonical = |label: &str| -> Vec<String> {
        label.split('&').map(|n| registry.canonical_name(n).map(String::from).unwrap_or_default()).collect()
    };
    let row = (0..table.rows().len())
        .find(|&r| canonical(table.printed(r, "mixture").unwrap()) == ["Methane", "Ethane"])
        .unwrap();
    let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rhomolar").unwrap());
    let oracle = table.value(row, "alpha0").unwrap();
    let (mut weighted, mut r_bar) = (0.0, 0.0);
    for (name, xi) in ["Methane", "Ethane"].iter().zip(x) {
        let eos = registry.get(name).unwrap().model().helmholtz().unwrap();
        let r = eos.gas_constant() / phasekit_mix::R_CODATA;
        weighted += xi * r * (eos.ideal(t, rho, Order::One).get(0, 0).unwrap() + phasekit_core::math::ln(xi));
        r_bar += xi * r;
    }
    assert!((oracle - weighted).abs() <= 1e-12 * oracle.abs(), "{oracle} against {weighted}");
    let ours = fluid.model().helmholtz().unwrap().ideal(t, rho, Order::One).get(0, 0).unwrap();
    let predicted = (1.0 - r_bar) * phasekit_core::math::ln(rho);
    assert!(((ours - oracle) - predicted).abs() <= 1e-12 * oracle.abs(), "{} against {predicted}", ours - oracle);
    let row = (0..table.rows().len()).find(|&r| table.printed(r, "mixture") == Some("EBENZENE&PXYLENE")).unwrap();
    let value = |column: &str| table.value(row, column).unwrap();
    let (z1, t) = (value("z1"), value("T"));
    let sum = z1 * value("chemical_potential_1") + (1.0 - z1) * value("chemical_potential_2");
    let g = value("hmolar") - t * value("smolar");
    assert!((sum - g).abs() > 0.04, "{sum} against {g}");
    // (4) Its critical points: with every R_i/R taken as 1, as its L* takes them, phasekit's are the oracle's within
    // `Flash` (each binary at z1 = 0.5, DIV-0056's point aside), and with its own weights one is more than 1e-7 away.
    let (mut compared, mut own) = (0, 0.0_f64);
    let mut fluid: Option<(String, phasekit_core::Fluid)> = None;
    for o in mix_crit::oracle_points().into_iter().filter(|o| o.z1 == 0.5) {
        if fluid.as_ref().is_none_or(|(label, _)| *label != o.label) {
            fluid = Some((o.label.clone(), mix_crit::binary(&registry, &o.label, o.z1)));
        }
        let Some((_, fluid)) = &fluid else { continue };
        if o.t < fluid.model().limits().unwrap().t_min() || o.t == 231.162_260_498_000_05 {
            continue;
        }
        let mix = mix_crit::mixture(fluid);
        let unit = mix_crit::offsets(&phasekit_mix::internal::critical_points_with_unit_weights(mix), &o).unwrap();
        assert!(unit.iter().all(|x| *x <= ToleranceClass::Flash.bound(1.0).unwrap()), "{} {}: {unit:?}", o.label, o.t);
        own = mix_crit::offsets(mix.critical_points(), &o).unwrap().into_iter().fold(own, f64::max);
        compared += 1;
    }
    assert!(compared >= 14 && own > 1e-7, "{compared} points, own weights {own:e} away");
}

/// DIV-0053 (`SkipOracle`, PLAN.md M13.4; ROT-062, ROT-103): R134a's critical T is not its reducing one, and v8.0.0
/// evaluates its α⁰ at the critical one in a mixture. Lemmon and Jacobsen 2004 Table 6 (REFPROP) arbitrates: (1)
/// phasekit's R32/R134a c_v at its 300 K, 15 mol/dm³ row is within 1e-6 of the table; (2) the oracle's is 3.1e-5 below.
fn div_0053() {
    let (path, text) = fixture!("paper/mixtures/Lemmon-JPCRD-2004.6.csv");
    let table = Fixture::parse(path, text).unwrap();
    let row = (0..table.rows().len())
        .find(|&r| table.printed(r, "mixture") == Some("R32&R134a") && table.printed(r, "rho") == Some("15."))
        .unwrap();
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let fluid = phasekit_mix::mixture(&registry, &[("R32", 0.5), ("R134a", 0.5)], DataSet::Corrected).unwrap();
    let input = Input::new(phasekit_core::Pair::DT, 15_000.0, 300.0, Basis::Molar).unwrap();
    let cv = fluid.flash(input, &FlashOptions::new().with_phase(Phase::Liquid)).unwrap().cv(Basis::Molar).unwrap();
    let printed = table.value(row, "cv").unwrap();
    assert!((cv / printed - 1.0).abs() <= 1e-6, "{cv} against {printed}");
    let oracle = fact("div0053_cvmolar_r32_r134a_t300_rho15000");
    assert!((oracle / printed - 1.0 + 3.1e-5).abs() < 0.2e-5, "the oracle {oracle} against the table {printed}");
}

/// DIV-0054 (`KeepOracle`, PLAN.md M13.4): Lemmon and Jacobsen 2004 Table 6 against phasekit's model, whose R is
/// R_CODATA where the table's is the components': away from the critical point the worst cell is between 1e-6 and the
/// registered 3e-6; at the six near-critical rows p, c_v and w within the table bound of 1e-5, beyond 3e-6.
fn div_0054() {
    let (path, text) = fixture!("paper/mixtures/Lemmon-JPCRD-2004.6.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let (mut away, mut near) = (0.0_f64, 0.0_f64);
    for row in 0..table.rows().len() {
        let names: Vec<&str> = table.printed(row, "mixture").unwrap().split('&').collect();
        let x: Vec<f64> =
            table.printed(row, "x").unwrap().split('/').map(|v| v.parse::<f64>().unwrap() / 100.0).collect();
        let parts: Vec<(&str, f64)> = names.iter().copied().zip(x).collect();
        let fluid = phasekit_mix::mixture(&registry, &parts, DataSet::Corrected).unwrap();
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap() * 1e3);
        let s = fluid
            .flash(
                Input::new(phasekit_core::Pair::DT, rho, t, Basis::Molar).unwrap(),
                &FlashOptions::new().with_phase(Phase::Gas),
            )
            .unwrap();
        let near_critical = table.printed(row, "note").is_some_and(|n| !n.is_empty());
        let mut cells =
            vec![("p", s.p() / 1e6), ("cv", s.cv(Basis::Molar).unwrap()), ("w", s.speed_of_sound().unwrap())];
        if !near_critical {
            cells.push(("cp", s.cp(Basis::Molar).unwrap()));
        }
        for (column, got) in cells {
            let off = (got / table.value(row, column).unwrap() - 1.0).abs();
            if near_critical { near = near.max(off) } else { away = away.max(off) }
        }
    }
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0054").unwrap();
    assert_eq!(entry.tolerance, Some(phasekit_verify::Tolerance::Relative(3e-6)));
    assert!((1e-6..=3e-6).contains(&away) && (3e-6..=1e-5).contains(&near), "{away:e} {near:e}");
}

/// DIV-0056 (`SkipOracle`, PLAN.md M13.7): at its three points of v8.0.0's `all_critical_points`, (1) L*'s first row,
/// δ_1j/x_1 + n ∂²(n α^r)/∂n_1∂n_j, vanishes to 1e-13 of its largest entry (4.6e-14 at most, std or libm), so
/// det L* = det M* = 0 whatever the third derivatives; (2) Q's null vector there is the first component's direction,
/// and the cubic form along it,
/// ∂³(A/RT)/∂n_1³ = n ∂³(n α^r)/∂n_1³ − 1/x_1², is 1.69, 1.50 and 375.9: no critical point; (3) phasekit reports none
/// within 1 % of it.
fn div_0056() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0056").unwrap();
    let listed: &[f64] = match entry.exempt {
        Some(phasekit_verify::Exempt { rows: phasekit_verify::Rows::At(listed), .. }) => listed,
        _ => &[],
    };
    assert_eq!(listed.len(), 3, "DIV-0056 lists its rows");
    let points: Vec<_> = mix_crit::oracle_points().into_iter().filter(|o| listed.contains(&o.t)).collect();
    assert_eq!(points.len(), listed.len());
    for (o, cubic) in points.iter().zip([1.69, 1.50, 375.9]) {
        let fluid = mix_crit::binary(&registry, &o.label, o.z1);
        let mix = mix_crit::mixture(&fluid);
        let c = mix.composition_at(o.t, o.rho, Order::Three).unwrap();
        let x = [o.z1, 1.0 - o.z1];
        let n = phasekit_mix::Var::N;
        let l = |i: usize, j: usize| c.psi(&[n(i), n(j)]) + if i == j { 1.0 / x[i] } else { 0.0 };
        let (row, largest) = (l(0, 0).abs().max(l(0, 1).abs()), l(1, 1).abs());
        assert!(row <= 1e-13 * largest, "{} at {} K: L*'s first row {row:e} of {largest}", o.label, o.t);
        // To the evidence's printed digits.
        let along = c.psi(&[n(0), n(0), n(0)]) - 1.0 / (x[0] * x[0]);
        assert!((along / cubic - 1.0).abs() < 3e-3, "{} at {} K: {along}", o.label, o.t);
        let near = mix
            .critical_points()
            .iter()
            .any(|p| ((p.t - o.t) / o.t).abs() < 1e-2 && ((p.rho - o.rho) / o.rho).abs() < 1e-2);
        assert!(!near, "{} at {} K: {:?}", o.label, o.t, mix.critical_points());
    }
}

/// The `mix_flash` row of `label` at (T, p), and the registry it is read on.
fn flash_row(label: &str, t: f64, p: f64) -> (mix_flash::FlashRow, Registry) {
    let row = mix_flash::flash_rows().into_iter().find(|o| o.label == label && o.t == t && o.p == p).unwrap();
    (row, Registry::from_embedded(DataSet::Parity).unwrap())
}

/// DIV-0057 (`SkipOracle`, PLAN.md M13.8): at its two rows of v8.0.0's PT flash, (1) the oracle's density reproduces p
/// within `Flash`; (2) dp/dρ ≤ 0 at 131 and 119 of 200 points between 0 and it, so no gas branch reaches it; (3) its
/// g is −13126 and −653017 J/mol, below the liquid root's; (4) phasekit's flash is single-phase on that liquid root,
/// 14510.0 and 20944.5 mol/m³.
fn div_0057() {
    let cases = [
        ("Methane&n-Butane", 224.459_211_179_501_2, 6_192_796.478_490_418, 131, -13126.0, 14510.0),
        ("CarbonDioxide&Methane", 181.292_230_629_320_58, 2_997_846.370_061_397, 119, -653_017.0, 20944.5),
    ];
    for (label, t, p, falling, g, ours) in cases {
        let (o, registry) = flash_row(label, t, p);
        let fluid = mix_flash::binary(&registry, label, o.z1);
        let model = fluid.model().helmholtz().unwrap();
        let rt = model.gas_constant() * t;
        let at = |rho: f64| {
            let d = model.residual(t, rho, Order::Two);
            let (a01, a02) = (d.get(0, 1).unwrap(), d.get(0, 2).unwrap());
            let total = model.ideal(t, rho, Order::One) + model.residual(t, rho, Order::One);
            (
                rho * rt * (1.0 + a01),
                rt * (1.0 + 2.0 * a01 + a02),
                rt * (total.get(0, 0).unwrap() + total.get(0, 1).unwrap()),
            )
        };
        let (p_o, _, g_o) = at(o.rho);
        assert!((p_o - p).abs() <= ToleranceClass::Flash.bound(p).unwrap(), "{label}: p {p_o} against {p}");
        let count = (1..=200).filter(|k| at(o.rho * f64::from(*k) / 200.0).1 <= 0.0).count();
        assert_eq!(count, falling, "{label}");
        assert!((g_o / g - 1.0).abs() < 1e-4, "{label}: g {g_o}");
        let rho = mix_flash::single(&mix_flash::mixture(&fluid).equilibrium(t, p).unwrap()).unwrap();
        assert!((rho / ours - 1.0).abs() < 1e-5 && at(rho).2 > g_o, "{label}: {rho}");
    }
}

/// DIV-0058 (`SkipOracle`, PLAN.md M13.8), near R134a/R1234yf's azeotrope: (1) at z₁ 0.25, 257.34 K the oracle's split
/// is phasekit's with the phases swapped: its Q is 1 − β, its liquid fraction the vapour's, and its "liquid", on its
/// stable root at (T, p), is the dilute phase (91 mol/m³ against its "vapour"'s 11229); the two meet the fugacity
/// equations, so it is the same split, named the other way round; (2) at 0.25, 312.48 K and 0.5, 313.32 K the state lies between the
/// oracle's own bubble and dew pressures (its rows at 1.1 p_bub and 0.9 p_dew), the oracle returns one phase, and
/// phasekit's split meets the fugacity gate and lowers the Gibbs energy; at 313.32 K its compositions differ by less
/// than v8.0.0's gate's 1e-4 spread.
fn div_0058() {
    let label = "R134a&R1234yf";
    let (o, registry) = flash_row(label, 257.337_465_084_179_54, 181_489.100_783_288_2);
    let fluid = mix_flash::binary(&registry, label, o.z1);
    let mix = mix_flash::mixture(&fluid);
    let split = mix.equilibrium(o.t, o.p).unwrap();
    let (beta, x, y, _, _) = mix_flash::parts(&split).unwrap();
    assert!((beta - (1.0 - o.q)).abs() <= 1e-6 && (y[0] - o.x1).abs() <= 1e-7 && (x[0] - o.y1).abs() <= 1e-7);
    let (liquid, vapour) = ([o.x1, 1.0 - o.x1], [o.y1, 1.0 - o.y1]);
    let (rho_l, rho_v) =
        (mix_flash::root(&registry, mix, o.t, o.p, &liquid), mix_flash::root(&registry, mix, o.t, o.p, &vapour));
    let labelled = phasekit_mix::Equilibrium::Split {
        beta: o.q,
        x: liquid.to_vec(),
        y: vapour.to_vec(),
        rho_liquid: rho_l,
        rho_vapour: rho_v,
    };
    assert!(rho_l < 100.0 && rho_v > 11_000.0, "the oracle's liquid at {rho_l}, its vapour at {rho_v}");
    let worst =
        mix_flash::fugacity_differences(mix, o.t, o.p, &labelled).iter().fold(0.0_f64, |m, (d, _)| m.max(d.abs()));
    assert!(worst < 1e-9, "{worst}");
    for (z1, t, p) in
        [(0.25, 312.481_207_602_218_03, 1_036_661.768_427_621_1), (0.5, 313.316_216_669_962_2, 1_071_511.306_009_241_1)]
    {
        let rows: Vec<_> =
            mix_flash::flash_rows().into_iter().filter(|o| o.label == label && o.z1 == z1 && o.t == t).collect();
        let pressures: Vec<f64> = rows.iter().map(|o| o.p).collect();
        let (p_bub, p_dew) = (
            pressures.iter().fold(0.0_f64, |m, p| m.max(*p)) / 1.1,
            pressures.iter().fold(f64::INFINITY, |m, p| m.min(*p)) / 0.9,
        );
        assert!(p_dew < p && p < p_bub, "{t} K: {p_dew} < {p} < {p_bub}");
        assert!(rows.iter().any(|o| o.p == p && o.q == -1.0));
        let fluid = mix_flash::binary(&registry, label, z1);
        let mix = mix_flash::mixture(&fluid);
        let split = mix.equilibrium(t, p).unwrap();
        let (beta, x, y, rho_liquid, rho_vapour) = mix_flash::parts(&split).unwrap();
        assert!(mix_flash::fugacity_differences(mix, t, p, &split).iter().all(|(d, bound)| d.abs() <= *bound));
        let feed =
            mix_flash::gibbs(mix, t, mix_flash::root(&registry, mix, t, p, mix.composition()), mix.composition());
        let g = (1.0 - beta) * mix_flash::gibbs(mix, t, rho_liquid, x) + beta * mix_flash::gibbs(mix, t, rho_vapour, y);
        assert!(g < feed, "{t} K: {g} against {feed}");
        assert_eq!((y[0] - x[0]).abs() < 1e-4, z1 == 0.5, "{t} K");
    }
}

/// DIV-0059 (`KeepOracle`, PLAN.md M13.8): v8.0.0's splits are converged to its second-order stage's 1e-9, phasekit's
/// to the fugacity gate. At R32/R125 z₁ 0.75, 300.6 K, (1) the oracle's β is more than 1e-8 and less than its
/// measured 1e-7 from phasekit's; (2) its phases' fractions, each on its stable root, leave ln f at (T, p) unequal by
/// 5.5e-11, more than 1e-11, where phasekit's split meets the gate's 1e-12 (6.4e-15).
fn div_0059() {
    let label = "R32&R125";
    let row = mix_flash::flash_rows()
        .into_iter()
        .find(|o| o.label == label && o.z1 == 0.75 && o.q > 0.0 && o.t > 300.0)
        .unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let fluid = mix_flash::binary(&registry, label, row.z1);
    let mix = mix_flash::mixture(&fluid);
    let ours = mix.equilibrium(row.t, row.p).unwrap();
    let (beta, ..) = mix_flash::parts(&ours).unwrap();
    let off = ((beta - row.q) / row.q).abs();
    assert!(off > 1e-8 && off < 1e-7, "{off:e}");
    let (liquid, vapour) = ([row.x1, 1.0 - row.x1], [row.y1, 1.0 - row.y1]);
    let (rho_l, rho_v) = (
        mix_flash::root(&registry, mix, row.t, row.p, &liquid),
        mix_flash::root(&registry, mix, row.t, row.p, &vapour),
    );
    let oracle = phasekit_mix::Equilibrium::Split {
        beta: row.q,
        x: liquid.to_vec(),
        y: vapour.to_vec(),
        rho_liquid: rho_l,
        rho_vapour: rho_v,
    };
    let worst = |e: &phasekit_mix::Equilibrium| {
        mix_flash::fugacity_differences(mix, row.t, row.p, e).iter().fold(0.0_f64, |m, (d, _)| m.max(d.abs()))
    };
    assert!(worst(&oracle) > 1e-11 && worst(&ours) <= 1e-12, "{:e} against {:e}", worst(&oracle), worst(&ours));
}

/// DIV-0060 (`KeepOracle`, PLAN.md M13.9): v8.0.0's bubble and dew points end on its step, phasekit's on the fugacity
/// gate. At Methane/n-Butane z₁ 0.75's bubble point at 220.30 K, (1) the oracle's vapour density is more than 1e-7 and
/// less than the measured 5e-7 from phasekit's; (2) its phases, at its own densities and p, leave ln f unequal by
/// 1.5e-6, more than 1e-6, where phasekit's meet the gate's 1e-12 (8.9e-15).
fn div_0060() {
    let label = "Methane&n-Butane";
    let row = mix_sat::sat_rows()
        .into_iter()
        .find(|o| o.label == label && o.z1 == 0.75 && o.given_t && o.q == 0.0 && o.t > 220.0)
        .unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let fluid = mix_sat::row_fluid(&registry, label, row.z1);
    let mix = mix_sat::mixture(&fluid);
    let ours = mix.saturation(phasekit_mix::Side::Bubble, phasekit_mix::Given::T(row.t)).unwrap();
    let off = ((ours.rho_incipient - row.rho_vapour) / row.rho_vapour).abs();
    assert!(off > 1e-7 && off < 5e-7, "{off:e}");
    let worst = |t: f64, p: f64, liquid: (&[f64], f64), vapour: (&[f64], f64)| {
        mix_sat::fugacity_differences(mix, t, p, liquid, vapour).iter().fold(0.0_f64, |m, (d, _)| m.max(d.abs()))
    };
    let (x, y) = ([row.x1, 1.0 - row.x1], [row.y1, 1.0 - row.y1]);
    let oracle = worst(row.t, row.p, (&x, row.rho_liquid), (&y, row.rho_vapour));
    let ((xs, rho_l), (ys, rho_v)) = mix_sat::phases(mix, phasekit_mix::Side::Bubble, &ours);
    let phasekit = worst(ours.t, ours.p, (&xs, rho_l), (&ys, rho_v));
    assert!(oracle > 1e-6 && phasekit <= 1e-12, "{oracle:e} against {phasekit:e}");
}

/// The `mix_flash` row of `label` at z₁, `pair`, T and (for QT and PQ) Q.
fn mix_flash_row(label: &str, z1: f64, pair: &str, t: f64, q: Option<f64>) -> mix_flash::FlashRow {
    mix_flash::rows_of(pair)
        .into_iter()
        .find(|o| o.label == label && o.z1 == z1 && (o.t - t).abs() < 0.01 && q.is_none_or(|q| o.q == q))
        .unwrap()
}

/// DIV-0061 (`SkipOracle`, PLAN.md M13.10): R32/R125 at z₁ 0.5 and 246.72 K, at the bulk density of the oracle's PT
/// split at √(p_bub p_dew): (1) its DT is one phase (Q −1) at the homogeneous state's p(T, ρ), 513.0 kPa, within
/// `Flash`; (2) phasekit's DT is the split at the PT row's p and Q within DIV-0060's 5e-7; (3) the split's molar
/// Helmholtz energy lies below the homogeneous state's.
fn div_0061() {
    let label = "R32&R125";
    let pt = mix_flash::rows_of("PT");
    let twin =
        pt.iter().find(|o| o.label == label && o.z1 == 0.5 && (o.t - 246.716).abs() < 0.01 && o.q > 0.0).unwrap();
    let dt_rows = mix_flash::rows_of("DT");
    let row = dt_rows.iter().find(|o| o.label == label && o.t == twin.t && o.rho == twin.rho).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let fluid = mix_flash::binary(&registry, label, row.z1);
    let dt = |opts: &FlashOptions| {
        fluid.flash(Input::new(phasekit_core::Pair::DT, row.rho, row.t, Basis::Molar).unwrap(), opts).unwrap()
    };
    let homogeneous = dt(&FlashOptions::new().with_phase(Phase::Gas));
    assert!(row.q == -1.0 && (homogeneous.p() / row.p - 1.0).abs() <= 1e-9 && (row.p - 513_000.0).abs() < 100.0);
    let split = dt(&FlashOptions::new());
    let q = split.quality().unwrap();
    assert!((split.p() / twin.p - 1.0).abs() <= 5e-7 && (q / twin.q - 1.0).abs() <= 5e-7, "{} {q}", split.p());
    assert!(
        split.a(Basis::Molar) < homogeneous.a(Basis::Molar),
        "{} {}",
        split.a(Basis::Molar),
        homogeneous.a(Basis::Molar)
    );
}

/// DIV-0062 (`SkipOracle`, PLAN.md M13.10): Methane/n-Butane at z₁ 0.75, QT at 181.43 K and Q 0.5: (1) the oracle's own
/// phases miss the feed, (1 − Q) x₁ + Q y₁ more than 7e-3 from z₁; (2) its p is more than 2e-2 from phasekit's; (3)
/// phasekit's phases add up to the feed within 1e-15 and their fugacities are equal within `Identity`.
fn div_0062() {
    let label = "Methane&n-Butane";
    let row = mix_flash_row(label, 0.75, "QT", 181.425, Some(0.5));
    assert!(((1.0 - row.q) * row.x1 + row.q * row.y1 - row.z1).abs() > 7e-3, "{} {}", row.x1, row.y1);
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let fluid = mix_flash::binary(&registry, label, row.z1);
    let mix = mix_flash::mixture(&fluid);
    let c = mix.coexisting(row.q, phasekit_mix::Given::T(row.t)).unwrap();
    assert!((c.p / row.p - 1.0).abs() > 2e-2, "{} against {}", c.p, row.p);
    for (i, z) in mix.composition().iter().enumerate() {
        assert!(((1.0 - c.beta) * c.x[i] + c.beta * c.y[i] - z).abs() <= 1e-15);
    }
    let differences = mix_sat::fugacity_differences(mix, c.t, c.p, (&c.x, c.rho_liquid), (&c.y, c.rho_vapour));
    assert!(differences.iter().all(|(d, bound)| d.abs() <= *bound), "{differences:?}");
}

/// DIV-0063 (`SkipOracle`, PLAN.md M13.12): Methane/Ethane at z₁ 0.5, the oracle's envelope bubble point at 261.69 K
/// and 6.774 MPa: (1) its phases lie within 0.1 of each other in |ln ρ|, near the critical point; (2) in phasekit's
/// model they leave ln f unequal by more than 5e-8, its Newton's stopping residual; (3) phasekit's bubble point at that
/// T, from the oracle's point, meets M13.9's gate (`Identity`) and lies more than 1e-4 from its bulk density.
fn div_0063() {
    let rows = mix_env::env_rows();
    let row = rows
        .iter()
        .find(|o| o.label == "Methane&Ethane" && o.z1 == 0.5 && o.q == 0.0 && (o.t - 261.687).abs() < 0.01)
        .unwrap();
    assert!(row.near_critical(), "{} {}", row.rho_incipient, row.rho_bulk);
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let fluid = mix_flash::binary(&registry, &row.label, row.z1);
    let mix = mix_flash::mixture(&fluid);
    let (z, incipient) = (vec![row.z1, 1.0 - row.z1], vec![row.x1, 1.0 - row.x1]);
    let oracle = mix_sat::fugacity_differences(mix, row.t, row.p, (&z, row.rho_bulk), (&incipient, row.rho_incipient));
    assert!(oracle.iter().any(|(d, _)| d.abs() > 5e-8), "{oracle:?}");
    let ln_k: Vec<f64> = incipient.iter().zip(&z).map(|(y, x)| math::ln(y / x)).collect();
    let seed = (row.t, row.p, ln_k);
    let s =
        phasekit_mix::internal::saturation_from(mix, phasekit_mix::Side::Bubble, phasekit_mix::Given::T(row.t), seed)
            .unwrap();
    let ours = mix_sat::fugacity_differences(mix, s.t, s.p, (&z, s.rho), (&s.incipient, s.rho_incipient));
    assert!(ours.iter().all(|(d, bound)| d.abs() <= *bound), "{ours:?}");
    assert!((s.rho / row.rho_bulk - 1.0).abs() > 1e-4, "{} against {}", s.rho, row.rho_bulk);
}

/// DIV-0064 (`UsePaper`, PLAN.md M13.13; ROT-199): v8.0.0 stores its predefined blends as mole fractions that its
/// ASHRAE 34 injection derived from the standard's mass fractions with its own molar masses, rounded in older entries;
/// `Corrected` derives them from the mass fractions with the registry's. R411B, R1270/R22/R152a at 3/94/3 % by mass, of
/// the blends v8.0.0 can build the farthest off: (1) under `Corrected` its mole fractions are the mass fractions' within
/// 4 ulp, and its molar mass theirs; (2) the oracle's molar mass misses theirs by more than 1e-7 of it; (3) under
/// `Parity` its molar mass is the oracle's within 4 ulp.
fn div_0064() {
    let mass = [0.03, 0.94, 0.03];
    let molar_mass = |set: DataSet| {
        let registry = Registry::from_embedded(set).unwrap();
        let fluid = phasekit_mix::predefined(&registry, "R411B", set).unwrap();
        let mix = mix_flash::mixture(&fluid);
        let masses: Vec<f64> = mix.model().fluids().map(|f| f.info().molar_mass()).collect();
        (fluid.info().molar_mass(), mix.composition().to_vec(), masses)
    };
    let (corrected, x, masses) = molar_mass(DataSet::Corrected);
    let moles: Vec<f64> = mass.iter().zip(&masses).map(|(w, m)| w / m).collect();
    let total: f64 = moles.iter().sum();
    assert!(x.iter().zip(&moles).all(|(x, n)| (x / (n / total) - 1.0).abs() <= 4.0 * f64::EPSILON), "{x:?}");
    let arbiter = 1.0 / total;
    assert!((corrected / arbiter - 1.0).abs() <= 4.0 * f64::EPSILON, "{corrected} against {arbiter}");
    let oracle = fact("div0064_molar_mass_r411b");
    assert!((oracle / arbiter - 1.0).abs() > 1e-7, "the oracle {oracle} against {arbiter}");
    let (parity, _, _) = molar_mass(DataSet::Parity);
    assert!((parity / oracle - 1.0).abs() <= 4.0 * f64::EPSILON, "{parity} against the oracle {oracle}");
}

/// DIV-0055 (`KeepOracle`, PLAN.md M13.5): AGA8's binary test set comes from an implementation whose θ_k are TM15's
/// ϑ_k T_c, where phasekit's are AGA8's θ_k in K. Nitrogen/isobutane's c_v at 361.72 K and 8 mol/l is 67.8393755004
/// here, past the half unit of the sheet's 67.839375 and within the registered 2e-10 of it; `tests/gerg.rs` checks the
/// whole set at that tolerance.
fn div_0055() {
    let (path, text) = fixture!("aga8/binary.csv");
    let table = Fixture::parse(path, text).unwrap();
    let row = (0..table.rows().len())
        .find(|&r| {
            table.printed(r, "pair") == Some("nitrogen&isobutan")
                && table.printed(r, "T") == Some("361.72")
                && table.printed(r, "D") == Some("8.0")
        })
        .unwrap();
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let fluid = phasekit_mix::gerg2008(&registry, &[("Nitrogen", 0.5), ("IsoButane", 0.5)]).unwrap();
    let input = Input::new(phasekit_core::Pair::DT, 8e3, 361.72, Basis::Molar).unwrap();
    let cv = fluid.flash(input, &FlashOptions::new().with_phase(Phase::Gas)).unwrap().cv(Basis::Molar).unwrap();
    let (want, half) = (table.value(row, "cv").unwrap(), 5e-7);
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0055").unwrap();
    let measured = match entry.tolerance {
        Some(phasekit_verify::Tolerance::Relative(measured)) => measured,
        _ => f64::NAN, // fails the bound below
    };
    assert_eq!(table.printed(row, "cv"), Some("67.839375"));
    assert!((cv - want).abs() > half && (cv - want).abs() <= half + measured * want, "{cv}");
}

/// DIV-0065 (`SkipOracle`, PLAN.md M14.3): the speed of sound of IF97's saturated liquid at 623.15 K. (1) phasekit-iapws's
/// is Eq. (7)'s, evaluated in 50 digits by `scripts/math/if97.py` (576.91493103792562 m/s), within `Prop`; (2) the
/// `if97` kind's row, v8.0.0's, is more than 2e-12 off it; (3) and within the registered 5e-12.
fn div_0065() {
    const W: f64 = 576.914_931_037_925_6;
    let water = phasekit_iapws::if97::water().unwrap();
    let state = water.state(Input::qt(Quality::new(0.0).unwrap(), Temperature::new(623.15).unwrap())).unwrap();
    let ours = state.speed_of_sound().unwrap();
    assert!((ours / W - 1.0).abs() <= 1e-12, "{ours}");
    let (path, text) = fixture!("coolprop-8.0.0/all/if97.csv");
    let table = Fixture::parse(path, text).unwrap();
    let row = (0..table.rows().len())
        .find(|&i| {
            table.printed(i, "pair") == Some("QT")
                && table.value(i, "x") == Some(0.0)
                && table.value(i, "y") == Some(623.15)
        })
        .unwrap();
    let oracle = table.value(row, "speed_sound").unwrap();
    assert!((oracle / W - 1.0).abs() > 2e-12 && (oracle / W - 1.0).abs() <= 5e-12, "{oracle}");
}

/// DIV-0066 (`UsePaper`, PLAN.md M14.4; ROT-247): ice's g₀₀. (1) Under `Corrected` R10-06(2009) Table 6 holds
/// (`tests/iapws.rs::ice_is_table_6`); at its three states (2) `Parity`'s g, h, u and f are `Corrected`'s less the
/// 2006 and 2009 values' difference, 1.13611e-4 J/kg, within 1e-9 J/kg, and (3) its c_p and ρ are `Corrected`'s bit for
/// bit and its s within `Prop` (the state's s comes from bundle terms that both carry g, so g₀₀ cancels to an ulp).
fn div_0066() {
    let ice = |data: DataSet| {
        let water = Registry::from_embedded(data).unwrap().get("Water").unwrap().clone();
        phasekit_iapws::ice::ice(&water, data).unwrap()
    };
    let (corrected, parity) = (ice(DataSet::Corrected), ice(DataSet::Parity));
    let offset = -0.632_020_233_449_497e6 - -0.632_020_233_335_886e6;
    for (t, p) in [(273.16, 611.657), (273.152_519, 101_325.0), (100.0, 100e6)] {
        let at = |fluid: &phasekit_core::Fluid| {
            fluid.state(Input::pt(Pressure::new(p).unwrap(), Temperature::new(t).unwrap())).unwrap()
        };
        let (c, q) = (at(&corrected), at(&parity));
        let m = Basis::Mass;
        for (what, shift) in
            [("g", q.g(m) - c.g(m)), ("h", q.h(m) - c.h(m)), ("u", q.u(m) - c.u(m)), ("f", q.a(m) - c.a(m))]
        {
            assert!((shift - offset).abs() <= 1e-9, "{what} at {t} K: {shift}");
        }
        assert_eq!([q.cp(m).unwrap(), q.rho(m)].map(f64::to_bits), [c.cp(m).unwrap(), c.rho(m)].map(f64::to_bits));
        assert!((q.s(m) - c.s(m)).abs() <= ToleranceClass::Prop.bound(c.s(m).abs()).unwrap(), "s at {t} K");
    }
}

/// DIV-0067 (`KeepOracle`, PLAN.md M18.1; ROT-271): C_aww. (1) Humid air's C_aww is v8.0.0's (`HAProps_Aux`'s Caww
/// within `Prop` at 200, 300 and 400 K); (2) it misses IAPWS G8-10 Table 15's C_aww by more than 5e-5 at each, and
/// (3) by no more than the registered 1.3e-4 (`tests/humidair.rs` holds the derivatives too).
fn div_0067() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let water = registry.get("Water").unwrap().clone();
    let parts = phasekit_humidair::Parts {
        water: water.clone(),
        air: registry.get("Air").unwrap().clone(),
        liquid: phasekit_iapws::if97::water().unwrap(),
        ice: phasekit_iapws::ice::ice(&water, DataSet::Parity).unwrap(),
    };
    let air = phasekit_humidair::HumidAir::new(parts, DataSet::Parity).unwrap();
    let (path, text) = fixture!("coolprop-8.0.0/all/humidair_aux.csv");
    let oracle = Fixture::parse(path, text).unwrap();
    for (t, g8_10) in [(200.0, -0.349_872_634e-5), (300.0, -0.115_552_784e-6), (400.0, -0.200_806_021e-7)] {
        let ours = air.virials(t).unwrap().c_aww.value;
        let row = (0..oracle.rows().len()).find(|&r| oracle.value(r, "T") == Some(t)).unwrap();
        let want = oracle.value(row, "Caww").unwrap();
        assert!((ours - want).abs() <= ToleranceClass::Prop.bound(want.abs()).unwrap(), "{t} K: {ours} {want}");
        let gap = (ours / g8_10 - 1.0).abs();
        assert!(gap > 5e-5 && gap <= 1.3e-4, "{t} K: {gap}");
    }
}
