//! The cubic family against CoolProp 8.0.0 (PLAN.md M11.4; map 06 §8): phasekit-cubic's SRK and PR at the `cubic`
//! kind's 8 supercritical states of each of v8.0.0's 116 cubic fluids, α^r and its A_ij to order 2, p, h, u, g, a, c_v,
//! c_p, w and c_p⁰ (never entropy, DIV-0010). On `Parity` (v8.0.0's Ω literals, DIV-0047) every cell matches within
//! 1e-12 (h, u, g and a scaled by RT), except DIV-0048's fluids, whose library T_c, p_c and ω drifted from the canonical
//! record, and DIV-0049's, whose library α⁰ did.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::BTreeMap;

use phasekit_core::{Basis, DataSet, DomainPolicy, FlashOptions, Fluid, Input, Order, Pair, Prop, Registry, State};
use phasekit_cubic::{Kind, R_CODATA, with_cubics};
use phasekit_verify::register::exempt_at;
use phasekit_verify::{Cell, DIVERGENCES, Fixture, Kind as FixtureKind, fixture};

/// The residual columns and their (i, j).
const RESIDUAL: [(&str, usize, usize); 6] =
    [("alphar", 0, 0), ("A10", 1, 0), ("A01", 0, 1), ("A20", 2, 0), ("A11", 1, 1), ("A02", 0, 2)];

/// The property columns.
const PROPS: [(&str, Prop); 9] = [
    ("p", Prop::P),
    ("hmolar", Prop::Hmolar),
    ("umolar", Prop::Umolar),
    ("gibbsmolar", Prop::Gmolar),
    ("helmholtzmolar", Prop::Helmholtzmolar),
    ("cvmolar", Prop::Cvmolar),
    ("cpmolar", Prop::Cpmolar),
    ("speed_sound", Prop::SpeedOfSound),
    ("cp0molar", Prop::Cp0molar),
];

/// The bound of map 06 §10 Q9: the cubic against the oracle at about 1e-12, relative (h, u, g and a to RT).
const BOUND: f64 = 1e-12;

/// One cell's difference from the oracle, relative to its scale.
fn difference(column: &str, got: f64, want: f64, t: f64) -> f64 {
    let scale = match column {
        "hmolar" | "umolar" | "gibbsmolar" | "helmholtzmolar" => R_CODATA * t,
        _ => want.abs().max(1e-12),
    };
    (got - want).abs() / scale
}

/// Every cell of a row, as (column, phasekit's value, the oracle's).
fn cells(table: &Fixture<'_>, i: usize, fluid: &Fluid, t: f64, rho: f64) -> Vec<(&'static str, f64, f64)> {
    let d = fluid.model().helmholtz().unwrap().residual(t, rho, Order::Two);
    let opts = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    let state: State = fluid.flash(Input::new(Pair::DT, rho, t, Basis::Molar).unwrap(), &opts).unwrap();
    let residual = RESIDUAL.iter().map(|&(c, i_, j)| (c, d.get(i_, j).unwrap()));
    let props = PROPS.iter().map(|&(c, prop)| (c, fluid.prop(&state, prop).unwrap()));
    residual.chain(props).map(|(c, got)| (c, got, table.value(i, c).unwrap())).collect()
}

fn table() -> Fixture<'static> {
    let (path, text) = fixture!("coolprop-8.0.0/all/cubic.csv");
    Fixture::parse(path, text).unwrap()
}

fn row_key<'a>(table: &Fixture<'a>, i: usize) -> (&'a str, &'a str, f64, f64) {
    let text = |k: usize| match table.rows()[i].cells.get(k) {
        Some(Cell::Text(t)) => *t,
        _ => "",
    };
    (text(0), text(1), table.value(i, "T").unwrap(), table.value(i, "rhomolar").unwrap())
}

/// The oracle check (Parity), and DIV-0048 and DIV-0049's counts: 1856 rows, every one `ok`; 21 fluids' every cell and
/// 12 fluids' eight α⁰-dependent cells exempt, the rest within [`BOUND`]. Each exempt fluid still differs beyond 1e-12
/// somewhere (the oracle still shows the defect).
#[test]
fn cubic_matches_the_oracle() {
    let table = table();
    let parity = Registry::from_embedded(DataSet::Parity).unwrap();
    let registry = with_cubics(&parity, &[Kind::Srk, Kind::Pr], DataSet::Parity).unwrap();
    let (mut asserted, mut exempt) = (0, 0);
    let mut exempt_worst: BTreeMap<&str, f64> = BTreeMap::new();
    for i in 0..table.rows().len() {
        let (name, kind, t, rho) = row_key(&table, i);
        assert_eq!(table.printed(i, "status"), Some("ok"), "{kind}::{name} at {t} K");
        let fluid = registry.get(&format!("{kind}::{name}")).unwrap();
        for (column, got, want) in cells(&table, i, fluid, t, rho) {
            let diff = difference(column, got, want, t);
            match exempt_at(DIVERGENCES, name, FixtureKind::Cubic, column, t) {
                Some(id) => {
                    exempt += 1;
                    let worst = exempt_worst.entry(id).or_insert(0.0);
                    *worst = worst.max(diff);
                }
                None => {
                    asserted += 1;
                    assert!(diff <= BOUND, "{kind}::{name} at {t} K, {rho} mol/m3: {column} {got} against {want}");
                }
            }
        }
    }
    assert_eq!(table.rows().len(), 116 * 2 * 8);
    assert_eq!((exempt, asserted), (21 * 16 * 15 + 12 * 16 * 8, 1856 * 15 - 21 * 16 * 15 - 12 * 16 * 8));
    assert!(exempt_worst.get("DIV-0048").is_some_and(|w| *w > 1.0), "{exempt_worst:?}");
    assert!(exempt_worst.get("DIV-0049").is_some_and(|w| *w > 1.0), "{exempt_worst:?}");
}

/// DIV-0047: on `Corrected` (the exact Ω), SRK parts from the oracle by about its 4e-10, where PR, whose Ω_b differs by
/// 5e-14 only, does not reach 1e-12 in p.
#[test]
fn corrected_srk_parts_from_the_oracle_by_div_0047() {
    let table = table();
    let corrected = Registry::from_embedded(DataSet::Corrected).unwrap();
    let registry = with_cubics(&corrected, &[Kind::Srk, Kind::Pr], DataSet::Corrected).unwrap();
    let (mut srk, mut pr) = (0.0_f64, 0.0_f64);
    for i in 0..table.rows().len() {
        let (name, kind, t, rho) = row_key(&table, i);
        if exempt_at(DIVERGENCES, name, FixtureKind::Cubic, "p", t).is_some() || name != "n-Propane" {
            continue;
        }
        let fluid = registry.get(&format!("{kind}::{name}")).unwrap();
        let p = cells(&table, i, fluid, t, rho).into_iter().find(|(c, ..)| *c == "p").unwrap();
        let diff = difference("p", p.1, p.2, t);
        if kind == "SRK" { srk = srk.max(diff) } else { pr = pr.max(diff) }
    }
    assert!((1e-11..1e-8).contains(&srk) && pr < 1e-12, "SRK {srk:e}, PR {pr:e}");
}
