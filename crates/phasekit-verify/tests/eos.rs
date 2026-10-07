//! L2 properties at (T, ρ) (map 10 §8.5): the `eos` oracle fixtures (VERIFICATION.md §3.5) of the core subset and the
//! all-fluid tier against the DT flash of the `Parity` data with the oracle's phase imposed, and the L4 identities
//! (VERIFICATION.md §8.1) at the same states.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::FluidRecord;
use phasekit_core::{Basis, DataSet, DerivVar, Partial, Registry, State};
use phasekit_verify::eos::{self, EosCheck, IdentityCheck};
use phasekit_verify::{Cell, Fixture, ToleranceClass, Window, fixture};

/// The core subset's `eos` files (VERIFICATION.md §3.6).
const EOS_CORE: [(&str, &str); 14] = [
    fixture!("coolprop-8.0.0/eos/Air.csv"),
    fixture!("coolprop-8.0.0/eos/Ammonia.csv"),
    fixture!("coolprop-8.0.0/eos/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/eos/HFE143m.csv"),
    fixture!("coolprop-8.0.0/eos/Helium.csv"),
    fixture!("coolprop-8.0.0/eos/Methanol.csv"),
    fixture!("coolprop-8.0.0/eos/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/eos/R1130(E).csv"),
    fixture!("coolprop-8.0.0/eos/R1234yf.csv"),
    fixture!("coolprop-8.0.0/eos/R1234ze(E).csv"),
    fixture!("coolprop-8.0.0/eos/R125.csv"),
    fixture!("coolprop-8.0.0/eos/R410A.csv"),
    fixture!("coolprop-8.0.0/eos/Water.csv"),
    fixture!("coolprop-8.0.0/eos/n-Heptane.csv"),
];

/// The decoded `Parity` record of `name`.
fn record(registry: &Registry, name: &str) -> FluidRecord {
    phasekit_core::internal::record(registry, name).unwrap()
}

/// The all-fluid tier's rows, grouped by fluid in file order.
fn by_fluid<'a>(fixture: &Fixture<'a>) -> Vec<(&'a str, Vec<usize>)> {
    let mut groups: Vec<(&str, Vec<usize>)> = Vec::new();
    for (row, cells) in fixture.rows().iter().enumerate() {
        let fluid = match cells.cells[0] {
            Cell::Text(fluid) => fluid,
            Cell::Num(_) | Cell::Blank => "", // no fluid of that name: the caller's lookup fails on it
        };
        match groups.last_mut() {
            Some((name, rows)) if *name == fluid => rows.push(row),
            _ => groups.push((fluid, vec![row])),
        }
    }
    groups
}

/// Every row's status is `ok` and its region one of the three labels (VERIFICATION.md §3.3).
fn labels_are_valid(fixture: &Fixture<'_>) {
    for row in fixture.rows() {
        let label = |column: &str| {
            let i = fixture.columns().iter().position(|c| *c == column).unwrap();
            match row.cells[i] {
                Cell::Text(text) => text,
                Cell::Num(_) | Cell::Blank => "",
            }
        };
        assert_eq!(label("status"), "ok", "line {}: the oracle failed", row.line);
        assert!(["stable", "metastable", "unstable"].contains(&label("region")), "line {}", row.line);
    }
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/eos/<Fluid>.csv and all/eos.csv (PLAN.md M5.1; map 10 §8.5 L2): p, h,
/// s, u, cv, cp, w, Z, (∂p/∂ρ)_T and (∂p/∂T)_ρ at 500 (T, ρ) per core fluid and 8 per fluid of all 136, T ~ U[T_low,
/// Tmax], ρ log-spaced over six decades below ρL(T_low), stable, metastable and unstable states alike, phase imposed.
/// Class `Prop` with the floors of VERIFICATION.md §5, or `Term` carried through the relation where a relation cancels
/// (M5.1: 7 entries near a spinodal or just outside the near-critical window need it); w is NaN on both sides where
/// (∂p/∂ρ)_T < 0 (map 10 R18).
#[test]
fn eos_fixtures_match_oracle() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = EosCheck::default();
    for (path, text) in EOS_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        labels_are_valid(&fixture);
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let rows: Vec<usize> = (0..fixture.rows().len()).collect();
        assert_eq!(rows.len(), 500, "{path}");
        check.rows(&fixture, &rows, registry.get(name).unwrap(), &record(&registry, name));
    }
    let (path, text) = fixture!("coolprop-8.0.0/all/eos.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    labels_are_valid(&fixture);
    let groups = by_fluid(&fixture);
    for (name, rows) in &groups {
        assert_eq!(rows.len(), 8, "{name}");
        check.rows(&fixture, rows, registry.get(name).unwrap(), &record(&registry, name));
    }
    assert_eq!(groups.len(), 136);
    assert_eq!(check.checked, (14 * 500 + 136 * 8) * 10);
    assert_eq!(check.report(20), None);
}

/// The identities of VERIFICATION.md §8.1 at a single-phase `state` of a fluid of molar mass `m` (kg/mol): h = u + p/ρ,
/// g = h − Ts, a = u − Ts, μ = a + p/ρ = g (a pure fluid), Z = p/(ρRT), cp − cv = T(∂p/∂T)²_ρ/(ρ²(∂p/∂ρ)_T), (∂h/∂T)_p
/// = cp, (∂s/∂T)_p = cp/T, (∂g/∂T)_p = −s and w² = (∂p/∂ρ)_s (mass density); where w is refused, (∂p/∂ρ)_s ≤ 0. The
/// relations compute each side from the bundle along different algebra; a first partial's terms are the two summands
/// of its Jacobian ratio.
fn identities(check: &mut IdentityCheck, at: &str, state: &State, m: f64, window: Window) {
    use DerivVar::{Dmass, Dmolar, Gmolar, Hmolar, P, Smolar, T};
    let molar = Basis::Molar;
    let (t, p, rho, r) = (state.t(), state.p(), state.rho(molar), state.gas_constant());
    let (h, s, u, g, a) = (state.h(molar), state.s(molar), state.u(molar), state.g(molar), state.a(molar));
    let d = |of, wrt, at| state.partial(Partial { of, wrt, at }).unwrap();
    // (∂X/∂T)_p = X_T − X_ρ·P_T/P_ρ.
    let (p_t, p_rho) = (d(P, T, Dmolar), d(P, Dmolar, T));
    let along_p = |x| [d(x, T, Dmolar), d(x, Dmolar, T) * p_t / p_rho];
    let mut id = |name: &str, sides: (f64, f64), terms: &[f64]| check.check(at, name, sides, terms, window);
    id("h = u + p/ρ", (h, u + p / rho), &[u, p / rho]);
    id("g = h − Ts", (g, h - t * s), &[h, t * s]);
    id("a = u − Ts", (a, u - t * s), &[u, t * s]);
    id("μ = a + p/ρ = g", (a + p / rho, g), &[a, p / rho]);
    id("Z = p/(ρRT)", (state.z(), p / (rho * r * t)), &[]);
    id("(∂g/∂T)_p = −s", (d(Gmolar, T, P), -s), &along_p(Gmolar));
    let (cv, cp) = (state.cv(molar).unwrap(), state.cp(molar).unwrap());
    let rhs = t * p_t * p_t / (rho * rho * p_rho);
    id("cp − cv = T(∂p/∂T)²_ρ/(ρ²(∂p/∂ρ)_T)", (cp - cv, rhs), &[cp, cv]);
    id("(∂h/∂T)_p = cp", (d(Hmolar, T, P), cp), &along_p(Hmolar));
    id("(∂s/∂T)_p = cp/T", (d(Smolar, T, P), cp / t), &along_p(Smolar));
    // (∂p/∂ρ)_s = (P_ρ − P_T·S_ρ/S_T)/M in mass density.
    let dpds = d(P, Dmass, Smolar);
    let terms = [p_rho / m, p_t * d(Smolar, Dmolar, T) / d(Smolar, T, Dmolar) / m];
    match state.speed_of_sound() {
        Ok(w) => id("w² = (∂p/∂ρ)_s", (w * w, dpds), &terms),
        Err(_) => id("refused w: (∂p/∂ρ)_s ≤ 0", (dpds.max(0.0), 0.0), &terms),
    }
}

/// Identity (VERIFICATION.md §8.1; map 01 §8, map 02 §8): the L4 identities, μ = g included (ROT-062), at the 500
/// (T, ρ) of every core fluid's `eos` grid, stable, metastable and unstable states alike with the phase imposed, on the
/// `Corrected` data. Class `Identity`: 1e-12 of the largest term, 1e-8 in the near-critical window.
#[test]
fn identities_hold() {
    let registry = Registry::embedded().unwrap();
    let mut check = IdentityCheck::default();
    for (path, text) in EOS_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let fluid = registry.get(name).unwrap();
        let (m, critical) = (fluid.info().molar_mass(), fluid.model().critical_point().unwrap());
        for row in 0..fixture.rows().len() {
            let (t, rho) = (fixture.value(row, "T").unwrap(), fixture.value(row, "rhomolar").unwrap());
            let state = eos::state(fluid, t, rho).unwrap();
            let at = format!("{name} at {t} K, {rho} mol/m³");
            identities(&mut check, &at, &state, m, Window::at(t, rho, critical.t, critical.rho));
        }
    }
    assert_eq!(check.checked, 14 * 500 * 10);
    assert_eq!(check.report(20), None);
}

/// VERIFICATION.md §3.2, §11.3: the `eos` files come from the pinned runner image, like every committed oracle fixture.
#[test]
fn eos_fixtures_name_the_pinned_generator_environment() {
    let environment = |(path, text): (&str, &str)| {
        let generator = Fixture::parse(path, text).unwrap().header("generator").unwrap_or_default();
        generator.split(' ').filter(|field| !field.starts_with("sha256=")).collect::<Vec<_>>().join(" ")
    };
    let smoke = environment(fixture!("coolprop-8.0.0/facts/smoke.csv"));
    for file in EOS_CORE.into_iter().chain([fixture!("coolprop-8.0.0/all/eos.csv")]) {
        assert_eq!(environment(file), smoke, "{}", file.0);
    }
}

/// The `eos` fixtures' M5.5 columns of the core subset and the all-fluid tier against the `Parity` data: the partials,
/// or cp⁰ and the residual parts.
fn m5_5_columns(parts: bool) -> EosCheck {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = EosCheck::default();
    for (path, text) in EOS_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let rows: Vec<usize> = (0..fixture.rows().len()).collect();
        check.partial_rows(&fixture, &rows, registry.get(name).unwrap(), &record(&registry, name), parts);
    }
    let (path, text) = fixture!("coolprop-8.0.0/all/eos.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    for (name, rows) in &by_fluid(&fixture) {
        check.partial_rows(&fixture, rows, registry.get(name).unwrap(), &record(&registry, name), parts);
    }
    check
}

/// Oracle: CoolProp 8.0.0, the `eos` fixtures' partial columns (PLAN.md M5.5): 12 first partials, each of CoolProp's 12
/// first-order variables once differentiated, once the variable and once held constant, against `first_partial_deriv`
/// at 8,088 states. Class `Prop`, floor |X|/|Y|, with `Term` carried through the Jacobian ratio (user decision TC1:
/// (∂ρ_mass/∂h_mass)_u cancels in h_T·u_ρ − h_ρ·u_T, 1,787 entries need it).
#[test]
fn first_partials_match_oracle() {
    let check = m5_5_columns(false);
    assert_eq!(check.checked, (14 * 500 + 136 * 8) * 12);
    assert_eq!(check.report(30), None);
}

/// Oracle: CoolProp 8.0.0, the `eos` fixtures' `cp0molar` and `*_residual` columns (PLAN.md M5.5): c_p⁰ = R·(1 − A20⁰)
/// and the residual h, s, g from the model's residual part (`ThermoModel::derivs`, E1). Class `Prop`.
#[test]
fn cp0_and_residual_parts_match_oracle() {
    let check = m5_5_columns(true);
    assert_eq!(check.checked, (14 * 500 + 136 * 8) * 4);
    assert_eq!(check.report(30), None);
}

/// VERIFICATION.md §8.3, class `Fd` (1e-7, relative step 1e-5): A10 and A01 of every core fluid's residual part against
/// central differences in the dimensional T and ρ, so a family's reducing choice cannot hide an error, at its `eos`
/// grid's stable states where α^r is not negligible (|A01^r| > 1e-6; below that the differences lose the digits). Inside
/// the dome a multiparameter EOS oscillates (Water near 440 K and 29,000 mol/m³: p ≈ 1e11 Pa), and a step of 1e-5
/// cannot resolve its curvature.
#[test]
fn fd_first_order_on_every_core_fluid() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let tol = ToleranceClass::Fd.bound(1.0).unwrap();
    let mut checked = 0;
    for (path, text) in EOS_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let eos = registry.get(name).unwrap().model().helmholtz().unwrap();
        let region = fixture.columns().iter().position(|c| *c == "region").unwrap();
        for row in 0..fixture.rows().len() {
            let (t, rho) = (fixture.value(row, "T").unwrap(), fixture.value(row, "rhomolar").unwrap());
            let stable = fixture.rows()[row].cells[region] == Cell::Text("stable");
            if !stable || eos.residual(t, rho, phasekit_core::Order::One).get(0, 1).unwrap().abs() <= 1e-6 {
                continue;
            }
            assert_eq!(
                phasekit_verify::fd_first_order(eos, t, rho, 1e-5, tol),
                Ok(()),
                "{name} at {t} K, {rho} mol/m³"
            );
            checked += 1;
        }
    }
    assert!(checked > 14 * 300, "{checked}");
}
