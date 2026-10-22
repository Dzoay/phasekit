//! Mollier's h–x chart (PLAN.md M18.10; user decision HA7): M18.9's psychrometric chart in its European form, over the
//! same range and through the same seam ([`HumidAirBatch`]). W runs across and h up on skewed axes: a vertex is drawn
//! at W and y = h − r₀W, where r₀ is water vapour's h at 0 °C as the humid-air model has it, the slope of its 0 °C
//! isotherm from dry air to saturation, so that isotherm lies level. Its lines: the saturation line and φ at its tenths,
//! swept in T as M18.9's; isotherms at the range's ticks in °C, from dry air to saturation; h and v at 1-2-5 values,
//! swept in W; each refined in the drawn plane as M19.3's lines are ([`refine`]). The fog region below the saturation
//! line is left empty, since M18's states stop at saturation.

use phasekit_core::{Error, Pair};

use crate::batch::{Inputs, Row};
use crate::plane::{Axis, Quantity};
use crate::psychro::{
    ChartLine, ChartRange, Held, HumidAirBatch, celsius_ticks, family, in_t, inside, runs, sweep, sweep_w,
};
use crate::refine::{Halving, Sides, Square, points, refine};

/// 0 °C, K: where r₀ is taken.
const T_ZERO: f64 = 273.15;

/// The refusal where r₀ has no value: water does not saturate air at 0 °C below the chart's pressure.
const NO_R0: Error = Error::InvalidState { reason: "Mollier's chart needs air saturated with water at 0 °C" };

/// A point of Mollier's chart: the state's dry-bulb T, W and h, and the height it is drawn at, h − r₀W.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MollierVertex {
    /// The dry-bulb temperature, K.
    pub t: f64,
    /// The humidity ratio, kg/kg: the drawn x.
    pub w: f64,
    /// The enthalpy, J per kilogram of dry air.
    pub h: f64,
    /// The drawn height, h − r₀W, J per kilogram of dry air.
    pub y: f64,
}

/// Mollier's h–x chart at one pressure.
#[derive(Clone, Debug, PartialEq)]
pub struct Mollier {
    /// The pressure, Pa.
    pub p: f64,
    /// The range: the states drawn are the psychrometric chart's.
    pub range: ChartRange,
    /// The skew: water vapour's h at 0 °C, J/kg.
    pub r0: f64,
    /// The saturation line.
    pub saturation: ChartLine<MollierVertex>,
    /// The φ lines, the isotherms and the h and v lines, in that order, each family's values rising.
    pub lines: Vec<ChartLine<MollierVertex>>,
}

/// One point of a line before it is a row: its T, W and h, each where there is one.
type State = (Option<f64>, Option<f64>, Option<f64>);

/// r₀: the slope of the 0 °C isotherm at `batch`'s pressure from dry air to saturation.
fn r0(batch: &dyn HumidAirBatch) -> Result<f64, Error> {
    let saturated = batch.ratios(Held::Saturation, &[T_ZERO])?.first().copied().flatten();
    let w_s = saturated.filter(|w| *w > 0.0).ok_or(NO_R0)?;
    match batch.enthalpies(&[(T_ZERO, 0.0), (T_ZERO, w_s)])?.as_slice() {
        [Some(dry), Some(wet)] => Ok((wet - dry) / w_s),
        _ => Err(NO_R0),
    }
}

/// h at each (T, W) where both are values, in one batch call; `None` elsewhere.
fn enthalpies(batch: &dyn HumidAirBatch, tw: &[(Option<f64>, Option<f64>)]) -> Result<Vec<Option<f64>>, Error> {
    let at: Vec<(f64, f64)> = tw.iter().filter_map(|&(t, w)| Some((t?, w?))).collect();
    let mut h = batch.enthalpies(&at)?.into_iter();
    Ok(tw.iter().map(|(t, w)| if t.is_some() && w.is_some() { h.next().flatten() } else { None }).collect())
}

/// The states of a line of `held` at its sweep values `at`: in T for the saturation and φ lines, in W for the others.
fn states(batch: &dyn HumidAirBatch, held: Held, at: &[f64]) -> Result<Vec<State>, Error> {
    let tw: Vec<(Option<f64>, Option<f64>)> = match held {
        _ if in_t(held) => at.iter().map(|&t| Some(t)).zip(batch.ratios(held, at)?).collect(),
        Held::DryBulb(t) => at.iter().map(|&w| (Some(t), Some(w))).collect(),
        _ => batch.temperatures(held, at)?.into_iter().zip(at.iter().map(|&w| Some(w))).collect(),
    };
    let h = match held {
        Held::Enthalpy(h) => tw.iter().map(|(t, _)| t.map(|_| h)).collect(),
        _ => enthalpies(batch, &tw)?,
    };
    Ok(tw.into_iter().zip(h).map(|((t, w), h)| (t, w, h)).collect())
}

/// A row at a state: its cells W, y, T and h, a vertex where all are values and (T, W) inside `range`.
fn row(p: f64, (range, r0): (ChartRange, f64), (t, w, h): State) -> Row {
    let ok = h.is_some() && inside(range, (t, w));
    let (t, w, h) = (t.unwrap_or(f64::NAN), w.unwrap_or(f64::NAN), h.unwrap_or(f64::NAN));
    let y = h - r0 * w;
    Row { cells: vec![w, y, t, h], ok: vec![ok; 4], inputs: Inputs { pair: Pair::PT, x: p, y: t } }
}

/// The top of an isotherm's sweep: its saturation, or the range's top W where that is lower or there is none.
fn isotherm_top(batch: &dyn HumidAirBatch, t: f64, w_max: f64) -> Result<f64, Error> {
    let saturated = batch.ratios(Held::Saturation, &[t])?.first().copied().flatten();
    Ok(saturated.map_or(w_max, |w| w.min(w_max)))
}

/// The span of the drawn height over the range: y at dry air and at its top W, at each end of T.
fn y_span(batch: &dyn HumidAirBatch, (range, r0): (ChartRange, f64)) -> Result<(f64, f64), Error> {
    let (lo, hi) = range.t;
    let (top_lo, top_hi) = (isotherm_top(batch, lo, range.w_max)?, isotherm_top(batch, hi, range.w_max)?);
    let corners = [(lo, 0.0), (lo, top_lo), (hi, 0.0), (hi, top_hi)];
    let h = batch.enthalpies(&corners)?;
    let y = corners.iter().zip(h).filter_map(|(&(_, w), h)| h.map(|h| h - r0 * w));
    let (y_lo, y_hi) = y.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| (a.min(y), b.max(y)));
    Ok(if y_lo < y_hi { (y_lo, y_hi) } else { (0.0, 1.0) })
}

/// The line of `held` over the chart, refined in the drawn plane `square`.
fn line(
    batch: &dyn HumidAirBatch,
    held: Held,
    (range, r0): (ChartRange, f64),
    square: &Square,
) -> Result<ChartLine<MollierVertex>, Error> {
    let eval = |at: &[f64]| -> Result<Vec<Row>, Error> {
        Ok(states(batch, held, at)?.into_iter().map(|s| row(batch.p(), (range, r0), s)).collect())
    };
    let at = match held {
        _ if in_t(held) => sweep(range.t),
        Held::DryBulb(t) => sweep_w(isotherm_top(batch, t, range.w_max)?),
        _ => sweep_w(range.w_max),
    };
    let swept = refine(points(&at, eval(&at)?), (Halving::Linear, Sides::One), square, eval)?;
    let segments = runs(swept, |cells| match cells {
        [w, y, t, h] => Some(MollierVertex { t: *t, w: *w, h: *h, y: *y }),
        _ => None,
    });
    Ok(ChartLine { held, segments })
}

/// Mollier's h–x chart of `batch` over `range` (see the module's notes).
pub fn mollier(batch: &dyn HumidAirBatch, range: ChartRange) -> Result<Mollier, Error> {
    let r0 = r0(batch)?;
    let axis = Axis { quantity: Quantity::T, log: false };
    let square = Square::new((axis, (0.0, range.w_max)), (axis, y_span(batch, (range, r0))?));
    let saturation = line(batch, Held::Saturation, (range, r0), &square)?;
    let relative = (1..10).map(|k| Held::Relative(f64::from(k) / 10.0));
    let isotherms = celsius_ticks(range.t).into_iter().map(Held::DryBulb);
    let enthalpy = family(batch, range, 0)?.into_iter().map(Held::Enthalpy);
    let volume = family(batch, range, 1)?.into_iter().map(Held::Volume);
    let held: Vec<Held> = relative.chain(isotherms).chain(enthalpy).chain(volume).collect();
    let lines = held.into_iter().map(|h| line(batch, h, (range, r0), &square)).collect::<Result<Vec<_>, _>>()?;
    Ok(Mollier { p: batch.p(), range, r0, saturation, lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A batch whose saturation W is `w_s` at every T and whose h is `h(T, W)`; it has no other states.
    struct Fake {
        w_s: Option<f64>,
        h: fn(f64, f64) -> Option<f64>,
    }

    impl HumidAirBatch for Fake {
        fn p(&self) -> f64 {
            101_325.0
        }
        fn ratios(&self, _: Held, t: &[f64]) -> Result<Vec<Option<f64>>, Error> {
            Ok(vec![self.w_s; t.len()])
        }
        fn temperatures(&self, _: Held, w: &[f64]) -> Result<Vec<Option<f64>>, Error> {
            Ok(vec![None; w.len()])
        }
        fn held_at(&self, _: f64, _: f64) -> Result<[f64; 3], Error> {
            Err(NO_R0)
        }
        fn enthalpies(&self, at: &[(f64, f64)]) -> Result<Vec<Option<f64>>, Error> {
            Ok(at.iter().map(|&(t, w)| (self.h)(t, w)).collect())
        }
    }

    /// Saturated air with no water at 0 °C has no r₀: its slope would divide by a zero W.
    #[test]
    fn r0_needs_water_at_saturation() {
        let batch = Fake { w_s: Some(0.0), h: |t, w| Some(1000.0 * t + 2.5e6 * w) };
        assert_eq!(r0(&batch), Err(NO_R0));
    }

    /// Each h stays with its own (T, W): a point without a W takes none, and the next point keeps its own.
    #[test]
    fn enthalpies_skip_the_points_without_both() {
        let batch = Fake { w_s: None, h: |t, w| Some(t + w) };
        let h = enthalpies(&batch, &[(Some(1.0), None), (Some(2.0), Some(3.0)), (None, Some(4.0))]).unwrap();
        assert_eq!(h, [None, Some(5.0), None]);
    }

    /// A range whose corners are drawn at one height spans 0 to 1 instead: the square must not be flat.
    #[test]
    fn a_flat_y_span_is_the_unit_span() {
        let batch = Fake { w_s: Some(0.01), h: |_, _| Some(7.0) };
        assert_eq!(y_span(&batch, (ChartRange::default(), 0.0)).unwrap(), (0.0, 1.0));
    }
}
