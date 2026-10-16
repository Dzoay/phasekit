//! The divergence register is consistent with the corrections shipped in the data (VERIFICATION.md §7.2): the tests
//! decode every embedded record and collect its patches (PLAN.md M2.7).

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::{Edit, FluidRecord, IdealTerm, Patch, SaFreshness};
use phasekit_core::{DataSet, Density, Input, Order, Prop, Registry, Temperature};
use phasekit_verify::{
    Cell, DIVERGENCES, Fix, Fixture, Policy, Provenance, RegisterError, Tolerance, check_register, fixture,
    from_printed,
};

/// Every embedded fluid's record, as decoded (uncorrected).
fn records() -> Vec<FluidRecord> {
    let registry = Registry::embedded().unwrap();
    phasekit_data::FLUIDS.iter().map(|f| phasekit_core::internal::record(registry, f.name).unwrap()).collect()
}

/// The patches the embedded data ships.
fn seed_corrections() -> Vec<Patch> {
    records().into_iter().flat_map(|r| r.corrections).collect()
}

fn patch(id: &str, edit: Edit) -> Patch {
    Patch { divergence: id.into(), edit }
}

#[test]
fn corrections_cite_use_paper_entries() {
    assert_eq!(check_register(DIVERGENCES, &seed_corrections()), Ok(()));
    let skip = [patch("DIV-0004", Edit::GasConstant(8.314))];
    assert_eq!(check_register(DIVERGENCES, &skip), Err(RegisterError::NotUsePaper("DIV-0004".into())));
    let unknown = [patch("DIV-9999", Edit::GasConstant(8.314))];
    assert_eq!(check_register(DIVERGENCES, &unknown), Err(RegisterError::UnknownId("DIV-9999".into())));
}

/// DIV-0005 (Helium) is an accepted divergence: CoolProp's R is kept, so no correction may cite it and
/// Corrected equals Parity for helium (docs/DECISIONS.md, question 5).
#[test]
fn accepted_divergences_ship_no_patch() {
    let helium = DIVERGENCES.iter().find(|d| d.id == "DIV-0005").unwrap();
    assert_eq!((helium.fluids, helium.policy, helium.fix), (&["Helium"][..], Policy::KeepOracle, Fix::None));
    let table_1_r = [patch("DIV-0005", Edit::GasConstant(8.314_472))];
    assert_eq!(check_register(DIVERGENCES, &table_1_r), Err(RegisterError::NotUsePaper("DIV-0005".into())));
}

#[test]
fn use_paper_entries_name_an_arbiter() {
    for d in DIVERGENCES.iter().filter(|d| d.policy == Policy::UsePaper) {
        assert!(d.arbiter.is_some(), "{}", d.id);
    }
    assert!(Provenance::Iapws { release: "R14-08" }.is_arbiter());
    assert!(!Provenance::Oracle { version: "8.0.0" }.is_arbiter());
    assert!(!Provenance::SelfReferential.is_arbiter());
}

/// VERIFICATION.md §7.2: every `UsePaper` entry with `fix: Data` is implemented by at least one patch, and no patch
/// cites an entry whose fix is not data.
#[test]
fn use_paper_data_fixes_are_cited_by_a_patch() {
    let without_0002: Vec<Patch> = seed_corrections().into_iter().filter(|p| &*p.divergence != "DIV-0002").collect();
    assert_eq!(check_register(DIVERGENCES, &without_0002), Err(RegisterError::UncitedDataFix("DIV-0002")));
    let mut code = DIVERGENCES.to_vec();
    if let Some(entry) = code.iter_mut().find(|d| d.id == "DIV-0003") {
        entry.fix = Fix::Code("phasekit_core::data");
    }
    assert_eq!(check_register(&code, &seed_corrections()), Err(RegisterError::NotDataFix("DIV-0003".into())));
    // A UsePaper entry fixed in code (the M11 cubic gas constant) needs no patch.
    let without_0003: Vec<Patch> = seed_corrections().into_iter().filter(|p| &*p.divergence != "DIV-0003").collect();
    assert_eq!(check_register(&code, &without_0003), Ok(()));
}

/// Every entry's evidence cites a map item ("map NN"), so each divergence traces to its measurement.
#[test]
fn every_entry_cites_a_map_id() {
    let mut uncited = DIVERGENCES.to_vec();
    if let Some(entry) = uncited.iter_mut().find(|d| d.id == "DIV-0007") {
        entry.evidence = "measured once, somewhere";
    }
    assert_eq!(check_register(&uncited, &seed_corrections()), Err(RegisterError::NoMapCitation("DIV-0007")));
    assert!(DIVERGENCES.iter().all(|d| d.proof.iter().all(|m| (2..=19).contains(m))), "proof milestones are M2-M19");
}

/// The `facts` row named `name`: its status and value (NaN for a failed call).
fn fact<'a>(facts: &Fixture<'a>, name: &str) -> Option<(&'a str, f64)> {
    let row = facts.rows().iter().position(|row| row.cells.first() == Some(&Cell::Text(name)))?;
    match facts.rows()[row].cells.get(2)? {
        Cell::Text(status) => Some((status, facts.value(row, "value")?)),
        Cell::Num(_) | Cell::Blank => None,
    }
}

/// `value` is within half a unit in the last digit of `figure`, the value a map section or the register cites.
fn cites(value: f64, figure: &str) -> bool {
    match (from_printed(figure), figure.parse::<f64>()) {
        (Some(Tolerance::Absolute(half)), Ok(want)) => (value - want).abs() <= half,
        _ => false,
    }
}

/// The relative differences of the oracle from a printed table's column at the given rows, as (min, max).
fn residuals(table: &Fixture, column: &str, scale: f64, oracle: impl Fn(usize) -> f64, rows: &[usize]) -> (f64, f64) {
    let rel = |row: usize| table.value(row, column).map(|printed| (oracle(row) / scale - printed) / printed);
    let rel: Vec<f64> = rows.iter().map(|&row| rel(row).unwrap_or(f64::NAN)).collect();
    (rel.iter().copied().fold(f64::INFINITY, f64::min), rel.iter().copied().fold(f64::NEG_INFINITY, f64::max))
}

/// The value of the fact `name`, whose call must have succeeded.
fn ok_fact(facts: &Fixture<'_>, name: &str) -> f64 {
    let found = fact(facts, name);
    assert!(matches!(found, Some(("ok", _))), "fact `{name}`: {found:?}");
    found.map_or(f64::NAN, |(_, value)| value)
}

/// A committed paper table.
fn paper((path, text): (&'static str, &'static str)) -> Fixture<'static> {
    Fixture::parse(path, text).map_err(|e| e.to_string()).unwrap()
}

/// The facts of DIV-0001 to DIV-0005, each asserted; returns the entries checked.
fn div_0001_to_0005(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    let mut checked = Vec::new();

    // DIV-0001: the stored R is 8.314472, and the oracle's p is +1.0e-6..+1.4e-6 above Thol's Table 3 at the five
    // states with ρ > 0 (map 13 R1).
    assert_eq!(ok("div0001_gas_constant"), 8.314_472);
    let thol = paper(fixture!("paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.csv"));
    let (lo, hi) = residuals(&thol, "p", 1e6, |row| ok(&format!("div0001_p_row{row}")), &[0, 1, 2, 4, 5]);
    assert!(cites(lo, "1.0e-6") && cites(hi, "1.4e-6"), "DIV-0001: {lo:e}..{hi:e}");
    checked.push("DIV-0001");

    // DIV-0002: ice VI with p0 = 623.4 MPa: T(1356.76 MPa) = 320.965 K, p(320 K) = 1337.45 MPa (map 10 R10).
    assert!(cites(ok("div0002_melting_t_1356.76mpa"), "320.965"));
    assert!(cites(ok("div0002_melting_p_320k") / 1e6, "1337.45"));
    checked.push("DIV-0002");

    // DIV-0003: Nitrogen's reducing density (map 12 §6.3).
    assert_eq!(ok("div0003_rhomolar_reducing"), 11183.901464580624);
    checked.push("DIV-0003");

    // DIV-0004: a two-phase viscosity, returned without error (map 12 §6.3).
    assert!(cites(ok("div0004_eta_t500_q0.5"), "1.6048e-5"));
    checked.push("DIV-0004");

    // DIV-0005: the stored R is 8.3144598, and the oracle's p, c_v and w are −4.8e-7..−0.9e-7 off NIST IR 8474
    // Table 3 at all six states (map 13 R3).
    assert_eq!(ok("div0005_gas_constant"), 8.314_459_8);
    let helium = paper(fixture!("paper/Helium/OrtizVega-JPCRD-2019.3.csv"));
    let rows = [0, 1, 2, 3, 4, 5];
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (column, scale) in [("p", 1e6), ("cv", 1.0), ("w", 1.0)] {
        let (l, h) = residuals(&helium, column, scale, |row| ok(&format!("div0005_{column}_row{row}")), &rows);
        (lo, hi) = (lo.min(l), hi.max(h));
    }
    assert!(cites(lo, "-4.8e-7") && cites(hi, "-0.9e-7"), "DIV-0005: {lo:e}..{hi:e}");
    checked.push("DIV-0005");
    checked
}

/// The facts of DIV-0006 to DIV-0012, each asserted; returns the entries checked.
fn div_0006_to_0012(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    let mut checked = Vec::new();

    // DIV-0006..0008: the v8.0.0 reducing densities (and molar masses) corrected upstream in 2acbbc82.
    assert_eq!((ok("div0006_rhomolar_reducing"), ok("div0006_molar_mass")), (7636.76598074554, 0.02805376));
    assert_eq!((ok("div0007_rhomolar_reducing"), ok("div0007_molar_mass")), (15444.54031369981, 0.00201594));
    assert_eq!(ok("div0008_rhomolar_reducing"), 1514.916863638556);
    checked.extend(["DIV-0006", "DIV-0007", "DIV-0008"]);

    // DIV-0009: R1233zd(E) has no viscosity model in v8.0.0.
    assert_eq!(fact(facts, "div0009_eta_t300_p101325").map(|(status, _)| status), Some("err:notimpl"));
    checked.push("DIV-0009");

    // DIV-0010: PR propane at 400 K, 1 bar: T (∂s/∂T)_p = 91.35 against c_p = 93.89 J/(mol K).
    let ds = ok("div0010_smolar_t400.01") - ok("div0010_smolar_t399.99");
    assert!(cites(400.0 * ds / (400.01 - 399.99), "91.35") && cites(ok("div0010_cpmolar_t400"), "93.89"));
    checked.push("DIV-0010");

    // DIV-0011: C from δ = 1e-12 against ∂²αʳ/∂δ² extrapolated to ρ = 0 through 1..4 mol/m³ (cubic Lagrange
    // weights 4, −6, 4, −1): −6.7e-5, −7.1e-5, +1.9e-5 for Propane 300 K, N₂ 300 K, Water 600 K.
    for (fluid, figure) in [("propane", "-6.7e-5"), ("nitrogen", "-7.1e-5"), ("water", "1.9e-5")] {
        let d2 = |rho: u8| ok(&format!("div0011_d2alphar_ddelta2_{fluid}_rho{rho}"));
        let at_zero = 4.0 * d2(1) - 6.0 * d2(2) + 4.0 * d2(3) - d2(4);
        let rho_r = ok(&format!("div0011_rhomolar_reducing_{fluid}"));
        let exact = at_zero / (rho_r * rho_r);
        let error = (ok(&format!("div0011_cvirial_{fluid}")) - exact) / exact;
        assert!(cites(error, figure), "DIV-0011 {fluid}: {error:e}");
    }
    checked.push("DIV-0011");

    // DIV-0012: Water below Tmin, (55018.5 mol/m³, 250 K), gives a pressure without error.
    assert_eq!(fact(facts, "div0012_p_t250_rho55018.5"), Some(("ok", -5.9277123935677105)));
    checked.push("DIV-0012");
    checked
}

/// The facts of DIV-0013 to DIV-0017, each asserted; returns the entries checked.
fn div_0013_to_0017(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    let mut checked = Vec::new();

    // DIV-0013: R1234yf + R1234ze(E), z₁ = 0.4, 469 K, 3399 mol/m³: αʳ = −0.464679 against Bell's −0.460595.
    assert!(cites(ok("div0013_alphar_t469_rho3399"), "-0.464679"));
    checked.push("DIV-0013");

    // DIV-0014: R1224YDZ p(400 K, 8000 mol/m³) = 21.1790735 MPa, 3.3 half units below the printed 21.17909.
    let p = ok("div0014_p_t400_rho8000");
    let printed = paper(fixture!("paper/R1224YDZ/Akasaka-IJT-2023-R1224ydZ.7.csv"));
    let half_units = (printed.value(0, "p").unwrap_or(f64::NAN) - p) / 5.0;
    assert!(cites(p / 1e6, "21.1790735") && cites(half_units, "3.3"), "DIV-0014: {half_units}");
    checked.push("DIV-0014");

    // DIV-0015: R123's T_r is 456.831 K, and the oracle's c_p⁰ at 300 K is the one its c_p⁰ blocks give with their own
    // Tc = 456.82 K: Parity's, within 1e-12.
    assert_eq!(ok("div0015_t_reducing"), 456.831);
    let r123 = phasekit_core::internal::record(Registry::embedded().unwrap(), "R123").unwrap();
    let (r, rho_r) = (r123.eos.gas_constant, r123.eos.rho_reducing);
    let parity = r * (1.0 - r123.compile().unwrap().eos().ideal(300.0, rho_r, Order::Two).get(2, 0).unwrap());
    assert!((ok("div0015_cp0molar_t300") / parity - 1.0).abs() < 1e-12, "DIV-0015");
    checked.push("DIV-0015");

    // DIV-0016: CoolProp's own VLE (superancillaries off) at PropyleneGlycol's triple point is phasekit's, within `Prop`.
    let pg = phasekit_core::internal::record(Registry::embedded().unwrap(), "PropyleneGlycol").unwrap();
    let sa = pg.superancillary_curve().unwrap().at_t(213.0).unwrap();
    let sat =
        phasekit_core::internal::vle_at_t(pg.compile().unwrap().eos(), 213.0, (sa.bubble.rho, sa.dew.rho)).unwrap();
    for (name, ours) in
        [("div0016_rhomolar_liquid_t213", sat.bubble.rho), ("div0016_rhomolar_vapour_t213", sat.dew.rho)]
    {
        assert!((ok(name) / ours - 1.0).abs() < 1e-12, "DIV-0016: {name}");
    }
    checked.push("DIV-0016");

    // DIV-0017: the oracle's computed critical points (the crit rows, the register's facts for it) of DimethylCarbonate
    // and Chlorine, with their K2 = 2A01 + 4A02 + A03 in phasekit's EOS: 7.5e-11 at DimethylCarbonate's (on a flat
    // isotherm), within 1e-11 at Chlorine's; phasekit's own Chlorine point is 2.6 % away in ρ.
    let (path, text) = fixture!("coolprop-8.0.0/all/crit.csv");
    let crit = Fixture::parse(path, text).unwrap();
    let k2_at_oracle = |name: &str| {
        let r = (0..crit.rows().len()).find(|&r| crit.printed(r, "fluid") == Some(name)).unwrap();
        let (t, rho) = (crit.value(r, "Tc_num").unwrap(), crit.value(r, "rhoc_num").unwrap());
        let fluid = phasekit_core::internal::record(Registry::embedded().unwrap(), name).unwrap().compile().unwrap();
        let d = fluid.eos().ideal(t, rho, Order::Three) + fluid.eos().residual(t, rho, Order::Three);
        let a = |j| d.get(0, j).unwrap();
        (2.0 * a(1) + 4.0 * a(2) + a(3), rho)
    };
    let (dmc, _) = k2_at_oracle("DimethylCarbonate");
    let (chlorine, rho) = k2_at_oracle("Chlorine");
    assert!(dmc > 5e-11 && chlorine.abs() < 1e-11 && (rho / 7949.83 - 1.0).abs() < 1e-6, "DIV-0017: {dmc}, {chlorine}");
    checked.push("DIV-0017");
    checked
}

/// The facts of DIV-0018 to DIV-0020, each asserted; returns the entries checked.
fn div_0018_to_0020(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    let mut checked = Vec::new();

    // DIV-0018: in all/sat.csv, the oracle's PQ at R245fa's p at Θ = 1e-7 misses the QT row that gave it that p by
    // 2.0e-11 in T and 1.1e-6 in ρ (its T(ln p) inverse, unpolished).
    let (path, text) = fixture!("coolprop-8.0.0/all/sat.csv");
    let sat = Fixture::parse(path, text).unwrap();
    let last = |input| {
        let rows = 0..sat.rows().len();
        rows.rev().find(|&r| sat.printed(r, "fluid") == Some("R245fa") && sat.printed(r, "input") == Some(input))
    };
    let (qt, pq) = (last("T").unwrap(), last("p").unwrap());
    assert_eq!(sat.value(qt, "p"), sat.value(pq, "p"));
    let miss = |column| (sat.value(pq, column).unwrap() / sat.value(qt, column).unwrap() - 1.0).abs();
    let (t, rho) = (miss("T"), miss("rhoL").max(miss("rhoV")));
    assert!(cites(t, "2.0e-11") && cites(rho, "1.1e-6"), "DIV-0018: {t:e}, {rho:e}");
    checked.push("DIV-0018");

    // DIV-0019: CoolProp's in-dome p of SES36 at 206.2675 K differs between two densities of its dome (by 6.9e-10), and
    // phasekit's (the pure VLE's, the same at both) is 1e-8 from either.
    let [p1, p2] = ["div0019_p_t206.2675_rho1.97058", "div0019_p_t206.2675_rho0.656961"].map(ok);
    let ses36 = Registry::from_embedded(DataSet::Parity).unwrap();
    let dt = |rho: f64| Input::dt(Density::molar(rho).unwrap(), Temperature::new(206.2675).unwrap());
    let ours = [1.97058, 0.656961].map(|rho| ses36.get("SES36").unwrap().state(dt(rho)).unwrap().p());
    assert_eq!(ours[0], ours[1], "DIV-0019: the VLE's p");
    let apart = |a: f64, b: f64| (a / b - 1.0).abs();
    assert!(cites(apart(p2, p1), "6.9e-10") && apart(ours[0], p1) > 9e-9, "DIV-0019: {p1} {p2} {ours:?}");
    checked.push("DIV-0019");

    // DIV-0020: in flash/Nitrogen.csv, the oracle's PT row at 107.45 MPa and 1192.98 K reports a p 7.9e-9 off its input
    // and off the DT row read off it, whose p at the same (T, ρ) is the input to 2.2e-16.
    let (path, text) = fixture!("coolprop-8.0.0/flash/Nitrogen.csv");
    let flash = Fixture::parse(path, text).unwrap();
    let pt = (0..flash.rows().len())
        .find(|&r| {
            let near = |column, want: f64| (flash.value(r, column).unwrap() / want - 1.0).abs() < 1e-4;
            flash.printed(r, "pair") == Some("PT") && near("x1", 107.45e6) && near("x2", 1192.98)
        })
        .unwrap();
    let (input, reported, dt) = (flash.value(pt, "x1").unwrap(), flash.value(pt, "p").unwrap(), pt + 1);
    assert_eq!((flash.printed(dt, "pair"), flash.value(dt, "x1")), (Some("DT"), flash.value(pt, "rho")));
    let own = flash.value(dt, "p").unwrap();
    let apart = |a: f64, b: f64| (a / b - 1.0).abs();
    assert!(cites(apart(reported, input), "7.9e-9") && cites(apart(reported, own), "7.9e-9"), "DIV-0020: {reported}");
    assert!(cites(apart(own, input), "2.2e-16"), "DIV-0020: {own}");
    // Its h too: in flash/Air.csv at 685.58 MPa and 1514.94 K, 6.1e-10 off the DT row read off the same PT row.
    let (path, text) = fixture!("coolprop-8.0.0/flash/Air.csv");
    let air = Fixture::parse(path, text).unwrap();
    let pt = (0..air.rows().len())
        .find(|&r| {
            let near = |column, want: f64| (air.value(r, column).unwrap() / want - 1.0).abs() < 1e-4;
            air.printed(r, "pair") == Some("PT") && near("x1", 685.58e6) && near("x2", 1514.94)
        })
        .unwrap();
    let h = |row| air.value(row, "h").unwrap();
    assert!(cites(apart(h(pt), h(pt + 1)), "6.1e-10"), "DIV-0020: {} and {}", h(pt), h(pt + 1));
    checked.push("DIV-0020");
    checked
}

/// DIV-0021 and DIV-0022: the oracle's η at the transport check rows its correlations miss, as far off the printed
/// values (CoolProp-Tests.cpp, in Pa s) as the entries cite.
fn div_0021_to_0022(facts: &Fixture<'_>) -> Vec<&'static str> {
    let off = |name: &str, printed: f64| ok_fact(facts, name) / printed - 1.0;
    assert!(cites(off("div0021_eta_sf6_t300_rho0", 15.2887e-6), "-3.9e-5"), "DIV-0021: SF6 at 300 K");
    assert!(cites(off("div0021_eta_sf6_t400_rho0", 19.6796e-6), "-3.1e-5"), "DIV-0021: SF6 at 400 K");
    assert!(cites(off("div0021_eta_r125_t300", 177.37e-6), "9.3e-5"), "DIV-0021: R125 at 300 K");
    assert!(cites(off("div0022_eta_ammonia_t200", 507.28e-6), "-1.1e-5"), "DIV-0022: Ammonia");
    assert!(cites(off("div0022_eta_r123_t265", 627.1e-6), "1.15e-4"), "DIV-0022: R123");
    assert!(cites(off("div0022_eta_noctane_t300", 553.60e-6), "-4.9e-5"), "DIV-0022: n-Octane");
    assert!(cites(off("div0022_eta_npropane_t150", 656.9e-6), "-3.3e-3"), "DIV-0022: n-Propane");
    vec!["DIV-0021", "DIV-0022"]
}

/// DIV-0023 to DIV-0025: the oracle's λ at the conductivity check rows off their printed digits, as far off the printed
/// values (CoolProp-Tests.cpp, in W/(m K)) as the entries cite; n-Pentane's η there, against the 49.465 µPa s its
/// correlation was fitted with.
fn div_0023_to_0025(facts: &Fixture<'_>) -> Vec<&'static str> {
    let off = |name: &str, printed: f64| ok_fact(facts, name) / printed - 1.0;
    let near_critical = [
        ("div0023_lambda_air_t132.64", 75.6231e-3, "-4.5e-6"),
        ("div0023_lambda_argon_t150.69", 856.793e-3, "-6.8e-6"),
        ("div0023_lambda_nitrogen_t126.195", 675.800e-3, "-5.4e-6"),
        ("div0023_lambda_oxygen_t154.6", 377.476e-3, "1.2e-5"),
        ("div0023_lambda_sf6_t310", 48.705e-3, "-1.4e-5"),
        ("div0023_lambda_nonane_t300", 0.13031, "5.0e-5"),
    ];
    for (name, printed, cited) in near_critical {
        assert!(cites(off(name, printed), cited), "DIV-0023: {name} is {:e} off", off(name, printed));
    }
    let methane = [
        ("div0024_lambda_methane_t400", 49.96e-3, "-1.4e-4"),
        ("div0024_lambda_methane_t182_q0", 82.5e-3, "1.2e-3"),
        ("div0024_lambda_methane_t100", 234e-3, "-8.8e-3"),
    ];
    for (name, printed, cited) in methane {
        assert!(cites(off(name, printed), cited), "DIV-0024: {name} is {:e} off", off(name, printed));
    }
    assert!(cites(off("div0025_lambda_npentane_t460", 71.300e-3), "4.75e-2"), "DIV-0025: lambda");
    assert!(cites(ok_fact(facts, "div0025_eta_npentane_t460") * 1e6, "27.15"), "DIV-0025: eta");
    vec!["DIV-0023", "DIV-0024", "DIV-0025"]
}

/// DIV-0026: the oracle's λ at the all-fluid tier's states of the five fluids, where CoolProp keeps q_D = 2e9 1/m, as far
/// from phasekit's, which reads each file's q_D, as the entry cites (the extreme relative difference per fluid).
fn div_0026(_: &Fixture<'_>) -> Vec<&'static str> {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (path, text) = fixture!("coolprop-8.0.0/all/transport.csv");
    let table = Fixture::parse(path, text).unwrap();
    let cited = [("R13", "1.9e-2"), ("R14", "1.8e-1"), ("R142b", "-2.0e-3"), ("R218", "-3.2e-3"), ("RC318", "1.1e-1")];
    for (name, figure) in cited {
        let fluid = registry.get(name).unwrap();
        let rows = (0..table.rows().len()).filter(|&r| table.printed(r, "fluid") == Some(name));
        let off = rows.filter(|&r| table.printed(r, "lambda_status") == Some("ok")).map(|r| {
            let (t, rho) = (table.value(r, "T").unwrap(), table.value(r, "rho").unwrap());
            let state = fluid.state(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap())).unwrap();
            fluid.prop(&state, Prop::Conductivity).unwrap() / table.value(r, "lambda").unwrap() - 1.0
        });
        let extreme = off.fold(0.0_f64, |e, x| if x.abs() > e.abs() { x } else { e });
        assert!(cites(extreme, figure), "DIV-0026: {name} is {extreme:e} off");
    }
    vec!["DIV-0026"]
}

/// DIV-0027: CoolProp's dilute λ of EthylBenzene is Mylona et al.'s Eq. (18) (its oracle column `lambda_dilute` at the
/// all-fluid tier's eight states), which at 500 and 600 K is as far below their Table 21 as the entry cites.
fn div_0027(facts: &Fixture<'_>) -> Vec<&'static str> {
    let (path, text) = fixture!("coolprop-8.0.0/all/transport.csv");
    let table = Fixture::parse(path, text).unwrap();
    let eq_18 = |t: f64| {
        let tr = t / 617.12;
        let a = [-1.10708, 10.8026, -28.9015, 41.9227, 20.9133, -4.01492];
        a.iter().rev().fold(0.0, |s, a| s * tr + a) / (0.259475 - 0.343879 * tr + tr * tr) * 1e-3
    };
    let rows: Vec<usize> =
        (0..table.rows().len()).filter(|&r| table.printed(r, "fluid") == Some("EthylBenzene")).collect();
    for &r in &rows {
        let (t, dilute) = (table.value(r, "T").unwrap(), table.value(r, "lambda_dilute").unwrap());
        assert!((dilute / eq_18(t) - 1.0).abs() < 1e-12, "DIV-0027: {t} K, {dilute} against {}", eq_18(t));
    }
    assert_eq!(rows.len(), 8);
    assert!(cites(eq_18(500.0) * 1e3 / 29.16 - 1.0, "-1.8e-4") && cites(eq_18(600.0) * 1e3 / 41.14 - 1.0, "-1.3e-4"));
    // o-xylene's Table 6 at 0 MPa and 300 K, 13.68 mW/(m K), against the oracle's Eq. (12).
    assert!(cites(ok_fact(facts, "div0027_lambda_oxylene_t300") / 13.68e-3 - 1.0, "-3.9e-4"), "DIV-0027: o-xylene");
    vec!["DIV-0027"]
}

/// DIV-0028 and DIV-0029: the oracle evaluates R1234yf's viscosity with its stored (ρ s_r)_c, and the four fluids'
/// ECS conductivity with Chung's Lennard-Jones estimate: phasekit, with both as stored, matches its η and λ at the
/// all-fluid tier's states (`TransportDirect` and `TransportEcs`).
fn div_0028_to_0029(_: &Fixture<'_>) -> Vec<&'static str> {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (path, text) = fixture!("coolprop-8.0.0/all/transport.csv");
    let table = Fixture::parse(path, text).unwrap();
    let ecs = |name| (name, Prop::Conductivity, "lambda", 1e-8);
    let cases = [("R1234yf", Prop::Viscosity, "eta", 1e-12), ecs("R124"), ecs("R22"), ecs("R245fa"), ecs("R32")];
    for (name, prop, column, bound) in cases {
        let fluid = registry.get(name).unwrap();
        let rows = (0..table.rows().len()).filter(|&r| table.printed(r, "fluid") == Some(name));
        let rows: Vec<usize> = rows.filter(|&r| table.printed(r, &format!("{column}_status")) == Some("ok")).collect();
        for &r in &rows {
            let (t, rho) = (table.value(r, "T").unwrap(), table.value(r, "rho").unwrap());
            let state = fluid.state(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap())).unwrap();
            let want = table.value(r, column).unwrap();
            let got = fluid.prop(&state, prop).unwrap();
            assert!((got / want - 1.0).abs() <= bound, "{name} row {r}: {got} against {want}");
        }
        assert!(rows.len() >= 7, "{name}");
    }
    vec!["DIV-0028", "DIV-0029"]
}

/// DIV-0030 to DIV-0034: the oracle at M8.9's check rows. Ammonia's λ at 405.3999 K and 50 kg/m³ is the 6.26 W/(m K)
/// the entry cites and at 405.4 K the oracle has none; the others are as far off the printed values (CoolProp-Tests.cpp)
/// as the entries cite, and the viscosities DIV-0032's critical terms read are the cited ones.
fn div_0030_to_0034(facts: &Fixture<'_>) -> Vec<&'static str> {
    assert!(cites(ok_fact(facts, "div0030_lambda_ammonia_t405.3999_rho50"), "6.26"), "DIV-0030");
    assert!(
        matches!(fact(facts, "div0030_lambda_ammonia_t405.4_rho50"), Some((status, v)) if status.starts_with("err") && v.is_nan())
    );
    let off = |name: &str, printed: f64| ok_fact(facts, name) / printed - 1.0;
    let cited = [
        ("div0031_eta_benzene_t300", 608.52e-6, "1.8e-5"),
        ("div0031_eta_heptane_t250", 725.69e-6, "-1.0e-4"),
        ("div0031_eta_methanol_t300", 0.5422e-3, "-1.3e-4"),
        ("div0032_lambda_cyclopentane_t512", 69.698e-3, "-1.81e-2"),
        ("div0032_lambda_isopentane_t460", 59.649e-3, "-4.4e-3"),
        ("div0032_lambda_heptane_t535", 51.655e-3, "-2.9e-3"),
        ("div0032_lambda_hexane_t400", 129.28e-3, "-2.3e-4"),
        ("div0033_eta_ethane_t100", 878.6e-6, "1.7e-3"),
        ("div0033_lambda_ethane_t440", 45.9e-3, "-4.3e-3"),
        ("div0034_eta_r23_t180", 353.88e-6, "-8.9e-5"),
        ("div0034_eta_co2_t300", 0.50594e-3, "1.05e-5"),
        ("div0034_lambda_r123_t180", 2.473e-3, "-9.6e-4"),
    ];
    for (name, printed, figure) in cited {
        assert!(cites(off(name, printed), figure), "{name} is {:e} off", off(name, printed));
    }
    let eta = [("div0032_eta_cyclopentane_t512", "47.82"), ("div0032_eta_isopentane_t460", "37.20")];
    for (name, figure) in eta.into_iter().chain([("div0032_eta_heptane_t535", "15.726")]) {
        assert!(cites(ok_fact(facts, name) * 1e6, figure), "{name}");
    }
    vec!["DIV-0030", "DIV-0031", "DIV-0032", "DIV-0033", "DIV-0034"]
}

/// DIV-0035: the oracle's helium λ is NaN in both lobes of its critical term's negative blend.
fn div_0035(facts: &Fixture<'_>) -> Vec<&'static str> {
    for name in [
        "div0035_lambda_helium_t5.5_rho17000",
        "div0035_lambda_helium_t5.5_rho18000",
        "div0035_lambda_helium_t5.9_rho15000",
    ] {
        assert!(matches!(fact(facts, name), Some(("ok", v)) if v.is_nan()), "{name}");
    }
    vec!["DIV-0035"]
}

/// DIV-0036: Hydrogen's melting line at its triple point, three orders of magnitude above its triple pressure, and at
/// 20 K twice ParaHydrogen's; OrthoHydrogen's at its own triple point.
fn div_0036(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    assert!(cites(ok("div0036_melting_p_hydrogen_t13.957") / 1e6, "23.6"));
    assert!(cites(ok("div0036_ptriple_hydrogen") / 1e3, "7.36"));
    assert!(cites(ok("div0036_melting_p_hydrogen_t20") / 1e6, "44.9"));
    assert!(cites(ok("div0036_melting_p_parahydrogen_t20") / 1e6, "22.7"));
    assert!(ok("div0036_melting_p_orthohydrogen_t14.008") > 1e3 * ok("div0036_ptriple_orthohydrogen"));
    vec!["DIV-0036"]
}

/// DIV-0037 to DIV-0045 (M10.2, the props kind): the oracle's Q sentinel and a pseudo-pure gas's Qmass, R134a's GWP100,
/// the refused `rhomass_reducing`, Ammonia's T_triple (its T_min), σ refused at a gas, the trivial output and echo
/// answered at a refused pseudo-pure QT state, a pseudo-pure g refused where h is given, Z inside the dome and at
/// PropyleneGlycol's saturated vapour against its own p/(ρRT), and the two ECS values the entry cites.
fn div_0037_to_0045(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    let refused = |name: &str| matches!(fact(facts, name), Some((status, _)) if status.starts_with("err:"));
    assert_eq!((ok("div0037_q_water_t400_rho10"), ok("div0037_qmass_r407c_t291.68")), (-1.0, 1.0));
    assert!(cites(ok("div0038_gwp100_r134a"), "1430"));
    assert!(refused("div0039_rhomass_reducing_water"));
    assert!(cites(ok("div0040_t_triple_ammonia"), "195.495"));
    assert!(refused("div0041_sigma_r125_t318.0"));
    assert!(ok("div0042_tcrit_air_q0.5_t100").is_finite() && refused("div0042_h_air_q0.5_t100"));
    assert!(refused("div0043_g_air_q0_t80") && cites(ok("div0043_h_air_q0_t80"), "61.749"));
    assert!(cites(ok("div0044_z_r1234yf_t203.94"), "-0.164"));
    let z = ok("div0044_z_propyleneglycol_q1_t225.99");
    let (p, rho) = (ok("div0044_p_propyleneglycol_q1_t225.99"), ok("div0044_dmolar_propyleneglycol_q1_t225.99"));
    let own = p / (rho * ok("div0044_gas_constant_propyleneglycol") * 225.991_054_973_623_68);
    assert!(cites(z, "1.0") && cites(own, "1.47") && cites((own - z) / z, "0.47"), "Z {z}, p/(ρRT) {own}");
    assert!(cites(ok("div0045_eta_propylene_q1_t104.87") * 1e6, "3.0273"));
    assert!(cites(ok("div0045_lambda_r14_q1_t215.07"), "0.0150"));
    vec!["DIV-0037", "DIV-0038", "DIV-0039", "DIV-0040", "DIV-0041", "DIV-0042", "DIV-0043", "DIV-0044", "DIV-0045"]
}

/// DIV-0046 (user decision CB1, PLAN.md M11.1): v8.0.0's cubics use R_U_CODATA, and a PR fluid's c_p⁰ is its HEOS
/// source's times R_CODATA/R_source: n-Propane (R = 8.314472) −1.128e-6, Water (IAPWS-95) +1.10e-5, CycloPropane
/// (R = 8.3143) +1.96e-5.
fn div_0046(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    let r = ok("div0046_gas_constant_pr_water");
    assert_eq!(r, 8.314_462_618_153_24);
    for (fluid, t, ratio) in
        [("n-propane", 300, "-1.128e-6"), ("water", 400, "1.10e-5"), ("cyclopropane", 300, "1.96e-5")]
    {
        let (heos, pr) =
            (ok(&format!("div0046_cp0molar_heos_{fluid}_t{t}")), ok(&format!("div0046_cp0molar_pr_{fluid}_t{t}")));
        let source = ok(&format!("div0046_gas_constant_{fluid}_t{t}"));
        assert!(cites(pr / heos - 1.0, ratio) && cites(r / source - 1.0, ratio), "{fluid}: {pr} / {heos}, R {source}");
    }
    vec!["DIV-0046"]
}

/// DIV-0047 (PLAN.md M11.2): at n-Propane's exact SRK critical point v8.0.0's Z is 1/3 (1 + 2.08e-9) and its
/// (∂p/∂ρ)_T/RT 1.46e-9, where both conditions hold exactly with the stated Ω; PR's misses by about 1e-13.
fn div_0047(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    let rt = 8.314_462_618_153_24 * 369.89;
    assert!(cites(ok("div0047_z_srk_n-propane") * 3.0 - 1.0, "2.08e-9"));
    assert!(cites(ok("div0047_dpdrho_srk_n-propane") / rt, "1.46e-9"));
    let pr = ok("div0047_z_pr_n-propane") / 0.307_401_308_698_703_86 - 1.0;
    assert!(cites(pr, "-1.2e-13") && (ok("div0047_dpdrho_pr_n-propane") / rt).abs() < 1e-12, "{pr:e}");
    vec!["DIV-0047"]
}

/// DIV-0048 and DIV-0049 (PLAN.md M11.4): v8.0.0's cubic library's T_c, p_c and ω for Neon and D5, and its α⁰ for R40
/// (h 2.38e4 J/mol below HEOS's) and R134a (c_p⁰ −5.19e-5) at 500 K and 1e4 Pa.
fn div_0048_to_0049(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    assert!(cites(ok("div0048_tcrit_pr_neon"), "44.4918") && cites(ok("div0048_pcrit_pr_neon") / 1e6, "2.68"));
    assert!(cites(ok("div0048_acentric_pr_neon"), "-0.0384"));
    assert!(cites(ok("div0048_tcrit_pr_d5"), "619.15") && cites(ok("div0048_pcrit_pr_d5") / 1e6, "1.16"));
    assert!(cites(ok("div0048_acentric_pr_d5"), "0.658"));
    assert!(cites(ok("div0049_hmolar_pr_r40_t500") - ok("div0049_hmolar_heos_r40_t500"), "-2.38e4"));
    assert!(cites(ok("div0049_cp0molar_pr_r134a_t500") / ok("div0049_cp0molar_heos_r134a_t500") - 1.0, "-5.19e-5"));
    vec!["DIV-0048", "DIV-0049"]
}

/// DIV-0050 cites no oracle fact: its evidence is two printed tables (Bell 2022 Table XI, Bell 2023 Table XIII), which
/// `tests/mixture.rs` and its proof check.
fn div_0050(_: &Fixture<'_>) -> Vec<&'static str> {
    vec!["DIV-0050"]
}

/// DIV-0051: the oracle's p at Tkaczuk et al. 2020 Table 8's three states, against the table: +3.4e-7 and −2.68e-6.
/// DIV-0052 and DIV-0054 cite no oracle fact (the `mix_eos` kind and a printed table; `tests/mixture.rs` and their
/// proofs check them). DIV-0053: the oracle's R32/R134a c_v at Lemmon and Jacobsen 2004 Table 6's 300 K, 15 mol/dm³
/// row, 71.03576 printed, is 3.1e-5 below it.
fn div_0051_to_0054(facts: &Fixture<'_>) -> Vec<&'static str> {
    let ok = |name: &str| ok_fact(facts, name);
    assert!(cites(ok("div0051_p_he_ne") / 18_430_775.292_600_896 - 1.0, "3.4e-7"));
    assert!(cites(ok("div0051_p_he_ar") / 17_128_034.388_362_616 - 1.0, "-2.68e-6"));
    assert!(cites(ok("div0051_p_ne_ar") / 15_905_875.375_781_253 - 1.0, "-2.68e-6"));
    assert!(cites(ok("div0053_cvmolar_r32_r134a_t300_rho15000") / 71.035_76 - 1.0, "-3.1e-5"));
    vec!["DIV-0051", "DIV-0052", "DIV-0053", "DIV-0054"]
}

/// PLAN.md M1.13: every seed entry's cited CoolProp 8.0.0 behaviour reproduces from `facts/register.csv`, which the
/// oracle generates (map 12 §6.3, map 10 §8.4, map 13 §3); a fact that stops reproducing is fixed in the register.
#[test]
fn register_cites_reproducible_oracle_facts() {
    let (path, text) = fixture!("coolprop-8.0.0/facts/register.csv");
    let facts = Fixture::parse(path, text).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((facts.kind(), facts.provenance()), ("facts", Provenance::Oracle { version: "8.0.0" }));
    let checked: Vec<&str> = [
        div_0001_to_0005,
        div_0006_to_0012,
        div_0013_to_0017,
        div_0018_to_0020,
        div_0021_to_0022,
        div_0023_to_0025,
        div_0026,
        div_0027,
        div_0028_to_0029,
        div_0030_to_0034,
        div_0035,
        div_0036,
        div_0037_to_0045,
        div_0046,
        div_0047,
        div_0048_to_0049,
        div_0050,
        div_0051_to_0054,
    ]
    .iter()
    .flat_map(|group| group(&facts))
    .collect();
    let ids: Vec<&str> = DIVERGENCES.iter().map(|d| d.id).collect();
    assert_eq!(checked, ids, "every register entry has its facts checked");
    for row in facts.rows() {
        let Some(Cell::Text(name)) = row.cells.first() else { panic!("line {}: no name", row.line) };
        let id = name.get(3..7).map(|n| format!("DIV-{n}"));
        assert!(name.starts_with("div") && id.is_some_and(|id| ids.contains(&id.as_str())), "fact `{name}`");
    }
}

/// PLAN.md M2.7: the embedded data ships exactly the three `UsePaper` data corrections, and the register accepts them.
#[test]
fn check_register_accepts_the_shipped_patches() {
    let shipped = seed_corrections();
    let ids: Vec<&str> = shipped.iter().map(|p| &*p.divergence).collect();
    assert_eq!(ids, ["DIV-0003", "DIV-0001", "DIV-0002"]); // Nitrogen, R1234ze(E), Water: index order
    assert_eq!(check_register(DIVERGENCES, &shipped), Ok(()));
    assert_eq!(shipped[2].edit, Edit::MeltingP0 { segment: 3, p0: 632.4e6 });
}

/// VERIFICATION.md §7.2 on real data: for all 136 fluids, Corrected differs from Parity by exactly the shipped edits
/// (undoing them gives Parity back), `applied` lists them, and the model key changes if and only if a patch applies.
#[test]
fn parity_and_corrected_differ_by_exactly_the_patches() {
    let mut changed = Vec::new();
    for record in records() {
        let mut parity = record.clone();
        parity.apply(DataSet::Parity).unwrap();
        let mut corrected = record.clone();
        corrected.apply(DataSet::Corrected).unwrap();
        assert_eq!(parity, record, "{}: Parity is the v8.0.0 data", record.name);
        let ids: Vec<Box<str>> = record.corrections.iter().map(|p| p.divergence.clone()).collect();
        assert_eq!(corrected.applied, ids, "{}", record.name);
        assert_eq!(parity.model_key() == corrected.model_key(), ids.is_empty(), "{}", record.name);
        let mut undone = corrected.clone();
        for patch in &record.corrections {
            match patch.edit {
                Edit::GasConstant(_) => undone.eos.gas_constant = parity.eos.gas_constant,
                Edit::ReducingDensity(_) => undone.eos.rho_reducing = parity.eos.rho_reducing,
                Edit::MolarMass(_) => undone.molar_mass = parity.molar_mass,
                Edit::MeltingP0 { segment, .. } => {
                    let s = usize::from(segment);
                    undone.melting[s].p0 = parity.melting[s].p0;
                }
                _ => panic!("an edit kind this test does not know"),
            }
        }
        undone.applied.clear();
        assert_eq!(undone, parity, "{}", record.name);
        if !ids.is_empty() {
            changed.push(record.name.clone());
        }
    }
    assert_eq!(changed, ["Nitrogen", "R1234ze(E)", "Water"]);
}

/// DIV-0005 (`KeepOracle`, user decision 5): Helium ships no patch, Corrected is Parity, and no patch may cite the entry.
#[test]
fn helium_ships_no_patch() {
    let helium = records().into_iter().find(|r| r.name == "Helium").unwrap();
    assert!(helium.corrections.is_empty());
    let (mut parity, mut corrected) = (helium.clone(), helium);
    parity.apply(DataSet::Parity).unwrap();
    corrected.apply(DataSet::Corrected).unwrap();
    assert_eq!(parity, corrected);
    let citing = [patch("DIV-0005", Edit::GasConstant(8.314_472))];
    assert_eq!(check_register(DIVERGENCES, &citing), Err(RegisterError::NotUsePaper("DIV-0005".into())));
}

/// The superancillary hash gate (VERIFICATION.md §7.3; E14; PLAN.md M2.8) of every record under `set`: (name, result)
/// for the fluids that ship a superancillary.
fn freshness(set: DataSet) -> Vec<(String, SaFreshness)> {
    let mut out = Vec::new();
    for mut record in records() {
        record.apply(set).unwrap();
        if let Some(f) = record.superancillary_freshness() {
            out.push((record.name, f));
        }
    }
    out
}

/// Under Parity every one of the 130 shipped superancillaries is fresh: it was fitted to exactly this EOS (M2.2), and
/// the six pseudo-pure fluids ship none.
#[test]
fn all_130_superancillaries_are_fresh_under_parity() {
    let parity = freshness(DataSet::Parity);
    assert_eq!(parity.len(), 130);
    assert!(parity.iter().all(|(_, f)| *f == SaFreshness::Fresh));
    let without: Vec<String> =
        records().into_iter().filter(|r| r.superancillary_fit.is_none()).map(|r| r.name).collect();
    assert_eq!(without, ["Air", "R404A", "R407C", "R410A", "R507A", "SES36"]);
}

/// Under Corrected, the R correction (DIV-0001) and the ρ_r correction (DIV-0003) leave the shape alone, so their
/// curves rescale exactly: ρ′, ρ″ by ρ_r′/ρ_r and p_sat by (R′/R)(ρ_r′/ρ_r). Water's melting correction (DIV-0002)
/// leaves its curve fresh. 128 fresh, 2 rescaled.
#[test]
fn rho_r_and_r_corrections_rescale_exactly() {
    let corrected = freshness(DataSet::Corrected);
    let rescaled: Vec<&(String, SaFreshness)> = corrected.iter().filter(|(_, f)| *f != SaFreshness::Fresh).collect();
    let rho = 11_183.9 / 11_183.901_464_580_624;
    assert_eq!(
        rescaled,
        [
            &("Nitrogen".to_string(), SaFreshness::Rescaled { p: rho, rho }),
            &("R1234ze(E)".to_string(), SaFreshness::Rescaled { p: 8.314_462_1 / 8.314_472, rho: 1.0 }),
        ]
    );
    assert_eq!(corrected.len(), 130);
}

/// Any EOS edit other than R and ρ_r (a residual or ideal-gas coefficient, T_r) marks the curve stale, so it becomes
/// a `Guess` polished by VLE (M6.6); ρ_max, a bracketing bound, is not part of the fit.
#[test]
fn any_other_eos_edit_marks_the_curve_stale() {
    let nitrogen = records().into_iter().find(|r| r.name == "Nitrogen").unwrap();
    type Edit = fn(&mut FluidRecord);
    let stale: [(&str, Edit); 4] = [
        ("power n", |r| r.eos.power[0].n *= 1.0 + 1e-15),
        ("gaussian beta", |r| r.eos.gaussian[0].beta += 1e-12),
        ("ideal term", |r| r.eos.ideal[0] = IdealTerm::Lead { a1: 0.0, a2: 0.0 }),
        ("T_r", |r| r.eos.t_reducing += 1e-12),
    ];
    for (what, edit) in stale {
        let mut r = nitrogen.clone();
        edit(&mut r);
        assert_eq!(r.superancillary_freshness(), Some(SaFreshness::Stale), "{what}");
    }
    let mut bound = nitrogen.clone();
    bound.eos.rho_max *= 2.0;
    assert_eq!(bound.superancillary_freshness(), Some(SaFreshness::Fresh));
    let mut r = nitrogen.clone();
    r.eos.gas_constant = 8.314_462_618;
    assert!(matches!(r.superancillary_freshness(), Some(SaFreshness::Rescaled { rho: 1.0, .. })));
}
