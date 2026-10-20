//! A diagram's ranges, grids and isoline values (PLAN.md M19.1, the site's `ranges.ts` of M21.2), from the fluid's own
//! constants and limits.

use phasekit_core::{Constant, Error, Fluid, math};

use crate::plane::Quantity;

/// The constants a diagram needs: the critical point, the triple point and the equation of state's limits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// The critical temperature, K.
    pub t_critical: f64,
    /// The critical pressure, Pa.
    pub p_critical: f64,
    /// The critical density, kg/m³.
    pub rho_critical: f64,
    /// The triple point's temperature, K.
    pub t_triple: f64,
    /// The triple point's pressure, Pa.
    pub p_triple: f64,
    /// The lowest temperature of the equation of state, K.
    pub t_min: f64,
    /// The highest temperature of the equation of state, K.
    pub t_max: f64,
    /// The highest pressure of the equation of state, Pa.
    pub p_max: f64,
}

impl Limits {
    /// The fluid's, as `Props1SI` reads them (`Fluid::constant`); the first constant the model lacks is the error.
    pub fn of(fluid: &Fluid) -> Result<Limits, Error> {
        let c = |k| fluid.constant(k);
        Ok(Limits {
            t_critical: c(Constant::TCritical)?,
            p_critical: c(Constant::PCritical)?,
            rho_critical: c(Constant::RhomassCritical)?,
            t_triple: c(Constant::TTriple)?,
            p_triple: c(Constant::PTriple)?,
            t_min: c(Constant::TMin)?,
            t_max: c(Constant::TMax)?,
            p_max: c(Constant::PMax)?,
        })
    }
}

/// The plotted temperatures and pressures, each from its least to its greatest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ranges {
    pub t: (f64, f64),
    pub p: (f64, f64),
}

/// From the triple point (or the equation of state's lowest T) to 1.6 T_c, and from p_c / 10⁴ (or the triple point's
/// pressure) to 10 p_c, within the equation of state's limits.
pub(crate) fn ranges(l: &Limits) -> Ranges {
    Ranges {
        t: (l.t_min.max(l.t_triple), l.t_max.min(l.t_critical * 1.6)),
        p: (l.p_triple.max(l.p_critical / 1e4), l.p_max.min(l.p_critical * 10.0)),
    }
}

/// `n` values, the last exactly `b` and the others `at(i, n − 1)`.
fn spaced(b: f64, n: usize, at: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    let last = n.saturating_sub(1);
    (0..n).map(|i| if i == last { b } else { at(i as f64, last as f64) }).collect()
}

/// `n` values from `a` to `b`, evenly spaced, ending exactly on `b`.
pub(crate) fn lin_grid(a: f64, b: f64, n: usize) -> Vec<f64> {
    spaced(b, n, |i, last| a + ((b - a) * i) / last)
}

/// `n` values from `a` to `b`, evenly spaced in their logarithm, ending exactly on `b`.
pub(crate) fn log_grid(a: f64, b: f64, n: usize) -> Vec<f64> {
    spaced(b, n, |i, last| a * math::powf(b / a, i / last))
}

/// `n` values from `a` to exactly `b`, closing in on `a`: a + (b − a)·(i/(n − 1))², the site's `awayFrom` (M23.2).
pub(crate) fn away_from(a: f64, b: f64, n: usize) -> Vec<f64> {
    let last = n.saturating_sub(1);
    let at = |i: usize| {
        let x = i as f64 / last as f64;
        if i == last { b } else { a + (b - a) * (x * x) }
    };
    (0..n).map(at).collect()
}

/// `n` temperatures from exactly `low` up to just below T_c, closing in on it, where the dome narrows fastest:
/// T_c − (T_c − low)·(1 − i/n)².
pub(crate) fn dome_temperatures(low: f64, tc: f64, n: usize) -> Vec<f64> {
    let at = |i: usize| {
        let gap = 1.0 - i as f64 / n as f64;
        if i == 0 { low } else { tc - (tc - low) * (gap * gap) }
    };
    (0..n).map(at).collect()
}

/// The values a family's lines take, strictly inside `range`: 1-2-5 per decade for pressure and density while that is
/// at most 12 lines, decades past that; d3's nice linear ticks for the rest, about 8 (tenths, for quality's [0, 1]).
pub(crate) fn values(quantity: Quantity, range: (f64, f64)) -> Vec<f64> {
    let all = match quantity {
        Quantity::P | Quantity::Dmass => decades(range),
        _ => ticks(range),
    };
    all.into_iter().filter(|&v| range.0 < v && v < range.1).collect()
}

/// 1, 2 and 5 times each power of ten over the range while that is at most 12 values inside it, else the powers.
fn decades(range: (f64, f64)) -> Vec<f64> {
    let inside = |mantissas: &[f64]| -> Vec<f64> {
        let all = per_decade(range, mantissas);
        all.into_iter().filter(|&v| range.0 < v && v < range.1).collect()
    };
    let fine = inside(&[1.0, 2.0, 5.0]);
    if fine.len() <= 12 { fine } else { inside(&[1.0]) }
}

/// Each mantissa times each power of ten from the decade of the range's start to the one at or above its end; none
/// for a range that does not start above zero.
fn per_decade(range: (f64, f64), mantissas: &[f64]) -> Vec<f64> {
    if !(range.0 > 0.0 && range.1.is_finite()) {
        return Vec::new();
    }
    let (low, high) = (floor_log10(range.0), ceil_log10(range.1));
    (low..=high).flat_map(|k| mantissas.iter().map(move |&m| decimal(m, k))).collect()
}

/// m·10^k, correctly rounded for |k| ≤ 22: 10^k is exact there, and a negative power divides by 10^−k.
fn decimal(m: f64, k: i32) -> f64 {
    // At k = 0 either arm is m·1.
    // equivalent mutant: replace < with <=
    if k < 0 { m / math::powi(10.0, k.saturating_neg()) } else { m * math::powi(10.0, k) }
}

/// The k with 10^k ≤ x < 10^(k + 1), for a finite x > 0: the logarithm's estimate, corrected by one either way. The
/// estimate is held to the decades of finite floats, so that no input makes a range of decades unbounded.
fn floor_log10(x: f64) -> i32 {
    let k = (math::ln(x) / std::f64::consts::LN_10).floor().clamp(-330.0, 310.0) as i32;
    if decimal(1.0, k) > x {
        k.saturating_sub(1)
    } else if decimal(1.0, k.saturating_add(1)) <= x {
        k.saturating_add(1)
    } else {
        k
    }
}

/// The least k with 10^k ≥ x, for a finite x > 0.
fn ceil_log10(x: f64) -> i32 {
    let k = floor_log10(x);
    if decimal(1.0, k) < x { k.saturating_add(1) } else { k }
}

/// d3-array 3.2.4's `ticks(a, b, 8)`, which the site's linear scales called (M21.2): multiples of 1, 2 or 5 times a
/// power of ten, the step nearest (b − a)/8 in its logarithm, from the first at or above a to the last at or below b.
fn ticks((a, b): (f64, f64)) -> Vec<f64> {
    if a.partial_cmp(&b) != Some(std::cmp::Ordering::Less) {
        return Vec::new();
    }
    let step = (b - a) / 8.0;
    let power = floor_log10(step);
    let error = step / decimal(1.0, power);
    let factor = if error >= math::sqrt(50.0) {
        10.0
    } else if error >= math::sqrt(10.0) {
        5.0
    } else if error >= math::sqrt(2.0) {
        2.0
    } else {
        1.0
    };
    // At power 0 the step is the factor, and the arm below would divide by its inverse: the same ticks for 1 and 2,
    // whose inverses are exact, and for 5 and 10, whose multiples i/(1/f) round back to i·f (checked by brute force for
    // a from −300 to 300 and spans from 8 to 80).
    // equivalent mutant: replace < with <=
    if power < 0 {
        ticks_over(a, b, decimal(1.0, power.saturating_neg()) / factor)
    } else {
        ticks_by(a, b, decimal(factor, power))
    }
}

/// The multiples i·step in [a, b], for a step of at least 1.
fn ticks_by(a: f64, b: f64, step: f64) -> Vec<f64> {
    let (first, last) = (js_round(a / step), js_round(b / step));
    let first = if first * step < a { first + 1.0 } else { first };
    let last = if last * step > b { last - 1.0 } else { last };
    counted(first, last).map(|i| i * step).collect()
}

/// The fractions i/inverse in [a, b], for a step below 1 given as its inverse, which is exact.
fn ticks_over(a: f64, b: f64, inverse: f64) -> Vec<f64> {
    let (first, last) = (js_round(a * inverse), js_round(b * inverse));
    let first = if first / inverse < a { first + 1.0 } else { first };
    let last = if last / inverse > b { last - 1.0 } else { last };
    counted(first, last).map(|i| i / inverse).collect()
}

/// The integers from `first` to `last` as floats, none where `last` is below `first`.
fn counted(first: f64, last: f64) -> impl Iterator<Item = f64> {
    let n = if last >= first { ((last - first) as usize).saturating_add(1) } else { 0 };
    (0..n).map(move |i| first + i as f64)
}

/// JavaScript's `Math.round`: to the nearest integer, a half up.
fn js_round(x: f64) -> f64 {
    let down = x.floor();
    if x - down >= 0.5 { down + 1.0 } else { down }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WATER: Limits = Limits {
        t_critical: 647.096,
        p_critical: 22_064_000.0,
        rho_critical: 322.0,
        t_triple: 273.16,
        p_triple: 611.65,
        t_min: 273.16,
        t_max: 2000.0,
        p_max: 1e9,
    };

    /// The site's ranges (M21.2): the fluid's range up to 1.6 T_c and 10 p_c.
    #[test]
    fn ranges_span_the_fluids_range_up_to_1_6_tc_and_10_pc() {
        assert_eq!(ranges(&WATER), Ranges { t: (273.16, 647.096 * 1.6), p: (2206.4, 220_640_000.0) });
        let hot = Limits { t_max: 700.0, p_max: 1e8, p_triple: 1e4, t_min: 300.0, ..WATER };
        assert_eq!(ranges(&hot).t, (300.0, 700.0));
        assert_eq!(ranges(&Limits { t_min: 273.16, ..hot }).p, (1e4, 1e8));
    }

    /// Each grid runs from its first end to exactly its last, which a computed end would round off.
    #[test]
    fn grids_run_from_end_to_end() {
        assert_eq!(lin_grid(0.0, 10.0, 6), [0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(lin_grid(1.0, 3.0, 3), [1.0, 2.0, 3.0]);
        assert_eq!(lin_grid(0.2, 0.9, 8).last(), Some(&0.9));
        let log: Vec<f64> = log_grid(1.0, 1000.0, 4).iter().map(|v| v.round()).collect();
        assert_eq!(log, [1.0, 10.0, 100.0, 1000.0]);
        assert_eq!(log_grid(2.0, 8.0, 3), [2.0, 4.0, 8.0]);
        assert_eq!(log_grid(0.1, 27.500_000_000_000_004, 3).last(), Some(&27.500_000_000_000_004));
    }

    /// The dome's temperatures start on exactly the lowest, which T_c − (T_c − low) misses by a rounding for R12 and
    /// n-Pentane (their T_min and triple point; M21.4), and close in on T_c from below.
    #[test]
    fn dome_temperatures_close_in_on_tc_from_exactly_the_lowest() {
        let ts = dome_temperatures(273.16, 647.096, 120);
        assert_eq!((ts.len(), ts.first()), (120, Some(&273.16)));
        assert!(ts.windows(2).all(|w| w[1] > w[0]));
        let last = ts.last().copied().unwrap_or(f64::NAN);
        assert!(last < 647.096 && 647.096 - last < 0.1, "{last}");
        // Halfway along, (1 − i/n)² is a quarter.
        assert_eq!(ts[60], 647.096 - (647.096 - 273.16) * 0.25);
        for (low, tc) in [(116.099, 385.119_999_768_139_2), (143.47, 469.699_999_871_119_5)] {
            assert_eq!(dome_temperatures(low, tc, 120).first(), Some(&low));
        }
    }

    /// The site's isoline values (M21.2): nice ticks strictly inside; 1-2-5 per decade for pressure and density,
    /// decades past 12 lines; tenths of quality.
    #[test]
    fn isoline_values_lie_strictly_inside_on_nice_numbers() {
        assert_eq!(values(Quantity::T, (273.16, 1035.0)), [300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0, 1000.0]);
        assert_eq!(values(Quantity::P, (2206.4, 2.2064e8)), [1e4, 1e5, 1e6, 1e7, 1e8]);
        let fine = [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0];
        assert_eq!(values(Quantity::Dmass, (0.05, 1000.0)), fine);
        assert_eq!(values(Quantity::Dmass, (0.0048, 999.0)), [0.01, 0.1, 1.0, 10.0, 100.0]);
        let s = [1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0];
        assert_eq!(values(Quantity::Smass, (0.0, 9000.0)), s);
        assert_eq!(values(Quantity::Q, (0.0, 1.0)), [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
    }

    /// A range that does not start above zero, or does not end finite, has no decades; a power of ten's decade is its
    /// own, and that of a float just below one, which the logarithm's estimate overshoots, the decade under it.
    #[test]
    fn decades_are_exact_at_and_just_below_a_power_of_ten() {
        assert!(values(Quantity::P, (0.0, 100.0)).is_empty());
        assert!(values(Quantity::P, (-1.0, 100.0)).is_empty());
        assert!(values(Quantity::Dmass, (1.0, f64::INFINITY)).is_empty());
        let below = |x: f64| f64::from_bits(x.to_bits() - 1);
        assert_eq!([floor_log10(100.0), floor_log10(below(1e5)), floor_log10(1e3)], [2, 4, 3]);
        assert_eq!([ceil_log10(100.0), ceil_log10(150.0)], [2, 3]);
    }

    /// d3's ticks (M21.2) run from the first multiple at or above a to the last at or below b: a step of at least 1 by
    /// multiplying, a smaller one by dividing by its inverse, at the factor of 1, 2 or 5 nearest an eighth of the span.
    #[test]
    fn ticks_run_from_the_first_multiple_at_or_above_a_to_the_last_at_or_below_b() {
        let tens = |from: u8, to: u8| (from..=to).map(|i| f64::from(i) * 10.0).collect::<Vec<_>>();
        assert_eq!(ticks((10.0, 100.0)), tens(1, 10));
        assert_eq!(ticks((12.0, 100.0)), tens(2, 10));
        assert_eq!(ticks((12.0, 97.0)), tens(2, 9));
        let tenths = |from: u8, to: u8| (from..=to).map(|i| f64::from(i) / 10.0).collect::<Vec<_>>();
        assert_eq!(ticks((0.1, 1.0)), tenths(1, 10));
        assert_eq!(ticks((0.12, 0.97)), tenths(2, 9));
        assert_eq!(ticks((0.0, 1.5)), [0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2, 1.4]);
    }

    /// JavaScript's `Math.round`, which d3's ticks call: a half rounds up, toward +∞, on either side of zero.
    #[test]
    fn js_round_rounds_a_half_up() {
        let got = [2.5, -2.5, 2.4, -2.6, 3.0].map(js_round);
        assert_eq!(got, [3.0, -2.0, 2.0, -3.0, 3.0]);
    }
}
