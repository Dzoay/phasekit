//! The flash's search scaffolds (PLAN.md M7.9e), each written once: TOMS 748 with a bracket's ends cached and its
//! first failure kept ([`toms748_kept`]), a descent to a bracket with bisection towards a refusal ([`Descent`],
//! [`bisect_to_refusal`]) and the choice among roots by `RootPolicy` ([`choose`]); and on them the search in T of P+X
//! and D+X ([`root_in_t`]).

use crate::error::{DomainError, Error, Roots};
use crate::fluid::PureFluid;
use crate::roots::{Root, Tol, toms748};
use crate::state::Strategy;

use super::{RootPolicy, T_ITER, T_TOL};

/// A point of a search: its abscissa and its residual there.
pub(super) type Point = (f64, f64);

/// Whether a residual lies above zero, on the far side of the root from a descent's end; NaN does not.
pub(super) fn positive(at: f64) -> bool {
    at > 0.0
}

/// Whether a residual lies at or below zero; NaN does not (HS's and SU's bisections hold a NaN point).
pub(super) fn at_or_below(at: f64) -> bool {
    at <= 0.0
}

/// TOMS 748 on [a, b] to `tol` in [`T_ITER`] iterations, for a residual that can fail. An end whose residual is given
/// (the search that found the bracket evaluated it) is not evaluated again; every other point is. A failure reads as
/// NaN, which stops TOMS 748 (`NotFinite`), and the first one is kept with its abscissa, returned beside the root.
pub(super) fn toms748_kept(
    mut residual: impl FnMut(f64) -> Result<f64, Error>,
    (a, at_a): (f64, Option<f64>),
    (b, at_b): (f64, Option<f64>),
    tol: Tol,
) -> (Root, Option<(f64, Error)>) {
    let mut failure = None;
    let given = |x: f64| {
        if x == a {
            at_a
        } else if x == b {
            at_b
        } else {
            None
        }
    };
    let evaluated = |x: f64| {
        given(x).unwrap_or_else(|| {
            residual(x).unwrap_or_else(|e| {
                failure.get_or_insert((x, e));
                f64::NAN
            })
        })
    };
    let root = toms748(evaluated, a, b, tol, T_ITER);
    (root, failure)
}

/// Bisection from `held`, a point not on the far side of the root, towards `refused`, where the residual failed. A
/// midpoint that fails becomes the refusal, one on the far side (`far`) ends the search, any other becomes `held`. At
/// most `steps` midpoints, and none once `held`'s residual is exactly zero. Returns the held point and the far one.
pub(super) fn bisect_to_refusal(
    mut residual: impl FnMut(f64) -> Result<f64, Error>,
    (mut held, mut at_held): Point,
    mut refused: f64,
    far: impl Fn(f64) -> bool,
    steps: u16,
) -> (Point, Option<Point>) {
    for _ in 0..steps {
        if at_held == 0.0 {
            break;
        }
        let next = 0.5 * (held + refused);
        match residual(next) {
            Ok(at) if far(at) => return ((held, at_held), Some((next, at))),
            Ok(at) => (held, at_held) = (next, at),
            Err(_) => refused = next,
        }
    }
    ((held, at_held), None)
}

/// A descent along a path on which a residual rises: x(T) − x down an isobar or an isochore ([`root_in_t`]), or
/// x(PS(p, s)) − x down an isentrope in ln p ([`sx`](super::isentrope::sx)). It stops at the bracket of the root: the
/// last point above zero and the first at or below it.
#[derive(Default)]
pub(super) struct Descent {
    /// The last point above zero.
    pub(super) above: Option<Point>,
    /// The first point at or below zero, where the descent stopped.
    pub(super) below: Option<Point>,
    /// The first and the last refusal since the descent last stepped above zero (or since its start).
    pub(super) refused: Option<(f64, f64)>,
    /// The first failure since then, unless a bisection towards it closed the bracket.
    pub(super) failed: Option<Error>,
    /// The first failure before the descent last stepped above zero.
    pub(super) failed_earlier: Option<Error>,
}

impl Descent {
    /// Steps down `points` to the first whose residual, `at(residual, x)`, is at or below zero. With `halvings`, a
    /// refusal right after a point above zero is bisected towards at once (T's descent); without, it is only noted.
    pub(super) fn down<R: FnMut(f64) -> Result<f64, Error>>(
        residual: &mut R,
        points: impl IntoIterator<Item = f64>,
        mut at: impl FnMut(&mut R, f64) -> Result<f64, Error>,
        halvings: Option<u16>,
    ) -> Descent {
        let mut descent = Descent::default();
        for x in points {
            match at(residual, x) {
                Ok(v) if positive(v) => descent.rise(x, v),
                Ok(v) => descent.below = Some((x, v)),
                Err(e) => descent.refuse(residual, x, e, halvings),
            }
            if descent.below.is_some() {
                break;
            }
        }
        descent
    }

    /// A step above zero at x: the bracket's top, the refusals and failure since the last one forgotten but the first.
    fn rise(&mut self, x: f64, at: f64) {
        self.failed_earlier = self.failed_earlier.take().or(self.failed.take());
        (self.above, self.refused) = (Some((x, at)), None);
    }

    /// A refusal at x with error `e`: with `halvings`, bisected towards from the last point above zero, which may close
    /// the bracket; else (or failing that) noted.
    fn refuse<R: FnMut(f64) -> Result<f64, Error>>(
        &mut self,
        residual: &mut R,
        x: f64,
        e: Error,
        halvings: Option<u16>,
    ) {
        if let (Some(halvings), Some(above)) = (halvings, self.above) {
            let (held, below) = bisect_to_refusal(&mut *residual, above, x, |at| !positive(at), halvings);
            (self.above, self.below) = (Some(held), below);
            if below.is_some() {
                return;
            }
        }
        self.failed.get_or_insert(e);
        self.refused = Some(self.refused.map_or((x, x), |(first, _)| (first, x)));
    }
}

/// The root `policy` takes among `roots`, ascending by `key`, one rule for every pair: the only one; of several, under
/// `Nearest(x)` the first whose key is [`nearest`]'s, else `Ambiguous` with every key. `None` without roots.
pub(crate) fn choose<T: Copy>(roots: &[T], key: impl Fn(&T) -> f64, policy: RootPolicy) -> Result<Option<T>, Error> {
    let keys: Vec<f64> = roots.iter().map(&key).collect();
    let ambiguous = || Error::Ambiguous { roots: Roots::new(&keys) };
    match (roots, policy) {
        ([], _) => Ok(None),
        ([one], _) => Ok(Some(*one)),
        (several, RootPolicy::Nearest(x)) => {
            let pick = nearest(x, &keys);
            several.iter().find(|r| Some(key(r)) == pick).map(|r| Some(*r)).ok_or_else(ambiguous)
        }
        _ => Err(ambiguous()),
    }
}

/// `RootPolicy::Nearest(x)`'s choice among ascending `roots`, one rule for every pair: the root nearest to `x`, and of
/// two equally near the first, the smaller; `None` without roots.
pub(super) fn nearest(x: f64, roots: &[f64]) -> Option<f64> {
    let distance = |root: &f64| (root - x).abs();
    roots.iter().copied().min_by(|a, b| distance(a).total_cmp(&distance(b)))
}

/// The bisections [`root_in_t`] takes from a T above x towards a refusal right below it: from a step of a fifth of T to
/// under [`T_TOL`] of it (0.2·2⁻⁴⁵ ≈ 6e-15).
const T_HALVINGS: u16 = 45;

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
pub(super) fn root_in_t(
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
    let steps = core::iter::successors(Some(hi), |&t| (t != lo).then(|| (0.8 * t).max(lo)));
    let at = |residual: &mut _, t| stepped(residual, t, (lo, hi), ends, tiny);
    let descent = Descent::down(&mut residual, steps.take(usize::from(T_ITER)), at, Some(T_HALVINGS));
    let above_max = || DomainError::AboveMaxTemperature { t: hi, t_max: fluid.limits().t_max() }.into();
    let Some((b, at_b)) = descent.above else { return Err(descent.failed.unwrap_or_else(above_max)) };
    let Some((a, at_a)) = descent.below else { return Err(descent.failed.unwrap_or(colder.into())) };
    if let Some(e) = descent.failed {
        return Err(e);
    }
    let (root, failure) = toms748_kept(&mut residual, (a, Some(at_a)), (b, Some(at_b)), Tol::Relative(T_TOL));
    if let Some((_, e)) = failure {
        return Err(e);
    }
    let root = root.converged(strategy)?;
    Ok((root.x, root.iterations))
}

/// x(T) − x at a step of [`root_in_t`]'s descent: the residual given at `hi` or `lo` (`ends`) or evaluated, and at
/// `lo` one within `tiny` the root, a state there, reproduced.
fn stepped(
    residual: &mut impl FnMut(f64) -> Result<f64, Error>,
    t: f64,
    (lo, hi): (f64, f64),
    (at_lo, at_hi): (Option<f64>, Option<f64>),
    tiny: f64,
) -> Result<f64, Error> {
    if t == hi {
        return at_hi.map_or_else(|| residual(t), Ok);
    }
    if t != lo {
        return residual(t);
    }
    let at = at_lo.map_or_else(|| residual(t), Ok)?;
    Ok(if at > tiny { at } else { at.min(0.0) })
}
