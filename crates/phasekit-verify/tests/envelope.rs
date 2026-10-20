//! A mixture's phase envelope (PLAN.md M13.12): every point a bubble or a dew point by M13.9's gate (ROT-185), M13.7's
//! critical point on the curve, typed ends within bounds where v8.0.0 runs away (ROT-188), and the oracle's
//! `build_phase_envelope`.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Fluid, Order, Registry, math};
use phasekit_mix::{Envelope, EnvelopeEnd, EnvelopeOptions, Given, Mixture, MixtureModel, Side};
use phasekit_verify::register::{RowKey, exempt_row};
use phasekit_verify::{DIVERGENCES, Kind, Tolerance};

#[path = "common/mix_env.rs"]
mod mix_env;
#[path = "common/mix_sat.rs"]
mod mix_sat;
use mix_env::{EnvRow, env_rows};
use mix_sat::{fugacity_differences, mixture, row_fluid};

fn registry() -> Registry {
    Registry::from_embedded(DataSet::Parity).unwrap()
}

/// The oracle's ten binaries (`MIX_CRIT_PAIRS`) and their compositions.
const PAIRS: [&str; 10] = [
    "Methane&Ethane",
    "Methane&n-Propane",
    "Methane&n-Butane",
    "Ethane&n-Propane",
    "Nitrogen&Methane",
    "CarbonDioxide&Methane",
    "R32&R125",
    "R32&R134a",
    "R125&R134a",
    "R134a&R1234yf",
];
const Z1: [f64; 3] = [0.25, 0.5, 0.75];

/// The envelope of `fluid`'s mixture under `opts`.
fn envelope(fluid: &Fluid, opts: &EnvelopeOptions) -> Envelope {
    mixture(fluid).envelope(opts).unwrap()
}

/// Point k's liquid and vapour as M13.9's gate judges them: the feed's fractions at the bulk density and the incipient
/// phase's at its own, the denser of them the liquid.
fn phases(mix: &Mixture, env: &Envelope, k: usize) -> ((Vec<f64>, f64), (Vec<f64>, f64)) {
    let feed = (mix.composition().to_vec(), env.rho_bulk[k]);
    let incipient = (env.incipient.iter().map(|x| x[k]).collect(), env.rho_incipient[k]);
    if env.side[k] == Side::Bubble { (feed, incipient) } else { (incipient, feed) }
}

/// Where point k of `env` fails M13.9's gate: each phase's density reproduces p by core's acceptance gate, and the
/// fugacities agree within `Identity`; the side names the denser phase as the liquid.
fn off_gate(mix: &Mixture, env: &Envelope, k: usize) -> Option<String> {
    let (t, p) = (env.t[k], env.p[k]);
    let ((x, rho_l), (y, rho_v)) = phases(mix, env, k);
    if rho_l <= rho_v {
        return Some(format!(
            "point {k} at {t} K, {p} Pa: {:?} with the liquid at {rho_l}, the vapour at {rho_v}",
            env.side[k]
        ));
    }
    for (fractions, rho) in [(&x, rho_l), (&y, rho_v)] {
        let c = mix.model().composition(t, rho, fractions, Order::One).unwrap();
        let scale = (rho * mix.model().gas_constant() * t * (1.0 + (c.z() - 1.0).abs())).max(-c.dp_dv() / rho);
        if (c.pressure() - p).abs() > 1e-12 * scale {
            return Some(format!("point {k} at {t} K, {p} Pa: p({rho}) = {}", c.pressure()));
        }
    }
    let differences = fugacity_differences(mix, t, p, (&x, rho_l), (&y, rho_v));
    let off = differences.iter().position(|(d, bound)| d.abs() > *bound)?;
    Some(format!("point {k} at {t} K, {p} Pa: ln f_{off} differs by {:e}", differences[off].0))
}

/// The mixtures the envelope tests trace: the ten binaries at each composition, two predefined blends and a GERG-2008
/// natural gas, each with its label.
fn mixtures(registry: &Registry) -> Vec<(String, Fluid)> {
    let mut out: Vec<(String, Fluid)> = PAIRS
        .iter()
        .flat_map(|pair| Z1.iter().map(move |z1| (format!("{pair} {z1}"), *pair, *z1)))
        .map(|(label, pair, z1)| (label, row_fluid(registry, pair, z1)))
        .collect();
    for blend in ["R407C.mix", "R410A.mix"] {
        out.push((blend.to_string(), row_fluid(registry, blend, f64::NAN)));
    }
    let gas = [("Methane", 0.85), ("Ethane", 0.07), ("n-Propane", 0.03), ("Nitrogen", 0.03), ("CarbonDioxide", 0.02)];
    out.push(("GERG-2008 gas".to_string(), phasekit_mix::gerg2008(registry, &gas).unwrap()));
    out
}

/// ROT-185 along the whole curve: the envelopes of the ten binaries at three compositions, R407C, R410A and a
/// GERG-2008 natural gas, from 1 bar (or the model's lowest temperature) through the critical point, each close or
/// end at a bound of the model's range, in at least 10 points within its temperature range, every one a bubble or a dew
/// point by M13.9's gate. A closed trace turns from one side to the other once, at the critical point it reports, and its
/// last step crosses its starting pressure; one at a bound turns at most once.
#[test]
fn every_envelope_point_is_a_bubble_or_dew_point() {
    let registry = registry();
    let mut failures = Vec::new();
    for (label, fluid) in mixtures(&registry) {
        let env = envelope(&fluid, &EnvelopeOptions::new());
        let mix = mixture(&fluid);
        if env.len() < 10 || !matches!(env.end, EnvelopeEnd::Closed | EnvelopeEnd::Bound(_)) {
            failures.push(format!("{label}: {} points, {:?}", env.len(), env.end));
            continue;
        }
        let turns = env.side.windows(2).filter(|w| w[0] != w[1]).count();
        let closed = env.end == EnvelopeEnd::Closed;
        if turns > 1 || (closed && (turns != 1 || env.critical.is_none())) {
            failures.push(format!("{label}: {:?}, turns {turns} times, critical {:?}", env.end, env.critical));
        }
        let limits = fluid.model().limits().unwrap();
        let inside = env.t.iter().all(|t| (limits.t_min()..=limits.t_max()).contains(t));
        let last = env.len() - 1;
        let back_at_start = !closed || (env.p[last] <= env.p[0] && env.p[last - 1] > env.p[0]);
        if !inside || !back_at_start {
            failures.push(format!("{label}: inside the T range {inside}, back at the start {back_at_start}"));
        }
        failures.extend((0..env.len()).filter_map(|k| off_gate(mix, &env, k).map(|e| format!("{label}: {e}"))));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// M13.7's critical point lies on the envelope: for each mixture whose critical point lies within the model's range,
/// the trace passes it, and (T, p) interpolated at ln K = 0 is `ThermoModel::critical_point`'s within the accuracy GERG
/// TM15 §7.7.1 states for the method's interpolated points, 0.02 K and 2 kPa (measured: 0.42 mK and 27 Pa, CO₂/methane
/// at 50/50). Methane/n-butane at 0.75 has its critical point at 14.4 MPa, above n-butane's 12 MPa, and its trace ends
/// there.
#[test]
fn the_critical_point_lies_on_the_envelope() {
    let registry = registry();
    let (mut failures, mut beyond) = (Vec::new(), Vec::new());
    for (label, fluid) in mixtures(&registry) {
        let env = envelope(&fluid, &EnvelopeOptions::new());
        let c = fluid.model().critical_point().unwrap();
        if c.p > fluid.model().limits().unwrap().p_max() {
            beyond.push(label);
            continue;
        }
        match env.critical {
            Some((t, p)) if (t - c.t).abs() <= 0.02 && (p - c.p).abs() <= 2e3 => {}
            other => failures.push(format!("{label}: {other:?} against ({}, {})", c.t, c.p)),
        }
    }
    assert!(failures.is_empty() && beyond == ["Methane&n-Butane 0.75"], "{failures:#?} {beyond:?}");
}

/// #256: a trace that starts at the model's lowest temperature, where both the bubble and the dew point at 1 bar lie
/// below it, runs up in temperature into the range: CO₂/N₂ at 50/50 and 90/10 (CO₂'s triple point, 216.592 K) trace their
/// bubble side from there, through M13.7's critical point (within TM15's 0.02 K and 2 kPa), and back down their dew side
/// until it leaves the range below T_min, in at least 10 points, every one a bubble or a dew point by M13.9's gate.
#[test]
fn traces_from_the_lowest_temperature_run_into_the_range() {
    let registry = registry();
    let mut failures = Vec::new();
    for z1 in [0.5, 0.9] {
        let fluid = phasekit_mix::mixture(&registry, &[("CarbonDioxide", z1), ("Nitrogen", 1.0 - z1)], DataSet::Parity)
            .unwrap();
        let (env, mix) = (envelope(&fluid, &EnvelopeOptions::new()), mixture(&fluid));
        let (t_min, c) = (fluid.model().limits().unwrap().t_min(), fluid.model().critical_point().unwrap());
        let label = format!("CO₂/N₂ {z1}: {} points, {:?}, critical {:?}", env.len(), env.end, env.critical);
        let started = env.len() >= 10 && env.side[0] == Side::Bubble && (env.t[0] / t_min - 1.0).abs() <= 1e-12;
        let ended = matches!(env.end, EnvelopeEnd::Bound(phasekit_core::DomainError::BelowMinTemperature { .. }))
            && env.side[env.len() - 1] == Side::Dew;
        let through = env.critical.is_some_and(|(t, p)| (t - c.t).abs() <= 0.02 && (p - c.p).abs() <= 2e3);
        if !(started && ended && through) {
            failures.push(format!("{label} against ({}, {})", c.t, c.p));
        }
        failures.extend((0..env.len()).filter_map(|k| off_gate(mix, &env, k).map(|e| format!("{label}: {e}"))));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// ROT-188 (map 04 #15): where v8.0.0's tracer fails at its start (CO₂/water at 0.9/0.1) or runs to 6.8 GPa (methane/
/// n-decane), 21 GPa (hydrogen/methane) or 23 GPa and 7135 K (N₂/C1/C2/C3), phasekit's ends typed: every point within
/// the model's temperature range and below the stated 100 MPa, each a bubble or a dew point by M13.9's gate, and a
/// trace that reaches 100 MPa ends there, at that bound.
#[test]
fn runaway_envelopes_end_typed_within_bounds() {
    let registry = registry();
    let hard: [&[(&str, f64)]; 4] = [
        &[("CarbonDioxide", 0.9), ("Water", 0.1)],
        &[("Methane", 0.5), ("n-Decane", 0.5)],
        &[("Hydrogen", 0.5), ("Methane", 0.5)],
        &[("Nitrogen", 0.25), ("Methane", 0.25), ("Ethane", 0.25), ("n-Propane", 0.25)],
    ];
    let p_max = 1e8;
    let mut failures = Vec::new();
    for parts in hard {
        let fluid = phasekit_mix::mixture(&registry, parts, DataSet::Parity).unwrap();
        let env = envelope(&fluid, &EnvelopeOptions::new().with_max_pressure(p_max));
        let mix = mixture(&fluid);
        let limits = fluid.model().limits().unwrap();
        let label = format!("{parts:?}: {} points, {:?}", env.len(), env.end);
        let inside =
            env.t.iter().all(|t| (limits.t_min()..=limits.t_max()).contains(t)) && env.p.iter().all(|p| *p <= p_max);
        let bounded = !matches!(env.end, EnvelopeEnd::Bound(phasekit_core::DomainError::AboveMaxPressure { .. }))
            || env.p.iter().any(|p| *p > 0.5 * p_max);
        if env.is_empty() || !inside || !bounded {
            failures.push(label.clone());
        }
        failures.extend((0..env.len()).filter_map(|k| off_gate(mix, &env, k).map(|e| format!("{label}: {e}"))));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The seed of M13.9's Newton at the oracle's point k: its side, what it is given (its T, else its p where the curve
/// between its neighbours is steeper in ln p than in ln T) and its ln K.
fn oracle_seed(rows: &[EnvRow], k: usize) -> (Side, Given, Vec<f64>) {
    let row = &rows[k];
    let side = if row.q == 1.0 { Side::Dew } else { Side::Bubble };
    let (z, incipient) = ([row.z1, 1.0 - row.z1], [row.x1, 1.0 - row.x1]);
    let (y, x) = if side == Side::Bubble { (incipient, z) } else { (z, incipient) };
    let ln_k = y.iter().zip(x).map(|(y, x)| math::ln(y / x)).collect();
    let neighbours = [k.saturating_sub(1), (k + 1).min(rows.len() - 1)].map(|j| &rows[j]);
    let steep = (math::ln(neighbours[1].p / neighbours[0].p) / math::ln(neighbours[1].t / neighbours[0].t)).abs() > 1.0;
    let given = if steep { Given::P(row.p) } else { Given::T(row.t) };
    (side, given, ln_k)
}

/// The oracle's `build_phase_envelope` for the ten binaries at three compositions: each of its points within the
/// mixture's temperature range, the seed of M13.9's Newton at its T (at its p where the curve is steeper in ln p than in
/// ln T), is phasekit's bubble or dew point within DIV-0060's measured bound in the other of T and p, both densities
/// and the incipient fraction. Skipped: the 525 points below the mixtures' T_min (v8.0.0's trace runs to 52 K), and
/// DIV-0063's 82 near the critical point, whose phases lie within 0.1 in |ln ρ|.
#[test]
fn envelopes_match_the_oracle() {
    let registry = registry();
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0060").unwrap();
    let Some(Tolerance::Relative(measured)) = entry.tolerance else { panic!("DIV-0060 has no relative bound") };
    let rows = env_rows();
    let (mut misses, mut compared, mut below, mut near) = (Vec::new(), 0, 0, 0);
    for (k, row) in rows.iter().enumerate() {
        let fluid = row_fluid(&registry, &row.label, row.z1);
        if row.t < fluid.model().limits().unwrap().t_min() {
            below += 1;
            continue;
        }
        let key = RowKey {
            t: row.t,
            input: None,
            two_phase: false,
            p: Some(row.p),
            balanced: true,
            near_critical: row.near_critical(),
        };
        if exempt_row(DIVERGENCES, &row.label, Kind::MixEnv, "*", key) == Some("DIV-0063") {
            near += 1;
            continue;
        }
        let (side, given, ln_k) = oracle_seed(&rows, k);
        let found = phasekit_mix::internal::saturation_from(mixture(&fluid), side, given, (row.t, row.p, ln_k));
        compared += 1;
        let Ok(s) = found else {
            misses.push(format!("{} {} point {k} at {} K, {} Pa: {found:?}", row.label, row.z1, row.t, row.p));
            continue;
        };
        let pairs = [
            (s.t, row.t),
            (s.p, row.p),
            (s.rho, row.rho_bulk),
            (s.rho_incipient, row.rho_incipient),
            (s.incipient[0], row.x1),
        ];
        let offsets: Vec<f64> = pairs.iter().map(|(got, want)| ((got - want) / want).abs()).collect();
        if offsets.iter().any(|d| *d > measured) {
            misses
                .push(format!("{} {} point {k} {side:?} at {} K, {} Pa: {offsets:?}", row.label, row.z1, row.t, row.p));
        }
    }
    let counts = (compared, below, near);
    assert!(misses.is_empty() && counts == (1451, 525, 82), "{} of {counts:?} miss: {misses:#?}", misses.len());
}
