//! The numerical critical point of a Helmholtz model (PLAN.md M6.7; map 03 §6; ROT-084): where the isotherm's
//! ∂p/∂ρ and ∂²p/∂ρ² both vanish. With A_ij = τ^i δ^j ∂^(i+j)α/∂τ^i∂δ^j of the total α, ∂p/∂ρ = RT·K1 and
//! ∂²p/∂ρ² = (RT/ρ)·K2 for K1 = 2A01 + A02 and K2 = 2A01 + 4A02 + A03. Newton in (ln T, ln ρ) solves K1 = K2 = 0:
//! ∂K1/∂ln ρ is K2 itself, ∂K2/∂ln ρ = 2A01 + 10A02 + 7A03 + A04, and T·∂/∂T at fixed ρ is −(2A11 + A12) and
//! −(2A11 + 4A12 + A13), so the Jacobian at the root is triangular with a nonzero determinant. Below Tc the
//! isotherms of a multiparameter EOS have several inflections, so a nested search along them is ill-posed, where
//! Newton from a close guess (a superancillary's top, a published point) is not.

use crate::derivs::Order;
use crate::error::Error;
use crate::helmholtz::HelmholtzModel;
use crate::num::{math, solve_small};
use crate::state::Strategy;

/// Newton iterations allowed.
const MAX_ITER: u16 = 50;

/// Converged when neither ln T nor ln ρ moves by more than this.
const STEP_TOL: f64 = 1e-14;

/// K1 and K2 this small are at their rounding floor: Newton has converged there once its step stops shrinking (no
/// longer halved since the last iteration). ∂K2/∂ln ρ is small at the critical point (2.7e-3 for Ammonia), so the
/// steps are K2's rounding over it, about 1e-11 in ln ρ, and never reach [`STEP_TOL`].
const FLOOR: f64 = 1e-12;

/// The largest step in ln T and in ln ρ: the guess is within a few % of the root (R40's published Tc is 0.56 % off).
const MAX_STEP: (f64, f64) = (0.005, 0.05);

/// (K1, K2) at (T, ρ) and the Jacobian of (K1, K2) in (ln T, ln ρ); `None` if the model refuses the point.
fn system(eos: &dyn HelmholtzModel, t: f64, rho: f64) -> Option<([f64; 2], [[f64; 2]; 2])> {
    let d = eos.ideal(t, rho, Order::Four) + eos.residual(t, rho, Order::Four);
    let a = |i, j| d.get(i, j);
    let [a01, a02, a03, a04, a11, a12, a13] = [a(0, 1)?, a(0, 2)?, a(0, 3)?, a(0, 4)?, a(1, 1)?, a(1, 2)?, a(1, 3)?];
    let k = [2.0 * a01 + a02, 2.0 * a01 + 4.0 * a02 + a03];
    let jacobian =
        [[-(2.0 * a11 + a12), k[1]], [-(2.0 * a11 + 4.0 * a12 + a13), 2.0 * a01 + 10.0 * a02 + 7.0 * a03 + a04]];
    (k.iter().chain(jacobian.iter().flatten()).all(|v| v.is_finite())).then_some((k, jacobian))
}

/// The critical point (T, ρ, p) of `eos` from the guess (T₀, ρ₀), else `NoConvergence { Vle }`.
pub(crate) fn numerical(eos: &dyn HelmholtzModel, t0: f64, rho0: f64) -> Result<(f64, f64, f64), Error> {
    let failed = |iterations| Error::NoConvergence { strategy: Strategy::Vle, iterations };
    let (mut t, mut rho, mut previous) = (t0, rho0, f64::INFINITY);
    for iteration in 1..=MAX_ITER {
        let (k, jacobian) = system(eos, t, rho).ok_or(failed(iteration))?;
        let step = solve_small(jacobian, [-k[0], -k[1]]).map_err(|_| failed(iteration))?;
        let length = step[0].abs().max(step[1].abs());
        let scale = (MAX_STEP.0 / step[0].abs()).min(MAX_STEP.1 / step[1].abs()).min(1.0);
        (t, rho) = (t * math::exp(scale * step[0]), rho * math::exp(scale * step[1]));
        let at_floor = k[0].abs().max(k[1].abs()) <= FLOOR && length > previous / 2.0;
        previous = length;
        if length <= STEP_TOL || at_floor {
            let d = eos.ideal(t, rho, Order::One) + eos.residual(t, rho, Order::One);
            let a01 = d.get(0, 1).ok_or(failed(iteration))?;
            return Ok((t, rho, rho * eos.gas_constant() * t * a01));
        }
    }
    Err(failed(MAX_ITER))
}
