//! The browser package's panics (PLAN.md M24.2; issue #219), in a test binary of its own: a panic leaves the module it
//! happens in unusable, so it must not share one with other tests. Built with `--features fault-injection`, which gives
//! `new Registry()` compat's fault-injection names.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#![allow(clippy::unwrap_used, reason = "test-crate helpers")]

use phasekit_compat::fault::FLASH;
use phasekit_wasm::{Registry, panicked};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen(inline_js = r#"
export function propsSI(registry, fluid) {
    try {
        return `ok ${registry.PropsSI("Hmass", "T", 300, "P", 1e5, fluid)}`;
    } catch (e) {
        return `${e.constructor.name}: ${e.message}`;
    }
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = propsSI)]
    fn props_si(registry: &JsValue, fluid: &str) -> String;
}

/// A panic where a call reaches a model (compat's `fault::flash`, whose flash panics) reaches JavaScript as an `Error`
/// that names it and says the module must be loaded anew, in place of `RuntimeError: unreachable`; `panicked()` then
/// returns its message, and every later call that computes refuses with it, the same call on Water and a new registry
/// included. Before the panic, `panicked()` is `undefined` and Water's `PropsSI` answers.
#[wasm_bindgen_test]
fn a_panic_reaches_javascript_and_retires_the_module() {
    let registry = JsValue::from(Registry::new().unwrap());
    assert_eq!(panicked(), None);
    assert!(props_si(&registry, "Water").starts_with("ok 1"), "{}", props_si(&registry, "Water"));
    let thrown = props_si(&registry, FLASH);
    assert!(thrown.starts_with("Error: phasekit panicked"), "{thrown}");
    assert!(thrown.contains("fault injection: fault::flash's flash") && thrown.contains("load it anew"), "{thrown}");
    let message = panicked().unwrap();
    assert!(message.contains("fault injection: fault::flash's flash"), "{message}");
    let refused = props_si(&registry, "Water");
    assert!(refused.starts_with("Error: phasekit panicked earlier") && refused.contains(&message), "{refused}");
    assert!(Registry::new().is_err(), "a new registry is refused too");
}
