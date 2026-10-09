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
        let liquid = at_t_p(eos, 400.0, 1e6, 10_000.0, true).unwrap();
        let vapour = at_t_p(eos, 400.0, 1e5, 10_000.0, false).unwrap();
        assert!(liquid > 52_000.0 && (p_at(400.0, liquid) / 1e6 - 1.0).abs() < 1e-12, "{liquid}");
        assert!((vapour - 30.3).abs() < 0.1 && (p_at(400.0, vapour) / 1e5 - 1.0).abs() < 1e-12, "{vapour}");
        let (t, rho_c) = (647.1, 17_873.728);
        let p = p_at(t, rho_c);
        let near = at_t_p(eos, t, p, rho_c * 1.001, true).unwrap();
        assert!((near / rho_c - 1.0).abs() < 1e-6, "{near}");
    }
}
