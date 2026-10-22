//! The 19 × 2 capability matrix's truths and expected tables (VERIFICATION.md §8.5, PLAN.md M7.9), shared by
//! `tests/capability.rs` (the core subset, G3) and `tests/sweeps.rs` (every fluid, nightly).

use phasekit_core::internal::FluidRecord;
use phasekit_core::{Density, Error, Fluid, Input, Pair, Pressure, Prop, Quality, Registry, State, Temperature};
use phasekit_verify::{Outcome, capability_matrix};

/// What a cell must hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    /// `Outcome::Pass`: the truth, recovered within class `Flash`.
    Pass,
    /// `Undefined { prop: Q }`: a single phase has no quality to read off, and PT inside a pseudo-pure fluid's dome,
    /// between its bubble and dew temperatures, has none to give.
    NoQ,
    /// `Ambiguous`, none of whose roots is the truth: PT inside the dome is both saturated sides at once.
    Ambig,
    /// `InvalidInput` of the quality: a pseudo-pure fluid's fractional quality at a temperature, which its two domes
    /// (D4) leave without meaning, as in CoolProp.
    BadQ,
    /// The other dome's (D4): a pseudo-pure fluid's pair that follows its EOS's VLE, of a truth on its ancillaries'
    /// dome, or the reverse, returns that dome's own state (another quality and temperature) or a typed refusal where
    /// that dome has none (its VLE fails just below the published Tc, M7.4); never `Unsupported`.
    Other,
    /// PT on a pseudo-pure fluid's dome edge, at a bubble or dew temperature: that side, or inside the dome
    /// (`Undefined`) as the rounding of the edge's temperature falls.
    Edge,
}

use Want::{Ambig, BadQ, Edge, NoQ, Other, Pass};

/// The truths, in the tables' column order: three single phases at 0.8 Tc and above Tc, and five two-phase states.
const COLUMNS: [&str; 8] = ["liquid", "gas", "supercritical", "Q = 0", "Q = ½", "Q = 1", "Q = ½ by Tc", "DT dome"];

/// The pairs of [`capability_matrix`] calls, as (single-phase, two-phase) columns: every truth in at least one.
pub const CALLS: [(usize, usize); 5] = [(0, 3), (1, 5), (2, 4), (0, 6), (1, 7)];

/// A pure fluid: every pair recovers every truth but the documented refusals. A single phase has no quality, and PT of
/// a two-phase state inside the dome is `Ambiguous` between its saturated sides; at Q = 0 and 1 the truth is one of
/// them.
const PURE: [(Pair, [Want; 8]); 19] = [
    (Pair::QT, [NoQ, NoQ, NoQ, Pass, Pass, Pass, Pass, Pass]),
    (Pair::PQ, [NoQ, NoQ, NoQ, Pass, Pass, Pass, Pass, Pass]),
    (Pair::QS, [NoQ, NoQ, NoQ, Pass, Pass, Pass, Pass, Pass]),
    (Pair::HQ, [NoQ, NoQ, NoQ, Pass, Pass, Pass, Pass, Pass]),
    (Pair::DQ, [NoQ, NoQ, NoQ, Pass, Pass, Pass, Pass, Pass]),
    (Pair::PT, [Pass, Pass, Pass, Pass, Ambig, Pass, Ambig, Ambig]),
    (Pair::DT, [Pass; 8]),
    (Pair::HT, [Pass; 8]),
    (Pair::ST, [Pass; 8]),
    (Pair::TU, [Pass; 8]),
    (Pair::DP, [Pass; 8]),
    (Pair::HP, [Pass; 8]),
    (Pair::PS, [Pass; 8]),
    (Pair::PU, [Pass; 8]),
    (Pair::HS, [Pass; 8]),
    (Pair::SU, [Pass; 8]),
    (Pair::DH, [Pass; 8]),
    (Pair::DS, [Pass; 8]),
    (Pair::DU, [Pass; 8]),
];

/// A pseudo-pure fluid (Air, R410A) has two domes (D4): its ancillaries' for PQ, P+X, HS and SU, as CoolProp's PQ and
/// PH, and its EOS's VLE for DT, T+X, DP and D+X. Its two-phase truths are PQ states, on the ancillaries' dome, but the
/// DT one, on the EOS's; each pair recovers the truths of its own dome. QT and the Q-pairs take only Q = 0 and 1.
const PSEUDO_PURE: [(Pair, [Want; 8]); 19] = [
    (Pair::QT, [NoQ, NoQ, NoQ, Pass, BadQ, Pass, BadQ, BadQ]),
    (Pair::PQ, [NoQ, NoQ, NoQ, Pass, Pass, Pass, Pass, Other]),
    (Pair::QS, [NoQ, NoQ, NoQ, Pass, BadQ, Pass, BadQ, BadQ]),
    (Pair::HQ, [NoQ, NoQ, NoQ, Pass, BadQ, Pass, BadQ, BadQ]),
    (Pair::DQ, [NoQ, NoQ, NoQ, Pass, BadQ, Pass, BadQ, BadQ]),
    (Pair::PT, [Pass, Pass, Pass, Edge, NoQ, Edge, Other, NoQ]),
    (Pair::DT, [Pass, Pass, Pass, Pass, Other, Pass, Other, Pass]),
    (Pair::HT, [Pass, Pass, Pass, Pass, Other, Pass, Other, Pass]),
    (Pair::ST, [Pass, Pass, Pass, Pass, Other, Pass, Other, Pass]),
    (Pair::TU, [Pass, Pass, Pass, Pass, Other, Pass, Other, Pass]),
    (Pair::DP, [Pass, Pass, Pass, Pass, Other, Pass, Other, Pass]),
    (Pair::HP, [Pass, Pass, Pass, Pass, Pass, Pass, Pass, Other]),
    (Pair::PS, [Pass, Pass, Pass, Pass, Pass, Pass, Pass, Other]),
    (Pair::PU, [Pass, Pass, Pass, Pass, Pass, Pass, Pass, Other]),
    (Pair::HS, [Pass, Pass, Pass, Pass, Pass, Pass, Pass, Other]),
    (Pair::SU, [Pass, Pass, Pass, Pass, Pass, Pass, Pass, Other]),
    (Pair::DH, [Pass, Pass, Pass, Pass, Other, Pass, Other, Pass]),
    (Pair::DS, [Pass, Pass, Pass, Pass, Other, Pass, Other, Pass]),
    (Pair::DU, [Pass, Pass, Pass, Pass, Other, Pass, Other, Pass]),
];

/// Cells where a fluid departs from its table (fluid, pair, column, what the cell holds). SES36's ancillaries and EOS
/// disagree far more than the other pseudo-pure fluids': its dew state at 0.8 Tc, whose density is the EOS's at the
/// dew ancillary's (T, p), lies inside its EOS's dome (DT there gives Q = 0.88), and PT at its EOS's saturation lies
/// outside its ancillaries' dome. So its EOS-family pairs see the vapour truth as the other dome's, as PT the DT truth;
/// by its published pc its PU, HS and SU are refused or land at another quality; and its HS of the saturated liquid
/// lands at Q = 0.03, a second root, as on an ancillary dome T rises with q at fixed p and dh = v·dp, which makes HS
/// single-rooted on an isentrope (M7.8), does not hold.
const EXCEPTIONS: [(&str, Pair, usize, Want); 13] = [
    ("SES36", Pair::DT, 5, Other),
    ("SES36", Pair::HT, 5, Other),
    ("SES36", Pair::ST, 5, Other),
    ("SES36", Pair::TU, 5, Other),
    ("SES36", Pair::DP, 5, Other),
    ("SES36", Pair::DH, 5, Other),
    ("SES36", Pair::DS, 5, Other),
    ("SES36", Pair::DU, 5, Other),
    ("SES36", Pair::HS, 3, Other),
    ("SES36", Pair::PU, 6, Other),
    ("SES36", Pair::HS, 6, Other),
    ("SES36", Pair::SU, 6, Other),
    ("SES36", Pair::PT, 7, Other),
];

/// The eight truths of [`COLUMNS`]. At 0.8 Tc the saturation pressure p_sat and, at p_sat, the bubble temperature T_sat
/// (a pure fluid's is 0.8 Tc): the liquid at 2·p_sat and the gas at 0.5·p_sat, both at T_sat; the supercritical state
/// at 2·pc, halfway from Tc to Tmax. A pressure or temperature beyond its bound is halfway to it instead, and at most
/// 0.99 of it (HFE143m's pmax is 1.98·pc, R161's is below its pc, R236EA's Tmax below its Tc). In the dome a pure
/// fluid's QT at 0.8 Tc and at Q = ½ 0.05 K below its curve's top (ROT-086), or where the domain ends below that, at
/// 0.999 Tmax or the temperature of PQ at 0.999 pmax (R236EA's Tmax is below its Tc, R161's pmax below its pc); a
/// pseudo-pure fluid's PQ at p_sat and at 0.999·pc; and DT at ρc and 0.8 Tc.
fn truths(fluid: &Fluid, record: &FluidRecord) -> Result<[State; 8], String> {
    let c = fluid.info().published_critical().ok_or("no critical point")?;
    let state = |input: Input| fluid.state(input).map_err(|e| format!("truth {input:?}: {e:?}"));
    let pt = |p: f64, t: f64| state(Input::pt(Pressure::new(p).unwrap(), Temperature::new(t).unwrap()));
    let qt = |q: f64, t: f64| state(Input::qt(Quality::new(q).unwrap(), Temperature::new(t).unwrap()));
    let pq = |p: f64, q: f64| state(Input::pq(Pressure::new(p).unwrap(), Quality::new(q).unwrap()));
    let (p_max, t_1) = (record.limits.p_max(), 0.8 * c.t);
    let p_sat = qt(0.0, t_1)?.p();
    let t_sat = pq(p_sat, 0.0)?.t();
    let below_p_max = |p: f64, from: f64| p.min(0.5 * (from + p_max)).min(0.99 * p_max);
    let t_high = (0.5 * (c.t + record.limits.t_max())).min(0.99 * record.limits.t_max());
    let p_high = below_p_max(2.0 * c.p, c.p);
    let single = [pt(below_p_max(2.0 * p_sat, p_sat), t_sat)?, pt(0.5 * p_sat, t_sat)?, pt(p_high, t_high)?];
    let dome = Input::dt(Density::molar(c.rho).unwrap(), Temperature::new(t_1).unwrap());
    let two_phase = match record.superancillary_curve() {
        Some(curve) => {
            let near_t = (curve.t_range().1 - 0.05).min(0.999 * record.limits.t_max());
            let near = qt(0.5, near_t).or_else(|_| qt(0.5, pq(0.999 * p_max, 0.5)?.t()))?;
            [qt(0.0, t_1)?, qt(0.5, t_1)?, qt(1.0, t_1)?, near, state(dome)?]
        }
        None => [pq(p_sat, 0.0)?, pq(p_sat, 0.5)?, pq(p_sat, 1.0)?, pq(0.999 * c.p, 0.5)?, state(dome)?],
    };
    let [l, g, s] = single;
    let [q0, q5, q1, nc, dt] = two_phase;
    Ok([l, g, s, q0, q5, q1, nc, dt])
}

/// Whether a cell holds what its table wants.
fn holds(outcome: &Outcome, want: Want) -> bool {
    let no_quality = |e: &Error| matches!(e, Error::Undefined { prop: Prop::Q, .. });
    match (want, outcome) {
        (Pass | Edge, Outcome::Pass) => true,
        (NoQ | Edge, Outcome::Refused(e)) => no_quality(e),
        (Ambig, Outcome::Refused(e)) => matches!(e, Error::Ambiguous { .. }),
        (BadQ, Outcome::Refused(e)) => {
            matches!(e, Error::InvalidInput { quantity: phasekit_core::Quantity::QUALITY, .. })
        }
        (Other, Outcome::Fail(_) | Outcome::Refused(_)) => true,
        _ => false,
    }
}

/// The cells of one fluid's matrices that do not hold what its table wants, and the number of cells compared.
pub fn mismatches(registry: &Registry, name: &str) -> (Vec<String>, usize) {
    let fluid = registry.get(name).unwrap();
    let record = phasekit_core::internal::record(registry, name).unwrap();
    let table = if record.pseudo_pure.is_some() { &PSEUDO_PURE } else { &PURE };
    let truths = match truths(fluid, &record) {
        Ok(truths) => truths,
        Err(e) => return (vec![format!("{name}: {e}")], 0),
    };
    let (mut bad, mut compared) = (Vec::new(), 0);
    for (single, two_phase) in CALLS {
        let matrix = capability_matrix(fluid, [&truths[single], &truths[two_phase]]);
        for ((pair, cells), (want_pair, wants)) in matrix.iter().zip(table) {
            assert_eq!(pair, want_pair);
            for (cell, column) in cells.iter().zip([single, two_phase]) {
                compared += 1;
                bad.extend(mismatch(name, *pair, column, cell, wants));
            }
        }
    }
    (bad, compared)
}

/// The cell of `pair` and the truth in `column`, if it does not hold what its table's row (`wants`), or an exception
/// of `name`'s, wants: what it holds and what is wanted.
fn mismatch(name: &str, pair: Pair, column: usize, cell: &Outcome, wants: &[Want; 8]) -> Option<String> {
    let exception = EXCEPTIONS.iter().find(|e| (e.0, e.1, e.2) == (name, pair, column));
    let want = exception.map_or(wants[column], |e| e.3);
    (!holds(cell, want)).then(|| format!("{name} {pair:?} {}: {cell:?}, want {want:?}", COLUMNS[column]))
}
