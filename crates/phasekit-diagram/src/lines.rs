//! The isolines (PLAN.md M19.2, the site's `lines.ts` of M21.2): each family's lines on fixed grids of their sweep,
//! split at the dome and at failed cells. A line meets the dome at saturation states the batch computes (`QT`, `PQ`,
//! `QS`, `HQ`, `DQ`), its two-phase part between them; a failed cell breaks it.

use phasekit_core::Error;

use crate::batch::{Batch, Row, Vertex, along, vertex};
use crate::plane::{Family, Plane, Quantity};
use crate::ranges::{lin_grid, values};

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

/// Each family's lines on `plane`: `sweeps` gives each swept quantity's grid, `spans` each held quantity's span, and
/// `temperatures` the dome's, which the quality lines run across.
pub(crate) fn lines(
    batch: &impl Batch,
    plane: Plane,
    spans: &impl Fn(Quantity) -> (f64, f64),
    sweeps: &impl Fn(Quantity) -> Vec<f64>,
    temperatures: &[f64],
) -> Result<Vec<Line>, Error> {
    let mut out = Vec::new();
    for &family in plane.families() {
        for value in values(family.held, spans(family.held)) {
            let segments = match family.held {
                Quantity::Q => held_q(batch, plane, value, temperatures)?,
                Quantity::T | Quantity::P => held_tp(batch, plane, family, value, &sweeps(family.sweep))?,
                _ => held_other(batch, plane, family, value, &sweeps(family.sweep))?,
            };
            out.push(Line { held: family.held, value, segments });
        }
    }
    Ok(out)
}

/// The qualities a two-phase part of an isotherm or isobar takes, from 0 to 1.
const QUALITIES: usize = 11;

/// The row's vertex on one side of the dome or inside it; a failed cell where it has none.
fn item(row: &Row, two_phase: bool) -> Item {
    vertex(row).map_or(Item::Gap, |vertex| Item::Vertex { vertex, two_phase })
}

/// A line of constant T or p: one-phase on either side of the dome, and inside it the two-phase part from `QT` or
/// `PQ`, which needs both its ends, the saturation states at Q = 0 and 1 with their sweep values; a quality the batch
/// fails between them is a gap inside the dome. Above the critical point, or without either end, the line crosses
/// nothing. A pseudo-pure fluid's line meets the dome at its dew and bubble states, apart, and the batch refuses the
/// qualities between (user decision 4), so it has no two-phase part.
fn held_tp(batch: &impl Batch, plane: Plane, f: Family, v: f64, sweep: &[f64]) -> Result<Vec<Segment>, Error> {
    let (x, y) = (plane.x().quantity, plane.y().quantity);
    let swept = along(batch, &[x, y], (f.held, &vec![v; sweep.len()]), (f.sweep, sweep))?;
    let qualities = lin_grid(0.0, 1.0, QUALITIES);
    let mut sat = along(batch, &[x, y, f.sweep], (f.held, &[v; QUALITIES]), (Quantity::Q, &qualities))?;
    if f.held == Quantity::T {
        sat.reverse();
    }
    let Some((entry, exit)) = sat.first().and_then(swept_at).zip(sat.last().and_then(swept_at)) else {
        return Ok(segments(swept.iter().map(|r| item(r, false))));
    };
    let side = |keep: fn(f64, f64, f64) -> bool| {
        let kept = move |(_, s): &(&Row, &f64)| keep(**s, entry, exit);
        swept.iter().zip(sweep).filter(kept).map(|(r, _)| item(r, false))
    };
    let before = side(|s, entry, _| s < entry);
    let after = side(|s, _, exit| s > exit);
    Ok(segments(before.chain(inside(&sat)).chain(after)))
}

/// The sweep value of a saturation state, its third cell, where the row is whole.
fn swept_at(row: &Row) -> Option<f64> {
    row.cells.get(2).copied().filter(|_| row.ok.iter().all(|&ok| ok))
}

/// The two-phase part in the sweep's order, its ends on the dome's edge: an isotherm, swept up in p, enters from the
/// vapour (its rows Q from 1 to 0); an isobar, swept up in T, from the liquid (Q from 0 to 1).
fn inside(sat: &[Row]) -> Vec<Item> {
    let last = sat.len().saturating_sub(1);
    let at = |(i, row): (usize, &Row)| match vertex(row) {
        Some(v) if i == 0 || i == last => Item::Edge(v),
        _ => item(row, true),
    };
    sat.iter().enumerate().map(at).collect()
}

/// A line of constant s, h or ρ: the batch's states along the sweep, two-phase where the batch defines Q, with the
/// saturation states at its value (`QS`, `HQ`, `DQ`) where it meets the dome, each placed by its own sweep value.
fn held_other(batch: &impl Batch, plane: Plane, f: Family, v: f64, sweep: &[f64]) -> Result<Vec<Segment>, Error> {
    let (x, y) = (plane.x().quantity, plane.y().quantity);
    let swept = along(batch, &[x, y, Quantity::Q], (f.held, &vec![v; sweep.len()]), (f.sweep, sweep))?;
    let sat = along(batch, &[x, y, f.sweep], (f.held, &[v, v]), (Quantity::Q, &[0.0, 1.0]))?;
    let defined = |r: &Row| r.ok.get(2) == Some(&true);
    let mut items: Vec<(f64, Item)> = swept.iter().zip(sweep).map(|(r, &at)| (at, item(r, defined(r)))).collect();
    let edge = |r: &Row| Some((r.cells.get(2).copied().filter(|_| defined(r))?, Item::Edge(vertex(r)?)));
    items.extend(sat.iter().filter_map(edge));
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(segments(items.into_iter().map(|(_, item)| item)))
}

/// A line of constant quality, across the dome's temperatures.
fn held_q(batch: &impl Batch, plane: Plane, v: f64, temperatures: &[f64]) -> Result<Vec<Segment>, Error> {
    let (x, y) = (plane.x().quantity, plane.y().quantity);
    let found = along(batch, &[x, y], (Quantity::T, temperatures), (Quantity::Q, &vec![v; temperatures.len()]))?;
    Ok(segments(found.iter().map(|r| item(r, true))))
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
