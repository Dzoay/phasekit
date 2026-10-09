//! The root toolbox (PLAN.md M6.2; D6; map 03 §3.4, §6, §9): native TOMS 748 and a bracketed Newton/Halley. Every
//! solver returns the residual evaluated AT the returned point ([`Root`]), reports exhaustion, a lost bracket and a
//! non-finite residual as a [`Stop`] instead of accepting the last iterate, and stops on a typed tolerance ([`Tol`]):
//! CoolProp's absolute x tolerance gave a 6.1 % density error at 1e-8 mol/m³, its Newton stops on the step alone and
//! no TOMS 748 call site checks `max_iter` (map 03 §3.4, §6; ROT-074, ROT-075, ROT-076).
#![cfg_attr(not(test), expect(dead_code, reason = "the pure VLE (M6.3) is the first user"))]

use crate::error::Error;
use crate::num::math;
use crate::state::Strategy;

#[cfg(test)]
mod aps;

/// Why a solver stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// The tolerance was met: a bracketing solver's on its bracket, Newton's and Halley's on the step and the residual.
    Converged,
    /// `max_iter` was reached.
    MaxIterations,
    /// The interval held no sign change.
    BracketLost,
    /// A residual was NaN or infinite.
    NotFinite,
}

/// A typed tolerance on two abscissae (a bracket's ends, or the two ends of a step).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tol {
    /// |b − a| ≤ tol·min(|a|, |b|).
    Relative(f64),
    /// |b − a| ≤ tol.
    Absolute(f64),
    /// |ln(b/a)| ≤ tol, for a positive quantity spanning decades (densities); never met by a non-positive end.
    LogAxis(f64),
}

impl Tol {
    /// Whether `a` and `b` lie within the tolerance of each other.
    pub(crate) fn met(self, a: f64, b: f64) -> bool {
        match self {
            Tol::Relative(tol) => (b - a).abs() <= tol * a.abs().min(b.abs()),
            Tol::Absolute(tol) => (b - a).abs() <= tol,
            // Both ends negative is the only case ln(b/a) does not refuse by itself (a zero end gives ±∞ or NaN).
            Tol::LogAxis(tol) => a.is_sign_positive() && math::ln(b / a).abs() <= tol,
        }
    }

    /// The width the tolerance allows at `x`: `tol` for an absolute tolerance, `tol·|x|` for the others.
    pub(crate) fn width(self, x: f64) -> f64 {
        match self {
            Tol::Absolute(tol) => tol,
            Tol::Relative(tol) | Tol::LogAxis(tol) => tol * x.abs(),
        }
    }
}

/// A root with the residual at that root.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Root {
    /// The returned abscissa.
    pub(crate) x: f64,
    /// The residual evaluated at `x`, not at an earlier iterate.
    pub(crate) f: f64,
    /// Iterations spent.
    pub(crate) iterations: u16,
    /// Why the solver stopped.
    pub(crate) stop: Stop,
}

impl Root {
    /// The root if the solver converged, else `Error::NoConvergence` naming `strategy`: no iterate is accepted silently.
    pub(crate) fn converged(self, strategy: Strategy) -> Result<Root, Error> {
        if self.stop == Stop::Converged {
            Ok(self)
        } else {
            Err(Error::NoConvergence { strategy, iterations: self.iterations })
        }
    }
}

/// `c` if it lies strictly inside (a, b), else the midpoint.
fn interior(a: f64, b: f64, c: f64) -> f64 {
    if a < c && c < b { c } else { a + (b - a) / 2.0 }
}

/// The zero of the quadratic through (a, fa), (b, fb), (d, fd) by `steps` Newton steps from the end where the
/// quadratic's curvature and value have the same sign (Alefeld, Potra & Shi's NEWQUA); the secant root when the
/// three points are collinear or a step meets a flat tangent. The result may leave (a, b); [`Bracket::adjust`] moves it.
fn newton_quadratic(a: f64, fa: f64, b: f64, fb: f64, d: f64, fd: f64, steps: u32) -> f64 {
    let slope = (fb - fa) / (b - a);
    let curvature = ((fd - fb) / (d - b) - slope) / (d - a);
    let secant = a - fa / slope;
    if curvature == 0.0 {
        return secant;
    }
    let mut c = if curvature.is_sign_positive() == fa.is_sign_positive() { a } else { b };
    for _ in 0..steps {
        let value = fa + (slope + curvature * (c - b)) * (c - a);
        let tangent = slope + curvature * (2.0 * c - (a + b));
        if tangent == 0.0 {
            return secant;
        }
        c -= value / tangent;
    }
    c
}

/// The zero of the inverse cubic through four points (x as a cubic in f), by the Aitken-Neville scheme of Alefeld,
/// Potra & Shi's PZERO (after Stoer & Bulirsch).
fn inverse_cubic([a, b, d, e]: [f64; 4], [fa, fb, fd, fe]: [f64; 4]) -> f64 {
    let q11 = (d - e) * fd / (fe - fd);
    let q21 = (b - d) * fb / (fd - fb);
    let q31 = (a - b) * fa / (fb - fa);
    let d21 = (b - d) * fd / (fd - fb);
    let d31 = (a - b) * fb / (fb - fa);
    let q22 = (d21 - q11) * fb / (fe - fb);
    let q32 = (d31 - q21) * fa / (fd - fa);
    let d32 = (d31 - q21) * fd / (fd - fa);
    let q33 = (d32 - q22) * fa / (fe - fa);
    a + (q31 + q32 + q33)
}

/// The bracket a TOMS 748 iteration works on: [a, b] with a sign change, `d` the end the last step replaced and `e`
/// the one before (NaN until there is one).
struct Bracket {
    a: f64,
    fa: f64,
    b: f64,
    fb: f64,
    d: f64,
    fd: f64,
    e: f64,
    fe: f64,
}

impl Bracket {
    /// The midpoint.
    fn mid(&self) -> f64 {
        self.a + 0.5 * (self.b - self.a)
    }

    /// The termination width of Alefeld, Potra & Shi's TOLE at the end with the smaller |f|, u:
    /// 2·(2ε·|u| + `tol` there), never below a few ulps of u.
    fn tole(&self, tol: Tol) -> f64 {
        let u = if self.fb.abs() <= self.fa.abs() { self.b } else { self.a };
        2.0 * (tol.width(u) + 2.0 * f64::EPSILON * u.abs())
    }

    /// Whether the bracket is within [`Bracket::tole`], or holds no float strictly inside it. The second test only
    /// matters when tole is below one ulp (u = 0 or subnormal, tol = 0), where half of a one-ulp bracket rounds to 0
    /// and the midpoint is `a`; for a normal u, tole ≥ 4ε·|u| covers a bracket of two adjacent floats.
    fn done(&self, tol: Tol) -> bool {
        self.b - self.a <= self.tole(tol) || self.mid() <= self.a
    }

    /// Alefeld, Potra & Shi's BRACKT adjustment: `c` at least 0.7·tole inside either end, so that a step landing on
    /// a converged end closes the bracket instead of creeping; the midpoint once the bracket is within 1.4·tole, or
    /// if `c` is still not strictly inside (a NaN step, or a zero tole at u = 0).
    fn adjust(&self, c: f64, tol: Tol) -> f64 {
        let delta = 0.7 * self.tole(tol);
        let c = if self.b - self.a <= 2.0 * delta {
            self.mid()
        } else if c <= self.a + delta {
            self.a + delta
        } else if c >= self.b - delta {
            self.b - delta
        } else {
            c
        };
        if self.a < c && c < self.b { c } else { self.mid() }
    }

    /// Replaces the end on c's side of the sign change by `c` (f(c) ≠ 0), keeping the replaced end as `d`.
    fn take(&mut self, c: f64, fc: f64) {
        (self.e, self.fe) = (self.d, self.fd);
        if fc.is_sign_negative() != self.fa.is_sign_negative() {
            (self.d, self.fd) = (self.b, self.fb);
            (self.b, self.fb) = (c, fc);
        } else {
            (self.d, self.fd) = (self.a, self.fa);
            (self.a, self.fa) = (c, fc);
        }
    }

    /// The end with the smaller |f|, as a root that stopped for `stop`.
    fn best(&self, iterations: u16, stop: Stop) -> Root {
        let (x, f) = if self.fa.abs() <= self.fb.abs() { (self.a, self.fa) } else { (self.b, self.fb) };
        Root { x, f, iterations, stop }
    }

    /// The inverse cubic's zero through (a, b, d, e), if it lies strictly inside (a, b). Until `e` exists its NaN makes
    /// the zero NaN, and two equal f values make the formula divide by zero: either way the result fails the test, as
    /// Alefeld, Potra & Shi's check that the four values are distinct would (theirs multiplies the six differences,
    /// which underflows for f values near 1e-60; problem 26 of their test set).
    fn cubic(&self) -> Option<f64> {
        let c = inverse_cubic([self.a, self.b, self.d, self.e], [self.fa, self.fb, self.fd, self.fe]);
        (self.a < c && c < self.b).then_some(c)
    }

    /// The double-length secant step from the end u with the smaller |f|: u − 2·f(u)·(b − a)/(f(b) − f(a)), or the
    /// midpoint if that moves more than half the bracket.
    fn doubled_secant(&self) -> f64 {
        let (u, fu) = if self.fa.abs() < self.fb.abs() { (self.a, self.fa) } else { (self.b, self.fb) };
        let c = u - 2.0 * (fu / (self.fb - self.fa)) * (self.b - self.a);
        if (c - u).abs() > 0.5 * (self.b - self.a) { self.mid() } else { c }
    }
}

/// TOMS 748 (Alefeld, Potra & Shi, ACM TOMS 21 (1995) 327-344, Algorithm 4.2, μ = 1/2, as their ACM Algorithm 748
/// runs it) on [a, b] with f(a)·f(b) ≤ 0: a secant step, then per iteration two interpolation steps (inverse cubic
/// once four distinct values exist and its zero lies inside, else a quadratic by 2 then 3 Newton steps), a
/// double-length secant step from the better end, and a bisection unless the bracket halved. Every point is first
/// moved by [`Bracket::adjust`]. Converged when the bracket is within [`Bracket::tole`], 2·(2ε·|u| + `tol` at u), u
/// the end with the smaller |f|, or holds no float strictly inside; the root is that end, with its f. A residual
/// that is exactly zero ends the search there.
pub(crate) fn toms748(mut f: impl FnMut(f64) -> f64, a: f64, b: f64, tol: Tol, max_iter: u16) -> Root {
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    let (fa, fb) = (f(a), f(b));
    let mut s = Bracket { a, fa, b, fb, d: f64::NAN, fd: f64::NAN, e: f64::NAN, fe: f64::NAN };
    if !fa.is_finite() || !fb.is_finite() {
        let (x, fx) = if fa.is_finite() { (b, fb) } else { (a, fa) };
        return Root { x, f: fx, iterations: 0, stop: Stop::NotFinite };
    }
    if fa == 0.0 || fb == 0.0 {
        return s.best(0, Stop::Converged);
    }
    if fa.is_sign_negative() == fb.is_sign_negative() {
        return s.best(0, Stop::BracketLost);
    }
    if s.done(tol) {
        return s.best(0, Stop::Converged);
    }
    // Adjusts and evaluates c, narrows the bracket, and ends the search on a zero, a non-finite value or convergence.
    let mut step = |s: &mut Bracket, c: f64, iterations: u16| -> Option<Root> {
        let c = s.adjust(c, tol);
        let fc = f(c);
        if !fc.is_finite() {
            return Some(Root { x: c, f: fc, iterations, stop: Stop::NotFinite });
        }
        if fc == 0.0 {
            return Some(Root { x: c, f: fc, iterations, stop: Stop::Converged });
        }
        s.take(c, fc);
        s.done(tol).then(|| s.best(iterations, Stop::Converged))
    };
    let secant = s.a - (s.fa / (s.fb - s.fa)) * (s.b - s.a);
    if let Some(root) = step(&mut s, secant, 0) {
        return root;
    }
    for iteration in 1..=max_iter {
        let width = s.b - s.a;
        for steps in [2, 3] {
            let c = s.cubic().unwrap_or_else(|| newton_quadratic(s.a, s.fa, s.b, s.fb, s.d, s.fd, steps));
            if let Some(root) = step(&mut s, c, iteration) {
                return root;
            }
        }
        let c = s.doubled_secant();
        if let Some(root) = step(&mut s, c, iteration) {
            return root;
        }
        if s.b - s.a >= 0.5 * width {
            let mid = s.mid();
            if let Some(root) = step(&mut s, mid, iteration) {
                return root;
            }
        }
    }
    s.best(max_iter, Stop::MaxIterations)
}

/// Newton on [lo, hi] with a sign change, from `x0`; `f(x)` returns (f, f′). A step that leaves the closed bracket,
/// or a non-finite one, becomes a bisection (rtsafe). Converged only when the step meets `tol` AND |f| ≤ `ftol` at the new
/// point: CoolProp's Newton stops on the step alone (map 03 §3.4).
pub(crate) fn newton(
    mut f: impl FnMut(f64) -> (f64, f64),
    (lo, hi): (f64, f64),
    x0: f64,
    tol: Tol,
    ftol: f64,
    max_iter: u16,
) -> Root {
    bracketed(
        |x| {
            let (value, slope) = f(x);
            (value, value / slope)
        },
        (lo, hi),
        x0,
        (tol, ftol),
        max_iter,
    )
}

/// Halley on [lo, hi] with a sign change, from `x0`; `f(x)` returns (f, f′, f″), and the step is 2ff′/(2f′² − ff″).
/// Otherwise as [`newton`].
pub(crate) fn halley(
    mut f: impl FnMut(f64) -> (f64, f64, f64),
    (lo, hi): (f64, f64),
    x0: f64,
    tol: Tol,
    ftol: f64,
    max_iter: u16,
) -> Root {
    bracketed(
        |x| {
            let (value, slope, curvature) = f(x);
            (value, 2.0 * value * slope / (2.0 * slope * slope - value * curvature))
        },
        (lo, hi),
        x0,
        (tol, ftol),
        max_iter,
    )
}

/// The bracketed iteration behind [`newton`] and [`halley`]: `f(x)` returns (f, the step to subtract).
fn bracketed(
    mut f: impl FnMut(f64) -> (f64, f64),
    (lo, hi): (f64, f64),
    x0: f64,
    (tol, ftol): (Tol, f64),
    max_iter: u16,
) -> Root {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let ((f_lo, _), (f_hi, _)) = (f(lo), f(hi));
    for (x, fx) in [(lo, f_lo), (hi, f_hi)] {
        if !fx.is_finite() {
            return Root { x, f: fx, iterations: 0, stop: Stop::NotFinite };
        }
        if fx == 0.0 {
            return Root { x, f: fx, iterations: 0, stop: Stop::Converged };
        }
    }
    if f_lo.is_sign_negative() == f_hi.is_sign_negative() {
        let (x, fx) = if f_lo.abs() <= f_hi.abs() { (lo, f_lo) } else { (hi, f_hi) };
        return Root { x, f: fx, iterations: 0, stop: Stop::BracketLost };
    }
    // The ends where f is negative and positive.
    let (mut neg, mut pos) = if f_lo.is_sign_negative() { (lo, hi) } else { (hi, lo) };
    let mut x = interior(lo, hi, x0);
    let (mut fx, mut step) = f(x);
    for iteration in 1..=max_iter {
        if !fx.is_finite() {
            return Root { x, f: fx, iterations: iteration - 1, stop: Stop::NotFinite };
        }
        if fx == 0.0 {
            return Root { x, f: fx, iterations: iteration - 1, stop: Stop::Converged };
        }
        if fx.is_sign_negative() {
            neg = x;
        } else {
            pos = x;
        }
        let previous = x;
        let (low, high, next) = (neg.min(pos), neg.max(pos), x - step);
        // A step anywhere in the closed bracket is taken (a converged step lands on the end just updated); one that
        // leaves it, or is not finite, becomes a bisection (rtsafe).
        x = if low <= next && next <= high { next } else { low + (high - low) / 2.0 };
        (fx, step) = f(x);
        if fx.is_finite() && fx.abs() <= ftol && tol.met(previous, x) {
            return Root { x, f: fx, iterations: iteration, stop: Stop::Converged };
        }
    }
    let stop = if fx.is_finite() { Stop::MaxIterations } else { Stop::NotFinite };
    Root { x, f: fx, iterations: max_iter, stop }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// x³ − 2x − 5, Wallis's cubic: one real root near 2.0946 in [2, 3].
    fn wallis(x: f64) -> f64 {
        x * x * x - 2.0 * x - 5.0
    }

    const WALLIS: f64 = 2.094_551_481_542_327;

    /// ROT-074 (map 03 §3.4): the returned x is a point the solver evaluated, and the returned f is the residual it
    /// evaluated there, for TOMS 748, Newton and Halley alike; never a step past the last evaluation.
    #[test]
    fn root_reports_the_point_it_evaluated() {
        let mut seen = Vec::new();
        let mut g = |x: f64| {
            seen.push((x, wallis(x)));
            wallis(x)
        };
        let root = toms748(&mut g, 2.0, 3.0, Tol::Relative(1e-15), 50);
        assert!((root.x / WALLIS - 1.0).abs() < 1e-15 && root.stop == Stop::Converged, "{root:?}");
        assert!(seen.contains(&(root.x, root.f)), "{root:?} not among {seen:?}");
        let mut seen = Vec::new();
        let mut g = |x: f64| {
            seen.push((x, wallis(x)));
            (wallis(x), 3.0 * x * x - 2.0)
        };
        let root = newton(&mut g, (2.0, 3.0), 2.5, Tol::Relative(1e-14), 1e-12, 50);
        assert!((root.x / WALLIS - 1.0).abs() < 1e-14 && root.stop == Stop::Converged, "{root:?}");
        assert!(seen.contains(&(root.x, root.f)), "{root:?}");
        let root =
            halley(|x| (wallis(x), 3.0 * x * x - 2.0, 6.0 * x), (2.0, 3.0), 2.5, Tol::Relative(1e-14), 1e-12, 50);
        assert_eq!((root.f, root.stop), (wallis(root.x), Stop::Converged), "{root:?}");
        assert!(root.iterations < 6, "Halley converges cubically: {root:?}");
    }

    /// ROT-075 (map 03 §6: no TOMS 748 call site checks `max_iter`): running out of iterations is a stop of its own,
    /// and an error once the caller asks for the root.
    #[test]
    fn exhausted_iterations_are_an_error() {
        let root = toms748(|x| math::exp(x) - 1e10, -10.0, 40.0, Tol::Relative(1e-15), 1);
        assert_eq!(root.stop, Stop::MaxIterations, "{root:?}");
        let refused = Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: 1 });
        assert_eq!(root.converged(Strategy::Vle), refused);
        let root = newton(|x| (wallis(x), 3.0 * x * x - 2.0), (2.0, 3.0), 2.9, Tol::Relative(1e-15), 1e-12, 2);
        assert_eq!((root.stop, root.iterations), (Stop::MaxIterations, 2), "{root:?}");
        let found = toms748(|x| math::exp(x) - 1e10, -10.0, 40.0, Tol::Relative(1e-15), 100);
        assert_eq!(found.converged(Strategy::Vle).map(|r| r.stop), Ok(Stop::Converged), "{found:?}");
        assert!((found.x / math::ln(1e10) - 1.0).abs() < 1e-14, "{found:?}");
    }

    /// ROT-075 (map 03 §3.4: CoolProp's Newton stops on |Δx/x| whatever the residual): Newton converges only when the
    /// step meets the tolerance AND the residual is within `ftol`. A jump with a huge reported slope takes steps of
    /// 1e-30 that leave x where it is with |f| = 1, where CoolProp would stop as converged: never converged here.
    #[test]
    fn newton_converges_on_residual_and_step() {
        let root = newton(|x| (x * x - 2.0, 2.0 * x), (1.0, 2.0), 1.0, Tol::Relative(1e-15), 1e-14, 50);
        assert_eq!(root.stop, Stop::Converged, "{root:?}");
        assert!((root.x - core::f64::consts::SQRT_2).abs() <= f64::EPSILON && root.f.abs() <= 1e-14, "{root:?}");
        let jump = |x: f64| (if x > 1.0 { 1.0 } else { -1.0 }, 1e30);
        let root = newton(jump, (0.0, 2.0), 0.5, Tol::Relative(1e-12), 1e-6, 40);
        assert_eq!((root.stop, root.f.abs()), (Stop::MaxIterations, 1.0), "{root:?}");
    }

    /// ROT-076 (map 03 §3.4: an extrapolated guess is returned when f is NaN): a NaN residual stops every solver as
    /// `NotFinite`, with that residual reported, and is an error once the caller asks for the root.
    #[test]
    fn a_nan_residual_is_an_error() {
        let nan_above = |x: f64| if x > 2.05 { f64::NAN } else { wallis(x) };
        let root = toms748(nan_above, 2.0, 3.0, Tol::Relative(1e-15), 50);
        assert_eq!((root.stop, root.x, root.iterations), (Stop::NotFinite, 3.0, 0), "the end where f is NaN: {root:?}");
        assert!(root.f.is_nan(), "{root:?}");
        let refused = Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: root.iterations });
        assert_eq!(root.converged(Strategy::Vle), refused);
        let root = newton(|x| (nan_above(x), 3.0 * x * x - 2.0), (2.0, 3.0), 2.01, Tol::Relative(1e-15), 1e-12, 50);
        assert_eq!(root.stop, Stop::NotFinite, "{root:?}");
        // NaN at the first interior point: stopped there, before any iteration completes.
        let nan_inside = |x: f64| if 2.05 < x && x < 2.5 { f64::NAN } else { wallis(x) };
        let root = newton(|x| (nan_inside(x), 3.0 * x * x - 2.0), (2.0, 3.0), 2.1, Tol::Relative(1e-15), 1e-12, 50);
        assert_eq!((root.stop, root.x, root.iterations), (Stop::NotFinite, 2.1, 0), "{root:?}");
        assert_eq!(toms748(|_| f64::NAN, 2.0, 3.0, Tol::Relative(1e-15), 50).stop, Stop::NotFinite);
        assert_eq!(
            newton(|_| (f64::INFINITY, 1.0), (2.0, 3.0), 2.5, Tol::Relative(1e-15), 1e-12, 50).stop,
            Stop::NotFinite
        );
    }

    /// Map 03 §6 (an absolute tolerance gave CoolProp a 6.1 % error in ρ(T, s) at ρ = 1e-8 mol/m³): on a log axis the
    /// tolerance is relative at any magnitude. An absolute 1e-9 would accept 1.00e-8 against 1.05e-8; a log-axis
    /// 1e-12 finds a root at 1.2345e-8 to 1e-12.
    #[test]
    fn log_axis_tolerance_is_relative() {
        assert!(Tol::Absolute(1e-9).met(1.0e-8, 1.05e-8), "the defect: 5 % passes an absolute 1e-9");
        assert!(!Tol::LogAxis(1e-10).met(1.0e-8, 1.05e-8));
        assert!(Tol::LogAxis(1e-10).met(1.0e-8, 1.0e-8 * (1.0 + 5e-11)) && !Tol::LogAxis(1.0).met(-1.0, 1.0));
        let rho = 1.2345e-8;
        let root = toms748(|x| math::ln(x / rho), 1e-12, 1e3, Tol::LogAxis(1e-12), 100);
        assert_eq!(root.stop, Stop::Converged, "{root:?}");
        assert!((root.x / rho - 1.0).abs() < 1e-12, "{root:?}");
        assert!(Tol::Relative(1e-3).met(1000.0, 1000.5) && !Tol::Relative(1e-3).met(1000.0, 1002.0));
        assert!(!Tol::LogAxis(1.0).met(-1.0, -1.0), "two negative ends: ln(b/a) = 0, still refused");
        assert!(!Tol::LogAxis(1.0).met(0.0, 1.0) && !Tol::LogAxis(1.0).met(1.0, 0.0));
        let widths = [Tol::Absolute(0.5), Tol::Relative(0.5), Tol::LogAxis(0.5)].map(|tol| tol.width(-4.0));
        assert_eq!(widths, [0.5, 2.0, 2.0]);
    }

    /// No sign change on the interval is `BracketLost`, for every solver; reversed ends and an exact zero are fine.
    #[test]
    fn a_bracket_without_a_sign_change_is_lost() {
        assert_eq!(toms748(wallis, 3.0, 4.0, Tol::Relative(1e-15), 50).stop, Stop::BracketLost);
        let lost = newton(|x| (wallis(x), 1.0), (3.0, 4.0), 3.5, Tol::Relative(1e-15), 1e-12, 50);
        assert_eq!((lost.stop, lost.x), (Stop::BracketLost, 3.0), "the end nearer a root: {lost:?}");
        let reversed = toms748(wallis, 3.0, 2.0, Tol::Relative(1e-15), 50);
        assert_eq!(reversed.stop, Stop::Converged);
        assert_eq!(
            toms748(|x| x - 2.0, 2.0, 3.0, Tol::Relative(1e-15), 50),
            Root { x: 2.0, f: 0.0, iterations: 0, stop: Stop::Converged }
        );
        // An exact zero ends Newton where it lands: x − 1.5 from 1.25 steps onto 1.5 in its first iteration.
        assert_eq!(
            newton(|x| (x - 1.5, 1.0), (1.0, 2.0), 1.25, Tol::Relative(1e-15), 1e-12, 50),
            Root { x: 1.5, f: 0.0, iterations: 1, stop: Stop::Converged }
        );
        // The first secant step of a straight line is its zero: three evaluations.
        let mut evaluations = 0;
        let line = toms748(
            |x| {
                evaluations += 1;
                x - 0.25
            },
            0.0,
            1.0,
            Tol::Relative(1e-15),
            50,
        );
        assert_eq!((line, evaluations), (Root { x: 0.25, f: 0.0, iterations: 0, stop: Stop::Converged }, 3));
    }

    /// The interpolation steps on their own models, against classical values. The quadratic through three points of
    /// x² − 2 is x² − 2, so its Newton steps are the Babylonian iterates of √2: from 3 (the end where curvature and
    /// value share a sign) 11/6 then 193/132, from 2 then 3/2, 17/12 and 577/408; on 2 − x² they start from the same
    /// end. Three collinear points give the secant zero; the inverse cubic finds a zero of x as a cubic in f.
    #[test]
    fn interpolation_steps_are_exact_on_their_models() {
        let close = |got: f64, want: f64| (got - want).abs() <= 4.0 * f64::EPSILON * want.abs();
        let q = |x: f64| x * x - 2.0;
        let r = newton_quadratic(1.0, q(1.0), 3.0, q(3.0), 4.0, q(4.0), 2);
        assert!(close(r, 193.0 / 132.0), "{r}");
        let r = newton_quadratic(1.0, -q(1.0), 3.0, -q(3.0), 4.0, -q(4.0), 2);
        assert!(close(r, 193.0 / 132.0), "2 − x²: {r}");
        for (steps, want) in [(1, 1.5), (2, 17.0 / 12.0), (3, 577.0 / 408.0)] {
            let r = newton_quadratic(1.0, q(1.0), 2.0, q(2.0), 3.0, q(3.0), steps);
            assert!(close(r, want), "{steps} steps: {r}");
        }
        let line = |x: f64| 2.0 * x - 2.5;
        assert_eq!(newton_quadratic(1.0, line(1.0), 2.0, line(2.0), 3.0, line(3.0), 2), 1.25);
        let x_of = |f: f64| 1.25 + 0.5 * f - 0.25 * f * f + 0.125 * f * f * f;
        let fs = [-0.75, -0.25, 0.5, 1.5];
        assert!((inverse_cubic(fs.map(x_of), fs) - 1.25).abs() < 1e-15);
        assert_eq!([2.5, 1.25, 1.0, 2.0].map(|c| interior(1.0, 2.0, c)), [1.5, 1.25, 1.5, 1.5]);
    }

    /// Newton's and Halley's own iterates, against their classical values for √2 from 3/2 on (1, 3): Newton's
    /// 17/12, 577/408, 665857/470832, converged on the step after the next; Halley's x(x² + 6)/(3x² + 2), 99/70 then
    /// 3880899/2744210.
    #[test]
    fn newton_and_halley_take_their_classical_steps() {
        let close = |got: f64, want: f64| (got - want).abs() <= 4.0 * f64::EPSILON * want;
        let mut seen = Vec::new();
        let root = newton(
            |x| {
                seen.push(x);
                (x * x - 2.0, 2.0 * x)
            },
            (1.0, 3.0),
            1.5,
            Tol::Relative(1e-15),
            1e-14,
            50,
        );
        let want = [1.0, 3.0, 1.5, 17.0 / 12.0, 577.0 / 408.0, 665_857.0 / 470_832.0];
        assert!(seen.iter().zip(want).all(|(x, w)| close(*x, w)) && seen.len() == 8, "{seen:?}");
        assert_eq!((root.stop, root.iterations), (Stop::Converged, 5), "{root:?}");
        let mut seen = Vec::new();
        let root = halley(
            |x| {
                seen.push(x);
                (x * x - 2.0, 2.0 * x, 2.0)
            },
            (1.0, 3.0),
            1.5,
            Tol::Relative(1e-15),
            1e-14,
            50,
        );
        let want = [1.0, 3.0, 1.5, 99.0 / 70.0, 3_880_899.0 / 2_744_210.0];
        assert!(seen.iter().zip(want).all(|(x, w)| close(*x, w)), "{seen:?}");
        assert_eq!(root.stop, Stop::Converged, "{root:?}");
        assert!(close(root.x, core::f64::consts::SQRT_2), "{root:?}");
    }

    /// rtsafe: a Newton step that leaves the bracket becomes a bisection, and every point evaluated lies in the
    /// bracket. On atan from 2 over [−1, 3], Newton's step lands at −3.5; the bracket is then [−1, 2] and the next
    /// point its midpoint, 0.5; from there Newton converges to the root 0.
    #[test]
    fn a_step_that_leaves_the_bracket_bisects() {
        let mut seen = Vec::new();
        let root = newton(
            |x| {
                seen.push(x);
                (math::atan(x), 1.0 / (1.0 + x * x))
            },
            (-1.0, 3.0),
            2.0,
            Tol::Absolute(1e-12),
            1e-12,
            50,
        );
        assert_eq!(seen[2..4], [2.0, 0.5], "{seen:?}");
        assert!(seen.iter().all(|x| (-1.0..=3.0).contains(x)), "{seen:?}");
        assert!(root.stop == Stop::Converged && root.x.abs() <= 1e-12, "{root:?}");
    }

    /// The bracket's rules at their boundaries: tole at the end with the smaller |f| (b on a tie), the best end `a` on
    /// a tie; the adjustment keeps a point 0.7·tole inside either end, replaces a NaN by the midpoint, and an end it
    /// cannot leave (tole = 0 at u = 0) too; done at exactly tole, and when no float lies strictly inside.
    #[test]
    fn bracket_rules_at_their_boundaries() {
        let bracket = |a: f64, fa: f64, b: f64, fb: f64| Bracket {
            a,
            fa,
            b,
            fb,
            d: f64::NAN,
            fd: f64::NAN,
            e: f64::NAN,
            fe: f64::NAN,
        };
        let eps = f64::EPSILON;
        assert_eq!(bracket(-1.0, -1.0, 4.0, 1.0).tole(Tol::Relative(0.125)), 2.0 * (0.5 + 8.0 * eps));
        assert_eq!(bracket(-1.0, -0.5, 4.0, 1.0).tole(Tol::Relative(0.125)), 2.0 * (0.125 + 2.0 * eps));
        assert_eq!(bracket(-1.0, -1.0, 4.0, 1.0).best(3, Stop::Converged).x, -1.0);
        // u = 0 and w = 0.125: tole = 0.25, so points stay 0.175 inside [0, 1].
        let (s, tol, delta) = (bracket(0.0, -1e-3, 1.0, 1.0), Tol::Absolute(0.125), 0.7 * 0.25);
        assert_eq!([0.05, 0.99, 0.5, f64::NAN].map(|c| s.adjust(c, tol)), [delta, 1.0 - delta, 0.5, 0.5]);
        assert_eq!(bracket(0.0, -1e-300, 1.0, 1.0).adjust(-1.0, Tol::Absolute(0.0)), 0.5);
        assert_eq!(bracket(-1.0, -1.0, 0.0, 1e-300).adjust(5.0, Tol::Absolute(0.0)), -0.5);
        assert!(bracket(0.0, -1e-3, 0.25, 1.0).done(tol) && !bracket(0.0, -1e-3, 0.5, 1.0).done(tol));
        // Within 2δ of each other the ends give the midpoint, away from 0 too.
        let near = bracket(10.0, -1e-3, 10.3, 1.0);
        assert_eq!(near.adjust(10.05, tol), near.mid());
        // The doubled secant from a (|f(a)| smaller) moves 2/3 of [10, 11]: more than half, so the midpoint; from f(a) =
        // −1, f(b) = 4 it moves 0.4.
        assert_eq!(bracket(10.0, -1.0, 11.0, 2.0).doubled_secant(), 10.5);
        assert_eq!(bracket(10.0, -1.0, 11.0, 4.0).doubled_secant(), 10.0 + 0.4);
        assert_eq!(bracket(10.0, -4.0, 11.0, 1.0).doubled_secant(), 11.0 - 0.4);
        // The cubic x = 1 + f(f + 1)(f + 1)/4 through (−1, 1), (1, 2), (2, 5.5), (−2, 0.5) has its zero at a = 1: not
        // strictly inside, so no cubic step; with e not yet known, none either.
        let mut s = bracket(1.0, -1.0, 2.0, 1.0);
        (s.d, s.fd, s.e, s.fe) = (5.5, 2.0, 0.5, -2.0);
        assert_eq!(inverse_cubic([s.a, s.b, s.d, s.e], [s.fa, s.fb, s.fd, s.fe]), 1.0);
        assert_eq!(s.cubic(), None);
        // And x = 2 + f(f − 1)(f − 1)/4 through (−1, 1), (1, 2), (2, 2.5), (−2, −2.5) at b = 2.
        (s.d, s.fd, s.e, s.fe) = (2.5, 2.0, -2.5, -2.0);
        assert_eq!(inverse_cubic([s.a, s.b, s.d, s.e], [s.fa, s.fb, s.fd, s.fe]), 2.0);
        assert_eq!(s.cubic(), None);
        (s.e, s.fe) = (f64::NAN, f64::NAN);
        assert_eq!(s.cubic(), None);
        let tiny = f64::from_bits(1);
        assert!(bracket(0.0, -1e-300, tiny, 1.0).done(Tol::Absolute(0.0)), "no float inside [0, 5e-324]");
        assert!(!bracket(0.0, -1e-300, 2.0 * tiny, 1.0).done(Tol::Absolute(0.0)), "5e-324 lies inside");
    }

    /// TOMS 748's efficiency on the paper's harder shapes: a steep exponential, a high power and a near-flat function
    /// converge within 20 iterations to a relative 1e-14 (Alefeld, Potra & Shi's table 1 functions behave alike).
    #[test]
    fn toms748_converges_quickly_on_hard_shapes() {
        type Case = (fn(f64) -> f64, f64, f64, f64);
        let cases: [Case; 3] = [
            (|x| math::exp(20.0 * x) - 3.0, -1.0, 1.0, math::ln(3.0) / 20.0),
            (|x| math::powi(x, 15) - 0.5, 0.0, 2.0, math::powf(0.5, 1.0 / 15.0)),
            (|x| (x - 0.7) * (1.0 + 1e-6 * x), 0.0, 1.0, 0.7),
        ];
        for (f, a, b, want) in cases {
            let root = toms748(f, a, b, Tol::Relative(1e-14), 20);
            assert_eq!(root.stop, Stop::Converged, "{root:?}");
            assert!((root.x / want - 1.0).abs() < 1e-13, "{root:?} against {want}");
        }
    }
}
