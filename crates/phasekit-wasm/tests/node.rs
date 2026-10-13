//! The browser package under Node (PLAN.md M10.8): the pure-fluid parts of CoolProp's `test_wasm.mjs` (map 11 §8),
//! the packs of E6, the batch's typed arrays (map 11 F23) and the handles' memory (map 11 §5g). wasm-bindgen-test runs
//! them through the package's JS bindings; the inline JS below calls the exported classes as a page does.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#![allow(clippy::unwrap_used, reason = "test-crate helpers")]

use std::sync::Arc;

use phasekit_core::batch::Status;
use phasekit_verify::ToleranceClass;
use phasekit_wasm::Registry;
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
