//! The density of a Helmholtz model at a given (T, p) on a phase's branch, from a seed density (PLAN.md M6.9: the
//! saturated phases of a pseudo-pure fluid, CoolProp's `update_TP_guessrho`, `HelmholtzEOSMixtureBackend.cpp:1422-1434`
//! and `solver_rho_Tp`, 3036-3074). Newton in ln ρ on p(T, ρ) − p, whose derivative is (∂p/∂ln ρ)_T = ρRT(2A01 + A02),
//! each step at most [`MAX_STEP`]; every iterate must be mechanically stable, (∂p/∂ρ)_T > 0. A seed in the unstable
//! region (an ancillary's near the critical point) is replaced by the branch's far end, as CoolProp restarts: the
//! model's largest density for a liquid, the ideal gas's p/RT for a vapour.

use crate::derivs::Order;
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

/// The density at (T, p) on the liquid (`liquid`) or vapour branch, from `seed` (mol/m³), else from the branch's far
/// end; `NoConvergence { DensityNewton }` if neither converges.
pub(crate) fn at_t_p(eos: &dyn HelmholtzModel, t: f64, p: f64, seed: f64, liquid: bool) -> Result<f64, Error> {
    newton(eos, t, p, seed).or_else(|_| {
        let far = if liquid { eos.rho_max(t) } else { p / (eos.gas_constant() * t) };
        newton(eos, t, p, far)
    })
}

/// Newton from `seed`, refused at the first unstable or non-finite iterate.
fn newton(eos: &dyn HelmholtzModel, t: f64, p: f64, seed: f64) -> Result<f64, Error> {
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
        let size = (pressure - p).abs() / (rho * rt * (1.0 + b.a01.abs()));
        u += step.clamp(-MAX_STEP, MAX_STEP);
        if newton_converged(step.abs(), size, previous, (STEP_TOL, FLOOR)) {
            return Ok(math::exp(u));
        }
        previous = step.abs();
    }
    Err(failed(MAX_ITER))
}
