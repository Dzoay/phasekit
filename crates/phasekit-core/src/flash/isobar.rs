//! P+X: PH, PS and PU along the isobar ([`px`]), split by the saturation at p ([`saturation_at_p`]), T found on each
//! single-phase stretch by the search in T ([`isobar`]).

use crate::derivs::Order;
use crate::error::{DomainError, Error};
use crate::fluid::PureFluid;
use crate::input::Pair;
use crate::model::ThermoModel;
use crate::num::math;
use crate::roots::Tol;
use crate::saturation::{SatAccuracy, SatPair, SatSide};
use crate::state::{Phase, SolvePath, State, Strategy};
use crate::units::Quality;

use super::cubic::Cubic;
use super::phase::{Branch, Where, generic_vle, labelling_critical, pt_branch};
use super::quality::{curve_at_p, split_sides};
use super::search::{root_in_t, toms748_kept};
use super::{
    AT_LOWEST_T, Caloric, DomainPolicy, FlashOptions, T_TOL, accept, check_p, no_quality, reproduces_x, solve_branch,
    t_search, total, two_phase, within_domain,
};

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
/// [0, 1]. Errors name this pair ([`named`](super::named)).
pub(super) fn px(fluid: &PureFluid, p: f64, x: f64, var: Caloric, opts: &FlashOptions) -> Result<State, Error> {
    if opts.domain() == DomainPolicy::Enforce {
        check_p(fluid, p)?;
    }
    let (t_low, t_high) = t_search(fluid);
    let hint = opts.phase_hint();
    let along = |lo, hi, ends, liquid, edge| isobar(fluid, p, x, var, opts, Span { lo, hi, ends, liquid, edge });
    if hint.is_some_and(|h| h != Phase::TwoPhase) {
        return along(t_low, t_high, (None, None), false, None);
    }
    let Some((sat, strategy)) = saturation_at_p(fluid, p, t_low, opts)? else {
        if hint == Some(Phase::TwoPhase) {
            return Err(no_quality());
        }
        return along(t_low, t_high, (None, None), false, None);
    };
    // Each saturated side's x, the EOS's at its (T, ρ): a pseudo-pure side's ρ is the EOS's at its (T, p), as PQ's.
    // Each side's x and its (T, ρ), where the side's span begins its Newton ([`newton_t_rho`]).
    let side = |side: &SatSide, liquid: bool| -> Result<(f64, (f64, f64)), Error> {
        let rho = match strategy {
            Strategy::Ancillary => crate::density::at_t_p(fluid.eos(), side.t, side.p, side.rho, liquid)?.0,
            _ => side.rho,
        };
        Ok((var.at(fluid.eos().gas_constant(), side.t, &total(fluid, side.t, rho)?), (side.t, rho)))
    };
    // The lever rule's quality on x: below 0 the liquid, above 1 the gas, unless `TwoPhase` is imposed; then an x the
    // acceptance gate takes for a side's is that side (an HS root on the dome's edge).
    let ((x_l, bubble), (x_v, dew)) = (side(&sat.bubble, true)?, side(&sat.dew, false)?);
    let (q, imposed) = ((x - x_l) / (x_v - x_l), hint == Some(Phase::TwoPhase));
    if !imposed && q < 0.0 {
        return along(t_low, sat.bubble.t, (None, Some(x_l)), true, Some(bubble));
    }
    if !imposed && q > 1.0 {
        return along(sat.dew.t, t_high, (Some(x_v), None), false, Some(dew));
    }
    let q = edge_quality(var, fluid.eos().gas_constant(), &sat, x, (x_l, x_v), q)?;
    let state = match strategy {
        Strategy::Ancillary => split_sides(fluid, &sat, q, q.get() * sat.dew.t + (1.0 - q.get()) * sat.bubble.t, p),
        strategy => two_phase(fluid, &sat, q, strategy),
    }?;
    within_domain(fluid, state, opts)
}

/// P+X's quality `q`, the lever rule's on x between the saturated sides' `x_l` and `x_v` (`sat`), as 0 or 1 where the
/// acceptance gate takes x for that side's (an HS root on the dome's edge).
fn edge_quality(var: Caloric, r: f64, sat: &SatPair, x: f64, (x_l, x_v): (f64, f64), q: f64) -> Result<Quality, Error> {
    let q = if reproduces_x(var, r, sat.bubble.t, x, x_l) { q.max(0.0) } else { q };
    Quality::new(if reproduces_x(var, r, sat.dew.t, x, x_v) { q.min(1.0) } else { q })
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
    let gap = |t: f64| {
        let pressure =
            if t >= t_c { Ok(p_c) } else { generic_vle(fluid, Some(crit), t, Pair::PQ).map(|sat| sat.bubble.p) };
        pressure.map(|pressure| math::ln(pressure / p))
    };
    // TOMS 748 evaluates T_low again: no end is given.
    let at_low = gap(t_low)?;
    let Some((root, failure)) =
        (at_low < 0.0).then(|| toms748_kept(&gap, (t_low, None), (t_c, None), Tol::Relative(T_TOL)))
    else {
        return Ok(None);
    };
    if let Some((_, e)) = failure {
        return Err(e);
    }
    let t = root.converged(Strategy::Vle)?.x;
    let seed = generic_vle(fluid, Some(crit), t, Pair::PQ)?;
    Ok(Some((crate::vle::at_p(fluid.eos(), p, (t, seed.bubble.rho, seed.dew.rho))?, Strategy::Vle)))
}

/// A stretch of a P+X isobar that holds one single-phase branch: T from `lo` to `hi`, x known at a saturated end
/// (`ends`), the side of the dome it lies on (`liquid`: the root PT takes at exactly psat), and that saturated end's
/// (T, ρ) (`edge`). A span never reaches into a pseudo-pure dome: its saturated end is the bubble or dew temperature,
/// whose x it is given.
pub(super) struct Span {
    pub(super) lo: f64,
    pub(super) hi: f64,
    pub(super) ends: (Option<f64>, Option<f64>),
    pub(super) liquid: bool,
    pub(super) edge: Option<(f64, f64)>,
}

/// The single-phase state of x on `span` of the isobar at p ([`Strategy::Isobar`]): Newton in (T, ln ρ)
/// ([`newton_t_rho`]), else the root in T of x(T) − x ([`root_in_t`]), each iterate PT's density at (T, p) by its branch
/// rule ([`pt_branch`]), seeded by the last iterate's ([`Walk`]). The acceptance gate: p and x reproduced, and unless a
/// phase is imposed the state stable; then the domain, as PT's.
fn isobar(fluid: &PureFluid, p: f64, x: f64, var: Caloric, opts: &FlashOptions, span: Span) -> Result<State, Error> {
    let eos = fluid.eos();
    let (r, m, key) = (eos.gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    if let Some(state) = newton_t_rho(fluid, p, x, var, opts, &span) {
        return within_domain(fluid, state, opts);
    }
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

/// The single-phase state of x on `span` of the isobar at p by Newton in (T, ln ρ) on (p, x) (issue #143; CoolProp's
/// `HSU_P_flash_singlephase_Newton`), one bundle an iterate, from the span's saturated end or else Peng and Robinson's
/// (T, ρ) at (p, x) ([`cubic_seed`]); a step is shortened to at most 20 % in T and 0.5 in ln ρ. `None`, and
/// [`isobar`]'s search answers, where a phase is imposed or it has no seed, an iterate leaves the span or the stable
/// states, it does not converge, or the state it reaches is not PT's at its (p, T): outside that branch's densities,
/// or failing the acceptance gate.
fn newton_t_rho(fluid: &PureFluid, p: f64, x: f64, var: Caloric, opts: &FlashOptions, span: &Span) -> Option<State> {
    if opts.phase_hint().is_some() {
        return None;
    }
    let (r, m, key) = (fluid.eos().gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    let (mut t, rho) = span.edge.or_else(|| cubic_seed(fluid, p, x, var, span))?;
    let mut u = math::ln(rho);
    for iteration in 1..=NEWTON_ITER {
        let rho = math::exp(u);
        let b = total(fluid, t, rho).ok()?;
        let (dp, dx) = (crate::relations::pressure(r, t, rho, &b) - p, var.at(r, t, &b) - x);
        let (p_t, p_u) = (rho * r * (b.a01 - b.a11), rho * r * t * (2.0 * b.a01 + b.a02));
        let (x_t, x_u) = var.jacobian(r, t, &b);
        let det = p_t * x_u - p_u * x_t;
        // An iterate where the model is mechanically unstable ends it: Peng and Robinson's seed can lie inside the
        // model's spinodal near its critical point (R32 at 5.95 MPa and 351 K). A singular or non-finite step leaves T
        // non-finite, which the span refuses below.
        if p_u <= 0.0 {
            return None;
        }
        let (step_t, step_u) = ((dx * p_u - dp * x_u) / det, (dp * x_t - dx * p_t) / det);
        if (step_t / t).abs() <= NEWTON_TOL && step_u.abs() <= NEWTON_TOL {
            return pt_state(fluid, (p, x, var), opts, span, (t, rho, &b)).map(|branch| {
                let path = SolvePath { strategy: Strategy::TRhoNewton, iterations: iteration };
                State::single(key, t, rho, r, m, branch.phase, &b, path).ok()
            })?;
        }
        let shrink = (0.2 * t / step_t.abs()).min(0.5 / step_u.abs()).min(1.0);
        (t, u) = (t + shrink * step_t, u + shrink * step_u);
        if !(span.lo <= t && t <= span.hi) {
            return None;
        }
    }
    None
}

/// Iterations [`newton_t_rho`] may take.
const NEWTON_ITER: u16 = 40;

/// [`newton_t_rho`] has converged when its step in T is within this share of T and its step in ln ρ within this.
const NEWTON_TOL: f64 = 1e-13;

/// The branch of PT at (p, T) on the span's side, if the state (T, ρ) with total bundle `b` is that branch's and passes
/// [`isobar`]'s acceptance gate: ρ within the branch's densities, p and x reproduced, the state stable.
pub(super) fn pt_state(
    fluid: &PureFluid,
    (p, x, var): (f64, f64, Caloric),
    opts: &FlashOptions,
    span: &Span,
    (t, rho, b): (f64, f64, &crate::derivs::Bundle),
) -> Option<Branch> {
    let r = fluid.eos().gas_constant();
    let branch = match pt_branch(fluid, p, t, opts).ok()? {
        Where::One(branch) => branch,
        Where::Both { liquid, gas } => {
            if span.liquid {
                liquid
            } else {
                gas
            }
        }
    };
    let (lo, hi) = branch.bracket?;
    let reproduced = crate::density::reproduces(r, t, rho, p, b) && reproduces_x(var, r, t, x, var.at(r, t, b));
    (lo <= rho && rho <= hi && reproduced && accept(r, t, b).is_ok()).then_some(branch)
}

/// Peng and Robinson's (T, ρ) at (p, x) on `span` ([`Cubic`]): the T where the model's own ideal-gas x at (T, p) plus
/// the cubic's departure is x, by Newton on a numerical slope kept within the span, from the critical temperature (the
/// span's nearer end outside it). `None` without a critical point or an acentric factor, where the cubic has no root,
/// where x falls as T rises, or where it does not converge.
pub(super) fn cubic_seed(fluid: &PureFluid, p: f64, x: f64, var: Caloric, span: &Span) -> Option<(f64, f64)> {
    let eos = fluid.eos();
    let (r, crit) = (eos.gas_constant(), fluid.critical_point()?);
    let cubic = Cubic::new(crit.t, crit.p, fluid.info().constants().acentric?, r);
    let residual = |t: f64| -> Option<f64> {
        let ideal = eos.ideal(t, p / (r * t), Order::Two).bundle()?;
        Some(var.at(r, t, &ideal) + cubic.departure(var, t, p, cubic.density(t, p)?)? - x)
    };
    let mut t = crit.t.clamp(span.lo, span.hi);
    for _ in 0..SEED_ITER {
        let (at, beside) = (residual(t)?, residual(t * (1.0 + SEED_STEP))?);
        let slope = (beside - at) / (t * SEED_STEP);
        if slope <= 0.0 {
            return None;
        }
        let next = (t - at / slope).clamp(span.lo, span.hi);
        if (next - t).abs() <= SEED_TOL * t {
            return Some((next, cubic.density(next, p)?));
        }
        t = next;
    }
    None
}

/// [`cubic_seed`]'s iterations, its relative step in T for the slope, and the share of T its last step is within.
const SEED_ITER: u16 = 30;
const SEED_STEP: f64 = 1e-6;
const SEED_TOL: f64 = 1e-6;
