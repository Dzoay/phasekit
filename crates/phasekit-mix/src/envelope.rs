//! Phase envelopes (PLAN.md M13.12): a mixture's bubble and dew points traced as one curve by Michelsen's continuation,
//! as GERG TM15 §7.7.1 (Kunz et al. 2007) describes it.
//!
//! - The unknowns are X = (ln K_1 .. ln K_N, ln T, ln p) at a fixed vapour fraction β, 0 from a bubble point (1 from a
//!   dew point), the phases x_i = z_i/(1 + β(K_i − 1)) and y_i = K_i x_i as in M13.10. The first N + 1 equations are
//!   M13.9's, F_i = ln K_i + ln φ_i(y) − ln φ_i(x) and Σ(y − x); the last specifies one unknown, X_s − S. Each point
//!   ends on M13.9's residual gate, so it is a bubble or a dew point.
//! - From a converged point the sensitivities dX/dS solve J dX/dS = e_{N+2} (TM15 Eq. 7.131). The next point specifies
//!   the unknown that varies fastest, so no other moves more than it does: most often ln K of the most or the least
//!   volatile component, and never ln T or ln p near the critical point, where they would lead to the trivial solution,
//!   nor at a pressure maximum, where dX/d ln p is infinite. It starts from the first-order prediction X + (dX/dS) ΔS
//!   (TM15 Eq. 7.133).
//! - The step in S grows where a point converged in few Newton iterations and shrinks where it took many (TM15's target
//!   of three or four), and halves where a point fails, down to `MIN_STEP`.
//! - β stays fixed through the critical point, where every ln K changes sign: the phase of the feed's fractions turns
//!   from the denser to the lighter (TM15). The trace keeps which phase is the denser and swaps their density roots
//!   where a step crosses ln K = 0 of the component most volatile at the start while the phases' densities are within
//!   `NEAR_CRITICAL` of each other in ln ρ; an azeotrope, where every ln K is 0 too, keeps a liquid and a vapour. The
//!   crossing itself is interpolated, never solved, for there the two phases are one: (T, p) at ln K = 0 by cubic
//!   Hermite interpolation between the points either side (TM15 Eq. 7.134).
//! - It starts at the bubble point at the stated low pressure (1 bar by default) and, where that fails or lies below the
//!   model's lowest temperature, at the dew point there. It ends typed (ROT-188): closed where it returns to the starting
//!   pressure past its highest, at a bound where a point leaves the model's temperature range or rises above the stated
//!   (else the model's) highest pressure, and without convergence where the step falls below `MIN_STEP`, the start
//!   fails, or the trace runs past its stated number of points.
//! - Nothing in a flash reads an envelope (ROT-189): it is a value the caller holds.

use phasekit_core::density::Branch;
use phasekit_core::{DomainError, Error, Limits, Strategy, math};

use crate::Mixture;
use crate::equilibrium::{AtTp, normalised, solve, square};
use crate::saturation::{Given, Side, fractions, meets_gate, on_branch, saturation, trivial};

/// Newton iterations of one point, and each iteration's largest step in ln K and in ln T or ln p (M13.9's).
const CORRECTOR_STEPS: usize = 12;
const CUT: (f64, f64) = (0.5, 0.1);

/// The step in the specified unknown: the first, the largest, and the smallest before the trace gives up.
const FIRST_STEP: f64 = 0.05;
const MAX_STEP: f64 = 0.25;
const MIN_STEP: f64 = 1e-6;

/// Phases within this of each other in |ln(ρ′/ρ″)| are near the critical point, where a crossing of ln K = 0 swaps
/// which of them is the denser.
const NEAR_CRITICAL: f64 = 0.5;

/// Where a step crosses the critical point, the trace closes on it from each side to within this in ln K of the key, in
/// at most this many points a side, before it interpolates the crossing: nearer, the equations' conditioning moves a
/// point by more than the interpolation gains.
const CRITICAL_SPAN: f64 = 0.002;
const REFINEMENTS: usize = 8;

/// What an envelope's trace is asked: its starting pressure, the highest pressure it may reach, and the most points it
/// may take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvelopeOptions {
    p_start: f64,
    p_max: Option<f64>,
    points: usize,
}

impl EnvelopeOptions {
    /// From a bubble or dew point at 1 bar, up to the model's highest pressure, in at most 1000 points.
    pub fn new() -> EnvelopeOptions {
        EnvelopeOptions { p_start: 1e5, p_max: None, points: 1000 }
    }

    /// Starts the trace at `p`, Pa.
    #[must_use]
    pub fn with_start_pressure(mut self, p: f64) -> EnvelopeOptions {
        self.p_start = p;
        self
    }

    /// Bounds the trace at `p`, Pa, below the model's highest pressure where that is lower.
    #[must_use]
    pub fn with_max_pressure(mut self, p: f64) -> EnvelopeOptions {
        self.p_max = Some(p);
        self
    }

    /// Ends the trace, without convergence, after `points` points.
    #[must_use]
    pub fn with_max_points(mut self, points: usize) -> EnvelopeOptions {
        self.points = points;
        self
    }
}

impl Default for EnvelopeOptions {
    fn default() -> EnvelopeOptions {
        EnvelopeOptions::new()
    }
}

/// How an envelope's trace ended.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum EnvelopeEnd {
    /// It returned to its starting pressure past its highest.
    Closed,
    /// Its next point lies outside the model's temperature range or above the highest pressure.
    Bound(DomainError),
    /// Its start failed, its step fell below the smallest, or it ran past its points; the error that ended it.
    NoConvergence(Error),
}

/// A phase envelope as a struct of arrays, one entry a point in the trace's order, each a bubble or a dew point.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    /// Temperature, K.
    pub t: Vec<f64>,
    /// Pressure, Pa.
    pub p: Vec<f64>,
    /// Which phase the point names: `Side::Bubble` where the feed (the bulk phase) is the denser and a lighter phase is
    /// incipient, `Side::Dew` where the feed is the lighter.
    pub side: Vec<Side>,
    /// The feed's molar density, mol/m³.
    pub rho_bulk: Vec<f64>,
    /// The incipient phase's molar density, mol/m³.
    pub rho_incipient: Vec<f64>,
    /// The incipient phase's mole fractions, component-major: `incipient[i][k]` is component i's at point k.
    pub incipient: Vec<Vec<f64>>,
    /// The critical point the trace passed, (T, p) interpolated at ln K = 0; `None` where it passed none.
    pub critical: Option<(f64, f64)>,
    /// How the trace ended.
    pub end: EnvelopeEnd,
}

impl Envelope {
    /// The number of points.
    pub fn len(&self) -> usize {
        self.t.len()
    }

    /// Whether the trace found no point.
    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }
}

impl Envelope {
    /// No points yet, for `n` components.
    fn empty(n: usize) -> Envelope {
        Envelope {
            t: Vec::new(),
            p: Vec::new(),
            side: Vec::new(),
            rho_bulk: Vec::new(),
            rho_incipient: Vec::new(),
            incipient: vec![Vec::new(); n],
            critical: None,
            end: EnvelopeEnd::Closed,
        }
    }

    /// This envelope, ended by `end`.
    fn ended(mut self, end: EnvelopeEnd) -> Envelope {
        self.end = end;
        self
    }
}

impl Mixture {
    /// The phase envelope at the feed's composition, traced as `opts` asks (PLAN.md M13.12). A starting pressure that is
    /// not finite and positive, or a highest pressure not above it, is `InvalidInput`; the trace itself always ends
    /// typed, in `Envelope::end`.
    pub fn envelope(&self, opts: &EnvelopeOptions) -> Result<Envelope, Error> {
        let p_start = opts.p_start;
        if !(p_start.is_finite() && p_start > 0.0) {
            return Err(Error::InvalidInput { quantity: "starting pressure", value: p_start });
        }
        if let Some(p_max) = opts.p_max
            && !(p_max.is_finite() && p_max > p_start)
        {
            return Err(Error::InvalidInput { quantity: "highest pressure", value: p_max });
        }
        let limits = self.range();
        let p_max = opts.p_max.map_or(limits.p_max(), |p| p.min(limits.p_max()));
        Ok(match Tracer::start(self, p_start, limits.t_min()) {
            Ok((tracer, first)) => tracer.trace(first, limits, (p_max, opts.points)),
            Err(e) => Envelope::empty(self.composition().len()).ended(EnvelopeEnd::NoConvergence(e)),
        })
    }
}

/// A trace's state: the feed, its vapour fraction β, the component most volatile at the start (whose ln K changes sign
/// at the critical point), whether the phase of fractions x is the denser, and each phase's last density, the next
/// solve's seed.
struct Tracer<'a> {
    mixture: &'a Mixture,
    beta: f64,
    z: Vec<f64>,
    key: usize,
    x_denser: bool,
    seeds: (Option<f64>, Option<f64>),
}

/// The phases at an iterate: of fractions x and of y, and those fractions as the mass balance gives them.
struct Phases {
    of_x: AtTp,
    of_y: AtTp,
    x: Vec<f64>,
    y: Vec<f64>,
}

/// A converged point: its unknowns (ln K_1 .. ln K_N, ln T, ln p), its phases, and the Newton iterations it took.
struct Point {
    v: Vec<f64>,
    at: Phases,
    iterations: usize,
}

/// The index of the largest |s_i|: the unknown that varies fastest.
fn fastest(s: &[f64]) -> usize {
    (0..s.len()).max_by(|a, b| s[*a].abs().total_cmp(&s[*b].abs())).unwrap_or(0)
}

/// The unknown that varies fastest along sensitivities `s`, and the sensitivities in it, dX/dX_next = s/s_next.
fn along(s: &[f64]) -> (usize, Vec<f64>) {
    let next = fastest(s);
    (next, s.iter().map(|d| d / s[next]).collect())
}

/// Whether a ln K changes sign from `a` to `b`, 0 counted with the positive.
fn crosses(a: f64, b: f64) -> bool {
    (a < 0.0) != (b < 0.0)
}

/// The target of a step from `from` to `to` in unknown `spec` of N + 2: `to` itself, but for a ln K (spec < N) that
/// would cross 0 or land on it, where the two phases are one, the mirror of `from`.
fn landing((spec, n): (usize, usize), from: f64, to: f64) -> f64 {
    if spec < n && (to == 0.0 || crosses(from, to)) { -from } else { to }
}

/// Each Newton iteration's largest step in unknown i of N + 2: `CUT`'s first in a ln K, its second in ln T and ln p.
fn cut((i, n): (usize, usize)) -> f64 {
    if i < n { CUT.0 } else { CUT.1 }
}

/// The side a point names by its feed's density `bulk` and the incipient phase's: a bubble point where the feed is the
/// denser, else a dew point.
fn side_of(bulk: f64, incipient: f64) -> Side {
    if bulk > incipient { Side::Bubble } else { Side::Dew }
}

/// The bound a point at (T, p) lies beyond: below the model's lowest temperature, above its highest, or above `p_max`.
/// A point on a limit lies within it.
fn bound(t: f64, p: f64, limits: &Limits, p_max: f64) -> Option<DomainError> {
    if t < limits.t_min() {
        Some(DomainError::BelowMinTemperature { t, t_min: limits.t_min() })
    } else if t > limits.t_max() {
        Some(DomainError::AboveMaxTemperature { t, t_max: limits.t_max() })
    } else if p > p_max {
        Some(DomainError::AboveMaxPressure { p, p_max })
    } else {
        None
    }
}

/// The step after one that converged in `iterations`: grown where they were few, kept at four, halved beyond (TM15's
/// target of three or four), at most `MAX_STEP`.
fn adapted(h: f64, iterations: usize) -> f64 {
    let factor = match iterations {
        0..=2 => 2.0,
        3 => 1.25,
        4 => 1.0,
        _ => 0.5,
    };
    (h * factor).min(MAX_STEP)
}

/// The cubic Hermite interpolant at u through (u_a, f_a) and (u_b, f_b) with slopes d_a and d_b (TM15 Eq. 7.134).
fn hermite((ua, fa, da): (f64, f64, f64), (ub, fb, db): (f64, f64, f64), u: f64) -> f64 {
    let h = ub - ua;
    let s = (u - ua) / h;
    let (s2, s3) = (s * s, s * s * s);
    (2.0 * s3 - 3.0 * s2 + 1.0) * fa + (s3 - 2.0 * s2 + s) * h * da + (3.0 * s2 - 2.0 * s3) * fb + (s3 - s2) * h * db
}

impl<'a> Tracer<'a> {
    /// The trace's first point and its state: the bubble point at `p_start`, else the dew point there, else the bubble
    /// or the dew point at the model's lowest temperature `t_min`; the first of them at or above `t_min` that
    /// converges as the trace's own point, specified in ln p.
    fn start(mixture: &'a Mixture, p_start: f64, t_min: f64) -> Result<(Tracer<'a>, Point), Error> {
        let mut last = Error::NoConvergence { strategy: Strategy::Vle, iterations: 0 };
        let starts = [(Side::Bubble, Given::P(p_start)), (Side::Dew, Given::P(p_start))];
        for (side, given) in starts.into_iter().chain([(Side::Bubble, Given::T(t_min)), (Side::Dew, Given::T(t_min))]) {
            match saturation(mixture, side, given) {
                Ok(s) if s.t >= t_min => match Tracer::from_point(mixture, side, &s) {
                    Ok(found) => return Ok(found),
                    Err(e) => last = e,
                },
                Ok(s) => last = DomainError::BelowMinTemperature { t: s.t, t_min }.into(),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// The trace from bubble or dew point `s` on `side`: β 0 or 1, ln K = ln(y/x), the key the component of largest
    /// ln K, the denser phase the liquid's fractions x; `s` solved again as the trace's own point.
    fn from_point(mixture: &'a Mixture, side: Side, s: &crate::Saturation) -> Result<(Tracer<'a>, Point), Error> {
        let z = mixture.composition().to_vec();
        let ln_k: Vec<f64> = match side {
            Side::Bubble => s.incipient.iter().zip(&z).map(|(y, x)| math::ln(y / x)).collect(),
            Side::Dew => z.iter().zip(&s.incipient).map(|(y, x)| math::ln(y / x)).collect(),
        };
        let (beta, seeds) = match side {
            Side::Bubble => (0.0, (s.rho, s.rho_incipient)),
            Side::Dew => (1.0, (s.rho_incipient, s.rho)),
        };
        let key = (0..ln_k.len()).max_by(|a, b| ln_k[*a].total_cmp(&ln_k[*b])).unwrap_or(0);
        let n = z.len();
        let mut tracer = Tracer { mixture, beta, z, key, x_denser: true, seeds: (Some(seeds.0), Some(seeds.1)) };
        let mut v = ln_k;
        v.extend([math::ln(s.t), math::ln(s.p)]);
        let target = v[n + 1];
        let point = tracer.correct(v, (n + 1, target))?;
        Ok((tracer, point))
    }

    /// The phases at unknowns `v`, the denser on the liquid's branch.
    fn phases(&mut self, v: &[f64]) -> Result<Phases, Error> {
        let n = self.z.len();
        let (t, p) = (math::exp(v[n]), math::exp(v[n + 1]));
        let (x, y) = fractions(self.beta, &self.z, &v[..n]);
        let (for_x, for_y) =
            if self.x_denser { (Branch::Liquid, Branch::Vapour) } else { (Branch::Vapour, Branch::Liquid) };
        let of_x = on_branch(self.mixture, t, p, &normalised(&x), for_x, self.seeds.0)?;
        let of_y = on_branch(self.mixture, t, p, &normalised(&y), for_y, self.seeds.1)?;
        self.seeds = (Some(of_x.rho), Some(of_y.rho));
        Ok(Phases { of_x, of_y, x, y })
    }

    /// M13.9's N + 1 equations at `v`, F_i = ln K_i + ln φ_i(y) − ln φ_i(x) and Σ(y − x), with the phases.
    fn equations(&mut self, v: &[f64]) -> Result<(Vec<f64>, Phases), Error> {
        let at = self.phases(v)?;
        let (of_x, of_y) = (at.of_x.ln_phi(), at.of_y.ln_phi());
        let mut f: Vec<f64> = (0..self.z.len()).map(|i| v[i] + of_y[i] - of_x[i]).collect();
        f.push(at.y.iter().zip(&at.x).map(|(y, x)| y - x).sum());
        Ok((f, at))
    }

    /// The Jacobian of the N + 2 equations in (ln K, ln T, ln p), the last specifying unknown `spec` (TM15 Eqs.
    /// 7.122-7.128, with M13.9's composition block).
    fn jacobian(&self, v: &[f64], at: &Phases, spec: usize) -> Vec<Vec<f64>> {
        let (n, beta) = (self.z.len(), self.beta);
        let (t, p) = (math::exp(v[n]), math::exp(v[n + 1]));
        let moves: Vec<f64> = (0..n).map(|k| at.x[k] * at.y[k] / self.z[k]).collect();
        let (sum_x, sum_y) = (at.x.iter().sum::<f64>(), at.y.iter().sum::<f64>());
        let phi = |i: usize, k: usize| {
            (1.0 - beta) * at.of_y.c.n_dln_phi_dn(i, k) / sum_y + beta * at.of_x.c.n_dln_phi_dn(i, k) / sum_x
        };
        let in_k = |i: usize, k: usize| (if i == k { 1.0 } else { 0.0 }) + moves[k] * phi(i, k);
        square(n + 2, |i, k| match (i, k) {
            (i, k) if i == n + 1 => f64::from(u8::from(k == spec)),
            (i, k) if i == n => {
                if k < n {
                    moves[k]
                } else {
                    0.0
                }
            }
            (i, k) if k < n => in_k(i, k),
            (i, k) if k == n => t * (at.of_y.c.dln_phi_dt(i) - at.of_x.c.dln_phi_dt(i)),
            (i, _) => p * (at.of_y.c.dln_phi_dp(i) - at.of_x.c.dln_phi_dp(i)),
        })
    }

    /// Newton's method from `v` with unknown `spec` held at `target`, each step cut to `CUT`, to M13.9's gate; the
    /// trivial solution refused.
    fn correct(&mut self, mut v: Vec<f64>, (spec, target): (usize, f64)) -> Result<Point, Error> {
        let n = self.z.len();
        let failed =
            |iterations: usize| Error::NoConvergence { strategy: Strategy::Vle, iterations: iterations as u16 };
        v[spec] = target;
        for iterations in 0..CORRECTOR_STEPS {
            let (f, at) = self.equations(&v)?;
            if trivial(&v[..n], at.of_x.rho, at.of_y.rho) {
                return Err(failed(iterations));
            }
            if meets_gate(&f) {
                return Ok(Point { v, at, iterations });
            }
            let rhs: Vec<f64> = f.iter().map(|f| -f).chain([0.0]).collect();
            let Some(step) = solve(self.jacobian(&v, &at, spec), rhs) else { return Err(failed(iterations)) };
            for (i, (x, s)) in v.iter_mut().zip(&step).enumerate() {
                *x += s.clamp(-cut((i, n)), cut((i, n)));
            }
        }
        Err(failed(CORRECTOR_STEPS))
    }

    /// dX/dX_spec at a converged point: J dX/dS = e_{N+2} (TM15 Eq. 7.131).
    fn sensitivity(&self, point: &Point, spec: usize) -> Option<Vec<f64>> {
        let size = self.z.len() + 2;
        let e = (0..size).map(|i| if i + 1 == size { 1.0 } else { 0.0 }).collect();
        solve(self.jacobian(&point.v, &point.at, spec), e)
    }

    /// Whether a step from `point` to `to` swaps the denser phase: it crosses ln K = 0 of the key where the phases are
    /// near the critical point.
    fn swaps(&self, point: &Point, to: &[f64]) -> bool {
        crosses(point.v[self.key], to[self.key])
            && math::ln(point.at.of_x.rho / point.at.of_y.rho).abs() < NEAR_CRITICAL
    }

    /// The next point from `point` along `ds` (dX/dX_spec), the trace running in X_spec the way `direction` says: from
    /// step h, halved where the point fails or its crossing of ln K = 0 is not the one its phases were chosen for,
    /// down to `MIN_STEP`; with the step for the one after.
    fn advance(
        &mut self,
        point: &Point,
        ds: &[f64],
        (spec, direction): (usize, f64),
        mut h: f64,
    ) -> Result<(Point, f64), Error> {
        let (seeds, denser, n) = (self.seeds, self.x_denser, self.z.len());
        loop {
            let from = point.v[spec];
            let target = landing((spec, n), from, from + direction * h);
            let predicted: Vec<f64> = point.v.iter().zip(ds).map(|(x, d)| x + d * (target - from)).collect();
            let swapped = self.swaps(point, &predicted);
            self.x_denser = denser != swapped;
            let error = match self.correct(predicted, (spec, target)) {
                Ok(next) if self.swaps(point, &next.v) == swapped => {
                    let step = adapted(h, next.iterations);
                    return Ok((next, step));
                }
                Ok(next) => Error::NoConvergence { strategy: Strategy::Vle, iterations: next.iterations as u16 },
                Err(e) => e,
            };
            (self.seeds, self.x_denser) = (seeds, denser);
            h /= 2.0;
            if h < MIN_STEP {
                return Err(error);
            }
        }
    }

    /// Points nearer the critical point than `CRITICAL_SPAN` in ln K_key, from `from` on its own side of it (`x_denser`
    /// as its phases are): each a quarter of the last's ln K_key, at most `REFINEMENTS` of them, until one fails. The
    /// points solved, nearest last; the trace's own state as it was.
    fn toward_critical(&mut self, from: &Point, x_denser: bool) -> Vec<Point> {
        let saved = (self.seeds, self.x_denser);
        self.x_denser = x_denser;
        let mut out: Vec<Point> = Vec::new();
        for _ in 0..REFINEMENTS {
            let last = out.last().unwrap_or(from);
            let u = last.v[self.key];
            if u.abs() <= CRITICAL_SPAN {
                break;
            }
            let Some(ds) = self.sensitivity(last, self.key) else { break };
            let target = 0.25 * u;
            let predicted: Vec<f64> = last.v.iter().zip(&ds).map(|(x, d)| x + d * (target - u)).collect();
            self.seeds = (Some(last.at.of_x.rho), Some(last.at.of_y.rho));
            match self.correct(predicted, (self.key, target)) {
                Ok(near) => out.push(near),
                Err(_) => break,
            }
        }
        (self.seeds, self.x_denser) = saved;
        out
    }

    /// Where the step from `point` to `found` crossed the critical point: the points that close on it from each side,
    /// recorded in the trace's order, and the crossing interpolated between the nearest two.
    fn cross_critical(&mut self, env: &mut Envelope, point: &Point, found: &Point) {
        let before = self.toward_critical(point, !self.x_denser);
        let after = self.toward_critical(found, self.x_denser);
        let (a, b) = (before.last().unwrap_or(point), after.last().unwrap_or(found));
        env.critical = self.critical_between(a, b);
        for near in before.iter().chain(after.iter().rev()) {
            self.record(env, near);
        }
    }

    /// (T, p) at ln K_key = 0 between points `a` and `b` either side of it, by cubic Hermite interpolation in ln K_key.
    fn critical_between(&self, a: &Point, b: &Point) -> Option<(f64, f64)> {
        let n = self.z.len();
        let (sa, sb) = (self.sensitivity(a, self.key)?, self.sensitivity(b, self.key)?);
        let at = |j: usize| math::exp(hermite((a.v[self.key], a.v[j], sa[j]), (b.v[self.key], b.v[j], sb[j]), 0.0));
        Some((at(n), at(n + 1)))
    }

    /// Appends `point`: its T and p, the feed's and the incipient phase's densities and the incipient fractions, and the
    /// side its denser phase names.
    fn record(&self, env: &mut Envelope, point: &Point) {
        let n = self.z.len();
        let (bulk, incipient, fractions) = if self.beta == 0.0 {
            (point.at.of_x.rho, point.at.of_y.rho, normalised(&point.at.y))
        } else {
            (point.at.of_y.rho, point.at.of_x.rho, normalised(&point.at.x))
        };
        env.t.push(math::exp(point.v[n]));
        env.p.push(math::exp(point.v[n + 1]));
        env.side.push(side_of(bulk, incipient));
        env.rho_bulk.push(bulk);
        env.rho_incipient.push(incipient);
        for (column, x) in env.incipient.iter_mut().zip(fractions) {
            column.push(x);
        }
    }

    /// The envelope from `first`: each next point specifying the unknown that varies fastest, until the trace returns
    /// to the first point's pressure past its highest, leaves `limits` or rises above `p_max`, or fails.
    fn trace(mut self, first: Point, limits: &Limits, (p_max, points): (f64, usize)) -> Envelope {
        let n = self.z.len();
        let mut env = Envelope::empty(n);
        let failed =
            |iterations: usize| Error::NoConvergence { strategy: Strategy::Vle, iterations: iterations as u16 };
        let ln_start = first.v[n + 1];
        let mut highest = ln_start;
        self.record(&mut env, &first);
        let (mut point, mut spec, mut direction, mut h) = (first, n + 1, 1.0, FIRST_STEP);
        loop {
            if env.len() >= points {
                return env.ended(EnvelopeEnd::NoConvergence(failed(points)));
            }
            let Some(s) = self.sensitivity(&point, spec) else {
                return env.ended(EnvelopeEnd::NoConvergence(failed(0)));
            };
            let (next, ds) = along(&s);
            direction *= s[next].signum();
            spec = next;
            let (found, step) = match self.advance(&point, &ds, (spec, direction), h) {
                Ok(found) => found,
                Err(e) => return env.ended(EnvelopeEnd::NoConvergence(e)),
            };
            if let Some(beyond) = bound(math::exp(found.v[n]), math::exp(found.v[n + 1]), limits, p_max) {
                return env.ended(EnvelopeEnd::Bound(beyond));
            }
            if self.swaps(&point, &found.v) {
                self.cross_critical(&mut env, &point, &found);
            }
            self.record(&mut env, &found);
            highest = highest.max(found.v[n + 1]);
            if found.v[n + 1] <= ln_start && highest > ln_start {
                return env.ended(EnvelopeEnd::Closed);
            }
            (point, h) = (found, step);
        }
    }
}

#[cfg(test)]
mod tests {
    use phasekit_core::{DataSet, Registry};

    use super::*;
    use crate::MultiFluid;

    /// The library's mixture of `components` at `z`, on `Parity`.
    fn library(components: &[&str], z: &[f64]) -> Mixture {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap_or_else(|e| panic!("{e:?}"));
        let model = MultiFluid::new(&registry, components, DataSet::Parity).unwrap_or_else(|e| panic!("{e:?}"));
        Mixture::new(model, z).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn ok<T>(r: Result<T, Error>) -> T {
        r.unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// The trace of methane/ethane at 50/50 from its bubble or dew point at 1 MPa, and that point.
    fn traced(side: Side) -> (Mixture, Point, Vec<f64>) {
        let m = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let s = ok(saturation(&m, side, Given::P(1e6)));
        let (_, point) = ok(Tracer::from_point(&m, side, &s));
        let v = point.v.clone();
        (m, point, v)
    }

    /// The central difference of the trace's N + 1 equations in unknown k at `v`.
    fn column(tracer: &mut Tracer<'_>, v: &[f64], k: usize) -> Vec<f64> {
        let h = 1e-6;
        let mut moved = |d: f64| {
            let mut w = v.to_vec();
            w[k] += d;
            ok(tracer.equations(&w)).0
        };
        let (up, down) = (moved(h), moved(-h));
        up.iter().zip(&down).map(|(u, d)| (u - d) / (2.0 * h)).collect()
    }

    /// The fastest-varying unknown is the one of largest |s_i|, of either sign; of none, the first.
    #[test]
    fn the_fastest_unknown_is_the_largest() {
        assert_eq!((fastest(&[0.1, -0.5, 0.3]), fastest(&[0.4, 0.2, -0.1]), fastest(&[])), (1, 0, 0));
    }

    /// A ln K target that crosses 0, or lands on it, lands at the mirror of where the step starts; a ln K target on the
    /// step's own side, and a ln T or ln p target (unknowns N and N + 1), is the target itself. A sign change counts 0
    /// with the positive.
    #[test]
    fn steps_never_land_on_the_critical_point() {
        let (k, t, p) = ((0, 2), (2, 2), (3, 2));
        assert_eq!((landing(k, 0.1, 0.05), landing(k, -0.1, -0.02), landing(k, 0.0, 0.05)), (0.05, -0.02, 0.05));
        assert_eq!((landing(k, 0.1, -0.05), landing(k, -0.1, 0.05), landing(k, 0.1, 0.0)), (-0.1, 0.1, -0.1));
        assert_eq!((landing(t, 0.1, -0.05), landing(p, 0.1, -0.05)), (-0.05, -0.05));
        assert!(crosses(0.1, -0.1) && crosses(-0.1, 0.1) && !crosses(-0.1, -0.2));
        assert!(!crosses(0.0, 0.1) && !crosses(0.1, 0.0) && crosses(0.0, -0.1));
    }

    /// The unknown that varies fastest and the sensitivities in it; each Newton step cut to 0.5 in a ln K and 0.1 in
    /// ln T and ln p; a point's side by which phase is the denser, the feed's on a tie a dew point.
    #[test]
    fn the_trace_s_small_rules() {
        assert_eq!(along(&[0.5, -2.0, 1.0]), (1, vec![-0.25, 1.0, -0.5]));
        assert_eq!([0, 1, 2, 3].map(|i| cut((i, 2))), [0.5, 0.5, 0.1, 0.1]);
        assert_eq!((side_of(2.0, 1.0), side_of(1.0, 2.0), side_of(1.0, 1.0)), (Side::Bubble, Side::Dew, Side::Dew));
    }

    /// A point beyond the model's lowest or highest temperature, or above the highest pressure, is at that bound; one
    /// on a limit is within it.
    #[test]
    fn points_beyond_the_limits_are_bounds() {
        let m = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let limits = m.range();
        let (lo, hi, p_max) = (limits.t_min(), limits.t_max(), 1e6);
        assert_eq!(
            [bound(lo, 1e5, limits, p_max), bound(hi, 1e5, limits, p_max), bound(200.0, p_max, limits, p_max)],
            [None, None, None]
        );
        let below = bound(0.5 * lo, 1e5, limits, p_max);
        assert_eq!(below, Some(DomainError::BelowMinTemperature { t: 0.5 * lo, t_min: lo }));
        let above = bound(2.0 * hi, 1e5, limits, p_max);
        assert_eq!(above, Some(DomainError::AboveMaxTemperature { t: 2.0 * hi, t_max: hi }));
        assert_eq!(bound(200.0, 2e6, limits, p_max), Some(DomainError::AboveMaxPressure { p: 2e6, p_max }));
    }

    /// A step that no point converges from is halved down to `MIN_STEP` and given up, its last error returned: a
    /// prediction that is not a number from methane/ethane's bubble point at 1 MPa.
    #[test]
    fn a_step_that_never_converges_is_given_up() {
        let (m, point, _) = traced(Side::Bubble);
        let s = ok(saturation(&m, Side::Bubble, Given::P(1e6)));
        let (mut tracer, _) = ok(Tracer::from_point(&m, Side::Bubble, &s));
        let nowhere = vec![f64::NAN; 4];
        assert!(tracer.advance(&point, &nowhere, (3, 1.0), FIRST_STEP).is_err());
        assert!(
            Envelope::empty(2).is_empty() && !ok(m.envelope(&EnvelopeOptions::new().with_max_points(3))).is_empty()
        );
    }

    /// The step doubles after at most two iterations, grows by a quarter after three, holds at four and halves beyond,
    /// never past `MAX_STEP`.
    #[test]
    fn steps_follow_the_iterations() {
        assert_eq!([0, 2, 3, 4, 5, 9].map(|i| adapted(0.0625, i)), [0.125, 0.125, 0.078125, 0.0625, 0.03125, 0.03125]);
        assert_eq!(adapted(0.2, 0), MAX_STEP);
    }

    /// The cubic Hermite interpolant is exact for a cubic: 1 + 2u − u² + u³/2 through −0.5 and 1 with its slopes, at
    /// either end and inside.
    #[test]
    fn hermite_reproduces_a_cubic() {
        let f = |u: f64| 1.0 + 2.0 * u - u * u + 0.5 * u * u * u;
        let df = |u: f64| 2.0 - 2.0 * u + 1.5 * u * u;
        let (a, b) = ((-0.5, f(-0.5), df(-0.5)), (1.0, f(1.0), df(1.0)));
        for u in [-0.5, -0.2, 0.3, 0.7, 1.0] {
            assert!((hermite(a, b, u) - f(u)).abs() <= 1e-14, "{u}: {} against {}", hermite(a, b, u), f(u));
        }
    }

    /// The trace's Jacobian is its equations' derivative by central differences in each unknown (ln K, ln T, ln p), at a
    /// point 0.05 off methane/ethane's bubble and dew points at 1 MPa in each ln K and 0.02 in ln T and ln p; its last
    /// row specifies the unknown it is asked to.
    #[test]
    fn the_jacobian_is_the_equations_derivative() {
        for side in [Side::Bubble, Side::Dew] {
            let (m, _, v0) = traced(side);
            let s = ok(saturation(&m, side, Given::P(1e6)));
            let (mut tracer, _) = ok(Tracer::from_point(&m, side, &s));
            let v: Vec<f64> = v0.iter().enumerate().map(|(i, x)| x + if i < 2 { 0.05 } else { 0.02 }).collect();
            let (_, at) = ok(tracer.equations(&v));
            let jacobian = tracer.jacobian(&v, &at, 1);
            for k in 0..4 {
                let fd = column(&mut tracer, &v, k);
                let scale = fd.iter().fold(1.0_f64, |m, c| m.max(c.abs()));
                let close = (0..3).all(|i| (jacobian[i][k] - fd[i]).abs() <= 1e-6 * scale);
                assert!(close && jacobian[3][k] == f64::from(u8::from(k == 1)), "{side:?} {k}: {jacobian:?} {fd:?}");
            }
        }
    }

    /// At a converged point the sensitivities solve the extended system, J (dX/dS) = e_{N+2}: the specified unknown's
    /// own is 1, and the N + 1 equations do not move to first order.
    #[test]
    fn sensitivities_solve_the_extended_system() {
        let (m, point, _) = traced(Side::Bubble);
        let s = ok(saturation(&m, Side::Bubble, Given::P(1e6)));
        let (tracer, _) = ok(Tracer::from_point(&m, Side::Bubble, &s));
        for spec in [0, 3] {
            let ds = tracer.sensitivity(&point, spec).unwrap_or_else(|| panic!("singular"));
            let jacobian = tracer.jacobian(&point.v, &point.at, spec);
            let applied: Vec<f64> = jacobian.iter().map(|row| row.iter().zip(&ds).map(|(j, d)| j * d).sum()).collect();
            let close = applied[..3].iter().all(|a| a.abs() <= 1e-12) && (applied[3] - 1.0).abs() <= 1e-12;
            assert!(close && (ds[spec] - 1.0).abs() <= 1e-12, "{spec}: {applied:?} {ds:?}");
        }
    }

    /// A trace starts at the bubble point at its starting pressure (methane/ethane at 1 bar), else at the dew point
    /// there (methane/n-butane at 75/25, whose bubble point at 1 bar lies below n-butane's triple point), else at the
    /// bubble point at the model's lowest temperature (CO₂/methane at 25/75, whose bubble and dew points at 1 bar lie
    /// below CO₂'s triple point).
    #[test]
    fn traces_start_where_they_can() {
        let opts = EnvelopeOptions::new();
        let first = |m: &Mixture| {
            let env = ok(m.envelope(&opts));
            (env.side[0], env.t[0], env.p[0])
        };
        let (side, _, p) = first(&library(&["Methane", "Ethane"], &[0.5, 0.5]));
        assert!(side == Side::Bubble && (p / 1e5 - 1.0).abs() <= 1e-12, "{side:?} {p}");
        let (side, _, p) = first(&library(&["Methane", "n-Butane"], &[0.75, 0.25]));
        assert!(side == Side::Dew && (p / 1e5 - 1.0).abs() <= 1e-12, "{side:?} {p}");
        let co2 = library(&["CarbonDioxide", "Methane"], &[0.25, 0.75]);
        let t_min = co2.range().t_min();
        let (side, t, p) = first(&co2);
        assert!(side == Side::Bubble && (t / t_min - 1.0).abs() <= 1e-12 && p > 1e5, "{side:?} {t} {p}");
    }

    /// A starting pressure that is not finite and positive, or a highest pressure not above it, is refused; a trace
    /// bounded at 3 MPa ends at that bound with every point below it, one of at most five points without convergence
    /// after five.
    #[test]
    fn options_bound_the_trace() {
        let m = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        for p in [f64::NAN, 0.0, -1.0, f64::INFINITY] {
            let refused = m.envelope(&EnvelopeOptions::new().with_start_pressure(p));
            assert!(matches!(refused, Err(Error::InvalidInput { quantity: "starting pressure", .. })), "{p}");
        }
        let low = m.envelope(&EnvelopeOptions::new().with_max_pressure(1e5));
        assert_eq!(low.err(), Some(Error::InvalidInput { quantity: "highest pressure", value: 1e5 }));
        let bounded = ok(m.envelope(&EnvelopeOptions::new().with_max_pressure(3e6)));
        let at_bound =
            matches!(bounded.end, EnvelopeEnd::Bound(DomainError::AboveMaxPressure { p_max, .. }) if p_max == 3e6);
        assert!(at_bound && bounded.p.iter().all(|p| *p <= 3e6) && bounded.critical.is_none(), "{:?}", bounded.end);
        let short = ok(m.envelope(&EnvelopeOptions::new().with_max_points(5)));
        let stopped = Error::NoConvergence { strategy: Strategy::Vle, iterations: 5 };
        assert!(
            short.len() == 5 && short.end == EnvelopeEnd::NoConvergence(stopped),
            "{} {:?}",
            short.len(),
            short.end
        );
    }
}
