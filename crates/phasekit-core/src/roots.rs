//! The root toolbox (PLAN.md M6.2; D6; map 03 §3.4, §6, §9): native TOMS 748 and a bracketed Newton/Halley. Every
//! solver returns the residual evaluated AT the returned point ([`Root`]), reports exhaustion, a lost bracket and a
//! non-finite residual as a [`Stop`] instead of accepting the last iterate, and stops on a typed tolerance ([`Tol`]):
//! CoolProp's absolute x tolerance gave a 6.1 % density error at 1e-8 mol/m³, its Newton stops on the step alone and
//! no TOMS 748 call site checks `max_iter` (map 03 §3.4, §6; ROT-074, ROT-075, ROT-076).
#![cfg_attr(not(test), expect(dead_code, reason = "the pure VLE (M6.3) is the first user"))]

use crate::error::Error;
use crate::num::math;
use crate::state::Strategy;

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
            Tol::LogAxis(tol) => a > 0.0 && b > 0.0 && math::ln(b / a).abs() <= tol,
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

/// The zero on (a, b) of the quadratic through (a, fa), (b, fb), (d, fd) by `steps` Newton steps from the end where
/// the quadratic's curvature and value have the same sign (Alefeld, Potra & Shi's NEWQUAD); the secant root when the
/// three points are collinear; the midpoint if the result leaves (a, b).
fn newton_quadratic(a: f64, fa: f64, b: f64, fb: f64, d: f64, fd: f64, steps: u32) -> f64 {
    let slope = (fb - fa) / (b - a);
    let curvature = ((fd - fb) / (d - b) - slope) / (d - a);
    if curvature == 0.0 || !curvature.is_finite() {
        return interior(a, b, a - fa / slope);
    }
    let mut r = if curvature * fa > 0.0 { a } else { b };
    for _ in 0..steps {
        let p = fa + (slope + curvature * (r - b)) * (r - a);
        r -= p / (slope + curvature * (2.0 * r - a - b));
    }
    interior(a, b, r)
}

/// The zero of the inverse cubic through four points (x as a polynomial in f), by Lagrange's formula at f = 0.
fn inverse_cubic(x: [f64; 4], f: [f64; 4]) -> f64 {
    let mut sum = 0.0;
    for i in 0..4 {
        let mut term = x[i];
        for j in (0..4).filter(|&j| j != i) {
            term *= f[j] / (f[j] - f[i]);
        }
        sum += term;
    }
    sum
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
    /// Replaces the end on c's side of the sign change by `c` (f(c) ≠ 0), keeping the replaced end as `d`.
    fn take(&mut self, c: f64, fc: f64) {
        (self.e, self.fe) = (self.d, self.fd);
        if self.fa.signum() * fc.signum() < 0.0 {
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

    /// Whether the bracket meets `tol`, or holds no float strictly inside it.
    fn done(&self, tol: Tol) -> bool {
        let mid = self.a + (self.b - self.a) / 2.0;
        tol.met(self.a, self.b) || mid <= self.a || mid >= self.b
    }

    /// The four points for inverse cubic interpolation, when `e` exists and all four f values differ.
    fn four(&self) -> Option<([f64; 4], [f64; 4])> {
        let f = [self.fa, self.fb, self.fd, self.fe];
        let distinct = (0..4).all(|i| (i + 1..4).all(|j| f[i] != f[j]));
        (self.e.is_finite() && distinct).then_some(([self.a, self.b, self.d, self.e], f))
    }
}

/// TOMS 748 (Alefeld, Potra & Shi, ACM TOMS 21 (1995) 327-344, Algorithm 4.2, μ = 1/2) on [a, b] with f(a)·f(b) ≤ 0:
/// per iteration two interpolation steps (inverse cubic once four distinct values exist, else a quadratic by Newton
/// steps), a double-length secant step from the better end, and a bisection whenever the bracket did not halve.
/// Converged when the bracket meets `tol` (or holds no float strictly inside); the root is the end with the smaller
/// |f|, with its f. A residual that is exactly zero ends the search there.
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
    if fa.signum() == fb.signum() {
        return s.best(0, Stop::BracketLost);
    }
    // Evaluates c, ends the search on a zero or a non-finite value, else narrows the bracket.
    let mut step = |s: &mut Bracket, c: f64, iterations: u16| -> Option<Root> {
        let fc = f(c);
        if !fc.is_finite() {
            return Some(Root { x: c, f: fc, iterations, stop: Stop::NotFinite });
        }
        if fc == 0.0 {
            return Some(Root { x: c, f: fc, iterations, stop: Stop::Converged });
        }
        s.take(c, fc);
        None
    };
    let secant = interior(s.a, s.b, s.a - s.fa * (s.b - s.a) / (s.fb - s.fa));
    if let Some(root) = step(&mut s, secant, 0) {
        return root;
    }
    for iteration in 1..=max_iter {
        if s.done(tol) {
            return s.best(iteration - 1, Stop::Converged);
        }
        let width = s.b - s.a;
        for steps in [2, 3] {
            let cubic = s.four().map(|(x, f)| inverse_cubic(x, f)).filter(|&c| s.a < c && c < s.b);
            let c = cubic.unwrap_or_else(|| newton_quadratic(s.a, s.fa, s.b, s.fb, s.d, s.fd, steps));
            if let Some(root) = step(&mut s, c, iteration) {
                return root;
            }
            if s.done(tol) {
                return s.best(iteration, Stop::Converged);
            }
        }
        let (u, fu) = if s.fa.abs() < s.fb.abs() { (s.a, s.fa) } else { (s.b, s.fb) };
        let doubled = u - 2.0 * fu / (s.fb - s.fa) * (s.b - s.a);
        let c = if (doubled - u).abs() > (s.b - s.a) / 2.0 { s.a + (s.b - s.a) / 2.0 } else { doubled };
        let c = interior(s.a, s.b, c);
        if let Some(root) = step(&mut s, c, iteration) {
            return root;
        }
        if s.b - s.a > 0.5 * width {
            let mid = s.a + (s.b - s.a) / 2.0;
            if let Some(root) = step(&mut s, mid, iteration) {
                return root;
            }
        }
    }
    let stop = if s.done(tol) { Stop::Converged } else { Stop::MaxIterations };
    s.best(max_iter, stop)
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
    if f_lo.signum() == f_hi.signum() {
        let (x, fx) = if f_lo.abs() <= f_hi.abs() { (lo, f_lo) } else { (hi, f_hi) };
        return Root { x, f: fx, iterations: 0, stop: Stop::BracketLost };
    }
    // The ends where f is negative and positive.
    let (mut neg, mut pos) = if f_lo < 0.0 { (lo, hi) } else { (hi, lo) };
    let mut x = interior(lo, hi, x0);
    let (mut fx, mut step) = f(x);
    for iteration in 1..=max_iter {
        if !fx.is_finite() {
            return Root { x, f: fx, iterations: iteration - 1, stop: Stop::NotFinite };
        }
        if fx == 0.0 {
            return Root { x, f: fx, iterations: iteration - 1, stop: Stop::Converged };
        }
        if fx < 0.0 {
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
        assert_eq!(root.stop, Stop::NotFinite, "{root:?}");
        assert!(root.f.is_nan() && root.x > 2.05, "{root:?}");
        let refused = Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: root.iterations });
        assert_eq!(root.converged(Strategy::Vle), refused);
        let root = newton(|x| (nan_above(x), 3.0 * x * x - 2.0), (2.0, 3.0), 2.01, Tol::Relative(1e-15), 1e-12, 50);
        assert_eq!(root.stop, Stop::NotFinite, "{root:?}");
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
    }

    /// The interpolation steps are exact on their own model: the quadratic step finds a quadratic's zero, and the
    /// inverse cubic a zero of x as a cubic in f; outside (a, b) both fall back to the midpoint.
    #[test]
    fn interpolation_steps_are_exact_on_their_models() {
        let q = |x: f64| (x - 1.5) * (x + 4.0);
        let r = newton_quadratic(1.0, q(1.0), 2.0, q(2.0), 3.0, q(3.0), 8);
        assert!((r - 1.5).abs() < 1e-15, "{r}");
        let line = |x: f64| 2.0 * x - 3.0;
        assert_eq!(newton_quadratic(1.0, line(1.0), 2.0, line(2.0), 3.0, line(3.0), 2), 1.5);
        let x_of = |f: f64| 1.25 + 0.5 * f - 0.25 * f * f + 0.125 * f * f * f;
        let fs = [-0.75, -0.25, 0.5, 1.5];
        assert!((inverse_cubic(fs.map(x_of), fs) - 1.25).abs() < 1e-15);
        assert_eq!(interior(1.0, 2.0, 2.5), 1.5);
        assert_eq!(interior(1.0, 2.0, 1.25), 1.25);
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
