//! Ice on Water's diagrams (PLAN.md M18.11; user decision HA7): M14's ice Ih and its sublimation curve on M19's planes,
//! passed to the crate as data, as a mixture's envelope is (DG3's rule). Along the sublimation line and ice Ih's
//! melting line, each point's two sides; and the solid along the isobar of the ice Ih–III–liquid triple point, where
//! ice Ih's melting curve ends (no other ice is modelled), and along the lowest temperature's isotherm. [`Ice::of`]
//! fills them from Water and an ice `Fluid` through core's API alone (phasekit-iapws's ice Ih is one).
//!
//! [`sample_with_ice`] samples Water's diagram over its plotted range reaching down to the lowest temperature and its
//! sublimation pressure, the dome from the triple point, and adds the two lines, the axes widened to take them in: on
//! p–T each line's two sides coincide and meet the saturation line at the triple point; on the other planes they bound
//! the solid–vapour and solid–liquid two-phase areas. [`sample_regions_with_ice`] samples Water's regions with their
//! floor down to that pressure and adds the solid region, edged by the two lines, the isobar and the isotherm, and the
//! two areas. Below 273.16 K, its equation's lowest temperature, the library has no state of Water's: an isoline does
//! not follow ice down, and an isotherm there has no segment. It adds no numerics: every vertex and corner of ice's is
//! a state of the data.

use phasekit_core::{
    Basis, Constant, DomainPolicy, Error, FlashOptions, Fluid, Input, Pair, Phase, Pressure, Quality, State,
    Temperature,
};

use crate::batch::{Batch, FluidBatch, Inputs, Vertex};
use crate::diagram::{Diagram, sample_over};
use crate::plane::Plane;
use crate::ranges::{Limits, Ranges, lin_grid, log_grid, ranges};
use crate::regions::{Corner, Region, Regions, sample_regions_down_to};

/// The pressure of the ice Ih–III–liquid triple point, Pa (IAPWS R14-08(2011) Table 1), where ice Ih's melting curve
/// ends.
pub const P_ICE_III: f64 = 208.566e6;

/// The lowest temperature of ice's lines unless the caller sets another, K: the sublimation pressure there is 0.16 Pa.
pub const ICE_FROM: f64 = 200.0;

/// The points along each of the two lines.
const LINE_POINTS: usize = 64;

/// The lowest temperature of ice's lines, which [`Ice::of`] refuses where the sublimation line has no point (core's
/// `Quantity`, not the diagram's axis quantity; PLAN.md M28.1).
const T_LOW: &phasekit_core::Quantity =
    &phasekit_core::Quantity::new("lowest temperature of ice's lines", "T_low", phasekit_core::Dimension::Temperature);

/// The points along the isobar and the isotherm that close the solid.
const EDGE_POINTS: usize = 32;

/// Water's ice as the diagrams take it, each state a corner with every plane's axes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ice {
    /// The sublimation line from the lowest temperature up to the triple point: each point's solid and vapour.
    pub sublimation: Vec<[Corner; 2]>,
    /// Ice Ih's melting line from the triple point up in pressure to [`P_ICE_III`]: each point's solid and liquid.
    pub melting: Vec<[Corner; 2]>,
    /// The solid along [`P_ICE_III`]'s isobar, from the melting line's end down to the lowest temperature.
    pub top: Vec<Corner>,
    /// The solid along the lowest temperature's isotherm, from [`P_ICE_III`] down to the sublimation line.
    pub cold: Vec<Corner>,
}

/// A state's corner.
fn corner(s: &State) -> Corner {
    Corner { t: s.t(), p: s.p(), h: s.h(Basis::Mass), s: s.s(Basis::Mass), rho: s.rho(Basis::Mass) }
}

/// The corner of `fluid`'s state at (p, T) under `opts`: its p and T the inputs, which a Gibbs model's own p can miss
/// by an ulp.
fn at_pt(fluid: &Fluid, (p, t): (f64, f64), opts: &FlashOptions) -> Result<Corner, Error> {
    let s = fluid.flash(Input::pt(Pressure::new(p)?, Temperature::new(t)?), opts)?;
    Ok(Corner { p, t, ..corner(&s) })
}

/// `n` pressures from `a` to `b`, evenly in their logarithm, both ends exactly: `log_grid`'s first can be an ulp off
/// `a` (core's `powf` to the power 0), and below the triple pressure Water has no melting temperature.
fn pressures(a: f64, b: f64, n: usize) -> Vec<f64> {
    let mut all = log_grid(a, b, n);
    if let Some(first) = all.first_mut() {
        *first = a;
    }
    all
}

impl Ice {
    /// Water's ice from `t_low` (K) up, by the two fluids' states: the sublimation line's sides are `ice`'s QT
    /// states at quality 0 and 1, from `t_low` to Water's triple-point temperature; the melting line's points, from the
    /// sublimation line's top pressure to [`P_ICE_III`], are at Water's melting temperature (`melting_temperature`,
    /// single-valued in p, where in T Water's curves of ice III and V overlap ice Ih's), the solid `ice`'s PT state and
    /// the liquid Water's under `DomainPolicy::Extrapolate`, below its equation's lowest temperature; the isobar and
    /// the isotherm are `ice`'s PT states. A PT state's corner takes its inputs for p and T. A state either fluid
    /// refuses is the error.
    pub fn of(water: &Fluid, ice: &Fluid, t_low: f64) -> Result<Ice, Error> {
        let at_quality = |q: f64, t: f64| ice.state(Input::qt(Quality::new(q)?, Temperature::new(t)?));
        let temperatures = lin_grid(t_low, water.constant(Constant::TTriple)?, LINE_POINTS);
        let sublimation = temperatures
            .iter()
            .map(|&t| Ok([corner(&at_quality(0.0, t)?), corner(&at_quality(1.0, t)?)]))
            .collect::<Result<Vec<_>, Error>>()?;
        let ([bottom, _], [triple, _]) = match (sublimation.first(), sublimation.last()) {
            (Some(first), Some(last)) => (*first, *last),
            _ => return Err(Error::InvalidInput { quantity: T_LOW, value: t_low }),
        };
        let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
        let melting = pressures(triple.p, P_ICE_III, LINE_POINTS)
            .into_iter()
            .map(|p| {
                let t = water.model().melting_temperature(p)?;
                Ok([at_pt(ice, (p, t), &FlashOptions::new())?, at_pt(water, (p, t), &extrapolate)?])
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let t_end = melting.last().map_or(triple.t, |[solid, _]| solid.t);
        let solid = |(p, t): (f64, f64)| at_pt(ice, (p, t), &FlashOptions::new());
        let top =
            lin_grid(t_end, t_low, EDGE_POINTS).into_iter().map(|t| solid((P_ICE_III, t))).collect::<Result<_, _>>()?;
        let cold = pressures(P_ICE_III, bottom.p, EDGE_POINTS)
            .into_iter()
            .map(|p| solid((p, t_low)))
            .collect::<Result<_, _>>()?;
        Ok(Ice { sublimation, melting, top, cold })
    }
}

/// A two-phase line of the solid on a plane: its solid side and its fluid side, point for point.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sides {
    /// The solid's states.
    pub solid: Vec<Vertex>,
    /// The vapour's states on the sublimation line, the liquid's on the melting line.
    pub fluid: Vec<Vertex>,
}

/// Ice's two lines on a plane, their vertices the data's corners.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IceLines {
    /// The sublimation line, from the lowest temperature up to the triple point.
    pub sublimation: Sides,
    /// The melting line, from the triple point up in pressure to [`P_ICE_III`].
    pub melting: Sides,
}

/// The corner's vertex on `plane`: its axes' values, at its (p, T).
fn on(plane: Plane, c: &Corner) -> Option<Vertex> {
    let (x, y) = (c.get(plane.x().quantity)?, c.get(plane.y().quantity)?);
    Some(Vertex { x, y, inputs: Inputs { pair: Pair::PT, x: c.p, y: c.t } })
}

/// Side `k` of each point of a line, on `plane`.
fn side(plane: Plane, line: &[[Corner; 2]], k: usize) -> Vec<Vertex> {
    line.iter().filter_map(|pair| pair.get(k)).filter_map(|c| on(plane, c)).collect()
}

/// `span` widened to take in `values`.
fn widened(span: (f64, f64), values: impl Iterator<Item = f64>) -> (f64, f64) {
    values.fold(span, |(lo, hi), v| (lo.min(v), hi.max(v)))
}

/// Ice's lowest temperature and pressure: its sublimation line's first point.
fn lowest(ice: &Ice) -> (f64, f64) {
    ice.sublimation.first().map_or((f64::INFINITY, f64::INFINITY), |[solid, _]| (solid.t, solid.p))
}

/// Water's diagram on `plane` with `ice`, by its own batch under the default options.
pub fn diagram_with_ice(water: &Fluid, plane: Plane, ice: &Ice) -> Result<Diagram, Error> {
    sample_with_ice(&FluidBatch::new(water), plane, &Limits::of(water)?, ice)
}

/// The diagram on `plane` of Water, whose batch and limits these are, with `ice` (see the module's notes): sampled
/// over its plotted range reaching down to ice's lowest temperature and pressure, its dome from the triple point; then
/// ice's two lines added, and the axes widened to take them in.
pub fn sample_with_ice(batch: &impl Batch, plane: Plane, limits: &Limits, ice: &Ice) -> Result<Diagram, Error> {
    let ((t_low, p_low), r) = (lowest(ice), ranges(limits));
    let lowered = Ranges { t: (r.t.0.min(t_low), r.t.1), p: (r.p.0.min(p_low), r.p.1) };
    let mut diagram = sample_over(batch, plane, limits, lowered)?;
    let sides = |line: &[[Corner; 2]]| Sides { solid: side(plane, line, 0), fluid: side(plane, line, 1) };
    let lines = IceLines { sublimation: sides(&ice.sublimation), melting: sides(&ice.melting) };
    let all = || {
        let (s, m) = (&lines.sublimation, &lines.melting);
        s.solid.iter().chain(&s.fluid).chain(&m.solid).chain(&m.fluid)
    };
    diagram.x = widened(diagram.x, all().map(|v| v.x));
    diagram.y = widened(diagram.y, all().map(|v| v.y));
    diagram.ice = Some(lines);
    Ok(diagram)
}

/// A two-phase area: its solid side, then its fluid side back.
fn area(line: &[[Corner; 2]]) -> Vec<Corner> {
    let side = |k: usize| line.iter().filter_map(move |pair| pair.get(k).copied());
    side(0).chain(side(1).collect::<Vec<_>>().into_iter().rev()).collect()
}

/// Water's phase regions with ice's, by its own batch under the default options.
pub fn regions_with_ice(water: &Fluid, ice: &Ice) -> Result<Regions, Error> {
    let batch = FluidBatch::new(water);
    sample_regions_with_ice(&batch, &|inputs| batch.phase(inputs), &Limits::of(water)?, ice)
}

/// The phase regions of Water, whose batch and limits these are, with ice's (see the module's notes): Water's, their
/// floor down to ice's lowest pressure; then the solid, edged by the sublimation and melting lines' solid sides,
/// [`P_ICE_III`]'s isobar and the lowest temperature's isotherm; the solid + vapour; and the solid + liquid, each area
/// its line's solid side and back along its fluid side.
pub fn sample_regions_with_ice(
    batch: &impl Batch,
    phase: &impl Fn(Inputs) -> Option<Phase>,
    limits: &Limits,
    ice: &Ice,
) -> Result<Regions, Error> {
    let mut regions = sample_regions_down_to(batch, phase, limits, lowest(ice).1)?;
    let solid_side = |line: &[[Corner; 2]]| line.iter().map(|[solid, _]| *solid).collect::<Vec<_>>();
    let outline = [solid_side(&ice.sublimation), solid_side(&ice.melting), ice.top.clone(), ice.cold.clone()].concat();
    regions.regions.extend([
        Region { phase: Phase::Solid, outline },
        Region { phase: Phase::TwoPhase, outline: area(&ice.sublimation) },
        Region { phase: Phase::TwoPhase, outline: area(&ice.melting) },
    ]);
    Ok(regions)
}
