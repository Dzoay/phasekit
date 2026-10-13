//! Melting curves (PLAN.md M8.11; map 02 §3.7, §6): CoolProp's three segment forms in its operation order
//! (`Ancillaries.h:200-244`), each segment evaluated only inside its own range (user decision 6): a Simon fit is never
//! inverted below its T_min (ROT-049: CoolProp's inversion checks T ≥ T₀, and gives Hydrogen 9.52 K below its 13.957 K
//! T_min), and no curve reaches past its last segment. Segments come in file order, which is increasing pressure, and
//! two adjacent ones share a temperature bound, where their fits meet; their printed constants leave them a little
//! apart in p (Water's ice V ends 0.65 kPa below ice VI's p₀ under `Corrected`; Propylene's two Simon fits are 92 MPa
//! apart at 129 K), and a pressure between them melts at that shared temperature.

use crate::data::{MeltingForm, MeltingSegment};
use crate::error::{DomainError, Error, Roots};
use crate::num::math;
use crate::roots::{Tol, toms748};
use crate::state::Strategy;

/// CoolProp's Brent tolerance on T for a polynomial segment's inversion, K (`Ancillaries.cpp:226, 257`).
const T_TOL: f64 = 1e-12;

impl MeltingSegment {
    /// p at `t`, Pa, in CoolProp's operation order.
    fn p(&self, t: f64) -> f64 {
        match &self.form {
            MeltingForm::Simon { a, c } => self.p0 + a * (math::powf(t / self.t0, *c) - 1.0),
            MeltingForm::PolynomialInTr(terms) => {
                let mut sum = 0.0;
                for &(a, n) in terms.iter() {
                    sum += a * (math::powf(t / self.t0, n) - 1.0);
                }
                self.p0 * (1.0 + sum)
            }
            MeltingForm::PolynomialInTheta(terms) => {
                let mut sum = 0.0;
                for &(a, n) in terms.iter() {
                    sum += a * math::powf(t / self.t0 - 1.0, n);
                }
                self.p0 * (1.0 + sum)
            }
        }
    }

    fn holds(&self, t: f64) -> bool {
        self.t_min <= t && t <= self.t_max
    }

    /// The bound this segment shares with `next`, where their fits meet.
    fn shared_bound(&self, next: &MeltingSegment) -> Option<f64> {
        [self.t_min, self.t_max].into_iter().find(|t| *t == next.t_min || *t == next.t_max)
    }
}

/// One segment with its pressures at its two bounds, computed once: the flash reads them on every call.
#[derive(Clone, Debug, PartialEq)]
struct Piece {
    segment: MeltingSegment,
    /// p at `t_min` and at `t_max`.
    p_at: (f64, f64),
}

impl Piece {
    fn p_range(&self) -> (f64, f64) {
        let (a, b) = self.p_at;
        (a.min(b), a.max(b))
    }

    fn holds_p(&self, p: f64) -> bool {
        let (lo, hi) = self.p_range();
        lo <= p && p <= hi
    }

    /// Whether p rises with T on this segment (it falls on ice Ih's).
    fn rising(&self) -> bool {
        self.p_at.1 >= self.p_at.0
    }

    /// T at `p`, K, which [`Piece::holds_p`]: Simon's closed form, else TOMS 748 over the segment's range; kept inside
    /// the range against rounding at its ends.
    fn t(&self, p: f64) -> Result<f64, Error> {
        let s = &self.segment;
        let t = match s.form {
            MeltingForm::Simon { a, c } => s.t0 * math::powf((p - s.p0) / a + 1.0, 1.0 / c),
            _ => toms748(|t| s.p(t) - p, s.t_min, s.t_max, Tol::Absolute(T_TOL), 100).converged(Strategy::Melting)?.x,
        };
        Ok(t.max(s.t_min).min(s.t_max))
    }
}

/// A fluid's melting curve: its segments in file order, in increasing pressure.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MeltingCurve {
    pieces: Box<[Piece]>,
}

impl MeltingCurve {
    /// The curve of `segments`; `None` without any.
    pub(crate) fn new(segments: &[MeltingSegment]) -> Option<MeltingCurve> {
        let piece = |s: &MeltingSegment| Piece { segment: s.clone(), p_at: (s.p(s.t_min), s.p(s.t_max)) };
        (!segments.is_empty()).then(|| MeltingCurve { pieces: segments.iter().map(piece).collect() })
    }

    /// The curve's lowest and highest pressures, Pa.
    fn p_bounds(&self) -> (f64, f64) {
        let lo = self.pieces.iter().map(|p| p.p_range().0).fold(f64::INFINITY, f64::min);
        let hi = self.pieces.iter().map(|p| p.p_range().1).fold(f64::NEG_INFINITY, f64::max);
        (lo, hi)
    }

    /// Where `p` lies on the curve: on the first segment, in file order, whose pressure range holds it (CoolProp's
    /// rule: under `Parity` Water's ice V and VI overlap by 9 MPa, DIV-0002), or between two adjacent segments, at the
    /// temperature they share; otherwise outside the curve's pressures.
    fn locate(&self, p: f64) -> Result<Located<'_>, DomainError> {
        if let Some(piece) = self.pieces.iter().find(|piece| piece.holds_p(p)) {
            return Ok(Located::On(piece));
        }
        let (lo, hi) = self.p_bounds();
        if p < lo {
            return Err(DomainError::BelowMinPressure { p, p_min: lo });
        }
        if p > hi {
            return Err(DomainError::AboveMaxPressure { p, p_max: hi });
        }
        // Between two segments: at the bound they share (datagen checks every neighbour has one).
        let gap = self.pieces.windows(2).find(|w| w[0].p_range().1 < p && p < w[1].p_range().0);
        match gap.map(|w| (w[0].segment.shared_bound(&w[1].segment), w[1].p_range().0)) {
            Some((Some(t), _)) => Ok(Located::Join(t)),
            Some((None, p_min)) => Err(DomainError::BelowMinPressure { p, p_min }),
            None => Err(DomainError::BelowMinPressure { p, p_min: lo }),
        }
    }

    /// T_melt(p), K ([`MeltingCurve::locate`]'s segment); outside the curve's pressures a `Domain` error.
    pub(crate) fn temperature(&self, p: f64) -> Result<f64, Error> {
        match self.locate(p)? {
            Located::On(piece) => piece.t(p),
            Located::Join(t) => Ok(t),
        }
    }

    /// p_melt(T), Pa, on the segments whose range holds `t`, two adjacent segments that meet at `t` counting once, as
    /// the first in file order (the one CoolProp evaluates); where several remain, `Ambiguous` with their pressures
    /// ascending (Water's and HeavyWater's ice Ih branch beside ice III and V). Below every segment's range
    /// `BelowMinTemperature`, above it `AboveMaxTemperature`.
    pub(crate) fn pressure(&self, t: f64) -> Result<f64, Error> {
        let (mut found, mut n) = ([f64::NAN; 4], 0);
        for (i, piece) in self.pieces.iter().enumerate() {
            let s = &piece.segment;
            let joined = i > 0 && self.pieces[i - 1].segment.shared_bound(s) == Some(t);
            if s.holds(t) && !joined && n < found.len() {
                found[n] = s.p(t);
                n += 1;
            }
        }
        let found = &mut found[..n];
        found.sort_by(f64::total_cmp);
        match found {
            [p] => Ok(*p),
            [] => {
                let t_min = self.pieces.iter().map(|p| p.segment.t_min).fold(f64::INFINITY, f64::min);
                let t_max = self.pieces.iter().map(|p| p.segment.t_max).fold(f64::NEG_INFINITY, f64::max);
                Err(if t < t_min {
                    DomainError::BelowMinTemperature { t, t_min }
                } else {
                    DomainError::AboveMaxTemperature { t, t_max }
                }
                .into())
            }
            several => Err(Error::Ambiguous { roots: Roots::new(several) }),
        }
    }

    /// T_melt(p) where a state at (`t`, `p`) lies below the curve, in the solid; `None` above it, or where the curve
    /// says nothing (outside its pressures). On a segment one evaluation decides: p is compared with the segment's
    /// p(T), and T_melt is solved only for a state it refuses.
    pub(crate) fn below(&self, t: f64, p: f64) -> Option<f64> {
        match self.locate(p).ok()? {
            Located::Join(t_melt) => (t < t_melt).then_some(t_melt),
            Located::On(piece) => {
                let s = &piece.segment;
                let below = if t < s.t_min {
                    true
                } else if t >= s.t_max {
                    false
                } else {
                    // T < T_melt(p) is p beyond p(T) the way p rises along the segment.
                    let p_t = s.p(t);
                    if piece.rising() { p > p_t } else { p < p_t }
                };
                if below { piece.t(p).ok() } else { None }
            }
        }
    }
}

/// Where a pressure lies on a curve.
enum Located<'a> {
    On(&'a Piece),
    Join(f64),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_tr(t0: f64, p0: f64, terms: &[(f64, f64)], (t_min, t_max): (f64, f64)) -> MeltingSegment {
        MeltingSegment { form: MeltingForm::PolynomialInTr(terms.into()), t0, p0, t_min, t_max }
    }

    /// v8.0.0's Water curve (`Water.json` ANCILLARIES.melting_line, IAPWS R14-08's Eqs. (1)-(4)), ice VI's p₀ as given.
    fn water(p0_vi: f64) -> MeltingCurve {
        let ih = [(-1_195_393.37, 3.0), (-80_818.3159, 25.75), (-3_338.2686, 103.75)];
        MeltingCurve::new(&[
            in_tr(273.16, 611.657, &ih, (251.165, 273.16)),
            in_tr(251.165, 208.566e6, &[(0.299_948, 60.0)], (251.165, 256.164)),
            in_tr(256.164, 350.1e6, &[(1.187_21, 8.0)], (256.164, 273.31)),
            in_tr(273.31, p0_vi, &[(1.074_76, 4.6)], (273.31, 355.0)),
        ])
        .unwrap()
    }

    /// Rot: ROT-049 (map 02 §6 row 6). A Simon segment is inverted only inside its own range: below the curve's lowest
    /// pressure, its value at T_min, T_melt is a `Domain` error, where CoolProp's inversion checks T ≥ T₀ and answers
    /// T(0.5·p_min) = 9.52 K for Hydrogen (T_min 13.957 K), 1.88 K for Helium (2.18 K), and a few hundredths of a
    /// kelvin below T_min for Oxygen, Krypton and ParaHydrogen. At the lowest pressure it is T_min.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn simon_segments_respect_t_min() {
        let registry = crate::Registry::embedded().unwrap();
        for name in ["Hydrogen", "Helium", "Oxygen", "Krypton", "ParaHydrogen"] {
            let segments = registry.record(name).unwrap().melting;
            assert!(matches!(segments[0].form, MeltingForm::Simon { .. }), "{name}");
            let curve = MeltingCurve::new(&segments).unwrap();
            let (p_min, t_min) = (curve.p_bounds().0, segments[0].t_min);
            let below = DomainError::BelowMinPressure { p: 0.5 * p_min, p_min };
            assert_eq!(curve.temperature(0.5 * p_min), Err(below.into()), "{name}");
            let model = registry.get(name).unwrap().model();
            assert_eq!(model.melting_temperature(0.5 * p_min), Err(below.into()), "{name}");
            let at_p_min = curve.temperature(p_min).unwrap();
            assert!((at_p_min / t_min - 1.0).abs() < 1e-12 && at_p_min >= t_min, "{name}: {at_p_min} at {t_min}");
            let below_t = DomainError::BelowMinTemperature { t: 0.99 * t_min, t_min };
            assert_eq!(curve.pressure(0.99 * t_min), Err(below_t.into()), "{name}");
        }
    }

    /// Water's four segments (IAPWS R14-08, Eqs. (1)-(4)): ice Ih's pressure falls with T, so between 251.165 and
    /// 273.16 K two branches hold a temperature and p_melt is `Ambiguous` with both; where two segments meet, at
    /// 251.165, 256.164 and 273.31 K, they count once, as the first in file order. T_melt is single-valued: at
    /// `Parity` the first segment holding p (ice V where its range overlaps ice VI's by 9 MPa, DIV-0002), under
    /// `Corrected` the 0.65 kPa between ice V's end and ice VI's p₀ at their shared 273.31 K. Neither is evaluated
    /// outside the curve.
    #[test]
    fn water_curve_branches_and_joins() {
        let (parity, corrected) = (water(623.4e6), water(632.4e6));
        let close = |a: f64, b: f64, rel: f64| (a / b - 1.0).abs() < rel;
        match corrected.pressure(260.0) {
            Err(Error::Ambiguous { roots }) => {
                let [ih, v] = roots.as_slice() else { panic!("{roots:?}") };
                assert!(close(*ih, 138.268e6, 1e-6) && close(*v, 402.5826e6, 1e-6), "{roots:?}");
            }
            other => panic!("{other:?}"),
        }
        assert!(corrected.pressure(251.165).is_ok_and(|p| close(p, 208.566_566e6, 1e-9)), "ice Ih's, not ice III's");
        assert!(matches!(corrected.pressure(256.164), Err(Error::Ambiguous { roots }) if roots.as_slice().len() == 2));
        assert!(corrected.pressure(273.31).is_ok_and(|p| close(p, 632.399_347e6, 1e-9)), "ice V's");
        assert!(corrected.pressure(273.2).is_ok_and(|p| p > 600e6));
        assert!(corrected.pressure(300.242_822_876).is_ok_and(|p| close(p, 1e9, 1e-9)));
        let below = DomainError::BelowMinTemperature { t: 250.0, t_min: 251.165 };
        assert_eq!(corrected.pressure(250.0), Err(below.into()));
        assert_eq!(corrected.pressure(356.0), Err(DomainError::AboveMaxTemperature { t: 356.0, t_max: 355.0 }.into()));
        let t = corrected.temperature(138.268e6);
        assert!(t.as_ref().is_ok_and(|t| (t - 260.0).abs() < 1e-4), "{t:?}");
        assert!(corrected.temperature(1356.76e6).is_ok_and(|t| (t - 320.0).abs() < 1e-3));
        assert!(parity.temperature(1356.76e6).is_ok_and(|t| (t - 320.965).abs() < 1e-3), "map 10 R10");
        assert!(
            parity.temperature(625e6).is_ok_and(|t| (t - 272.946_116).abs() < 1e-5),
            "ice V's, first in file order"
        );
        assert_eq!(corrected.temperature(632.3995e6), Ok(273.31), "the join");
        let (p_min, p_max) = corrected.p_bounds();
        assert_eq!(
            corrected.temperature(0.5 * p_min),
            Err(DomainError::BelowMinPressure { p: 0.5 * p_min, p_min }.into())
        );
        assert_eq!(
            corrected.temperature(2.0 * p_max),
            Err(DomainError::AboveMaxPressure { p: 2.0 * p_max, p_max }.into())
        );
    }

    /// Below the curve is the solid: T < T_melt(p), decided by one evaluation on the segment holding p, its T_melt solved
    /// for the refusal. Ice VI melts at 300.243 K at 1 GPa; ice Ih, whose melting temperature falls with p, at 264.209 K
    /// at 100 MPa; at the ice V-VI join at 273.31 K; below the curve's lowest pressure nothing is decided.
    #[test]
    fn below_the_curve_is_the_solid() {
        let water = water(632.4e6);
        assert!(water.below(300.0, 1e9).is_some_and(|t| (t - 300.242_823).abs() < 1e-5));
        assert_eq!(water.below(300.3, 1e9), None);
        assert!(water.below(264.0, 100e6).is_some_and(|t| (t - 264.208_746).abs() < 1e-5));
        assert_eq!(water.below(264.3, 100e6), None);
        assert_eq!(water.below(250.0, 100e6), Some(water.temperature(100e6).unwrap()), "below the segment's range");
        assert_eq!(water.below(280.0, 100e6), None, "above the segment's range");
        assert_eq!(water.below(273.30, 632.3995e6), Some(273.31));
        assert_eq!(water.below(273.32, 632.3995e6), None);
        assert_eq!(water.below(200.0, 300.0), None, "below 611.657 Pa there is no melting line");
        assert_eq!(water.below(200.0, 3e9), None, "nor above its highest pressure");
    }
}
