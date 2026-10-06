//! The scale of the `Term` class (VERIFICATION.md §5): for an entry `A_ij = τ^i δ^j ∂^(i+j)α/∂τ^i∂δ^j`, the sum of
//! the absolute values of the summands of its expansion by the product and chain rules. It bounds the rounding error
//! of every order in which those summands can be evaluated, so two correct evaluations (CoolProp's, which divides by
//! δ^j, and phasekit's exact δ-polynomials) agree within a small multiple of it; for `A_00` it is Σ_k |φ_k|.
//!
//! A raw Σ_k |φ_k| cannot serve the derivatives: a τ-derivative of a term with t = 50 carries the factor
//! t(t − 1)(t − 2)(t − 3) ≈ 5.5e6, so its rounding alone exceeds 1e-13 · Σ_k |φ_k| (PLAN.md M3.1, measured on Water).

use phasekit_core::internal::PowerTerm;
use phasekit_core::math;

/// `|a|(|a| + 1)…(|a| + n − 1)`: the falling factorial `a(a − 1)…(a − n + 1)` with every summand of its expansion in
/// powers of `a` taken positive (the unsigned Stirling numbers of the first kind).
fn rising(a: f64, n: usize) -> f64 {
    (0..n).fold(1.0, |acc, k| acc * (a.abs() + k as f64))
}

/// The complete Bell polynomial `B_m(a_1, …, a_m)` (positive integer coefficients), by `B_(n+1) = Σ_k C(n, k)
/// B_(n−k) a_(k+1)`.
fn bell(m: usize, a: &[f64; 4]) -> f64 {
    let mut b = [1.0; 5];
    for n in 1..=m {
        b[n] = (0..n).map(|k| binomial(n - 1, k) * b[n - 1 - k] * a[k]).sum();
    }
    b[m]
}

fn binomial(n: usize, k: usize) -> f64 {
    (0..k).fold(1.0, |acc, i| acc * (n - i) as f64 / (i + 1) as f64)
}

/// The `Term` scale of entry `(i, j)`, `i + j ≤ 4`, of one power term `n τ^t δ^d e^(−cδ^l)` at (τ, δ). With
/// `φ = n τ^t δ^d e^(−x)`, `x = cδ^l`: `τ^i ∂^i τ^t = (t)_i τ^t` and, by Leibniz and Faà di Bruno,
/// `δ^j ∂^j (δ^d e^(−x)) = δ^d e^(−x) Σ_m C(j, m) (d)_(j−m) B_m(−(l)_1 x, …, −(l)_m x)`. The scale replaces every
/// falling factorial by its rising counterpart and `x` by `|x|`:
/// `|φ| · t^(i) · Σ_m C(j, m) d^(j−m) B_m(l^(1)|x|, …, l^(m)|x|)`.
pub fn power(term: &PowerTerm, tau: f64, delta: f64, i: usize, j: usize) -> f64 {
    let x = term.c * math::powi(delta, i32::from(term.l));
    let phi = term.n * math::exp(term.t * math::ln(tau) - x) * math::powi(delta, i32::from(term.d));
    let mut a = [0.0; 4];
    for (k, slot) in a.iter_mut().enumerate() {
        *slot = rising(f64::from(term.l), k + 1) * x.abs();
    }
    let delta_side: f64 = (0..=j).map(|m| binomial(j, m) * rising(f64::from(term.d), j - m) * bell(m, &a)).sum();
    phi.abs() * rising(term.t, i) * delta_side
}

#[cfg(test)]
mod tests {
    use super::*;

    /// By hand: rising(1.5, 2) = 1.5·2.5; rising(−2, 3) = 2·3·4; the falling factorial of 4 has |summands| 1, 6, 11,
    /// 6 summing to 24 = rising(1, 4); B_2(a1, a2) = a1² + a2, B_3 = a1³ + 3a1a2 + a3.
    #[test]
    fn rising_factorials_and_bell_polynomials_by_hand() {
        assert_eq!((rising(1.5, 2), rising(-2.0, 3), rising(1.0, 4), rising(7.0, 0)), (3.75, 24.0, 24.0, 1.0));
        let a = [2.0, 3.0, 5.0, 7.0];
        assert_eq!((bell(0, &a), bell(1, &a), bell(2, &a), bell(3, &a)), (1.0, 2.0, 7.0, 31.0));
        assert_eq!(bell(4, &a), 16.0 + 6.0 * 4.0 * 3.0 + 4.0 * 2.0 * 5.0 + 3.0 * 9.0 + 7.0);
    }

    /// A_00 is |φ|; with c = 0 (no exponential) A_0j is |φ|·d^(j), the τ-side t^(i); at δ → 0 the δ-side of
    /// `δ e^(−δ)` keeps d^(4) = 24 although the entry itself, δ^3(δ − 4)φ, vanishes: the summands that cancel are
    /// what an evaluation rounds.
    #[test]
    fn power_scale_by_hand() {
        let close = |a: f64, b: f64| ((a / b) - 1.0).abs() < 1e-15;
        let term = PowerTerm::new(-0.5, 1.5, 2, 0, 0.0);
        let phi = 0.5 * math::powf(2.0, 1.5) * 0.25;
        assert!(close(power(&term, 2.0, 0.5, 0, 0), phi));
        assert!(close(power(&term, 2.0, 0.5, 0, 3), phi * 2.0 * 3.0 * 4.0));
        assert!(close(power(&term, 2.0, 0.5, 2, 0), phi * 1.5 * 2.5));
        let exp_term = PowerTerm::new(1.0, 0.0, 1, 1, 1.0);
        let delta = 1e-8;
        let phi = delta * math::exp(-delta);
        // j = 4: Σ_m C(4, m) 1^(4−m) B_m(x, 2x, 6x, 24x) = 24 + 24x + 12(x² + 2x) + 4(…) + … = 24 + O(x).
        assert!((power(&exp_term, 1.0, delta, 0, 4) / (24.0 * phi) - 1.0).abs() < 1e-6);
        // j = 1: d + l|x| = 1 + δ.
        assert!(close(power(&exp_term, 1.0, delta, 0, 1), (1.0 + delta) * phi));
        // d = 1, l = 2, x = 0.25 at δ = 0.5: a₁ = 2x, a₂ = 2·3x, so j = 2 gives d^(2) + 2·d·a₁ + a₁² + a₂ = 4.75.
        let square = PowerTerm::new(1.0, 0.0, 1, 2, 1.0);
        assert!(close(power(&square, 1.0, 0.5, 0, 2), 4.75 * 0.5 * math::exp(-0.25)));
    }
}
