//! HS and SU along the isentrope ([`sx`]): the root in ln p of x(PS(p, s)) − x.

use crate::error::{DomainError, Error};
use crate::fluid::PureFluid;
use crate::num::math;
use crate::roots::{Root, Tol};
use crate::state::{SolvePath, State, Strategy};

use super::isobar::px;
use super::search::{Descent, Point, at_or_below, bisect_to_refusal, positive, toms748_kept};
use super::{Caloric, DomainPolicy, FlashOptions, T_TOL, reproduces_x};

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
/// below them (the domain's lowest temperature), or `InvalidInput`. Errors name this pair ([`named`](super::named)).
pub(super) fn sx(fluid: &PureFluid, s: f64, x: f64, var: Caloric, opts: &FlashOptions) -> Result<State, Error> {
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
    let mut residual = |u: f64| {
        let (at, t) = raw(u)?;
        Ok(if at.abs() <= ISENTROPE_NOISE * var.scale(r, t, x) { 0.0 } else { at })
    };
    // Down from pmax a decade at a time, over p where PS refuses, to the first p where x(p) is at or below x; the last
    // p above x before it closes the bracket unless PS refused between them. Otherwise bisection in ln p between the
    // bracket's known end and the nearest refusal beside it finds its other end ([`isentrope_bracket`]).
    let decades = core::iter::successors(Some(math::ln(p_max)), |u| Some(u - core::f64::consts::LN_10));
    let descent = Descent::down(&mut residual, decades.take(usize::from(SX_DECADES)), |residual, u| residual(u), None);
    let invalid = || Error::InvalidInput { quantity: var.name(), value: x };
    let bracket = isentrope_bracket(&mut residual, descent, p_max, invalid)?;
    let root = isentrope_root(|u| raw(u).map(|(at, _)| at), &mut residual, bracket)?.converged(Strategy::Isentrope)?;
    let state = px(fluid, p_of(root.x), s, Caloric::S, opts)?;
    if !reproduces_x(var, r, state.t(), x, var.of(&state)) {
        return Err(Error::NoConvergence { strategy: Strategy::Isentrope, iterations: root.iterations });
    }
    Ok(state.with_path(SolvePath { strategy: Strategy::Isentrope, iterations: root.iterations }))
}

/// HS's and SU's bracket in ln p, (at or below x, above x), from their descent down the isentrope ([`sx`]): the last p
/// above x and the first at or below it, unless PS refused between them; else bisection between the bracket's known end
/// and the nearest refusal beside it finds its other end ([`towards_refusal`]). The isentrope can leave the domain and
/// come back (at the triple point's temperature water's s(p) turns at 38.6 MPa), or end in it (under the triple point's
/// pressure), and a root at such an edge (a state at the triple point's temperature) is reached there, its residual
/// within the noise. Without a bracket: `AboveMaxPressure` at pmax where x lies above x(pmax), else the descent's first
/// failure, or `invalid`.
fn isentrope_bracket(
    residual: &mut impl FnMut(f64) -> Result<f64, Error>,
    descent: Descent,
    p_max: f64,
    invalid: impl FnOnce() -> Error,
) -> Result<(Point, Point), Error> {
    let (above, below, refused) = (descent.above, descent.below, descent.refused);
    let found = match (above, below) {
        (Some(above), Some(below)) if refused.is_none() => Some((below, above)),
        (_, Some(below)) if below.1 == 0.0 => Some((below, below)),
        (_, Some(below)) => {
            let Some((_, last)) = refused else { return Err(DomainError::AboveMaxPressure { p: p_max, p_max }.into()) };
            towards_refusal(residual, below, last, positive)
        }
        (Some(above), None) => {
            refused.and_then(|(first, _)| towards_refusal(residual, above, first, at_or_below)).map(|(b, a)| (a, b))
        }
        (None, None) => None,
    };
    found.ok_or_else(|| descent.failed_earlier.or(descent.failed).unwrap_or_else(invalid))
}

/// HS's and SU's bisection in ln p from a known end, `held`, towards a refusal: the held end and the first point on the
/// far side (`far`), or a held end whose residual is 0 as both ends of its own bracket; `None` if neither.
fn towards_refusal(
    residual: &mut impl FnMut(f64) -> Result<f64, Error>,
    held: Point,
    refused: f64,
    far: impl Fn(f64) -> bool,
) -> Option<(Point, Point)> {
    match bisect_to_refusal(&mut *residual, held, refused, far, SX_STEPS) {
        (held, Some(found)) => Some((held, found)),
        (held, None) => (held.1 == 0.0).then_some((held, held)),
    }
}

/// TOMS 748 in ln p on HS's and SU's bracket (`a` at or below x, `b` above it) on the `raw` residual, its ends' values
/// the noise-snapped `residual`'s. A refusal inside it (the isentrope dips out of the domain between its ends:
/// Hydrogen's under its triple point's temperature near 20 K and 470 MPa) splits it: bisection from its upper end
/// towards the refusal finds a p below x, or else from its lower end one above x, and TOMS 748 runs again on that part,
/// at most [`SX_SPLITS`] times.
fn isentrope_root(
    mut raw: impl FnMut(f64) -> Result<f64, Error>,
    residual: &mut impl FnMut(f64) -> Result<f64, Error>,
    (mut a, mut b): (Point, Point),
) -> Result<Root, Error> {
    for _ in 0..SX_SPLITS {
        let (root, refused) = toms748_kept(&mut raw, (a.0, Some(a.1)), (b.0, Some(b.1)), Tol::Absolute(T_TOL));
        let Some((at, e)) = refused else { return Ok(root) };
        (a, b) = match towards_refusal(residual, b, at, at_or_below) {
            Some((above, below)) => (below, above),
            None => towards_refusal(residual, a, at, positive).ok_or(e)?,
        };
    }
    Err(Error::NoConvergence { strategy: Strategy::Isentrope, iterations: 0 })
}
