//! Humid air through `phasekit-humidair` (PLAN.md M18; user decisions HA1-HA6): its parts, its virials against IAPWS
//! G11-15 and G8-10, within their printed digits (`Paper`), the values converted to the tables' units.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::math;
use phasekit_core::{Basis, DataSet, Error, Fluid, Gauge, Input, Pair, ReferenceState, Registry};
use phasekit_humidair::{Coefficient, Gas, Given, HumidAir, Humidity, MoistAir, Parts, Per, Virials, henry_constant};
use phasekit_verify::register::exempt_at;
use phasekit_verify::{Cell, DIVERGENCES, Fixture, Kind, Tolerance, ToleranceClass, fixture};

/// The table at `path`, parsed.
fn table((path, text): (&'static str, &'static str)) -> Fixture<'static> {
    Fixture::parse(path, text).unwrap()
}

/// Humid air's four parts under `data`: core's IAPWS-95 water and Lemmon's air, phasekit-iapws's IF97 water and ice.
fn parts(data: DataSet) -> Parts {
    let registry = Registry::from_embedded(data).unwrap();
    let water = registry.get("Water").unwrap().clone();
    let air = registry.get("Air").unwrap().clone();
    let liquid = phasekit_iapws::if97::water().unwrap();
    let ice = phasekit_iapws::ice::ice(&water, data).unwrap();
    Parts { water, air, liquid, ice }
}

/// RP-1485's humid air under `data`.
fn humid_air(data: DataSet) -> HumidAir {
    HumidAir::new(parts(data), data).unwrap()
}

/// The row's first cell, its quantity.
fn quantity<'a>(table: &Fixture<'a>, row: usize) -> &'a str {
    match table.rows()[row].cells.first() {
        Some(Cell::Text(q)) => q,
        _ => "",
    }
}

/// A coefficient's value or derivative by a table's quantity name: `B_AW`, `dB_AW/dT`, `d2B_AW/dT2`.
fn by_name(v: &Virials, name: &str) -> Option<f64> {
    let (base, pick): (&str, fn(&Coefficient) -> f64) = match name.split_once('/') {
        None => (name, |c| c.value),
        Some((d, "dT")) => (d.trim_start_matches('d'), |c| c.d_dt),
        Some((d, _)) => (d.trim_start_matches("d2"), |c| c.d2_dt2),
    };
    let c = match base {
        "B_AA" => v.b_aa,
        "C_AAA" => v.c_aaa,
        "B_WW" => v.b_ww,
        "C_WWW" => v.c_www,
        "B_AW" => v.b_aw,
        "C_AAW" => v.c_aaw,
        "C_AWW" => v.c_aww,
        _ => return None,
    };
    Some(pick(&c))
}

/// DIV-0067 (`KeepOracle`): C_aww's b₀ and b₁ are v8.0.0's, which miss G8-10's (Hyland and Wexler's) at the tables'
/// 200, 300 and 400 K by up to 1.06e-4 in C_aww, 1.15e-4 in its first and 1.22e-4 in its second T-derivative,
/// relative, all at 200 K (measured at M18.1): the register's bound; NaN, which fails every comparison, without one.
fn c_aww_gap() -> f64 {
    match DIVERGENCES.iter().find(|d| d.id == "DIV-0067").and_then(|d| d.tolerance) {
        Some(Tolerance::Relative(bound)) => bound,
        _ => f64::NAN,
    }
}

/// The misses of `table`'s virial rows against humid air's, each `Paper` but C_aww's, which DIV-0067 holds to its gap.
fn virial_misses(table: &Fixture<'_>, air: &HumidAir) -> (Vec<String>, usize) {
    let (mut missed, mut kept) = (Vec::new(), 0);
    for row in 0..table.rows().len() {
        let (name, t) = (quantity(table, row), table.value(row, "T").unwrap());
        let got = by_name(&air.virials(t).unwrap(), name).unwrap();
        if name.contains("C_AWW") {
            let want = table.value(row, "value").unwrap();
            if (got / want - 1.0).abs() > c_aww_gap() {
                missed.push(format!("{name} at {t} K: {got} against {want}, beyond DIV-0067's gap"));
            }
            kept += 1;
            continue;
        }
        missed.extend(table.check(row, "value", got).err().map(|e| format!("{name} at {t} K: {e:?}")));
    }
    (missed, kept)
}

/// Arbiter: IAPWS G11-15 Table 7, the seven virials at 200, 300 and 400 K (PLAN.md M18.1): B_aa, C_aaa, B_ww and C_www
/// the air and water parts' exact virials, B_aw and C_aaw G8-10's forms, within their printed digits; C_aww within
/// DIV-0067's gap.
#[test]
fn virials_are_g11_15_table_7() {
    let (missed, kept) =
        virial_misses(&table(fixture!("paper/HumidAir/IAPWS-G11-15.7.csv")), &humid_air(DataSet::Corrected));
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!(kept, 3);
}

/// Arbiter: IAPWS G8-10 Table 15, B_aw, C_aaw and C_aww with their first two T-derivatives at 200, 300 and 400 K
/// (PLAN.md M18.1), within their printed digits; C_aww's three within DIV-0067's gap.
#[test]
fn cross_virials_are_g8_10_table_15() {
    let (missed, kept) =
        virial_misses(&table(fixture!("paper/HumidAir/IAPWS-G8-10.15.csv")), &humid_air(DataSet::Corrected));
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!(kept, 9);
}

/// Each part checked at construction (PLAN.md M18.1; ROT-267): a water or air without exact virials, a liquid or ice
/// of another molar mass (Air; heavy-water ice), and one with no saturation state where it needs one (a van der Waals
/// water has no quality pairs; IF97 water stops at 273.15 K) are each refused with the part's own error.
#[test]
fn humid_air_checks_its_parts() {
    let good = parts(DataSet::Corrected);
    let ice_as_water = Parts { water: good.ice.clone(), ..good.clone() };
    assert!(matches!(HumidAir::new(ice_as_water, DataSet::Corrected), Err(Error::NoModel { .. })));
    let vdw = phasekit_cubic::fluid(&good.water, phasekit_cubic::Kind::Vdw, None, DataSet::Corrected).unwrap();
    let no_quality = Parts { liquid: vdw.clone(), ..good.clone() };
    assert_eq!(HumidAir::new(no_quality, DataSet::Corrected).unwrap_err(), Error::Unsupported { pair: Pair::QT });
    let no_quality_ice = Parts { ice: vdw, ..good.clone() };
    assert_eq!(HumidAir::new(no_quality_ice, DataSet::Corrected).unwrap_err(), Error::Unsupported { pair: Pair::QT });
    let air_as_liquid = Parts { liquid: good.air.clone(), ..good.clone() };
    assert!(matches!(HumidAir::new(air_as_liquid, DataSet::Corrected), Err(Error::InvalidInput { .. })));
    let water_as_ice = Parts { ice: good.liquid.clone(), ..good.clone() };
    assert!(matches!(HumidAir::new(water_as_ice, DataSet::Corrected), Err(Error::Domain(_))), "no IF97 at 250 K");
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let heavy = phasekit_iapws::ice::ice(registry.get("HeavyWater").unwrap(), DataSet::Corrected).unwrap();
    let heavy_ice = Parts { ice: heavy, ..good.clone() };
    assert!(matches!(HumidAir::new(heavy_ice, DataSet::Corrected), Err(Error::InvalidInput { .. })));
    let air = humid_air(DataSet::Parity);
    assert_eq!(air.data_set(), DataSet::Parity);
    assert_eq!(air.parts().water.model().info().name(), "Water");
}

/// ROT-268: humid air is `Send + Sync`, holds its parts by value with no thread-local copy, and gives eight threads the
/// same virials bit for bit.
#[cfg(not(target_family = "wasm"))] // wasip2 has no thread spawning
#[test]
fn humid_air_is_shared_across_threads() {
    fn shared<T: Send + Sync>(_: &T) {}
    let air = humid_air(DataSet::Corrected);
    shared(&air);
    let bits = |a: &HumidAir| {
        let v = a.virials(287.5).unwrap();
        [v.b_aa, v.c_www, v.b_aw, v.c_aww].map(|c| [c.value, c.d_dt, c.d2_dt2].map(f64::to_bits))
    };
    let want = bits(&air);
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..8).map(|_| s.spawn(|| bits(&air))).collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), want);
        }
    });
    let _: Option<Fluid> = None;
}

/// The relative bound the register entry `id` measured: DIV-0011's, v8.0.0's B from its EOS at δ = 1e-12 and what
/// inherits it; DIV-0069's, its moist-air states. NaN, which fails every comparison, without one.
fn registered(id: &str) -> f64 {
    match DIVERGENCES.iter().find(|d| d.id == id).and_then(|d| d.tolerance) {
        Some(Tolerance::Relative(bound)) => bound,
        _ => f64::NAN,
    }
}

/// G8-10's own C_aww (Eq. (11), Table 4's bᵢ: Hyland and Wexler 1983) and its two T-derivatives at `t`, for the table's
/// self-consistency: C = −c* e^S, S = Σ bᵢ T̄^−ⁱ, T̄ = T/100 K.
fn g8_10_c_aww(t: f64) -> [f64; 3] {
    let (x, b) = (t / 100.0, [-0.107_288_76e2, 0.347_802e2, -0.383_383e2, 0.334_06e2]);
    let (mut s, mut s1, mut s2) = (0.0, 0.0, 0.0);
    for (i, bi) in (0_i32..).zip(b) {
        let fi = f64::from(i);
        s += bi * phasekit_core::math::powi(x, -i);
        s1 -= fi * bi * phasekit_core::math::powi(x, -i - 1) / 100.0;
        s2 += fi * (fi + 1.0) * bi * phasekit_core::math::powi(x, -i - 2) / 1e4;
    }
    let c = -1e-6 * phasekit_core::math::exp(s);
    [c, c * s1, c * (s2 + s1 * s1)]
}

/// IAPWS G8-10 Table 15's C_aww rows are its own Eq. (11) with its Table 4, within their printed digits: the table is
/// self-consistent, which `ARBITERS` records, though humid air keeps v8.0.0's C_aww (DIV-0067).
#[test]
fn g8_10_table_15_is_its_own_c_aww() {
    let table = table(fixture!("paper/HumidAir/IAPWS-G8-10.15.csv"));
    let mut missed = Vec::new();
    for row in (0..table.rows().len()).filter(|&r| quantity(&table, r).contains("C_AWW")) {
        let (name, t) = (quantity(&table, row), table.value(row, "T").unwrap());
        let k = usize::from(name.starts_with("dC")) + 2 * usize::from(name.starts_with("d2"));
        let got = g8_10_c_aww(t).get(k).copied().unwrap();
        missed.extend(table.check(row, "value", got).err().map(|e| format!("{name} at {t} K: {e:?}")));
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Oracle: CoolProp 8.0.0's `HAProps_Aux`, `fixtures/coolprop-8.0.0/all/humidair_aux.csv` (PLAN.md M18.1): at 17
/// temperatures from 200 to 600 K, the cross virials B_aw, C_aaw and C_aww (DIV-0067: v8.0.0's digits) within `Prop`,
/// and B_aa and B_ww within DIV-0011's bound, v8.0.0 taking them at δ = 1e-12; its C_aaa and C_www are DIV-0011's.
#[test]
fn virials_match_the_oracle() {
    let (table, air) = (table(fixture!("coolprop-8.0.0/all/humidair_aux.csv")), humid_air(DataSet::Parity));
    let div_0011 = registered("DIV-0011");
    let (mut missed, mut exempt) = (Vec::new(), 0);
    for row in 0..table.rows().len() {
        let t = table.value(row, "T").unwrap();
        let v = air.virials(t).unwrap();
        let cells = [
            ("Baa", v.b_aa.value, div_0011),
            ("Caaa", v.c_aaa.value, 0.0),
            ("Bww", v.b_ww.value, div_0011),
            ("Cwww", v.c_www.value, 0.0),
            ("Baw", v.b_aw.value, 0.0),
            ("Caaw", v.c_aaw.value, 0.0),
            ("Caww", v.c_aww.value, 0.0),
        ];
        for (column, got, relative) in cells {
            if exempt_at(DIVERGENCES, "Humid air", Kind::HumidairAux, column, t).is_some() {
                exempt += 1;
                continue;
            }
            let want = table.value(row, column).unwrap();
            let bound =
                if relative > 0.0 { relative * want.abs() } else { ToleranceClass::Prop.bound(want.abs()).unwrap() };
            let error = (got - want).abs();
            if error.is_nan() || error > bound {
                missed.push(format!("{column} at {t} K: {got} against {want}"));
            }
        }
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!(exempt, 2 * table.rows().len(), "C_aaa and C_www, DIV-0011's");
}

/// Arbiter: IAPWS G7-04 Table 6, ln(k_H/1 GPa) of N₂, O₂ and Ar in H₂O at 300 to 600 K, within their printed digits
/// (PLAN.md M18.2): Eq. (3) over Eq. (4)'s vapour pressure; Ar at 600 K lies outside its fit (italic) and still holds.
#[test]
fn henry_is_g7_04_table_6() {
    let table = table(fixture!("paper/HumidAir/IAPWS-G7-04.6.csv"));
    let mut missed = Vec::new();
    for row in 0..table.rows().len() {
        let gas = match quantity(&table, row) {
            "N2" => Gas::N2,
            "O2" => Gas::O2,
            _ => Gas::Ar,
        };
        let t = table.value(row, "T").unwrap();
        let ln_k = math::ln(henry_constant(gas, t).unwrap() / 1e9);
        missed.extend(table.check(row, "value", ln_k).err().map(|e| format!("{gas:?} at {t} K: {e:?}")));
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Arbiter: the authors' Tables 1 and 2 (Herrmann, Kretzschmar and Gatley 2009), W_s of saturated moist air at
/// 101.325 kPa from −60 to 90 °C, within its printed digits under `Corrected` (PLAN.md M18.2): over ice below the
/// triple point, over IF97's liquid above.
#[test]
fn saturated_humidity_ratio_is_tables_1_and_2() {
    let air = humid_air(DataSet::Corrected);
    let mut missed = Vec::new();
    for table in
        [fixture!("paper/HumidAir/Herrmann-HVACR-2009.1.csv"), fixture!("paper/HumidAir/Herrmann-HVACR-2009.2.csv")]
    {
        let table = table_of(table);
        for row in 0..table.rows().len() {
            let t = table.value(row, "t").unwrap() + 273.15;
            let w = air.saturated_humidity_ratio(t, 101_325.0).unwrap_or(f64::NAN);
            missed.extend(table.check(row, "Ws", w).err().map(|e| format!("{t} K: {e:?}")));
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// DIV-0071's bound on the printed `column` of the table at `path`, in the table's units (user decision HA8): h (`h`,
/// `hs`) within the cell's `half` unit plus W times the entry's constant per kilogram of water, s within the entry's
/// own bound for Tables 8 and 12; every other cell within its `half` unit.
fn div_0071_bound(path: &str, column: &str, w: f64, half: f64) -> f64 {
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0071").unwrap();
    let own = entry.table_tolerances.iter().find(|(file, _)| *file == path).map(|(_, bound)| *bound);
    match (column, entry.tolerance, own) {
        ("h" | "hs", Some(Tolerance::Absolute(per_kg_water)), _) => half + w * per_kg_water / 1e3,
        ("s", _, Some(Tolerance::Absolute(bound))) => bound,
        _ => half,
    }
}

/// Half a unit in the last digit of the printed `column` at `row`, in the table's units.
fn half_unit(table: &Fixture<'_>, row: usize, column: &str) -> f64 {
    match table.printed(row, column).and_then(phasekit_verify::from_printed) {
        Some(Tolerance::Absolute(half)) => half,
        _ => f64::NAN,
    }
}

/// Arbiter: the authors' Tables 1 and 2, v_s, h_s and s_s of saturated moist air at 101.325 kPa from −60 to 90 °C, per
/// kilogram of dry air, within their printed digits under `Corrected` (PLAN.md M18.3), Table 2's h_s within DIV-0071's
/// bound (user decision HA8).
#[test]
fn saturated_states_are_tables_1_and_2() {
    let air = humid_air(DataSet::Corrected);
    let mut missed = Vec::new();
    for (fixture, widened) in [
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.1.csv"), false),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.2.csv"), true),
    ] {
        let table = table_of(fixture);
        for row in 0..table.rows().len() {
            missed.extend(saturated_row(&air, (fixture.0, &table), row, widened));
        }
    }
    assert_eq!(missed, Vec::<String>::new());
}

/// Row `row` of Table 1 or 2 at `path`: its v_s, h_s and s_s within their printed digits, or where `widened` (Table 2)
/// within DIV-0071's bound; the misses, as text.
fn saturated_row(air: &HumidAir, (path, table): (&str, &Fixture<'_>), row: usize, widened: bool) -> Vec<String> {
    let t = table.value(row, "t").unwrap() + 273.15;
    let psi = air.saturation(t, 101_325.0).unwrap().psi_ws;
    let state = air.moist_air(t, 101_325.0, psi).unwrap();
    let cells =
        [("vs", state.v(Per::KgDryAir)), ("hs", state.h(Per::KgDryAir) / 1e3), ("ss", state.s(Per::KgDryAir) / 1e3)];
    let w = table.value(row, "Ws").unwrap();
    let mut missed = Vec::new();
    for (column, got) in cells {
        let (want, half) = (table.value(row, column).unwrap(), half_unit(table, row, column));
        let bound = if widened { div_0071_bound(path, column, w, half) } else { half };
        let check = table.check_bound(row, column, got, bound);
        missed.extend(check.err().map(|_| format!("{column} at {t} K: {got} against {want}")));
    }
    missed
}

/// The authors' Tables 5-14 with their temperature (°C) and pressure (Pa).
fn tables_5_to_14() -> [((&'static str, &'static str), f64, f64); 10] {
    [
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.5.csv"), 200.0, 101_325.0),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.6.csv"), 200.0, 1e6),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.7.csv"), 200.0, 2e6),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.8.csv"), 200.0, 5e6),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.9.csv"), 200.0, 1e7),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.10.csv"), 320.0, 101_325.0),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.11.csv"), 320.0, 1e6),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.12.csv"), 320.0, 2e6),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.13.csv"), 320.0, 5e6),
        (fixture!("paper/HumidAir/Herrmann-HVACR-2009.14.csv"), 320.0, 1e7),
    ]
}

/// Arbiter: the authors' Tables 5-14, moist air at 200 and 320 °C from 101.325 kPa to 10 MPa: v, h and s per kilogram
/// of dry air at each row's W, within their printed digits under `Corrected` (PLAN.md M18.3; t_wb and φ are M18.4's),
/// h and Tables 8's and 12's s within DIV-0071's bound (user decision HA8).
#[test]
fn states_are_tables_5_to_14() {
    let air = humid_air(DataSet::Corrected);
    let (mut missed, mut checked) = (Vec::new(), 0);
    for (fixture, t, p) in tables_5_to_14() {
        let (path, table, t) = (fixture.0, table_of(fixture), t + 273.15);
        for row in 0..table.rows().len() {
            let w = table.value(row, "W").unwrap();
            let psi = air.mole_fraction(t, p, Humidity::Ratio(w)).unwrap();
            let state = air.moist_air(t, p, psi).unwrap();
            let cells = [
                ("v", state.v(Per::KgDryAir)),
                ("h", state.h(Per::KgDryAir) / 1e3),
                ("s", state.s(Per::KgDryAir) / 1e3),
            ];
            for (column, got) in cells {
                checked += 1;
                let (want, half) = (table.value(row, column).unwrap(), half_unit(&table, row, column));
                missed.extend(
                    table
                        .check_bound(row, column, got, div_0071_bound(path, column, w, half))
                        .err()
                        .map(|_| format!("{column} at {t} K, {p} Pa, W {w}: {got} against {want}")),
                );
            }
        }
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!(checked, 318);
}

/// Arbiter: the authors' Tables 5-14 at each row's W under `Corrected` (PLAN.md M18.4): t_wb (106 values), and φ at
/// 200 °C from 2 MPa (22, in percent), within their printed digits.
#[test]
fn wet_bulb_and_relative_humidity_are_tables_5_to_14() {
    let air = humid_air(DataSet::Corrected);
    let (mut missed, mut checked) = (Vec::new(), 0);
    for (fixture, t, p) in tables_5_to_14() {
        let (table, t) = (table_of(fixture), t + 273.15);
        for row in 0..table.rows().len() {
            let w = table.value(row, "W").unwrap();
            let psi = air.mole_fraction(t, p, Humidity::Ratio(w)).unwrap();
            let state = air.moist_air(t, p, psi).unwrap();
            let mut cells = vec![("twb", air.wet_bulb(&state).map_or(f64::NAN, |twb| twb - 273.15))];
            if table.columns().contains(&"phi") {
                cells.push(("phi", air.relative_humidity(t, p, psi).map_or(f64::NAN, |phi| 100.0 * phi)));
            }
            for (column, got) in cells {
                checked += 1;
                let want = table.value(row, column).unwrap();
                missed.extend(
                    table
                        .check(row, column, got)
                        .err()
                        .map(|_| format!("{column} at {t} K, {p} Pa, W {w}: {got} {want}")),
                );
            }
        }
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!(checked, 128);
}

/// What a state gives every input as: T, W, ψ_w, h, s, v, T_wb, T_dp (none for dry air) and φ (none above water's
/// boiling point at p, where v8.0.0 refuses it).
fn as_inputs(air: &HumidAir, state: &MoistAir) -> Vec<Given> {
    let (t, p, psi) = (state.t(), state.p(), state.mole_fraction());
    let mut given = vec![
        Given::Temperature(t),
        Given::Humidity(Humidity::Ratio(state.humidity_ratio())),
        Given::Humidity(Humidity::MoleFraction(psi)),
        Given::Enthalpy(state.h(Per::KgDryAir)),
        Given::Entropy(state.s(Per::KgDryAir)),
        Given::Volume(state.v(Per::KgDryAir)),
        Given::WetBulb(air.wet_bulb(state).unwrap()),
    ];
    if psi > 0.0 {
        given.push(Given::Humidity(Humidity::DewPoint(air.dew_point(p, psi).unwrap())));
    }
    if air.saturation(t, p).unwrap().psi_ws < 1.0 {
        given.push(Given::Humidity(Humidity::Relative(air.relative_humidity(t, p, psi).unwrap())));
    }
    given
}

/// Whether `HumidAir::state` takes the pair: T and anything else; a humidity and h, s, v or T_wb; W, ψ_w or T_dp with
/// φ but for `dry` air (PLAN.md M18.4).
fn takes(a: Given, b: Given, dry: bool) -> bool {
    let fixed = |g: Given| matches!(g, Given::Humidity(h) if !matches!(h, Humidity::Relative(_)));
    let relative = |g: Given| matches!(g, Given::Humidity(Humidity::Relative(_)));
    let humidity = |g: Given| matches!(g, Given::Humidity(_));
    let temperature = |g: Given| matches!(g, Given::Temperature(_));
    match () {
        () if temperature(a) || temperature(b) => !(temperature(a) && temperature(b)),
        () if humidity(a) && humidity(b) => !dry && ((fixed(a) && relative(b)) || (relative(a) && fixed(b))),
        () => humidity(a) || humidity(b),
    }
}

/// Every input pair round-trips (PLAN.md M18.4): from unsaturated states at 260 to 470 K, 10 kPa to 5 MPa and W from 0
/// to 0.1, both data sets, every pair `HumidAir::state` takes, in either order, gives back T and ψ_w within `Flash`
/// (ψ_w's scale at least 1e-6), and every other pair is refused: 1996 pairs taken.
#[test]
fn every_input_pair_round_trips() {
    let mut checked = 0;
    for data in [DataSet::Corrected, DataSet::Parity] {
        let air = humid_air(data);
        for (t, p, w) in round_trip_states(&air) {
            let state = air.moist_air(t, p, air.mole_fraction(t, p, Humidity::Ratio(w)).unwrap()).unwrap();
            checked += round_trips_from(&air, &state);
        }
    }
    assert_eq!(checked, 1996);
}

/// Every pair of `state`'s inputs, each through [`round_trip`]: how many `HumidAir::state` takes.
fn round_trips_from(air: &HumidAir, state: &MoistAir) -> usize {
    let given = as_inputs(air, state);
    let pairs = given.iter().enumerate().flat_map(|(i, &a)| given.iter().skip(i + 1).map(move |&b| (a, b)));
    pairs.map(|(a, b)| round_trip(air, state, a, b)).sum()
}

/// The grid's unsaturated states (T, p, W) that the virial equation has (at 5 MPa its branch ends before some).
fn round_trip_states(air: &HumidAir) -> Vec<(f64, f64, f64)> {
    let mut states = Vec::new();
    for t in [260.0, 300.0, 350.0, 470.0] {
        for p in [1e4, 101_325.0, 1e6, 5e6] {
            let w_s = air.saturated_humidity_ratio(t, p).unwrap_or(f64::INFINITY);
            let unsaturated = |w: f64| w < w_s.max(0.0) || w_s < 0.0;
            let exists =
                |w: f64| air.mole_fraction(t, p, Humidity::Ratio(w)).and_then(|psi| air.moist_air(t, p, psi)).is_ok();
            states.extend(
                [0.0, 0.001, 0.01, 0.1].into_iter().filter(|&w| unsaturated(w) && exists(w)).map(|w| (t, p, w)),
            );
        }
    }
    states
}

/// One pair's round trip from `state`: 1 if `HumidAir::state` takes it and gives `state` back, 0 if it refuses a pair
/// it should; a panic otherwise.
fn round_trip(air: &HumidAir, state: &MoistAir, a: Given, b: Given) -> usize {
    let (p, t, psi) = (state.p(), state.t(), state.mole_fraction());
    let flash = |scale: f64| ToleranceClass::Flash.bound(scale).unwrap();
    for (x, y) in [(a, b), (b, a)] {
        let got = air.state(p, x, y);
        if !takes(x, y, psi == 0.0) {
            assert!(matches!(got, Err(Error::InvalidState { .. })), "{x:?}, {y:?} at {p} Pa: {got:?}");
            continue;
        }
        assert!(got.is_ok(), "{x:?}, {y:?} at {t} K, {p} Pa, ψ {psi}: {got:?}");
        let got = got.unwrap();
        assert!((got.t() - t).abs() <= flash(t), "{x:?}, {y:?}: T {} against {t}", got.t());
        let psi_got = got.mole_fraction();
        assert!((psi_got - psi).abs() <= flash(psi.max(1e-6)), "{x:?}, {y:?}: ψ {psi_got} against {psi}");
    }
    usize::from(takes(a, b, psi == 0.0))
}

/// ROT-104 (PLAN.md M18.4): every output, T_wb and T_dp included, is the one the parts give in their models' own
/// gauge, whatever reference state they carry: the IF97 water under NBP and IIR, the IAPWS-95 water under NBP and an
/// arbitrary gauge (IIR anchors at 0 °C, below IAPWS-95's triple point, which core refuses), the air and the ice under
/// that gauge; both data sets, bit for bit.
#[test]
fn every_output_is_gauge_invariant() {
    for data in [DataSet::Corrected, DataSet::Parity] {
        let native = humid_air(data);
        for reference in [ReferenceState::Nbp, ReferenceState::Iir] {
            let custom = Gauge::new(1234.5, -6.75).unwrap();
            let Parts { water, air, liquid, ice } = parts(data);
            let water = match reference {
                ReferenceState::Nbp => water.with_reference(reference).unwrap(),
                _ => water.with_gauge(custom),
            };
            let liquid = liquid.with_reference(reference).unwrap();
            let (air, ice) = (air.with_gauge(custom), ice.with_gauge(custom));
            let gauged = HumidAir::new(Parts { water, air, liquid, ice }, data).unwrap();
            for (t, p, w) in
                [(250.0, 101_325.0, 0.0003), (300.0, 101_325.0, 0.01), (320.0, 2e5, 0.02), (473.15, 1e6, 0.3)]
            {
                let outputs = |air: &HumidAir| {
                    let psi = air.mole_fraction(t, p, Humidity::Ratio(w)).unwrap();
                    let s = air.moist_air(t, p, psi).unwrap();
                    let per = Per::KgDryAir;
                    [s.h(per), s.s(per), s.u(per), s.v(per), s.cp(per), s.cv(per), s.speed_of_sound()]
                        .into_iter()
                        .chain([air.wet_bulb(&s).unwrap(), air.dew_point(p, psi).unwrap()])
                        .chain([air.relative_humidity(t, p, psi).unwrap()])
                        .collect::<Vec<_>>()
                };
                assert_eq!(outputs(&gauged), outputs(&native), "{data:?}, {reference:?}: {t} K, {p} Pa, W {w}");
            }
        }
    }
}

/// Oracle: map 11 §8's `HAPropsSI("H","T",300,"P",1e5,"R",0)`, 27013.112479771713 J/kg of dry air (PLAN.md M18.3),
/// under `Parity`: dry air's h on v8.0.0's zeros, within DIV-0069's bound (measured: 5.1e-11, v8.0.0's B and C at
/// δ = 1e-12).
#[test]
fn dry_air_at_300_k_is_map_11s_enthalpy() {
    let h = humid_air(DataSet::Parity).moist_air(300.0, 1e5, 0.0).unwrap().h(Per::KgDryAir);
    let want = 27_013.112_479_771_713;
    assert!((h / want - 1.0).abs() <= registered("DIV-0069"), "{h}");
}

/// The table at `path`, parsed (for a `fixture!` passed as a value).
fn table_of(fixture: (&'static str, &'static str)) -> Fixture<'static> {
    table(fixture)
}

/// Oracle: CoolProp 8.0.0's `HAProps_Aux`, `fixtures/coolprop-8.0.0/all/humidair_aux.csv` (PLAN.md M18.2), under
/// `Parity`: p_ws and v̄_ws within `Prop`; β_H where it enters f (liquid water with p_ws below p; v8.0.0's
/// `HAProps_Aux` returns its Henry's constant unconditionally, inf below 273.15 K); f within DIV-0011's bound, since
/// v8.0.0's Eq. (3.25) takes its B and C at δ = 1e-12 (measured: up to 9.8e-8 at 200 K and 5 MPa, 3.9e-11 at
/// 101.325 kPa).
#[test]
fn saturation_matches_the_oracle() {
    let (table, air) = (table(fixture!("coolprop-8.0.0/all/humidair_aux.csv")), humid_air(DataSet::Parity));
    let (mut missed, mut henry) = (Vec::new(), 0);
    for row in 0..table.rows().len() {
        let (t, p) = (table.value(row, "T").unwrap(), table.value(row, "p").unwrap());
        let s = air.saturation(t, p).unwrap();
        let mut cells = vec![("p_ws", s.p_ws), ("f", s.f), ("vbar_ws", s.v_condensed)];
        if s.beta_h > 0.0 {
            cells.push(("beta_H", s.beta_h));
            henry += 1;
        }
        for (column, got) in cells {
            let want = table.value(row, column).unwrap();
            let error = (got - want).abs();
            let bound = match exempt_at(DIVERGENCES, "Humid air", Kind::HumidairAux, column, t) {
                Some(_) => registered("DIV-0011") * want.abs(),
                None => ToleranceClass::Prop.bound(want.abs()).unwrap(),
            };
            if error.is_nan() || error > bound {
                missed.push(format!("{column} at {t} K, {p} Pa: {got} against {want} ({:.1e})", error / want.abs()));
            }
        }
    }
    assert_eq!(missed, Vec::<String>::new());
    assert!(henry > 20, "{henry}");
}

/// ROT-261, ROT-263, ROT-270 (PLAN.md M18.2): the condensed phase is liquid at the triple point and above and ice
/// below, one predicate; its κ_T and molar volume are its part's state at (T, p), a typed input: ice's (R10-06(2009),
/// Table 6's κ_T) and IF97's region 1 under `Corrected`; and inside IF97's 3.3e-5 band about the saturation line,
/// where v8.0.0 switches κ_T to a polynomial to dodge a throw (map 07 H11), it is still IF97's.
#[test]
fn the_condensed_phase_is_its_parts_state() {
    let (air, parts) = (humid_air(DataSet::Corrected), parts(DataSet::Corrected));
    let pt = |f: &Fluid, t: f64, p: f64| f.state(Input::new(Pair::PT, p, t, Basis::Mass).unwrap()).unwrap();
    let ice = pt(&parts.ice, 250.0, 1e6);
    let s = air.saturation(250.0, 1e6).unwrap();
    assert_eq!((s.kappa_t, s.v_condensed), (ice.isothermal_compressibility().unwrap(), 1.0 / ice.rho(Basis::Molar)));
    assert_eq!(s.beta_h, 0.0);
    let triple = air.saturation(273.16, 1e6).unwrap();
    assert_eq!(triple.kappa_t, pt(&parts.liquid, 273.16, 1e6).isothermal_compressibility().unwrap());
    assert!(triple.beta_h > 0.0);
    let just_below = air.saturation(273.159_99, 1e6).unwrap();
    assert_eq!(just_below.beta_h, 0.0, "ice below the triple point");
    let p_s = phasekit_iapws::if97::saturation_pressure(373.0).unwrap();
    for p in [p_s * (1.0 + 1e-5), p_s * (1.0 + 3e-5)] {
        let s = air.saturation(373.0, p).unwrap();
        assert_eq!(s.kappa_t, pt(&parts.liquid, 373.0, p).isothermal_compressibility().unwrap(), "{p}");
    }
}

/// The `humidair` kind's outputs of a moist-air state, by column, each with its §5 floor (VERIFICATION.md §5: R·T for
/// an energy, R for an entropy or a heat capacity, per kilogram of dry or humid air; 0 elsewhere).
fn moist_air_outputs(s: &MoistAir) -> [(&'static str, f64, f64); 18] {
    let (da, ha) = (Per::KgDryAir, Per::KgHumidAir);
    let r = |per| s.cp(per) / s.cp(Per::MolHumidAir) * phasekit_humidair::constants::R;
    let (rt_da, rt_ha) = (r(da) * s.t(), r(ha) * s.t());
    [
        ("W", s.humidity_ratio(), 0.0),
        ("Y", s.mole_fraction(), 0.0),
        ("P_w", s.partial_pressure(), 0.0),
        ("V", s.v(da), 0.0),
        ("Vha", s.v(ha), 0.0),
        ("H", s.h(da), rt_da),
        ("Hha", s.h(ha), rt_ha),
        ("U", s.u(da), rt_da),
        ("Uha", s.u(ha), rt_ha),
        ("S", s.s(da), r(da)),
        ("Sha", s.s(ha), r(ha)),
        ("C", s.cp(da), r(da)),
        ("Cha", s.cp(ha), r(ha)),
        ("CV", s.cv(da), r(da)),
        ("CVha", s.cv(ha), r(ha)),
        ("Z", s.z(), 0.0),
        ("speed_of_sound", s.speed_of_sound(), 0.0),
        ("isentropic_exponent", s.isentropic_exponent(), 0.0),
    ]
}

/// A `humidair` row's T, p and humidity.
fn humidair_inputs(table: &Fixture<'_>, row: usize) -> (f64, f64, Humidity) {
    let (t, p, value) =
        (table.value(row, "T").unwrap(), table.value(row, "p").unwrap(), table.value(row, "value").unwrap());
    match table.printed(row, "input") {
        Some("R") => (t, p, Humidity::Relative(value)),
        _ => (t, p, Humidity::Ratio(value)),
    }
}

/// Oracle: CoolProp 8.0.0's `HAPropsSI`, `fixtures/coolprop-8.0.0/all/humidair.csv` (PLAN.md M18.3), under `Parity`:
/// at each row's T, p and humidity, W, ψ_w and p_w within DIV-0011's bound (at a relative humidity they carry f's;
/// at a given W they hold to 1.4e-16); v, h, u, s, c_p, c_v, Z, w and the isentropic exponent within DIV-0069's, each
/// of max(|value|, floor) with §5's floors (v8.0.0 takes its virials at δ = 1e-12 and its heat capacities and w by
/// finite differences). The rows v8.0.0 refuses, a relative humidity where water's saturation pressure exceeds p, are
/// counted.
#[test]
fn moist_air_matches_the_oracle() {
    let (table, air) = (table(fixture!("coolprop-8.0.0/all/humidair.csv")), humid_air(DataSet::Parity));
    let (mut missed, mut asserted, mut refused) = (Vec::new(), 0, 0);
    for row in 0..table.rows().len() {
        if table.printed(row, "status") != Some("ok") {
            refused += 1;
            continue;
        }
        let (t, p, humidity) = humidair_inputs(&table, row);
        let state = air.mole_fraction(t, p, humidity).and_then(|psi| air.moist_air(t, p, psi)).unwrap();
        for (column, got, floor) in moist_air_outputs(&state) {
            let want = table.value(row, column).unwrap();
            let bound = match exempt_at(DIVERGENCES, "Humid air", Kind::Humidair, column, t) {
                Some(id) => registered(id) * want.abs().max(floor),
                None => ToleranceClass::Prop.bound(want.abs().max(floor)).unwrap(),
            };
            let error = (got - want).abs();
            if error.is_nan() || error > bound {
                missed.push(format!("{column} at {t} K, {p} Pa, {humidity:?}: {got} against {want}"));
            }
        }
        asserted += 1;
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!((asserted, refused), (276, 80));
}

/// Whether `got` is `want` within DIV-0069's bound, relative to max(|want|, `floor`).
fn close(got: f64, want: f64, floor: f64) -> bool {
    (got - want).abs() <= registered("DIV-0069") * want.abs().max(floor)
}

/// `target`'s value at (T, W) and p in phasekit, with its floor (§5's, as `moist_air_outputs` takes them).
fn target_at(air: &HumidAir, (t, p, w): (f64, f64, f64), target: Given) -> (f64, f64) {
    let state = air.moist_air(t, p, air.mole_fraction(t, p, Humidity::Ratio(w)).unwrap()).unwrap();
    let r = state.cp(Per::KgDryAir) / state.cp(Per::MolHumidAir) * phasekit_humidair::constants::R;
    match target {
        Given::Enthalpy(_) => (state.h(Per::KgDryAir), r * t),
        Given::Entropy(_) => (state.s(Per::KgDryAir), r),
        Given::Volume(_) => (state.v(Per::KgDryAir), 0.0),
        _ => (air.wet_bulb(&state).unwrap(), 0.0),
    }
}

/// The bound on a solve's unknown from the oracle's `target` at the row's state: DIV-0069's bound on the target,
/// carried through the target's slope in the unknown there (a central difference of phasekit's, 1e-6 of the unknown),
/// plus `Flash`'s on the unknown.
fn carried(air: &HumidAir, (t, p, w): (f64, f64, f64), target: Given, unknown_is_t: bool) -> f64 {
    let (x, floor) = target_at(air, (t, p, w), target);
    let d = if unknown_is_t { 1e-6 * t } else { 1e-6 * w.max(1e-6) };
    let at = |k: f64| {
        let (t, w) = if unknown_is_t { (t + k * d, w) } else { (t, w + k * d) };
        target_at(air, (t, p, w), target).0
    };
    let up = if unknown_is_t && t + d > 623.15 { 0.0 } else { 1.0 };
    let slope = (at(up) - at(up - 2.0)) / (2.0 * d);
    let unknown = if unknown_is_t { t } else { w.max(1e-6) };
    registered("DIV-0069") * x.abs().max(floor) / slope.abs() + ToleranceClass::Flash.bound(unknown).unwrap()
}

/// The row's state solved back from its own H, S, V and T_wb, each with its T and with its W: the misses, as text.
/// Dry air at its T, saturated air at either and air at 350 °C at its W sit at an end of the solve's range, where
/// v8.0.0's outputs, a part in 1e7 off phasekit's, may lie just outside it, and are left out.
fn from_outputs(air: &HumidAir, oracle: &Fixture<'_>, row: usize, state: &MoistAir) -> Vec<String> {
    let (t, p, w) = (state.t(), state.p(), state.humidity_ratio());
    let value = |column: &str| oracle.value(row, column).unwrap_or(f64::NAN);
    let saturated = oracle.printed(row, "input") == Some("R") && oracle.value(row, "value") == Some(1.0);
    let targets = [
        Given::Enthalpy(value("H")),
        Given::Entropy(value("S")),
        Given::Volume(value("V")),
        Given::WetBulb(value("Twb")),
    ];
    let mut missed = Vec::new();
    for target in targets.into_iter().filter(|g| !matches!(g, Given::WetBulb(x) if x.is_nan())) {
        let knowns = [
            (Given::Temperature(t), w > 0.0 && !saturated),
            (Given::Humidity(Humidity::Ratio(w)), !saturated && t < 623.15),
        ];
        for (known, unknown_is_t) in
            knowns.into_iter().filter(|(_, inside)| *inside).map(|(k, _)| (k, matches!(k, Given::Humidity(_))))
        {
            let got = air.state(p, known, target);
            let bound = carried(air, (t, p, w), target, unknown_is_t);
            let ok = got.as_ref().is_ok_and(|s| (s.t() - t).abs().max((s.humidity_ratio() - w).abs()) <= bound);
            if !ok {
                missed.push(format!("{known:?} with {target:?} at {t} K, {p} Pa, W {w} (bound {bound:e}): {got:?}"));
            }
        }
    }
    missed
}

/// Whether the register exempts the `humidair` kind's dew point of dry air, outside the dew point's domain.
fn exempt_dew_point() -> bool {
    DIVERGENCES.iter().any(|d| d.exempt.is_some_and(|e| e.kinds == [Kind::Humidair] && e.columns == ["Tdp"]))
}

/// Oracle: the `humidair` rows across the input pairs (PLAN.md M18.4), under `Parity`. At each answered row's state,
/// T_wb, T_dp and φ are v8.0.0's within DIV-0069's bound; the cells v8.0.0 refuses are counted (66 wet bulbs, 30 R
/// where water's saturation pressure exceeds p), and so are the 58 dry rows' T_dp, which phasekit refuses where v8.0.0's
/// is about 149.4 K (DIV-0072, ROT-272). From the row's own H, S, V and T_wb, each with its T and with its W, the state
/// is the row's within that bound carried through the target's slope in the unknown.
#[test]
fn the_oracle_across_input_pairs() {
    let (path, text) = fixture!("coolprop-8.0.0/all/humidair.csv");
    let oracle = Fixture::parse(path, text).unwrap();
    let air = humid_air(DataSet::Parity);
    let (mut missed, mut checked, mut refused, mut exempt) = (Vec::new(), 0, 0, 0);
    for row in (0..oracle.rows().len()).filter(|&r| oracle.printed(r, "status") == Some("ok")) {
        let (t, p, humidity) = humidair_inputs(&oracle, row);
        let psi = air.mole_fraction(t, p, humidity).unwrap();
        let state = air.moist_air(t, p, psi).unwrap();
        let forward =
            [("Twb", air.wet_bulb(&state)), ("Tdp", air.dew_point(p, psi)), ("R", air.relative_humidity(t, p, psi))];
        for (column, got) in forward {
            let want = oracle.value(row, column).unwrap_or(f64::NAN);
            let dry_air = column == "Tdp" && psi == 0.0;
            match got {
                _ if want.is_nan() => refused += 1,
                Err(Error::Domain(_)) if dry_air && exempt_dew_point() => exempt += 1,
                Ok(got) if !dry_air && close(got, want, 0.0) => checked += 1,
                got => missed.push(format!("{column} at {t} K, {p} Pa, {humidity:?}: {got:?} against {want}")),
            }
        }
        missed.extend(from_outputs(&air, &oracle, row, &state));
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!((checked, refused, exempt), (674, 96, 58));
}

/// Oracle: the `humidair` rows' viscosity and thermal conductivity (PLAN.md M18.5; provisional, user decision HA3),
/// under `Parity`: at a given W within `TransportDirect` (measured: 9.3e-15), at a given R within DIV-0011's bound,
/// ψ_w carrying f's difference (1.5e-8). Below water's triple-point pressure phasekit refuses them (DIV-0073), and
/// those cells are counted.
#[test]
fn transport_matches_the_oracle() {
    let (oracle, air) = (table(fixture!("coolprop-8.0.0/all/humidair.csv")), humid_air(DataSet::Parity));
    let (mut missed, mut checked, mut exempt) = (Vec::new(), 0, 0);
    for row in (0..oracle.rows().len()).filter(|&r| oracle.printed(r, "status") == Some("ok")) {
        let (t, p, humidity) = humidair_inputs(&oracle, row);
        let state = air.moist_air(t, p, air.mole_fraction(t, p, humidity).unwrap()).unwrap();
        for (column, got) in [("M", air.viscosity(&state)), ("K", air.conductivity(&state))] {
            let want = oracle.value(row, column).unwrap();
            let bound = match (humidity, exempt_at(DIVERGENCES, "Humid air", Kind::Humidair, column, t)) {
                (Humidity::Ratio(_), _) => ToleranceClass::TransportDirect.bound(want.abs()).unwrap(),
                (_, Some(id)) => registered(id) * want.abs(),
                _ => f64::NAN,
            };
            match got {
                Err(Error::Domain(_)) if p < 611.655 && transport_exempt() => exempt += 1,
                Ok(got) if (got - want).abs() <= bound => checked += 1,
                got => missed.push(format!("{column} at {t} K, {p} Pa, {humidity:?}: {got:?} against {want}")),
            }
        }
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!((checked, exempt), (456, 96));
}

/// Whether the register exempts the `humidair` kind's M and K below water's triple-point pressure.
fn transport_exempt() -> bool {
    DIVERGENCES.iter().any(|d| d.exempt.is_some_and(|e| e.kinds == [Kind::Humidair] && e.columns == ["M", "K"]))
}

/// The direct API's value of the `HAPropsSI` key `column` at `state`.
fn api_value(air: &HumidAir, state: &MoistAir, column: &str) -> Result<f64, Error> {
    let (t, p, psi) = (state.t(), state.p(), state.mole_fraction());
    let (da, ha) = (Per::KgDryAir, Per::KgHumidAir);
    Ok(match column {
        "T" => t,
        "P" => p,
        "W" => state.humidity_ratio(),
        "Y" => psi,
        "P_w" => state.partial_pressure(),
        "R" => air.relative_humidity(t, p, psi)?,
        "Tdp" => air.dew_point(p, psi)?,
        "Twb" => air.wet_bulb(state)?,
        "H" => state.h(da),
        "Hha" => state.h(ha),
        "U" => state.u(da),
        "Uha" => state.u(ha),
        "S" => state.s(da),
        "Sha" => state.s(ha),
        "V" => state.v(da),
        "Vha" => state.v(ha),
        "M" => air.viscosity(state)?,
        "K" => air.conductivity(state)?,
        "C" => state.cp(da),
        "Cha" => state.cp(ha),
        "CV" => state.cv(da),
        "CVha" => state.cv(ha),
        "Z" => state.z(),
        "speed_of_sound" => state.speed_of_sound(),
        _ => state.isentropic_exponent(),
    })
}

/// Oracle: the `humidair` rows through the strings (PLAN.md M18.6), under `Parity`: at every answered row, each of its
/// columns by `HAPropsSI` from the row's T, p and humidity is the direct API's, bit for bit, or both refuse; the direct
/// API's are held to v8.0.0's by the tests above.
#[test]
fn the_humidair_rows_through_the_strings() {
    let (oracle, air) = (table(fixture!("coolprop-8.0.0/all/humidair.csv")), humid_air(DataSet::Parity));
    let columns: Vec<&str> = oracle.columns().iter().skip(5).copied().collect();
    let (mut missed, mut checked) = (Vec::new(), 0);
    for row in (0..oracle.rows().len()).filter(|&r| oracle.printed(r, "status") == Some("ok")) {
        let (t, p, humidity) = humidair_inputs(&oracle, row);
        let state = air.moist_air(t, p, air.mole_fraction(t, p, humidity).unwrap()).unwrap();
        let (input, value) = (oracle.printed(row, "input").unwrap(), oracle.value(row, "value").unwrap());
        for &column in &columns {
            let via = phasekit_compat::ha_props_si_in(&air, column, "T", t, "P", p, input, value);
            let direct = api_value(&air, &state, column);
            let same = match (&via, &direct) {
                (Ok(a), Ok(b)) => a.to_bits() == b.to_bits(),
                (Err(_), Err(_)) => true,
                _ => false,
            };
            if same { checked += 1 } else { missed.push(format!("{column} at row {row}: {via:?} against {direct:?}")) }
        }
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!(checked, 276 * 23);
}

/// v8.0.0's names grouped by key, the first in each group the oracle's column (`Name2Type`, map 07 §2.3).
const ALIASES: [&[&str]; 25] = [
    &["W", "Omega", "HumRat"],
    &["Y", "psi_w"],
    &["Tdp", "T_dp", "DewPoint", "D"],
    &["Twb", "T_wb", "WetBulb", "B"],
    &["H", "Enthalpy", "Hda"],
    &["Hha"],
    &["U", "InternalEnergy", "Uda"],
    &["Uha"],
    &["S", "Entropy", "Sda"],
    &["Sha"],
    &["R", "RH", "RelHum"],
    &["T", "Tdb", "T_db"],
    &["P"],
    &["V", "Vda"],
    &["Vha"],
    &["M", "mu", "Visc"],
    &["K", "k", "Conductivity"],
    &["C", "cp"],
    &["Cha", "cp_ha"],
    &["CV"],
    &["CVha", "cv_ha"],
    &["P_w"],
    &["isentropic_exponent"],
    &["speed_of_sound"],
    &["Z"],
];

/// Every name `HAPropsSI` reads is one of [`ALIASES`]' (PLAN.md M18.6): as an output, each alias gives its key's value
/// bit for bit at 300 K, 101.325 kPa and φ = 0.5; as an input, with the pressure and T or W, each name `HumidAir::state`
/// takes gives the state back, every other name is refused as no input.
#[test]
fn every_name_and_alias_is_its_key() {
    use phasekit_compat::{CompatError, ha_names, ha_props_si_in};
    let air = humid_air(DataSet::Parity);
    let names: Vec<&str> = ha_names().collect();
    let grouped: Vec<&str> = ALIASES.iter().flat_map(|group| group.iter().copied()).collect();
    assert_eq!((names.len(), grouped.len()), (52, 52));
    assert!(names.iter().all(|name| grouped.contains(name)), "{names:?}");
    let at = |output: &str| ha_props_si_in(&air, output, "T", 300.0, "P", 101_325.0, "R", 0.5);
    let state = air.moist_air(300.0, 101_325.0, air.mole_fraction(300.0, 101_325.0, Humidity::Relative(0.5)).unwrap());
    let state = state.unwrap();
    let inputs = ["W", "Y", "Tdp", "Twb", "H", "S", "R", "T", "V"];
    for group in ALIASES {
        let want = at(group[0]).unwrap();
        for &name in group {
            assert_eq!(at(name).map(f64::to_bits), Ok(want.to_bits()), "{name}");
            let known = if group[0] == "T" { ("W", state.humidity_ratio()) } else { ("T", 300.0) };
            let back = ha_props_si_in(&air, "Y", name, want, "P", 101_325.0, known.0, known.1);
            if inputs.contains(&group[0]) {
                let psi = back.unwrap();
                assert!((psi - state.mole_fraction()).abs() <= 1e-9 * state.mole_fraction(), "{name}: {psi}");
            } else if name != "P" {
                assert_eq!(back, Err(CompatError::UnknownInput(name.into())), "{name}");
            }
        }
    }
}

/// `HAPropsSI`'s refusals (PLAN.md M18.6): no pressure, an unknown output or input, two inputs that fix no state, and
/// inputs and states out of v8.0.0's range (T from 130 K to 350 °C, W to 10, ψ_w to 0.94145, φ to 1); an output equal
/// to an input is computed, not echoed (DIV-0074), so φ = 5 is refused where v8.0.0 returns T as given. T's range takes
/// its ends, and the pressure may be any of the three inputs.
#[test]
fn ha_props_si_refuses() {
    use phasekit_compat::{CompatError, ha_props_si_in};
    let air = humid_air(DataSet::Parity);
    let call = |o: &str, (n1, v1): (&str, f64), (n2, v2): (&str, f64), (n3, v3): (&str, f64)| {
        ha_props_si_in(&air, o, n1, v1, n2, v2, n3, v3)
    };
    let (t, p, r) = (("T", 300.0), ("P", 101_325.0), ("R", 0.5));
    let invalid_state =
        |result: Result<f64, CompatError>| matches!(result, Err(CompatError::Core(Error::InvalidState { .. })));
    assert!(invalid_state(call("H", t, r, ("W", 0.01))), "no pressure");
    assert_eq!(call("Q", t, p, r), Err(CompatError::UnknownOutput("Q".into())));
    assert_eq!(call("H", ("Tc", 300.0), p, r), Err(CompatError::UnknownInput("Tc".into())));
    assert!(invalid_state(call("H", t, p, ("T_db", 310.0))), "two temperatures");
    assert!(invalid_state(call("H", ("W", 0.01), p, ("Y", 0.01))), "two water contents");
    let range = [
        (call("H", ("T", 129.0), p, r), "T"),
        (call("H", ("T", 624.0), p, r), "T"),
        (call("H", t, p, ("W", 10.5)), "W"),
        (call("H", t, p, ("Y", 0.95)), "Y"),
        (call("H", t, p, ("R", 1.5)), "R"),
        (call("T", t, p, ("R", 5.0)), "echo"),
        (call("H", t, p, ("Tdp", 120.0)), "Tdp"),
    ];
    for (result, case) in range {
        assert!(
            matches!(result, Err(CompatError::Core(Error::Domain(_) | Error::InvalidInput { .. }))),
            "{case}: {result:?}"
        );
    }
    assert_eq!(call("T", t, p, r), Ok(300.0));
    let (dry, h) = (("W", 0.0), call("H", t, p, r).map(f64::to_bits));
    assert!(call("H", ("T", 130.0), p, dry).is_ok() && call("H", ("T", 623.15), p, dry).is_ok(), "T's range ends");
    assert_eq!(
        (call("H", p, t, r).map(f64::to_bits), call("H", t, r, p).map(f64::to_bits)),
        (h.clone(), h),
        "P's place"
    );
}

/// `HAPropsSI` over vectors (PLAN.md M18.6): a length-1 operand broadcasts, each point is the scalar call's bit for bit,
/// a failed point follows the fill policy but where every point failed (the first failure), any other length mismatch
/// is `Shape`; and the embedded registry's
/// `ha_props_si` is `Corrected`'s.
#[test]
fn ha_props_si_over_vectors_and_the_embedded_registry() {
    use phasekit_compat::{CompatError, FillPolicy, ha_props_si, ha_props_si_in, ha_props_si_vec_in};
    let air = humid_air(DataSet::Parity);
    let t = [300.0, 100.0, 310.0];
    let call = |fill| ha_props_si_vec_in(&air, "H", ("T", &t), ("P", &[101_325.0]), ("R", &[0.5]), fill);
    let one = |t| ha_props_si_in(&air, "H", "T", t, "P", 101_325.0, "R", 0.5);
    let inf = call(FillPolicy::Inf).unwrap();
    assert_eq!(
        inf.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        [one(300.0).unwrap().to_bits(), f64::INFINITY.to_bits(), one(310.0).unwrap().to_bits()]
    );
    assert!(call(FillPolicy::Nan).unwrap()[1].is_nan());
    assert_eq!(call(FillPolicy::Error), Err(one(100.0).unwrap_err()));
    let none = |fill| ha_props_si_vec_in(&air, "H", ("T", &[100.0, 120.0]), ("P", &[101_325.0]), ("R", &[0.5]), fill);
    assert_eq!(
        (none(FillPolicy::Inf), none(FillPolicy::Nan)),
        (Err(one(100.0).unwrap_err()), Err(one(100.0).unwrap_err()))
    );
    let shape = ha_props_si_vec_in(&air, "H", ("T", &t), ("P", &[1e5, 2e5]), ("R", &[0.5]), FillPolicy::Inf);
    assert_eq!(shape, Err(CompatError::Core(Error::Shape { expected: 3, found: 2 })));
    let corrected = humid_air(DataSet::Corrected);
    assert_eq!(
        ha_props_si("H", "T", 300.0, "P", 1e5, "R", 0.0),
        ha_props_si_in(&corrected, "H", "T", 300.0, "P", 1e5, "R", 0.0)
    );
}

/// The browser package's two steps (PLAN.md M18.8): `ha_state_in`, then `ha_output_at` for every name, give
/// `HAPropsSI`'s value at the same inputs bit for bit, refusals alike.
#[test]
fn a_state_s_outputs_are_hapropssis() {
    use phasekit_compat::{ha_names, ha_output_at, ha_props_si_in, ha_state_in};
    let air = humid_air(DataSet::Parity);
    let state = ha_state_in(&air, 101_325.0, ("T", 300.0), ("R", 0.5)).unwrap();
    for name in ha_names() {
        let want = ha_props_si_in(&air, name, "T", 300.0, "P", 101_325.0, "R", 0.5);
        assert_eq!(ha_output_at(&air, &state, name).map(f64::to_bits), want.map(f64::to_bits), "{name}");
    }
}
