//! The pairs with a quality: QT and PQ on the saturation curve ([`qt`], [`pq`]), and the Q-pairs QS, HQ and DQ, every
//! root in T along it ([`qx`]).

use crate::derivs::{Bundle, Derivs, Order};
use crate::error::Error;
use crate::exec::ExecPolicy;
use crate::fluid::PureFluid;
use crate::input::Pair;
use crate::model::ThermoModel;
use crate::roots::{Tol, toms748};
use crate::saturation::{SatAccuracy, SatPair, SatSide, SaturationCurve};
use crate::state::{Phase, SolvePath, State, Strategy};
use crate::units::{Basis, Quality};

use super::search::{Point, choose, nearest};
use super::{
    DomainPolicy, FlashOptions, GROUP, RootPolicy, T_ITER, T_TOL, above_melting_line, phase_bundle, phase_bundle_of,
    phase_order, two_phase, two_phase_from, within_domain,
};

/// What a Q-pair fixes beside the quality: h, s or, for DQ, the molar volume v = 1/ρ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QVar {
    H,
    S,
    V,
}

impl QVar {
    /// Its value on a state (molar).
    pub(super) fn of(self, state: &State) -> f64 {
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
    pub(super) fn lever(self, q: f64, sides: [f64; 6]) -> Option<f64> {
        let [hl, hv, sl, sv, ..] = sides;
        match self {
            QVar::H => Some((1.0 - q) * hl + q * hv),
            QVar::S => Some((1.0 - q) * sl + q * sv),
            QVar::V => None,
        }
    }
}

/// The grid cells along a saturation curve's fitted range on which a Q-pair brackets its roots.
pub(super) const QX_CELLS: u16 = 200;

/// The times a Q-pair's grid halves its last cell towards the critical point, where x(T) goes like powers of Tc − T and
/// its turns crowd: down to 1e-9 of a cell.
const QX_TOP_HALVINGS: usize = 30;

/// The golden section's ratio, (√5 − 1)/2.
const GOLDEN: f64 = 0.618_033_988_749_894_9;

/// The nodes of a Q-pair's grid over a curve's fitted range [`lo`, `hi`]: [`QX_CELLS`] even cells, the last halved
/// [`QX_TOP_HALVINGS`] times towards `hi`, both ends included.
pub(super) fn qx_nodes(lo: f64, hi: f64) -> impl Iterator<Item = f64> {
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
pub(super) fn qx(
    fluid: &PureFluid,
    q: f64,
    x: f64,
    var: QVar,
    pair: Pair,
    opts: &FlashOptions,
) -> Result<State, Error> {
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
    let (runs, mut refused) = scan_runs(qx_nodes(lo, hi), &scan);
    let (mut roots, polish_on) = (Vec::new(), calorics.is_some().then_some(&exact));
    for grid in runs {
        run_roots(&with_turns(&grid, &scan, &mut refused), &scan, polish_on, &mut roots)?;
    }
    roots.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some((t, iterations)) = choose(&roots, |r| r.0, opts.roots())? else {
        return Err(refused.unwrap_or(Error::InvalidInput { quantity: var_name(var), value: var.input(x) }));
    };
    Ok(qt(fluid, q, t, opts)?.with_path(SolvePath { strategy: Strategy::Saturation, iterations }))
}

/// A Q-pair's grid `nodes` with x(T) − x there (`scan`), in runs between the nodes where it is refused, and the first
/// refusal.
fn scan_runs(
    nodes: impl Iterator<Item = f64>,
    scan: &impl Fn(f64) -> Result<f64, Error>,
) -> (Vec<Vec<Point>>, Option<Error>) {
    let (mut runs, mut refused): (Vec<Vec<Point>>, _) = (vec![Vec::new()], None);
    for t in nodes {
        match scan(t) {
            Ok(g) => runs.last_mut().into_iter().for_each(|run| run.push((t, g))),
            Err(e) => {
                refused.get_or_insert(e);
                runs.push(Vec::new());
            }
        }
    }
    (runs, refused)
}

/// A run of a Q-pair's grid with each turn of x(T) between its nodes ([`turn_of`]), ascending in T. A turn the search
/// cannot reach (x refused inside the cells) is left out, as a refused node; its refusal is kept in `refused` if the
/// first.
fn with_turns(grid: &[Point], scan: &impl Fn(f64) -> Result<f64, Error>, refused: &mut Option<Error>) -> Vec<Point> {
    let mut nodes = grid.to_vec();
    for triple in grid.windows(3) {
        if let [(t0, g0), (_, g1), (t2, g2)] = triple
            && (g1 > g0) != (g2 > g1)
        {
            match turn_of(scan, *t0, *t2, g1 > g0) {
                Ok(node) => nodes.push(node),
                Err(e) => _ = refused.get_or_insert(e),
            }
        }
    }
    nodes.sort_by(|a, b| a.0.total_cmp(&b.0));
    nodes
}

/// The roots of x(T) − x (`scan`) between a Q-pair's ascending `nodes`, added to `roots` with their iterations: TOMS
/// 748 in each pair of adjacent nodes where x(T) − x is 0 or changes sign, polished on the EOS where the scan read the
/// caloric curves (`polish_on`, [`polish`]).
fn run_roots(
    nodes: &[Point],
    scan: &impl Fn(f64) -> Result<f64, Error>,
    polish_on: Option<&impl Fn(f64) -> Result<f64, Error>>,
    roots: &mut Vec<(f64, u16)>,
) -> Result<(), Error> {
    for pair in nodes.windows(2) {
        let [(a, ga), (b, gb)] = pair else { continue };
        if *ga != 0.0 && *gb != 0.0 && ga.is_sign_negative() == gb.is_sign_negative() {
            continue;
        }
        let found = toms748(|t| scan(t).unwrap_or(f64::NAN), *a, *b, Tol::Relative(T_TOL), T_ITER);
        let found = found.converged(Strategy::Saturation)?;
        let t = match polish_on {
            Some(exact) => polish(exact, found.x)?,
            None => found.x,
        };
        // A root on a node (or a turn) closes the brackets either side of it; each returns that node exactly.
        if !roots.iter().any(|(r, _)| r.to_bits() == t.to_bits()) {
            roots.push((t, found.iterations));
        }
    }
    Ok(())
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
pub(super) fn polish(exact: &impl Fn(f64) -> Result<f64, Error>, t: f64) -> Result<f64, Error> {
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
pub(super) fn split_sides(fluid: &PureFluid, sat: &SatPair, q: Quality, t: f64, p: f64) -> Result<State, Error> {
    let (eos, m, key) = (fluid.eos(), fluid.info().molar_mass(), fluid.info().key());
    let (r, path) = (eos.gas_constant(), SolvePath { strategy: Strategy::Ancillary, iterations: 0 });
    let phase = |side: &SatSide, phase: Phase| {
        let (rho, _) = crate::density::at_t_p(eos, side.t, side.p, side.rho, phase == Phase::Liquid)?;
        State::single(key, side.t, rho, r, m, phase, &phase_bundle(fluid, side.t, rho, q)?, path)
    };
    State::split(phase(&sat.bubble, Phase::Liquid)?, phase(&sat.dew, Phase::Gas)?, q, t, p, path)
}

/// QT: the saturation at T, then the state at quality q (ROT-004: QT and PQ share one `SaturationCurve` path). An
/// `Exact` curve's equilibrium is the answer (ROT-092: no VLE); a `Guess` curve seeds the pure VLE (ROT-088). The path
/// names the source (ROT-085). The curve refuses T outside its fitted range under either domain policy (D6).
pub(super) fn qt(fluid: &PureFluid, q: f64, t: f64, opts: &FlashOptions) -> Result<State, Error> {
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
pub(super) fn pq(fluid: &PureFluid, p: f64, q: f64, opts: &FlashOptions) -> Result<State, Error> {
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
pub(super) fn curve_at_p(curve: &dyn SaturationCurve, p: f64, opts: &FlashOptions) -> Result<SatPair, Error> {
    nearest_at_p(curve, p, curve.at_p(p), opts)
}

/// `sat`, the saturation of `curve` at p, with several saturation temperatures resolved as [`curve_at_p`] does.
fn nearest_at_p(
    curve: &dyn SaturationCurve,
    p: f64,
    sat: Result<SatPair, Error>,
    opts: &FlashOptions,
) -> Result<SatPair, Error> {
    match (sat, opts.roots()) {
        (Err(Error::Ambiguous { roots }), RootPolicy::Nearest(x)) => {
            let sat = curve.at_t(nearest(x, roots.as_slice()).ok_or(Error::Ambiguous { roots })?)?;
            Ok(sat.at_pressure(p))
        }
        (sat, _) => sat,
    }
}

/// What a QT or PQ batch fixes beside the quality ([`sat_lanes`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SatBy {
    /// T: QT, the native point (Q, T).
    T,
    /// p: PQ, the native point (p, Q).
    P,
}

/// QT or PQ at every native point `(x[i], y[i])` into `out[i]`, each [`qt`]'s or [`pq`]'s result, checked against the
/// melting line as `flash` checks it, bit for bit (PLAN.md M12.9). On an `Exact` curve, [`GROUP`] points at a time:
/// their saturations through the curve's batch hook (`at_t_batch`, `at_p_batch`) and both phases' bundles through the
/// EOS's ([`phase_bundles`]), under `ExecPolicy::Simd`, then each point's state as `two_phase` builds it. A curve that
/// only seeds the VLE or defines saturation (pseudo-pure fluids), or none, runs each point's scalar path.
pub(super) fn sat_lanes(
    fluid: &PureFluid,
    by: SatBy,
    (x, y): (&[f64], &[f64]),
    opts: &FlashOptions,
    out: &mut [Result<State, Error>],
) {
    let checked = |state: Result<State, Error>| state.and_then(|state| above_melting_line(fluid, state, opts));
    let Some(curve) = fluid.saturation().ok().flatten().filter(|curve| curve.accuracy() == SatAccuracy::Exact) else {
        for ((x, y), out) in x.iter().zip(y).zip(out) {
            *out = checked(if by == SatBy::T { qt(fluid, *x, *y, opts) } else { pq(fluid, *x, *y, opts) });
        }
        return;
    };
    for ((x, y), out) in x.chunks(GROUP).zip(y.chunks(GROUP)).zip(out.chunks_mut(GROUP)) {
        let points = sat_points(curve, by, (x, y), opts);
        let bundles = phase_bundles(fluid, &points);
        for ((point, [liquid, vapour]), out) in points.into_iter().zip(bundles).zip(out) {
            let state = point
                .and_then(|(sat, q)| two_phase_from(fluid, &sat, q, Strategy::Superancillary, || liquid, || vapour));
            *out = checked(state.and_then(|state| within_domain(fluid, state, opts)));
        }
    }
}

/// The quality and the saturation of each of up to [`GROUP`] native points, as [`qt`] and [`pq`] take them: the curve's
/// batch hook under `ExecPolicy::Simd`, and at p the root `RootPolicy::Nearest` names ([`curve_at_p`]).
fn sat_points(
    curve: &dyn SaturationCurve,
    by: SatBy,
    (x, y): (&[f64], &[f64]),
    opts: &FlashOptions,
) -> [Result<(SatPair, Quality), Error>; GROUP] {
    let unset = || Err(Error::Shape { expected: GROUP, found: x.len() });
    let mut sats: [Result<SatPair, Error>; GROUP] = core::array::from_fn(|_| unset());
    let (q, at) = if by == SatBy::T { (x, y) } else { (y, x) };
    let called = match sats.get_mut(..at.len()) {
        Some(sats) if by == SatBy::T => curve.at_t_batch(at, sats, ExecPolicy::Simd),
        Some(sats) => curve.at_p_batch(at, sats, ExecPolicy::Simd),
        None => unset().map(|_: SatPair| ()),
    };
    let mut sats = sats.into_iter().zip(at);
    core::array::from_fn(|k| {
        let quality = Quality::new(q.get(k).copied().unwrap_or(f64::NAN))?;
        let (sat, at) = sats.next().ok_or(Error::Shape { expected: GROUP, found: k })?;
        called.clone()?;
        let sat = if by == SatBy::T { sat } else { nearest_at_p(curve, *at, sat, opts) };
        Ok((sat?, quality))
    })
}

/// Both phases' bundles at each point's saturation, liquid then vapour, each [`phase_bundle`]'s bit for bit: the EOS's
/// batch hooks under `ExecPolicy::Simd`, at each order ([`phase_order`]) and for each phase over that order's points.
fn phase_bundles(
    fluid: &PureFluid,
    points: &[Result<(SatPair, Quality), Error>; GROUP],
) -> [[Result<Bundle, Error>; 2]; GROUP] {
    let unset = || Err(Error::Shape { expected: GROUP, found: 0 });
    let mut bundles = core::array::from_fn(|_| [unset(), unset()]);
    let mut parts = ([Derivs::zero(Order::Two); GROUP], [Derivs::zero(Order::Two); GROUP]);
    for order in [Order::One, Order::Two] {
        for vapour in [false, true] {
            bundles_at(fluid, (order, vapour), points, &mut parts, &mut bundles);
        }
    }
    bundles
}

/// One phase's bundles (the vapour's if `vapour`) at the points of `points` whose order is `order`, into `bundles`;
/// `parts` holds their ideal and residual parts on the way.
fn bundles_at(
    fluid: &PureFluid,
    (order, vapour): (Order, bool),
    points: &[Result<(SatPair, Quality), Error>; GROUP],
    (ideal, residual): &mut ([Derivs; GROUP], [Derivs; GROUP]),
    bundles: &mut [[Result<Bundle, Error>; 2]; GROUP],
) {
    // The points at `order`, packed: each one's index, T and this phase's density.
    let (mut at, mut n) = ([(0, 0.0, 0.0); GROUP], 0);
    for (i, point) in points.iter().enumerate() {
        let Ok((sat, q)) = point else { continue };
        let rho = if vapour { sat.dew.rho } else { sat.bubble.rho };
        if let Some(slot) = at.get_mut(n).filter(|_| phase_order(*q) == order) {
            (*slot, n) = ((i, sat.bubble.t, rho), n + 1);
        }
    }
    let (t, rho) = (at.map(|(_, t, _)| t), at.map(|(_, _, rho)| rho));
    let (Some(t), Some(rho), Some(ideal), Some(residual)) =
        (t.get(..n), rho.get(..n), ideal.get_mut(..n), residual.get_mut(..n))
    else {
        return;
    };
    let eos = fluid.eos();
    let called = eos.ideal_batch(t, rho, order, ideal, ExecPolicy::Simd).and(eos.residual_batch(
        t,
        rho,
        order,
        residual,
        ExecPolicy::Simd,
    ));
    for ((i, ..), (ideal, residual)) in at.iter().zip(ideal.iter().zip(residual.iter())) {
        if let Some(slot) = bundles.get_mut(*i).and_then(|b| b.get_mut(usize::from(vapour))) {
            *slot = called.clone().and_then(|()| phase_bundle_of(*ideal + *residual, order));
        }
    }
}
