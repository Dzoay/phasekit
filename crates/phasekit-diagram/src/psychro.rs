//! The psychrometric chart (PLAN.md M18.9; user decisions HA3, DG3): humid air at one pressure, ASHRAE's form, dry-bulb
//! T across and the humidity ratio W up. Its lines: the saturation line, φ at its tenths, and h, v and T_wb at nice
//! 1-2-5 values ([`ranges`]' ticks) over what the range's corners span. Each line is a run of states the caller's humid
//! air computed, refined as M19.3's lines are ([`refine`]). The saturation and φ lines are swept in T, from the range's
//! ticks in °C; the h, v and T_wb lines, each from dry air up to saturation and some only a kelvin wide, in W from W's
//! ticks. A vertex outside the range is a gap whose edge is closed in on. The crate stays on core alone (DG3's rule):
//! the chart samples through [`HumidAirBatch`], which the browser package fills from `phasekit-humidair` and the crate's
//! tests through a dev-dependency.

use phasekit_core::{Error, Pair};

use crate::batch::{Inputs, Row};
use crate::plane::{Axis, Quantity};
use crate::ranges::ticks;
use crate::refine::{Halving, Sides, Square, points, refine};

/// What a line of the chart holds.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Held {
    /// Saturation, φ = 1.
    Saturation,
    /// The relative humidity φ, 0 to 1.
    Relative(f64),
    /// The enthalpy, J per kilogram of dry air.
    Enthalpy(f64),
    /// The volume, m³ per kilogram of dry air.
    Volume(f64),
    /// The wet-bulb temperature, K.
    WetBulb(f64),
}

/// Humid air at one pressure, as the chart samples it.
pub trait HumidAirBatch {
    /// The pressure, Pa.
    fn p(&self) -> f64;

    /// W, kg/kg, of the state at each dry-bulb T (K) that holds `held`; `None` where no state does.
    fn ratios(&self, held: Held, t: &[f64]) -> Result<Vec<Option<f64>>, Error>;

    /// The dry-bulb T, K, of the state at each W (kg/kg) that holds `held`; `None` where no state does.
    fn temperatures(&self, held: Held, w: &[f64]) -> Result<Vec<Option<f64>>, Error>;

    /// h (J/kg of dry air), v (m³/kg of dry air) and T_wb (K) of the state at (T, W).
    fn held_at(&self, t: f64, w: f64) -> Result<[f64; 3], Error>;
}

/// The chart's range: dry-bulb T, K, and W from 0 to its top, kg/kg.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartRange {
    /// The dry-bulb temperatures across, K.
    pub t: (f64, f64),
    /// The top of W.
    pub w_max: f64,
}

impl Default for ChartRange {
    /// 0 to 50 °C and W to 0.03, a comfort chart's.
    fn default() -> Self {
        ChartRange { t: (273.15, 323.15), w_max: 0.03 }
    }
}

/// A point of a chart's line: its dry-bulb T and its W, the state at T that holds the line's quantity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartVertex {
    /// The dry-bulb temperature, K.
    pub t: f64,
    /// The humidity ratio, kg/kg.
    pub w: f64,
}

/// A line of the chart: what it holds and its runs, a gap between two where no state lies inside the range.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartLine {
    /// What it holds.
    pub held: Held,
    /// Its runs, each at least two vertices long, in T's order.
    pub segments: Vec<Vec<ChartVertex>>,
}

/// The psychrometric chart at one pressure.
#[derive(Clone, Debug, PartialEq)]
pub struct Psychrometric {
    /// The pressure, Pa.
    pub p: f64,
    /// The range.
    pub range: ChartRange,
    /// The saturation line.
    pub saturation: ChartLine,
    /// The φ, h, v and T_wb lines, in that order, each family's values rising.
    pub lines: Vec<ChartLine>,
}

/// A row at (T, W): a vertex where both are values inside `range`.
fn row(p: f64, range: ChartRange, (t, w): (Option<f64>, Option<f64>)) -> Row {
    let inside = matches!((t, w), (Some(t), Some(w)) if (range.t.0..=range.t.1).contains(&t) && (0.0..=range.w_max).contains(&w));
    let (t, w) = (t.unwrap_or(f64::NAN), w.unwrap_or(f64::NAN));
    Row { cells: vec![t, w], ok: vec![inside, inside], inputs: Inputs { pair: Pair::PT, x: p, y: t } }
}

/// The rows of a line swept in T: W at each of `t`.
fn rows_in_t(batch: &dyn HumidAirBatch, held: Held, range: ChartRange, t: &[f64]) -> Result<Vec<Row>, Error> {
    let w = batch.ratios(held, t)?;
    Ok(t.iter().zip(w).map(|(&t, w)| row(batch.p(), range, (Some(t), w))).collect())
}

/// The rows of a line swept in W: T at each of `w`.
fn rows_in_w(batch: &dyn HumidAirBatch, held: Held, range: ChartRange, w: &[f64]) -> Result<Vec<Row>, Error> {
    let t = batch.temperatures(held, w)?;
    Ok(w.iter().zip(t).map(|(&w, t)| row(batch.p(), range, (t, Some(w)))).collect())
}

/// The temperatures a line's sweep starts on: the range's ticks in °C, and its ends.
fn sweep((lo, hi): (f64, f64)) -> Vec<f64> {
    let celsius = ticks((lo - 273.15, hi - 273.15)).into_iter().map(|c| c + 273.15);
    let mut t: Vec<f64> = [lo].into_iter().chain(celsius).chain([hi]).filter(|t| (lo..=hi).contains(t)).collect();
    t.dedup();
    t
}

/// Whether a line of `held` is swept in T: the saturation and φ lines; the others are swept in W.
fn in_t(held: Held) -> bool {
    matches!(held, Held::Saturation | Held::Relative(_))
}

/// The line of `held` over `range`, refined.
fn line(batch: &dyn HumidAirBatch, held: Held, range: ChartRange) -> Result<ChartLine, Error> {
    let axis = Axis { quantity: Quantity::T, log: false };
    let square = Square::new((axis, range.t), (axis, (0.0, range.w_max)));
    let eval = |at: &[f64]| {
        if in_t(held) { rows_in_t(batch, held, range, at) } else { rows_in_w(batch, held, range, at) }
    };
    let at = if in_t(held) { sweep(range.t) } else { sweep_w(range.w_max) };
    let swept = refine(points(&at, eval(&at)?), (Halving::Linear, Sides::One), &square, eval)?;
    let (mut segments, mut run) = (Vec::new(), Vec::new());
    for point in swept {
        match (point.row.ok.get(1), point.row.cells.as_slice()) {
            (Some(true), [t, w]) => run.push(ChartVertex { t: *t, w: *w }),
            _ => segments.push(std::mem::take(&mut run)),
        }
    }
    segments.push(run);
    segments.retain(|s| s.len() >= 2);
    Ok(ChartLine { held, segments })
}

/// The humidity ratios a line's sweep in W starts on: W's ticks from 0 to the top, both ends included.
fn sweep_w(w_max: f64) -> Vec<f64> {
    let mut w: Vec<f64> = [0.0].into_iter().chain(ticks((0.0, w_max))).chain([w_max]).collect();
    w.dedup();
    w
}

/// The values a family takes over what the range's corners span: its quantity at (T_lo, 0) and at (T_hi, the top or
/// saturation, whichever is lower), each `held_at` index `k`.
fn family(batch: &dyn HumidAirBatch, range: ChartRange, k: usize) -> Result<Vec<f64>, Error> {
    let (lo, hi) = range.t;
    let w_top =
        batch.ratios(Held::Saturation, &[hi])?.first().copied().flatten().map_or(range.w_max, |w| w.min(range.w_max));
    let (a, b) = (batch.held_at(lo, 0.0)?, batch.held_at(hi, w_top)?);
    let (a, b) = (a.get(k).copied().unwrap_or(f64::NAN), b.get(k).copied().unwrap_or(f64::NAN));
    Ok(ticks((a.min(b), a.max(b))))
}

/// The psychrometric chart of `batch` over `range` (see the module's notes).
pub fn psychrometric(batch: &dyn HumidAirBatch, range: ChartRange) -> Result<Psychrometric, Error> {
    let saturation = line(batch, Held::Saturation, range)?;
    let relative = (1..10).map(|k| Held::Relative(f64::from(k) / 10.0));
    let enthalpy = family(batch, range, 0)?.into_iter().map(Held::Enthalpy);
    let volume = family(batch, range, 1)?.into_iter().map(Held::Volume);
    let wet_bulb = family(batch, range, 2)?.into_iter().map(Held::WetBulb);
    let held: Vec<Held> = relative.chain(enthalpy).chain(volume).chain(wet_bulb).collect();
    let lines = held.into_iter().map(|h| line(batch, h, range)).collect::<Result<Vec<_>, _>>()?;
    Ok(Psychrometric { p: batch.p(), range, saturation, lines })
}
