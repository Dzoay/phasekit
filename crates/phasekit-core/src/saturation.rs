//! Saturation curves: an open trait, so a family brings its own (superancillary, ancillary, a fitted curve)
//! without a core edit, and every curve says how far it can be trusted (Extensible graft).

use core::fmt;

use crate::data::{SaFreshness, Superancillary};
use crate::error::{DomainError, Error};
use crate::input::Pair;

/// How the flash may use a curve's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SatAccuracy {
    /// Accurate to the EOS (a fresh or exactly rescaled superancillary, ~1e-14): used as the answer.
    Exact,
    /// A starting point only (ancillaries, or a superancillary made stale by a data correction): the flash
    /// polishes it with a VLE solve and records `Strategy::Vle` (M6; refused as `Unsupported` before).
    Guess,
    /// The curve defines saturation (pseudo-pure bubble/dew ancillaries): used as the answer by definition.
    Definition,
}

/// One side of a saturation state: T, p and the coexisting molar density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SatSide {
    /// Temperature, K.
    pub t: f64,
    /// Pressure, Pa.
    pub p: f64,
    /// Molar density, mol/m³.
    pub rho: f64,
}

/// Bubble (saturated liquid) and dew (saturated vapour) points at one T (`at_t`) or one p (`at_p`). For a
/// pure fluid both share T and p. For the six pseudo-pure v0.1 fluids (Air, R404A, R407C, R410A, R507A,
/// SES36) they differ: CoolProp 8.0.0 gives R410A at 280 K p = 990480.5 Pa (Q = 0) vs 987288.1 Pa (Q = 1)
/// (map 04 §1, map 03 §3.1; E3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SatPair {
    /// The saturated-liquid side.
    pub bubble: SatSide,
    /// The saturated-vapour side.
    pub dew: SatSide,
}

impl SatPair {
    /// True when bubble and dew share T and p (every pure fluid).
    pub fn is_pure(&self) -> bool {
        self.bubble.t == self.dew.t && self.bubble.p == self.dew.p
    }
}

/// A vapour-liquid saturation curve of one pure or pseudo-pure fluid. Implementations are immutable and
/// lock-free: CoolProp's superancillary inverse built under a mutex on first use (map 11 F7) becomes
/// datagen output.
pub trait SaturationCurve: Send + Sync + fmt::Debug {
    /// Trust level of this curve's answers.
    fn accuracy(&self) -> SatAccuracy;
    /// Fitted temperature range. The flash never evaluates the curve outside it, under any `DomainPolicy`
    /// (D6; CoolProp extrapolates its superancillary below the triple point, map 03 §6).
    fn t_range(&self) -> (f64, f64);
    /// Saturation at temperature `t`.
    fn at_t(&self, t: f64) -> Result<SatPair, Error>;
    /// Saturation at pressure `p`.
    fn at_p(&self, p: f64) -> Result<SatPair, Error>;
}

/// Σ_k c_k T_k(x) for x in [−1, 1] by Clenshaw's recurrence, b_k = 2x·b_(k+1) − b_(k+2) + c_k from the top and f = c_0 +
/// x·b_1 − b_2, in a fixed order with no fused multiply-add: CoolProp's order for two or more coefficients (map 03
/// §3.3). One coefficient is the constant c_0 (CoolProp returns c_0·(1 + x), map 03 §6); none is the zero series.
pub fn clenshaw(coef: &[f64], x: f64) -> f64 {
    let Some((&c0, rest)) = coef.split_first() else { return 0.0 };
    let Some((&top, middle)) = rest.split_last() else { return c0 };
    let (mut b1, mut b2) = (top, 0.0);
    for &c in middle.iter().rev() {
        let b = 2.0 * x * b1 - b2 + c;
        b2 = b1;
        b1 = b;
    }
    c0 + x * b1 - b2
}

/// The Chebyshev coefficients of the x-derivative of Σ_k c_k T_k(x): d_(k−1) = d_(k+1) + 2k·c_k from the top, d_0
/// halved (Mason & Handscomb eq. 2.52, as CoolProp cites it; map 03 §3.3). One coefficient fewer than `coef`, every one
/// kept: CoolProp trims trailing zeros and then drops the last nonzero coefficient too (map 03 §6, ROT-087).
pub fn chebyshev_derivative(coef: &[f64]) -> Vec<f64> {
    let n = coef.len().saturating_sub(1);
    let mut d = vec![0.0; n];
    let (mut next, mut after) = (0.0, 0.0); // d_k and d_(k+1) while d_(k−1) is formed
    for (k, &c) in coef.iter().enumerate().skip(1).rev() {
        let value = after + 2.0 * k as f64 * c;
        if let Some(slot) = d.get_mut(k - 1) {
            *slot = value;
        }
        (after, next) = (next, value);
    }
    if let Some(d0) = d.first_mut() {
        *d0 *= 0.5;
    }
    d
}

/// The piece of `breaks` (n + 1 strictly increasing boundaries of n pieces) that holds `x`. A boundary belongs to the
/// piece it starts and the last one to the last piece, as in CoolProp's lookup; outside [breaks_0, breaks_n], and for
/// NaN, there is none (ROT-093: CoolProp's `bisect_vector` walks an exact hit to the wrong cell, and its superancillary
/// lookup extrapolates; map 08 R3b, map 03 §6).
pub fn piece(breaks: &[f64], x: f64) -> Option<usize> {
    let (&first, &last) = (breaks.first()?, breaks.last()?);
    if breaks.len() < 2 || !(first <= x && x <= last) {
        return None;
    }
    Some((breaks.partition_point(|&b| b <= x) - 1).min(breaks.len() - 2))
}

/// x mapped from [a, b] to [−1, 1] as CoolProp maps it: (2x − (b + a))/(b − a).
fn scaled(x: f64, a: f64, b: f64) -> f64 {
    (2.0 * x - (b + a)) / (b - a)
}

/// A superancillary as a [`SaturationCurve`] (map 03 §3.3): Clenshaw on the stored pieces, nothing built at run time
/// (D6). `Exact` when fresh, and when only R or ρ_r changed, which scales ρ′ and ρ″ by `rho` and p by `p` exactly;
/// `Guess` when the EOS changed otherwise (the hash gate, E14).
#[derive(Debug)]
pub(crate) struct SuperancillaryCurve {
    data: Superancillary,
    accuracy: SatAccuracy,
    p: f64,
    rho: f64,
}

impl SuperancillaryCurve {
    /// The curve of `data` for a record whose EOS relates to it as `freshness` says.
    pub(crate) fn new(data: Superancillary, freshness: SaFreshness) -> SuperancillaryCurve {
        let (accuracy, p, rho) = match freshness {
            SaFreshness::Fresh => (SatAccuracy::Exact, 1.0, 1.0),
            SaFreshness::Rescaled { p, rho } => (SatAccuracy::Exact, p, rho),
            SaFreshness::Stale => (SatAccuracy::Guess, 1.0, 1.0),
        };
        SuperancillaryCurve { data, accuracy, p, rho }
    }
}

impl SaturationCurve for SuperancillaryCurve {
    fn accuracy(&self) -> SatAccuracy {
        self.accuracy
    }

    fn t_range(&self) -> (f64, f64) {
        let breaks = &self.data.breaks;
        (breaks.first().copied().unwrap_or(f64::NAN), breaks.last().copied().unwrap_or(f64::NAN))
    }

    /// ρ′, ρ″ and p at T from one piece lookup (the three curves share their pieces); refused outside the fit.
    fn at_t(&self, t: f64) -> Result<SatPair, Error> {
        let (t_min, t_max) = self.t_range();
        if t < t_min {
            return Err(DomainError::BelowMinTemperature { t, t_min }.into());
        }
        if t > t_max {
            return Err(DomainError::AboveMaxTemperature { t, t_max }.into());
        }
        let i = piece(&self.data.breaks, t).ok_or(Error::InvalidInput { quantity: "T", value: t })?;
        let (a, b) = (self.data.breaks.get(i).copied(), self.data.breaks.get(i + 1).copied());
        let x = scaled(t, a.unwrap_or(t_min), b.unwrap_or(t_max));
        let [rho_l, rho_v, p] =
            self.data.curves.each_ref().map(|curve| curve.get(i).map_or(f64::NAN, |c| clenshaw(c, x)));
        let p = p * self.p;
        let side = |rho: f64| SatSide { t, p, rho: rho * self.rho };
        Ok(SatPair { bubble: side(rho_l), dew: side(rho_v) })
    }

    /// PQ lands at M6.8; datagen already stores the T(ln p) inverse it will evaluate.
    fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
        Err(Error::Unsupported { pair: Pair::PQ })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Map 03 §6 (ROT-087): a single coefficient is the constant c_0 at every x; CoolProp's loop returns c_0·(1 + x).
    #[test]
    fn degree_zero_clenshaw_is_c0() {
        for x in [-1.0, -0.3, 0.0, 0.7, 1.0] {
            assert_eq!(clenshaw(&[2.5], x), 2.5);
        }
    }

    /// Map 03 §6 (ROT-087, bead CoolProp-1tbe.21): degenerate expansions evaluate, never crash: none is 0, one is
    /// c_0, two is c_0 + c_1 x; a longer one equals its power-basis polynomial (T_3 = 4x³ − 3x).
    #[test]
    fn short_expansions_do_not_crash() {
        assert_eq!(clenshaw(&[], 0.4), 0.0);
        assert_eq!(clenshaw(&[1.5, -2.0], 0.25), 1.0);
        assert!(chebyshev_derivative(&[]).is_empty() && chebyshev_derivative(&[3.0]).is_empty());
        assert_eq!(chebyshev_derivative(&[3.0, 2.0]), vec![2.0]);
        let x = 0.3;
        let t3 = 4.0 * x * x * x - 3.0 * x;
        assert!((clenshaw(&[0.5, 0.0, 0.0, 2.0], x) - (0.5 + 2.0 * t3)).abs() < 1e-15);
        assert!((clenshaw(&[0.0, 0.0, 1.0], x) - (2.0 * x * x - 1.0)).abs() < 1e-15);
    }

    /// Map 03 §6 (ROT-087): the derivative keeps every coefficient. T_3′ = 3U_2 = 6T_2 + 3T_0; with a zero top
    /// coefficient (1,386 shipped pieces have one) the result is the same plus a trailing zero, where CoolProp trims the
    /// zero and then drops the 6 as well. The derivative agrees with central differences of `clenshaw`.
    #[test]
    fn chebyshev_derivative_keeps_every_coefficient() {
        assert_eq!(chebyshev_derivative(&[0.0, 0.0, 0.0, 1.0]), vec![3.0, 0.0, 6.0]);
        assert_eq!(chebyshev_derivative(&[0.0, 0.0, 0.0, 1.0, 0.0]), vec![3.0, 0.0, 6.0, 0.0]);
        let c = [0.3, -1.2, 0.7, 0.05, -0.4, 0.0];
        let d = chebyshev_derivative(&c);
        for x in [-0.9, -0.2, 0.4, 0.8] {
            let h = 1e-6;
            let fd = (clenshaw(&c, x + h) - clenshaw(&c, x - h)) / (2.0 * h);
            assert!((clenshaw(&d, x) - fd).abs() < 1e-8, "x = {x}");
        }
    }

    /// ROT-093 (map 08 R3b): an exact hit on an interior boundary belongs to the piece it starts (CoolProp's
    /// `bisect_vector` walks it to the wrong cell); the last boundary belongs to the last piece; outside the range and
    /// NaN find none; fewer than two boundaries hold no piece.
    #[test]
    fn piece_lookup_exact_hit_endpoints_and_nan() {
        let breaks = [1.0, 2.0, 4.0, 8.0];
        let cases = [(1.0, Some(0)), (1.5, Some(0)), (2.0, Some(1)), (3.9, Some(1)), (4.0, Some(2)), (8.0, Some(2))];
        for (x, want) in cases {
            assert_eq!(piece(&breaks, x), want, "x = {x}");
        }
        for x in [0.999, 8.001, f64::NAN, f64::INFINITY] {
            assert_eq!(piece(&breaks, x), None, "x = {x}");
        }
        assert_eq!((piece(&[1.0], 1.0), piece(&[], 1.0)), (None, None));
        assert_eq!((piece(&[1.0, 2.0], 1.5), piece(&[1.0, 2.0], 2.0)), (Some(0), Some(0)));
        assert_eq!(scaled(3.0, 2.0, 4.0), 0.0);
        assert_eq!((scaled(2.0, 2.0, 4.0), scaled(4.0, 2.0, 4.0)), (-1.0, 1.0));
    }

    /// Two pieces on [200, 300] and [300, 400] K: ρ′ = 1000 − T, ρ″ = T/10 and p = 10T in each piece's x.
    fn synthetic() -> Superancillary {
        let line = |a: f64, b: f64, slope: f64, offset: f64| {
            let mut c = [0.0; 13];
            (c[0], c[1]) = (offset + slope * (a + b) / 2.0, slope * (b - a) / 2.0);
            c
        };
        let curve = |slope, offset| vec![line(200.0, 300.0, slope, offset), line(300.0, 400.0, slope, offset)];
        let mut inverse = [0.0; 13];
        inverse[0] = 300.0;
        Superancillary {
            breaks: vec![200.0, 300.0, 400.0],
            curves: [curve(-1.0, 1000.0), curve(0.1, 0.0), curve(10.0, 0.0)],
            extrema: [vec![], vec![], vec![]],
            ln_p_breaks: vec![7.6, 8.3],
            t_of_ln_p: vec![inverse],
        }
    }

    /// Map 03 §6 (D6): the curve answers inside its fitted range only, under any policy: below the triple point, above
    /// the critical point and for NaN it refuses with a typed error, where CoolProp extrapolates. Inside, one lookup
    /// gives all three curves; a rescaled curve applies its factors exactly and stays `Exact`; a stale one is a `Guess`.
    #[test]
    fn curve_refuses_outside_its_fitted_range() {
        let curve = SuperancillaryCurve::new(synthetic(), SaFreshness::Fresh);
        assert_eq!((curve.t_range(), curve.accuracy()), ((200.0, 400.0), SatAccuracy::Exact));
        let below = DomainError::BelowMinTemperature { t: 199.9, t_min: 200.0 };
        assert_eq!(curve.at_t(199.9), Err(below.into()));
        let above = DomainError::AboveMaxTemperature { t: 400.1, t_max: 400.0 };
        assert_eq!(curve.at_t(400.1), Err(above.into()));
        assert!(matches!(curve.at_t(f64::NAN), Err(Error::InvalidInput { quantity: "T", .. })));
        let sat = curve.at_t(250.0).unwrap();
        assert_eq!(sat.bubble, SatSide { t: 250.0, p: 2500.0, rho: 750.0 });
        assert_eq!(sat.dew, SatSide { t: 250.0, p: 2500.0, rho: 25.0 });
        assert!(sat.is_pure());
        assert_eq!(curve.at_t(400.0).unwrap().dew.rho, 40.0);
        assert_eq!(curve.at_t(200.0).unwrap().bubble.rho, 800.0);
        let rescaled = SuperancillaryCurve::new(synthetic(), SaFreshness::Rescaled { p: 1.5, rho: 0.5 });
        let r = rescaled.at_t(350.0).unwrap();
        let f = curve.at_t(350.0).unwrap();
        assert_eq!((r.bubble.p, r.bubble.rho, r.dew.rho), (f.bubble.p * 1.5, f.bubble.rho * 0.5, f.dew.rho * 0.5));
        assert_eq!(rescaled.accuracy(), SatAccuracy::Exact);
        assert_eq!(SuperancillaryCurve::new(synthetic(), SaFreshness::Stale).accuracy(), SatAccuracy::Guess);
        assert_eq!(curve.at_p(2500.0), Err(Error::Unsupported { pair: Pair::PQ }));
    }
}
