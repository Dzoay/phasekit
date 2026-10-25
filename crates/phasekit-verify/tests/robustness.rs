//! VERIFICATION.md §8.2 (PLAN.md M7.9; map 10 §8.1, map 12 §8): the robustness corpus, class `Smoke`. States from
//! CoolProp 8.0.0's issue-linked tests (`src/Tests/CoolProp-Tests.cpp` unless another file is named), as literals with
//! their issue numbers; a few follow the test's recipe from the fluid's critical or triple point. Each flash returns a
//! state that reproduces its inputs, or a typed error, and never panics. Each case also names which of those it gives,
//! so a regression either way fails. The issues' melting-line refusals (#2639: Water 5 K below its melting line up to
//! pmax; #1936: CO₂ below its melting line at the triple point's temperature) are `BelowMeltingLine` since M8.11 where
//! they lie above the triple point's temperature; #1098's state lies below Water's triple point and is refused. The D+X and HS cases of
//! `CoolProp-Tests-HSU_D.cpp` and the P+X cases of `-PXFlash.cpp` and `-PXcdj.cpp` are in `tests/flash.rs`.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::panic::{AssertUnwindSafe, catch_unwind};

use phasekit_core::{
    Basis, DataSet, DomainError, Error, FlashOptions, Fluid, Input, Pair, Phase, Registry, RootPolicy, State, Var,
};

/// What a corpus state gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    /// A state that reproduces the inputs.
    State,
    /// Such a state, in this phase.
    In(Phase),
    /// `Ambiguous { roots }`, each root taken by `Nearest` a state that reproduces the inputs (#2773).
    Ambiguous,
    /// A typed refusal.
    Refused,
    /// `BelowMeltingLine`: a solid (M8.11).
    Solid,
}

/// One corpus state: its issue (or test file), fluid, pair, values in the pair's name order and their basis, an
/// imposed phase, and what it gives.
#[derive(Clone, Debug)]
struct Case {
    issue: &'static str,
    fluid: &'static str,
    pair: Pair,
    xy: (f64, f64),
    basis: Basis,
    phase: Option<Phase>,
    want: Want,
}

/// A molar case without an imposed phase.
fn case(issue: &'static str, fluid: &'static str, pair: Pair, x: f64, y: f64, want: Want) -> Case {
    Case { issue, fluid, pair, xy: (x, y), basis: Basis::Molar, phase: None, want }
}

impl Case {
    /// The same case with D, H, S and U on a mass basis, as CoolProp's `PropsSI` and `*mass` keys give them.
    fn mass(self) -> Case {
        Case { basis: Basis::Mass, ..self }
    }

    /// The same case under an imposed phase.
    fn under(self, phase: Phase) -> Case {
        Case { phase: Some(phase), ..self }
    }
}

/// The corpus. Cases that follow a recipe read the fluid's critical point or its curve's lowest point.
#[expect(clippy::too_many_lines, reason = "stays: a data table, the corpus's 206 cases")]
fn corpus(registry: &Registry) -> Vec<Case> {
    use Pair::{DP, DQ, DS, DT, HP, HQ, HS, PQ, PS, PT, PU, QS, QT, ST};
    use Phase::{Gas, Liquid, Supercritical, SupercriticalGas, SupercriticalLiquid, TwoPhase};
    use Want::{Ambiguous, In, Refused, State};
    // CoolProp's critical point with its superancillaries on (the default): the curve's top, which labels states here
    // too; the published one where a fluid has no curve.
    let critical = |name: &str| match phasekit_core::internal::record(registry, name).unwrap().superancillary_curve() {
        Some(curve) => (curve.t_range().1, curve.at_t(curve.t_range().1).unwrap().bubble.p),
        None => registry.get(name).unwrap().info().published_critical().map(|c| (c.t, c.p)).unwrap(),
    };
    let mut corpus = vec![
        // #2545 (:1687): R134a's saturated vapour at 273 K, p 291 215 Pa.
        case("#2545", "R134a", QT, 1.0, 273.0, State),
        // #2470 (:3160): Nitrogen's saturated vapour at 3.369 MPa; PT there under an imposed gas; HP just above Tc.
        case("#2470", "Nitrogen", PQ, 3_368_965.046, 1.0, State),
        case("#2470", "Nitrogen", PT, 3_368_965.046, 126.027_397_187_217_72, In(Gas)).under(Gas),
        case("#2470", "Nitrogen", HP, 67_040.578_57, 3_368_965.046, In(SupercriticalGas)).mass(),
        // #2467 (:3175): n-Pentane's PS from one saturated vapour's pressure at another's entropy.
        case("#2467", "n-Pentane", QT, 1.0, 353.15, State),
        case("#2467", "n-Pentane", QT, 1.0, 433.15, State),
        case("#2467", "n-Pentane", PT, 368_181.691_476_945_5, 393.15, In(Gas)),
        case("#2467", "n-Pentane", PS, 1_888_965.722_192_735, 1_454.268_709_307_456, In(Gas)).mass(),
        // #1870 (:3187).
        case("#1870", "n-Pentane", PS, 1e6, 1500.0, In(Gas)).mass(),
        // #2491 (:3206): Xenon's liquid at 289.3 K, just below its critical point.
        case("#2491", "Xenon", HP, 59_867.351_071_950_76, 5_835_843.730_589_151, In(Liquid)).mass(),
        // #2608 (:3212): CO₂ above pc at 218.05 K, 1 mK above its melting line; PS there, and under an imposed phase.
        case("#2608", "CarbonDioxide", PT, 73.8e5, 218.050, In(SupercriticalLiquid)),
        case("#2608", "CarbonDioxide", PS, 73.8e5, 1840.68, State).mass(),
        case("#2608", "CarbonDioxide", PS, 73.8e5, 1840.68, In(SupercriticalGas)).mass().under(SupercriticalGas),
        // #2622 (:3229): R123 1 mK below its triple point's temperature, which CoolProp accepts; HP at 5 MPa.
        case("#2622", "R123", PT, 5e6, 165.999, Refused),
        case("#2622", "R123", HP, 233_250.0, 5e6, State).mass(),
        // #2582 (:3264): CO₂'s saturated liquid at 7.333 MPa, 44 kPa below pc.
        case("#2582", "CarbonDioxide", PQ, 73.33e5, 0.0, State),
        // #2594 (:3283): CO₂ 37 Pa below pc: PQ, DP at its liquid's density, PT under an imposed liquid, PU.
        case("#2594", "CarbonDioxide", PQ, 7_377_262.928_140_703, 0.0, State),
        case("#2594", "CarbonDioxide", DP, 10_822.715_329_412_771, 7_377_262.928_140_703, State),
        case("#2594", "CarbonDioxide", PT, 7_377_262.928_140_703, 304.127_992_154_002_9, In(Liquid)).under(Liquid),
        case("#2594", "CarbonDioxide", PU, 7_377_262.928_140_703, 314_719.530_650_325_7, State).mass(),
        // #2538 (:3361): Methanol at 1 atm and 400 K, and a dilute gas.
        case("#2538", "Methanol", PQ, 101_325.0, 0.0, State),
        case("#2538", "Methanol", QT, 0.0, 400.0, State),
        case("#2538", "Methanol", DT, 1e-5, 400.0, In(Gas)),
        // #2438 (:3910, :3941): CO₂'s HP and ST inside the dome.
        case("#2438", "CarbonDioxide", HP, 300e3, 70e5, In(TwoPhase)).mass(),
        case("#2438", "CarbonDioxide", ST, 63.775_557_895_598_745, 298.0, In(TwoPhase)),
        // #2079 (:5181): Water's ST near 3.1 MPa.
        case("#2079", "Water", ST, 6_763.617_210_539_725, 631.0, State).mass(),
        case("#2079", "Water", ST, 6_763.617_210_539_725, 632.0, State).mass(),
        case("#2079", "Water", ST, 6_763.617_210_539_725, 633.0, State).mass(),
        case("#2079", "Water", ST, 6_763.617_210_539_725, 634.0, State).mass(),
        // #1611 (:5215): R407C's PQ above its critical pressure, and at 1 MPa.
        case("#1611", "R407C", PQ, 4_863_285.0, 0.0, Refused),
        case("#1611", "R407C", PQ, 1e6, 0.0, State),
        // #2417 (:5344).
        case("#2417", "Ammonia", PQ, 101_325.0, 0.0, State),
        // #2195 (:5677): Water's PQ at 10 MPa, and qualities outside [0, 1].
        case("#2195", "Water", PQ, 10e6, 0.5, In(TwoPhase)),
        case("#2195", "Water", PQ, 10e6, 1.1, Refused),
        case("#2195", "Water", PQ, 10e6, -0.1, Refused),
        case("#2195", "Water", QT, 1.5, 400.0, Refused),
        case("#2195", "Water", QT, -0.5, 400.0, Refused),
        // #1098 (:5703): Water at 1 atm and 268.15 K, below its triple point's temperature.
        case("#1098", "Water", PT, 101_325.0, 268.15, Refused),
        // #2244 (:5856): Ammonia's saturated liquid at 60.11 kPa, ± 1 mPa.
        case("#2244", "Ammonia", PQ, 60_110.772_331_077_3 - 0.001, 0.0, State),
        case("#2244", "Ammonia", PQ, 60_110.772_331_077_3, 0.0, State),
        case("#2244", "Ammonia", PQ, 60_110.772_331_077_3 + 0.001, 0.0, State),
        // #2461 (:5872): Ammonia's saturated vapour at 130 kPa, and the gas 2 K above it.
        case("#2461", "Ammonia", PQ, 1.3e5, 1.0, State),
        case("#2461", "Ammonia", PT, 1.3e5, 244.916_176_193_119_2 + 2.0, In(Gas)),
        // #2773 (:5896-6248): Water's Q-pairs with several roots, and with one.
        case("#2773", "Water", QT, 0.0, 277.0, State),
        case("#2773", "Water", QT, 0.0, 273.5, State),
        case("#2773", "Water", QT, 0.0, 290.0, State),
        case("#2773", "Water", DQ, 55_501.242_996_103_39, 0.0, Ambiguous),
        case("#2773", "Water", HQ, 50_263.342_594_087_08, 1.0, Ambiguous),
        case("#2773", "Water", DQ, 52_038.417_601_825_53, 0.0, In(TwoPhase)),
        case("#2773", "Water", QS, 1.0, 112.327_730_276_805_17, In(TwoPhase)),
        case("#2773", "Water", HQ, 2_790_041.346_822_432_7, 1.0, Ambiguous).mass(),
        case("#2773", "Water", QS, 1.0, 6_235.140_674_943_341, In(TwoPhase)).mass(),
        case("#2773", "n-Propane", QS, 0.0, 54.732_574_724_485_87, In(TwoPhase)),
        // #1845 (:6342): CO₂'s saturated vapour at 15 kg/m³.
        case("#1845", "CarbonDioxide", DQ, 15.0, 1.0, In(TwoPhase)).mass(),
        // #1950 (:6365).
        case("#1950", "Nitrogen", PT, 2e5, 78.0, In(Liquid)),
        // r1w7 (`CoolProp-Tests-AirMelting.cpp:30`): Air at 1.035 GPa and 210 K, and its HP, PS and PU.
        case("r1w7", "Air", PT, 1_035_411_979.236_946_7, 210.015_384_615_384_62, State),
        case("r1w7", "Air", HP, 28_603.225_178_609_056, 1_035_411_979.236_946_7, State),
        case("r1w7", "Air", PS, 1_035_411_979.236_946_7, 8.870_411_338_058_638, State),
        case("r1w7", "Air", PU, 1_035_411_979.236_946_7, 4_265.763_410_875_754, State),
        // `CoolProp-Tests-NeonMelting.cpp:32`: Neon at 43.85 kPa, by its triple point and far above it.
        case("NeonMelting", "Neon", PT, 43_851.4, 24.56, State),
        case("NeonMelting", "Neon", PT, 43_851.4, 100.0, In(SupercriticalGas)),
    ];
    // #2738 (:1530): PT at the published critical point.
    for name in ["CarbonDioxide", "Water", "R134a"] {
        let (t_c, p_c) = critical(name);
        corpus.push(case("#2738", name, PT, p_c, t_c, In(Phase::CriticalPoint)));
    }
    // #2582 (:3264): HP of CO₂ on both sides of pc, h over 100 to 700 kJ/kg (every 100th of the test's 1000).
    for k in (0..1000).step_by(100).chain([999]) {
        let h = 100e3 + 600e3 * f64::from(k) / 999.0;
        corpus.push(case("#2582", "CarbonDioxide", HP, h, 73.76e5, State).mass());
        corpus.push(case("#2582", "CarbonDioxide", HP, h, 73.33e5, State).mass());
    }
    // #2598 (:3329): 50 kPa and 3 K around the published critical point of each core fluid, three phases.
    for name in CORE {
        let (t_c, p_c) = critical(name);
        corpus.push(case("#2598", name, PT, p_c + 5e4, t_c + 3.0, In(Supercritical)));
        corpus.push(case("#2598", name, PT, p_c + 5e4, t_c - 3.0, In(SupercriticalLiquid)));
        corpus.push(case("#2598", name, PT, p_c - 5e4, t_c + 3.0, In(SupercriticalGas)));
    }
    // #2438 (:3928): n-Propane's ST at the dome's middle entropy at 298 K, a few ulps either side.
    for i in -2..=2 {
        let s = 78.801_985_914_798_05 + f64::from(i) * 1e-14;
        corpus.push(case("#2438", "n-Propane", ST, s, 298.0, In(TwoPhase)));
    }
    // #2589 (:4034): Air's ideal-gas density at 1 atm under an imposed gas.
    for t in [200.0, 300.0, 400.0, 500.0] {
        corpus.push(case("#2589", "Air", DT, 101_325.0 / (8.314_51 * t), t, In(Gas)).under(Gas));
    }
    // #2762-#2765 (:4787): the papers' check states of twelve fluids, (T, ρ) in K and mol/m³.
    for (name, t, rho) in [
        ("R1224YDZ", 400.0, 8000.0),
        ("R1132(E)", 330.0, 12000.0),
        ("Tetrahydrofuran", 450.0, 10000.0),
        ("PropyleneGlycol", 400.0, 13000.0),
        ("VinylChloride", 300.0, 15000.0),
        ("R1123", 320.0, 11000.0),
        ("n-Perfluorobutane", 360.0, 5200.0),
        ("n-Perfluoropentane", 390.0, 4200.0),
        ("n-Perfluorohexane", 410.0, 3700.0),
        ("R1233zd(E)", 400.0, 8000.0),
        ("R1130(E)", 320.0, 12500.0),
        ("R1243zf", 280.0, 11000.0),
    ] {
        corpus.push(case("#2762", name, DT, rho, t, State));
    }
    // #1907 (:5721): Water's saturated vapour by density, kg/m³, and DS at its entropy.
    for d in [200.0, 175.0, 130.0, 70.0, 30.0, 15.126, 11.141, 5.0] {
        corpus.push(case("#1907", "Water", DQ, d, 1.0, In(TwoPhase)).mass());
        let dew = registry.get("Water").unwrap().state(Input::new(DQ, d, 1.0, Basis::Mass).unwrap());
        let s = dew.map_or(f64::NAN, |state| state.s(Basis::Mass));
        corpus.push(case("#1907", "Water", DS, d, s, State).mass());
    }
    // #1730 (:5199): Water's HS near 2.97 MPa.
    for h in [3_132_000.0, 3_133_000.0, 3_133_500.0, 3_133_800.0, 3_134_000.0, 3_135_000.0] {
        corpus.push(case("#1730", "Water", HS, h, 6777.0, State).mass());
    }
    // #3189 (`CoolProp-Tests-SVDSBTL.cpp`, the HEOS half): PT at the triple point's pressure and 1 % above, 400 K.
    for (name, phase) in
        [("Water", Gas), ("Nitrogen", SupercriticalGas), ("Methane", SupercriticalGas), ("Hydrogen", SupercriticalGas)]
    {
        let curve = phasekit_core::internal::record(registry, name).unwrap().superancillary_curve().unwrap();
        let p_triple = curve.at_t(curve.t_range().0).unwrap().bubble.p;
        corpus.push(case("#3189", name, PT, p_triple, 400.0, In(phase)));
        corpus.push(case("#3189", name, PT, 1.01 * p_triple, 400.0, In(phase)));
    }
    // #2247 (`CoolProp-Tests-SVDSBTL.cpp`, the HEOS half): R32's saturated vapour, and the gas 1 K above it.
    for i in (0..=60).step_by(10) {
        let t = 243.15 + f64::from(i);
        corpus.push(case("#2247", "R32", QT, 1.0, t, State));
        let p = registry.get("R32").unwrap().state(Input::new(QT, 1.0, t, Basis::Molar).unwrap());
        corpus.push(case("#2247", "R32", PT, p.map_or(f64::NAN, |s| s.p()), t + 1.0, In(Gas)));
    }
    corpus.extend(melting_cases(registry));
    corpus
}

/// The issues' melting-line states (M8.11).
fn melting_cases(registry: &Registry) -> Vec<Case> {
    use Pair::PT;
    use Want::{Refused, Solid};
    let mut corpus = Vec::new();
    // #2639 (:4074): Water 5 K below its melting line from just above its triple point's pressure to pmax: below
    // 273.16 K, under the triple point's temperature; on the ice VI branch, from 777 MPa, below its melting line.
    let water = registry.get("Water").unwrap();
    let (p_triple, p_max) =
        (611.655 * 1.0001, phasekit_core::internal::record(registry, "Water").unwrap().limits.p_max());
    for i in 0..10 {
        let p = p_triple + (p_max - p_triple) * f64::from(i) / 9.0;
        let t = water.model().melting_temperature(p).unwrap() - 5.0;
        corpus.push(case("#2639", "Water", PT, p, t, if t < 273.16 { Refused } else { Solid }));
    }
    // #1936 (:4089): CO₂ below its melting line at its triple point's temperature, between its triple and critical
    // pressures and at 5 pc, and at 100 MPa and 230 K.
    let (t_triple, p_triple, p_c) = (216.592, 517_950.0, 7_377_300.0);
    for (p, t) in [(0.5 * (p_triple + p_c), t_triple), (5.0 * p_c, t_triple), (1e8, 230.0)] {
        corpus.push(case("#1936", "CarbonDioxide", PT, p, t, Solid));
    }
    corpus
}

/// The core subset's fluids (`phasekit_verify::eos::CORE_FILES`).
const CORE: [&str; 14] = [
    "Air",
    "Ammonia",
    "CarbonDioxide",
    "HFE143m",
    "Helium",
    "Methanol",
    "Nitrogen",
    "R1130(E)",
    "R1234yf",
    "R1234ze(E)",
    "R125",
    "R410A",
    "Water",
    "n-Heptane",
];

/// `var` of `state` in `basis`.
fn value_of(state: &State, var: Var, basis: Basis) -> Option<f64> {
    Some(match var {
        Var::T => state.t(),
        Var::P => state.p(),
        Var::Q => state.quality()?,
        Var::D => state.rho(basis),
        Var::H => state.h(basis),
        Var::S => state.s(basis),
        Var::U => state.u(basis),
        _ => return None,
    })
}

/// Whether `state` reproduces the case's inputs as the flash's acceptance gate asks (D6): T and ρ within 1e-9 of each,
/// p within 1e-9 of it and 1e-12 of ρRT, Q within 1e-9, and h, s and u within 1e-9 of max(|x|, RT), of s's R.
fn reproduces(state: &State, case: &Case) -> bool {
    let (vx, vy) = case.pair.vars();
    let molar_mass = state.rho(Basis::Mass) / state.rho(Basis::Molar);
    let r = state.gas_constant() / if case.basis == Basis::Mass { molar_mass } else { 1.0 };
    let holds = |var: Var, want: f64| {
        let Some(got) = value_of(state, var, case.basis) else { return false };
        let bound = match var {
            Var::T | Var::D => 1e-9 * want.abs(),
            Var::P => 1e-9 * want + 1e-12 * state.rho(Basis::Molar) * state.gas_constant() * state.t(),
            Var::Q => 1e-9,
            Var::S => 1e-9 * want.abs().max(r),
            Var::H | Var::U => 1e-9 * want.abs().max(r * state.t()),
            _ => return false,
        };
        (got - want).abs() <= bound
    };
    holds(vx, case.xy.0) && holds(vy, case.xy.1)
}

/// What a case gave, or `None` if it gave what it wants.
fn check(fluid: &Fluid, case: &Case) -> Option<String> {
    let input = match Input::new(case.pair, case.xy.0, case.xy.1, case.basis) {
        Ok(input) => input,
        Err(_) if case.want == Want::Refused => return None,
        Err(e) => return Some(format!("{e:?}")),
    };
    let opts = case.phase.map_or(FlashOptions::new(), |phase| FlashOptions::new().with_phase(phase));
    let flashed = catch_unwind(AssertUnwindSafe(|| fluid.flash(input, &opts)));
    let Ok(result) = flashed else { return Some("panicked".to_string()) };
    let holds = match (&result, case.want) {
        (Ok(state), Want::State) => reproduces(state, case),
        (Ok(state), Want::In(phase)) => reproduces(state, case) && state.phase() == phase,
        (Err(Error::Ambiguous { roots }), Want::Ambiguous) => roots.as_slice().iter().all(|&root| {
            let nearest = opts.with_roots(RootPolicy::Nearest(root));
            fluid.flash(input, &nearest).is_ok_and(|state| reproduces(&state, case))
        }),
        (Err(Error::Ambiguous { .. }), _) => false,
        (Err(_), Want::Refused) => true,
        (Err(Error::Domain(DomainError::BelowMeltingLine { t, t_melt, .. })), Want::Solid) => t < t_melt,
        _ => false,
    };
    if holds { None } else { Some(format!("{result:?}")) }
}

/// PLAN.md M7.9 (map 10 §8.1): every corpus state gives what it names, a state that reproduces its inputs or a typed
/// error, and none panics (each flash runs under `catch_unwind`, so one panic names its state and the rest still run).
#[test]
fn robustness_corpus_never_panics() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let corpus = corpus(&registry);
    let bad: Vec<String> = corpus
        .iter()
        .filter_map(|case| {
            let fluid = registry.get(case.fluid).unwrap();
            check(fluid, case).map(|got| format!("{} {} {case:?}: {got}", case.issue, case.fluid))
        })
        .collect();
    assert!(bad.is_empty(), "{} of {} states:\n{}", bad.len(), corpus.len(), bad.join("\n"));
    assert!(corpus.len() >= 100, "{} states", corpus.len());
}
