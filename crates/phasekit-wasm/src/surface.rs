//! A fluid's thermodynamic surface (PLAN.md M26.2; user decisions DG7, DG8): phasekit-diagram's triangle mesh over T
//! and ln ρ as typed arrays, each vertex's T, ρ, p, u and s (so p–v–T and Gibbs's u–s–v, v = 1/ρ), its Q, phase and
//! inputs, the triangles' vertex indices and the dome's, so that a page draws it in WebGL and samples nothing itself.

use phasekit_compat as compat;
use phasekit_core::batch::ExecPolicy;
use phasekit_diagram::{self as diagram, FluidBatch, Limits, SurfaceVertex};
use wasm_bindgen::prelude::*;

use crate::diagram::{offset, pair_code};
use crate::{Fluid, alive, js, mixture_of};

/// A surface's mesh, as flat arrays: vertex `i`'s state is `T[i]`, `rho[i]`, `p[i]`, `u[i]`, `s[i]` and `Q[i]`, its
/// phase `phaseNames[phases[i]]`, its inputs `inputs[2i]` and `inputs[2i + 1]` of the pair `pairs[i]`; triangle `j` is
/// the vertices `triangles[3j]` to `triangles[3j + 2]`, counter-clockwise in (ln ρ, T).
#[wasm_bindgen]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    t: Vec<f64>,
    rho: Vec<f64>,
    p: Vec<f64>,
    u: Vec<f64>,
    s: Vec<f64>,
    q: Vec<f64>,
    phases: Vec<u8>,
    phase_names: Vec<String>,
    inputs: Vec<f64>,
    pairs: Vec<u8>,
    triangles: Vec<u32>,
    liquid: Vec<u32>,
    vapour: Vec<u32>,
    critical: Vec<u32>,
    triple: Vec<u32>,
}

impl Surface {
    fn of(m: &diagram::Surface) -> Result<Surface, JsError> {
        let mut out = Surface::default();
        m.vertices.iter().for_each(|v| out.push(v));
        out.triangles = indices(m.triangles.iter().flatten())?;
        out.liquid = indices(&m.dome.liquid)?;
        out.vapour = indices(&m.dome.vapour)?;
        out.critical = indices(&m.dome.critical)?;
        out.triple = indices(&m.dome.triple)?;
        Ok(out)
    }

    fn push(&mut self, v: &SurfaceVertex) {
        self.t.push(v.t);
        self.rho.push(v.rho);
        self.p.push(v.p);
        self.u.push(v.u);
        self.s.push(v.s);
        self.q.push(v.q.unwrap_or(f64::NAN));
        let phase = self.phase_code(compat::phase_name(v.phase));
        self.phases.push(phase);
        self.inputs.extend([v.inputs.x, v.inputs.y]);
        self.pairs.push(pair_code(v.inputs.pair));
    }

    /// The phase's index into `phaseNames`, a name added where it is new.
    fn phase_code(&mut self, name: &str) -> u8 {
        let k = self.phase_names.iter().position(|n| n == name).unwrap_or_else(|| {
            self.phase_names.push(name.to_string());
            self.phase_names.len().saturating_sub(1)
        });
        u8::try_from(k).unwrap_or(u8::MAX)
    }
}

/// Vertex indices, as a `Uint32Array` holds them.
fn indices<'a>(all: impl IntoIterator<Item = &'a usize>) -> Result<Vec<u32>, JsError> {
    all.into_iter().map(|&k| offset(k)).collect()
}

#[wasm_bindgen]
impl Surface {
    /// Each vertex's temperature, K.
    #[wasm_bindgen(getter, js_name = T)]
    pub fn t(&self) -> Vec<f64> {
        self.t.clone()
    }

    /// Each vertex's density, kg/m³; v = 1/ρ.
    #[wasm_bindgen(getter)]
    pub fn rho(&self) -> Vec<f64> {
        self.rho.clone()
    }

    /// Each vertex's pressure, Pa.
    #[wasm_bindgen(getter)]
    pub fn p(&self) -> Vec<f64> {
        self.p.clone()
    }

    /// Each vertex's specific internal energy, J/kg.
    #[wasm_bindgen(getter)]
    pub fn u(&self) -> Vec<f64> {
        self.u.clone()
    }

    /// Each vertex's specific entropy, J/(kg·K).
    #[wasm_bindgen(getter)]
    pub fn s(&self) -> Vec<f64> {
        self.s.clone()
    }

    /// Each vertex's vapour quality where its state is two-phase, 0 and 1 on the dome; NaN elsewhere.
    #[wasm_bindgen(getter, js_name = Q)]
    pub fn q(&self) -> Vec<f64> {
        self.q.clone()
    }

    /// Each vertex's phase, as a `Uint8Array` of indices into `phaseNames`.
    #[wasm_bindgen(getter)]
    pub fn phases(&self) -> Vec<u8> {
        self.phases.clone()
    }

    /// The phases the vertices have, as CoolProp's `PhaseSI` names them, in the order they first appear.
    #[wasm_bindgen(getter, js_name = phaseNames)]
    pub fn phase_names(&self) -> Vec<String> {
        self.phase_names.clone()
    }

    /// The input values each vertex was computed at, two a vertex in its pair's order, D per unit mass.
    #[wasm_bindgen(getter)]
    pub fn inputs(&self) -> Vec<f64> {
        self.inputs.clone()
    }

    /// Each vertex's input pair, as a `Uint8Array` of indices into `pairKeys()`'s pairs: `DT` or `QT`.
    #[wasm_bindgen(getter)]
    pub fn pairs(&self) -> Vec<u8> {
        self.pairs.clone()
    }

    /// The triangles, three vertex indices each, counter-clockwise in (ln ρ, T).
    #[wasm_bindgen(getter)]
    pub fn triangles(&self) -> Vec<u32> {
        self.triangles.clone()
    }

    /// The saturated liquid's vertices, Q = 0, in order of temperature up to just below T_c.
    #[wasm_bindgen(getter)]
    pub fn liquid(&self) -> Vec<u32> {
        self.liquid.clone()
    }

    /// The saturated vapour's vertices, Q = 1, at the liquid's temperatures.
    #[wasm_bindgen(getter)]
    pub fn vapour(&self) -> Vec<u32> {
        self.vapour.clone()
    }

    /// The critical point's vertex, the state at (T_c, ρ_c); empty where the library refuses it.
    #[wasm_bindgen(getter)]
    pub fn critical(&self) -> Vec<u32> {
        self.critical.clone()
    }

    /// The triple line's vertices, from the saturated vapour to the saturated liquid, where the plotted range starts
    /// at the triple point; else empty.
    #[wasm_bindgen(getter)]
    pub fn triple(&self) -> Vec<u32> {
        self.triple.clone()
    }
}

#[wasm_bindgen]
impl Fluid {
    /// Its p–v–T and u–s–v surface (PLAN.md M26.2): phasekit-diagram's mesh, by its batch in lanes
    /// (`ExecPolicy::Simd`) and the default flash options, each vertex's phase the library's at its inputs. A mixture
    /// has none and is refused.
    pub fn surface(&self) -> Result<Surface, JsError> {
        alive()?;
        if mixture_of(&self.inner).is_some() {
            return Err(js(format!("{} is a mixture: its surface is not drawn", self.name())));
        }
        let batch = FluidBatch::new(&self.inner).with_exec(ExecPolicy::Simd);
        let limits = Limits::of(&self.inner).map_err(js)?;
        Surface::of(&diagram::sample_surface(&batch, &|inputs| batch.phase(inputs), &limits).map_err(js)?)
    }
}
