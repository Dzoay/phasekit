//! L4 the Helmholtz flash (D6): a pure function of an immutable package, a native input and per-call
//! options. No global configuration, no sticky phase, no exception cascade (map 01 R12, map 03 §6, map 12
//! R3/R4). Each pair runs a fixed, ordered list of strategies returning `Result`; the winner is recorded in
//! `State::path`. DT (M5.3), QT and PQ (M6.8), PT (M7.1; its phase rule complete at M7.2), PH, PS and PU (M7.3), DH,
//! DS and DU (M7.4), HT, ST and TU (M7.5), DP (M7.6), QS, HQ and DQ (M7.7) and HS and SU (M7.8) are implemented: all
//! 19 pairs (`IMPLEMENTED`).
#![deny(clippy::indexing_slicing)] // E12: no panicking index on the flash path

use crate::derivs::{Bundle, Order};
use crate::error::{DomainError, Error, Roots};
use crate::fluid::PureFluid;
use crate::input::{NativeInput, Pair, Var};
use crate::model::ThermoModel;
use crate::num::math;
use crate::prop::Prop;
use crate::relations;
use crate::roots::{Tol, toms748};
use crate::saturation::{SatAccuracy, SatPair, SatSide, SaturationCurve};
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
    match input.pair() {
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
    }
}

/// Where a (T, ρ) point lies.
enum Region {
    Single(Phase),
    /// The saturation at T and the strategy that gave it.
    Dome(SatPair, Strategy),
}

/// The one kernel-defined phase rule (map 01 R26): per-call hint, else the model's critical point, else the
/// saturation curve. The critical point that labels states is the model's own: the top of an exact saturation curve
/// (CoolProp's with superancillaries on), else the published one; R114's published 418.83 K and 3.257 MPa sit 1.8 K and
/// 3 % below its EOS's (map 03 §3.3, measured at M5.3), and the dome reaches up to the EOS's. A `Guess` curve (a
/// superancillary the EOS was edited away from, E14) only seeds the pure VLE, whose saturation decides (M6.6; ROT-088,
/// ROT-096). Below Tc without a curve whose fitted range covers T the generic VLE of the EOS decides (M7.2,
/// [`generic_vle`]), never a load error, a guess or a fit evaluated outside its range (S-10, D6).
fn region(fluid: &PureFluid, t: f64, rho: f64, p: f64, opts: &FlashOptions) -> Result<Region, Error> {
    let hint = opts.phase_hint();
    if let Some(single) = hint.filter(|h| *h != Phase::TwoPhase) {
        return Ok(Region::Single(single));
    }
    let curve = fluid.saturation()?;
    let crit = labelling_critical(fluid, curve)?;
    // At and above the critical temperature there is no dome to impose: no quality (D6, ROT-070).
    if hint == Some(Phase::TwoPhase) && crit.is_some_and(|c| t >= c.0) {
        return Err(Error::Undefined { prop: Prop::Q, phase: Phase::TwoPhase });
    }
    if hint.is_none()
        && let Some((t_c, p_c, rho_c)) = crit.filter(|c| t >= c.0)
    {
        let phase = if t == t_c && rho == rho_c {
            Phase::CriticalPoint
        } else if p > p_c {
            Phase::Supercritical
        } else {
            Phase::SupercriticalGas
        };
        return Ok(Region::Single(phase));
    }
    // A liquid above the critical pressure is a supercritical liquid, as CoolProp labels it.
    let liquid = |crit: Option<(f64, f64, f64)>| match crit {
        Some((_, p_c, _)) if p > p_c => Phase::SupercriticalLiquid,
        _ => Phase::Liquid,
    };
    if let Some(curve) = curve.filter(|c| fitted(*c, t) && c.accuracy() == SatAccuracy::Definition)
        && hint.is_none()
    {
        match pseudo_pure_bands(fluid, t, rho, p, &curve.at_t(t)?) {
            Some(Phase::Liquid) => return Ok(Region::Single(liquid(crit))),
            Some(phase) => return Ok(Region::Single(phase)),
            None => {}
        }
    }
    let (sat, strategy) = saturation_at_t(fluid, curve, crit, t)?;
    Ok(if hint == Some(Phase::TwoPhase) {
        Region::Dome(sat, strategy)
    } else if rho >= sat.bubble.rho {
        Region::Single(liquid(crit))
    } else if rho <= sat.dew.rho {
        Region::Single(Phase::Gas)
    } else {
        Region::Dome(sat, strategy)
    })
}

/// The saturation at T below the labelling critical point and the strategy that gave it: an exact curve's; a stale
/// curve's, or a pseudo-pure fluid's definition, seeding the pure VLE (M6.6; inside its dome CoolProp solves the
/// pure-fluid VLE of the blend's EOS from the ancillaries' densities, D4, map 04 U4); else the generic VLE (M7.2).
fn saturation_at_t(
    fluid: &PureFluid,
    curve: Option<&dyn SaturationCurve>,
    crit: Option<(f64, f64, f64)>,
    t: f64,
) -> Result<(SatPair, Strategy), Error> {
    Ok(match curve.filter(|c| fitted(*c, t)) {
        Some(curve) if curve.accuracy() == SatAccuracy::Exact => (curve.at_t(t)?, Strategy::Superancillary),
        Some(curve) => {
            let seed = curve.at_t(t)?;
            (crate::vle::at_t(fluid.eos(), t, (seed.bubble.rho, seed.dew.rho))?, Strategy::Vle)
        }
        None => (generic_vle(fluid, crit, t, Pair::DT)?, Strategy::Vle),
    })
}

/// Whether `curve` is fitted at T (a fit is never evaluated outside its range, D6).
fn fitted(curve: &dyn SaturationCurve, t: f64) -> bool {
    let (lo, hi) = curve.t_range();
    (lo..=hi).contains(&t)
}

/// The saturation at T where no curve's fitted range covers T (PLAN.md M7.2): the generic pure VLE of the EOS, seeded
/// by its own Maxwell construction ([`crate::vle::from_eos`]), below the labelling critical point; `Unsupported` for
/// `pair` without one, or at or above it.
fn generic_vle(fluid: &PureFluid, crit: Option<(f64, f64, f64)>, t: f64, pair: Pair) -> Result<SatPair, Error> {
    match crit {
        Some((t_c, _, rho_c)) if t < t_c => crate::vle::from_eos(fluid.eos(), t, rho_c),
        _ => Err(Error::Unsupported { pair }),
    }
}

/// The critical point (T, p, ρ) that labels states: the top of an exact saturation curve, else the published one
/// ([`region`]).
fn labelling_critical(
    fluid: &PureFluid,
    curve: Option<&dyn SaturationCurve>,
) -> Result<Option<(f64, f64, f64)>, Error> {
    Ok(match curve.filter(|c| c.accuracy() == SatAccuracy::Exact) {
        Some(c) => {
            let t_c = c.t_range().1;
            let top = c.at_t(t_c)?;
            Some((t_c, top.bubble.p, top.bubble.rho))
        }
        None => fluid.critical_point().map(|c| (c.t, c.p, c.rho)),
    })
}

/// CoolProp's density bands for a pseudo-pure fluid's DT below its critical temperature, before its VLE (D4, user
/// decision PS3; `HelmholtzEOSMixtureBackend.cpp:2340-2380`), at (T, ρ) of `fluid` with the EOS's p: see [`band`],
/// stability being (∂p/∂ρ)_T > 0 and (∂²p/∂ρ²)_T > 0 (`crate::crit::conditions`).
fn pseudo_pure_bands(fluid: &PureFluid, t: f64, rho: f64, p: f64, sat: &SatPair) -> Option<Phase> {
    band(rho, p, sat, || stable(crate::crit::conditions(fluid.eos(), t, rho)))
}

/// Stability as the bands ask it, from (∂p/∂ρ)_T and (∂²p/∂ρ²)_T (`crate::crit::conditions`): both positive.
fn stable(conditions: Option<[f64; 2]>) -> bool {
    conditions.is_some_and(|[k1, k2]| k1 > 0.0 && k2 > 0.0)
}

/// The bands at density ρ and pressure p for the ancillary sides `sat`: gas below 0.95 of the dew density, liquid above
/// 1.05 of the bubble one, and in the liquid strip, above 0.9975 of the bubble density, liquid by [`strip_liquid`].
/// `None`: the VLE of the blend's EOS decides. CoolProp asks the same in a vapour strip below 0.9975 of the dew
/// density, where the ancillary quality exceeds 1 (ρ < ρ″ ≤ ρ′), so it never answers liquid there and is not asked. The
/// products are CoolProp's, rounded as it rounds them.
fn band(rho: f64, p: f64, sat: &SatPair, stable: impl FnOnce() -> bool) -> Option<Phase> {
    let (rho_l, rho_v) = (sat.bubble.rho, sat.dew.rho);
    let (rho_vap, rho_liq) = (0.95 * rho_v, 1.05 * rho_l);
    if rho < rho_vap {
        return Some(Phase::Gas);
    }
    if rho > rho_liq {
        return Some(Phase::Liquid);
    }
    if rho <= 0.95 * rho_liq {
        return None;
    }
    let q_anc = (1.0 / rho - 1.0 / rho_l) / (1.0 / rho_v - 1.0 / rho_l);
    strip_liquid(q_anc, p, sat.bubble.p, stable).then_some(Phase::Liquid)
}

/// A state in the liquid strip is liquid when its ancillary quality is below 0.01, p is above 1.05 of the bubble
/// pressure `p_bubble` and it is `stable`, asked last and only then.
fn strip_liquid(q_anc: f64, p: f64, p_bubble: f64, stable: impl FnOnce() -> bool) -> bool {
    q_anc < 0.01 && p > 1.05 * p_bubble && stable()
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
            match opts.roots() {
                RootPolicy::Nearest(x) if nearest(x, &[g.0, l.0]) == Some(l.0) => (liquid, l),
                RootPolicy::Nearest(_) => (gas, g),
                _ => return Err(Error::Ambiguous { roots: Roots::new(&[g.0, l.0]) }),
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

/// The branch of a PT state: its phase label, its density's bracket (`None`: solved from the seed alone), the seed, and
/// whether a restart begins at the liquid's far end.
struct Branch {
    phase: Phase,
    bracket: Option<(f64, f64)>,
    seed: f64,
    liquid: bool,
}

/// Where (p, T) lies: one branch, or both at the saturation pressure.
enum Where {
    One(Branch),
    Both { gas: Branch, liquid: Branch },
}

/// Where (p, T) lies, by CoolProp's T-based rule (`HelmholtzEOSMixtureBackend.cpp:2177-2260`, 2329-2350; map 03 §3.1),
/// on the critical point that labels states ([`labelling_critical`]):
/// - an imposed single phase: its own branch, unbracketed, from the curve's density at T (a liquid's) or the ideal
///   gas's; an imposed `TwoPhase` is refused, since (p, T) fix no quality;
/// - the critical point itself: that point;
/// - at or above Tc: supercritical above pc, else a supercritical gas, on one branch from a thousandth of the ideal
///   gas's density (or of the largest) to the model's largest;
/// - below Tc above pc, or above the saturation pressure: a (supercritical) liquid between ρ′(T) and the largest
///   density, from ρ′; below it, a gas between a thousandth of the ideal gas's density and ρ″(T), from the ideal gas's.
///   A `Guess` curve's saturation is the VLE's, seeded by it;
/// - a pseudo-pure fluid's `Definition`: liquid above the bubble pressure ancillary, gas below the dew one, each from
///   its ancillary's seed and unbracketed, as CoolProp's (the blend's EOS has its own dome); between them (p, T) is
///   two-phase, where it fixes no quality.
///
/// At exactly the saturation pressure both the liquid and the vapour are roots ([`Where::Both`]; ROT-080: CoolProp's
/// answer there depends on T). Below Tc without a curve whose fitted range covers T, the generic VLE of the EOS gives
/// the saturation, as for DT ([`generic_vle`]).
fn pt_branch(fluid: &PureFluid, p: f64, t: f64, opts: &FlashOptions) -> Result<Where, Error> {
    let eos = fluid.eos();
    let (rho_max, ideal) = (eos.rho_max(t), p / (eos.gas_constant() * t));
    let floor = 1e-3 * ideal.min(rho_max);
    let no_quality = Error::Undefined { prop: Prop::Q, phase: Phase::TwoPhase };
    let curve = fluid.saturation()?;
    let fitted = curve.filter(|c| fitted(*c, t));
    if let Some(hint) = opts.phase_hint() {
        let liquid = matches!(hint, Phase::Liquid | Phase::SupercriticalLiquid);
        let seed = match fitted.map(|c| c.at_t(t)) {
            _ if !liquid => ideal,
            Some(Ok(sat)) => sat.bubble.rho,
            _ => rho_max,
        };
        return if hint == Phase::TwoPhase {
            Err(no_quality)
        } else {
            Ok(Where::One(Branch { phase: hint, bracket: None, seed, liquid }))
        };
    }
    let crit = labelling_critical(fluid, curve)?;
    if let Some((t_c, p_c, rho_c)) = crit {
        if t == t_c && p == p_c {
            return Ok(Where::One(Branch { phase: Phase::CriticalPoint, bracket: None, seed: rho_c, liquid: false }));
        }
        if t >= t_c {
            let phase = if p > p_c { Phase::Supercritical } else { Phase::SupercriticalGas };
            return Ok(Where::One(Branch { phase, bracket: Some((floor, rho_max)), seed: ideal, liquid: false }));
        }
    }
    let above_pc = crit.is_some_and(|(_, p_c, _)| p > p_c);
    let liquid = if above_pc { Phase::SupercriticalLiquid } else { Phase::Liquid };
    let sat = match fitted {
        Some(curve) => {
            let sat = curve.at_t(t)?;
            match curve.accuracy() {
                SatAccuracy::Definition if p > sat.bubble.p => {
                    let branch = Branch { phase: liquid, bracket: None, seed: sat.bubble.rho, liquid: true };
                    return Ok(Where::One(branch));
                }
                SatAccuracy::Definition if p < sat.dew.p => {
                    let branch = Branch { phase: Phase::Gas, bracket: None, seed: ideal, liquid: false };
                    return Ok(Where::One(branch));
                }
                SatAccuracy::Definition => return Err(no_quality),
                SatAccuracy::Exact => sat,
                _ => crate::vle::at_t(eos, t, (sat.bubble.rho, sat.dew.rho))?,
            }
        }
        None => generic_vle(fluid, crit, t, Pair::PT)?,
    };
    let lower = sat.bubble.rho * (1.0 - SAT_RELAX);
    let liquid = Branch { phase: liquid, bracket: Some((lower, rho_max)), seed: sat.bubble.rho, liquid: true };
    let upper = sat.dew.rho / (1.0 - SAT_RELAX);
    let gas = Branch { phase: Phase::Gas, bracket: Some((floor, upper)), seed: ideal, liquid: false };
    Ok(if above_pc || p > sat.bubble.p {
        Where::One(liquid)
    } else if p < sat.bubble.p {
        Where::One(gas)
    } else {
        Where::Both { gas, liquid }
    })
}

/// A PT bracket's saturated end reaches this far past ρ′ (into the metastable liquid) or ρ″ (the metastable vapour), a
/// factor 1 − `SAT_RELAX` in ρ either way: a curve's densities and pressure agree with the EOS's own VLE only to its
/// fit, so for p a hair from psat the root lies a hair past them. The spinodal is far further (0.2·ln(ρ′/ρ″) inside
/// each phase near Tc, mean field), so the branch stays monotonic (PLAN.md M7.2).
const SAT_RELAX: f64 = 1e-9;

/// `RootPolicy::Nearest(x)`'s choice among ascending `roots`, one rule for every pair: the root nearest to `x`, and of
/// two equally near the first, the smaller; `None` without roots.
fn nearest(x: f64, roots: &[f64]) -> Option<f64> {
    let distance = |root: &f64| (root - x).abs();
    roots.iter().copied().min_by(|a, b| distance(a).total_cmp(&distance(b)))
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

/// The bisections [`root_in_t`] takes from a T above x towards a refusal right below it: from a step of a fifth of T to
/// under [`T_TOL`] of it (0.2·2⁻⁴⁵ ≈ 6e-15).
const T_HALVINGS: u16 = 45;

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

/// P+X: PH, PS and PU (PLAN.md M7.3; map 03 §3.1, §9 `flash::px`). The saturation at p ([`saturation_at_p`]) splits
/// the isobar: an x below the saturated liquid's is the liquid, between T_low and the bubble temperature; above the
/// saturated vapour's the gas, between the dew temperature and 1.5·Tmax (CoolProp's bound); between them the dome, by
/// the lever rule on x. Above the dome's top or below its lowest pressure one branch runs from T_low to 1.5·Tmax. On a
/// branch T is found by TOMS 748 ([`isobar`]). T_low is the domain's lowest temperature, the higher of Tmin and the
/// triple point: an x colder than the branch there is `BelowMinTemperature` at T_low, and one hotter than 1.5·Tmax is
/// `AboveMaxTemperature` at that bound; a T solved between Tmax and 1.5·Tmax is refused under `Enforce` and flagged
/// under `Extrapolate`, as PT's. A pseudo-pure fluid's dome follows its ancillaries, as its PQ and CoolProp's PH: T
/// linear in the quality between the bubble and dew temperatures. An imposed single phase is solved on its branch from
/// T_low to 1.5·Tmax, without the stability check; an imposed `TwoPhase` needs the saturation at p and a quality in
/// [0, 1]. Errors name this pair ([`named`]).
fn px(fluid: &PureFluid, p: f64, x: f64, var: Caloric, opts: &FlashOptions) -> Result<State, Error> {
    if opts.domain() == DomainPolicy::Enforce {
        check_p(fluid, p)?;
    }
    let (t_low, t_high) = t_search(fluid);
    let hint = opts.phase_hint();
    let along = |lo, hi, ends, liquid| isobar(fluid, p, x, var, opts, Span { lo, hi, ends, liquid });
    if hint.is_some_and(|h| h != Phase::TwoPhase) {
        return along(t_low, t_high, (None, None), false);
    }
    let Some((sat, strategy)) = saturation_at_p(fluid, p, t_low, opts)? else {
        if hint == Some(Phase::TwoPhase) {
            return Err(Error::Undefined { prop: Prop::Q, phase: Phase::TwoPhase });
        }
        return along(t_low, t_high, (None, None), false);
    };
    // Each saturated side's x, the EOS's at its (T, ρ): a pseudo-pure side's ρ is the EOS's at its (T, p), as PQ's.
    let side = |side: &SatSide, liquid: bool| -> Result<f64, Error> {
        let rho = match strategy {
            Strategy::Ancillary => crate::density::at_t_p(fluid.eos(), side.t, side.p, side.rho, liquid)?.0,
            _ => side.rho,
        };
        Ok(var.at(fluid.eos().gas_constant(), side.t, &total(fluid, side.t, rho)?))
    };
    // The lever rule's quality on x: below 0 the liquid, above 1 the gas, unless `TwoPhase` is imposed; then an x the
    // acceptance gate takes for a side's is that side (an HS root on the dome's edge).
    let (x_l, x_v) = (side(&sat.bubble, true)?, side(&sat.dew, false)?);
    let (q, imposed) = ((x - x_l) / (x_v - x_l), hint == Some(Phase::TwoPhase));
    if !imposed && q < 0.0 {
        return along(t_low, sat.bubble.t, (None, Some(x_l)), true);
    }
    if !imposed && q > 1.0 {
        return along(sat.dew.t, t_high, (Some(x_v), None), false);
    }
    let r = fluid.eos().gas_constant();
    let q = if reproduces_x(var, r, sat.bubble.t, x, x_l) { q.max(0.0) } else { q };
    let q = Quality::new(if reproduces_x(var, r, sat.dew.t, x, x_v) { q.min(1.0) } else { q })?;
    let state = match strategy {
        Strategy::Ancillary => split_sides(fluid, &sat, q, q.get() * sat.dew.t + (1.0 - q.get()) * sat.bubble.t, p),
        strategy => two_phase(fluid, &sat, q, strategy),
    }?;
    within_domain(fluid, state, opts)
}

/// The share of its scale (`Caloric::scale`) within which x(PS(p, s)) is x while HS and SU seek their bracket: just
/// above the noise PS's own tolerance on T leaves in x (c·T·1e-14, under 1e-13 of RT), far inside class `Flash`.
const ISENTROPE_NOISE: f64 = 1e-13;

/// The decades of p below the model's largest pressure that HS and SU search.
const SX_DECADES: u16 = 30;

/// The bisections HS and SU take towards the domain's edge along the isentrope: from a decade of p, 2.3 in ln p, to its
/// last bit (2.3·2⁻⁶⁰ ≈ 2e-18).
const SX_STEPS: u16 = 60;

/// The most times HS and SU split their bracket around a refusal inside it.
const SX_SPLITS: u16 = 8;

/// HS and SU (PLAN.md M7.8; map 03 §3.1, §8, §9 `flash::hs`). Along the isentrope h and u rise with p, through the dome
/// as on either side of it: dh = v·dp, and du = −p·dv with v falling as p rises. So the state is the root in ln p of
/// x(PS(p, s)) − x, each iterate a PS flash ([`px`], which holds the dome and the domain), bracketed by decades of p
/// from the model's largest pressure (see the code), then TOMS 748 in ln p (`Strategy::Isentrope`). CoolProp's HS
/// screens the dome on 40 points of its caloric curves and runs a cascade of homotopies in (T, ln ρ) from four anchors
/// (map 03 §3.1); the isentrope needs neither. The state is PS's at the root, with PS's acceptance (dp/dρ > 0 and c_v >
/// 0 where no phase is imposed, map 03 §8's agent notes) and domain checks, and x reproduced. An x above every state
/// the isentrope holds is PS's refusal above them, or `AboveMaxPressure` at pmax; one below every state, PS's refusal
/// below them (the domain's lowest temperature), or `InvalidInput`. Errors name this pair ([`named`]).
fn sx(fluid: &PureFluid, s: f64, x: f64, var: Caloric, opts: &FlashOptions) -> Result<State, Error> {
    let p_max = fluid.limits().p_max();
    // An imposed phase holds along the whole isentrope. An imposed `TwoPhase` keeps the search in the dome, where x(p)
    // is well conditioned up to its edges; on a liquid's side u moves little with p, (∂u/∂p)_s = (p/ρ²)(∂ρ/∂p)_s, so a
    // root there holds p only to about 1e-6, and the dome's edge at that p misses s (Water's saturated liquid at 400
    // K).
    let along = opts.with_domain(DomainPolicy::Extrapolate);
    // While the bracket is sought, a residual within the nested solve's noise ([`ISENTROPE_NOISE`]) is the root: where
    // the isentrope leaves the domain just past it (a state at pmax, or at the triple point's temperature), no state
    // beyond it brackets it. TOMS 748 inside the bracket reads the raw residual.
    let r = fluid.eos().gas_constant();
    // p from ln p, never above pmax: exp(ln pmax) can round a few ulp over it (CO₂'s 800 MPa), which PS refuses.
    let p_of = |u: f64| math::exp(u).min(p_max);
    let raw = |u: f64| px(fluid, p_of(u), s, Caloric::S, &along).map(|state| (var.of(&state) - x, state.t()));
    let residual = |u: f64| {
        let (at, t) = raw(u)?;
        Ok(if at.abs() <= ISENTROPE_NOISE * var.scale(r, t, x) { 0.0 } else { at })
    };
    // Down from pmax a decade at a time, over p where PS refuses, to the first p where x(p) is at or below x; the last
    // p above x before it closes the bracket unless PS refused between them. Otherwise bisection in ln p between the
    // bracket's known end and the nearest refusal beside it finds its other end: the isentrope can leave the domain and
    // come back (at the triple point's temperature water's s(p) turns at 38.6 MPa), or end in it (under the triple
    // point's pressure), and a root at such an edge (a state at the triple point's temperature) is reached there, its
    // residual within the noise.
    let mut u = math::ln(p_max);
    let (mut above, mut below, mut failed) = (None, None, None);
    let (mut refused_over_below, mut refused_under_above) = (None, None);
    for _ in 0..SX_DECADES {
        match residual(u) {
            Ok(at) if at > 0.0 => (above, refused_over_below, refused_under_above) = (Some((u, at)), None, None),
            Ok(at) => {
                below = Some((u, at));
                break;
            }
            Err(e) => {
                failed.get_or_insert(e);
                refused_over_below = Some(u);
                refused_under_above = refused_under_above.or(above.map(|_| u));
            }
        }
        u -= core::f64::consts::LN_10;
    }
    let invalid = || Error::InvalidInput { quantity: var.name(), value: x };
    // Bisection between a known end and a refusal: the first p whose residual is on `side` (above x, or at or below
    // it).
    let bisect = |(mut held, mut at_held): (f64, f64), mut refused: f64, side: bool| {
        for _ in 0..SX_STEPS {
            if at_held == 0.0 {
                break;
            }
            let next = 0.5 * (held + refused);
            match residual(next) {
                Ok(at) if if side { at > 0.0 } else { at <= 0.0 } => {
                    return Ok(((held, at_held), (next, at)));
                }
                Ok(at) => (held, at_held) = (next, at),
                Err(_) => refused = next,
            }
        }
        if at_held == 0.0 { Ok(((held, at_held), (held, at_held))) } else { Err(()) }
    };
    let ((a, at_a), (b, at_b)) = match (above, below) {
        (Some(above), Some(below)) if refused_over_below.is_none() => (below, above),
        (_, Some(below)) if below.1 == 0.0 => (below, below),
        (_, Some(below)) => match refused_over_below {
            Some(refused) => bisect(below, refused, true).map_err(|()| failed.unwrap_or_else(invalid))?,
            None => return Err(DomainError::AboveMaxPressure { p: p_max, p_max }.into()),
        },
        (Some(above), None) => match refused_under_above {
            Some(refused) => {
                bisect(above, refused, false).map(|(b, a)| (a, b)).map_err(|()| failed.unwrap_or_else(invalid))?
            }
            None => return Err(failed.unwrap_or_else(invalid)),
        },
        (None, None) => return Err(failed.unwrap_or_else(invalid)),
    };
    // TOMS 748 in the bracket. A refusal inside it (the isentrope dips out of the domain between its ends: Hydrogen's
    // under its triple point's temperature near 20 K and 470 MPa) splits it: bisection from its upper end towards
    // the refusal finds a p below x, or else from its lower end one above x, and TOMS 748 runs again on that part.
    let (mut a, mut at_a, mut b, mut at_b) = (a, at_a, b, at_b);
    let mut solved = None;
    for _ in 0..SX_SPLITS {
        let mut refused = None;
        let ended = |u: f64| {
            if u == a {
                at_a
            } else if u == b {
                at_b
            } else {
                raw(u).map_or_else(
                    |e| {
                        refused.get_or_insert((u, e));
                        f64::NAN
                    },
                    |(at, _)| at,
                )
            }
        };
        let root = toms748(ended, a, b, Tol::Absolute(T_TOL), T_ITER);
        let Some((at, e)) = refused else {
            solved = Some(root);
            break;
        };
        ((a, at_a), (b, at_b)) = match bisect((b, at_b), at, false) {
            Ok(((above, at_above), below)) => (below, (above, at_above)),
            Err(()) => bisect((a, at_a), at, true).map_err(|()| e)?,
        };
    }
    let Some(root) = solved else { return Err(Error::NoConvergence { strategy: Strategy::Isentrope, iterations: 0 }) };
    let root = root.converged(Strategy::Isentrope)?;
    let state = px(fluid, p_of(root.x), s, Caloric::S, opts)?;
    if !reproduces_x(var, r, state.t(), x, var.of(&state)) {
        return Err(Error::NoConvergence { strategy: Strategy::Isentrope, iterations: root.iterations });
    }
    Ok(state.with_path(SolvePath { strategy: Strategy::Isentrope, iterations: root.iterations }))
}

/// The saturation at p that splits a P+X isobar, and the strategy its two-phase states take: an exact curve's
/// (`Superancillary`), a stale curve's polished by the VLE at p (`Vle`), a pseudo-pure fluid's ancillaries
/// (`Ancillary`), or without a curve the generic VLE's ([`generic_sat_p`]). `None` where p has no saturation: above
/// the curve's top, below its lowest pressure.
fn saturation_at_p(
    fluid: &PureFluid,
    p: f64,
    t_low: f64,
    opts: &FlashOptions,
) -> Result<Option<(SatPair, Strategy)>, Error> {
    let Some(curve) = fluid.saturation()? else {
        return generic_sat_p(fluid, p, t_low);
    };
    let sat = match curve_at_p(curve, p, opts) {
        Err(Error::Domain(DomainError::AboveMaxPressure { .. } | DomainError::BelowMinPressure { .. })) => {
            return Ok(None);
        }
        sat => sat?,
    };
    Ok(Some(match curve.accuracy() {
        SatAccuracy::Guess => {
            let seed = (sat.bubble.t, sat.bubble.rho, sat.dew.rho);
            (crate::vle::at_p(fluid.eos(), p, seed)?, Strategy::Vle)
        }
        SatAccuracy::Definition => (sat, Strategy::Ancillary),
        _ => (sat, Strategy::Superancillary),
    }))
}

/// The saturation at p of a model without a curve (PLAN.md M7.3): the generic VLE ([`generic_vle`]) at the T where
/// its pressure is p, found by TOMS 748 in T between T_low and the labelling critical temperature (where the pressure
/// is the critical one), then polished at p (`vle::at_p`). `None` without a critical point, at or above its pressure,
/// and at or below the saturation pressure at T_low, where every state of the domain is a gas.
fn generic_sat_p(fluid: &PureFluid, p: f64, t_low: f64) -> Result<Option<(SatPair, Strategy)>, Error> {
    let Some(crit @ (t_c, p_c, _)) = labelling_critical(fluid, None)? else { return Ok(None) };
    if p >= p_c || t_low >= t_c {
        return Ok(None);
    }
    let mut failure = None;
    let mut gap = |t: f64| {
        let pressure =
            if t >= t_c { Ok(p_c) } else { generic_vle(fluid, Some(crit), t, Pair::PQ).map(|sat| sat.bubble.p) };
        pressure.map_or_else(
            |e| {
                failure.get_or_insert(e);
                f64::NAN
            },
            |pressure| math::ln(pressure / p),
        )
    };
    let at_low = gap(t_low);
    let root = (at_low < 0.0).then(|| toms748(&mut gap, t_low, t_c, Tol::Relative(T_TOL), T_ITER));
    if let Some(e) = failure {
        return Err(e);
    }
    let Some(root) = root else { return Ok(None) };
    let t = root.converged(Strategy::Vle)?.x;
    let seed = generic_vle(fluid, Some(crit), t, Pair::PQ)?;
    Ok(Some((crate::vle::at_p(fluid.eos(), p, (t, seed.bubble.rho, seed.dew.rho))?, Strategy::Vle)))
}

/// A stretch of a P+X isobar that holds one single-phase branch: T from `lo` to `hi`, x known at a saturated end
/// (`ends`), and the side of the dome it lies on (`liquid`: the root PT takes at exactly psat). A span never reaches
/// into a pseudo-pure dome: its saturated end is the bubble or dew temperature, whose x it is given.
struct Span {
    lo: f64,
    hi: f64,
    ends: (Option<f64>, Option<f64>),
    liquid: bool,
}

/// The single-phase state of x on `span` of the isobar at p ([`Strategy::Isobar`]): the root in T of x(T) − x
/// ([`root_in_t`]), each iterate PT's density at (T, p) by its branch rule ([`pt_branch`]), seeded by the last
/// iterate's ([`Walk`]). The acceptance gate: p and x reproduced, and unless a phase is imposed the state stable; then
/// the domain, as PT's.
fn isobar(fluid: &PureFluid, p: f64, x: f64, var: Caloric, opts: &FlashOptions, span: Span) -> Result<State, Error> {
    let eos = fluid.eos();
    let (r, m, key) = (eos.gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    let (lo, hi, ends) = (span.lo, span.hi, (span.ends.0.map(|end| end - x), span.ends.1.map(|end| end - x)));
    let mut walk = Walk { fluid, p, opts, span, seed: f64::NAN };
    let residual = |t: f64| {
        let (_, rho) = walk.density(t)?;
        Ok(var.at(r, t, &total(fluid, t, rho)?) - x)
    };
    let tiny = AT_LOWEST_T * var.scale(r, lo, x);
    let (t, iterations) = root_in_t(fluid, (lo, hi), (ends, tiny), Strategy::Isobar, residual)?;
    let path = SolvePath { strategy: Strategy::Isobar, iterations };
    let (branch, rho) = walk.density(t)?;
    let b = total(fluid, t, rho)?;
    if !crate::density::reproduces(r, t, rho, p, &b) {
        return Err(Error::NoConvergence { strategy: Strategy::DensityNewton, iterations });
    }
    if !reproduces_x(var, r, t, x, var.at(r, t, &b)) {
        return Err(Error::NoConvergence { strategy: Strategy::Isobar, iterations });
    }
    if opts.phase_hint().is_none() {
        accept(r, t, &b)?;
    }
    within_domain(fluid, State::single(key, t, rho, r, m, branch.phase, &b, path)?, opts)
}

/// The root in T, with its iterations, of `residual`, x(T) − x along a path of the state space (an isobar, an
/// isochore), between `lo` and `hi`; a residual known at an end is given (`ends`). The root is bracketed from the top:
/// down from `hi` in steps of 20 % in T until x(T) falls below x, so the search never evaluates far below its root,
/// where a state can be costly or have no solution (a pseudo-pure fluid's VLE near its critical point), and of several
/// roots it takes the highest. x rises with T along such a path ((∂h/∂T)_p = c_p, (∂s/∂T)_p = c_p/T, (∂u/∂T)_ρ = c_v),
/// but an EOS can turn back far below its melting line, where its states are solid: Helium's s at 362 MPa is higher at
/// 2.2 K than at 168 K. Then TOMS 748 in the step that brackets it, to [`T_TOL`]. An x above x(`hi`) is refused as
/// `AboveMaxTemperature` at `hi`, one that no step reaches down to `lo` as `BelowMinTemperature` at `lo` (`lo` above
/// `hi` too: a liquid whose bubble temperature lies below the domain). A state that fails during the descent is stepped
/// over, not one between the bracket's ends; where no state above or below x holds, the first failure is the answer; a
/// failed TOMS 748 is `NoConvergence` of `strategy`.
fn root_in_t(
    fluid: &PureFluid,
    (lo, hi): (f64, f64),
    (ends, tiny): ((Option<f64>, Option<f64>), f64),
    strategy: Strategy,
    mut residual: impl FnMut(f64) -> Result<f64, Error>,
) -> Result<(f64, u16), Error> {
    let colder = DomainError::BelowMinTemperature { t: lo, t_min: lo };
    if lo >= hi {
        return Err(colder.into());
    }
    // Down from `hi` in steps of 20 % in T to the first T where x(T) falls below x, the last T above it the bracket's
    // top. A state that fails on the way is stepped over (an imposed liquid beyond its superheat limit), but not one
    // between the two that bracket the root. A refusal right below a T above x is first approached by bisection: the
    // branch can end there with the root just above its end (an imposed gas, or a supercritical state on the gas's
    // branch, at 30 MPa has no state below Water's critical temperature).
    let (mut t, mut above, mut below, mut failed) = (hi, None, None, None);
    for _ in 0..T_ITER {
        let at = match (t == hi, t == lo) {
            (true, _) => ends.1.map_or_else(|| residual(t), Ok),
            // At the domain's low end a residual within `tiny` is the root: a state there, reproduced.
            (_, true) => ends.0.map_or_else(|| residual(t), Ok).map(|at| if at > tiny { at } else { at.min(0.0) }),
            _ => residual(t),
        };
        match at {
            Ok(at) if at > 0.0 => (above, failed) = (Some((t, at)), None),
            Ok(at) => {
                below = Some((t, at));
                break;
            }
            Err(e) => {
                if let Some((mut held, mut at_held)) = above {
                    let mut refused = t;
                    for _ in 0..T_HALVINGS {
                        let next = 0.5 * (held + refused);
                        match residual(next) {
                            Ok(at) if at > 0.0 => (held, at_held) = (next, at),
                            Ok(at) => {
                                below = Some((next, at));
                                break;
                            }
                            Err(_) => refused = next,
                        }
                    }
                    above = Some((held, at_held));
                    if below.is_some() {
                        break;
                    }
                }
                failed.get_or_insert(e);
            }
        }
        if t == lo {
            break;
        }
        t = (0.8 * t).max(lo);
    }
    let above_max = || DomainError::AboveMaxTemperature { t: hi, t_max: fluid.limits().t_max() }.into();
    let Some((b, at_b)) = above else { return Err(failed.unwrap_or_else(above_max)) };
    let Some((a, at_a)) = below else { return Err(failed.unwrap_or(colder.into())) };
    if let Some(e) = failed {
        return Err(e);
    }
    let mut failure = None;
    let ended = |t: f64| {
        if t == a {
            at_a
        } else if t == b {
            at_b
        } else {
            residual(t).unwrap_or_else(|e| {
                failure.get_or_insert(e);
                f64::NAN
            })
        }
    };
    let root = toms748(ended, a, b, Tol::Relative(T_TOL), T_ITER);
    if let Some(e) = failure {
        return Err(e);
    }
    let root = root.converged(strategy)?;
    Ok((root.x, root.iterations))
}

/// D+X: DH, DS and DU (PLAN.md M7.4; map 03 §3.1, §9 `flash::dx`). Along the isochore x(T) is DT's ([`isochore_x`]): a
/// single phase's from the EOS at (T, ρ), in the dome the lever rule's on the saturation at T (DT's phase rule,
/// [`region`]), so it is continuous through the dome's edge, where ρ is a saturated density. T is the root of x(T) − x
/// from T_low to 1.5·Tmax ([`root_in_t`], `Strategy::Isochore`), and the state DT's at it, with DT's acceptance and
/// domain checks and x reproduced. CoolProp splits the range at the saturation crossings and classifies each interval
/// (map 03 §3.1); a residual continuous through the dome needs no intervals. An imposed single phase holds along the
/// whole isochore; an imposed `TwoPhase` holds at the root, so a single-phase answer is refused there (a quality
/// outside [0, 1]). Errors name this pair ([`named`]).
fn dx(fluid: &PureFluid, rho: f64, x: f64, var: Caloric, opts: &FlashOptions) -> Result<State, Error> {
    let along = if opts.phase_hint() == Some(Phase::TwoPhase) { FlashOptions::new() } else { *opts };
    let residual = |t: f64| Ok(isochore_x(fluid, t, rho, var, &along)? - x);
    let (span, r) = (t_search(fluid), fluid.eos().gas_constant());
    let tiny = AT_LOWEST_T * var.scale(r, span.0, x);
    let (t, iterations) = root_in_t(fluid, span, ((None, None), tiny), Strategy::Isochore, residual)?;
    let state = dt(fluid, t, rho, opts)?;
    let r = fluid.eos().gas_constant();
    if !reproduces_x(var, r, t, x, var.of(&state)) {
        return Err(Error::NoConvergence { strategy: Strategy::Isochore, iterations });
    }
    Ok(state.with_path(SolvePath { strategy: Strategy::Isochore, iterations }))
}

/// DP (PLAN.md M7.6; map 03 §3.1, §9 `flash::dp`): T where DT's pressure at (T, ρ) is p ([`isochore_p`]), by the root
/// search in T along the isochore ([`root_in_t`], `Strategy::Isochore`), and the state DT's at it, with DT's checks and
/// p reproduced. In the dome DT's p is the saturation pressure at T; on either side it rises with T, (∂p/∂T)_ρ > 0, but
/// in water's liquid by its density maximum, where the search takes the highest root. An imposed phase is honoured, as
/// by every pair (ROT-070: CoolProp's DP ignores one, map 03 §6): an imposed single phase is the EOS's along the whole
/// isochore, an imposed `TwoPhase` holds at the root. Errors name this pair ([`named`]).
fn dp(fluid: &PureFluid, rho: f64, p: f64, opts: &FlashOptions) -> Result<State, Error> {
    if opts.domain() == DomainPolicy::Enforce {
        check_p(fluid, p)?;
    }
    let along = if opts.phase_hint() == Some(Phase::TwoPhase) { FlashOptions::new() } else { *opts };
    let residual = |t: f64| Ok(isochore_p(fluid, t, rho, &along)? - p);
    let (t, iterations) =
        root_in_t(fluid, t_search(fluid), ((None, None), AT_LOWEST_T * p), Strategy::Isochore, residual)?;
    let state = dt(fluid, t, rho, opts)?;
    if !reproduces_p(p, state.p()) {
        return Err(Error::NoConvergence { strategy: Strategy::Isochore, iterations });
    }
    Ok(state.with_path(SolvePath { strategy: Strategy::Isochore, iterations }))
}

/// p at (T, ρ) as DT's state has it: the EOS's for a single phase, the saturation pressure in the dome (DT's phase
/// rule); where a pseudo-pure fluid's VLE fails just below its published Tc, the EOS's (see [`isochore_x`]).
fn isochore_p(fluid: &PureFluid, t: f64, rho: f64, opts: &FlashOptions) -> Result<f64, Error> {
    let r = fluid.eos().gas_constant();
    let p = relations::pressure(r, t, rho, &total(fluid, t, rho)?);
    let pseudo_pure = || fluid.saturation().is_ok_and(|c| c.is_some_and(|c| c.accuracy() == SatAccuracy::Definition));
    Ok(match region(fluid, t, rho, p, opts) {
        Err(Error::NoConvergence { strategy: Strategy::Vle, .. }) if pseudo_pure() => p,
        region => match region? {
            Region::Single(_) => p,
            Region::Dome(sat, _) => sat.bubble.p,
        },
    })
}

/// x at (T, ρ) as DT's state has it: the EOS's for a single phase; in the dome (DT's phase rule) the lever rule's on
/// molar volume between the saturated sides at T. Where a pseudo-pure fluid's VLE fails just below its published
/// critical temperature, above its EOS's own (Air at 13 119 mol/m³ from 131 K to 132.53 K; R407C's EOS critical point
/// is 356.60 K, the published one 359.345 K), the EOS has no dome and its single phase's x is the one the search
/// follows, continuous with the lever rule's where that dome closes; DT itself refuses those states.
fn isochore_x(fluid: &PureFluid, t: f64, rho: f64, var: Caloric, opts: &FlashOptions) -> Result<f64, Error> {
    let r = fluid.eos().gas_constant();
    let b = total(fluid, t, rho)?;
    let pseudo_pure = || fluid.saturation().is_ok_and(|c| c.is_some_and(|c| c.accuracy() == SatAccuracy::Definition));
    Ok(match region(fluid, t, rho, relations::pressure(r, t, rho, &b), opts) {
        Err(Error::NoConvergence { strategy: Strategy::Vle, .. }) if pseudo_pure() => var.at(r, t, &b),
        region => match region? {
            Region::Single(_) => var.at(r, t, &b),
            Region::Dome(sat, _) => {
                let (rl, rv) = (sat.bubble.rho, sat.dew.rho);
                let q = (1.0 / rho - 1.0 / rl) / (1.0 / rv - 1.0 / rl);
                let (xl, xv) = (var.at(r, t, &total(fluid, t, rl)?), var.at(r, t, &total(fluid, t, rv)?));
                xl + q * (xv - xl)
            }
        },
    })
}

/// The decades of ρ below the model's largest density that T+X searches.
const TX_DECADES: u16 = 15;

/// The grid nodes per decade of ρ on which T+X finds the turns of x along an isotherm's single phase.
const TX_NODES: u16 = 8;

/// The fewest grid cells on a stretch of single phase: the liquid's is short in ln ρ and turns near ρ′.
const TX_CELLS: u16 = 16;

/// T+X: HT, ST and TU (PLAN.md M7.5; map 03 §6 rows 3, 10; §9 `flash::tx`). Along the isotherm, on an axis in ln ρ from
/// 1e-15 of the model's largest density to it, x(ρ) is DT's. Below the labelling critical temperature it has three
/// stretches: the vapour up to ρ″, the dome, where x is the lever rule's, linear in v, so its one root is closed-form,
/// and the liquid from ρ′; above it one. On a single phase x need not be monotonic: in the compressed liquid h and u
/// rise again from the saturated liquid's ((∂h/∂ρ)_T changes sign at the Joule-Thomson inversion), and by water's
/// density maximum its s rises with ρ just above ρ′. So each such stretch is cut at the turns of x, where (∂x/∂ln ρ)_T
/// changes sign between the nodes of a grid of [`TX_NODES`] per decade (at least [`TX_CELLS`] cells), and each
/// monotonic piece holds at most one root, found by TOMS 748 in ln ρ ([`Strategy::Isotherm`]). Each root's state is
/// DT's, with DT's acceptance and domain checks. One root is the answer. Several are `Ambiguous` under `Strict` and
/// `Stable` (each is a state at its own pressure; CoolProp silently returns the two-phase one, map 03 §6, ROT-071), and
/// `Nearest` takes the one nearest its density. No root is `InvalidInput`: x is beyond every state at T. An imposed
/// single phase holds along the whole isotherm, an imposed `TwoPhase` at the root, as in [`dx`]. Errors name this pair
/// ([`named`]).
fn tx(fluid: &PureFluid, t: f64, x: f64, var: Caloric, opts: &FlashOptions) -> Result<State, Error> {
    if opts.domain() == DomainPolicy::Enforce {
        check_t(fluid, t)?;
    }
    let (eos, r) = (fluid.eos(), fluid.eos().gas_constant());
    let top = math::ln(eos.rho_max(t));
    let bottom = top - f64::from(TX_DECADES) * core::f64::consts::LN_10;
    let curve = fluid.saturation()?;
    let crit = labelling_critical(fluid, curve)?;
    let single_hint = opts.phase_hint().is_some_and(|h| h != Phase::TwoPhase);
    let pseudo_pure = curve.is_some_and(|c| c.accuracy() == SatAccuracy::Definition);
    let dome = match crit {
        Some((t_c, ..)) if t >= t_c => None,
        _ if single_hint => None,
        // A pseudo-pure fluid's VLE fails just below its published Tc, above its EOS's own: no dome (`isochore_x`).
        _ => match saturation_at_t(fluid, curve, crit, t) {
            Err(Error::NoConvergence { strategy: Strategy::Vle, .. }) if pseudo_pure => None,
            sat => Some(sat?.0),
        },
    };
    let mut roots: Vec<(f64, u16)> = Vec::new();
    let mut stretches: Vec<(f64, f64)> = Vec::new();
    match dome {
        Some(sat) => {
            let (vl, vv) = (1.0 / sat.bubble.rho, 1.0 / sat.dew.rho);
            let (xl, xv) =
                (var.at(r, t, &total(fluid, t, sat.bubble.rho)?), var.at(r, t, &total(fluid, t, sat.dew.rho)?));
            if (xl.min(xv)..=xl.max(xv)).contains(&x) {
                roots.push((-math::ln(vl + (x - xl) / (xv - xl) * (vv - vl)), 0));
            }
            stretches.push((bottom, math::ln(sat.dew.rho).max(bottom)));
            stretches.push((math::ln(sat.bubble.rho).min(top), top));
        }
        None => stretches.push((bottom, top)),
    }
    let point = |u: f64| -> Result<(f64, f64), Error> {
        let b = total(fluid, t, math::exp(u))?;
        Ok((var.at(r, t, &b) - x, var.slope_sign(&b)))
    };
    // A stretch is empty where the dome reaches the axis's end (from = to, never beyond it).
    for (from, to) in stretches.into_iter().filter(|(from, to)| from != to) {
        let (mut a, mut at_a) = (from, point(from)?);
        for b in tx_nodes(from, to).skip(1) {
            let at_b = point(b)?;
            roots.extend(cell_roots(&point, (a, at_a), (b, at_b))?);
            (a, at_a) = (b, at_b);
        }
    }
    roots.sort_by(|a, b| a.0.total_cmp(&b.0));
    roots.dedup_by(|a, b| (a.0 - b.0).abs() <= T_TOL);
    let mut states = Vec::new();
    let mut refused = None;
    for (u, iterations) in roots {
        match dt(fluid, t, math::exp(u), opts) {
            Ok(state) => states.push(state.with_path(SolvePath { strategy: Strategy::Isotherm, iterations })),
            Err(e) => refused = refused.or(Some(e)),
        }
    }
    let densities: Vec<f64> = states.iter().map(|s| s.rho(Basis::Molar)).collect();
    let state = match (states.as_slice(), opts.roots()) {
        ([], _) => return Err(refused.unwrap_or(Error::InvalidInput { quantity: var.name(), value: x })),
        ([one], _) => *one,
        (several, RootPolicy::Nearest(rho)) => {
            let pick = nearest(rho, &densities);
            let found = several.iter().find(|s| Some(s.rho(Basis::Molar)) == pick);
            *found.ok_or(Error::Ambiguous { roots: Roots::new(&densities) })?
        }
        _ => return Err(Error::Ambiguous { roots: Roots::new(&densities) }),
    };
    if !reproduces_x(var, r, t, x, var.of(&state)) {
        return Err(Error::NoConvergence { strategy: Strategy::Isotherm, iterations: state.path().iterations });
    }
    Ok(state)
}

/// The nodes of T+X's grid over a stretch of ln ρ from `from` to `to`: [`TX_NODES`] per decade of ρ and at least
/// [`TX_CELLS`] cells, evenly spaced, both ends included.
fn tx_nodes(from: f64, to: f64) -> impl Iterator<Item = f64> {
    let decades = (to - from) / core::f64::consts::LN_10;
    let cells = (decades * f64::from(TX_NODES)).ceil().max(f64::from(TX_CELLS));
    let step = (to - from) / cells;
    (0..=cells as u32).map(move |k| if f64::from(k) >= cells { to } else { from + f64::from(k) * step })
}

/// The roots of x(ln ρ) − x in one grid cell of an isotherm's single phase, from its ends' residuals and slopes
/// (`point`): the cell is cut at a turn of x where the slope changes sign, and each monotonic piece holds at most one
/// root (TOMS 748 in ln ρ to [`T_TOL`]).
fn cell_roots(
    point: &impl Fn(f64) -> Result<(f64, f64), Error>,
    (a, at_a): (f64, (f64, f64)),
    (b, at_b): (f64, (f64, f64)),
) -> Result<Vec<(f64, u16)>, Error> {
    let mut pieces = vec![(a, at_a.0, b, at_b.0)];
    if at_a.1 * at_b.1 < 0.0 {
        let turn = toms748(|u| point(u).map_or(f64::NAN, |p| p.1), a, b, Tol::Absolute(T_TOL), T_ITER);
        let turn = turn.converged(Strategy::Isotherm)?.x;
        let at_turn = point(turn)?.0;
        pieces = vec![(a, at_a.0, turn, at_turn), (turn, at_turn, b, at_b.0)];
    }
    let mut roots = Vec::new();
    for (lo, at_lo, hi, at_hi) in pieces {
        if at_lo == 0.0 || at_hi == 0.0 || at_lo.is_sign_negative() != at_hi.is_sign_negative() {
            let root = toms748(|u| point(u).map_or(f64::NAN, |p| p.0), lo, hi, Tol::Absolute(T_TOL), T_ITER);
            let root = root.converged(Strategy::Isotherm)?;
            roots.push((root.x, root.iterations));
        }
    }
    Ok(roots)
}

/// An [`isobar`] solve between its iterates: the last density, the next iterate's seed.
struct Walk<'a> {
    fluid: &'a PureFluid,
    p: f64,
    opts: &'a FlashOptions,
    span: Span,
    seed: f64,
}

impl Walk<'_> {
    /// The branch and density at (T, p): PT's branch rule, on the span's side at exactly psat; seeded by the last
    /// density.
    fn density(&mut self, t: f64) -> Result<(Branch, f64), Error> {
        let (fluid, p, span) = (self.fluid, self.p, &self.span);
        let branch = match pt_branch(fluid, p, t, self.opts)? {
            Where::One(branch) => branch,
            Where::Both { liquid, gas } => {
                if span.liquid {
                    liquid
                } else {
                    gas
                }
            }
        };
        let seed = if self.seed.is_nan() { branch.seed } else { self.seed };
        let (rho, _) = solve_branch(fluid, p, t, &branch, seed)?;
        self.seed = rho;
        Ok((branch, rho))
    }
}

/// What a Q-pair fixes beside the quality: h, s or, for DQ, the molar volume v = 1/ρ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QVar {
    H,
    S,
    V,
}

impl QVar {
    /// Its value on a state (molar).
    fn of(self, state: &State) -> f64 {
        match self {
            QVar::H => state.h(Basis::Molar),
            QVar::S => state.s(Basis::Molar),
            QVar::V => 1.0 / state.rho(Basis::Molar),
        }
    }

    /// The input that gives `x`: h and s themselves, the density 1/v for DQ.
    fn input(self, x: f64) -> f64 {
        if self == QVar::V { 1.0 / x } else { x }
    }

    /// The lever rule's value at quality q from the caloric curves' h′, h″, s′, s″, u′, u″; `None` for v.
    fn lever(self, q: f64, sides: [f64; 6]) -> Option<f64> {
        let [hl, hv, sl, sv, ..] = sides;
        match self {
            QVar::H => Some((1.0 - q) * hl + q * hv),
            QVar::S => Some((1.0 - q) * sl + q * sv),
            QVar::V => None,
        }
    }
}

/// The grid cells along a saturation curve's fitted range on which a Q-pair brackets its roots.
const QX_CELLS: u16 = 200;

/// The times a Q-pair's grid halves its last cell towards the critical point, where x(T) goes like powers of Tc − T and
/// its turns crowd: down to 1e-9 of a cell.
const QX_TOP_HALVINGS: usize = 30;

/// The golden section's ratio, (√5 − 1)/2.
const GOLDEN: f64 = 0.618_033_988_749_894_9;

/// The nodes of a Q-pair's grid over a curve's fitted range [`lo`, `hi`]: [`QX_CELLS`] even cells, the last halved
/// [`QX_TOP_HALVINGS`] times towards `hi`, both ends included.
fn qx_nodes(lo: f64, hi: f64) -> impl Iterator<Item = f64> {
    let cell = (hi - lo) / f64::from(QX_CELLS);
    let top = core::iter::successors(Some(0.5 * cell), |w| Some(0.5 * w)).take(QX_TOP_HALVINGS).map(move |w| hi - w);
    (0..QX_CELLS).map(move |k| lo + cell * f64::from(k)).chain(top).chain([hi])
}

/// Golden-section steps that locate a turn of a Q-pair's x(T) inside two grid cells: 0.618^60 of their width.
const TURN_STEPS: u16 = 60;

/// QS, HQ and DQ (PLAN.md M7.7; map 03 §3.1, §6, §9 `flash::qx`): every T on the saturation curve where the lever rule
/// at quality q gives x (h, s, or v = 1/ρ), each root's state QT's. x(T) is not monotonic: Water's h″ peaks near 507 K,
/// R1234ze(E)'s s″ turns twice, and Water's ρ′ peaks at 277 K. So x(T) is scanned on a grid of [`QX_CELLS`] cells over
/// the curve's fitted range, up to the critical point, its last cell halved [`QX_TOP_HALVINGS`] times towards it
/// (R1234yf's s and h at Q = ½ turn within 0.74 K of it, its cell), from the cheapest exact-enough source: an exact
/// curve's caloric curves for h and s (M5.2a; CoolProp builds them at first use), its densities for v, else QT's own
/// state. A turn between grid nodes is located by golden-section search, so two roots near it are both bracketed; each
/// bracket's root is found by TOMS 748 and, where the scan read the caloric curves, polished on the EOS
/// (`Strategy::Saturation`). Every Q in [0, 1] is accepted for every pair (CoolProp's HQ accepts only Q = 1 and its QS
/// no fractional Q; its DQ brackets T only to Tc − 0.1 K). One root is the answer; several are `Ambiguous` with their
/// temperatures under `Strict` and `Stable` (no T at a quality is more stable than another), and `Nearest(T)` takes one
/// (ROT-071; CoolProp throws `MultipleSolutionsError`). No root is `InvalidInput`. A fluid without a curve has no
/// Q-pair (`Unsupported`); a pseudo-pure fluid's has only Q = 0 and 1, as its QT.
fn qx(fluid: &PureFluid, q: f64, x: f64, var: QVar, pair: Pair, opts: &FlashOptions) -> Result<State, Error> {
    Quality::new(q)?;
    let curve = curve_for(fluid, pair)?;
    let (lo, hi) = curve.t_range();
    let along = opts.with_domain(DomainPolicy::Extrapolate);
    let exact = |t: f64| qt(fluid, q, t, &along).map(|state| var.of(&state) - x);
    let exact_curve = curve.accuracy() == SatAccuracy::Exact;
    let calorics = if exact_curve && var != QVar::V { fluid.calorics()? } else { None };
    let scan = |t: f64| -> Result<f64, Error> {
        if var == QVar::V && exact_curve {
            let sat = curve.at_t(t)?;
            return Ok((1.0 - q) / sat.bubble.rho + q / sat.dew.rho - x);
        }
        match calorics.and_then(|c| var.lever(q, c.at(t)?)) {
            Some(value) => Ok(value - x),
            None => exact(t),
        }
    };
    // The grid in runs of nodes where the saturated state exists (a pseudo-pure fluid's definition fails within 1 mK of
    // its curve's top: R410A), and each turn between a run's nodes where golden-section search puts it.
    let (mut runs, mut refused): (Vec<Vec<(f64, f64)>>, _) = (vec![Vec::new()], None);
    for t in qx_nodes(lo, hi) {
        match scan(t) {
            Ok(g) => runs.last_mut().into_iter().for_each(|run| run.push((t, g))),
            Err(e) => {
                refused.get_or_insert(e);
                runs.push(Vec::new());
            }
        }
    }
    let mut roots: Vec<(f64, u16)> = Vec::new();
    for grid in runs {
        let mut nodes = grid.clone();
        for triple in grid.windows(3) {
            if let [(t0, g0), (_, g1), (t2, g2)] = triple
                && (g1 > g0) != (g2 > g1)
            {
                // A turn the search cannot reach (x refused inside the cells) is left out, as a refused node.
                match turn_of(&scan, *t0, *t2, g1 > g0) {
                    Ok(node) => nodes.push(node),
                    Err(e) => _ = refused.get_or_insert(e),
                }
            }
        }
        nodes.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in nodes.windows(2) {
            let [(a, ga), (b, gb)] = pair else { continue };
            if *ga != 0.0 && *gb != 0.0 && ga.is_sign_negative() == gb.is_sign_negative() {
                continue;
            }
            let found = toms748(|t| scan(t).unwrap_or(f64::NAN), *a, *b, Tol::Relative(T_TOL), T_ITER);
            let found = found.converged(Strategy::Saturation)?;
            let t = if calorics.is_some() { polish(&exact, found.x)? } else { found.x };
            // A root on a node (or a turn) closes the brackets either side of it; each returns that node exactly.
            if !roots.iter().any(|(r, _)| r.to_bits() == t.to_bits()) {
                roots.push((t, found.iterations));
            }
        }
    }
    roots.sort_by(|a, b| a.0.total_cmp(&b.0));
    let temperatures: Vec<f64> = roots.iter().map(|r| r.0).collect();
    let (t, iterations) = match (roots.as_slice(), opts.roots()) {
        ([], _) => return Err(refused.unwrap_or(Error::InvalidInput { quantity: var_name(var), value: var.input(x) })),
        ([one], _) => *one,
        (_, RootPolicy::Nearest(target)) => {
            let pick = nearest(target, &temperatures);
            *roots.iter().find(|r| Some(r.0) == pick).ok_or(Error::Ambiguous { roots: Roots::new(&temperatures) })?
        }
        _ => return Err(Error::Ambiguous { roots: Roots::new(&temperatures) }),
    };
    Ok(qt(fluid, q, t, opts)?.with_path(SolvePath { strategy: Strategy::Saturation, iterations }))
}

/// The name a Q-pair's other input goes by.
fn var_name(var: QVar) -> &'static str {
    match var {
        QVar::H => "H",
        QVar::S => "S",
        QVar::V => "D",
    }
}

/// The turn of `g` between `a` and `b` (a maximum if `max`, else a minimum) by golden-section search, with g there.
fn turn_of(g: &impl Fn(f64) -> Result<f64, Error>, a: f64, b: f64, max: bool) -> Result<(f64, f64), Error> {
    // A minimum of f: g itself, or −g for a maximum.
    let f = |t: f64| g(t).map(|v| if max { -v } else { v });
    let r = GOLDEN;
    let (mut a, mut b) = (a, b);
    let (mut c, mut d) = (b - r * (b - a), a + r * (b - a));
    let (mut gc, mut gd) = (f(c)?, f(d)?);
    for _ in 0..TURN_STEPS {
        if gc < gd {
            (b, d, gd) = (d, c, gc);
            c = b - r * (b - a);
            gc = f(c)?;
        } else {
            (a, c, gc) = (c, d, gd);
            d = a + r * (b - a);
            gd = f(d)?;
        }
    }
    let t = 0.5 * (a + b);
    Ok((t, g(t)?))
}

/// The brackets [`polish`] tries, 1e-9 to 1e-4 of T, each ten times the last.
const POLISH_WIDENINGS: u16 = 6;

/// A root `t` of the caloric curves' x(T) polished on the EOS's (`exact`): TOMS 748 in a bracket around it, widened
/// tenfold from 1e-9 of t until it holds a sign change; the curves' root where none does by 1e-4.
fn polish(exact: &impl Fn(f64) -> Result<f64, Error>, t: f64) -> Result<f64, Error> {
    let mut width = 1e-9;
    for _ in 0..POLISH_WIDENINGS {
        let (a, b) = (t * (1.0 - width), t * (1.0 + width));
        let (ga, gb) = (exact(a)?, exact(b)?);
        if ga == 0.0 || gb == 0.0 || ga.is_sign_negative() != gb.is_sign_negative() {
            let root = toms748(|u| exact(u).unwrap_or(f64::NAN), a, b, Tol::Relative(T_TOL), T_ITER);
            return Ok(root.converged(Strategy::Saturation)?.x);
        }
        width *= 10.0;
    }
    Ok(t)
}

/// The saturation curve for the Q pair `pair`; a fluid without one has no Q pair.
fn curve_for(fluid: &PureFluid, pair: Pair) -> Result<&dyn SaturationCurve, Error> {
    fluid.saturation()?.ok_or(Error::Unsupported { pair })
}

/// QT of a pseudo-pure fluid (D4; `FlashRoutines.cpp:952-975`): at Q = 0 the bubble point, its p the bubble pressure
/// ancillary's, and at Q = 1 the dew point; between them a quality has no meaning for these fluids, as in CoolProp.
fn pseudo_pure_qt(fluid: &PureFluid, sat: &SatPair, q: Quality) -> Result<State, Error> {
    let p = if q.get() == 0.0 {
        sat.bubble.p
    } else if q.get() == 1.0 {
        sat.dew.p
    } else {
        return Err(Error::InvalidInput { quantity: "Q", value: q.get() });
    };
    split_sides(fluid, sat, q, sat.bubble.t, p)
}

/// A pseudo-pure fluid's two-phase state at quality `q`, temperature `t` and pressure `p` from its definition's sides,
/// each phase at its side's T with the EOS's density at the side's (T, p), seeded by its density ancillary.
fn split_sides(fluid: &PureFluid, sat: &SatPair, q: Quality, t: f64, p: f64) -> Result<State, Error> {
    let (eos, m, key) = (fluid.eos(), fluid.info().molar_mass(), fluid.info().key());
    let (r, path) = (eos.gas_constant(), SolvePath { strategy: Strategy::Ancillary, iterations: 0 });
    let phase = |side: &SatSide, phase: Phase| {
        let (rho, _) = crate::density::at_t_p(eos, side.t, side.p, side.rho, phase == Phase::Liquid)?;
        State::single(key, side.t, rho, r, m, phase, &total(fluid, side.t, rho)?, path)
    };
    State::split(phase(&sat.bubble, Phase::Liquid)?, phase(&sat.dew, Phase::Gas)?, q, t, p, path)
}

/// QT: the saturation at T, then the state at quality q (ROT-004: QT and PQ share one `SaturationCurve` path). An
/// `Exact` curve's equilibrium is the answer (ROT-092: no VLE); a `Guess` curve seeds the pure VLE (ROT-088). The path
/// names the source (ROT-085). The curve refuses T outside its fitted range under either domain policy (D6).
fn qt(fluid: &PureFluid, q: f64, t: f64, opts: &FlashOptions) -> Result<State, Error> {
    let q = Quality::new(q)?;
    let curve = curve_for(fluid, Pair::QT)?;
    let sat = curve.at_t(t)?;
    let state = match curve.accuracy() {
        SatAccuracy::Guess => {
            two_phase(fluid, &crate::vle::at_t(fluid.eos(), t, (sat.bubble.rho, sat.dew.rho))?, q, Strategy::Vle)
        }
        SatAccuracy::Definition => pseudo_pure_qt(fluid, &sat, q),
        _ => two_phase(fluid, &sat, q, Strategy::Superancillary),
    }?;
    within_domain(fluid, state, opts)
}

/// PQ: the saturation at p, then the state at quality q, as [`qt`]; the curve refuses p above its top or below its
/// lowest pressure. Where p has several saturation temperatures (a curve whose p dips) the root policy chooses: the
/// nearest to `Nearest`'s T; `Strict` and `Stable` (no temperature at p is more stable than another) are `Ambiguous`.
fn pq(fluid: &PureFluid, p: f64, q: f64, opts: &FlashOptions) -> Result<State, Error> {
    let q = Quality::new(q)?;
    let curve = curve_for(fluid, Pair::PQ)?;
    let sat = curve_at_p(curve, p, opts)?;
    let state = match curve.accuracy() {
        SatAccuracy::Guess => {
            let seed = (sat.bubble.t, sat.bubble.rho, sat.dew.rho);
            two_phase(fluid, &crate::vle::at_p(fluid.eos(), p, seed)?, q, Strategy::Vle)
        }
        // A pseudo-pure fluid's bubble and dew temperatures differ at p; its T is linear in Q between them, as
        // CoolProp's (`FlashRoutines.cpp:1183-1199`).
        SatAccuracy::Definition => split_sides(fluid, &sat, q, q.get() * sat.dew.t + (1.0 - q.get()) * sat.bubble.t, p),
        _ => two_phase(fluid, &sat, q, Strategy::Superancillary),
    }?;
    within_domain(fluid, state, opts)
}

/// The saturation of `curve` at p; where p has several saturation temperatures (a curve whose p dips), the one
/// `RootPolicy::Nearest` names, else `Ambiguous`.
fn curve_at_p(curve: &dyn SaturationCurve, p: f64, opts: &FlashOptions) -> Result<SatPair, Error> {
    match (curve.at_p(p), opts.roots()) {
        (Err(Error::Ambiguous { roots }), RootPolicy::Nearest(x)) => {
            let sat = curve.at_t(nearest(x, roots.as_slice()).ok_or(Error::Ambiguous { roots })?)?;
            Ok(sat.at_pressure(p))
        }
        (sat, _) => sat,
    }
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
/// counted here): both phases from the EOS at their densities.
fn two_phase(fluid: &PureFluid, sat: &SatPair, q: Quality, strategy: Strategy) -> Result<State, Error> {
    let (r, m, key) = (fluid.eos().gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    let path = SolvePath { strategy, iterations: 0 };
    let (t, rl, rv) = (sat.bubble.t, sat.bubble.rho, sat.dew.rho);
    let liquid = State::single(key, t, rl, r, m, Phase::Liquid, &total(fluid, t, rl)?, path)?;
    let vapour = State::single(key, t, rv, r, m, Phase::Gas, &total(fluid, t, rv)?, path)?;
    State::split(liquid, vapour, q, t, sat.bubble.p, path)
}

/// The no-iteration path.
const DIRECT: SolvePath = SolvePath { strategy: Strategy::Direct, iterations: 0 };

/// The acceptance gate for a single-phase state the flash chose: (∂p/∂ρ)_T > 0 and cv > 0 (D6); an infinite cv (a
/// non-analytic critical point) passes.
fn accept(r: f64, t: f64, b: &Bundle) -> Result<(), Error> {
    let stable = relations::dp_drho_t(r, t, b) > 0.0 && relations::cv(r, b) > 0.0;
    if stable { Ok(()) } else { Err(DomainError::MechanicallyUnstable.into()) }
}

#[cfg(test)]
mod tests;
