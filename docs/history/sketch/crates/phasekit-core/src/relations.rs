//! L1 relations: properties as pure functions of the order-2 total bundle (materials S1). Written once for
//! every potential: Helmholtz families directly, Gibbs families after [`bundle_from_gibbs`]. Molar SI.
//! Crate-private except the Gibbs seam: users reach these through `State` and `Fluid::prop`.

use crate::derivs::Bundle;
use crate::error::{DomainError, Error};
use crate::prop::DerivVar;

/// p = ρRT·A01.
pub(crate) fn pressure(r: f64, t: f64, rho: f64, b: &Bundle) -> f64 {
    rho * r * t * b.a01
}

/// u = RT·A10.
pub(crate) fn internal_energy(r: f64, t: f64, b: &Bundle) -> f64 {
    r * t * b.a10
}

/// h = RT·(A10 + A01).
pub(crate) fn enthalpy(r: f64, t: f64, b: &Bundle) -> f64 {
    r * t * (b.a10 + b.a01)
}

/// s = R·(A10 − A00).
pub(crate) fn entropy(r: f64, b: &Bundle) -> f64 {
    r * (b.a10 - b.a00)
}

/// cv = −R·A20 (non-finite at a non-analytic critical point; the caller reports `Undefined`).
pub(crate) fn cv(r: f64, b: &Bundle) -> f64 {
    -r * b.a20
}

/// (∂p/∂ρ)_T = RT·(2·A01 + A02).
pub(crate) fn dp_drho_t(r: f64, t: f64, b: &Bundle) -> f64 {
    r * t * (2.0 * b.a01 + b.a02)
}

/// cp = cv + R·(A01 − A11)² / (2·A01 + A02) (infinite where (∂p/∂ρ)_T = 0; the caller reports `Undefined`).
pub(crate) fn cp(r: f64, b: &Bundle) -> f64 {
    let x = b.a01 - b.a11;
    cv(r, b) + r * x * x / (2.0 * b.a01 + b.a02)
}

/// w² in (m/s)² = (cp/cv)·(∂p/∂ρ)_T / M, with M in kg/mol.
pub(crate) fn speed_of_sound_squared(r: f64, t: f64, molar_mass: f64, b: &Bundle) -> f64 {
    cp(r, b) / cv(r, b) * dp_drho_t(r, t, b) / molar_mass
}

/// One phase point as the derivative engine sees it.
pub(crate) struct At<'a> {
    pub(crate) r: f64,
    pub(crate) t: f64,
    pub(crate) rho: f64,
    pub(crate) molar_mass: f64,
    /// Entropy gauge offset: the only gauge term with a T-derivative (g = h − Ts).
    pub(crate) ds: f64,
    pub(crate) b: &'a Bundle,
}

/// `(∂X/∂T)_ρ` and `(∂X/∂ρ)_T` of a first-order state function (CoolProp's `get_dT_drho`, map 01 §4a).
fn dt_drho(x: DerivVar, at: &At<'_>) -> (f64, f64) {
    let (r, t, rho, b) = (at.r, at.t, at.rho, at.b);
    let (rt_rho, m) = (r * t / rho, at.molar_mass);
    let (h, s, u, g) = (
        (r * (b.a01 - b.a20 - b.a11), rt_rho * (b.a11 + b.a01 + b.a02)),
        (-r * b.a20 / t, r / rho * (b.a11 - b.a01)),
        (-r * b.a20, rt_rho * b.a11),
        (r * (b.a00 + b.a01 - b.a10 - b.a11) - at.ds, rt_rho * (2.0 * b.a01 + b.a02)),
    );
    let mass = |(a, b): (f64, f64)| (a / m, b / m);
    match x {
        DerivVar::T => (1.0, 0.0),
        DerivVar::P => (rho * r * (b.a01 - b.a11), dp_drho_t(r, t, b)),
        DerivVar::Dmolar => (0.0, 1.0),
        DerivVar::Dmass => (0.0, m),
        DerivVar::Hmolar => h,
        DerivVar::Hmass => mass(h),
        DerivVar::Smolar => s,
        DerivVar::Smass => mass(s),
        DerivVar::Umolar => u,
        DerivVar::Umass => mass(u),
        DerivVar::Gmolar => g,
        DerivVar::Gmass => mass(g),
    }
}

/// `(∂of/∂wrt)_at` by the Jacobian ratio in (T, ρ): `[X_T Z_ρ − X_ρ Z_T] / [Y_T Z_ρ − Y_ρ Z_T]`
/// (map 01 §4a, U8). `None` where the denominator vanishes (e.g. `(∂T/∂T)_T`).
pub(crate) fn first_partial(of: DerivVar, wrt: DerivVar, at_const: DerivVar, at: &At<'_>) -> Option<f64> {
    let ((xt, xr), (yt, yr), (zt, zr)) = (dt_drho(of, at), dt_drho(wrt, at), dt_drho(at_const, at));
    let den = yt * zr - yr * zt;
    let v = (xt * zr - xr * zt) / den;
    (den != 0.0 && v.is_finite()).then_some(v)
}

/// Molar Gibbs energy and its (T, p) derivatives to order 2. IF97 regions 1/2/5, IAPWS-06 ice Ih and
/// Bollengier share this one property map (map 07); a Gibbs family supplies order 3 through
/// `ThermoModel::derivs` (E1).
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)] // g and its partial derivatives, molar SI
pub struct GibbsDerivs {
    pub g: f64,
    pub g_t: f64,
    pub g_p: f64,
    pub g_tt: f64,
    pub g_tp: f64,
    pub g_pp: f64,
}

/// Exact algebraic Legendre transform from a Gibbs function at (T, p) to `(ρ, Bundle)` at (T, ρ = 1/g_p):
/// the states-of-matter seam (D10). `r` only scales the bundle; any positive constant gives the same
/// properties. Refuses mechanically unstable input (g_p ≤ 0 or g_pp ≥ 0).
pub fn bundle_from_gibbs(r: f64, t: f64, p: f64, g: &GibbsDerivs) -> Result<(f64, Bundle), Error> {
    if !(g.g_p > 0.0 && g.g_pp < 0.0) {
        return Err(DomainError::MechanicallyUnstable.into());
    }
    let v = g.g_p;
    let rt = r * t;
    let pv = p * v / rt;
    let b = Bundle {
        a00: (g.g - p * v) / rt,
        a10: (g.g - p * v - t * g.g_t) / rt,
        a01: pv,
        a20: t * (g.g_tt * g.g_pp - g.g_tp * g.g_tp) / (r * g.g_pp),
        a11: pv + v * g.g_tp / (r * g.g_pp),
        a02: -v * v / (g.g_pp * rt) - 2.0 * pv,
    };
    Ok((1.0 / v, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Monatomic ideal gas: A01 = 1, A02 = −1, A20 = −3/2, A11 = 0.
    #[test]
    fn ideal_gas_limit_and_jacobian_identities() {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let (r, t, rho) = (8.314462618, 300.0, 40.0);
        assert_eq!(pressure(r, t, rho, &b), rho * r * t);
        assert!((cp(r, &b) - 2.5 * r).abs() < 1e-12);
        let at = At { r, t, rho, molar_mass: 0.04, ds: 0.0, b: &b };
        let d = |of, wrt, c| first_partial(of, wrt, c, &at).unwrap();
        use DerivVar::*;
        assert!((d(Hmolar, T, P) - cp(r, &b)).abs() < 1e-12); // (∂h/∂T)_p = cp
        assert!((d(Smolar, T, P) - cp(r, &b) / t).abs() < 1e-14); // (∂s/∂T)_p = cp/T
        assert!((d(Dmolar, P, T) - 1.0 / (r * t)).abs() < 1e-18); // ideal gas: (∂ρ/∂p)_T = 1/RT
        assert!((d(Gmolar, P, T) - 1.0 / rho).abs() < 1e-15); // (∂g/∂p)_T = v
        assert_eq!(first_partial(T, T, T, &at), None);
    }
}
