//! Humid air in the browser package (PLAN.md M18.8): `Registry.humidAir()`, RP-1485's humid air over the registry's
//! Water and Air (the package does not embed Air: a page adds its pack with `withPack` first) and `phasekit-iapws`'s
//! IF97 water and ice, under `Corrected`. `HumidAir.HAPropsSI(...)` is compat's `ha_props_si_in`; `HumidAir.state(p,
//! name1, value1, name2, value2)` is the state the pressure and two more inputs name, whose `get(output)` takes every
//! `HAPropsSI` output; `humidAirKeys()` lists the names, aliases included.

use phasekit_compat as compat;
use phasekit_core::DataSet;
use wasm_bindgen::prelude::*;

use crate::{Registry, alive, js};

/// Every name `HAPropsSI` and `HumidAirState.get` read, v8.0.0's aliases included, in v8.0.0's order.
#[wasm_bindgen(js_name = humidAirKeys)]
pub fn humid_air_keys() -> Vec<String> {
    compat::ha_names().map(String::from).collect()
}

/// RP-1485's humid air over a registry's fluids. A handle object: free it with `free()`.
#[wasm_bindgen]
pub struct HumidAir {
    inner: phasekit_humidair::HumidAir,
}

#[wasm_bindgen]
impl Registry {
    /// Humid air over this registry's Water and Air, which needs Air's pack (`withPack`), and `phasekit-iapws`'s IF97
    /// water and ice, under `Corrected`. A handle object: free it with `free()`.
    #[wasm_bindgen(js_name = humidAir)]
    pub fn humid_air(&self) -> Result<HumidAir, JsError> {
        alive()?;
        Ok(HumidAir { inner: compat::humid_air_in(&self.inner, DataSet::Corrected).map_err(js)? })
    }
}

#[wasm_bindgen]
impl HumidAir {
    /// CoolProp's `HAPropsSI(output, name1, value1, name2, value2, name3, value3)`: the pressure and two more inputs,
    /// v8.0.0's names and aliases, every output from the state.
    #[wasm_bindgen(js_name = HAPropsSI)]
    #[allow(clippy::too_many_arguments, reason = "HAPropsSI's seven arguments")]
    pub fn ha_props_si(
        &self,
        output: &str,
        name1: &str,
        value1: f64,
        name2: &str,
        value2: f64,
        name3: &str,
        value3: f64,
    ) -> Result<f64, JsError> {
        alive()?;
        compat::ha_props_si_in(&self.inner, output, name1, value1, name2, value2, name3, value3).map_err(js)
    }

    /// The state at the pressure `p` (Pa) and two more inputs, as `HAPropsSI` reads them.
    pub fn state(&self, p: f64, name1: &str, value1: f64, name2: &str, value2: f64) -> Result<HumidAirState, JsError> {
        alive()?;
        let state = compat::ha_state_in(&self.inner, p, (name1, value1), (name2, value2)).map_err(js)?;
        Ok(HumidAirState { air: self.inner.clone(), state })
    }
}

/// A state of humid air, from [`HumidAir::state`].
#[wasm_bindgen]
pub struct HumidAirState {
    air: phasekit_humidair::HumidAir,
    state: phasekit_humidair::MoistAir,
}

#[wasm_bindgen]
impl HumidAirState {
    /// Its dry-bulb temperature, K.
    #[wasm_bindgen(getter, js_name = T)]
    pub fn t(&self) -> f64 {
        self.state.t()
    }

    /// Its pressure, Pa.
    #[wasm_bindgen(getter, js_name = P)]
    pub fn p(&self) -> f64 {
        self.state.p()
    }

    /// Its humidity ratio, kg of water per kg of dry air.
    #[wasm_bindgen(getter, js_name = W)]
    pub fn w(&self) -> f64 {
        self.state.humidity_ratio()
    }

    /// Any `HAPropsSI` output at this state.
    pub fn get(&self, output: &str) -> Result<f64, JsError> {
        alive()?;
        compat::ha_output_at(&self.air, &self.state, output).map_err(js)
    }
}
