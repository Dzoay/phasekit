//! The density of a Helmholtz model at a given (T, p) on a phase's branch.
//!
//! - [`bracketed`] (PLAN.md M7.1; map 03 §9 `flash::density`, CoolProp-uqvr "always-bracketed density solve"): the PT
//!   flash's solve between two densities where p(T, ρ) − p changes sign once, so the root is unique by construction. Its
//!   Newton evaluates `residual + IDEAL_DELTA` and never α⁰ (ARCHITECTURE.md D6, Lean graft).
//! - [`at_t_p`] (PLAN.md M6.9): from a seed density, unbracketed, for the saturated phases of a pseudo-pure fluid and
//!   an imposed phase, as CoolProp's `update_TP_guessrho` (`HelmholtzEOSMixtureBackend.cpp:1422-1434`) and
//!   `solver_rho_Tp` (3036-3074). Every iterate must be mechanically stable, (∂p/∂ρ)_T > 0. A seed in the unstable
//!   region (an ancillary's near the critical point) is replaced by the branch's far end, as CoolProp restarts: the
//!   model's largest density for a liquid, the ideal gas's p/RT for a vapour.
//!
//! Both run Newton in ln ρ on p(T, ρ) − p, whose derivative is (∂p/∂ln ρ)_T = ρRT(2A01 + A02).
//!
//! The public items are the seam a family with its own flash uses (PLAN.md M13.1; a mixture's phases at (T, p)): [`solve`]
//! on a branch, [`roots`] from both branches' far ends, [`select`] among them under a [`RootPolicy`], and the acceptance
//! gate [`reproduces`] the core flash applies to every state it returns.

use crate::derivs::{Bundle, Derivs, Order};
use crate::error::{Error, Roots};
use crate::flash::RootPolicy;
use crate::flash::search::choose;
use crate::helmholtz::HelmholtzModel;
use crate::num::math;
use crate::roots::newton_converged;
use crate::state::Strategy;

/// Newton iterations allowed.
const MAX_ITER: u16 = 50;

/// Converged when ln ρ moves by no more than this.
const STEP_TOL: f64 = 1e-14;

/// The residual |p(T, ρ) − p| relative to the scale it cancels from, ρRT(1 + |A01|), at its rounding floor: near the
/// critical point the steps are that rounding over a small (∂p/∂ln ρ)_T and never reach [`STEP_TOL`]
/// ([`newton_converged`]).
const FLOOR: f64 = 1e-14;

/// The largest step in ln ρ: a seed from an ancillary is within a few % of the root.
const MAX_STEP: f64 = 0.25;

/// Iterations a bracketed solve may spend: Newton, or bisection in ln ρ where Newton leaves the bracket.
const MAX_BRACKETED: u16 = 100;

/// A bracketed solve has converged when its confirmed bracket is this narrow in ln ρ. Where p's terms are large its
/// rounding stops Newton short of [`STEP_TOL`]: R22 at 132 K (p's noise 4e-12 of ρRT, its terms large at τ = 2.8) sends
/// it between two densities 4.4e-14 apart in ln ρ, each step longer than the last (PLAN.md M7.1, the nightly grid).
const WIDTH_TOL: f64 = 1e-13;

/// The density at (T, p) between `lo` and `hi`, where p(T, ρ) − p is negative at `lo`, positive at `hi` and rises
/// monotonically (a branch of stable states), from `seed` (moved to the middle in ln ρ if outside); with the iterations
/// spent. Newton in ln ρ on `residual + IDEAL_DELTA`; every iterate narrows the bracket, and a step that leaves it, or
/// meets a slope that is not positive, bisects it in ln ρ. Converged as [`at_t_p`]'s Newton ([`newton_converged`]), or
/// when the bracket is within [`WIDTH_TOL`] in ln ρ and an iterate has confirmed each of its ends: on a near-critical
/// isotherm, flat to rounding, Newton's steps leave the bracket and only its halving closes in, and where p's terms are
/// large its rounding sends Newton between two densities for good. The ends are not
/// evaluated up front; one that no iterate confirms (the root lies beyond it) leaves the bracket closing on it, and
/// that is `NoConvergence { DensityNewton }`, as are an iterate that is not finite and running out of iterations.
pub(crate) fn bracketed(
    eos: &dyn HelmholtzModel,
    t: f64,
    p: f64,
    (lo, hi): (f64, f64),
    seed: f64,
) -> Result<(f64, u16), Error> {
    let rt = eos.gas_constant() * t;
    let mut bracket = LnBracket { below: math::ln(lo), above: math::ln(hi), confirmed: [false; 2] };
    let start = math::ln(seed);
    let mut u = if bracket.holds(start) { start } else { bracket.middle() };
    let mut previous = f64::INFINITY;
    for iteration in 1..=MAX_BRACKETED {
        let rho = math::exp(u);
        let b = (eos.residual(t, rho, Order::Two) + Derivs::IDEAL_DELTA).bundle().ok_or(failed(iteration))?;
        let (pressure, slope) = (rho * rt * b.a01, rho * rt * (2.0 * b.a01 + b.a02));
        if !pressure.is_finite() {
            return Err(failed(iteration));
        }
        if pressure == p {
            return Ok((rho, iteration));
        }
        bracket.take(u, pressure < p);
        if let Some(closed) = bracket.closed(iteration) {
            return closed;
        }
        let next = u + (p - pressure) / slope;
        if !(slope > 0.0 && bracket.holds(next)) {
            (u, previous) = (bracket.middle(), f64::INFINITY);
            continue;
        }
        let step = (next - u).abs();
        if stops(step, scaled_residual(pressure, p, rho, rt, b.a01), previous, slope <= SOFT * p) {
            return Ok((math::exp(next), iteration));
        }
        (u, previous) = (next, step);
    }
    Err(failed(MAX_BRACKETED))
}

/// Whether [`bracketed`]'s Newton stops after a step of `length` with the residual's scaled `size` and the last step
/// `previous`: converged ([`newton_converged`]), or on a `soft` branch ([`SOFT`]) [`quadratically_done`].
fn stops(length: f64, size: f64, previous: f64, soft: bool) -> bool {
    newton_converged(length, size, previous, (STEP_TOL, FLOOR)) || (soft && quadratically_done(length, previous))
}

/// Whether Newton, converging quadratically, has a step of `length` after one of `previous` that leaves the next
/// within [`PREDICTED`] of the root (issue #143: it spares the iteration that would confirm it). The next error is
/// about C·length², C ≈ length/previous² from the two steps; where convergence is slower than quadratic the estimate
/// only grows, and the test is stricter than [`STEP_TOL`].
fn quadratically_done(length: f64, previous: f64) -> bool {
    previous.is_finite() && length <= 1e-6 && length * length * length <= PREDICTED * previous * previous
}

/// The error in ln ρ, predicted from the quadratic convergence, below which [`quadratically_done`] stops: a hundredth of
/// [`STEP_TOL`], so the density returned is as precise as a step to [`STEP_TOL`] leaves it.
const PREDICTED: f64 = 1e-16;

/// [`quadratically_done`] stops a solve only on a soft branch, (∂p/∂ln ρ)_T within this many times p (a gas, a
/// supercritical state): a stiff liquid's p moves by its bulk modulus per unit of ln ρ, so an ulp of ln ρ there is
/// 1e-11 of p, and only the iteration that confirms the root holds p to its rounding.
const SOFT: f64 = 100.0;

/// [`bracketed`]'s bracket in ln ρ, p(T, ρ) below p at `below` and above it at `above`, and whether an iterate has
/// confirmed each end (`confirmed`: below, above).
struct LnBracket {
    below: f64,
    above: f64,
    confirmed: [bool; 2],
}

impl LnBracket {
    /// Whether `u` lies in the closed bracket.
    fn holds(&self, u: f64) -> bool {
        self.below <= u && u <= self.above
    }

    /// The bracket's middle.
    fn middle(&self) -> f64 {
        midpoint(self.below, self.above)
    }

    /// The iterate `u`, its p below p (`lower`) or not, as the end on its side, confirmed.
    fn take(&mut self, u: f64, lower: bool) {
        if lower {
            (self.below, self.confirmed[0]) = (u, true);
        } else {
            (self.above, self.confirmed[1]) = (u, true);
        }
    }

    /// The solve's end once the bracket is within [`WIDTH_TOL`] in ln ρ at `iteration`: its middle's density if an
    /// iterate has confirmed both ends, else `NoConvergence` (the root lies beyond the unconfirmed one).
    fn closed(&self, iteration: u16) -> Option<Result<(f64, u16), Error>> {
        (self.above - self.below <= WIDTH_TOL).then(|| {
            if self.confirmed == [true; 2] { Ok((math::exp(self.middle()), iteration)) } else { Err(failed(iteration)) }
        })
    }
}

/// A density solve's `NoConvergence` after `iterations`.
fn failed(iterations: u16) -> Error {
    Error::NoConvergence { strategy: Strategy::DensityNewton, iterations }
}

/// The acceptance gate's "inputs reproduced" (D6) for a density solve: |p(T, ρ) − p| at most this much of the larger of
/// the scale p cancels from, ρRT(1 + |A01|), and the change a relative change of ρ makes, (∂p/∂ln ρ)_T. The second is
/// the density's own precision: a stiff liquid's p moves 2.2e9 Pa per unit of ln ρ (R22 at 121 K), so a density right
/// to 1.4e-14 leaves p 4e-5 Pa off, 2e-12 of ρRT (PLAN.md M7.1, the nightly grid).
const REPRODUCED: f64 = 1e-12;

/// Whether the state at (T, ρ) with total bundle `b` reproduces the pressure `p` to 1e-12 of the scale it cancels from:
/// the acceptance gate the core flash applies to every state it returns.
pub fn reproduces(r: f64, t: f64, rho: f64, p: f64, b: &Bundle) -> bool {
    let rt = r * t;
    let scale = (rho * rt * (1.0 + b.a01.abs())).max(rho * rt * (2.0 * b.a01 + b.a02));
    (crate::relations::pressure(r, t, rho, b) - p).abs() <= REPRODUCED * scale
}

/// The liquid density at (T, p) above `lo`, a liquid's density where p(T, lo) < p, with the iterations of its bracketed
/// solve (PLAN.md M7.1: datagen's `rho_max` is ρ(T_min, p_max) above the saturated liquid at T_min, and PT's bracket
/// grows past `rho_max` where a model is not densest at T_min, OrthoHydrogen at 1.7 GPa and 174 K): the bracket's
/// upper end grows by 5 % until p(T, ρ) passes p, at most [`GROWTH`] times, then [`bracketed`] from `lo`. The steps are small because some models turn back:
/// R123's p at 166 K peaks at 1.6e8 Pa near 1.1 times its saturated liquid's density and is negative at 1.2 times.
pub(crate) fn liquid_above(eos: &dyn HelmholtzModel, t: f64, p: f64, lo: f64) -> Result<(f64, u16), Error> {
    let rt = eos.gas_constant() * t;
    let mut hi = lo;
    for _ in 0..GROWTH {
        hi *= 1.05;
        let b = (eos.residual(t, hi, Order::Two) + Derivs::IDEAL_DELTA).bundle();
        if b.is_some_and(|b| hi * rt * b.a01 > p) {
            return bracketed(eos, t, p, (lo, hi), lo);
        }
    }
    Err(Error::NoConvergence { strategy: Strategy::DensityNewton, iterations: 0 })
}

/// The branch of an isotherm a density solve follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Branch {
    /// The dense branch, entered from the model's largest density.
    Liquid,
    /// The dilute branch, entered from the ideal gas's p/RT.
    Vapour,
}

/// The density (mol/m³) at (T, p) on `branch`: Newton in ln ρ from `seed` when given, else from the branch's far end,
/// through mechanically stable states only ((∂p/∂ρ)_T > 0), and refused (`NoConvergence { DensityNewton }`) unless the
/// root [`reproduces`] p. A seed in the unstable region restarts from the far end.
pub fn solve(eos: &dyn HelmholtzModel, t: f64, p: f64, branch: Branch, seed: Option<f64>) -> Result<f64, Error> {
    let liquid = branch == Branch::Liquid;
    let (rho, iterations) = match seed {
        Some(seed) => at_t_p(eos, t, p, seed, liquid)?,
        None => newton(eos, t, p, far_end(eos, t, p, liquid))?,
    };
    let b = (eos.residual(t, rho, Order::Two) + Derivs::IDEAL_DELTA).bundle().ok_or(failed(iterations))?;
    if reproduces(eos.gas_constant(), t, rho, p, &b) { Ok(rho) } else { Err(failed(iterations)) }
}

/// Two roots closer than this in ln ρ are one, reached from both ends (above the critical temperature, or off the
/// dome): each is converged to [`STEP_TOL`].
const SAME_ROOT: f64 = 1e-12;

/// Every distinct root [`solve`] reaches from the two branches' far ends at (T, p), ascending: two where a vapour and a
/// liquid (each stable or metastable) coexist on the isotherm, one elsewhere, none where neither converges. Never a root
/// on the unstable branch between them (ROT-181).
pub fn roots(eos: &dyn HelmholtzModel, t: f64, p: f64) -> Roots {
    let [vapour, liquid] = [Branch::Vapour, Branch::Liquid].map(|branch| solve(eos, t, p, branch, None).ok());
    match (vapour, liquid) {
        (Some(v), Some(l)) if (math::ln(l) - math::ln(v)).abs() > SAME_ROOT => Roots::new(&[v.min(l), v.max(l)]),
        (Some(one), _) | (None, Some(one)) => Roots::new(&[one]),
        (None, None) => Roots::new(&[]),
    }
}

/// One root of [`roots`] under `policy`: the only one; of two, `Strict` refuses with `Ambiguous`, `Nearest(ρ)` takes the
/// one nearest ρ, `Stable` the one of lower Gibbs energy (at fixed T, p and composition, the stable single phase).
/// `NoConvergence { DensityNewton }` without a root.
pub fn select(eos: &dyn HelmholtzModel, t: f64, p: f64, policy: RootPolicy) -> Result<f64, Error> {
    let roots = roots(eos, t, p);
    let chosen = match (roots.as_slice(), policy) {
        (several @ [_, _, ..], RootPolicy::Stable) => {
            let gibbs = |rho: &f64| {
                let d = eos.ideal(t, *rho, Order::One) + eos.residual(t, *rho, Order::One);
                d.get(0, 0).zip(d.get(0, 1)).map_or(f64::NAN, |(a00, a01)| a00 + a01)
            };
            several.iter().copied().min_by(|a, b| gibbs(a).total_cmp(&gibbs(b)))
        }
        (all, policy) => choose(all, |rho| *rho, policy)?,
    };
    chosen.ok_or(failed(0))
}

/// Growth steps [`liquid_above`] may take: 1.05^100 ≈ 131 times its lower end.
const GROWTH: u16 = 100;

/// The middle of [a, b], in the variable given (ln ρ here).
fn midpoint(a: f64, b: f64) -> f64 {
    a + (b - a) / 2.0
}

/// The density at (T, p) on the liquid (`liquid`) or vapour branch, from `seed` (mol/m³), else from the branch's far
/// end, with the iterations of the solve that converged; `NoConvergence { DensityNewton }` if neither converges.
pub(crate) fn at_t_p(eos: &dyn HelmholtzModel, t: f64, p: f64, seed: f64, liquid: bool) -> Result<(f64, u16), Error> {
    newton(eos, t, p, seed).or_else(|_| newton(eos, t, p, far_end(eos, t, p, liquid)))
}

/// The branch's far end, where a restart begins: the model's largest density for a liquid, the ideal gas's p/RT for a
/// vapour.
fn far_end(eos: &dyn HelmholtzModel, t: f64, p: f64, liquid: bool) -> f64 {
    if liquid { eos.rho_max(t) } else { p / (eos.gas_constant() * t) }
}

/// |p(T, ρ) − p| relative to the scale it cancels from, ρRT(1 + |A01|), with `rt` = RT.
fn scaled_residual(pressure: f64, p: f64, rho: f64, rt: f64, a01: f64) -> f64 {
    (pressure - p).abs() / (rho * rt * (1.0 + a01.abs()))
}

/// Newton from `seed`, refused at the first unstable or non-finite iterate; with its iterations.
fn newton(eos: &dyn HelmholtzModel, t: f64, p: f64, seed: f64) -> Result<(f64, u16), Error> {
    let rt = eos.gas_constant() * t;
    let (mut u, mut previous) = (math::ln(seed), f64::INFINITY);
    for iteration in 1..=MAX_ITER {
        let rho = math::exp(u);
        let d = eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two);
        let b = d.bundle().ok_or(failed(iteration))?;
        let (pressure, slope) = (rho * rt * b.a01, rho * rt * (2.0 * b.a01 + b.a02));
        if !(pressure.is_finite() && slope > 0.0) {
            return Err(failed(iteration));
        }
        let step = (p - pressure) / slope;
        let size = scaled_residual(pressure, p, rho, rt, b.a01);
        u += step.clamp(-MAX_STEP, MAX_STEP);
        if newton_converged(step.abs(), size, previous, (STEP_TOL, FLOOR)) {
            return Ok((math::exp(u), iteration));
        }
        previous = step.abs();
    }
    Err(failed(MAX_ITER))
}

#[cfg(all(test, feature = "fluids-all"))]
mod tests {
    use super::*;

    /// Water at 400 K (saturation 245.77 kPa): from a seed inside the spinodal region (10 000 mol/m³, where
    /// (∂p/∂ρ)_T < 0) the solve restarts from the branch's far end, the model's largest density for the compressed
    /// liquid at 1 MPa and the ideal gas's p/RT for the vapour at 100 kPa, and finds each root to rounding; just above the
    /// critical point (647.1 K), on a flat isotherm, it converges from a seed 1e-3 off.
    #[test]
    fn unstable_seeds_restart_from_the_branch_end() {
        let registry = crate::Registry::from_embedded(crate::DataSet::Parity).unwrap();
        let fluid = crate::internal::record(&registry, "Water").unwrap().compile().unwrap();
        let eos = fluid.eos();
        let p_at = |t: f64, rho: f64| {
            let d = eos.ideal(t, rho, Order::One) + eos.residual(t, rho, Order::One);
            rho * eos.gas_constant() * t * d.get(0, 1).unwrap()
        };
        assert!(newton(eos, 400.0, 1e6, 10_000.0).is_err(), "the seed is unstable");
        let (liquid, _) = at_t_p(eos, 400.0, 1e6, 10_000.0, true).unwrap();
        let (vapour, _) = at_t_p(eos, 400.0, 1e5, 10_000.0, false).unwrap();
        assert!(liquid > 52_000.0 && (p_at(400.0, liquid) / 1e6 - 1.0).abs() < 1e-12, "{liquid}");
        assert!((vapour - 30.3).abs() < 0.1 && (p_at(400.0, vapour) / 1e5 - 1.0).abs() < 1e-12, "{vapour}");
        let (t, rho_c) = (647.1, 17_873.728);
        let p = p_at(t, rho_c);
        let (near, _) = at_t_p(eos, t, p, rho_c * 1.001, true).unwrap();
        assert!((near / rho_c - 1.0).abs() < 1e-6, "{near}");
        assert_eq!(far_end(eos, 400.0, 1e6, true), eos.rho_max(400.0));
        assert_eq!(far_end(eos, 400.0, 1e5, false), 1e5 / (eos.gas_constant() * 400.0));
    }

    /// The acceptance gate's "inputs reproduced", exactly: |p(T, ρ) − p| against 1e-12 of the larger of ρRT(1 + |A01|)
    /// and (∂p/∂ln ρ)_T. At R = 2, T = 3 and ρ = 5 (ρRT = 30) with A01 = 0.5, p(T, ρ) = 15: a soft state
    /// (2A01 + A02 = 0.1) allows 1e-12 of 45, a stiff one (2A01 + A02 = 100) 1e-12 of 3000.
    #[test]
    fn reproduction_is_judged_on_both_scales() {
        let bundle = |a02| Bundle { a00: 0.0, a10: 0.0, a01: 0.5, a20: 0.0, a11: 0.0, a02 };
        let (soft, stiff) = (bundle(-0.9), bundle(99.0));
        let at = |b: &Bundle, dp: f64| reproduces(2.0, 3.0, 5.0, 15.0 + dp, b);
        assert!(at(&soft, 4.4e-11) && !at(&soft, 4.6e-11));
        assert!(at(&stiff, 2.9e-9) && !at(&stiff, 3.01e-9));
    }

    /// Issue #143: a step of 1e-9 after one of 1e-4 predicts the next within 1e-16 of the root (1e-27/1e-8), so the
    /// solve stops; one of 1e-6 after 1e-3 does not (1e-18/1e-6), nor a first step, nor one over 1e-6, nor slow
    /// convergence, a step half the last.
    #[test]
    fn quadratic_convergence_stops_a_step_early() {
        assert!(quadratically_done(1e-9, 1e-4));
        assert!(!quadratically_done(1e-6, 1e-3) && !quadratically_done(1e-9, f64::INFINITY));
        assert!(!quadratically_done(2e-6, 1.0) && !quadratically_done(1e-15, 2e-15));
    }

    /// The residual's scale, exactly: |3 − 1| / (2·5·(1 + |−0.5|)) = 2/15.
    #[test]
    fn residual_is_scaled_by_its_cancelling_terms() {
        assert_eq!(scaled_residual(3.0, 1.0, 2.0, 5.0, -0.5), 2.0 / 15.0);
    }
}
