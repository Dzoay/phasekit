//! The isolines (PLAN.md M19.2, the site's `lines.ts` of M21.2): each family's lines from a grid of their sweep,
//! refined where they bend (M19.3), split at the dome and at failed cells. A line meets the dome at saturation states
//! the batch computes (`QT`, `PQ`, `QS`, `HQ`, `DQ`), its two-phase part between them; a failed cell breaks it.

use phasekit_core::Error;

use crate::batch::{Batch, Row, Vertex, along, vertex};
use crate::plane::{Plane, Quantity};
use crate::ranges::{lin_grid, values};
use crate::refine::{Halving, Sides, Square, Swept, points, refine};

/// A line of constant `held`, in segments.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// The quantity the line holds, which its label names.
    pub held: Quantity,
    /// Its value, the label's.
    pub value: f64,
    /// The line's runs, in the order of its sweep; a gap or the dome's edge between them.
    pub segments: Vec<Segment>,
}

/// A run of a line on one side of the dome or inside it, at least two vertices long.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    /// Whether it runs inside the dome.
    pub two_phase: bool,
    /// Its vertices, in the order of the line's sweep.
    pub vertices: Vec<Vertex>,
}

/// A point along a line, in its sweep's order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Item {
    /// A vertex on one side of the dome, or inside it.
    Vertex { vertex: Vertex, two_phase: bool },
    /// A saturation state on the dome's edge, which ends one segment and starts the next.
    Edge(Vertex),
    /// A failed cell.
    Gap,
}

/// Segments from items: a failed cell ends one, a change of side starts one, and an edge vertex ends one and starts
/// the next, so a line runs on through the dome's edge. A lone vertex draws nothing, so it is dropped.
pub(crate) fn segments(items: impl IntoIterator<Item = Item>) -> Vec<Segment> {
    let mut builder = Builder::default();
    items.into_iter().for_each(|item| builder.push(item));
    builder.close();
    builder.out
}

/// The segments so far, and the one being built: its side, once a vertex has set it, and its vertices.
#[derive(Default)]
struct Builder {
    out: Vec<Segment>,
    side: Option<bool>,
    vertices: Vec<Vertex>,
}

impl Builder {
    fn push(&mut self, item: Item) {
        match item {
            Item::Gap => self.close(),
            Item::Edge(vertex) => {
                self.vertices.push(vertex);
                self.close();
                self.vertices.push(vertex);
            }
            Item::Vertex { vertex, two_phase } => {
                if self.side.is_some_and(|side| side != two_phase) {
                    self.close();
                }
                self.side = Some(two_phase);
                self.vertices.push(vertex);
            }
        }
    }

    /// Ends the segment being built, kept if it has two vertices or more.
    fn close(&mut self) {
        let vertices = std::mem::take(&mut self.vertices);
        if vertices.len() > 1 {
            self.out.push(Segment { two_phase: self.side.unwrap_or(false), vertices });
        }
        self.side = None;
    }
}

/// Each family's lines on `plane`, refined in its unit square `square`: `sweeps` gives each swept quantity's starting
/// grid, `spans` each held quantity's span, and `temperatures` the dome's, on which the quality lines start.
pub(crate) fn lines(
    batch: &impl Batch,
    plane: Plane,
    square: &Square,
    spans: &impl Fn(Quantity) -> (f64, f64),
    sweeps: &impl Fn(Quantity) -> Vec<f64>,
    temperatures: &[f64],
) -> Result<Vec<Line>, Error> {
    let mut out = Vec::new();
    for &family in plane.families() {
        for value in values(family.held, spans(family.held)) {
            let on = On { batch, plane, square, held: family.held, value };
            let segments = match family.held {
                Quantity::Q => on.quality(temperatures)?,
                Quantity::T | Quantity::P => on.crossing(family.sweep, &sweeps(family.sweep))?,
                _ => on.swept(family.sweep, &sweeps(family.sweep))?,
            };
            out.push(Line { held: family.held, value, segments });
        }
    }
    Ok(out)
}

/// The qualities a two-phase part of an isotherm or isobar starts on, from 0 to 1.
const QUALITIES: usize = 32;

/// The row's vertex on one side of the dome or inside it; a failed cell where it has none.
fn item(row: &Row, two_phase: bool) -> Item {
    vertex(row).map_or(Item::Gap, |vertex| Item::Vertex { vertex, two_phase })
}

/// How a quantity's sweep is halved, p in its logarithm as its grid runs, and whether its points lie on either side
/// of the dome.
fn rule(sweep: Quantity, sides: Sides) -> (Halving, Sides) {
    (if sweep == Quantity::P { Halving::Geometric } else { Halving::Linear }, sides)
}

/// A line being sampled: the batch, the plane and its unit square, and the quantity the line holds at `value`.
struct On<'a, B> {
    batch: &'a B,
    plane: Plane,
    square: &'a Square,
    held: Quantity,
    value: f64,
}

impl<B: Batch> On<'_, B> {
    /// Rows of the plane's axes, and of `extra`, at the line's states where `sweep` takes the values `at`.
    fn rows(&self, extra: Option<Quantity>, sweep: Quantity, at: &[f64]) -> Result<Vec<Row>, Error> {
        let outputs: Vec<Quantity> =
            [self.plane.x().quantity, self.plane.y().quantity].into_iter().chain(extra).collect();
        along(self.batch, &outputs, (self.held, &vec![self.value; at.len()]), (sweep, at))
    }

    /// An isotherm or isobar: one-phase on either side of the dome, and inside it the two-phase part from `QT` or
    /// `PQ`, which needs both its ends, the saturation states at Q = 0 and 1 with their sweep values; a quality the
    /// batch fails between them is a gap inside the dome. Above the critical point, or without either end, the line
    /// crosses nothing. A pseudo-pure fluid's line meets the dome at its dew and bubble states, apart, and the batch
    /// refuses the qualities between (user decision 4), so it has no two-phase part. Each piece is refined on its own.
    fn crossing(&self, sweep: Quantity, grid: &[f64]) -> Result<Vec<Segment>, Error> {
        let swept_at = |at: &[f64]| self.rows(None, sweep, at);
        let sat_at = |q: &[f64]| self.rows(Some(sweep), Quantity::Q, q);
        let qualities = lin_grid(0.0, 1.0, QUALITIES);
        let all = points(grid, swept_at(grid)?);
        let inside = points(&qualities, sat_at(&qualities)?);
        let from_vapour = self.held == Quantity::T;
        let Some((entry, exit)) = ends(&inside, from_vapour) else {
            return Ok(segments(one_phase(&refine(all, rule(sweep, Sides::One), self.square, swept_at)?)));
        };
        let (before, after) = sides(&all, entry, exit);
        let before = refine(before, rule(sweep, Sides::One), self.square, swept_at)?;
        let after = refine(after, rule(sweep, Sides::One), self.square, swept_at)?;
        let mut inside = refine(inside, (Halving::Linear, Sides::One), self.square, sat_at)?;
        if from_vapour {
            inside.reverse();
        }
        Ok(segments(one_phase(&before).chain(edged(&inside)).chain(one_phase(&after))))
    }

    /// An isentrope, isenthalp or isochore: the batch's states along the sweep, two-phase where the batch defines Q,
    /// with the saturation states at its value (`QS`, `HQ`, `DQ`) where it meets the dome, each placed by its own
    /// sweep value, all refined together, a change of side closed in on.
    fn swept(&self, sweep: Quantity, grid: &[f64]) -> Result<Vec<Segment>, Error> {
        let swept_at = |at: &[f64]| self.rows(Some(Quantity::Q), sweep, at);
        let sat = self.rows(Some(sweep), Quantity::Q, &[0.0, 1.0])?;
        let mut all = points(grid, swept_at(grid)?);
        all.extend(sat.iter().filter_map(edge));
        Ok(segments(refine(all, rule(sweep, Sides::ByQuality), self.square, swept_at)?.iter().map(swept_item)))
    }

    /// A line of constant quality, starting on the dome's temperatures.
    fn quality(&self, temperatures: &[f64]) -> Result<Vec<Segment>, Error> {
        let at = |t: &[f64]| self.rows(None, Quantity::T, t);
        let all = refine(points(temperatures, at(temperatures)?), (Halving::Linear, Sides::One), self.square, at)?;
        Ok(segments(all.iter().map(|p| item(&p.row, true))))
    }
}

/// A saturation state as an edge point at its sweep value, its third cell; `None` unless the row is whole.
fn edge(row: &Row) -> Option<Swept> {
    let at = row.cells.get(2).copied().filter(|_| row.ok.iter().all(|&ok| ok))?;
    Some(Swept { at, row: row.clone(), edge: true })
}

/// The two-phase part's ends in the sweep's order, as edge points: an isotherm, swept up in p, enters from the vapour
/// (Q = 1) and leaves to the liquid; an isobar, swept up in T, the other way. `None` without either.
fn ends(inside: &[Swept], from_vapour: bool) -> Option<(Swept, Swept)> {
    let (first, last) = (inside.first()?, inside.last()?);
    let (entry, exit) = if from_vapour { (last, first) } else { (first, last) };
    Some((edge(&entry.row)?, edge(&exit.row)?))
}

/// The one-phase pieces before the dome and after it, each ending on its edge.
fn sides(all: &[Swept], entry: Swept, exit: Swept) -> (Vec<Swept>, Vec<Swept>) {
    let (entry_at, exit_at) = (entry.at, exit.at);
    let before = all.iter().filter(|p| p.at < entry_at).cloned().chain([entry]).collect();
    let after = [exit].into_iter().chain(all.iter().filter(|p| p.at > exit_at).cloned()).collect();
    (before, after)
}

/// A one-phase piece's points as items, without its ends on the dome's edge, which the two-phase part gives.
fn one_phase(piece: &[Swept]) -> impl Iterator<Item = Item> + '_ {
    piece.iter().filter(|p| !p.edge).map(|p| item(&p.row, false))
}

/// The two-phase part in the sweep's order as items, its ends on the dome's edge.
fn edged(inside: &[Swept]) -> Vec<Item> {
    let last = inside.len().saturating_sub(1);
    let at = |(i, p): (usize, &Swept)| match vertex(&p.row) {
        Some(v) if i == 0 || i == last => Item::Edge(v),
        _ => item(&p.row, true),
    };
    inside.iter().enumerate().map(at).collect()
}

/// A point of an isentrope, isenthalp or isochore as an item: on the dome's edge, or two-phase where the batch
/// defines Q.
fn swept_item(p: &Swept) -> Item {
    match vertex(&p.row) {
        Some(v) if p.edge => Item::Edge(v),
        _ => item(&p.row, p.row.ok.get(2) == Some(&true)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::Inputs;
    use phasekit_core::Pair;

    fn at(n: f64) -> Vertex {
        Vertex { x: n, y: n, inputs: Inputs { pair: Pair::PT, x: n, y: n } }
    }
    fn one(n: f64) -> Item {
        Item::Vertex { vertex: at(n), two_phase: false }
    }
    fn two(n: f64) -> Item {
        Item::Vertex { vertex: at(n), two_phase: true }
    }
    fn edge(n: f64) -> Item {
        Item::Edge(at(n))
    }
    fn shape(items: Vec<Item>) -> Vec<(bool, Vec<f64>)> {
        segments(items).into_iter().map(|s| (s.two_phase, s.vertices.iter().map(|v| v.x).collect())).collect()
    }

    /// The site's segments (M21.2): a failed cell ends a segment, and a lone vertex draws nothing.
    #[test]
    fn a_failed_cell_ends_a_segment_and_a_lone_vertex_draws_nothing() {
        let items = vec![one(1.0), one(2.0), Item::Gap, one(3.0), Item::Gap, one(4.0), one(5.0)];
        assert_eq!(shape(items), [(false, vec![1.0, 2.0]), (false, vec![4.0, 5.0])]);
    }

    /// A change of side splits a line, and an edge vertex belongs to both sides.
    #[test]
    fn a_change_of_side_splits_a_line_and_an_edge_belongs_to_both_sides() {
        assert_eq!(
            shape(vec![one(1.0), one(2.0), two(3.0), two(4.0)]),
            [(false, vec![1.0, 2.0]), (true, vec![3.0, 4.0])]
        );
        let through = vec![one(1.0), edge(2.0), two(3.0), edge(4.0), one(5.0)];
        assert_eq!(shape(through), [(false, vec![1.0, 2.0]), (true, vec![2.0, 3.0, 4.0]), (false, vec![4.0, 5.0])]);
        assert_eq!(shape(vec![edge(1.0), edge(2.0)]), [(false, vec![1.0, 2.0])]);
    }
}
