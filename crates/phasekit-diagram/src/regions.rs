//! The phase regions (PLAN.md M19.5, the site's `regions.ts` of M21.9 and #253): each a polygon whose edges are the
//! library's lines, sampled once a fluid with every plane's axes among the outputs, so that every plane reads its axes
//! from the same corners. Each region's phase is the library's (`State::phase`) at a state inside it; the crate names
//! no phase itself. A failed cell is left out of an edge, so where the library has no state an edge cuts across to the
//! next state it has; on the cold side, where the melting line ends the liquid, the edge follows it.
//!
//! In (T, p) the regions are the four quadrants about the critical point, the low-T, low-p one split by the saturation
//! curve into liquid, liquid + vapour and gas. T runs from the plotted lowest to T_max and p from below the saturation
//! pressure at that lowest T (so that every fluid's regions have one shape) to P_max. M19.3's refinement does not reach
//! the regions: their legs run on M21.9's fixed grids.

use phasekit_core::{Error, Fluid, Pair, Phase, math};

use crate::batch::{Batch, FluidBatch, Inputs, Row, along};
use crate::plane::Quantity;
use crate::ranges::{Limits, away_from, dome_temperatures, lin_grid, log_grid, ranges};

/// A state on a region's edge, by its value of each plane's axes, D, H and S per unit mass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    /// Temperature, K.
    pub t: f64,
    /// Pressure, Pa.
    pub p: f64,
    /// Specific enthalpy, J/kg.
    pub h: f64,
    /// Specific entropy, J/(kg·K).
    pub s: f64,
    /// Density, kg/m³.
    pub rho: f64,
}

impl Corner {
    /// Its value of `quantity`; `None` for quality, which no plane plots.
    pub fn get(&self, quantity: Quantity) -> Option<f64> {
        match quantity {
            Quantity::T => Some(self.t),
            Quantity::P => Some(self.p),
            Quantity::Hmass => Some(self.h),
            Quantity::Smass => Some(self.s),
            Quantity::Dmass => Some(self.rho),
            Quantity::Q => None,
        }
    }
}

/// A phase region: the library's phase at a state inside it, and its outline.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// The library's phase at a state inside the region.
    pub phase: Phase,
    /// Its outline, a closed polygon of states.
    pub outline: Vec<Corner>,
}

/// A fluid's phase regions and the critical lines between them.
#[derive(Clone, Debug, PartialEq)]
pub struct Regions {
    /// Each region whose phase the library gives: above p_c and T_c, below p_c and above T_c, above p_c and below T_c,
    /// then, where the saturated liquid's middle state is the library's, the liquid, the gas and the liquid + vapour.
    pub regions: Vec<Region>,
    /// The critical isotherm, up in p through the critical point, broken where a cell failed.
    pub isotherm: Vec<Vec<Corner>>,
    /// The critical isobar, up in T through the critical point, broken where a cell failed.
    pub isobar: Vec<Vec<Corner>>,
    /// The critical point, the library's state at (T_c, ρ_c), where it has one: an edge without it joins across.
    pub critical: Option<Corner>,
    /// The fluid's critical temperature, K, its constant.
    pub t_critical: f64,
    /// The fluid's critical pressure, Pa, its constant.
    pub p_critical: f64,
}

/// The fluid's phase regions, by its own batch under the default options.
pub fn regions(fluid: &Fluid) -> Result<Regions, Error> {
    let batch = FluidBatch::new(fluid);
    sample_regions(&batch, &|inputs| batch.phase(inputs), &Limits::of(fluid)?)
}

/// The phase regions of the fluid whose batch and limits these are: `phase` gives the library's phase at the state of
/// some inputs, `None` where it refuses the state, and a region whose phase it refuses is left out.
pub fn sample_regions(
    batch: &impl Batch,
    phase: &impl Fn(Inputs) -> Option<Phase>,
    limits: &Limits,
) -> Result<Regions, Error> {
    let s = Sampler { batch, low: ranges(limits).t.0, t_c: limits.t_critical, p_c: limits.p_critical };
    let critical = s.states((Quantity::T, &[s.t_c]), (Quantity::Dmass, &[limits.rho_critical]))?;
    let critical = critical.into_iter().next().flatten();
    let temperatures = dome_temperatures(s.low, s.t_c, DOME_POINTS);
    let side = |q: f64| s.states((Quantity::T, &temperatures), (Quantity::Q, &vec![q; temperatures.len()]));
    let dome = Dome { liquid: side(0.0)?, vapour: side(1.0)?, critical };
    let (t_critical, p_critical) = (s.t_c, s.p_c);
    let Some(saturated) = dome.liquid.first().copied().flatten() else {
        return Ok(Regions {
            regions: Vec::new(),
            isotherm: Vec::new(),
            isobar: Vec::new(),
            critical,
            t_critical,
            p_critical,
        });
    };
    let e = Edges::sample(&s, limits, saturated)?;
    let seeded = outlines(&e, &dome, (s.low, t_critical, p_critical));
    let regions = seeded.into_iter().filter_map(|(seed, outline)| region(phase(seed)?, outline));
    let isotherm = runs(e.tc_down.iter().rev().chain([&critical]).chain(&e.tc_up).copied());
    let isobar = runs(e.pc_left.iter().rev().chain([&critical]).chain(&e.pc_right).copied());
    Ok(Regions { regions: regions.collect(), isotherm, isobar, critical, t_critical, p_critical })
}

/// The points an edge's leg takes, M21.9's.
const POINTS: usize = 160;

/// The temperatures the dome's sides take, closing in on T_c.
const DOME_POINTS: usize = 120;

/// How many isobars the cold edge crosses on either side of p_c, and how many halvings find each its temperature.
const COLD_ROWS: usize = 48;
const HALVINGS: usize = 12;

/// The outputs every corner carries: each plane's axes.
const AXES: [Quantity; 5] = [Quantity::T, Quantity::P, Quantity::Hmass, Quantity::Smass, Quantity::Dmass];

/// The row's corner, where every output is a value.
fn corner(row: &Row) -> Option<Corner> {
    match row.cells.as_slice() {
        &[t, p, h, s, rho] if row.ok.iter().all(|&ok| ok) => Some(Corner { t, p, h, s, rho }),
        _ => None,
    }
}

/// Runs of consecutive states, broken at each failed cell; a run of one draws nothing and is dropped.
fn runs(states: impl IntoIterator<Item = Option<Corner>>) -> Vec<Vec<Corner>> {
    let mut out = vec![Vec::new()];
    for state in states {
        match (state, out.last_mut()) {
            (Some(c), Some(run)) => run.push(c),
            (Some(_), None) => {}
            (None, _) => out.push(Vec::new()),
        }
    }
    out.retain(|run| run.len() > 1);
    out
}

/// A region of `phase` whose outline is the states that are the library's.
fn region(phase: Phase, outline: Vec<Option<Corner>>) -> Option<Region> {
    Some(Region { phase, outline: outline.into_iter().flatten().collect() })
}

/// The batch's states along the regions' edges.
struct Sampler<'a, B> {
    batch: &'a B,
    low: f64,
    t_c: f64,
    p_c: f64,
}

/// A leg of states, a cell failed where it has none.
type Leg = Vec<Option<Corner>>;

impl<B: Batch> Sampler<'_, B> {
    /// The states where two quantities take these values.
    fn states(&self, a: (Quantity, &[f64]), b: (Quantity, &[f64])) -> Result<Leg, Error> {
        Ok(along(self.batch, &AXES, a, b)?.iter().map(corner).collect())
    }

    /// An isotherm over pressures from `a` to `b`.
    fn isotherm(&self, t: f64, a: f64, b: f64) -> Result<Leg, Error> {
        self.states((Quantity::T, &[t; POINTS]), (Quantity::P, &log_grid(a, b, POINTS)))
    }

    /// An isobar over temperatures from `a` to `b`.
    fn isobar(&self, p: f64, a: f64, b: f64) -> Result<Leg, Error> {
        self.states((Quantity::T, &lin_grid(a, b, POINTS)), (Quantity::P, &[p; POINTS]))
    }

    /// The critical isobar from T_c to `b`, closing in on T_c: there c_p grows without bound and the isobar lies flat
    /// on T–s, so a first leg a hundred and sixtieth of the range long cut across its bend, and Nitrogen's
    /// supercritical gas took in states above p_c (PLAN.md M23.2). Its first cell, the critical point by (T, p), is
    /// left out: the library's critical point by (T, ρ) replaces it.
    fn critical_isobar(&self, b: f64) -> Result<Leg, Error> {
        let leg = self.states((Quantity::T, &away_from(self.t_c, b, POINTS)), (Quantity::P, &[self.p_c; POINTS]))?;
        Ok(from_critical(leg))
    }

    /// The cold edge over `pressures`: on each isobar the lowest temperature below its `tops` at which the library has
    /// a state, to within (top − T)/2^12: the isotherm at the lowest T where the library has it, the melting line where
    /// it does not. An isobar with no state below its top has no vertex.
    fn cold(&self, pressures: &[f64], tops: &[f64]) -> Result<Leg, Error> {
        let mut found = self.states((Quantity::T, &vec![self.low; pressures.len()]), (Quantity::P, pressures))?;
        let open: Vec<usize> = found.iter().enumerate().filter(|(_, c)| c.is_none()).map(|(i, _)| i).collect();
        let at: Vec<f64> = open.iter().filter_map(|&i| pressures.get(i).copied()).collect();
        let mut bounds: Vec<(f64, f64)> =
            open.iter().map(|&i| (self.low, tops.get(i).copied().unwrap_or(self.t_c))).collect();
        if open.is_empty() {
            return Ok(found);
        }
        for _ in 0..HALVINGS {
            let mids: Vec<f64> = bounds.iter().map(|(a, b)| (a + b) / 2.0).collect();
            let got = self.states((Quantity::T, &mids), (Quantity::P, &at))?;
            for ((&i, bound), (mid, c)) in open.iter().zip(&mut bounds).zip(mids.into_iter().zip(got)) {
                halve(bound, found.get_mut(i), mid, c);
            }
        }
        Ok(found)
    }
}

/// One halving of an isobar's bracket: a state at `mid` lowers its top and is its vertex so far; none raises its
/// bottom.
fn halve(bound: &mut (f64, f64), slot: Option<&mut Option<Corner>>, mid: f64, c: Option<Corner>) {
    match (c, slot) {
        (Some(c), Some(slot)) => {
            bound.1 = mid;
            *slot = Some(c);
        }
        (Some(_), None) => bound.1 = mid,
        (None, _) => bound.0 = mid,
    }
}

/// A leg from the critical point without its first cell, the critical point by (T, p), which the library's by (T, ρ)
/// replaces.
fn from_critical(leg: Leg) -> Leg {
    leg.into_iter().skip(1).collect()
}

/// `rows` pressures from `a` onto the saturation pressure `b`, short of it: a state there is on the curve, and either
/// phase.
fn to_saturation(a: f64, b: f64, rows: usize) -> Vec<f64> {
    let mut all = log_grid(a, b, rows);
    all.pop();
    all
}

/// The dome's sides on the regions' temperatures, and the critical point.
struct Dome {
    liquid: Leg,
    vapour: Leg,
    critical: Option<Corner>,
}

/// Every edge the regions share: the critical isotherm down and up from p_c, the critical isobar left and right from
/// T_c, the isotherm at T_max, the isobars at P_max and at the floor, and the cold edges.
struct Edges {
    tc_down: Leg,
    tc_up: Leg,
    pc_left: Leg,
    pc_right: Leg,
    hot_down: Leg,
    hot_up: Leg,
    top_left: Leg,
    top_right: Leg,
    floor_left: Leg,
    floor_right: Leg,
    cold_high: Leg,
    cold_low: Leg,
    cold_gas: Leg,
    /// T_max, the lower of the range's lowest pressure and half the saturation pressure at the lowest T, and P_max.
    high: f64,
    floor: f64,
    top: f64,
}

impl Edges {
    fn sample(s: &Sampler<'_, impl Batch>, limits: &Limits, saturated: Corner) -> Result<Edges, Error> {
        let (low, high, top) = (s.low, limits.t_max, limits.p_max);
        let (t_c, p_c) = (s.t_c, s.p_c);
        let floor = ranges(limits).p.0.min(saturated.p / 2.0);
        // Below p_c an isobar is halved below its bubble temperature, T_c where the library has none: a pseudo-pure
        // fluid has no state between its bubble and dew temperatures, and a halving that landed there would climb to
        // the dew side (#253).
        let low_pressures = to_saturation(p_c, saturated.p, COLD_ROWS);
        let bubbles = s.states((Quantity::P, &low_pressures), (Quantity::Q, &vec![0.0; low_pressures.len()]))?;
        let bubble_tops: Vec<f64> = bubbles.iter().map(|b| b.map_or(t_c, |b| b.t)).collect();
        let gas_pressures = to_saturation(floor, saturated.p, POINTS);
        Ok(Edges {
            tc_down: from_critical(s.isotherm(t_c, p_c, floor)?),
            tc_up: from_critical(s.isotherm(t_c, p_c, top)?),
            pc_left: s.critical_isobar(low)?,
            pc_right: s.critical_isobar(high)?,
            hot_down: s.isotherm(high, p_c, floor)?,
            hot_up: s.isotherm(high, p_c, top)?,
            top_left: s.isobar(top, t_c, low)?,
            top_right: s.isobar(top, t_c, high)?,
            floor_left: s.isobar(floor, t_c, low)?,
            floor_right: s.isobar(floor, t_c, high)?,
            cold_high: s.cold(&log_grid(top, p_c, COLD_ROWS), &[t_c; COLD_ROWS])?,
            cold_low: s.cold(&low_pressures, &bubble_tops)?,
            cold_gas: s.states((Quantity::T, &vec![low; gas_pressures.len()]), (Quantity::P, &gas_pressures))?,
            high,
            floor,
            top,
        })
    }
}

/// The states `p` and `t` name, as a seed of the phase.
fn at_pt(p: f64, t: f64) -> Inputs {
    Inputs { pair: Pair::PT, x: p, y: t }
}

/// Each region's seed, a state inside it, and its outline: the three about the critical point that lie above p_c or
/// T_c, then, where the saturated liquid's middle state is the library's, the liquid, the gas and the liquid + vapour.
fn outlines(e: &Edges, d: &Dome, (low, t_c, p_c): (f64, f64, f64)) -> Vec<(Inputs, Leg)> {
    let rev = |leg: &Leg| -> Leg { leg.iter().rev().copied().collect() };
    let critical: &[Option<Corner>] = &[d.critical];
    let (warm, mean) = ((t_c + e.high) / 2.0, math::sqrt(p_c * e.top));
    let mut out = vec![
        (at_pt(mean, warm), [critical, &e.pc_right, &e.hot_up, &rev(&e.top_right), &rev(&e.tc_up)].concat()),
        (
            at_pt(math::sqrt(e.floor * p_c), warm),
            [critical, &e.tc_down, &e.floor_right, &rev(&e.hot_down), &rev(&e.pc_right)].concat(),
        ),
        (at_pt(mean, (low + t_c) / 2.0), [critical, &e.tc_up, &e.top_left, &e.cold_high, &rev(&e.pc_left)].concat()),
    ];
    let Some(m) = d.liquid.get(d.liquid.len() / 2).copied().flatten() else {
        return out;
    };
    out.extend([
        (at_pt(math::sqrt(m.p * p_c), m.t), [&d.liquid, critical, &e.pc_left, &e.cold_low].concat()),
        (at_pt(math::sqrt(e.floor * m.p), m.t), [&d.vapour, critical, &e.tc_down, &e.floor_left, &e.cold_gas].concat()),
        (Inputs { pair: Pair::QT, x: 0.5, y: m.t }, [&d.liquid, critical, &rev(&d.vapour)].concat()),
    ]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(t: f64) -> Option<Corner> {
        Some(Corner { t, p: 1.0, h: 0.0, s: 0.0, rho: 0.0 })
    }

    /// The site's runs (M21.9): a failed cell breaks an edge, and a run of one draws nothing.
    #[test]
    fn edges_break_where_a_cell_failed_and_a_run_of_one_draws_nothing() {
        let got = runs([c(1.0), c(2.0), None, c(3.0), None, c(4.0), c(5.0), c(6.0)]);
        let ts: Vec<Vec<f64>> = got.iter().map(|run| run.iter().map(|c| c.t).collect()).collect();
        assert_eq!(ts, [vec![1.0, 2.0], vec![4.0, 5.0, 6.0]]);
        assert!(runs([None, c(1.0), None]).is_empty());
    }
}
