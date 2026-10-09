//! The pure-fluid VLE (PLAN.md M6.3; D6; map 04 U3; map 03 §3.4, §6): saturation of a Helmholtz model at a given T, by
//! Newton in (ln ρ′, ln ρ″), or at a given p, by Newton in (T, ln ρ′, ln ρ″). The Jacobian is exact, from the model's
//! order-2 bundle at each phase (the derivatives `Jet4` computed), and `num::solve_small` solves it; a backtracking line
//! search keeps every step inside the model's domain and lowering the residual. When Newton fails, nested bracketed
//! solves around the seeds take over (TOMS 748). A converged answer passes a residual gate (p′ = p″ to rounding,
//! g′ = g″, two distinct phases) or is refused: CoolProp's VLE routines exit on a small step with a large residual and
//! can return the trivial solution ρ′ = ρ″ (map 03 §6, map 04 U3).

use crate::derivs::{Bundle, Order};
use crate::error::Error;
use crate::helmholtz::HelmholtzModel;
use crate::num::{math, solve_small};
use crate::roots::{Stop, Tol, toms748};
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

/// A residual this small (relative to the scales of [`GATE`]) is at the rounding floor: a step that keeps it there is
/// taken even if it does not lower it.
const FLOOR: f64 = 1e-13;

/// Seeds at the triple point can sit a few ulp above the model's largest density; the domain check allows this much.
const RHO_MAX_MARGIN: f64 = 1.0 + 1e-6;

/// The largest change of ln ρ in one step, and of T relative.
const MAX_STEP: (f64, f64) = (0.5, 0.02);

/// The bracketed fallback's half-widths around the seeds, in ln ρ′ and ln ρ″ (never more than a quarter of ln(ρ′/ρ″),
/// so that neither window reaches the other phase), and relative in T.
const WINDOW: (f64, f64, f64) = (0.05, 0.5, 0.01);

/// Iterations allowed to each bracketed solve.
const BRACKET_ITER: u16 = 100;

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
    fn at(eos: &dyn HelmholtzModel, t: f64, rho: f64) -> Option<Point> {
        let d = eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two);
        let b = d.bundle()?;
        let finite = [b.a00, b.a10, b.a01, b.a20, b.a11, b.a02].iter().all(|v| v.is_finite());
        (finite && rho.is_finite() && rho > 0.0 && rho <= eos.rho_max(t) * RHO_MAX_MARGIN).then_some(Point { rho, b })
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
    let distinct = math::ln(liquid.rho / vapour.rho) > 1e-6;
    if !(pressure_ok && gibbs_ok && distinct && pv > 0.0) {
        return Err(failed(iterations));
    }
    let p = pv * r * t;
    Ok(SatPair { bubble: SatSide { t, p, rho: liquid.rho }, dew: SatSide { t, p, rho: vapour.rho } })
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
    let r = eos.gas_constant();
    let phase = |rho: f64| Point::at(eos, t, rho);
    let (mut liquid, mut vapour) = (phase(rho_l).ok_or(failed(0))?, phase(rho_v).ok_or(failed(0))?);
    for iteration in 1..=max_iter {
        let (res, size) = residual_t(&liquid, &vapour);
        let step = solve_small(jacobian_t(&liquid, &vapour), [-res[0], -res[1]]).map_err(|_| failed(iteration))?;
        if step[0].abs().max(step[1].abs()) <= STEP_TOL {
            // Converged: the last step is within rounding of nothing; take it if the model accepts it.
            let u = (math::exp(math::ln(liquid.rho) + step[0]), math::exp(math::ln(vapour.rho) + step[1]));
            if let (Some(l), Some(g)) = (phase(u.0), phase(u.1)) {
                (liquid, vapour) = (l, g);
            }
            return answer(r, t, liquid, vapour, iteration);
        }
        let scale = (MAX_STEP.0 / step[0].abs().max(step[1].abs())).min(1.0);
        let mut lambda = scale;
        let next = loop {
            let (u, v) = (math::ln(liquid.rho) + lambda * step[0], math::ln(vapour.rho) + lambda * step[1]);
            if let (Some(l), Some(g)) = (phase(math::exp(u)), phase(math::exp(v))) {
                let next = residual_t(&l, &g).1;
                if next < size || next <= FLOOR {
                    break Some((l, g));
                }
            }
            lambda /= 2.0;
            if lambda < scale / 1024.0 {
                break None;
            }
        };
        let Some((l, g)) = next else { return Err(failed(iteration)) };
        (liquid, vapour) = (l, g);
    }
    Err(failed(max_iter))
}

/// The fallback at a given T (map 04 U3): for a vapour density ρ″ in a window around its seed, the liquid density of
/// equal pressure in a window around the liquid seed (TOMS 748 on ln ρ′), and the ρ″ where the Gibbs energies agree
/// (TOMS 748 on ln ρ″). A window without a sign change, or one the model refuses, is no convergence.
fn bracketed_t(eos: &dyn HelmholtzModel, t: f64, (rho_l, rho_v): (f64, f64)) -> Result<SatPair, Error> {
    if !(rho_l > rho_v && rho_v > 0.0) {
        return Err(failed(0));
    }
    let phase = |rho: f64| Point::at(eos, t, rho);
    let quarter = math::ln(rho_l / rho_v) / 4.0;
    let (ul, uv) = (math::ln(rho_l), math::ln(rho_v));
    let (wl, wv) = (WINDOW.0.min(quarter), WINDOW.1.min(quarter));
    let top = (ul + wl).min(math::ln(eos.rho_max(t)));
    // The liquid point whose p/RT is `pressure`.
    let liquid_at = |pressure: f64| {
        let excess = |u: f64| phase(math::exp(u)).map_or(f64::NAN, |l| l.p_rt() - pressure);
        let root = toms748(excess, ul - wl, top, Tol::Absolute(BRACKET_TOL), BRACKET_ITER);
        (root.stop == Stop::Converged).then(|| phase(math::exp(root.x))).flatten()
    };
    let gibbs = |v: f64| {
        let vapour = phase(math::exp(v));
        let liquid = vapour.and_then(|g| liquid_at(g.p_rt()));
        liquid.zip(vapour).map_or(f64::NAN, |(l, g)| l.g_rt() - g.g_rt())
    };
    let root = toms748(gibbs, uv - wv, uv + wv, Tol::Absolute(BRACKET_TOL), BRACKET_ITER);
    let vapour = (root.stop == Stop::Converged).then(|| phase(math::exp(root.x))).flatten();
    let liquid = vapour.and_then(|g| liquid_at(g.p_rt()));
    let (Some(liquid), Some(vapour)) = (liquid, vapour) else { return Err(failed(root.iterations)) };
    answer(eos.gas_constant(), t, liquid, vapour, root.iterations)
}

/// Saturation of `eos` at the pressure `p` from the seed `(T, ρ′, ρ″)`.
pub(crate) fn at_p(eos: &dyn HelmholtzModel, p: f64, seed: (f64, f64, f64)) -> Result<SatPair, Error> {
    solve_p(eos, p, seed, MAX_ITER)
}

/// [`at_p`] with `max_iter` Newton iterations, then the bracketed fallback; Newton's error if both fail.
fn solve_p(eos: &dyn HelmholtzModel, p: f64, seed: (f64, f64, f64), max_iter: u16) -> Result<SatPair, Error> {
    newton_p(eos, p, seed, max_iter).or_else(|newton| bracketed_p(eos, p, seed).map_err(|_| newton))
}

/// The fallback at a given p: the T in a window around the seed's where ln(p_sat(T)/p) = 0 (TOMS 748), each p_sat by
/// [`solve_t`] from the seed densities; the answer passes [`GATE`] on that logarithm and reports p.
fn bracketed_p(eos: &dyn HelmholtzModel, p: f64, (t, rho_l, rho_v): (f64, f64, f64)) -> Result<SatPair, Error> {
    let at = |t: f64| solve_t(eos, t, (rho_l, rho_v), MAX_ITER).ok();
    let excess = |t: f64| at(t).map_or(f64::NAN, |sat| math::ln(sat.dew.p / p));
    let root = toms748(excess, t * (1.0 - WINDOW.2), t * (1.0 + WINDOW.2), Tol::Relative(BRACKET_TOL), BRACKET_ITER);
    let sat = (root.stop == Stop::Converged && root.f.abs() <= GATE).then(|| at(root.x)).flatten();
    let sat = sat.ok_or(failed(root.iterations))?;
    Ok(SatPair { bubble: SatSide { p, ..sat.bubble }, dew: SatSide { p, ..sat.dew } })
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
    // e1, e2: p′/p − 1, p″/p − 1; e3: (g′ − g″)/RT.
    let residual = |t: f64, (l, v): &(Point, Point)| {
        let e = [l.p_rt() * r * t / p - 1.0, v.p_rt() * r * t / p - 1.0, l.g_rt() - v.g_rt()];
        // Each relative to its cancellation scale (see [`GATE`]).
        let p_rt = p / (r * t);
        let sizes = [e[0] * p_rt / l.p_scale(), e[1] * p_rt / v.p_scale(), e[2] / (1.0 + l.g_rt().abs())];
        (e, sizes.iter().fold(0.0_f64, |m, x| m.max(x.abs())))
    };
    let (mut t, mut pair) = (t, phases(t, rho_l, rho_v).ok_or(failed(0))?);
    for iteration in 1..=max_iter {
        let (e, size) = residual(t, &pair);
        let (l, v) = pair;
        // Columns (T, ln ρ′, ln ρ″); T·∂/∂T of ρRT·A01 is ρRT·(A01 − A11), of g/RT −(A10 + A11).
        let jacobian = [
            [l.rho * r * (l.b.a01 - l.b.a11) / p, l.rho * r * t * l.stiffness() / p, 0.0],
            [v.rho * r * (v.b.a01 - v.b.a11) / p, 0.0, v.rho * r * t * v.stiffness() / p],
            [-((l.b.a10 + l.b.a11) - (v.b.a10 + v.b.a11)) / t, l.stiffness(), -v.stiffness()],
        ];
        let step = solve_small(jacobian, [-e[0], -e[1], -e[2]]).map_err(|_| failed(iteration))?;
        if (step[0] / t).abs().max(step[1].abs()).max(step[2].abs()) <= STEP_TOL {
            let rho = |rho: f64, s: f64| math::exp(math::ln(rho) + s);
            if let Some(candidate) = phases(t + step[0], rho(l.rho, step[1]), rho(v.rho, step[2])) {
                (t, pair) = (t + step[0], candidate);
            }
            let sat = answer(r, t, pair.0, pair.1, iteration)?;
            // The pair is at p: report the given pressure, which both phases reproduce within the gate.
            return Ok(SatPair { bubble: SatSide { p, ..sat.bubble }, dew: SatSide { p, ..sat.dew } });
        }
        let largest = (step[0] / t / MAX_STEP.1).abs().max(step[1].abs().max(step[2].abs()) / MAX_STEP.0);
        let scale = (1.0 / largest).min(1.0);
        let mut lambda = scale;
        let next = loop {
            let tn = t + lambda * step[0];
            let rho = |rho: f64, s: f64| math::exp(math::ln(rho) + lambda * s);
            if let Some(candidate) = phases(tn, rho(l.rho, step[1]), rho(v.rho, step[2])) {
                let next = residual(tn, &candidate).1;
                if next < size || next <= FLOOR {
                    break Some((tn, candidate));
                }
            }
            lambda /= 2.0;
            if lambda < scale / 1024.0 {
                break None;
            }
        };
        let Some((tn, candidate)) = next else { return Err(failed(iteration)) };
        (t, pair) = (tn, candidate);
    }
    Err(failed(max_iter))
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "fluids-all")]
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
    /// 0.2 % and 5 % off them, and at a given p from a T 0.1 % off; a window without a root is no convergence, with
    /// Newton's iterations.
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
            let near = curve.at_t(t * 1.001).unwrap();
            let seed = (t * 1.001, near.bubble.rho, near.dew.rho);
            let (by_newton, fallback) =
                (at_p(eos, newton.dew.p, seed).unwrap(), solve_p(eos, newton.dew.p, seed, 0).unwrap());
            assert!(close(&fallback, &by_newton), "{name} at {} Pa: {fallback:?} vs {by_newton:?}", newton.dew.p);
        }
        let (record, fluid) = model("Water");
        let sat = record.superancillary_curve().unwrap().at_t(450.0).unwrap();
        let far = (sat.bubble.rho, sat.dew.rho * 3.0);
        let refused = Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: 0 });
        assert_eq!(solve_t(fluid.eos(), 450.0, far, 0), refused, "the vapour window misses the root");
    }
}
