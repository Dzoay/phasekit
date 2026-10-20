//! L2 properties at (T, ρ) (map 10 §8.5): the `eos` oracle fixtures (VERIFICATION.md §3.5) of the core subset and the
//! all-fluid tier against the DT flash of the `Parity` data with the oracle's phase imposed, and the L4 identities
//! (VERIFICATION.md §8.1) at the same states.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::FluidRecord;
use phasekit_core::{Basis, DataSet, Fluid, Prop, Registry, State};
use phasekit_verify::eos::{self, EosCheck, IdentityCheck, OrderThree};
use phasekit_verify::{Cell, Fixture, ToleranceClass, Window, fixture};

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
    for (path, text) in eos::CORE_FILES {
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

/// The DT states of every core fluid's `eos` grid (500 each) and of every fluid's in the all-fluid tier (8 each), the
/// phase imposed, on the `Corrected` data: each fluid with its states.
fn grid_states(registry: &Registry) -> Vec<(&Fluid, Vec<State>)> {
    let states = |fixture: &Fixture<'_>, fluid: &Fluid, rows: &[usize]| -> Vec<State> {
        let point = |row| (fixture.value(row, "T").unwrap(), fixture.value(row, "rhomolar").unwrap());
        rows.iter().map(|&row| point(row)).map(|(t, rho)| eos::state(fluid, t, rho).unwrap()).collect()
    };
    let mut grids = Vec::new();
    for (path, text) in eos::CORE_FILES {
        let fixture = Fixture::parse(path, text).unwrap();
        let fluid = registry.get(fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap()).unwrap();
        grids.push((fluid, states(&fixture, fluid, &(0..fixture.rows().len()).collect::<Vec<_>>())));
    }
    let (path, text) = fixture!("coolprop-8.0.0/all/eos.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    for (name, rows) in by_fluid(&fixture) {
        let fluid = registry.get(name).unwrap();
        grids.push((fluid, states(&fixture, fluid, &rows)));
    }
    grids
}

/// Identity (VERIFICATION.md §8.1; map 01 §8, map 02 §8; PLAN.md M7.10, M20.2): the conformance kit's `identities`,
/// the L4 list, μ = g included (ROT-062), and its order-3 part, at every core fluid's `eos` grid and every fluid's in
/// the all-fluid tier: stable, metastable and unstable states alike, the phase imposed, on the `Corrected` data. Class
/// `Identity`: 1e-12 of the largest term, 1e-8 in the near-critical window.
#[test]
fn identities_hold() {
    let registry = Registry::embedded().unwrap();
    let checked: usize = grid_states(registry)
        .iter()
        .map(|(fluid, states)| {
            phasekit_verify::identities(fluid, states).map_err(|m| (fluid.info().name(), m)).unwrap()
        })
        .sum();
    // Fifteen at each of the 8,088 states, and cp − cv and κ_T·ρ·w² from order 3 where cp, cv and w are defined; the
    // core grids' are M5.1's 70,000 and M7.10's 48,658.
    assert_eq!(checked, 137_074);
}

/// Identity (VERIFICATION.md §10; PLAN.md M20.2): the conformance kit's `part_additivity`, each model's ideal and
/// residual parts adding up to the bundle of each state of every core fluid's `eos` grid and every fluid's in the
/// all-fluid tier.
#[test]
fn parts_add_up_to_the_state() {
    let registry = Registry::embedded().unwrap();
    let checked: usize = grid_states(registry)
        .iter()
        .map(|(fluid, states)| {
            phasekit_verify::part_additivity(fluid, states).map_err(|m| (fluid.info().name(), m)).unwrap()
        })
        .sum();
    assert_eq!(checked, (14 * 500 + 136 * 8) * 6);
}

/// VERIFICATION.md §3.2, §11.3: the `eos` files come from the pinned runner image, like every committed oracle fixture.
#[test]
fn eos_fixtures_name_the_pinned_generator_environment() {
    assert_eq!(
        phasekit_verify::on_the_pinned_image(
            eos::CORE_FILES.into_iter().chain([fixture!("coolprop-8.0.0/all/eos.csv")])
        ),
        Ok(15)
    );
}

/// The `eos` fixtures' M5.5 columns of the core subset and the all-fluid tier against the `Parity` data: the partials,
/// or cp⁰ and the residual parts.
fn m5_5_columns(parts: bool) -> EosCheck {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = EosCheck::default();
    for (path, text) in eos::CORE_FILES {
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

fn order_three_columns(which: OrderThree) -> EosCheck {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = EosCheck::default();
    for (path, text) in eos::CORE_FILES {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let rows: Vec<usize> = (0..fixture.rows().len()).collect();
        check.order_three_rows(which, &fixture, &rows, registry.get(name).unwrap(), &record(&registry, name));
    }
    let (path, text) = fixture!("coolprop-8.0.0/all/eos.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    for (name, rows) in &by_fluid(&fixture) {
        check.order_three_rows(which, &fixture, rows, registry.get(name).unwrap(), &record(&registry, name));
    }
    check
}

/// Oracle: CoolProp 8.0.0 `second_partial_deriv`, the `eos` fixtures' 10 second partials (PLAN.md M7.10;
/// `eos::second_partials`) at 8,088 states, through `Fluid::prop`. Class `Prop`, floor |X|/(|Y|·|W|), with the
/// order-3 bundle's `Term` scales carried through the twice-differentiated Jacobian ratio.
#[test]
fn second_partials_match_oracle() {
    let check = order_three_columns(OrderThree::SecondPartials);
    assert_eq!(check.checked, (14 * 500 + 136 * 8) * 10);
    assert_eq!(check.report(30), None);
}

/// Oracle: CoolProp 8.0.0 `fundamental_derivative_of_gas_dynamics` at the `eos` fixtures' 8,088 states (PLAN.md M7.10):
/// Γ = 1 + ρ(∂²p/∂ρ²)_s/(2(∂p/∂ρ)_s), refused where w² ≤ 0 as the oracle's is nan there. Class `Prop`, floor 1.
#[test]
fn fundamental_derivative_matches_oracle() {
    let check = order_three_columns(OrderThree::Fundamental);
    assert_eq!(check.checked, 14 * 500 + 136 * 8);
    assert_eq!(check.report(30), None);
}

/// Oracle: CoolProp 8.0.0 `isothermal_compressibility` and `isobaric_expansion_coefficient` at the `eos` fixtures'
/// 8,088 states (PLAN.md M7.10): κ_T = (1/ρ)(∂ρ/∂p)_T and β = −(1/ρ)(∂ρ/∂T)_p from the order-2 bundle. Class `Prop`,
/// floors 1/|p| and 1/T.
#[test]
fn kappa_and_beta_match_oracle() {
    let check = order_three_columns(OrderThree::KappaBeta);
    assert_eq!(check.checked, (14 * 500 + 136 * 8) * 2);
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
    for (path, text) in eos::CORE_FILES {
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

/// A derivative string's output, read by compat's grammar as `PropsSI` reads it.
fn derivative(key: &str) -> Prop {
    let output = phasekit_compat::parse_output(key);
    let prop = match output {
        Some(phasekit_compat::Output::Prop(prop)) => Some(prop),
        _ => None,
    };
    prop.ok_or(format!("{key} reads as {output:?}")).unwrap()
}

/// c_v and c_p in both bases and w by CoolProp's names: the derivative variables of order 3 (PLAN.md M20.1).
const ORDER_THREE_VARIABLES: [&str; 5] = ["Cvmolar", "Cvmass", "Cpmolar", "Cpmass", "speed_of_sound"];

/// `x`'s central differences at (T, ρ) of `fluid`, relative step 1e-5 (VERIFICATION.md §8.3), (∂x/∂T)_ρ and (∂x/∂ρ)_T,
/// each with its truncation error estimated from the difference D at twice the step, |D(2h) − D(h)|/3 (a central
/// difference's error is ~h²); `None` where a state of the stencil refuses `x`.
fn central(fluid: &Fluid, x: Prop, t: f64, rho: f64) -> Option<[(f64, f64); 2]> {
    let at = |t, rho| eos::state(fluid, t, rho).ok().and_then(|s| fluid.prop(&s, x).ok());
    let in_t = |k: f64| Some((at(t + k * 1e-5 * t, rho)? - at(t - k * 1e-5 * t, rho)?) / (k * 2e-5 * t));
    let in_rho = |k: f64| Some((at(t, rho + k * 1e-5 * rho)? - at(t, rho - k * 1e-5 * rho)?) / (k * 2e-5 * rho));
    let (t1, t2, rho1, rho2) = (in_t(1.0)?, in_t(2.0)?, in_rho(1.0)?, in_rho(2.0)?);
    Some([(t1, (t2 - t1).abs() / 3.0), (rho1, (rho2 - rho1).abs() / 3.0)])
}

/// The running result of `Fd` checks: one line per failure, the largest error over its bound of those that held, and
/// the differences that cannot resolve the derivative.
#[derive(Default)]
struct FdCheck {
    checked: usize,
    failures: Vec<String>,
    headroom: f64,
    unresolved: usize,
}

impl FdCheck {
    /// `got` against the central difference `want`, in `Fd` at the larger of the two and `floor` (VERIFICATION.md §5),
    /// unless the difference's estimated truncation exceeds the bound at `want` (near a critical point c_p's curvature
    /// outgrows a step of 1e-5, and the identities check it there).
    fn check(&mut self, at: &str, (got, (want, truncation)): (f64, (f64, f64)), floor: f64) {
        if truncation > ToleranceClass::Fd.bound(want.abs().max(floor)).unwrap() {
            self.unresolved += 1;
            return;
        }
        let bound = ToleranceClass::Fd.bound(got.abs().max(want.abs()).max(floor)).unwrap();
        let error = (got - want).abs();
        self.checked += 1;
        if error <= bound {
            self.headroom = self.headroom.max(error / bound);
        } else {
            self.failures.push(format!("{at}: {got:?} against {want:?}, error {error:e} > {bound:e}"));
        }
    }
}

/// `v`'s first partials at `state` of `fluid`: (∂v/∂T)_ρ and (∂v/∂ρ)_T against its central differences along the held
/// variable (`Fd`, floors |v|/T and |v|/ρ); as the variable differentiated by and held, (∂p/∂v)_T·(∂v/∂ρ)_T = (∂p/∂ρ)_T
/// and the triple product (∂ρ/∂T)_v·(∂T/∂v)_ρ·(∂v/∂ρ)_T = −1 (`Identity`).
fn order_three_variable(
    checks: (&mut FdCheck, &mut IdentityCheck),
    at: (&str, Window),
    fluid: &Fluid,
    state: &State,
    v: &str,
) {
    let (at, window) = at;
    let (t, rho) = (state.t(), state.rho(Basis::Molar));
    let (Some([v_t, v_rho]), Ok(x)) = (central(fluid, derivative(v), t, rho), fluid.prop(state, derivative(v))) else {
        return;
    };
    let d = |key: String| fluid.prop(state, derivative(&key)).unwrap();
    let (fd, id) = checks;
    fd.check(&format!("{at}: d({v})/d(T)|Dmolar"), (d(format!("d({v})/d(T)|Dmolar")), v_t), x.abs() / t);
    fd.check(&format!("{at}: d({v})/d(Dmolar)|T"), (d(format!("d({v})/d(Dmolar)|T")), v_rho), x.abs() / rho);
    let (by, rho_v) = (d(format!("d(P)/d({v})|T")) * d(format!("d({v})/d(Dmolar)|T")), d("d(P)/d(Dmolar)|T".into()));
    id.check(at, &format!("(∂p/∂{v})_T·(∂{v}/∂ρ)_T"), (by, rho_v), &[], window);
    let held = [format!("d(Dmolar)/d(T)|{v}"), format!("d(T)/d({v})|Dmolar"), format!("d({v})/d(Dmolar)|T")];
    id.check(at, &format!("triple product at {v}"), (held.map(d).iter().product(), -1.0), &[], window);
}

/// τ and δ as derivative variables against the T and ρ ones, at the published reducing state (T_r, ρ_r): each key with
/// its base and the factor between them, from dτ/dT = −τ/T and dδ/dρ = δ/ρ, both constant where T is held; c_v, c_p and
/// w as the variable a second partial is differentiated by or at, against the first partials of the same derivative.
fn reduced_and_order_three_identities(check: &mut IdentityCheck, at: (&str, Window), fluid: &Fluid, state: &State) {
    let (at, window) = at;
    let d = |key: &str| fluid.prop(state, derivative(key)).unwrap();
    let (t, rho, tau, delta) = (state.t(), state.rho(Basis::Molar), d("Tau"), d("Delta"));
    let (tau_t, delta_rho) = (-tau / t, delta / rho);
    let scaled = [
        ("d(P)/d(Tau)|Delta", "d(P)/d(T)|Dmolar", 1.0 / tau_t),
        ("d(P)/d(Delta)|Tau", "d(P)/d(Dmolar)|T", 1.0 / delta_rho),
        ("d(Tau)/d(P)|Hmass", "d(T)/d(P)|Hmass", tau_t),
        ("d(Delta)/d(Hmolar)|Smolar", "d(Dmolar)/d(Hmolar)|Smolar", delta_rho),
        ("d(Hmass)/d(Smass)|Tau", "d(Hmass)/d(Smass)|T", 1.0),
        ("d(Tau)/d(Delta)|P", "d(T)/d(Dmolar)|P", tau_t / delta_rho),
        ("d(Cpmass)/d(Tau)|Delta", "d(Cpmass)/d(T)|Dmolar", 1.0 / tau_t),
        ("d(Umolar)/d(Delta)|Cvmolar", "d(Umolar)/d(Dmolar)|Cvmolar", 1.0 / delta_rho),
        ("d(d(P)/d(Tau)|Delta)/d(Delta)|Tau", "d(d(P)/d(T)|Dmolar)/d(Dmolar)|T", 1.0 / (tau_t * delta_rho)),
        ("d(d(P)/d(Delta)|Tau)/d(Tau)|Delta", "d(d(P)/d(Dmolar)|T)/d(T)|Dmolar", 1.0 / (tau_t * delta_rho)),
        ("d(d(Delta)/d(T)|P)/d(T)|P", "d(d(Dmolar)/d(T)|P)/d(T)|P", delta_rho),
        ("d(d(Hmass)/d(T)|P)/d(Tau)|P", "d(d(Hmass)/d(T)|P)/d(T)|P", 1.0 / tau_t),
        ("d(d(Delta)/d(Tau)|P)/d(P)|Tau", "d(d(Dmolar)/d(T)|P)/d(P)|T", delta_rho / tau_t),
    ];
    for (key, base, factor) in scaled {
        check.check(at, key, (d(key), factor * d(base)), &[], window);
    }
    check.check(at, "(∂²τ/∂T²)_p = 2τ/T²", (d("d(d(Tau)/d(T)|P)/d(T)|P"), 2.0 * tau / (t * t)), &[], window);
    // (∂h/∂τ)_p = c_p/(dτ/dT) = −c_p·T/τ, differentiated in T at p: −(T/τ)(∂c_p/∂T)_p − (2/τ)c_p.
    let by_tau = [-t / tau * d("d(d(Hmass)/d(T)|P)/d(T)|P"), -2.0 / tau * d("d(Hmass)/d(T)|P")];
    check.check(at, "(∂/∂T (∂h/∂τ)_p)_p", (d("d(d(Hmass)/d(Tau)|P)/d(T)|P"), by_tau.iter().sum()), &by_tau, window);
    // (∂F/∂W)_V = (∂F/∂T)_V/(∂W/∂T)_V, and (∂F/∂T)_W = F_T − F_ρ·W_T/W_ρ in (T, ρ).
    let outer = [
        ("d(d(P)/d(T)|Dmolar)/d(Cvmolar)|Dmolar", "d(d(P)/d(T)|Dmolar)/d(T)|Dmolar", "d(Cvmolar)/d(T)|Dmolar"),
        ("d(d(Smolar)/d(T)|P)/d(Cpmolar)|P", "d(d(Smolar)/d(T)|P)/d(T)|P", "d(Cpmolar)/d(T)|P"),
        (
            "d(d(Hmolar)/d(P)|Smolar)/d(speed_of_sound)|T",
            "d(d(Hmolar)/d(P)|Smolar)/d(Dmolar)|T",
            "d(speed_of_sound)/d(Dmolar)|T",
        ),
    ];
    for (key, f, w) in outer {
        check.check(at, key, (d(key), d(f) / d(w)), &[], window);
    }
    let held = [
        ("d(d(P)/d(T)|Dmolar)/d(T)|Cpmass", "d(d(P)/d(T)|Dmolar)", "T", "Dmolar", "Cpmass"),
        ("d(d(P)/d(Dmolar)|T)/d(Dmolar)|Cvmass", "d(d(P)/d(Dmolar)|T)", "Dmolar", "T", "Cvmass"),
    ];
    for (key, first, along, other, w) in held {
        let f = [d(&format!("{first}/d({along})|{other}")), d(&format!("{first}/d({other})|{along}"))];
        let ratio = d(&format!("d({w})/d({along})|{other}")) / d(&format!("d({w})/d({other})|{along}"));
        let terms = [f[0], f[1] * ratio];
        check.check(at, key, (d(key), terms[0] - terms[1]), &terms, window);
    }
}

/// Fd and Identity (VERIFICATION.md §8.3, §8.1; PLAN.md M20.1, user decision OD1): c_v, c_p and w as derivative
/// variables through the order-3 bundle, and τ and δ through the published reducing state, by the strings of compat's
/// grammar, at the stable states of every core fluid's `eos` grid (the phase imposed): each of c_v, c_p and w
/// differentiated in T and ρ against its central differences (`Fd`, where the difference resolves the derivative), then
/// differentiated by and held, τ and δ against the T and ρ derivatives scaled by the reducing state, and c_v, c_p and w
/// as a second partial's outer variables (`Identity`). No oracle: CoolProp's second derivatives refuse c_v, c_p and w.
#[test]
fn derivative_variables_match_central_differences() {
    let registry = Registry::embedded().unwrap();
    let (mut fd, mut id) = (FdCheck::default(), IdentityCheck::default());
    for (path, text) in eos::CORE_FILES {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let (fluid, region) = (registry.get(name).unwrap(), fixture.columns().iter().position(|c| *c == "region"));
        let critical = fluid.model().critical_point().unwrap();
        for row in
            (0..fixture.rows().len()).filter(|&row| fixture.rows()[row].cells[region.unwrap()] == Cell::Text("stable"))
        {
            let (t, rho) = (fixture.value(row, "T").unwrap(), fixture.value(row, "rhomolar").unwrap());
            let (state, at) = (eos::state(fluid, t, rho).unwrap(), format!("{name} at {t} K, {rho} mol/m³"));
            let window = Window::at(t, rho, critical.t, critical.rho);
            for v in ORDER_THREE_VARIABLES {
                order_three_variable((&mut fd, &mut id), (&at, window), fluid, &state, v);
            }
            reduced_and_order_three_identities(&mut id, (&at, window), fluid, &state);
        }
    }
    let shown: Vec<&String> = fd.failures.iter().take(20).collect();
    assert!(fd.failures.is_empty(), "{} of {} outside Fd: {shown:#?}", fd.failures.len(), fd.checked);
    // Ten differences and 30 identities at each of the 5,883 stable states. On x86-64 Linux 44 differences are
    // unresolved, (∂c_p/∂T)_ρ within a few kelvin of HFE143m's, Methanol's and Nitrogen's critical points; another
    // target's maths may move one across the bound.
    assert_eq!((fd.checked + fd.unresolved, id.checked), (58_830, 176_490));
    assert!(fd.unresolved <= 48, "{} unresolved", fd.unresolved);
    assert_eq!(id.report(20), None);
}
