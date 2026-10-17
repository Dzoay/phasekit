//! Two phases at a given vapour fraction (PLAN.md M13.9, M13.10): at a fixed T or p and β, the other of T and p and
//! the two phases, in equilibrium and adding up to the feed. β = 0 is the bubble point (the feed the liquid, the vapour
//! incipient) and β = 1 the dew point (the feed the vapour).
//!
//! - The unknowns are ln K_i and ln X, X the unknown of p and T. The phases follow from them and the mass balance,
//!   x_i = z_i/(1 + β(K_i − 1)) and y_i = K_i x_i, so (1 − β)x + βy = z by construction (ROT-182). The equations are
//!   F_i = ln K_i + ln φ_i^V − ln φ_i^L = 0, the fugacities' equality, and F_{N+1} = Σ (y_i − x_i) = 0, Rachford–Rice
//!   at the given β: Σ z_i K_i − 1 at a bubble point, 1 − Σ z_i/K_i at a dew point.
//! - Wilson's K seeds X, then up to eight successive substitutions K, then Newton's method with the Jacobian from
//!   M13.6's composition derivatives: ∂F_i/∂ln K_k = δ_ik + (x_k y_k/z_k)((1 − β)Φ^V_ik/Σy + βΦ^L_ik/Σx) over each
//!   phase's Φ = n ∂ln φ/∂n, ∂F_{N+1}/∂ln K_k = x_k y_k/z_k, and ∂F_i/∂ln X from (∂ln φ/∂p)_T or (∂ln φ/∂T)_p of both
//!   phases.
//! - Each phase is on its own branch, the liquid's dense and the vapour's dilute, its root polished to the rounding of
//!   p: at coexistence the phases are known.
//! - It ends on a residual gate over all N + 1 equations, half `FUGACITY_GATE`, at the point returned, never on the step
//!   (ROT-185). A solve that falls into the trivial solution, every K_i near 1 and both phases on one root, is refused:
//!   no coexistence there, or the critical point. An azeotrope's K_i are near 1 as well, its phases a liquid and a
//!   vapour.
//! - At a bubble or dew point, where Wilson's seed fails, as it does near the critical point, where it lies above the
//!   dome and the solve falls into the trivial solution, M13.8's PT flash seeds it: Newton's method from the split
//!   nearest the side's own end of Wilson's interval, its K = y/x; and where that fails too, from the last split
//!   before that split's boundary with the feed's own single phase, bisected. Inside the dome the bubble and dew points
//!   at the same T or p seed it, ln X and ln K interpolated in β.

use phasekit_core::density::Branch;
use phasekit_core::{Error, Order, Strategy, ThermoModel, math};

use crate::equilibrium::{AtTp, FUGACITY_GATE, Phase, branch_root, normalised, polished, solve, square, wilson_parts};
use crate::illinois::{End, illinois};
use crate::{Equilibrium, Mixture};

/// The bubble point (the feed liquid, a vapour incipient) or the dew point (the feed vapour, a liquid incipient).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The feed is the liquid.
    Bubble,
    /// The feed is the vapour.
    Dew,
}

impl Side {
    /// The vapour fraction at this side's point: 0 at a bubble point, 1 at a dew point.
    fn beta(self) -> f64 {
        if self == Side::Bubble { 0.0 } else { 1.0 }
    }
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

/// Two phases in equilibrium at a vapour fraction β, adding up to the feed.
#[derive(Clone, Debug, PartialEq)]
pub struct Coexisting {
    /// Temperature, K.
    pub t: f64,
    /// Pressure, Pa.
    pub p: f64,
    /// The vapour's molar fraction of the whole.
    pub beta: f64,
    /// The liquid's and the vapour's mole fractions.
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// The liquid's and the vapour's molar densities, mol/m³.
    pub rho_liquid: f64,
    pub rho_vapour: f64,
}

/// The solve's own gate: half the fugacity gate, so the point returned meets that gate with room for the rounding of
/// its recomputation (R418A's dew point at 258.27 K, accepted at the full gate, recomputed 1.0016e-12 off).
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

/// Bisections of Wilson's seed inside the dome, in ln p or 1/T.
const WILSON_BISECTIONS: usize = 80;

/// A phase of `x` at (T, p) on `branch`, from `seed` where one is known, polished, to `order`.
fn on_branch(mixture: &Mixture, t: f64, p: f64, x: &[f64], branch: Branch, seed: Option<f64>) -> Result<AtTp, Error> {
    let phase = Phase { mixture, x };
    let rho = polished(&phase, t, p, branch_root(&phase, t, p, branch, seed)?);
    AtTp::new(mixture, t, p, rho, x, Order::Two)
}

/// Whether the equations `f` (F_1 .. F_N in ln K, then Rachford–Rice's Σ(y − x)) meet `GATE` at the point
/// returned, whose fractions are x/Σx and y/Σy: Σx = 1 − βF_{N+1} and Σy = 1 + (1 − β)F_{N+1} there, so
/// ln(y_i/x_i) is ln K_i − F_{N+1} to its square (1e-24 inside the gate), each F_i is judged with that shift, and
/// F_{N+1} itself.
fn meets_gate(f: &[f64]) -> bool {
    let Some((balance, fugacities)) = f.split_last() else { return false };
    balance.abs() <= GATE && fugacities.iter().all(|f| (f - balance).abs() <= GATE)
}

/// Whether an iterate is the trivial solution (`TRIVIAL`): every |ln K_i| and |ln(ρ′/ρ″)| small.
fn trivial(ln_k: &[f64], rho_liquid: f64, rho_vapour: f64) -> bool {
    ln_k.iter().all(|l| l.abs() < TRIVIAL.0) && math::ln(rho_liquid / rho_vapour).abs() < TRIVIAL.1
}

/// Rachford–Rice at `beta` for Wilson's K at (T, p), K_i = (p_c,i/p) exp(a_i(T)): Σ z_i (K_i − 1)/(1 + β(K_i − 1)),
/// which falls as p rises.
fn wilson_balance(mixture: &Mixture, beta: f64, t: f64, p: f64) -> f64 {
    let parts = wilson_parts(mixture, t);
    parts
        .iter()
        .zip(mixture.composition())
        .map(|((pc, a), z)| {
            let k = pc / p * math::exp(*a);
            z * (k - 1.0) / (1.0 + beta * (k - 1.0))
        })
        .sum()
}

/// Wilson's seed: the p at T where Rachford–Rice at `beta` holds for his K. At the bubble point (β = 0) it is
/// Σ z_i p_c,i e^(a_i), at the dew point (β = 1) 1/Σ z_i/(p_c,i e^(a_i)); between them, by bisection in ln p.
fn wilson_p(mixture: &Mixture, beta: f64, t: f64) -> f64 {
    let parts = wilson_parts(mixture, t);
    let z = mixture.composition();
    let bubble: f64 = parts.iter().zip(z).map(|((pc, a), z)| z * pc * math::exp(*a)).sum();
    let dew = 1.0 / parts.iter().zip(z).map(|((pc, a), z)| z / (pc * math::exp(*a))).sum::<f64>();
    if beta == 0.0 {
        return bubble;
    }
    if beta == 1.0 {
        return dew;
    }
    let (mut lo, mut hi) = (math::ln(dew), math::ln(bubble));
    for _ in 0..WILSON_BISECTIONS {
        let mid = (lo + hi) / 2.0;
        if wilson_balance(mixture, beta, t, math::exp(mid)) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    math::exp((lo + hi) / 2.0)
}

/// Wilson's seed: the T at p, by bisection in 1/T over the mixture's range, where Wilson's p at `beta` is p (it rises
/// with T).
fn wilson_t(mixture: &Mixture, beta: f64, p: f64) -> f64 {
    let (mut lo, mut hi) = mixture.limits().map_or((1.0, 1e4), |l| (l.t_min(), l.t_max()));
    for _ in 0..WILSON_BISECTIONS {
        let mid = 2.0 / (1.0 / lo + 1.0 / hi);
        if wilson_p(mixture, beta, mid) < p {
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
    let found = at_side(mixture, side, given)?;
    let (feed, incipient) = match side {
        Side::Bubble => ((found.rho_liquid, found.y), found.rho_vapour),
        Side::Dew => ((found.rho_vapour, found.x), found.rho_liquid),
    };
    Ok(Saturation { t: found.t, p: found.p, rho: feed.0, incipient: feed.1, rho_incipient: incipient })
}

/// The bubble or dew point as two coexisting phases (`saturation`'s solve).
fn at_side(mixture: &Mixture, side: Side, given: Given) -> Result<Coexisting, Error> {
    let z = mixture.composition().to_vec();
    let beta = side.beta();
    let solve = |seed: (f64, f64, Vec<f64>)| Solver::new(mixture, beta, &z).newton(given, seed);
    let first = match solve(Solver::new(mixture, beta, &z).seed_wilson(given)) {
        Ok(found) => return Ok(found),
        Err(error) => error,
    };
    let search = SplitSearch { mixture, side, given };
    let Some(split) = search.first() else { return Err(first) };
    solve(search.seed(&split)).or_else(|_| search.boundary(split).map_or(Err(first), |near| solve(search.seed(&near))))
}

/// The two phases of `mixture` at vapour fraction `beta` in [0, 1] and `given` T or p: from Wilson's seed at β; else,
/// inside the dome, from the bubble and dew points there, ln X and ln K interpolated in β.
pub(crate) fn coexisting(mixture: &Mixture, beta: f64, given: Given) -> Result<Coexisting, Error> {
    if beta == 0.0 || beta == 1.0 {
        return at_side(mixture, if beta == 0.0 { Side::Bubble } else { Side::Dew }, given);
    }
    let z = mixture.composition().to_vec();
    let first = match Solver::new(mixture, beta, &z).newton(given, Solver::new(mixture, beta, &z).seed_wilson(given)) {
        Ok(found) => return Ok(found),
        Err(error) => error,
    };
    let (bubble, dew) = (at_side(mixture, Side::Bubble, given)?, at_side(mixture, Side::Dew, given)?);
    Solver::new(mixture, beta, &z).newton(given, between(given, &bubble, &dew, beta)).map_err(|_| first)
}

/// Regula falsi steps of the isochoric split in β, and the bracket's width in β at which it ends.
const ISOCHORIC_ITERATIONS: usize = 100;
const ISOCHORIC_WIDTH: f64 = 1e-14;

/// The split at (T, ρ) of a feed unstable there (PLAN.md M13.10): the vapour fraction β at which the two phases' molar
/// volume (1 − β)/ρ′ + β/ρ″ is 1/ρ. That volume rises with β from the bubble point (β = 0) to the dew point (β = 1) at T,
/// so β is found by the Illinois regula falsi between them, each point's Newton solve seeded by the points solved nearest
/// it on either side, ln p and ln K interpolated in β (and where that fails, solved as `coexisting` solves it), to
/// `ISOCHORIC_WIDTH`. `None` where T has no bubble or dew point, or 1/ρ lies outside their volumes (above a
/// mixture's critical temperature, where two dew points bound the dome).
pub(crate) fn isochoric(mixture: &Mixture, t: f64, rho: f64) -> Result<Option<Coexisting>, Error> {
    let given = Given::T(t);
    let (Ok(bubble), Ok(dew)) = (at_side(mixture, Side::Bubble, given), at_side(mixture, Side::Dew, given)) else {
        return Ok(None);
    };
    // ln(vρ) of a point: negative below the volume sought.
    let g = |c: &Coexisting| Ok(math::ln(((1.0 - c.beta) / c.rho_liquid + c.beta / c.rho_vapour) * rho));
    in_beta(mixture, given, (bubble, dew), &g)
}

/// The two phases at `given` where `g` is 0, g rising with β from `bubble` (β = 0) to `dew` (β = 1), the bubble and dew
/// points at `given`: the Illinois regula falsi in β, each point's Newton solve seeded by the points solved nearest it on
/// either side, the other of T and p and ln K interpolated in β (and where that fails, solved as `coexisting` solves it),
/// to `ISOCHORIC_WIDTH`. `None` where g does not change sign between them.
pub(crate) fn in_beta(
    mixture: &Mixture,
    given: Given,
    (bubble, dew): (Coexisting, Coexisting),
    g: &dyn Fn(&Coexisting) -> Result<f64, Error>,
) -> Result<Option<Coexisting>, Error> {
    let z = mixture.composition().to_vec();
    let (lo, hi) = (End::new(0.0, (g(&bubble)?, bubble.clone())), End::new(1.0, (g(&dew)?, dew.clone())));
    if !(lo.value < 0.0 && hi.value > 0.0) {
        return Ok(None);
    }
    // Every point solved, for the next one's seed from its neighbours in β.
    let mut known = vec![bubble, dew];
    let mut at = |beta: f64| -> Result<(f64, Coexisting), Error> {
        let seed = neighbours(&known, beta).map(|(a, b)| between(given, a, b, beta));
        let solved = seed.map(|seed| Solver::new(mixture, beta, &z).newton(given, seed));
        let c = match solved {
            Some(Ok(c)) => c,
            _ => coexisting(mixture, beta, given)?,
        };
        known.push(c.clone());
        Ok((g(&c)?, c))
    };
    Ok(Some(illinois(lo, hi, &mut at, (ISOCHORIC_WIDTH, ISOCHORIC_ITERATIONS))?.at))
}

/// The points of `known` nearest `beta` on either side: the highest β below it and the lowest above it.
fn neighbours(known: &[Coexisting], beta: f64) -> Option<(&Coexisting, &Coexisting)> {
    let below = known.iter().filter(|c| c.beta < beta).max_by(|a, b| a.beta.total_cmp(&b.beta));
    let above = known.iter().filter(|c| c.beta > beta).min_by(|a, b| a.beta.total_cmp(&b.beta));
    below.zip(above)
}

/// The Newton seed at `beta` between two coexisting points `a` and `b` at `given`: the given T or p itself, ln K and the
/// logarithm of the other interpolated in β.
fn between(given: Given, a: &Coexisting, b: &Coexisting, beta: f64) -> (f64, f64, Vec<f64>) {
    let s = (beta - a.beta) / (b.beta - a.beta);
    let ln_k: Vec<f64> = ln_k(a).iter().zip(ln_k(b)).map(|(a, b)| a + s * (b - a)).collect();
    let log = |a: f64, b: f64| math::exp(math::ln(a) + s * (math::ln(b) - math::ln(a)));
    match given {
        Given::T(t) => (t, log(a.p, b.p), ln_k),
        Given::P(p) => (log(a.t, b.t), p, ln_k),
    }
}

/// ln K = ln(y/x) of two coexisting phases.
pub(crate) fn ln_k(c: &Coexisting) -> Vec<f64> {
    ln_k_of(&c.x, &c.y)
}

/// ln K_i = ln(y_i/x_i) of a liquid's fractions `x` and a vapour's `y`.
pub(crate) fn ln_k_of(x: &[f64], y: &[f64]) -> Vec<f64> {
    x.iter().zip(y).map(|(x, y)| math::ln(y / x)).collect()
}

/// The two phases at `beta` and `given` from `seed` (T, p, ln K), a point solved nearby: M13.10's Newton from it, without
/// `coexisting`'s seeds (M13.11's outer solves step along a path). The given T or p is the seed's.
pub(crate) fn coexisting_from(
    mixture: &Mixture,
    beta: f64,
    given: Given,
    seed: (f64, f64, Vec<f64>),
) -> Result<Coexisting, Error> {
    let z = mixture.composition().to_vec();
    Solver::new(mixture, beta, &z).newton(given, seed)
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
            Given::T(t) => (math::ln(wilson_p(self.mixture, 0.0, t)), math::ln(wilson_p(self.mixture, 1.0, t))),
            Given::P(p) => (math::ln(wilson_t(self.mixture, 0.0, p)), math::ln(wilson_t(self.mixture, 1.0, p))),
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

/// The Newton solve's state: the feed, the vapour fraction, and each phase's last density (liquid, vapour) for the
/// next solve's seed.
struct Solver<'a> {
    mixture: &'a Mixture,
    beta: f64,
    z: &'a [f64],
    seeds: (Option<f64>, Option<f64>),
}

/// The phases at an iterate: the liquid and the vapour, and their fractions x and y as the mass balance gives them
/// (summing to 1 only where Rachford–Rice holds).
struct Phases {
    liquid: AtTp,
    vapour: AtTp,
    x: Vec<f64>,
    y: Vec<f64>,
}

impl<'a> Solver<'a> {
    fn new(mixture: &'a Mixture, beta: f64, z: &'a [f64]) -> Solver<'a> {
        Solver { mixture, beta, z, seeds: (None, None) }
    }

    /// x_i = z_i/(1 + β(K_i − 1)) and y_i = K_i x_i.
    fn fractions(&self, ln_k: &[f64]) -> (Vec<f64>, Vec<f64>) {
        self.z
            .iter()
            .zip(ln_k)
            .map(|(z, l)| {
                let k = math::exp(*l);
                let x = z / (1.0 + self.beta * (k - 1.0));
                (x, k * x)
            })
            .unzip()
    }

    /// Wilson's seed at `given` and up to `SUBSTITUTIONS` successive substitutions from it: K from the phases' fugacity
    /// coefficients, and at a given T p by Σy/Σx (p Σ z_i K_i at a bubble point, p/Σ z_i/K_i at a dew point); at a
    /// given p, Newton's method moves T. (T, p, ln K).
    fn seed_wilson(&mut self, given: Given) -> (f64, f64, Vec<f64>) {
        let (t, mut p) = match given {
            Given::T(t) => (t, wilson_p(self.mixture, self.beta, t)),
            Given::P(p) => (wilson_t(self.mixture, self.beta, p), p),
        };
        let mut ln_k: Vec<f64> = wilson_parts(self.mixture, t).iter().map(|(pc, a)| math::ln(pc / p) + a).collect();
        for _ in 0..SUBSTITUTIONS {
            let Ok(at) = self.phases(t, p, &ln_k) else { break };
            ln_k = at.liquid.ln_phi().iter().zip(at.vapour.ln_phi()).map(|(l, v)| l - v).collect();
            if let Given::T(_) = given {
                p *= at.y.iter().sum::<f64>() / at.x.iter().sum::<f64>();
            }
        }
        (t, p, ln_k)
    }

    /// The liquid and the vapour at (T, p) from `ln_k`, each on its branch.
    fn phases(&mut self, t: f64, p: f64, ln_k: &[f64]) -> Result<Phases, Error> {
        let (x, y) = self.fractions(ln_k);
        let liquid = on_branch(self.mixture, t, p, &normalised(&x), Branch::Liquid, self.seeds.0)?;
        let vapour = on_branch(self.mixture, t, p, &normalised(&y), Branch::Vapour, self.seeds.1)?;
        self.seeds = (Some(liquid.rho), Some(vapour.rho));
        Ok(Phases { liquid, vapour, x, y })
    }

    /// The N + 1 equations at (T, p, ln K), F_i = ln K_i + ln φ_i^V − ln φ_i^L and Σ(y − x), with the phases they were
    /// evaluated on.
    fn equations(&mut self, t: f64, p: f64, ln_k: &[f64]) -> Result<(Vec<f64>, Phases), Error> {
        let at = self.phases(t, p, ln_k)?;
        let (liquid, vapour) = (at.liquid.ln_phi(), at.vapour.ln_phi());
        let mut f: Vec<f64> = (0..self.z.len()).map(|i| ln_k[i] + vapour[i] - liquid[i]).collect();
        f.push(at.y.iter().zip(&at.x).map(|(y, x)| y - x).sum());
        Ok((f, at))
    }

    /// Newton's method on the N + 1 equations in (ln K, ln X) from `seed` (T, p, ln K), each step cut to 0.5 in ln K and
    /// 0.1 in ln X, to the residual gate.
    fn newton(&mut self, given: Given, (mut t, mut p, mut ln_k): (f64, f64, Vec<f64>)) -> Result<Coexisting, Error> {
        let failed = |iterations| Error::NoConvergence { strategy: Strategy::Vle, iterations };
        let n = self.z.len();
        for iteration in 0..NEWTON_STEPS {
            let (f, at) = self.equations(t, p, &ln_k)?;
            // The trivial solution: both phases one, on the same root. An azeotrope's K_i are all near 1 too, but its
            // phases' densities are a liquid's and a vapour's.
            if trivial(&ln_k, at.liquid.rho, at.vapour.rho) {
                return Err(failed(iteration));
            }
            if meets_gate(&f) {
                return Ok(Coexisting {
                    t,
                    p,
                    beta: self.beta,
                    x: normalised(&at.x),
                    y: normalised(&at.y),
                    rho_liquid: at.liquid.rho,
                    rho_vapour: at.vapour.rho,
                });
            }
            let jacobian = self.jacobian(given, (t, p), &at);
            let Some(step) = solve(jacobian, f.iter().map(|f| -f).collect()) else { return Err(failed(iteration)) };
            let cut = |s: f64, limit: f64| s.clamp(-limit, limit);
            for (l, s) in ln_k.iter_mut().zip(&step) {
                *l += cut(*s, 0.5);
            }
            let dx = cut(step[n], 0.1);
            match given {
                Given::T(_) => p *= math::exp(dx),
                Given::P(_) => t *= math::exp(dx),
            }
        }
        Err(failed(NEWTON_STEPS))
    }

    /// The Jacobian of (F_1 .. F_N, F_{N+1}) in (ln K_1 .. ln K_N, ln X).
    fn jacobian(&self, given: Given, (t, p): (f64, f64), at: &Phases) -> Vec<Vec<f64>> {
        let (n, beta) = (self.z.len(), self.beta);
        // ∂x_k/∂ln K_k = −β x_k y_k/z_k and ∂y_k/∂ln K_k = (1 − β) x_k y_k/z_k. ln φ depends on the fractions alone, so
        // its derivative in a phase's amounts at x (or y) is Φ/Σx (Φ/Σy): both sums are 1 where Rachford–Rice holds.
        let moves: Vec<f64> = (0..n).map(|k| at.x[k] * at.y[k] / self.z[k]).collect();
        let (sum_x, sum_y) = (at.x.iter().sum::<f64>(), at.y.iter().sum::<f64>());
        let phi = |i: usize, k: usize| {
            (1.0 - beta) * at.vapour.c.n_dln_phi_dn(i, k) / sum_y + beta * at.liquid.c.n_dln_phi_dn(i, k) / sum_x
        };
        let d_ln_x = |a: &AtTp, i: usize| match given {
            Given::T(_) => p * a.c.dln_phi_dp(i),
            Given::P(_) => t * a.c.dln_phi_dt(i),
        };
        square(n + 1, |i, k| match (i < n, k < n) {
            (true, true) => (if i == k { 1.0 } else { 0.0 }) + moves[k] * phi(i, k),
            (true, false) => d_ln_x(&at.vapour, i) - d_ln_x(&at.liquid, i),
            (false, true) => moves[k],
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

    /// The kinds of point: a bubble point, a dew point and β = 0.4, each at a given T (200 K) and a given p (1 MPa).
    const KINDS: [(f64, Given); 6] = [
        (0.0, Given::T(200.0)),
        (1.0, Given::T(200.0)),
        (0.4, Given::T(200.0)),
        (0.0, Given::P(1e6)),
        (1.0, Given::P(1e6)),
        (0.4, Given::P(1e6)),
    ];

    /// Wilson's seeds are his K's Rachford–Rice: Σ z_i p_c,i e^(a_i) at a bubble point and 1/Σ z_i/(p_c,i e^(a_i)) at a
    /// dew point, a_i = 5.373 (1 + ω_i)(1 − T_c,i/T), from each component's published critical point and acentric
    /// factor; at β = 0.4 Rachford–Rice vanishes at Wilson's p within 1e-12; and Wilson's T at p inverts his p, by
    /// bisection in 1/T, to 1e-12.
    #[test]
    fn wilson_seeds_solve_his_rachford_rice() {
        let m = methane_ethane(0.4, false);
        let t = 200.0;
        let parts: Vec<f64> = m
            .model()
            .fluids()
            .map(|f| {
                let c = f.info().published_critical().unwrap_or_else(|| panic!("critical"));
                let omega = f.constant(Constant::Acentric).unwrap_or(f64::NAN);
                c.p * math::exp(5.373 * (1.0 + omega) * (1.0 - c.t / t))
            })
            .collect();
        let bubble = 0.4 * parts[0] + 0.6 * parts[1];
        let dew = 1.0 / (0.4 / parts[0] + 0.6 / parts[1]);
        assert!(
            (wilson_p(&m, 0.0, t) / bubble - 1.0).abs() <= 1e-15 && (wilson_p(&m, 1.0, t) / dew - 1.0).abs() <= 1e-15
        );
        let p = wilson_p(&m, 0.4, t);
        assert!(dew < p && p < bubble && wilson_balance(&m, 0.4, t, p).abs() <= 1e-12, "{p}");
        for beta in [0.0, 0.4, 1.0] {
            let t = wilson_t(&m, beta, 1e6);
            assert!((wilson_p(&m, beta, t) / 1e6 - 1.0).abs() <= 1e-12, "{beta}: {t} K");
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

    /// The gate judges the point returned, the fractions x/Σx and y/Σy: each F_i shifted by −F_{N+1}, and F_{N+1}
    /// itself, each within half the fugacity gate, 5e-13, the gate included.
    #[test]
    fn the_gate_judges_the_point_returned() {
        let g = 5e-13;
        assert!(meets_gate(&[g, g, g]) && !meets_gate(&[-g, -g, g]) && meets_gate(&[0.0, 0.0, -g]));
        assert!(meets_gate(&[2.0 * g, 0.0, g]) && !meets_gate(&[2.5 * g, 0.0, g]));
        assert!(!meets_gate(&[1.5 * g, 1.5 * g, 1.5 * g]) && !meets_gate(&[]));
    }

    /// The mass balance gives the phases: x_i = z_i/(1 + β(K_i − 1)) and y_i = K_i x_i, so (1 − β)x + βy = z; at β = 0
    /// x is the feed and at β = 1 y is.
    #[test]
    fn the_phases_add_up_to_the_feed() {
        let m = methane_ethane(0.4, false);
        let z = m.composition().to_vec();
        let ln_k = [math::ln(4.0), math::ln(0.25)];
        for beta in [0.0, 0.25, 1.0] {
            let (x, y) = Solver::new(&m, beta, &z).fractions(&ln_k);
            for i in 0..2 {
                let balance = (1.0 - beta) * x[i] + beta * y[i] - z[i];
                assert!(
                    balance.abs() <= 1e-15 && (y[i] / x[i] - [4.0, 0.25][i]).abs() <= 1e-15,
                    "{beta} {i}: {x:?} {y:?}"
                );
            }
        }
        assert_eq!(Solver::new(&m, 0.0, &z).fractions(&ln_k).0, z);
        assert_eq!(Solver::new(&m, 1.0, &z).fractions(&ln_k).1, z);
        assert_eq!(Solver::new(&m, 0.25, &z).fractions(&ln_k).0, vec![0.4 / 1.75, 0.6 / 0.8125]);
    }

    /// Successive substitution from Wilson's seed lands within 0.05 in ln p and ln T together and 0.06 in ln K of the
    /// point, for each kind of point.
    #[test]
    fn substitution_moves_wilsons_seed_to_the_point() {
        let m = methane_ethane(0.4, false);
        let z = m.composition().to_vec();
        for (beta, given) in KINDS {
            let c = coexisting(&m, beta, given).unwrap_or_else(|e| panic!("{e:?}"));
            let (t, p, ln_k) = Solver::new(&m, beta, &z).seed_wilson(given);
            let off = math::ln(p / c.p).abs() + math::ln(t / c.t).abs();
            let k_off = ln_k.iter().zip(super::ln_k(&c)).fold(0.0_f64, |m, (a, b)| m.max((a - b).abs()));
            assert!(off < 0.05 && k_off < 0.06, "{beta} {given:?}: {off} {k_off}");
        }
    }

    /// The Newton step's Jacobian is the equations' derivative in (ln K, ln X): against central differences of step
    /// 1e-6, within 1e-6 of each column's largest entry, off the solution (ln K + 0.05, ln X + 0.02), for each kind of
    /// point.
    #[test]
    fn the_jacobian_is_the_equations_derivative() {
        let m = methane_ethane(0.4, false);
        let z = m.composition().to_vec();
        for (beta, given) in KINDS {
            let c = coexisting(&m, beta, given).unwrap_or_else(|e| panic!("{e:?}"));
            let ln_k: Vec<f64> = super::ln_k(&c).iter().map(|l| l + 0.05).collect();
            let (t, p) = match given {
                Given::T(_) => (c.t, c.p * math::exp(0.02)),
                Given::P(_) => (c.t * math::exp(0.02), c.p),
            };
            let mut solver = Solver::new(&m, beta, &z);
            let (_, at) = solver.equations(t, p, &ln_k).unwrap_or_else(|e| panic!("{e:?}"));
            let jacobian = solver.jacobian(given, (t, p), &at);
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
                    solver.equations(t, p, &ln_k).unwrap_or_else(|e| panic!("{e:?}")).0
                };
                let (up, down) = (moved(h), moved(-h));
                let column: Vec<f64> = up.iter().zip(&down).map(|(u, d)| (u - d) / (2.0 * h)).collect();
                let scale = column.iter().fold(1.0_f64, |m, c| m.max(c.abs()));
                for (i, fd) in column.iter().enumerate() {
                    let at = format!("{beta} {given:?} ({i}, {k}): {} against {fd}", jacobian[i][k]);
                    assert!((jacobian[i][k] - fd).abs() <= 1e-6 * scale, "{at}");
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
        let (own, other) = (math::ln(wilson_p(&m, 0.0, 200.0)), math::ln(wilson_p(&m, 1.0, 200.0)));
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

    /// Inside the dome the bubble and dew points seed the solve where Wilson's does not: the seed interpolated at β = 0.4
    /// between them converges to the same point as the solve.
    #[test]
    fn the_dome_s_edges_seed_its_inside() {
        let m = methane_ethane(0.5, false);
        let z = m.composition().to_vec();
        let given = Given::T(200.0);
        let (bubble, dew) = (at_side(&m, Side::Bubble, given), at_side(&m, Side::Dew, given));
        let (bubble, dew) = (bubble.unwrap_or_else(|e| panic!("{e:?}")), dew.unwrap_or_else(|e| panic!("{e:?}")));
        let seed_k: Vec<f64> =
            super::ln_k(&bubble).iter().zip(super::ln_k(&dew)).map(|(b, d)| 0.6 * b + 0.4 * d).collect();
        let p = math::exp(0.6 * math::ln(bubble.p) + 0.4 * math::ln(dew.p));
        let from_edges = Solver::new(&m, 0.4, &z).newton(given, (200.0, p, seed_k)).unwrap_or_else(|e| panic!("{e:?}"));
        let solved = coexisting(&m, 0.4, given).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(
            (from_edges.p / solved.p - 1.0).abs() <= 1e-12 && dew.p < solved.p && solved.p < bubble.p,
            "{solved:?}"
        );
    }

    /// A point of two phases for the seeds' tests: T, p, β and each phase's first fraction.
    fn point(t: f64, p: f64, beta: f64, (x1, y1): (f64, f64)) -> Coexisting {
        Coexisting { t, p, beta, x: vec![x1, 1.0 - x1], y: vec![y1, 1.0 - y1], rho_liquid: 1e4, rho_vapour: 1e2 }
    }

    /// A seed between two points keeps the given T or p itself and interpolates ln K and the logarithm of the other in β:
    /// a quarter of the way from β = 0.25 to 0.75, p at 2 MPa between 1 and 16, ln K a quarter of the way; at a given p,
    /// T at 400 K between 200 and 3200.
    #[test]
    fn seeds_between_two_points_interpolate_in_beta() {
        let (a, b) = (point(200.0, 1e6, 0.25, (0.4, 0.8)), point(200.0, 16e6, 0.75, (0.5, 0.6)));
        let (t, p, k) = between(Given::T(200.0), &a, &b, 0.375);
        let k_quarter: Vec<f64> = ln_k(&a).iter().zip(ln_k(&b)).map(|(a, b)| a + 0.25 * (b - a)).collect();
        assert!(t == 200.0 && (p / 2e6 - 1.0).abs() <= 1e-15 && k == k_quarter, "{t} {p} {k:?}");
        let (a, b) = (point(200.0, 1e6, 0.25, (0.4, 0.8)), point(3200.0, 1e6, 0.75, (0.5, 0.6)));
        let (t, p, _) = between(Given::P(1e6), &a, &b, 0.375);
        assert!((t / 400.0 - 1.0).abs() <= 1e-15 && p == 1e6, "{t} {p}");
        let want = [math::ln(2.0), math::ln(1.0 / 3.0)];
        assert!(ln_k(&a).iter().zip(want).all(|(got, want)| (got - want).abs() <= 1e-15), "{:?}", ln_k(&a));
    }

    /// The seeds' neighbours are the points nearest β on either side, strictly: at 0.6 those at 0.5 and 0.75, at 0.5
    /// those at 0.25 and 0.75; none at the ends.
    #[test]
    fn the_neighbours_are_the_nearest_on_either_side() {
        let known: Vec<Coexisting> = [0.0, 1.0, 0.5, 0.25, 0.75].map(|b| point(200.0, 1e6, b, (0.4, 0.8))).to_vec();
        let betas = |n: Option<(&Coexisting, &Coexisting)>| n.map(|(a, b)| (a.beta, b.beta));
        assert_eq!(betas(neighbours(&known, 0.6)), Some((0.5, 0.75)));
        assert_eq!(betas(neighbours(&known, 0.5)), Some((0.25, 0.75)));
        assert_eq!((betas(neighbours(&known, 0.0)), betas(neighbours(&known, 1.0))), (None, None));
    }

    /// The isochoric split (M13.10): at 200 K, a density between the dew point's vapour and the bubble point's liquid
    /// gives two phases whose molar volume is 1/ρ within 1e-13, β inside (0, 1); a density outside them gives none, as
    /// does a T above the critical point's, where T has no bubble point.
    #[test]
    fn the_isochoric_split_has_the_density() {
        let m = methane_ethane(0.5, false);
        let t = 200.0;
        let (bubble, dew) = (at_side(&m, Side::Bubble, Given::T(t)), at_side(&m, Side::Dew, Given::T(t)));
        let (bubble, dew) = (bubble.unwrap_or_else(|e| panic!("{e:?}")), dew.unwrap_or_else(|e| panic!("{e:?}")));
        let (v_liquid, v_vapour) = (math::ln(1.0 / bubble.rho_liquid), math::ln(1.0 / dew.rho_vapour));
        for share in [0.1, 0.5, 0.9] {
            let v = math::exp(v_liquid + share * (v_vapour - v_liquid));
            let c = isochoric(&m, t, 1.0 / v).unwrap_or_else(|e| panic!("{e:?}")).unwrap_or_else(|| panic!("{share}"));
            let volume = (1.0 - c.beta) / c.rho_liquid + c.beta / c.rho_vapour;
            assert!((volume / v - 1.0).abs() <= 1e-13 && c.beta > 0.0 && c.beta < 1.0, "{share}: {c:?}");
        }
        assert_eq!(isochoric(&m, t, 1.01 * bubble.rho_liquid), Ok(None));
        assert_eq!(isochoric(&m, t, 0.99 * dew.rho_vapour), Ok(None));
        assert_eq!(isochoric(&m, 300.0, 8000.0), Ok(None));
    }
}
