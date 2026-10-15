//! The SRK and PR saturation curves of Bell and Deiters (IECR 60 (2021) 9983): p̃ = p b²/a, ρ̃′ = ρ′b and ρ̃″ = ρ″b as
//! piecewise Chebyshev series in T̃ = RTb/a(T), one universal set per kind, so any alpha function and either dataset's Ω
//! use them (map 06 U4, §3.1; ROT-219). Van der Waals has none; the core's VLE serves it.

mod tables;

use phasekit_core::{DomainError, Error, SatAccuracy, SatPair, SatSide, SaturationCurve, Strategy};

use crate::{Kind, R_CODATA, Residual};

/// One Chebyshev piece of an expansion, over [xmin, xmax] in T̃.
#[derive(Debug, PartialEq)]
pub(crate) struct Piece {
    coeffs: &'static [f64],
    xmin: f64,
    xmax: f64,
}

impl Piece {
    /// Σ c_k T_k(x) with x scaled to [−1, 1], by Clenshaw's recurrence in CoolProp's order (its `Chebyshev::y`).
    fn eval(&self, t_tilde: f64) -> f64 {
        let x = (2.0 * t_tilde - (self.xmax + self.xmin)) / (self.xmax - self.xmin);
        let Some((&c0, rest)) = self.coeffs.split_first() else { return 0.0 };
        let Some((&top, middle)) = rest.split_last() else { return c0 };
        let (mut b1, mut b2) = (top, 0.0);
        for &c in middle.iter().rev() {
            let b = 2.0 * x * b1 - b2 + c;
            b2 = b1;
            b1 = b;
        }
        c0 + x * b1 - b2
    }
}

/// The value of an expansion at T̃, its piece found as CoolProp's `SuperAncillary::get_index` finds it, or `None`
/// outside the expansion.
fn expansion(pieces: &[Piece], t_tilde: f64) -> Option<f64> {
    let (first, last) = (pieces.first()?, pieces.last()?);
    if !(first.xmin..=last.xmax).contains(&t_tilde) {
        return None;
    }
    let (mut lo, mut hi) = (0, pieces.len() - 1);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if pieces.get(mid).is_some_and(|p| t_tilde >= p.xmin) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let i = if pieces.get(lo).is_some_and(|p| t_tilde < p.xmax) { lo } else { hi };
    pieces.get(i).map(|p| p.eval(t_tilde))
}

/// A cubic's saturation from the superancillary: `Exact`, over the T whose T̃ the expansions cover, [0.1, 1]·T̃_c.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicSaturation {
    residual: Residual,
    tables: [&'static [Piece]; 3],
    t_range: (f64, f64),
}

impl CubicSaturation {
    /// The curve of `residual`'s cubic, or `None` for van der Waals, which has no superancillary.
    pub fn new(residual: Residual) -> Option<Self> {
        let tables = match residual.kind() {
            Kind::Srk => [tables::SRK_P, tables::SRK_RHOL, tables::SRK_RHOV],
            Kind::Pr => [tables::PR_P, tables::PR_RHOL, tables::PR_RHOV],
            Kind::Vdw => return None,
        };
        let (lo, hi) = (tables[1].first()?.xmin, tables[1].last()?.xmax);
        let curve = Self { residual, tables, t_range: (0.0, 0.0) };
        Some(Self { t_range: (curve.t_at(lo)?, curve.t_at(hi)?), ..curve })
    }

    /// T̃ = RTb/a(T).
    fn t_tilde(&self, t: f64) -> f64 {
        R_CODATA * t * self.residual.b() / self.residual.a(t).0
    }

    /// The T whose T̃ is `t_tilde`, by bisection on [T_c/1000, 1.1 T_c], where T̃ rises with T (α falls; the expansions
    /// end at T̃_c, which is T_c, and Soave's α turns up again only above (1 + 1/m)² T_c, 2.2 T_c for SRK
    /// MethylLinolenate's m = 2.05): 200 halvings reach adjacent doubles.
    fn t_at(&self, t_tilde: f64) -> Option<f64> {
        let t_c = self.residual.t_c();
        let (mut lo, mut hi) = (1e-3 * t_c, 1.1 * t_c);
        if !(self.t_tilde(lo) <= t_tilde && t_tilde <= self.t_tilde(hi)) {
            return None;
        }
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if mid <= lo || mid >= hi {
                break;
            }
            if self.t_tilde(mid) <= t_tilde { lo = mid } else { hi = mid }
        }
        Some(if self.t_tilde(hi) <= self.tables[1].last()?.xmax { hi } else { lo })
    }
}

impl SaturationCurve for CubicSaturation {
    fn accuracy(&self) -> SatAccuracy {
        SatAccuracy::Exact
    }

    fn t_range(&self) -> (f64, f64) {
        self.t_range
    }

    /// p = p̃ a/b², ρ′ = ρ̃′/b and ρ″ = ρ̃″/b at T̃(T); outside the range a `Domain` error.
    fn at_t(&self, t: f64) -> Result<SatPair, Error> {
        let (t_min, t_max) = self.t_range;
        if t.is_nan() || t < t_min {
            return Err(Error::Domain(DomainError::BelowMinTemperature { t, t_min }));
        }
        if t > t_max {
            return Err(Error::Domain(DomainError::AboveMaxTemperature { t, t_max }));
        }
        let (a, b) = (self.residual.a(t).0, self.residual.b());
        let t_tilde = (R_CODATA * t * b / a).min(self.tables[1].last().map_or(f64::NAN, |p| p.xmax));
        let [p, liquid, vapour] = self.tables.map(|pieces| expansion(pieces, t_tilde).unwrap_or(f64::NAN));
        let p = p * a / (b * b);
        Ok(SatPair { bubble: SatSide { t, p, rho: liquid / b }, dew: SatSide { t, p, rho: vapour / b } })
    }

    /// The T where p_sat(T) = p, by a bracketed solve in ln p over the range (K14), then [`Self::at_t`] there with p
    /// reported as given.
    fn at_p(&self, p: f64) -> Result<SatPair, Error> {
        let (t_lo, t_hi) = self.t_range;
        let ln_p = |t: f64| self.at_t(t).map(|s| phasekit_core::math::ln(s.bubble.p));
        let (f_lo, f_hi) = (ln_p(t_lo)?, ln_p(t_hi)?);
        let target = phasekit_core::math::ln(p);
        if target.is_nan() || target < f_lo {
            return Err(Error::Domain(DomainError::BelowMinPressure { p, p_min: phasekit_core::math::exp(f_lo) }));
        }
        if target > f_hi {
            return Err(Error::Domain(DomainError::AboveMaxPressure { p, p_max: phasekit_core::math::exp(f_hi) }));
        }
        let t = bracketed_root(|t| ln_p(t).map(|f| f - target), (t_lo, f_lo - target), (t_hi, f_hi - target))?;
        let sat = self.at_t(t)?;
        Ok(SatPair { bubble: SatSide { p, ..sat.bubble }, dew: SatSide { p, ..sat.dew } })
    }
}

/// A root of `f` between `lo` and `hi`, whose values bracket it, by the Illinois method (regula falsi halving the
/// stale end's value) to adjacent doubles, bisecting when a step stalls; at most 200 evaluations.
fn bracketed_root(
    f: impl Fn(f64) -> Result<f64, Error>,
    (mut a, mut fa): (f64, f64),
    (mut b, mut fb): (f64, f64),
) -> Result<f64, Error> {
    if fa == 0.0 {
        return Ok(a);
    }
    if fb == 0.0 {
        return Ok(b);
    }
    let mut side = 0;
    for _ in 0..200 {
        let mut c = (a * fb - b * fa) / (fb - fa);
        if !(a.min(b) < c && c < a.max(b)) {
            c = 0.5 * (a + b);
        }
        if c == a || c == b {
            return Ok(if fa.abs() <= fb.abs() { a } else { b });
        }
        let fc = f(c)?;
        if fc == 0.0 {
            return Ok(c);
        }
        if (fc > 0.0) == (fb > 0.0) {
            (b, fb) = (c, fc);
            if side == -1 {
                fa *= 0.5;
            }
            side = -1;
        } else {
            (a, fa) = (c, fc);
            if side == 1 {
                fb *= 0.5;
            }
            side = 1;
        }
    }
    Err(Error::NoConvergence { strategy: Strategy::Superancillary, iterations: 200 })
}
