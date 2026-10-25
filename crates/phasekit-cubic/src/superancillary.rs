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

/// The value of an expansion at T̃ from the last piece that starts at or below it, or `None` outside the expansion. The
/// pieces meet end to end, so that is the piece CoolProp's `SuperAncillary::get_index` finds, a bound going to the
/// piece above it.
fn expansion(pieces: &[Piece], t_tilde: f64) -> Option<f64> {
    let (first, last) = (pieces.first()?, pieces.last()?);
    if !(first.xmin..=last.xmax).contains(&t_tilde) {
        return None;
    }
    let above = pieces.partition_point(|p| p.xmin <= t_tilde);
    pieces.get(above.checked_sub(1)?).map(|p| p.eval(t_tilde))
}

/// A cubic's saturation from the superancillary: `Exact`, over the T whose T̃ the expansions cover, [0.1, 1]·T̃_c.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicSaturation {
    residual: Residual,
    tables: [&'static [Piece]; 3],
    t_range: (f64, f64),
    /// The saturation pressures at the range's ends, which bound `at_p`.
    p_range: (f64, f64),
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
        let curve = Self { residual, tables, t_range: (0.0, 0.0), p_range: (0.0, 0.0) };
        let curve = Self { t_range: (curve.t_at(lo)?, curve.t_at(hi)?), ..curve };
        let p = |t: f64| curve.at_t(t).ok().map(|s| s.bubble.p);
        Some(Self { p_range: (p(curve.t_range.0)?, p(curve.t_range.1)?), ..curve })
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
            // Equivalent (M11.5): the bisection stops once its ends are adjacent doubles, where the midpoint is one of
            // them and further halvings move neither end.
            // equivalent mutant: replace || with &&
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
        let (p_min, p_max) = self.p_range;
        if p.is_nan() || p < p_min {
            return Err(Error::Domain(DomainError::BelowMinPressure { p, p_min }));
        }
        if p > p_max {
            return Err(Error::Domain(DomainError::AboveMaxPressure { p, p_max }));
        }
        let target = phasekit_core::math::ln(p);
        let t = bracketed_root(|t| self.at_t(t).map(|s| phasekit_core::math::ln(s.bubble.p) - target), self.t_range)?;
        let sat = self.at_t(t)?;
        Ok(SatPair { bubble: SatSide { p, ..sat.bubble }, dew: SatSide { p, ..sat.dew } })
    }
}

/// A root of `f` between two points whose values bracket it, by the Illinois method (regula falsi halving the stale
/// end's value) to adjacent doubles, bisecting when a step leaves the bracket; at most 200 steps.
fn bracketed_root(mut f: impl FnMut(f64) -> Result<f64, Error>, (lo, hi): (f64, f64)) -> Result<f64, Error> {
    let mut bracket = Bracket { a: (lo, f(lo)?), b: (hi, f(hi)?), side: 0 };
    for _ in 0..200 {
        if let Some(root) = bracket.root() {
            return Ok(root);
        }
        let c = bracket.trial();
        bracket.update(c, f(c)?);
    }
    Err(Error::no_convergence(Strategy::Superancillary, 200))
}

/// [`bracketed_root`]'s state: the ends with their values, and which end moved last (−1 b, 1 a, 0 neither).
struct Bracket {
    a: (f64, f64),
    b: (f64, f64),
    side: i8,
}

impl Bracket {
    /// The root, once an end's value is zero or the ends are adjacent doubles (the end with the smaller value).
    fn root(&self) -> Option<f64> {
        let ((a, fa), (b, fb)) = (self.a, self.b);
        let c = self.trial();
        match () {
            () if fa == 0.0 => Some(a),
            () if fb == 0.0 => Some(b),
            () if c == a || c == b => Some(if fa.abs() <= fb.abs() { a } else { b }),
            () => None,
        }
    }

    /// The false-position point, or the midpoint where that leaves the open bracket.
    fn trial(&self) -> f64 {
        let ((a, fa), (b, fb)) = (self.a, self.b);
        let c = (a * fb - b * fa) / (fb - fa);
        if a.min(b) < c && c < a.max(b) { c } else { 0.5 * (a + b) }
    }

    /// Replaces the end whose value has `fc`'s sign by (c, fc), halving the other end's value when the same end moved
    /// twice running (Illinois).
    fn update(&mut self, c: f64, fc: f64) {
        // Equivalent (M11.5): a value of exactly 0 goes to either end alike: no end holds 0 at an update (an end at 0
        // is the root before the next step), and a step whose value is 0 is the root at the next check whichever end it
        // replaced.
        // equivalent mutant: replace > with >=
        if (fc > 0.0) == (self.b.1 > 0.0) {
            self.b = (c, fc);
            if self.side == -1 {
                self.a.1 *= 0.5;
            }
            self.side = -1;
        } else {
            self.a = (c, fc);
            if self.side == 1 {
                self.b.1 *= 0.5;
            }
            self.side = 1;
        }
    }
}

#[cfg(test)]
mod tests;
