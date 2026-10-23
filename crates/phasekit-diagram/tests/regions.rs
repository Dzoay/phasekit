//! The phase regions (PLAN.md M19.5, the site's `regions.test.ts` of M21.9 and #253): inside each region and clear of
//! its edges, the library's phase by the plane's own pair is the region's; each region's phase is the library's at a
//! state inside it; the cold edge halves only the isobars the library refuses at the lowest T and follows the melting
//! line; a pseudo-pure fluid's cold edge stays on the liquid side of its dome; the critical isotherm and isobar run
//! through the critical point.
#![allow(clippy::disallowed_types, reason = "a test-only record of the sampler's calls; the library keeps none (D8)")]

mod common;

use std::cell::RefCell;

use common::{Tamper, fail, fluid, plotted_range};
use phasekit_core::{Argument, Basis, Error, Fluid, Input, Pair, Phase, Prop, math};
use phasekit_diagram::{
    Batch, Corner, FluidBatch, Inputs, Limits, Plane, Quantity, Regions, Row, regions, sample_regions,
};

/// The five embedded fluids.
const EMBEDDED: [&str; 5] = ["Water", "n-Propane", "CarbonDioxide", "Nitrogen", "R134a"];

/// The six phases a fluid's regions have.
const ALL: [Phase; 6] = [
    Phase::Liquid,
    Phase::Gas,
    Phase::TwoPhase,
    Phase::Supercritical,
    Phase::SupercriticalLiquid,
    Phase::SupercriticalGas,
];

/// The cold edge's isobars on either side of p_c.
const COLD_ROWS: usize = 48;

type Point = (f64, f64);

fn hypot(a: f64, b: f64) -> f64 {
    math::sqrt(a * a + b * b)
}

/// The test's own plane geometry: whether `p` is inside the polygon by the even-odd rule.
fn within(p: Point, polygon: &[Point]) -> bool {
    let mut odd = false;
    let n = polygon.len();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + n - 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) {
            odd = !odd;
        }
    }
    odd
}

/// The distance from `p` to the polygon's nearest edge.
fn depth(p: Point, polygon: &[Point]) -> f64 {
    let n = polygon.len();
    let edge = |i: usize| {
        let (a, b) = (polygon[(i + n - 1) % n], polygon[i]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = dx * dx + dy * dy;
        let t = if length == 0.0 { 0.0 } else { ((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length };
        let t = t.clamp(0.0, 1.0);
        hypot(p.0 - a.0 - t * dx, p.1 - a.1 - t * dy)
    };
    (0..n).map(edge).fold(f64::INFINITY, f64::min)
}

/// The points of a `columns` × `rows` grid over [x0, x1] × [y0, y1], ends left out, inside the polygon, with depths.
fn grid(polygon: &[Point], (x0, x1, y0, y1): (f64, f64, f64, f64), columns: u32, rows: u32) -> Vec<(Point, f64)> {
    let at = |i: u32, j: u32| {
        (x0 + (x1 - x0) * f64::from(i) / f64::from(columns), y0 + (y1 - y0) * f64::from(j) / f64::from(rows))
    };
    let points = (1..columns).flat_map(|i| (1..rows).map(move |j| (i, j))).map(|(i, j)| at(i, j));
    points.filter(|&p| within(p, polygon)).map(|p| (p, depth(p, polygon))).collect()
}

/// A plane's plotted box in [0, 1]², an axis's logarithm where it is logarithmic: the regions' corners inside the
/// plotted ranges span it.
struct Frame {
    plane: Plane,
    x: (f64, f64),
    y: (f64, f64),
}

impl Frame {
    fn of(plane: Plane, r: &Regions, limits: &Limits) -> Frame {
        let ((t0, t1), (p0, p1)) = plotted_range(limits);
        let inside = |c: &&Corner| t0 <= c.t && c.t <= t1 && p0 <= c.p && c.p <= p1;
        let corners: Vec<&Corner> = r.regions.iter().flat_map(|g| &g.outline).filter(inside).collect();
        let span = |axis: phasekit_diagram::Axis| {
            let all = corners.iter().filter_map(|c| c.get(axis.quantity)).map(|v| scaled(axis.log, v));
            all.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
        };
        Frame { plane, x: span(plane.x()), y: span(plane.y()) }
    }

    fn at(&self, c: &Corner) -> Point {
        let unit = |axis: phasekit_diagram::Axis, (lo, hi): (f64, f64)| {
            (scaled(axis.log, c.get(axis.quantity).unwrap_or(f64::NAN)) - lo) / (hi - lo)
        };
        (unit(self.plane.x(), self.x), unit(self.plane.y(), self.y))
    }

    /// The plane's values at a point of the box.
    fn from(&self, (u, v): Point) -> Point {
        let value = |log: bool, (lo, hi): (f64, f64), t: f64| {
            let s = lo + t * (hi - lo);
            if log { math::exp(s) } else { s }
        };
        (value(self.plane.x().log, self.x, u), value(self.plane.y().log, self.y, v))
    }
}

fn scaled(log: bool, v: f64) -> f64 {
    if log { math::ln(v) } else { v }
}

/// The library's phase at the plane's coordinates (x, y), by the plane's own pair.
fn phase_on(f: &Fluid, plane: Plane, (x, y): Point) -> Result<Phase, Error> {
    let (qx, qy) = (plane.x().quantity, plane.y().quantity);
    const PLANE: &Argument = &Argument::new("plane", "one whose quantities make an input pair");
    let unpaired = Error::InvalidArgument { argument: PLANE, value: None };
    let (pair, swapped) = qx.var().zip(qy.var()).and_then(|(x, y)| Pair::from_vars(x, y)).ok_or(unpaired)?;
    let (a, b) = if swapped { (y, x) } else { (x, y) };
    Ok(f.state(Input::new(pair, a, b, Basis::Mass)?)?.phase())
}

/// The points to probe a region at on `frame`'s plane: up to eight of a coarse grid's clear of its edges by 2 % of the
/// box, else the deepest of a fine grid over the region's part of it (the compressed liquid is a sliver on T–s, T–ρ
/// and h–s).
fn probes(outline: &[Point]) -> Vec<Point> {
    let coarse: Vec<Point> =
        grid(outline, (0.0, 1.0, 0.0, 1.0), 40, 30).into_iter().filter(|g| g.1 >= 0.02).map(|g| g.0).collect();
    if coarse.is_empty() {
        let extent = |f: fn(&Point) -> f64| {
            let all = outline.iter().map(|q| f(q).clamp(0.0, 1.0));
            all.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
        };
        let ((x0, x1), (y0, y1)) = (extent(|q| q.0), extent(|q| q.1));
        let fine = grid(outline, (x0, x1, y0, y1), 150, 150);
        return fine.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|g| g.0).into_iter().collect();
    }
    let every = coarse.len().div_ceil(8);
    coarse.into_iter().step_by(every).collect()
}

/// The phases' names, sorted.
fn names(phases: impl Iterator<Item = Phase>) -> Vec<String> {
    let mut all: Vec<String> = phases.map(|p| format!("{p:?}")).collect();
    all.sort();
    all
}

/// Each probe of each region on `plane`: the region's phase, and the library's at the probe by the plane's pair.
fn probed(f: &Fluid, plane: Plane, r: &Regions, limits: &Limits) -> Vec<(Phase, Result<Phase, Error>, Point)> {
    let frame = Frame::of(plane, r, limits);
    let mut out = Vec::new();
    for region in &r.regions {
        let outline: Vec<Point> = region.outline.iter().map(|c| frame.at(c)).collect();
        let at = probes(&outline).into_iter().map(|p| frame.from(p));
        out.extend(at.map(|p| (region.phase, phase_on(f, plane, p), p)));
    }
    out
}

/// Inside each region, clear of its edges, the library's phase by each plane's own pair is the region's; on p–T the
/// two-phase region is the vapour-pressure curve, with nothing inside it.
#[test]
fn inside_each_region_clear_of_its_edges_the_librarys_phase_is_the_regions() {
    for name in EMBEDDED {
        let f = fluid(name).unwrap();
        let limits = Limits::of(f).unwrap();
        let r = regions(f).unwrap();
        assert_eq!(names(r.regions.iter().map(|g| g.phase)), names(ALL.into_iter()), "{name}");
        for plane in Plane::ALL {
            let probes = probed(f, plane, &r, &limits);
            let wrong: Vec<_> = probes.iter().filter(|(want, got, _)| *got != Ok(*want)).collect();
            assert!(wrong.is_empty(), "{name} {plane:?}: {wrong:?}");
            let shown = |phase: &&Phase| plane != Plane::Pt || **phase != Phase::TwoPhase;
            let seen = |phase: &&Phase| probes.iter().any(|(want, _, _)| want == *phase);
            assert!(ALL.iter().filter(shown).all(|p| seen(&p)), "{name} {plane:?}");
        }
    }
}

/// The states the sampler asks the phase of, in its order, and the regions it draws, of `f`.
fn seeded(f: &Fluid) -> Result<(Vec<Inputs>, Regions), Error> {
    let batch = FluidBatch::new(f);
    let seeds = RefCell::new(Vec::new());
    let phase = |i: Inputs| {
        seeds.borrow_mut().push(i);
        batch.phase(i)
    };
    let r = sample_regions(&batch, &phase, &Limits::of(f)?)?;
    Ok((seeds.into_inner(), r))
}

/// Each region's phase is the library's at the state the sampler asked it of, which lies inside the region: on p–h,
/// in h and ln p, where every region has room. On CO₂ too, whose plotted floor, half its saturation pressure at its
/// high triple point, lies far above Water's.
#[test]
fn each_regions_phase_is_the_librarys_at_a_state_inside_it() {
    for name in ["Water", "CarbonDioxide"] {
        let f = fluid(name).unwrap();
        let (seeds, r) = seeded(f).unwrap();
        assert_eq!(seeds.len(), 6, "{name}");
        let ph = |c: &Corner| (c.h, math::ln(c.p));
        for (region, seed) in r.regions.iter().zip(&seeds) {
            let state = f.state(seed.input().unwrap()).unwrap();
            let (h, p) = (f.prop(&state, Prop::Hmass).unwrap(), f.prop(&state, Prop::P).unwrap());
            let outline: Vec<Point> = region.outline.iter().map(ph).collect();
            assert!(within((h, math::ln(p)), &outline), "{name} {:?}", region.phase);
            assert_eq!(state.phase(), region.phase, "{name}");
        }
    }
}

/// Each outline walks its region's edge, a step between corners at most a tenth of the range in T and ln p.
#[test]
fn each_outline_walks_its_regions_edge_in_steps_of_at_most_a_tenth() {
    for name in ["Water", "n-Propane"] {
        let f = fluid(name).unwrap();
        let limits = Limits::of(f).unwrap();
        let r = regions(f).unwrap();
        let ((low, _), _) = plotted_range(&limits);
        assert_eq!(r.regions.len(), 6, "{name}");
        let floor = r.regions.iter().flat_map(|g| &g.outline).map(|c| c.p).fold(f64::INFINITY, f64::min);
        let u =
            |c: &Corner| ((c.t - low) / (limits.t_max - low), math::ln(c.p / floor) / math::ln(limits.p_max / floor));
        for region in &r.regions {
            let n = region.outline.len();
            let step = |i: usize| {
                let (a, b) = (u(&region.outline[(i + n - 1) % n]), u(&region.outline[i]));
                hypot(a.0 - b.0, a.1 - b.1)
            };
            let longest = (0..n).map(step).fold(0.0, f64::max);
            assert!(longest < 0.1, "{name} {:?}: {longest}", region.phase);
        }
    }
}

/// A `PT` call: its temperatures, its pressures and the pressures where any output failed.
type Call = (Vec<f64>, Vec<f64>, Vec<f64>);

/// A batch that records each `PT` call.
struct Recorder<'a> {
    inner: FluidBatch<'a>,
    calls: RefCell<Vec<Call>>,
}

impl Batch for Recorder<'_> {
    fn rows(&self, outputs: &[Quantity], pair: Pair, x: &[f64], y: &[f64]) -> Result<Vec<Row>, Error> {
        let rows = self.inner.rows(outputs, pair, x, y)?;
        if pair == Pair::PT {
            let refused = rows.iter().filter(|r| r.ok.iter().any(|ok| !ok)).map(|r| r.inputs.x).collect();
            self.calls.borrow_mut().push((y.to_vec(), x.to_vec(), refused));
        }
        Ok(rows)
    }
}

/// Water's `PT` calls as the regions' sampler makes them, and its lowest plotted temperature.
fn water_pt_calls() -> Result<(Vec<Call>, f64), Error> {
    let water = fluid("Water")?;
    let limits = Limits::of(water)?;
    let recorder = Recorder { inner: FluidBatch::new(water), calls: RefCell::new(Vec::new()) };
    let phase = |i: Inputs| recorder.inner.phase(i);
    sample_regions(&recorder, &phase, &limits)?;
    Ok((recorder.calls.into_inner(), plotted_range(&limits).0.0))
}

/// The cold edge halves only the isobars the library refused at the lowest T: Water's isotherm at its triple point
/// reaches ice above about 0.6 GPa, and those isobars, and only they, are halved.
#[test]
fn the_cold_edge_halves_only_the_isobars_refused_at_the_lowest_t() {
    let (calls, low) = water_pt_calls().unwrap();
    let first = calls.iter().position(|(t, p, _)| p.len() == COLD_ROWS && t.iter().all(|&t| t == low)).unwrap();
    let refused = &calls[first].2;
    assert!(!refused.is_empty() && refused.len() < COLD_ROWS, "{}", refused.len());
    assert_eq!(&calls[first + 1].1, refused);
    assert!(calls[first + 1].0.iter().all(|&t| t > low));
}

/// A leg onto the saturation curve stops a cell short of it: the gas side's isotherm at the lowest T, 159 cells from
/// below, ends a log step under the saturation pressure, and the liquid side's cold edge, its first 47 cells from p_c
/// down, a step over it.
#[test]
fn a_leg_onto_the_saturation_curve_stops_a_cell_short_of_it() {
    let (calls, low) = water_pt_calls().unwrap();
    let water = fluid("Water").unwrap();
    let saturated = water.prop(&water.state(Input::new(Pair::QT, 0.0, low, Basis::Mass).unwrap()).unwrap(), Prop::P);
    let saturated = saturated.unwrap();
    let cold: Vec<&Vec<f64>> = calls.iter().filter(|(t, _, _)| t.iter().all(|&t| t == low)).map(|c| &c.1).collect();
    let gas = cold.iter().find(|p| p.len() == 159).unwrap();
    let highest = gas.iter().copied().fold(0.0, f64::max);
    assert!(highest < saturated * (1.0 - 1e-9) && highest > saturated * 0.99, "{highest} {saturated}");
    let liquid = cold.iter().find(|p| p.len() == COLD_ROWS - 1).unwrap();
    let lowest = liquid.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(lowest > saturated * (1.0 + 1e-9) && lowest < saturated * 1.5, "{lowest} {saturated}");
}

/// The cold edge follows the melting line, by bisection on each isobar: n-Propane's isotherm at its lowest T is past
/// its melting line at every pressure above p_c, so each cold isobar there has its state at a higher T.
#[test]
fn the_cold_edge_follows_the_melting_line() {
    let propane = fluid("n-Propane").unwrap();
    let limits = Limits::of(propane).unwrap();
    let r = regions(propane).unwrap();
    let outline = &r.regions.iter().find(|g| g.phase == Phase::SupercriticalLiquid).unwrap().outline;
    let ((low, _), _) = plotted_range(&limits);
    let ratio = limits.p_critical / limits.p_max;
    for i in 0..COLD_ROWS {
        let p = limits.p_max * math::powf(ratio, i as f64 / (COLD_ROWS - 1) as f64);
        let on: Vec<f64> =
            outline.iter().filter(|c| (c.p / p - 1.0).abs() < 1e-9 && c.t < limits.t_critical).map(|c| c.t).collect();
        assert!(!on.is_empty(), "{p} Pa");
        assert!(on.iter().copied().fold(f64::INFINITY, f64::min) > low, "{p} Pa");
    }
}

/// A pseudo-pure fluid's cold edge stays on the liquid side of its dome: Air has no state between its bubble and dew
/// temperatures at a pressure below p_c, and its lowest plotted T is under its melting line, so every cold isobar is
/// halved, and a halving that landed between them would climb to the dew side (#253).
#[test]
fn a_pseudo_pure_fluids_cold_edge_stays_on_the_liquid_side_of_its_dome() {
    let air = fluid("Air").unwrap();
    let limits = Limits::of(air).unwrap();
    let r = regions(air).unwrap();
    let liquid = &r.regions.iter().find(|g| g.phase == Phase::Liquid).unwrap().outline;
    let below: Vec<&Corner> = liquid.iter().filter(|c| c.p < limits.p_critical).collect();
    assert!(below.len() > COLD_ROWS, "{}", below.len());
    let bubble =
        |p: f64| -> Result<f64, Error> { air.prop(&air.state(Input::new(Pair::PQ, p, 0.0, Basis::Mass)?)?, Prop::T) };
    let across: Vec<String> = below
        .iter()
        .filter(|c| bubble(c.p).is_ok_and(|t| c.t > t * (1.0 + 1e-9)))
        .map(|c| format!("{} K at {} Pa", c.t, c.p))
        .collect();
    assert!(across.is_empty(), "{across:#?}");
}

/// Checks `r`'s critical lines: each in order along it (p up the isotherm, T along the isobar), every corner but the
/// critical point at T_c or p_c; the isotherm from below p_c/10 to above 10 p_c, the isobar from below halfway down to
/// the lowest plotted T to above halfway up to T_max; through the critical point where the library gives one, and
/// broken there, in two runs, where it refuses it.
fn critical_lines(name: &str, r: &Regions, limits: &Limits) {
    let lines = [(&r.isotherm, true, limits.t_critical), (&r.isobar, false, limits.p_critical)];
    for (line, isotherm, held) in lines {
        let all: Vec<&Corner> = line.iter().flatten().collect();
        match r.critical {
            Some(c) => assert!(all.contains(&&c), "{name} isotherm {isotherm}"),
            None => assert_eq!(line.len(), 2, "{name} isotherm {isotherm}"),
        }
        let along: Vec<f64> = all.iter().map(|c| if isotherm { c.p } else { c.t }).collect();
        assert!(along.windows(2).all(|w| w[1] > w[0]), "{name} isotherm {isotherm}");
        let value = |c: &Corner| if isotherm { c.t } else { c.p };
        let on = |c: &&&Corner| Some(***c) != r.critical;
        assert!(all.iter().filter(on).all(|c| (value(c) / held - 1.0).abs() < 1e-9), "{name} isotherm {isotherm}");
        let (lo, hi) = (along[0], along[along.len() - 1]);
        let ((low, _), _) = plotted_range(limits);
        if isotherm {
            assert!(lo < limits.p_critical / 10.0 && hi > limits.p_critical * 10.0, "{name}: {lo} {hi}");
        } else {
            let (below, above) = ((low + limits.t_critical) / 2.0, (limits.t_critical + limits.t_max) / 2.0);
            assert!(lo < below && hi > above, "{name}: {lo} {hi}");
        }
    }
}

/// The critical isotherm and isobar run through the library's critical point, the state at (T_c, ρ_c), where it gives
/// one, and break there where it refuses it: with core's `fast` maths it refuses n-Propane's (`MechanicallyUnstable`).
#[test]
fn the_critical_isotherm_and_isobar_run_through_the_critical_point() {
    let mut through = 0;
    for name in ["n-Propane", "CarbonDioxide", "Water"] {
        let f = fluid(name).unwrap();
        let limits = Limits::of(f).unwrap();
        let r = regions(f).unwrap();
        let state = f.state(Input::new(Pair::DT, limits.rho_critical, limits.t_critical, Basis::Mass).unwrap());
        assert_eq!(r.critical.is_some(), state.is_ok(), "{name}: {state:?}");
        if let Some(c) = r.critical {
            assert!((c.t / limits.t_critical - 1.0).abs() < 1e-9, "{name}");
            through += 1;
        }
        critical_lines(name, &r, &limits);
    }
    assert!(through >= 2, "{through}");
}

/// A region whose phase the library refuses is left out; a critical point by (T_c, ρ_c) that it refuses, as it refused
/// Nitrogen's until core's own exp (PLAN.md M23.2), joins the regions across it, breaks the critical lines there, and
/// leaves T_c and p_c the constants'.
#[test]
fn a_refused_phase_leaves_its_region_out_and_a_refused_critical_point_is_joined_across() {
    let water = fluid("Water").unwrap();
    let batch = FluidBatch::new(water);
    let refused = |i: Inputs| (i.pair != Pair::QT).then_some(Phase::Gas);
    let r = sample_regions(&batch, &refused, &Limits::of(water).unwrap()).unwrap();
    assert_eq!(r.regions.iter().map(|g| g.phase).collect::<Vec<_>>(), [Phase::Gas; 5]);
    let nitrogen = fluid("Nitrogen").unwrap();
    let limits = Limits::of(nitrogen).unwrap();
    let unstable = Tamper {
        inner: FluidBatch::new(nitrogen),
        when: |_: &[Quantity], pair: Pair, _: &[f64], _: &[f64]| pair == Pair::DT,
        change: |rows: &mut [Row]| rows.iter_mut().for_each(fail),
    };
    let phase = |i: Inputs| unstable.inner.phase(i);
    let r = sample_regions(&unstable, &phase, &limits).unwrap();
    assert_eq!(r.critical, None);
    assert_eq!(names(r.regions.iter().map(|g| g.phase)), names(ALL.into_iter()));
    assert_eq!((r.isotherm.len(), r.isobar.len()), (2, 2));
    assert_eq!((r.t_critical, r.p_critical), (limits.t_critical, limits.p_critical));
}

/// A state whose any output failed is left out: the critical point's h failed leaves it out; the saturated liquid's
/// middle state failed leaves out the three regions under the critical point that it seeds.
#[test]
fn a_state_whose_any_output_failed_is_left_out() {
    let water = fluid("Water").unwrap();
    let limits = Limits::of(water).unwrap();
    let any = |_: Inputs| Some(Phase::Gas);
    let no_h = Tamper {
        inner: FluidBatch::new(water),
        when: |_: &[Quantity], pair: Pair, _: &[f64], _: &[f64]| pair == Pair::DT,
        change: |rows: &mut [Row]| rows.iter_mut().for_each(|r| r.ok[2] = false),
    };
    let r = sample_regions(&no_h, &any, &limits).unwrap();
    assert_eq!((r.critical, r.regions.len()), (None, 6));
    let no_middle = Tamper {
        inner: FluidBatch::new(water),
        when: |_: &[Quantity], pair: Pair, x: &[f64], _: &[f64]| pair == Pair::QT && x.len() == 120 && x[0] == 0.0,
        change: |rows: &mut [Row]| rows[60].ok[2] = false,
    };
    assert_eq!(sample_regions(&no_middle, &any, &limits).unwrap().regions.len(), 3);
}

/// Nothing is drawn where the library has no saturated liquid at the lowest temperature, but the critical point.
#[test]
fn nothing_is_drawn_without_a_saturated_liquid_at_the_lowest_temperature() {
    let water = fluid("Water").unwrap();
    let limits = Limits::of(water).unwrap();
    let cold = Limits { t_min: 200.0, t_triple: 200.0, ..limits };
    let r = sample_regions(&FluidBatch::new(water), &|_: Inputs| Some(Phase::Gas), &cold).unwrap();
    assert!(r.regions.is_empty() && r.isotherm.is_empty() && r.isobar.is_empty());
    assert!((r.critical.unwrap().t - limits.t_critical).abs() < 1e-6);
    assert_eq!((r.t_critical, r.p_critical), (limits.t_critical, limits.p_critical));
}
