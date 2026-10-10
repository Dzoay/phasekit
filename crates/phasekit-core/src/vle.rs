//! The pure-fluid VLE (PLAN.md M6.3; D6; map 04 U3; map 03 §3.4, §6): saturation of a Helmholtz model at a given T, by
//! Newton in (ln ρ′, ln ρ″), or at a given p, by Newton in (T, ln ρ′, ln ρ″). The Jacobian is exact, from the model's
//! order-2 bundle at each phase (the derivatives `Jet4` computed), and `num::solve_small` solves it; a backtracking line
//! search keeps every step inside the model's domain and lowering the residual. When Newton fails, nested bracketed
//! solves around the seeds take over (TOMS 748). A converged answer passes a residual gate (p′ = p″ to rounding,
//! g′ = g″, two distinct phases) or is refused: CoolProp's VLE routines exit on a small step with a large residual and
//! can return the trivial solution ρ′ = ρ″ (map 03 §6, map 04 U3).

use crate::derivs::{Bundle, Derivs, Order};
use crate::error::Error;
use crate::helmholtz::HelmholtzModel;
use crate::num::{math, solve_small};
use crate::roots::{Root, Stop, Tol, newton_converged, toms748};
use crate::saturation::{SatPair, SatSide};
use crate::state::Strategy;

/// Newton iterations allowed.
const MAX_ITER: u16 = 50;

/// Converged when no unknown moves by more than this (in ln ρ, and relative in T).
const STEP_TOL: f64 = 1e-13;

/// The residual gate: |p′ − p″|/RT within this of the pressures' cancellation scale ρ·(1 + |A01|) summed over the
/// phases, and |g′ − g″|/RT within it of 1 + |g/RT|. A liquid's p/RT = ρ′·A01 is what is left of ρ′ after 1 + A01^r
/// cancels (A01 ≈ 1e-5 for D4 at 294 K), so its rounding is relative to ρ′, not to p; a stall leaves a residual of
/// the order of p itself.
const GATE: f64 = 1e-10;

/// A residual this small (relative to the scales of [`GATE`]) is at the rounding floor: a line-search step that keeps
/// it there is taken even if it does not lower it, and Newton may stop there ([`converged`]).
const FLOOR: f64 = 1e-13;

/// The domain check allows densities this far above the model's largest density. That was its triple-point liquid's
/// until M7.1, which water's and heavy water's saturated liquids exceed just above the triple point (by up to 1.3e-4
/// and 5.4e-4, their density maximum); it is ρ(T_min, p_max) since, far above every saturated liquid, and the margin
/// stays for a seed a little past it.
const RHO_MAX_MARGIN: f64 = 1.01;

/// The largest change of ln ρ in one step, and of T relative.
const MAX_STEP: (f64, f64) = (0.5, 0.02);

/// The bracketed fallback's half-widths around the seeds, in ln ρ′ and ln ρ″, and relative in T. Near Tc neither
/// density window is more than an eighth of ln(ρ′/ρ″): an analytic EOS is mean-field there, its spinodal densities
/// 1/√3 of the binodal's distance from ρc, about 0.21·ln(ρ′/ρ″) inside each phase, and a window reaching past one meets
/// the unstable branch (Water at Θ = 0.02 with a quarter).
const WINDOW: (f64, f64, f64) = (0.05, 0.5, 0.01);

/// Iterations allowed to each bracketed solve.
const BRACKET_ITER: u16 = 100;

/// [`from_eos`]'s tolerance on a spinodal's ln ρ: it only bounds a branch, so a loose one serves.
const SPINODAL_TOL: f64 = 1e-10;

/// [`from_eos`]'s tolerance on the Maxwell pressure's ln p: [`at_t`] polishes its seeds to rounding.
const MAXWELL_TOL: f64 = 1e-10;

/// The bracketed solves' tolerance on ln ρ, and relative on T.
const BRACKET_TOL: f64 = 1e-15;

/// One phase at (T, ρ), with the total (ideal + residual) order-2 bundle there: p/RT = ρ·A01 and g/RT = A00 + A01, whose
/// derivatives with respect to ln ρ are ρ·(2A01 + A02) and 2A01 + A02, and with respect to T at fixed ρ
/// ∂p/∂T = ρR·(A01 − A11) and ∂(g/RT)/∂T = −(A10 + A11)/T.
#[derive(Clone, Copy, Debug)]
struct Point {
    rho: f64,
    b: Bundle,
}

impl Point {
    /// The phase at (T, ρ) if the model's bundle there is finite and ρ is at most the model's largest density (with
    /// [`RHO_MAX_MARGIN`]). A ρ ≤ 0 or NaN fails both: the ideal part's ln δ is not finite there.
    fn at(eos: &dyn HelmholtzModel, t: f64, rho: f64) -> Option<Point> {
        let d = eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two);
        let b = d.bundle()?;
        let finite = [b.a00, b.a10, b.a01, b.a20, b.a11, b.a02].iter().all(|v| v.is_finite());
        (finite && rho <= eos.rho_max(t) * RHO_MAX_MARGIN).then_some(Point { rho, b })
    }

    /// p/RT (mol/m³).
    fn p_rt(&self) -> f64 {
        self.rho * self.b.a01
    }

    /// g/RT.
    fn g_rt(&self) -> f64 {
        self.b.a00 + self.b.a01
    }

    /// The scale p/RT cancels from: ρ·(1 + |A01|).
    fn p_scale(&self) -> f64 {
        self.rho * (1.0 + self.b.a01.abs())
    }

    /// ∂(g/RT)/∂ln ρ at fixed T; ∂(p/RT)/∂ln ρ is ρ times it.
    fn stiffness(&self) -> f64 {
        2.0 * self.b.a01 + self.b.a02
    }
}

/// The failure every path reports: the VLE did not converge.
fn failed(iterations: u16) -> Error {
    Error::NoConvergence { strategy: Strategy::Vle, iterations }
}

/// The residual gate and the answer: p′ = p″ and g′ = g″ within [`GATE`] of their scales and two distinct phases, else
/// no convergence. p is the vapour side's, the better conditioned.
fn answer(r: f64, t: f64, liquid: Point, vapour: Point, iterations: u16) -> Result<SatPair, Error> {
    let (pl, pv) = (liquid.p_rt(), vapour.p_rt());
    let pressure_ok = (pl - pv).abs() <= GATE * (liquid.p_scale() + vapour.p_scale());
    let gibbs_ok = (liquid.g_rt() - vapour.g_rt()).abs() <= GATE * (1.0 + liquid.g_rt().abs());
    if !(pressure_ok && gibbs_ok && distinct(&liquid, &vapour) && pv > 0.0) {
        return Err(failed(iterations));
    }
    let p = pv * r * t;
    Ok(SatPair { bubble: SatSide { t, p, rho: liquid.rho }, dew: SatSide { t, p, rho: vapour.rho } })
}

/// Whether the two phases are distinct, ln(ρ′/ρ″) > 1e-6: the trivial solution ρ′ = ρ″ satisfies both equations.
fn distinct(liquid: &Point, vapour: &Point) -> bool {
    math::ln(liquid.rho / vapour.rho) > 1e-6
}

/// Whether Newton has converged, its next step `length` long at a residual of `size` after a step `previous` long:
/// the step is below [`STEP_TOL`], or the residual is at [`FLOOR`] and the step no longer halves, what is left being
/// rounding. Near the critical point the steps are the residuals' rounding divided by a small stiffness (1e-12 for
/// Water at Θ = 5e-4) and never fall below [`STEP_TOL`]; once they stop shrinking, every step that still improved the
/// answer has been taken.
fn converged(length: f64, size: f64, previous: f64) -> bool {
    newton_converged(length, size, previous, (STEP_TOL, FLOOR))
}

/// The largest step multiple, at most 1, that moves no unknown by more than its limit: `moves` holds each unknown's
/// change under the full step with its limit ([`MAX_STEP`]).
fn step_scale<const N: usize>(moves: [(f64, f64); N]) -> f64 {
    let largest = moves.iter().fold(0.0_f64, |m, (change, limit)| m.max(change.abs() / limit));
    (1.0 / largest).min(1.0)
}

/// A backtracking line search: from λ = `scale`, halved up to 10 times, the first point `at(λ)` the model accepts whose
/// residual size is below `size`, or at [`FLOOR`]; `None` if there is none.
fn backtrack<P>(scale: f64, size: f64, mut at: impl FnMut(f64) -> Option<(P, f64)>) -> Option<P> {
    let mut lambda = scale;
    for _ in 0..=10 {
        if let Some((point, next)) = at(lambda)
            && (next < size || next <= FLOOR)
        {
            return Some(point);
        }
        lambda /= 2.0;
    }
    None
}

/// The Jacobian at a given T: rows (p/RT, g/RT) equality, columns (ln ρ′, ln ρ″).
fn jacobian_t(liquid: &Point, vapour: &Point) -> [[f64; 2]; 2] {
    let (kl, kv) = (liquid.stiffness(), vapour.stiffness());
    [[liquid.rho * kl, -vapour.rho * kv], [kl, -kv]]
}

/// The residuals at a given T, (p′ − p″)/RT and (g′ − g″)/RT, and their size relative to their scales.
fn residual_t(liquid: &Point, vapour: &Point) -> ([f64; 2], f64) {
    let r = [liquid.p_rt() - vapour.p_rt(), liquid.g_rt() - vapour.g_rt()];
    let size = (r[0] / (liquid.p_scale() + vapour.p_scale())).abs().max((r[1] / (1.0 + liquid.g_rt().abs())).abs());
    (r, size)
}

/// The residuals at the pressure `p` and temperature `t`, p′/p − 1, p″/p − 1 and (g′ − g″)/RT, and their size, each
/// relative to its cancellation scale (see [`GATE`]); `r` is the gas constant.
fn residual_p(r: f64, t: f64, p: f64, l: &Point, v: &Point) -> ([f64; 3], f64) {
    let e = [l.p_rt() * r * t / p - 1.0, v.p_rt() * r * t / p - 1.0, l.g_rt() - v.g_rt()];
    let p_rt = p / (r * t);
    let sizes = [e[0] * p_rt / l.p_scale(), e[1] * p_rt / v.p_scale(), e[2] / (1.0 + l.g_rt().abs())];
    (e, sizes.iter().fold(0.0_f64, |m, x| m.max(x.abs())))
}

/// The Jacobian of [`residual_p`], columns (T, ln ρ′, ln ρ″): T·∂/∂T at fixed ρ of ρRT·A01 is ρRT·(A01 − A11), of g/RT
/// −(A10 + A11).
fn jacobian_p(r: f64, t: f64, p: f64, l: &Point, v: &Point) -> [[f64; 3]; 3] {
    [
        [l.rho * r * (l.b.a01 - l.b.a11) / p, l.rho * r * t * l.stiffness() / p, 0.0],
        [v.rho * r * (v.b.a01 - v.b.a11) / p, 0.0, v.rho * r * t * v.stiffness() / p],
        [-((l.b.a10 + l.b.a11) - (v.b.a10 + v.b.a11)) / t, l.stiffness(), -v.stiffness()],
    ]
}

/// The saturation at T of a model with no curve that covers T (PLAN.md M7.2; ARCHITECTURE.md D6: hint → critical point
/// → curve → generic pure VLE), seeded from the EOS alone. Below Tc the isotherm loops around the critical density
/// `rho_c`: p rises to the vapour spinodal, falls, and rises again from the liquid spinodal, the zeros of (∂p/∂ρ)_T
/// nearest the dilute gas and the model's largest density. Between the spinodal pressures the liquid's and the vapour's
/// Gibbs energies cross once, since d(g′ − g″)/dp = v′ − v″ < 0: a Maxwell construction in ln p, each side's density
/// bracketed on its branch ([`crate::density::bracketed`]), seeds [`at_t`]. The pressure's lower end is the liquid
/// spinodal's, or 1e-40 of the vapour spinodal's where that is not positive (a liquid under tension).
/// `NoConvergence { Vle }` when the loop is not there: no turn before `rho_c` from either side.
pub(crate) fn from_eos(eos: &dyn HelmholtzModel, t: f64, rho_c: f64) -> Result<SatPair, Error> {
    let rt = eos.gas_constant() * t;
    // p, (∂p/∂ρ)_T and g/RT up to a function of T alone (ln ρ + α^r + Z), from `residual + IDEAL_DELTA`.
    let mech = |rho: f64| {
        let b = (eos.residual(t, rho, Order::Two) + Derivs::IDEAL_DELTA).bundle();
        b.map_or([f64::NAN; 3], |b| [rho * rt * b.a01, rt * (2.0 * b.a01 + b.a02), math::ln(rho) + b.a00 + b.a01])
    };
    // The first turn of p(ρ) from `from` towards `to`: steps of 25 % in ρ until (∂p/∂ρ)_T is no longer positive, then
    // TOMS 748 in that step. Inside the loop a multiparameter EOS can oscillate (R22's p swings ±1e16 Pa at 121 K), so
    // each spinodal is the turn nearest its own phase.
    let turn = |from: f64, to: f64| -> Result<f64, Error> {
        let (factor, up) = if to > from { (1.25, true) } else { (0.8, false) };
        let past = |rho: f64| if up { rho >= to } else { rho <= to };
        let mut a = from;
        while !past(a) {
            let b = if past(a * factor) { to } else { a * factor };
            if mech(b)[1] <= 0.0 {
                let (lo, hi) = (math::ln(a.min(b)), math::ln(a.max(b)));
                let root = toms748(|u| mech(math::exp(u))[1], lo, hi, Tol::Absolute(SPINODAL_TOL), BRACKET_ITER);
                return root.converged(Strategy::Vle).map(|r| math::exp(r.x));
            }
            a = b;
        }
        Err(failed(0))
    };
    let (vapour, liquid) = (turn(1e-9 * rho_c, rho_c)?, turn(eos.rho_max(t), rho_c)?);
    // Strictly between the spinodal pressures: at either one a side's root is its spinodal, where (∂p/∂ρ)_T = 0.
    let p_hi = mech(vapour)[0] * (1.0 - 1e-9);
    let p_lo = mech(liquid)[0].max(1e-40 * p_hi) * (1.0 + 1e-9);
    if !(p_lo < p_hi) {
        return Err(failed(0));
    }
    let sides = |p: f64| {
        let ideal = p / rt;
        let gas = crate::density::bracketed(eos, t, p, ((1e-3 * ideal).min(1e-3 * vapour), vapour), ideal);
        let liq = crate::density::bracketed(eos, t, p, (liquid, eos.rho_max(t)), liquid);
        gas.and_then(|(v, _)| Ok((liq?.0, v)))
    };
    // g′ − g″ at p: positive at the liquid spinodal's pressure, negative at the vapour spinodal's.
    let gap = |u: f64| sides(math::exp(u)).map_or(f64::NAN, |(l, v)| mech(l)[2] - mech(v)[2]);
    let root = toms748(gap, math::ln(p_lo), math::ln(p_hi), Tol::Absolute(MAXWELL_TOL), BRACKET_ITER);
    at_t(eos, t, sides(math::exp(root.converged(Strategy::Vle)?.x))?)
}

/// Saturation of `eos` at `t` from the seed densities `(ρ′, ρ″)` (a superancillary's or an ancillary's).
pub(crate) fn at_t(eos: &dyn HelmholtzModel, t: f64, seeds: (f64, f64)) -> Result<SatPair, Error> {
    solve_t(eos, t, seeds, MAX_ITER)
}

/// [`at_t`] with `max_iter` Newton iterations, then the bracketed fallback; Newton's error if both fail.
fn solve_t(eos: &dyn HelmholtzModel, t: f64, seeds: (f64, f64), max_iter: u16) -> Result<SatPair, Error> {
    newton_t(eos, t, seeds, max_iter).or_else(|newton| bracketed_t(eos, t, seeds).map_err(|_| newton))
}

/// Newton at a given T.
fn newton_t(eos: &dyn HelmholtzModel, t: f64, (rho_l, rho_v): (f64, f64), max_iter: u16) -> Result<SatPair, Error> {
    let phase = |rho: f64| Point::at(eos, t, rho);
    let (mut liquid, mut vapour) = (phase(rho_l).ok_or(failed(0))?, phase(rho_v).ok_or(failed(0))?);
    let mut previous = f64::INFINITY;
    for iteration in 1..=max_iter {
        let (res, size) = residual_t(&liquid, &vapour);
        let step = solve_small(jacobian_t(&liquid, &vapour), [-res[0], -res[1]]).map_err(|_| failed(iteration))?;
        let length = step[0].abs().max(step[1].abs());
        if converged(length, size, previous) {
            // Converged: what is left moves no density by more than rounding.
            return answer(eos.gas_constant(), t, liquid, vapour, iteration);
        }
        let (ul, uv) = (math::ln(liquid.rho), math::ln(vapour.rho));
        let moved = |lambda: f64| {
            let (l, g) = (phase(math::exp(ul + lambda * step[0]))?, phase(math::exp(uv + lambda * step[1]))?);
            Some(((l, g), residual_t(&l, &g).1))
        };
        let scale = step_scale([(step[0], MAX_STEP.0), (step[1], MAX_STEP.0)]);
        (liquid, vapour) = backtrack(scale, size, moved).ok_or(failed(iteration))?;
        previous = length;
    }
    Err(failed(max_iter))
}

/// The fallback at a given T (map 04 U3), 1-D in pressure: each phase's density at a pressure is found in a window
/// around its seed (TOMS 748 on ln ρ, where p(ρ) is monotonic), and the pressure where the Gibbs energies agree in the
/// range both windows reach (TOMS 748 on p/RT; g′ − g″ changes sign once across it). A window that misses the root, or
/// one the model refuses, is no convergence; seeds in the wrong order give no common range.
fn bracketed_t(eos: &dyn HelmholtzModel, t: f64, (rho_l, rho_v): (f64, f64)) -> Result<SatPair, Error> {
    let phase = |rho: f64| Point::at(eos, t, rho);
    let eighth = math::ln(rho_l / rho_v) / 8.0;
    let (ul, uv) = (math::ln(rho_l), math::ln(rho_v));
    let (wl, wv) = (WINDOW.0.min(eighth), WINDOW.1.min(eighth));
    let liquid = (ul - wl, (ul + wl).min(math::ln(eos.rho_max(t))));
    let vapour = (uv - wv, uv + wv);
    // The phase in `window` (ln ρ) whose p/RT is `pressure`.
    let at = |(lo, hi): (f64, f64), pressure: f64| {
        let excess = |u: f64| phase(math::exp(u)).map_or(f64::NAN, |x| x.p_rt() - pressure);
        let root = toms748(excess, lo, hi, Tol::Absolute(BRACKET_TOL), BRACKET_ITER);
        (root.stop == Stop::Converged).then(|| phase(math::exp(root.x))).flatten()
    };
    let p_rt = |u: f64| phase(math::exp(u)).map_or(f64::NAN, |x| x.p_rt());
    let (low, high) = (p_rt(liquid.0).max(p_rt(vapour.0)), p_rt(liquid.1).min(p_rt(vapour.1)));
    let gibbs =
        |pressure: f64| at(liquid, pressure).zip(at(vapour, pressure)).map_or(f64::NAN, |(l, g)| l.g_rt() - g.g_rt());
    let root = toms748(gibbs, low, high, Tol::Relative(BRACKET_TOL), BRACKET_ITER);
    let pair = (root.stop == Stop::Converged).then(|| at(liquid, root.x).zip(at(vapour, root.x))).flatten();
    let (l, g) = pair.ok_or(failed(root.iterations))?;
    answer(eos.gas_constant(), t, l, g, root.iterations)
}

/// Saturation of `eos` at the pressure `p` from the seed `(T, ρ′, ρ″)`.
pub(crate) fn at_p(eos: &dyn HelmholtzModel, p: f64, seed: (f64, f64, f64)) -> Result<SatPair, Error> {
    solve_p(eos, p, seed, MAX_ITER)
}

/// [`at_p`] with `max_iter` Newton iterations, then the bracketed fallback; Newton's error if both fail.
fn solve_p(eos: &dyn HelmholtzModel, p: f64, seed: (f64, f64, f64), max_iter: u16) -> Result<SatPair, Error> {
    newton_p(eos, p, seed, max_iter).or_else(|newton| bracketed_p(eos, p, seed).map_err(|_| newton))
}

/// A bracketed solve's root, if it converged with its residual within [`GATE`]: a bracket that closed on a jump (the VLE
/// changing branch between two temperatures) converges with a large residual.
fn solved(root: Root) -> Option<f64> {
    (root.stop == Stop::Converged && root.f.abs() <= GATE).then_some(root.x)
}

/// The fallback at a given p: the T in a window around the seed's where ln(p_sat(T)/p) = 0 (TOMS 748), each p_sat by
/// [`solve_t`] from the seed densities; the answer reports p.
fn bracketed_p(eos: &dyn HelmholtzModel, p: f64, (t, rho_l, rho_v): (f64, f64, f64)) -> Result<SatPair, Error> {
    let at = |t: f64| solve_t(eos, t, (rho_l, rho_v), MAX_ITER).ok();
    let excess = |t: f64| at(t).map_or(f64::NAN, |sat| math::ln(sat.dew.p / p));
    let root = toms748(excess, t * (1.0 - WINDOW.2), t * (1.0 + WINDOW.2), Tol::Relative(BRACKET_TOL), BRACKET_ITER);
    let sat = solved(root).and_then(at).ok_or(failed(root.iterations))?;
    Ok(sat.at_pressure(p))
}

/// Newton at a given p.
fn newton_p(
    eos: &dyn HelmholtzModel,
    p: f64,
    (t, rho_l, rho_v): (f64, f64, f64),
    max_iter: u16,
) -> Result<SatPair, Error> {
    let r = eos.gas_constant();
    let phases = |t: f64, rho_l: f64, rho_v: f64| Some((Point::at(eos, t, rho_l)?, Point::at(eos, t, rho_v)?));
    let (mut t, (mut l, mut v)) = (t, phases(t, rho_l, rho_v).ok_or(failed(0))?);
    let mut previous = f64::INFINITY;
    for iteration in 1..=max_iter {
        let (e, size) = residual_p(r, t, p, &l, &v);
        let step = solve_small(jacobian_p(r, t, p, &l, &v), [-e[0], -e[1], -e[2]]).map_err(|_| failed(iteration))?;
        // The step's moves: T relative, ln ρ′, ln ρ″.
        let moves = [step[0] / t, step[1], step[2]];
        let length = moves.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        if converged(length, size, previous) {
            return Ok(answer(r, t, l, v, iteration)?.at_pressure(p));
        }
        let (ul, uv) = (math::ln(l.rho), math::ln(v.rho));
        let moved = |lambda: f64| {
            let tn = t + lambda * step[0];
            let (ln, vn) = phases(tn, math::exp(ul + lambda * step[1]), math::exp(uv + lambda * step[2]))?;
            Some(((tn, ln, vn), residual_p(r, tn, p, &ln, &vn).1))
        };
        let scale = step_scale([(moves[0], MAX_STEP.1), (moves[1], MAX_STEP.0), (moves[2], MAX_STEP.0)]);
        (t, l, v) = backtrack(scale, size, moved).ok_or(failed(iteration))?;
        previous = length;
    }
    Err(failed(max_iter))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "fluids-all")]
    use crate::data::FluidRecord;
    #[cfg(feature = "fluids-all")]
    use crate::{DataSet, Registry};

    /// The Jacobian [`at_t`] uses, for the test against differences.
    #[cfg(feature = "fluids-all")]
    fn jacobian_at(eos: &dyn HelmholtzModel, t: f64, rho_l: f64, rho_v: f64) -> Option<([[f64; 2]; 2], [f64; 2])> {
        let (l, v) = (Point::at(eos, t, rho_l)?, Point::at(eos, t, rho_v)?);
        Some((jacobian_t(&l, &v), residual_t(&l, &v).0))
    }

    /// The decoded `Parity` record of `name` and its compiled model.
    #[cfg(feature = "fluids-all")]
    fn model(name: &str) -> (FluidRecord, crate::fluid::PureFluid) {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let record = crate::internal::record(&registry, name).unwrap();
        (record.clone(), record.compile().unwrap())
    }

    #[cfg(feature = "fluids-all")]
    /// map 04 U3 (the AD Jacobian): the Jacobian from the model's order-2 bundle, whose derivatives are exact, equals
    /// central differences of the residuals in ln ρ′ and ln ρ″ within class `Fd` (1e-7), for Water at 450 K and
    /// CarbonDioxide at 280 K, at the superancillary's densities and off them.
    #[test]
    fn vle_jacobian_matches_ad() {
        for (name, t) in [("Water", 450.0), ("CarbonDioxide", 280.0)] {
            let (record, fluid) = model(name);
            let sat = record.superancillary_curve().unwrap().at_t(t).unwrap();
            let eos = fluid.eos();
            for (rl, rv) in [(sat.bubble.rho, sat.dew.rho), (sat.bubble.rho * 0.999, sat.dew.rho * 1.05)] {
                let (j, _) = jacobian_at(eos, t, rl, rv).unwrap();
                let h = 1e-6;
                let diff = |which: usize| {
                    let at = |s: f64| {
                        let (l, v) = if which == 0 { (rl * math::exp(s), rv) } else { (rl, rv * math::exp(s)) };
                        jacobian_at(eos, t, l, v).unwrap().1
                    };
                    let (up, down) = (at(h), at(-h));
                    [(up[0] - down[0]) / (2.0 * h), (up[1] - down[1]) / (2.0 * h)]
                };
                for (col, fd) in [diff(0), diff(1)].into_iter().enumerate() {
                    for (row, fd) in fd.into_iter().enumerate() {
                        let scale = j[row][col].abs().max(1e-300);
                        assert!((fd - j[row][col]).abs() <= 1e-7 * scale, "{name} {row}{col}: {} vs {fd}", j[row][col]);
                    }
                }
            }
        }
    }

    #[cfg(feature = "fluids-all")]
    /// map 03 §6, map 04 U3: a seed that makes the model non-finite, or both seeds in one phase (the trivial solution
    /// ρ′ = ρ″ satisfies both equations), is `NoConvergence { Vle }`, never an answer.
    #[test]
    fn vle_nan_or_stall_is_no_convergence() {
        let (record, fluid) = model("Water");
        let sat = record.superancillary_curve().unwrap().at_t(450.0).unwrap();
        let eos = fluid.eos();
        let refused =
            |r: Result<SatPair, Error>| matches!(r, Err(Error::NoConvergence { strategy: Strategy::Vle, .. }));
        assert!(refused(at_t(eos, 450.0, (f64::NAN, sat.dew.rho))));
        assert!(refused(at_t(eos, 450.0, (eos.rho_max(450.0) * 2.0, sat.dew.rho))), "beyond the model's density");
        assert!(refused(at_t(eos, 450.0, (sat.dew.rho * 1.0001, sat.dew.rho))), "both seeds in the vapour");
        assert!(refused(at_p(eos, sat.dew.p, (450.0, sat.dew.rho, sat.dew.rho * 1.0001))));
        assert!(at_t(eos, 450.0, (sat.bubble.rho, sat.dew.rho)).is_ok());
    }

    #[cfg(feature = "fluids-all")]
    /// map 04 U3 (the bracketed 1-D fallback): when Newton fails (here given no iterations), the nested bracketed
    /// solves give Newton's answer within 1e-11, at a given T from the superancillary's densities and from seeds
    /// 0.2 % and 5 % off them, and at a given p from a T 0.1 % above and 0.4 % on either side; both report the given p
    /// exactly. Near Tc (Water at Θ = 1e-3, where ln(ρ′/ρ″)/8 narrows both windows) it agrees within 1e-9. A window
    /// without a root, or seeds in the wrong order, is no convergence, with Newton's iterations.
    #[test]
    fn vle_falls_back_to_a_bracket() {
        let close = |a: &SatPair, b: &SatPair| {
            let pairs = [(a.dew.p, b.dew.p), (a.bubble.rho, b.bubble.rho), (a.dew.rho, b.dew.rho), (a.dew.t, b.dew.t)];
            pairs.iter().all(|(x, y)| (x / y - 1.0).abs() <= 1e-11)
        };
        for (name, t) in [("Water", 450.0), ("CarbonDioxide", 280.0), ("n-Heptane", 300.0), ("Helium", 4.0)] {
            let (record, fluid) = model(name);
            let curve = record.superancillary_curve().unwrap();
            let sat = curve.at_t(t).unwrap();
            let eos = fluid.eos();
            let newton = at_t(eos, t, (sat.bubble.rho, sat.dew.rho)).unwrap();
            for seeds in [(sat.bubble.rho, sat.dew.rho), (sat.bubble.rho * 1.002, sat.dew.rho * 0.95)] {
                let fallback = solve_t(eos, t, seeds, 0).unwrap();
                assert!(close(&fallback, &newton), "{name} at {t} K from {seeds:?}: {fallback:?} vs {newton:?}");
            }
            let p = newton.dew.p;
            for k in [1.001, 1.004, 0.996] {
                let near = curve.at_t(t * k).unwrap();
                let seed = (t * k, near.bubble.rho, near.dew.rho);
                let (by_newton, fallback) = (at_p(eos, p, seed).unwrap(), solve_p(eos, p, seed, 0).unwrap());
                assert!(close(&fallback, &by_newton), "{name} at {p} Pa from {k}·T: {fallback:?} vs {by_newton:?}");
                let reported = [by_newton.bubble.p, by_newton.dew.p, fallback.bubble.p, fallback.dew.p];
                assert_eq!(reported, [p; 4], "{name}: the given pressure");
            }
        }
        let (record, fluid) = model("Water");
        let t = 647.096 * (1.0 - 1e-3);
        let sat = record.superancillary_curve().unwrap().at_t(t).unwrap();
        let (newton, fallback) = (
            at_t(fluid.eos(), t, (sat.bubble.rho, sat.dew.rho)).unwrap(),
            solve_t(fluid.eos(), t, (sat.bubble.rho, sat.dew.rho), 0).unwrap(),
        );
        let pairs = [(fallback.bubble.rho, newton.bubble.rho), (fallback.dew.rho, newton.dew.rho)];
        assert!(pairs.iter().all(|(x, y)| (x / y - 1.0).abs() <= 1e-9), "near Tc: {fallback:?} vs {newton:?}");
        assert!(bracketed_t(fluid.eos(), t, (sat.dew.rho, sat.bubble.rho)).is_err(), "seeds in the wrong order");
        let (record, fluid) = model("Water");
        let sat = record.superancillary_curve().unwrap().at_t(450.0).unwrap();
        let far = (sat.bubble.rho, sat.dew.rho * 3.0);
        let refused = Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: 0 });
        assert_eq!(solve_t(fluid.eos(), 450.0, far, 0), refused, "the vapour window misses the root");
    }
    /// A phase point from its density and bundle entries (the others 0).
    fn point(rho: f64, a00: f64, a01: f64) -> Point {
        Point { rho, b: Bundle { a00, a10: 0.0, a01, a20: 0.0, a11: 0.0, a02: 0.0 } }
    }

    /// The pieces Newton is made of, on synthetic phase points with exact values: the point's p/RT, g/RT, scale and
    /// stiffness; both residuals and their sizes; the gate at its edges (90 % and 110 % of each scale, the distinct-phase
    /// test at ln(ρ′/ρ″) of 2e-6 and 5e-7, a zero vapour pressure); the step clipping; the backtracking (its λ sequence,
    /// a decrease, an equal size, the floor, a refused point); a bracketed root's acceptance; the reported pressure.
    #[test]
    fn newton_pieces_on_synthetic_points() {
        let a = Point { rho: 2.0, b: Bundle { a00: 1.5, a10: 0.25, a01: -3.0, a20: 0.0, a11: 0.5, a02: 0.75 } };
        assert_eq!([a.p_rt(), a.g_rt(), a.p_scale(), a.stiffness()], [-6.0, -1.5, 8.0, -5.25]);
        let b = point(1.0, 0.5, 1.0);
        assert_eq!(residual_t(&a, &b), ([-7.0, -3.0], 3.0 / 2.5));
        assert_eq!(residual_t(&a, &point(1.0, -2.5, 1.0)), ([-7.0, 0.0], 7.0 / 10.0), "the pressure term's size");
        // At p/RT = 2 (R = 2, T = 4, p = 16, where R·T and R + T differ): the liquid's pressure term dominates (size 1), then the Gibbs term (1.2),
        // then the vapour's (3.5, at ρ″ = 0.25).
        assert_eq!(residual_p(2.0, 4.0, 16.0, &a, &point(1.0, -2.5, 1.0)), ([-4.0, -0.5, 0.0], 1.0));
        assert_eq!(residual_p(2.0, 4.0, 16.0, &a, &b), ([-4.0, -0.5, -3.0], 3.0 / 2.5));
        assert_eq!(residual_p(2.0, 4.0, 16.0, &a, &point(0.25, -2.5, 1.0)), ([-4.0, -0.875, 0.0], 3.5));
        // The gate: p/RT 2 on both sides (scales 6 and 3), g/RT 3 (scale 4).
        let liquid = point(4.0, 2.5, 0.5);
        let vapour = |dp: f64, dg: f64| point(1.0, 1.0 - dp - dg, 2.0 + dp);
        let ok = |l: Point, v: Point| answer(2.0, 3.0, l, v, 7).is_ok();
        assert!(ok(liquid, vapour(0.0, 0.0)) && answer(2.0, 3.0, liquid, vapour(0.0, 0.0), 7).unwrap().dew.p == 12.0);
        assert!(ok(liquid, vapour(0.9 * GATE * 9.0, 0.0)) && !ok(liquid, vapour(1.1 * GATE * 9.0, 0.0)));
        assert!(ok(liquid, vapour(0.0, 0.9 * GATE * 4.0)) && !ok(liquid, vapour(0.0, 1.1 * GATE * 4.0)));
        let near = |x: f64| point(4.0 * (1.0 + x), 3.0 - 2.0 / (1.0 + x), 2.0 / (1.0 + x));
        assert!(ok(near(2e-6), point(4.0, 1.0, 2.0)) && !ok(near(5e-7), point(4.0, 1.0, 2.0)), "distinct phases");
        assert_eq!(answer(1.0, 1.0, point(4.0, 1.0, 0.0), point(1.0, 1.0, 0.0), 7), Err(failed(7)), "p = 0");
        // Convergence: a step at STEP_TOL; at the floor, a step more than half the last one (but not half itself).
        let step = [STEP_TOL, 0.5 * STEP_TOL, 2.0 * STEP_TOL].map(|length| converged(length, 1.0, 0.0));
        assert_eq!(step, [true, true, false]);
        let floor = [(0.75, FLOOR), (0.75, 0.5 * FLOOR), (0.75, 2.0 * FLOOR), (0.5, FLOOR), (0.25, FLOOR)];
        assert_eq!(floor.map(|(length, size)| converged(length, size, 1.0)), [true, true, false, false, false]);
        // Step clipping and backtracking.
        assert_eq!(step_scale([(0.25, 0.5), (-1.0, 0.5)]), 0.5);
        assert_eq!(step_scale([(0.1, 0.5)]), 1.0);
        assert_eq!(step_scale([(-2.0, 0.5), (0.0, 0.02)]), 0.25);
        let mut tried = Vec::new();
        assert_eq!(
            backtrack::<f64>(1.0, 1.0, |l| {
                tried.push(l);
                None
            }),
            None
        );
        assert_eq!(tried, (0..=10).map(|k| math::powi(0.5, k)).collect::<Vec<_>>());
        let sizes = |l: f64| {
            if l >= 1.0 {
                2.0
            } else if l >= 0.5 {
                1.0
            } else {
                0.75
            }
        };
        assert_eq!(backtrack(1.0, 1.0, |l| Some((l, sizes(l)))), Some(0.25), "an equal size is no decrease");
        assert_eq!(backtrack(0.5, 0.5, |l| Some((l, sizes(l)))), None);
        assert_eq!(backtrack(1.0, 1e-14, |l| Some((l, FLOOR))), Some(1.0), "at the floor");
        assert_eq!(backtrack(1.0, 1e-14, |l| Some((l, 2.0 * FLOOR))), None);
        assert_eq!(backtrack(1.0, 1.0, |l| (l < 1.0).then_some((l, 0.5))), Some(0.5), "the model refuses λ = 1");
        // A bracketed root counts with |f| up to GATE, and only converged.
        let root = |f: f64, stop: Stop| Root { x: 1.5, f, iterations: 3, stop };
        assert_eq!(solved(root(GATE, Stop::Converged)), Some(1.5));
        assert_eq!(solved(root(2.0 * GATE, Stop::Converged)), None);
        assert_eq!(solved(root(0.0, Stop::MaxIterations)), None);
        let sat = answer(2.0, 3.0, liquid, vapour(0.0, 0.0), 7).unwrap();
        let reported = sat.at_pressure(7.0);
        assert_eq!((reported.bubble.p, reported.dew.p, reported.bubble.rho, reported.dew.t), (7.0, 7.0, 4.0, 3.0));
    }

    #[cfg(feature = "fluids-all")]
    /// The Jacobian at a given p against central differences of its residuals in T, ln ρ′ and ln ρ″ (class `Fd`),
    /// for Water at 450 K and CarbonDioxide at 280 K, at the superancillary's state and off it.
    #[test]
    fn vle_jacobian_at_p_matches_ad() {
        for (name, t) in [("Water", 450.0), ("CarbonDioxide", 280.0)] {
            let (record, fluid) = model(name);
            let sat = record.superancillary_curve().unwrap().at_t(t).unwrap();
            let (eos, r, p) = (fluid.eos(), fluid.eos().gas_constant(), sat.dew.p);
            let at = |t: f64, rl: f64, rv: f64| {
                let (l, v) = (Point::at(eos, t, rl).unwrap(), Point::at(eos, t, rv).unwrap());
                (jacobian_p(r, t, p, &l, &v), residual_p(r, t, p, &l, &v).0)
            };
            for (t0, rl, rv) in
                [(t, sat.bubble.rho, sat.dew.rho), (t * 1.001, sat.bubble.rho * 0.999, sat.dew.rho * 1.05)]
            {
                let (j, _) = at(t0, rl, rv);
                let h = 1e-6;
                let moved = |k: usize, s: f64| match k {
                    0 => at(t0 + s * t0, rl, rv).1,
                    1 => at(t0, rl * math::exp(s), rv).1,
                    _ => at(t0, rl, rv * math::exp(s)).1,
                };
                for (col, scale) in [t0, 1.0, 1.0].into_iter().enumerate() {
                    let (up, down) = (moved(col, h), moved(col, -h));
                    for (row, entries) in j.iter().enumerate() {
                        let fd = (up[row] - down[row]) / (2.0 * h * scale);
                        let want = entries[col];
                        assert!(
                            (fd - want).abs() <= 1e-7 * want.abs().max(1e-12),
                            "{name} [{row}][{col}]: {want} vs {fd}"
                        );
                    }
                }
            }
        }
    }

    #[cfg(feature = "fluids-all")]
    /// Newton itself, without the fallback that would hide its failure: from seeds 0.1 % and 1 % off it converges in
    /// 4 iterations at a given T, and from a T 1 % off in 5 at a given p (here one more of each), for Water,
    /// CarbonDioxide and n-Heptane.
    #[test]
    fn newtons_converge_quadratically() {
        for (name, t) in [("Water", 450.0), ("CarbonDioxide", 280.0), ("n-Heptane", 300.0)] {
            let (record, fluid) = model(name);
            let curve = record.superancillary_curve().unwrap();
            let (sat, near) = (curve.at_t(t).unwrap(), curve.at_t(t * 1.01).unwrap());
            let eos = fluid.eos();
            assert!(newton_t(eos, t, (sat.bubble.rho * 1.001, sat.dew.rho * 0.99), 5).is_ok(), "{name} at {t} K");
            let seed = (t * 1.01, near.bubble.rho, near.dew.rho);
            assert!(newton_p(eos, sat.dew.p, seed, 6).is_ok(), "{name} at {} Pa", sat.dew.p);
        }
    }

    #[cfg(feature = "fluids-all")]
    /// Near the critical point the residual reaches its rounding floor at once and Newton's steps are that rounding
    /// divided by a small stiffness, never below `STEP_TOL`: converged at the floor once they stop shrinking (M6.4;
    /// these ran out of iterations before). And at 277 K, water's density maximum, where its saturated liquid is denser
    /// than its triple-point liquid (the model's `rho_max` until M7.1).
    #[test]
    fn vle_converges_near_the_critical_point_and_at_waters_density_maximum() {
        let (record, fluid) = model("Water");
        let curve = record.superancillary_curve().unwrap();
        let eos = fluid.eos();
        for theta in [5.46e-4, 1e-6, 1e-8] {
            let t = 647.096 * (1.0 - theta);
            let sat = curve.at_t(t).unwrap();
            let solved = at_t(eos, t, (sat.bubble.rho, sat.dew.rho)).unwrap();
            assert!((solved.dew.rho / sat.dew.rho - 1.0).abs() < 1e-3, "Θ = {theta}: {solved:?}");
            let seed = (t, sat.bubble.rho, sat.dew.rho);
            assert!(at_p(eos, solved.dew.p, seed).is_ok(), "Θ = {theta} at p");
        }
        let sat = curve.at_t(277.0).unwrap();
        assert!(sat.bubble.rho > curve.at_t(273.16).unwrap().bubble.rho);
        let solved = at_t(eos, 277.0, (sat.bubble.rho, sat.dew.rho)).unwrap();
        assert!((solved.bubble.rho / sat.bubble.rho - 1.0).abs() < 1e-12, "{solved:?}");
    }

    #[cfg(feature = "fluids-all")]
    /// Far below 1 Pa, where the pressure term's scale is the liquid's ρ′ and a seed's ρ″ can be decades off: at 213 K,
    /// PropyleneGlycol's triple point (2e-4 Pa), seeded with the superancillary's densities (4e-8 of ρ″ there), the VLE
    /// gives CoolProp 8.0.0's own VLE (superancillaries off; the register's facts, DIV-0016) within 1e-12.
    #[test]
    fn vle_converges_far_below_one_pascal() {
        let (record, fluid) = model("PropyleneGlycol");
        let sa = record.superancillary_curve().unwrap().at_t(213.0).unwrap();
        let sat = at_t(fluid.eos(), 213.0, (sa.bubble.rho, sa.dew.rho)).unwrap();
        let close = |got: f64, want: f64| (got / want - 1.0).abs() < 1e-12;
        assert!(
            close(sat.dew.rho, 1.234_967_773_820_784_8e-7) && close(sat.bubble.rho, 14_414.121_785_589_81),
            "{sat:?}"
        );
    }

    #[cfg(feature = "fluids-all")]
    /// The domain of a phase point: the model's largest density times the margin is in, a little more is not, nor
    /// twice it, nor ρ ≤ 0 or NaN.
    #[test]
    fn phase_points_stay_in_the_models_domain() {
        let (_, fluid) = model("Water");
        let eos = fluid.eos();
        let top = eos.rho_max(450.0) * RHO_MAX_MARGIN;
        assert!(
            Point::at(eos, 450.0, top).is_some() && Point::at(eos, 450.0, eos.rho_max(450.0) * (1.0 + 5e-7)).is_some()
        );
        for rho in [top * (1.0 + 1e-9), 2.0 * eos.rho_max(450.0), 0.0, -1.0, f64::NAN] {
            assert!(Point::at(eos, 450.0, rho).is_none(), "{rho}");
        }
    }
}
