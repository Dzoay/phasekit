//! Vapour-liquid equilibrium at (T, p) (PLAN.md M13.8): Wilson's K, Rachford–Rice, Michelsen's tangent-plane
//! stability test and the PT split.
//!
//! - The equilibrium conditions are the fugacities' equality, f_i = x_i φ_i p in each phase, with ln φ_i from M13.6's
//!   composition derivatives of the residual.
//! - Every composition at (T, p) is evaluated on its stable density root, the one of least Gibbs energy among the roots
//!   core's density solve finds (M13.1; ROT-181, ROT-183, ROT-186): no branch is forced.
//! - The stability test (Michelsen 1982a) minimises the tangent-plane distance from Wilson's two trial phases and one
//!   near-pure trial phase per component: successive substitution, then Newton's method in α_i = 2√W_i on the
//!   n ∂ln φ/∂n Hessian.
//! - The split (Michelsen 1982b) starts from the unstable trial phase: successive substitution on Rachford–Rice,
//!   accelerated by the dominant eigenvalue every fifth step, then Newton's method in the minority phase's mole numbers.
//!   It ends on a residual gate over the fugacity equations, never on the step.

use phasekit_core::density::{self, Branch};
use phasekit_core::{Constant, Derivs, Error, HelmholtzModel, Order, Reducing, Strategy, Virials, math};

use crate::{Composition, Mixture, MixtureModel, Var};

/// Successive substitutions of the tangent-plane test before its Newton stage, and that stage's steps.
const SUBSTITUTIONS: usize = 12;
const DESCENTS: usize = 30;

/// A split ends where every ln f_i^V − ln f_i^L is within this of 0.
pub(crate) const FUGACITY_GATE: f64 = 1e-12;

/// A tangent-plane distance below −this proves the feed unstable; a trial phase within √this of the feed (Σ (w_i −
/// z_i)²) is the trivial solution.
const TM_TOLERANCE: f64 = 1e-10;

/// A phase of a mixture's components at mole fractions `x`: what core's density solve evaluates.
#[derive(Debug)]
pub(crate) struct Phase<'a> {
    pub(crate) mixture: &'a Mixture,
    pub(crate) x: &'a [f64],
}

impl HelmholtzModel for Phase<'_> {
    fn gas_constant(&self) -> f64 {
        self.mixture.model().gas_constant()
    }

    fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs {
        self.mixture.model().residual_at(self.x, t, rho, order)
    }

    fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs {
        self.mixture.ideal_at(self.x, t, rho, order)
    }

    fn rho_max(&self, t: f64) -> f64 {
        self.mixture.rho_max_at(self.x, t)
    }

    fn zero_density(&self, t: f64) -> Option<Virials> {
        self.mixture.model().virials(self.x, t)
    }

    fn reducing(&self) -> Option<Reducing> {
        let (t, rho) = self.mixture.model().reducing_state(self.x);
        Some(Reducing { t, rho, p: None })
    }
}

/// A composition at (T, p) on its stable density root: the density and the composition derivatives there.
#[derive(Clone, Debug)]
pub(crate) struct AtTp {
    pub(crate) rho: f64,
    pub(crate) c: Composition,
    /// The pressure the phase is at, Pa, and RT, J/mol.
    pub(crate) p: f64,
    pub(crate) rt: f64,
}

impl AtTp {
    /// `x` at (T, ρ), a root at p, to `order`.
    pub(crate) fn new(mixture: &Mixture, t: f64, p: f64, rho: f64, x: &[f64], order: Order) -> Result<AtTp, Error> {
        let c = mixture.model().composition(t, rho, x, order)?;
        Ok(AtTp { rho, c, p, rt: mixture.model().gas_constant() * t })
    }

    /// ln φ_i = ∂Ψ/∂n_i − ln(p/(ρRT)) for every component, at the pressure the phase is at. At the root it is
    /// `Composition::ln_phi`'s, which takes Z from the model at ρ; but a dense phase's p(ρ) moves by K_T/p ulps of p per
    /// ulp of ρ (2.5e4 for a propane-rich liquid at 41 kPa), so the model's Z there leaves ln φ a floor of 1e-12 that
    /// p's own does not.
    pub(crate) fn ln_phi(&self) -> Vec<f64> {
        let ln_z = math::ln(self.p / (self.rho * self.rt));
        (0..self.c.components()).map(|i| self.c.psi(&[Var::N(i)]) - ln_z).collect()
    }

    /// n (∂ln φ_i/∂n_j)_(T,p), needs `Order::Two`.
    fn hessian(&self) -> Vec<Vec<f64>> {
        let n = self.c.components();
        (0..n).map(|i| (0..n).map(|j| self.c.n_dln_phi_dn(i, j)).collect()).collect()
    }
}

/// Mole fractions `x` (every one positive, as given: a caller with mole numbers normalises them, so a split's fractions
/// are the ones its fugacities were evaluated at) at (T, p) on their stable root, to `order`.
pub(crate) fn at_t_p(mixture: &Mixture, t: f64, p: f64, x: &[f64], order: Order) -> Result<AtTp, Error> {
    let phase = Phase { mixture, x };
    let rho = polished(&phase, t, p, stable_root(&phase, t, p)?);
    AtTp::new(mixture, t, p, rho, x, order)
}

/// `rho`, a root of p(T, ρ) = p, polished by up to three Newton steps to the rounding of p: core's solve accepts a root
/// that reproduces p to its gate, and a liquid's ln f_i moves with p by v̄_i/RT, which at low pressure leaves the
/// fugacity equations a floor of 1e-11 (Methane/n-Butane's butane-rich liquid at 181 K and 66.5 kPa). A step that
/// does not lower |p(ρ) − p| is not taken.
pub(crate) fn polished(model: &dyn HelmholtzModel, t: f64, p: f64, mut rho: f64) -> f64 {
    let rt = model.gas_constant() * t;
    let at = |rho: f64| {
        let d = model.residual(t, rho, Order::Two);
        let (a01, a02) = (d.get(0, 1).unwrap_or(f64::NAN), d.get(0, 2).unwrap_or(f64::NAN));
        (rho * rt * (1.0 + a01) - p, rt * (1.0 + 2.0 * a01 + a02))
    };
    let (mut miss, mut slope) = at(rho);
    for _ in 0..3 {
        let next = rho - miss / slope;
        let (next_miss, next_slope) = at(next);
        if next_miss.abs() >= miss.abs() || next_miss.is_nan() {
            break;
        }
        (rho, miss, slope) = (next, next_miss, next_slope);
    }
    rho
}

/// Points of the isotherm a root's branch is checked at.
const BRANCH_POINTS: usize = 32;

/// Whether p(T, ρ) rises with ρ, (∂p/∂ρ)_T > 0, at `BRANCH_POINTS` evenly spaced points from `from` (excluded) to `to`
/// (included): the branch a root `to` lies on, reached from `from`.
fn rising(model: &dyn HelmholtzModel, t: f64, from: f64, to: f64) -> bool {
    (1..=BRANCH_POINTS).all(|k| {
        let rho = from + (to - from) * k as f64 / BRANCH_POINTS as f64;
        let d = model.residual(t, rho, Order::Two);
        let (a01, a02) = (d.get(0, 1).unwrap_or(f64::NAN), d.get(0, 2).unwrap_or(f64::NAN));
        1.0 + 2.0 * a01 + a02 > 0.0
    })
}

/// The density at (T, p) on `branch`, from `seed` where one is known: core's solve (M13.1), and where it does not
/// converge, Newton's method in ln ρ from the same start that ends on core's acceptance gate (`density::reproduces`), not
/// on the step. Core's Newton stops on a step within 1e-14 in ln ρ, or on a residual within 1e-14 of ρRT and a step no
/// longer halving; an R22-rich liquid's p rounds at 2e-14 of ρRT, so its steps wander by 1e-14 at the root until the
/// iterations run out (R411B's incipient liquid at 258 K and 291 kPa). Core stays frozen to M13's close.
pub(crate) fn branch_root(
    model: &dyn HelmholtzModel,
    t: f64,
    p: f64,
    branch: Branch,
    seed: Option<f64>,
) -> Result<f64, Error> {
    density::solve(model, t, p, branch, seed).or_else(|error| {
        let far_end = far_end(model, t, p, branch);
        seed.and_then(|seed| gated(model, t, p, seed)).or_else(|| gated(model, t, p, far_end)).ok_or(error)
    })
}

/// Where a branch's solve starts without a seed, as core's: the model's largest density for the liquid, the ideal gas's
/// p/RT for the vapour.
fn far_end(model: &dyn HelmholtzModel, t: f64, p: f64, branch: Branch) -> f64 {
    if branch == Branch::Liquid { model.rho_max(t) } else { p / (model.gas_constant() * t) }
}

/// Newton iterations [`gated`] may spend, and its largest step in ln ρ: core's.
const GATED_ITERATIONS: usize = 50;
const GATED_STEP: f64 = 0.25;

/// Newton's method in ln ρ on p(T, ρ) − p from `start`, through mechanically stable states only, until the density
/// reproduces p by core's acceptance gate; `None` at an unstable or non-finite iterate, or out of iterations.
fn gated(model: &dyn HelmholtzModel, t: f64, p: f64, start: f64) -> Option<f64> {
    let (r, mut u) = (model.gas_constant(), math::ln(start));
    for _ in 0..GATED_ITERATIONS {
        let rho = math::exp(u);
        let b = (model.residual(t, rho, Order::Two) + Derivs::IDEAL_DELTA).bundle()?;
        let (pressure, slope) = (rho * r * t * b.a01, rho * r * t * (2.0 * b.a01 + b.a02));
        if !(pressure.is_finite() && slope > 0.0) {
            return None;
        }
        if density::reproduces(r, t, rho, p, &b) {
            return Some(rho);
        }
        u += ((p - pressure) / slope).clamp(-GATED_STEP, GATED_STEP);
    }
    None
}

/// Two roots closer than this in ln ρ are one, reached from both branches' ends: core's `density::roots`'.
const SAME_ROOT: f64 = 1e-12;

/// The distinct roots [`branch_root`] reaches from the vapour's and the liquid's far end, ascending: core's
/// `density::roots` on [`branch_root`].
fn roots(model: &dyn HelmholtzModel, t: f64, p: f64) -> Vec<f64> {
    let [vapour, liquid] = [Branch::Vapour, Branch::Liquid].map(|branch| branch_root(model, t, p, branch, None).ok());
    match (vapour, liquid) {
        (Some(v), Some(l)) if (math::ln(l) - math::ln(v)).abs() > SAME_ROOT => vec![v.min(l), v.max(l)],
        (Some(one), _) | (None, Some(one)) => vec![one],
        (None, None) => vec![],
    }
}

/// The stable density root at (T, p): of the roots [`branch_root`] finds on the vapour and the liquid branch, those
/// its branch reaches with the pressure rising throughout, and of those the one of least Gibbs energy (core's
/// `RootPolicy::Stable`), checked first. Where two roots are found, the dilute one must rise from the ideal gas
/// and the dense one from the model's largest density, dp/dρ > 0 at `BRANCH_POINTS` points between: a root on a loop of the
/// isotherm past a spinodal, where the multi-fluid equations are extrapolated, fails it and is dropped, however low its
/// Gibbs energy (#3283's ρ ≈ 6751 for C1/C2/C3 at 2 MPa; a butane-rich trial phase's ρ ≈ 4646 at 220 K and 8 MPa).
/// Where neither is on its branch, the one of least Gibbs energy.
pub(crate) fn stable_root(model: &dyn HelmholtzModel, t: f64, p: f64) -> Result<f64, Error> {
    let [dilute, dense] = match roots(model, t, p)[..] {
        [dilute, dense] => [dilute, dense],
        [one] => return Ok(one),
        _ => return Err(Error::NoConvergence { strategy: Strategy::DensityNewton, iterations: 0 }),
    };
    let rising = |from: f64, to: f64| rising(model, t, from, to);
    let gibbs = |rho: f64| {
        let d = model.ideal(t, rho, Order::One) + model.residual(t, rho, Order::One);
        d.get(0, 0).zip(d.get(0, 1)).map_or(f64::NAN, |(a00, a01)| a00 + a01)
    };
    // The lower-Gibbs root first; the other only where the first is not on its branch.
    let ends = [(dilute, 0.0), (dense, model.rho_max(t))];
    let [first, second] = if gibbs(dense) < gibbs(dilute) { [ends[1], ends[0]] } else { ends };
    if !rising(first.1, first.0) && rising(second.1, second.0) { Ok(second.0) } else { Ok(first.0) }
}

/// `x` divided by its sum.
pub(crate) fn normalised(x: &[f64]) -> Vec<f64> {
    let total: f64 = x.iter().sum();
    x.iter().map(|x| x / total).collect()
}

/// Wilson's ln K_i = ln(p_c,i/p) + a_i(T), a_i = 5.373 (1 + ω_i)(1 − T_c,i/T), from each component's published critical
/// point and acentric factor (0 where it has none, as GERG-2008's components).
pub(crate) fn wilson(mixture: &Mixture, t: f64, p: f64) -> Vec<f64> {
    wilson_parts(mixture, t).iter().map(|(p_c, a)| math::ln(p_c / p) + a).collect()
}

/// Each component's (p_c,i, a_i(T)) of Wilson's K; a component without a published critical point takes (1 MPa, 0),
/// a seed and no more.
pub(crate) fn wilson_parts(mixture: &Mixture, t: f64) -> Vec<(f64, f64)> {
    mixture
        .model()
        .fluids()
        .map(|f| {
            let omega = f.constant(Constant::Acentric).unwrap_or(0.0);
            f.info().published_critical().map_or((1e6, 0.0), |c| (c.p, 5.373 * (1.0 + omega) * (1.0 - c.t / t)))
        })
        .collect()
}

/// The vapour fraction β of `z` split with equilibrium ratios K_i = exp(`ln_k`_i): the root of
/// g(β) = Σ z_i (K_i − 1)/(1 + β(K_i − 1)) in (1/(1 − K_max), 1/(1 − K_min)), the interval where every phase's
/// fractions are positive, by Newton's method kept in its shrinking bracket (g falls monotonically there). A β outside
/// [0, 1] is returned: the negative flash. `None` when every K_i lies on one side of 1 (g has no root).
pub(crate) fn rachford_rice(z: &[f64], ln_k: &[f64]) -> Option<f64> {
    rachford_rice_steps(z, ln_k).map(|(b, _)| b)
}

/// [`rachford_rice`]'s root with the iterations its solve spent.
fn rachford_rice_steps(z: &[f64], ln_k: &[f64]) -> Option<(f64, usize)> {
    let k: Vec<f64> = ln_k.iter().map(|l| math::exp(*l)).collect();
    let (k_min, k_max) = k.iter().fold((f64::INFINITY, 0.0_f64), |(lo, hi), k| (lo.min(*k), hi.max(*k)));
    if !(k_max > 1.0 && k_min < 1.0) {
        return None;
    }
    let (mut lo, mut hi) = (1.0 / (1.0 - k_max), 1.0 / (1.0 - k_min));
    let mut b = 0.5;
    for step in 1..=RACHFORD_RICE_STEPS {
        let (g, dg) = rachford_rice_terms(z, &k, b);
        if g == 0.0 {
            return Some((b, step));
        }
        if g > 0.0 {
            lo = b;
        } else {
            hi = b;
        }
        let newton = b - g / dg;
        let next = if newton > lo && newton < hi { newton } else { 0.5 * (lo + hi) };
        if (next - b).abs() <= 1e-15 * b.abs().max(1.0) {
            return Some((next, step));
        }
        b = next;
    }
    Some((b, RACHFORD_RICE_STEPS))
}

/// Iterations Rachford–Rice's solve may spend.
const RACHFORD_RICE_STEPS: usize = 200;

/// Rachford–Rice's g(β) = Σ z_i (K_i − 1)/(1 + β(K_i − 1)) and its slope −Σ z_i (K_i − 1)²/(1 + β(K_i − 1))².
fn rachford_rice_terms(z: &[f64], k: &[f64], b: f64) -> (f64, f64) {
    let (mut g, mut dg) = (0.0, 0.0);
    for (z, k) in z.iter().zip(k) {
        let (km1, d) = (k - 1.0, 1.0 + b * (k - 1.0));
        (g, dg) = (g + z * km1 / d, dg - z * km1 * km1 / (d * d));
    }
    (g, dg)
}

/// The two phases of a split by its vapour fraction β and equilibrium ratios: x_i = z_i/(1 + β(K_i − 1)), y_i = K_i x_i.
fn phases(z: &[f64], ln_k: &[f64], beta: f64) -> (Vec<f64>, Vec<f64>) {
    z.iter()
        .zip(ln_k)
        .map(|(z, l)| {
            let k = math::exp(*l);
            let x = z / (1.0 + beta * (k - 1.0));
            (x, k * x)
        })
        .unzip()
}

/// The tangent-plane test's verdict on a feed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Stability {
    /// Every trial phase's distance is ≥ 0 (to `TM_TOLERANCE`), or trivial.
    Stable,
    /// A trial phase of negative distance: its mole fractions, the incipient phase's.
    Unstable(Vec<f64>),
}

/// Michelsen's tangent-plane test of `z` at (T, p), whose stable root is `feed`: tm(W) = 1 + Σ W_i (ln W_i + ln φ_i(W)
/// − d_i − 1), d_i = ln z_i + ln φ_i(z), minimised from Wilson's vapour-like (z_i K_i) and liquid-like (z_i/K_i) trial
/// phases and one near-pure trial phase per component. The most negative non-trivial minimum decides.
pub(crate) fn stability(mixture: &Mixture, t: f64, p: f64, z: &[f64], feed: &AtTp) -> Result<Stability, Error> {
    let d: Vec<f64> = z.iter().zip(feed.ln_phi()).map(|(z, l)| math::ln(*z) + l).collect();
    let tpd = Tpd { mixture, t, p, z, d: &d };
    // A trial whose composition has no stable root at (T, p) proves nothing either way.
    let minima = trials(z, &wilson(mixture, t, p)).into_iter().filter_map(|trial| tpd.minimise(trial).ok().flatten());
    Ok(most_unstable(minima).map_or(Stability::Stable, |w| Stability::Unstable(normalised(&w))))
}

/// The stability test's trial phases' mole numbers: Wilson's vapour-like z_i K_i and liquid-like z_i/K_i, then for each
/// component j one rich in it, 1 of j and 1e-3 z_i of every other.
fn trials(z: &[f64], ln_k: &[f64]) -> Vec<Vec<f64>> {
    let mut trials: Vec<Vec<f64>> = vec![
        z.iter().zip(ln_k).map(|(z, l)| z * math::exp(*l)).collect(),
        z.iter().zip(ln_k).map(|(z, l)| z / math::exp(*l)).collect(),
    ];
    for j in 0..z.len() {
        trials.push(z.iter().enumerate().map(|(i, z)| if i == j { 1.0 } else { 1e-3 * z }).collect());
    }
    trials
}

/// The minimiser of the most negative tangent-plane distance among `minima` (tm, W), where it lies below
/// −`TM_TOLERANCE`; `None` where none does (the feed is stable).
fn most_unstable(minima: impl Iterator<Item = (f64, Vec<f64>)>) -> Option<Vec<f64>> {
    minima.filter(|(tm, _)| *tm < -TM_TOLERANCE).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, w)| w)
}

/// The tangent-plane distance of `z` at (T, p): d_i = ln z_i + ln φ_i(z) on the feed's stable root.
struct Tpd<'a> {
    mixture: &'a Mixture,
    t: f64,
    p: f64,
    z: &'a [f64],
    d: &'a [f64],
}

impl Tpd<'_> {
    /// Trial mole numbers `w` at (T, p) on their stable root, to `order`.
    fn at(&self, w: &[f64], order: Order) -> Result<AtTp, Error> {
        at_t_p(self.mixture, self.t, self.p, &normalised(w), order)
    }

    /// Whether `w` is the feed: Σ (w_i/Σw − z_i)² under `TM_TOLERANCE`.
    fn trivial(&self, w: &[f64]) -> bool {
        normalised(w).iter().zip(self.z).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() < TM_TOLERANCE
    }

    /// tm(W) = 1 + Σ W_i (ln W_i + ln φ_i − d_i − 1).
    fn tm(&self, w: &[f64], ln_phi: &[f64]) -> f64 {
        1.0 + w.iter().zip(ln_phi).zip(self.d).map(|((w, l), d)| w * (math::ln(*w) + l - d - 1.0)).sum::<f64>()
    }

    /// The minimum from trial mole numbers `w`, with the minimiser; `None` for the trivial one.
    fn minimise(&self, w: Vec<f64>) -> Result<Option<(f64, Vec<f64>)>, Error> {
        let Some((w, _)) = self.substitute(w)? else { return Ok(None) };
        let (w, _) = self.descend(w)?;
        if self.trivial(&w) {
            return Ok(None);
        }
        let ln_phi = self.at(&w, Order::One)?.ln_phi();
        Ok(Some((self.tm(&w, &ln_phi), w)))
    }

    /// Up to `SUBSTITUTIONS` steps of successive substitution, ln W_i = d_i − ln φ_i(W), to 1e-10 in ln W, with the steps
    /// taken; `None` once trivial.
    fn substitute(&self, mut w: Vec<f64>) -> Result<Option<(Vec<f64>, usize)>, Error> {
        for step in 1..=SUBSTITUTIONS {
            let ln_phi = self.at(&w, Order::One)?.ln_phi();
            let next: Vec<f64> = self.d.iter().zip(&ln_phi).map(|(d, l)| math::exp(d - l)).collect();
            let change = next.iter().zip(&w).map(|(a, b)| (math::ln(*a) - math::ln(*b)).abs()).fold(0.0, f64::max);
            w = next;
            if self.trivial(&w) {
                return Ok(None);
            }
            if change < 1e-10 {
                return Ok(Some((w, step)));
            }
        }
        Ok(Some((w, SUBSTITUTIONS)))
    }

    /// Newton's method in α_i = 2√W_i on H_ij = δ_ij + √(W_i W_j) ∂ln φ_i/∂W_j, to 1e-12 in the gradient
    /// g_i = √W_i (ln W_i + ln φ_i − d_i), each step halved until tm falls; with the steps taken.
    fn descend(&self, mut w: Vec<f64>) -> Result<(Vec<f64>, usize), Error> {
        for step in 0..DESCENTS {
            let at = self.at(&w, Order::Two)?;
            let (ln_phi, phi_n) = (at.ln_phi(), at.hessian());
            let total: f64 = w.iter().sum();
            let g: Vec<f64> =
                (0..w.len()).map(|i| math::sqrt(w[i]) * (math::ln(w[i]) + ln_phi[i] - self.d[i])).collect();
            if g.iter().all(|g| g.abs() <= 1e-12) {
                return Ok((w, step));
            }
            let root = |i: usize, j: usize| math::sqrt(w[i] * w[j]);
            let h = square(w.len(), |i, j| if i == j { 1.0 } else { 0.0 } + root(i, j) * phi_n[i][j] / total);
            let Some(delta) = solve(h, g.iter().map(|g| -g).collect()) else { return Ok((w, step)) };
            let Some(next) = self.line_search(&w, &delta, self.tm(&w, &ln_phi)) else { return Ok((w, step)) };
            w = next;
        }
        Ok((w, DESCENTS))
    }

    /// W after the step in α, halved up to eight times until tm is no more than `before`; `None` if none is.
    fn line_search(&self, w: &[f64], step: &[f64], before: f64) -> Option<Vec<f64>> {
        let alpha: Vec<f64> = w.iter().map(|w| 2.0 * math::sqrt(*w)).collect();
        (0..8).map(|k| math::powi(0.5, k)).find_map(|scale| {
            let trial: Vec<f64> =
                alpha.iter().zip(step).map(|(a, s)| 0.25 * (a + scale * s) * (a + scale * s)).collect();
            let next = trial.iter().all(|w| *w > 0.0).then(|| self.at(&trial, Order::One).ok()).flatten()?;
            (self.tm(&trial, &next.ln_phi()) <= before).then_some(trial)
        })
    }
}

/// The n × n matrix of `f(i, j)`.
pub(crate) fn square(n: usize, f: impl Fn(usize, usize) -> f64) -> Vec<Vec<f64>> {
    (0..n).map(|i| (0..n).map(|j| f(i, j)).collect()).collect()
}

/// A two-phase split at (T, p).
#[derive(Clone, Debug)]
pub(crate) struct Split {
    /// The vapour's molar fraction of the whole.
    pub(crate) beta: f64,
    /// The liquid's and the vapour's mole fractions.
    pub(crate) x: Vec<f64>,
    pub(crate) y: Vec<f64>,
    /// Each phase on its stable root.
    pub(crate) liquid: AtTp,
    pub(crate) vapour: AtTp,
}

/// The PT split of `z` from equilibrium ratios `ln_k`: successive substitution on Rachford–Rice (GDEM-accelerated every
/// fifth step) to 1e-6 in ln K, then Newton's method in the minority phase's mole numbers, to the fugacity gate. The
/// substitution runs as a negative flash, β anywhere in Rachford–Rice's bracket: an iterate outside (0, 1) is no
/// verdict. `None` where the converged β lies outside (0, 1), Rachford–Rice has no root or the phases fall together:
/// a single phase.
pub(crate) fn split(mixture: &Mixture, t: f64, p: f64, z: &[f64], mut ln_k: Vec<f64>) -> Result<Option<Split>, Error> {
    let failed = |iterations| Error::NoConvergence { strategy: Strategy::Vle, iterations };
    let mut history: Vec<Vec<f64>> = Vec::new();
    for iteration in 0..200_u16 {
        let Some(beta) = rachford_rice(z, &ln_k) else { return Ok(None) };
        let (x, y) = phases(z, &ln_k, beta);
        let (x, y) = (normalised(&x), normalised(&y));
        let (liquid, vapour) = (at_t_p(mixture, t, p, &x, Order::One)?, at_t_p(mixture, t, p, &y, Order::One)?);
        let next: Vec<f64> = liquid.ln_phi().iter().zip(vapour.ln_phi()).map(|(l, v)| l - v).collect();
        let step: Vec<f64> = next.iter().zip(&ln_k).map(|(a, b)| a - b).collect();
        if collapsed(&next) {
            return Ok(None);
        }
        ln_k = next;
        if step.iter().all(|s| s.abs() < 1e-6) {
            let inside = rachford_rice(z, &ln_k).is_some_and(|b| b > 0.0 && b < 1.0);
            return if inside { newton(mixture, t, p, z, &ln_k).map(Some) } else { Ok(None) };
        }
        if let Some(last) = history.last()
            && iteration % 5 == 4
        {
            accelerate(&mut ln_k, &step, last);
        }
        history.push(step);
    }
    Err(failed(200))
}

/// Whether equilibrium ratios have collapsed onto the feed: Σ (ln K_i)² under `TM_TOLERANCE`.
fn collapsed(ln_k: &[f64]) -> bool {
    ln_k.iter().map(|l| l * l).sum::<f64>() < TM_TOLERANCE
}

/// β, x and y from the minority phase's amounts per mole of feed (the vapour's where `vapour_minor`), the majority's
/// z less them.
fn from_minority(z: &[f64], minor: &[f64], vapour_minor: bool) -> (f64, Vec<f64>, Vec<f64>) {
    let major: Vec<f64> = z.iter().zip(minor).map(|(z, m)| z - m).collect();
    let (v, l) = if vapour_minor { (minor, major.as_slice()) } else { (major.as_slice(), minor) };
    (v.iter().sum::<f64>(), normalised(l), normalised(v))
}

/// GDEM (Crowe and Nishio): ln K += λ/(1 − λ) Δ, λ the dominant eigenvalue of the last two steps, where 0 < λ < 1.
fn accelerate(ln_k: &mut [f64], step: &[f64], last: &[f64]) {
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
    let lambda = dot(step, last) / dot(last, last);
    if lambda > 0.0 && lambda < 1.0 {
        ln_k.iter_mut().zip(step).for_each(|(l, s)| *l += lambda / (1.0 - lambda) * s);
    }
}

/// Newton's method on F_i = ln f_i^V − ln f_i^L in the minority phase's mole numbers per mole of feed (v = β y where
/// β < 1/2, else l = (1 − β) x), the majority's z less them, so the smaller amounts are never a difference. In v the
/// Jacobian is J_ij = (δ_ij/y_i − 1 + Φ^V_ij)/β + (δ_ij/x_i − 1 + Φ^L_ij)/(1 − β), Φ = n ∂ln φ/∂n, and in l its
/// negative. Each step is shortened to keep every amount in (0, z_i). It ends where every |F_i| is within the gate.
fn newton(mixture: &Mixture, t: f64, p: f64, z: &[f64], ln_k: &[f64]) -> Result<Split, Error> {
    let failed = |iterations| Error::NoConvergence { strategy: Strategy::Vle, iterations };
    let beta = rachford_rice(z, ln_k).ok_or(failed(0))?;
    let (x, y) = phases(z, ln_k, beta);
    let vapour_minor = beta < 0.5;
    let mut minor: Vec<f64> =
        if vapour_minor { y.iter().map(|y| beta * y).collect() } else { x.iter().map(|x| (1.0 - beta) * x).collect() };
    for iteration in 0..40_u16 {
        let (beta, x, y) = from_minority(z, &minor, vapour_minor);
        let (liquid, vapour) = (at_t_p(mixture, t, p, &x, Order::Two)?, at_t_p(mixture, t, p, &y, Order::Two)?);
        let (phi_l, phi_v) = (liquid.ln_phi(), vapour.ln_phi());
        let f: Vec<f64> = (0..z.len()).map(|i| math::ln(y[i]) + phi_v[i] - math::ln(x[i]) - phi_l[i]).collect();
        if f.iter().all(|f| f.abs() <= FUGACITY_GATE) {
            return Ok(Split { beta, x, y, liquid, vapour });
        }
        let jacobian = split_jacobian(beta, (&x, &liquid.hessian()), (&y, &vapour.hessian()));
        let sign = if vapour_minor { -1.0 } else { 1.0 };
        let Some(step) = solve(jacobian, f.iter().map(|f| sign * f).collect()) else { return Err(failed(iteration)) };
        let scale = shortened(&minor, &step, z);
        minor.iter_mut().zip(&step).for_each(|(m, s)| *m += scale * s);
    }
    Err(failed(40))
}

/// The split's Jacobian in the vapour's amounts v per mole of feed, J_ij = (δ_ij/y_i − 1 + Φ^V_ij)/β +
/// (δ_ij/x_i − 1 + Φ^L_ij)/(1 − β), from each phase's fractions and Φ = n ∂ln φ/∂n.
fn split_jacobian(beta: f64, (x, hl): (&[f64], &[Vec<f64>]), (y, hv): (&[f64], &[Vec<f64>])) -> Vec<Vec<f64>> {
    square(x.len(), |i, j| {
        let delta = if i == j { 1.0 } else { 0.0 };
        (delta / y[i] - 1.0 + hv[i][j]) / beta + (delta / x[i] - 1.0 + hl[i][j]) / (1.0 - beta)
    })
}

/// The fraction of `step` that keeps every amount of `minor` in (0, z_i): 1, or 0.9 of the way to the nearest bound.
fn shortened(minor: &[f64], step: &[f64], z: &[f64]) -> f64 {
    let mut scale = 1.0_f64;
    for ((m, s), z) in minor.iter().zip(step).zip(z) {
        if m + s <= 0.0 {
            scale = scale.min(0.9 * m / -s);
        } else if m + s >= *z {
            scale = scale.min(0.9 * (z - m) / s);
        }
    }
    scale
}

/// The solution of a x = b by Gaussian elimination with partial pivoting; `None` if `a` is singular (a zero or non-finite
/// pivot leaves x not finite).
pub(crate) fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&r, &s| a[r][col].abs().total_cmp(&a[s][col].abs()))?;
        a.swap(pivot, col);
        b.swap(pivot, col);
        let (top, rest) = a.split_at_mut(col + 1);
        let pivot = &top[col];
        for (offset, row) in rest.iter_mut().enumerate() {
            let f = row[col] / pivot[col];
            row[col..].iter_mut().zip(&pivot[col..]).for_each(|(x, p)| *x -= f * p);
            b[col + 1 + offset] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|c| a[r][c] * x[c]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    x.iter().all(|x| x.is_finite()).then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// g(β) at a returned β is 0 to rounding, and β lies in the bracket where every phase fraction is positive.
    fn root_of(z: &[f64], ln_k: &[f64]) -> f64 {
        let b = rachford_rice(z, ln_k).unwrap_or(f64::NAN);
        let (x, y) = phases(z, ln_k, b);
        assert!(x.iter().chain(&y).all(|v| *v > 0.0), "{x:?} {y:?}");
        let g: f64 =
            z.iter().zip(ln_k).map(|(z, l)| z * (math::exp(*l) - 1.0) / (1.0 + b * (math::exp(*l) - 1.0))).sum();
        assert!(g.abs() <= 1e-15, "g({b}) = {g:e}");
        b
    }

    /// K = (2, 1/2) at z = (1/2, 1/2) splits at β = 1/2 exactly; a feed outside its envelope gives a β outside [0, 1]
    /// (the negative flash) on either side; K all above 1, or all below, has no root.
    #[test]
    fn rachford_rice_finds_the_bracketed_root() {
        let ln2 = math::ln(2.0);
        assert!((root_of(&[0.5, 0.5], &[ln2, -ln2]) - 0.5).abs() <= 1e-15);
        assert!(root_of(&[0.05, 0.95], &[ln2, -ln2]) < 0.0);
        assert!(root_of(&[0.95, 0.05], &[ln2, -ln2]) > 1.0);
        let b = root_of(&[0.2, 0.3, 0.5], &[math::ln(5.0), math::ln(0.9), math::ln(0.05)]);
        assert!(b > 0.0 && b < 1.0, "{b}");
        assert_eq!(rachford_rice(&[0.5, 0.5], &[0.1, 0.2]), None);
        assert_eq!(rachford_rice(&[0.5, 0.5], &[-0.1, -0.2]), None);
        assert_eq!(rachford_rice(&[0.5, 0.5], &[0.0, -0.2]), None);
    }

    /// GDEM's dominant eigenvalue of the last two steps, λ = Δ·Δ′/Δ′·Δ′, extrapolates ln K by λ/(1 − λ) Δ where
    /// 0 < λ < 1: Δ = (1/4, 1/2) after Δ′ = (1, 1/2) gives λ = 0.4, so ln K moves by 2/3 Δ; at λ = 1 and λ < 0 it does not
    /// move.
    #[test]
    fn gdem_extrapolates_by_the_dominant_eigenvalue() {
        let (step, ln_k) = ([0.25, 0.5], [1.0, -1.0]);
        let mut moved = ln_k;
        accelerate(&mut moved, &step, &[1.0, 0.5]);
        assert!(
            (moved[0] - (1.0 + 0.25 * 2.0 / 3.0)).abs() <= 1e-15
                && (moved[1] - (-1.0 + 0.5 * 2.0 / 3.0)).abs() <= 1e-15
        );
        for last in [[0.25, 0.5], [-0.5, -1.0]] {
            let mut kept = ln_k;
            accelerate(&mut kept, &step, &last);
            assert_eq!(kept, ln_k, "{last:?}");
        }
    }

    /// A step is shortened to 0.9 of the way to the nearest bound it would reach or pass, 0 or z_i, and kept whole
    /// otherwise.
    #[test]
    fn steps_keep_every_amount_inside_its_bounds() {
        let (minor, z) = ([0.25, 0.25], [0.5, 0.5]);
        assert_eq!(shortened(&minor, &[0.1, -0.1], &z), 1.0);
        assert_eq!(shortened(&minor, &[-0.25, 0.0], &z), 0.9);
        assert_eq!(shortened(&minor, &[0.0, 0.25], &z), 0.9);
        assert_eq!(shortened(&minor, &[-0.5, 0.0], &z), 0.45);
        assert_eq!(shortened(&minor, &[-0.5, 1.0], &z), 0.9 * 0.25);
    }

    /// The minority phase's amounts give β, x and y: the vapour's at z = (1/2, 1/2), v = (1/8, 3/8), β = 1/2,
    /// x = (3/4, 1/4) and y = (1/4, 3/4); the same amounts as the liquid's the other way round.
    #[test]
    fn the_minority_phase_gives_the_split() {
        let (z, minor) = ([0.5, 0.5], [0.125, 0.375]);
        assert_eq!(from_minority(&z, &minor, true), (0.5, vec![0.75, 0.25], vec![0.25, 0.75]));
        assert_eq!(from_minority(&z, &minor, false), (0.5, vec![0.25, 0.75], vec![0.75, 0.25]));
    }

    /// The split's Jacobian is ∂F_i/∂v_j of F_i = ln f_i^V − ln f_i^L in the vapour's amounts (Methane/Ethane at 50/50,
    /// 180 K, 1 MPa, off the split by 2 %): against central differences of step 1e-7, within 1e-6 of each column's
    /// largest entry.
    #[test]
    fn the_split_jacobian_is_the_fugacities_derivative() {
        let registry =
            phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Parity).unwrap_or_else(|e| panic!("{e:?}"));
        let model = crate::MultiFluid::new(&registry, &["Methane", "Ethane"], phasekit_core::DataSet::Parity)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let mixture = Mixture::new(model, &[0.5, 0.5]).unwrap_or_else(|e| panic!("{e:?}"));
        let (t, p, z) = (180.0, 1e6, [0.5, 0.5]);
        let f = |v: &[f64]| {
            let (_, x, y) = from_minority(&z, v, true);
            let at = |w: &[f64]| at_t_p(&mixture, t, p, w, Order::Two).unwrap_or_else(|e| panic!("{e:?}"));
            let (liquid, vapour) = (at(&x), at(&y));
            let (phi_l, phi_v) = (liquid.ln_phi(), vapour.ln_phi());
            let f: Vec<f64> = (0..2).map(|i| math::ln(y[i]) + phi_v[i] - math::ln(x[i]) - phi_l[i]).collect();
            (f, x, y, liquid, vapour)
        };
        let Ok(Some(s)) = split(&mixture, t, p, &z, wilson(&mixture, t, p)) else { panic!("no split") };
        let v: Vec<f64> = s.y.iter().map(|y| 1.02 * s.beta * y).collect();
        let (_, x, y, liquid, vapour) = f(&v);
        let beta: f64 = v.iter().sum();
        let jacobian = split_jacobian(beta, (&x, &liquid.hessian()), (&y, &vapour.hessian()));
        let h = 1e-7;
        for j in 0..2 {
            let moved = |d: f64| {
                let mut v = v.clone();
                v[j] += d;
                f(&v).0
            };
            let (up, down) = (moved(h), moved(-h));
            let column: Vec<f64> = up.iter().zip(&down).map(|(u, d)| (u - d) / (2.0 * h)).collect();
            let scale = column.iter().fold(1.0_f64, |m, c| m.max(c.abs()));
            for (i, fd) in column.iter().enumerate() {
                assert!((jacobian[i][j] - fd).abs() <= 1e-6 * scale, "({i}, {j}): {} against {fd}", jacobian[i][j]);
            }
        }
    }

    /// A mixture of the library's fluids at mole fractions `x`, on `Parity`.
    fn library(components: &[&str], x: &[f64]) -> Mixture {
        let registry =
            phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Parity).unwrap_or_else(|e| panic!("{e:?}"));
        let model = crate::MultiFluid::new(&registry, components, phasekit_core::DataSet::Parity)
            .unwrap_or_else(|e| panic!("{e:?}"));
        Mixture::new(model, x).unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// p(T, ρ) − p of `model` and (∂p/∂ln ρ)_T.
    fn miss(model: &dyn HelmholtzModel, t: f64, p: f64, rho: f64) -> (f64, f64) {
        let d = model.residual(t, rho, Order::Two);
        let (a01, a02) = (d.get(0, 1).unwrap_or(f64::NAN), d.get(0, 2).unwrap_or(f64::NAN));
        let rt = model.gas_constant() * t;
        (rho * rt * (1.0 + a01) - p, rho * rt * (1.0 + 2.0 * a01 + a02))
    }

    /// A phase is the mixture's model at its own fractions: its reducing state and its virials are the model's at x.
    #[test]
    fn a_phase_is_the_model_at_its_fractions() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let x = [0.3, 0.7];
        let phase = Phase { mixture: &mixture, x: &x };
        let (t, rho) = mixture.model().reducing_state(&x);
        assert_eq!(phase.reducing().map(|r| (r.t, r.rho)), Some((t, rho)));
        let virials = phase.zero_density(250.0);
        assert!(virials.is_some() && virials == mixture.model().virials(&x, 250.0));
    }

    /// At a root of p(T, ρ) = p, ln φ from the pressure the phase is at is `Composition::ln_phi`'s, which takes Z from
    /// the model at ρ: library Methane/Ethane's 30/70 liquid and 90/10 vapour at 200 K and 1.5 MPa.
    #[test]
    fn ln_phi_at_the_phase_s_pressure_is_the_model_s() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        for x in [[0.3, 0.7], [0.9, 0.1]] {
            let phase = at_t_p(&mixture, 200.0, 1.5e6, &x, Order::One).unwrap_or_else(|e| panic!("{e:?}"));
            for (i, ln_phi) in phase.ln_phi().into_iter().enumerate() {
                let model = phase.c.ln_phi(i);
                assert!((ln_phi - model).abs() < 1e-10, "{x:?}: {i}: {ln_phi} {model}");
            }
        }
    }

    /// A root is polished to the rounding of p: from 1e-7 off Methane/Ethane's 50/50 liquid at 180 K and 1 MPa, p(T, ρ)
    /// comes within 1e-15 of (∂p/∂ln ρ)_T of p. A Newton step that would leave |p(T, ρ) − p| larger is not taken: just
    /// above the liquid's spinodal, where the slope is small, the first step overshoots, and the density is kept.
    #[test]
    fn roots_are_polished_by_steps_that_lower_the_miss() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let phase = Phase { mixture: &mixture, x: &[0.5, 0.5] };
        let (t, p) = (180.0, 1e6);
        let root = stable_root(&phase, t, p).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(polished(&phase, t, p, f64::NAN).to_bits(), f64::NAN.to_bits());
        let polished_root = polished(&phase, t, p, root * (1.0 + 1e-7));
        let (m, slope) = miss(&phase, t, p, polished_root);
        assert!(m.abs() <= 1e-15 * slope && root > 15_000.0, "{root}: {m} of {slope}");
        let spinodal =
            (1..400).map(|k| root * (1.0 - 0.0025 * f64::from(k))).take_while(|rho| miss(&phase, t, p, *rho).1 > 0.0);
        let near = spinodal.last().unwrap_or(f64::NAN);
        let (m, slope) = miss(&phase, t, p, near);
        assert!(miss(&phase, t, p, near * (1.0 - m / slope)).0.abs() > m.abs(), "the step from {near} overshoots");
        assert_eq!(polished(&phase, t, p, near), near);
    }

    /// The dense root rises from the model's largest density and the dilute one does not rise from 0: a butane-rich
    /// Methane/n-Butane phase (0.2, 0.8) at 220 K and 8 MPa, whose dilute root, about 4472 mol/m³, lies on a loop of the
    /// isotherm past a spinodal.
    #[test]
    fn a_root_past_a_spinodal_does_not_rise_from_its_branch() {
        let mixture = library(&["Methane", "n-Butane"], &[0.2, 0.8]);
        let (t, p) = (220.0, 8e6);
        let roots = density::roots(&mixture, t, p);
        let &[dilute, dense] = roots.as_slice() else { panic!("{roots:?}") };
        assert!((dilute - 4472.0).abs() < 1.0 && !rising(&mixture, t, 0.0, dilute), "{dilute}");
        assert!(rising(&mixture, t, mixture.rho_max(t), dense) && dense > 12_000.0, "{dense}");
        assert_eq!(stable_root(&mixture, t, p).ok(), Some(dense));
        // A gas root rises from 0 (Methane/Ethane at 50/50, 300 K and 1 MPa); the stretch between a 2-phase isotherm's
        // spinodals (180 K, from 4000 to 8000 mol/m³) does not.
        let gas = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let root = stable_root(&gas, 300.0, 1e6).unwrap_or(f64::NAN);
        assert!(root < 1_000.0 && rising(&gas, 300.0, 0.0, root), "{root}");
        assert!(!rising(&gas, 180.0, 4_000.0, 8_000.0));
        assert!(miss(&gas, 180.0, 1e6, 6_000.0).1 < 0.0);
        // The slope is 1 + 2 A01 + A02 itself: at 180 K it is −2.98 at 2000 mol/m³ (A01 −1.130, A02 −1.724) and 51.4 at
        // 10400 (A01 43.96, A02 −37.53).
        assert!(!rising(&gas, 180.0, 1_999.9, 2_000.0) && rising(&gas, 180.0, 10_399.9, 10_400.0));
    }

    /// Rachford–Rice needs K on both sides of 1 (K = (2, 1) has no root), and its Newton steps, kept in the bracket where
    /// every fraction is positive, converge in six for two splits; negative flashes near the bracket's ends, where
    /// Newton's steps leave it, take 24 (β −0.22) and 52 (β 1.04) with the bisections; one far outside, β 6.554, ends in
    /// 9 on a step within 1e-15 of |β|.
    #[test]
    fn rachford_rice_converges_in_a_few_newton_steps() {
        let ln2 = math::ln(2.0);
        assert_eq!(rachford_rice(&[0.5, 0.5], &[ln2, 0.0]), None);
        let ln_k = [math::ln(5.0), math::ln(0.9), math::ln(0.05)];
        let cases: [(&[f64], usize); 4] =
            [(&[0.2, 0.3, 0.5], 6), (&[0.5, 0.3, 0.2], 6), (&[0.02, 0.08, 0.9], 24), (&[0.95, 0.04, 0.01], 52)];
        let (far, steps) = rachford_rice_steps(&[0.336, 0.664], &[math::ln(5.0), math::ln(0.95)]).unwrap_or((0.0, 0));
        assert!(steps == 9 && (far - 6.554).abs() < 1e-9, "{far} after {steps}");
        for (z, expected) in cases {
            let (b, steps) = rachford_rice_steps(z, &ln_k).unwrap_or((f64::NAN, 0));
            assert_eq!(steps, expected, "{z:?}: {b}");
            let k: Vec<f64> = ln_k.iter().map(|l| math::exp(*l)).collect();
            let (g, dg) = rachford_rice_terms(z, &k, b);
            assert!(
                (g / dg).abs() <= 1e-15 && (1.0 / (1.0 - 5.0)..1.0 / (1.0 - 0.05)).contains(&b),
                "{z:?}: g({b}) = {g}"
            );
        }
    }

    /// The trial phases: Wilson's vapour-like z_i K_i and liquid-like z_i/K_i, then one rich in each component, 1 of it and
    /// 1e-3 z_i of every other.
    #[test]
    fn the_trial_phases_are_wilsons_and_one_rich_in_each_component() {
        let (z, ln_k) = ([0.25, 0.75], [math::ln(2.0), math::ln(0.5)]);
        let trials = trials(&z, &ln_k);
        let expected = [[0.5, 0.375], [0.125, 1.5], [1.0, 1e-3 * 0.75], [1e-3 * 0.25, 1.0]];
        assert_eq!(trials.len(), 4);
        for (got, want) in trials.iter().zip(expected) {
            assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() <= 1e-15), "{got:?} against {want:?}");
        }
    }

    /// The most negative distance decides, only below −1e-10: of (1e-3, −0.5, −0.2, −5e-11) the −0.5 trial's; of (0,
    /// −5e-11) none; a lone −1e-9 is unstable.
    #[test]
    fn the_most_negative_distance_decides() {
        let minima = |tms: &[f64]| tms.iter().enumerate().map(|(k, tm)| (*tm, vec![k as f64])).collect::<Vec<_>>();
        assert_eq!(most_unstable(minima(&[1e-3, -0.5, -0.2, -5e-11]).into_iter()), Some(vec![1.0]));
        assert_eq!(most_unstable(minima(&[0.0, -5e-11]).into_iter()), None);
        assert_eq!(most_unstable(minima(&[-1e-9]).into_iter()), Some(vec![0.0]));
    }

    /// The tangent-plane distance of Methane/Ethane at 50/50, inside the dome.
    fn tangent_plane(mixture: &Mixture, t: f64, p: f64) -> (Vec<f64>, AtTp) {
        let z = mixture.composition().to_vec();
        let feed = at_t_p(mixture, t, p, &z, Order::One).unwrap_or_else(|e| panic!("{e:?}"));
        (z.iter().zip(feed.ln_phi()).map(|(z, l)| math::ln(*z) + l).collect(), feed)
    }

    /// √W_i (ln W_i + ln φ_i(W) − d_i), the distance's gradient in α_i = 2√W_i.
    fn gradient(tpd: &Tpd<'_>, w: &[f64]) -> Vec<f64> {
        let ln_phi = tpd.at(w, Order::One).unwrap_or_else(|e| panic!("{e:?}")).ln_phi();
        (0..w.len()).map(|i| math::sqrt(w[i]) * (math::ln(w[i]) + ln_phi[i] - tpd.d[i])).collect()
    }

    /// Methane/Ethane at 50/50: `Tpd::trivial` within 1e-10 of the feed in Σ Δx², for an even feed and a (0.3, 0.7) one;
    /// the feed's distance from itself 0. At 180 K and 1 MPa substitution from
    /// Wilson's vapour-like trial stops on its change, after 6 of its 12 steps, at a fixed point. At 258 K and 6 MPa, near
    /// the critical point, it runs all 12 and stops short; Newton's stage then reaches the stationary point in 2 steps,
    /// its gradient within 1e-12, at a negative distance.
    #[test]
    fn the_tangent_plane_distance_is_minimised_to_its_stationary_point() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let z = [0.5, 0.5];
        let (d, _) = tangent_plane(&mixture, 180.0, 1e6);
        let tpd = Tpd { mixture: &mixture, t: 180.0, p: 1e6, z: &z, d: &d };
        assert!(
            tpd.trivial(&[1.0, 1.0])
                && tpd.trivial(&[0.5 + 5e-6, 0.5 - 5e-6])
                && !tpd.trivial(&[0.5 + 1e-4, 0.5 - 1e-4])
        );
        let feed_phi = at_t_p(&mixture, 180.0, 1e6, &z, Order::One).map(|a| a.ln_phi()).unwrap_or_default();
        assert!(tpd.tm(&z, &feed_phi).abs() <= 1e-15, "the feed's distance from itself is 0");
        let uneven = [0.3, 0.7];
        let other = Tpd { mixture: &mixture, t: 180.0, p: 1e6, z: &uneven, d: &d };
        assert!(other.trivial(&[0.3 + 5e-6, 0.7 - 5e-6]) && !other.trivial(&[0.3 + 1e-4, 0.7 - 1e-4]));
        let trial = trials(&z, &wilson(&mixture, 180.0, 1e6)).swap_remove(0);
        let (w, steps) = tpd.substitute(trial).unwrap_or_else(|e| panic!("{e:?}")).unwrap_or_else(|| panic!("trivial"));
        assert!(steps == 6 && gradient(&tpd, &w).iter().all(|g| g.abs() <= 1e-12), "{steps}: {:?}", gradient(&tpd, &w));
        let (d, _) = tangent_plane(&mixture, 258.0, 6e6);
        let tpd = Tpd { mixture: &mixture, t: 258.0, p: 6e6, z: &z, d: &d };
        let trial = trials(&z, &wilson(&mixture, 258.0, 6e6)).swap_remove(0);
        let (w, steps) =
            tpd.substitute(trial.clone()).unwrap_or_else(|e| panic!("{e:?}")).unwrap_or_else(|| panic!("trivial"));
        assert!(
            steps == SUBSTITUTIONS && gradient(&tpd, &w).iter().any(|g| g.abs() > 1e-9),
            "{:?}",
            gradient(&tpd, &w)
        );
        let (_, steps) = tpd.descend(w.clone()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(steps, 2, "Newton's stage from the substitution's point");
        let (tm, minimum) =
            tpd.minimise(trial).unwrap_or_else(|e| panic!("{e:?}")).unwrap_or_else(|| panic!("trivial"));
        let g = gradient(&tpd, &minimum);
        assert!(tm < -1e-3 && g.iter().all(|g| g.abs() <= 1e-12), "{tm}: {g:?}");
    }

    /// A line search from Wilson's vapour-like trial at 258 K and 6 MPa keeps a short step against the gradient whole,
    /// halves a long one until tm falls, and refuses a step along the gradient.
    #[test]
    fn line_searches_halve_until_the_distance_falls() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let z = [0.5, 0.5];
        let (d, _) = tangent_plane(&mixture, 258.0, 6e6);
        let tpd = Tpd { mixture: &mixture, t: 258.0, p: 6e6, z: &z, d: &d };
        let w = trials(&z, &wilson(&mixture, 258.0, 6e6)).swap_remove(0);
        let before = tpd.tm(&w, &tpd.at(&w, Order::One).unwrap_or_else(|e| panic!("{e:?}")).ln_phi());
        let g = gradient(&tpd, &w);
        let alpha: Vec<f64> = w.iter().map(|w| 2.0 * math::sqrt(*w)).collect();
        let at = |scale: f64, step: &[f64]| -> Vec<f64> {
            alpha.iter().zip(step).map(|(a, s)| 0.25 * (a + scale * s) * (a + scale * s)).collect()
        };
        let down: Vec<f64> = g.iter().map(|g| -1e-3 * g).collect();
        assert_eq!(tpd.line_search(&w, &down, before), Some(at(1.0, &down)));
        let long: Vec<f64> = g.iter().map(|g| -10.0 * g).collect();
        let halved = tpd.line_search(&w, &long, before).unwrap_or_else(|| panic!("no step"));
        let k = (1..8).find(|k| at(math::powi(0.5, *k), &long) == halved);
        let after = tpd.tm(&halved, &tpd.at(&halved, Order::One).unwrap_or_else(|e| panic!("{e:?}")).ln_phi());
        assert!(k.is_some() && after <= before, "{halved:?}: {after} after {before}");
        let up: Vec<f64> = g.iter().map(|g| 1e-3 * g).collect();
        assert_eq!(tpd.line_search(&w, &up, before), None);
    }

    /// K has collapsed onto the feed where Σ (ln K_i)² is under 1e-10: (7e-6, 7e-6) has, (1e-5, 1e-5) and (1e-5, 0) have
    /// not. A split from K within 1e-7 of 1 at a stable state is no split (Methane/Ethane at 50/50, 200 K and 4 MPa, a
    /// compressed liquid).
    #[test]
    fn a_split_from_the_trivial_k_is_no_split() {
        assert!(collapsed(&[7e-6, 7e-6]) && !collapsed(&[1e-5, 1e-5]) && collapsed(&[-7e-6, 0.0]));
        assert!(!collapsed(&[1.1e-5, 0.0]) && !collapsed(&[0.0, -1.1e-5]));
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let got = split(&mixture, 200.0, 4e6, &[0.5, 0.5], vec![1e-7, -1e-7]);
        assert!(matches!(got, Ok(None)), "{:?}", got.map(|s| s.map(|s| s.beta)));
    }

    /// A split from K that collapses onto the feed is no split (Methane/Ethane at 50/50, 300 K and 5 MPa, above its
    /// critical temperature, from Wilson's K, which straddle 1), nor is one whose converged β lies outside (0, 1) (at
    /// 200 K and 2.8 MPa, just above the bubble pressure).
    #[test]
    fn a_split_needs_two_phases_inside_its_bracket() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let z = [0.5, 0.5];
        for (t, p) in [(300.0, 5e6), (200.0, 2.8e6)] {
            let k = wilson(&mixture, t, p);
            assert!(k[0] > 0.0 && k[1] < 0.0, "{k:?}");
            let got = split(&mixture, t, p, &z, k);
            assert!(matches!(got, Ok(None)), "{t} K: {:?}", got.map(|s| s.map(|s| s.beta)));
        }
    }

    /// The split's Newton stage works in the minority phase's amounts, so a split at the edge of the dome keeps its
    /// precision: Methane/Ethane at 50/50 and 200 K, its pressure bisected 30 times between a split and a single phase
    /// toward the bubble point and toward the dew point, splits with β within 1e-9 of 0 and of 1 whose fugacities are
    /// equal within the gate and whose phases add up to the feed. A minority of 2e-10 of the feed, where the stability
    /// test stops seeing the split, taken as the feed less the majority would keep 1e-7 of its precision.
    #[test]
    fn splits_at_the_dome_s_edges_keep_their_precision() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let (t, z) = (200.0, [0.5, 0.5]);
        let split_at = |p: f64| match mixture.equilibrium(t, p) {
            Ok(crate::Equilibrium::Split { beta, x, y, .. }) => Some((beta, x, y)),
            _ => None,
        };
        for (inside, outside) in [(2e6, 3e6), (1e6, 2e5)] {
            let (mut inside, mut outside) = (math::ln(inside), math::ln(outside));
            for _ in 0..30 {
                let middle = (inside + outside) / 2.0;
                if split_at(math::exp(middle)).is_some() {
                    inside = middle;
                } else {
                    outside = middle;
                }
            }
            let p = math::exp(inside);
            let (beta, x, y) = split_at(p).unwrap_or_else(|| panic!("no split at {p}"));
            assert!(beta.min(1.0 - beta) < 1e-9, "{p}: {beta}");
            let (liquid, vapour) = (at_t_p(&mixture, t, p, &x, Order::One), at_t_p(&mixture, t, p, &y, Order::One));
            let (liquid, vapour) =
                (liquid.unwrap_or_else(|e| panic!("{e:?}")), vapour.unwrap_or_else(|e| panic!("{e:?}")));
            let (phi_l, phi_v) = (liquid.ln_phi(), vapour.ln_phi());
            for i in 0..2 {
                let f = math::ln(y[i]) + phi_v[i] - math::ln(x[i]) - phi_l[i];
                let total = (1.0 - beta) * x[i] + beta * y[i];
                assert!(f.abs() <= FUGACITY_GATE && (total - z[i]).abs() <= 1e-15, "{p}: {i}: {f} {total}");
            }
        }
    }

    /// p(T, ρ) of `model` and (∂p/∂ρ)_T.
    fn pressure(model: &dyn HelmholtzModel, t: f64, rho: f64) -> (f64, f64) {
        let d = model.residual(t, rho, Order::Two);
        let (a01, a02) = (d.get(0, 1).unwrap_or(f64::NAN), d.get(0, 2).unwrap_or(f64::NAN));
        let rt = model.gas_constant() * t;
        (rho * rt * (1.0 + a01), rt * (1.0 + 2.0 * a01 + a02))
    }

    /// R411B's incipient liquid at its dew point, 258.36 K and 290.9 kPa: core's Newton reaches the root and then wanders
    /// by 1e-14 in ln ρ, its p rounding at 2e-14 of ρRT, until its iterations run out. `branch_root` ends there on the
    /// acceptance gate, from the model's largest density or from a seed; the vapour's branch is core's.
    #[test]
    fn a_branch_ends_on_the_acceptance_gate_where_core_s_newton_wanders() {
        let blend = crate::generated::PREDEFINED.iter().find(|m| m.name == "R411B").unwrap_or_else(|| panic!("R411B"));
        let mixture = library(blend.components, blend.mole_fractions);
        let x = normalised(&[0.047_147_088_441_823_304, 0.880_369_508_739_880_1, 0.072_483_402_818_296_75]);
        let phase = Phase { mixture: &mixture, x: &x };
        let (t, p) = (258.356_778_119_287_8, 290_942.622_783_934_23);
        assert!(density::solve(&phase, t, p, Branch::Liquid, None).is_err());
        for seed in [None, Some(15_000.0)] {
            let rho = branch_root(&phase, t, p, Branch::Liquid, seed).unwrap_or_else(|e| panic!("{e:?}"));
            let (got, slope) = pressure(&phase, t, rho);
            assert!((got - p).abs() <= 1e-12 * rho * slope && (rho - 15_248.134).abs() < 1e-3, "{seed:?}: {rho}");
        }
        let vapour = branch_root(&phase, t, p, Branch::Vapour, None);
        assert_eq!(vapour, density::solve(&phase, t, p, Branch::Vapour, None));
    }

    /// The gated Newton refuses a start where (∂p/∂ρ)_T ≤ 0, inside library Methane/Ethane's spinodals at 50/50, 200 K and
    /// 1.5 MPa; a branch starts from the model's largest density or p/RT.
    #[test]
    fn the_gated_newton_starts_on_a_stable_state() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let (t, p) = (200.0, 1.5e6);
        let unstable = (1..100).map(|k| 200.0 * f64::from(k)).find(|rho| pressure(&mixture, t, *rho).1 < 0.0);
        let unstable = unstable.unwrap_or_else(|| panic!("no unstable density"));
        assert_eq!(gated(&mixture, t, p, unstable), None);
        assert_eq!(far_end(&mixture, t, p, Branch::Liquid), mixture.rho_max(t));
        assert_eq!(far_end(&mixture, t, p, Branch::Vapour), p / (mixture.gas_constant() * t));
    }

    /// A model that counts its residual evaluations.
    #[derive(Debug)]
    struct Counting<'a> {
        model: &'a dyn HelmholtzModel,
        #[allow(clippy::disallowed_types)] // the test's own counter; HelmholtzModel is Sync
        residuals: std::sync::atomic::AtomicUsize,
    }

    impl<'a> Counting<'a> {
        #[allow(clippy::disallowed_types)] // the test's own counter
        fn new(model: &'a dyn HelmholtzModel) -> Counting<'a> {
            Counting { model, residuals: std::sync::atomic::AtomicUsize::new(0) }
        }

        fn evaluations(&self) -> usize {
            self.residuals.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    impl HelmholtzModel for Counting<'_> {
        fn gas_constant(&self) -> f64 {
            self.model.gas_constant()
        }

        fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs {
            self.residuals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.model.residual(t, rho, order)
        }

        fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs {
            self.model.ideal(t, rho, order)
        }

        fn rho_max(&self, t: f64) -> f64 {
            self.model.rho_max(t)
        }
    }

    /// The gated Newton's slope is (∂p/∂ln ρ)_T, so it converges quadratically: from the ideal gas's p/RT to library
    /// Methane/Ethane's 50/50 vapour at 250 K and 2 MPa, core's root, in a few evaluations of the model.
    #[test]
    fn the_gated_newton_converges_in_a_few_steps() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let counting = Counting::new(&mixture);
        let (t, p) = (250.0, 2e6);
        let rho = gated(&counting, t, p, far_end(&mixture, t, p, Branch::Vapour)).unwrap_or(f64::NAN);
        let core = density::solve(&mixture, t, p, Branch::Vapour, None).unwrap_or_else(|e| panic!("{e:?}"));
        let evaluations = counting.evaluations();
        assert!((rho / core - 1.0).abs() < 1e-12 && evaluations <= 6, "{rho} {core}: {evaluations}");
    }

    /// Two roots where both branches reach their own (Methane/Ethane at 50/50, 200 K, 1.5 MPa, inside the dome), ascending;
    /// one where both reach the same, above the critical temperature (300 K, 5 MPa); the stable one of either.
    #[test]
    fn roots_are_distinct_and_ascending() {
        let mixture = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let two = roots(&mixture, 200.0, 1.5e6);
        assert!(two.len() == 2 && two[0] < 2_000.0 && two[1] > 15_000.0, "{two:?}");
        let one = roots(&mixture, 300.0, 5e6);
        assert_eq!(one.len(), 1, "{one:?}");
        assert_eq!(stable_root(&mixture, 300.0, 5e6).ok(), Some(one[0]));
        assert!(two.contains(&stable_root(&mixture, 200.0, 1.5e6).unwrap_or(f64::NAN)));
    }

    /// The solve inverts a 3 × 3 system that needs pivoting, and refuses a singular one.
    #[test]
    fn solve_pivots_and_refuses_singular_systems() {
        let a = vec![vec![0.0, 2.0, 1.0], vec![1.0, 1.0, 0.0], vec![3.0, 0.0, 1.0]];
        let x = solve(a, vec![5.0, 3.0, 4.0]).unwrap_or_default();
        assert_eq!(x.len(), 3);
        for (got, want) in x.iter().zip([1.0, 2.0, 1.0]) {
            assert!((got - want).abs() <= 1e-15, "{x:?}");
        }
        assert_eq!(solve(vec![vec![1.0, 2.0], vec![2.0, 4.0]], vec![1.0, 2.0]), None);
    }
}
