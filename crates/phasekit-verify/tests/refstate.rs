//! CoolProp's named reference states (PLAN.md M10.1; map 01 §3, §8): IIR, ASHRAE and NBP, each a gauge that
//! `Fluid::with_reference` resolves by flashing its anchor (E10), against the oracle's `refstate` fixtures and the
//! anchors' defining values.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{Basis, DataSet, Error, Fluid, Input, Pressure, Quality, ReferenceState, Registry, Temperature};
use phasekit_verify::term::IdealScale;
use phasekit_verify::{Fixture, ToleranceClass, fixture};

#[path = "common/caloric.rs"]
mod caloric;
#[path = "common/sweep.rs"]
mod sweep;

use caloric::eos_bound;
use sweep::Failures;

/// The oracle's reference-state files: the core subset and the three fluids of CoolProp's own reference-state test.
const REFSTATE_FIXTURES: [(&str, &str); 17] = [
    fixture!("coolprop-8.0.0/refstate/Air.csv"),
    fixture!("coolprop-8.0.0/refstate/Ammonia.csv"),
    fixture!("coolprop-8.0.0/refstate/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/refstate/HFE143m.csv"),
    fixture!("coolprop-8.0.0/refstate/Helium.csv"),
    fixture!("coolprop-8.0.0/refstate/Methanol.csv"),
    fixture!("coolprop-8.0.0/refstate/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/refstate/R1130(E).csv"),
    fixture!("coolprop-8.0.0/refstate/R1234yf.csv"),
    fixture!("coolprop-8.0.0/refstate/R1234ze(E).csv"),
    fixture!("coolprop-8.0.0/refstate/R124.csv"),
    fixture!("coolprop-8.0.0/refstate/R125.csv"),
    fixture!("coolprop-8.0.0/refstate/R134a.csv"),
    fixture!("coolprop-8.0.0/refstate/R410A.csv"),
    fixture!("coolprop-8.0.0/refstate/Water.csv"),
    fixture!("coolprop-8.0.0/refstate/n-Heptane.csv"),
    fixture!("coolprop-8.0.0/refstate/n-Propane.csv"),
];

/// VERIFICATION.md §3.2, §11.3: the `refstate` files come from the pinned runner image, like every committed oracle
/// fixture.
#[test]
fn refstate_fixtures_name_the_pinned_generator_environment() {
    assert_eq!(phasekit_verify::on_the_pinned_image(REFSTATE_FIXTURES), Ok(17));
}

/// The reference states CoolProp refuses for these fluids, each anchor off the fluid's saturation curve: above its
/// critical (or, pseudo-pure, maximum) temperature, below its triple point, or 1 atm below its triple-point pressure.
const REFUSED: [(&str, &str); 10] = [
    ("Air", "IIR"),
    ("Air", "ASHRAE"),
    ("CarbonDioxide", "NBP"),
    ("HFE143m", "ASHRAE"),
    ("Helium", "IIR"),
    ("Helium", "ASHRAE"),
    ("Nitrogen", "IIR"),
    ("Nitrogen", "ASHRAE"),
    ("Water", "IIR"),
    ("Water", "ASHRAE"),
];

/// A fixture's reference state and its anchor's defining h and s, J/kg and J/(kg K) (map 01 §3).
fn named(label: &str) -> Option<(ReferenceState, f64, f64)> {
    match label {
        "IIR" => Some((ReferenceState::Iir, 200e3, 1e3)),
        "ASHRAE" => Some((ReferenceState::Ashrae, 0.0, 0.0)),
        "NBP" => Some((ReferenceState::Nbp, 0.0, 0.0)),
        _ => None,
    }
}

/// The input of a row: its pair's two inputs in SI units (QT: Q, K; PQ: Pa, Q).
fn input(table: &Fixture<'_>, row: usize) -> Input {
    let (x1, x2) = (table.value(row, "x1").unwrap(), table.value(row, "x2").unwrap());
    match table.printed(row, "pair").unwrap() {
        "PQ" => Input::pq(Pressure::new(x1).unwrap(), Quality::new(x2).unwrap()),
        _ => Input::qt(Quality::new(x1).unwrap(), Temperature::new(x2).unwrap()),
    }
}

/// The h and s bounds, J/kg and J/(kg K), of a reference state's value at `input`: h = h(input) − h(anchor) + h₀, so
/// `Prop`'s bounds on the native h (or s) at both states, with `Term` carried through the relation, add.
fn offset_bounds(
    fluid: &Fluid,
    model: (&phasekit_core::internal::FluidRecord, &IdealScale),
    at: [Input; 2],
) -> [f64; 2] {
    let m = fluid.info().molar_mass();
    let bound = |entropy: bool| {
        let one = |input| {
            let state = fluid.state(input).unwrap();
            let (t, rho) = (state.t(), state.rho(Basis::Molar));
            let want = if entropy { state.s(Basis::Molar) } else { state.h(Basis::Molar) };
            eos_bound(fluid, model, t, rho, entropy, want)
        };
        (one(at[0]) + one(at[1])) / m
    };
    [bound(false), bound(true)]
}

/// Map 01 §8 (`CoolProp-Tests.cpp:2263-2436`, whose fluids n-Propane, R134a and R124 are three of these 17): under
/// `Parity`, each reference state the oracle sets puts its defining h and s at its anchor, saturated liquid at 0 °C
/// (IIR), at −40 °C (ASHRAE) or at 1 atm (NBP), within `RefAnchor` (1e-8 J/kg and J/(kg K), the bound CoolProp's own
/// test uses; the oracle's anchors hold it too), under `Corrected` as well; and its 20 saturated states per reference
/// state match the oracle's within `Prop` at both states the gauge is the difference of. The ten reference states
/// CoolProp refuses ([`REFUSED`]) are typed domain errors here.
#[test]
fn reference_states_hit_their_check_values() {
    let anchor = ToleranceClass::RefAnchor.bound(0.0).unwrap();
    let registries = [DataSet::Parity, DataSet::Corrected].map(|set| Registry::from_embedded(set).unwrap());
    let (mut failures, mut states, mut refused) = (Failures::default(), 0, Vec::new());
    let mut headroom = [0.0_f64; 2];
    for (path, text) in REFSTATE_FIXTURES {
        let table = Fixture::parse(path, text).unwrap();
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".csv");
        let fluid = registries[0].get(name).unwrap();
        let record = phasekit_core::internal::record(&registries[0], name).unwrap();
        let ideal = IdealScale::new(&record).unwrap();
        let mut anchor_at = None;
        for row in 0..table.rows().len() {
            let label = table.printed(row, "refstate").unwrap();
            let (reference, h0, s0) = named(label).unwrap();
            let at = input(&table, row);
            let resolved = fluid.with_reference(reference);
            if table.printed(row, "status") != Some("ok") {
                refused.push((name, label));
                let typed = matches!(resolved, Err(Error::Domain(_)));
                failures.check(typed, || format!("{name} {label}: the oracle refuses it, here {resolved:?}"));
                continue;
            }
            let state = resolved.unwrap().state(at).unwrap();
            let got = [state.h(Basis::Mass), state.s(Basis::Mass)];
            let want = [table.value(row, "h_mass").unwrap(), table.value(row, "s_mass").unwrap()];
            if table.printed(row, "role") == Some("anchor") {
                anchor_at = Some(at);
                for set in &registries {
                    let state = set.get(name).unwrap().with_reference(reference).unwrap().state(at).unwrap();
                    for (value, defined) in [(state.h(Basis::Mass), h0), (state.s(Basis::Mass), s0)] {
                        let off = (value - defined).abs() > anchor;
                        failures.check(!off, || format!("{name} {label} anchor: {value} for {defined} ({set:?})"));
                    }
                }
                for (value, defined) in [(want[0], h0), (want[1], s0)] {
                    let off = (value - defined).abs() > anchor;
                    failures.check(!off, || format!("{name} {label}: the oracle's anchor {value} for {defined}"));
                }
                continue;
            }
            let bounds = offset_bounds(fluid, (&record, &ideal), [at, anchor_at.unwrap()]);
            for (column, k) in [("h_mass", 0), ("s_mass", 1)] {
                let checked = table.check_bound(row, column, got[k], bounds[k]);
                let ratio = failures.ok(checked, |e| format!("{name} {label}: {e:?}"));
                headroom[k] = headroom[k].max(ratio.unwrap_or(0.0));
            }
            states += 1;
        }
    }
    let shown = failures.iter().take(20).collect::<Vec<_>>();
    assert_eq!(
        shown,
        Vec::<&String>::new(),
        "{} failures; the rest within {headroom:?} of their bounds",
        failures.len()
    );
    assert_eq!(refused, REFUSED);
    assert_eq!(states, (17 * 3 - REFUSED.len()) * 20);
}
