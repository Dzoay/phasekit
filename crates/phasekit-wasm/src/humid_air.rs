//! Humid air in the browser package (PLAN.md M18.8): `Registry.humidAir()`, RP-1485's humid air over the registry's
//! Water and Air (the package does not embed Air: a page adds its pack with `withPack` first) and `phasekit-iapws`'s
//! IF97 water and ice, under `Corrected`. `HumidAir.HAPropsSI(...)` is compat's `ha_props_si_in`; `HumidAir.state(p,
//! name1, value1, name2, value2)` is the state the pressure and two more inputs name, whose `get(output)` takes every
//! `HAPropsSI` output; `humidAirKeys()` lists the names, aliases included. `HumidAir.psychrometric(p)` and
//! `HumidAir.mollier(p)` are `phasekit-diagram`'s psychrometric chart and Mollier's h–x chart at p (PLAN.md M18.9,
//! M18.10), their lines as typed arrays (`Chart`).

use phasekit_compat as compat;
use phasekit_core::{DataSet, Error};
use phasekit_diagram::{ChartLine, ChartRange, ChartVertex, Held, HumidAirBatch, MollierVertex};
use phasekit_humidair::{Given, Humidity, Per};
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

    /// The psychrometric chart at the pressure `p` (Pa), 0 to 50 °C and W to 0.03 unless `t_min`, `t_max` (K) and
    /// `w_max` (kg/kg) say otherwise (PLAN.md M18.9).
    pub fn psychrometric(
        &self,
        p: f64,
        t_min: Option<f64>,
        t_max: Option<f64>,
        w_max: Option<f64>,
    ) -> Result<Chart, JsError> {
        alive()?;
        let range = chart_range(t_min, t_max, w_max);
        let chart = phasekit_diagram::psychrometric(&AtP { air: &self.inner, p }, range).map_err(js)?;
        Chart::of(std::iter::once(&chart.saturation).chain(&chart.lines), None)
    }

    /// Mollier's h–x chart at the pressure `p` (Pa), over the psychrometric chart's range and its defaults (PLAN.md
    /// M18.10): its vertices carry h and the drawn height h − r₀W besides T and W.
    pub fn mollier(
        &self,
        p: f64,
        t_min: Option<f64>,
        t_max: Option<f64>,
        w_max: Option<f64>,
    ) -> Result<Chart, JsError> {
        alive()?;
        let range = chart_range(t_min, t_max, w_max);
        let chart = phasekit_diagram::mollier(&AtP { air: &self.inner, p }, range).map_err(js)?;
        Chart::of(std::iter::once(&chart.saturation).chain(&chart.lines), Some(chart.r0))
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

/// A chart's range: 0 to 50 °C and W to 0.03 unless the caller says otherwise.
fn chart_range(t_min: Option<f64>, t_max: Option<f64>, w_max: Option<f64>) -> ChartRange {
    let default = ChartRange::default();
    let t = (t_min.unwrap_or(default.t.0), t_max.unwrap_or(default.t.1));
    ChartRange { t, w_max: w_max.unwrap_or(default.w_max) }
}

/// Humid air at one pressure, as `phasekit-diagram`'s charts sample it (PLAN.md M18.9).
struct AtP<'a> {
    air: &'a phasekit_humidair::HumidAir,
    p: f64,
}

impl AtP<'_> {
    /// The input that holds `held`, besides the saturation line's.
    fn given(held: Held) -> Option<Given> {
        match held {
            Held::Relative(phi) => Some(Given::Humidity(Humidity::Relative(phi))),
            Held::Enthalpy(h) => Some(Given::Enthalpy(h)),
            Held::Volume(v) => Some(Given::Volume(v)),
            Held::WetBulb(t_wb) => Some(Given::WetBulb(t_wb)),
            _ => None,
        }
    }

    /// W of the state at `t` holding `held`, where there is one.
    fn ratio(&self, held: Held, t: f64) -> Option<f64> {
        if held == Held::Saturation {
            return self.air.saturated_humidity_ratio(t, self.p).ok().filter(|w| *w >= 0.0);
        }
        let state = self.air.state(self.p, Given::Temperature(t), Self::given(held)?);
        state.ok().map(|s| s.humidity_ratio())
    }

    /// T of the state at `w` holding `held`, where there is one.
    fn temperature(&self, held: Held, w: f64) -> Option<f64> {
        let state = self.air.state(self.p, Given::Humidity(Humidity::Ratio(w)), Self::given(held)?);
        state.ok().map(|s| s.t())
    }
}

impl HumidAirBatch for AtP<'_> {
    fn p(&self) -> f64 {
        self.p
    }

    fn ratios(&self, held: Held, t: &[f64]) -> Result<Vec<Option<f64>>, Error> {
        Ok(t.iter().map(|&t| self.ratio(held, t)).collect())
    }

    fn temperatures(&self, held: Held, w: &[f64]) -> Result<Vec<Option<f64>>, Error> {
        Ok(w.iter().map(|&w| self.temperature(held, w)).collect())
    }

    fn held_at(&self, t: f64, w: f64) -> Result<[f64; 3], Error> {
        let state = self.air.state(self.p, Given::Temperature(t), Given::Humidity(Humidity::Ratio(w)))?;
        Ok([state.h(Per::KgDryAir), state.v(Per::KgDryAir), self.air.wet_bulb(&state)?])
    }

    fn enthalpies(&self, at: &[(f64, f64)]) -> Result<Vec<Option<f64>>, Error> {
        let h = |(t, w)| self.air.state(self.p, Given::Temperature(t), Given::Humidity(Humidity::Ratio(w)));
        Ok(at.iter().map(|&tw| h(tw).ok().map(|s| s.h(Per::KgDryAir))).collect())
    }
}

/// A chart's lines, as flat arrays: line `i` holds `held[i]` (`saturation`, `R`, `T`, `H`, `V` or `Twb`) at
/// `values[i]` (1 for the saturation line), and its runs are those from `segments[i]` to `segments[i + 1]`; run `j`'s
/// vertices are those of `T` and `W` from `starts[j]` to `starts[j + 1]`, and on Mollier's chart those of `H` and `Y`.
#[wasm_bindgen]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chart {
    held: Vec<String>,
    values: Vec<f64>,
    segments: Vec<u32>,
    starts: Vec<u32>,
    t: Vec<f64>,
    w: Vec<f64>,
    h: Vec<f64>,
    y: Vec<f64>,
    r0: Option<f64>,
}

/// A chart's vertex as the flat arrays take it.
trait Cells {
    fn push(&self, chart: &mut Chart);
}

impl Cells for ChartVertex {
    fn push(&self, chart: &mut Chart) {
        chart.t.push(self.t);
        chart.w.push(self.w);
    }
}

impl Cells for MollierVertex {
    fn push(&self, chart: &mut Chart) {
        chart.t.push(self.t);
        chart.w.push(self.w);
        chart.h.push(self.h);
        chart.y.push(self.y);
    }
}

/// A count as a typed array's offset.
fn offset(n: usize) -> Result<u32, JsError> {
    u32::try_from(n).map_err(|_| js("the chart has too many vertices"))
}

/// A line's held key and value.
fn label(held: Held) -> (&'static str, f64) {
    match held {
        Held::Relative(phi) => ("R", phi),
        Held::Enthalpy(h) => ("H", h),
        Held::Volume(v) => ("V", v),
        Held::WetBulb(t_wb) => ("Twb", t_wb),
        Held::DryBulb(t) => ("T", t),
        _ => ("saturation", 1.0),
    }
}

impl Chart {
    /// `lines` as flat arrays, with Mollier's skew `r0`.
    fn of<'a, V: Cells + 'a>(lines: impl Iterator<Item = &'a ChartLine<V>>, r0: Option<f64>) -> Result<Chart, JsError> {
        let mut out = Chart { segments: vec![0], starts: vec![0], r0, ..Chart::default() };
        for line in lines {
            let (key, value) = label(line.held);
            out.held.push(key.to_string());
            out.values.push(value);
            for segment in &line.segments {
                segment.iter().for_each(|v| v.push(&mut out));
                out.starts.push(offset(out.t.len())?);
            }
            out.segments.push(offset(out.starts.len().saturating_sub(1))?);
        }
        Ok(out)
    }
}

#[wasm_bindgen]
impl Chart {
    /// Each line's held key: `saturation`, `R`, `T`, `H`, `V` or `Twb`.
    #[wasm_bindgen(getter)]
    pub fn held(&self) -> Vec<String> {
        self.held.clone()
    }

    /// Each line's value: φ, T (K), h (J/kg of dry air), v (m³/kg of dry air), T_wb (K), or 1 for the saturation line.
    #[wasm_bindgen(getter)]
    pub fn values(&self) -> Vec<f64> {
        self.values.clone()
    }

    /// Each line's first run, and one past the last's: one more entry than lines.
    #[wasm_bindgen(getter)]
    pub fn segments(&self) -> Vec<u32> {
        self.segments.clone()
    }

    /// Each run's first vertex, and one past the last's: one more entry than runs.
    #[wasm_bindgen(getter)]
    pub fn starts(&self) -> Vec<u32> {
        self.starts.clone()
    }

    /// The vertices' dry-bulb temperatures, K.
    #[wasm_bindgen(getter, js_name = T)]
    pub fn t(&self) -> Vec<f64> {
        self.t.clone()
    }

    /// The vertices' humidity ratios, kg/kg.
    #[wasm_bindgen(getter, js_name = W)]
    pub fn w(&self) -> Vec<f64> {
        self.w.clone()
    }

    /// Mollier's chart: the vertices' enthalpies, J/kg of dry air; empty on the psychrometric chart.
    #[wasm_bindgen(getter, js_name = H)]
    pub fn h(&self) -> Vec<f64> {
        self.h.clone()
    }

    /// Mollier's chart: the vertices' drawn heights h − r₀W, J/kg of dry air; empty on the psychrometric chart.
    #[wasm_bindgen(getter, js_name = Y)]
    pub fn y(&self) -> Vec<f64> {
        self.y.clone()
    }

    /// Mollier's chart: its skew r₀, water vapour's h at 0 °C, J/kg; undefined on the psychrometric chart.
    #[wasm_bindgen(getter)]
    pub fn r0(&self) -> Option<f64> {
        self.r0
    }
}
