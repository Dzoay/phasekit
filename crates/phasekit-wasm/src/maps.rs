//! A property map (PLAN.md M26.6; user decision DG11): any output over one of the planes, phasekit-diagram's contour
//! lines and its bands' polygons, as typed arrays, so that a page fills the bands, draws the lines over them and
//! samples nothing itself.

use phasekit_compat::{self as compat, Output};
use phasekit_core::batch::ExecPolicy;
use phasekit_diagram::{self as diagram, FluidBatch, Limits, Plane, Quantity, Vertex};
use wasm_bindgen::prelude::*;

use crate::diagram::{Vertices, offset};
use crate::{Fluid, alive, js, mixture_of, refuse};

/// Runs of vertices, as flat arrays: run `k`'s vertices are those of `vertices` from `starts[k]` to `starts[k + 1]`.
#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq)]
pub struct Runs {
    starts: Vec<u32>,
    vertices: Vertices,
}

impl Default for Runs {
    fn default() -> Self {
        Runs { starts: vec![0], vertices: Vertices::default() }
    }
}

impl Runs {
    fn push(&mut self, run: &[Vertex]) -> Result<(), JsError> {
        run.iter().for_each(|v| self.vertices.push(v));
        self.starts.push(offset(self.vertices.length())?);
        Ok(())
    }

    /// The number of runs.
    fn count(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }
}

#[wasm_bindgen]
impl Runs {
    /// Each run's first vertex, and one past the last's: one more entry than runs.
    #[wasm_bindgen(getter)]
    pub fn starts(&self) -> Vec<u32> {
        self.starts.clone()
    }

    /// Every run's vertices, in order.
    #[wasm_bindgen(getter)]
    pub fn vertices(&self) -> Vertices {
        self.vertices.clone()
    }
}

/// A quantity's map over a plane, as flat arrays: level `i` is `levels[i]`, its contour lines the runs of `lines` from
/// `contours[i]` to `contours[i + 1]`, each from its end at the lower first input of the plane's pair, a closed one
/// returning to its first vertex; band `j` holds the states from `low[j]` up to `high[j]` (NaN where it is unbounded),
/// its polygons the runs of `polygons` from `bands[j]` to `bands[j + 1]`, each counter-clockwise in the plane's inputs.
#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyMap {
    plane: String,
    key: String,
    values: Vec<f64>,
    x: Vec<f64>,
    y: Vec<f64>,
    levels: Vec<f64>,
    contours: Vec<u32>,
    lines: Runs,
    low: Vec<f64>,
    high: Vec<f64>,
    bands: Vec<u32>,
    polygons: Runs,
}

impl PropertyMap {
    fn of(m: &diagram::Map, key: &str) -> Result<PropertyMap, JsError> {
        let mut out = PropertyMap {
            plane: m.plane.id().to_string(),
            key: key.to_string(),
            values: vec![m.values.0, m.values.1],
            x: vec![m.x.0, m.x.1],
            y: vec![m.y.0, m.y.1],
            levels: Vec::new(),
            contours: vec![0],
            lines: Runs::default(),
            low: Vec::new(),
            high: Vec::new(),
            bands: vec![0],
            polygons: Runs::default(),
        };
        for contour in &m.contours {
            out.levels.push(contour.level);
            contour.lines.iter().try_for_each(|line| out.lines.push(line))?;
            out.contours.push(offset(out.lines.count())?);
        }
        for band in &m.bands {
            out.low.push(band.low.unwrap_or(f64::NAN));
            out.high.push(band.high.unwrap_or(f64::NAN));
            band.polygons.iter().try_for_each(|polygon| out.polygons.push(polygon))?;
            out.bands.push(offset(out.polygons.count())?);
        }
        Ok(out)
    }
}

#[wasm_bindgen]
impl PropertyMap {
    /// Its plane's id.
    #[wasm_bindgen(getter)]
    pub fn plane(&self) -> String {
        self.plane.clone()
    }

    /// The mapped output's key, as the call named it.
    #[wasm_bindgen(getter)]
    pub fn key(&self) -> String {
        self.key.clone()
    }

    /// The least and greatest of the output over the map's grid.
    #[wasm_bindgen(getter)]
    pub fn values(&self) -> Vec<f64> {
        self.values.clone()
    }

    /// The horizontal axis's span, its diagram's.
    #[wasm_bindgen(getter)]
    pub fn x(&self) -> Vec<f64> {
        self.x.clone()
    }

    /// The vertical axis's span, its diagram's.
    #[wasm_bindgen(getter)]
    pub fn y(&self) -> Vec<f64> {
        self.y.clone()
    }

    /// The contours' levels, rising.
    #[wasm_bindgen(getter)]
    pub fn levels(&self) -> Vec<f64> {
        self.levels.clone()
    }

    /// Each level's first line in `lines`, and one past the last's: one more entry than levels.
    #[wasm_bindgen(getter)]
    pub fn contours(&self) -> Vec<u32> {
        self.contours.clone()
    }

    /// Every contour's lines.
    #[wasm_bindgen(getter)]
    pub fn lines(&self) -> Runs {
        self.lines.clone()
    }

    /// Each band's lower level; NaN for the band below the lowest.
    #[wasm_bindgen(getter)]
    pub fn low(&self) -> Vec<f64> {
        self.low.clone()
    }

    /// Each band's upper level; NaN for the band above the highest.
    #[wasm_bindgen(getter)]
    pub fn high(&self) -> Vec<f64> {
        self.high.clone()
    }

    /// Each band's first polygon in `polygons`, and one past the last's: one more entry than bands.
    #[wasm_bindgen(getter)]
    pub fn bands(&self) -> Vec<u32> {
        self.bands.clone()
    }

    /// Every band's polygons.
    #[wasm_bindgen(getter)]
    pub fn polygons(&self) -> Runs {
        self.polygons.clone()
    }
}

/// The quantity an output's property is: a plane's or a curve's own where one is, else any output.
fn quantity_of(prop: phasekit_core::Prop) -> Quantity {
    let named = [
        Quantity::T,
        Quantity::P,
        Quantity::Hmass,
        Quantity::Smass,
        Quantity::Dmass,
        Quantity::Q,
        Quantity::Umass,
        Quantity::Z,
    ];
    named.into_iter().find(|q| q.prop() == prop).unwrap_or(Quantity::Output(prop))
}

#[wasm_bindgen]
impl Fluid {
    /// Its map of the output `output` (a `PropsSI` output key) on `plane` (`ph`, `ts`, `pt`, `trho` or `hs`; PLAN.md
    /// M26.4, M26.6): phasekit-diagram's, by its batch in lanes (`ExecPolicy::Simd`) and the default flash options. A
    /// mixture has none and is refused, as are an unknown plane or output and the phase, which is no quantity.
    pub fn map(&self, plane: &str, output: &str) -> Result<PropertyMap, JsValue> {
        alive()?;
        let unknown = || js(format!("`{plane}` is no plane: ph, ts, pt, trho or hs"));
        let id = Plane::from_id(plane).ok_or_else(unknown)?;
        let prop = match compat::parse_output(output) {
            Some(Output::Prop(prop)) => prop,
            _ => return Err(js(format!("`{output}` is no output a map can show")).into()),
        };
        if mixture_of(&self.inner).is_some() {
            return Err(js(format!("{} is a mixture: its maps are not drawn", self.name())).into());
        }
        let batch = FluidBatch::new(&self.inner).with_exec(ExecPolicy::Simd);
        let limits = Limits::of(&self.inner).map_err(refuse)?;
        let m = diagram::sample_map(&batch, id, quantity_of(prop), &limits).map_err(refuse)?;
        Ok(PropertyMap::of(&m, output)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::key;
    use phasekit_core::Prop;

    /// A plane's quantity stands for its own property, so that its levels are its nice values; any other is an output.
    #[test]
    fn a_planes_property_is_its_quantity() {
        assert_eq!(quantity_of(Prop::P), Quantity::P);
        assert_eq!(quantity_of(Prop::Dmass), Quantity::Dmass);
        assert_eq!(quantity_of(Prop::SpeedOfSound), Quantity::Output(Prop::SpeedOfSound));
        assert_eq!(key(quantity_of(Prop::Hmass)), "Hmass");
    }
}
