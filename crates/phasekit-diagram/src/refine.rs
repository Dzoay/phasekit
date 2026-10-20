//! Sampling where a line bends (PLAN.md M19.3). A piece of a line starts on its sweep's points, and an interval is
//! halved while its midpoint's vertex lies farther than [`TOLERANCE`] from the chord in the plane's unit square, at
//! most [`DEPTH`] times; one batch call takes a level's midpoints. An interval with one failed end is halved to the
//! same depth toward the failure, so a gap's edge lies within 2⁻⁶ of a starting interval of where the batch stops
//! answering. The rule is deterministic: the same points and batch give the same vertices.

use phasekit_core::{Error, math};

use crate::batch::{Row, vertex};
use crate::plane::Axis;

/// How far a midpoint may lie from its interval's chord in the plane's unit square: a pixel of a 1000-pixel plot.
const TOLERANCE: f64 = 1e-3;

/// The most times an interval is halved.
const DEPTH: usize = 6;

/// One axis's map onto [0, 1] over its span, a logarithmic axis's in its logarithm.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Scale {
    lo: f64,
    width: f64,
    log: bool,
}

impl Scale {
    fn new(axis: Axis, (lo, hi): (f64, f64)) -> Scale {
        let log = axis.log;
        let scaled = |v: f64| if log { math::ln(v) } else { v };
        Scale { lo: scaled(lo), width: scaled(hi) - scaled(lo), log }
    }

    fn unit(&self, v: f64) -> f64 {
        let v = if self.log { math::ln(v) } else { v };
        // A midpoint's distance from its chord is the same however the square is shifted, so `+ lo`, which shifts
        // every point of the axis by 2·lo/width, changes no vertex.
        // equivalent mutant: replace - with +
        (v - self.lo) / self.width
    }
}

/// A plane's unit square: each axis scaled to its span.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Square {
    x: Scale,
    y: Scale,
}

impl Square {
    /// The square of the axes `x` and `y` over their spans.
    pub(crate) fn new(x: (Axis, (f64, f64)), y: (Axis, (f64, f64))) -> Square {
        Square { x: Scale::new(x.0, x.1), y: Scale::new(y.0, y.1) }
    }

    /// The row's vertex in the square; `None` where the row has no vertex.
    fn at(&self, row: &Row) -> Option<(f64, f64)> {
        vertex(row).map(|v| (self.x.unit(v.x), self.y.unit(v.y)))
    }
}

/// A point of a piece of a line: its sweep value, its row, and whether it is a saturation state on the dome's edge.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Swept {
    pub at: f64,
    pub row: Row,
    pub edge: bool,
}

/// Points at the sweep values `at`, their rows `rows`, none an edge.
pub(crate) fn points(at: &[f64], rows: Vec<Row>) -> Vec<Swept> {
    at.iter().zip(rows).map(|(&at, row)| Swept { at, row, edge: false }).collect()
}

/// How a sweep is halved: at the arithmetic mean, or at the geometric one for a logarithmic sweep (p).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Halving {
    Linear,
    Geometric,
}

impl Halving {
    fn mid(self, a: f64, b: f64) -> f64 {
        match self {
            Halving::Linear => (a + b) / 2.0,
            Halving::Geometric => math::sqrt(a * b),
        }
    }
}

/// Whether a piece's points lie on either side of the dome: an isentrope's, isenthalp's or isochore's can, a point
/// two-phase where its third cell, Q, is a value, and an edge on both. Where two neighbours lie on different sides, the
/// change of side is closed in on as a gap's edge is: for a pseudo-pure fluid it need not fall on the saturation state
/// (user decision 4), and the segments either side of it share no vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sides {
    One,
    ByQuality,
}

impl Sides {
    /// The point's side, two-phase or not, where the piece has two and the point is no edge.
    fn of(self, p: &Swept) -> Option<bool> {
        match self {
            Sides::ByQuality if !p.edge => p.row.ok.get(2).copied(),
            _ => None,
        }
    }

    /// Whether the points lie on one side, those with a side.
    fn one(self, points: [&Swept; 3]) -> bool {
        let mut sides = points.into_iter().filter_map(|p| self.of(p));
        sides.next().is_none_or(|first| sides.all(|side| side == first))
    }
}

/// An interval to examine, its ends in sweep order.
type Interval = (Swept, Swept);

/// The piece's points in sweep order, with the midpoints refinement kept: `eval` gives the rows at sweep values.
pub(crate) fn refine(
    start: Vec<Swept>,
    (halving, sides): (Halving, Sides),
    square: &Square,
    eval: impl Fn(&[f64]) -> Result<Vec<Row>, Error>,
) -> Result<Vec<Swept>, Error> {
    let mut all = start;
    all.sort_by(|p, q| p.at.total_cmp(&q.at));
    let mut open: Vec<Interval> = all.windows(2).filter_map(interval).filter(|i| any_vertex(square, i)).collect();
    for _ in 0..DEPTH {
        if open.is_empty() {
            break;
        }
        let mids: Vec<f64> = open.iter().map(|(a, b)| halving.mid(a.at, b.at)).collect();
        let rows = eval(&mids)?;
        let level = open.into_iter().zip(points(&mids, rows));
        let mut next = Vec::new();
        all.extend(level.filter_map(|((a, b), m)| split((square, sides), a, b, m, &mut next)));
        open = next;
    }
    all.sort_by(|p, q| p.at.total_cmp(&q.at));
    Ok(all)
}

/// Two neighbouring points as an interval.
fn interval(pair: &[Swept]) -> Option<Interval> {
    match pair {
        [a, b] => Some((a.clone(), b.clone())),
        _ => None,
    }
}

/// Whether either end of the interval is a vertex: one with both ends failed lies inside a gap.
fn any_vertex(square: &Square, (a, b): &Interval) -> bool {
    square.at(&a.row).is_some() || square.at(&b.row).is_some()
}

/// The midpoint `m` of `a` and `b`, kept unless both ends and it are vertices on one side of the dome and it lies
/// within the tolerance of their chord. A kept midpoint's halves are examined at the next level, but a half with both
/// ends failed, which lies inside a gap: so a bend is halved until its chords are within the tolerance, and a gap's
/// edges and a change of side are each closed in on.
fn split((square, sides): (&Square, Sides), a: Swept, b: Swept, m: Swept, next: &mut Vec<Interval>) -> Option<Swept> {
    let within = match (square.at(&a.row), square.at(&b.row), square.at(&m.row)) {
        (Some(pa), Some(pb), Some(pm)) => sides.one([&a, &m, &b]) && distance(pa, pb, pm) <= TOLERANCE,
        _ => false,
    };
    if within {
        return None;
    }
    let halves = [(a, m.clone()), (m.clone(), b)];
    next.extend(halves.into_iter().filter(|half| any_vertex(square, half)));
    Some(m)
}

/// The distance from `m` to the segment from `a` to `b`.
fn distance(a: (f64, f64), b: (f64, f64), m: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mx, my) = (m.0 - a.0, m.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 { ((mx * dx + my * dy) / length).clamp(0.0, 1.0) } else { 0.0 };
    let (ex, ey) = (mx - t * dx, my - t * dy);
    math::sqrt(ex * ex + ey * ey)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::Inputs;
    use crate::plane::Quantity;
    use phasekit_core::Pair;

    /// A row at (x, y), failed where `ok` is false.
    fn row(x: f64, y: f64, ok: bool) -> Row {
        Row { cells: vec![x, y], ok: vec![ok, ok], inputs: Inputs { pair: Pair::PT, x, y } }
    }

    fn unit_square() -> Square {
        let axis = Axis { quantity: Quantity::T, log: false };
        Square::new((axis, (0.0, 1.0)), (axis, (0.0, 1.0)))
    }

    /// A line y = x², sampled at x: a row a point, failed for x in `gap`.
    fn parabola(gap: (f64, f64)) -> impl Fn(&[f64]) -> Result<Vec<Row>, Error> {
        move |at: &[f64]| Ok(at.iter().map(|&x| row(x, x * x, !(gap.0 < x && x < gap.1))).collect())
    }

    /// The distance to a segment is to its nearest point, an end where the foot falls outside it.
    #[test]
    fn the_distance_is_to_the_segments_nearest_point() {
        assert_eq!(distance((0.0, 0.0), (2.0, 0.0), (1.0, 0.5)), 0.5);
        assert_eq!(distance((0.0, 0.0), (1.0, 0.0), (2.0, 0.0)), 1.0);
        assert_eq!(distance((0.0, 0.0), (1.0, 0.0), (-3.0, 4.0)), 5.0);
        assert_eq!(distance((1.0, 1.0), (1.0, 1.0), (4.0, 5.0)), 5.0);
    }

    /// A straight line keeps its points; a bend is halved until its chords are within the tolerance, or six times.
    #[test]
    fn a_bend_is_halved_until_its_chords_are_within_the_tolerance() {
        let line = |at: &[f64]| Ok(at.iter().map(|&x| row(x, x, true)).collect());
        let ends = points(&[0.0, 1.0], vec![row(0.0, 0.0, true), row(1.0, 1.0, true)]);
        assert_eq!(refine(ends.clone(), (Halving::Linear, Sides::One), &unit_square(), line).unwrap(), ends);
        let eval = parabola((2.0, 2.0));
        let start = points(&[0.0, 1.0], eval(&[0.0, 1.0]).unwrap());
        let got = refine(start, (Halving::Linear, Sides::One), &unit_square(), &eval).unwrap();
        let at: Vec<f64> = got.iter().map(|p| p.at).collect();
        // The sagitta of y = x² over an interval of width w is w²/4: within 1e-3 once w is 1/16.
        assert_eq!(at, (0..=16).map(|i| f64::from(i) / 16.0).collect::<Vec<_>>());
    }

    /// An interval with one failed end is halved six times toward the failure, so the gap's edges lie within 1/64 of
    /// the interval of where the batch stops answering; an interval with both ends failed is not halved.
    #[test]
    fn a_gaps_edges_are_closed_in_on_to_the_depth() {
        let eval = parabola((0.3, 0.7));
        let start = points(&[0.0, 0.5, 1.0], eval(&[0.0, 0.5, 1.0]).unwrap());
        let got = refine(start, (Halving::Linear, Sides::One), &unit_square(), &eval).unwrap();
        let answered = |p: &&Swept| p.row.ok == [true, true];
        let below = got.iter().filter(answered).map(|p| p.at).filter(|&x| x < 0.5).fold(0.0, f64::max);
        let above = got.iter().filter(answered).map(|p| p.at).filter(|&x| x > 0.5).fold(1.0, f64::min);
        assert!(0.3 - below <= 0.5 / 64.0 && above - 0.7 <= 0.5 / 64.0, "{below} {above}");
        let failed = [row(0.0, 0.0, false), row(1.0, 1.0, false)];
        let lost = points(&[0.0, 1.0], failed.to_vec());
        assert_eq!(
            refine(lost.clone(), (Halving::Linear, Sides::One), &unit_square(), parabola((-1.0, 2.0))).unwrap(),
            lost
        );
    }

    /// Where a straight line changes side, two-phase from x = 0.3 on, the change is closed in on to the depth; an edge
    /// lies on both sides, and a piece on one side is not halved for it.
    #[test]
    fn a_change_of_side_is_closed_in_on_to_the_depth() {
        let eval = |at: &[f64]| -> Result<Vec<Row>, Error> {
            let at_x =
                |x: f64| Row { cells: vec![x, x, 0.5], ok: vec![true, true, x >= 0.3], inputs: row(x, x, true).inputs };
            Ok(at.iter().map(|&x| at_x(x)).collect())
        };
        let start = points(&[0.0, 1.0], eval(&[0.0, 1.0]).unwrap());
        let got = refine(start.clone(), (Halving::Linear, Sides::ByQuality), &unit_square(), eval).unwrap();
        let one_phase = got.iter().filter(|p| !p.row.ok[2]).map(|p| p.at).fold(0.0, f64::max);
        let two_phase = got.iter().filter(|p| p.row.ok[2]).map(|p| p.at).fold(1.0, f64::min);
        assert!(0.3 - one_phase <= 1.0 / 64.0 && two_phase - 0.3 <= 1.0 / 64.0, "{one_phase} {two_phase}");
        assert_eq!(refine(start.clone(), (Halving::Linear, Sides::One), &unit_square(), eval).unwrap(), start);
        let mut edged = start;
        edged[0].edge = true;
        assert_eq!(refine(edged.clone(), (Halving::Linear, Sides::ByQuality), &unit_square(), eval).unwrap(), edged);
    }

    /// A logarithmic sweep is halved at the geometric mean, and a logarithmic axis measured in its logarithm.
    #[test]
    fn a_logarithmic_sweep_is_halved_geometrically() {
        assert_eq!(Halving::Geometric.mid(1.0, 100.0), 10.0);
        assert_eq!(Halving::Linear.mid(1.0, 100.0), 50.5);
        let log = Scale::new(Axis { quantity: Quantity::P, log: true }, (1.0, 100.0));
        assert!((log.unit(10.0) - 0.5).abs() < 1e-15);
    }
}
