//! IAPWS water and ice through `phasekit-iapws` (PLAN.md M14; user decisions HA1, HA4): each release's check table,
//! within its printed digits (`Paper`), the values converted to the table's units.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::num::NonZeroUsize;

use phasekit_core::batch::{BatchRequest, ExecPolicy};
use phasekit_core::math;
use phasekit_core::{
    Basis, DataSet, Enthalpy, Entropy, Error, Fluid, Gauge, Input, Pair, Phase, Prop, ReferenceState, Registry, State,
    Var,
};
use phasekit_iapws::if97;
use phasekit_verify::conformance::{Outcome, capability_matrix, gauge_invariance, identities, policy_equivalence};
use phasekit_verify::register::{RowKey, exempt_row};
use phasekit_verify::{Cell, DIVERGENCES, Fixture, Kind, ToleranceClass, Window, fixture};

/// The table at `path`, parsed.
fn table((path, text): (&'static str, &'static str)) -> Fixture<'static> {
    Fixture::parse(path, text).unwrap()
}

/// Each `(column, value)` of a row checked against `table`'s row `row`: the misses, named.
fn misses(table: &Fixture<'_>, row: usize, values: &[(&str, f64)]) -> Vec<String> {
    values.iter().filter_map(|&(column, got)| table.check(row, column, got).err().map(|e| format!("{e:?}"))).collect()
}

/// A state's v, h, u, s, c_p and w in Table 5's and Table 15's units: m³/kg, kJ/kg, kJ/(kg K), m/s.
fn printed_properties(state: &State) -> [(&'static str, f64); 6] {
    [
        ("v", 1.0 / state.rho(Basis::Mass)),
        ("h", state.h(Basis::Mass) / 1e3),
        ("u", state.u(Basis::Mass) / 1e3),
        ("s", state.s(Basis::Mass) / 1e3),
        ("cp", state.cp(Basis::Mass).unwrap_or(f64::NAN) / 1e3),
        ("w", state.speed_of_sound().unwrap_or(f64::NAN)),
    ]
}

/// Arbiter: IAPWS R7-97(2012) Table 5, region 1's v, h, u, s, c_p and w at (300 K, 3 MPa), (300 K, 80 MPa) and
/// (500 K, 3 MPa), through core's `bundle_from_gibbs` and `State::from_total` (PLAN.md M14.2).
#[test]
fn region_1_is_table_5() {
    let table = table(fixture!("paper/Water/IAPWS-R7-97-2012.5.csv"));
    let mut missed = Vec::new();
    for row in 0..table.rows().len() {
        let (t, p) = (table.value(row, "T").unwrap(), table.value(row, "p").unwrap() * 1e6);
        match if97::region1(t, p) {
            Ok(state) => missed.extend(misses(&table, row, &printed_properties(&state))),
            Err(e) => missed.push(format!("row {row}: {e}")),
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Arbiter: IAPWS R7-97(2012) Tables 35 and 36, the saturation pressure at 300, 500 and 600 K (Eq. (30)) and the
/// saturation temperature at 0.1, 1 and 10 MPa (Eq. (31)) (PLAN.md M14.2).
#[test]
fn the_saturation_line_is_tables_35_and_36() {
    let (pressures, temperatures) = (
        table(fixture!("paper/Water/IAPWS-R7-97-2012.35.csv")),
        table(fixture!("paper/Water/IAPWS-R7-97-2012.36.csv")),
    );
    let mut missed = Vec::new();
    for row in 0..pressures.rows().len() {
        let t = pressures.value(row, "T").unwrap();
        let p = if97::saturation_pressure(t).map_or(f64::NAN, |p| p / 1e6);
        missed.extend(misses(&pressures, row, &[("p", p)]));
    }
    for row in 0..temperatures.rows().len() {
        let p = temperatures.value(row, "p").unwrap() * 1e6;
        let t = if97::saturation_temperature(p).unwrap_or(f64::NAN);
        missed.extend(misses(&temperatures, row, &[("T", t)]));
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Each `if97` row's pair and inputs: PT (x = p, y = T), QT (x = Q, y = T) or PQ (x = p, y = Q).
fn if97_row<'a>(table: &Fixture<'a>, row: usize) -> (&'a str, f64, f64) {
    let pair = match table.rows()[row].cells.first() {
        Some(Cell::Text(pair)) => *pair,
        _ => "",
    };
    (pair, table.value(row, "x").unwrap(), table.value(row, "y").unwrap())
}

/// `got` against the row's `column` within `Prop`, 1e-12 of the larger of the value and `floor` (VERIFICATION.md §5:
/// R·T for an energy, R for an entropy or a heat capacity, in the row's mass units; 0 elsewhere): the miss, named.
fn prop_miss(table: &Fixture<'_>, row: usize, (column, got): (&str, f64), floor: f64) -> Option<String> {
    let want = table.value(row, column).unwrap();
    let bound = ToleranceClass::Prop.bound(want.abs().max(floor)).unwrap();
    let error = (got - want).abs();
    (error.is_nan() || error > bound).then(|| format!("row {row} {column}: {got} against {want}"))
}

/// Oracle: CoolProp 8.0.0's `IF97::Water`, `fixtures/coolprop-8.0.0/all/if97.csv` (PLAN.md M14.2). Region 1's PT rows,
/// those at or above Eq. (30)'s saturation pressure, give the oracle's ρ, h, s, u, c_p and w within `Prop`; the QT and
/// PQ rows give its saturation pressure and temperature. A row the oracle refuses is QT at 273.15 K, below its
/// saturation line's 273.16 K, where Eq. (30) is defined (§8.1: it extrapolates to 273.15 K). Region 2's rows and the
/// saturated states are M14.3's.
#[test]
fn region_1_and_the_saturation_line_match_the_oracle() {
    let table = table(fixture!("coolprop-8.0.0/all/if97.csv"));
    let (mut missed, mut counts) = (Vec::new(), [0; 4]);
    for row in 0..table.rows().len() {
        let (pair, x, y) = if97_row(&table, row);
        if table.printed(row, "status") != Some("ok") {
            assert_eq!((pair, y), ("QT", 273.15), "row {row}: the oracle refuses");
            counts[3] += 1;
            continue;
        }
        match pair {
            "PT" if x >= if97::saturation_pressure(y).unwrap() => {
                let state = if97::region1(y, x).unwrap();
                let (energy, entropy) = (if97::R * y, if97::R);
                let values = [
                    (("rhomass", state.rho(Basis::Mass)), 0.0),
                    (("hmass", state.h(Basis::Mass)), energy),
                    (("smass", state.s(Basis::Mass)), entropy),
                    (("umass", state.u(Basis::Mass)), energy),
                    (("cpmass", state.cp(Basis::Mass).unwrap()), entropy),
                    (("speed_sound", state.speed_of_sound().unwrap()), 0.0),
                ];
                missed.extend(values.into_iter().filter_map(|(value, floor)| prop_miss(&table, row, value, floor)));
                counts[0] += 1;
            }
            "QT" => {
                missed.extend(prop_miss(&table, row, ("p", if97::saturation_pressure(y).unwrap()), 0.0));
                counts[1] += 1;
            }
            "PQ" => {
                missed.extend(prop_miss(&table, row, ("T", if97::saturation_temperature(x).unwrap()), 0.0));
                counts[2] += 1;
            }
            _ => {}
        }
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!(counts, [58, 20, 12, 2], "region 1's PT rows, QT rows, PQ rows, refusals");
}

/// Arbiter: IAPWS R7-97(2012) Table 15, region 2's v, h, u, s, c_p and w at (300 K, 3.5 kPa), (700 K, 3.5 kPa) and
/// (700 K, 30 MPa) (PLAN.md M14.3): the region's function, which the `Fluid` uses only to 623.15 K.
#[test]
fn region_2_is_table_15() {
    let table = table(fixture!("paper/Water/IAPWS-R7-97-2012.15.csv"));
    let mut missed = Vec::new();
    for row in 0..table.rows().len() {
        let (t, p) = (table.value(row, "T").unwrap(), table.value(row, "p").unwrap() * 1e6);
        match if97::region2(t, p) {
            Ok(state) => missed.extend(misses(&table, row, &printed_properties(&state))),
            Err(e) => missed.push(format!("row {row}: {e}")),
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Arbiter: IAPWS R7-97(2012) §4, the T-p point the B23-equation must meet, (623.15 K, 16.5291643 MPa), both ways:
/// Eq. (5)'s pressure at its T and Eq. (6)'s temperature at its p (PLAN.md M27.1).
#[test]
fn the_b23_line_meets_its_check_point() {
    let table = table(fixture!("paper/Water/IAPWS-R7-97-2012.1.csv"));
    let (t, p) = (table.value(0, "T").unwrap(), table.value(0, "p").unwrap() * 1e6);
    let pressure = if97::b23_pressure(t).map_or(f64::NAN, |p| p / 1e6);
    let temperature = if97::b23_temperature(p).unwrap_or(f64::NAN);
    assert_eq!(misses(&table, 0, &[("p", pressure), ("T", temperature)]), Vec::<String>::new());
}

/// The B23 line is defined from 623.15 to 863.15 K, 16.5292 to 100 MPa (R7-97(2012) §4): outside, each way refuses.
#[test]
fn the_b23_line_refuses_outside_its_range() {
    for t in [623.0, 863.2] {
        assert!(matches!(if97::b23_pressure(t), Err(Error::Domain(_))), "{t} K");
    }
    for p in [16.5e6, 100.1e6] {
        assert!(matches!(if97::b23_temperature(p), Err(Error::Domain(_))), "{p} Pa");
    }
    // Both ends belong to the line, and each equation inverts the other there as far as Table 1's 14 digits let them:
    // in 40 digits, 1.6e-10 K at 623.15 K and 2.3e-13 of p at 100 MPa (`mpmath`, the session that wrote this test).
    let p_min = if97::b23_pressure(623.15).unwrap();
    assert!((if97::b23_temperature(p_min).unwrap() - 623.15).abs() < 1e-9);
    assert!((if97::b23_pressure(if97::b23_temperature(100e6).unwrap()).unwrap() / 100e6 - 1.0).abs() < 1e-12);
}

/// Arbiter: IAPWS R7-97(2012) Table 42, region 5's v, h, u, s, c_p and w at (1500 K, 0.5 MPa), (1500 K, 30 MPa) and
/// (2000 K, 30 MPa) (PLAN.md M27.1).
#[test]
fn region_5_is_table_42() {
    let table = table(fixture!("paper/Water/IAPWS-R7-97-2012.42.csv"));
    let mut missed = Vec::new();
    for row in 0..table.rows().len() {
        let (t, p) = (table.value(row, "T").unwrap(), table.value(row, "p").unwrap() * 1e6);
        match if97::region5(t, p) {
            Ok(state) => missed.extend(misses(&table, row, &printed_properties(&state))),
            Err(e) => missed.push(format!("row {row}: {e}")),
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Region 5 is defined for any p > 0 (R7-97(2012) §9), which v8.0.0 refuses below 611.213 Pa (ROT-242). At 1 and
/// 100 Pa its v, h, u, s, c_p and w are Eq. (32)'s, evaluated in 50 digits by `scripts/math/if97.py` (PLAN.md M27.1),
/// within `Prop`.
#[test]
fn region_5_goes_below_611_pa() {
    // v (m³/kg), h, u (J/kg), s, c_p (J/(kg K)), w (m/s), as the script prints them.
    let reference: [(f64, f64, [f64; 6]); 2] = [
        (
            1500.0,
            1.0,
            [
                692_288.999_972_511_4,
                5_220_674.572_773_722,
                4_528_385.572_801_21,
                15_710.995_549_471_028,
                2_614.216_653_171_444,
                916.903_628_965_799_4,
            ],
        ),
        (
            2000.0,
            100.0,
            [
                9_230.520_354_293_878,
                6_588_466.694_012_86,
                5_665_414.658_583_472_5,
                14_371.009_386_344_016,
                2_840.351_763_452_987_6,
                1_049.827_287_132_459_8,
            ],
        ),
    ];
    let mut missed = Vec::new();
    for (t, p, want) in reference {
        let state = if97::region5(t, p).unwrap();
        let got = [
            1.0 / state.rho(Basis::Mass),
            state.h(Basis::Mass),
            state.u(Basis::Mass),
            state.s(Basis::Mass),
            state.cp(Basis::Mass).unwrap(),
            state.speed_of_sound().unwrap(),
        ];
        for ((name, got), want) in ["v", "h", "u", "s", "cp", "w"].into_iter().zip(got).zip(want) {
            let bound = ToleranceClass::Prop.bound(want.abs()).unwrap();
            if (got - want).abs() > bound {
                missed.push(format!("{t} K, {p} Pa, {name}: {got} against {want}"));
            }
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Region 5's bounds (R7-97(2012) §9): 1073.15 ≤ T ≤ 2273.15 K and 0 < p ≤ 50 MPa; outside, `region5` refuses with
/// the bound it crosses.
#[test]
fn region_5_refuses_outside_its_range() {
    let refused = [(1073.0, 1e6), (2273.2, 1e6), (1500.0, 50.1e6), (1500.0, 0.0), (1500.0, f64::NAN)];
    for (t, p) in refused {
        assert!(if97::region5(t, p).is_err(), "{t} K, {p} Pa");
    }
    assert!(if97::region5(1073.15, 50e6).is_ok() && if97::region5(2273.15, 1e-3).is_ok());
}

/// Every region-5 state is above the critical temperature: supercritical above the critical pressure, 22.064 MPa,
/// and a supercritical gas at or below it.
#[test]
fn region_5_states_are_supercritical() {
    let phase = |p: f64| if97::region5(1500.0, p).unwrap().phase();
    assert_eq!(
        [phase(0.5e6), phase(22.064e6), phase(30e6)],
        [Phase::SupercriticalGas, Phase::SupercriticalGas, Phase::Supercritical]
    );
}

/// Arbiter: IAPWS R7-97(2012) Table 33, region 3's p, h, u, s, c_p and w at (650 K, 500 kg/m³), (650 K, 200 kg/m³)
/// and (750 K, 500 kg/m³), from Eq. (28)'s bundle through `State::from_total` (PLAN.md M27.2).
#[test]
fn region_3_is_table_33() {
    let table = table(fixture!("paper/Water/IAPWS-R7-97-2012.33.csv"));
    let mut missed = Vec::new();
    for row in 0..table.rows().len() {
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap());
        match if97::region3(rho, t) {
            Ok(state) => {
                let mut values = printed_properties(&state);
                values[0] = ("p", state.p() / 1e6);
                missed.extend(misses(&table, row, &values));
            }
            Err(e) => missed.push(format!("row {row}: {e}")),
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// `scripts/math/if97.py`'s region-3 states (T in K, ρ in kg/m³) and their p (Pa), h, u (J/kg), s, c_p (J/(kg K)) and
/// w (m/s), as it prints them (PLAN.md M27.2).
const REGION_3_IN_50_DIGITS: [(f64, f64, [f64; 6]); 20] = [
    (
        623.15,
        600.0,
        [
            19_891_144.217_076_052,
            1_646_568.909_422_533_1,
            1_613_417.002_394_073_2,
            3_730.124_010_736_243_7,
            8_149.264_062_299_549,
            662.245_536_137_369_6,
        ],
    ),
    (
        623.15,
        650.0,
        [
            31_907_233.571_489_602,
            1_604_149.013_249_826,
            1_555_060.961_601_380_5,
            3_631.320_002_051_432,
            6_224.265_107_563_121,
            829.527_324_532_063_1,
        ],
    ),
    (
        630.0,
        600.0,
        [
            24_694_130.045_995_723,
            1_675_354.972_923_423_9,
            1_634_198.089_513_430_8,
            3_763.290_876_389_496_6,
            7_740.797_708_660_933,
            688.714_106_393_375_7,
        ],
    ),
    (
        630.0,
        700.0,
        [
            60_592_337.176_677_13,
            1_602_304.290_039_218_7,
            1_515_743.808_358_251_4,
            3_560.726_383_613_217,
            5_200.914_259_880_592,
            1_020.727_130_291_262_1,
        ],
    ),
    (
        640.0,
        150.0,
        [
            19_750_722.501_476_873,
            2_491_057.050_515_988,
            2_359_385.567_172_809,
            5_056.960_268_655_095,
            23_681.816_716_703_885,
            409.192_278_840_570_4,
        ],
    ),
    (
        640.0,
        170.0,
        [
            20_166_514.781_295_92,
            2_419_449.902_848_415_5,
            2_300_823.345_311_380_5,
            4_940.984_180_109_268,
            41_072.505_425_003_86,
            387.693_795_965_315_5,
        ],
    ),
    (
        647.096,
        300.0,
        [
            22_063_313.626_575_503,
            2_126_150.505_276_826,
            2_052_606.126_521_574_6,
            4_471.681_729_943_823,
            4_931_815.188_569_455,
            319.548_367_862_814_8,
        ],
    ),
    (
        647.096,
        350.0,
        [
            22_065_722.395_693_168,
            2_043_137.535_277_524_3,
            1_980_092.614_146_972_3,
            4_343.385_101_008_246,
            2_140_800.665_291_94,
            313.921_056_946_756_55,
        ],
    ),
    (
        647.096,
        400.0,
        [
            22_129_968.315_376_66,
            1_973_184.533_615_765_4,
            1_917_859.612_827_323_8,
            4_235.023_786_860_205,
            133_931.730_632_313_96,
            335.165_758_892_197_3,
        ],
    ),
    (
        647.096,
        500.0,
        [
            24_173_740.162_788_01,
            1_851_287.040_157_812_9,
            1_802_939.559_832_236_7,
            4_039.897_106_073_956_4,
            14_846.365_773_405_376,
            488.589_772_706_318_3,
        ],
    ),
    (
        647.096,
        600.0,
        [
            36_935_919.715_554_27,
            1_746_613.615_585_349,
            1_685_053.749_392_758_6,
            3_842.942_908_745_865,
            6_989.868_636_026_705,
            743.926_591_879_638_9,
        ],
    ),
    (
        660.0,
        250.0,
        [
            24_903_935.465_578_057,
            2_295_409.599_895_67,
            2_195_793.858_033_357_7,
            4_714.874_983_886_921,
            47_948.471_276_016_57,
            379.397_569_247_890_4,
        ],
    ),
    (
        660.0,
        450.0,
        [
            27_786_405.176_455_15,
            1_965_742.078_818_864,
            1_903_994.511_760_074_9,
            4_203.372_120_686_825,
            18_496.791_539_732_716,
            456.169_735_808_746_9,
        ],
    ),
    (
        700.0,
        300.0,
        [
            35_865_421.467_688_02,
            2_363_803.951_608_028_3,
            2_244_252.546_715_735,
            4_757.076_227_896_314,
            15_263.476_914_446_024,
            466.280_298_685_919_1,
        ],
    ),
    (
        700.0,
        400.0,
        [
            41_063_748.089_095_555,
            2_197_168.462_703_736,
            2_094_509.092_480_997,
            4_497.871_815_659_144,
            12_223.957_701_608_342,
            519.301_219_078_149_6,
        ],
    ),
    (
        700.0,
        500.0,
        [
            51_299_552.105_968_12,
            2_064_960.747_565_701,
            1_962_361.643_353_764_7,
            4_276.877_705_201_055,
            8_024.093_982_096_447,
            653.902_433_058_547_4,
        ],
    ),
    (
        700.0,
        600.0,
        [
            76_083_465.672_208_7,
            1_962_677.146_415_185,
            1_835_871.370_294_837,
            4_067.056_429_500_978_5,
            5_744.593_351_298_259,
            876.211_309_248_404_4,
        ],
    ),
    (
        800.0,
        400.0,
        [
            79_372_866.105_196_07,
            2_572_225.356_097_83,
            2_373_793.190_834_84,
            4_871.390_134_752_187,
            6_288.556_405_823_818,
            700.435_944_795_085_3,
        ],
    ),
    (
        850.0,
        400.0,
        [
            98_709_923.128_462_21,
            2_749_220.339_278_856_3,
            2_502_445.531_457_700_8,
            5_027.406_163_197_728,
            5_394.701_781_923_866,
            766.141_995_116_98,
        ],
    ),
    (
        860.0,
        390.0,
        [
            99_711_360.300_089_75,
            2_797_742.935_843_928,
            2_542_072.781_228_313,
            5_081.193_740_663_229,
            5_296.056_520_661_786,
            766.442_371_903_215,
        ],
    ),
];

/// Region 3's p, h, u, s, c_p and w at 20 states inside it (between the B23 line and 100 MPa, and below T_c a single
/// phase), Eq. (28) evaluated in 50 digits by `scripts/math/if97.py` from `region3.rs`'s coefficients, within `Prop`
/// with §5's floors (R·T for an energy, R for an entropy or a heat capacity) and its near-critical window, where c_p's
/// denominator cancels: on the critical isotherm at 300 and 350 kg/m³ it is 1.4e-11 and 1.4e-12 off (PLAN.md M27.2).
#[test]
fn region_3_is_eq_28_in_50_digits() {
    let mut missed = Vec::new();
    for (t, rho, want) in REGION_3_IN_50_DIGITS {
        let state = if97::region3(rho, t).unwrap();
        let got = [
            state.p(),
            state.h(Basis::Mass),
            state.u(Basis::Mass),
            state.s(Basis::Mass),
            state.cp(Basis::Mass).unwrap(),
            state.speed_of_sound().unwrap(),
        ];
        let floors = [0.0, if97::R * t, if97::R * t, if97::R, if97::R, 0.0];
        let window = Window::at(t, rho, 647.096, 322.0);
        for (((name, got), want), floor) in ["p", "h", "u", "s", "cp", "w"].into_iter().zip(got).zip(want).zip(floors) {
            let bound = ToleranceClass::Prop.bound_in(want.abs().max(floor), window).unwrap();
            if (got - want).abs() > bound {
                missed.push(format!("{t} K, {rho} kg/m³, {name}: {got} against {want}"));
            }
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Region 3's temperatures (R7-97(2012) §7): 623.15 K to 863.15 K, where the B23 line meets 100 MPa; outside, or at
/// a density that is not positive, `region3` refuses. Its phase is labelled by T and p against the critical point,
/// and below both by ρ against ρ_c.
#[test]
fn region_3_refuses_outside_its_temperatures_and_labels_phases() {
    for (rho, t) in [(500.0, 623.0), (500.0, 863.2), (0.0, 650.0), (f64::NAN, 650.0)] {
        assert!(if97::region3(rho, t).is_err(), "{rho} kg/m³, {t} K");
    }
    assert!(if97::region3(600.0, 623.15).is_ok() && if97::region3(390.0, 863.15).is_ok());
    let phase = |rho: f64, t: f64| if97::region3(rho, t).unwrap().phase();
    assert_eq!(
        [phase(500.0, 750.0), phase(80.0, 680.0), phase(650.0, 640.0), phase(600.0, 623.15)],
        [Phase::Supercritical, Phase::SupercriticalGas, Phase::SupercriticalLiquid, Phase::Liquid,]
    );
    assert_eq!(phase(150.0, 640.0), Phase::Gas);
}

/// IF97 water's `Fluid` (PLAN.md M14.3; user decision HA4).
fn water() -> Fluid {
    if97::water().unwrap()
}

/// The `if97` row's input for IF97 water's `Fluid`.
fn if97_input(pair: &str, x: f64, y: f64) -> Input {
    let pair = match pair {
        "PT" => Pair::PT,
        "QT" => Pair::QT,
        _ => Pair::PQ,
    };
    Input::new(pair, x, y, Basis::Mass).unwrap()
}

/// Oracle: CoolProp 8.0.0's `IF97::Water`, `fixtures/coolprop-8.0.0/all/if97.csv` (PLAN.md M14.3). Every row through
/// IF97 water's `Fluid`: T, p, ρ, h, s, u, c_p and w within `Prop` with §5's floors, region 2's PT rows and the
/// saturated sides (Q 0 and 1: c_p and w the saturated phase's) among them, DIV-0065's saturated liquid at 623.15 K at
/// its registered 5e-12 in w. The oracle refuses QT at 273.15 K, below its saturation line; the `Fluid` answers there
/// (two oracle-gap rows).
#[test]
fn if97_water_matches_the_oracle() {
    let (table, water) = (table(fixture!("coolprop-8.0.0/all/if97.csv")), water());
    let (mut missed, mut asserted, mut gaps, mut exempt) = (Vec::new(), 0, 0, 0);
    for row in 0..table.rows().len() {
        let (pair, x, y) = if97_row(&table, row);
        let state = water.state(if97_input(pair, x, y));
        if table.printed(row, "status") != Some("ok") {
            assert!(state.is_ok(), "row {row}: {state:?}");
            gaps += 1;
            continue;
        }
        let state = state.unwrap();
        let t = state.t();
        let (energy, entropy) = (if97::R * t, if97::R);
        let values = [
            (("T", t), 0.0),
            (("p", state.p()), 0.0),
            (("rhomass", state.rho(Basis::Mass)), 0.0),
            (("hmass", state.h(Basis::Mass)), energy),
            (("smass", state.s(Basis::Mass)), entropy),
            (("umass", state.u(Basis::Mass)), energy),
            (("cpmass", state.cp(Basis::Mass).unwrap_or(f64::NAN)), entropy),
            (("speed_sound", state.speed_of_sound().unwrap_or(f64::NAN)), 0.0),
        ];
        for ((column, got), floor) in values {
            let p = table.value(row, "p");
            let key = RowKey { t, input: Some(pair), two_phase: false, p, balanced: true, near_critical: false };
            if exempt_row(DIVERGENCES, "IF97 water", Kind::If97, column, key).is_some() {
                let want = table.value(row, column).unwrap();
                assert!((got / want - 1.0).abs() <= 5e-12, "DIV-0065: row {row} {column}: {got} against {want}");
                exempt += 1;
                continue;
            }
            missed.extend(prop_miss(&table, row, (column, got), floor));
        }
        asserted += 1;
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!((asserted, gaps, exempt), (120, 2, 2), "rows asserted, oracle gaps, cells exempt (w at Q 0 and 1)");
}

/// The conformance kit (VERIFICATION.md §10) on IF97 water (PLAN.md M14.3): its two truths, a liquid at (300 K, 1 MPa)
/// and the dome at (Q 0.4, 400 K), round-trip through PT, QT and PQ, the rest of the 19 pairs `Unsupported`; a
/// single phase has no quality, so its quality pairs are refused before the model is asked, and PT on the line is
/// `Ambiguous` between the two sides, neither of which is the dome.
#[test]
fn if97_water_capability_matrix() {
    let water = water();
    let single = water.state(if97_input("PT", 1e6, 300.0)).unwrap();
    let dome = water.state(if97_input("QT", 0.4, 400.0)).unwrap();
    for (pair, [one_phase, two_phase]) in capability_matrix(&water, [&single, &dome]) {
        let declared = matches!(pair, Pair::PT | Pair::QT | Pair::PQ);
        let q_pair = matches!(pair.vars(), (Var::Q, _) | (_, Var::Q));
        let one_ok = match &one_phase {
            Outcome::Pass => declared && !q_pair,
            Outcome::Unsupported => !declared && !q_pair,
            Outcome::Refused(Error::Undefined { .. }) => q_pair,
            _ => false,
        };
        let two_ok = match &two_phase {
            Outcome::Pass => declared && q_pair,
            Outcome::Unsupported => !declared,
            Outcome::Refused(Error::Ambiguous { .. }) => pair == Pair::PT,
            _ => false,
        };
        assert!(one_ok && two_ok, "{pair:?}: {one_phase:?}, {two_phase:?}");
    }
}

/// The states the kit's other checks run on: liquid and vapour from region 1 and 2 across the `Fluid`'s range, and
/// both saturated sides and the dome at 450 K.
fn kit_inputs() -> Vec<Input> {
    let mut inputs = Vec::new();
    for t in [273.16, 300.0, 400.0, 500.0, 600.0, 623.15] {
        inputs.extend([if97_input("PT", 1e3, t), if97_input("PT", 50e6, t)]);
    }
    inputs.extend([0.0, 0.5, 1.0].map(|q| if97_input("QT", q, 450.0)));
    inputs
}

/// The kit's identities (`Identity`) and gauge invariance on IF97 water (PLAN.md M14.3).
#[test]
fn if97_water_passes_the_kit() {
    let fluid = water();
    let states: Vec<State> = kit_inputs().into_iter().map(|input| fluid.state(input).unwrap()).collect();
    let checked = identities(&fluid, &states).unwrap();
    assert!(checked >= 12 * 10, "the first-order identities at every single-phase state: {checked}");
    let gauged = gauge_invariance(&fluid, Gauge::new(1234.5, -6.75).unwrap(), &kit_inputs());
    assert!(gauged.is_ok(), "{gauged:?}");
}

/// The kit's thread invariance on IF97 water (PLAN.md M14.3): eight threads sharing one fluid get a lone fluid's states
/// bit for bit.
#[cfg(not(target_family = "wasm"))] // wasip2 has no thread spawning
#[test]
fn if97_water_is_thread_invariant() {
    use phasekit_verify::conformance::thread_invariance;
    let threads = thread_invariance(water, &kit_inputs(), 8);
    assert!(threads.is_ok(), "{threads:?}");
}

/// K17: PT, QT and PQ batches of IF97 water under `Parallel` and `Simd` give `Reference`'s cells bit for bit, its
/// refusals (below 273.15 K, above 623.15 K or 100 MPa) among them (PLAN.md M14.3).
#[test]
fn if97_water_batches_agree_under_every_policy() {
    let water = water();
    let outputs = [Prop::P, Prop::Hmass, Prop::Cpmass, Prop::Q];
    let t: Vec<f64> = (0..37).map(|i| 270.0 + 10.0 * f64::from(i)).collect();
    let p: Vec<f64> = (0..37).map(|i| 1e3 * math::powi(1.4, i)).collect();
    let q: Vec<f64> = (0..37).map(|i| f64::from(i % 5) / 4.0).collect();
    let requests = [
        BatchRequest::new(Pair::PT, Basis::Mass, &p, &t, &outputs),
        BatchRequest::new(Pair::QT, Basis::Mass, &q, &t, &outputs),
        BatchRequest::new(Pair::PQ, Basis::Mass, &p, &q, &outputs),
    ];
    for exec in [ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() }, ExecPolicy::Simd] {
        for req in &requests {
            let result = policy_equivalence(&water, req, exec);
            assert!(result.is_ok(), "{exec:?}: {result:?}");
        }
    }
}

/// ROT-242, region 2's half: v8.0.0's IF97 refuses p below 611.213 Pa (its PT rows start at 1 kPa), though region 2
/// is defined for any p > 0 (R7-97(2012) §6.1); IF97 water's vapour at 300 K and 100 Pa is region 2's state, its
/// compressibility factor 1 − 5.8e-5, the second virial's share there.
#[test]
fn if97_water_vapour_goes_below_611_pa() {
    let state = water().state(if97_input("PT", 100.0, 300.0)).unwrap();
    assert_eq!(state.phase(), Phase::Gas);
    assert_eq!(state.rho(Basis::Mass).to_bits(), if97::region2(300.0, 100.0).unwrap().rho(Basis::Mass).to_bits());
    let z = state.z();
    assert!(-1e-4 < z - 1.0 && z < 1.0, "Z = {z}");
}

/// Ice Ih (PLAN.md M14.4) on core's embedded IAPWS-95 water, under `data`.
fn ice(data: DataSet) -> Fluid {
    let water = Registry::from_embedded(data).unwrap().get("Water").unwrap().clone();
    phasekit_iapws::ice::ice(&water, data).unwrap()
}

/// Table 6's 16 quantities at a state of ice, from its `State` alone: g and its derivatives through g_p = 1/ρ,
/// g_T = −s, g_pp = −κ_T/ρ, g_Tp = α/ρ, g_TT = −c_p/T; f = g − pv; β = α/κ_T; κ_s = 1/(ρw²).
fn table_6_quantities(state: &State) -> Vec<(&'static str, f64)> {
    let (t, rho) = (state.t(), state.rho(Basis::Mass));
    let (kappa_t, alpha) = (state.isothermal_compressibility().unwrap(), state.isobaric_expansion().unwrap());
    let (cp, w) = (state.cp(Basis::Mass).unwrap(), state.speed_of_sound().unwrap());
    vec![
        ("g", state.g(Basis::Mass)),
        ("g_p", 1.0 / rho),
        ("g_T", -state.s(Basis::Mass)),
        ("g_pp", -kappa_t / rho),
        ("g_Tp", alpha / rho),
        ("g_TT", -cp / t),
        ("h", state.h(Basis::Mass)),
        ("f", state.a(Basis::Mass)),
        ("u", state.u(Basis::Mass)),
        ("s", state.s(Basis::Mass)),
        ("cp", cp),
        ("rho", rho),
        ("alpha", alpha),
        ("beta", alpha / kappa_t),
        ("kappa_T", kappa_t),
        ("kappa_s", 1.0 / (rho * w * w)),
    ]
}

/// Arbiter: IAPWS R10-06(2009) Table 6, ice Ih at the triple point, the normal melting point and (100 K, 100 MPa), its
/// 48 values within their printed digits, under `Corrected` (the 2009 g₀₀; PLAN.md M14.4).
#[test]
fn ice_is_table_6() {
    let (table, ice) = (table(fixture!("paper/Water/IAPWS-R10-06-2009.6.csv")), ice(DataSet::Corrected));
    let mut missed = Vec::new();
    for row in 0..table.rows().len() {
        let quantity = match table.rows()[row].cells.first() {
            Some(Cell::Text(q)) => *q,
            _ => "",
        };
        let (t, p) = (table.value(row, "T").unwrap(), table.value(row, "p").unwrap());
        let got = ice.state(Input::new(Pair::PT, p, t, Basis::Mass).unwrap()).map(|s| table_6_quantities(&s));
        match got {
            Ok(values) => {
                let value = values.iter().find(|(q, _)| *q == quantity).map_or(f64::NAN, |(_, v)| *v);
                missed.extend(table.check(row, "value", value).err().map(|e| format!("{quantity} at {t} K: {e:?}")));
            }
            Err(e) => missed.push(format!("{quantity} at {t} K: {e}")),
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// ROT-248 (PLAN.md M14.4): ice's properties come from core's relations over all six Gibbs derivatives: the kit's
/// first-order identities hold at ice states from 50 K to the triple point and to 200 MPa, where v8.0.0's ice lacks
/// g_TT and g_Tp (map 07 C2); its range is refused beyond, never a sentinel.
#[test]
fn ice_properties_come_from_the_shared_relations() {
    let ice = ice(DataSet::Corrected);
    let pt = |p: f64, t: f64| Input::new(Pair::PT, p, t, Basis::Mass).unwrap();
    let states: Vec<State> = [50.0, 100.0, 200.0, 273.16]
        .into_iter()
        .flat_map(|t| [1.0, 1e5, 200e6].map(|p| ice.state(pt(p, t)).unwrap()))
        .collect();
    assert!(states.iter().all(|s| s.phase() == Phase::Solid));
    let checked = identities(&ice, &states).unwrap();
    assert!(checked >= 12 * 10, "{checked}");
    assert!(matches!(ice.state(pt(1e5, 273.2)), Err(Error::Domain(_))));
    assert!(matches!(ice.state(pt(211e6, 200.0)), Err(Error::Domain(_))));
}

/// Arbiter: IAPWS R14-08(2011) Table 3's sublimation row, p = 8.94735e-6 MPa at 230 K, within its printed digits, and
/// Eq. (6)'s inverse back to 230 K (PLAN.md M14.5).
#[test]
fn sublimation_is_table_3() {
    let table = table(fixture!("paper/Water/IAPWS-R14-08-2011.3.csv"));
    let row = (0..table.rows().len()).find(|&r| table.printed(r, "note") == Some("ice Ih - gas Eq. (6)")).unwrap();
    let t = table.value(row, "T").unwrap();
    let p = phasekit_iapws::ice::sublimation_pressure(t).unwrap();
    assert_eq!(table.check(row, "p", p / 1e6).map_err(|e| format!("{e:?}")), Ok(()));
    let back = phasekit_iapws::ice::sublimation_temperature(p).unwrap();
    assert!((back / t - 1.0).abs() <= 4.0 * f64::EPSILON, "{back}");
}

/// The molar Gibbs energy and volume of `water`'s vapour at (T, p), through its model (no gauge).
fn vapour_g_and_v(water: &Fluid, t: f64, p: f64) -> (f64, f64) {
    let eos = water.model().helmholtz().unwrap();
    let rho = phasekit_core::density::solve(eos, t, p, phasekit_core::density::Branch::Vapour, None).unwrap();
    let total = eos.ideal(t, rho, phasekit_core::Order::One) + eos.residual(t, rho, phasekit_core::Order::One);
    (eos.gas_constant() * t * (total.get(0, 0).unwrap() + total.get(0, 1).unwrap()), 1.0 / rho)
}

/// R14-08(2011) §6 Eq. (7a): the expanded uncertainty (k = 2) of Eq. (6)'s pressure above 130 K, relative.
fn sublimation_uncertainty(t: f64) -> f64 {
    let x = 273.16 / t;
    let (a, b, c) = (1.1e-5, 1e-2 * (x - 1.0), 4e-4 * (x - 1.0 - math::ln(x)));
    math::sqrt(a * a + b * b + c * c)
}

/// R14-08(2011) §3, §6: Eq. (6) was fitted to the equilibrium of R10-06(2009)'s ice and IAPWS-95's vapour from 130 K.
/// At 15 temperatures from 130 K to the triple point the pressure where ice's and the vapour's Gibbs energies are
/// equal (Newton in p from Eq. (6)'s) lies within Eq. (7a)'s uncertainty of Eq. (6)'s, the largest gap 0.33 of it
/// (measured at M14.5).
#[test]
fn sublimation_is_the_equilibrium_of_ice_and_iapws_95() {
    let water = Registry::from_embedded(DataSet::Corrected).unwrap().get("Water").unwrap().clone();
    let ice = ice(DataSet::Corrected);
    let mut worst = 0.0_f64;
    for k in 0..15 {
        let t = 130.0 + f64::from(k) * (273.16 - 130.0) / 14.0;
        let p_subl = phasekit_iapws::ice::sublimation_pressure(t).unwrap();
        let mut p = p_subl;
        for _ in 0..8 {
            let solid = ice.state(Input::new(Pair::PT, p, t, Basis::Mass).unwrap()).unwrap();
            let (g, v) = vapour_g_and_v(&water, t, p);
            p -= (solid.g(Basis::Molar) - g) / (1.0 / solid.rho(Basis::Molar) - v);
        }
        let ratio = (p / p_subl - 1.0).abs() / sublimation_uncertainty(t);
        worst = worst.max(ratio);
        assert!(ratio <= 1.0, "{t} K: equilibrium {p} Pa against Eq. (6)'s {p_subl} Pa");
    }
    assert!(worst <= 0.34, "the largest gap, as a share of Eq. (7a)'s uncertainty: {worst}");
}

/// Ice's sublimation states (PLAN.md M14.5): ice and the borrowed water's vapour through `State::from_split` at Eq.
/// (6)'s pressure, the lever rule in h; PQ the same state; refused outside Eq. (6)'s 50 K to the triple point.
#[test]
fn sublimation_states_split_ice_and_vapour() {
    let (ice, water) = (ice(DataSet::Corrected), Registry::embedded().unwrap().get("Water").unwrap().clone());
    let (t, q) = (250.0, 0.25);
    let s = ice.state(Input::new(Pair::QT, q, t, Basis::Mass).unwrap()).unwrap();
    let p = phasekit_iapws::ice::sublimation_pressure(t).unwrap();
    assert_eq!((s.phase(), s.quality(), s.p(), s.t()), (Phase::TwoPhase, Some(q), p, t));
    let solid = ice.state(Input::new(Pair::PT, p, t, Basis::Mass).unwrap()).unwrap().h(Basis::Molar);
    let eos = water.model().helmholtz().unwrap();
    let rho = phasekit_core::density::solve(eos, t, p, phasekit_core::density::Branch::Vapour, None).unwrap();
    let total = eos.ideal(t, rho, phasekit_core::Order::One) + eos.residual(t, rho, phasekit_core::Order::One);
    let vapour = eos.gas_constant() * t * (total.get(1, 0).unwrap() + total.get(0, 1).unwrap());
    assert!((s.h(Basis::Molar) / ((1.0 - q) * solid + q * vapour) - 1.0).abs() < 1e-12, "{}", s.h(Basis::Molar));
    let pq = ice.state(Input::new(Pair::PQ, p, q, Basis::Mass).unwrap()).unwrap();
    assert!((pq.t() / t - 1.0).abs() <= 4.0 * f64::EPSILON && pq.quality() == Some(q));
    for t in [49.9, 273.17] {
        assert!(matches!(ice.state(Input::new(Pair::QT, q, t, Basis::Mass).unwrap()), Err(Error::Domain(_))), "{t}");
    }
}

/// `tests/gibbs_seam.rs`'s checks on the real ice model (PLAN.md M14.5): registered beside the embedded fluids, it is
/// the same through compat's strings, the batch (PT and QT, `Simd` the reference bit for bit) and a reference state.
#[test]
fn ice_reaches_registry_batch_compat_and_reference_states() {
    let embedded = Registry::embedded().unwrap();
    let water = embedded.get("Water").unwrap().clone();
    let registry = embedded.with_model(phasekit_iapws::ice::model(&water, DataSet::Corrected).unwrap()).unwrap();
    let ice = registry.get("Ice Ih").unwrap();
    let pt = |p: f64, t: f64| Input::new(Pair::PT, p, t, Basis::Mass).unwrap();
    let h = ice.state(pt(101_325.0, 250.0)).unwrap().h(Basis::Mass);
    assert_eq!(phasekit_compat::props_si_in(&registry, "Hmass", "P", 101_325.0, "T", 250.0, "Ice Ih").unwrap(), h);
    let outputs = [Prop::Dmass, Prop::Hmass, Prop::Q];
    let (p, t, q) = ([101_325.0, 2e8, 1e3], [250.0, 260.0, 270.0], [0.0, 0.5, 1.0]);
    for req in [
        BatchRequest::new(Pair::PT, Basis::Mass, &p, &t, &outputs),
        BatchRequest::new(Pair::QT, Basis::Mass, &q, &t, &outputs),
    ] {
        let result = policy_equivalence(ice, &req, ExecPolicy::Simd);
        assert!(result.is_ok(), "{result:?}");
    }
    let anchor = pt(101_325.0, 273.15);
    let (h0, s0) = (Enthalpy::mass(-333_430.0).unwrap(), Entropy::mass(-1220.0).unwrap());
    let shifted = ice.with_reference(ReferenceState::Custom { at: anchor, h: h0, s: s0 }).unwrap();
    assert!((shifted.state(anchor).unwrap().h(Basis::Mass) + 333_430.0).abs() < 1e-6);
    let gauged = gauge_invariance(
        ice,
        shifted.gauge(),
        &[anchor, pt(5e7, 200.0), Input::new(Pair::QT, 0.5, 250.0, Basis::Mass).unwrap()],
    );
    assert!(gauged.is_ok(), "{gauged:?}");
}
