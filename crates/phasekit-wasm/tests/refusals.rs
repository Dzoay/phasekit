//! A refusal as data (PLAN.md M28.2; issue #191; user decisions EM1, EM2): a refused call throws a JS `Error` whose
//! message is core's long form in SI and which carries its code, quantity and numbers, and `refusal.explain(units,
//! style)` writes core's wording in the caller's units. The inline JS reaches each refusal as a page does.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#![allow(clippy::unwrap_used, reason = "test-crate helpers")]

use phasekit_wasm::Registry;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen(inline_js = r#"
const ENGINEERING = {
    temperature: { label: "°C", scale: 1, offset: 273.15 },
    pressure: { label: "bar", scale: 1e5, offset: 0 },
};
function thrown(call) {
    try {
        call();
    } catch (e) {
        return e;
    }
    throw new Error("the call was not refused");
}
const CALLS = {
    zeroKelvin: (r) => r.PropsSI("Hmass", "T", 0, "P", 1e5, "Water"),
    aboveTmax: (r) => r.PropsSI("Hmass", "T", 700, "Q", 0.5, "Water"),
    unknownOutput: (r) => r.PropsSI("Nonsense", "T", 300, "P", 1e5, "Water"),
    outputAtState: (r) => r.fluid("Water").flash("T", 300, "P", 1e5).get("SurfaceTension"),
    humidAir: (r) => r.humidAir(),
};
export function fields(registry, call) {
    const e = thrown(() => CALLS[call](registry));
    const values = Array.from(e.values).join(",");
    return [e instanceof Error, e.message, e.code, e.rule, e.quantity, e.dimension, values].join("|");
}
export function explained(registry, call, style) {
    return thrown(() => CALLS[call](registry)).refusal.explain(ENGINEERING, style);
}
export function inSI(registry, call, units) {
    return thrown(() => CALLS[call](registry)).refusal.explain(units, "short");
}
export function remedied(registry, call) {
    const e = thrown(() => CALLS[call](registry));
    return [Array.from(e.remedies).join(","), e.limit, Array.from(e.refusal.remedies).join(",")].join("|");
}
export function ofFluid(registry, call) {
    return thrown(() => CALLS[call](registry)).refusal.explain(ENGINEERING, "long", "Water");
}
export function nearest(registry, name) {
    return registry.nearest(name, 2).join(",");
}
export function detailed(registry) {
    const batch = registry.fluid("Water").evaluateDetailed(["Hmass"], "T", [300, -1, 700], "P", [1e5, 1e5, 2e9]);
    const numbers = Array.from(batch.numbers).map((v) => (Number.isNaN(v) ? "" : String(v))).join(",");
    return [batch.codes.join(","), batch.rules.join(","), Array.from(batch.limits).join(","), numbers].join("|");
}
export function noUnits() {
    return {};
}
export function halfUnits() {
    return { temperature: { label: "°C" } };
}
export function refusedExplain(registry, units, style) {
    const refusal = thrown(() => CALLS.zeroKelvin(registry)).refusal;
    return thrown(() => refusal.explain(units, style)).message;
}
"#)]
extern "C" {
    fn fields(registry: &JsValue, call: &str) -> String;
    fn explained(registry: &JsValue, call: &str, style: &str) -> String;
    #[wasm_bindgen(js_name = inSI)]
    fn in_si(registry: &JsValue, call: &str, units: JsValue) -> String;
    fn remedied(registry: &JsValue, call: &str) -> String;
    #[wasm_bindgen(js_name = ofFluid)]
    fn of_fluid(registry: &JsValue, call: &str) -> String;
    fn nearest(registry: &JsValue, name: &str) -> String;
    fn detailed(registry: &JsValue) -> String;
    #[wasm_bindgen(js_name = noUnits)]
    fn no_units() -> JsValue;
    #[wasm_bindgen(js_name = halfUnits)]
    fn half_units() -> JsValue;
    #[wasm_bindgen(js_name = refusedExplain)]
    fn refused_explain(registry: &JsValue, units: JsValue, style: &str) -> String;
}

/// A registry of the five embedded fluids, as a page holds it.
fn registry() -> JsValue {
    Registry::new().unwrap().into()
}

/// A refused call throws an `Error`: its message core's long form in SI, its fields the refusal's, its code and an
/// invalid input's rule core's (user decision EM11).
#[wasm_bindgen_test]
fn a_refusal_is_an_error_with_its_fields() {
    let r = registry();
    let zero = "true|temperature 0 K is not above absolute zero|invalid_input|not_above_absolute_zero|temperature|temperature|0";
    assert_eq!(fields(&r, "zeroKelvin"), zero);
    let above =
        "true|temperature 700 K is above the maximum, 647.096 K|above_max_temperature||temperature|temperature|";
    assert!(fields(&r, "aboveTmax").starts_with(above), "{}", fields(&r, "aboveTmax"));
    assert_eq!(fields(&r, "unknownOutput"), "true|unknown output key `Nonsense`|unknown_output||||");
    // Humid air's calls refuse alike: the package embeds no Air.
    let humid = "true|load error: Air is not embedded; enable `fluid-air`|load||||";
    assert_eq!(fields(&r, "humidAir"), humid);
}

/// `explain` writes core's wording in the caller's units, in either style.
#[wasm_bindgen_test]
fn explain_writes_the_callers_units() {
    let r = registry();
    assert_eq!(explained(&r, "zeroKelvin", "long"), "temperature −273.15 °C is not above absolute zero");
    assert_eq!(explained(&r, "zeroKelvin", "short"), "T = −273.15 °C ≤ −273.15 °C");
    assert_eq!(explained(&r, "aboveTmax", "short"), "T = 426.85 °C > T_max = 373.946 °C");
    assert_eq!(explained(&r, "unknownOutput", "short"), "unknown output key `Nonsense`");
}

/// A dimension the units lack, or no units at all, is written in SI; a refusal at a state carries the same fields.
#[wasm_bindgen_test]
fn an_absent_unit_is_si() {
    let r = registry();
    assert_eq!(in_si(&r, "zeroKelvin", JsValue::UNDEFINED), "T = 0 K ≤ 0 K");
    assert_eq!(in_si(&r, "zeroKelvin", no_units()), "T = 0 K ≤ 0 K");
    assert!(fields(&r, "outputAtState").starts_with("true|"), "{}", fields(&r, "outputAtState"));
}

/// A style other than "long" or "short", or a unit without its three fields, is refused by name.
#[wasm_bindgen_test]
fn explain_refuses_what_it_cannot_read() {
    let r = registry();
    let style = refused_explain(&r, JsValue::UNDEFINED, "tiny");
    assert_eq!(style, "`tiny` is not a style: \"long\" or \"short\"");
    let unit = refused_explain(&r, half_units(), "long");
    assert_eq!(unit, "units.temperature is not {label, scale, offset} with a finite, non-zero scale");
}

/// PLAN.md M29.6 (user decisions EM15, EM16): a refusal's remedies on the thrown `Error` and its `refusal`, and the end
/// of the range a domain limit is; compat's for an unknown key.
#[wasm_bindgen_test]
fn a_refusal_carries_its_remedies() {
    let r = registry();
    assert_eq!(remedied(&r, "aboveTmax"), "extrapolate|max|extrapolate");
    assert_eq!(remedied(&r, "unknownOutput"), "nearest_name||nearest_name");
    assert_eq!(remedied(&r, "zeroKelvin"), "||");
}

/// PLAN.md M29.5: `explain` writes a domain limit as the subject's the page names.
#[wasm_bindgen_test]
fn explain_names_whose_limit() {
    let r = registry();
    assert_eq!(of_fluid(&r, "aboveTmax"), "temperature 426.85 °C is above Water's maximum, 373.946 °C");
}

/// The registry's names nearest a typo.
#[wasm_bindgen_test]
fn the_names_nearest_a_typo() {
    assert_eq!(nearest(&registry(), "Watr"), "Water");
}

/// User decision EM17: `evaluateDetailed`'s refusals, a point each: its code, rule, limit and four numbers.
#[wasm_bindgen_test]
fn a_detailed_batch_keeps_each_refusal() {
    let want = ",invalid_input,above_max_pressure|,not_above_absolute_zero,|0,0,1|,,,,-1,,,,2000000000,1000000000,,";
    assert_eq!(detailed(&registry()), want);
}
