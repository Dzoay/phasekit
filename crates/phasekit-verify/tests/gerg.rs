//! GERG-2008 (PLAN.md M13.5): its transcription against GERG TM15's GERG-2004 tables, AGA8's check state and binary
//! test set, and teqp's vectors.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::HashMap;
use std::sync::OnceLock;

use phasekit_core::{
    Basis, DataSet, Density, DerivVar, DomainPolicy, FlashOptions, Fluid, Input, Order, Partial, Phase, Pressure,
    Registry, SecondPartial, Temperature,
};
use phasekit_mix::{MultiFluid, R_GERG, gerg2008};
use phasekit_verify::eos::{Majorants, carried};
use phasekit_verify::{DIVERGENCES, Fixture, Tolerance, ToleranceClass, fixture, from_printed, majorant};

/// GERG-2008's 21 components in AGA8's order, as the registry names them.
const GERG: [&str; 21] = [
    "Methane",
    "Nitrogen",
    "CarbonDioxide",
    "Ethane",
    "n-Propane",
    "IsoButane",
    "n-Butane",
    "Isopentane",
    "n-Pentane",
    "n-Hexane",
    "n-Heptane",
    "n-Octane",
    "n-Nonane",
    "n-Decane",
    "Hydrogen",
    "Oxygen",
    "CarbonMonoxide",
    "Water",
    "HydrogenSulfide",
    "Helium",
    "Argon",
];

/// The same components as teqp names them (`teqp-0.23.2/gerg/*.csv`) and as AGA8's test data spell them
/// (`aga8/binary.csv`), in the same order.
const TEQP: [&str; 21] = [
    "methane",
    "nitrogen",
    "carbondioxide",
    "ethane",
    "propane",
    "isobutane",
    "n-butane",
    "isopentane",
    "n-pentane",
    "n-hexane",
    "n-heptane",
    "n-octane",
    "n-nonane",
    "n-decane",
    "hydrogen",
    "oxygen",
    "carbonmonoxide",
    "water",
    "hydrogensulfide",
    "helium",
    "argon",
];
const AGA8: [&str; 21] = [
    "methane", "nitrogen", "CO2", "ethane", "propane", "isobutan", "butane", "ipentane", "pentane", "hexane",
    "heptane", "octane", "nonane", "decane", "hydrogen", "oxygen", "CO", "water", "H2S", "helium", "argon",
];

fn registry() -> Registry {
    Registry::from_embedded(DataSet::Corrected).unwrap()
}

/// GERG-2008's transcription keyed as AGA8 assigns it (`phasekit_mix::internal::gerg_2008_aga8`), built once.
fn aga8() -> &'static HashMap<(&'static str, Vec<usize>), f64> {
    static TABLES: OnceLock<HashMap<(&'static str, Vec<usize>), f64>> = OnceLock::new();
    TABLES.get_or_init(|| {
        phasekit_mix::internal::gerg_2008_aga8().into_iter().map(|(name, idx, v)| ((name, idx), v)).collect()
    })
}

/// The transcription's `name[idx]`.
fn entry(name: &'static str, idx: &[usize]) -> Option<f64> {
    aga8().get(&(name, idx.to_vec())).copied()
}

/// The registry's name of a component as `names` spell it.
fn component(names: &[&str; 21], spelling: &str) -> &'static str {
    GERG[names.iter().position(|n| *n == spelling).unwrap()]
}

/// GERG TM15's names for the 18 GERG-2004 components (Tables A3.1-A3.7) and its formulas (Table A3.8), with AGA8's
/// index (from 1) of each.
const TM15: [(&str, &str, usize); 18] = [
    ("Methane", "CH4", 1),
    ("Nitrogen", "N2", 2),
    ("Carbon dioxide", "CO2", 3),
    ("Ethane", "C2H6", 4),
    ("Propane", "C3H8", 5),
    ("Isobutane", "i-C4H10", 6),
    ("n-Butane", "n-C4H10", 7),
    ("Isopentane", "i-C5H12", 8),
    ("n-Pentane", "n-C5H12", 9),
    ("n-Hexane", "n-C6H14", 10),
    ("n-Heptane", "n-C7H16", 11),
    ("n-Octane", "n-C8H18", 12),
    ("Hydrogen", "H2", 15),
    ("Oxygen", "O2", 16),
    ("Carbon monoxide", "CO", 17),
    ("Water", "H2O", 18),
    ("Helium", "He", 20),
    ("Argon", "Ar", 21),
];

/// The components each of Tables A3.2 and A3.3 prints one exponent table (`*`) for.
const SHARED: [(&str, &[&str]); 2] = [
    (
        "A3.2",
        &[
            "Propane",
            "n-Butane",
            "Isobutane",
            "n-Pentane",
            "Isopentane",
            "n-Hexane",
            "n-Heptane",
            "n-Octane",
            "Oxygen",
            "Carbon monoxide",
            "Argon",
        ],
    ),
    ("A3.3", &["Methane", "Nitrogen", "Ethane"]),
];

/// Table A3.7's blocks by AGA8's model number.
const MODELS: [(&str, usize); 8] = [
    ("Methane&Ethane", 1),
    ("Methane&Propane", 2),
    ("Methane&Nitrogen", 3),
    ("Methane&Carbon dioxide", 4),
    ("Nitrogen&Carbon dioxide", 5),
    ("Nitrogen&Ethane", 6),
    ("Methane&Hydrogen", 7),
    ("generalized", 10),
];

/// AGA8's index of a TM15 name or formula.
fn index(label: &str) -> Option<usize> {
    TM15.iter().find(|(name, formula, _)| *name == label || *formula == label).map(|t| t.2)
}

/// A TM15 cell's AGA8 entries, each with the function of the AGA8 value it compares, and the bound's factor on the
/// printed half unit; `None` for a cell GERG-2008 computes rather than transcribes (Table A3.1's n₁ and n₂).
#[allow(clippy::type_complexity)]
fn entries(table: &str, component: &str, quantity: &str) -> Option<Vec<(&'static str, Vec<usize>, bool)>> {
    let (name, k) = match quantity.split_once('[') {
        Some((name, k)) => (name, k.trim_end_matches(']').parse::<usize>().ok()),
        None => (quantity, None),
    };
    let pair = || {
        let (a, b) = component.split_once('&')?;
        let (a, b) = (index(a)?, index(b)?);
        Some((a.min(b), a.max(b), a > b))
    };
    let components: Vec<usize> = match component {
        "*" => SHARED.iter().find(|(t, _)| *t == table)?.1.iter().filter_map(|c| index(c)).collect(),
        c => index(c).into_iter().collect(),
    };
    let each = |array: &'static str, k: Option<usize>| -> Vec<(&'static str, Vec<usize>, bool)> {
        components.iter().map(|&i| (array, k.map_or(vec![i], |k| vec![i, k]), false)).collect()
    };
    Some(match (table, name) {
        ("A3.1", "n0") if matches!(k, Some(1 | 2)) => return None,
        ("A3.1", "n0") => each("n0i", k),
        ("A3.1", "theta") => each("th0i", k),
        ("A3.2" | "A3.3" | "A3.4", "n") => each("noik", k),
        ("A3.2" | "A3.3" | "A3.4", "c") => each("coik", k),
        ("A3.2" | "A3.3" | "A3.4", "d") => each("doik", k),
        ("A3.2" | "A3.3" | "A3.4", "t") => each("toik", k),
        ("A3.2" | "A3.3" | "A3.4", "KPol") => each("kpol", None),
        ("A3.2" | "A3.3" | "A3.4", "KExp") => each("kexp", None),
        ("A3.5", "rhoc") => each("Dc", None),
        ("A3.5", "Tc") => each("Tc", None),
        ("A3.5", "M") => each("MMiGERG", None),
        ("A3.6", "F") => pair().map(|(i, j, _)| vec![("fij", vec![i, j], false)])?,
        ("A3.7", _) => {
            let m = MODELS.iter().find(|(c, _)| *c == component)?.1;
            let array = match name {
                "d" => "dijk",
                "t" => "tijk",
                "n" => "nijk",
                "eta" => "cijk",
                "epsilon" => "eijk",
                "beta" => "bijk",
                "gamma" => "gijk",
                "KPol" => "kpolij",
                "KExp" => "kexpij",
                _ => return Some(Vec::new()),
            };
            vec![(array, k.map_or(vec![m], |k| vec![m, k]), false)]
        }
        ("A3.8", _) => {
            let (i, j, swapped) = pair()?;
            let (array, inverted) = match name {
                "beta_v" => ("bvij", swapped),
                "gamma_v" => ("gvij", false),
                "beta_T" => ("btij", swapped),
                "gamma_T" => ("gtij", false),
                _ => return Some(Vec::new()),
            };
            vec![(array, vec![i, j], inverted)]
        }
        _ => Vec::new(),
    })
}

/// The γ of Table A3.8 that GERG-2008 changed: isopentane's γ_v and carbon monoxide's γ_T with the C₄+ alkanes.
const CHANGED_GAMMAS: [&str; 16] = [
    "C2H6&i-C5H12 gamma_v",
    "n-C4H10&i-C5H12 gamma_v",
    "n-C4H10&CO gamma_T",
    "i-C4H10&i-C5H12 gamma_v",
    "i-C4H10&CO gamma_T",
    "n-C5H12&i-C5H12 gamma_v",
    "n-C5H12&CO gamma_T",
    "i-C5H12&n-C6H14 gamma_v",
    "i-C5H12&n-C7H16 gamma_v",
    "i-C5H12&n-C8H18 gamma_v",
    "i-C5H12&H2 gamma_v",
    "i-C5H12&CO gamma_v",
    "i-C5H12&CO gamma_T",
    "n-C6H14&CO gamma_T",
    "n-C7H16&CO gamma_T",
    "n-C8H18&CO gamma_T",
];

/// Whether GERG-2008 changed a TM15 cell: isopentane's and carbon monoxide's equations (their 12 coefficients each)
/// and critical parameters (ρ_c 3.271 mol/dm³, T_c 132.86 K), so CO's ϑ = θ/T_c, and the 16 γ of [`CHANGED_GAMMAS`].
fn changed_in_gerg_2008(table: &str, component: &str, quantity: &str) -> bool {
    let changed = ["Isopentane", "Carbon monoxide"].contains(&component);
    match table {
        "A3.2" => changed && quantity.starts_with("n["),
        "A3.5" => matches!((component, quantity), ("Isopentane", "rhoc") | ("Carbon monoxide", "Tc")),
        "A3.1" => component == "Carbon monoxide" && quantity.starts_with("theta["),
        "A3.8" => CHANGED_GAMMAS.contains(&format!("{component} {quantity}").as_str()),
        _ => false,
    }
}

/// GERG TM15 Tables A3.1-A3.8 (GERG-2004, double-entered) against GERG-2008's transcription from AGA8, all 2135
/// cells: each within half a unit of its last printed digit, but for the 44 GERG-2008 changed
/// ([`changed_in_gerg_2008`]). AGA8 stores θ_k in K, TM15 ϑ_k = θ_k/T_c, printed negative for nitrogen's and carbon
/// dioxide's cosh term (cosh is even); AGA8's n₃ is c_p⁰'s constant, α⁰'s plus 1; AGA8 orders n-butane and isobutane
/// the other way round, so their β are reciprocals. Table A3.1's n₁ and n₂ are computed here, not transcribed (the
/// check state tests them): GERG-2004's differ in convention.
#[test]
fn gerg_2008_extends_tm15_s_gerg_2004() {
    let (path, text) = fixture!("paper/mixtures/Kunz-GERG-TM15-2007.A3.csv");
    let table = Fixture::parse(path, text).unwrap();
    let aga8 = aga8();
    let (mut compared, mut changed, mut unmatched) = (0, 0, Vec::new());
    for row in 0..table.rows().len() {
        let cell = |c: &str| table.printed(row, c).unwrap();
        let (t, component, quantity, printed) = (cell("table"), cell("component"), cell("quantity"), cell("value"));
        let Some(entries) = entries(t, component, quantity) else { continue };
        if entries.is_empty() {
            unmatched.push(format!("{t} {component} {quantity}: no AGA8 entry"));
        }
        let Some(Tolerance::Absolute(half)) = from_printed(printed) else { panic!("{printed}") };
        let want = table.value(row, "value").unwrap();
        for (array, idx, inverted) in entries {
            let Some(&got) = aga8.get(&(array, idx.clone())) else {
                unmatched.push(format!("{t} {component} {quantity}: no {array}{idx:?}"));
                continue;
            };
            let (got, want, bound) = match array {
                "th0i" => (got / aga8[&("Tc", vec![idx[0]])], want.abs(), half),
                "n0i" if idx[1] == 3 => (got - 1.0, want, half),
                _ if inverted => (1.0 / got, want, half / (want * want)),
                _ => (got, want, half),
            };
            let differs = (got - want).abs() > bound;
            if differs != changed_in_gerg_2008(t, component, quantity) {
                unmatched.push(format!("{t} {component} {quantity}: {printed} against {got}"));
            }
            (compared, changed) = (compared + 1, changed + usize::from(differs));
        }
    }
    assert!(unmatched.is_empty(), "{unmatched:#?}");
    assert_eq!((compared, changed), (2135, 44));
}

/// The gas constant GERG's ideal-gas equations were fitted with, J/(mol·K): AGA8 scales n₁ ... n₇ by R*/R.
const R_STAR: f64 = 8.314_51;

/// AGA8's check state (`AGA8CODE/C/GERG2008_test_01.cpp`): the 21 components at 400 K and 50 MPa. The density from a
/// PT solve on the gas branch within `Flash`, then at AGA8's density M, p, Z, (∂p/∂ρ)_T, (∂²p/∂ρ²)_T, (∂p/∂T)_ρ, c_v,
/// c_p, w, μ_JT and κ within `Prop` (AGA8's own test allows 1e-8 in its units), and u, h, s and g within `Prop` once
/// the rounding of AGA8's ten-digit n₁ and n₂ is added: u and h move by R Σ x_i ΔN₂,i, s by −R Σ x_i ΔN₁,i and g by
/// R Σ x_i (T ΔN₁,i + ΔN₂,i), ΔN = (R*/R)(printed − computed) (phasekit computes them exactly, as AGA8's own
/// commented-out code does).
#[test]
fn gerg_2008_matches_aga8_s_check_state() {
    let (path, text) = fixture!("aga8/check-state.csv");
    let table = Fixture::parse(path, text).unwrap();
    let want = |c: &str| table.value(0, c).unwrap();
    let x: Vec<f64> = (1..=21).map(|i| want(&format!("x{i}"))).collect();
    let components: Vec<(&str, f64)> = GERG.iter().copied().zip(x.iter().copied()).collect();
    let gas = gerg2008(&registry(), &components).unwrap();
    let (t, p) = (want("T"), 1e3 * want("P_in"));
    let solved = gas.flash(Input::pt(Pressure::new(p).unwrap(), Temperature::new(t).unwrap()), &gas_phase()).unwrap();
    let rho = solved.rho(Basis::Molar);
    table.check_scaled(0, "D", 1e-3 * rho, want("D")).unwrap();
    let rho = 1e3 * want("D");
    let s = gas.flash(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap()), &gas_phase()).unwrap();
    let eos = gas.model().helmholtz().unwrap();
    let total = eos.ideal(t, rho, Order::Three) + eos.residual(t, rho, Order::Three);
    let partial = |of, wrt, at| s.partial(Partial { of, wrt, at }).unwrap();
    let dp_drho = partial(DerivVar::P, DerivVar::Dmolar, DerivVar::T);
    let first = Partial { of: DerivVar::P, wrt: DerivVar::Dmolar, at: DerivVar::T };
    let d2p = s.second_partial(SecondPartial { first, wrt: DerivVar::Dmolar, at: DerivVar::T }, &total).unwrap();
    let (r, m, w) = (R_GERG, gas.info().molar_mass(), s.speed_of_sound().unwrap());
    // The ideal-gas constants' rounding, per component, scaled as AGA8 scales them.
    let printed = |i: usize, j: usize| entry("n0i", &[i, j]).unwrap();
    let computed = phasekit_mix::internal::gerg_2008_n1_n2();
    let delta = |j: usize| -> f64 {
        x.iter().enumerate().map(|(i, xi)| xi * (R_STAR / r) * (printed(i + 1, j) - computed[i][j - 1])).sum()
    };
    let (dn1, dn2) = (delta(1), delta(2));
    let cells = [
        ("mm", 1e3 * m, 1e3 * m),
        ("P", 1e-3 * s.p(), 1e-3 * s.p()),
        ("Z", s.z(), 1.0),
        ("dPdD", dp_drho, s.p() / rho),
        ("d2PdD2", 1e3 * d2p, 1e3 * s.p() / (rho * rho)),
        ("dPdT", 1e-3 * partial(DerivVar::P, DerivVar::T, DerivVar::Dmolar), 1e-3 * s.p() / t),
        ("U", s.u(Basis::Molar) + r * dn2, r * t),
        ("H", s.h(Basis::Molar) + r * dn2, r * t),
        ("s", s.s(Basis::Molar) - r * dn1, r),
        ("Cv", s.cv(Basis::Molar).unwrap(), r),
        ("Cp", s.cp(Basis::Molar).unwrap(), r),
        ("W", w, w),
        ("G", s.g(Basis::Molar) + r * (t * dn1 + dn2), r * t),
        ("JT", 1e3 * partial(DerivVar::T, DerivVar::P, DerivVar::Hmolar), 1e3 * t / s.p()),
        ("Kappa", w * w * m * rho / s.p(), 1.0),
    ];
    let misses: Vec<String> = cells
        .iter()
        .filter_map(|(column, got, floor)| {
            table.check_scaled(0, column, *got, want(column).abs().max(floor.abs())).err()
        })
        .map(|e| format!("{e:?}"))
        .collect();
    assert!(misses.is_empty(), "{misses:#?}");
}

/// A flash on the gas branch, outside the components' limits too where a fixture asks for it.
fn gas_phase() -> FlashOptions {
    FlashOptions::new().with_phase(Phase::Gas).with_domain(DomainPolicy::Extrapolate)
}

/// The `Term` scales of the order-2 bundle of components `x` at (T, ρ) and the mixture's (τ, δ): the residual's
/// ([`term_scale`]) and the ideal gas's τ-part, Σ x_i (|N₃| + Σ_k |N_k| (θ_k u)² csch² or sech²), u = 1/T (its δ-part
/// is exact).
fn majorants(x: &[(usize, f64)], (tau, delta): (f64, f64), t: f64) -> Majorants {
    let get = |name: &'static str, idx: [usize; 2]| entry(name, &idx).unwrap();
    let ratio = R_STAR / R_GERG;
    let ideal_20: f64 = x
        .iter()
        .map(|&(c, xc)| {
            let hyperbolic: f64 = (4..=7)
                .map(|k| {
                    let (n, y) = (get("n0i", [c + 1, k]), get("th0i", [c + 1, k]) / t);
                    let f =
                        if k % 2 == 0 { y / phasekit_core::math::sinh(y) } else { y / phasekit_core::math::cosh(y) };
                    if y > 0.0 { (n * f * f).abs() } else { 0.0 }
                })
                .sum();
            xc * ratio * ((get("n0i", [c + 1, 3]) - 1.0).abs() + hyperbolic)
        })
        .sum();
    let m = |i, j| term_scale(x, tau, delta, (i, j));
    Majorants { m00: m(0, 0), m10: m(1, 0), m01: m(0, 1), m20: m(2, 0) + ideal_20, m11: m(1, 1), m02: m(0, 2) }
}

/// A binary's GERG-2008 fluid and its composition as AGA8 indices with fractions.
type Built = (Fluid, Vec<(usize, f64)>);

/// AGA8's binary test set (`TESTDATA/Test Data.xls`, its GERG-2008 columns): every pair of the 21 components at 50/50,
/// 3645 states along the saturation boundary, p, c_v, c_p and w at (T, ρ). Each is within half a unit of its eighth
/// significant digit, as printed, plus DIV-0055's measured 2e-10 of the value, or of `Term`'s carried scale over 10
/// where that is larger (p near zero cancels): the sheet comes from an implementation whose θ_k are ϑ_k T_c.
#[test]
fn gerg_2008_matches_aga8_s_binary_test_set() {
    let (path, text) = fixture!("aga8/binary.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = registry();
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0055").unwrap();
    let Some(Tolerance::Relative(measured)) = entry.tolerance else { panic!("{:?}", entry.tolerance) };
    let mut built: HashMap<&str, Built> = HashMap::new();
    let mut misses = Vec::new();
    for row in 0..table.rows().len() {
        let pair = table.printed(row, "pair").unwrap();
        let (fluid, x) = built.entry(pair).or_insert_with(|| {
            let (a, b) = pair.split_once('&').unwrap();
            let (a, b) = (component(&AGA8, a), component(&AGA8, b));
            let x = [a, b].map(|c| (GERG.iter().position(|g| *g == c).unwrap(), 0.5)).to_vec();
            (gerg2008(&registry, &[(a, 0.5), (b, 0.5)]).unwrap(), x)
        });
        let (t, rho) = (table.value(row, "T").unwrap(), 1e3 * table.value(row, "D").unwrap());
        let s = fluid.flash(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap()), &gas_phase());
        let s = s.unwrap();
        let eos = fluid.model().helmholtz().unwrap();
        let total = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
        let reducing = eos.reducing().unwrap();
        let m = majorants(x, (reducing.t / t, rho / reducing.rho), t);
        let cells = [
            ("P", "p", 1e-6, s.p()),
            ("cv", "cvmolar", 1.0, s.cv(Basis::Molar).unwrap_or(f64::NAN)),
            ("cp", "cpmolar", 1.0, s.cp(Basis::Molar).unwrap_or(f64::NAN)),
            ("w", "speed_sound", 1.0, s.speed_of_sound().unwrap_or(f64::NAN)),
        ];
        for (column, relation, unit, got) in cells {
            let printed = table.printed(row, column).unwrap();
            let Some(Tolerance::Absolute(half)) = from_printed(printed) else { panic!("{printed}") };
            let carried = carried(relation, &total, &m, R_GERG, t, rho, fluid.info().molar_mass());
            let scale = (got.abs() * unit).max(if carried.is_nan() { 0.0 } else { 0.1 * carried * unit });
            let bound = half + measured * scale;
            if let Err(e) = table.check_bound(row, column, got * unit, bound) {
                misses.push(format!("{pair} row {row} {column}: {e:?}"));
            }
        }
    }
    assert_eq!(table.rows().len(), 3645);
    assert!(misses.is_empty(), "{} misses: {:#?}", misses.len(), &misses[..misses.len().min(20)]);
}

/// The `Term` scale of α^r's entry (i, j) for components `x` at the mixture's (τ, δ): each component's GERG terms'
/// majorants weighted by x; the departure functions are left out, so the bound is if anything tighter than `Term`'s.
fn term_scale(x: &[(usize, f64)], tau: f64, delta: f64, (i, j): (usize, usize)) -> f64 {
    let get = |name: &'static str, idx: [usize; 2]| entry(name, &idx);
    let mut scale = 0.0;
    for &(c, xc) in x {
        for k in 1.. {
            let Some(n) = get("noik", [c + 1, k]) else { break };
            let (d, t, l) =
                (get("doik", [c + 1, k]).unwrap(), get("toik", [c + 1, k]).unwrap(), get("coik", [c + 1, k]));
            let l = l.unwrap();
            scale += xc * majorant::separable(n, t, 0.0, 0.0, d, f64::from(u8::from(l > 0.0)), l, (tau, delta), (i, j));
        }
    }
    scale
}

/// The columns of teqp's α^r and their (i, j).
const ALPHAR: [(&str, (usize, usize)); 10] = [
    ("alphar", (0, 0)),
    ("A10", (1, 0)),
    ("A01", (0, 1)),
    ("A20", (2, 0)),
    ("A11", (1, 1)),
    ("A02", (0, 2)),
    ("A30", (3, 0)),
    ("A21", (2, 1)),
    ("A12", (1, 2)),
    ("A03", (0, 3)),
];

/// How far AGA8's rounding of isobutane/n-butane's reciprocal β (nine decimals, components 6 and 7) moves the
/// reducing sum Y_r (T_r, or v_r = 1/ρ_r where `volume`) at composition `x`: the pair's term
/// P = 2βγ Y_ij x_i x_j (x_i + x_j)/(β² x_i + x_j) moves by |∂P/∂β| = |P (1/β − 2β x_i/(β² x_i + x_j))| times half a
/// unit of β's last printed digit. Zero without both components, and to first order at x_i = x_j.
fn reciprocal_rounding(x: &[(usize, f64)], volume: bool) -> f64 {
    let fraction = |c: usize| x.iter().find(|(i, _)| *i == c).map(|(_, xc)| *xc);
    let (Some(xi), Some(xj)) = (fraction(5), fraction(6)) else { return 0.0 };
    let get = |name: &'static str, idx: &[usize]| entry(name, idx).unwrap();
    let (beta, gamma) = if volume {
        (get("bvij", &[6, 7]), get("gvij", &[6, 7]))
    } else {
        (get("btij", &[6, 7]), get("gtij", &[6, 7]))
    };
    let y_ij = if volume {
        let root = |c: usize| phasekit_core::math::powf(1.0 / (1e3 * get("Dc", &[c])), 1.0 / 3.0);
        phasekit_core::math::powi(root(6) + root(7), 3) / 8.0
    } else {
        phasekit_core::math::sqrt(get("Tc", &[6]) * get("Tc", &[7]))
    };
    let p = 2.0 * beta * gamma * y_ij * xi * xj * (xi + xj) / (beta * beta * xi + xj);
    // A printed digit always has a half unit; NaN (failing every comparison) if it had none.
    let half = match from_printed(&beta.to_string()) {
        Some(Tolerance::Absolute(half)) => half,
        _ => f64::NAN,
    };
    (p * (1.0 / beta - 2.0 * beta * xi / (beta * beta * xi + xj))).abs() * half
}

/// One teqp vector file against phasekit's GERG-2008: each row's model (`model(row)`: the multi-fluid model and its
/// composition, as AGA8 indices with fractions), T_r and ρ_r within `Prop`, and α^r's A_ij at teqp's own τ and δ (as
/// the `term` kind compares them) within `Term` on the components' majorants, or `Prop` of max(|A_ij|, 1) where that is
/// larger. AGA8 writes isobutane/n-butane's β as their reciprocals rounded to nine decimals (it orders the pair the
/// other way round from the publication, which teqp follows), so T_r and ρ_r also allow that rounding
/// ([`reciprocal_rounding`]), and τ moves with them, which comparing at teqp's τ and δ leaves out. Returns the rows
/// compared.
fn teqp_vectors(table: &Fixture<'static>, model: impl Fn(usize) -> (MultiFluid, Vec<(usize, f64)>)) -> usize {
    let mut misses = Vec::new();
    for row in 0..table.rows().len() {
        let (mix, x) = model(row);
        let z: Vec<f64> = x.iter().map(|(_, xi)| *xi).collect();
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rhomolar").unwrap());
        let (t_r, rho_r) = mix.reducing_state(&z);
        let rounding = [reciprocal_rounding(&x, false), rho_r * rho_r * reciprocal_rounding(&x, true)];
        for ((column, got), rounding) in [("Tr", t_r), ("rhor", rho_r)].into_iter().zip(rounding) {
            let bound = ToleranceClass::Prop.bound(got.abs()).unwrap() + rounding;
            if let Err(e) = table.check_bound(row, column, got, bound) {
                misses.push(format!("row {row} {column}: {e:?}"));
            }
        }
        let (t_r, rho_r) = (table.value(row, "Tr").unwrap(), table.value(row, "rhor").unwrap());
        let (at_t, at_rho) = (t * (mix.reducing_state(&z).0 / t_r), rho * (mix.reducing_state(&z).1 / rho_r));
        let d = mix.residual(&z, at_t, at_rho, Order::Three).unwrap();
        for (column, (i, j)) in ALPHAR {
            let want = table.value(row, column).unwrap();
            let term = ToleranceClass::Term.bound(term_scale(&x, t_r / t, rho / rho_r, (i, j))).unwrap();
            let prop = ToleranceClass::Prop.bound(want.abs().max(1.0)).unwrap();
            if let Err(e) = table.check_bound(row, column, d.get(i, j).unwrap(), term.max(prop)) {
                misses.push(format!("row {row} {column}: {e:?}"));
            }
        }
    }
    assert!(misses.is_empty(), "{} misses: {:#?}", misses.len(), &misses[..misses.len().min(20)]);
    table.rows().len()
}

/// The multi-fluid model of AGA8 components `index`, with a pure component's twin when there is one: a GERG pure
/// fluid enters a two-component model at x = (1, 0) with methane, or with nitrogen if it is methane.
fn model_of(index: &[usize]) -> MultiFluid {
    let names: Vec<&str> = index.iter().map(|&i| GERG[i]).collect();
    MultiFluid::gerg2008(&registry(), &names).unwrap()
}

/// ROT-198, ROT-203: teqp 0.23.2's GERG-2008 (`GERG2008resid`, NIST public domain; differential, never an arbiter)
/// for the 21 pure components, every binary at 50/50 and AGA8's 200 natural gases (`teqp-0.23.2/gerg/*.csv`).
#[test]
fn gerg_2008_matches_teqp() {
    let (path, text) = fixture!("teqp-0.23.2/gerg/pure.csv");
    let pure = Fixture::parse(path, text).unwrap();
    let index = |name: &str| TEQP.iter().position(|n| *n == name).unwrap();
    let rows = teqp_vectors(&pure, |row| {
        let i = index(pure.printed(row, "component").unwrap());
        let twin = usize::from(i == 0);
        (model_of(&[i, twin]), vec![(i, 1.0), (twin, 0.0)])
    });
    let (path, text) = fixture!("teqp-0.23.2/gerg/binary.csv");
    let binary = Fixture::parse(path, text).unwrap();
    let rows = rows
        + teqp_vectors(&binary, |row| {
            let (a, b) = binary.printed(row, "pair").unwrap().split_once('&').unwrap();
            let (a, b) = (index(a), index(b));
            (model_of(&[a, b]), vec![(a, 0.5), (b, 0.5)])
        });
    let (path, text) = fixture!("aga8/gases.csv");
    let gases = Fixture::parse(path, text).unwrap();
    let composition = |gas: &str| -> Vec<(usize, f64)> {
        let row = (0..gases.rows().len()).find(|&r| gases.printed(r, "gas") == Some(gas)).unwrap();
        TEQP.iter().enumerate().map(|(i, n)| (i, gases.value(row, n).unwrap())).filter(|(_, x)| *x != 0.0).collect()
    };
    let (path, text) = fixture!("teqp-0.23.2/gerg/gas.csv");
    let gas = Fixture::parse(path, text).unwrap();
    let rows = rows
        + teqp_vectors(&gas, |row| {
            let mut x = composition(gas.printed(row, "gas").unwrap());
            if let [(i, _)] = x[..] {
                x.push((usize::from(i == 0), 0.0)); // AGA8's gas 201 is pure methane
            }
            (model_of(&x.iter().map(|(i, _)| *i).collect::<Vec<_>>()), x)
        });
    assert_eq!(rows, 84 + 840 + 400);
}
