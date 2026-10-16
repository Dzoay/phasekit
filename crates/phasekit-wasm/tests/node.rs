//! The browser package under Node (PLAN.md M10.8): the pure-fluid parts of CoolProp's `test_wasm.mjs` (map 11 §8),
//! the packs of E6, the batch's typed arrays (map 11 F23) and the handles' memory (map 11 §5g). wasm-bindgen-test runs
//! them through the package's JS bindings; the inline JS below calls the exported classes as a page does.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#![allow(clippy::unwrap_used, reason = "test-crate helpers")]

use std::sync::Arc;

use phasekit_core::batch::Status;
use phasekit_verify::{Fixture, ToleranceClass};
use phasekit_wasm::{Registry, output_keys};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen(inline_js = r#"
const T = new Float64Array([300, 500, -1]), P = new Float64Array([1e5, 1e6, 1e5]);
export function churn(registry, n) {
    for (let i = 0; i < n; i++) registry.fluid("Water").free();
}
export function batchTypes(fluid) {
    const batch = fluid.evaluate(["Hmass", "Q"], "T", T, "P", P);
    const typed = batch.values instanceof Float64Array && batch.status instanceof Uint8Array && batch.outputs === 2;
    batch.free();
    return typed;
}
export function batchValues(fluid) {
    const batch = fluid.evaluate(["Hmass", "Q"], "T", T, "P", P), values = batch.values;
    batch.free();
    return values;
}
export function batchStatus(fluid) {
    const batch = fluid.evaluate(["Hmass", "Q"], "T", T, "P", P), status = batch.status;
    batch.free();
    return status;
}
export function outputErrors(registry, keys) {
    const state = registry.fluid("Water").flash("T", 300, "P", 1e5), errors = [];
    for (const key of keys) {
        try {
            state.get(key);
        } catch (e) {
            errors.push(e.message);
        }
    }
    state.free();
    return errors;
}
export function props1Error(registry, key) {
    try {
        registry.Props1SI(key, "Water");
        return "no error";
    } catch (e) {
        return e.message;
    }
}
export function errorMessage(registry) {
    try {
        registry.PropsSI("X", "T", 300, "P", 1e5, "Water");
        return "no error";
    } catch (e) {
        return e instanceof Error ? e.message : "not an Error";
    }
}
"#)]
extern "C" {
    fn churn(registry: &JsValue, n: u32);
    #[wasm_bindgen(js_name = batchTypes)]
    fn batch_types(fluid: &JsValue) -> bool;
    #[wasm_bindgen(js_name = batchValues)]
    fn batch_values(fluid: &JsValue) -> Vec<f64>;
    #[wasm_bindgen(js_name = batchStatus)]
    fn batch_status(fluid: &JsValue) -> Vec<u8>;
    #[wasm_bindgen(js_name = outputErrors)]
    fn output_errors(registry: &JsValue, keys: Vec<String>) -> Vec<String>;
    #[wasm_bindgen(js_name = props1Error)]
    fn props1_error(registry: &JsValue, key: &str) -> String;
    #[wasm_bindgen(js_name = errorMessage)]
    fn error_message(registry: &JsValue) -> String;
}

/// `test_wasm.mjs:8-12`: Water's normal boiling point is in [373.124, 373.125] K; a bad key is a JS `Error`.
#[wasm_bindgen_test]
fn water_nbp_in_range() {
    let registry = Registry::new().unwrap();
    let t = registry.props_si("T", "P", 101_325.0, "Q", 0.0, "Water").unwrap();
    assert!((373.124..=373.125).contains(&t), "{t}");
    let state = registry.fluid("Water").unwrap().flash("P", 101_325.0, "Q", 0.0).unwrap();
    assert_eq!((state.t().to_bits(), state.phase().as_str()), (t.to_bits(), "twophase"));
    assert_eq!(error_message(&JsValue::from(registry)), "unknown output key `X`");
}

/// `test_wasm.mjs:176-185`: (∂h/∂T)_p is c_p, within the `Identity` class, at Water's (1 MPa, 300 K); the typed
/// flash reads the same c_p.
#[wasm_bindgen_test]
fn dh_dt_at_constant_p_equals_cp() {
    let registry = Registry::new().unwrap();
    let dh_dt = registry.props_si("d(Hmolar)/d(T)|P", "P", 1e6, "T", 300.0, "Water").unwrap();
    let cp = registry.props_si("Cpmolar", "P", 1e6, "T", 300.0, "Water").unwrap();
    let bound = ToleranceClass::Identity.bound(dh_dt.abs().max(cp.abs())).unwrap();
    assert!((dh_dt - cp).abs() <= bound, "{dh_dt} against {cp}, bound {bound}");
    let state = registry.fluid("Water").unwrap().flash("T", 300.0, "P", 1e6).unwrap();
    assert_eq!(state.get("Cpmolar").unwrap().to_bits(), cp.to_bits());
}

/// `test_wasm.mjs:188-195`: dp/dT along the saturation curve is positive at Water's normal boiling point, here by
/// Clausius-Clapeyron, (s″ − s′)/(v″ − v′), from the two saturated states.
#[wasm_bindgen_test]
fn dp_dt_sat_is_positive() {
    let registry = Registry::new().unwrap();
    let at = |output: &str, q: f64| registry.props_si(output, "P", 101_325.0, "Q", q, "Water").unwrap();
    let ds = at("Smolar", 1.0) - at("Smolar", 0.0);
    let dv = 1.0 / at("Dmolar", 1.0) - 1.0 / at("Dmolar", 0.0);
    assert!((ds / dv).is_finite() && ds / dv > 0.0, "{ds} / {dv}");
}

/// E6: a page adds a fetched pack over the embedded fluids with `withPack`. A name the registry misses resolves in the
/// new one, by its alias too, so the miss was not cached in the embedded layer they share; the first registry is
/// unchanged, and a corrupt pack is refused.
#[wasm_bindgen_test]
fn with_pack_adds_a_fluid_and_misses_are_not_cached() {
    let blob: Arc<[u8]> = phasekit_core::internal::FluidRecord::synthetic("PackA").unwrap().encode().into();
    let bytes = phasekit_core::internal::pack(&[(vec!["PackA".to_string(), "pa-1".to_string()], vec![], blob)]);
    let registry = Registry::new().unwrap();
    assert!(registry.fluid("pa-1").is_err(), "not embedded");
    let page = registry.with_pack(&bytes).unwrap();
    assert_eq!(page.fluid("pa-1").unwrap().name(), "PackA");
    assert!(registry.fluid("PackA").is_err(), "the first registry is unchanged");
    assert_eq!(page.fluid("Water").unwrap().name(), "Water", "the embedded fluids stay");
    let mut corrupt = bytes;
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(registry.with_pack(&corrupt).is_err());
}

/// Map 11 F23: the batch takes and gives `Float64Array`s, point-major (N × M), with a `Uint8Array` of cell codes; its
/// cells are compat's `batch_si`'s, bit for bit, the T < 0 point failing.
#[wasm_bindgen_test]
fn batch_round_trips_float64array() {
    let fluid = JsValue::from(Registry::new().unwrap().fluid("Water").unwrap());
    assert!(batch_types(&fluid));
    let native = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
    let (mut want, mut cells) = ([0.0; 6], [Status::Ok; 6]);
    let (t, p) = ([300.0, 500.0, -1.0], [1e5, 1e6, 1e5]);
    let water = native.get("Water").unwrap();
    phasekit_compat::batch_si(water, &["Hmass", "Q"], ("T", &t), ("P", &p), &mut want, &mut cells).unwrap();
    let values: Vec<u64> = batch_values(&fluid).iter().map(|v| v.to_bits()).collect();
    assert_eq!(values, want.map(f64::to_bits));
    let status = batch_status(&fluid);
    assert_eq!(status, cells.map(phasekit_compat::status_code));
    assert!(status[0] == 0 && status[4] == 2 && status[5] == 2, "{status:?}");
}

/// M12.10 (user decision SL8): the batch runs `ExecPolicy::Simd`, eight lanes to a group on the simd128 build (with
/// the package's `simd` feature) and two on the baseline, and its cells are compat's `batch_si`'s on the `Reference` path, bit for
/// bit. n-Propane's and Water's DT, QT and PQ batches of 75 points: whole groups and a short last one, single-phase,
/// two-phase and saturated points side by side, and points that fail (above T_c, NaN, p ≤ 0).
#[wasm_bindgen_test]
fn the_batch_in_lanes_is_the_reference_paths() {
    let registry = Registry::new().unwrap();
    let native = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
    let n = 75;
    let q: Vec<f64> = (0..n).map(|i| [0.0, 1.0, f64::from(i) / 75.0][i as usize % 3]).collect();
    for (name, t_low, t_c, p_c) in [("n-Propane", 90.0, 369.89, 4.2512e6), ("Water", 275.0, 647.096, 2.2064e7)] {
        let t = |i: i32| t_low + (t_c - t_low) * f64::from(i) / 72.0;
        let mut ts: Vec<f64> = (0..n).map(t).collect();
        ts[n as usize - 1] = f64::NAN;
        let ds: Vec<f64> = core::iter::successors(Some(0.5), |d| Some(d * 1.1)).take(ts.len()).collect();
        let ps: Vec<f64> = (0..n).map(|i| p_c * 1.2 * f64::from(i - 1) / 72.0).collect();
        let fluid = registry.fluid(name).unwrap();
        for (outputs, (n1, v1), (n2, v2)) in [
            (["Hmass", "Cpmass", "Q"], ("T", &ts), ("Dmass", &ds)),
            (["Hmass", "Dmass", "P"], ("T", &ts), ("Q", &q)),
            (["Hmass", "Dmass", "T"], ("P", &ps), ("Q", &q)),
        ] {
            let batch = fluid.evaluate(outputs.map(String::from).to_vec(), n1, v1, n2, v2).unwrap();
            let (mut want, mut cells) = (vec![0.0; 3 * ts.len()], vec![Status::Ok; 3 * ts.len()]);
            let reference = native.get(name).unwrap();
            phasekit_compat::batch_si(reference, &outputs, (n1, v1), (n2, v2), &mut want, &mut cells).unwrap();
            let bits = |values: &[f64]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&batch.values()), bits(&want), "{name} {n1}{n2}");
            let codes: Vec<u8> = cells.into_iter().map(phasekit_compat::status_code).collect();
            assert_eq!(batch.status(), codes, "{name} {n1}{n2}");
            assert!(codes.contains(&0) && codes.iter().any(|c| *c > 1), "{name} {n1}{n2}: {codes:?}");
        }
    }
}

/// Map 11 §5g: a fluid handle made and freed 10⁵ times through the JS bindings leaks nothing: wasm memory stops
/// growing after the first 10³ *(inference)*.
#[wasm_bindgen_test]
fn handles_do_not_leak_over_1e5_calls() {
    let registry = JsValue::from(Registry::new().unwrap());
    churn(&registry, 1_000);
    let warm = core::arch::wasm32::memory_size::<0>();
    churn(&registry, 99_000);
    assert_eq!(core::arch::wasm32::memory_size::<0>(), warm, "pages after 10^3 and after 10^5 handles");
}

/// M21.1: `outputKeys()` names CoolProp's 85 outputs in its order, as compat's table holds them, and `State.get` knows
/// each one: at a state of Water, a key may be refused for the state or the fluid, but never as an unknown key.
#[wasm_bindgen_test]
fn output_keys_are_coolprops_85() {
    let keys = output_keys();
    let want: Vec<&str> = phasekit_compat::output_names().take(phasekit_compat::OUTPUT_COUNT).collect();
    assert_eq!(keys, want);
    assert_eq!(
        (keys.len(), keys.first().map(String::as_str), keys.last().map(String::as_str)),
        (85, Some("T"), Some("Phase"))
    );
    let registry = JsValue::from(Registry::new().unwrap());
    let unknown: Vec<String> = output_errors(&registry, keys).into_iter().filter(|e| e.contains("unknown")).collect();
    assert!(unknown.is_empty(), "{unknown:?}");
}

/// M21.2: `Props1SI` answers a state-independent output as compat does, bit for bit, and refuses one that depends on
/// the state, so the site can read a fluid's limits before it has a state.
#[wasm_bindgen_test]
fn props1si_answers_constants_as_compat_does() {
    let registry = Registry::new().unwrap();
    let native = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
    for key in ["T_critical", "p_critical", "rhomass_critical", "T_triple", "p_triple", "T_min", "T_max", "P_max"] {
        for fluid in ["Water", "n-Propane", "R134a"] {
            let want = phasekit_compat::props1_si_in(&native, key, fluid).unwrap();
            assert_eq!(registry.props1_si(key, fluid).map(f64::to_bits).ok(), Some(want.to_bits()), "{key} {fluid}");
        }
    }
    let refusal = props1_error(&JsValue::from(registry), "T");
    assert_eq!(refusal, phasekit_compat::props1_si_in(&native, "T", "Water").unwrap_err().to_string());
    assert!(refusal.contains("depends on the state"), "{refusal}");
}

/// The oracle's all-fluid tier, read as the package's tests run: compiled in (wasm32-unknown-unknown has no files).
const CRIT: (&str, &str) = ("all/crit.csv", include_str!("../../phasekit-verify/fixtures/coolprop-8.0.0/all/crit.csv"));
const TRANSPORT: (&str, &str) =
    ("all/transport.csv", include_str!("../../phasekit-verify/fixtures/coolprop-8.0.0/all/transport.csv"));

/// R32's pack as `cargo xtask packs` writes it: its names and requirements from the index, its committed blob.
fn r32_pack() -> Vec<u8> {
    let r32 = phasekit_data::FLUIDS.iter().find(|f| f.name == "R32").unwrap();
    let names = core::iter::once(r32.name).chain(r32.aliases.iter().copied()).map(String::from).collect();
    let requires = r32.requires.iter().copied().map(String::from).collect();
    let blob: Arc<[u8]> = Arc::from(&include_bytes!("../../phasekit-data/blobs/r32.bin")[..]);
    phasekit_core::internal::pack(&[(names, requires, blob)])
}

/// PLAN.md M21.4: the registry names the five fluids the package embeds, and R32 once its pack is added, the pack's
/// first. It has decoded none until one is used: R32's enthalpy decodes R32 alone, and its transport the reference
/// fluid it reads too (its conductivity is ECS on n-Propane, map 05 §5), listed in layer order. The first registry
/// shares its embedded layer with the new one (E6), so it has n-Propane decoded too, and never R32.
#[wasm_bindgen_test]
fn names_and_loaded_follow_the_packs() {
    let registry = Registry::new().unwrap();
    assert_eq!(registry.names(), ["CarbonDioxide", "Nitrogen", "R134a", "Water", "n-Propane"]);
    assert!(registry.fluid("R32").is_err(), "the package embeds five fluids, R32 not among them");
    let page = registry.with_pack(&r32_pack()).unwrap();
    assert_eq!(page.names(), ["R32", "CarbonDioxide", "Nitrogen", "R134a", "Water", "n-Propane"]);
    assert_eq!(page.loaded(), Vec::<String>::new());
    page.props_si("Hmolar", "T", 300.0, "Dmolar", 100.0, "R32").unwrap();
    assert_eq!(page.loaded(), ["R32"]);
    page.props_si("L", "T", 300.0, "Dmolar", 100.0, "R32").unwrap();
    assert_eq!(page.loaded(), ["n-Propane", "R32"]);
    assert_eq!(registry.loaded(), ["n-Propane"]);
}

/// PLAN.md M21.4: R32 from its pack, over the five embedded fluids, gives the oracle's values in their classes, as the
/// library's own tests check them. Through `Props1SI`: its triple point, limits, molar mass and gas constant (`Exact`),
/// and its critical point, the model's as CoolProp computes it with superancillaries on (`Flash`). Through `PropsSI`, at
/// the all-fluid tier's eight states: its viscosity (ρ*_sr-CS, `TransportDirect`) and conductivity (ECS on the embedded
/// n-Propane, `TransportEcs`).
#[wasm_bindgen_test]
fn r32_from_its_pack_gives_the_fixture_values() {
    let page = Registry::new().unwrap().with_pack(&r32_pack()).unwrap();
    let crit = Fixture::parse(CRIT.0, CRIT.1).unwrap();
    let row = (0..crit.rows().len()).find(|&r| crit.printed(r, "fluid") == Some("R32")).unwrap();
    let mut failures = Vec::new();
    for (column, key) in [("Tc_num", "T_critical"), ("pc_num", "p_critical"), ("rhoc_num", "rhomolar_critical")] {
        let (got, want) = (page.props1_si(key, "R32").unwrap(), crit.value(row, column).unwrap());
        failures.extend(crit.check_scaled(row, column, got, want.abs()).err().map(|e| format!("{key}: {e:?}")));
    }
    let keys = [
        ("Ttriple", "T_triple"),
        ("ptriple", "p_triple"),
        ("Tmin", "T_min"),
        ("Tmax", "T_max"),
        ("pmax", "P_max"),
        ("M", "molar_mass"),
        ("R", "gas_constant"),
    ];
    for (column, key) in keys {
        let got = page.props1_si(key, "R32").unwrap();
        failures.extend(crit.check(row, column, got).err().map(|e| format!("{key}: {e:?}")));
    }
    let transport = Fixture::parse(TRANSPORT.0, TRANSPORT.1).unwrap();
    let mut rows = 0;
    for row in (0..transport.rows().len()).filter(|&r| transport.printed(r, "fluid") == Some("R32")) {
        let (t, rho) = (transport.value(row, "T").unwrap(), transport.value(row, "rho").unwrap());
        for (column, key, class) in
            [("eta", "V", ToleranceClass::TransportDirect), ("lambda", "L", ToleranceClass::TransportEcs)]
        {
            assert_eq!(transport.printed(row, &format!("{column}_status")), Some("ok"));
            let got = page.props_si(key, "T", t, "Dmolar", rho, "R32").unwrap();
            let bound = class.bound(transport.value(row, column).unwrap()).unwrap();
            failures.extend(transport.check_bound(row, column, got, bound).err().map(|e| format!("{key}: {e:?}")));
        }
        rows += 1;
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(rows, 8);
}
