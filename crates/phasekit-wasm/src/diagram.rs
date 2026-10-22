//! The diagrams' geometry (PLAN.md M19.6; user decisions DG1-DG3): `planes`, each plane's axes and isoline families,
//! and a fluid's `diagram(plane)` and `regions()` from phasekit-diagram, as typed arrays, so that a page draws them and
//! samples nothing itself.
//!
//! - A `Vertices` list: each vertex's x and y (`xy`, two a vertex), the input values it was computed at (`inputs`, two
//!   a vertex, in its pair's order) and its pair (`pairs`, an index into `pair_keys`).
//! - A `Diagram`: its axes' spans (`x`, `y`), the dome's `liquid`, `vapour` and `critical` vertices, and its `lines`
//!   (`Lines`): per line its held key and value and its segments, per segment whether it is two-phase and its run of
//!   vertices, as offsets into flat arrays.
//! - The `Regions`: per region its phase and outline, the critical isotherm and isobar, each a `Corners` list of
//!   runs whose corners carry T, P, Hmass, Smass and Dmass, so that every plane reads its axes from them.

use phasekit_compat as compat;
use phasekit_core::batch::ExecPolicy;
use phasekit_core::{Pair, ThermoModel, Var};
use phasekit_diagram::{self as diagram, FluidBatch, Limits, Plane, Quantity};
use phasekit_mix::{EnvelopeOptions, Mixture, Side};
use wasm_bindgen::prelude::*;

use crate::{Fluid, alive, end_name, end_reason, js, mixture_of};

/// A quantity's key, as `PropsSI` names it.
fn key(quantity: Quantity) -> &'static str {
    match quantity {
        Quantity::T => "T",
        Quantity::P => "P",
        Quantity::Hmass => "Hmass",
        Quantity::Smass => "Smass",
        Quantity::Dmass => "Dmass",
        Quantity::Q => "Q",
        Quantity::Umass => "Umass",
        Quantity::Z => "Z",
        Quantity::Bvirial => "Bvirial",
        Quantity::DBvirialDT => "dBvirial_dT",
        Quantity::DpDrhoAtT => "d(P)/d(Dmass)|T",
        Quantity::DpDTAtRho => "d(P)/d(T)|Dmass",
        Quantity::JouleThomson => "d(T)/d(P)|Hmass",
        // Any other output: a property map's, which no plane plots, so none of the package's planes names it.
        Quantity::Output(_) => "?",
    }
}

/// An input variable's key, D, H, S and U per unit mass, as a vertex's inputs are; a variable core adds later is `?`
/// until it is named here.
fn var_key(var: Var) -> &'static str {
    match var {
        Var::T => "T",
        Var::P => "P",
        Var::Q => "Q",
        Var::D => "Dmass",
        Var::H => "Hmass",
        Var::S => "Smass",
        Var::U => "Umass",
        _ => "?",
    }
}

/// Each input pair's two keys, in its order, pair after pair: the keys of the pair a vertex's `pairs` entry `k` names
/// are `pairKeys()[2k]` and `pairKeys()[2k + 1]`, and its `inputs` are their values.
#[wasm_bindgen(js_name = pairKeys)]
pub fn pair_keys() -> Vec<String> {
    Pair::ALL.iter().flat_map(|pair| <[Var; 2]>::from(pair.vars())).map(|var| var_key(var).to_string()).collect()
}

/// The diagrams' planes, `ph`, `ts`, `pt`, `trho` and `hs`, in the site's order (PLAN.md M19.6).
#[wasm_bindgen]
pub fn planes() -> Vec<PlaneInfo> {
    Plane::ALL.into_iter().map(|plane| PlaneInfo { plane }).collect()
}

/// A plane: its id, its axes, which of them is logarithmic, and its families of isolines.
#[wasm_bindgen(js_name = Plane)]
pub struct PlaneInfo {
    plane: Plane,
}

#[wasm_bindgen(js_class = Plane)]
impl PlaneInfo {
    /// Its id: `ph`, `ts`, `pt`, `trho` or `hs`.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.plane.id().to_string()
    }

    /// The horizontal axis's key.
    #[wasm_bindgen(getter, js_name = xKey)]
    pub fn x_key(&self) -> String {
        key(self.plane.x().quantity).to_string()
    }

    /// Whether the horizontal axis is logarithmic.
    #[wasm_bindgen(getter, js_name = xLog)]
    pub fn x_log(&self) -> bool {
        self.plane.x().log
    }

    /// The vertical axis's key.
    #[wasm_bindgen(getter, js_name = yKey)]
    pub fn y_key(&self) -> String {
        key(self.plane.y().quantity).to_string()
    }

    /// Whether the vertical axis is logarithmic.
    #[wasm_bindgen(getter, js_name = yLog)]
    pub fn y_log(&self) -> bool {
        self.plane.y().log
    }

    /// The key each family of isolines holds, in the plane's order.
    #[wasm_bindgen(getter)]
    pub fn held(&self) -> Vec<String> {
        self.plane.families().iter().map(|f| key(f.held).to_string()).collect()
    }

    /// The key each family's lines are swept over.
    #[wasm_bindgen(getter)]
    pub fn sweep(&self) -> Vec<String> {
        self.plane.families().iter().map(|f| key(f.sweep).to_string()).collect()
    }
}

/// A list of vertices, each the library's x and y at the inputs it carries.
#[wasm_bindgen]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vertices {
    xy: Vec<f64>,
    inputs: Vec<f64>,
    pairs: Vec<u8>,
}

impl Vertices {
    fn of<'a>(vertices: impl IntoIterator<Item = &'a diagram::Vertex>) -> Vertices {
        let mut out = Vertices::default();
        vertices.into_iter().for_each(|v| out.push(v));
        out
    }

    fn push(&mut self, v: &diagram::Vertex) {
        self.xy.extend([v.x, v.y]);
        self.inputs.extend([v.inputs.x, v.inputs.y]);
        self.pairs.push(pair_code(v.inputs.pair));
    }
}

/// A pair's index into `pairKeys()`'s pairs.
pub(crate) fn pair_code(pair: Pair) -> u8 {
    let code = Pair::ALL.iter().position(|p| *p == pair).and_then(|k| u8::try_from(k).ok());
    code.unwrap_or(u8::MAX)
}

#[wasm_bindgen]
impl Vertices {
    /// Each vertex's x and y, two a vertex, as a `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn xy(&self) -> Vec<f64> {
        self.xy.clone()
    }

    /// The input values each vertex was computed at, two a vertex in its pair's order, D, H and S per unit mass.
    #[wasm_bindgen(getter)]
    pub fn inputs(&self) -> Vec<f64> {
        self.inputs.clone()
    }

    /// Each vertex's input pair, as a `Uint8Array` of indices into `pairKeys()`'s pairs.
    #[wasm_bindgen(getter)]
    pub fn pairs(&self) -> Vec<u8> {
        self.pairs.clone()
    }

    /// The number of vertices.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.pairs.len()
    }
}

/// A count or an offset into a typed array, as a `Uint32Array` holds it.
pub(crate) fn offset(n: usize) -> Result<u32, JsError> {
    u32::try_from(n).map_err(|_| js("the diagram has too many vertices"))
}

/// A diagram's isolines, as flat arrays: line `i` holds `held[i]` at `values[i]`, and its segments are those from
/// `segments[i]` to `segments[i + 1]`; segment `j` is two-phase where `twoPhase[j]` is 1, and its vertices are those of
/// `vertices` from `starts[j]` to `starts[j + 1]`.
#[wasm_bindgen]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lines {
    held: Vec<String>,
    values: Vec<f64>,
    segments: Vec<u32>,
    two_phase: Vec<u8>,
    starts: Vec<u32>,
    vertices: Vertices,
}

impl Lines {
    fn of(lines: &[diagram::Line]) -> Result<Lines, JsError> {
        let mut out = Lines { segments: vec![0], starts: vec![0], ..Lines::default() };
        for line in lines {
            out.held.push(key(line.held).to_string());
            out.values.push(line.value);
            for segment in &line.segments {
                out.two_phase.push(u8::from(segment.two_phase));
                segment.vertices.iter().for_each(|v| out.vertices.push(v));
                out.starts.push(offset(out.vertices.length())?);
            }
            out.segments.push(offset(out.two_phase.len())?);
        }
        Ok(out)
    }
}

#[wasm_bindgen]
impl Lines {
    /// Each line's held key.
    #[wasm_bindgen(getter)]
    pub fn held(&self) -> Vec<String> {
        self.held.clone()
    }

    /// Each line's value, its label's.
    #[wasm_bindgen(getter)]
    pub fn values(&self) -> Vec<f64> {
        self.values.clone()
    }

    /// Each line's first segment, and one past the last's: one more entry than lines.
    #[wasm_bindgen(getter)]
    pub fn segments(&self) -> Vec<u32> {
        self.segments.clone()
    }

    /// Whether each segment runs inside the dome: 1 where it does.
    #[wasm_bindgen(getter, js_name = twoPhase)]
    pub fn two_phase(&self) -> Vec<u8> {
        self.two_phase.clone()
    }

    /// Each segment's first vertex, and one past the last's: one more entry than segments.
    #[wasm_bindgen(getter)]
    pub fn starts(&self) -> Vec<u32> {
        self.starts.clone()
    }

    /// Every segment's vertices, in order.
    #[wasm_bindgen(getter)]
    pub fn vertices(&self) -> Vertices {
        self.vertices.clone()
    }
}

/// A fluid's diagram on one plane, from `Fluid::diagram`.
#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq)]
pub struct Diagram {
    plane: Plane,
    x: Vec<f64>,
    y: Vec<f64>,
    liquid: Vertices,
    vapour: Vertices,
    critical: Vertices,
    lines: Lines,
    end: String,
    reason: String,
}

impl Diagram {
    fn of(d: &diagram::Diagram, end: String, reason: String) -> Result<Diagram, JsError> {
        Ok(Diagram {
            plane: d.plane,
            x: vec![d.x.0, d.x.1],
            y: vec![d.y.0, d.y.1],
            liquid: Vertices::of(&d.dome.liquid),
            vapour: Vertices::of(&d.dome.vapour),
            critical: Vertices::of(&d.dome.critical),
            lines: Lines::of(&d.lines)?,
            end,
            reason,
        })
    }
}

#[wasm_bindgen]
impl Diagram {
    /// Its plane's id.
    #[wasm_bindgen(getter)]
    pub fn plane(&self) -> String {
        self.plane.id().to_string()
    }

    /// The horizontal axis's span, `[lo, hi]`.
    #[wasm_bindgen(getter)]
    pub fn x(&self) -> Vec<f64> {
        self.x.clone()
    }

    /// The vertical axis's span.
    #[wasm_bindgen(getter)]
    pub fn y(&self) -> Vec<f64> {
        self.y.clone()
    }

    /// The dome's liquid side, toward the critical point: a pure fluid's saturated liquid, a mixture's bubble points.
    #[wasm_bindgen(getter)]
    pub fn liquid(&self) -> Vertices {
        self.liquid.clone()
    }

    /// The dome's vapour side, toward the critical point: a pure fluid's saturated vapour, a mixture's dew points.
    #[wasm_bindgen(getter)]
    pub fn vapour(&self) -> Vertices {
        self.vapour.clone()
    }

    /// The critical point, one vertex, or none where the library gives none.
    #[wasm_bindgen(getter)]
    pub fn critical(&self) -> Vertices {
        self.critical.clone()
    }

    /// Its isolines; a mixture's diagram has none.
    #[wasm_bindgen(getter)]
    pub fn lines(&self) -> Lines {
        self.lines.clone()
    }

    /// How a mixture's trace of its envelope ended, as `Envelope.end` says; empty for a pure fluid.
    #[wasm_bindgen(getter)]
    pub fn end(&self) -> String {
        self.end.clone()
    }

    /// Why it ended at a bound or failed, as `Envelope.reason` says; empty where it closed and for a pure fluid.
    #[wasm_bindgen(getter)]
    pub fn reason(&self) -> String {
        self.reason.clone()
    }
}

/// Runs of corners, each corner five values, T, P, Hmass, Smass and Dmass: run `i` is the corners from `starts[i]` to
/// `starts[i + 1]`, corner `k` the values `corners[5k]` to `corners[5k + 4]`.
#[wasm_bindgen]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Corners {
    starts: Vec<u32>,
    corners: Vec<f64>,
}

impl Corners {
    fn of<'a>(runs: impl IntoIterator<Item = &'a [diagram::Corner]>) -> Result<Corners, JsError> {
        let mut out = Corners { starts: vec![0], corners: Vec::new() };
        for run in runs {
            out.corners.extend(run.iter().flat_map(|c| [c.t, c.p, c.h, c.s, c.rho]));
            out.starts.push(offset(out.corners.len() / 5)?);
        }
        Ok(out)
    }
}

#[wasm_bindgen]
impl Corners {
    /// Each run's first corner, and one past the last's: one more entry than runs.
    #[wasm_bindgen(getter)]
    pub fn starts(&self) -> Vec<u32> {
        self.starts.clone()
    }

    /// Every corner's T, P, Hmass, Smass and Dmass, five a corner.
    #[wasm_bindgen(getter)]
    pub fn corners(&self) -> Vec<f64> {
        self.corners.clone()
    }
}

/// A fluid's phase regions, from `Fluid::regions`.
#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq)]
pub struct Regions {
    phases: Vec<String>,
    outlines: Corners,
    isotherm: Corners,
    isobar: Corners,
    critical: Vec<f64>,
    t_critical: f64,
    p_critical: f64,
}

impl Regions {
    fn of(r: &diagram::Regions) -> Result<Regions, JsError> {
        Ok(Regions {
            phases: r.regions.iter().map(|g| compat::phase_name(g.phase).to_string()).collect(),
            outlines: Corners::of(r.regions.iter().map(|g| g.outline.as_slice()))?,
            isotherm: Corners::of(r.isotherm.iter().map(Vec::as_slice))?,
            isobar: Corners::of(r.isobar.iter().map(Vec::as_slice))?,
            critical: r.critical.iter().flat_map(|c| [c.t, c.p, c.h, c.s, c.rho]).collect(),
            t_critical: r.t_critical,
            p_critical: r.p_critical,
        })
    }
}

#[wasm_bindgen]
impl Regions {
    /// Each region's phase, as CoolProp's `PhaseSI` names it: the library's at a state inside it.
    #[wasm_bindgen(getter)]
    pub fn phases(&self) -> Vec<String> {
        self.phases.clone()
    }

    /// Each region's outline, a run of corners a region, in `phases`' order.
    #[wasm_bindgen(getter)]
    pub fn outlines(&self) -> Corners {
        self.outlines.clone()
    }

    /// The critical isotherm, up in p through the critical point, broken where a cell failed.
    #[wasm_bindgen(getter)]
    pub fn isotherm(&self) -> Corners {
        self.isotherm.clone()
    }

    /// The critical isobar, up in T through the critical point, broken where a cell failed.
    #[wasm_bindgen(getter)]
    pub fn isobar(&self) -> Corners {
        self.isobar.clone()
    }

    /// The critical point's T, P, Hmass, Smass and Dmass, the library's state at (T_c, ρ_c); empty where it has none.
    #[wasm_bindgen(getter)]
    pub fn critical(&self) -> Vec<f64> {
        self.critical.clone()
    }

    /// The fluid's critical temperature, K, its constant.
    #[wasm_bindgen(getter, js_name = Tc)]
    pub fn t_critical(&self) -> f64 {
        self.t_critical
    }

    /// The fluid's critical pressure, Pa, its constant.
    #[wasm_bindgen(getter, js_name = pc)]
    pub fn p_critical(&self) -> f64 {
        self.p_critical
    }
}

/// A mixture's traced envelope as phasekit-diagram takes it (DG3), and how and why the trace ended.
fn trace(mixture: &Mixture) -> Result<(diagram::Trace, String, String), JsError> {
    let e = mixture.envelope(&EnvelopeOptions::new()).map_err(js)?;
    let (end, reason) = (end_name(&e.end).to_string(), end_reason(&e.end));
    let m = mixture.info().molar_mass();
    let rho = e.rho_bulk.iter().map(|rho| rho * m).collect();
    let q = e.side.iter().map(|side| if *side == Side::Bubble { 0.0 } else { 1.0 }).collect();
    let stop = match e.end {
        phasekit_mix::EnvelopeEnd::Bound(d) => Some(phasekit_core::Error::Domain(d)),
        phasekit_mix::EnvelopeEnd::NoConvergence(error) => Some(error),
        _ => None,
    };
    Ok((diagram::Trace { t: e.t, p: e.p, rho, q, critical: e.critical, end: stop }, end, reason))
}

#[wasm_bindgen]
impl Fluid {
    /// Its diagram on `plane` (`ph`, `ts`, `pt`, `trho` or `hs`; PLAN.md M19.6): a pure fluid's dome and isolines, by
    /// its batch in lanes (`ExecPolicy::Simd`), the default flash options and the vertices bit for bit `evaluate`'s; or
    /// a mixture's phase envelope on `pt` and `trho`, traced as `envelope()` traces it, the other planes refused.
    pub fn diagram(&self, plane: &str) -> Result<Diagram, JsError> {
        alive()?;
        let unknown = || js(format!("`{plane}` is no plane: ph, ts, pt, trho or hs"));
        let plane = Plane::from_id(plane).ok_or_else(unknown)?;
        if let Some(mixture) = mixture_of(&self.inner) {
            let (trace, end, reason) = trace(mixture)?;
            return Diagram::of(&diagram::envelope(&self.inner, &trace, plane).map_err(js)?, end, reason);
        }
        let batch = FluidBatch::new(&self.inner).with_exec(ExecPolicy::Simd);
        let d = diagram::sample(&batch, plane, &Limits::of(&self.inner).map_err(js)?).map_err(js)?;
        Diagram::of(&d, String::new(), String::new())
    }

    /// Its phase regions (PLAN.md M19.5): polygons whose corners carry every plane's axes, each region's phase the
    /// library's at a state inside it, and the critical isotherm and isobar. A mixture has none and is refused.
    pub fn regions(&self) -> Result<Regions, JsError> {
        alive()?;
        if mixture_of(&self.inner).is_some() {
            return Err(js(format!("{} is a mixture: its phase regions are not drawn", self.name())));
        }
        Regions::of(&diagram::regions(&self.inner).map_err(js)?)
    }
}
