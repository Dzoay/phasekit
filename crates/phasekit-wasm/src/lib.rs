//! `phasekit-wasm`: the browser package (PLAN.md M10.8; D11; E6, E7; map 11 U12, F23). wasm-bindgen over
//! `phasekit-compat`, with the five core fluids embedded (E7) and more from packs a page fetches (`withPack`, E6).
//!
//! - [`Registry`]: `new Registry()`, `withPack(bytes)`, `withReference(fluid, state)`, `PropsSI(...)`, `Props1SI(output,
//!   fluid)`, `fluid(name)`, and `names()` and `loaded()`, what it serves and what it has decoded.
//! - [`Fluid`]: `flash(name1, value1, name2, value2)`, the typed flash, and `evaluate(outputs, name1, values1, name2,
//!   values2)`, the one batch: point-major `Float64Array` values (N × M) and a `Uint8Array` of cell codes.
//! - [`output_keys`]: `outputKeys()`, CoolProp's 85 output keys, which `PropsSI` and `State.get` take.
//! - Errors are JS `Error`s. JS's garbage collector never frees wasm memory, so `Registry` and `Fluid`, the handle
//!   objects, are freed with `free()`; the `State` and `Batch` a call returns are values, freed the same way.
//! - A panic, a bug, cannot be caught on wasm32 and leaves the module unusable. It reaches JavaScript as an `Error` that
//!   names it, [`panicked`] then returns its message, and every later call that computes refuses with it: load the
//!   module anew (PLAN.md M24.2; issue #219).
//!
//! Two builds: the baseline, and `simd128` (`-C target-feature=+simd128`) with this crate's `simd` feature, which enables
//! core's, so the batch runs 8 states to a lane group (PLAN.md M12.10). Only the browser target has JavaScript;
//! elsewhere the crate is empty.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

use core::fmt::Display;
use std::sync::OnceLock;

use phasekit_compat as compat;
use phasekit_core::batch::{ExecPolicy, Status};
use phasekit_core::{DataSet, FlashOptions, Pack};
use wasm_bindgen::prelude::*;

/// A JS `Error` with `e`'s message.
fn js(e: impl Display) -> JsError {
    JsError::new(&e.to_string())
}

/// What a panic's `Error` and every refusal after it end with.
const RETIRED: &str = "This module cannot be used again: load it anew";

/// The first panic's message. wasm32 cannot unwind, so a panic leaves the call it happens in, and the module's state,
/// where it stopped; the panic hook keeps its message here, and every later call that computes refuses with it.
static PANICKED: OnceLock<String> = OnceLock::new();

/// Set once the panic hook is installed.
static HOOK: OnceLock<()> = OnceLock::new();

/// The message of the panic that retired this module, or `undefined` before one: a page that sees it loads the module
/// anew.
#[wasm_bindgen]
pub fn panicked() -> Option<String> {
    PANICKED.get().cloned()
}

/// Every call that computes starts here: the panic hook is installed on the first, and once the module has panicked
/// each refuses with the panic's message. The hook runs the one before it (wasm-bindgen-test's, a page's), keeps the
/// message, and throws it to JavaScript as an `Error`, which ends the call there, as the trap would have.
fn alive() -> Result<(), JsError> {
    HOOK.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            previous(info);
            let message = PANICKED.get_or_init(|| info.to_string());
            wasm_bindgen::throw_str(&format!("phasekit panicked: {message}. {RETIRED}"));
        }));
    });
    match PANICKED.get() {
        Some(message) => Err(js(format!("phasekit panicked earlier: {message}. {RETIRED}"))),
        None => Ok(()),
    }
}

/// CoolProp's 85 output keys in its order (map 01 §4a), without their aliases: what `PropsSI` and `State.get` take as
/// an output, besides the aliases and the derivative grammar. The site lists them (PLAN.md M21.1).
#[wasm_bindgen(js_name = outputKeys)]
pub fn output_keys() -> Vec<String> {
    compat::output_names().take(compat::OUTPUT_COUNT).map(String::from).collect()
}

/// A registry: the five core fluids, any fetched packs, any re-gauged fluids. A handle object: free it with `free()`.
#[wasm_bindgen]
pub struct Registry {
    inner: phasekit_core::Registry,
}

#[wasm_bindgen]
impl Registry {
    /// The embedded fluids, CoolProp's data with the published corrections (user decision 2), and CoolProp's mixture
    /// names over them and the packs added later (`HEOS::CarbonDioxide[0.5]&Nitrogen[0.5]`, `R410A.mix`; PLAN.md M13.14).
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Registry, JsError> {
        alive()?;
        let embedded = phasekit_core::Registry::from_embedded(DataSet::Corrected).map_err(js)?;
        let inner = phasekit_mix::with_mixtures(&embedded, DataSet::Corrected);
        // The panic test's build (PLAN.md M24.2): names that panic on purpose where a call reaches a model.
        #[cfg(feature = "fault-injection")]
        let inner = compat::fault::with_faults(&inner);
        Ok(Registry { inner })
    }

    /// A new registry that also serves the fluids of a pack (E6): its bytes as fetched. This one is unchanged.
    #[wasm_bindgen(js_name = withPack)]
    pub fn with_pack(&self, bytes: &[u8]) -> Result<Registry, JsError> {
        alive()?;
        let pack = Pack::new(bytes.into()).map_err(js)?;
        Ok(Registry { inner: self.inner.with_source(Box::new(pack), DataSet::Corrected).map_err(js)? })
    }

    /// A new registry in which the fluid `fluid` names reports in the reference state named `reference`: CoolProp's
    /// `IIR`, `ASHRAE`, `NBP`, or `DEF` (and `RESET`) for the model's own (E10). This one is unchanged.
    #[wasm_bindgen(js_name = withReference)]
    pub fn with_reference(&self, fluid: &str, reference: &str) -> Result<Registry, JsError> {
        alive()?;
        let state = compat::parse_reference_state(reference)
            .ok_or_else(|| js(format!("`{reference}` is not IIR, ASHRAE, NBP, DEF or RESET")))?;
        let name = compat::fluid_in(&self.inner, fluid).map_err(js)?.info().name().to_string();
        Ok(Registry { inner: self.inner.with_reference(&name, state).map_err(js)? })
    }

    /// CoolProp's `PropsSI(output, name1, value1, name2, value2, fluid)`: the compat strings' keys and checks.
    #[wasm_bindgen(js_name = PropsSI)]
    pub fn props_si(
        &self,
        output: &str,
        name1: &str,
        value1: f64,
        name2: &str,
        value2: f64,
        fluid: &str,
    ) -> Result<f64, JsError> {
        alive()?;
        compat::props_si_in(&self.inner, output, name1, value1, name2, value2, fluid).map_err(js)
    }

    /// CoolProp's `Props1SI(output, fluid)`: a state-independent output (T_critical, p_triple, …); a state-dependent one
    /// is refused. The site reads a fluid's limits with it (PLAN.md M21.2).
    #[wasm_bindgen(js_name = Props1SI)]
    pub fn props1_si(&self, output: &str, fluid: &str) -> Result<f64, JsError> {
        alive()?;
        compat::props1_si_in(&self.inner, output, fluid).map_err(js)
    }

    /// The canonical name of every fluid it serves, decoded or not, each once, the newest pack's first and the embedded
    /// five last (core's `Registry::names`): a page fetches a fluid's pack only when this lacks it (PLAN.md M21.4).
    pub fn names(&self) -> Vec<String> {
        self.inner.names().map(String::from).collect()
    }

    /// The canonical names of the fluids it has decoded so far, the embedded ones first, then each pack's in the order
    /// they were added (core's `Registry::loaded`): a fluid is decoded on first use, its references with it.
    pub fn loaded(&self) -> Vec<String> {
        self.inner.loaded().map(String::from).collect()
    }

    /// The fluid a fluid string names (a name, alias or CAS number). A handle object: free it with `free()`.
    pub fn fluid(&self, name: &str) -> Result<Fluid, JsError> {
        alive()?;
        Ok(Fluid { inner: compat::fluid_in(&self.inner, name).map_err(js)? })
    }
}

/// One fluid of a registry. A handle object: free it with `free()`.
#[wasm_bindgen]
pub struct Fluid {
    inner: phasekit_core::Fluid,
}

#[wasm_bindgen]
impl Fluid {
    /// Its canonical name.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.inner.info().name().to_string()
    }

    /// The typed flash: the state at the values of two input keys, in either order as `PropsSI`'s.
    pub fn flash(&self, name1: &str, value1: f64, name2: &str, value2: f64) -> Result<State, JsError> {
        alive()?;
        let state =
            compat::flash_si(&self.inner, (name1, value1), (name2, value2), &FlashOptions::new()).map_err(js)?;
        Ok(State { fluid: self.inner.clone(), state })
    }

    /// The batch (map 11 F23): N points of the pair two input keys name, at `values1[i]` and `values2[i]`; output `k`
    /// of point `i`, of the output key `outputs[k]`, is `values[i * M + k]`, with its cell code at `status[i * M + k]`.
    /// A failed cell is NaN with its code; a bad key or a length mismatch refuses the whole call. The points run side by
    /// side in lanes (`ExecPolicy::Simd`), 8 to a group on the simd128 build, with the scalar path's bits (M12.10).
    pub fn evaluate(
        &self,
        outputs: Vec<String>,
        name1: &str,
        values1: &[f64],
        name2: &str,
        values2: &[f64],
    ) -> Result<Batch, JsError> {
        alive()?;
        let keys: Vec<&str> = outputs.iter().map(String::as_str).collect();
        let cells = values1.len().checked_mul(keys.len()).ok_or_else(|| js("the batch has too many cells"))?;
        let (mut values, mut cell) = (vec![0.0; cells], vec![Status::Ok; cells]);
        let (input1, input2) = ((name1, values1), (name2, values2));
        compat::batch_si_with(&self.inner, ExecPolicy::Simd, &keys, input1, input2, &mut values, &mut cell)
            .map_err(js)?;
        Ok(Batch { values, status: cell.into_iter().map(compat::status_code).collect(), outputs: keys.len() })
    }
}

/// A state of a fluid, from [`Fluid::flash`].
#[wasm_bindgen]
pub struct State {
    fluid: phasekit_core::Fluid,
    state: phasekit_core::State,
}

#[wasm_bindgen]
impl State {
    /// Its temperature, K.
    #[wasm_bindgen(getter, js_name = T)]
    pub fn t(&self) -> f64 {
        self.state.t()
    }

    /// Its pressure, Pa.
    #[wasm_bindgen(getter, js_name = P)]
    pub fn p(&self) -> f64 {
        self.state.p()
    }

    /// Its phase, as CoolProp's `PhaseSI` names it.
    #[wasm_bindgen(getter)]
    pub fn phase(&self) -> String {
        compat::phase_name(self.state.phase()).to_string()
    }

    /// Any `PropsSI` output key at this state.
    pub fn get(&self, output: &str) -> Result<f64, JsError> {
        alive()?;
        compat::output_at(&self.fluid, &self.state, output).map_err(js)
    }
}

/// A batch's result, from [`Fluid::evaluate`].
#[wasm_bindgen]
pub struct Batch {
    values: Vec<f64>,
    status: Vec<u8>,
    outputs: usize,
}

#[wasm_bindgen]
impl Batch {
    /// The values, point-major (N × M), as a `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn values(&self) -> Vec<f64> {
        self.values.clone()
    }

    /// Each cell's code, as a `Uint8Array`: 0 for success, 1 for a flagged extrapolation, then the failures, as the C
    /// batch's `PK_*` codes number them.
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> Vec<u8> {
        self.status.clone()
    }

    /// M, the outputs per point.
    #[wasm_bindgen(getter)]
    pub fn outputs(&self) -> usize {
        self.outputs
    }
}
