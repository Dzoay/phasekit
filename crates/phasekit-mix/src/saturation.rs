//! Bubble and dew points (PLAN.md M13.9): at a fixed T or p, the other and the incipient phase in equilibrium with the
//! feed.
//!
//! - The unknowns are ln K_i and ln X, X the unknown of p and T, with the incipient phase's mole numbers K_i z_i
//!   (bubble) or z_i/K_i (dew). The equations are F_i = ln K_i + ln φ_i^V − ln φ_i^L = 0, the fugacities' equality,
//!   and F_{N+1} = Σ z_i K_i − 1 (bubble) or Σ z_i/K_i − 1 (dew) = 0, the summation.
//! - Wilson's K seeds X, then up to eight successive substitutions K, then Newton's method with the Jacobian from
//!   M13.6's composition derivatives: ∂F_i/∂ln K_j = δ_ij ± Φ_ij w_j over the incipient phase's Φ = n ∂ln φ/∂n and
//!   fractions w, and ∂F_i/∂ln X from (∂ln φ/∂p)_T or (∂ln φ/∂T)_p of both phases.
//! - Each phase is on its own branch, the liquid's dense and the vapour's dilute, its root polished to the rounding of
//!   p: at a saturation point the phases are known.
//! - It ends on a residual gate over all N + 1 equations, half `FUGACITY_GATE`, never on the step (ROT-185). A solve that
//!   falls into the trivial solution, every K_i near 1 and both phases on one root, is refused: no saturation point
//!   there, or the critical point. An azeotrope's K_i are near 1 as well, its phases a liquid and a vapour.
//! - Where Wilson's seed fails, as it does near the critical point, where it lies above the dome and the solve falls
//!   into the trivial solution, M13.8's PT flash seeds it: Newton's method from the split nearest the side's own end of
//!   Wilson's interval, its K = y/x; and where that fails too, from the last split before that split's boundary with
//!   the feed's own single phase, bisected.

use phasekit_core::density::Branch;
use phasekit_core::{Error, Order, Strategy, ThermoModel, math};

use crate::equilibrium::{AtTp, FUGACITY_GATE, Phase, branch_root, normalised, polished, solve, square, wilson_parts};
use crate::{Equilibrium, Mixture};

/// The bubble point (the feed liquid, a vapour incipient) or the dew point (the feed vapour, a liquid incipient).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The feed is the liquid.
    Bubble,
    /// The feed is the vapour.
    Dew,
}

/// Which of T and p is given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Given {
    /// T (K) given, p solved.
    T(f64),
    /// p (Pa) given, T solved.
    P(f64),
}

/// A bubble or dew point.
#[derive(Clone, Debug, PartialEq)]
pub struct Saturation {
    /// Temperature, K.
    pub t: f64,
    /// Pressure, Pa.
    pub p: f64,
    /// The feed's molar density, mol/m³.
    pub rho: f64,
    /// The incipient phase's mole fractions.
    pub incipient: Vec<f64>,
    /// The incipient phase's molar density, mol/m³.
    pub rho_incipient: f64,
}

/// The solve's own gate: half the fugacity gate, so the point returned meets that gate with room for the rounding of
/// its recomputation (R418A's dew point at 258.27 K, accepted at the full gate in an iteration path, recomputed
/// 1.0016e-12 off).
const GATE: f64 = FUGACITY_GATE / 2.0;

/// Iterations of the substitution, then of Newton's method.
const SUBSTITUTIONS: usize = 8;
const NEWTON_STEPS: u16 = 60;

/// An iterate is the trivial solution where every |ln K_i| is under the first and the phases' densities are within
/// the second in ln ρ: the incipient phase is the feed, on its root.
const TRIVIAL: (f64, f64) = (1e-4, 1e-3);

/// The PT flashes the fallback tries between Wilson's bubble and dew points, the factor it steps out by to the feed's
/// own phase (at most `STEPS_OUT` times), and the bisections of the boundary.
const CANDIDATES: usize = 9;
const STEP_OUT: f64 = 1.25;
const STEPS_OUT: usize = 10;
const BISECTIONS: usize = 8;

/// A phase of `x` at (T, p) on `branch`, from `seed` where one is known, polished, to `order`.
fn on_branch(mixture: &Mixture, t: f64, p: f64, x: &[f64], branch: Branch, seed: Option<f64>) -> Result<AtTp, Error> {
    let phase = Phase { mixture, x };
    let rho = polished(&phase, t, p, branch_root(&phase, t, p, branch, seed)?);
    AtTp::new(mixture, t, p, rho, x, Order::Two)
}

/// Whether the equations `f` (F_1 .. F_N in ln K, then Σw − 1) meet `GATE` at the point returned, whose
/// incipient fractions are w/Σw: there ln(y_i/x_i) is ln K_i − ln Σw at a bubble point and ln K_i + ln Σw at a dew
/// point, so each F_i is judged with that shift (ln Σw is Σw − 1 to its square, 1e-24 inside the gate), and Σw − 1
/// itself.
fn meets_gate(side: Side, f: &[f64]) -> bool {
    let Some((summation, fugacities)) = f.split_last() else { return false };
    let shift = if side == Side::Bubble { -summation } else { *summation };
    summation.abs() <= GATE && fugacities.iter().all(|f| (f + shift).abs() <= GATE)
}

/// Whether an iterate is the trivial solution (`TRIVIAL`): every |ln K_i| and |ln(ρ/ρ_incipient)| small.
fn trivial(ln_k: &[f64], rho: f64, rho_incipient: f64) -> bool {
    ln_k.iter().all(|l| l.abs() < TRIVIAL.0) && math::ln(rho / rho_incipient).abs() < TRIVIAL.1
}

/// Wilson's seed: the p at T where Σ z_i K_i = 1 (bubble) or Σ z_i/K_i = 1 (dew), K_i = (p_c,i/p) exp(a_i(T)).
fn wilson_p(mixture: &Mixture, side: Side, t: f64) -> f64 {
    let parts = wilson_parts(mixture, t);
    let z = mixture.composition();
    match side {
        Side::Bubble => parts.iter().zip(z).map(|((pc, a), z)| z * pc * math::exp(*a)).sum(),
        Side::Dew => 1.0 / parts.iter().zip(z).map(|((pc, a), z)| z / (pc * math::exp(*a))).sum::<f64>(),
    }
}

/// Wilson's seed: the T at p, by bisection in 1/T over the mixture's range, where Wilson's p is p (it rises with T).
fn wilson_t(mixture: &Mixture, side: Side, p: f64) -> f64 {
    let (mut lo, mut hi) = mixture.limits().map_or((1.0, 1e4), |l| (l.t_min(), l.t_max()));
    for _ in 0..80 {
        let mid = 2.0 / (1.0 / lo + 1.0 / hi);
        if wilson_p(mixture, side, mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    2.0 / (1.0 / lo + 1.0 / hi)
}

/// The bubble or dew point of `mixture` at `given` T or p: from Wilson's seed; else from the split nearest the side's own
/// end of Wilson's interval; else from that split's boundary with the feed's own phase, bisected.
pub(crate) fn saturation(mixture: &Mixture, side: Side, given: Given) -> Result<Saturation, Error> {
    let z = mixture.composition().to_vec();
    let solve = |(mut t, mut p, mut ln_k): (f64, f64, Vec<f64>)| {
        Solver { mixture, side, z: &z, seeds: (None, None) }.newton(given, &mut t, &mut p, &mut ln_k)
    };
    let mut wilson = Solver { mixture, side, z: &z, seeds: (None, None) };
    let first = match solve(wilson.seed_wilson(given)) {
        Ok(found) => return Ok(found),
        Err(error) => error,
    };
    let search = SplitSearch { mixture, side, given };
    let Some(split) = search.first() else { return Err(first) };
    solve(search.seed(&split)).or_else(|_| search.boundary(split).map_or(Err(first), |near| solve(search.seed(&near))))
}

/// A split of the PT flash (M13.8) at ln X, X the unknown of p and T: ln X and the split's ln K = ln(y/x).
type SplitAt = (f64, Vec<f64>);

/// The fallback seeds' search, by M13.8's PT flash, for the saturation point on `side` at `given` (the module's last
/// bullet).
struct SplitSearch<'a> {
    mixture: &'a Mixture,
    side: Side,
    given: Given,
}

impl SplitSearch<'_> {
    /// (T, p) at ln X.
    fn at(&self, x: f64) -> (f64, f64) {
        match self.given {
            Given::T(t) => (t, math::exp(x)),
            Given::P(p) => (math::exp(x), p),
        }
    }

    /// The split at ln X; `None` where the PT flash finds one phase, or fails.
    fn split(&self, x: f64) -> Option<SplitAt> {
        let (t, p) = self.at(x);
        match self.mixture.equilibrium(t, p) {
            Ok(Equilibrium::Split { x: liquid, y, .. }) => {
                Some((x, liquid.iter().zip(&y).map(|(x, y)| math::ln(y / x)).collect()))
            }
            _ => None,
        }
    }

    /// The first split of `CANDIDATES` points evenly spaced in ln X between Wilson's bubble and dew points, from the
    /// side's own end: the one nearest the saturation point sought.
    fn first(&self) -> Option<SplitAt> {
        let (bubble, dew) = match self.given {
            Given::T(t) => {
                (math::ln(wilson_p(self.mixture, Side::Bubble, t)), math::ln(wilson_p(self.mixture, Side::Dew, t)))
            }
            Given::P(p) => {
                (math::ln(wilson_t(self.mixture, Side::Bubble, p)), math::ln(wilson_t(self.mixture, Side::Dew, p)))
            }
        };
        let (own, other) = if self.side == Side::Bubble { (bubble, dew) } else { (dew, bubble) };
        (0..CANDIDATES).find_map(|k| self.split(own + (other - own) * k as f64 / (CANDIDATES - 1) as f64))
    }

    /// The boundary between `split` and the feed's own single phase (at a higher p or lower T at a bubble point, a lower
    /// p or higher T at a dew point): stepped out by `STEP_OUT` (at most `STEPS_OUT` times) to the single phase, then
    /// bisected in ln X `BISECTIONS` times; the last split. `None` where no single phase lies within the steps.
    fn boundary(&self, split: SplitAt) -> Option<SplitAt> {
        let out = match (self.given, self.side) {
            (Given::T(_), Side::Bubble) | (Given::P(_), Side::Dew) => math::ln(STEP_OUT),
            _ => -math::ln(STEP_OUT),
        };
        let (mut inside, mut outside) = (split, None);
        for _ in 0..STEPS_OUT {
            let x = inside.0 + out;
            match self.split(x) {
                Some(next) => inside = next,
                None => {
                    outside = Some(x);
                    break;
                }
            }
        }
        let mut outside = outside?;
        for _ in 0..BISECTIONS {
            let middle = (inside.0 + outside) / 2.0;
            match self.split(middle) {
                Some(next) => inside = next,
                None => outside = middle,
            }
        }
        Some(inside)
    }

    /// The Newton solve's seed from a split: (T, p, ln K).
    fn seed(&self, (x, ln_k): &SplitAt) -> (f64, f64, Vec<f64>) {
        let (t, p) = self.at(*x);
        (t, p, ln_k.clone())
    }
}

/// The Newton solve's state: the feed, the side, and each phase's last density for the next solve's seed.
struct Solver<'a> {
    mixture: &'a Mixture,
    side: Side,
    z: &'a [f64],
    seeds: (Option<f64>, Option<f64>),
}

impl Solver<'_> {
    /// Wilson's seed at `given` and up to `SUBSTITUTIONS` successive substitutions from it: K from the phases' fugacity
    /// coefficients, and at a given T p from the summation (p Σ z_i K_i at a bubble point, p/Σ z_i/K_i at a dew point);
    /// at a given p, Newton's method moves T. (T, p, ln K).
    fn seed_wilson(&mut self, given: Given) -> (f64, f64, Vec<f64>) {
        let (t, mut p) = match given {
            Given::T(t) => (t, wilson_p(self.mixture, self.side, t)),
            Given::P(p) => (wilson_t(self.mixture, self.side, p), p),
        };
        let mut ln_k: Vec<f64> = wilson_parts(self.mixture, t).iter().map(|(pc, a)| math::ln(pc / p) + a).collect();
        for _ in 0..SUBSTITUTIONS {
            let Ok((feed, incipient, w)) = self.phases(t, p, &ln_k) else { break };
            let (liquid, vapour) = self.liquid_vapour(&feed, &incipient);
            ln_k = liquid.iter().zip(&vapour).map(|(l, v)| l - v).collect();
            if let Given::T(_) = given {
                let sum: f64 = w.iter().sum();
                p = if self.side == Side::Bubble { p * sum } else { p / sum };
            }
        }
        (t, p, ln_k)
    }

    /// The feed and the incipient phase at (T, p) from `ln_k`, each on its branch, and the incipient phase's mole
    /// numbers w (K_i z_i or z_i/K_i).
    fn phases(&mut self, t: f64, p: f64, ln_k: &[f64]) -> Result<(AtTp, AtTp, Vec<f64>), Error> {
        let w: Vec<f64> = match self.side {
            Side::Bubble => self.z.iter().zip(ln_k).map(|(z, l)| z * math::exp(*l)).collect(),
            Side::Dew => self.z.iter().zip(ln_k).map(|(z, l)| z / math::exp(*l)).collect(),
        };
        let (feed_branch, incipient_branch) =
            if self.side == Side::Bubble { (Branch::Liquid, Branch::Vapour) } else { (Branch::Vapour, Branch::Liquid) };
        let feed = on_branch(self.mixture, t, p, self.z, feed_branch, self.seeds.0)?;
        let incipient = on_branch(self.mixture, t, p, &normalised(&w), incipient_branch, self.seeds.1)?;
        self.seeds = (Some(feed.rho), Some(incipient.rho));
        Ok((feed, incipient, w))
    }

    /// ln φ of the liquid and of the vapour, from the feed's and the incipient phase's.
    fn liquid_vapour(&self, feed: &AtTp, incipient: &AtTp) -> (Vec<f64>, Vec<f64>) {
        match self.side {
            Side::Bubble => (feed.ln_phi(), incipient.ln_phi()),
            Side::Dew => (incipient.ln_phi(), feed.ln_phi()),
        }
    }

    /// The N + 1 equations at (T, p, ln K), F_i = ln K_i + ln φ_i^V − ln φ_i^L and Σw − 1, with the phases they were
    /// evaluated on: the feed, the incipient phase and its mole numbers w.
    fn equations(&mut self, t: f64, p: f64, ln_k: &[f64]) -> Result<(Vec<f64>, AtTp, AtTp, Vec<f64>), Error> {
        let (feed, incipient, w) = self.phases(t, p, ln_k)?;
        let (liquid, vapour) = self.liquid_vapour(&feed, &incipient);
        let mut f: Vec<f64> = (0..self.z.len()).map(|i| ln_k[i] + vapour[i] - liquid[i]).collect();
        f.push(w.iter().sum::<f64>() - 1.0);
        Ok((f, feed, incipient, w))
    }

    /// Newton's method on the N + 1 equations in (ln K, ln X), each step cut to 0.5 in ln K and 0.1 in ln X, to the
    /// residual gate.
    fn newton(&mut self, given: Given, t: &mut f64, p: &mut f64, ln_k: &mut [f64]) -> Result<Saturation, Error> {
        let failed = |iterations| Error::NoConvergence { strategy: Strategy::Vle, iterations };
        let n = self.z.len();
        for iteration in 0..NEWTON_STEPS {
            let (f, feed, incipient, w) = self.equations(*t, *p, ln_k)?;
            // The trivial solution: the incipient phase is the feed, on the same root. An azeotrope's K_i are all near
            // 1 too, but its phases' densities are a liquid's and a vapour's.
            if trivial(ln_k, feed.rho, incipient.rho) {
                return Err(failed(iteration));
            }
            if meets_gate(self.side, &f) {
                return Ok(Saturation {
                    t: *t,
                    p: *p,
                    rho: feed.rho,
                    incipient: normalised(&w),
                    rho_incipient: incipient.rho,
                });
            }
            let jacobian = self.jacobian(given, *t, *p, &feed, &incipient, &w);
            let Some(step) = solve(jacobian, f.iter().map(|f| -f).collect()) else { return Err(failed(iteration)) };
            let cut = |s: f64, limit: f64| s.clamp(-limit, limit);
            for (l, s) in ln_k.iter_mut().zip(&step) {
                *l += cut(*s, 0.5);
            }
            let dx = cut(step[n], 0.1);
            match given {
                Given::T(_) => *p *= math::exp(dx),
                Given::P(_) => *t *= math::exp(dx),
            }
        }
        Err(failed(NEWTON_STEPS))
    }

    /// The Jacobian of (F_1 .. F_N, F_{N+1}) in (ln K_1 .. ln K_N, ln X).
    fn jacobian(&self, given: Given, t: f64, p: f64, feed: &AtTp, incipient: &AtTp, w: &[f64]) -> Vec<Vec<f64>> {
        let n = self.z.len();
        let x = normalised(w);
        // The incipient phase's mole numbers move with ln K as +w (bubble) or −w (dew).
        let sign = if self.side == Side::Bubble { 1.0 } else { -1.0 };
        // F_i carries +ln φ^V − ln φ^L: the incipient phase's terms enter with +1 at a bubble point (the vapour) and
        // −1 at a dew point (the liquid), so ∂F_i/∂ln K_j = δ_ij + Φ_ij x_j either way.
        let phi_n = |i: usize, j: usize| incipient.c.n_dln_phi_dn(i, j);
        let d_ln_x = |a: &AtTp, i: usize| match given {
            Given::T(_) => p * a.c.dln_phi_dp(i),
            Given::P(_) => t * a.c.dln_phi_dt(i),
        };
        let (liquid, vapour) = match self.side {
            Side::Bubble => (feed, incipient),
            Side::Dew => (incipient, feed),
        };
        square(n + 1, |i, k| match (i < n, k < n) {
            (true, true) => (if i == k { 1.0 } else { 0.0 }) + phi_n(i, k) * x[k],
            (true, false) => d_ln_x(vapour, i) - d_ln_x(liquid, i),
            (false, true) => sign * w[k],
            (false, false) => 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use phasekit_core::{Constant, DataSet, Registry};

    use super::*;
    use crate::MultiFluid;

    /// The library's or GERG-2008's Methane/Ethane at z₁.
    fn methane_ethane(z1: f64, gerg: bool) -> Mixture {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap_or_else(|e| panic!("{e:?}"));
        let names = ["Methane", "Ethane"];
        let model = if gerg {
            MultiFluid::gerg2008(&registry, &names)
        } else {
            MultiFluid::new(&registry, &names, DataSet::Parity)
        };
        Mixture::new(model.unwrap_or_else(|e| panic!("{e:?}")), &[z1, 1.0 - z1]).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn solver<'a>(mixture: &'a Mixture, side: Side, z: &'a [f64]) -> Solver<'a> {
        Solver { mixture, side, z, seeds: (None, None) }
    }

    /// ln K = ln(y/x) of a saturation point of `z`.
    fn ln_k_of(side: Side, z: &[f64], s: &Saturation) -> Vec<f64> {
        let (x, y) = if side == Side::Bubble { (z, &s.incipient[..]) } else { (&s.incipient[..], z) };
        x.iter().zip(y).map(|(x, y)| math::ln(y / x)).collect()
    }

    /// The four kinds of point: a bubble and a dew point, each at a given T (200 K) and a given p (1 MPa).
    const KINDS: [(Side, Given); 4] = [
        (Side::Bubble, Given::T(200.0)),
        (Side::Dew, Given::T(200.0)),
        (Side::Bubble, Given::P(1e6)),
        (Side::Dew, Given::P(1e6)),
    ];

    /// Wilson's seeds are his K's summations, Σ z_i p_c,i e^(a_i) at a bubble point and 1/Σ z_i/(p_c,i e^(a_i)) at a dew
    /// point, a_i = 5.373 (1 + ω_i)(1 − T_c,i/T), from each component's published critical point and acentric factor;
    /// and Wilson's T at p inverts them, by bisection in 1/T, to 1e-12.
    #[test]
    fn wilson_seeds_are_his_summations() {
        let m = methane_ethane(0.4, false);
        let t = 200.0;
        let parts: Vec<f64> = m
            .model()
            .fluids()
            .map(|f| {
                let (c, omega) = (
                    f.info().published_critical().unwrap_or_else(|| panic!("critical")),
                    f.constant(Constant::Acentric).unwrap_or(f64::NAN),
                );
                c.p * math::exp(5.373 * (1.0 + omega) * (1.0 - c.t / t))
            })
            .collect();
        let bubble = 0.4 * parts[0] + 0.6 * parts[1];
        let dew = 1.0 / (0.4 / parts[0] + 0.6 / parts[1]);
        assert!(
            (wilson_p(&m, Side::Bubble, t) / bubble - 1.0).abs() <= 1e-15
                && (wilson_p(&m, Side::Dew, t) / dew - 1.0).abs() <= 1e-15
        );
        for side in [Side::Bubble, Side::Dew] {
            let t = wilson_t(&m, side, 1e6);
            assert!((wilson_p(&m, side, t) / 1e6 - 1.0).abs() <= 1e-12, "{side:?}: {t} K");
        }
    }

    /// The trivial solution needs both: every |ln K_i| under 1e-4, 1e-4 itself not, and the densities within 1e-3 in
    /// ln ρ, either way.
    #[test]
    fn the_trivial_solution_needs_both_closenesses() {
        assert!(trivial(&[9e-5, -9e-5], 1000.0, 1000.9) && trivial(&[0.0, 0.0], 1000.9, 1000.0));
        assert!(!trivial(&[1.1e-4, 0.0], 1000.0, 1000.0) && !trivial(&[0.0, -1.1e-4], 1000.0, 1000.0));
        assert!(!trivial(&[1e-4, 0.0], 1000.0, 1000.0) && !trivial(&[0.0, -1e-4], 1000.0, 1000.0));
        assert!(!trivial(&[0.0, 0.0], 1000.0, 1001.1) && !trivial(&[0.0, 0.0], 1001.1, 1000.0));
    }

    /// The gate judges the point returned, the incipient fractions w/Σw: each F_i shifted by −(Σw − 1) at a bubble point
    /// and +(Σw − 1) at a dew point, and Σw − 1 itself, each within half the fugacity gate, 5e-13, the gate included.
    #[test]
    fn the_gate_judges_the_point_returned() {
        let g = 5e-13;
        assert!(meets_gate(Side::Bubble, &[g, g, g]) && !meets_gate(Side::Dew, &[g, g, g]));
        assert!(meets_gate(Side::Dew, &[-g, -g, g]) && !meets_gate(Side::Bubble, &[-g, -g, g]));
        assert!(meets_gate(Side::Bubble, &[2.0 * g, 0.0, g]) && !meets_gate(Side::Bubble, &[2.5 * g, 0.0, g]));
        assert!(!meets_gate(Side::Bubble, &[1.5 * g, 1.5 * g, 1.5 * g]) && !meets_gate(Side::Bubble, &[]));
    }

    /// Successive substitution from Wilson's seed lands within 0.05 in ln p and ln T together and 0.06 in ln K of the
    /// point, for each kind of point.
    #[test]
    fn substitution_moves_wilsons_seed_to_the_point() {
        let m = methane_ethane(0.4, false);
        let z = m.composition().to_vec();
        for (side, given) in KINDS {
            let s = saturation(&m, side, given).unwrap_or_else(|e| panic!("{e:?}"));
            let (t, p, ln_k) = solver(&m, side, &z).seed_wilson(given);
            let off = math::ln(p / s.p).abs() + math::ln(t / s.t).abs();
            let k_off = ln_k.iter().zip(ln_k_of(side, &z, &s)).fold(0.0_f64, |m, (a, b)| m.max((a - b).abs()));
            assert!(off < 0.05 && k_off < 0.06, "{side:?} {given:?}: {off} {k_off}");
        }
    }

    /// The Newton step's Jacobian is the equations' derivative in (ln K, ln X): against central differences of step
    /// 1e-6, within 1e-6 of each column's largest entry, off the solution (ln K + 0.05, ln X + 0.02), for each kind of
    /// point.
    #[test]
    fn the_jacobian_is_the_equations_derivative() {
        let m = methane_ethane(0.4, false);
        let z = m.composition().to_vec();
        for (side, given) in KINDS {
            let s = saturation(&m, side, given).unwrap_or_else(|e| panic!("{e:?}"));
            let ln_k: Vec<f64> = ln_k_of(side, &z, &s).iter().map(|l| l + 0.05).collect();
            let (t, p) = match given {
                Given::T(_) => (s.t, s.p * math::exp(0.02)),
                Given::P(_) => (s.t * math::exp(0.02), s.p),
            };
            let mut at = solver(&m, side, &z);
            let (_, feed, incipient, w) = at.equations(t, p, &ln_k).unwrap_or_else(|e| panic!("{e:?}"));
            let jacobian = at.jacobian(given, t, p, &feed, &incipient, &w);
            let h = 1e-6;
            for k in 0..=z.len() {
                let mut moved = |d: f64| {
                    let mut ln_k = ln_k.clone();
                    let (mut t, mut p) = (t, p);
                    match (k < z.len(), given) {
                        (true, _) => ln_k[k] += d,
                        (false, Given::T(_)) => p *= math::exp(d),
                        (false, Given::P(_)) => t *= math::exp(d),
                    }
                    at.equations(t, p, &ln_k).unwrap_or_else(|e| panic!("{e:?}")).0
                };
                let (up, down) = (moved(h), moved(-h));
                let column: Vec<f64> = up.iter().zip(&down).map(|(u, d)| (u - d) / (2.0 * h)).collect();
                let scale = column.iter().fold(1.0_f64, |m, c| m.max(c.abs()));
                for (i, fd) in column.iter().enumerate() {
                    assert!(
                        (jacobian[i][k] - fd).abs() <= 1e-6 * scale,
                        "{side:?} {given:?} ({i}, {k}): {} against {fd}",
                        jacobian[i][k]
                    );
                }
            }
        }
    }

    /// The split search starts from the side's own end of Wilson's interval: near GERG-2008 Methane/Ethane's critical
    /// point (x₁ 0.847 at 200 K, bubble point 4.496 MPa, where Wilson's bubble pressure lies above the dome) its first
    /// split is a candidate k ≥ 1 of the nine, the one before it a single phase, and its ln K is the split's ln(y/x).
    #[test]
    fn the_split_search_starts_from_the_side_s_own_end() {
        let m = methane_ethane(0.847, true);
        let search = SplitSearch { mixture: &m, side: Side::Bubble, given: Given::T(200.0) };
        let (own, other) = (math::ln(wilson_p(&m, Side::Bubble, 200.0)), math::ln(wilson_p(&m, Side::Dew, 200.0)));
        let candidate = |k: usize| own + (other - own) * k as f64 / (CANDIDATES - 1) as f64;
        let (x, ln_k) = search.first().unwrap_or_else(|| panic!("no split"));
        let k = (1..CANDIDATES).find(|k| candidate(*k) == x).unwrap_or_else(|| panic!("{x} is no candidate"));
        assert!(search.split(candidate(k - 1)).is_none(), "{k}");
        let Ok(Equilibrium::Split { x: liquid, y, .. }) = m.equilibrium(200.0, math::exp(x)) else { panic!("{x}") };
        for i in 0..2 {
            assert_eq!(ln_k[i], math::ln(y[i] / liquid[i]));
        }
        assert_eq!(search.seed(&(x, ln_k.clone())), (200.0, math::exp(x), ln_k));
    }

    /// From a split between them, the boundary search ends on the last split within ln 1.25/2⁸ in ln p of the bubble
    /// point above it and of the dew point below it (library Methane/Ethane at 50/50, 200 K).
    #[test]
    fn the_boundary_is_bisected_to_the_feed_s_own_phase() {
        let m = methane_ethane(0.5, false);
        let points = [Side::Bubble, Side::Dew]
            .map(|side| saturation(&m, side, Given::T(200.0)).map(|s| s.p).unwrap_or(f64::NAN));
        let middle = (math::ln(points[0]) + math::ln(points[1])) / 2.0;
        let width = math::ln(STEP_OUT) / f64::from(1 << BISECTIONS);
        for (side, sat) in [Side::Bubble, Side::Dew].into_iter().zip(points) {
            let search = SplitSearch { mixture: &m, side, given: Given::T(200.0) };
            let split = search.split(middle).unwrap_or_else(|| panic!("no split at {middle}"));
            let (x, _) = search.boundary(split).unwrap_or_else(|| panic!("{side:?}: no boundary"));
            let inside = if side == Side::Bubble { math::ln(sat) - x } else { x - math::ln(sat) };
            assert!(inside > 0.0 && inside <= width, "{side:?}: {inside} inside {sat} Pa");
        }
    }
}
