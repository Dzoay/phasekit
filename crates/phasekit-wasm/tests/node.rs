//! The browser package under Node (PLAN.md M10.8): the pure-fluid parts of CoolProp's `test_wasm.mjs` (map 11 §8),
//! the packs of E6, the batch's typed arrays (map 11 F23) and the handles' memory (map 11 §5g). wasm-bindgen-test runs
//! them through the package's JS bindings; the inline JS below calls the exported classes as a page does.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#![allow(clippy::unwrap_used, reason = "test-crate helpers")]

use std::any::Any;
use std::sync::Arc;

use phasekit_core::batch::{ExecPolicy, Status};
use phasekit_core::{DataSet, Pack};
use phasekit_mix::{EnvelopeOptions, Given, Mixture, Side};
use phasekit_verify::{Fixture, ToleranceClass};
use phasekit_wasm::{Registry, binary_pairs, mixture_components, output_keys, pair_keys, planes, predefined_mixtures};
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

/// PLAN.md M22.8 (user decision ML8): a mixture's batch is compat's `batch_si` on the same mixture rebuilt under
/// `ExecPolicy::Reference`, bit for bit, on both builds: each point's flash evaluates its liquid and vapour side by side,
/// in two lanes on the baseline (M22.5) and point by point on simd128. CO2/n-Propane at 40/60, two of the five fluids:
/// QT and PQ at Q = 0, 0.5 and 1 from 230 to 270 K and from 0.5 to 2.5 MPa, and PT and DT about the dome.
#[wasm_bindgen_test]
fn a_mixtures_batch_is_its_reference_paths() {
    let name = "CarbonDioxide[0.4]&n-Propane[0.6]";
    let page = Registry::new().unwrap().fluid(name).unwrap();
    let embedded = phasekit_core::Registry::from_embedded(DataSet::Corrected).unwrap();
    let native = phasekit_mix::with_mixtures(&embedded, DataSet::Corrected);
    let fluid = phasekit_compat::fluid_in(&native, name).unwrap();
    let scalar = Mixture::new(mixture(&fluid).model().clone(), mixture(&fluid).composition()).unwrap();
    let reference = phasekit_core::Fluid::new(Arc::new(scalar.with_exec(ExecPolicy::Reference)));
    let ts: Vec<f64> = (0..15).map(|i| 230.0 + 10.0 * f64::from(i / 3)).collect();
    let qs: Vec<f64> = (0..15).map(|i| f64::from(i % 3) / 2.0).collect();
    let ps: Vec<f64> = (0..15).map(|i| 5e5 + 5e5 * f64::from(i / 3)).collect();
    let ds: Vec<f64> = (0..15).map(|i| [50.0, 500.0, 2000.0, 8000.0, 12000.0][i as usize % 5]).collect();
    for (outputs, (n1, v1), (n2, v2)) in [
        (["Hmolar", "Dmolar", "P"], ("T", &ts), ("Q", &qs)),
        (["Hmolar", "Dmolar", "T"], ("P", &ps), ("Q", &qs)),
        (["Hmolar", "Dmolar", "Q"], ("P", &ps), ("T", &ts)),
        (["Hmolar", "P", "Q"], ("Dmolar", &ds), ("T", &ts)),
    ] {
        let batch = page.evaluate(outputs.map(String::from).to_vec(), n1, v1, n2, v2).unwrap();
        let (mut want, mut cells) = (vec![0.0; 3 * ts.len()], vec![Status::Ok; 3 * ts.len()]);
        phasekit_compat::batch_si(&reference, &outputs, (n1, v1), (n2, v2), &mut want, &mut cells).unwrap();
        let bits = |values: &[f64]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&batch.values()), bits(&want), "{n1}{n2}");
        let codes: Vec<u8> = cells.into_iter().map(phasekit_compat::status_code).collect();
        assert_eq!(batch.status(), codes, "{n1}{n2}");
        assert!(codes.iter().filter(|c| **c == 0).count() >= 30, "{n1}{n2}: {codes:?}");
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
const MIXTURES: (&str, &str) =
    ("facts/mixtures.csv", include_str!("../../phasekit-verify/fixtures/coolprop-8.0.0/facts/mixtures.csv"));
const CRIT: (&str, &str) = ("all/crit.csv", include_str!("../../phasekit-verify/fixtures/coolprop-8.0.0/all/crit.csv"));
const TRANSPORT: (&str, &str) =
    ("all/transport.csv", include_str!("../../phasekit-verify/fixtures/coolprop-8.0.0/all/transport.csv"));

/// R32's pack as `cargo xtask packs` writes it: its names and requirements from the index, its committed blob.
fn r32_pack() -> Vec<u8> {
    pack("R32", include_bytes!("../../phasekit-data/blobs/r32.bin"))
}

/// R125's pack, R410A's other component (PLAN.md M21.5).
fn r125_pack() -> Vec<u8> {
    pack("R125", include_bytes!("../../phasekit-data/blobs/r125.bin"))
}

/// The pack of fluid `name`, its blob `blob`.
fn pack(name: &str, blob: &[u8]) -> Vec<u8> {
    let fluid = phasekit_data::FLUIDS.iter().find(|f| f.name == name).unwrap();
    let names = core::iter::once(fluid.name).chain(fluid.aliases.iter().copied()).map(String::from).collect();
    let requires = fluid.requires.iter().copied().map(String::from).collect();
    phasekit_core::internal::pack(&[(names, requires, Arc::from(blob))])
}

/// R410A.mix as the page's registry reads it, with R32's and R125's packs; and the same mixture built here by
/// phasekit-mix over the same fluids, the package's answers' oracle (`Exact`).
fn r410a() -> (phasekit_wasm::Fluid, phasekit_core::Fluid) {
    let page = Registry::new().unwrap().with_pack(&r32_pack()).unwrap().with_pack(&r125_pack()).unwrap();
    let mut registry = phasekit_core::Registry::from_embedded(DataSet::Corrected).unwrap();
    for bytes in [r32_pack(), r125_pack()] {
        registry = registry.with_source(Box::new(Pack::new(bytes.into()).unwrap()), DataSet::Corrected).unwrap();
    }
    (page.fluid("R410A.mix").unwrap(), phasekit_mix::predefined(&registry, "R410A", DataSet::Corrected).unwrap())
}

fn mixture(fluid: &phasekit_core::Fluid) -> &Mixture {
    (fluid.model() as &dyn Any).downcast_ref().unwrap()
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

/// PLAN.md M13.14 (user decision MX5): the browser package reads CoolProp's mixture strings over its five fluids:
/// v8.0.0's density of CO₂/N₂ at 50/50, 300 K and 1 bar (fixtures/coolprop-8.0.0/facts/mixtures.csv) within `Flash`, by
/// `propsSI` and by a fluid handle, the same; `R410A.mix`, whose R32 and R125 the package does not embed, is refused.
#[wasm_bindgen_test]
fn mixture_strings_reach_the_browser() {
    let facts = Fixture::parse(MIXTURES.0, MIXTURES.1).unwrap();
    let row = (0..facts.rows().len()).find(|&r| facts.printed(r, "name") == Some("co2_n2_dmolar_t300_p1e5")).unwrap();
    let want = facts.value(row, "value").unwrap();
    let (registry, mix) = (Registry::new().unwrap(), "HEOS::CarbonDioxide[0.5]&Nitrogen[0.5]");
    let got = registry.props_si("Dmolar", "T", 300.0, "P", 1e5, mix).unwrap();
    assert!((got - want).abs() <= ToleranceClass::Flash.bound(want).unwrap(), "{got} against {want}");
    let state = registry.fluid(mix).unwrap().flash("T", 300.0, "P", 1e5).unwrap();
    assert_eq!(state.get("Dmolar").unwrap(), got);
    assert!(registry.props_si("Dmolar", "T", 300.0, "P", 1e5, "R410A.mix").is_err());
}

/// PLAN.md M21.5: what a page needs to offer mixtures. The predefined mixtures are the library's, each named as a fluid
/// string reads it; the binary pairs the library's, by CAS number; a mixture name's components as it writes them, a
/// predefined mixture's as the library lists them, none for a pure fluid's name, and a malformed or unknown name
/// refused as `fluid` refuses it.
#[wasm_bindgen_test]
fn the_mixture_lists_are_the_librarys() {
    let predefined: Vec<String> = phasekit_mix::predefined_names().map(|n| format!("{n}.mix")).collect();
    assert_eq!(predefined_mixtures(), predefined);
    assert!(predefined.contains(&"R410A.mix".to_string()));
    let pairs: Vec<String> = phasekit_mix::binary_pairs().map(|[a, b]| format!("{a}&{b}")).collect();
    assert_eq!(binary_pairs(), pairs);
    assert!(pairs.contains(&"354-33-6&75-10-5".to_string()), "R125 and R32 pair");
    assert_eq!(mixture_components("R410A.mix").unwrap(), ["R32", "R125"]);
    assert_eq!(mixture_components("Air.mix").unwrap(), ["NITROGEN", "ARGON", "OXYGEN"]);
    assert_eq!(mixture_components("HEOS::CarbonDioxide[0.5]&Nitrogen[0.5]").unwrap(), ["CarbonDioxide", "Nitrogen"]);
    assert_eq!(mixture_components("Water").unwrap(), Vec::<String>::new());
    let registry = Registry::new().unwrap();
    for bad in ["R999.mix", "R32[0.5]&R125"] {
        assert!(mixture_components(bad).is_err(), "{bad}");
        assert!(registry.fluid(bad).is_err(), "{bad}");
    }
}

/// PLAN.md M21.5: a mixture's fluid has its components, composition and phase envelope, each phasekit-mix's bit for bit
/// (`Exact`): R410A from R32's and R125's packs, its envelope closed through its critical point, the feed's mass density
/// its molar density times its molar mass, its incipient phase point-major. A pure fluid is one component at 1 and has
/// no envelope; CO₂/N₂'s trace, which starts at CO₂'s triple point, runs through its critical point and ends back at
/// that bound, and says so (#256).
#[wasm_bindgen_test]
fn a_mixture_has_its_components_composition_and_envelope() {
    let (page, oracle) = r410a();
    let model = mixture(&oracle);
    assert_eq!(page.components(), ["R32", "R125"]);
    assert_eq!(page.composition(), model.composition());
    let (got, want) = (page.envelope().unwrap(), model.envelope(&EnvelopeOptions::new()).unwrap());
    assert_eq!((got.t(), got.p(), got.dmolar()), (want.t.clone(), want.p.clone(), want.rho_bulk.clone()));
    assert_eq!(got.incipient_dmolar(), want.rho_incipient);
    let m = oracle.info().molar_mass();
    assert_eq!(got.dmass(), want.rho_bulk.iter().map(|rho| rho * m).collect::<Vec<f64>>());
    let q: Vec<f64> = want.side.iter().map(|s| if *s == Side::Bubble { 0.0 } else { 1.0 }).collect();
    assert_eq!(got.q(), q);
    assert!(q.contains(&0.0) && q.contains(&1.0), "both sides");
    let incipient: Vec<f64> = (0..want.len()).flat_map(|k| [want.incipient[0][k], want.incipient[1][k]]).collect();
    assert_eq!(got.incipient(), incipient);
    let critical = want.critical.unwrap();
    assert_eq!((got.critical_t(), got.critical_p()), (Some(critical.0), Some(critical.1)));
    assert_eq!((got.end(), got.reason()), ("closed".to_string(), String::new()));
    let water = Registry::new().unwrap().fluid("Water").unwrap();
    assert_eq!((water.components(), water.composition()), (vec!["Water".to_string()], vec![1.0]));
    assert!(water.envelope().is_err());
    let bound = Registry::new().unwrap().fluid("CarbonDioxide[0.5]&Nitrogen[0.5]").unwrap().envelope().unwrap();
    assert!(bound.t().len() > 10 && bound.critical_t().is_some() && bound.end() == "bound", "{}", bound.t().len());
    assert!(bound.reason().contains("BelowMinTemperature"), "{}", bound.reason());
}

/// PLAN.md M21.5: a mixture's two-phase state has each phase's mole fractions, phasekit-mix's `Mixture::phases` bit for
/// bit, the QT flash's own solve at a QT state; a single phase, and a pure fluid's two-phase state, have none.
#[wasm_bindgen_test]
fn a_mixtures_two_phase_state_has_its_phases() {
    let (page, oracle) = r410a();
    let want = mixture(&oracle).coexisting(0.5, Given::T(250.0)).unwrap();
    let phases = page.flash("Q", 0.5, "T", 250.0).unwrap().phases().unwrap().unwrap();
    assert_eq!((phases.liquid(), phases.vapour()), (want.x, want.y));
    assert!(page.flash("T", 300.0, "P", 1e5).unwrap().phases().unwrap().is_none());
    let water = Registry::new().unwrap().fluid("Water").unwrap();
    assert!(water.flash("Q", 0.5, "T", 373.0).unwrap().phases().unwrap().is_none());
}

/// The keys a plane quantity has in the package, as phasekit-diagram's `Quantity` names them.
fn key(quantity: phasekit_diagram::Quantity) -> &'static str {
    use phasekit_diagram::Quantity;
    match quantity {
        Quantity::T => "T",
        Quantity::P => "P",
        Quantity::Hmass => "Hmass",
        Quantity::Smass => "Smass",
        Quantity::Dmass => "Dmass",
        Quantity::Q => "Q",
        Quantity::Umass => "Umass",
    }
}

/// PLAN.md M19.6: `planes()` are phasekit-diagram's, in the site's order, each with its axes, which of them is
/// logarithmic, and its families' held and swept keys; `pairKeys()` names each input pair's keys in its order, D, H and
/// S per unit mass.
#[wasm_bindgen_test]
fn the_planes_and_the_pairs_keys_are_the_crates() {
    use phasekit_diagram::Plane;
    let got = planes();
    assert_eq!(got.iter().map(|p| p.id()).collect::<Vec<_>>(), ["ph", "ts", "pt", "trho", "hs"]);
    for (info, plane) in got.iter().zip(Plane::ALL) {
        let (x, y) = (plane.x(), plane.y());
        assert_eq!(
            (info.x_key(), info.x_log(), info.y_key(), info.y_log()),
            (key(x.quantity).into(), x.log, key(y.quantity).into(), y.log)
        );
        assert_eq!(info.held(), plane.families().iter().map(|f| key(f.held)).collect::<Vec<_>>());
        assert_eq!(info.sweep(), plane.families().iter().map(|f| key(f.sweep)).collect::<Vec<_>>());
    }
    assert_eq!((got[0].x_key(), got[0].y_key(), got[0].y_log()), ("Hmass".into(), "P".into(), true));
    let keys = pair_keys();
    assert_eq!(keys.len(), 2 * phasekit_core::Pair::ALL.len());
    let qt = phasekit_core::Pair::ALL.iter().position(|p| *p == phasekit_core::Pair::QT).unwrap();
    assert_eq!(keys[2 * qt..2 * qt + 2], ["Q", "T"]);
}

/// The vertices' x and y, inputs and pairs as the package lays them out (`Exact`).
fn laid_out<'a>(vertices: impl IntoIterator<Item = &'a phasekit_diagram::Vertex>) -> (Vec<u64>, Vec<u64>, Vec<u8>) {
    let all: Vec<_> = vertices.into_iter().collect();
    let code = |pair| phasekit_core::Pair::ALL.iter().position(|p| *p == pair).unwrap() as u8;
    let xy = all.iter().flat_map(|v| [v.x.to_bits(), v.y.to_bits()]).collect();
    let inputs = all.iter().flat_map(|v| [v.inputs.x.to_bits(), v.inputs.y.to_bits()]).collect();
    (xy, inputs, all.iter().map(|v| code(v.inputs.pair)).collect())
}

fn bits(values: Vec<f64>) -> Vec<u64> {
    values.into_iter().map(f64::to_bits).collect()
}

fn exported(v: &phasekit_wasm::Vertices) -> (Vec<u64>, Vec<u64>, Vec<u8>) {
    (bits(v.xy()), bits(v.inputs()), v.pairs())
}

/// Water from the embedded registry the package reads, the oracle's.
fn water() -> phasekit_core::Fluid {
    phasekit_core::Registry::from_embedded(DataSet::Corrected).unwrap().get("Water").unwrap().clone()
}

/// PLAN.md M19.6: a pure fluid's diagram is phasekit-diagram's, bit for bit, computed here on the same build by its
/// batch in lanes (`Exact`): Water on each plane, its axes' spans, the dome and every line's segments and vertices.
#[wasm_bindgen_test]
fn a_fluids_diagram_is_the_crates_bit_for_bit() {
    use phasekit_diagram::{FluidBatch, Limits, Plane, sample};
    let oracle = water();
    let batch = FluidBatch::new(&oracle).with_exec(ExecPolicy::Simd);
    let page = Registry::new().unwrap().fluid("Water").unwrap();
    for plane in Plane::ALL {
        let (got, want) =
            (page.diagram(plane.id()).unwrap(), sample(&batch, plane, &Limits::of(&oracle).unwrap()).unwrap());
        assert_eq!(
            (got.plane(), bits(got.x()), bits(got.y())),
            (plane.id().to_string(), bits(vec![want.x.0, want.x.1]), bits(vec![want.y.0, want.y.1]))
        );
        assert_eq!(exported(&got.liquid()), laid_out(&want.dome.liquid), "{plane:?}");
        assert_eq!(exported(&got.vapour()), laid_out(&want.dome.vapour), "{plane:?}");
        assert_eq!(exported(&got.critical()), laid_out(&want.dome.critical), "{plane:?}");
        let lines = got.lines();
        assert_eq!(lines.held(), want.lines.iter().map(|l| key(l.held)).collect::<Vec<_>>());
        assert_eq!(bits(lines.values()), bits(want.lines.iter().map(|l| l.value).collect()));
        let segments = want.lines.iter().flat_map(|l| &l.segments);
        assert_eq!(lines.two_phase(), segments.clone().map(|s| u8::from(s.two_phase)).collect::<Vec<_>>());
        assert_eq!(exported(&lines.vertices()), laid_out(segments.clone().flat_map(|s| &s.vertices)), "{plane:?}");
        let mut starts = vec![0];
        segments.for_each(|s| starts.push(starts.last().unwrap() + s.vertices.len() as u32));
        assert_eq!(lines.starts(), starts);
        let mut by_line = vec![0];
        want.lines.iter().for_each(|l| by_line.push(by_line.last().unwrap() + l.segments.len() as u32));
        assert_eq!(lines.segments(), by_line);
        assert_eq!((got.end(), got.reason()), (String::new(), String::new()));
    }
}

/// Checks that each vertex of `vertices` is `evaluate`'s `x` and `y` at its inputs, bit for bit, one call a pair, and
/// returns how many it checked.
fn evaluated(page: &phasekit_wasm::Fluid, vertices: &phasekit_wasm::Vertices, (x, y): (&str, &str)) -> usize {
    let (keys, xy, inputs, pairs) = (pair_keys(), vertices.xy(), vertices.inputs(), vertices.pairs());
    let mut checked = 0;
    for code in 0..phasekit_core::Pair::ALL.len() as u8 {
        let at: Vec<usize> = (0..pairs.len()).filter(|&i| pairs[i] == code).collect();
        let k = 2 * code as usize;
        let (a, b): (Vec<f64>, Vec<f64>) = at.iter().map(|&i| (inputs[2 * i], inputs[2 * i + 1])).unzip();
        let values = page.evaluate(vec![x.into(), y.into()], &keys[k], &a, &keys[k + 1], &b).unwrap().values();
        let got: Vec<u64> = values.iter().map(|v| v.to_bits()).collect();
        let want: Vec<u64> = at.iter().flat_map(|&i| [xy[2 * i].to_bits(), xy[2 * i + 1].to_bits()]).collect();
        assert_eq!(got, want, "pair {}{}", keys[k], keys[k + 1]);
        checked += at.len();
    }
    checked
}

/// PLAN.md M19.6, M19.2's check through the exports: every vertex of Water's p–h and h–s diagrams, the dome's and the
/// lines', is `evaluate`'s x and y at its inputs, bit for bit, the batch a page calls; and an unknown plane is refused.
#[wasm_bindgen_test]
fn every_vertex_is_evaluates_value_at_its_inputs() {
    let page = Registry::new().unwrap().fluid("Water").unwrap();
    for (id, axes) in [("ph", ("Hmass", "P")), ("hs", ("Smass", "Hmass"))] {
        let d = page.diagram(id).unwrap();
        let all = [d.liquid(), d.vapour(), d.critical(), d.lines().vertices()];
        let checked: usize = all.iter().map(|vertices| evaluated(&page, vertices, axes)).sum();
        assert!(checked > 1000, "{id}: {checked}");
    }
    assert!(page.diagram("xy").is_err());
}

/// PLAN.md M19.6, M19.4's check through the exports: R410A.mix's diagram on p–T and T–ρ is its envelope as
/// `Fluid.envelope()` gives it, bit for bit, the bubble side in the trace's order and the dew side reversed, with the
/// model's critical point (`Props1SI`) and how the trace ended; its other planes are refused, and so are its regions.
#[wasm_bindgen_test]
fn a_mixtures_diagram_is_its_envelope() {
    let (page, _) = r410a();
    let e = page.envelope().unwrap();
    let registry = Registry::new().unwrap().with_pack(&r32_pack()).unwrap().with_pack(&r125_pack()).unwrap();
    let constant = |k: &str| registry.props1_si(k, "R410A.mix").unwrap();
    for (id, x, y, critical) in [
        ("pt", e.t(), e.p(), [constant("T_critical"), constant("p_critical")]),
        ("trho", e.dmass(), e.t(), [constant("rhomass_critical"), constant("T_critical")]),
    ] {
        let d = page.diagram(id).unwrap();
        let (liquid, vapour) = (d.liquid().xy(), d.vapour().xy());
        let dew: Vec<f64> = vapour.chunks(2).rev().flat_map(|p| p.to_vec()).collect();
        let order: Vec<f64> = liquid.iter().copied().chain(dew).collect();
        let want: Vec<f64> = x.iter().zip(&y).flat_map(|(&a, &b)| [a, b]).collect();
        assert_eq!(bits(order), bits(want), "{id}");
        assert_eq!(bits(d.critical().xy()), bits(critical.to_vec()), "{id}");
        assert_eq!((d.lines().held().len(), d.end(), d.reason()), (0, e.end(), e.reason()));
    }
    assert!(page.diagram("ph").is_err());
    assert!(page.regions().is_err());
}

/// PLAN.md M19.6, M19.5's checks through the exports: Water's regions are phasekit-diagram's, bit for bit, their phases
/// the library's six as `PhaseSI` names them, and its critical isotherm and isobar run through the critical point.
#[wasm_bindgen_test]
fn a_fluids_regions_are_the_crates_bit_for_bit() {
    let oracle = water();
    let want = phasekit_diagram::regions(&oracle).unwrap();
    let got = Registry::new().unwrap().fluid("Water").unwrap().regions().unwrap();
    let mut phases = got.phases();
    assert_eq!(phases, want.regions.iter().map(|g| phasekit_compat::phase_name(g.phase)).collect::<Vec<_>>());
    phases.sort();
    assert_eq!(phases, ["gas", "liquid", "supercritical", "supercritical_gas", "supercritical_liquid", "twophase"]);
    let corners = |runs: Vec<&[phasekit_diagram::Corner]>| -> (Vec<u32>, Vec<u64>) {
        let mut starts = vec![0];
        runs.iter().for_each(|r| starts.push(starts.last().unwrap() + r.len() as u32));
        (
            starts,
            runs.iter().flat_map(|r| r.iter()).flat_map(|c| [c.t, c.p, c.h, c.s, c.rho]).map(f64::to_bits).collect(),
        )
    };
    let outlines = got.outlines();
    assert_eq!(
        (outlines.starts(), bits(outlines.corners())),
        corners(want.regions.iter().map(|g| g.outline.as_slice()).collect())
    );
    for (line, runs) in [(got.isotherm(), &want.isotherm), (got.isobar(), &want.isobar)] {
        assert_eq!((line.starts(), bits(line.corners())), corners(runs.iter().map(Vec::as_slice).collect()));
        let critical = got.critical();
        assert_eq!(critical.len(), 5);
        assert!(line.corners().chunks(5).any(|c| c == critical.as_slice()));
    }
    assert_eq!((got.t_critical(), got.p_critical()), (want.t_critical, want.p_critical));
}

#[wasm_bindgen(inline_js = r#"
export function diagramTypes(fluid) {
    const d = fluid.diagram("ph"), liquid = d.liquid, lines = d.lines, v = lines.vertices;
    const r = fluid.regions(), o = r.outlines;
    const typed = d.x instanceof Float64Array && liquid.xy instanceof Float64Array &&
        liquid.inputs instanceof Float64Array && liquid.pairs instanceof Uint8Array && liquid.length > 100 &&
        lines.values instanceof Float64Array && lines.segments instanceof Uint32Array &&
        lines.twoPhase instanceof Uint8Array && lines.starts instanceof Uint32Array && v.xy instanceof Float64Array &&
        Array.isArray(lines.held) && o.corners instanceof Float64Array && o.starts instanceof Uint32Array &&
        r.critical instanceof Float64Array && typeof r.Tc === "number" && Array.isArray(r.phases);
    for (const handle of [v, lines, liquid, d, o, r]) handle.free();
    return typed;
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = diagramTypes)]
    fn diagram_types(fluid: &JsValue) -> bool;
}

/// PLAN.md M19.6: a page reads a diagram and the regions as typed arrays, the inline JS calling the exported classes
/// as a page does: `Float64Array` values, `Uint8Array` pair codes and flags, `Uint32Array` offsets.
#[wasm_bindgen_test]
fn the_diagram_and_the_regions_are_typed_arrays() {
    let fluid = JsValue::from(Registry::new().unwrap().fluid("Water").unwrap());
    assert!(diagram_types(&fluid));
}
