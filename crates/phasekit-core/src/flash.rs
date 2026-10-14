//! L4 the Helmholtz flash (D6): a pure function of an immutable package, a native input and per-call
//! options. No global configuration, no sticky phase, no exception cascade (map 01 R12, map 03 §6, map 12
//! R3/R4). Each pair runs a fixed, ordered list of strategies returning `Result`; the winner is recorded in
//! `State::path`. DT (M5.3), QT and PQ (M6.8), PT (M7.1; its phase rule complete at M7.2), PH, PS and PU (M7.3), DH,
//! DS and DU (M7.4), HT, ST and TU (M7.5), DP (M7.6), QS, HQ and DQ (M7.7) and HS and SU (M7.8) are implemented: all
//! 19 pairs (`IMPLEMENTED`). Each family's search has its module: the phase rule ([`phase`]), the search scaffolds
//! ([`search`]), P+X along the isobar ([`isobar`]), HS and SU along the isentrope ([`isentrope`]), D+X and DP along the
//! isochore ([`isochore`]), T+X along the isotherm ([`isotherm`]) and the pairs with a quality ([`quality`]); this one
//! dispatches, solves DT and PT, and holds what they share.
#![deny(clippy::indexing_slicing)] // E12: no panicking index on the flash path, its modules' included

mod cubic;
mod isentrope;
mod isobar;
mod isochore;
mod isotherm;
mod phase;
mod quality;
mod search;

use isentrope::sx;
use isobar::px;
use isochore::{dp, dx};
use isotherm::tx;
use phase::{Branch, Region, Where, pt_branch, region};
use quality::{QVar, pq, qt, qx};
use search::choose;

use crate::derivs::{Bundle, Order};
use crate::error::{DomainError, Error};
use crate::fluid::PureFluid;
use crate::input::{NativeInput, Pair, Var};
use crate::model::ThermoModel;
use crate::prop::Prop;
use crate::relations;
use crate::saturation::SatPair;
use crate::state::{Phase, SolvePath, State, Strategy};
use crate::units::{Basis, Quality};

/// How to choose among several roots of a non-unique pair (T+H in compressed liquid, Q-pairs with 2-3 roots;
/// map 03 §6). The default never guesses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum RootPolicy {
    /// Several roots → `Error::Ambiguous { roots }`.
    #[default]
    Strict,
    /// The root nearest to this value of the solved variable; of two equally near, the smaller.
    Nearest(f64),
    /// The thermodynamically stable root (lowest Gibbs energy).
    Stable,
}

/// Whether the validity domain is enforced, per call. Replaces CoolProp's global `DONT_CHECK_PROPERTY_LIMITS`
/// (map 03 §6). Under either policy a saturation fit (superancillary, ancillary) is never evaluated outside
/// its fitted range (D6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DomainPolicy {
    /// Refuse states below the triple point or `Tmin`, above `Tmax` or `pmax`, or below the melting line.
    #[default]
    Enforce,
    /// Evaluate the EOS there anyway, for metastable single-phase states (supercooled liquid water). The
    /// state is flagged (`State::is_extrapolated`). Where the phase rule would need a fit outside its range,
    /// the phase comes from a single-phase hint or the generic VLE (M6).
    Extrapolate,
}

/// Per-call options, passed by value. Private fields and `const` builders keep them growable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlashOptions {
    phase_hint: Option<Phase>,
    roots: RootPolicy,
    domain: DomainPolicy,
    guess: Option<(f64, f64)>,
}

impl FlashOptions {
    /// CoolProp-equivalent defaults: no hint, strict roots, domain enforced, cold start.
    pub const fn new() -> Self {
        Self { phase_hint: None, roots: RootPolicy::Strict, domain: DomainPolicy::Enforce, guess: None }
    }
    /// Imposes a phase for this call only (never sticky, ROT-069), honoured by every pair (ROT-070). A single-phase
    /// label skips the phase rule and solves on its branch; a single phase has no quality, so a Q-pair under one is
    /// `Undefined { prop: Q }`. `TwoPhase` requires the dome: inputs outside it are refused by their quality
    /// (`InvalidInput`, outside [0, 1]), and at or above the critical temperature, where there is no dome, the quality
    /// is `Undefined`. An input on the dome's edge, within the acceptance gate of a saturated side, is that side.
    pub const fn with_phase(mut self, phase: Phase) -> Self {
        self.phase_hint = Some(phase);
        self
    }
    /// Root selection policy.
    pub const fn with_roots(mut self, roots: RootPolicy) -> Self {
        self.roots = roots;
        self
    }
    /// Domain policy.
    pub const fn with_domain(mut self, domain: DomainPolicy) -> Self {
        self.domain = domain;
        self
    }
    /// Warm start at (T, ρ_molar). Opt-in: results then depend on the guess (K7).
    pub const fn with_guess(mut self, t: f64, rho: f64) -> Self {
        self.guess = Some((t, rho));
        self
    }
    /// The phase hint.
    pub const fn phase_hint(&self) -> Option<Phase> {
        self.phase_hint
    }
    /// The root policy.
    pub const fn roots(&self) -> RootPolicy {
        self.roots
    }
    /// The domain policy.
    pub const fn domain(&self) -> DomainPolicy {
        self.domain
    }
    /// The warm-start guess, if any.
    pub const fn guess(&self) -> Option<(f64, f64)> {
        self.guess
    }
}

/// The flash of a [`PureFluid`]: dispatch on the pair of an already normalised input.
pub(crate) fn flash(fluid: &PureFluid, input: NativeInput, opts: &FlashOptions) -> Result<State, Error> {
    let (x, y) = input.values();
    // A quality fixes a two-phase state, which an imposed single phase is not: there its quality is undefined (D6).
    let (vx, vy) = input.pair().vars();
    if let Some(phase) = opts.phase_hint().filter(|h| *h != Phase::TwoPhase)
        && (vx == Var::Q || vy == Var::Q)
    {
        return Err(Error::Undefined { prop: Prop::Q, phase });
    }
    let state = match input.pair() {
        Pair::DT => dt(fluid, y, x, opts),
        Pair::PT => pt(fluid, x, y, opts),
        Pair::QT => qt(fluid, x, y, opts),
        Pair::PQ => pq(fluid, x, y, opts),
        Pair::HP => named(Pair::HP, px(fluid, y, x, Caloric::H, opts)),
        Pair::PS => named(Pair::PS, px(fluid, x, y, Caloric::S, opts)),
        Pair::PU => named(Pair::PU, px(fluid, x, y, Caloric::U, opts)),
        Pair::HS => named(Pair::HS, sx(fluid, y, x, Caloric::H, opts)),
        Pair::SU => named(Pair::SU, sx(fluid, x, y, Caloric::U, opts)),
        Pair::DH => named(Pair::DH, dx(fluid, x, y, Caloric::H, opts)),
        Pair::DS => named(Pair::DS, dx(fluid, x, y, Caloric::S, opts)),
        Pair::DU => named(Pair::DU, dx(fluid, x, y, Caloric::U, opts)),
        Pair::HT => named(Pair::HT, tx(fluid, y, x, Caloric::H, opts)),
        Pair::ST => named(Pair::ST, tx(fluid, y, x, Caloric::S, opts)),
        Pair::TU => named(Pair::TU, tx(fluid, x, y, Caloric::U, opts)),
        Pair::DP => named(Pair::DP, dp(fluid, x, y, opts)),
        Pair::QS => qx(fluid, x, y, QVar::S, Pair::QS, opts),
        Pair::HQ => qx(fluid, y, x, QVar::H, Pair::HQ, opts),
        Pair::DQ => qx(fluid, y, 1.0 / x, QVar::V, Pair::DQ, opts),
    }?;
    above_melting_line(fluid, state, opts)
}

/// D6 (user decision 6): whatever pair found it, a single-phase state below the model's melting line, T < T_melt(p), is
/// refused under `Enforce` and returned with the flag under `Extrapolate`; a T within [`AT_BOUND`] of T_melt is on the
/// line, as at every bound, and where the curve says nothing (outside its pressures) nothing is checked. A two-phase
/// state is not checked: vapour and liquid at p_sat(T) meet the melting line only at the triple point, which a curve
/// whose p₀ lies below the EOS's triple pressure would refuse (CO₂'s by 14 Pa, 3 µK).
fn above_melting_line(fluid: &PureFluid, state: State, opts: &FlashOptions) -> Result<State, Error> {
    if state.quality().is_some() {
        return Ok(state);
    }
    let curve = fluid.melting_curve();
    match curve.and_then(|c| c.below(state.t(), state.p())).filter(|t_melt| state.t() < t_melt * (1.0 - AT_BOUND)) {
        None => Ok(state),
        Some(t_melt) if opts.domain() == DomainPolicy::Enforce => {
            Err(DomainError::BelowMeltingLine { t: state.t(), t_melt }.into())
        }
        Some(_) => Ok(state.mark_extrapolated()),
    }
}

/// Order-2 total bundle at (T, ρ): one ideal and one residual evaluation.
fn total(fluid: &PureFluid, t: f64, rho: f64) -> Result<Bundle, Error> {
    let eos = fluid.eos();
    let d = eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two);
    d.bundle().ok_or(Error::InvalidState { reason: "the model returned a bundle below the requested order 2" })
}

/// DT: no iteration. Temperature check, bundle, phase rule, `State`, then the pressure check on the state's own p (in
/// the dome the saturation pressure, not the single-phase EOS's, which can be 1e11 Pa there); flagged if outside the
/// domain (D6). A single-phase state the phase rule chose (no hint) must be mechanically stable: the acceptance gate
/// (D6; map 03 §6, `agent-notes.md`: dp/dρ > 0 and cv > 0).
fn dt(fluid: &PureFluid, t: f64, rho: f64, opts: &FlashOptions) -> Result<State, Error> {
    let enforce = opts.domain() == DomainPolicy::Enforce;
    let t_check = check_t(fluid, t);
    if enforce {
        t_check?;
    }
    let (r, m, key) = (fluid.eos().gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    let b = total(fluid, t, rho)?;
    let p = relations::pressure(r, t, rho, &b);
    let state = match region(fluid, t, rho, p, opts)? {
        Region::Single(phase) => {
            if opts.phase_hint().is_none() && phase != Phase::CriticalPoint {
                accept(r, t, &b)?;
            }
            State::single(key, t, rho, r, m, phase, &b, DIRECT)
        }
        Region::Dome(sat, strategy) => {
            // Lever rule on molar volume; an imposed TwoPhase outside the dome fails here (q ∉ [0, 1]), but at a
            // density within the acceptance gate's share of a saturated one it is that side (a D+X or T+X root on the
            // dome's edge rounds to either side of it).
            let (rl, rv) = (sat.bubble.rho, sat.dew.rho);
            let q = (1.0 / rho - 1.0 / rl) / (1.0 / rv - 1.0 / rl);
            let q = if rho <= rl * (1.0 + REPRODUCED_X) { q.max(0.0) } else { q };
            let q = if rho >= rv * (1.0 - REPRODUCED_X) { q.min(1.0) } else { q };
            two_phase(fluid, &sat, Quality::new(q)?, strategy)
        }
    }?;
    let p_check = check_p(fluid, state.p());
    if enforce {
        p_check?;
    }
    // Under `Extrapolate`, a state outside the domain is returned, never silently: it carries the flag.
    Ok(if t_check.is_ok() && p_check.is_ok() { state } else { state.mark_extrapolated() })
}

/// PT (PLAN.md M7.1, M7.2; map 03 §3.1, §9 `flash::pt`): the domain checked, the branch of (p, T) ([`pt_branch`]), then
/// the density on it; at the saturation pressure both roots, which `Nearest` chooses between and `Strict` and `Stable`
/// return as `Ambiguous` (ROT-080). A bracketed branch holds one root ([`crate::density::bracketed`]), so a guess
/// (`with_guess`) only seeds its Newton and moves no output beyond rounding (ROT-017, ROT-072); where the seed picks
/// the root (a pseudo-pure fluid's branch, an imposed phase) a guess is not used. The acceptance gate (D6): the state's
/// own p reproduces the input to the rounding of the terms it cancels from (ROT-074: CoolProp's PT reports p up to 1e-8
/// off it, and a liquid at 1 Pa cannot hold p to 1e-9 in double precision), and, unless a phase is imposed, the state
/// is mechanically stable.
fn pt(fluid: &PureFluid, p: f64, t: f64, opts: &FlashOptions) -> Result<State, Error> {
    let check = check_t(fluid, t).and_then(|()| check_p(fluid, p));
    if opts.domain() == DomainPolicy::Enforce {
        check?;
    }
    let eos = fluid.eos();
    let (r, m, key) = (eos.gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    let solve = |branch: &Branch| solve_branch(fluid, p, t, branch, opts.guess().map_or(branch.seed, |(_, rho)| rho));
    // At psat `Nearest` picks the root nearer its density; `Strict` and `Stable` (neither is more stable) return both.
    let (branch, (rho, path)) = match pt_branch(fluid, p, t, opts)? {
        Where::One(branch) => {
            let solved = solve(&branch)?;
            (branch, solved)
        }
        Where::Both { gas, liquid } => {
            let (g, l) = (solve(&gas)?, solve(&liquid)?);
            match choose(&[g.0, l.0], |rho| *rho, opts.roots())? {
                Some(rho) if rho == l.0 => (liquid, l),
                _ => (gas, g),
            }
        }
    };
    let b = total(fluid, t, rho)?;
    if branch.phase != Phase::CriticalPoint {
        if !crate::density::reproduces(r, t, rho, p, &b) {
            return Err(Error::NoConvergence { strategy: Strategy::DensityNewton, iterations: path.iterations });
        }
        if opts.phase_hint().is_none() {
            accept(r, t, &b)?;
        }
    }
    let state = State::single(key, t, rho, r, m, branch.phase, &b, path)?;
    Ok(if check.is_ok() { state } else { state.mark_extrapolated() })
}

/// The density on `branch` at (T, p) and the path of its solve: a bracketed branch from `seed` (outside the bracket,
/// from its middle), one without a bracket from the branch's own seed, the critical point with no solve.
fn solve_branch(fluid: &PureFluid, p: f64, t: f64, branch: &Branch, seed: f64) -> Result<(f64, SolvePath), Error> {
    let eos = fluid.eos();
    let (rho, iterations) = match branch.bracket {
        _ if branch.phase == Phase::CriticalPoint => return Ok((branch.seed, DIRECT)),
        Some((lo, hi)) => {
            let solved = crate::density::bracketed(eos, t, p, (lo, hi), seed);
            // A root past the model's largest density: its bracket grows (`density::liquid_above`).
            let grown = |e| if hi == eos.rho_max(t) { crate::density::liquid_above(eos, t, p, hi) } else { Err(e) };
            solved.or_else(grown)?
        }
        None => crate::density::at_t_p(eos, t, p, branch.seed, branch.liquid)?,
    };
    Ok((rho, SolvePath { strategy: Strategy::DensityNewton, iterations }))
}

/// The caloric variable a P+X pair fixes, molar in the model's native gauge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Caloric {
    H,
    S,
    U,
}

impl Caloric {
    /// The name its input goes by.
    fn name(self) -> &'static str {
        match self {
            Caloric::H => "H",
            Caloric::S => "S",
            Caloric::U => "U",
        }
    }

    /// Its value on a state of the flash (native gauge).
    fn of(self, state: &State) -> f64 {
        match self {
            Caloric::H => state.h(Basis::Molar),
            Caloric::S => state.s(Basis::Molar),
            Caloric::U => state.u(Basis::Molar),
        }
    }

    /// Its value at T on a single phase with total bundle `b`.
    fn at(self, r: f64, t: f64, b: &Bundle) -> f64 {
        match self {
            Caloric::H => relations::enthalpy(r, t, b),
            Caloric::S => relations::entropy(r, b),
            Caloric::U => relations::internal_energy(r, t, b),
        }
    }

    /// The sign of its slope along an isotherm, (∂x/∂ln ρ)_T, on a single phase with total bundle `b`, as a number of
    /// that sign: δ·∂/∂δ takes A_i0 to A_i1 and A01 to A01 + A02, and the slope is this times RT (R for s), which
    /// T+X reads only for where it changes sign.
    fn slope_sign(self, b: &Bundle) -> f64 {
        match self {
            Caloric::H => b.a11 + b.a01 + b.a02,
            Caloric::S => b.a11 - b.a01,
            Caloric::U => b.a11,
        }
    }

    /// (∂x/∂T)_ρ and (∂x/∂ln ρ)_T from the total bundle at T: for h R·(A01 − A20 − A11), for s −R·A20/T and for u
    /// −R·A20, then the slope [`Caloric::slope_sign`] signs, times RT (R for s). P+X's Newton in (T, ln ρ) reads them.
    fn jacobian(self, r: f64, t: f64, b: &Bundle) -> (f64, f64) {
        match self {
            Caloric::H => (r * (b.a01 - b.a20 - b.a11), r * t * self.slope_sign(b)),
            Caloric::S => (-r * b.a20 / t, r * self.slope_sign(b)),
            Caloric::U => (-r * b.a20, r * t * self.slope_sign(b)),
        }
    }

    /// The scale its input is reproduced to (the acceptance gate, D6): |x|, at least RT for h and u and R for s.
    fn scale(self, r: f64, t: f64, x: f64) -> f64 {
        x.abs().max(if self == Caloric::S { r } else { r * t })
    }
}

/// The relative tolerance on T of a P+X or D+X solve: a hundredth of class `Flash`'s 1e-9 even where
/// (∂ln ρ/∂ln T)_p is 1e3.
const T_TOL: f64 = 1e-14;

/// Iterations allowed to a P+X or D+X solve in T.
const T_ITER: u16 = 100;

/// The share of its scale (`Caloric::scale`) to which a P+X or D+X state reproduces x (class `Flash`).
const REPRODUCED_X: f64 = 1e-9;

/// The share of its scale within which an input is the value of a state at the domain's lowest temperature, where no
/// colder state brackets the root: the rounding of x evaluated there (about 1e-15), far inside the acceptance gate. A
/// looser share would make every state within it the root, a band of p 1.2e-8 wide for CO₂'s gas at its triple point.
const AT_LOWEST_T: f64 = 1e-13;

/// The acceptance gate's "inputs reproduced" for a caloric input (D6): `got` within [`REPRODUCED_X`] of x's scale.
fn reproduces_x(var: Caloric, r: f64, t: f64, x: f64, got: f64) -> bool {
    (got - x).abs() <= REPRODUCED_X * var.scale(r, t, x)
}

/// The acceptance gate's "inputs reproduced" for a pressure input (D6): `got` within [`REPRODUCED_X`] of p.
fn reproduces_p(p: f64, got: f64) -> bool {
    (got - p).abs() <= REPRODUCED_X * p
}

/// The domain's lowest temperature, the higher of Tmin and the triple point, and the highest a P+X or D+X solve
/// searches, 1.5·Tmax (CoolProp's bound).
fn t_search(fluid: &PureFluid) -> (f64, f64) {
    let limits = fluid.limits();
    (limits.t_triple().map_or(limits.t_min(), |t| t.max(limits.t_min())), 1.5 * limits.t_max())
}

/// A P+X or D+X result with another pair's refusal from inside it named for `pair` (ROT-021: CoolProp's PH and PS
/// brackets raise "Pressure to PQ_flash"): a curve with no pressure inverse, a model with no phase rule.
fn named(pair: Pair, result: Result<State, Error>) -> Result<State, Error> {
    result.map_err(|e| match e {
        Error::Unsupported { .. } => Error::Unsupported { pair },
        e => e,
    })
}

/// A share of a domain bound within which a value is the bound, given or solved: a T or p solved at Tmax or pmax lands
/// a few ulp to either side (a P+X state at Tmax 5e-13 K over it), and a state's own p, read back as an input, carries
/// its rounding (CO₂'s PT at its pmax of 800 MPa reports p 6.6e-14 above it). Every pair's inputs and states are
/// checked with it (ROT-079).
const AT_BOUND: f64 = 1e-12;

/// The model's temperature limits (D6, `Limits::check_t`), a T within [`AT_BOUND`] of a bound counting as on it.
fn check_t(fluid: &PureFluid, t: f64) -> Result<(), DomainError> {
    let limits = fluid.limits();
    let t_low = limits.t_triple().map_or(limits.t_min(), |tt| tt.max(limits.t_min()));
    let on = |bound: f64| (t - bound).abs() <= AT_BOUND * bound;
    limits.check_t(if on(t_low) {
        t_low
    } else if on(limits.t_max()) {
        limits.t_max()
    } else {
        t
    })
}

/// The model's pressure limit (D6, `Limits::check_p`), a p within [`AT_BOUND`] of pmax counting as on it.
fn check_p(fluid: &PureFluid, p: f64) -> Result<(), DomainError> {
    let p_max = fluid.limits().p_max();
    fluid.limits().check_p(if (p - p_max).abs() <= AT_BOUND * p_max { p_max } else { p })
}

/// A saturation state against the model's limits (D6): its T and p checked; under `Enforce` a state outside them is
/// refused, under `Extrapolate` returned with the flag.
fn within_domain(fluid: &PureFluid, state: State, opts: &FlashOptions) -> Result<State, Error> {
    match check_t(fluid, state.t()).and_then(|()| check_p(fluid, state.p())) {
        Ok(()) => Ok(state),
        Err(e) if opts.domain() == DomainPolicy::Enforce => Err(e.into()),
        Err(_) => Ok(state.mark_extrapolated()),
    }
}

/// The two-phase state at quality `q` on the pure saturation `sat`, solved by `strategy` (a VLE's iterations are not
/// counted here): both phases from the EOS at their densities ([`phase_bundle`]).
fn two_phase(fluid: &PureFluid, sat: &SatPair, q: Quality, strategy: Strategy) -> Result<State, Error> {
    let (r, m, key) = (fluid.eos().gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    let path = SolvePath { strategy, iterations: 0 };
    let (t, rl, rv) = (sat.bubble.t, sat.bubble.rho, sat.dew.rho);
    let liquid = State::single(key, t, rl, r, m, Phase::Liquid, &phase_bundle(fluid, t, rl, q)?, path)?;
    let vapour = State::single(key, t, rv, r, m, Phase::Gas, &phase_bundle(fluid, t, rv, q)?, path)?;
    State::split(liquid, vapour, q, t, sat.bubble.p, path)
}

/// A saturated phase's total bundle at (T, ρ) in a two-phase state at quality `q`: to order 2 at Q = 0 and 1, where the
/// state reads that phase's c_p, c_v, w and partials (user decisions SQ1-SQ3); to order 1 strictly inside the dome,
/// where it reads only ρ, h, s and u and refuses the rest, its second-order entries zero (user decision PF1).
fn phase_bundle(fluid: &PureFluid, t: f64, rho: f64, q: Quality) -> Result<Bundle, Error> {
    if q.get() == 0.0 || q.get() == 1.0 {
        return total(fluid, t, rho);
    }
    let eos = fluid.eos();
    let d = eos.ideal(t, rho, Order::One) + eos.residual(t, rho, Order::One);
    let at = |i, j| d.get(i, j).ok_or(Error::InvalidState { reason: "the model returned a bundle below order 1" });
    Ok(Bundle { a00: at(0, 0)?, a10: at(1, 0)?, a01: at(0, 1)?, a20: 0.0, a11: 0.0, a02: 0.0 })
}

/// The no-iteration path.
const DIRECT: SolvePath = SolvePath { strategy: Strategy::Direct, iterations: 0 };

/// The acceptance gate for a single-phase state the flash chose: (∂p/∂ρ)_T > 0 and cv > 0 (D6); an infinite cv (a
/// non-analytic critical point) passes.
fn accept(r: f64, t: f64, b: &Bundle) -> Result<(), Error> {
    let stable = relations::dp_drho_t(r, t, b) > 0.0 && relations::cv(r, b) > 0.0;
    if stable { Ok(()) } else { Err(DomainError::MechanicallyUnstable.into()) }
}

/// The refusal of a quality where there is none to give: (p, T), or an imposed `TwoPhase` where there is no dome (D6).
fn no_quality() -> Error {
    Error::Undefined { prop: Prop::Q, phase: Phase::TwoPhase }
}

#[cfg(test)]
mod tests;
