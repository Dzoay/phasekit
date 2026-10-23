//! A diagram: its plane's saturation dome, its isolines and the ranges of its axes (PLAN.md M19.1, M19.2), sampled by
//! the batch and refined where they bend (M19.3). It adds no numerics: every vertex is the batch's value at the inputs
//! it carries.

use phasekit_core::{Error, Fluid};

use crate::batch::{Batch, FluidBatch, Row, Vertex, along, vertex};
use crate::lines::{Line, lines};
use crate::plane::{Plane, Quantity};
use crate::ranges::{Limits, Ranges, dome_temperatures, lin_grid, log_grid, ranges};
use crate::refine::{Halving, Sides, Square, points, refine};

/// The saturated liquid and vapour, each up to just below T_c, in order of temperature, and the critical point. A
/// pseudo-pure fluid's liquid side is its bubble points, its vapour side its dew points.
#[derive(Clone, Debug, PartialEq)]
pub struct Dome {
    /// The saturated liquid, Q = 0.
    pub liquid: Vec<Vertex>,
    /// The saturated vapour, Q = 1.
    pub vapour: Vec<Vertex>,
    /// The critical point, the state at (T_c, ρ_c); `None` where the batch refused it.
    pub critical: Option<Vertex>,
}

/// A plane's geometry for one fluid.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagram {
    /// The plane.
    pub plane: Plane,
    /// The saturation dome.
    pub dome: Dome,
    /// Each family's lines, family by family in the plane's order, each family's in order of value.
    pub lines: Vec<Line>,
    /// The span of the horizontal axis.
    pub x: (f64, f64),
    /// The span of the vertical axis.
    pub y: (f64, f64),
}

/// The fluid's diagram on `plane`, by its own batch under the default options.
pub fn diagram(fluid: &Fluid, plane: Plane) -> Result<Diagram, Error> {
    sample(&FluidBatch::new(fluid), plane, &Limits::of(fluid)?)
}

/// The diagram on `plane` of the fluid whose batch and limits these are.
pub fn sample(batch: &impl Batch, plane: Plane, limits: &Limits) -> Result<Diagram, Error> {
    let r = ranges(limits);
    let outputs = outputs(plane);
    let DomeSides { liquid, vapour, temperatures, spans } = dome_sides(batch, plane, limits)?;
    let (t_c, rho_c) = ([limits.t_critical], [limits.rho_critical]);
    let critical = along(batch, &outputs, (Quantity::T, &t_c), (Quantity::Dmass, &rho_c))?;
    let dome =
        Dome { liquid: vertices(&liquid), vapour: vertices(&vapour), critical: critical.first().and_then(vertex) };
    let sweeps = |q: Quantity| match q {
        Quantity::T => lin_grid(r.t.0, r.t.1, SWEEP_POINTS),
        _ => log_grid(r.p.0, r.p.1, SWEEP_POINTS),
    };
    let lines = lines(batch, plane, &spans.square(plane), &|q| spans.get(q), &sweeps, &temperatures)?;
    Ok(Diagram { plane, dome, lines, x: spans.get(plane.x().quantity), y: spans.get(plane.y().quantity) })
}

/// The spans of the plane's axes, as its diagram's (`Diagram::x` and `Diagram::y`), without its lines.
pub(crate) fn axes_spans(batch: &impl Batch, plane: Plane, limits: &Limits) -> Result<[(f64, f64); 2], Error> {
    let spans = dome_sides(batch, plane, limits)?.spans;
    Ok([spans.get(plane.x().quantity), spans.get(plane.y().quantity)])
}

/// The batch's outputs on a plane: its x and y, then h, s and ρ for the spans.
fn outputs(plane: Plane) -> [Quantity; 5] {
    [plane.x().quantity, plane.y().quantity, Quantity::Hmass, Quantity::Smass, Quantity::Dmass]
}

/// The dome's sides on a plane, refined where they bend on it, the temperatures they start on, and the spans of the
/// axes over them and the plotted range's corners.
struct DomeSides {
    liquid: Vec<Row>,
    vapour: Vec<Row>,
    temperatures: Vec<f64>,
    spans: Spans,
}

fn dome_sides(batch: &impl Batch, plane: Plane, limits: &Limits) -> Result<DomeSides, Error> {
    let r = ranges(limits);
    let outputs = outputs(plane);
    let temperatures = dome_temperatures(dome_start(batch, &r)?, limits.t_critical, DOME_POINTS);
    let side = |q: f64| move |t: &[f64]| along(batch, &outputs, (Quantity::T, t), (Quantity::Q, &vec![q; t.len()]));
    let (liquid, vapour) = (side(0.0)(&temperatures)?, side(1.0)(&temperatures)?);
    let (t, p) = ([r.t.0, r.t.0, r.t.1, r.t.1], [r.p.0, r.p.1, r.p.0, r.p.1]);
    let corners = along(batch, &outputs, (Quantity::T, &t), (Quantity::P, &p))?;
    let coarse = Spans::of(r, &[&liquid, &vapour, &corners]).square(plane);
    let refined = |rows: Vec<Row>, q: f64| -> Result<Vec<Row>, Error> {
        let all = refine(points(&temperatures, rows), (Halving::Linear, Sides::One), &coarse, side(q))?;
        Ok(all.into_iter().map(|p| p.row).collect())
    };
    let (liquid, vapour) = (refined(liquid, 0.0)?, refined(vapour, 1.0)?);
    let spans = Spans::of(r, &[&liquid, &vapour, &corners]);
    Ok(DomeSides { liquid, vapour, temperatures, spans })
}

/// The temperatures a side of the dome starts on, which the quality lines start on too: closing in on T_c, they reach
/// within 0.03 K of Water's (32 would stop 0.37 K short).
const DOME_POINTS: usize = 120;

/// The points a line's sweep starts on, linear in T and logarithmic in p, across the plotted range.
const SWEEP_POINTS: usize = 32;

/// Where the dome enters the plotted pressures: at the saturation temperature of the lowest, or at the lowest plotted
/// temperature if that is higher or the batch has no such state.
fn dome_start(batch: &impl Batch, r: &Ranges) -> Result<f64, Error> {
    let lowest = along(batch, &[Quantity::T], (Quantity::P, &[r.p.0]), (Quantity::Q, &[0.0]))?;
    let t = lowest.first().filter(|row| row.ok.first() == Some(&true)).and_then(|row| row.cells.first().copied());
    Ok(t.map_or(r.t.0, |t| r.t.0.max(t)))
}

fn vertices(rows: &[Row]) -> Vec<Vertex> {
    rows.iter().filter_map(vertex).collect()
}

/// The span of each quantity an axis can plot: of T and p the plotted range, of Q [0, 1], and of h, s and ρ the least
/// and greatest values the dome and the range's corners gave, cells 2, 3 and 4 of their rows; of u and the curves'
/// outputs, which no plane plots, none.
struct Spans {
    ranges: Ranges,
    h: (f64, f64),
    s: (f64, f64),
    d: (f64, f64),
}

impl Spans {
    fn of(ranges: Ranges, rows: &[&[Row]]) -> Spans {
        Spans { ranges, h: extent(rows, 2), s: extent(rows, 3), d: extent(rows, 4) }
    }

    /// The plane's unit square over these spans, where its lines are refined.
    fn square(&self, plane: Plane) -> Square {
        let (x, y) = (plane.x(), plane.y());
        Square::new((x, self.get(x.quantity)), (y, self.get(y.quantity)))
    }

    fn get(&self, quantity: Quantity) -> (f64, f64) {
        match quantity {
            Quantity::T => self.ranges.t,
            Quantity::P => self.ranges.p,
            Quantity::Q => (0.0, 1.0),
            Quantity::Hmass => self.h,
            Quantity::Smass => self.s,
            Quantity::Dmass => self.d,
            Quantity::Umass
            | Quantity::Z
            | Quantity::Bvirial
            | Quantity::DBvirialDT
            | Quantity::DpDrhoAtT
            | Quantity::DpDTAtRho
            | Quantity::JouleThomson
            | Quantity::Output(_) => (f64::INFINITY, f64::NEG_INFINITY),
        }
    }
}

/// The least and greatest of cell `k` over the rows where it is a value, whatever became of the others.
pub(crate) fn extent(rows: &[&[Row]], k: usize) -> (f64, f64) {
    let value = |row: &Row| match (row.cells.get(k), row.ok.get(k)) {
        (Some(&v), Some(true)) => Some(v),
        _ => None,
    };
    let all = rows.iter().flat_map(|r| r.iter()).filter_map(value);
    all.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use phasekit_core::Pair;

    /// The site's extents (M21.2): each key's values where the batch gave them, whatever became of x and y.
    #[test]
    fn extents_take_each_cell_where_it_is_a_value() {
        let inputs = crate::batch::Inputs { pair: Pair::PT, x: 1.0, y: 1.0 };
        let row = |cells: [f64; 5], ok: [bool; 5]| Row { cells: cells.to_vec(), ok: ok.to_vec(), inputs };
        let rows = [
            row([0.0, 0.0, 10.0, 20.0, 30.0], [false, false, true, true, true]),
            row([0.0, 0.0, 1.0, 2.0, 3.0], [true; 5]),
            row([0.0, 0.0, -5.0, -5.0, -5.0], [true, true, false, false, false]),
        ];
        let ranges = Ranges { t: (1.0, 2.0), p: (3.0, 4.0) };
        let spans = Spans::of(ranges, &[&rows]);
        let got: Vec<_> = [Quantity::Hmass, Quantity::Smass, Quantity::Dmass, Quantity::T, Quantity::P, Quantity::Q]
            .into_iter()
            .map(|q| spans.get(q))
            .collect();
        assert_eq!(got, [(1.0, 10.0), (2.0, 20.0), (3.0, 30.0), (1.0, 2.0), (3.0, 4.0), (0.0, 1.0)]);
        assert_eq!(spans.get(Quantity::Umass), (f64::INFINITY, f64::NEG_INFINITY));
    }
}
