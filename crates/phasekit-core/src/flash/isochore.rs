//! D+X (DH, DS and DU) and DP along the isochore ([`dx`], [`dp`]): the root in T of DT's x or p at ρ.

use crate::error::Error;
use crate::fluid::PureFluid;
use crate::relations;
use crate::saturation::SatAccuracy;
use crate::state::{Phase, SolvePath, State, Strategy};

use super::phase::{Region, region};
use super::search::root_in_t;
use super::{
    AT_LOWEST_T, Caloric, DomainPolicy, FlashOptions, check_p, dt, reproduces_p, reproduces_x, t_search, total,
};

/// D+X: DH, DS and DU (PLAN.md M7.4; map 03 §3.1, §9 `flash::dx`). Along the isochore x(T) is DT's ([`isochore_x`]): a
/// single phase's from the EOS at (T, ρ), in the dome the lever rule's on the saturation at T (DT's phase rule,
/// [`region`]), so it is continuous through the dome's edge, where ρ is a saturated density. T is the root of x(T) − x
/// from T_low to 1.5·Tmax ([`root_in_t`], `Strategy::Isochore`), and the state DT's at it, with DT's acceptance and
/// domain checks and x reproduced. CoolProp splits the range at the saturation crossings and classifies each interval
/// (map 03 §3.1); a residual continuous through the dome needs no intervals. An imposed single phase holds along the
/// whole isochore; an imposed `TwoPhase` holds at the root, so a single-phase answer is refused there (a quality
/// outside [0, 1]). Errors name this pair ([`named`](super::named)).
pub(super) fn dx(fluid: &PureFluid, rho: f64, x: f64, var: Caloric, opts: &FlashOptions) -> Result<State, Error> {
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
/// isochore, an imposed `TwoPhase` holds at the root. Errors name this pair ([`named`](super::named)).
pub(super) fn dp(fluid: &PureFluid, rho: f64, p: f64, opts: &FlashOptions) -> Result<State, Error> {
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
