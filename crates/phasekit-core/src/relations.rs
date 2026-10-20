//! L1 relations: properties as pure functions of the order-2 total bundle (materials S1). Written once for
//! every potential: Helmholtz families directly, Gibbs families after [`bundle_from_gibbs`]. Molar SI.
//! Crate-private except the Gibbs seam: users reach these through `State` and `Fluid::prop`.

use crate::derivs::{Bundle, Derivs, SLOTS, idx};
use crate::error::{DomainError, Error};
use crate::model::Reducing;
use crate::num::math;
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

/// g = RT·(A00 + A01): for a pure fluid also the chemical potential μ = (∂(nA)/∂n)_{T,V} = a + p/ρ (ROT-062).
pub(crate) fn gibbs_energy(r: f64, t: f64, b: &Bundle) -> f64 {
    r * t * (b.a00 + b.a01)
}

/// a = RT·A00.
pub(crate) fn helmholtz_energy(r: f64, t: f64, b: &Bundle) -> f64 {
    r * t * b.a00
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

/// w² in (m/s)² = (cp/cv)·(∂p/∂ρ)_T / M = RT·(2·A01 + A02 + (A01 − A11)²/(−A20)) / M, with M in kg/mol. The sum
/// never divides by (∂p/∂ρ)_T, so it keeps its digits near the spinodal, where (cp/cv)·(∂p/∂ρ)_T loses them (M5.1).
/// Non-finite where cv diverges (the caller reports `Undefined`).
pub(crate) fn speed_of_sound_squared(r: f64, t: f64, molar_mass: f64, b: &Bundle) -> f64 {
    if !b.a20.is_finite() {
        return f64::NAN;
    }
    let x = b.a01 - b.a11;
    r * t * (2.0 * b.a01 + b.a02 - x * x / b.a20) / molar_mass
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
    /// The model's order-3 total there, which the derivatives of c_v, c_p and w read.
    pub(crate) third: Option<&'a Derivs>,
    /// The published reducing state, at which τ and δ are taken.
    pub(crate) reducing: Option<Reducing>,
}

/// `(∂X/∂T)_ρ` and `(∂X/∂ρ)_T` of a derivative variable (CoolProp's `get_dT_drho`, map 01 §4a); `None` for c_v, c_p and
/// w without the order-3 total and for τ and δ without the reducing state.
fn dt_drho(x: DerivVar, at: &At<'_>) -> Option<(f64, f64)> {
    let (r, t, rho, b) = (at.r, at.t, at.rho, at.b);
    let (rt_rho, m) = (r * t / rho, at.molar_mass);
    let (h, s, u, g) = (
        (r * (b.a01 - b.a20 - b.a11), rt_rho * (b.a11 + b.a01 + b.a02)),
        (-r * b.a20 / t, r / rho * (b.a11 - b.a01)),
        (-r * b.a20, rt_rho * b.a11),
        (r * (b.a00 + b.a01 - b.a10 - b.a11) - at.ds, rt_rho * (2.0 * b.a01 + b.a02)),
    );
    let mass = |(a, b): (f64, f64)| (a / m, b / m);
    Some(match x {
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
        DerivVar::Cvmolar | DerivVar::Cvmass | DerivVar::Cpmolar | DerivVar::Cpmass | DerivVar::SpeedOfSound => {
            return second_order_dt_drho(x, at);
        }
        DerivVar::Tau => (-at.reducing?.t / (t * t), 0.0),
        DerivVar::Delta => (0.0, 1.0 / at.reducing?.rho),
    })
}

/// A value with its logarithmic derivatives `T(∂/∂T)_ρ` and `ρ(∂/∂ρ)_T`, carried through arithmetic by the sum, product
/// and quotient rules: c_v, c_p and w differentiated through their relations in the `A_ij`.
#[derive(Clone, Copy, Debug)]
struct Logs {
    v: f64,
    t: f64,
    r: f64,
}

impl Logs {
    /// `Σ c_ij A_ij` and its two logarithmic derivatives at the order-3 total `d`; `None` past its order.
    fn of(terms: &[(u8, u8, f64)], d: &Derivs) -> Option<Logs> {
        let l = Lin::of(terms);
        Some(Logs { v: l.value(d)?, t: l.t_log().value(d)?, r: l.rho_log().value(d)? })
    }

    /// `k·self`.
    fn scale(self, k: f64) -> Logs {
        Logs { v: k * self.v, t: k * self.t, r: k * self.r }
    }

    /// `√self`.
    fn sqrt(self) -> Logs {
        let v = math::sqrt(self.v);
        Logs { v, t: self.t / (2.0 * v), r: self.r / (2.0 * v) }
    }

    /// `self + o`.
    fn plus(self, o: Logs) -> Logs {
        Logs { v: self.v + o.v, t: self.t + o.t, r: self.r + o.r }
    }

    /// `self − o`.
    fn minus(self, o: Logs) -> Logs {
        self.plus(o.scale(-1.0))
    }

    /// `self·o`.
    fn times(self, o: Logs) -> Logs {
        Logs { v: self.v * o.v, t: self.t * o.v + self.v * o.t, r: self.r * o.v + self.v * o.r }
    }

    /// `self/o`.
    fn over(self, o: Logs) -> Logs {
        let q = self.v / o.v;
        Logs { v: q, t: (self.t - q * o.t) / o.v, r: (self.r - q * o.r) / o.v }
    }
}

/// `(∂X/∂T)_ρ` and `(∂X/∂ρ)_T` of c_v, c_p or w from the order-3 total (`At::third`): with x = A01 − A11 and
/// D = 2·A01 + A02, cv = −R·A20, cp = cv + R·x²/D and w² = RT·(D − x²/A20)/M, as [`cv`], [`cp`] and
/// [`speed_of_sound_squared`] take them from the bundle; a mass basis divides by M. `None` without the order-3 total or
/// past its order; not finite where the relation is not (w² < 0, cp at (∂p/∂ρ)_T = 0).
fn second_order_dt_drho(var: DerivVar, at: &At<'_>) -> Option<(f64, f64)> {
    let d = at.third?;
    let a20 = Logs::of(&[(2, 0, 1.0)], d)?;
    let x = Logs::of(&[(0, 1, 1.0), (1, 1, -1.0)], d)?;
    let den = Logs::of(&[(0, 1, 2.0), (0, 2, 1.0)], d)?;
    let cv = a20.scale(-at.r);
    let value = match molar(var) {
        DerivVar::Cvmolar => cv,
        DerivVar::Cpmolar => cv.plus(x.times(x).over(den).scale(at.r)),
        // w² = (R/M)·T·(D − x²/A20), where T(∂T/∂T) = T.
        _ => {
            let t = Logs { v: at.t, t: at.t, r: 0.0 };
            t.times(den.minus(x.times(x).over(a20))).scale(at.r / at.molar_mass).sqrt()
        }
    };
    let per = if molar(var) == var { 1.0 } else { 1.0 / at.molar_mass };
    Some((per * value.t / at.t, per * value.r / at.rho))
}

/// `(∂of/∂wrt)_at` by the Jacobian ratio in (T, ρ): `[X_T Z_ρ − X_ρ Z_T] / [Y_T Z_ρ − Y_ρ Z_T]`
/// (map 01 §4a, U8). `None` where the denominator vanishes (e.g. `(∂T/∂T)_T`). One quantity in two bases makes its
/// Jacobian vanish only up to rounding, so it is decided before the ratio (PLAN.md M7.1, found by
/// `a_guess_changes_no_output`): `None` where `wrt` and `at` measure one quantity (`(∂T/∂u_molar)_u_mass`), and exactly
/// 0 where `of` and `at` do (`(∂u_molar/∂T)_u_mass`).
pub(crate) fn first_partial(of: DerivVar, wrt: DerivVar, at_const: DerivVar, at: &At<'_>) -> Option<f64> {
    if molar(wrt) == molar(at_const) {
        return None;
    }
    if molar(of) == molar(at_const) {
        return Some(0.0);
    }
    let ((xt, xr), (yt, yr), (zt, zr)) = (dt_drho(of, at)?, dt_drho(wrt, at)?, dt_drho(at_const, at)?);
    let den = yt * zr - yr * zt;
    let v = (xt * zr - xr * zt) / den;
    (den != 0.0 && v.is_finite()).then_some(v)
}

/// The molar variable of the quantity `v` measures (`Dmass` → `Dmolar`, …); T, p, w, τ and δ as they are.
fn molar(v: DerivVar) -> DerivVar {
    match v {
        DerivVar::Dmass => DerivVar::Dmolar,
        DerivVar::Hmass => DerivVar::Hmolar,
        DerivVar::Smass => DerivVar::Smolar,
        DerivVar::Umass => DerivVar::Umolar,
        DerivVar::Gmass => DerivVar::Gmolar,
        DerivVar::Cvmass => DerivVar::Cvmolar,
        DerivVar::Cpmass => DerivVar::Cpmolar,
        v => v,
    }
}

/// The `(i, j)` of `A_ij` to order 3.
const ORDER_3: [(u8, u8); 10] = [(0, 0), (1, 0), (0, 1), (2, 0), (1, 1), (0, 2), (3, 0), (2, 1), (1, 2), (0, 3)];

/// A linear combination `Σ c_ij A_ij` of the scaled derivatives of α to order 3 (slots as in `Derivs`).
#[derive(Clone, Copy, Debug)]
struct Lin([f64; SLOTS]);

impl Lin {
    /// `Σ c A_ij` over `terms`.
    fn of(terms: &[(u8, u8, f64)]) -> Lin {
        let mut lin = Lin([0.0; SLOTS]);
        terms.iter().for_each(|&(i, j, c)| lin.add(i, j, c));
        lin
    }

    /// `c_ij += c`.
    fn add(&mut self, i: u8, j: u8, c: f64) {
        if let Some(slot) = self.0.get_mut(idx(usize::from(i), usize::from(j))) {
            *slot += c;
        }
    }

    /// `c_ij`.
    fn coef(&self, i: u8, j: u8) -> f64 {
        self.0.get(idx(usize::from(i), usize::from(j))).copied().unwrap_or(0.0)
    }

    /// `self + k·other`.
    fn plus(mut self, k: f64, other: &Lin) -> Lin {
        ORDER_3.iter().for_each(|&(i, j)| self.add(i, j, k * other.coef(i, j)));
        self
    }

    /// `T(∂/∂T)_ρ`, which takes `A_ij` to `−(i·A_ij + A_(i+1)j)` (`τ∂/∂τ = −T∂/∂T`).
    fn t_log(&self) -> Lin {
        let mut out = Lin([0.0; SLOTS]);
        for (i, j) in ORDER_3 {
            let c = self.coef(i, j);
            out.add(i, j, -f64::from(i) * c);
            out.add(i.saturating_add(1), j, -c);
        }
        out
    }

    /// `ρ(∂/∂ρ)_T`, which takes `A_ij` to `j·A_ij + A_i(j+1)` (`δ∂/∂δ = ρ∂/∂ρ`).
    fn rho_log(&self) -> Lin {
        let mut out = Lin([0.0; SLOTS]);
        for (i, j) in ORDER_3 {
            let c = self.coef(i, j);
            out.add(i, j, f64::from(j) * c);
            out.add(i, j.saturating_add(1), c);
        }
        out
    }

    /// `Σ c_ij A_ij` with the values of `d`; `None` past its order.
    fn value(&self, d: &Derivs) -> Option<f64> {
        ORDER_3
            .iter()
            .filter(|&&(i, j)| self.coef(i, j) != 0.0)
            .try_fold(0.0, |sum, &(i, j)| d.get(usize::from(i), usize::from(j)).map(|a| sum + self.coef(i, j) * a))
    }
}

/// `[(∂²X/∂T²)_ρ, ∂²X/∂T∂ρ, (∂²X/∂ρ²)_T]` of a first-order state function from the order-3 total `d` (CoolProp's
/// `get_dT_drho_second_derivatives`): `X = R·T^m·ρ^n·L` with `L` linear in the `A_ij` (p: `TρA01`; h: `T(A10 + A01)`;
/// s: `A10 − A00`; u: `TA10`; g: `T(A00 + A01)`), differentiated through `T∂/∂T` and `ρ∂/∂ρ`. T, ρ and δ have none, τ =
/// T_r/T only (∂²τ/∂T²)_ρ = 2T_r/T³; a mass-basis quantity is its molar one over M, and a gauge term is at most linear in
/// T. `None` past `d`'s order, for τ without the reducing state, and for c_v, c_p and w, whose are order 4.
fn dt_drho2(x: DerivVar, at: &At<'_>, d: &Derivs) -> Option<[f64; 3]> {
    let (m, n, l) = match molar(x) {
        DerivVar::P => (1, 1, Lin::of(&[(0, 1, 1.0)])),
        DerivVar::Hmolar => (1, 0, Lin::of(&[(1, 0, 1.0), (0, 1, 1.0)])),
        DerivVar::Smolar => (0, 0, Lin::of(&[(1, 0, 1.0), (0, 0, -1.0)])),
        DerivVar::Umolar => (1, 0, Lin::of(&[(1, 0, 1.0)])),
        DerivVar::Gmolar => (1, 0, Lin::of(&[(0, 0, 1.0), (0, 1, 1.0)])),
        DerivVar::T | DerivVar::Dmolar | DerivVar::Delta => return Some([0.0; 3]),
        DerivVar::Tau => return Some([2.0 * at.reducing?.t / (at.t * at.t * at.t), 0.0, 0.0]),
        // c_v, c_p and w; `molar` returns no mass-basis variable.
        _ => return None,
    };
    let per = if molar(x) == x { 1.0 } else { 1.0 / at.molar_mass };
    let (t, rho, fm, fn_) = (at.t, at.rho, f64::from(m), f64::from(n));
    let x0 = at.r * math::powi(t, m) * math::powi(rho, n) * per;
    let (lt, lr) = (l.t_log(), l.rho_log());
    let tt = Lin::of(&[]).plus(fm * fm - fm, &l).plus(2.0 * fm - 1.0, &lt).plus(1.0, &lt.t_log());
    let tr = Lin::of(&[]).plus(fn_ * fm, &l).plus(fn_, &lt).plus(fm, &lr).plus(1.0, &lr.t_log());
    let rr = Lin::of(&[]).plus(fn_ * fn_ - fn_, &l).plus(2.0 * fn_ - 1.0, &lr).plus(1.0, &lr.rho_log());
    Some([x0 / (t * t) * tt.value(d)?, x0 / (t * rho) * tr.value(d)?, x0 / (rho * rho) * rr.value(d)?])
}

/// `(∂/∂wrt₂ (∂of/∂wrt₁)_at₁)_at₂` from the order-3 total `d`: the first partial's ratio `N/D` (see
/// [`first_partial`]) differentiated in (T, ρ) and divided by `wrt₂`'s Jacobian at constant `at₂` (CoolProp's
/// `calc_second_partial_deriv`). `None` where either derivative holds the quantity it differentiates by, or the result
/// is not finite (a vanishing denominator, or the critical point's order-3 entries); exactly 0 where `of` and `at₁`
/// measure one quantity (the first partial is then 0).
pub(crate) fn second_partial(
    (of, wrt1, at1): (DerivVar, DerivVar, DerivVar),
    (wrt2, at2): (DerivVar, DerivVar),
    at: &At<'_>,
    d: &Derivs,
) -> Option<f64> {
    if molar(wrt1) == molar(at1) || molar(wrt2) == molar(at2) {
        return None;
    }
    if molar(of) == molar(at1) {
        return Some(0.0);
    }
    let ((xt, xr), (yt, yr), (zt, zr)) = (dt_drho(of, at)?, dt_drho(wrt1, at)?, dt_drho(at1, at)?);
    let ([xtt, xtr, xrr], [ytt, ytr, yrr], [ztt, ztr, zrr]) =
        (dt_drho2(of, at, d)?, dt_drho2(wrt1, at, d)?, dt_drho2(at1, at, d)?);
    let (num, den) = (xt * zr - xr * zt, yt * zr - yr * zt);
    let (num_t, num_r) = (xtt * zr + xt * ztr - xtr * zt - xr * ztt, xtr * zr + xt * zrr - xrr * zt - xr * ztr);
    let (den_t, den_r) = (ytt * zr + yt * ztr - ytr * zt - yr * ztt, ytr * zr + yt * zrr - yrr * zt - yr * ztr);
    let (f_t, f_r) = ((num_t * den - num * den_t) / (den * den), (num_r * den - num * den_r) / (den * den));
    let ((wt, wr), (vt, vr)) = (dt_drho(wrt2, at)?, dt_drho(at2, at)?);
    let v = (f_t * vr - f_r * vt) / (wt * vr - wr * vt);
    v.is_finite().then_some(v)
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
        let at = At { r, t, rho, molar_mass: 0.04, ds: 0.0, b: &b, third: None, reducing: None };
        let d = |of, wrt, c| first_partial(of, wrt, c, &at).unwrap();
        use DerivVar::*;
        assert!((d(Hmolar, T, P) - cp(r, &b)).abs() < 1e-12); // (∂h/∂T)_p = cp
        assert!((d(Smolar, T, P) - cp(r, &b) / t).abs() < 1e-14); // (∂s/∂T)_p = cp/T
        assert!((d(Dmolar, P, T) - 1.0 / (r * t)).abs() < 1e-18); // ideal gas: (∂ρ/∂p)_T = 1/RT
        assert!((d(Gmolar, P, T) - 1.0 / rho).abs() < 1e-15); // (∂g/∂p)_T = v
        assert_eq!(first_partial(T, T, T, &at), None);
        // One quantity in two bases held and varied: undefined, never the ratio of two rounding errors; held and
        // differentiated: exactly 0. Differentiated in one basis by the other, the basis only scales.
        let bases = [(Dmolar, Dmass), (Hmolar, Hmass), (Smolar, Smass), (Umolar, Umass), (Gmolar, Gmass)];
        for (molar, mass) in bases {
            for (wrt, c) in [(molar, mass), (mass, molar), (molar, molar), (mass, mass)] {
                assert_eq!(first_partial(T, wrt, c, &at), None, "{wrt:?} at {c:?}");
            }
            assert_eq!([first_partial(molar, P, mass, &at), first_partial(mass, P, molar, &at)], [Some(0.0); 2]);
        }
        assert!((d(Dmolar, Dmass, T) - 25.0).abs() < 1e-12); // 1/M
    }

    /// On the spinodal, (∂p/∂ρ)_T = 0: cp diverges, but w² = RT·(2·A01 + A02 + (A01 − A11)²/(−A20))/M stays finite,
    /// where (cp/cv)·(∂p/∂ρ)_T would be ∞·0. A divergent cv leaves it undefined (E17).
    #[test]
    fn speed_of_sound_is_finite_on_the_spinodal() {
        let b = Bundle { a00: -0.5, a10: 1.2, a01: 0.4, a20: -3.0, a11: 0.1, a02: -0.8 };
        let (r, t, m) = (8.314462618, 300.0, 0.04);
        assert_eq!(dp_drho_t(r, t, &b), 0.0);
        assert!(cp(r, &b).is_infinite());
        let w2 = r * t * 0.09 / 3.0 / m;
        assert!((speed_of_sound_squared(r, t, m, &b) - w2).abs() < 1e-15 * w2);
        assert!(speed_of_sound_squared(r, t, m, &Bundle { a20: f64::NEG_INFINITY, ..b }).is_nan());
    }

    /// ROT-062 (map 02 §6 row 1): CoolProp takes a pure fluid's chemical potential through its mixture code at the
    /// critical, not the reducing, constants (Methanol +83 J/mol against g). Here μ = (∂(nA)/∂n)_{T,V}, by central
    /// differences of n·a(T, nρ) from the model's own α⁰ + α^r (relative step 1e-5), equals the state's g = RT(A00 +
    /// A01) within the FD class (1e-7 of max(|g|, RT)), for fluids whose reducing state is not their critical point.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn mu_equals_molar_gibbs() {
        use crate::{Basis, DataSet, Density, DomainPolicy, FlashOptions, Input, Order, Phase, Registry, Temperature};
        // A private registry: the process-wide one must stay unloaded for a registry test.
        let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
        let opts = FlashOptions::new().with_phase(Phase::Gas).with_domain(DomainPolicy::Extrapolate);
        for name in ["Methanol", "MDM", "Air", "R134a", "Helium"] {
            let fluid = registry.get(name).unwrap();
            let eos = fluid.model().helmholtz().unwrap();
            let c = fluid.model().critical_point().unwrap();
            let r = eos.gas_constant();
            for (t, rho) in [(1.1 * c.t, 0.5 * c.rho), (1.1 * c.t, 1.5 * c.rho), (0.9 * c.t, 1e-3 * c.rho)] {
                let input = Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
                let g = fluid.flash(input, &opts).unwrap().g(Basis::Molar);
                let a = |n: f64| {
                    let alpha = eos.ideal(t, n * rho, Order::One) + eos.residual(t, n * rho, Order::One);
                    n * r * t * alpha.get(0, 0).unwrap()
                };
                let h = 1e-5;
                let mu = (a(1.0 + h) - a(1.0 - h)) / (2.0 * h);
                assert!((mu - g).abs() <= 1e-7 * g.abs().max(r * t), "{name} at {t} K, {rho} mol/m³: μ {mu}, g {g}");
            }
        }
    }
}
