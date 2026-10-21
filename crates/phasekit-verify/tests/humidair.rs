//! Humid air through `phasekit-humidair` (PLAN.md M18; user decisions HA1-HA6): its parts, its virials against IAPWS
//! G11-15 and G8-10, within their printed digits (`Paper`), the values converted to the tables' units.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::math;
use phasekit_core::{Basis, DataSet, Error, Fluid, Input, Pair, Registry};
use phasekit_humidair::{Coefficient, Gas, HumidAir, Parts, Virials, henry_constant};
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

/// DIV-0011's registered bound, relative: v8.0.0's B from its EOS at δ = 1e-12, and what inherits it; NaN, which fails
/// every comparison, without one.
fn div_0011() -> f64 {
    match DIVERGENCES.iter().find(|d| d.id == "DIV-0011").and_then(|d| d.tolerance) {
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
    let div_0011 = div_0011();
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
                Some(_) => div_0011() * want.abs(),
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
