//! num-dual 0.15, the AD oracle of the cubic's jets (ROT-218): a nested hyper-dual with four ε directions, as in
//! `phasekit-verify`'s `tests/terms.rs`, and the check of a whole `Derivs` against it.

use num_dual::HyperDual;
use phasekit_core::{Derivs, math};

/// Four ε directions (outer: 1 and 2, inner: 3 and 4): one evaluation gives one mixed derivative up to order 4.
pub type D4 = HyperDual<HyperDual<f64>>;

/// `x` seeded in the directions where `seeds` is true.
pub fn var(x: f64, seeds: [bool; 4]) -> D4 {
    let one = |seeded: bool| if seeded { 1.0 } else { 0.0 };
    let constant = |v: f64| HyperDual::new(v, 0.0, 0.0, 0.0);
    let inner = HyperDual::new(x, one(seeds[2]), one(seeds[3]), 0.0);
    HyperDual::new(inner, constant(one(seeds[0])), constant(one(seeds[1])), constant(0.0))
}

/// The ε-part of directions 1..n.
pub fn part(f: &D4, n: usize) -> f64 {
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

/// The first A_ij = τ^i δ^j ∂^(i+j)f/∂τ^i∂δ^j of `d`, to order 4, farther than 1e-12 of the largest of its |value| and 1
/// from num-dual's derivatives of `f` at (τ, δ), as a message; `None` when every one agrees.
pub fn ad_mismatch(d: &Derivs, tau: f64, delta: f64, f: impl Fn(&D4, &D4) -> D4) -> Option<String> {
    let seeds = |lo: usize, hi: usize| [0, 1, 2, 3].map(|k| (lo..hi).contains(&k));
    (0..=4)
        .flat_map(|n| (0..=n).map(move |i| (i, n - i)))
        .map(|(i, j)| {
            let ad = f(&var(tau, seeds(0, i)), &var(delta, seeds(i, i + j)));
            let want = part(&ad, i + j) * math::powi(tau, i as i32) * math::powi(delta, j as i32);
            (i, j, d.get(i, j).unwrap_or(f64::NAN), want)
        })
        .find(|&(_, _, got, want)| (got - want).abs().is_nan() || (got - want).abs() > 1e-12 * want.abs().max(1.0))
        .map(|(i, j, got, want)| format!("A{i}{j} {got} against {want}"))
}
