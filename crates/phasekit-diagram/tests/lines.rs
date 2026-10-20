//! The isolines (PLAN.md M19.2). Every vertex is the flash's value at its inputs (`Exact`), and flashed again from its
//! plane's coordinates it is the same state within `Flash` (VERIFICATION.md §8.2's round trip), so it lies on its line;
//! a line meets the dome at the saturation states at its value (`QT`, `PQ`, `QS`, `HQ`, `DQ`), and a failed cell
//! breaks it.

mod common;

use common::{Tamper, fail_row, fluid, is_flash};
use phasekit_core::{Basis, Error, Fluid, Input, Pair, Phase, Prop, math};
use phasekit_diagram::{Diagram, FluidBatch, Limits, Line, Plane, Quantity, Row, Segment, Vertex, diagram, sample};
use phasekit_verify::tolerance::{ToleranceClass, Window};

const CASES: [(&str, Plane); 8] = [
    ("Water", Plane::Ph),
    ("Water", Plane::Ts),
    ("Water", Plane::Hs),
    ("n-Propane", Plane::Ph),
    ("n-Propane", Plane::TRho),
    ("CarbonDioxide", Plane::Pt),
    ("R410A", Plane::Ph),
    ("Air", Plane::Ts),
];

/// T, ρ, p and Q of a state: Q NaN in one phase.
fn tdpq(f: &Fluid, input: Input) -> Result<[f64; 4], Error> {
    let s = f.state(input)?;
    let get = |p| f.prop(&s, p);
    Ok([get(Prop::T)?, get(Prop::Dmass)?, get(Prop::P)?, s.quality().unwrap_or(f64::NAN)])
}

/// The input of the plane's coordinates at the vertex, D, H and S per unit mass.
fn plotted(plane: Plane, v: &Vertex) -> Result<Input, Error> {
    let (x, y) = (plane.x().quantity, plane.y().quantity);
    let unpaired = Error::InvalidInput { quantity: "plane", value: f64::NAN };
    let (pair, swapped) = Pair::from_vars(x.var(), y.var()).ok_or(unpaired)?;
    let (a, b) = if swapped { (v.y, v.x) } else { (v.x, v.y) };
    Input::new(pair, a, b, Basis::Mass)
}

/// Whether `got` is `want` within `Flash` (VERIFICATION.md §5): T and ρ 1e-9, near the critical point T 1e-8 and ρ
/// 1e-6, relative; Q 1e-8, near the critical point 1e-6, absolute, and in one phase on both. Not p, which follows from
/// T and ρ: in a liquid at a low pressure it carries ρ's last bits magnified (Water at 300 K and 3.7 kPa: ρ 5e-15
/// apart, p 1e-10).
fn within_flash(got: [f64; 4], want: [f64; 4], window: Window) -> bool {
    let near = window == Window::NearCritical;
    let flash = |scale: f64| ToleranceClass::Flash.bound_in(scale, window).unwrap_or(0.0);
    let t = if near { ToleranceClass::FLASH_NC_T * want[0] } else { flash(want[0]) };
    let rho = if near { ToleranceClass::FLASH_NC_RHO * want[1] } else { flash(want[1]) };
    let q = if near { 1e-6 } else { ToleranceClass::FLASH_QUALITY };
    let qs = (got[3] - want[3]).abs() <= q || (got[3].is_nan() && want[3].is_nan());
    let close = |k: usize, bound: f64| (got[k] - want[k]).abs() <= bound;
    close(0, t) && close(1, rho) && qs
}

/// The pseudo-pure fluids among the cases. Their two-phase states follow CoolProp's ancillary rules, pair by pair (user
/// decision 4): R410A's `DP` and `HP` states at one ρ = 2 kg/m³, h and p are 2e-4 apart in ρ, and (h, p) at its
/// bubble point is refused (`Undefined` Q), the dew point at that p lying at a higher T.
const PSEUDO_PURE: [&str; 2] = ["Air", "R410A"];

/// Whether the vertex, flashed again from its plane's coordinates, is the state of its inputs within `Flash`; true
/// where they name several states (`Ambiguous`: Water's liquid near its density maximum has three at one s and T),
/// and for a pseudo-pure fluid's two-phase states, saturated ones included; and true on the domain's edge where the
/// second flash is refused for the domain: there it can land a rounding outside it, and the `HS`, `PS` and `HP` solvers
/// do not extrapolate (Water's 1 bar isobar at its T_min of 273.16 K, by `HS` with core's `fast` maths).
fn round_trips(f: &Fluid, limits: &Limits, plane: Plane, v: &Vertex) -> Result<bool, Error> {
    let want = tdpq(f, v.inputs.input()?)?;
    if PSEUDO_PURE.contains(&f.info().name()) && !want[3].is_nan() {
        return Ok(true);
    }
    let got = match tdpq(f, plotted(plane, v)?) {
        Err(Error::Ambiguous { .. }) => return Ok(true),
        Err(Error::Domain(_)) if on_edge(want, limits) => return Ok(true),
        got => got?,
    };
    let window = Window::at(want[0], want[1], limits.t_critical, limits.rho_critical);
    let got = if want[3] == 0.0 || want[3] == 1.0 { at_edge(f, got, want[3])? } else { got };
    Ok(within_flash(got, want, window))
}

/// A second flash's answer at a saturation state on the dome's edge `q`, where either side's answer is that state: a
/// one-phase answer is taken at the edge's quality, and a two-phase one within `Flash`'s quality of the edge is its
/// saturation state at its T, whose density so small a quality would otherwise move (Water's saturated liquid at
/// 300 K, by `HS` with core's `fast` maths, lands at Q = 2.1e-12, its ρ 8e-8 off for v″/v′ ≈ 39 000).
fn at_edge(f: &Fluid, got: [f64; 4], q: f64) -> Result<[f64; 4], Error> {
    if got[3].is_nan() {
        return Ok([got[0], got[1], got[2], q]);
    }
    if (got[3] - q).abs() <= ToleranceClass::FLASH_QUALITY {
        return tdpq(f, Input::new(Pair::QT, q, got[0], Basis::Mass)?);
    }
    Ok(got)
}

/// Whether the state, T, ρ, p and Q, lies on the domain's edge: at the lowest temperature, T_max or P_max.
fn on_edge([t, _, p, _]: [f64; 4], l: &Limits) -> bool {
    t <= l.t_min.max(l.t_triple) || t >= l.t_max || p >= l.p_max
}

/// Every vertex of every line, with its line and segment.
fn vertices(d: &Diagram) -> impl Iterator<Item = (&Line, &Segment, &Vertex)> {
    d.lines.iter().flat_map(|l| l.segments.iter().flat_map(move |s| s.vertices.iter().map(move |v| (l, s, v))))
}

fn line(d: &Diagram, held: Quantity, value: f64) -> Option<&Line> {
    d.lines.iter().find(|l| l.held == held && l.value == value)
}

/// Every vertex of every line is the flash's x and y at its inputs, bit for bit, and lies on its line: flashed again
/// from its plane's coordinates, it is the same state within `Flash`, wherever they name one state (a two-phase state
/// on p–T they do not).
#[test]
fn a_vertex_gives_back_its_lines_value() {
    for (name, plane) in CASES {
        let f = fluid(name).unwrap();
        let limits = Limits::of(f).unwrap();
        let d = diagram(f, plane).unwrap();
        let mut count = 0;
        for (l, s, v) in vertices(&d) {
            let label = format!("{name} {plane:?} {:?}={} at {:?}", l.held, l.value, v.inputs);
            assert!(is_flash(f, plane, v).unwrap(), "{label}");
            let skip = plane == Plane::Pt && s.two_phase;
            assert!(skip || round_trips(f, &limits, plane, v).unwrap_or(false), "{label}");
            count += 1;
        }
        assert!(count > 500, "{name} {plane:?}: {count}");
    }
}

/// Whether the vertex is the saturation state at the line's value: its inputs the line's value and Q = 0 or 1.
fn saturated(l: &Line, v: &Vertex) -> bool {
    let i = v.inputs;
    let q = |q: f64| q == 0.0 || q == 1.0;
    match (i.pair, l.held) {
        (Pair::QT, Quantity::T) | (Pair::QS, Quantity::Smass) => i.y == l.value && q(i.x),
        (Pair::PQ, Quantity::P) | (Pair::HQ, Quantity::Hmass) | (Pair::DQ, Quantity::Dmass) => i.x == l.value && q(i.y),
        _ => false,
    }
}

fn two_phase(f: &Fluid, v: &Vertex) -> Result<bool, Error> {
    Ok(f.state(v.inputs.input()?)?.phase() == Phase::TwoPhase)
}

/// A two-phase segment of an isotherm or isobar ends on the `QT` or `PQ` saturation states at its value; wherever a
/// line passes between the dome and either side, the vertex both segments share is the saturation state at its value;
/// inside a two-phase segment every state is two-phase, inside a one-phase one none is. Every plane but p–T, whose
/// isochores may miss the dome, has lines that end on it.
#[test]
fn dome_crossings_are_the_saturation_states_at_the_lines_value() {
    for (name, plane) in CASES {
        let f = fluid(name).unwrap();
        let d = diagram(f, plane).unwrap();
        let mut c = Crossings::default();
        d.lines.iter().for_each(|l| cross(f, plane, l, &mut c));
        if plane != Plane::Pt {
            assert!(c.ends > 3, "{name} {plane:?}: {}", c.ends);
        }
        assert!(
            c.junctions >= 2 * c.crossings,
            "{name} {plane:?}: {} junctions, {} crossings",
            c.junctions,
            c.crossings
        );
    }
}

/// What a diagram's lines show of the dome: the two-phase parts of isotherms and isobars, the vertices a line's side
/// and its two-phase part share, and the segments' ends on the dome.
#[derive(Default)]
struct Crossings {
    crossings: usize,
    junctions: usize,
    ends: usize,
}

/// Checks a line's segments and the vertices they share, and counts them into `c`.
fn cross(f: &Fluid, plane: Plane, l: &Line, c: &mut Crossings) {
    let label = format!("{} {plane:?} {:?}={}", f.info().name(), l.held, l.value);
    let tp = matches!(l.held, Quantity::T | Quantity::P);
    for s in &l.segments {
        check_segment(f, plane, l, s, &label);
        c.crossings += usize::from(s.two_phase && tp);
    }
    let tips = l.segments.iter().flat_map(|s| [s.vertices.first(), s.vertices.last()]).flatten();
    c.ends += tips.filter(|v| saturated(l, v)).count();
    let junctions = l.segments.windows(2).filter(|w| w[0].two_phase != w[1].two_phase);
    let shared =
        junctions.filter_map(|w| Some((w[0].vertices.last()?, w[1].vertices.first()?))).filter(|(a, b)| a == b);
    for (v, _) in shared {
        assert!(saturated(l, v), "{label} at {:?}", v.inputs);
        c.junctions += 1;
    }
}

/// Inside a two-phase segment every state is two-phase, inside a one-phase one none is; an isotherm's or isobar's
/// two-phase segment ends on the saturation states at its value, `Exact`.
fn check_segment(f: &Fluid, plane: Plane, l: &Line, s: &Segment, label: &str) {
    let inner = &s.vertices[1..s.vertices.len() - 1];
    assert!(inner.iter().all(|v| two_phase(f, v).ok() == Some(s.two_phase)), "{label}");
    if s.two_phase && matches!(l.held, Quantity::T | Quantity::P) {
        let mut ends = s.vertices.first().into_iter().chain(s.vertices.last());
        assert!(ends.all(|v| saturated(l, v) && is_flash(f, plane, v).unwrap_or(false)), "{label}");
    }
}

/// Water on p–h with its 400 K isotherm's sweep (`PT` at T = 400 K) changed by `change`.
fn water_ph_with_isotherm(change: impl Fn(&mut [Row])) -> Result<Diagram, Error> {
    let water = fluid("Water")?;
    let broken = Tamper {
        inner: FluidBatch::new(water),
        when: |_: &[Quantity], pair: Pair, _: &[f64], y: &[f64]| pair == Pair::PT && y.first() == Some(&400.0),
        change,
    };
    sample(&broken, Plane::Ph, &Limits::of(water)?)
}

/// Water's p–h sweep's pressure `i` of 160, as the diagram computes it.
fn swept_pressure(i: f64) -> Result<f64, Error> {
    let limits = Limits::of(fluid("Water")?)?;
    let (a, b) = (limits.p_triple.max(limits.p_critical / 1e4), limits.p_max.min(limits.p_critical * 10.0));
    Ok(a * math::powf(b / a, i / 159.0))
}

/// Whether the 400 K isotherm has a vertex at the swept pressure `p`.
fn has_sweep_point(d: &Diagram, p: f64) -> bool {
    let vertices = line(d, Quantity::T, 400.0).into_iter().flat_map(|l| &l.segments).flat_map(|s| &s.vertices);
    vertices.into_iter().any(|v| v.inputs.pair == Pair::PT && v.inputs.x == p)
}

/// A failed cell breaks its line and is never joined across: Water's 400 K isotherm, which crosses the dome, with one
/// state of its vapour side failed, has three one-phase segments.
#[test]
fn a_failed_cell_breaks_its_line() {
    let d = water_ph_with_isotherm(|rows| fail_row(rows, 50)).unwrap();
    let lost = swept_pressure(50.0).unwrap();
    assert!(!has_sweep_point(&d, lost));
    let l = line(&d, Quantity::T, 400.0).unwrap();
    assert_eq!(l.segments.iter().filter(|s| !s.two_phase).count(), 3);
    assert!(has_sweep_point(&diagram(fluid("Water").unwrap(), Plane::Ph).unwrap(), lost));
}

/// A cell whose x or y failed is a gap, whatever its other cells.
#[test]
fn a_cell_whose_x_or_y_failed_is_a_gap() {
    for k in [0, 1] {
        let d = water_ph_with_isotherm(|rows| rows[40].ok[k] = false).unwrap();
        assert!(!has_sweep_point(&d, swept_pressure(40.0).unwrap()), "output {k}");
    }
}

/// Water on p–h with its 400 K isotherm's saturation states (`QT` at T = 400 K, Q the tenths) changed by `change`.
fn water_ph_with_saturation(change: impl Fn(&mut [Row])) -> Result<Diagram, Error> {
    let water = fluid("Water")?;
    let broken = Tamper {
        inner: FluidBatch::new(water),
        when: |_: &[Quantity], pair: Pair, x: &[f64], y: &[f64]| {
            pair == Pair::QT && x.len() == 11 && y.first() == Some(&400.0)
        },
        change,
    };
    sample(&broken, Plane::Ph, &Limits::of(water)?)
}

/// A crossing needs both its ends, the saturation states with their sweep values: without the saturated liquid there
/// is none; a failed quality between them is a gap inside the dome. A saturation state without its sweep value is no
/// edge of an isentrope.
#[test]
fn a_crossing_needs_both_its_ends_and_a_failed_quality_between_is_a_gap() {
    let water = fluid("Water").unwrap();
    let limits = Limits::of(water).unwrap();
    let inside = |d: &Diagram| line(d, Quantity::T, 400.0).unwrap().segments.iter().filter(|s| s.two_phase).count();
    assert_eq!(inside(&diagram(water, Plane::Ph).unwrap()), 1);
    assert_eq!(inside(&water_ph_with_saturation(|rows| rows[0].ok[2] = false).unwrap()), 0);
    assert_eq!(inside(&water_ph_with_saturation(|rows| rows[10].ok[0] = false).unwrap()), 0);
    assert_eq!(inside(&water_ph_with_saturation(|rows| rows[3].ok[0] = false).unwrap()), 2);
    let no_sweep = Tamper {
        inner: FluidBatch::new(water),
        when: |_: &[Quantity], pair: Pair, _: &[f64], _: &[f64]| pair == Pair::QS,
        change: |rows: &mut [Row]| rows.iter_mut().for_each(|r| r.ok[2] = false),
    };
    let d = sample(&no_sweep, Plane::Ph, &limits).unwrap();
    let isentropes = d.lines.iter().filter(|l| l.held == Quantity::Smass);
    assert!(isentropes.flat_map(|l| &l.segments).flat_map(|s| &s.vertices).all(|v| v.inputs.pair != Pair::QS));
}

/// A sweep point at the line's saturation pressure itself is the dome's edge, not a side's: Water's 400 K isotherm,
/// its saturation states moved onto its sweep's tenth pressure, has no `PT` vertex there.
#[test]
fn a_sweep_point_on_the_dome_is_its_edge() {
    let at = swept_pressure(10.0).unwrap();
    assert!(has_sweep_point(&diagram(fluid("Water").unwrap(), Plane::Ph).unwrap(), at));
    let d = water_ph_with_saturation(|rows| rows.iter_mut().for_each(|r| r.cells[2] = at)).unwrap();
    assert!(!has_sweep_point(&d, at));
}

/// An isotherm, swept up in p, enters the dome from the vapour; an isobar, swept up in T, from the liquid; the quality
/// lines are the tenths, each one segment across the dome.
#[test]
fn an_isotherm_enters_the_dome_from_the_vapour_and_an_isobar_from_the_liquid() {
    let water = fluid("Water").unwrap();
    let entry = |d: &Diagram, held: Quantity| -> Vec<f64> {
        let first_two_phase = |l: &Line| l.segments.iter().position(|s| s.two_phase).filter(|&i| i > 0);
        let quality = |v: &Vertex| if v.inputs.pair == Pair::QT { v.inputs.x } else { v.inputs.y };
        let lines = d.lines.iter().filter(|l| l.held == held);
        lines.filter_map(|l| first_two_phase(l).map(|i| quality(&l.segments[i].vertices[0]))).collect()
    };
    let ph = diagram(water, Plane::Ph).unwrap();
    let from_vapour = entry(&ph, Quantity::T);
    assert!(from_vapour.len() > 2 && from_vapour.iter().all(|&q| q == 1.0), "{from_vapour:?}");
    let ts = diagram(water, Plane::Ts).unwrap();
    let from_liquid = entry(&ts, Quantity::P);
    assert!(from_liquid.len() > 2 && from_liquid.iter().all(|&q| q == 0.0), "{from_liquid:?}");
    let quality: Vec<_> = ph.lines.iter().filter(|l| l.held == Quantity::Q).collect();
    let values: Vec<f64> = quality.iter().map(|l| l.value).collect();
    assert_eq!(values, [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
    assert!(quality.iter().all(|l| l.segments.len() == 1 && l.segments[0].vertices.len() > 100));
}

/// A pseudo-pure fluid's isotherm meets the dome at its dew and bubble states, the dew pressure the lower, with no
/// two-phase part: the batch refuses the qualities between (user decision 4).
#[test]
fn a_pseudo_pure_isotherm_meets_the_dome_at_its_dew_and_bubble_states() {
    let r410a = fluid("R410A").unwrap();
    let d = diagram(r410a, Plane::Ph).unwrap();
    let l = line(&d, Quantity::T, 250.0).unwrap();
    assert!(l.segments.iter().all(|s| !s.two_phase));
    let at = |q: f64| {
        let ends = l.segments.iter().flat_map(|s| [s.vertices.first(), s.vertices.last()]).flatten();
        ends.copied().find(|v| v.inputs.pair == Pair::QT && v.inputs.x == q && v.inputs.y == 250.0)
    };
    let (dew, bubble) = (at(1.0).unwrap(), at(0.0).unwrap());
    assert!(dew.y < bubble.y, "{} {}", dew.y, bubble.y);
}
