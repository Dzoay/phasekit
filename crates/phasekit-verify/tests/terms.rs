//! L1, two derivative mechanisms (VERIFICATION.md §2, §9.3): the in-house `Jet4` against num-dual on every `Real`
//! method, and the power-term fast path (`accumulate`, reached through a compiled record) against num-dual AD of the
//! paper formula on the core subset's real terms. Class `Term`, scale [`majorant`] (VERIFICATION.md §5).

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use num_dual::{DualNum, HyperDual};
use phasekit_core::internal::{EosRecord, FluidRecord};
use phasekit_core::{Basis, Error, HelmholtzModel, Jet4, Order, Phase, Prop, Real, Registry, State, ThermoModel};
use phasekit_verify::{SplitMix64, ToleranceClass, majorant, term};

/// A nested hyper-dual with four ε directions (outer: 1 and 2, inner: 3 and 4): one evaluation gives one mixed
/// derivative up to order 4, `∂ⁿf/∂x_1…∂x_n` for the variables seeded in directions 1..n.
type D4 = HyperDual<HyperDual<f64>>;

/// `x` seeded in the directions where `seeds` is true.
fn var(x: f64, seeds: [bool; 4]) -> D4 {
    let one = |seeded: bool| if seeded { 1.0 } else { 0.0 };
    let constant = |v: f64| HyperDual::new(v, 0.0, 0.0, 0.0);
    let inner = HyperDual::new(x, one(seeds[2]), one(seeds[3]), 0.0);
    HyperDual::new(inner, constant(one(seeds[0])), constant(one(seeds[1])), constant(0.0))
}

/// The ε-part of directions 1..n.
fn part(f: &D4, n: usize) -> f64 {
    let outer = match n {
        0 => &f.re,
        1 => &f.eps1,
        _ => &f.eps1eps2,
    };
    match n {
        0..=2 => outer.re,
        3 => outer.eps1,
        _ => outer.eps1eps2,
    }
}

/// τ and δ seeded for `∂^(i+j)/∂τ^i∂δ^j`: τ in the first i directions, δ in the next j.
fn tau_delta(tau: f64, delta: f64, i: usize, j: usize) -> (D4, D4) {
    let seeds = |lo: usize, hi: usize| [0, 1, 2, 3].map(|k| (lo..hi).contains(&k));
    (var(tau, seeds(0, i)), var(delta, seeds(i, i + j)))
}

/// Every (i, j) with i + j ≤ 4.
fn entries() -> impl Iterator<Item = (usize, usize)> {
    (0..=4).flat_map(|n| (0..=n).map(move |i| (i, n - i)))
}

/// A `Real` method and the same function in num-dual.
struct Method {
    name: &'static str,
    jet: fn(Jet4) -> Jet4,
    dual: fn(&D4) -> D4,
    /// Scales the inner function's argument (small for expm1 and ln_1p, whose point is accuracy near 0).
    scale: f64,
}

const METHODS: [Method; 14] = [
    Method { name: "exp", jet: |x| x.exp(), dual: |x| x.exp(), scale: 1.0 },
    Method { name: "expm1", jet: |x| x.expm1(), dual: |x| x.exp_m1(), scale: 1e-6 },
    Method { name: "ln", jet: |x| x.ln(), dual: |x| x.ln(), scale: 1.0 },
    Method { name: "ln_1p", jet: |x| x.ln_1p(), dual: |x| x.ln_1p(), scale: 1e-6 },
    Method { name: "powi(-3)", jet: |x| x.powi(-3), dual: |x| x.powi(-3), scale: 1.0 },
    Method { name: "powi(5)", jet: |x| x.powi(5), dual: |x| x.powi(5), scale: 1.0 },
    Method { name: "powf(2.5)", jet: |x| x.powf(2.5), dual: |x| x.powf(2.5), scale: 1.0 },
    Method { name: "powf(-1.5)", jet: |x| x.powf(-1.5), dual: |x| x.powf(-1.5), scale: 1.0 },
    Method { name: "sqrt", jet: |x| x.sqrt(), dual: |x| x.sqrt(), scale: 1.0 },
    Method { name: "sinh", jet: |x| x.sinh(), dual: |x| x.sinh(), scale: 1.0 },
    Method { name: "cosh", jet: |x| x.cosh(), dual: |x| x.cosh(), scale: 1.0 },
    Method { name: "atan", jet: |x| x.atan(), dual: |x| x.atan(), scale: 1.0 },
    Method { name: "(x+2)/x", jet: |x| (x + 2.0) / x, dual: |x| (*x + 2.0) / *x, scale: 1.0 },
    Method { name: "-x²-x", jet: |x| -(x * x) - x, dual: |x| -(*x * *x) - *x, scale: 1.0 },
];

/// AD oracle: num-dual 0.15. Each method `m` of `Real` (and the operators: a quotient, a negated difference) composed with `g = s·(τδ + τ)` at 100 SplitMix64 points
/// (τ, δ) ∈ [0.5, 2]², all 15 derivatives to order 4 against num-dual's, class `Term`. The scale is the Faà di Bruno
/// expansion's: `Σ_n |m⁽ⁿ⁾(g₀)|/n! (g − g₀)ⁿ`, whose powers have positive coefficients here (τ₀, δ₀ > 0), so `Jet4`
/// arithmetic computes it without cancellation.
#[test]
fn jet4_matches_num_dual_on_every_real_method() {
    let mut rng = SplitMix64::new(1);
    let (mut failures, mut checked) = (Vec::new(), 0);
    for _ in 0..100 {
        let (tau, delta) = (rng.uniform(0.5, 2.0), rng.uniform(0.5, 2.0));
        for m in &METHODS {
            let g = |t: Jet4, d: Jet4| (t * d + t) * m.scale;
            let inner = g(Jet4::tau(tau), Jet4::delta(delta));
            let got = (m.jet)(inner);
            // |m⁽ⁿ⁾(g₀)| from num-dual on one variable, then the expansion's scale.
            let g0 = inner.value();
            let eps = inner + -g0;
            let mut power = Jet4::constant(1.0);
            let mut scale = Jet4::constant(0.0);
            for n in 0..=4 {
                let x = var(g0, [0, 1, 2, 3].map(|k| k < n));
                let factorial = [1.0, 1.0, 2.0, 6.0, 24.0][n];
                scale = scale + power * (part(&(m.dual)(&x), n).abs() / factorial);
                power = power * eps;
            }
            for (i, j) in entries() {
                let (t, d) = tau_delta(tau, delta, i, j);
                let want = part(&(m.dual)(&((t * d + t) * m.scale)), i + j);
                let (got, bound) = (got.derivative(i, j).unwrap(), scale.derivative(i, j).unwrap());
                checked += 1;
                if (got - want).abs() > ToleranceClass::Term.bound(bound).unwrap() {
                    failures.push(format!("{} at ({tau}, {delta}), ∂{i}{j}: jet {got:e}, num-dual {want:e}", m.name));
                }
            }
        }
    }
    assert_eq!(checked, 100 * 14 * 15);
    assert!(failures.is_empty(), "{} of {checked} outside Term:\n{}", failures.len(), failures.join("\n"));
}

/// 300 SplitMix64 points for a fluid's terms: τ ~ U[T_r/Tmax, T_r/Tmin], δ ~ logU[1e-12, ρ_max/ρ_r], 10 of them at
/// δ = 1e-8, 10 at δ = 1e-12 and 10 within 1e-2 of τ = δ = 1 (VERIFICATION.md §9.3).
fn points(record: &FluidRecord) -> Vec<(f64, f64)> {
    let e = &record.eos;
    let (tau_lo, tau_hi) = (e.t_reducing / record.limits.t_max(), e.t_reducing / record.limits.t_min());
    let mut rng = SplitMix64::new(1);
    (0..300)
        .map(|k| {
            let tau = if k >= 290 { 1.0 + rng.uniform(-1e-2, 1e-2) } else { rng.uniform(tau_lo, tau_hi) };
            let delta = match k {
                0..270 => rng.log_uniform(1e-12, e.rho_max / e.rho_reducing),
                270..280 => 1e-8,
                280..290 => 1e-12,
                _ => 1.0 + rng.uniform(-1e-2, 1e-2),
            };
            (tau, delta)
        })
        .collect()
}

/// `eos` (one kind's terms of `fluid`) compiled alone, at the [`points`] of `fluid`: all 15 `A_ij` of `residual`
/// against num-dual AD of `paper`, class `Term` with the entry's `scale`. Returns the entries checked; panics with the
/// first 20 failures.
fn check_ad(
    fluid: &str,
    select: impl Fn(&EosRecord, &mut EosRecord),
    paper: impl Fn(&EosRecord, &D4, &D4) -> D4,
    scale: impl Fn(&EosRecord, f64, f64, usize, usize) -> f64,
) -> usize {
    let record = phasekit_core::internal::record(Registry::embedded().unwrap(), fluid).unwrap();
    let e = &record.eos;
    let mut eos = EosRecord::new(e.gas_constant, e.t_reducing, e.rho_reducing, e.rho_max);
    select(e, &mut eos);
    let model = FluidRecord::new(fluid, record.molar_mass, record.source.clone(), eos.clone(), record.limits);
    let model = model.compile().unwrap();
    let (t_r, rho_r) = (e.t_reducing, e.rho_reducing);
    let (mut failures, mut checked) = (Vec::new(), 0);
    for (tau, delta) in points(&record) {
        // The model's own τ and δ, from the (T, ρ) it is called with.
        let (t, rho) = (t_r / tau, delta * rho_r);
        let (tau, delta) = (t_r / t, rho / rho_r);
        let got = model.eos().residual(t, rho, Order::Four);
        for (i, j) in entries() {
            let (x, y) = tau_delta(tau, delta, i, j);
            let factor = phasekit_core::math::powi(tau, i as i32) * phasekit_core::math::powi(delta, j as i32);
            let want = part(&paper(&eos, &x, &y), i + j) * factor;
            let got = got.get(i, j).unwrap();
            checked += 1;
            let scale = majorant::floored(scale(&eos, tau, delta, i, j), scale(&eos, tau, delta, 0, 0));
            if (got - want).abs() > ToleranceClass::Term.bound(scale).unwrap() {
                failures.push(format!("{fluid} at ({tau}, {delta}), A{i}{j}: fast path {got:e}, num-dual {want:e}"));
            }
        }
    }
    let shown = failures.iter().take(20).cloned().collect::<Vec<_>>().join("\n");
    assert!(failures.is_empty(), "{} of {checked} outside Term:\n{shown}", failures.len());
    checked
}

/// The paper formula `Σ n τ^t δ^d e^(−cδ^l)` of power and Exponential terms in num-dual.
fn power_paper(e: &EosRecord, tau: &D4, delta: &D4) -> D4 {
    let mut sum = var(0.0, [false; 4]);
    for k in &e.power {
        let damping = (delta.powi(i32::from(k.l)) * -k.c).exp();
        sum += tau.powf(k.t) * delta.powi(i32::from(k.d)) * damping * k.n;
    }
    sum
}

fn power_scale(e: &EosRecord, tau: f64, delta: f64, i: usize, j: usize) -> f64 {
    e.power.iter().map(|term| majorant::power(term, tau, delta, i, j)).sum()
}

/// AD oracle: num-dual 0.15 on the paper formula (map 02 §3.1). The power list of every core-subset fluid that has
/// one (Power and Exponential terms, as datagen merges them), compiled alone, at 300 [`points`] each: all 15 `A_ij`
/// of `residual` (the `accumulate` fast path) against num-dual, class `Term`, scale [`majorant::power`]. Replaces
/// the seed's test-only hyper-dual (S-07; ROT-137).
#[test]
fn jets_match_num_dual_ad() {
    let core = ["Air", "Ammonia", "CarbonDioxide", "HFE143m", "Helium", "Methanol", "Nitrogen", "R1130(E)", "R1234yf"];
    let core = [&core[..], &["R1234ze(E)", "R410A", "Water", "n-Heptane"]].concat();
    let select = |e: &EosRecord, eos: &mut EosRecord| eos.power.clone_from(&e.power);
    let checked: usize = core.iter().map(|fluid| check_ad(fluid, select, power_paper, power_scale)).sum();
    assert_eq!(checked, 13 * 300 * 15);
}

/// AD oracle: num-dual 0.15 on `n τ^t δ^d e^(−gδ^l)`, the Exponential terms alone: R1130(E)'s 5 (after its 6 Power
/// terms; g ≠ 1) and Methanol's 36 (map 02 §3.1, map 13 §8).
#[test]
fn exponential_matches_ad_of_the_paper_formula() {
    let tail = |from: usize| {
        move |e: &EosRecord, eos: &mut EosRecord| {
            assert_eq!(e.power.len(), from + if from == 0 { 36 } else { 5 });
            eos.power = e.power[from..].to_vec();
        }
    };
    let checked = check_ad("R1130(E)", tail(6), power_paper, power_scale)
        + check_ad("Methanol", tail(0), power_paper, power_scale);
    assert_eq!(checked, 2 * 300 * 15);
}

/// AD oracle: num-dual 0.15 on `n τ^t δ^d e^(−δ^l − τ^m)` with each exponential absent when its exponent is 0
/// (Lemmon & Jacobsen 2005), R125's 18 terms; scale [`majorant::lemmon2005`].
#[test]
fn lemmon2005_matches_ad_of_the_paper_formula() {
    let paper = |e: &EosRecord, tau: &D4, delta: &D4| {
        let mut sum = var(0.0, [false; 4]);
        for k in &e.lemmon2005 {
            let mut exponent = var(0.0, [false; 4]);
            if k.l > 0 {
                exponent -= delta.powi(i32::from(k.l));
            }
            if k.m > 0.0 {
                exponent -= tau.powf(k.m);
            }
            sum += tau.powf(k.t) * delta.powi(i32::from(k.d)) * exponent.exp() * k.n;
        }
        sum
    };
    let scale = |e: &EosRecord, tau: f64, delta: f64, i: usize, j: usize| {
        e.lemmon2005.iter().map(|term| majorant::lemmon2005(term, tau, delta, i, j)).sum()
    };
    let select = |e: &EosRecord, eos: &mut EosRecord| eos.lemmon2005.clone_from(&e.lemmon2005);
    assert_eq!(check_ad("R125", select, paper, scale), 300 * 15);
}

/// AD oracle: num-dual 0.15 on `n τ^t δ^d e^(−g_d δ^(l_d) − g_t τ^(l_t))` (de Reuck & Craven 1993), Methanol's 8
/// terms with g_t < 0; scale [`majorant::double_exponential`].
#[test]
fn double_exponential_matches_ad_of_the_paper_formula() {
    let paper = |e: &EosRecord, tau: &D4, delta: &D4| {
        let mut sum = var(0.0, [false; 4]);
        for k in &e.double_exponential {
            let exponent = -(delta.powi(i32::from(k.ld)) * k.gd) - tau.powf(k.lt) * k.gt;
            sum += tau.powf(k.t) * delta.powi(i32::from(k.d)) * exponent.exp() * k.n;
        }
        sum
    };
    let scale = |e: &EosRecord, tau: f64, delta: f64, i: usize, j: usize| {
        e.double_exponential.iter().map(|term| majorant::double_exponential(term, tau, delta, i, j)).sum()
    };
    let select = |e: &EosRecord, eos: &mut EosRecord| eos.double_exponential.clone_from(&e.double_exponential);
    assert_eq!(check_ad("Methanol", select, paper, scale), 300 * 15);
}

/// AD oracle: num-dual 0.15 on `n τ^t δ^d e^(−η(δ − ε)² − β(τ − γ)²)`, the Gaussian blocks of R1234yf (7 terms) and
/// Water (3 terms, β up to 250; map 02 §3.1); scale [`majorant::gaussian`].
#[test]
fn gaussian_matches_ad_of_the_paper_formula() {
    let paper = |e: &EosRecord, tau: &D4, delta: &D4| {
        let mut sum = var(0.0, [false; 4]);
        for k in &e.gaussian {
            let (wt, wd) = (*tau - k.gamma, *delta - k.epsilon);
            let exponent = -(wd * wd * k.eta) - wt * wt * k.beta;
            sum += tau.powf(k.t) * delta.powi(i32::from(k.d)) * exponent.exp() * k.n;
        }
        sum
    };
    let scale = |e: &EosRecord, tau: f64, delta: f64, i: usize, j: usize| {
        e.gaussian.iter().map(|term| majorant::gaussian(term, tau, delta, i, j)).sum()
    };
    let select = |e: &EosRecord, eos: &mut EosRecord| eos.gaussian.clone_from(&e.gaussian);
    let checked = check_ad("R1234yf", select, paper, scale) + check_ad("Water", select, paper, scale);
    assert_eq!(checked, 2 * 300 * 15);
}

/// AD oracle: num-dual 0.15 on `n τ^t δ^d e^(−η(δ − ε)² + 1/(β(τ − γ)² + b))` (Gao et al. 2020), Ammonia's 2 terms;
/// scale [`majorant::gao_b`].
#[test]
fn gao_b_matches_ad_of_the_paper_formula() {
    let paper = |e: &EosRecord, tau: &D4, delta: &D4| {
        let mut sum = var(0.0, [false; 4]);
        for k in &e.gao_b {
            let (wt, wd) = (*tau - k.gamma, *delta - k.epsilon);
            let exponent = (wt * wt * k.beta + k.b).recip() - wd * wd * k.eta;
            sum += tau.powf(k.t) * delta.powi(i32::from(k.d)) * exponent.exp() * k.n;
        }
        sum
    };
    let scale = |e: &EosRecord, tau: f64, delta: f64, i: usize, j: usize| {
        e.gao_b.iter().map(|term| majorant::gao_b(term, tau, delta, i, j)).sum()
    };
    let select = |e: &EosRecord, eos: &mut EosRecord| eos.gao_b.clone_from(&e.gao_b);
    assert_eq!(check_ad("Ammonia", select, paper, scale), 300 * 15);
}

/// PLAN.md M3.7 (map 02 §3.1): at ρ = 0 every `A_ij` of every fluid (all 136; Water's and CarbonDioxide's
/// non-analytic terms since M4.1) is finite at T_min, T_r and T_max, and every δ-derivative vanishes (`A_ij = 0` for j ≥ 1:
/// each carries δ^j). The 57 MBWR d = 0 terms (CycloPropane, Propyne, R114, R123, R13, R14, R152A, R21, RC318) and
/// Methane's three d = 0 Gaussians are among them; their α^r at ρ = 0 is finite, not NaN from `0 · ln 0`. The exact
/// virials exist and are finite for the same fluids.
#[test]
fn finite_at_zero_density_for_every_fluid() {
    let registry = Registry::embedded().unwrap();
    let mut d0_fluids = Vec::new();
    let mut fluids = 0;
    for f in phasekit_data::FLUIDS {
        let record = phasekit_core::internal::record(registry, f.name).unwrap();
        let e = &record.eos;
        if e.power.iter().any(|k| k.d == 0) || e.gaussian.iter().any(|k| k.d == 0) {
            d0_fluids.push(f.name);
        }
        let eos = term::residual_part(e);
        let model = term::residual_model(&record, &eos).unwrap();
        for t in [record.limits.t_min(), e.t_reducing, record.limits.t_max()] {
            let a = model.eos().residual(t, 0.0, Order::Four);
            for (i, j) in entries() {
                let aij = a.get(i, j).unwrap();
                assert!(aij.is_finite(), "{} at {t} K: A{i}{j} = {aij}", f.name);
                assert!(j == 0 || aij == 0.0, "{} at {t} K: A{i}{j} = {aij} at ρ = 0", f.name);
            }
            let v = model.eos().zero_density(t).unwrap();
            assert!([v.b, v.c, v.db_dt, v.dc_dt].iter().all(|x| x.is_finite()), "{} at {t} K: {v:?}", f.name);
        }
        fluids += 1;
    }
    assert_eq!(fluids, 136);
    let want = ["CycloPropane", "Methane", "Propyne", "R114", "R123", "R13", "R14", "R152A", "R21", "RC318"];
    d0_fluids.sort_unstable();
    assert_eq!(d0_fluids, want);
}

/// PLAN.md M3.7 (E4): at δ = 1e-12 the δ-factors keep full relative accuracy for every (d, l, c) of the power and
/// Exponential terms in the data (68 (d, l) pairs): each such term, compiled alone at τ = 1.3, gives all 15 `A_ij`
/// within 1e-13 of num-dual's, entry by entry (not against a Σ|φ| scale), and exactly zero where num-dual's is.
/// num-dual differentiates δ^d e^(−cδ^l) without dividing by δ, so it is a valid reference there; CoolProp is not
/// (it divides by δ^j, VERIFICATION.md §3.5).
#[test]
fn delta_factors_are_cancellation_free_near_zero_density() {
    let registry = Registry::embedded().unwrap();
    let mut seen = std::collections::BTreeMap::new();
    for f in phasekit_data::FLUIDS {
        let record = phasekit_core::internal::record(registry, f.name).unwrap();
        for term in &record.eos.power {
            seen.entry((term.d, term.l, term.c.to_bits())).or_insert((record.clone(), *term));
        }
    }
    let pairs: std::collections::BTreeSet<_> = seen.keys().map(|&(d, l, _)| (d, l)).collect();
    assert_eq!(pairs.len(), 68);
    for ((d, l, _), (record, term)) in &seen {
        let e = &record.eos;
        let mut eos = EosRecord::new(e.gas_constant, e.t_reducing, e.rho_reducing, e.rho_max);
        eos.power = vec![*term];
        let model = term::residual_model(record, &eos).unwrap();
        let (t, rho) = (e.t_reducing / 1.3, 1e-12 * e.rho_reducing);
        let (tau, delta) = (e.t_reducing / t, rho / e.rho_reducing);
        let got = model.eos().residual(t, rho, Order::Four);
        for (i, j) in entries() {
            let (x, y) = tau_delta(tau, delta, i, j);
            let factor = phasekit_core::math::powi(tau, i as i32) * phasekit_core::math::powi(delta, j as i32);
            let want = part(&power_paper(&eos, &x, &y), i + j) * factor;
            let got = got.get(i, j).unwrap();
            assert!(
                (got - want).abs() <= 1e-13 * want.abs(),
                "d = {d}, l = {l}, {term:?}: A{i}{j} {got:e} vs {want:e}"
            );
        }
    }
}

/// AD oracle: num-dual 0.15 on the paper formula of Wagner & Pruß 2002 (map 02 §3.1), `n Δ^b δ ψ` with
/// `[(δ − 1)²]^p` taken literally (`powf` of the square): Water's 2 and CarbonDioxide's 3 non-analytic terms at the 300
/// [`points`] of each fluid, those within 1e-2 of τ = δ = 1 included; class `Term`, scale [`majorant::non_analytic`].
#[test]
fn nonanalytic_matches_ad_of_the_paper_formula() {
    let paper = |e: &EosRecord, tau: &D4, delta: &D4| {
        let mut sum = var(0.0, [false; 4]);
        for k in &e.non_analytic {
            let (w, u) = (*delta - 1.0, *tau - 1.0);
            let w2 = w * w;
            let theta = -*tau + 1.0 + w2.powf(1.0 / (2.0 * k.beta)) * k.big_a;
            let big_delta = theta * theta + w2.powf(k.a) * k.big_b;
            let psi = (-(w2 * k.big_c) - u * u * k.big_d).exp();
            sum += big_delta.powf(k.b) * *delta * psi * k.n;
        }
        sum
    };
    let scale = |e: &EosRecord, tau: f64, delta: f64, i: usize, j: usize| {
        e.non_analytic.iter().map(|term| majorant::non_analytic(term, tau, delta, i, j)).sum()
    };
    let select = |e: &EosRecord, eos: &mut EosRecord| eos.non_analytic.clone_from(&e.non_analytic);
    let checked = check_ad("Water", select, paper, scale) + check_ad("CarbonDioxide", select, paper, scale);
    assert_eq!(checked, 2 * 300 * 15);
}

/// The `entries` of `eos`'s α^r at (T, ρ) are the limits of their neighbours at T(1 ± 1e-9) and ρ(1 ± 1e-9): within
/// 1e-6 of each, relative to max(1, |neighbour|). The step moves a smooth entry by about 1e-9 of its next derivative
/// (Water's and CarbonDioxide's Gaussians make that up to 4.4e-7 at 0.8 T_c), and the non-analytic parts by
/// |1 − τ|^(2b−1) or |δ − 1|^(1/β−2) (b ≥ 0.85, β = 0.3), at most 1e-7 at the critical point.
fn limit_of_neighbours(name: &str, eos: &dyn HelmholtzModel, t: f64, rho: f64, entries: &[(usize, usize)]) {
    let a = eos.residual(t, rho, Order::Two);
    let near = [(t * (1.0 - 1e-9), rho), (t * (1.0 + 1e-9), rho), (t, rho * (1.0 - 1e-9)), (t, rho * (1.0 + 1e-9))];
    for (tn, rn) in near {
        let b = eos.residual(tn, rn, Order::Two);
        for &(i, j) in entries {
            let (got, want) = (a.get(i, j).unwrap(), b.get(i, j).unwrap());
            assert!((got - want).abs() <= 1e-6 * want.abs().max(1.0), "{name} ({t}, {rho}): A{i}{j} {got} vs {want}");
        }
    }
}

/// PLAN.md M4.6 (E17, ROT-065): at (T_c, ρ_c) of Water and CarbonDioxide, which are their reducing points (τ = δ = 1),
/// every non-analytic Δ vanishes. Evaluated there, with no nudge off the point, α^r keeps α and its first
/// derivatives finite: A20 = −∞ (c_v diverges); A11 and A02 are finite (their non-analytic parts tend to 0); nothing of
/// order ≤ 2 is NaN, and each finite entry is the limit of its neighbours. Orders 3 and 4 have no limit there (A30
/// changes sign with 1 − τ) and are NaN. `State::from_total` then reports p, h and s, and `Undefined` for c_v, c_p
/// and w.
#[test]
fn water_and_co2_critical_bundles_keep_first_order() {
    let registry = Registry::embedded().unwrap();
    for name in ["Water", "CarbonDioxide"] {
        let record = phasekit_core::internal::record(registry, name).unwrap();
        let critical = record.critical.as_ref().unwrap();
        let (t, rho) = (record.eos.t_reducing, record.eos.rho_reducing);
        assert_eq!((critical.t, critical.rho), (t, rho), "{name}: the critical point is the reducing point");
        let model = record.clone().compile().unwrap();
        let eos = model.eos();
        let a = eos.residual(t, rho, Order::Four);
        assert_eq!(a.get(2, 0), Some(f64::NEG_INFINITY), "{name}");
        limit_of_neighbours(name, eos, t, rho, &[(0, 0), (1, 0), (0, 1), (1, 1), (0, 2)]);
        for (i, j) in entries().filter(|(i, j)| i + j >= 3) {
            assert!(a.get(i, j).unwrap().is_nan(), "{name}: A{i}{j} = {:?}", a.get(i, j));
        }
        let total = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
        let (r, m) = (eos.gas_constant(), record.molar_mass);
        let state = State::from_total(model.info().key(), t, rho, r, m, Phase::CriticalPoint, &total).unwrap();
        let first_order = [state.p(), state.h(Basis::Molar), state.s(Basis::Molar)];
        assert!(first_order.iter().all(|x| x.is_finite()), "{name}: {first_order:?}");
        assert!((state.p() / critical.p - 1.0).abs() < 1e-4, "{name}: p = {} vs p_c = {}", state.p(), critical.p);
        let undefined = |prop| Err(Error::Undefined { prop, phase: Phase::CriticalPoint });
        assert_eq!(state.cv(Basis::Molar), undefined(Prop::Cvmolar), "{name}");
        assert_eq!(state.cp(Basis::Molar), undefined(Prop::Cpmolar), "{name}");
        assert_eq!(state.speed_of_sound(), undefined(Prop::SpeedOfSound), "{name}");
    }
}

/// PLAN.md M4.6: on the critical isochore ρ = ρ_c away from T_c (δ = 1, τ ≠ 1), |δ − 1|^(1/β) has no fourth
/// derivative (1/β = 10/3). Every A_ij but A04 is still finite, and to order 2 the limit of its neighbours, at 0.8, 1.2
/// and 2 T_c. A04 has no finite value there: each term's is infinite, and NaN where terms of opposite sign meet.
#[test]
fn critical_isochore_is_finite_but_for_a04() {
    let registry = Registry::embedded().unwrap();
    for name in ["Water", "CarbonDioxide"] {
        let record = phasekit_core::internal::record(registry, name).unwrap();
        let model = record.clone().compile().unwrap();
        let (tc, rho) = (record.eos.t_reducing, record.eos.rho_reducing);
        for t in [0.8 * tc, 1.2 * tc, 2.0 * tc] {
            let a = model.eos().residual(t, rho, Order::Four);
            for (i, j) in entries() {
                let aij = a.get(i, j).unwrap();
                assert_eq!(aij.is_finite(), (i, j) != (0, 4), "{name} at {t} K: A{i}{j} = {aij}");
            }
            limit_of_neighbours(name, model.eos(), t, rho, &[(0, 0), (1, 0), (0, 1), (2, 0), (1, 1), (0, 2)]);
        }
    }
}
