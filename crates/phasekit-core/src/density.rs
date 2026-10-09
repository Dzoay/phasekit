//! The density of a Helmholtz model at a given (T, p), on the branch of a seed density (PLAN.md M6.9: the saturated
//! phases of a pseudo-pure fluid, CoolProp's `update_TP_guessrho`, `HelmholtzEOSMixtureBackend.cpp:1422-1434`). Newton
//! in ln ρ on p(T, ρ)/p − 1, whose derivative is (∂p/∂ln ρ)_T/p = ρRT(2A01 + A02)/p, from the seed, each step at most
//! [`MAX_STEP`]; the answer must be mechanically stable, (∂p/∂ρ)_T > 0, and is refused otherwise: a step into the
//! spinodal region leaves the seed's branch.

use crate::derivs::Order;
use crate::error::Error;
use crate::helmholtz::HelmholtzModel;
use crate::num::math;
use crate::state::Strategy;

/// Newton iterations allowed.
const MAX_ITER: u16 = 50;

/// Converged when ln ρ moves by no more than this.
const STEP_TOL: f64 = 1e-14;

/// The largest step in ln ρ: a seed from an ancillary is within a few % of the root.
const MAX_STEP: f64 = 0.25;

/// The density at (T, p) on the branch of `seed` (mol/m³), else `NoConvergence { DensityNewton }`.
pub(crate) fn at_t_p(eos: &dyn HelmholtzModel, t: f64, p: f64, seed: f64) -> Result<f64, Error> {
    let failed = |iterations| Error::NoConvergence { strategy: Strategy::DensityNewton, iterations };
    let rt = eos.gas_constant() * t;
    let mut u = math::ln(seed);
    for iteration in 1..=MAX_ITER {
        let rho = math::exp(u);
        let d = eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two);
        let b = d.bundle().ok_or(failed(iteration))?;
        let (pressure, slope) = (rho * rt * b.a01, rho * rt * (2.0 * b.a01 + b.a02));
        if !(pressure.is_finite() && slope > 0.0) {
            return Err(failed(iteration));
        }
        let step = (p - pressure) / slope;
        let length = step.abs();
        u += step.clamp(-MAX_STEP, MAX_STEP);
        if length <= STEP_TOL {
            return Ok(math::exp(u));
        }
    }
    Err(failed(MAX_ITER))
}
