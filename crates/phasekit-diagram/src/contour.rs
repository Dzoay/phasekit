//! Where a field of the state crosses zero over a grid of states at an input pair's values (PLAN.md M26.3, M26.4):
//! marching squares over the grid's cells, each crossing found by bisection along its edge, at one input's value, so
//! that every vertex is a state the batch computed. One batch call takes a level's midpoints. An edge with one end where the field has no
//! value is first halved toward it [`GAP_DEPTH`] times, as M19.3 closes in on a gap's edge, and its last valued state
//! stands in for that end: a line runs to within 2⁻⁶ of a grid step of a gap (the two-phase region, a state the batch
//! fails, the equation of state's limits). A crossing whose bisection meets a state without a value is left out, and
//! the line ends there.

use std::collections::{BTreeMap, BTreeSet};

use phasekit_core::{Error, Pair};

use crate::batch::{Batch, Row};
use crate::plane::Quantity;
use crate::refine::Halving;

/// The times a crossing's bracket is halved: from a grid step of 10 % (a ratio of 1.1, or a sixty-fourth of a linear
/// span), 2⁻⁴⁰ of it is 9e-14 of the input, within a few ulps.
const BISECTIONS: usize = 40;

/// The times an edge with one end in a gap is halved toward it, M19.3's depth.
const GAP_DEPTH: usize = 6;

/// A field of the state: a value where the row gives one, `None` where it does not.
pub(crate) type Field<'a> = &'a dyn Fn(&Row) -> Option<f64>;

/// The input pair a grid's states are at, and how each of its inputs is halved, in the pair's order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Axes {
    pub pair: Pair,
    pub halving: [Halving; 2],
}

impl Axes {
    /// (ρ, T), each halved at the geometric mean.
    pub(crate) const DT: Axes = Axes { pair: Pair::DT, halving: [Halving::Geometric, Halving::Geometric] };

    /// The midpoint of two points of the pair's inputs.
    fn mid(&self, a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
        let [hx, hy] = self.halving;
        (hx.mid(a.0, b.0), hy.mid(a.1, b.1))
    }
}

/// The states at every value `x` of the pair's first input and `y` of its second, each ascending, row-major by `y`.
pub(crate) struct Grid {
    pub axes: Axes,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub rows: Vec<Row>,
}

impl Grid {
    /// The grid's states by one call of `outputs`.
    pub(crate) fn sample(
        batch: &impl Batch,
        outputs: &[Quantity],
        axes: Axes,
        (x, y): (Vec<f64>, Vec<f64>),
    ) -> Result<Grid, Error> {
        let points: Vec<(f64, f64)> = y.iter().flat_map(|&b| x.iter().map(move |&a| (a, b))).collect();
        let (xs, ys): (Vec<f64>, Vec<f64>) = points.into_iter().unzip();
        let rows = batch.rows(outputs, axes.pair, &xs, &ys)?;
        Ok(Grid { axes, x, y, rows })
    }

    pub(crate) fn row(&self, (i, j): Node) -> Option<&Row> {
        self.rows.get(i.checked_mul(self.x.len())?.checked_add(j)?)
    }

    /// Every edge, along x then along y, each by its lower node.
    fn edges(&self) -> impl Iterator<Item = Edge> {
        let (rows, columns) = (self.y.len(), self.x.len());
        let along = |rows: usize, columns: usize| (0..rows).flat_map(move |i| (0..columns).map(move |j| (i, j)));
        let isotherms = along(rows, columns.saturating_sub(1)).map(Edge::AlongX);
        isotherms.chain(along(rows.saturating_sub(1), columns).map(Edge::AlongY))
    }
}

/// The field's value at a row and the row, where it has one.
type End = (f64, Row);

fn valued(field: Field, row: &Row) -> Option<End> {
    Some((field(row).filter(|v| v.is_finite())?, row.clone()))
}

/// A node of the grid: its index in `y` and its index in `x`.
pub(crate) type Node = (usize, usize);

/// An edge of the grid, by its lower node: along x to the next value of x (on a `DT` grid, an isotherm), or along y to
/// the next value of y (an isochore).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Edge {
    AlongX(Node),
    AlongY(Node),
}

impl Edge {
    fn ends(self) -> [Node; 2] {
        match self {
            Edge::AlongX((i, j)) => [(i, j), (i, j.saturating_add(1))],
            Edge::AlongY((i, j)) => [(i, j), (i.saturating_add(1), j)],
        }
    }
}

/// Each edge's two ends with values: its nodes', or, for one end in a gap, the last valued state toward it.
pub(crate) type Ends = BTreeMap<Edge, [End; 2]>;

/// How far a crossing is bisected and how near zero it must end: a bracket stops once an end is within `stop` of zero,
/// and a crossing farther than `tolerance` from it lies on a jump in the field, not a zero of it (a density across the
/// saturation line on p–T), and is left out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Precision {
    pub stop: f64,
    pub tolerance: f64,
}

impl Precision {
    /// All `BISECTIONS` halvings, and every crossing kept.
    pub(crate) const FULL: Precision = Precision { stop: 0.0, tolerance: f64::INFINITY };
}

/// Where a field crosses zero over a grid: each cell's segments, and the state found on each crossed edge.
pub(crate) struct Crossings {
    segments: Vec<(Edge, Edge)>,
    at: BTreeMap<Edge, Row>,
}

/// Where `field` crosses zero over the grid, from its edges' `ends` ([`ends`]), the batch's `outputs` the grid's.
pub(crate) fn crossings(
    batch: &impl Batch,
    (outputs, grid): (&[Quantity], &Grid),
    field: Field,
    ends: &Ends,
    precision: Precision,
) -> Result<Crossings, Error> {
    let segments = segments(grid, ends);
    let crossed: BTreeSet<Edge> = segments.iter().flat_map(|&(a, b)| [a, b]).collect();
    let brackets = crossed.iter().map(|e| ends.get(e).map(|[a, b]| Bracket::new(a.clone(), b.clone()))).collect();
    let found = bisect(batch, (outputs, grid.axes), field, brackets, precision.stop)?;
    let near = |row: Row| field(&row).is_some_and(|v| v.abs() <= precision.tolerance).then_some(row);
    let at = crossed.into_iter().zip(found).filter_map(|(e, row)| Some((e, near(row?)?))).collect();
    Ok(Crossings { segments, at })
}

impl Crossings {
    /// The lines, each a run of rows in order along it from its end at the lower first input (on a `DT` grid, the
    /// lower density); a closed line returns to its first row.
    pub(crate) fn lines(&self) -> Vec<Vec<Row>> {
        let found = |(a, b): &&(Edge, Edge)| self.at.contains_key(a) && self.at.contains_key(b);
        let kept: Vec<(Edge, Edge)> = self.segments.iter().filter(found).copied().collect();
        let rows = |chain: Vec<Edge>| chain.iter().filter_map(|e| self.at.get(e).cloned()).collect::<Vec<Row>>();
        chains(&kept).into_iter().map(rows).map(from_low_x).collect()
    }
}

/// The lines where `field` crosses zero over the grid ([`Crossings::lines`]), however far from zero a crossing is.
pub(crate) fn zero_lines(
    batch: &impl Batch,
    outputs: &[Quantity],
    grid: &Grid,
    field: Field,
) -> Result<Vec<Vec<Row>>, Error> {
    let ends = ends(batch, outputs, grid, field)?;
    Ok(crossings(batch, (outputs, grid), field, &ends, Precision::FULL)?.lines())
}

/// The ends with the field less `level`: where a field has a value, so does it less any level, and a gap's edge is
/// closed in on once for every level.
pub(crate) fn less(ends: &Ends, level: f64) -> Ends {
    let shift = |(v, row): &End| (v - level, row.clone());
    ends.iter().map(|(&e, [a, b])| (e, [shift(a), shift(b)])).collect()
}

/// Every edge's ends with values: both nodes', or one node's and the last valued state halving toward the other found.
pub(crate) fn ends(batch: &impl Batch, outputs: &[Quantity], grid: &Grid, field: Field) -> Result<Ends, Error> {
    let mut all = Ends::new();
    let mut gaps: Vec<(Edge, Gap)> = Vec::new();
    for edge in grid.edges() {
        let [a, b] = edge.ends().map(|n| grid.row(n));
        let [va, vb] = [a, b].map(|r| r.and_then(|r| valued(field, r)));
        match (va, vb, a, b) {
            (Some(va), Some(vb), _, _) => {
                all.insert(edge, [va, vb]);
            }
            (Some(valued), None, _, Some(failed)) | (None, Some(valued), Some(failed), _) => {
                gaps.push((edge, Gap { valued, toward: (failed.inputs.x, failed.inputs.y) }));
            }
            _ => {}
        }
    }
    let (edges, gaps): (Vec<Edge>, Vec<Gap>) = gaps.into_iter().unzip();
    let closed = close_in(batch, outputs, grid.axes, field, gaps)?;
    all.extend(edges.into_iter().zip(closed));
    Ok(all)
}

/// An edge's valued end, or the last valued state found from it, and the inputs of the end in a gap, or of the first
/// state found without a value.
struct Gap {
    valued: End,
    toward: (f64, f64),
}

impl Gap {
    fn mid(&self, axes: Axes) -> (f64, f64) {
        axes.mid((self.valued.1.inputs.x, self.valued.1.inputs.y), self.toward)
    }

    /// The half toward the gap from the state at `at`, its row `row`.
    fn halve(&mut self, field: Field, row: &Row, at: (f64, f64)) {
        match valued(field, row) {
            Some(found) => self.valued = found,
            None => self.toward = at,
        }
    }
}

/// Each gap's edge halved `GAP_DEPTH` times toward the gap: the valued end, and the last valued state found toward the
/// other, or the valued end again where none was.
fn close_in(
    batch: &impl Batch,
    outputs: &[Quantity],
    axes: Axes,
    field: Field,
    gaps: Vec<Gap>,
) -> Result<Vec<[End; 2]>, Error> {
    let starts: Vec<End> = gaps.iter().map(|g| g.valued.clone()).collect();
    let mut open = gaps;
    for _ in 0..GAP_DEPTH {
        let mids: Vec<(f64, f64)> = open.iter().map(|g| g.mid(axes)).collect();
        let (x, y): (Vec<f64>, Vec<f64>) = mids.iter().copied().unzip();
        let rows = batch.rows(outputs, axes.pair, &x, &y)?;
        for ((gap, row), at) in open.iter_mut().zip(&rows).zip(mids) {
            gap.halve(field, row, at);
        }
    }
    Ok(starts.into_iter().zip(open).map(|(start, gap)| [start, gap.valued]).collect())
}

/// Every cell of the grid, by its lower node.
fn cells(grid: &Grid) -> impl Iterator<Item = Node> {
    let columns = grid.x.len().saturating_sub(1);
    (0..grid.y.len().saturating_sub(1)).flat_map(move |i| (0..columns).map(move |j| (i, j)))
}

/// Each cell's segments, a pair of the edges the line crosses (`cell_segments`).
fn segments(grid: &Grid, ends: &Ends) -> Vec<(Edge, Edge)> {
    cells(grid).flat_map(|cell| cell_segments(ends, cell)).collect()
}

/// A cell's edges counter-clockwise from its lower left corner: below, right, above and left.
fn perimeter((i, j): Node) -> [Edge; 4] {
    let (i1, j1) = (i.saturating_add(1), j.saturating_add(1));
    [Edge::AlongX((i, j)), Edge::AlongY((i, j1)), Edge::AlongX((i1, j)), Edge::AlongY((i, j))]
}

/// A cell's segments. Its corners counter-clockwise from the lower left, a, b, c and d, and its edges below, right,
/// above and left; an edge is crossed where its ends' values have opposite signs. Two crossed edges make a segment, a
/// corner in a gap or not. Four make a saddle's two, which cut off b and d where the centre, the mean of the corners,
/// is on a's side, and a and c otherwise: the mean of the lower and upper edges' ends, the corners where none is in a gap.
fn cell_segments(ends: &Ends, cell: Node) -> Vec<(Edge, Edge)> {
    let edges = perimeter(cell);
    let [lower, _, upper, _] = edges;
    let crosses = |e: &Edge| ends.get(e).is_some_and(|[(u, _), (v, _)]| (*u >= 0.0) != (*v >= 0.0));
    let crossed: Vec<Edge> = edges.iter().copied().filter(crosses).collect();
    match *crossed.as_slice() {
        [e, f] => vec![(e, f)],
        [below, right, above, left] if centred(ends, lower, upper) => vec![(below, right), (above, left)],
        [below, right, above, left] => vec![(below, left), (right, above)],
        _ => Vec::new(),
    }
}

/// Whether a saddle's centre, the mean of its lower and upper edges' ends, is on the side of the lower edge's first:
/// the mean's sign is their sum's.
fn centred(ends: &Ends, lower: Edge, upper: Edge) -> bool {
    let values = |e: Edge| ends.get(&e).map(|[(u, _), (v, _)]| [*u, *v]);
    values(lower).zip(values(upper)).is_some_and(|([a, b], [d, c])| (a + b + c + d >= 0.0) == (a >= 0.0))
}

/// A band's bound: its level and where the field less the level crosses zero.
pub(crate) type Bound<'a> = (f64, &'a Crossings);

/// The band of `value` from `low` up to `high` (unbounded where `None`), as each cell's piece of it: the polygon its
/// perimeter makes, counter-clockwise from its lower left corner, of the corners inside the band and the crossings of
/// its bounds on the edges between, in order along each. A cell with a corner without a value, or whose piece needs a
/// crossing that was left out, has none; at a saddle the piece is one polygon, around the centre.
pub(crate) fn band(grid: &Grid, value: Field, low: Option<Bound>, high: Option<Bound>) -> Vec<Vec<Row>> {
    cells(grid).filter_map(|cell| piece(grid, value, cell, [low, high])).collect()
}

/// The cell's piece of the band, where it has one.
fn piece(grid: &Grid, value: Field, cell: Node, bounds: [Option<Bound>; 2]) -> Option<Vec<Row>> {
    let [low, high] = bounds;
    let inside = |v: f64| low.is_none_or(|(l, _)| v >= l) && high.is_none_or(|(h, _)| v < h);
    let mut polygon = Vec::new();
    for (edge, reversed) in perimeter(cell).into_iter().zip([false, false, true, true]) {
        let [from, to] = if reversed { rev(edge.ends()) } else { edge.ends() };
        let [u, v] = [from, to].map(|n| grid.row(n).and_then(|r| valued(value, r)));
        let ((u, start), (v, _)) = (u?, v?);
        if inside(u) {
            polygon.push(start);
        }
        let mut cuts: Vec<(f64, Row)> = Vec::new();
        for (level, crossings) in bounds.into_iter().flatten() {
            if (u >= level) != (v >= level) {
                cuts.push(((u - level).abs(), crossings.at.get(&edge)?.clone()));
            }
        }
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        polygon.extend(cuts.into_iter().map(|(_, row)| row));
    }
    // A piece has a corner and the two crossings either side of it, or two crossings of each bound: three at least.
    (!polygon.is_empty()).then_some(polygon)
}

fn rev([a, b]: [Node; 2]) -> [Node; 2] {
    [b, a]
}

/// A crossing's bracket: its ends' field values and rows, the field's signs at them opposite.
pub(crate) struct Bracket {
    ends: [End; 2],
}

/// Each bracket halved up to `BISECTIONS` times, until an end is within `stop` of zero, each half at its ends' inputs'
/// midpoint as `axes` takes it; its row is the end nearer zero, `None` where the bracket was `None` or a midpoint's
/// field has no value.
pub(crate) fn bisect(
    batch: &impl Batch,
    (outputs, axes): (&[Quantity], Axes),
    field: Field,
    brackets: Vec<Option<Bracket>>,
    stop: f64,
) -> Result<Vec<Option<Row>>, Error> {
    let mut open = brackets;
    for _ in 0..BISECTIONS {
        let live = |b: &&Bracket| !b.within(stop);
        let mids: Vec<(f64, f64)> = open.iter().flatten().filter(live).map(|b| b.mid(axes)).collect();
        if mids.is_empty() {
            break;
        }
        let (x, y): (Vec<f64>, Vec<f64>) = mids.into_iter().unzip();
        let mut rows = batch.rows(outputs, axes.pair, &x, &y)?.into_iter();
        let mut step = |b: Bracket| if live(&&b) { rows.next().and_then(|row| b.halve(field, row)) } else { Some(b) };
        open = open.into_iter().map(|b| b.and_then(&mut step)).collect();
    }
    Ok(open.into_iter().map(|b| b.map(Bracket::nearer)).collect())
}

impl Bracket {
    /// A bracket between two rows, each with its field value.
    pub(crate) fn new(a: End, b: End) -> Bracket {
        Bracket { ends: [a, b] }
    }

    fn mid(&self, axes: Axes) -> (f64, f64) {
        let [(_, a), (_, b)] = &self.ends;
        axes.mid((a.inputs.x, a.inputs.y), (b.inputs.x, b.inputs.y))
    }

    fn halve(self, field: Field, row: Row) -> Option<Bracket> {
        let m = valued(field, &row)?;
        let [a, b] = self.ends;
        Some(Bracket { ends: if (m.0 >= 0.0) == (a.0 >= 0.0) { [m, b] } else { [a, m] } })
    }

    /// Whether an end is within `stop` of zero.
    fn within(&self, stop: f64) -> bool {
        self.ends.iter().any(|(v, _)| v.abs() <= stop)
    }

    fn nearer(self) -> Row {
        let [a, b] = self.ends;
        if a.0.abs() <= b.0.abs() { a.1 } else { b.1 }
    }
}

/// The segments joined into chains of edges: open chains from their ends first, then closed ones, each returning to
/// its first edge. Each edge lies in at most two cells, so in at most two segments.
fn chains(segments: &[(Edge, Edge)]) -> Vec<Vec<Edge>> {
    let mut next: BTreeMap<Edge, Vec<Edge>> = BTreeMap::new();
    for &(a, b) in segments {
        next.entry(a).or_default().push(b);
        next.entry(b).or_default().push(a);
    }
    let ends = next.iter().filter(|(_, n)| n.len() == 1).map(|(&e, _)| e);
    let starts: Vec<Edge> = ends.chain(next.keys().copied()).collect();
    let mut seen = BTreeSet::new();
    let mut all = Vec::new();
    for start in starts {
        if !seen.contains(&start) {
            all.push(walk(start, &next, &mut seen));
        }
    }
    all
}

/// The chain from `start`, each edge's unseen neighbour next, closed where its last edge neighbours its first.
fn walk(start: Edge, next: &BTreeMap<Edge, Vec<Edge>>, seen: &mut BTreeSet<Edge>) -> Vec<Edge> {
    let mut chain = vec![start];
    seen.insert(start);
    let mut at = start;
    // Each edge once at most: the bound keeps a mutated walk from running away (one that dropped the `!` below filled a
    // CI runner's memory).
    for _ in 0..next.len() {
        let Some(&n) = next.get(&at).and_then(|ns| ns.iter().find(|n| !seen.contains(n))) else { break };
        seen.insert(n);
        chain.push(n);
        at = n;
    }
    let closes = chain.len() > 2 && next.get(&at).is_some_and(|ns| ns.contains(&start));
    if closes {
        chain.push(start);
    }
    chain
}

/// The rows in order from the end at the lower first input.
fn from_low_x(mut rows: Vec<Row>) -> Vec<Row> {
    if let (Some(first), Some(last)) = (rows.first(), rows.last())
        && last.inputs.x < first.inputs.x
    {
        rows.reverse();
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::Inputs;

    fn row(rho: f64, t: f64) -> Row {
        Row { cells: Vec::new(), ok: Vec::new(), inputs: Inputs { pair: Pair::DT, x: rho, y: t } }
    }

    /// Cell (0, 0)'s edges with the corner values a, b, c and d, each edge's ends from its lower node; `None` leaves
    /// a corner's two edges out, as a gap whose halving found nothing would not, but as one with no valued end does.
    fn cell(corners: [Option<f64>; 4]) -> Ends {
        let [a, b, c, d] = corners;
        let end = |v: Option<f64>| v.map(|v| (v, row(1.0, 1.0)));
        let edges = [
            (Edge::AlongX((0, 0)), a, b),
            (Edge::AlongY((0, 1)), b, c),
            (Edge::AlongX((1, 0)), d, c),
            (Edge::AlongY((0, 0)), a, d),
        ];
        edges.into_iter().filter_map(|(e, u, v)| Some((e, [end(u)?, end(v)?]))).collect()
    }

    const BELOW: Edge = Edge::AlongX((0, 0));
    const RIGHT: Edge = Edge::AlongY((0, 1));
    const ABOVE: Edge = Edge::AlongX((1, 0));
    const LEFT: Edge = Edge::AlongY((0, 0));

    /// Two crossed edges make a segment, a corner in a gap or not; a saddle's two cut off b and d where the centre is
    /// on a's side, a and c where it is not; an edge whose ends share a sign is not crossed.
    #[test]
    fn a_cell_joins_its_crossings_and_a_saddle_by_its_centre() {
        let segments = |corners| cell_segments(&cell(corners), (0, 0));
        assert_eq!(segments([Some(1.0), Some(-1.0), Some(-1.0), Some(1.0)]), [(BELOW, ABOVE)]);
        assert_eq!(segments([Some(-1.0), Some(-1.0), Some(1.0), Some(-1.0)]), [(RIGHT, ABOVE)]);
        assert_eq!(segments([Some(1.0), Some(-1.0), Some(1.0), Some(-1.0)]), [(BELOW, RIGHT), (ABOVE, LEFT)]);
        assert_eq!(segments([Some(1.0), Some(-3.0), Some(1.0), Some(-3.0)]), [(BELOW, LEFT), (RIGHT, ABOVE)]);
        assert_eq!(segments([Some(-1.0), Some(3.0), Some(-1.0), Some(3.0)]), [(BELOW, LEFT), (RIGHT, ABOVE)]);
        assert_eq!(segments([Some(1.0), Some(-1.0), Some(1.0), None]), [(BELOW, RIGHT)]);
        assert_eq!(segments([Some(0.0), Some(0.0), Some(0.0), Some(0.0)]), []);
        // Three crossed edges, where a halving toward a gap found the line on one, make none.
        let mut gapped = cell([Some(1.0), Some(-1.0), Some(1.0), Some(-1.0)]);
        gapped.remove(&ABOVE);
        assert_eq!(cell_segments(&gapped, (0, 0)), []);
    }

    /// Open chains run from an end, closed ones return to their first edge, and each edge is in one chain.
    #[test]
    fn chains_run_from_their_ends_and_close_loops() {
        let e = |i: usize| Edge::AlongX((i, 0));
        let f = |i: usize| Edge::AlongY((i, 5));
        let segments = [(e(1), e(2)), (f(0), f(1)), (e(0), e(1)), (f(1), f(2)), (f(2), f(3)), (f(3), f(0))];
        let all = chains(&segments);
        assert_eq!(all, [vec![e(0), e(1), e(2)], vec![f(0), f(1), f(2), f(3), f(0)]]);
        assert_eq!(chains(&[(e(0), e(1))]), [vec![e(0), e(1)]]);
    }

    /// A line's rows run from its lower-density end.
    #[test]
    fn a_line_runs_from_its_lower_first_input() {
        let densities = |rows: Vec<Row>| rows.iter().map(|r| r.inputs.x).collect::<Vec<_>>();
        assert_eq!(densities(from_low_x(vec![row(3.0, 1.0), row(2.0, 1.0), row(1.0, 1.0)])), [1.0, 2.0, 3.0]);
        assert_eq!(densities(from_low_x(vec![row(1.0, 1.0), row(5.0, 1.0)])), [1.0, 5.0]);
        assert!(from_low_x(Vec::new()).is_empty());
    }

    /// A gap's edge is halved toward the gap: to the last valued state, the valued end again where none was.
    #[test]
    fn a_gap_is_closed_in_on_from_its_valued_end() {
        let mut gap = Gap { valued: (1.0, row(1.0, 100.0)), toward: (4.0, 100.0) };
        assert_eq!(gap.mid(Axes::DT), (2.0, 100.0));
        gap.halve(&|_| Some(-2.0), &row(2.0, 100.0), (2.0, 100.0));
        assert_eq!((gap.valued.0, gap.valued.1.inputs.x, gap.toward), (-2.0, 2.0, (4.0, 100.0)));
        gap.halve(&|_| None, &row(3.0, 100.0), (3.0, 100.0));
        assert_eq!((gap.valued.1.inputs.x, gap.toward), (2.0, (3.0, 100.0)));
    }

    /// A cell of corner values a, b, c and d (lower left, lower right, upper right, upper left), its crossings of the
    /// levels 0 and 1 on each edge they cross, each a row at a distinct point.
    fn banded(corners: [f64; 4]) -> (Grid, [Crossings; 2]) {
        let [a, b, c, d] = corners;
        let node =
            |v: f64, x: f64, y: f64| Row { cells: vec![v], ok: vec![true], inputs: Inputs { pair: Pair::DT, x, y } };
        let rows = vec![node(a, 1.0, 1.0), node(b, 2.0, 1.0), node(d, 1.0, 2.0), node(c, 2.0, 2.0)];
        let grid = Grid { axes: Axes::DT, x: vec![1.0, 2.0], y: vec![1.0, 2.0], rows };
        let ends = [(BELOW, a, b), (RIGHT, b, c), (ABOVE, d, c), (LEFT, a, d)];
        let level = |l: f64| {
            let at = ends.iter().filter(|(_, u, v)| (*u >= l) != (*v >= l));
            let at = at.map(|&(e, _, _)| (e, row(10.0 * l + f64::from(e == RIGHT || e == LEFT), 0.0))).collect();
            Crossings { segments: Vec::new(), at }
        };
        (grid, [level(0.0), level(1.0)])
    }

    /// A cell's piece of a band runs around it from its lower left corner: the corners inside the band and its
    /// bounds' crossings between, in order along each edge; a cell wholly inside is its four corners, one wholly
    /// outside or missing a crossing it needs has none.
    #[test]
    fn a_band_is_each_cells_piece_of_it() {
        let value = |r: &Row| r.cells.first().copied();
        let inputs = |rows: Vec<Vec<Row>>| {
            rows.iter().map(|p| p.iter().map(|r| (r.inputs.x, r.inputs.y)).collect()).collect::<Vec<Vec<_>>>()
        };
        let (grid, [zero, one]) = banded([-1.0, 0.5, 2.0, 0.5]);
        let middle = band(&grid, &value, Some((0.0, &zero)), Some((1.0, &one)));
        // The crossings of 0 below (0, 0) and left (1, 0), of 1 right (11, 0) and above (10, 0).
        let want = [(0.0, 0.0), (2.0, 1.0), (11.0, 0.0), (10.0, 0.0), (1.0, 2.0), (1.0, 0.0)];
        assert_eq!(inputs(middle), [want.to_vec()]);
        assert_eq!(inputs(band(&grid, &value, None, Some((0.0, &zero)))), [vec![(1.0, 1.0), (0.0, 0.0), (1.0, 0.0)]]);
        assert_eq!(inputs(band(&grid, &value, Some((1.0, &one)), None)), [vec![(11.0, 0.0), (2.0, 2.0), (10.0, 0.0)]]);
        let (inside, [zero, one]) = banded([0.2, 0.4, 0.6, 0.8]);
        let all = [(1.0, 1.0), (2.0, 1.0), (2.0, 2.0), (1.0, 2.0)];
        assert_eq!(inputs(band(&inside, &value, Some((0.0, &zero)), Some((1.0, &one)))), [all.to_vec()]);
        assert!(band(&inside, &value, Some((1.0, &one)), None).is_empty());
        let (grid, [zero, _]) = banded([-1.0, 0.5, 2.0, 0.5]);
        let missing = Crossings { segments: Vec::new(), at: BTreeMap::new() };
        assert!(band(&grid, &value, Some((0.0, &zero)), Some((1.0, &missing))).is_empty());
    }

    /// A segment with a crossing that was not found draws nothing: the line ends at the crossing before it.
    #[test]
    fn a_line_ends_where_a_crossing_was_not_found() {
        let e = |i: usize| Edge::AlongX((i, 0));
        let found = |edges: &[usize]| {
            edges.iter().map(|&i| (e(i), row(f64::from(u8::try_from(i).unwrap_or(0)), 1.0))).collect()
        };
        let segments = vec![(e(0), e(1)), (e(1), e(2)), (e(2), e(3))];
        let densities = |lines: Vec<Vec<Row>>| {
            lines.iter().map(|l| l.iter().map(|r| r.inputs.x).collect()).collect::<Vec<Vec<f64>>>()
        };
        assert_eq!(
            densities(Crossings { segments: segments.clone(), at: found(&[0, 1, 2, 3]) }.lines()),
            [vec![0.0, 1.0, 2.0, 3.0]]
        );
        assert_eq!(densities(Crossings { segments, at: found(&[0, 2, 3]) }.lines()), [vec![2.0, 3.0]]);
    }

    /// A bisected bracket's row is its end nearer zero.
    #[test]
    fn a_bracket_ends_at_its_end_nearer_zero() {
        let nearer = |a: f64, b: f64| Bracket::new((a, row(1.0, 1.0)), (b, row(2.0, 1.0))).nearer().inputs.x;
        assert_eq!([nearer(0.5, -0.1), nearer(-0.1, 0.5), nearer(0.2, -0.2)], [2.0, 1.0, 1.0]);
    }

    /// A corner at a level lies in the band above it, as a crossing's sides take it.
    #[test]
    fn a_corner_at_a_level_is_the_band_above() {
        let value = |r: &Row| r.cells.first().copied();
        let (grid, [zero, one]) = banded([0.0, 0.5, 1.0, 0.5]);
        assert!(band(&grid, &value, None, Some((0.0, &zero))).is_empty());
        let middle = band(&grid, &value, Some((0.0, &zero)), Some((1.0, &one)));
        assert_eq!(middle.iter().map(Vec::len).collect::<Vec<_>>(), [5]);
        assert_eq!(band(&grid, &value, Some((1.0, &one)), None).iter().map(Vec::len).collect::<Vec<_>>(), [3]);
    }

    /// A closed line, whose ends are at one first input, keeps its order.
    #[test]
    fn a_closed_line_keeps_its_order() {
        let ts = |rows: Vec<Row>| rows.iter().map(|r| r.inputs.y).collect::<Vec<_>>();
        assert_eq!(ts(from_low_x(vec![row(1.0, 1.0), row(2.0, 2.0), row(1.0, 3.0)])), [1.0, 2.0, 3.0]);
    }
}
