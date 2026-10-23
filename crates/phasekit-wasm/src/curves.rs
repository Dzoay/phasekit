//! A fluid's characteristic curves and compressibility chart (PLAN.md M26.6; user decision DG10): phasekit-diagram's,
//! as typed arrays, so that a page draws them and samples nothing itself.

use phasekit_core::batch::ExecPolicy;
use phasekit_diagram::{self as diagram, CurveVertex, FluidBatch, Limits};
use wasm_bindgen::prelude::*;

use crate::diagram::{offset, pair_code};
use crate::{Fluid, alive, js, mixture_of, refuse};

/// Runs of states, as flat arrays: run `k`'s vertices are those from `starts[k]` to `starts[k + 1]`, vertex `i`'s state
/// `T[i]`, `rho[i]`, `p[i]` and `Z[i]`, its inputs `inputs[2i]` and `inputs[2i + 1]` of the pair `pairs[i]`.
#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq)]
pub struct CurveRuns {
    starts: Vec<u32>,
    t: Vec<f64>,
    rho: Vec<f64>,
    p: Vec<f64>,
    z: Vec<f64>,
    inputs: Vec<f64>,
    pairs: Vec<u8>,
}

impl Default for CurveRuns {
    fn default() -> Self {
        CurveRuns {
            starts: vec![0],
            t: Vec::new(),
            rho: Vec::new(),
            p: Vec::new(),
            z: Vec::new(),
            inputs: Vec::new(),
            pairs: Vec::new(),
        }
    }
}

impl CurveRuns {
    fn push(&mut self, run: &[CurveVertex]) -> Result<(), JsError> {
        for v in run {
            self.t.push(v.t);
            self.rho.push(v.rho);
            self.p.push(v.p);
            self.z.push(v.z);
            self.inputs.extend([v.inputs.x, v.inputs.y]);
            self.pairs.push(pair_code(v.inputs.pair));
        }
        self.starts.push(offset(self.t.len())?);
        Ok(())
    }
}

#[wasm_bindgen]
impl CurveRuns {
    /// Each run's first vertex, and one past the last's: one more entry than runs.
    #[wasm_bindgen(getter)]
    pub fn starts(&self) -> Vec<u32> {
        self.starts.clone()
    }

    /// Each vertex's temperature, K.
    #[wasm_bindgen(getter, js_name = T)]
    pub fn t(&self) -> Vec<f64> {
        self.t.clone()
    }

    /// Each vertex's density, kg/m³.
    #[wasm_bindgen(getter)]
    pub fn rho(&self) -> Vec<f64> {
        self.rho.clone()
    }

    /// Each vertex's pressure, Pa.
    #[wasm_bindgen(getter)]
    pub fn p(&self) -> Vec<f64> {
        self.p.clone()
    }

    /// Each vertex's compressibility factor p/(ρRT).
    #[wasm_bindgen(getter, js_name = Z)]
    pub fn z(&self) -> Vec<f64> {
        self.z.clone()
    }

    /// The input values each vertex was computed at, two a vertex in its pair's order, D per unit mass.
    #[wasm_bindgen(getter)]
    pub fn inputs(&self) -> Vec<f64> {
        self.inputs.clone()
    }

    /// Each vertex's input pair, as a `Uint8Array` of indices into `pairKeys()`'s pairs: `DT` on a curve, `PT` or `QT`
    /// on an isotherm.
    #[wasm_bindgen(getter)]
    pub fn pairs(&self) -> Vec<u8> {
        self.pairs.clone()
    }
}

/// A fluid's four characteristic curves and its compressibility chart: curve `i` is `names[i]`, its lines the runs of
/// `lines` from `curves[i]` to `curves[i + 1]`, and it meets zero density at `zeroDensity[i]` (NaN where it does not
/// below the equation of state's highest temperature); the chart's isotherm `j` is at `isotherms[j]`, its vertices the
/// run `j` of `chart`.
#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq)]
pub struct Curves {
    names: Vec<String>,
    zero_density: Vec<f64>,
    curves: Vec<u32>,
    lines: CurveRuns,
    isotherms: Vec<f64>,
    chart: CurveRuns,
}

impl Curves {
    fn of(c: &diagram::Curves) -> Result<Curves, JsError> {
        let mut out = Curves {
            names: Vec::new(),
            zero_density: Vec::new(),
            curves: vec![0],
            lines: CurveRuns::default(),
            isotherms: Vec::new(),
            chart: CurveRuns::default(),
        };
        for curve in &c.curves {
            out.names.push(format!("{:?}", curve.characteristic));
            out.zero_density.push(curve.zero_density.unwrap_or(f64::NAN));
            for line in &curve.lines {
                out.lines.push(line)?;
            }
            out.curves.push(offset(out.lines.starts.len().saturating_sub(1))?);
        }
        for isotherm in &c.compressibility {
            out.isotherms.push(isotherm.t);
            out.chart.push(&isotherm.vertices)?;
        }
        Ok(out)
    }
}

#[wasm_bindgen]
impl Curves {
    /// Each curve's name: `Ideal`, `Boyle`, `JouleThomsonInversion` and `JouleInversion`, Brown's order.
    #[wasm_bindgen(getter)]
    pub fn names(&self) -> Vec<String> {
        self.names.clone()
    }

    /// The temperature, K, at which each curve meets zero density; NaN where it does not in the sampled range.
    #[wasm_bindgen(getter, js_name = zeroDensity)]
    pub fn zero_density(&self) -> Vec<f64> {
        self.zero_density.clone()
    }

    /// Each curve's first line in `lines`, and one past the last's: one more entry than curves.
    #[wasm_bindgen(getter)]
    pub fn curves(&self) -> Vec<u32> {
        self.curves.clone()
    }

    /// Every curve's lines, each from its lower-density end.
    #[wasm_bindgen(getter)]
    pub fn lines(&self) -> CurveRuns {
        self.lines.clone()
    }

    /// The compressibility chart's isotherms' temperatures, K, rising.
    #[wasm_bindgen(getter)]
    pub fn isotherms(&self) -> Vec<f64> {
        self.isotherms.clone()
    }

    /// The chart's isotherms, a run each, Z against p in order of pressure; below T_c the saturated vapour and then
    /// the saturated liquid at the saturation pressure, both `QT` states, between which nothing is drawn.
    #[wasm_bindgen(getter)]
    pub fn chart(&self) -> CurveRuns {
        self.chart.clone()
    }
}

#[wasm_bindgen]
impl Fluid {
    /// Its characteristic curves and compressibility chart (PLAN.md M26.3, M26.6): phasekit-diagram's, by its batch in
    /// lanes (`ExecPolicy::Simd`) and the default flash options, over the equation of state's range (user decision
    /// DG10). A mixture has none and is refused.
    pub fn curves(&self) -> Result<Curves, JsValue> {
        alive()?;
        if mixture_of(&self.inner).is_some() {
            return Err(js(format!("{} is a mixture: its characteristic curves are not drawn", self.name())).into());
        }
        let batch = FluidBatch::new(&self.inner).with_exec(ExecPolicy::Simd);
        let limits = Limits::of(&self.inner).map_err(refuse)?;
        Ok(Curves::of(&diagram::sample_curves(&batch, &limits).map_err(refuse)?)?)
    }
}
