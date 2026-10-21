//! Humid air through `phasekit-humidair` (PLAN.md M18; user decisions HA1-HA6): its parts, its virials against IAPWS
//! G11-15 and G8-10, within their printed digits (`Paper`), the values converted to the tables' units.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Error, Fluid, Pair, Registry};
use phasekit_humidair::{Coefficient, HumidAir, Parts, Virials};
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
/// relative, all at 200 K (measured at M18.1): the register's bound.
fn c_aww_gap() -> f64 {
    match DIVERGENCES.iter().find(|d| d.id == "DIV-0067").and_then(|d| d.tolerance) {
        Some(Tolerance::Relative(bound)) => bound,
        other => panic!("DIV-0067 registers a relative bound: {other:?}"),
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
    let div_0011 = match DIVERGENCES.iter().find(|d| d.id == "DIV-0011").and_then(|d| d.tolerance) {
        Some(Tolerance::Relative(bound)) => bound,
        other => panic!("{other:?}"),
    };
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
