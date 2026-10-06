//! L1, two derivative mechanisms (VERIFICATION.md §2, §9.3): the in-house `Jet4` against num-dual on every `Real`
//! method, and the power-term fast path (`accumulate`, reached through a compiled record) against num-dual AD of the
//! paper formula on the core subset's real terms. Class `Term`, scale [`majorant`] (VERIFICATION.md §5).

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use num_dual::{DualNum, HyperDual};
use phasekit_core::internal::{EosRecord, FluidRecord, PowerTerm};
use phasekit_core::{Jet4, Order, Real, Registry};
use phasekit_verify::{SplitMix64, ToleranceClass, majorant};

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

/// The paper formula `Σ n τ^t δ^d e^(−cδ^l)` in num-dual.
fn paper(terms: &[PowerTerm], tau: &D4, delta: &D4) -> D4 {
    let mut sum = var(0.0, [false; 4]);
    for k in terms {
        let damping = (delta.powi(i32::from(k.l)) * -k.c).exp();
        sum += tau.powf(k.t) * delta.powi(i32::from(k.d)) * damping * k.n;
    }
    sum
}

/// AD oracle: num-dual 0.15 on the paper formula (map 02 §3.1). The power list of every core-subset fluid (Power and
/// Exponential terms, as datagen merges them), compiled alone, at 300 SplitMix64 points: τ ~ U[T_r/Tmax, T_r/Tmin],
/// δ ~ logU[1e-12, ρ_max/ρ_r], 10 of them at δ = 1e-8 and 10 at δ = 1e-12, and 10 within 1e-2 of τ = δ = 1. All 15
/// `A_ij` of `residual` (the `accumulate` fast path) against num-dual, class `Term`, scale [`majorant::power`].
/// Replaces the seed's test-only hyper-dual (S-07; ROT-137).
#[test]
fn jets_match_num_dual_ad() {
    let registry = Registry::embedded().unwrap();
    let core = ["Air", "Ammonia", "CarbonDioxide", "HFE143m", "Helium", "Methanol", "Nitrogen", "R1130(E)", "R1234yf"];
    let core = [&core[..], &["R1234ze(E)", "R410A", "Water", "n-Heptane"]].concat();
    let (mut failures, mut checked) = (Vec::new(), 0);
    for name in core {
        let record = phasekit_core::internal::record(registry, name).unwrap();
        let e = &record.eos;
        let mut eos = EosRecord::new(e.gas_constant, e.t_reducing, e.rho_reducing, e.rho_max);
        eos.power.clone_from(&e.power);
        let model = FluidRecord::new(name, record.molar_mass, record.source.clone(), eos, record.limits);
        let model = model.compile().unwrap();
        let (t_r, rho_r) = (e.t_reducing, e.rho_reducing);
        let (tau_lo, tau_hi) = (t_r / record.limits.t_max(), t_r / record.limits.t_min());
        let mut rng = SplitMix64::new(1);
        for k in 0..300 {
            let tau = if k >= 290 { 1.0 + rng.uniform(-1e-2, 1e-2) } else { rng.uniform(tau_lo, tau_hi) };
            let delta = match k {
                0..270 => rng.log_uniform(1e-12, e.rho_max / rho_r),
                270..280 => 1e-8,
                280..290 => 1e-12,
                _ => 1.0 + rng.uniform(-1e-2, 1e-2),
            };
            // The model's own τ and δ, from the (T, ρ) it is called with.
            let (t, rho) = (t_r / tau, delta * rho_r);
            let (tau, delta) = (t_r / t, rho / rho_r);
            let got = model.eos().residual(t, rho, Order::Four);
            for (i, j) in entries() {
                let (x, y) = tau_delta(tau, delta, i, j);
                let factor = phasekit_core::math::powi(tau, i as i32) * phasekit_core::math::powi(delta, j as i32);
                let want = part(&paper(&e.power, &x, &y), i + j) * factor;
                let scale: f64 = e.power.iter().map(|term| majorant::power(term, tau, delta, i, j)).sum();
                let got = got.get(i, j).unwrap();
                checked += 1;
                if (got - want).abs() > ToleranceClass::Term.bound(scale).unwrap() {
                    failures.push(format!("{name} at ({tau}, {delta}), A{i}{j}: fast path {got:e}, num-dual {want:e}"));
                }
            }
        }
    }
    assert_eq!(checked, 13 * 300 * 15);
    let shown = failures.iter().take(20).cloned().collect::<Vec<_>>().join("\n");
    assert!(failures.is_empty(), "{} of {checked} outside Term:\n{shown}", failures.len());
}
