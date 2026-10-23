//! Every other input pair (PLAN.md M13.11): outer solves along a path of the state space, on the feed's homogeneous
//! states and on the coexisting phases of M13.9 and M13.10.
//!
//! - A single phase is solved on the feed's homogeneous states, one evaluation of the model each, and kept where it is
//!   stable: mechanically and by Michelsen's tangent-plane test at its own pressure, DT's check. One tangent-plane test
//!   of a five-component GERG-2008 gas costs as much as several thousand evaluations, so a search never runs one per
//!   step; only where the root it finds is unstable are the dome's phases solved.
//! - Two phases coexist at a vapour fraction β (M13.10's Newton), each point seeded by the points solved nearest it.
//! - HP, PS and PU along the isobar, where x rises with T, through the dome too ((∂h/∂T)_p = c_p): the root in T of x on
//!   the feed's stable root, else the root in β between the bubble and dew points at p.
//! - DP, DH, DS and DU along the isochore, where x rises with T ((∂u/∂T)_ρ = c_v): the root in T of the homogeneous
//!   state's x, else Newton's method in (ln T, β) on the coexisting phases, from the isochoric split at that T.
//! - HT, ST and TU along the isotherm, where x need not be monotonic: the homogeneous isotherm cut at its turns on core's
//!   grid in ln ρ, each root kept where it is stable; where one is not, the root in β between the bubble and dew points
//!   at T too. Several are `Ambiguous` under `Strict` and `Stable`, keyed by density; `Nearest` takes the nearest.
//! - HS and SU along the isentrope, where h and u rise with p ((∂h/∂p)_s = v): the root in ln p of x at PS(p, s), down
//!   from the model's highest pressure by decades, each PS the isobar's solve, through the dome too.
//! - QS, HQ and DQ along the line of quality Q, molar or by mass: the two phases at Q on a grid in T from the domain's
//!   lowest temperature to the line's top, cut at its turns, each root by the regula falsi in T. Several are
//!   `Ambiguous`, keyed by T, as core's.
//! - A search in T descends from 1.5·Tmax in steps of 20 % until x falls below the input, as core's, so it never
//!   evaluates far below its root; then the Illinois regula falsi in the step that brackets it.
//! - Each answer reproduces its input within core's acceptance gate (D6): 1e-9 of max(|x|, RT), of max(|s|, R) for s,
//!   of p or of ρ. A single-phase hint takes the homogeneous root without the stability test, as hinted DT does, and
//!   `TwoPhase` the coexisting one.

use phasekit_core::density::Branch;
use phasekit_core::{
    Basis, DerivVar, Derivs, DomainError, DomainPolicy, Error, FlashOptions, HelmholtzModel, Limit, NativeInput, Order,
    Pair, Partial, Phase, Quantity, RootPolicy, Roots, State, Strategy, ThermoModel, Var, math,
};

use super::{Equilibrium, Mixture, pressure};
use crate::equilibrium;
use crate::illinois::{End, illinois};
use crate::quantities::VAPOUR_FRACTION;
use crate::saturation::{self, Coexisting, Given, ln_k};
use crate::slot;

/// Regula falsi steps of a search, and its bracket's width: relative in T and in ln ρ's ρ, absolute in β.
const STEPS: usize = 100;
const WIDTH: f64 = 1e-14;

/// A search in T steps down from its top by this factor until x falls below the input (core's 20 %).
const DESCENT: f64 = 0.8;

/// An isotherm's search: the decades of ρ below the model's largest density, its grid's nodes per decade and its fewest
/// cells (core's T+X).
const DECADES: f64 = 15.0;
const NODES_PER_DECADE: f64 = 8.0;
const CELLS: f64 = 16.0;

/// A line of quality's grid: its cells between the domain's lowest temperature and 1.2 T_c, the halvings that find its
/// top where a point first fails, and the golden-section steps that find a turn.
const LINE_CELLS: usize = 24;
const LINE_HALVINGS: usize = 20;
const GOLDEN_STEPS: usize = 40;

/// The acceptance gate's share of an input's scale (core's D6).
const REPRODUCED: f64 = 1e-9;

/// A dome's bracket along a path: the factor in T each step takes from the homogeneous root toward the sign change,
/// and the steps it may take.
const STEP_T: f64 = 1.05;
const BRACKET_STEPS: u16 = 30;

/// Where a dome's bracket starts without a homogeneous root: this share of the critical temperature.
const DOME_START: f64 = 0.95;

/// Newton's method on coexisting phases: its steps, its forward differences in ln X and β, its largest step in ln X and
/// the share of each input's scale it ends within.
const DOME_STEPS: usize = 30;
const DOME_DIFF: f64 = 1e-6;
const DOME_STEP: f64 = 0.1;
const DOME_GATE: f64 = 1e-11;

/// A state an outer solve finds: one phase at (T, ρ), or two coexisting phases.
#[derive(Clone, Debug)]
enum Found {
    Single(f64, f64),
    Two(Coexisting),
}

/// Search states may lie outside the limits; an answer is rebuilt under the caller's options.
fn searching() -> FlashOptions {
    FlashOptions::new().with_domain(DomainPolicy::Extrapolate)
}

/// `var`'s value at `state`, molar, in the native gauge.
fn value(state: &State, var: Var) -> f64 {
    match var {
        Var::T => state.t(),
        Var::P => state.p(),
        Var::D => state.rho(Basis::Molar),
        Var::H => state.h(Basis::Molar),
        Var::S => state.s(Basis::Molar),
        Var::U => state.u(Basis::Molar),
        _ => state.quality().unwrap_or(f64::NAN),
    }
}

/// The scale an input `x` of `var` is reproduced to at T (core's D6): |x|, at least RT for h and u and R for s.
fn scale(var: Var, r: f64, t: f64, x: f64) -> f64 {
    match var {
        Var::H | Var::U => x.abs().max(r * t),
        Var::S => x.abs().max(r),
        _ => x.abs(),
    }
}

/// `got` less the input `x` of `var` at T, in the scale x is reproduced to.
fn scaled(var: Var, r: f64, t: f64, x: f64, got: f64) -> f64 {
    (got - x) / scale(var, r, t, x)
}

/// Whether `state` reproduces the input `x` of `var` within the acceptance gate.
fn reproduces(var: Var, r: f64, x: f64, state: &State) -> bool {
    scaled(var, r, state.t(), x, value(state, var)).abs() <= REPRODUCED
}

/// The caloric variable's (∂x/∂ρ)_T, whose sign finds an isotherm's turns.
fn slope(state: &State, var: Var) -> Result<f64, Error> {
    let of = match var {
        Var::H => DerivVar::Hmolar,
        Var::S => DerivVar::Smolar,
        _ => DerivVar::Umolar,
    };
    state.partial(Partial { of, wrt: DerivVar::Dmolar, at: DerivVar::T })
}

/// The branch a single-phase hint names: the liquid's for a liquid, the vapour's otherwise.
fn branch_of(phase: Phase) -> Branch {
    if matches!(phase, Phase::Liquid | Phase::SupercriticalLiquid) { Branch::Liquid } else { Branch::Vapour }
}

/// The root `policy` takes among `states`, ascending by `key` (core's rule for every pair): the only one; of several,
/// under `Nearest(x)` the one nearest x, else `Ambiguous` with every key. `none` without one.
fn choose(
    mut states: Vec<State>,
    (key, quantity): (fn(&State) -> f64, &'static Quantity),
    policy: RootPolicy,
    none: Error,
) -> Result<State, Error> {
    states.sort_by(|a, b| key(a).total_cmp(&key(b)));
    states.dedup_by(|a, b| (key(a) - key(b)).abs() <= WIDTH * key(b).abs());
    let keys: Vec<f64> = states.iter().map(key).collect();
    match (states.len(), policy) {
        (0, _) => Err(none),
        (1, _) => Ok(states.remove(0)),
        (_, RootPolicy::Nearest(x)) => {
            let distance = |k: &f64| (k - x).abs();
            let nearest = keys.iter().enumerate().min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)));
            Ok(states.remove(nearest.map_or(0, |(k, _)| k)))
        }
        _ => Err(Error::Ambiguous { roots: Roots::new(quantity, &keys) }),
    }
}

/// The residual x(state) − x nearest zero of those a search saw: where it found no root, every state's x lies on one
/// side of x, and this one's is the extreme it reached (PLAN.md M29.2; user decision EM16).
#[derive(Clone, Copy, Debug, Default)]
struct Reached(Option<f64>);

impl Reached {
    /// Notes a residual; one that is not finite is not a state's.
    fn see(&mut self, residual: f64) {
        if residual.is_finite() && self.0.is_none_or(|r| residual.abs() < r.abs()) {
            self.0 = Some(residual);
        }
    }

    /// x of `quantity`, beyond every state the search saw at `given`, as `Unreachable`; `InvalidInput` where it saw
    /// none.
    fn refusal(self, (quantity, x): (&'static Quantity, f64), given: (&'static Quantity, f64)) -> Error {
        let Some(r) = self.0 else { return Error::InvalidInput { quantity, value: x } };
        let limit = if r < 0.0 { Limit::Max } else { Limit::Min };
        Error::Unreachable { quantity, value: x, limit, bound: x + r, given }
    }
}

/// The root in [`illinois`] of a search's `f` between two ends whose values differ in sign, to `WIDTH` of the larger
/// end, or the end itself where its value is 0.
fn falsi<E: Clone>(a: End<E>, b: End<E>, f: &mut impl FnMut(f64) -> Result<(f64, E), Error>) -> Result<End<E>, Error> {
    if a.value == 0.0 || b.value == 0.0 {
        return Ok(if a.value == 0.0 { a } else { b });
    }
    let width = WIDTH * a.x.abs().max(b.x.abs());
    // Equivalent (M13.11): two ends at one x leave a bracket of no width, which the regula falsi returns at once as the
    // end of smaller |value| in either order; only a tie in |value| as well would tell them apart.
    // equivalent mutant: replace < with <=
    if a.x < b.x { illinois(a, b, f, (width, STEPS)) } else { illinois(b, a, f, (width, STEPS)) }
}

/// The share 2^−k (k ≥ 0) of a Newton step `step` in β that keeps β strictly inside (0, 1), the largest; `None` where
/// none above `DOME_DIFF` does.
fn inside_share(beta: f64, step: f64) -> Option<f64> {
    let mut share = 1.0;
    while share >= DOME_DIFF {
        if beta + share * step > 0.0 && beta + share * step < 1.0 {
            return Some(share);
        }
        share /= 2.0;
    }
    None
}

/// A descent's bracket: the last point at or above the input, and the first below it.
type Bracket<E> = (Option<End<E>>, Option<End<E>>);

/// The bracket of a descent in ln p by decades from `top` through `DECADES` of them, for an x that rises with p (`at`:
/// x less the input at ln p). A point whose solve fails before any lies above the input is stepped over; one after
/// that is the error.
fn by_decades<E>(top: f64, at: &mut impl FnMut(f64) -> Result<(f64, E), Error>) -> Result<Bracket<E>, Error> {
    let mut above = None;
    for decade in 0..=DECADES as usize {
        let u = top - decade as f64 * core::f64::consts::LN_10;
        match at(u) {
            Ok(here) if here.0 < 0.0 => return Ok((above, Some(End::new(u, here)))),
            Ok(here) => above = Some(End::new(u, here)),
            Err(e) if above.is_some() => return Err(e),
            Err(_) => {}
        }
    }
    Ok((above, None))
}

/// Whether `x` lies strictly below its critical value `critical` (none where the model has no critical point): where
/// it does, the bubble and dew points there bound the dome and decide a phase without the tangent-plane test.
fn below(x: f64, critical: Option<f64>) -> bool {
    critical.is_some_and(|c| x < c)
}

/// Each interior node of `line` where its value turns between its neighbours (a maximum, `rising` to it, or a minimum)
/// replaced in order by the turn that `turn` finds between them, so that each cell is monotonic.
fn cut_turns<E>(
    line: &mut [End<E>],
    mut turn: impl FnMut(&End<E>, &End<E>, bool) -> Result<End<E>, Error>,
) -> Result<(), Error> {
    for k in 1..line.len().saturating_sub(1) {
        let (prev, next) = (k.saturating_sub(1), k.saturating_add(1));
        let (Some(b), Some(h), Some(a)) = (line.get(prev), line.get(k), line.get(next)) else { continue };
        let (before, here, after) = (b.value, h.value, a.value);
        if (here - before) * (after - here) < 0.0 {
            // Equivalent (M13.11): a turn has (here − before)(after − here) < 0, so here ≠ before, where `>=` and `>`
            // agree on which way the line rises.
            // equivalent mutant: replace > with >=
            let turned = turn(b, a, here > before)?;
            if let Some(node) = line.get_mut(k) {
                *node = turned;
            }
        }
    }
    Ok(())
}

/// The forward difference's step in β from `beta`: `DOME_DIFF` up where that stays below 1, else down.
fn beta_step(beta: f64) -> f64 {
    if beta + DOME_DIFF < 1.0 { DOME_DIFF } else { -DOME_DIFF }
}

/// Whether a cell whose ends' values are `lo` and `hi` holds a root: they differ in sign, or one is 0.
fn holds_root(lo: f64, hi: f64) -> bool {
    lo.signum() != hi.signum() || lo == 0.0 || hi == 0.0
}

/// Whether a cell whose ends' slopes are `a` and `b` holds a turn: they differ strictly in sign.
fn turns_between(a: f64, b: f64) -> bool {
    a * b < 0.0
}

/// A dome's bracket steps by this factor in T from a start where x less the input is `value`: up where x lies below the
/// input (x rises with T), else down.
fn toward_root(value: f64) -> f64 {
    if value < 0.0 { STEP_T } else { 1.0 / STEP_T }
}

/// Whether value `a` is nearer a line's turn than `b`: higher toward a maximum (`rising`), lower toward a minimum.
fn ahead(rising: bool, a: f64, b: f64) -> bool {
    if rising { a > b } else { a < b }
}

impl Mixture {
    /// The pairs other than PT, DT and a quality at T or p (M13.11), from molar values in the native gauge.
    pub(super) fn outer(&self, input: &NativeInput, opts: &FlashOptions) -> Result<State, Error> {
        let (pair, (a, b)) = (input.pair(), input.values());
        let quality = |q: f64| (q, input.quality_basis());
        match pair {
            Pair::HP => self.isobar(b, Var::H, a, opts),
            Pair::PS => self.isobar(a, Var::S, b, opts),
            Pair::PU => self.isobar(a, Var::U, b, opts),
            Pair::DP => self.isochore(a, Var::P, b, opts),
            Pair::DH => self.isochore(a, Var::H, b, opts),
            Pair::DS => self.isochore(a, Var::S, b, opts),
            Pair::DU => self.isochore(a, Var::U, b, opts),
            Pair::HT => self.isotherm(b, Var::H, a, opts),
            Pair::ST => self.isotherm(b, Var::S, a, opts),
            Pair::TU => self.isotherm(a, Var::U, b, opts),
            Pair::HS => self.isentrope(b, Var::H, a, opts),
            Pair::SU => self.isentrope(a, Var::U, b, opts),
            Pair::QS => self.quality_line(quality(a), Var::S, b, opts),
            Pair::HQ => self.quality_line(quality(b), Var::H, a, opts),
            Pair::DQ => self.quality_line(quality(b), Var::D, a, opts),
            _ => Err(Error::Unsupported { pair }),
        }
    }

    /// The feed's homogeneous state at (T, ρ) for a search, outside the limits too; its label is not the answer's.
    fn homogeneous(&self, t: f64, rho: f64) -> Result<State, Error> {
        self.state(t, rho, Phase::Gas, &searching())
    }

    /// Whether the feed's homogeneous state at (T, ρ) is stable (DT's check).
    fn stable_at(&self, t: f64, rho: f64) -> Result<bool, Error> {
        let b = (self.residual(t, rho, Order::Two) + Derivs::IDEAL_DELTA)
            .bundle()
            .ok_or(Error::InvalidInput { quantity: Quantity::density(Basis::Molar), value: rho })?;
        self.is_stable(t, rho, pressure(self.fluid.gas_constant(), t, rho, &b), &b)
    }

    /// Whether a homogeneous root (T, ρ) of a search is the answer: it reproduces the input x of `var` and is stable,
    /// or a single phase is hinted.
    fn kept(&self, (t, rho): (f64, f64), (var, x): (Var, f64), hint: Option<Phase>) -> Result<bool, Error> {
        if !reproduces(var, self.fluid.gas_constant(), x, &self.homogeneous(t, rho)?) {
            return Ok(false);
        }
        Ok(hint.is_some() || self.stable_at(t, rho)?)
    }

    /// The temperatures a search in T spans: the domain's lowest, and 1.5·Tmax (core's, CoolProp's bound).
    fn t_search(&self) -> (f64, f64) {
        (self.limits.t_min(), 1.5 * self.limits.t_max())
    }

    /// The root in T of `f`, x(T) − x along a path on which x rises with T, from the top of `t_search` down in steps of
    /// `DESCENT` until x falls below the input, then the regula falsi in that step. An x above x at the top is
    /// `AboveMaxTemperature` there, one below x at the lowest T `BelowMinTemperature` there.
    fn root_in_t<E: Clone>(&self, mut f: impl FnMut(f64) -> Result<(f64, E), Error>) -> Result<End<E>, Error> {
        let (lo, hi) = self.t_search();
        let mut above: Option<End<E>> = None;
        let mut t = hi;
        let below = loop {
            let here = End::new(t, f(t)?);
            if here.value < 0.0 {
                break here;
            }
            if t == lo {
                return Err(DomainError::BelowMinTemperature { t: lo, t_min: lo }.into());
            }
            above = Some(here);
            t = (t * DESCENT).max(lo);
        };
        let Some(above) = above else { return Err(DomainError::AboveMaxTemperature { t: hi, t_max: hi }.into()) };
        falsi(below, above, &mut f)
    }

    /// The dome's root at `given` T or p, where `x` of `var` lies between the bubble point's and the dew point's: the
    /// regula falsi in β (`in_beta`). `None` where either point fails, or x lies outside them.
    fn dome_root(&self, given: Given, (var, x): (Var, f64)) -> Result<Option<Coexisting>, Error> {
        let (Ok(bubble), Ok(dew)) = (self.coexisting(0.0, given), self.coexisting(1.0, given)) else { return Ok(None) };
        let g = |c: &Coexisting| Ok(value(&self.coexisting_state(c, &searching())?, var) - x);
        saturation::in_beta(self, given, (bubble, dew), &g)
    }

    /// HP, PS and PU: at p, the input x of `var` (H, S or U).
    fn isobar(&self, p: f64, var: Var, x: f64, opts: &FlashOptions) -> Result<State, Error> {
        let found = self.isobar_state(p, (var, x), opts.phase_hint())?;
        self.answer(found, (var, x), opts, Strategy::Isobar)
    }

    /// The equilibrium at p whose x of `var` is the input. Below the critical pressure, where the bubble and dew points
    /// at p are both found, they decide: below the bubble point's x the liquid, on its branch, above the dew point's the
    /// vapour, between them the two phases in β. Otherwise, and under a hint, the root on the feed's stable root (or the
    /// hinted branch) where it is stable, else the two phases in β.
    fn isobar_state(&self, p: f64, (var, x): (Var, f64), hint: Option<Phase>) -> Result<Found, Error> {
        let given = Given::P(p);
        let at =
            |c: &Coexisting| -> Result<f64, Error> { Ok(value(&self.coexisting_state(c, &searching())?, var) - x) };
        if hint.is_none()
            && below(p, self.critical_point().map(|c| c.p))
            && let (Ok(bubble), Ok(dew)) = (self.coexisting(0.0, given), self.coexisting(1.0, given))
        {
            let (lo, hi) = self.t_search();
            let (at_bubble, at_dew) = (at(&bubble)?, at(&dew)?);
            if at_bubble >= 0.0 {
                return self.branch_root_in_t(p, Branch::Liquid, (lo, bubble.t), (var, x));
            }
            if at_dew <= 0.0 {
                return self.branch_root_in_t(p, Branch::Vapour, (dew.t, hi), (var, x));
            }
            let split = saturation::in_beta(self, given, (bubble, dew), &at)?;
            return split.map(Found::Two).ok_or(Error::NoConvergence { strategy: Strategy::Isobar, iterations: 0 });
        }
        if hint != Some(Phase::TwoPhase) {
            // The feed on its stable root at each T, or on the hinted phase's branch.
            let found = self.root_in_t(|t| {
                let rho = match hint {
                    Some(phase) => equilibrium::branch_root(self, t, p, branch_of(phase), None)?,
                    None => equilibrium::stable_root(self, t, p)?,
                };
                Ok((value(&self.homogeneous(t, rho)?, var) - x, rho))
            })?;
            if self.kept((found.x, found.at), (var, x), hint)? {
                return Ok(Found::Single(found.x, found.at));
            }
        }
        let split = self.dome_root(given, (var, x))?;
        split.map(Found::Two).ok_or(Error::NoConvergence { strategy: Strategy::Isobar, iterations: 0 })
    }

    /// The root in T between `lo` and `hi` of x of `var` at p on `branch`, each density seeded by the last: a liquid's
    /// below its bubble point, a vapour's above its dew point. An x that no T between them reaches is refused at the
    /// end it lies beyond (`BelowMinTemperature` at lo, `AboveMaxTemperature` at hi).
    fn branch_root_in_t(
        &self,
        p: f64,
        branch: Branch,
        (lo, hi): (f64, f64),
        (var, x): (Var, f64),
    ) -> Result<Found, Error> {
        let mut at = |t: f64| -> Result<(f64, f64), Error> {
            let rho = equilibrium::branch_root(self, t, p, branch, None)?;
            Ok((value(&self.homogeneous(t, rho)?, var) - x, rho))
        };
        let (a, b) = (End::new(lo, at(lo)?), End::new(hi, at(hi)?));
        if a.value > 0.0 {
            return Err(DomainError::BelowMinTemperature { t: lo, t_min: lo }.into());
        }
        if b.value < 0.0 {
            return Err(DomainError::AboveMaxTemperature { t: hi, t_max: hi }.into());
        }
        let found = falsi(a, b, &mut at)?;
        Ok(Found::Single(found.x, found.at))
    }

    /// The value of `var` at a found state.
    fn found_value(&self, found: &Found, var: Var) -> Result<f64, Error> {
        Ok(value(
            &match found {
                Found::Single(t, rho) => self.homogeneous(*t, *rho)?,
                Found::Two(c) => self.coexisting_state(c, &searching())?,
            },
            var,
        ))
    }

    /// The answer of a found state under the caller's options, where it reproduces the input x of `var`: one phase
    /// labelled as DT labels it, or as the hint; two as coexisting phases.
    fn answer(&self, found: Found, (var, x): (Var, f64), opts: &FlashOptions, path: Strategy) -> Result<State, Error> {
        let state = match found {
            Found::Single(t, rho) => {
                let phase = opts
                    .phase_hint()
                    .map_or_else(|| self.homogeneous(t, rho).map(|s| self.label(t, rho, s.p())), Ok)?;
                self.state(t, rho, phase, opts)?
            }
            Found::Two(c) => self.coexisting_state(&c, opts)?,
        };
        if reproduces(var, self.fluid.gas_constant(), x, &state) {
            Ok(state)
        } else {
            Err(Error::NoConvergence { strategy: path, iterations: 0 })
        }
    }

    /// DP, DH, DS and DU: at ρ, the input x of `var` (P, H, S or U).
    fn isochore(&self, rho: f64, var: Var, x: f64, opts: &FlashOptions) -> Result<State, Error> {
        let found = self.root_in_t(|t| Ok((value(&self.homogeneous(t, rho)?, var) - x, ())));
        if let Ok(found) = &found
            && opts.phase_hint() != Some(Phase::TwoPhase)
            && self.kept((found.x, rho), (var, x), opts.phase_hint())?
        {
            return self.answer(Found::Single(found.x, rho), (var, x), opts, Strategy::Isochore);
        }
        let at = |t: f64| self.isochore_at(t, rho, (var, x));
        let start = self.dome_start(found.as_ref().ok().map(|f| f.x));
        let c = self
            .dome_from(start, at, [(Var::D, rho), (var, x)], Strategy::Isochore)
            .map_err(|e| found.err().unwrap_or(e))?;
        self.answer(Found::Two(c), (var, x), opts, Strategy::Isochore)
    }

    /// x of `var` less the input x along the equilibrium isochore at T, and the split there: the isochoric split's where
    /// the feed splits at (T, ρ), else the homogeneous state's.
    fn isochore_at(&self, t: f64, rho: f64, (var, x): (Var, f64)) -> Result<(f64, Option<Coexisting>), Error> {
        match self.isochoric_split(t, rho)? {
            Some(c) => Ok((value(&self.coexisting_state(&c, &searching())?, var) - x, Some(c))),
            None => Ok((value(&self.homogeneous(t, rho)?, var) - x, None)),
        }
    }

    /// The feed at (T, ρ) as DT finds it: `None` where it is one phase, else the isochoric split. Below the critical
    /// temperature the bubble and dew points at T bound the dome, so they decide without the tangent-plane test (where
    /// both are found); above it, as DT does.
    fn isochoric_split(&self, t: f64, rho: f64) -> Result<Option<Coexisting>, Error> {
        let given = Given::T(t);
        if below(t, self.critical_point().map(|c| c.t))
            && let (Ok(bubble), Ok(dew)) = (self.coexisting(0.0, given), self.coexisting(1.0, given))
        {
            let g = |c: &Coexisting| Ok(math::ln(((1.0 - c.beta) / c.rho_liquid + c.beta / c.rho_vapour) * rho));
            return saturation::in_beta(self, given, (bubble, dew), &g);
        }
        if self.stable_at(t, rho)? {
            return Ok(None);
        }
        if let Some(c) = saturation::isochoric(self, t, rho)? {
            return Ok(Some(c));
        }
        match self.isochoric(t, rho, self.homogeneous(t, rho)?.p())? {
            (p, Equilibrium::Split { beta, x, y, rho_liquid, rho_vapour }) => {
                Ok(Some(Coexisting { t, p, beta, x, y, rho_liquid, rho_vapour }))
            }
            _ => Ok(None),
        }
    }

    /// Where a dome's bracket starts: at the homogeneous root where there is one, else just below the critical
    /// temperature (`DOME_START` of it), where the feed has a bubble and a dew point.
    fn dome_start(&self, root: Option<f64>) -> f64 {
        root.unwrap_or_else(|| self.critical_point().map_or(self.limits.t_max(), |c| DOME_START * c.t))
    }

    /// The two phases where x along a path that rises with T, `at` (x − the input at T, and the split there if the
    /// feed splits), is the input: from `t0` stepped by `STEP_T` toward the sign change at most `BRACKET_STEPS` times,
    /// then Newton's method in (ln T, β) on `specs` from the bracket's end nearer the root; where that fails, the regula
    /// falsi in T in the bracket.
    fn dome_from(
        &self,
        t0: f64,
        mut at: impl FnMut(f64) -> Result<(f64, Option<Coexisting>), Error>,
        specs: [(Var, f64); 2],
        path: Strategy,
    ) -> Result<Coexisting, Error> {
        let failed = |iterations| Error::NoConvergence { strategy: path, iterations };
        let mut last = End::new(t0, at(t0)?);
        let factor = toward_root(last.value);
        let mut bracketed = None;
        for _ in 0..BRACKET_STEPS {
            let t = last.x * factor;
            let next = End::new(t, at(t)?);
            if next.value.signum() != last.value.signum() {
                bracketed = Some(next);
                break;
            }
            last = next;
        }
        let Some(next) = bracketed else { return Err(failed(BRACKET_STEPS)) };
        let mut ends = [&last, &next];
        ends.sort_by(|a, b| a.value.abs().total_cmp(&b.value.abs()));
        for seed in ends.iter().filter_map(|end| end.at.clone()) {
            if let Ok(c) = self.dome_newton(true, specs, seed) {
                return Ok(c);
            }
        }
        falsi(last, next, &mut at)?.at.ok_or(failed(0))
    }

    /// HT, ST and TU: at T, the input x of `var` (H, S or U).
    fn isotherm(&self, t: f64, var: Var, x: f64, opts: &FlashOptions) -> Result<State, Error> {
        let hint = opts.phase_hint();
        let mut states = Vec::new();
        // A homogeneous root that is not stable lies inside the dome, which then holds a root of its own.
        let mut inside = hint == Some(Phase::TwoPhase);
        let mut reached = Reached::default();
        if hint != Some(Phase::TwoPhase) {
            let roots;
            (roots, reached) = self.homogeneous_roots(t, var, x)?;
            for rho in roots {
                if self.kept((t, rho), (var, x), hint)? {
                    states.push(self.answer(Found::Single(t, rho), (var, x), opts, Strategy::Isotherm)?);
                } else {
                    inside = true;
                }
            }
        }
        if inside && let Some(c) = self.dome_root(Given::T(t), (var, x))? {
            states.push(self.answer(Found::Two(c), (var, x), opts, Strategy::Isotherm)?);
        }
        // No homogeneous root: x lies beyond every state of the feed at T, the dome's (the lever rule's) among them.
        let none = if inside {
            Error::InvalidInput { quantity: var.quantity(Basis::Molar), value: x }
        } else {
            reached.refusal((var.quantity(Basis::Molar), x), (Quantity::TEMPERATURE, t))
        };
        choose(states, (|s| s.rho(Basis::Molar), Quantity::density(Basis::Molar)), opts.roots(), none)
    }

    /// The densities at T where the feed's homogeneous x of `var` is the input, ascending: its isotherm in ln ρ from
    /// `DECADES` below the model's largest density to it, on a grid of `NODES_PER_DECADE` (at least `CELLS` cells), each
    /// cell cut where (∂x/∂ρ)_T changes sign, each monotonic piece's root by the regula falsi; with them, the residual
    /// nearest zero of the nodes and turns.
    fn homogeneous_roots(&self, t: f64, var: Var, x: f64) -> Result<(Vec<f64>, Reached), Error> {
        let top = math::ln(self.rho_max(t));
        let bottom = top - DECADES * core::f64::consts::LN_10;
        let cells = (DECADES * NODES_PER_DECADE).max(CELLS);
        let at = |u: f64| -> Result<(f64, f64), Error> {
            let state = self.homogeneous(t, math::exp(u))?;
            Ok((value(&state, var) - x, slope(&state, var)?))
        };
        let node = |k: f64| if k >= cells { top } else { bottom + k * (top - bottom) / cells };
        let (mut roots, mut reached) = (Vec::new(), Reached::default());
        let (mut a, mut at_a) = (bottom, at(bottom)?);
        reached.see(at_a.0);
        for k in 1..=cells as usize {
            let b = node(k as f64);
            let at_b = at(b)?;
            reached.see(at_b.0);
            let (lo, hi) = (End::new(a, (at_a.0, ())), End::new(b, (at_b.0, ())));
            let pieces = if turns_between(at_a.1, at_b.1) {
                let mut turn = |u: f64| at(u).map(|v| (v.1, ()));
                let u = falsi(End::new(a, (at_a.1, ())), End::new(b, (at_b.1, ())), &mut turn)?.x;
                let at_turn = End::new(u, (at(u)?.0, ()));
                reached.see(at_turn.value);
                vec![(lo, at_turn.clone()), (at_turn, hi)]
            } else {
                vec![(lo, hi)]
            };
            for (lo, hi) in pieces {
                if holds_root(lo.value, hi.value) {
                    let mut residual = |u: f64| at(u).map(|v| (v.0, ()));
                    roots.push(math::exp(falsi(lo, hi, &mut residual)?.x));
                }
            }
            (a, at_a) = (b, at_b);
        }
        Ok((roots, reached))
    }

    /// HS and SU: s, and the input x of `var` (H or U). Along the isentrope h and u rise with p ((∂h/∂p)_s = v), so p is
    /// the root in ln p of x at PS(p, s), down from the model's highest pressure by decades until x falls below the
    /// input (a pressure whose PS fails above the root is stepped over), then the regula falsi in that decade.
    fn isentrope(&self, s: f64, var: Var, x: f64, opts: &FlashOptions) -> Result<State, Error> {
        let hint = opts.phase_hint();
        let mut at = |u: f64| -> Result<(f64, Found), Error> {
            let found = self.isobar_state(math::exp(u), (Var::S, s), hint)?;
            Ok((self.found_value(&found, var)? - x, found))
        };
        let (above, below) = by_decades(math::ln(self.limits.p_max()), &mut at)?;
        let p_max = self.limits.p_max();
        let Some(above) = above else { return Err(DomainError::AboveMaxPressure { p: p_max, p_max }.into()) };
        let Some(below) = below else {
            let mut reached = Reached::default();
            reached.see(above.value);
            let given = (Quantity::entropy(Basis::Molar), s);
            return Err(reached.refusal((var.quantity(Basis::Molar), x), given));
        };
        let found = falsi(below, above, &mut at)?;
        self.answer(found.at, (var, x), opts, Strategy::Isentrope)
    }

    /// Newton's method in (ln X, β), X the T (`by_t`) or p of `seed`, on coexisting phases, to the two inputs `specs`
    /// within `DOME_GATE` of their scales: each point seeded by the last, the Jacobian by forward differences, a step
    /// that would leave β outside (0, 1) halved.
    fn dome_newton(&self, by_t: bool, specs: [(Var, f64); 2], seed: Coexisting) -> Result<Coexisting, Error> {
        let r = self.fluid.gas_constant();
        let failed = |iterations| Error::NoConvergence { strategy: Strategy::Vle, iterations };
        let at = |u: f64, beta: f64, from: &Coexisting| -> Result<Coexisting, Error> {
            let v = math::exp(u);
            let (given, seed) =
                if by_t { (Given::T(v), (v, from.p, ln_k(from))) } else { (Given::P(v), (from.t, v, ln_k(from))) };
            saturation::coexisting_from(self, beta, given, seed)
        };
        let residuals = |c: &Coexisting| -> Result<Vec<f64>, Error> {
            let state = self.coexisting_state(c, &searching())?;
            Ok(specs.iter().map(|(var, x)| scaled(*var, r, c.t, *x, value(&state, *var))).collect())
        };
        let mut c = seed;
        for iteration in 0..DOME_STEPS as u16 {
            let f = residuals(&c)?;
            if f.iter().all(|f| f.abs() <= DOME_GATE) {
                return Ok(c);
            }
            let u = math::ln(if by_t { c.t } else { c.p });
            let db = beta_step(c.beta);
            let (fu, fb) = (residuals(&at(u + DOME_DIFF, c.beta, &c)?)?, residuals(&at(u, c.beta + db, &c)?)?);
            let rows = f.iter().zip(&fu).zip(&fb).take(2);
            let jacobian = rows.map(|((f, fu), fb)| vec![(fu - f) / DOME_DIFF, (fb - f) / db]).collect();
            let Some(step) = equilibrium::solve(jacobian, f.iter().map(|f| -f).collect()) else {
                return Err(failed(iteration));
            };
            let (du, dbeta) = (slot::at(&step, 0), slot::at(&step, 1));
            let Some(share) = inside_share(c.beta, dbeta) else { return Err(failed(iteration)) };
            c = at(u + (share * du).clamp(-DOME_STEP, DOME_STEP), c.beta + share * dbeta, &c)?;
        }
        Err(failed(DOME_STEPS as u16))
    }

    /// QS, HQ and DQ: the quality (molar, or by mass) and the input x of `var` (S, H or D).
    fn quality_line(&self, (q, basis): (f64, Basis), var: Var, x: f64, opts: &FlashOptions) -> Result<State, Error> {
        if !(0.0..=1.0).contains(&q) {
            return Err(Error::InvalidInput { quantity: VAPOUR_FRACTION, value: q });
        }
        // A mass quality of 0 or 1 is the molar one: `at_mass_quality` returns its first point.
        let by_mass = basis == Basis::Mass;
        // The two phases at Q and T, seeded by a point solved nearby where one is.
        let point = |t: f64, near: Option<&Coexisting>| -> Result<Coexisting, Error> {
            let given = Given::T(t);
            match near {
                _ if by_mass => self.at_mass_quality(q, given, near),
                Some(c) => saturation::coexisting_from(self, q, given, (t, c.p, ln_k(c))),
                None => self.coexisting(q, given),
            }
        };
        let at =
            |c: &Coexisting| -> Result<f64, Error> { Ok(value(&self.coexisting_state(c, &searching())?, var) - x) };
        // Inside the line, a point the last one does not seed is solved as `coexisting` solves it.
        let refined = |t: f64, near: &Coexisting| point(t, Some(near)).or_else(|_| point(t, None));
        let mut line: Vec<End<Coexisting>> = Vec::new();
        for c in self.line_nodes(&point)? {
            line.push(End::new(c.t, (at(&c)?, c)));
        }
        cut_turns(&mut line, |a, b, rising| self.turn((a, b), rising, &refined, &at))?;
        let mut reached = Reached::default();
        line.iter().for_each(|end| reached.see(end.value));
        let mut roots: Vec<Coexisting> = Vec::new();
        for pair in line.windows(2) {
            let [lo, hi] = pair else { continue };
            let (lo, hi) = (lo.clone(), hi.clone());
            if holds_root(lo.value, hi.value) {
                let mut near = lo.at.clone();
                let mut f = |t: f64| -> Result<(f64, Coexisting), Error> {
                    let c = refined(t, &near)?;
                    near = c.clone();
                    Ok((at(&c)?, c))
                };
                roots.push(falsi(lo, hi, &mut f)?.at);
            }
        }
        let states: Vec<State> = roots
            .into_iter()
            .map(|c| self.answer(Found::Two(c), (var, x), opts, Strategy::Saturation))
            .collect::<Result<_, _>>()?;
        let none = reached.refusal((var.quantity(Basis::Molar), x), (Quantity::QUALITY, q));
        choose(states, (State::t, Quantity::TEMPERATURE), opts.roots(), none)
    }

    /// The turn of a line of quality between two nodes, its maximum (`rising` to it) or minimum, by golden-section search
    /// in T over `GOLDEN_STEPS`, each point seeded by the last.
    fn turn(
        &self,
        (a, b): (&End<Coexisting>, &End<Coexisting>),
        rising: bool,
        point: &impl Fn(f64, &Coexisting) -> Result<Coexisting, Error>,
        at: &impl Fn(&Coexisting) -> Result<f64, Error>,
    ) -> Result<End<Coexisting>, Error> {
        // The golden ratio less 1, (√5 − 1)/2.
        let ratio = 0.618_033_988_749_894_9;
        let mut near = a.at.clone();
        let mut probe = |t: f64| -> Result<End<Coexisting>, Error> {
            let c = point(t, &near)?;
            near = c.clone();
            Ok(End::new(t, (at(&c)?, c)))
        };
        let (mut lo, mut hi) = (a.x, b.x);
        let mut left = probe(hi - ratio * (hi - lo))?;
        let mut right = probe(lo + ratio * (hi - lo))?;
        for _ in 0..GOLDEN_STEPS {
            if ahead(rising, left.value, right.value) {
                (hi, right) = (right.x, left);
                left = probe(hi - ratio * (hi - lo))?;
            } else {
                (lo, left) = (left.x, right);
                right = probe(lo + ratio * (hi - lo))?;
            }
        }
        Ok(if ahead(rising, left.value, right.value) { left } else { right })
    }

    /// The nodes of a line of quality: from the domain's lowest temperature up in `LINE_CELLS` cells to 1.2 T_c, each
    /// point seeded by the last, until one fails; then `LINE_HALVINGS` halvings of that cell toward its top.
    fn line_nodes(
        &self,
        point: &impl Fn(f64, Option<&Coexisting>) -> Result<Coexisting, Error>,
    ) -> Result<Vec<Coexisting>, Error> {
        let lo = self.t_search().0;
        let top = self.critical_point().map_or(self.limits.t_max(), |c| 1.2 * c.t).min(self.limits.t_max());
        let step = (top - lo) / LINE_CELLS as f64;
        let mut nodes: Vec<Coexisting> = vec![point(lo, None)?];
        let mut failed = None;
        for k in 1..=LINE_CELLS {
            let t = if k == LINE_CELLS { top } else { lo + k as f64 * step };
            match point(t, nodes.last()) {
                Ok(c) => nodes.push(c),
                Err(_) => {
                    failed = Some(t);
                    break;
                }
            }
        }
        if let Some(mut out) = failed {
            for _ in 0..LINE_HALVINGS {
                let last = nodes.last().map_or(lo, |c| c.t);
                let t = 0.5 * (last + out);
                match point(t, nodes.last()) {
                    Ok(c) => nodes.push(c),
                    Err(_) => out = t,
                }
            }
        }
        Ok(nodes)
    }
}

#[cfg(test)]
mod tests {
    use phasekit_core::{DataSet, Registry, RootPolicy};

    use super::*;
    use crate::MultiFluid;

    /// The library's mixture of `components` at `z`, on `Parity`.
    fn library(components: &[&str], z: &[f64]) -> Mixture {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap_or_else(|e| panic!("{e:?}"));
        let model = MultiFluid::new(&registry, components, DataSet::Parity).unwrap_or_else(|e| panic!("{e:?}"));
        Mixture::new(model, z).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn methane_ethane() -> Mixture {
        library(&["Methane", "Ethane"], &[0.5, 0.5])
    }

    fn ok<T>(r: Result<T, Error>) -> T {
        r.unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// A placeholder split at T for the searches' synthetic lines.
    fn node(t: f64) -> Coexisting {
        Coexisting { t, p: 1e6, beta: 0.5, x: vec![0.5, 0.5], y: vec![0.5, 0.5], rho_liquid: 1.0, rho_vapour: 1.0 }
    }

    /// Each variable's value at a state is its own molar value; Q is a split's quality and a single phase's NaN.
    #[test]
    fn values_are_the_state_s() {
        let m = methane_ethane();
        let s = ok(m.homogeneous(300.0, 1000.0));
        let want = [(Var::T, s.t()), (Var::P, s.p()), (Var::D, s.rho(Basis::Molar)), (Var::H, s.h(Basis::Molar))];
        let more = [(Var::S, s.s(Basis::Molar)), (Var::U, s.u(Basis::Molar))];
        assert!(want.iter().chain(&more).all(|(var, v)| value(&s, *var) == *v) && value(&s, Var::Q).is_nan());
        let split = ok(m.coexisting_state(&ok(m.coexisting(0.4, Given::T(200.0))), &searching()));
        assert_eq!(value(&split, Var::Q), 0.4);
    }

    /// The acceptance gate's scales (core's D6): |x| but at least RT for h and u, at least R for s, |x| for p and ρ;
    /// a state reproduces x within 1e-9 of that scale and not 1.1e-9, either side.
    #[test]
    fn inputs_are_reproduced_within_core_s_gate() {
        assert_eq!((scale(Var::H, 8.0, 300.0, 100.0), scale(Var::U, 8.0, 300.0, -5000.0)), (2400.0, 5000.0));
        assert_eq!((scale(Var::S, 8.0, 300.0, 5.0), scale(Var::S, 8.0, 300.0, -20.0)), (8.0, 20.0));
        assert_eq!((scale(Var::P, 8.0, 300.0, -1e5), scale(Var::D, 8.0, 300.0, 3.0)), (1e5, 3.0));
        let m = methane_ethane();
        let (s, r) = (ok(m.homogeneous(300.0, 1000.0)), m.fluid.gas_constant());
        let (h, gate) = (s.h(Basis::Molar), REPRODUCED * scale(Var::H, r, 300.0, s.h(Basis::Molar)));
        assert!(reproduces(Var::H, r, h + 0.9 * gate, &s) && reproduces(Var::H, r, h - 0.9 * gate, &s));
        assert!(!reproduces(Var::H, r, h + 1.1 * gate, &s) && !reproduces(Var::H, r, h - 1.1 * gate, &s));
        // A residual in that scale: 24 J/mol over RT = 2400 J/mol, 10 kPa over |−100 kPa|.
        assert_eq!((scaled(Var::H, 8.0, 300.0, 100.0, 124.0), scaled(Var::P, 8.0, 300.0, -1e5, -9e4)), (0.01, 0.1));
    }

    /// HS's descent by decades in ln p, on a synthetic x: the last point above the input and the first below it, a
    /// point at the input itself above it; a solve that fails before any point lies above is stepped over, one after
    /// that is the error; and one that never falls below the input ends without a point below.
    #[test]
    fn descents_by_decades_bracket_the_input() {
        let (top, ln10) = (10.0, core::f64::consts::LN_10);
        let failed = || Error::NoConvergence { strategy: Strategy::Isentrope, iterations: 0 };
        let xs = |bracket: Bracket<()>| (bracket.0.map(|e| e.x), bracket.1.map(|e| e.x));
        let decade = |k: f64| top - k * ln10;
        let root = decade(2.5);
        assert_eq!(xs(ok(by_decades(top, &mut |u| Ok((u - root, ()))))), (Some(decade(2.0)), Some(decade(3.0))));
        let mut skips = |u: f64| if u > decade(0.5) { Err(failed()) } else { Ok((u - root, ())) };
        assert_eq!(xs(ok(by_decades(top, &mut skips))), (Some(decade(2.0)), Some(decade(3.0))));
        let mut stops = |u: f64| if u < top { Err(failed()) } else { Ok((1.0, ())) };
        assert_eq!(by_decades(top, &mut stops).err(), Some(failed()));
        assert_eq!(xs(ok(by_decades(top, &mut |u| Ok((u - top, ()))))), (Some(top), Some(decade(1.0))));
        assert_eq!(xs(ok(by_decades(top, &mut |_| Ok((1.0, ()))))), (Some(decade(DECADES)), None));
    }

    /// An isotherm is cut where (∂x/∂ρ)_T of h, s or u changes sign; a hint's branch is the liquid's for a liquid.
    #[test]
    fn slopes_and_branches() {
        let s = ok(methane_ethane().homogeneous(300.0, 1000.0));
        for (var, of) in [(Var::H, DerivVar::Hmolar), (Var::S, DerivVar::Smolar), (Var::U, DerivVar::Umolar)] {
            assert_eq!(slope(&s, var), s.partial(Partial { of, wrt: DerivVar::Dmolar, at: DerivVar::T }));
        }
        assert!(branch_of(Phase::Liquid) == Branch::Liquid && branch_of(Phase::SupercriticalLiquid) == Branch::Liquid);
        assert!(branch_of(Phase::Gas) == Branch::Vapour && branch_of(Phase::Supercritical) == Branch::Vapour);
    }

    /// One root is the answer; several are `Ambiguous` with their keys ascending under `Strict` and `Stable`, or under
    /// `Nearest` the nearest; two within `WIDTH` are one; none is the refusal given.
    #[test]
    fn roots_are_chosen_as_core_chooses_them() {
        let m = methane_ethane();
        let at = |rho: f64| ok(m.homogeneous(300.0, rho));
        let none = || Error::InvalidInput { quantity: Quantity::QUALITY, value: 1.0 };
        let key = |s: &State| s.rho(Basis::Molar);
        let pick = |rhos: &[f64], policy| {
            choose(rhos.iter().map(|r| at(*r)).collect(), (key, Quantity::density(Basis::Molar)), policy, none())
        };
        assert_eq!(pick(&[], RootPolicy::Strict).err(), Some(none()));
        assert_eq!(pick(&[500.0], RootPolicy::Strict).map(|s| key(&s)), Ok(500.0));
        let ambiguous = Some(Error::Ambiguous { roots: Roots::new(Quantity::density(Basis::Molar), &[500.0, 2000.0]) });
        assert_eq!(pick(&[2000.0, 500.0], RootPolicy::Strict).err(), ambiguous);
        assert_eq!(pick(&[2000.0, 500.0], RootPolicy::Stable).err(), ambiguous);
        let nearest = pick(&[2000.0, 500.0, 1000.0], RootPolicy::Nearest(1400.0)).map(|s| key(&s));
        assert_eq!(nearest, Ok(1000.0));
        assert_eq!(pick(&[500.0, 500.0 * (1.0 + 1e-15)], RootPolicy::Strict).map(|s| key(&s)), Ok(500.0));
    }

    /// The regula falsi returns an end whose value is 0 without evaluating, takes its ends in either order, and closes
    /// to `WIDTH` of the larger end.
    #[test]
    fn falsi_takes_a_zero_end_and_either_order() {
        let mut calls = 0;
        let mut f = |x: f64| {
            calls += 1;
            Ok((x - 2.0, ()))
        };
        let zero = [(2.0, 0.0), (5.0, 3.0)].map(|(x, v)| End::new(x, (v, ())));
        assert_eq!(ok(falsi(zero[0].clone(), zero[1].clone(), &mut f)).x, 2.0);
        assert_eq!(ok(falsi(zero[1].clone(), zero[0].clone(), &mut f)).x, 2.0);
        let found = ok(falsi(End::new(5.0, (3.0, ())), End::new(1.0, (-1.0, ())), &mut f));
        assert!((found.x - 2.0).abs() <= 5.0 * WIDTH && calls >= 1, "{} {calls}", found.x);
        // A zero end is taken without an evaluation.
        let mut evaluations = 0;
        let mut counted = |x: f64| {
            evaluations += 1;
            Ok((x - 2.0, ()))
        };
        let taken = ok(falsi(zero[0].clone(), zero[1].clone(), &mut counted));
        assert_eq!((taken.x, evaluations), (2.0, 0));
    }

    /// The bracket's width is relative to its larger end: on a function that jumps from −1 to 1 at 300, never 0 (a smooth
    /// one lands on its root exactly and stops there whatever the width), the regula falsi between 100 and 1000 closes to
    /// 1e-14 of 1000 in about 43 evaluations; a width below the doubles' spacing there (1e-14 of 1/1000) spends all
    /// `STEPS`.
    #[test]
    fn falsi_s_width_scales_with_its_ends() {
        let g = |x: f64| if x < 300.0 { x - 301.0 } else { x - 299.0 };
        let mut calls = 0;
        let mut f = |x: f64| {
            calls += 1;
            Ok((g(x), ()))
        };
        let [a, b] = [100.0, 1000.0].map(|x| End::new(x, (g(x), ())));
        let found = ok(falsi(a, b, &mut f));
        assert!((found.x - 300.0).abs() <= 1e-11 && calls < STEPS / 2, "{} {calls}", found.x);
    }

    /// A search in T descends from 1.5 T_max by 20 % until x falls below the input, then closes on the root; an x above
    /// x at the top is `AboveMaxTemperature` there, one above x down to the lowest T `BelowMinTemperature` there.
    #[test]
    fn searches_in_t_descend_from_the_top() {
        let m = methane_ethane();
        let (lo, hi) = m.t_search();
        assert_eq!((lo, hi), (m.limits.t_min(), 1.5 * m.limits.t_max()));
        let mut visited = Vec::new();
        let found = ok(m.root_in_t(|t| {
            visited.push(t);
            // A search that never ends fails here instead of filling memory (a mutant of the descent's step once
            // grew this vector by gigabytes in the 300 s before cargo-mutants' timeout).
            if visited.len() > 4 * STEPS {
                return Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: STEPS as u16 });
            }
            Ok((t - 0.3 * hi, ()))
        }));
        let mut descent = vec![hi];
        while descent.len() < 7 {
            descent.push(descent[descent.len() - 1] * DESCENT);
        }
        assert!((found.x / (0.3 * hi) - 1.0).abs() <= 1e-14 && visited[..7] == descent[..], "{visited:?}");
        let above = DomainError::AboveMaxTemperature { t: hi, t_max: hi };
        assert_eq!(m.root_in_t(|t| Ok((t - 2.0 * hi, ()))).err(), Some(above.into()));
        let below = DomainError::BelowMinTemperature { t: lo, t_min: lo };
        assert_eq!(m.root_in_t(|t| Ok((t + 1.0, ()))).err(), Some(below.into()));
        let near_lo = ok(m.root_in_t(|t| Ok((t - 1.01 * lo, ()))));
        assert!((near_lo.x / (1.01 * lo) - 1.0).abs() <= 1e-14, "{}", near_lo.x);
        // An x that is the input at the top itself is the root there, not above it.
        assert_eq!(ok(m.root_in_t(|t| Ok((t - hi, ())))).x, hi);
    }

    /// On a branch at p, an x below the liquid's at the lowest T is `BelowMinTemperature` there, one above the vapour's
    /// at the top `AboveMaxTemperature` there; between, the root: library Methane/Ethane's vapour at 1 MPa and 300 K.
    #[test]
    fn branch_searches_refuse_beyond_their_ends() {
        let m = methane_ethane();
        let (p, (lo, hi)) = (1e6, m.t_search());
        let h_at =
            |t: f64, branch| value(&ok(m.homogeneous(t, ok(equilibrium::branch_root(&m, t, p, branch, None)))), Var::H);
        let below = DomainError::BelowMinTemperature { t: lo, t_min: lo };
        let liquid = m.branch_root_in_t(p, Branch::Liquid, (lo, 150.0), (Var::H, h_at(lo, Branch::Liquid) - 1.0));
        assert_eq!(liquid.err(), Some(below.into()));
        let above = DomainError::AboveMaxTemperature { t: hi, t_max: hi };
        let vapour = m.branch_root_in_t(p, Branch::Vapour, (250.0, hi), (Var::H, h_at(hi, Branch::Vapour) + 1.0));
        assert_eq!(vapour.err(), Some(above.into()));
        let Ok(Found::Single(t, _)) =
            m.branch_root_in_t(p, Branch::Vapour, (250.0, hi), (Var::H, h_at(300.0, Branch::Vapour)))
        else {
            panic!("no root")
        };
        assert!((t / 300.0 - 1.0).abs() <= 1e-12, "{t}");
        // An x that is the value at an end itself is the root there, not beyond it.
        let at_end = |branch, ends, t| m.branch_root_in_t(p, branch, ends, (Var::H, h_at(t, branch)));
        assert!(matches!(at_end(Branch::Liquid, (lo, 150.0), lo), Ok(Found::Single(t, _)) if t == lo));
        assert!(matches!(at_end(Branch::Vapour, (250.0, hi), hi), Ok(Found::Single(t, _)) if t == hi));
    }

    /// Along the isochore, x less the input is the homogeneous state's where the feed is one phase at (T, ρ), and the
    /// split's where it splits, with that split (library Methane/Ethane at 200 K: a gas at half the dew point's density,
    /// and the density halfway in ln ρ between the bubble and dew points).
    #[test]
    fn the_isochore_s_value_is_that_of_the_phases_present() {
        let m = methane_ethane();
        let (bubble, dew) = (ok(m.coexisting(0.0, Given::T(200.0))), ok(m.coexisting(1.0, Given::T(200.0))));
        let gas = 0.5 * dew.rho_vapour;
        let h = value(&ok(m.homogeneous(200.0, gas)), Var::H);
        let (at, split) = ok(m.isochore_at(200.0, gas, (Var::H, h)));
        assert!(at == 0.0 && split.is_none(), "{at} {split:?}");
        let rho = math::exp(0.5 * (math::ln(bubble.rho_liquid) + math::ln(dew.rho_vapour)));
        let c = ok(m.isochoric_split(200.0, rho)).unwrap_or_else(|| panic!("no split"));
        let h = value(&ok(m.coexisting_state(&c, &searching())), Var::H);
        let (at, split) = ok(m.isochore_at(200.0, rho, (Var::H, h)));
        assert!(at == 0.0 && split.is_some_and(|s| s.beta == c.beta), "{at}");
    }

    /// A Newton step in β keeps β strictly inside (0, 1) by halving: the whole step where it stays inside, an eighth of
    /// one that would reach 1.3 or −0.4 from 0.9 or 0.1, none of a step that is not a number.
    #[test]
    fn newton_steps_in_beta_stay_inside() {
        assert_eq!(inside_share(0.5, 0.2), Some(1.0));
        assert_eq!((inside_share(0.9, 0.4), inside_share(0.1, -0.5)), (Some(0.125), Some(0.125)));
        assert_eq!((inside_share(0.5, f64::NAN), inside_share(0.5, 1e9)), (None, None));
        // A share that lands on 0 or 1 exactly is not inside: the next, half as large, is.
        assert_eq!((inside_share(0.5, -1.0), inside_share(0.5, 1.0)), (Some(0.25), Some(0.25)));
    }

    /// The bubble and dew points decide only strictly below the critical value: not at it, not above it, and not
    /// without a critical point.
    #[test]
    fn saturation_decides_strictly_below_the_critical_point() {
        assert!(below(1.0, Some(2.0)));
        assert!(!below(2.0, Some(2.0)) && !below(3.0, Some(2.0)) && !below(1.0, None));
    }

    /// A line's turns are cut at the interior nodes where its value turns, in order, each from its two neighbours: on
    /// 0, 1, 3, 2, 1, 2, 2, 1, 1 a maximum at the third node and a minimum at the fifth; a monotonic or flat stretch (a
    /// fall onto a flat one too), and the ends, are left alone.
    #[test]
    fn a_line_s_turns_are_cut_at_its_nodes() {
        let mut line: Vec<End<f64>> = [0.0, 1.0, 3.0, 2.0, 1.0, 2.0, 2.0, 1.0, 1.0]
            .iter()
            .enumerate()
            .map(|(k, v)| End::new(k as f64, (*v, 0.0)))
            .collect();
        let mut cuts = Vec::new();
        ok(cut_turns(&mut line, |a, b, rising| {
            cuts.push((a.x, b.x, rising));
            Ok(End::new(0.5 * (a.x + b.x), (if rising { 4.0 } else { 0.5 }, 1.0)))
        }));
        assert_eq!(cuts, vec![(1.0, 3.0, true), (3.0, 5.0, false)]);
        let values: Vec<f64> = line.iter().map(|e| e.value).collect();
        assert_eq!(values, vec![0.0, 1.0, 4.0, 2.0, 0.5, 2.0, 2.0, 1.0, 1.0]);
    }

    /// A cell holds a root where its ends differ in sign or one is 0 (on either side, of either sign), and a turn
    /// where their slopes differ strictly in sign (a slope of 0 is no turn, of whichever sign its neighbour).
    #[test]
    fn cells_hold_roots_and_turns_by_their_ends() {
        assert!(holds_root(-1.0, 1.0) && holds_root(1.0, -1.0) && !holds_root(1.0, 2.0) && !holds_root(-1.0, -2.0));
        assert!(holds_root(0.0, 1.0) && holds_root(1.0, 0.0) && holds_root(0.0, -1.0) && holds_root(-1.0, 0.0));
        assert!(turns_between(-1.0, 2.0) && turns_between(2.0, -1.0) && !turns_between(1.0, 2.0));
        assert!(!turns_between(0.0, 1.0) && !turns_between(-1.0, 0.0) && !turns_between(1.0, 0.0));
    }

    /// A dome's bracket steps up in T where x lies below the input and down where it lies above it or on it; a turn's
    /// search keeps the higher value toward a maximum and the lower toward a minimum, neither of two equal ones.
    #[test]
    fn brackets_and_turns_step_toward_their_targets() {
        assert_eq!((toward_root(-1.0), toward_root(1.0), toward_root(0.0)), (STEP_T, 1.0 / STEP_T, 1.0 / STEP_T));
        assert!(ahead(true, 2.0, 1.0) && !ahead(true, 1.0, 2.0) && !ahead(true, 1.0, 1.0));
        assert!(ahead(false, 1.0, 2.0) && !ahead(false, 2.0, 1.0) && !ahead(false, 1.0, 1.0));
    }

    /// The forward difference in β steps up from 0 and 0.5, and down where a step up would reach 1 (at 1 − `DOME_DIFF`,
    /// where it reaches 1 exactly) or pass it.
    #[test]
    fn beta_differences_stay_below_one() {
        assert_eq!((beta_step(0.0), beta_step(0.5)), (DOME_DIFF, DOME_DIFF));
        assert_eq!((beta_step(1.0 - DOME_DIFF), beta_step(1.0 - 0.5 * DOME_DIFF)), (-DOME_DIFF, -DOME_DIFF));
    }

    /// Newton's method on coexisting phases returns to a split from a seed 2 % off in T and 0.05 in β: in (ln T, β) on
    /// its ρ and h, and in (ln p, β) on its s and h (library Methane/Ethane at 200 K and β = 0.4).
    #[test]
    fn dome_newton_returns_to_the_split() {
        let m = methane_ethane();
        let c = ok(m.coexisting(0.4, Given::T(200.0)));
        let s = ok(m.coexisting_state(&c, &searching()));
        let (rho, h, entropy) = (s.rho(Basis::Molar), s.h(Basis::Molar), s.s(Basis::Molar));
        let close = |back: &Coexisting| (back.t / c.t - 1.0).abs() <= 1e-9 && (back.beta - c.beta).abs() <= 1e-8;
        let by_t = ok(m.dome_newton(true, [(Var::D, rho), (Var::H, h)], ok(m.coexisting(0.45, Given::T(204.0)))));
        assert!(close(&by_t), "{by_t:?}");
        let seed = ok(m.coexisting(0.45, Given::P(1.02 * c.p)));
        let by_p = ok(m.dome_newton(false, [(Var::S, entropy), (Var::H, h)], seed));
        assert!(close(&by_p), "{by_p:?}");
    }

    /// The golden-section search finds a line's turn within 1e-6 K: the maximum of 1 − (T − 250 K)² and the minimum of
    /// (T − 230 K)², between 200 and 300 K.
    #[test]
    fn turns_are_found_by_golden_section() {
        let m = methane_ethane();
        let point = |t: f64, _: &Coexisting| Ok(node(t));
        let ends = |at: &dyn Fn(&Coexisting) -> Result<f64, Error>| {
            [200.0, 300.0].map(|t| End::new(t, (ok(at(&node(t))), node(t))))
        };
        let peak = |c: &Coexisting| Ok(1.0 - (c.t - 250.0) * (c.t - 250.0));
        let [a, b] = ends(&peak);
        let top = ok(m.turn((&a, &b), true, &point, &peak));
        let dip = |c: &Coexisting| Ok((c.t - 230.0) * (c.t - 230.0));
        let [a, b] = ends(&dip);
        let bottom = ok(m.turn((&a, &b), false, &point, &dip));
        assert!((top.x - 250.0).abs() <= 1e-6 && (bottom.x - 230.0).abs() <= 1e-6, "{} {}", top.x, bottom.x);
        assert!(top.at.t == top.x && top.value == ok(peak(&top.at)));
    }

    /// A line's nodes run from the lowest T up in `LINE_CELLS` cells to its top, each seeded by the last; where a point
    /// fails (above 260 K here) the cell is halved `LINE_HALVINGS` times toward the failure. A first point that fails is
    /// the refusal.
    #[test]
    fn a_line_s_nodes_end_at_its_top() {
        let m = methane_ethane();
        let lo = m.t_search().0;
        let top = m.critical_point().map_or(f64::NAN, |c| 1.2 * c.t).min(m.limits.t_max());
        // Each point after the first is seeded by the last one, below it.
        let seeded = |t: f64, near: Option<&Coexisting>| t == lo || near.is_some_and(|c| c.t < t);
        let below = |t: f64, near: Option<&Coexisting>| {
            if t < 260.0 && seeded(t, near) {
                Ok(node(t))
            } else {
                Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: 0 })
            }
        };
        let nodes = ok(m.line_nodes(&below));
        let step = (top - lo) / LINE_CELLS as f64;
        let last = nodes.last().map_or(f64::NAN, |c| c.t);
        assert!(nodes[0].t == lo && nodes.windows(2).all(|w| w[0].t < w[1].t) && last < 260.0, "{nodes:?}");
        assert!(260.0 - last <= step / math::powi(2.0, LINE_HALVINGS as i32), "{last}");
        let everywhere = |t: f64, near: Option<&Coexisting>| {
            if seeded(t, near) {
                Ok(node(t))
            } else {
                Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: 0 })
            }
        };
        let nodes = ok(m.line_nodes(&everywhere));
        assert!(nodes.len() == LINE_CELLS + 1 && nodes.last().map(|c| c.t) == Some(top));
        assert_eq!(nodes[1].t, lo + step);
        let nowhere =
            |_: f64, _: Option<&Coexisting>| Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: 0 });
        assert!(m.line_nodes(&nowhere).is_err());
    }

    /// A dome's bracket starts at the homogeneous root, or without one at 0.95 T_c.
    #[test]
    fn domes_start_at_the_root_or_below_the_critical_point() {
        let m = methane_ethane();
        let t_c = m.critical_point().map_or(f64::NAN, |c| c.t);
        assert!(m.dome_start(Some(123.0)) == 123.0 && m.dome_start(None) == 0.95 * t_c);
    }

    /// The feed at (T, ρ) as DT finds it: below T_c from the bubble and dew points alone, a density between their
    /// volumes splits with that volume and one outside them is one phase; above T_c (library Methane/Ethane at 85/15,
    /// 1.01 T_c), the split inside the retrograde dome is found in p, and a gas is one phase.
    #[test]
    fn isochoric_splits_below_and_above_the_critical_temperature() {
        let m = methane_ethane();
        let (bubble, dew) = (ok(m.coexisting(0.0, Given::T(200.0))), ok(m.coexisting(1.0, Given::T(200.0))));
        let rho = math::exp(0.5 * (math::ln(bubble.rho_liquid) + math::ln(dew.rho_vapour)));
        let c = ok(m.isochoric_split(200.0, rho)).unwrap_or_else(|| panic!("no split"));
        let volume = (1.0 - c.beta) / c.rho_liquid + c.beta / c.rho_vapour;
        assert!((volume * rho - 1.0).abs() <= 1e-13, "{c:?}");
        assert_eq!(ok(m.isochoric_split(200.0, 0.5 * dew.rho_vapour)), None);
        let rich = library(&["Methane", "Ethane"], &[0.85, 0.15]);
        let (t, p) = (220.148, 5e6);
        let Ok(Equilibrium::Split { beta, rho_liquid, rho_vapour, .. }) = rich.equilibrium(t, p) else {
            panic!("split")
        };
        let rho = 1.0 / ((1.0 - beta) / rho_liquid + beta / rho_vapour);
        let c = ok(rich.isochoric_split(t, rho)).unwrap_or_else(|| panic!("no split above T_c"));
        assert!((c.p / p - 1.0).abs() <= 1e-9 && (c.beta - beta).abs() <= 1e-8, "{c:?}");
        assert_eq!(ok(rich.isochoric_split(t, 100.0)), None);
    }

    /// Above T_c, u falls with ρ in the gas and rises in the dense fluid, so a u just above its minimum has two roots
    /// on the homogeneous isotherm, one on each side, and one below it none; through TU the two are `Ambiguous`, and
    /// `Nearest` takes the one nearest (library Methane/Ethane at 50/50, 1.5 T_c).
    #[test]
    fn an_isotherm_s_turn_gives_two_roots() {
        let m = methane_ethane();
        let t = 1.5 * m.critical_point().map_or(f64::NAN, |c| c.t);
        let top = math::ln(m.rho_max(t));
        let u_at = |u: f64| value(&ok(m.homogeneous(t, math::exp(u))), Var::U);
        // u's minimum along the isotherm, on a grid of its upper six decades, then on a finer one around it.
        let minimum = |from: f64, to: f64| {
            let grid = (0..=400).map(|k| from + (to - from) * f64::from(k) / 400.0).map(|u| (u, u_at(u)));
            grid.min_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((f64::NAN, f64::NAN))
        };
        let coarse = minimum(top - 6.0 * core::f64::consts::LN_10, top);
        let width = 6.0 * core::f64::consts::LN_10 / 400.0;
        let (u_min, at_min) = minimum(coarse.0 - width, coarse.0 + width);
        let x = at_min + 1.0;
        let roots = ok(m.homogeneous_roots(t, Var::U, x)).0;
        let r = m.fluid.gas_constant();
        let each = roots.iter().all(|rho| reproduces(Var::U, r, x, &ok(m.homogeneous(t, *rho))));
        assert!(roots.len() == 2 && roots[0] < math::exp(u_min) && math::exp(u_min) < roots[1] && each, "{roots:?}");
        assert!(ok(m.homogeneous_roots(t, Var::U, at_min - 1.0)).0.is_empty());
        assert!(matches!(m.isotherm(t, Var::U, x, &FlashOptions::new()), Err(Error::Ambiguous { .. })));
        let nearest = ok(m.isotherm(t, Var::U, x, &FlashOptions::new().with_roots(RootPolicy::Nearest(roots[1]))));
        assert!((nearest.rho(Basis::Molar) / roots[1] - 1.0).abs() <= 1e-12);
    }

    /// The isotherm's grid reaches `DECADES` below the model's largest density: library Methane/Ethane's s at 300 K and
    /// 1e-6 mol/m³, about ten decades below it, is its one root there. At the grid's top, where s is the isotherm's
    /// lowest, an s that is the top node's own is the root there.
    #[test]
    fn an_isotherm_s_grid_reaches_the_dilute_gas() {
        let m = methane_ethane();
        let rho = 1e-6;
        let s = value(&ok(m.homogeneous(300.0, rho)), Var::S);
        let roots = ok(m.homogeneous_roots(300.0, Var::S, s)).0;
        assert!(roots.len() == 1 && (roots[0] / rho - 1.0).abs() <= 1e-9, "{roots:?}");
        let top = math::exp(math::ln(m.rho_max(300.0)));
        let s_top = value(&ok(m.homogeneous(300.0, top)), Var::S);
        assert_eq!(ok(m.homogeneous_roots(300.0, Var::S, s_top)).0, vec![top]);
    }

    /// HS's descent in ln p runs by decades from the model's highest pressure through `DECADES` of them: library
    /// Methane/Ethane's stable states at 400 K and half that pressure, and at 300 K and 1e-4 of it, return their p.
    #[test]
    fn isentropes_reach_the_top_and_bottom_decades() {
        let m = methane_ethane();
        let p_max = m.limits.p_max();
        for (t, p) in [(400.0, 0.5 * p_max), (300.0, 1e-4 * p_max)] {
            let state = ok(m.homogeneous(t, ok(equilibrium::stable_root(&m, t, p))));
            let back = ok(m.isentrope(state.s(Basis::Molar), Var::H, state.h(Basis::Molar), &FlashOptions::new()));
            assert!((back.p() / state.p() - 1.0).abs() <= 1e-8, "{t} K: {} against {}", back.p(), state.p());
        }
    }

    /// A single-phase hint takes the homogeneous root without the tangent-plane test, labelled as hinted: HP and DH of a
    /// vapour 5 % denser than the dew point's at 200 K return that vapour as a gas, where without the hint they are two
    /// phases, as `TwoPhase` makes them; HT under `TwoPhase` is the two phases at T. An answer that does not reproduce
    /// its input is no convergence.
    #[test]
    fn hints_take_the_branch_they_name() {
        let m = methane_ethane();
        let dew = ok(m.coexisting(1.0, Given::T(200.0)));
        let rho = 1.05 * dew.rho_vapour;
        let s = ok(m.homogeneous(200.0, rho));
        let (h, p) = (s.h(Basis::Molar), s.p());
        let gas = FlashOptions::new().with_phase(Phase::Gas);
        let two = FlashOptions::new().with_phase(Phase::TwoPhase);
        let is_gas = |state: &State| state.phase() == Phase::Gas && (state.t() / 200.0 - 1.0).abs() <= 1e-9;
        assert!(is_gas(&ok(m.isobar(p, Var::H, h, &gas))) && is_gas(&ok(m.isochore(rho, Var::H, h, &gas))));
        for opts in [FlashOptions::new(), two] {
            let (by_p, by_rho) = (ok(m.isobar(p, Var::H, h, &opts)), ok(m.isochore(rho, Var::H, h, &opts)));
            assert!(by_p.quality().is_some() && by_rho.quality().is_some(), "{by_p:?} {by_rho:?}");
        }
        let split = ok(m.coexisting_state(&ok(m.coexisting(0.4, Given::T(200.0))), &searching()));
        let by_t =
            ok(m.isotherm(200.0, Var::H, split.h(Basis::Molar), &FlashOptions::new().with_phase(Phase::TwoPhase)));
        assert!(by_t.quality().is_some_and(|q| (q - 0.4).abs() <= 1e-8), "{by_t:?}");
        let wrong = m.answer(Found::Single(200.0, rho), (Var::H, h + 1.0), &FlashOptions::new(), Strategy::Isobar);
        assert_eq!(wrong.err(), Some(Error::NoConvergence { strategy: Strategy::Isobar, iterations: 0 }));
    }

    /// A quality outside [0, 1] is refused before any solve.
    #[test]
    fn qualities_outside_the_unit_interval_are_refused() {
        let m = methane_ethane();
        for q in [-0.1, 1.5] {
            let refused = m.quality_line((q, Basis::Molar), Var::S, 0.0, &FlashOptions::new());
            assert_eq!(refused.err(), Some(Error::InvalidInput { quantity: VAPOUR_FRACTION, value: q }));
        }
    }
}
