//! Inside a mixture's envelope (PLAN.md M13.10): QT and PQ at 0 < Q < 1 and DT, judged by the mass balance and the
//! fugacities (ROT-182), by a DT at a split's bulk density returning that split, and against the oracle's `mix_flash`.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::BTreeMap;

use phasekit_core::{Basis, DataSet, Error, FlashOptions, Fluid, Input, Pair, Phase, Registry, State};
use phasekit_mix::Given;
use phasekit_verify::register::{RowKey, exempt_row};
use phasekit_verify::{DIVERGENCES, Kind, Tolerance, ToleranceClass};

#[path = "common/mix_flash.rs"]
mod mix_flash;
#[path = "common/mix_sat.rs"]
mod mix_sat;
use mix_flash::{FlashRow, binary, mixture, rows_of};

fn registry() -> Registry {
    Registry::from_embedded(DataSet::Parity).unwrap()
}

/// Each row's binary, built once: a mixture finds its critical points on first use and keeps them (its single phases'
/// labels read them), which in the unoptimised test build takes about a second.
struct Fluids {
    registry: Registry,
    built: BTreeMap<String, Fluid>,
}

impl Fluids {
    fn new() -> Fluids {
        Fluids { registry: registry(), built: BTreeMap::new() }
    }

    fn of(&mut self, row: &FlashRow) -> &Fluid {
        let key = format!("{} {}", row.label, row.z1);
        self.built.entry(key).or_insert_with(|| binary(&self.registry, &row.label, row.z1))
    }
}

/// The flash of `fluid` by `pair` at the row's inputs, its quality molar or by mass.
fn flash(fluid: &Fluid, row: &FlashRow, basis: Basis) -> Result<State, Error> {
    let input = match row.pair.as_str() {
        "PT" => Input::new(Pair::PT, row.p, row.t, Basis::Molar)?,
        "QT" => Input::new(Pair::QT, row.q, row.t, Basis::Molar)?.with_quality_basis(basis),
        "PQ" => Input::new(Pair::PQ, row.p, row.q, Basis::Molar)?.with_quality_basis(basis),
        _ => Input::new(Pair::DT, row.rho, row.t, Basis::Molar)?,
    };
    fluid.flash(input, &FlashOptions::new())
}

/// Whether a row lies below its mixture's T_min (CO₂'s triple point in CO₂/Methane; a methane-rich bubble point below
/// n-butane's or methane's T_min).
fn below_t_min(fluid: &Fluid, t: f64) -> bool {
    t < fluid.model().limits().unwrap().t_min()
}

/// ROT-182: at the QT and PQ of the oracle's grid (Q 0.25, 0.5 and 0.75 at each binary's two T and at √(p_bub p_dew)),
/// within the mixtures' range, the two phases phasekit returns add up to the feed within `Identity`, their fugacities
/// at (T, p) are equal within `Identity` and each phase's density reproduces p by core's acceptance gate. The flash's
/// state is the split: Q, T and p. At z₁ = 0.5 a mass quality Q gives the state of mass quality Q.
#[test]
fn interior_q_conserves_mass_and_fugacity() {
    let mut fluids = Fluids::new();
    let mut solved = 0;
    for row in rows_of("QT").into_iter().chain(rows_of("PQ")) {
        let fluid = fluids.of(&row);
        if below_t_min(fluid, row.t) {
            continue;
        }
        let mix = mixture(fluid);
        let given = if row.pair == "QT" { Given::T(row.t) } else { Given::P(row.p) };
        let at = format!("{} {} {} at Q {}", row.label, row.z1, row.pair, row.q);
        let c = mix.coexisting(row.q, given).unwrap_or_else(|e| panic!("{at}: {e:?}"));
        for (i, z) in mix.composition().iter().enumerate() {
            let total = (1.0 - c.beta) * c.x[i] + c.beta * c.y[i];
            assert!((total - z).abs() <= ToleranceClass::Identity.bound(1.0).unwrap(), "{at}: {i}: {total} for {z}");
        }
        let differences = mix_sat::fugacity_differences(mix, c.t, c.p, (&c.x, c.rho_liquid), (&c.y, c.rho_vapour));
        assert!(differences.iter().all(|(d, bound)| d.abs() <= *bound), "{at}: {differences:?}");
        for (fractions, rho) in [(&c.x, c.rho_liquid), (&c.y, c.rho_vapour)] {
            let p =
                phasekit_mix::MixtureModel::composition(mix.model(), c.t, rho, fractions, phasekit_core::Order::One)
                    .unwrap()
                    .pressure();
            assert!((p / c.p - 1.0).abs() <= 1e-11, "{at}: p({rho}) = {p}");
        }
        let state = flash(fluid, &row, Basis::Molar).unwrap();
        assert_eq!((state.phase(), state.quality(), state.t(), state.p()), (Phase::TwoPhase, Some(row.q), c.t, c.p));
        if row.z1 == 0.5 {
            let by_mass = flash(fluid, &row, Basis::Mass).unwrap();
            let q = by_mass.mass_quality().unwrap();
            let bound = ToleranceClass::Identity.bound(1.0).unwrap();
            assert!((q - row.q).abs() <= bound && by_mass.quality() != Some(row.q), "{at}: {q}");
        }
        solved += 1;
    }
    assert!(solved >= 250, "{solved}");
}

/// A DT at a two-phase state's (T, ρ) returns its split: at each binary's √(p_bub p_dew) at 50/50 and 0.7 T_r (its lower
/// T), phasekit's PT split gives a bulk density whose DT is two-phase with the split's p and Q within `Flash`; at 1.1 p_bub
/// and 0.9 p_dew the single phase's density gives that phase, at its pressure.
#[test]
fn dt_in_the_dome_is_the_split() {
    let mut fluids = Fluids::new();
    let (mut splits, mut singles) = (0, 0);
    let rows = rows_of("PT");
    let lowest: BTreeMap<String, f64> = rows.iter().fold(BTreeMap::new(), |mut m, r| {
        let key = format!("{} {}", r.label, r.z1);
        let t = m.entry(key).or_insert(f64::INFINITY);
        *t = t.min(r.t);
        m
    });
    for row in rows.iter().filter(|r| r.z1 == 0.5 && r.t == lowest[&format!("{} {}", r.label, r.z1)]) {
        let fluid = fluids.of(row);
        if below_t_min(fluid, row.t) {
            continue;
        }
        let Ok(pt) = flash(fluid, row, Basis::Molar) else { continue };
        let at = format!("{} {} at {} K, {} Pa", row.label, row.z1, row.t, row.p);
        let dt_row = FlashRow { pair: "DT".to_string(), rho: pt.rho(Basis::Molar), ..row.clone() };
        let dt = flash(fluid, &dt_row, Basis::Molar).unwrap_or_else(|e| panic!("{at}: {e:?}"));
        let flash_bound = |want: f64| ToleranceClass::Flash.bound(want.abs()).unwrap();
        assert!((dt.p() - pt.p()).abs() <= flash_bound(pt.p()), "{at}: DT p {} against {}", dt.p(), pt.p());
        match pt.quality() {
            Some(q) => {
                let got = dt.quality().unwrap_or(f64::NAN);
                assert!(dt.phase() == Phase::TwoPhase && (got - q).abs() <= flash_bound(q), "{at}: Q {got} for {q}");
                splits += 1;
            }
            None => {
                assert_eq!((dt.phase(), dt.quality()), (pt.phase(), None), "{at}");
                singles += 1;
            }
        }
    }
    assert!(splits >= 8 && singles >= 16, "{splits} {singles}");
}

/// The oracle's measured bound for its step-ended Newton (DIV-0060).
fn measured() -> f64 {
    match DIVERGENCES.iter().find(|d| d.id == "DIV-0060").unwrap().tolerance {
        Some(Tolerance::Relative(x)) => x,
        _ => f64::NAN,
    }
}

/// The offsets of phasekit's state from a QT, PQ or DT row: T, p, the bulk density and Q (relative for a split, and
/// one phase against the oracle's −1 as 0 or 1).
fn offsets(state: &State, row: &FlashRow) -> [f64; 4] {
    let off = |got: f64, want: f64| ((got - want) / want).abs();
    let q = state.quality().unwrap_or(-1.0);
    let q_off = if row.q > 0.0 { off(q, row.q) } else { (q - row.q).abs() };
    [off(state.t(), row.t), off(state.p(), row.p), off(state.rho(Basis::Molar), row.rho), q_off]
}

/// Whether (T, ρ) lies inside the dome: 1/ρ between the bubble point's liquid and the dew point's vapour at T.
fn inside_dome(mix: &phasekit_mix::Mixture, t: f64, rho: f64) -> bool {
    let point = |side| mix.saturation(side, Given::T(t)).map(|s| s.rho).unwrap();
    let (liquid, vapour) = (point(phasekit_mix::Side::Bubble), point(phasekit_mix::Side::Dew));
    vapour < rho && rho < liquid
}

/// Oracle: CoolProp 8.0.0's QT and PQ at Q 0.25, 0.5 and 0.75 and its DT at its own PT rows' densities
/// (fixtures/coolprop-8.0.0/all/mix_flash.csv), on `Parity`: T, p, density and Q within DIV-0060's measured bound (its
/// Newton stops on its step). Skipped: DIV-0061's DT rows inside the dome (its DT returns one phase there), DIV-0062's
/// QT and PQ rows whose own phases do not add up to the feed, DIV-0058's DT row at R134a/R1234yf's azeotrope, and the
/// rows below the mixture's T_min. A DT row is two-phase where its density lies inside the dome at its T.
#[test]
fn quality_and_dt_flashes_match_the_oracle() {
    let mut fluids = Fluids::new();
    let measured = measured();
    let (mut compared, mut skipped, mut below, mut misses) = (0, BTreeMap::new(), 0, Vec::new());
    for row in ["QT", "PQ", "DT"].iter().flat_map(|pair| rows_of(pair)) {
        let fluid = fluids.of(&row);
        if below_t_min(fluid, row.t) {
            below += 1;
            continue;
        }
        let (two_phase, balanced) = if row.pair == "DT" {
            (inside_dome(mixture(fluid), row.t, row.rho), true)
        } else {
            (true, ((1.0 - row.q) * row.x1 + row.q * row.y1 - row.z1).abs() <= 1e-9)
        };
        let key =
            RowKey { t: row.t, input: Some(&row.pair), two_phase, p: Some(row.p), balanced, near_critical: false };
        if let Some(id) = exempt_row(DIVERGENCES, &row.label, Kind::MixFlash, "*", key) {
            *skipped.entry(id).or_insert(0) += 1;
            continue;
        }
        let at = format!("{} {} {} at {} K, {} Pa, Q {}", row.label, row.z1, row.pair, row.t, row.p, row.q);
        match flash(fluid, &row, Basis::Molar) {
            Ok(state) => {
                let o = offsets(&state, &row);
                if o.iter().all(|d| *d <= measured) {
                    compared += 1;
                } else {
                    misses.push(format!("{at}: {o:?}"));
                }
            }
            Err(e) => misses.push(format!("{at}: {e:?}")),
        }
    }
    assert!(misses.is_empty(), "{} misses: {misses:#?}", misses.len());
    let (dt_dome, unbalanced) = (skipped.get("DIV-0061").copied(), skipped.get("DIV-0062").copied());
    assert!(compared >= 391 && dt_dome >= Some(55) && unbalanced >= Some(45), "{compared} {skipped:?} {below}");
}
