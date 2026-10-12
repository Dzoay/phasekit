//! The phase rule (map 01 R26): where a (T, ρ) point lies ([`region`], DT's and the D+X searches') and the branch of a
//! (p, T) point ([`pt_branch`], PT's and the isobar's), both on the critical point that labels states
//! ([`labelling_critical`]).

use crate::error::Error;
use crate::fluid::PureFluid;
use crate::input::Pair;
use crate::model::ThermoModel;
use crate::saturation::{SatAccuracy, SatPair, SaturationCurve};
use crate::state::{Phase, Strategy};

use super::{FlashOptions, no_quality};

/// Where a (T, ρ) point lies.
pub(super) enum Region {
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
pub(super) fn region(fluid: &PureFluid, t: f64, rho: f64, p: f64, opts: &FlashOptions) -> Result<Region, Error> {
    let hint = opts.phase_hint();
    if let Some(single) = hint.filter(|h| *h != Phase::TwoPhase) {
        return Ok(Region::Single(single));
    }
    let curve = fluid.saturation()?;
    let crit = labelling_critical(fluid, curve)?;
    // At and above the critical temperature there is no dome to impose: no quality (D6, ROT-070).
    if hint == Some(Phase::TwoPhase) && crit.is_some_and(|c| t >= c.0) {
        return Err(no_quality());
    }
    if hint.is_none()
        && let Some(crit) = crit.filter(|c| t >= c.0)
    {
        return Ok(Region::Single(above_tc(t, rho, p, crit)));
    }
    if hint.is_none()
        && let Some(phase) = banded(fluid, curve, crit, (t, rho, p))?
    {
        return Ok(Region::Single(phase));
    }
    let (sat, strategy) = saturation_at_t(fluid, curve, crit, t)?;
    Ok(if hint == Some(Phase::TwoPhase) {
        Region::Dome(sat, strategy)
    } else if rho >= sat.bubble.rho {
        Region::Single(liquid_phase(above_pc(crit, p)))
    } else if rho <= sat.dew.rho {
        Region::Single(Phase::Gas)
    } else {
        Region::Dome(sat, strategy)
    })
}

/// The phase at or above the labelling critical temperature of `crit` (T, p, ρ): the critical point itself at its T and
/// ρ, else [`supercritical`] by p.
fn above_tc(t: f64, rho: f64, p: f64, (t_c, p_c, rho_c): (f64, f64, f64)) -> Phase {
    if t == t_c && rho == rho_c { Phase::CriticalPoint } else { supercritical(p, p_c) }
}

/// A state at or above the critical temperature, not the critical point: supercritical above the critical pressure
/// `p_c`, else a supercritical gas.
fn supercritical(p: f64, p_c: f64) -> Phase {
    if p > p_c { Phase::Supercritical } else { Phase::SupercriticalGas }
}

/// Whether p lies above the critical pressure of `crit` (T, p, ρ).
fn above_pc(crit: Option<(f64, f64, f64)>, p: f64) -> bool {
    crit.is_some_and(|(_, p_c, _)| p > p_c)
}

/// A liquid's label: above the critical pressure a supercritical liquid, as CoolProp labels it.
fn liquid_phase(above_pc: bool) -> Phase {
    if above_pc { Phase::SupercriticalLiquid } else { Phase::Liquid }
}

/// A pseudo-pure fluid's phase at (T, ρ, p) by its density bands ([`pseudo_pure_bands`]) where its `Definition` curve
/// is fitted at T, a liquid relabelled by p ([`liquid_phase`]); `None` where the bands leave it to the VLE, and for any
/// other curve.
fn banded(
    fluid: &PureFluid,
    curve: Option<&dyn SaturationCurve>,
    crit: Option<(f64, f64, f64)>,
    (t, rho, p): (f64, f64, f64),
) -> Result<Option<Phase>, Error> {
    let Some(curve) = curve.filter(|c| fitted(*c, t) && c.accuracy() == SatAccuracy::Definition) else {
        return Ok(None);
    };
    let relabelled = |phase| if phase == Phase::Liquid { liquid_phase(above_pc(crit, p)) } else { phase };
    Ok(pseudo_pure_bands(fluid, t, rho, p, &curve.at_t(t)?).map(relabelled))
}

/// The saturation at T below the labelling critical point and the strategy that gave it: an exact curve's; a stale
/// curve's, or a pseudo-pure fluid's definition, seeding the pure VLE (M6.6; inside its dome CoolProp solves the
/// pure-fluid VLE of the blend's EOS from the ancillaries' densities, D4, map 04 U4); else the generic VLE (M7.2).
pub(super) fn saturation_at_t(
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
pub(super) fn generic_vle(
    fluid: &PureFluid,
    crit: Option<(f64, f64, f64)>,
    t: f64,
    pair: Pair,
) -> Result<SatPair, Error> {
    match crit {
        Some((t_c, _, rho_c)) if t < t_c => crate::vle::from_eos(fluid.eos(), t, rho_c),
        _ => Err(Error::Unsupported { pair }),
    }
}

/// The critical point (T, p, ρ) that labels states: the top of an exact saturation curve, else the published one
/// ([`region`]).
pub(super) fn labelling_critical(
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
/// decision PP3; `HelmholtzEOSMixtureBackend.cpp:2340-2380`), at (T, ρ) of `fluid` with the EOS's p: see [`band`],
/// stability being (∂p/∂ρ)_T > 0 and (∂²p/∂ρ²)_T > 0 (`crate::crit::conditions`).
fn pseudo_pure_bands(fluid: &PureFluid, t: f64, rho: f64, p: f64, sat: &SatPair) -> Option<Phase> {
    band(rho, p, sat, || stable(crate::crit::conditions(fluid.eos(), t, rho)))
}

/// Stability as the bands ask it, from (∂p/∂ρ)_T and (∂²p/∂ρ²)_T (`crate::crit::conditions`): both positive.
pub(super) fn stable(conditions: Option<[f64; 2]>) -> bool {
    conditions.is_some_and(|[k1, k2]| k1 > 0.0 && k2 > 0.0)
}

/// The bands at density ρ and pressure p for the ancillary sides `sat`: gas below 0.95 of the dew density, liquid above
/// 1.05 of the bubble one, and in the liquid strip, above 0.9975 of the bubble density, liquid by [`strip_liquid`].
/// `None`: the VLE of the blend's EOS decides. CoolProp asks the same in a vapour strip below 0.9975 of the dew
/// density, where the ancillary quality exceeds 1 (ρ < ρ″ ≤ ρ′), so it never answers liquid there and is not asked. The
/// products are CoolProp's, rounded as it rounds them.
pub(super) fn band(rho: f64, p: f64, sat: &SatPair, stable: impl FnOnce() -> bool) -> Option<Phase> {
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
pub(super) fn strip_liquid(q_anc: f64, p: f64, p_bubble: f64, stable: impl FnOnce() -> bool) -> bool {
    q_anc < 0.01 && p > 1.05 * p_bubble && stable()
}

/// The branch of a PT state: its phase label, its density's bracket (`None`: solved from the seed alone), the seed, and
/// whether a restart begins at the liquid's far end.
pub(super) struct Branch {
    pub(super) phase: Phase,
    pub(super) bracket: Option<(f64, f64)>,
    pub(super) seed: f64,
    pub(super) liquid: bool,
}

/// Where (p, T) lies: one branch, or both at the saturation pressure.
pub(super) enum Where {
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
pub(super) fn pt_branch(fluid: &PureFluid, p: f64, t: f64, opts: &FlashOptions) -> Result<Where, Error> {
    let eos = fluid.eos();
    let (rho_max, ideal) = (eos.rho_max(t), p / (eos.gas_constant() * t));
    let floor = 1e-3 * ideal.min(rho_max);
    let curve = fluid.saturation()?;
    let fitted = curve.filter(|c| fitted(*c, t));
    if let Some(hint) = opts.phase_hint() {
        return hinted_branch(hint, fitted, t, (ideal, rho_max));
    }
    let crit = labelling_critical(fluid, curve)?;
    if let Some(branch) = critical_branch(crit, p, t, (floor, ideal, rho_max)) {
        return Ok(Where::One(branch));
    }
    let over_pc = above_pc(crit, p);
    let liquid = liquid_phase(over_pc);
    let sat = match fitted {
        Some(curve) => {
            let sat = curve.at_t(t)?;
            match curve.accuracy() {
                SatAccuracy::Definition => return pseudo_pure_branch(&sat, p, liquid, ideal),
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
    Ok(if over_pc || p > sat.bubble.p {
        Where::One(liquid)
    } else if p < sat.bubble.p {
        Where::One(gas)
    } else {
        Where::Both { gas, liquid }
    })
}

/// The branch of a phase imposed on PT ([`pt_branch`]): its own, unbracketed, a liquid's from the saturated liquid's
/// density at T of a curve `fitted` there (read whatever the phase), else from the model's largest, any other's from
/// the ideal gas's (`ideal`); an imposed `TwoPhase` is refused, since (p, T) fix no quality.
fn hinted_branch(
    hint: Phase,
    fitted: Option<&dyn SaturationCurve>,
    t: f64,
    (ideal, rho_max): (f64, f64),
) -> Result<Where, Error> {
    let liquid = matches!(hint, Phase::Liquid | Phase::SupercriticalLiquid);
    let seed = match fitted.map(|c| c.at_t(t)) {
        _ if !liquid => ideal,
        Some(Ok(sat)) => sat.bubble.rho,
        _ => rho_max,
    };
    if hint == Phase::TwoPhase {
        return Err(no_quality());
    }
    Ok(Where::One(Branch { phase: hint, bracket: None, seed, liquid }))
}

/// The PT branch at or above the labelling critical temperature of `crit` (T, p, ρ), if any: the critical point itself,
/// else one branch, [`supercritical`] by p, from a thousandth of the ideal gas's density (or of the largest, `floor`)
/// to the model's largest.
fn critical_branch(
    crit: Option<(f64, f64, f64)>,
    p: f64,
    t: f64,
    (floor, ideal, rho_max): (f64, f64, f64),
) -> Option<Branch> {
    let (t_c, p_c, rho_c) = crit?;
    if t == t_c && p == p_c {
        return Some(Branch { phase: Phase::CriticalPoint, bracket: None, seed: rho_c, liquid: false });
    }
    (t >= t_c).then(|| Branch {
        phase: supercritical(p, p_c),
        bracket: Some((floor, rho_max)),
        seed: ideal,
        liquid: false,
    })
}

/// A pseudo-pure fluid's PT branch by its `Definition` (`sat`, its ancillaries at T): the `liquid` above the bubble
/// pressure, gas below the dew one, each from its ancillary's seed (the ideal gas's for the gas) and unbracketed;
/// between them (p, T) is two-phase, where it fixes no quality.
fn pseudo_pure_branch(sat: &SatPair, p: f64, liquid: Phase, ideal: f64) -> Result<Where, Error> {
    if p > sat.bubble.p {
        return Ok(Where::One(Branch { phase: liquid, bracket: None, seed: sat.bubble.rho, liquid: true }));
    }
    if p < sat.dew.p {
        return Ok(Where::One(Branch { phase: Phase::Gas, bracket: None, seed: ideal, liquid: false }));
    }
    Err(no_quality())
}

/// A PT bracket's saturated end reaches this far past ρ′ (into the metastable liquid) or ρ″ (the metastable vapour), a
/// factor 1 − `SAT_RELAX` in ρ either way: a curve's densities and pressure agree with the EOS's own VLE only to its
/// fit, so for p a hair from psat the root lies a hair past them. The spinodal is far further (0.2·ln(ρ′/ρ″) inside
/// each phase near Tc, mean field), so the branch stays monotonic (PLAN.md M7.2).
const SAT_RELAX: f64 = 1e-9;
