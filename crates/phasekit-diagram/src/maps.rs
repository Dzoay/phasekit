//! A property map (PLAN.md M26.4; user decision DG11): any output of the batch over a plane, as contour lines at nice
//! values and the bands between them, each band as every grid cell's piece of it, a polygon. A grid of the plane's
//! inputs spans its diagram's axes; each contour vertex is found by bisection along a grid edge (`contour.rs`), and each
//! polygon's corners are the grid's states and those vertices. It adds no numerics: every vertex is the batch's state
//! at the inputs it carries.

#[cfg(test)]
use phasekit_core::math;
use phasekit_core::{Error, Fluid, Pair};

use crate::batch::{Batch, FluidBatch, Row, Vertex, vertex};
use crate::contour::{Axes, Bound, Crossings, Grid, Precision, band, crossings, ends, less};
use crate::diagram::{axes_spans, extent};
use crate::plane::{Axis, Plane, Quantity};
use crate::ranges::{Limits, lin_grid, log_grid, values};
use crate::refine::Halving;

/// A contour: the quantity's level and its lines, each from its end at the lower first input of the plane's pair.
#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    /// The level.
    pub level: f64,
    /// The lines; a closed line returns to its first vertex.
    pub lines: Vec<Vec<Vertex>>,
}

/// A band: the states whose quantity lies from `low` up to `high`, as polygons, each a grid cell's piece of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    /// Its lower level; `None` for the band below the lowest.
    pub low: Option<f64>,
    /// Its upper level; `None` for the band above the highest.
    pub high: Option<f64>,
    /// Its polygons, each counter-clockwise in the plane's inputs from its cell's lower left corner.
    pub polygons: Vec<Vec<Vertex>>,
}

/// A quantity's map over a plane.
#[derive(Clone, Debug, PartialEq)]
pub struct Map {
    /// The plane.
    pub plane: Plane,
    /// The mapped quantity.
    pub quantity: Quantity,
    /// The least and greatest of the quantity over the grid.
    pub values: (f64, f64),
    /// The contours, in order of level.
    pub contours: Vec<Contour>,
    /// The bands, one more than the contours, from the lowest up.
    pub bands: Vec<Band>,
    /// The span of the horizontal axis, its diagram's.
    pub x: (f64, f64),
    /// The span of the vertical axis, its diagram's.
    pub y: (f64, f64),
}

/// The fluid's map of `quantity` on `plane`, by its own batch under the default options.
pub fn map(fluid: &Fluid, plane: Plane, quantity: Quantity) -> Result<Map, Error> {
    sample_map(&FluidBatch::new(fluid), plane, quantity, &Limits::of(fluid)?)
}

/// The map of `quantity` on `plane` of the fluid whose batch and limits these are. One call takes a grid of `NODES`
/// values of each axis across its diagram's span, evenly in its scale; the levels are the quantity's nice values
/// strictly inside its span over the grid (M19's: 1-2-5 per decade for p and ρ, d3's ticks for the rest). A contour
/// crossing is bisected until it is within an eighth of `LEVEL` of the span of its level, and one farther than `LEVEL`
/// from it, on a jump in the quantity, is left out. A gap's edge is closed in on once for every level.
pub fn sample_map(batch: &impl Batch, plane: Plane, quantity: Quantity, limits: &Limits) -> Result<Map, Error> {
    let [x, y] = axes_spans(batch, plane, limits)?;
    let outputs = [plane.x().quantity, plane.y().quantity, quantity];
    let grid = Grid::sample(batch, &outputs, axes(plane)?, inputs(plane, x, y)?)?;
    let value = |row: &Row| cell(row, 2);
    let span = extent(&[&grid.rows], 2);
    let levels = values(quantity, span);
    let tolerance = LEVEL * (span.1 - span.0);
    let precision = Precision { stop: tolerance / 8.0, tolerance };
    let valued = ends(batch, &outputs, &grid, &value)?;
    let found = levels.iter().map(|&level| {
        let field = move |row: &Row| Some(value(row)? - level);
        crossings(batch, (&outputs, &grid), &field, &less(&valued, level), precision)
    });
    let found: Vec<Crossings> = found.collect::<Result<_, _>>()?;
    let contours = levels
        .iter()
        .zip(&found)
        .map(|(&level, c)| Contour { level, lines: c.lines().iter().map(|l| vertices(l)).collect() });
    let bounds: Vec<Bound> = levels.iter().copied().zip(&found).collect();
    let below = [None].into_iter().chain(bounds.iter().copied().map(Some));
    let above = bounds.iter().copied().map(Some).chain([None]);
    let bands = below.zip(above).map(|(low, high)| Band {
        low: low.map(|(l, _)| l),
        high: high.map(|(h, _)| h),
        polygons: band(&grid, &value, low, high).iter().filter_map(|p| whole(p)).collect(),
    });
    Ok(Map { plane, quantity, values: span, contours: contours.collect(), bands: bands.collect(), x, y })
}

/// The grid's values on each axis.
const NODES: usize = 64;

/// How near its level a contour vertex is, as a fraction of the quantity's span over the grid: bisection takes a
/// continuous quantity to within a few ulps of its input, and a crossing on a jump stays a jump's height away.
pub const LEVEL: f64 = 1e-6;

/// The plane's input pair, each input halved at the geometric mean on a logarithmic axis and the arithmetic one on a
/// linear axis.
fn axes(plane: Plane) -> Result<Axes, Error> {
    let (pair, swapped) = pair(plane)?;
    let halving = |axis: Axis| if axis.log { Halving::Geometric } else { Halving::Linear };
    let [hx, hy] = [plane.x(), plane.y()].map(halving);
    Ok(Axes { pair, halving: if swapped { [hy, hx] } else { [hx, hy] } })
}

/// The grid's values of the plane's inputs, in the pair's order: `NODES` across each axis's span, evenly in its scale.
fn inputs(plane: Plane, x: (f64, f64), y: (f64, f64)) -> Result<(Vec<f64>, Vec<f64>), Error> {
    let across = |axis: Axis, (a, b): (f64, f64)| if axis.log { log_grid(a, b, NODES) } else { lin_grid(a, b, NODES) };
    let (gx, gy) = (across(plane.x(), x), across(plane.y(), y));
    Ok(if pair(plane)?.1 { (gy, gx) } else { (gx, gy) })
}

/// The input pair of the plane's quantities, and whether its order is theirs swapped.
fn pair(plane: Plane) -> Result<(Pair, bool), Error> {
    let unpaired = Error::InvalidInput { quantity: "input pair", value: f64::NAN };
    let vars = plane.x().quantity.var().zip(plane.y().quantity.var());
    vars.and_then(|(x, y)| Pair::from_vars(x, y)).ok_or(unpaired)
}

/// The cell's value, where it is one.
fn cell(row: &Row, k: usize) -> Option<f64> {
    match (row.cells.get(k), row.ok.get(k)) {
        (Some(&v), Some(true)) => Some(v),
        _ => None,
    }
}

/// The rows' vertices, where they have them.
fn vertices(rows: &[Row]) -> Vec<Vertex> {
    rows.iter().filter_map(vertex).collect()
}

/// A polygon's vertices, where every row has its vertex.
fn whole(rows: &[Row]) -> Option<Vec<Vertex>> {
    rows.iter().map(vertex).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use phasekit_core::math;

    /// Each plane's grid is in its pair's order, each input halved on its axis's scale: p and ρ at the geometric mean,
    /// T, h and s at the arithmetic one.
    #[test]
    fn a_planes_grid_is_in_its_pairs_order_on_its_axes_scales() {
        let (geometric, linear) = (Halving::Geometric, Halving::Linear);
        let got: Vec<Axes> = Plane::ALL.into_iter().map(|p| axes(p).unwrap_or(Axes::DT)).collect();
        let want = [
            Axes { pair: Pair::HP, halving: [linear, geometric] },
            Axes { pair: Pair::ST, halving: [linear, linear] },
            Axes { pair: Pair::PT, halving: [geometric, linear] },
            Axes { pair: Pair::DT, halving: [geometric, linear] },
            Axes { pair: Pair::HS, halving: [linear, linear] },
        ];
        assert_eq!(got, want);
        let (x, y) = inputs(Plane::Pt, (300.0, 400.0), (1e5, 1e7)).unwrap_or_default();
        assert_eq!((x.first(), x.last(), y.first(), y.last()), (Some(&1e5), Some(&1e7), Some(&300.0), Some(&400.0)));
        assert_eq!(x.get(32), Some(&(1e5 * math::powf(100.0, 32.0 / 63.0))));
        let (h, p) = inputs(Plane::Ph, (0.0, 630.0), (1.0, 2.0)).unwrap_or_default();
        assert_eq!((h.get(1), p.len()), (Some(&10.0), NODES));
    }
}
