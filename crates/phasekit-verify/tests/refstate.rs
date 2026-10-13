//! CoolProp's named reference states (PLAN.md M10.1; map 01 §3, §8): IIR, ASHRAE and NBP, each a gauge that
//! `Fluid::with_reference` resolves by flashing its anchor (E10), against the oracle's `refstate` fixtures and the
//! anchors' defining values.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::FluidRecord;
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

/// A fixture's reference state, its label and its anchor's defining h and s, J/kg and J/(kg K) (map 01 §3).
fn named(label: &str) -> Option<(&'static str, ReferenceState, f64, f64)> {
    match label {
        "IIR" => Some(("IIR", ReferenceState::Iir, 200e3, 1e3)),
        "ASHRAE" => Some(("ASHRAE", ReferenceState::Ashrae, 0.0, 0.0)),
        "NBP" => Some(("NBP", ReferenceState::Nbp, 0.0, 0.0)),
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

/// What [`reference_states_hit_their_check_values`] found: its failures, the reference states the oracle refuses, the
/// states compared and the largest fraction of their bounds they use (h, s).
#[derive(Default)]
struct Found<'a> {
    failures: Failures,
    refused: Vec<(&'a str, &'static str)>,
    states: usize,
    headroom: [f64; 2],
}

/// One fixture row of a reference state the oracle sets, the one before it in the file being its anchor's.
struct Row<'t, 'a> {
    table: &'t Fixture<'a>,
    row: usize,
    name: &'a str,
    label: &'static str,
}

impl<'a> Found<'a> {
    /// A reference state's anchor (definition `defined`, h and s) under each data set, and the oracle's, within
    /// `RefAnchor`.
    fn anchor(&mut self, registries: &[Registry; 2], r: &Row<'_, 'a>, reference: ReferenceState, defined: [f64; 2]) {
        let bound = ToleranceClass::RefAnchor.bound(0.0).unwrap();
        let at = input(r.table, r.row);
        let oracle = [r.table.value(r.row, "h_mass").unwrap(), r.table.value(r.row, "s_mass").unwrap()];
        for set in registries {
            let state = set.get(r.name).unwrap().with_reference(reference).unwrap().state(at).unwrap();
            let got = [state.h(Basis::Mass), state.s(Basis::Mass)];
            for (value, defined) in got.into_iter().zip(defined) {
                let (name, label) = (r.name, r.label);
                let message = || format!("{name} {label} anchor: {value} for {defined}");
                self.failures.check((value - defined).abs() <= bound, message);
            }
        }
        for (value, defined) in oracle.into_iter().zip(defined) {
            let (name, label) = (r.name, r.label);
            let message = || format!("{name} {label}: the oracle's anchor {value} for {defined}");
            self.failures.check((value - defined).abs() <= bound, message);
        }
    }

    /// A saturated state in a reference state, against the oracle's, within `Prop` at it and at its `anchor` input.
    fn state(&mut self, referenced: &Fluid, model: (&FluidRecord, &IdealScale), r: &Row<'_, 'a>, anchor: Input) {
        let at = input(r.table, r.row);
        let state = referenced.state(at).unwrap();
        let bounds = offset_bounds(referenced, model, [at, anchor]);
        for (k, (column, got)) in
            [("h_mass", state.h(Basis::Mass)), ("s_mass", state.s(Basis::Mass))].into_iter().enumerate()
        {
            let checked = r.table.check_bound(r.row, column, got, bounds[k]);
            let ratio = self.failures.ok(checked, |e| format!("{} {}: {e:?}", r.name, r.label));
            self.headroom[k] = self.headroom[k].max(ratio.unwrap_or(0.0));
        }
        self.states += 1;
    }

    /// Every row of one fluid's file.
    fn file(&mut self, registries: &[Registry; 2], (path, text): (&'a str, &'a str)) {
        let table = Fixture::parse(path, text).unwrap();
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".csv");
        let fluid = registries[0].get(name).unwrap();
        let record = phasekit_core::internal::record(&registries[0], name).unwrap();
        let ideal = IdealScale::new(&record).unwrap();
        let mut anchor = None;
        for row in 0..table.rows().len() {
            let (label, reference, h0, s0) = named(table.printed(row, "refstate").unwrap()).unwrap();
            let r = Row { table: &table, row, name, label };
            match (fluid.with_reference(reference), table.printed(row, "status"), table.printed(row, "role")) {
                (Ok(_), Some("ok"), Some("anchor")) => {
                    self.anchor(registries, &r, reference, [h0, s0]);
                    anchor = Some(input(&table, row));
                }
                (Ok(referenced), Some("ok"), _) => self.state(&referenced, (&record, &ideal), &r, anchor.unwrap()),
                (resolved, _, _) if table.printed(row, "status") != Some("ok") => {
                    self.refused.push((name, label));
                    let typed = matches!(resolved, Err(Error::Domain(_)));
                    self.failures.check(typed, || format!("{name} {label}: the oracle refuses it, here {resolved:?}"));
                }
                (resolved, ..) => self.failures.check(false, || format!("{name} {label}: {resolved:?}")),
            }
        }
    }
}

/// Map 01 §8 (`CoolProp-Tests.cpp:2263-2436`, whose fluids n-Propane, R134a and R124 are three of these 17): under
/// `Parity`, each reference state the oracle sets puts its defining h and s at its anchor, saturated liquid at 0 °C
/// (IIR), at −40 °C (ASHRAE) or at 1 atm (NBP), within `RefAnchor` (1e-8 J/kg and J/(kg K), the bound CoolProp's own
/// test uses; the oracle's anchors hold it too), under `Corrected` as well; and its 20 saturated states per reference
/// state match the oracle's within `Prop` at both states the gauge is the difference of. The ten reference states
/// CoolProp refuses ([`REFUSED`]) are typed domain errors here.
#[test]
fn reference_states_hit_their_check_values() {
    let registries = [DataSet::Parity, DataSet::Corrected].map(|set| Registry::from_embedded(set).unwrap());
    let mut found = Found::default();
    for file in REFSTATE_FIXTURES {
        found.file(&registries, file);
    }
    let (failures, headroom) = (&found.failures, found.headroom);
    let shown = failures.iter().take(20).collect::<Vec<_>>();
    assert_eq!(
        shown,
        Vec::<&String>::new(),
        "{} failures; the rest within {headroom:?} of their bounds",
        failures.len()
    );
    assert_eq!(found.refused, REFUSED);
    assert_eq!(found.states, (17 * 3 - REFUSED.len()) * 20);
}
