//! Where a field of the state crosses zero over a grid of `DT` states (PLAN.md M26.3, M26.4): marching squares over
//! the grid's cells, each crossing found by bisection along its edge, an isotherm or an isochore, so that every vertex
//! is a state the batch computed. One batch call takes a level's midpoints. An edge with one end where the field has no
//! value is first halved toward it [`GAP_DEPTH`] times, as M19.3 closes in on a gap's edge, and its last valued state
//! stands in for that end: a line runs to within 2⁻⁶ of a grid step of a gap (the two-phase region, a state the batch
//! fails, the equation of state's limits). A crossing whose bisection meets a state without a value is left out, and
//! the line ends there.

use std::collections::{BTreeMap, BTreeSet};

use phasekit_core::{Error, Pair, math};

use crate::batch::{Batch, Row};
use crate::plane::Quantity;

/// The times a crossing's bracket is halved, in the logarithm of its varying input: from a grid step of 10 % (a ratio
/// of 1.1), 2⁻⁴⁰ of it is 9e-14, within a few ulps of the input.
const BISECTIONS: usize = 40;

/// The times an edge with one end in a gap is halved toward it, M19.3's depth.
const GAP_DEPTH: usize = 6;

/// A field of the state: a value where the row gives one, `None` where it does not.
pub(crate) type Field<'a> = &'a dyn Fn(&Row) -> Option<f64>;

/// The `DT` states at every temperature of `t` and density of `rho`, each ascending, row-major by temperature.
pub(crate) struct Grid {
    pub rho: Vec<f64>,
    pub t: Vec<f64>,
    pub rows: Vec<Row>,
}

impl Grid {
    /// The grid's states by one `DT` call of `outputs`.
    pub(crate) fn sample(batch: &impl Batch, outputs: &[Quantity], rho: Vec<f64>, t: Vec<f64>) -> Result<Grid, Error> {
        let points: Vec<(f64, f64)> = t.iter().flat_map(|&t| rho.iter().map(move |&rho| (rho, t))).collect();
        let (x, y): (Vec<f64>, Vec<f64>) = points.into_iter().unzip();
        let rows = batch.rows(outputs, Pair::DT, &x, &y)?;
        Ok(Grid { rho, t, rows })
    }

    fn row(&self, (i, j): Node) -> Option<&Row> {
        self.rows.get(i.checked_mul(self.rho.len())?.checked_add(j)?)
    }

    /// Every edge, isotherms then isochores, each by its lower node.
    fn edges(&self) -> impl Iterator<Item = Edge> {
        let (rows, columns) = (self.t.len(), self.rho.len());
        let along = |rows: usize, columns: usize| (0..rows).flat_map(move |i| (0..columns).map(move |j| (i, j)));
        let isotherms = along(rows, columns.saturating_sub(1)).map(Edge::Isotherm);
        isotherms.chain(along(rows.saturating_sub(1), columns).map(Edge::Isochore))
    }
}

/// The field's value at a row and the row, where it has one.
type End = (f64, Row);

fn valued(field: Field, row: &Row) -> Option<End> {
    Some((field(row).filter(|v| v.is_finite())?, row.clone()))
}

/// A node of the grid: its temperature's index and its density's.
type Node = (usize, usize);

/// An edge of the grid, by its lower node: along an isotherm to the next density, or along an isochore to the next
/// temperature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Edge {
    Isotherm(Node),
    Isochore(Node),
}

impl Edge {
    fn ends(self) -> [Node; 2] {
        match self {
            Edge::Isotherm((i, j)) => [(i, j), (i, j.saturating_add(1))],
            Edge::Isochore((i, j)) => [(i, j), (i.saturating_add(1), j)],
        }
    }
}

/// Each edge's two ends with values: its nodes', or, for one end in a gap, the last valued state toward it.
type Ends = BTreeMap<Edge, [End; 2]>;

/// The lines where `field` crosses zero, each a run of rows in order along it from its lower-density end (a closed
/// line returns to its first row); the batch's `outputs` are the grid's.
pub(crate) fn zero_lines(
    batch: &impl Batch,
    outputs: &[Quantity],
    grid: &Grid,
    field: Field,
) -> Result<Vec<Vec<Row>>, Error> {
    let ends = ends(batch, outputs, grid, field)?;
    let segments = segments(grid, &ends);
    let crossed: BTreeSet<Edge> = segments.iter().flat_map(|&(a, b)| [a, b]).collect();
    let brackets = crossed.iter().map(|e| ends.get(e).map(|[a, b]| Bracket::new(a.clone(), b.clone()))).collect();
    let found = bisect(batch, outputs, field, brackets)?;
    let at: BTreeMap<Edge, Row> = crossed.into_iter().zip(found).filter_map(|(e, row)| Some((e, row?))).collect();
    Ok(joined(segments, &at))
}

/// The lines the segments make where both their crossings were found, each as its crossings' rows.
fn joined(segments: Vec<(Edge, Edge)>, at: &BTreeMap<Edge, Row>) -> Vec<Vec<Row>> {
    let kept: Vec<(Edge, Edge)> =
        segments.into_iter().filter(|(a, b)| at.contains_key(a) && at.contains_key(b)).collect();
    let rows = |chain: Vec<Edge>| chain.iter().filter_map(|e| at.get(e).cloned()).collect::<Vec<Row>>();
    chains(&kept).into_iter().map(rows).map(from_low_density).collect()
}

/// Every edge's ends with values: both nodes', or one node's and the last valued state halving toward the other found.
fn ends(batch: &impl Batch, outputs: &[Quantity], grid: &Grid, field: Field) -> Result<Ends, Error> {
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
    let closed = close_in(batch, outputs, field, gaps)?;
    all.extend(edges.into_iter().zip(closed));
    Ok(all)
}

/// An edge's valued end, or the last valued state found from it, and the inputs (ρ, T) of the end in a gap, or of
/// the first state found without a value.
struct Gap {
    valued: End,
    toward: (f64, f64),
}

impl Gap {
    fn mid(&self) -> (f64, f64) {
        mid((self.valued.1.inputs.x, self.valued.1.inputs.y), self.toward)
    }

    /// The half toward the gap from the state at `at`, its row `row`.
    fn halve(&mut self, field: Field, row: &Row, at: (f64, f64)) {
        match valued(field, row) {
            Some(found) => self.valued = found,
            None => self.toward = at,
        }
    }
}

/// Each gap's edge halved `GAP_DEPTH` times toward the gap, at the geometric mean: the valued end, and the last valued
/// state found toward the other, or the valued end again where none was.
fn close_in(batch: &impl Batch, outputs: &[Quantity], field: Field, gaps: Vec<Gap>) -> Result<Vec<[End; 2]>, Error> {
    let starts: Vec<End> = gaps.iter().map(|g| g.valued.clone()).collect();
    let mut open = gaps;
    for _ in 0..GAP_DEPTH {
        let mids: Vec<(f64, f64)> = open.iter().map(Gap::mid).collect();
        let (rho, t): (Vec<f64>, Vec<f64>) = mids.iter().copied().unzip();
        let rows = batch.rows(outputs, Pair::DT, &rho, &t)?;
        for ((gap, row), at) in open.iter_mut().zip(&rows).zip(mids) {
            gap.halve(field, row, at);
        }
    }
    Ok(starts.into_iter().zip(open).map(|(start, gap)| [start, gap.valued]).collect())
}

/// The geometric mean of two points (ρ, T).
fn mid(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (math::sqrt(a.0 * b.0), math::sqrt(a.1 * b.1))
}

/// Each cell's segments, a pair of the edges the line crosses (`cell_segments`).
fn segments(grid: &Grid, ends: &Ends) -> Vec<(Edge, Edge)> {
    let cells =
        (0..grid.t.len().saturating_sub(1)).flat_map(|i| (0..grid.rho.len().saturating_sub(1)).map(move |j| (i, j)));
    cells.flat_map(|cell| cell_segments(ends, cell)).collect()
}

/// A cell's segments. Its corners counter-clockwise from the lower left, a, b, c and d, and its edges below, right,
/// above and left; an edge is crossed where its ends' values have opposite signs. Two crossed edges make a segment, a
/// corner in a gap or not. Four make a saddle's two, which cut off b and d where the centre, the mean of the corners,
/// is on a's side, and a and c otherwise: the mean of the lower and upper edges' ends, the corners where none is in a gap.
fn cell_segments(ends: &Ends, (i, j): Node) -> Vec<(Edge, Edge)> {
    let (i1, j1) = (i.saturating_add(1), j.saturating_add(1));
    let (lower, upper) = (Edge::Isotherm((i, j)), Edge::Isotherm((i1, j)));
    let edges = [lower, Edge::Isochore((i, j1)), upper, Edge::Isochore((i, j))];
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

/// A crossing's bracket: its ends' field values and rows, the field's signs at them opposite.
pub(crate) struct Bracket {
    ends: [End; 2],
}

/// Each bracket halved `BISECTIONS` times, each half at the geometric mean of its ends' inputs; its row is the end
/// nearer zero, `None` where the bracket was `None` or a midpoint's field has no value.
pub(crate) fn bisect(
    batch: &impl Batch,
    outputs: &[Quantity],
    field: Field,
    brackets: Vec<Option<Bracket>>,
) -> Result<Vec<Option<Row>>, Error> {
    let mut open = brackets;
    for _ in 0..BISECTIONS {
        let mids: Vec<(f64, f64)> = open.iter().flatten().map(Bracket::mid).collect();
        let (rho, t): (Vec<f64>, Vec<f64>) = mids.into_iter().unzip();
        let mut rows = batch.rows(outputs, Pair::DT, &rho, &t)?.into_iter();
        open = open.into_iter().map(|b| b.and_then(|b| b.halve(field, rows.next()?))).collect();
    }
    Ok(open.into_iter().map(|b| b.map(Bracket::nearer)).collect())
}

impl Bracket {
    /// A bracket between two rows of `DT` states, each with its field value.
    pub(crate) fn new(a: End, b: End) -> Bracket {
        Bracket { ends: [a, b] }
    }

    fn mid(&self) -> (f64, f64) {
        let [(_, a), (_, b)] = &self.ends;
        mid((a.inputs.x, a.inputs.y), (b.inputs.x, b.inputs.y))
    }

    fn halve(self, field: Field, row: Row) -> Option<Bracket> {
        let m = valued(field, &row)?;
        let [a, b] = self.ends;
        Some(Bracket { ends: if (m.0 >= 0.0) == (a.0 >= 0.0) { [m, b] } else { [a, m] } })
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

/// The rows in order from the end at the lower density.
fn from_low_density(mut rows: Vec<Row>) -> Vec<Row> {
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
            (Edge::Isotherm((0, 0)), a, b),
            (Edge::Isochore((0, 1)), b, c),
            (Edge::Isotherm((1, 0)), d, c),
            (Edge::Isochore((0, 0)), a, d),
        ];
        edges.into_iter().filter_map(|(e, u, v)| Some((e, [end(u)?, end(v)?]))).collect()
    }

    const BELOW: Edge = Edge::Isotherm((0, 0));
    const RIGHT: Edge = Edge::Isochore((0, 1));
    const ABOVE: Edge = Edge::Isotherm((1, 0));
    const LEFT: Edge = Edge::Isochore((0, 0));

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
        let e = |i: usize| Edge::Isotherm((i, 0));
        let f = |i: usize| Edge::Isochore((i, 5));
        let segments = [(e(1), e(2)), (f(0), f(1)), (e(0), e(1)), (f(1), f(2)), (f(2), f(3)), (f(3), f(0))];
        let all = chains(&segments);
        assert_eq!(all, [vec![e(0), e(1), e(2)], vec![f(0), f(1), f(2), f(3), f(0)]]);
        assert_eq!(chains(&[(e(0), e(1))]), [vec![e(0), e(1)]]);
    }

    /// A line's rows run from its lower-density end.
    #[test]
    fn a_line_runs_from_its_lower_density_end() {
        let densities = |rows: Vec<Row>| rows.iter().map(|r| r.inputs.x).collect::<Vec<_>>();
        assert_eq!(densities(from_low_density(vec![row(3.0, 1.0), row(2.0, 1.0), row(1.0, 1.0)])), [1.0, 2.0, 3.0]);
        assert_eq!(densities(from_low_density(vec![row(1.0, 1.0), row(5.0, 1.0)])), [1.0, 5.0]);
        assert!(from_low_density(Vec::new()).is_empty());
    }

    /// A gap's edge is halved toward the gap: to the last valued state, the valued end again where none was.
    #[test]
    fn a_gap_is_closed_in_on_from_its_valued_end() {
        let mut gap = Gap { valued: (1.0, row(1.0, 100.0)), toward: (4.0, 100.0) };
        assert_eq!(gap.mid(), (2.0, 100.0));
        gap.halve(&|_| Some(-2.0), &row(2.0, 100.0), (2.0, 100.0));
        assert_eq!((gap.valued.0, gap.valued.1.inputs.x, gap.toward), (-2.0, 2.0, (4.0, 100.0)));
        gap.halve(&|_| None, &row(3.0, 100.0), (3.0, 100.0));
        assert_eq!((gap.valued.1.inputs.x, gap.toward), (2.0, (3.0, 100.0)));
    }

    /// A segment with a crossing that was not found draws nothing: the line ends at the crossing before it.
    #[test]
    fn a_line_ends_where_a_crossing_was_not_found() {
        let e = |i: usize| Edge::Isotherm((i, 0));
        let found = |edges: &[usize]| {
            edges.iter().map(|&i| (e(i), row(f64::from(u8::try_from(i).unwrap_or(0)), 1.0))).collect()
        };
        let segments = vec![(e(0), e(1)), (e(1), e(2)), (e(2), e(3))];
        let densities = |lines: Vec<Vec<Row>>| {
            lines.iter().map(|l| l.iter().map(|r| r.inputs.x).collect()).collect::<Vec<Vec<f64>>>()
        };
        assert_eq!(densities(joined(segments.clone(), &found(&[0, 1, 2, 3]))), [vec![0.0, 1.0, 2.0, 3.0]]);
        assert_eq!(densities(joined(segments, &found(&[0, 2, 3]))), [vec![2.0, 3.0]]);
    }

    /// A bisected bracket's row is its end nearer zero.
    #[test]
    fn a_bracket_ends_at_its_end_nearer_zero() {
        let nearer = |a: f64, b: f64| Bracket::new((a, row(1.0, 1.0)), (b, row(2.0, 1.0))).nearer().inputs.x;
        assert_eq!([nearer(0.5, -0.1), nearer(-0.1, 0.5), nearer(0.2, -0.2)], [2.0, 1.0, 1.0]);
    }

    /// A closed line, whose ends are at one density, keeps its order.
    #[test]
    fn a_closed_line_keeps_its_order() {
        let ts = |rows: Vec<Row>| rows.iter().map(|r| r.inputs.y).collect::<Vec<_>>();
        assert_eq!(ts(from_low_density(vec![row(1.0, 1.0), row(2.0, 2.0), row(1.0, 3.0)])), [1.0, 2.0, 3.0]);
    }
}
