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

use crate::derivs::{Bundle, Derivs, Order};
use crate::error::Error;
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
    let failed = |iterations| Error::NoConvergence { strategy: Strategy::DensityNewton, iterations };
    let rt = eos.gas_constant() * t;
    let (mut below, mut above) = (math::ln(lo), math::ln(hi));
    let start = math::ln(seed);
    let mut u = if below <= start && start <= above { start } else { midpoint(below, above) };
    let (mut previous, mut confirmed) = (f64::INFINITY, [false; 2]);
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
        if pressure < p {
            (below, confirmed[0]) = (u, true);
        } else {
            (above, confirmed[1]) = (u, true);
        }
        if above - below <= WIDTH_TOL {
            return if confirmed == [true; 2] {
                Ok((math::exp(midpoint(below, above)), iteration))
            } else {
                Err(failed(iteration))
            };
        }
        let next = u + (p - pressure) / slope;
        if !(slope > 0.0 && below <= next && next <= above) {
            (u, previous) = (midpoint(below, above), f64::INFINITY);
            continue;
        }
        let step = (next - u).abs();
        if newton_converged(step, scaled_residual(pressure, p, rho, rt, b.a01), previous, (STEP_TOL, FLOOR)) {
            return Ok((math::exp(next), iteration));
        }
        (u, previous) = (next, step);
    }
    Err(failed(MAX_BRACKETED))
}

/// The acceptance gate's "inputs reproduced" (D6) for a density solve: |p(T, ρ) − p| at most this much of the larger of
/// the scale p cancels from, ρRT(1 + |A01|), and the change a relative change of ρ makes, (∂p/∂ln ρ)_T. The second is
/// the density's own precision: a stiff liquid's p moves 2.2e9 Pa per unit of ln ρ (R22 at 121 K), so a density right
/// to 1.4e-14 leaves p 4e-5 Pa off, 2e-12 of ρRT (PLAN.md M7.1, the nightly grid).
const REPRODUCED: f64 = 1e-12;

/// Whether the state at (T, ρ) with total bundle `b` reproduces the pressure `p` ([`REPRODUCED`]).
pub(crate) fn reproduces(r: f64, t: f64, rho: f64, p: f64, b: &Bundle) -> bool {
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
    let failed = |iterations| Error::NoConvergence { strategy: Strategy::DensityNewton, iterations };
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

    /// The residual's scale, exactly: |3 − 1| / (2·5·(1 + |−0.5|)) = 2/15.
    #[test]
    fn residual_is_scaled_by_its_cancelling_terms() {
        assert_eq!(scaled_residual(3.0, 1.0, 2.0, 5.0, -0.5), 2.0 / 15.0);
    }
}
