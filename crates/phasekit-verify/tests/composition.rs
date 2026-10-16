//! A multi-fluid model's composition derivatives (PLAN.md M13.6): identities they meet, their agreement with
//! differences of the residual itself, and the oracle's fugacity coefficients.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Order, Registry};
use phasekit_mix::{Composition, MixtureModel, MultiFluid, Var};
use phasekit_verify::ToleranceClass;

/// The library's and GERG-2008's models the checks run on, with a composition each: Lemmon's ξ/ζ pair with its
/// departure (R32/R125), a GERG pair with one (Methane/Ethane), a Lemmon ternary (R32/R125/R134a) and a GERG
/// natural gas of 21 components (AGA8's check composition).
fn models() -> Vec<(String, MultiFluid, Vec<f64>)> {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let library = |names: &[&str]| MultiFluid::new(&registry, names, DataSet::Corrected).unwrap();
    let gas = [
        0.77824, 0.02, 0.06, 0.08, 0.03, 0.0015, 0.003, 0.0005, 0.00165, 0.00215, 0.00088, 0.00024, 0.00015, 0.00009,
        0.004, 0.005, 0.002, 0.0001, 0.0025, 0.007, 0.001,
    ];
    let gerg_names = [
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
    vec![
        ("R32&R125".into(), library(&["R32", "R125"]), vec![0.4, 0.6]),
        ("Methane&Ethane".into(), library(&["Methane", "Ethane"]), vec![0.7, 0.3]),
        ("R32&R125&R134a".into(), library(&["R32", "R125", "R134a"]), vec![0.2, 0.5, 0.3]),
        ("GERG2008 natural gas".into(), MultiFluid::gerg2008(&registry, &gerg_names).unwrap(), gas.to_vec()),
    ]
}

/// Each model's composition derivatives to order 3 at a gas and a dense state of its own reducing state: (τ, δ) =
/// (0.8, 0.3) and (1.3, 2.2).
fn cases() -> Vec<(String, Composition, Vec<f64>)> {
    let mut out = Vec::new();
    for (name, mix, x) in models() {
        let (t_r, rho_r) = mix.reducing_state(&x);
        for (tau, delta) in [(0.8, 0.3), (1.3, 2.2)] {
            let c = mix.composition(t_r / tau, delta * rho_r, &x, Order::Three).unwrap();
            out.push((format!("{name} at tau {tau}, delta {delta}"), c, x.clone()));
        }
    }
    out
}

/// `Identity`'s bound (VERIFICATION.md §5) on a sum: 1e-12 of its largest term.
fn identity(terms: &[f64]) -> f64 {
    ToleranceClass::Identity.bound(terms.iter().fold(0.0, |m, t| m.max(t.abs()))).unwrap()
}

/// Ψ = n α^r(T, n/V, n/n) of mole numbers `n` and volume `v`, from the model's own residual: what the derivatives are
/// differentiated against.
fn psi(mix: &MultiFluid, t: f64, n: &[f64], v: f64) -> f64 {
    let total: f64 = n.iter().sum();
    let x: Vec<f64> = n.iter().map(|ni| ni / total).collect();
    total * mix.residual(&x, t, total / v, Order::One).unwrap().get(0, 0).unwrap()
}

/// `Fd`'s central difference (VERIFICATION.md §5) in `at`'s entry `i`, relative step 1e-5 of its scale: T's and V's
/// own, and for a mole number the total (one mole): a trace component's own mole number, 1e-4, would leave a step at
/// the rounding of Ψ.
fn central(f: &dyn Fn(&[f64]) -> f64, at: &[f64], i: usize) -> f64 {
    central_by(f, at, i, 1e-5)
}

/// Richardson's extrapolation of [`central_by`] at `step` and `step`/2: its h² error cancelled.
fn richardson(f: &dyn Fn(&[f64]) -> f64, at: &[f64], i: usize, step: f64) -> f64 {
    (4.0 * central_by(f, at, i, step / 2.0) - central_by(f, at, i, step)) / 3.0
}

/// [`central`] with relative step `step`.
fn central_by(f: &dyn Fn(&[f64]) -> f64, at: &[f64], i: usize, step: f64) -> f64 {
    let moles = at.len() - 2;
    let h = step * if i < moles { at[..moles].iter().sum() } else { at[i].abs() };
    let (mut up, mut down) = (at.to_vec(), at.to_vec());
    (up[i], down[i]) = (at[i] + h, at[i] - h);
    (f(&up) - f(&down)) / (2.0 * h)
}

/// ROT-210 (CoolProp's composition derivatives are hand-coded, one suspected of a missing factor 2): Ψ's first
/// derivatives in n_i, T and V against central differences of the model's own residual, its second and third against
/// central differences of the derivatives one order lower, all within `Fd` of the larger of the entry and its row's
/// largest; the second and third of the models whose fractions are all 0.1 or more (each pair and triple once, the
/// derivatives being symmetric).
#[test]
fn composition_derivatives_match_fd() {
    for (name, mix, x) in models() {
        let (t_r, rho_r) = mix.reducing_state(&x);
        let (t, rho) = (t_r / 0.9, 0.8 * rho_r);
        let n = x.len();
        let vars: Vec<Var> = (0..n).map(Var::N).chain([Var::T, Var::V]).collect();
        // Ψ's arguments as one vector: n_1 ... n_N, T, V.
        let at: Vec<f64> = x.iter().copied().chain([t, 1.0 / rho]).collect();
        let derivative = |order: Order, idx: &[Var], args: &[f64]| {
            let (moles, (t, v)) = (&args[..n], (args[n], args[n + 1]));
            let total: f64 = moles.iter().sum();
            let fractions: Vec<f64> = moles.iter().map(|m| m / total).collect();
            // Derivatives at `total` moles from those at one mole: Ψ is homogeneous of degree 1 in (n, V).
            let c = mix.composition(t, total / v, &fractions, order).unwrap();
            let degree = idx.iter().filter(|v| **v != Var::T).count() as i32;
            c.psi(idx) * phasekit_core::math::powi(total, 1 - degree)
        };
        let check = |what: String, got: f64, want: f64, row: f64| {
            let bound = ToleranceClass::Fd.bound(got.abs().max(want.abs()).max(row)).unwrap();
            assert!((got - want).abs() <= bound, "{name} {what}: {got} against {want}");
        };
        let plain = |args: &[f64]| psi(&mix, args[n], &args[..n], args[n + 1]);
        let first: Vec<f64> = (0..n + 2).map(|i| derivative(Order::One, &[vars[i]], &at)).collect();
        let largest = first.iter().fold(0.0_f64, |m, d| m.max(d.abs()));
        for (i, var) in vars.iter().enumerate() {
            check(format!("dΨ/d{var:?}"), first[i], central(&plain, &at, i), 1e-3 * largest);
        }
        // The components, T and V of the models whose fractions are all 0.1 or more: in the gas a difference in a
        // trace component's mole number (n-decane's 9e-5, nitrogen's 0.02 against its pairs with them) is rounding- or
        // truncation-limited either way, so its higher orders are num-dual's (`src/composition.rs`).
        if x.iter().any(|xi| *xi < 0.1) {
            continue;
        }
        let pairs: Vec<(usize, usize)> = (0..n + 2).flat_map(|i| (0..=i).map(move |j| (i, j))).collect();
        for &(i, j) in &pairs {
            let d2 = derivative(Order::Two, &[vars[i], vars[j]], &at);
            let fd = central(&|a: &[f64]| derivative(Order::One, &[vars[i]], a), &at, j);
            check(format!("d²Ψ/d{:?}d{:?}", vars[i], vars[j]), d2, fd, 1e-3 * d2.abs());
        }
        for (i, j, l) in pairs.iter().flat_map(|&(i, j)| (0..=j).map(move |l| (i, j, l))) {
            let d3 = derivative(Order::Three, &[vars[i], vars[j], vars[l]], &at);
            let fd = central(&|a: &[f64]| derivative(Order::Two, &[vars[i], vars[j]], a), &at, l);
            check(format!("d³Ψ/d{:?}d{:?}d{:?}", vars[i], vars[j], vars[l]), d3, fd, 1e-3 * d3.abs());
        }
    }
}

/// The models whose fractions are all 0.1 or more, at a gas state of their reducing state, (τ, δ) = (0.8, 0.3), and a
/// compressed liquid at τ = 1.3, the first δ of 2.2, 2.4, ... where p is above 1 MPa (ln φ needs p > 0).
fn balanced_states() -> Vec<(String, MultiFluid, Vec<f64>, f64, f64)> {
    let mut out = Vec::new();
    for (name, mix, x) in models().into_iter().filter(|(_, _, x)| x.iter().all(|xi| *xi >= 0.1)) {
        let (t_r, rho_r) = mix.reducing_state(&x);
        let p = |tau: f64, delta: f64| mix.composition(t_r / tau, delta * rho_r, &x, Order::One).unwrap().pressure();
        let liquid = (0..10).map(|k| 2.2 + 0.2 * f64::from(k)).find(|delta| p(1.3, *delta) > 1e6).unwrap();
        for (tau, delta) in [(0.8, 0.3), (1.3, liquid)] {
            out.push((format!("{name} at tau {tau}, delta {delta}"), mix.clone(), x.clone(), t_r / tau, delta * rho_r));
        }
    }
    out
}

/// `Fd`'s check of `got` against `want`, on the larger of the two and `row`.
fn check_fd(what: &str, got: f64, want: f64, row: f64) {
    let bound = ToleranceClass::Fd.bound(got.abs().max(want.abs()).max(row)).unwrap();
    assert!((got - want).abs() <= bound, "{what}: {got} against {want}");
}

/// p, Z and the pressure's derivatives: p against the residual's own, ρRT(1 + δ α^r_δ), within `Prop`; Z = p/(ρRT);
/// (∂p/∂T)_(V,n), (∂p/∂V)_(T,n) and (∂p/∂n_i)_(T,V) against central differences of that p (`Fd`).
#[test]
fn pressure_and_its_derivatives_match_fd() {
    for (name, mix, x, t, rho) in balanced_states() {
        let n = x.len();
        let p = |args: &[f64]| {
            let (moles, t, v) = (&args[..n], args[n], args[n + 1]);
            let total: f64 = moles.iter().sum();
            let fractions: Vec<f64> = moles.iter().map(|m| m / total).collect();
            let a01 = mix.residual(&fractions, t, total / v, Order::One).unwrap().get(0, 1).unwrap();
            total / v * mix.gas_constant() * t * (1.0 + a01)
        };
        let at: Vec<f64> = x.iter().copied().chain([t, 1.0 / rho]).collect();
        let c = mix.composition(t, rho, &x, Order::Two).unwrap();
        let p0 = p(&at);
        let prop = ToleranceClass::Prop.bound(p0.abs()).unwrap();
        assert!((c.pressure() - p0).abs() <= prop, "{name} p: {} against {p0}", c.pressure());
        let z = p0 / (rho * mix.gas_constant() * t);
        assert!((c.z() - z).abs() <= ToleranceClass::Prop.bound(z.abs()).unwrap(), "{name} Z: {} against {z}", c.z());
        check_fd(&format!("{name} dp/dT"), c.dp_dt(), central(&p, &at, n), 0.0);
        check_fd(&format!("{name} dp/dV"), c.dp_dv(), central(&p, &at, n + 1), 0.0);
        let row = (0..n).fold(0.0_f64, |m, i| m.max(c.dp_dn(i).abs()));
        for i in 0..n {
            check_fd(&format!("{name} dp/dn_{i}"), c.dp_dn(i), central(&p, &at, i), 1e-3 * row);
        }
    }
}

/// The composition at T, p and mole numbers `moles`: its density solved by Newton's method on the composition's own p
/// and ∂p/∂V from `rho`, to the rounding of p.
fn at_t_p(mix: &MultiFluid, t: f64, p: f64, moles: &[f64], mut rho: f64) -> Composition {
    let total: f64 = moles.iter().sum();
    let x: Vec<f64> = moles.iter().map(|m| m / total).collect();
    for _ in 0..30 {
        let c = mix.composition(t, rho, &x, Order::Two).unwrap();
        // dp/dρ = −V² ∂p/∂V at one mole, V = 1/ρ.
        let step = (c.pressure() - p) / (-c.dp_dv() / (rho * rho));
        rho -= step;
        if step.abs() <= 1e-15 * rho {
            break;
        }
    }
    mix.composition(t, rho, &x, Order::Two).unwrap()
}

/// The derivatives at constant pressure against central differences with each state's density solved again at (T, p)
/// (`Fd`): n (∂ln φ_i/∂n_j)_(T,p), (∂ln φ_i/∂T)_(p,n), (∂ln φ_i/∂p)_(T,n) and the partial molar volumes
/// (∂V/∂n_i)_(T,p). The differences are Richardson's at steps 1e-4 and 5e-5: in the liquid a density solved to its
/// rounding moves ln φ by about 1e-13, which a step of 1e-5 carries to 1e-8 (R32/R125/R134a's n ∂ln φ_1/∂n_2: 3.4e-9
/// from the derivative), and in the gas a plain step of 1e-4 leaves h² error on the small entries (2.4e-7 of its
/// n ∂ln φ_2/∂n_3).
#[test]
fn derivatives_at_constant_pressure_match_fd() {
    let mut checked = 0;
    for (name, mix, x, t, rho) in balanced_states() {
        let n = x.len();
        checked += n * (n + 3);
        let c = mix.composition(t, rho, &x, Order::Two).unwrap();
        let p = c.pressure();
        // ln φ_i and the volume of (n_1 ... n_N, T, p).
        let state = |args: &[f64]| at_t_p(&mix, args[n], args[n + 1], &args[..n], rho);
        let at: Vec<f64> = x.iter().copied().chain([t, p]).collect();
        for i in 0..n {
            let ln_phi = |args: &[f64]| state(args).ln_phi(i);
            check_fd(&format!("{name} dln phi_{i}/dT"), c.dln_phi_dt(i), richardson(&ln_phi, &at, n, 1e-4), 0.0);
            check_fd(&format!("{name} dln phi_{i}/dp"), c.dln_phi_dp(i), richardson(&ln_phi, &at, n + 1, 1e-4), 0.0);
            let row = (0..n).fold(0.0_f64, |m, j| m.max(c.n_dln_phi_dn(i, j).abs()));
            for j in 0..n {
                check_fd(
                    &format!("{name} n dln phi_{i}/dn_{j}"),
                    c.n_dln_phi_dn(i, j),
                    richardson(&ln_phi, &at, j, 1e-4),
                    1e-3 * row,
                );
            }
            let volume = |args: &[f64]| args[..n].iter().sum::<f64>() * state(args).volume();
            check_fd(&format!("{name} v_{i}"), c.partial_molar_volume(i), richardson(&volume, &at, i, 1e-4), 0.0);
        }
    }
    assert_eq!(checked, 2 * (10 + 10 + 18));
}

/// `composition` refuses mole fractions of another number and a T or ρ that is not positive and finite, each alone,
/// and an order above 3; both `components` count the model's.
#[test]
fn composition_refuses_bad_input() {
    let (_, mix, x) = models().swap_remove(0);
    let (t, rho) = (300.0, 1000.0);
    assert_eq!((mix.components(), mix.composition(t, rho, &x, Order::One).unwrap().components()), (2, 2));
    assert!(mix.composition(t, rho, &[1.0], Order::One).is_err());
    assert!(mix.composition(t, rho, &x, Order::Four).is_err());
    let bad =
        [(0.0, rho), (-1.0, rho), (f64::INFINITY, rho), (f64::NAN, rho), (t, 0.0), (t, f64::INFINITY), (t, f64::NAN)];
    for (t, rho) in bad {
        assert!(mix.composition(t, rho, &x, Order::One).is_err(), "T {t}, rho {rho}");
    }
}

/// Gibbs–Duhem (ROT-191): Σ_i x_i n (∂ln φ_i/∂n_j)_(T,p) = 0 for every j, within `Identity`.
#[test]
fn gibbs_duhem_holds() {
    for (name, c, x) in cases() {
        for j in 0..x.len() {
            let terms: Vec<f64> = (0..x.len()).map(|i| x[i] * c.n_dln_phi_dn(i, j)).collect();
            let sum: f64 = terms.iter().sum();
            assert!(sum.abs() <= identity(&terms), "{name}, column {j}: {sum:e}");
        }
    }
}

/// n (∂ln φ_i/∂n_j)_(T,p) is symmetric in i and j (ROT-191: CoolProp's second-order Hessians double-count their ideal
/// term), within `Identity` of the larger entry.
#[test]
fn ln_phi_hessian_is_symmetric() {
    for (name, c, x) in cases() {
        for i in 0..x.len() {
            for j in 0..i {
                let (a, b) = (c.n_dln_phi_dn(i, j), c.n_dln_phi_dn(j, i));
                assert!((a - b).abs() <= identity(&[a, b]), "{name}: ({i}, {j}) {a} against {b}");
            }
        }
    }
}

/// Ψ is homogeneous of degree 1 in (n, V) at fixed T, so Ψ = Σ n_i ∂Ψ/∂n_i + V ∂Ψ/∂V, its first derivatives of degree
/// 0 (Σ_j n_j ∂²Ψ/∂n_i∂n_j + V ∂²Ψ/∂n_i∂V = 0) and its second of degree −1 (Σ_k n_k ∂³Ψ/∂n_i∂n_j∂n_k + V ∂³Ψ/∂n_i∂n_j∂V
/// = −∂²Ψ/∂n_i∂n_j), within `Identity`, the one mole at V = 1/ρ.
#[test]
fn euler_homogeneity() {
    for (name, c, x) in cases() {
        let volume = c.volume();
        let mut terms: Vec<f64> = (0..x.len()).map(|i| x[i] * c.psi(&[Var::N(i)])).collect();
        terms.push(volume * c.psi(&[Var::V]));
        let sum: f64 = terms.iter().sum();
        assert!((sum - c.psi(&[])).abs() <= identity(&[&terms[..], &[c.psi(&[])]].concat()), "{name}: Ψ");
        for i in 0..x.len() {
            let mut terms: Vec<f64> = (0..x.len()).map(|j| x[j] * c.psi(&[Var::N(i), Var::N(j)])).collect();
            terms.push(volume * c.psi(&[Var::N(i), Var::V]));
            let sum: f64 = terms.iter().sum();
            assert!(sum.abs() <= identity(&terms), "{name}: ∂Ψ/∂n_{i}");
            for j in 0..=i {
                let mut terms: Vec<f64> =
                    (0..x.len()).map(|k| x[k] * c.psi(&[Var::N(i), Var::N(j), Var::N(k)])).collect();
                terms.push(volume * c.psi(&[Var::N(i), Var::N(j), Var::V]));
                terms.push(c.psi(&[Var::N(i), Var::N(j)]));
                let sum: f64 = terms.iter().sum();
                assert!(sum.abs() <= identity(&terms), "{name}: ∂²Ψ/∂n_{i}∂n_{j}");
            }
        }
    }
}

/// Oracle: CoolProp 8.0.0's fugacity coefficients (`mix_eos`'s `fugacity_coefficient_1` and `_2`, PLAN.md M13.6) for
/// every pair of its library at z₁ = 0.4 and two states of its reducing state, on `Parity`: φ_i = exp(ln φ_i) within
/// `Prop`. Where the state's pressure is negative ln Z, and so φ, is undefined on both sides (NaN agrees with NaN).
#[test]
fn fugacity_coefficients_match_the_oracle() {
    use phasekit_verify::{Fixture, fixture};
    let (path, text) = fixture!("coolprop-8.0.0/all/mix_eos.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut compared, mut misses) = (0, Vec::new());
    for row in 0..table.rows().len() {
        let label = table.printed(row, "mixture").unwrap();
        if label.ends_with(".mix") || table.printed(row, "status") != Some("ok") {
            continue;
        }
        let names: Vec<&str> = label.split('&').collect();
        let mix = MultiFluid::new(&registry, &names, DataSet::Parity).unwrap();
        let z1 = table.value(row, "z1").unwrap();
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rhomolar").unwrap());
        let c = mix.composition(t, rho, &[z1, 1.0 - z1], Order::One).unwrap();
        for (i, column) in ["fugacity_coefficient_1", "fugacity_coefficient_2"].into_iter().enumerate() {
            let got = phasekit_core::math::exp(c.ln_phi(i));
            if let Err(e) = table.check_scaled(row, column, got, got.abs()) {
                misses.push(format!("{label} row {row} {column}: {e:?}"));
            }
            compared += 1;
        }
    }
    assert!(misses.is_empty(), "{} misses: {:#?}", misses.len(), &misses[..misses.len().min(12)]);
    assert_eq!(compared, 2 * 1680);
}

/// DIV-0052's replacing check for the chemical potentials (PLAN.md M13.6): Euler's g = Σ x_i μ_i, the mixture's
/// molar Gibbs energy at (T, ρ) against its chemical potentials, within `Identity` of the largest term, for the pairs of
/// `models` at their two states.
#[test]
fn chemical_potentials_sum_to_g() {
    use phasekit_core::{Basis, DomainPolicy, FlashOptions, Input, Pair, Phase};
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    for (names, x) in [(["R32", "R125"], [0.4, 0.6]), (["Methane", "Ethane"], [0.7, 0.3])] {
        let parts = [(names[0], x[0]), (names[1], x[1])];
        let fluid = phasekit_mix::mixture(&registry, &parts, DataSet::Corrected).unwrap();
        let mix = (fluid.model() as &dyn std::any::Any).downcast_ref::<phasekit_mix::Mixture>().unwrap();
        let (t_r, rho_r) = mix.model().reducing_state(&x);
        for (tau, delta) in [(0.8, 0.3), (1.3, 2.2)] {
            let (t, rho) = (t_r / tau, delta * rho_r);
            let options = FlashOptions::new().with_phase(Phase::Gas).with_domain(DomainPolicy::Extrapolate);
            let g = fluid.flash(Input::new(Pair::DT, rho, t, Basis::Molar).unwrap(), &options).unwrap().g(Basis::Molar);
            let mu = mix.chemical_potentials(t, rho).unwrap();
            let terms = [x[0] * mu[0], x[1] * mu[1], g];
            let sum = terms[0] + terms[1];
            assert!((sum - g).abs() <= identity(&terms), "{names:?} at tau {tau}: {sum} against {g}");
        }
    }
}
