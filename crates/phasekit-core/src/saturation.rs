//! Saturation curves: an open trait, so a family brings its own (superancillary, ancillary, a fitted curve)
//! without a core edit, and every curve says how far it can be trusted (Extensible graft).

use core::{fmt, iter};

use crate::data::{Ancillary, PseudoPure, SaFreshness, Superancillary};
use crate::error::{DomainError, Error, Roots};
use crate::exec::{self, ExecPolicy, LaneKernel};
use crate::num::{Lanes, Real, math};
use crate::roots::{Stop, Tol, toms748};
use crate::state::Strategy;

/// How the flash may use a curve's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SatAccuracy {
    /// Accurate to the EOS (a fresh or exactly rescaled superancillary, ~1e-14): used as the answer.
    Exact,
    /// A starting point only (ancillaries, or a superancillary made stale by a data correction): the flash
    /// polishes it with a VLE solve and records `Strategy::Vle` (M6; refused as `Unsupported` before).
    Guess,
    /// The curve defines saturation (pseudo-pure bubble/dew ancillaries): QT and PQ answer by definition, each side's
    /// density the EOS's at its (T, p); inside the dome it seeds the EOS's VLE, as CoolProp's (D4; M6.9).
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

    /// The same saturation reporting the pressure `p` on both sides: a solve at p, whose sides reproduce it.
    pub(crate) fn at_pressure(self, p: f64) -> SatPair {
        SatPair { bubble: SatSide { p, ..self.bubble }, dew: SatSide { p, ..self.dew } }
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
    /// Saturation at each temperature `t[i]` into `out[i]` under `exec`, each [`at_t`](Self::at_t)'s result bit for bit
    /// (PLAN.md M12.9). The default evaluates point by point; the superancillary's Clenshaw sums run in lanes under
    /// `ExecPolicy::Simd`. Unequal lengths are [`Error::Shape`].
    fn at_t_batch(&self, t: &[f64], out: &mut [Result<SatPair, Error>], exec: ExecPolicy) -> Result<(), Error> {
        exec::points(exec, t, t, out, |t, _| self.at_t(t))
    }
    /// Saturation at each pressure `p[i]` into `out[i]` under `exec`, each [`at_p`](Self::at_p)'s result bit for bit, as
    /// [`at_t_batch`](Self::at_t_batch).
    fn at_p_batch(&self, p: &[f64], out: &mut [Result<SatPair, Error>], exec: ExecPolicy) -> Result<(), Error> {
        exec::points(exec, p, p, out, |p, _| self.at_p(p))
    }
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

/// [`clenshaw`] on W lanes, each with its own 13 coefficients (a superancillary's piece): the same operations in the
/// same order, so lane k is `clenshaw(coef[k], x[k])` bit for bit (PLAN.md M12.9).
#[inline(always)]
fn clenshaw_lanes<const W: usize>(coef: [&[f64; 13]; W], x: Lanes<W>) -> Lanes<W> {
    let c = |j: usize| Lanes(coef.map(|c| c.get(j).copied().unwrap_or(0.0)));
    let (two_x, mut b1, mut b2) = (x * 2.0, c(12), Lanes([0.0; W]));
    for j in (1..12).rev() {
        let b = two_x * b1 - b2 + c(j);
        b2 = b1;
        b1 = b;
    }
    c(0) + x * b1 - b2
}

/// The coefficients a lane outside a superancillary's pieces runs on; its result is dropped.
const NO_PIECE: [f64; 13] = [0.0; 13];

/// The coefficients of a curve the data gives no row for at a piece: its value there is NaN.
const NAN_PIECE: [f64; 13] = [f64::NAN; 13];

/// The Chebyshev coefficients of the x-derivative of Σ_k c_k T_k(x): d_(k−1) = d_(k+1) + 2k·c_k from the top, d_0
/// halved (Mason & Handscomb eq. 2.52, as CoolProp cites it; map 03 §3.3). One coefficient fewer than `coef`, every one
/// kept: CoolProp trims trailing zeros and then drops the last nonzero coefficient too (map 03 §6, ROT-087).
pub fn chebyshev_derivative(coef: &[f64]) -> Vec<f64> {
    let n = coef.len().saturating_sub(1);
    let mut d = vec![0.0; n];
    let (mut next, mut after) = (0.0, 0.0); // d_k and d_(k+1) while d_(k−1) is formed
    for (k, &c) in coef.iter().enumerate().skip(1).rev() {
        let value = after + 2.0 * k as f64 * c;
        if let Some(slot) = k.checked_sub(1).and_then(|below| d.get_mut(below)) {
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
    Some(breaks.partition_point(|&b| b <= x).saturating_sub(1).min(breaks.len().saturating_sub(2)))
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
    /// The ends of p(T)'s monotonic segments, T_min, p's extrema and T_max, each with its unscaled p.
    p_ends: Vec<(f64, f64)>,
    /// The lowest and highest unscaled p over [`Self::p_ends`].
    p_range: (f64, f64),
}

impl SuperancillaryCurve {
    /// The curve of `data` for a record whose EOS relates to it as `freshness` says, with p at the ends of p(T)'s
    /// segments, which every `at_p` reads.
    pub(crate) fn new(data: Superancillary, freshness: SaFreshness) -> SuperancillaryCurve {
        let (accuracy, p, rho) = match freshness {
            SaFreshness::Fresh => (SatAccuracy::Exact, 1.0, 1.0),
            SaFreshness::Rescaled { p, rho } => (SatAccuracy::Exact, p, rho),
            SaFreshness::Stale => (SatAccuracy::Guess, 1.0, 1.0),
        };
        let range = (f64::INFINITY, f64::NEG_INFINITY);
        let mut curve = SuperancillaryCurve { data, accuracy, p, rho, p_ends: Vec::new(), p_range: range };
        let (t_min, t_max) = curve.t_range();
        let ends = iter::once(t_min).chain(curve.data.extrema[2].iter().copied()).chain(iter::once(t_max));
        let p_ends: Vec<(f64, f64)> = ends.map(|t| (t, curve.raw(2, t))).collect();
        curve.p_range = p_ends.iter().fold(range, |(lo, hi), (_, v)| (lo.min(*v), hi.max(*v)));
        curve.p_ends = p_ends;
        curve
    }
}

impl SuperancillaryCurve {
    /// Curve `k` (0: ρ′, 1: ρ″, 2: p) of the fit at T, unscaled; NaN outside it.
    fn raw(&self, k: usize, t: f64) -> f64 {
        let breaks = &self.data.breaks;
        let Some(i) = piece(breaks, t) else { return f64::NAN };
        let (a, b) =
            (breaks.get(i).copied().unwrap_or(f64::NAN), breaks.get(i.saturating_add(1)).copied().unwrap_or(f64::NAN));
        self.data.curves.get(k).and_then(|curve| curve.get(i)).map_or(f64::NAN, |c| clenshaw(c, scaled(t, a, b)))
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
        let (rows, x) = self.fit(t).ok_or_else(|| self.refusal(t))?;
        Ok(self.pair_at(t, rows.map(|c| clenshaw(c, x))))
    }

    /// The saturation at p (PLAN.md M6.8; map 03 §3.3). p(T) is monotonic between the extrema datagen found
    /// (PropyleneGlycol's dips in its first interval, DIV-0016), so each such segment holds at most one root: TOMS 748 on
    /// the forward p(T) over the segment or, on the last one where the stored T(ln p) inverse (checked to 1e-12) has a
    /// piece, within the inverse's bracket 1e-9 wide; a bracket that misses its root is no convergence. One root is the
    /// answer, reporting the given p; several are `Ambiguous`, for the flash's `RootPolicy`. Refused above the curve's
    /// highest pressure, below its lowest, and for NaN.
    fn at_p(&self, p: f64) -> Result<SatPair, Error> {
        let t = self.t_at_p(p, |raw| self.inverse_t(raw))?;
        Ok(self.at_t(t)?.at_pressure(p))
    }

    /// The Clenshaw sums in lanes under `ExecPolicy::Simd` (PLAN.md M12.9).
    fn at_t_batch(&self, t: &[f64], out: &mut [Result<SatPair, Error>], exec: ExecPolicy) -> Result<(), Error> {
        exec::run(exec, &AtT(self), t, t, out)
    }

    /// The inverse's and the saturation's Clenshaw sums in lanes under `ExecPolicy::Simd`, each root by itself (K14;
    /// PLAN.md M12.9).
    fn at_p_batch(&self, p: &[f64], out: &mut [Result<SatPair, Error>], exec: ExecPolicy) -> Result<(), Error> {
        exec::run(exec, &AtP(self), p, p, out)
    }
}

impl SuperancillaryCurve {
    /// The saturation at T from the three curves' unscaled values there, ρ′, ρ″ and p, scaled as the record says.
    #[inline(always)]
    fn pair_at(&self, t: f64, [rho_l, rho_v, p]: [f64; 3]) -> SatPair {
        let p = p * self.p;
        let side = |rho: f64| SatSide { t, p, rho: rho * self.rho };
        SatPair { bubble: side(rho_l), dew: side(rho_v) }
    }

    /// The saturation temperature at p ([`SaturationCurve::at_p`]); `inverse` gives the stored inverse's T at an
    /// unscaled pressure ([`Self::inverse_t`]), which brackets the root on the last segment.
    fn t_at_p(&self, p: f64, inverse: impl Fn(f64) -> Option<f64>) -> Result<f64, Error> {
        if p.is_nan() {
            return Err(Error::InvalidInput { quantity: "p", value: p });
        }
        let raw = p / self.p;
        let (lowest, highest) = self.p_range;
        if raw > highest {
            return Err(DomainError::AboveMaxPressure { p, p_max: highest * self.p }.into());
        }
        if raw < lowest {
            return Err(DomainError::BelowMinPressure { p, p_min: lowest * self.p }.into());
        }
        let t_max = self.t_range().1;
        let (mut roots, mut found) = ([f64::NAN; 4], 0_usize);
        for pair in self.p_ends.windows(2) {
            let &[(a, pa), (b, pb)] = pair else { continue };
            let last = b == t_max;
            let Some(t) =
                segment_root(|t| self.raw(2, t), raw, [(a, pa), (b, pb)], || last.then(|| inverse(raw)).flatten())?
            else {
                continue;
            };
            // A p at an extremum is found at the end of both segments.
            let seen = found.checked_sub(1).is_some_and(|last| roots.get(last) == Some(&t));
            if !seen && let Some(slot) = roots.get_mut(found) {
                (*slot, found) = (t, found.saturating_add(1));
            }
        }
        match roots.get(..found) {
            Some(&[t]) => Ok(t),
            other => Err(Error::Ambiguous { roots: Roots::new(other.unwrap_or_default()) }),
        }
    }

    /// The stored inverse's T at the unscaled pressure `raw`, where it has a piece at ln p.
    fn inverse_t(&self, raw: f64) -> Option<f64> {
        let (row, x) = self.inverse_fit(math::ln(raw))?;
        Some(clenshaw(row, x))
    }

    /// The inverse's piece at ln p, its coefficients and ln p mapped to the piece's [−1, 1]; `None` outside its pieces.
    #[inline(always)]
    fn inverse_fit(&self, ln_p: f64) -> Option<(&[f64; 13], f64)> {
        let breaks = &self.data.ln_p_breaks;
        let i = piece(breaks, ln_p)?;
        Some((self.data.t_of_ln_p.get(i)?, scaled(ln_p, *breaks.get(i)?, *breaks.get(i.saturating_add(1))?)))
    }

    /// The fit's piece at T, the three curves' coefficients (NaN for a curve the data gives no row) and T mapped to the
    /// piece's [−1, 1]; `None` outside the fitted range and for NaN, where [`SaturationCurve::at_t`] refuses T
    /// ([`Self::refusal`]).
    #[inline(always)]
    fn fit(&self, t: f64) -> Option<([&[f64; 13]; 3], f64)> {
        let breaks = &self.data.breaks;
        let i = piece(breaks, t)?;
        let rows = self.data.curves.each_ref().map(|curve| curve.get(i).unwrap_or(&NAN_PIECE));
        Some((rows, scaled(t, *breaks.get(i)?, *breaks.get(i.saturating_add(1))?)))
    }

    /// Why [`SaturationCurve::at_t`] refuses a T that [`Self::fit`] has no piece for: below or above the fitted range,
    /// else an invalid T (NaN).
    fn refusal(&self, t: f64) -> Error {
        let (t_min, t_max) = self.t_range();
        if t < t_min {
            DomainError::BelowMinTemperature { t, t_min }.into()
        } else if t > t_max {
            DomainError::AboveMaxTemperature { t, t_max }.into()
        } else {
            Error::InvalidInput { quantity: "T", value: t }
        }
    }

    /// [`SaturationCurve::at_t`] at W temperatures (PLAN.md M12.9): the three curves' Clenshaw sums in lanes, each lane on
    /// its own piece; a lane outside the fit is `at_t`'s refusal ([`Self::refusal`]). Lane k is `at_t(t[k])` bit for bit.
    #[inline(always)]
    fn at_t_lanes<const W: usize>(&self, t: Lanes<W>) -> [Result<SatPair, Error>; W] {
        let fits = t.0.map(|t| self.fit(t));
        let x = Lanes(fits.map(|fit| fit.map_or(0.0, |(_, x)| x)));
        let sum = |k: usize| {
            let rows = fits.map(|fit| fit.and_then(|(rows, _)| rows.get(k).copied()).unwrap_or(&NO_PIECE));
            clenshaw_lanes(rows, x)
        };
        let (rho_l, rho_v, p) = (sum(0), sum(1), sum(2));
        core::array::from_fn(|k| {
            let lane = |v: Lanes<W>| v.0.get(k).copied().unwrap_or(f64::NAN);
            match fits.get(k) {
                Some(Some(_)) => Ok(self.pair_at(lane(t), [lane(rho_l), lane(rho_v), lane(p)])),
                _ => Err(self.refusal(lane(t))),
            }
        })
    }

    /// [`SaturationCurve::at_p`] at W pressures (PLAN.md M12.9): the stored inverse's T, ln p included, and the
    /// saturation at each root in lanes, each root found by TOMS 748 on its own as `at_p` finds it (K14). Lane k is
    /// `at_p(p[k])` bit for bit.
    #[inline(always)]
    fn at_p_lanes<const W: usize>(&self, p: Lanes<W>) -> [Result<SatPair, Error>; W] {
        let ln_p = (p / Lanes::from_f64(self.p)).ln();
        let fits = ln_p.0.map(|ln_p| self.inverse_fit(ln_p));
        let x = Lanes(fits.map(|fit| fit.map_or(0.0, |(_, x)| x)));
        let inverse = clenshaw_lanes(fits.map(|fit| fit.map_or(&NO_PIECE, |(row, _)| row)), x);
        let roots: [Result<f64, Error>; W] = core::array::from_fn(|k| {
            let seed = fits.get(k).copied().flatten().and(inverse.0.get(k).copied());
            self.t_at_p(p.0.get(k).copied().unwrap_or(f64::NAN), |_| seed)
        });
        let t_min = self.t_range().0;
        let sat = self.at_t_lanes(Lanes(roots.each_ref().map(|t| t.as_ref().copied().unwrap_or(t_min))));
        let mut lanes = roots.into_iter().zip(sat).zip(p.0);
        core::array::from_fn(|_| match lanes.next() {
            Some(((t, sat), p)) => t.and(sat).map(|sat| sat.at_pressure(p)),
            None => Err(Error::Shape { expected: W, found: 0 }),
        })
    }
}

/// The root on one segment of p(T) between its extrema, `ends` (T and the unscaled p at each), where the unscaled
/// pressure `raw(T)` is `raw` ([`SaturationCurve::at_p`]): `None` where the segment's pressures do not reach it; TOMS
/// 748 on the forward p(T) over the segment or, where `inverse` gives the stored inverse's T (on the last segment),
/// within 1e-9 of it; a bracket that misses its root is `NoConvergence`.
fn segment_root(
    p_of: impl Fn(f64) -> f64,
    raw: f64,
    [(a, pa), (b, pb)]: [(f64, f64); 2],
    inverse: impl FnOnce() -> Option<f64>,
) -> Result<Option<f64>, Error> {
    if !(pa.min(pb) <= raw && raw <= pa.max(pb)) {
        return Ok(None);
    }
    let (lo, hi) = match inverse() {
        Some(t) => ((t * (1.0 - 1e-9)).max(a), (t * (1.0 + 1e-9)).min(b)),
        None => (a, b),
    };
    let root = toms748(|t| p_of(t) - raw, lo, hi, Tol::Absolute(0.0), 100);
    if root.stop != Stop::Converged {
        return Err(Error::NoConvergence { strategy: Strategy::Superancillary, iterations: root.iterations });
    }
    Ok(Some(root.x))
}

/// [`SuperancillaryCurve::at_t`] as a lane kernel (PLAN.md M12.9): T to the saturation.
struct AtT<'a>(&'a SuperancillaryCurve);

impl LaneKernel for AtT<'_> {
    type Out = Result<SatPair, Error>;
    #[inline(always)]
    fn point(&self, t: f64, _: f64) -> Self::Out {
        self.0.at_t(t)
    }
    #[inline(always)]
    fn lanes<const W: usize>(&self, t: Lanes<W>, _: Lanes<W>) -> [Self::Out; W] {
        self.0.at_t_lanes(t)
    }
}

/// [`SuperancillaryCurve::at_p`] as a lane kernel (PLAN.md M12.9): p to the saturation.
struct AtP<'a>(&'a SuperancillaryCurve);

impl LaneKernel for AtP<'_> {
    type Out = Result<SatPair, Error>;
    #[inline(always)]
    fn point(&self, p: f64, _: f64) -> Self::Out {
        self.0.at_p(p)
    }
    #[inline(always)]
    fn lanes<const W: usize>(&self, p: Lanes<W>, _: Lanes<W>) -> [Self::Out; W] {
        self.0.at_p_lanes(p)
    }
}

/// A pseudo-pure fluid's saturation by definition (D4; map 04 U4; PLAN.md M6.9). Each side's pressure is its ancillary's
/// (`p_l` bubble, `p_v` dew) and its density the density ancillary's, which the flash only uses to seed the EOS's
/// density at (T, p). At p each side's T is its pressure ancillary's inversion, so bubble and dew differ in T, as in
/// CoolProp's PQ (`FlashRoutines.cpp:1183-1199`). Refused outside its range, QT's (`t_range`), as every curve.
#[derive(Debug)]
pub(crate) struct PseudoPureCurve(pub(crate) PseudoPure);

impl SaturationCurve for PseudoPureCurve {
    fn accuracy(&self) -> SatAccuracy {
        SatAccuracy::Definition
    }

    fn t_range(&self) -> (f64, f64) {
        self.0.t_range
    }

    fn at_t(&self, t: f64) -> Result<SatPair, Error> {
        let (t_min, t_max) = self.0.t_range;
        if t.is_nan() {
            return Err(Error::InvalidInput { quantity: "T", value: t });
        }
        if t < t_min {
            return Err(DomainError::BelowMinTemperature { t, t_min }.into());
        }
        if t > t_max {
            return Err(DomainError::AboveMaxTemperature { t, t_max }.into());
        }
        let side = |p: &Ancillary, rho: &Ancillary| SatSide { t, p: p.at(t), rho: rho.at(t) };
        Ok(SatPair { bubble: side(&self.0.p_l, &self.0.rho_l), dew: side(&self.0.p_v, &self.0.rho_v) })
    }

    /// Each side's T where its pressure ancillary is p, within that ancillary's range; refused beyond it.
    fn at_p(&self, p: f64) -> Result<SatPair, Error> {
        if p.is_nan() {
            return Err(Error::InvalidInput { quantity: "p", value: p });
        }
        let side = |pa: &Ancillary, rho: &Ancillary| -> Result<SatSide, Error> {
            let Some(t) = pa.invert(p) else {
                let p_max = pa.at(pa.t_max);
                // Equivalent: a PQ is refused only when the pressure ancillary has no root in its range, which it has
                // at its own value at Tmax, so p equal to that bound never reaches this; above and below are tested.
                // equivalent mutant: replace > with >=
                return Err(if p > p_max {
                    DomainError::AboveMaxPressure { p, p_max }
                } else {
                    DomainError::BelowMinPressure { p, p_min: pa.at(pa.t_min - 0.01) }
                }
                .into());
            };
            Ok(SatSide { t, p, rho: rho.at(t) })
        };
        Ok(SatPair { bubble: side(&self.0.p_l, &self.0.rho_l)?, dew: side(&self.0.p_v, &self.0.rho_v)? })
    }
}

impl Ancillary {
    /// The value at T (`Ancillaries.cpp:43-81`), extrapolated beyond [`t_min`, `t_max`] as CoolProp's; NaN above T_r,
    /// where Θ = 1 − T/T_r < 0.
    pub fn at(&self, t: f64) -> f64 {
        let theta = 1.0 - t / self.t_r;
        if theta < 0.0 {
            return f64::NAN;
        }
        let sum: f64 = self.n.iter().zip(&self.t).map(|(n, e)| n * math::powf(theta, *e)).sum();
        if self.exponential {
            let k = if self.tau_r { self.t_r / t } else { 1.0 };
            self.reducing * math::exp(k * sum)
        } else {
            self.reducing * (1.0 + sum)
        }
    }

    /// The T at which the ancillary takes `value`, by TOMS 748 to rounding over CoolProp's bracket of its inversion,
    /// [`t_min` − 0.01 K, `t_max`] (`Ancillaries.cpp:82-113`, which stops at 1e-10 K); `None` where `value` is outside
    /// the ancillary's values there.
    pub fn invert(&self, value: f64) -> Option<f64> {
        let root = toms748(|t| self.at(t) - value, self.t_min - 0.01, self.t_max, Tol::Absolute(0.0), 100);
        (root.stop == Stop::Converged).then_some(root.x)
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

    /// Two pieces on [200, 300] and [300, 400] K: ρ′ = 1000 − T, ρ″ = T/10 and p = 10T in each piece's x; the inverse is
    /// the constant 300 K over ln p from 7.7 to 8.2 (p 2208 to 3641 Pa).
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
            ln_p_breaks: vec![7.7, 8.2],
            t_of_ln_p: vec![inverse],
        }
    }

    /// A T with no piece is refused by the bound it is beyond, else as an invalid T: at the range's ends `fit` has a
    /// piece, so `refusal` there is only for data without one; NaN has none. Inside, `fit`'s rows and x are `at_t`'s.
    #[test]
    fn refusals_name_the_bound_and_fit_is_at_ts_piece() {
        let curve = SuperancillaryCurve::new(synthetic(), SaFreshness::Fresh);
        let below = |t| Error::from(DomainError::BelowMinTemperature { t, t_min: 200.0 });
        let above = |t| Error::from(DomainError::AboveMaxTemperature { t, t_max: 400.0 });
        let invalid = |t| Error::InvalidInput { quantity: "T", value: t };
        assert_eq!((curve.refusal(199.9), curve.refusal(400.1)), (below(199.9), above(400.1)));
        assert_eq!((curve.refusal(200.0), curve.refusal(400.0)), (invalid(200.0), invalid(400.0)));
        assert!(matches!(curve.refusal(f64::NAN), Error::InvalidInput { quantity: "T", .. }));
        assert!(curve.fit(199.9).is_none() && curve.fit(400.1).is_none() && curve.fit(f64::NAN).is_none());
        for t in [200.0, 250.0, 300.0, 350.0, 400.0] {
            let (rows, x) = curve.fit(t).unwrap();
            assert_eq!(Ok(curve.pair_at(t, rows.map(|c| clenshaw(c, x)))), curve.at_t(t), "{t}");
        }
        let mut short = synthetic();
        short.curves[1].pop();
        let short = SuperancillaryCurve::new(short, SaFreshness::Fresh);
        assert!(short.at_t(350.0).is_ok_and(|sat| sat.dew.rho.is_nan() && !sat.bubble.rho.is_nan()));
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
        let reported = curve.at_t(250.0).unwrap().at_pressure(7.0);
        assert_eq!((reported.bubble, reported.dew), (SatSide { p: 7.0, ..sat.bubble }, SatSide { p: 7.0, ..sat.dew }));
    }

    /// PQ on the curve (PLAN.md M6.8): T solved from the forward p(T) inside the inverse's 1e-9 bracket (the synthetic
    /// inverse is the constant 300 K, so a root 2e-9 away is missed and a root 5e-10 away found); outside the inverse's
    /// pieces over the whole segment; refused above the top, below the bottom and for NaN; an extremum splits the
    /// segments, and a p at it is one root; a rescaled curve solves for p/1.5 and reports p.
    #[test]
    fn pq_solves_the_forward_curve_within_the_inverse_bracket() {
        let curve = SuperancillaryCurve::new(synthetic(), SaFreshness::Fresh);
        let t_at = |p: f64| curve.at_p(p).map(|sat| (sat.bubble.t, sat.dew.t, sat.bubble.p));
        assert_eq!(t_at(3000.0), Ok((300.0, 300.0, 3000.0)));
        let near = t_at(3000.0 * (1.0 + 5e-10)).unwrap().0;
        assert!((near / (300.0 * (1.0 + 5e-10)) - 1.0).abs() < 1e-15, "{near}");
        assert!(matches!(curve.at_p(3000.0 * (1.0 + 2e-9)), Err(Error::NoConvergence { .. })));
        assert!(matches!(curve.at_p(3000.0 * (1.0 - 2e-9)), Err(Error::NoConvergence { .. })));
        let wide = [2100.0, 3900.0, 2000.0, 4000.0].map(|p| curve.at_p(p).map(|sat| sat.dew.rho));
        assert_eq!(wide, [Ok(21.0), Ok(39.0), Ok(20.0), Ok(40.0)], "ρ″ = T/10 at T = p/10");
        assert_eq!(curve.at_p(4000.5), Err(DomainError::AboveMaxPressure { p: 4000.5, p_max: 4000.0 }.into()));
        assert_eq!(curve.at_p(1999.0), Err(DomainError::BelowMinPressure { p: 1999.0, p_min: 2000.0 }.into()));
        assert!(matches!(curve.at_p(f64::NAN), Err(Error::InvalidInput { quantity: "p", .. })));
        let mut split = synthetic();
        split.extrema[2] = vec![220.0];
        let split = SuperancillaryCurve::new(split, SaFreshness::Fresh);
        let t_at = |p: f64| split.at_p(p).map(|sat| sat.bubble.t);
        assert_eq!(
            [t_at(2100.0), t_at(2200.0), t_at(3900.0)],
            [Ok(210.0), Ok(220.0), Ok(390.0)],
            "one root at 2200 Pa, the extremum"
        );
        let rescaled = SuperancillaryCurve::new(synthetic(), SaFreshness::Rescaled { p: 1.5, rho: 0.5 });
        let sat = rescaled.at_p(4500.0).unwrap();
        assert_eq!((sat.bubble.t, sat.bubble.p, sat.bubble.rho, sat.dew.rho), (300.0, 4500.0, 350.0, 15.0));
        assert_eq!(rescaled.at_p(6001.5), Err(DomainError::AboveMaxPressure { p: 6001.5, p_max: 6000.0 }.into()));
        assert_eq!(rescaled.at_p(2999.0), Err(DomainError::BelowMinPressure { p: 2999.0, p_min: 3000.0 }.into()));
    }

    /// The pseudo-pure fluids' ancillaries (PLAN.md M6.9; `Ancillaries.cpp:43-113`): R410A's bubble and dew pressures at
    /// 280 K are CoolProp 8.0.0's QT pressures there (990480.516605891 and 987288.0717853763 Pa, 03-decision-log), and
    /// their inversions at 1 MPa its PQ temperatures at Q = 0 and 1 (280.31657 and 280.42348 K, printed to 1e-5 K);
    /// above T_r an ancillary is NaN, and a value outside its range has no inversion.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn ancillaries_evaluate_and_invert_as_coolprops() {
        let registry = crate::Registry::from_embedded(crate::DataSet::Parity).unwrap();
        let record = crate::internal::record(&registry, "R410A").unwrap();
        let pp = record.pseudo_pure.unwrap();
        let close = |got: f64, want: f64| (got / want - 1.0).abs() < 1e-14;
        assert!(close(pp.p_l.at(280.0), 990_480.516_605_891) && close(pp.p_v.at(280.0), 987_288.071_785_376_3));
        let (t_l, t_v) = (pp.p_l.invert(1e6).unwrap(), pp.p_v.invert(1e6).unwrap());
        assert!((t_l - 280.316_57).abs() < 5e-6 && (t_v - 280.423_48).abs() < 5e-6, "{t_l} {t_v}");
        assert!(close(pp.p_l.at(t_l), 1e6), "{}", pp.p_l.at(t_l));
        assert!(pp.rho_v.at(pp.rho_v.t_r + 0.1).is_nan() && pp.p_l.invert(1e9).is_none());
        assert!(pp.rho_l.at(280.0) > pp.rho_v.at(280.0));
        assert!(crate::internal::record(&registry, "Water").unwrap().pseudo_pure.is_none());
    }

    /// A pseudo-pure curve's range (D6; PLAN.md M6.9): QT's, from the definition, both ends included; beyond them and for
    /// NaN refused. PQ beyond the bubble pressure ancillary's range is refused with the bound it reaches: its value at
    /// its Tmax above, and at Tmin − 0.01 K, the end of CoolProp's inversion bracket, below.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn pseudo_pure_curve_refuses_outside_its_range() {
        let registry = crate::Registry::from_embedded(crate::DataSet::Parity).unwrap();
        let definition = crate::internal::record(&registry, "R410A").unwrap().pseudo_pure.unwrap();
        let curve = PseudoPureCurve(definition.clone());
        let (lo, hi) = definition.t_range;
        assert!(curve.at_t(lo).is_ok() && curve.at_t(hi).is_ok() && curve.accuracy() == SatAccuracy::Definition);
        let above = DomainError::AboveMaxTemperature { t: hi + 1e-6, t_max: hi };
        let below = DomainError::BelowMinTemperature { t: lo - 1e-6, t_min: lo };
        assert_eq!((curve.at_t(hi + 1e-6), curve.at_t(lo - 1e-6)), (Err(above.into()), Err(below.into())));
        assert!(matches!(curve.at_t(f64::NAN), Err(Error::InvalidInput { quantity: "T", .. })));
        let p_l = &definition.p_l;
        let (p_max, p_min) = (p_l.at(p_l.t_max), p_l.at(p_l.t_min - 0.01));
        assert_eq!(curve.at_p(1e9), Err(DomainError::AboveMaxPressure { p: 1e9, p_max }.into()));
        assert_eq!(curve.at_p(1.0), Err(DomainError::BelowMinPressure { p: 1.0, p_min }.into()));
        assert!(matches!(curve.at_p(f64::NAN), Err(Error::InvalidInput { quantity: "p", .. })));
    }

    /// PQ where p(T) dips (DIV-0016's PropyleneGlycol): over [200, 300] K p falls from 3000 to 2000 Pa, over [300, 400] K
    /// it rises back, so 2500 Pa and 3000 Pa have two roots each, `Ambiguous` in ascending T, and 2000 Pa, the minimum,
    /// one; the range is the lowest and the highest p.
    #[test]
    fn pq_where_p_dips_has_two_roots() {
        let dip = SuperancillaryCurve::new(dip(), SaFreshness::Fresh);
        let roots = |p: f64| match dip.at_p(p) {
            Err(Error::Ambiguous { roots }) => roots.as_slice().to_vec(),
            other => panic!("{other:?}"),
        };
        assert_eq!((roots(2500.0), roots(3000.0)), (vec![250.0, 350.0], vec![200.0, 400.0]));
        assert_eq!(dip.at_p(2000.0).map(|sat| (sat.bubble.t, sat.dew.rho)), Ok((300.0, 30.0)));
        assert_eq!(dip.at_p(1999.0), Err(DomainError::BelowMinPressure { p: 1999.0, p_min: 2000.0 }.into()));
        assert_eq!(dip.at_p(3000.5), Err(DomainError::AboveMaxPressure { p: 3000.5, p_max: 3000.0 }.into()));
    }

    /// [`synthetic`] with p falling from 3000 to 2000 Pa over [200, 300] K and rising back over [300, 400] K, its
    /// extremum at 300 K, and an inverse over ln p from 9 to 9.5, above every p of the curve.
    fn dip() -> Superancillary {
        let mut dip = synthetic();
        let line = |a: f64, b: f64, slope: f64, offset: f64| {
            let mut c = [0.0; 13];
            (c[0], c[1]) = (offset + slope * (a + b) / 2.0, slope * (b - a) / 2.0);
            c
        };
        dip.curves[2] = vec![line(200.0, 300.0, -10.0, 5000.0), line(300.0, 400.0, 10.0, -1000.0)];
        (dip.extrema[2], dip.ln_p_breaks) = (vec![300.0], vec![9.0, 9.5]);
        dip
    }

    /// A saturation's bits, NaN canonical, or its refusal as printed (a NaN it carries is not equal to itself).
    fn bits(sat: &Result<SatPair, Error>) -> Result<[u64; 6], String> {
        let bits = |v: f64| if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() };
        let sides = |s: &SatPair| [s.bubble.t, s.bubble.p, s.bubble.rho, s.dew.t, s.dew.p, s.dew.rho].map(bits);
        sat.as_ref().map(sides).map_err(|e| format!("{e:?}"))
    }

    /// `kernel` over `x` in lanes of 2, 4 and 8 and, with the `simd` feature, at every dispatch level of this CPU, and
    /// `batch` under every policy, against `point` at each x.
    fn assert_lanes(
        what: &str,
        kernel: &impl LaneKernel<Out = Result<SatPair, Error>>,
        batch: impl Fn(&[f64], &mut [Result<SatPair, Error>], ExecPolicy) -> Result<(), Error>,
        point: impl Fn(f64) -> Result<SatPair, Error>,
        x: &[f64],
    ) {
        let check = |got: &[Result<SatPair, Error>], how: &str| {
            for (x, got) in x.iter().zip(got) {
                assert_eq!(bits(got), bits(&point(*x)), "{what} {how} at {x}");
            }
        };
        let unset = || vec![Err(Error::Shape { expected: 0, found: 0 }); x.len()];
        let mut out = unset();
        crate::exec::run_lanes::<_, 2>(kernel, x, x, &mut out);
        check(&out, "2 lanes");
        crate::exec::run_lanes::<_, 4>(kernel, x, x, &mut out);
        check(&out, "4 lanes");
        crate::exec::run_lanes::<_, 8>(kernel, x, x, &mut out);
        check(&out, "8 lanes");
        #[cfg(feature = "simd")]
        for level in crate::exec::tests::levels() {
            let mut out = unset();
            crate::exec::run_simd(level, kernel, x, x, &mut out);
            check(&out, &format!("{level:?}"));
        }
        let parallel = ExecPolicy::Parallel { chunk: core::num::NonZeroUsize::MIN };
        for exec in [ExecPolicy::Reference, ExecPolicy::Simd, parallel] {
            let mut out = unset();
            batch(x, &mut out, exec).unwrap();
            check(&out, &format!("{exec:?}"));
        }
        assert!(matches!(batch(x, &mut [], ExecPolicy::Simd), Err(Error::Shape { .. }) | Ok(())));
    }

    /// PLAN.md M12.9 (VERIFICATION.md §9.2, class `Exact`): the superancillary's `at_t` and `at_p` in lanes are `at_t` and
    /// `at_p` at every point bit for bit, refusals included, through the lane kernels on 2, 4 and 8 lanes and at every
    /// dispatch level, and through `at_t_batch` and `at_p_batch` under every policy. The curves are the synthetic ones,
    /// fresh, rescaled, split at an extremum and dipping (two roots, `Ambiguous`); T and p sweep past both ends of the
    /// fit, through its piece boundaries, the inverse's pieces and either side of its bracket, ±∞ and NaN.
    #[test]
    fn superancillary_lanes_are_per_point() {
        let mut split = synthetic();
        split.extrema[2] = vec![220.0];
        let rescaled = SaFreshness::Rescaled { p: 1.5, rho: 0.5 };
        let curves = [
            (synthetic(), SaFreshness::Fresh),
            (synthetic(), rescaled),
            (split, SaFreshness::Fresh),
            (dip(), rescaled),
        ]
        .map(|(data, freshness)| SuperancillaryCurve::new(data, freshness));
        let edges = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0];
        let t = (0..=55).map(|k| 190.0 + 4.0 * f64::from(k)).chain([200.0, 300.0, 400.0, 199.999, 400.001]);
        let t: Vec<f64> = t.chain(edges).collect();
        let near = [5e-10, -5e-10, 2e-9, -2e-9].map(|e| 3000.0 * (1.0 + e));
        let p = (0..=75).map(|k| 1000.0 + 80.0 * f64::from(k)).chain([2000.0, 2200.0, 3000.0, 4000.0, 4000.5, 1999.0]);
        let p: Vec<f64> = p.chain(near).chain(near.map(|p| 1.5 * p)).chain(edges).collect();
        for (k, curve) in curves.iter().enumerate() {
            let what = format!("curve {k}");
            assert_lanes(&what, &AtT(curve), |t, out, exec| curve.at_t_batch(t, out, exec), |t| curve.at_t(t), &t);
            assert_lanes(&what, &AtP(curve), |p, out, exec| curve.at_p_batch(p, out, exec), |p| curve.at_p(p), &p);
        }
        // The sweeps hold answers and refusals of every kind: two roots on the dipping curve, and a bracket that misses.
        let count = |curve: &SuperancillaryCurve, refused: fn(&Result<SatPair, Error>) -> bool| {
            p.iter().filter(|p| refused(&curve.at_p(**p))).count()
        };
        let [fresh, .., dip] = &curves;
        assert!(count(dip, |r| matches!(r, Err(Error::Ambiguous { .. }))) > 5);
        assert!(count(fresh, |r| matches!(r, Err(Error::NoConvergence { .. }))) >= 2);
        assert!(count(fresh, Result::is_ok) > 5 && t.iter().filter(|t| fresh.at_t(**t).is_ok()).count() > 40);
    }
}
