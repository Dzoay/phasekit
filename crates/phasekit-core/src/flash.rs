//! L4 the Helmholtz flash (D6): a pure function of an immutable package, a native input and per-call
//! options. No global configuration, no sticky phase, no exception cascade (map 01 R12, map 03 §6, map 12
//! R3/R4). Each pair runs a fixed, ordered list of strategies returning `Result`; the winner is recorded in
//! `State::path`. DT (M5.3), QT and PQ (M6.8) and PT (M7.1) are implemented; the other pairs land at M7 behind
//! `IMPLEMENTED`.
#![deny(clippy::indexing_slicing)] // E12: no panicking index on the flash path

use crate::derivs::{Bundle, Order};
use crate::error::{DomainError, Error};
use crate::fluid::PureFluid;
use crate::input::{NativeInput, Pair};
use crate::model::ThermoModel;
use crate::prop::Prop;
use crate::relations;
use crate::saturation::{SatAccuracy, SatPair, SatSide, SaturationCurve};
use crate::state::{Phase, SolvePath, State, Strategy};
use crate::units::Quality;

/// How to choose among several roots of a non-unique pair (T+H in compressed liquid, Q-pairs with 2-3 roots;
/// map 03 §6). The default never guesses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum RootPolicy {
    /// Several roots → `Error::Ambiguous { roots }`.
    #[default]
    Strict,
    /// The root nearest to this value of the solved variable.
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
    /// Imposes a phase for this call only (never sticky). A single-phase label skips the phase rule;
    /// `TwoPhase` requires the dome and is refused (quality outside [0, 1]) where the state is not in it.
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
    match input.pair() {
        Pair::DT => dt(fluid, y, x, opts),
        Pair::PT => pt(fluid, x, y, opts),
        Pair::QT => qt(fluid, x, y, opts),
        Pair::PQ => pq(fluid, x, y, opts),
        pair => Err(Error::Unsupported { pair }),
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
/// ROT-096). Below Tc without a curve whose fitted range covers T the case is `Unsupported`, never a load error, a
/// guess or a fit evaluated outside its range (S-10, D6).
fn region(fluid: &PureFluid, t: f64, rho: f64, p: f64, opts: &FlashOptions) -> Result<Region, Error> {
    let hint = opts.phase_hint();
    if let Some(single) = hint.filter(|h| *h != Phase::TwoPhase) {
        return Ok(Region::Single(single));
    }
    let curve = fluid.saturation()?;
    let crit = labelling_critical(fluid, curve)?;
    if hint.is_none() {
        if let Some((t_c, p_c, rho_c)) = crit.filter(|c| t >= c.0) {
            let phase = if t == t_c && rho == rho_c {
                Phase::CriticalPoint
            } else if p > p_c {
                Phase::Supercritical
            } else {
                Phase::SupercriticalGas
            };
            return Ok(Region::Single(phase));
        }
    }
    let unsupported = Error::Unsupported { pair: Pair::DT };
    let Some(curve) = curve else { return Err(unsupported) };
    let (t_lo, t_hi) = curve.t_range();
    if !(t_lo..=t_hi).contains(&t) {
        return Err(unsupported); // M6: a pure-fluid VLE solve outside the fit
    }
    let mut sat = curve.at_t(t)?;
    let mut strategy = Strategy::Superancillary;
    // A liquid above the critical pressure is a supercritical liquid, as CoolProp labels it.
    let liquid = |crit: Option<(f64, f64, f64)>| match crit {
        Some((_, p_c, _)) if p > p_c => Phase::SupercriticalLiquid,
        _ => Phase::Liquid,
    };
    if curve.accuracy() == SatAccuracy::Definition && hint.is_none() {
        match pseudo_pure_bands(fluid, t, rho, p, &sat) {
            Some(Phase::Liquid) => return Ok(Region::Single(liquid(crit))),
            Some(phase) => return Ok(Region::Single(phase)),
            None => {}
        }
    }
    // A stale curve seeds the pure VLE (M6.6), and so does a pseudo-pure fluid's definition: inside its dome CoolProp
    // solves the pure-fluid VLE of the blend's EOS from the ancillaries' densities (D4, map 04 U4; M6.9).
    if curve.accuracy() != SatAccuracy::Exact {
        sat = crate::vle::at_t(fluid.eos(), t, (sat.bubble.rho, sat.dew.rho))?;
        strategy = Strategy::Vle;
    }
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
    let t_check = fluid.limits().check_t(t);
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
            // Lever rule on molar volume; an imposed TwoPhase outside the dome fails here (q ∉ [0, 1]).
            let (rl, rv) = (sat.bubble.rho, sat.dew.rho);
            let q = Quality::new((1.0 / rho - 1.0 / rl) / (1.0 / rv - 1.0 / rl))?;
            two_phase(fluid, &sat, q, strategy)
        }
    }?;
    let p_check = fluid.limits().check_p(state.p());
    if enforce {
        p_check?;
    }
    // Under `Extrapolate`, a state outside the domain is returned, never silently: it carries the flag.
    Ok(if t_check.is_ok() && p_check.is_ok() { state } else { state.mark_extrapolated() })
}

/// PT (PLAN.md M7.1; map 03 §3.1, §9 `flash::pt`): the domain checked, the branch of (p, T) ([`pt_branch`]), then the
/// density on it. A bracketed branch holds one root ([`crate::density::bracketed`]), so a guess (`with_guess`) only
/// seeds its Newton and moves no output beyond rounding (ROT-017, ROT-072); where the seed picks the root (a pseudo-pure
/// fluid's branch, an imposed phase) a guess is not used. The acceptance gate (D6): the state's own p reproduces the
/// input to the rounding of the terms it cancels from (ROT-074: CoolProp's PT reports p up to 1e-8 off it, and a liquid
/// at 1 Pa cannot hold p to 1e-9 in double precision), and, unless a phase is imposed, the state is mechanically
/// stable.
fn pt(fluid: &PureFluid, p: f64, t: f64, opts: &FlashOptions) -> Result<State, Error> {
    let limits = fluid.limits();
    let check = limits.check_t(t).and_then(|()| limits.check_p(p));
    if opts.domain() == DomainPolicy::Enforce {
        check?;
    }
    let eos = fluid.eos();
    let (r, m, key) = (eos.gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    let branch = pt_branch(fluid, p, t, opts)?;
    let (rho, path) = match branch.bracket {
        _ if branch.phase == Phase::CriticalPoint => (branch.seed, DIRECT),
        Some((lo, hi)) => {
            let seed = opts.guess().map_or(branch.seed, |(_, rho)| rho);
            let solved = crate::density::bracketed(eos, t, p, (lo, hi), seed);
            // A root past the model's largest density: its bracket grows (`density::liquid_above`).
            let grown = |e| if hi == eos.rho_max(t) { crate::density::liquid_above(eos, t, p, hi) } else { Err(e) };
            let (rho, iterations) = solved.or_else(grown)?;
            (rho, SolvePath { strategy: Strategy::DensityNewton, iterations })
        }
        None => {
            let (rho, iterations) = crate::density::at_t_p(eos, t, p, branch.seed, branch.liquid)?;
            (rho, SolvePath { strategy: Strategy::DensityNewton, iterations })
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

/// The branch of a PT state: its phase label, its density's bracket (`None`: solved from the seed alone), the seed, and
/// whether a restart begins at the liquid's far end.
struct Branch {
    phase: Phase,
    bracket: Option<(f64, f64)>,
    seed: f64,
    liquid: bool,
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
/// Below Tc without a curve whose fitted range covers T the case is `Unsupported`, as for DT.
fn pt_branch(fluid: &PureFluid, p: f64, t: f64, opts: &FlashOptions) -> Result<Branch, Error> {
    let eos = fluid.eos();
    let (rho_max, ideal) = (eos.rho_max(t), p / (eos.gas_constant() * t));
    let floor = 1e-3 * ideal.min(rho_max);
    let no_quality = Error::Undefined { prop: Prop::Q, phase: Phase::TwoPhase };
    let curve = fluid.saturation()?;
    let fitted = curve.filter(|c| (c.t_range().0..=c.t_range().1).contains(&t));
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
            Ok(Branch { phase: hint, bracket: None, seed, liquid })
        };
    }
    let crit = labelling_critical(fluid, curve)?;
    if let Some((t_c, p_c, rho_c)) = crit {
        if t == t_c && p == p_c {
            return Ok(Branch { phase: Phase::CriticalPoint, bracket: None, seed: rho_c, liquid: false });
        }
        if t >= t_c {
            let phase = if p > p_c { Phase::Supercritical } else { Phase::SupercriticalGas };
            return Ok(Branch { phase, bracket: Some((floor, rho_max)), seed: ideal, liquid: false });
        }
    }
    let curve = fitted.ok_or(Error::Unsupported { pair: Pair::PT })?;
    let mut sat = curve.at_t(t)?;
    let above_pc = crit.is_some_and(|(_, p_c, _)| p > p_c);
    let liquid = if above_pc { Phase::SupercriticalLiquid } else { Phase::Liquid };
    match curve.accuracy() {
        SatAccuracy::Definition if p > sat.bubble.p => {
            Ok(Branch { phase: liquid, bracket: None, seed: sat.bubble.rho, liquid: true })
        }
        SatAccuracy::Definition if p < sat.dew.p => {
            Ok(Branch { phase: Phase::Gas, bracket: None, seed: ideal, liquid: false })
        }
        SatAccuracy::Definition => Err(no_quality),
        accuracy => {
            if accuracy != SatAccuracy::Exact {
                sat = crate::vle::at_t(eos, t, (sat.bubble.rho, sat.dew.rho))?;
            }
            Ok(if above_pc || p >= sat.bubble.p {
                Branch { phase: liquid, bracket: Some((sat.bubble.rho, rho_max)), seed: sat.bubble.rho, liquid: true }
            } else {
                Branch { phase: Phase::Gas, bracket: Some((floor, sat.dew.rho)), seed: ideal, liquid: false }
            })
        }
    }
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
    let sat = match (curve.at_p(p), opts.roots()) {
        (Err(Error::Ambiguous { roots }), RootPolicy::Nearest(x)) => {
            let distance = |t: &f64| (t - x).abs();
            let nearest = roots.as_slice().iter().copied().min_by(|a, b| distance(a).total_cmp(&distance(b)));
            let sat = curve.at_t(nearest.ok_or(Error::Ambiguous { roots })?)?;
            sat.at_pressure(p)
        }
        (sat, _) => sat?,
    };
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

/// A saturation state against the model's limits (D6): its T and p checked; under `Enforce` a state outside them is
/// refused, under `Extrapolate` returned with the flag.
fn within_domain(fluid: &PureFluid, state: State, opts: &FlashOptions) -> Result<State, Error> {
    let limits = fluid.limits();
    match limits.check_t(state.t()).and_then(|()| limits.check_p(state.p())) {
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
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::batch::{self, BatchRequest, Status};
    use crate::data::FluidRecord;
    use crate::fluid::Fluid;
    use crate::input::Input;
    use crate::model::Limits;
    #[cfg(feature = "fluids-all")]
    use crate::prop::{DerivVar, Partial};
    use crate::saturation::{SatSide, SaturationCurve};
    use crate::units::{Basis, Density, Pressure, Temperature};

    /// A curve fitted over 200-380 K that fails the test if it is ever evaluated outside that range.
    #[derive(Debug)]
    struct Fit;

    impl SaturationCurve for Fit {
        fn accuracy(&self) -> SatAccuracy {
            SatAccuracy::Exact
        }
        fn t_range(&self) -> (f64, f64) {
            (200.0, 380.0)
        }
        fn at_t(&self, t: f64) -> Result<SatPair, Error> {
            assert!((200.0..=380.0).contains(&t), "fit evaluated outside its range at {t} K");
            let side = |rho| SatSide { t, p: 1e9, rho }; // the top sets the critical pressure: above every toy state
            Ok(SatPair { bubble: side(4_000.0), dew: side(50.0) })
        }
        fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
            Err(Error::Unsupported { pair: Pair::PQ })
        }
    }

    /// D6: below the triple point the default refuses; `Extrapolate` evaluates a metastable single-phase
    /// state and flags it (scalar and batch); a saturation fit is never evaluated outside its range.
    #[test]
    fn extrapolation_is_opt_in_flagged_and_never_extends_a_fit() {
        let mut record = FluidRecord::synthetic("X").unwrap();
        record.limits = Limits::new(169.0, 420.0, 100e6).unwrap().with_t_triple(175.0);
        let fluid = Fluid::new(Arc::new(record.builder().unwrap().saturation(Fit).build()));
        let dt = |t| Input::dt(Density::molar(5_000.0).unwrap(), Temperature::new(t).unwrap());
        let enforce = FlashOptions::new();
        let extrapolate = enforce.with_domain(DomainPolicy::Extrapolate);
        let liquid = |opts: FlashOptions| opts.with_phase(Phase::Liquid);

        let below_triple = DomainError::BelowMinTemperature { t: 172.0, t_min: 175.0 };
        assert_eq!(fluid.flash(dt(172.0), &liquid(enforce)), Err(below_triple.into()));
        assert!(fluid.flash(dt(172.0), &liquid(extrapolate)).unwrap().is_extrapolated());
        assert!(!fluid.flash(dt(300.0), &liquid(extrapolate)).unwrap().is_extrapolated());
        // Without a hint the phase rule needs the fit; outside 200-380 K it is not evaluated under either policy.
        for (t, opts) in [(172.0, extrapolate), (190.0, enforce), (190.0, extrapolate)] {
            assert_eq!(fluid.flash(dt(t), &opts), Err(Error::Unsupported { pair: Pair::DT }));
        }
        assert_eq!(fluid.flash(dt(300.0), &enforce).unwrap().phase(), Phase::Liquid);

        // The flag survives the batch driver as a per-cell status that is not a failure.
        let (x, y) = ([5_000.0; 2], [172.0, 300.0]);
        let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &[Prop::P]).with_flash(liquid(extrapolate));
        let (mut out, mut status) = ([0.0; 2], [Status::Ok; 2]);
        let summary = batch::evaluate(&fluid, &req, &mut out, &mut status).unwrap();
        assert_eq!((status, summary.failed_cells), ([Status::Extrapolated, Status::Ok], 0));

        // Real Water (PLAN.md M5.3): supercooled liquid at 260 K is refused by default, flagged under `Extrapolate`
        // with a liquid hint, and without a hint not evaluated at all, because the superancillary starts at 273.16 K.
        #[cfg(feature = "fluids-all")]
        {
            let water = embedded("Water");
            let input = Input::dt(Density::molar(55_000.0).unwrap(), Temperature::new(260.0).unwrap());
            let below = DomainError::BelowMinTemperature { t: 260.0, t_min: 273.16 };
            assert_eq!(water.flash(input, &enforce), Err(below.into()));
            let supercooled = water.flash(input, &liquid(extrapolate)).unwrap();
            assert!(supercooled.is_extrapolated() && supercooled.phase() == Phase::Liquid);
            assert_eq!(water.flash(input, &extrapolate), Err(Error::Unsupported { pair: Pair::DT }));
        }
    }

    /// `name` from a private registry over the embedded data (the process-wide one stays untouched: a registry test
    /// checks that nothing in it is loaded).
    #[cfg(feature = "fluids-all")]
    fn embedded(name: &str) -> Fluid {
        crate::Registry::from_embedded(crate::DataSet::Corrected).unwrap().get(name).unwrap().clone()
    }

    /// The decoded record of `name`, from a private registry.
    #[cfg(feature = "fluids-all")]
    fn record_of(name: &str) -> FluidRecord {
        crate::internal::record(&crate::Registry::from_embedded(crate::DataSet::Corrected).unwrap(), name).unwrap()
    }

    #[cfg(feature = "fluids-all")]
    fn dt(rho: f64, t: f64) -> Input {
        Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap())
    }

    #[cfg(feature = "fluids-all")]
    fn pt(p: f64, t: f64) -> Input {
        Input::pt(Pressure::new(p).unwrap(), Temperature::new(t).unwrap())
    }

    /// ROT-074 (map 03 §6): PT's state is built at the density its solve returned, so its own p is the input to the
    /// rounding of p's terms; CoolProp's PT of Nitrogen at 10 MPa and 200 K reports p 3.3e-9 off.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn pt_reproduces_its_input_pressure() {
        let state = embedded("Nitrogen").state(pt(10e6, 200.0)).unwrap();
        assert_eq!((state.phase(), state.path().strategy), (Phase::Supercritical, Strategy::DensityNewton));
        assert!((state.p() / 10e6 - 1.0).abs() < 1e-15, "{}", state.p());
    }

    /// ROT-072 (map 03 §6): a guess only seeds PT's bracketed solve. CoolProp's PT of Water at 10 MPa and 300 K from a
    /// guess of 0.9 ρ lands on dp/dρ = −2.95e5 < 0, ρ 14.2 % off; here 0.9 ρ, a vapour's density, twice ρ and a guess
    /// made at another temperature all give the stable liquid root to rounding.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn a_guess_is_only_a_seed() {
        let water = embedded("Water");
        let rho = water.state(pt(10e6, 300.0)).unwrap().rho(Basis::Molar);
        let dp_drho = Partial { of: DerivVar::P, wrt: DerivVar::Dmolar, at: DerivVar::T };
        for (t, guess) in [(300.0, 0.9 * rho), (300.0, 4.0), (300.0, 2.0 * rho), (500.0, rho)] {
            let state = water.flash(pt(10e6, 300.0), &FlashOptions::new().with_guess(t, guess)).unwrap();
            assert!((state.rho(Basis::Molar) / rho - 1.0).abs() < 1e-14, "guess {guess}: {state:?}");
            assert!(state.partial(dp_drho).unwrap() > 0.0 && state.phase() == Phase::Liquid, "{state:?}");
        }
    }

    /// ROT-017 (map 01 R6): CoolProp's global `USE_GUESSES_IN_PROPSSI` makes most outputs throw. Here a guess is a
    /// per-call seed, and every `Prop` reads the same with and without one, within `Flash` (a warm start is another
    /// path, never `Exact`; VERIFICATION.md §11), errors included (`Undefined` Q, `NoModel`): Water's liquid, vapour
    /// and supercritical states and Nitrogen's dense supercritical one, each from guesses at half and twice ρ. The
    /// partial derivatives are every (of, wrt, at) of the twelve variables.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn a_guess_changes_no_output() {
        use DerivVar::*;
        let vars = [T, P, Dmolar, Dmass, Hmolar, Hmass, Smolar, Smass, Umolar, Umass, Gmolar, Gmass];
        let mut props = Vec::new();
        for of in vars {
            for wrt in vars {
                props.extend(vars.map(|at| Prop::Partial(Partial { of, wrt, at })));
            }
        }
        props.extend([
            Prop::T,
            Prop::P,
            Prop::Q,
            Prop::Dmolar,
            Prop::Dmass,
            Prop::Hmolar,
            Prop::Hmass,
            Prop::Smolar,
            Prop::Smass,
            Prop::Umolar,
            Prop::Umass,
            Prop::Gmolar,
            Prop::Gmass,
            Prop::Helmholtzmolar,
            Prop::Helmholtzmass,
            Prop::Cvmolar,
            Prop::Cvmass,
            Prop::Cpmolar,
            Prop::Cpmass,
            Prop::SpeedOfSound,
            Prop::Z,
            Prop::Cp0molar,
            Prop::Cp0mass,
            Prop::HmolarResidual,
            Prop::SmolarResidual,
            Prop::GmolarResidual,
            Prop::Bvirial,
            Prop::Cvirial,
            Prop::DBvirialDT,
            Prop::DCvirialDT,
            Prop::MolarMass,
            Prop::Viscosity,
            Prop::Conductivity,
            Prop::SurfaceTension,
        ]);
        for (name, p, t) in
            [("Water", 10e6, 300.0), ("Water", 1e4, 400.0), ("Water", 30e6, 700.0), ("Nitrogen", 10e6, 200.0)]
        {
            let fluid = embedded(name);
            let plain = fluid.state(pt(p, t)).unwrap();
            for factor in [0.5, 2.0] {
                let opts = FlashOptions::new().with_guess(t, factor * plain.rho(Basis::Molar));
                let warm = fluid.flash(pt(p, t), &opts).unwrap();
                for &prop in &props {
                    match (fluid.prop(&plain, prop), fluid.prop(&warm, prop)) {
                        (Ok(a), Ok(b)) => {
                            assert!((a - b).abs() <= 1e-9 * a.abs().max(b.abs()), "{name} {prop:?}: {a} and {b}");
                        }
                        (a, b) => assert_eq!(a, b, "{name} {prop:?}"),
                    }
                }
            }
        }
    }

    /// ROT-078, DIV-0012 (map 12 §6.3, #3394): Water at 250 K, below its 273.16 K triple point, is refused; CoolProp
    /// accepts DT(55018.5 mol/m³, 250 K) and returns p = −5.93 Pa.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn dt_below_the_triple_point_is_refused() {
        let below = DomainError::BelowMinTemperature { t: 250.0, t_min: 273.16 };
        assert_eq!(embedded("Water").state(dt(55_018.5, 250.0)), Err(below.into()));
    }

    /// D6 (map 03 §6, map 01 R12): `Extrapolate` belongs to the call. The same handle refuses the same state on the
    /// next call with the default options, and a hint given once is not remembered.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn extrapolate_is_per_call_never_sticky() {
        let water = embedded("Water");
        let extrapolate = FlashOptions::new().with_phase(Phase::Liquid).with_domain(DomainPolicy::Extrapolate);
        let below = Err(DomainError::BelowMinTemperature { t: 265.0, t_min: 273.16 }.into());
        for _ in 0..2 {
            assert!(water.flash(dt(55_000.0, 265.0), &extrapolate).unwrap().is_extrapolated());
            assert_eq!(water.state(dt(55_000.0, 265.0)), below);
            assert_eq!(water.state(dt(55_400.0, 300.0)).unwrap().phase(), Phase::Liquid); // above ρ′ = 55,315
        }
    }

    /// User decision 4, map 04 U4 (PLAN.md M6.9): below its critical temperature a pseudo-pure fluid's phase rule is
    /// the pure VLE of its EOS, seeded by its density ancillaries, as CoolProp's (`HelmholtzEOSMixtureBackend.cpp:
    /// 2447-2507`): R410A at 280 K is gas, two-phase on that VLE's densities and pressure, or liquid; above Tc it is
    /// supercritical.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn pseudo_pure_subcritical_dt_follows_its_eos_vle() {
        let r410a = embedded("R410A");
        let phases = [100.0, 5_000.0, 16_000.0].map(|rho| r410a.state(dt(rho, 280.0)).map(|s| s.phase()));
        assert_eq!(phases, [Ok(Phase::Gas), Ok(Phase::TwoPhase), Ok(Phase::Liquid)]);
        let record = record_of("R410A");
        let definition = record.pseudo_pure.as_ref().unwrap();
        let seeds = (definition.rho_l.at(280.0), definition.rho_v.at(280.0));
        let vle = crate::vle::at_t(r410a.model().helmholtz().unwrap(), 280.0, seeds).unwrap();
        let state = r410a.state(dt(5_000.0, 280.0)).unwrap();
        assert_eq!((state.p(), state.path().strategy), (vle.dew.p, Strategy::Vle));
        assert_eq!(r410a.state(dt(5_000.0, 400.0)).unwrap().phase(), Phase::Supercritical);
    }

    /// ROT-020: the state names the strategy that produced it: `Direct` for a single phase, `Superancillary` in the
    /// dome, where the curve's equilibrium is the answer.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn solve_path_names_the_winning_strategy() {
        let water = embedded("Water");
        let strategy = |rho, t| water.state(dt(rho, t)).unwrap().path();
        let direct = SolvePath { strategy: Strategy::Direct, iterations: 0 };
        assert_eq!([strategy(10.0, 700.0), strategy(10.0, 400.0), strategy(55_400.0, 300.0)], [direct; 3]);
        assert_eq!(strategy(1_000.0, 400.0), SolvePath { strategy: Strategy::Superancillary, iterations: 0 });
    }

    /// PLAN.md M6.6 (E14; ROT-088, ROT-096): an EOS edit other than R or ρ_r (here Nitrogen's first power-term
    /// coefficient, times 1 + 1e-6) leaves its superancillary stale, a `Guess`. DT inside the dome then only seeds the
    /// pure VLE with it: the two-phase state's pressure and quality are the edited EOS's own saturation, bit for bit
    /// that of `vle::at_t` from the same seeds, and the stale curve's pressure is 1e-7 or more away.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn stale_curve_is_a_guess_polished_by_vle() {
        let mut record = record_of("Nitrogen");
        record.eos.power.first_mut().unwrap().n *= 1.0 + 1e-6;
        assert_eq!(record.superancillary_freshness(), Some(crate::data::SaFreshness::Stale));
        let stale = record.superancillary_curve().unwrap();
        assert_eq!(stale.accuracy(), SatAccuracy::Guess);
        let curve_of = record.clone();
        let pure = record.builder().unwrap().lazy_saturation(move || Ok(curve_of.superancillary_curve())).build();
        let t = 100.0;
        let guess = stale.at_t(t).unwrap();
        let vle = crate::vle::at_t(pure.eos(), t, (guess.bubble.rho, guess.dew.rho)).unwrap();
        let fluid = Fluid::new(Arc::new(pure));
        let rho = crate::num::math::sqrt(vle.bubble.rho * vle.dew.rho);
        let state = fluid.state(dt(rho, t)).unwrap();
        assert_eq!((state.phase(), state.p().to_bits()), (Phase::TwoPhase, vle.dew.p.to_bits()));
        assert_eq!(state.path().strategy, Strategy::Vle);
        let q = (1.0 / rho - 1.0 / vle.bubble.rho) / (1.0 / vle.dew.rho - 1.0 / vle.bubble.rho);
        assert_eq!(state.quality().map(f64::to_bits), Some(q.to_bits()));
        assert!((guess.bubble.p / vle.dew.p - 1.0).abs() > 1e-7, "the edit moved saturation: {guess:?} {vle:?}");
    }

    /// ROT-092 (map 03 §6): a DT state in the dome reads the exact curve, no VLE: its pressure and phase densities are
    /// the curve's, bit for bit. The pressure check uses that saturation pressure: at 30,000 mol/m³ and 400 K the
    /// single-phase EOS gives 2.2e11 Pa, far above pmax, yet the state is a valid two-phase one.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn dt_in_dome_reads_the_curve_not_vle() {
        let state = embedded("Water").state(dt(30_000.0, 400.0)).unwrap();
        let sat = record_of("Water").superancillary_curve().unwrap().at_t(400.0).unwrap();
        assert_eq!((state.phase(), state.p().to_bits()), (Phase::TwoPhase, sat.bubble.p.to_bits()));
        let q = (1.0 / 30_000.0 - 1.0 / sat.bubble.rho) / (1.0 / sat.dew.rho - 1.0 / sat.bubble.rho);
        assert_eq!(state.quality().map(f64::to_bits), Some(q.to_bits()));
    }

    /// A curve whose bubble and dew sides differ (a pseudo-pure fluid's, `Definition`) over [300 K, 600 K]: p′ 2.0 MPa
    /// and p″ 1.9 MPa, ρ′ 52 000 and ρ″ 80 mol/m³ at every T (near Water's own at 400 K: the VLE's seeds).
    #[cfg(feature = "fluids-all")]
    #[derive(Debug)]
    struct Blend;

    #[cfg(feature = "fluids-all")]
    impl SaturationCurve for Blend {
        fn accuracy(&self) -> SatAccuracy {
            SatAccuracy::Definition
        }
        fn t_range(&self) -> (f64, f64) {
            (300.0, 600.0)
        }
        fn at_t(&self, t: f64) -> Result<SatPair, Error> {
            Ok(SatPair { bubble: SatSide { t, p: 2.0e6, rho: 52_000.0 }, dew: SatSide { t, p: 1.9e6, rho: 80.0 } })
        }
        fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
            Err(Error::Unsupported { pair: Pair::PQ })
        }
    }

    /// D4 (map 04 U4; PLAN.md M6.9): a curve whose bubble and dew sides differ, a `Definition`, only seeds the EOS's
    /// pure VLE inside its dome: Water's EOS under such a curve at 400 K is two-phase on Water's own saturation there,
    /// path `Vle`, and gas below its dew density.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn a_definitions_dome_is_its_eos_vle() {
        let blend = Fluid::new(Arc::new(record_of("Water").builder().unwrap().saturation(Blend).build()));
        let state = blend.state(dt(1_000.0, 400.0)).unwrap();
        let water = embedded("Water").state(dt(1_000.0, 400.0)).unwrap();
        assert_eq!((state.phase(), state.path().strategy), (Phase::TwoPhase, Strategy::Vle));
        assert!((state.p() / water.p() - 1.0).abs() < 1e-12, "{} against {}", state.p(), water.p());
        assert_eq!(blend.state(dt(5.0, 400.0)).map(|s| s.phase()), Ok(Phase::Gas));
    }

    /// The model's critical point labels states (map 03 §3.3): R114's published point (418.83 K, 3.257 MPa) lies below
    /// its EOS's, the top of its superancillary (420.61 K, 3.352 MPa), so at 419.5 K the dome is still there: a state
    /// between its densities is two-phase, as CoolProp says with superancillaries on, not supercritical.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn phase_labels_use_the_models_critical_point() {
        let record = record_of("R114");
        let published = record.critical.unwrap();
        let curve = record.superancillary_curve().unwrap();
        let (t, t_c) = (419.5, curve.t_range().1);
        assert!(published.t < t && t < t_c, "{} < {t} < {t_c}", published.t);
        let sat = curve.at_t(t).unwrap();
        let state = embedded("R114").state(dt((sat.bubble.rho + sat.dew.rho) / 2.0, t)).unwrap();
        assert_eq!(state.phase(), Phase::TwoPhase);
        assert_eq!(embedded("R114").state(dt(sat.dew.rho / 2.0, t)).unwrap().phase(), Phase::Gas);
    }

    /// The critical point labels a state only at both of the model's coordinates, the top of Water's exact curve; off
    /// it, at T_c or on the critical isochore, the pressure decides between supercritical and supercritical gas.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn critical_point_needs_both_coordinates() {
        let curve = record_of("Water").superancillary_curve().unwrap();
        let t_c = curve.t_range().1;
        let rho_c = curve.at_t(t_c).unwrap().bubble.rho;
        let water = embedded("Water");
        let at = |rho: f64, t: f64| water.state(dt(rho, t)).map(|s| s.phase());
        assert_eq!(at(rho_c, t_c), Ok(Phase::CriticalPoint));
        assert_eq!(at(1.2 * rho_c, t_c), Ok(Phase::Supercritical));
        assert_eq!(at(0.8 * rho_c, t_c), Ok(Phase::SupercriticalGas));
        assert_eq!(at(rho_c, 1.01 * t_c), Ok(Phase::Supercritical));
    }

    /// An exact curve over [300 K, `t`] whose top, the model's critical point, is (`t`, `p`, `rho`), with liquid at
    /// `rho_l` and vapour at `rho_v` below it: it puts Water's critical point where a test needs it.
    #[cfg(feature = "fluids-all")]
    #[derive(Debug)]
    struct Top {
        t: f64,
        p: f64,
        rho: f64,
        rho_l: f64,
        rho_v: f64,
    }

    #[cfg(feature = "fluids-all")]
    impl SaturationCurve for Top {
        fn accuracy(&self) -> SatAccuracy {
            SatAccuracy::Exact
        }
        fn t_range(&self) -> (f64, f64) {
            (300.0, self.t)
        }
        fn at_t(&self, t: f64) -> Result<SatPair, Error> {
            let (bubble, dew) = if t == self.t { (self.rho, self.rho) } else { (self.rho_l, self.rho_v) };
            Ok(SatPair { bubble: SatSide { t, p: self.p, rho: bubble }, dew: SatSide { t, p: self.p, rho: dew } })
        }
        fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
            Err(Error::Unsupported { pair: Pair::PQ })
        }
    }

    /// Water's EOS under the curve `top`.
    #[cfg(feature = "fluids-all")]
    fn water_under(top: Top) -> Fluid {
        Fluid::new(Arc::new(record_of("Water").builder().unwrap().saturation(top).build()))
    }

    /// The model's critical point where its solve cannot run (map 03 §6): under an exact curve whose top is Water's
    /// reducing point, τ = δ = 1, where the non-analytic terms are singular, it is that top, with origin `Model`.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn an_unsolvable_critical_point_is_the_exact_curves_top() {
        let eos = record_of("Water").eos;
        let (t, rho) = (eos.t_reducing, eos.rho_reducing);
        let model = water_under(Top { t, p: 22.064e6, rho, rho_l: 20_000.0, rho_v: 10_000.0 }).model().critical_point();
        let origin = crate::model::CriticalOrigin::Model;
        assert_eq!(model, Some(crate::model::CriticalPoint { t, p: 22.064e6, rho, origin }));
    }

    /// Water's pressure at (T, ρ) with the phase imposed: no phase rule, no acceptance gate.
    #[cfg(feature = "fluids-all")]
    fn imposed(rho: f64, t: f64) -> State {
        embedded("Water").flash(dt(rho, t), &FlashOptions::new().with_phase(Phase::Gas)).unwrap()
    }

    /// D6: the acceptance gate runs on every single-phase state the phase rule chose but the critical point, where
    /// (∂p/∂ρ)_T = 0 by definition. Under a curve whose top is 600 K, at 610 K the rule says supercritical, yet inside
    /// Water's own spinodal there (∂p/∂ρ)_T < 0 and the state is refused; the curve's top, put inside the spinodal at
    /// 600 K, is the critical point and a state.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn acceptance_gate_skips_only_the_critical_point() {
        let dp_drho = |rho: f64, t: f64| {
            imposed(rho, t).partial(Partial { of: DerivVar::P, wrt: DerivVar::Dmolar, at: DerivVar::T }).unwrap()
        };
        let spinodal = |t: f64| (20..160).map(|k| 250.0 * f64::from(k)).find(|&rho| dp_drho(rho, t) < 0.0).unwrap();
        let (rho_600, rho_610) = (spinodal(600.0), spinodal(610.0));
        let top = Top { t: 600.0, p: imposed(rho_600, 600.0).p(), rho: rho_600, rho_l: 40_000.0, rho_v: 1_000.0 };
        let fluid = water_under(top);
        assert_eq!(fluid.state(dt(rho_600, 600.0)).map(|s| s.phase()), Ok(Phase::CriticalPoint));
        assert_eq!(fluid.state(dt(rho_610, 610.0)), Err(DomainError::MechanicallyUnstable.into()));
    }

    /// The pressure that splits supercritical from supercritical gas, and liquid from supercritical liquid, is the
    /// critical pressure itself: a state exactly at p_c is not above it. Water under curves whose top pressure is a
    /// chosen state's own.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn a_state_at_the_critical_pressure_is_not_above_it() {
        let top = |p| Top { t: 650.0, p, rho: 17_000.0, rho_l: 50_000.0, rho_v: 100.0 };
        let (rho, t) = (5_000.0, 700.0);
        let fluid = water_under(top(imposed(rho, t).p()));
        assert_eq!(fluid.state(dt(rho, t)).map(|s| s.phase()), Ok(Phase::SupercriticalGas));
        assert_eq!(fluid.state(dt(1.01 * rho, t)).map(|s| s.phase()), Ok(Phase::Supercritical));
        let (rho, t) = (54_000.0, 400.0);
        let fluid = water_under(top(imposed(rho, t).p()));
        assert_eq!(fluid.state(dt(rho, t)).map(|s| s.phase()), Ok(Phase::Liquid));
        assert_eq!(fluid.state(dt(1.001 * rho, t)).map(|s| s.phase()), Ok(Phase::SupercriticalLiquid));
    }

    /// The acceptance gate (D6): a single-phase state needs (∂p/∂ρ)_T > 0 and cv > 0; an infinite cv passes.
    #[test]
    fn acceptance_gate_needs_mechanical_stability() {
        let stable = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let r = 8.314_462_618;
        assert_eq!(accept(r, 300.0, &stable), Ok(()));
        assert_eq!(accept(r, 300.0, &Bundle { a20: f64::NEG_INFINITY, ..stable }), Ok(()));
        let unstable = Err(DomainError::MechanicallyUnstable.into());
        assert_eq!(accept(r, 300.0, &Bundle { a02: -2.0, ..stable }), unstable); // (∂p/∂ρ)_T = 0
        assert_eq!(accept(r, 300.0, &Bundle { a20: 0.0, ..stable }), unstable); // cv = 0
    }

    /// QT at quality `q` and temperature `t`.
    fn q_t(q: f64, t: f64) -> Input {
        Input::qt(Quality::new(q).unwrap(), Temperature::new(t).unwrap())
    }

    /// PQ at pressure `p` and quality `q`.
    fn p_q(p: f64, q: f64) -> Input {
        Input::pq(Pressure::new(p).unwrap(), Quality::new(q).unwrap())
    }

    /// ROT-004, ROT-085 (map 03 §6 row 4; PLAN.md M6.8): QT and PQ read one `SaturationCurve` path, and `State::path`
    /// names the source. Water's exact superancillary is the answer: QT at 400 K has the curve's p there, bit for bit,
    /// and PQ at that pressure the curve's root at it. A stale curve (Nitrogen edited as in
    /// `stale_curve_is_a_guess_polished_by_vle`) only seeds the pure VLE: QT and PQ give `vle::at_t` and `vle::at_p` from
    /// its seeds, bit for bit. A pseudo-pure fluid (R410A) answers from its ancillary definition (M6.9).
    #[cfg(feature = "fluids-all")]
    #[test]
    fn q_pairs_report_their_saturation_source() {
        let exact = SolvePath { strategy: Strategy::Superancillary, iterations: 0 };
        let water = embedded("Water");
        let curve = record_of("Water").superancillary_curve().unwrap();
        let sat = curve.at_t(400.0).unwrap();
        let state = water.state(q_t(0.25, 400.0)).unwrap();
        assert_eq!((state.path(), state.phase(), state.quality()), (exact, Phase::TwoPhase, Some(0.25)));
        assert_eq!((state.t(), state.p()), (400.0, sat.bubble.p));
        let state = water.state(p_q(sat.bubble.p, 1.0)).unwrap();
        let root = curve.at_p(sat.bubble.p).unwrap();
        assert_eq!((state.path(), state.t(), state.p()), (exact, root.bubble.t, sat.bubble.p));
        assert!((root.bubble.t / 400.0 - 1.0).abs() < 1e-15, "{root:?}");

        let mut record = record_of("Nitrogen");
        record.eos.power.first_mut().unwrap().n *= 1.0 + 1e-6;
        let stale = record.superancillary_curve().unwrap();
        let curve_of = record.clone();
        let pure = record.builder().unwrap().lazy_saturation(move || Ok(curve_of.superancillary_curve())).build();
        let (t, guess) = (100.0, stale.at_t(100.0).unwrap());
        let vle = crate::vle::at_t(pure.eos(), t, (guess.bubble.rho, guess.dew.rho)).unwrap();
        let seed = stale.at_p(vle.dew.p).unwrap();
        let vle_p = crate::vle::at_p(pure.eos(), vle.dew.p, (seed.bubble.t, seed.bubble.rho, seed.dew.rho)).unwrap();
        let fluid = Fluid::new(Arc::new(pure));
        let polished = SolvePath { strategy: Strategy::Vle, iterations: 0 };
        let state = fluid.state(q_t(0.5, t)).unwrap();
        assert_eq!((state.path(), state.p().to_bits()), (polished, vle.dew.p.to_bits()));
        let state = fluid.state(p_q(vle.dew.p, 0.5)).unwrap();
        assert_eq!((state.path(), state.t().to_bits()), (polished, vle_p.bubble.t.to_bits()));
        assert!((vle_p.bubble.t / t - 1.0).abs() < 1e-12, "{vle_p:?}");

        let blend = embedded("R410A");
        let path = |input| blend.state(input).map(|s| s.path().strategy);
        assert_eq!((path(q_t(0.0, 280.0)), path(p_q(1e6, 0.5))), (Ok(Strategy::Ancillary), Ok(Strategy::Ancillary)));
    }

    /// A curve over [200 K, 380 K] with p = 10·T, ρ′ = 4000 and ρ″ = 50 mol/m³ at every T, of the given accuracy; its
    /// PQ at 2500 Pa has two roots, 260 K and 280 K.
    #[derive(Debug)]
    struct Line(SatAccuracy);

    impl SaturationCurve for Line {
        fn accuracy(&self) -> SatAccuracy {
            self.0
        }
        fn t_range(&self) -> (f64, f64) {
            (200.0, 380.0)
        }
        fn at_t(&self, t: f64) -> Result<SatPair, Error> {
            let side = |rho| SatSide { t, p: 10.0 * t, rho };
            Ok(SatPair { bubble: side(4_000.0), dew: side(50.0) })
        }
        fn at_p(&self, p: f64) -> Result<SatPair, Error> {
            if p == 2_500.0 {
                return Err(Error::Ambiguous { roots: crate::error::Roots::new(&[260.0, 280.0]) });
            }
            self.at_t(p / 10.0)
        }
    }

    /// D6 for the Q pairs (PLAN.md M6.8): a saturation state outside the model's limits (Tmin 250 K and pmax 3500 Pa
    /// here) is refused by default and flagged under `Extrapolate`; inside them it is a plain two-phase state at the
    /// given quality. Where p has two saturation temperatures, PQ takes the one `RootPolicy::Nearest` names and is
    /// otherwise `Ambiguous`. A fluid without a curve has no Q pair, and a `Definition` curve (a pseudo-pure fluid's,
    /// M6.9) no QT between Q = 0 and 1.
    #[test]
    fn q_pairs_check_the_domain_and_need_a_curve() {
        let fluid = |curve: Option<Line>| {
            let mut record = FluidRecord::synthetic("X").unwrap();
            record.limits = Limits::new(250.0, 420.0, 3_500.0).unwrap();
            let builder = record.builder().unwrap();
            Fluid::new(Arc::new(
                match curve {
                    Some(curve) => builder.saturation(curve),
                    None => builder,
                }
                .build(),
            ))
        };
        let line = fluid(Some(Line(SatAccuracy::Exact)));
        let state = line.state(q_t(0.5, 300.0)).unwrap();
        assert_eq!((state.p(), state.quality(), state.is_extrapolated()), (3_000.0, Some(0.5), false));
        assert_eq!(state.rho(Basis::Molar), 1.0 / (0.5 / 4_000.0 + 0.5 / 50.0));
        assert_eq!(line.state(p_q(3_000.0, 0.5)).map(|s| s.t()), Ok(300.0));
        let cold = DomainError::BelowMinTemperature { t: 220.0, t_min: 250.0 };
        let high = DomainError::AboveMaxPressure { p: 3_600.0, p_max: 3_500.0 };
        let outside =
            [(q_t(0.5, 220.0), cold), (p_q(2_200.0, 0.5), cold), (q_t(0.5, 360.0), high), (p_q(3_600.0, 0.5), high)];
        let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
        for (input, refused) in outside {
            assert_eq!(line.state(input), Err(refused.into()));
            assert!(line.flash(input, &extrapolate).unwrap().is_extrapolated());
        }
        let two = Error::Ambiguous { roots: crate::error::Roots::new(&[260.0, 280.0]) };
        assert_ne!(two, Error::Ambiguous { roots: crate::error::Roots::new(&[260.0]) });
        assert_eq!(line.state(p_q(2_500.0, 0.5)), Err(two.clone()));
        assert_eq!(line.flash(p_q(2_500.0, 0.5), &FlashOptions::new().with_roots(RootPolicy::Stable)), Err(two));
        let nearest = |t| line.flash(p_q(2_500.0, 0.5), &FlashOptions::new().with_roots(RootPolicy::Nearest(t)));
        assert_eq!(nearest(265.0).map(|s| (s.t(), s.p())), Ok((260.0, 2_500.0)));
        assert_eq!(nearest(275.0).map(|s| (s.t(), s.p())), Ok((280.0, 2_500.0)));
        let none = fluid(None);
        assert_eq!(none.state(q_t(0.5, 300.0)), Err(Error::Unsupported { pair: Pair::QT }));
        assert_eq!(none.state(p_q(3_000.0, 0.5)), Err(Error::Unsupported { pair: Pair::PQ }));
        let defined = fluid(Some(Line(SatAccuracy::Definition)));
        assert_eq!(defined.state(q_t(0.5, 300.0)), Err(Error::InvalidInput { quantity: "Q", value: 0.5 }));
    }

    /// The density bands (user decision PS3; `HelmholtzEOSMixtureBackend.cpp:2340-2380`) on a synthetic pair, ρ′ 1000 and
    /// ρ″ 100 mol/m³, p′ 1 MPa: gas below 0.95·ρ″ (not at it), liquid above 1.05·ρ′ (not at it); between, the strip within
    /// 0.9975 of either side asks for stability, and nothing else does; in the liquid strip, liquid when the ancillary
    /// quality is below 0.01, p above 1.05·p′ (not at it) and the state stable. Near the top (ρ″ 990) the liquid strip's
    /// quality exceeds 0.01.
    #[test]
    fn bands_are_coolprops() {
        let pair = |rho_v: f64| {
            let side = |p, rho| SatSide { t: 300.0, p, rho };
            SatPair { bubble: side(1e6, 1_000.0), dew: side(0.98e6, rho_v) }
        };
        let sat = pair(100.0);
        let mut asked = 0;
        let mut at = |rho: f64, p: f64, stable: bool| {
            band(rho, p, &sat, || {
                asked += 1;
                stable
            })
        };
        assert_eq!((at(0.95 * 100.0 - 1e-9, 1e6, true), at(0.95 * 100.0, 1e6, true)), (Some(Phase::Gas), None));
        assert_eq!((at(1.05 * 1_000.0 + 1e-9, 1e6, true), at(1.05 * 1_000.0, 1e6, true)), (Some(Phase::Liquid), None));
        assert_eq!((at(500.0, 2e6, true), at(99.8, 2e6, true), at(997.4, 2e6, true)), (None, None, None));
        assert_eq!(at(99.7, 2e6, true), None, "the vapour strip: quality above 1");
        let edge: f64 = 0.95 * (1.05 * 1_000.0);
        let above = f64::from_bits(edge.to_bits() + 1);
        assert_eq!((at(edge, 2e6, true), at(above, 2e6, true)), (None, Some(Phase::Liquid)), "the strip's edge");
        assert_eq!([at(998.0, 2e6, true), at(998.0, 2e6, false)], [Some(Phase::Liquid), None]);
        assert_eq!([at(998.0, 1.05 * 1e6, true), at(998.0, 1.04e6, true)], [None, None]);
        assert_eq!(asked, 3, "asked only in the liquid strip with the quality and p met");
        let near_top = pair(990.0);
        assert_eq!(band(998.0, 2e6, &near_top, || true), None, "quality 0.2 in the liquid strip");
        let side = |p, rho| SatSide { t: 300.0, p, rho };
        let scaled = SatPair { bubble: side(1e6, 1.0), dew: side(0.98e6, 0.1) };
        assert_eq!(band(0.998, 2e6, &scaled, || true), Some(Phase::Liquid), "the same bands 1000 times thinner");
    }

    /// The liquid strip's rule and the stability it asks, exactly: a quality of 0.01 or p at 1.05·p′ is not liquid, and
    /// stability needs both pressure derivatives positive (a zero, a negative or no value is unstable).
    #[test]
    fn strip_rule_and_stability_are_exact() {
        let liquid = |q: f64, p: f64, stable: bool| strip_liquid(q, p, 1e6, || stable);
        let (below, above) = (f64::from_bits(0.01_f64.to_bits() - 1), f64::from_bits((1.05 * 1e6_f64).to_bits() + 1));
        assert!(liquid(below, 2e6, true) && liquid(0.0, above, true));
        assert!(!liquid(0.01, 2e6, true) && !liquid(0.0, 1.05 * 1e6, true) && !liquid(0.0, 2e6, false));
        assert!(stable(Some([1.0, 1.0])));
        for c in [Some([0.0, 1.0]), Some([1.0, 0.0]), Some([-1.0, 1.0]), Some([1.0, -1.0]), None] {
            assert!(!stable(c), "{c:?}");
        }
    }

    /// PS3 on real blends: R407C's EOS has no dome at 358 K (its critical point is 356.60 K, the published one 359.345 K),
    /// where the bands alone decide gas and liquid (above pc a supercritical liquid), as CoolProp's; between them its VLE
    /// fails. An imposed `TwoPhase` skips
    /// the bands: R410A at 280 K and 100 mol/m³, gas by the bands, is refused (a quality outside [0, 1]).
    #[cfg(feature = "fluids-all")]
    #[test]
    fn bands_decide_where_the_eos_has_no_dome() {
        let r407c = embedded("R407C");
        let phase = |rho: f64| r407c.state(dt(rho, 358.0)).map(|s| s.phase());
        assert_eq!((phase(100.0), phase(12_000.0)), (Ok(Phase::Gas), Ok(Phase::SupercriticalLiquid)));
        assert!(phase(5_300.0).is_err(), "no dome on the EOS: {:?}", phase(5_300.0));
        let r410a = embedded("R410A");
        let imposed = FlashOptions::new().with_phase(Phase::TwoPhase);
        assert!(r410a.flash(dt(100.0, 280.0), &imposed).is_err());
        assert_eq!(r410a.state(dt(100.0, 280.0)).map(|s| s.phase()), Ok(Phase::Gas));
    }
}
