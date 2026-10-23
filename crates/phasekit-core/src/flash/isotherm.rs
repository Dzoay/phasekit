//! T+X (HT, ST and TU) along the isotherm ([`tx`]): the roots in ln ρ of DT's x − x, cut at its turns.

use crate::error::Error;
use crate::fluid::PureFluid;
use crate::num::math;
use crate::quantity::Quantity;
use crate::roots::{Tol, toms748};
use crate::saturation::SatAccuracy;
use crate::state::{Phase, SolvePath, State, Strategy};
use crate::units::Basis;

use super::phase::{labelling_critical, saturation_at_t};
use super::search::{Reached, choose};
use super::{Caloric, DomainPolicy, FlashOptions, T_ITER, T_TOL, check_t, dt, reproduces_x, total};

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
/// single phase holds along the whole isotherm, an imposed `TwoPhase` at the root, as in [`dx`](super::isochore::dx).
/// Errors name this pair ([`named`](super::named)).
pub(super) fn tx(fluid: &PureFluid, t: f64, x: f64, var: Caloric, opts: &FlashOptions) -> Result<State, Error> {
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
            // Equivalent (M7.5): a fluid whose saturation at T fails with `NoConvergence` from the VLE is a pseudo-pure
            // one; a pure fluid's below its curve's top comes from the curve.
            // equivalent mutant: replace match guard pseudo_pure with true
            Err(Error::NoConvergence { strategy: Strategy::Vle, .. }) if pseudo_pure => None,
            sat => Some(sat?.0),
        },
    };
    let mut roots: Vec<(f64, u16)> = Vec::new();
    let mut reached = Reached::default();
    let mut stretches: Vec<(f64, f64)> = Vec::new();
    match dome {
        Some(sat) => {
            let (vl, vv) = (1.0 / sat.bubble.rho, 1.0 / sat.dew.rho);
            let (xl, xv) =
                (var.at(r, t, &total(fluid, t, sat.bubble.rho)?), var.at(r, t, &total(fluid, t, sat.dew.rho)?));
            if (xl.min(xv)..=xl.max(xv)).contains(&x) {
                roots.push((-math::ln(vl + (x - xl) / (xv - xl) * (vv - vl)), 0));
            }
            reached.see(xl - x);
            reached.see(xv - x);
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
        reached.see(at_a.0);
        for b in tx_nodes(from, to).skip(1) {
            let at_b = point(b)?;
            let (found, turn) = cell_roots(&point, (a, at_a), (b, at_b))?;
            roots.extend(found);
            turn.into_iter().chain([at_b.0]).for_each(|r| reached.see(r));
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
    let Some(state) = choose(&states, |s| s.rho(Basis::Molar), Quantity::density(Basis::Molar), opts.roots())? else {
        return Err(refused.unwrap_or_else(|| reached.refusal((var.quantity(), x), |v| v, (Quantity::TEMPERATURE, t))));
    };
    if !reproduces_x(var, r, t, x, var.of(&state)) {
        return Err(Error::NoConvergence { strategy: Strategy::Isotherm, iterations: state.path().iterations });
    }
    Ok(state)
}

/// The nodes of T+X's grid over a stretch of ln ρ from `from` to `to`: [`TX_NODES`] per decade of ρ and at least
/// [`TX_CELLS`] cells, evenly spaced, both ends included.
pub(super) fn tx_nodes(from: f64, to: f64) -> impl Iterator<Item = f64> {
    let decades = (to - from) / core::f64::consts::LN_10;
    let cells = (decades * f64::from(TX_NODES)).ceil().max(f64::from(TX_CELLS));
    let step = (to - from) / cells;
    (0..=cells as u32).map(move |k| if f64::from(k) >= cells { to } else { from + f64::from(k) * step })
}

/// A cell's roots in ln ρ with their iterations, and the residual at its turn, if it has one.
type CellRoots = (Vec<(f64, u16)>, Option<f64>);

/// The roots of x(ln ρ) − x in one grid cell of an isotherm's single phase, from its ends' residuals and slopes
/// (`point`): the cell is cut at a turn of x where the slope changes sign, and each monotonic piece holds at most one
/// root (TOMS 748 in ln ρ to [`T_TOL`]). With them, the residual at the turn, if the cell has one.
fn cell_roots(
    point: &impl Fn(f64) -> Result<(f64, f64), Error>,
    (a, at_a): (f64, (f64, f64)),
    (b, at_b): (f64, (f64, f64)),
) -> Result<CellRoots, Error> {
    let (mut pieces, mut at) = (vec![(a, at_a.0, b, at_b.0)], None);
    // Equivalent (M7.5): a cell is cut at a turn where its ends' slopes have opposite signs; `a/b < 0` is the same
    // test, and `<= 0` differs only where a slope is exactly 0 at a node, where the cut falls on that node and the
    // pieces hold the same roots.
    // equivalent mutant: replace * with /
    // equivalent mutant: replace < with <=
    if at_a.1 * at_b.1 < 0.0 {
        let turn = toms748(|u| point(u).map_or(f64::NAN, |p| p.1), a, b, Tol::Absolute(T_TOL), T_ITER);
        let turn = turn.converged(Strategy::Isotherm)?.x;
        let at_turn = point(turn)?.0;
        pieces = vec![(a, at_a.0, turn, at_turn), (turn, at_turn, b, at_b.0)];
        at = Some(at_turn);
    }
    let mut roots = Vec::new();
    for (lo, at_lo, hi, at_hi) in pieces {
        // Equivalent (M7.5): with `&&` for an `||`, a root exactly on a node is still found from a cell that it ends,
        // unless x only touches the value there.
        // equivalent mutant: replace || with &&
        if at_lo == 0.0 || at_hi == 0.0 || at_lo.is_sign_negative() != at_hi.is_sign_negative() {
            let root = toms748(|u| point(u).map_or(f64::NAN, |p| p.0), lo, hi, Tol::Absolute(T_TOL), T_ITER);
            let root = root.converged(Strategy::Isotherm)?;
            roots.push((root.x, root.iterations));
        }
    }
    Ok((roots, at))
}
