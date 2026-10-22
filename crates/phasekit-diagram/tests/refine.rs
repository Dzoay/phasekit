//! Sampled where a line bends (PLAN.md M19.3). Each piece of a line, and each side of the dome, is refined from its
//! starting points: an interval is halved while its midpoint lies farther than 10⁻³ from its chord in the plane's unit
//! square, at most 6 times. So every interval left is within the tolerance or at the depth, and a gap's edge lies
//! within 1/64 of a starting interval of where the library stops answering.

mod common;

use common::{CASES, Tamper, fluid, plotted_range};
use phasekit_core::{Basis, Error, Fluid, Input, Pair, math};
use phasekit_diagram::{Axis, Diagram, FluidBatch, Inputs, Limits, Plane, Quantity, Row, Vertex, diagram, sample};

/// How far a midpoint may lie from its chord in the plane's unit square.
const TOLERANCE: f64 = 1e-3;

/// The most times an interval is halved: a starting interval's 64th is the finest.
const FINEST: f64 = 64.0;

/// The starting intervals of a sweep over the plotted T or p, or of an isotherm's or isobar's two-phase part over Q:
/// 32 points.
const INTERVALS: f64 = 31.0;

/// The dome's temperatures, which the quality lines start on too: 120, closing in on T_c.
const DOME_POINTS: f64 = 120.0;

/// A run of vertices along one sweep: the quantity its line holds and the value, the quantity it is swept over, and
/// whether it starts on the dome's temperatures.
struct Piece<'a> {
    held: (Quantity, f64),
    sweep: Quantity,
    on_dome: bool,
    vertices: &'a [Vertex],
}

/// Every piece of the diagram: each side of the dome, over T at Q = 0 or 1, and each segment of each line, a
/// two-phase segment of an isotherm or isobar over Q, a quality line over T and the rest over their family's sweep.
fn pieces(d: &Diagram) -> Vec<Piece<'_>> {
    let side = |q: f64, vertices| Piece { held: (Quantity::Q, q), sweep: Quantity::T, on_dome: true, vertices };
    let mut out = vec![side(0.0, &d.dome.liquid[..]), side(1.0, &d.dome.vapour[..])];
    for l in &d.lines {
        let family = d.plane.families().iter().find(|f| f.held == l.held);
        for s in &l.segments {
            let sweep = match l.held {
                Quantity::T | Quantity::P if s.two_phase => Quantity::Q,
                _ => family.map_or(Quantity::T, |f| f.sweep),
            };
            out.push(Piece { held: (l.held, l.value), sweep, on_dome: l.held == Quantity::Q, vertices: &s.vertices });
        }
    }
    out
}

/// A sweep value as its intervals are measured: p in its logarithm, as its grid runs.
fn metric(sweep: Quantity, v: f64) -> f64 {
    if sweep == Quantity::P { math::ln(v) } else { v }
}

/// The midpoint of two sweep values: geometric for p, arithmetic for the rest.
fn mid(sweep: Quantity, a: f64, b: f64) -> f64 {
    if sweep == Quantity::P { math::sqrt(a * b) } else { (a + b) / 2.0 }
}

/// The widest starting interval of a sweep, in its metric: a 31st of the plotted T, of ln p or of Q; on the dome's
/// temperatures, which close in on T_c, the first, under 2/120 of the way from the lowest plotted T to T_c.
fn widest(limits: &Limits, sweep: Quantity, on_dome: bool) -> f64 {
    let ((t0, t1), (p0, p1)) = plotted_range(limits);
    match sweep {
        Quantity::T if on_dome => 2.0 * (limits.t_critical - t0) / DOME_POINTS,
        Quantity::T => (t1 - t0) / INTERVALS,
        Quantity::P => (math::ln(p1) - math::ln(p0)) / INTERVALS,
        _ => 1.0 / INTERVALS,
    }
}

/// The vertex's sweep value: its input where its pair takes the sweep, else its state's (a saturation state's on the
/// dome's edge).
fn swept(f: &Fluid, sweep: Quantity, v: &Vertex) -> Result<f64, Error> {
    let i = v.inputs;
    match i.pair.vars() {
        (a, _) if a == sweep.var() => Ok(i.x),
        (_, b) if b == sweep.var() => Ok(i.y),
        _ => f.prop(&f.state(i.input()?)?, sweep.prop()),
    }
}

/// The plane's x and y at the state where the line holding `held` takes the sweep value `at`, by one flash.
fn plotted_at(f: &Fluid, plane: Plane, held: (Quantity, f64), sweep: (Quantity, f64)) -> Result<(f64, f64), Error> {
    let unpaired = Error::InvalidArgument { argument: "input pair", value: None };
    let (pair, swapped) = Pair::from_vars(held.0.var(), sweep.0.var()).ok_or(unpaired)?;
    let (x, y) = if swapped { (sweep.1, held.1) } else { (held.1, sweep.1) };
    let state = f.state(Input::new(pair, x, y, Basis::Mass)?)?;
    Ok((f.prop(&state, plane.x().quantity.prop())?, f.prop(&state, plane.y().quantity.prop())?))
}

/// A value's place on an axis spanning `span`, in [0, 1]: a logarithmic axis's in its logarithm.
fn unit(axis: Axis, span: (f64, f64), v: f64) -> f64 {
    let scaled = |v: f64| if axis.log { math::ln(v) } else { v };
    let lo = scaled(span.0);
    (scaled(v) - lo) / (scaled(span.1) - lo)
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

/// The diagram's unit square: a point's x and y each placed on its axis's span.
fn square(d: &Diagram, (x, y): (f64, f64)) -> (f64, f64) {
    (unit(d.plane.x(), d.x, x), unit(d.plane.y(), d.y, y))
}

/// Whether the interval from `a` to `b` of `piece` is at the depth, a starting interval's 64th or less, or else its
/// midpoint, flashed, lies within the tolerance of its chord.
fn settled(f: &Fluid, limits: &Limits, d: &Diagram, piece: &Piece, (a, b): (&Vertex, &Vertex)) -> Result<bool, Error> {
    let (sa, sb) = (swept(f, piece.sweep, a)?, swept(f, piece.sweep, b)?);
    let width = (metric(piece.sweep, sb) - metric(piece.sweep, sa)).abs();
    if width <= widest(limits, piece.sweep, piece.on_dome) / FINEST * (1.0 + 1e-9) {
        return Ok(true);
    }
    let m = plotted_at(f, d.plane, piece.held, (piece.sweep, mid(piece.sweep, sa, sb)))?;
    Ok(distance(square(d, (a.x, a.y)), square(d, (b.x, b.y)), square(d, m)) <= TOLERANCE)
}

/// An interval by its line's held quantity and value and its ends' inputs.
type Interval = ((Quantity, f64), Inputs, Inputs);

/// The intervals of the diagram neither within the chord tolerance nor at the depth.
fn unsettled(f: &Fluid, d: &Diagram) -> Result<Vec<Interval>, Error> {
    let limits = Limits::of(f)?;
    let mut found = Vec::new();
    for piece in pieces(d) {
        let open = |w: &&[Vertex]| !settled(f, &limits, d, &piece, (&w[0], &w[1])).unwrap_or(false);
        found.extend(piece.vertices.windows(2).filter(open).map(|w| (piece.held, w[0].inputs, w[1].inputs)));
    }
    Ok(found)
}

/// Every interval of every line and of the dome is within the chord tolerance, or at the depth: its midpoint, flashed
/// at the sweep's midpoint, lies within 10⁻³ of the chord in the plane's unit square, or it is a starting interval's
/// 64th.
#[test]
fn every_interval_is_within_the_chord_tolerance() {
    let mut cases = Vec::new();
    for (name, plane) in CASES {
        let f = fluid(name).unwrap();
        let found = unsettled(f, &diagram(f, plane).unwrap()).unwrap();
        if let Some(first) = found.first() {
            cases.push(format!("{name} {plane:?}: {} unsettled, first {first:?}", found.len()));
        }
    }
    assert!(cases.is_empty(), "{cases:#?}");
}

/// Water on p–h with its 400 K isotherm's states (`PT` at T = 400 K) failed strictly between `lo` and `hi`.
fn water_ph_with_a_gap(lo: f64, hi: f64) -> Result<Diagram, Error> {
    let water = fluid("Water")?;
    let broken = Tamper {
        inner: FluidBatch::new(water),
        when: |_: &[Quantity], pair: Pair, _: &[f64], y: &[f64]| pair == Pair::PT && y.first() == Some(&400.0),
        change: |rows: &mut [Row]| {
            let inside = rows.iter_mut().filter(|r| lo < r.inputs.x && r.inputs.x < hi);
            inside.for_each(|r| r.ok.iter_mut().for_each(|ok| *ok = false));
        },
    };
    sample(&broken, Plane::Ph, &Limits::of(water)?)
}

/// A gap's edges lie within 1/64 of a starting interval, in ln p, of where the batch stops answering, on either side:
/// Water's 400 K isotherm with its vapour failed between 20 and 50 kPa. The line never joins across the gap, and
/// sampling is deterministic.
#[test]
fn a_gap_ends_within_the_depths_resolution() {
    let (lo, hi) = (2e4, 5e4);
    let d = water_ph_with_a_gap(lo, hi).unwrap();
    let l = d.lines.iter().find(|l| l.held == Quantity::T && l.value == 400.0).unwrap();
    let pressures = || l.segments.iter().flat_map(|s| &s.vertices).filter(|v| v.inputs.pair == Pair::PT);
    assert!(pressures().all(|v| !(lo < v.inputs.x && v.inputs.x < hi)));
    let below = pressures().map(|v| v.inputs.x).filter(|&p| p <= lo).fold(0.0, f64::max);
    let above = pressures().map(|v| v.inputs.x).filter(|&p| p >= hi).fold(f64::INFINITY, f64::min);
    let resolution = widest(&Limits::of(fluid("Water").unwrap()).unwrap(), Quantity::P, false) / FINEST;
    assert!(math::ln(lo / below) <= resolution, "{below} below {lo}");
    assert!(math::ln(above / hi) <= resolution, "{above} above {hi}");
    let both = |w: &[Vertex]| w.iter().all(|v| v.inputs.pair == Pair::PT);
    let across = |w: &[Vertex]| both(w) && w[0].inputs.x <= lo && w[1].inputs.x >= hi;
    assert!(!l.segments.iter().any(|s| s.vertices.windows(2).any(across)));
    assert_eq!(water_ph_with_a_gap(lo, hi).unwrap(), d);
}

/// Each change of side of the dome along an isentrope, isenthalp or isochore with no vertex shared, as the distance in
/// its sweep's metric between the segments' ends, in starting intervals' 64ths.
fn side_changes(f: &Fluid, d: &Diagram) -> Result<Vec<f64>, Error> {
    let limits = Limits::of(f)?;
    let mut out = Vec::new();
    for l in d.lines.iter().filter(|l| !matches!(l.held, Quantity::T | Quantity::P | Quantity::Q)) {
        let sweep = d.plane.families().iter().find(|f| f.held == l.held).map_or(Quantity::T, |f| f.sweep);
        let finest = widest(&limits, sweep, false) / FINEST;
        let changes = l.segments.windows(2).filter(|w| w[0].two_phase != w[1].two_phase);
        let ends =
            changes.filter_map(|w| Some((w[0].vertices.last()?, w[1].vertices.first()?))).filter(|(a, b)| a != b);
        for (a, b) in ends {
            out.push((metric(sweep, swept(f, sweep, b)?) - metric(sweep, swept(f, sweep, a)?)).abs() / finest);
        }
    }
    Ok(out)
}

/// Where a line changes side of the dome with no vertex shared, its segments' ends lie within 1/64 of a starting
/// interval of each other: a pseudo-pure fluid's `DP` states turn one-phase short of its `DQ` dew state (user decision
/// 4; Air's at 2 kg/m³ near 40.9 and 42.1 kPa), and the change is closed in on there.
#[test]
fn a_change_of_side_is_closed_in_on_to_the_depth() {
    let mut count = 0;
    for (name, plane) in CASES {
        let f = fluid(name).unwrap();
        let changes = side_changes(f, &diagram(f, plane).unwrap()).unwrap();
        assert!(changes.iter().all(|&c| c <= 1.0 + 1e-9), "{name} {plane:?}: {changes:?}");
        count += changes.len();
    }
    assert!(count > 3, "{count}");
}
