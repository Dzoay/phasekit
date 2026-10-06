//! The scale of the `Term` class (VERIFICATION.md §5): for an entry `A_ij = τ^i δ^j ∂^(i+j)α/∂τ^i∂δ^j`, the sum of
//! the absolute values of the summands of its expansion by the product and chain rules. It bounds the rounding error
//! of every order in which those summands can be evaluated, so two correct evaluations (CoolProp's, which divides by
//! δ^j, and phasekit's exact δ-polynomials) agree within a small multiple of it; for `A_00` it is Σ_k |φ_k|.
//!
//! A raw Σ_k |φ_k| cannot serve the derivatives: a τ-derivative of a term with t = 50 carries the factor
//! t(t − 1)(t − 2)(t − 3) ≈ 5.5e6, so its rounding alone exceeds 1e-13 · Σ_k |φ_k| (PLAN.md M3.1, measured on Water).

use phasekit_core::internal::{DoubleExponentialTerm, GaoBTerm, GaussianTerm, Lemmon2005Term, PowerTerm};
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

/// The scale of one side of a separable term: `Σ_m C(k, m) p^(k−m) B_m(q^(1)|x|, …, q^(m)|x|)` for the factor
/// `z^p e^(−x)`, `x = c·z^q`, whose scaled derivative `z^k ∂^k (z^p e^(−x)) / (z^p e^(−x))` is, by Leibniz and Faà
/// di Bruno, `Σ_m C(k, m) (p)_(k−m) B_m(−(q)_1 x, …, −(q)_m x)` with every falling factorial made rising.
fn side(p: f64, q: f64, x: f64, k: usize) -> f64 {
    let mut a = [0.0; 4];
    for (n, slot) in a.iter_mut().enumerate() {
        *slot = rising(q, n + 1) * x.abs();
    }
    (0..=k).map(|m| binomial(k, m) * rising(p, k - m) * bell(m, &a)).sum()
}

/// The `Term` scale of entry `(i, j)`, `i + j ≤ 4`, of the separable term `n τ^t e^(−aτ^m) δ^d e^(−cδ^l)` at
/// (τ, δ): `|φ| ·` [`side`]`(t, m, aτ^m, i) ·` [`side`]`(d, l, cδ^l, j)`. Power, Exponential, Lemmon2005 and
/// DoubleExponential terms are all of this form.
#[allow(clippy::too_many_arguments)] // the symbols of the formula, as printed
pub fn separable(n: f64, t: f64, a: f64, m: f64, d: f64, c: f64, l: f64, at: (f64, f64), ij: (usize, usize)) -> f64 {
    let ((tau, delta), (i, j)) = (at, ij);
    let y = if a == 0.0 { 0.0 } else { a * math::powf(tau, m) };
    let x = if c == 0.0 { 0.0 } else { c * math::powf(delta, l) };
    let phi = n * math::exp(t * math::ln(tau) - y - x) * math::powf(delta, d);
    phi.abs() * side(t, m, y, i) * side(d, l, x, j)
}

/// [`separable`] for a power term `n τ^t δ^d e^(−cδ^l)`: `|φ| · t^(i) · side(d, l, cδ^l, j)`.
pub fn power(term: &PowerTerm, tau: f64, delta: f64, i: usize, j: usize) -> f64 {
    let (d, l) = (f64::from(term.d), f64::from(term.l));
    separable(term.n, term.t, 0.0, 0.0, d, term.c, l, (tau, delta), (i, j))
}

/// [`separable`] for a Lemmon2005 term `n τ^t δ^d e^(−δ^l − τ^m)`, each exponential absent when its exponent is 0.
pub fn lemmon2005(term: &Lemmon2005Term, tau: f64, delta: f64, i: usize, j: usize) -> f64 {
    let (a, c) = (if term.m > 0.0 { 1.0 } else { 0.0 }, if term.l > 0 { 1.0 } else { 0.0 });
    separable(term.n, term.t, a, term.m, f64::from(term.d), c, f64::from(term.l), (tau, delta), (i, j))
}

/// [`separable`] for a DoubleExponential term `n τ^t δ^d e^(−g_d δ^(l_d) − g_t τ^(l_t))`.
pub fn double_exponential(term: &DoubleExponentialTerm, tau: f64, delta: f64, i: usize, j: usize) -> f64 {
    let (d, l) = (f64::from(term.d), f64::from(term.ld));
    separable(term.n, term.t, term.gt, term.lt, d, term.gd, l, (tau, delta), (i, j))
}

/// The scale of one side of a Gaussian term, `z^p e^u` with `u = −η(z − ε)²`: Leibniz over the power's rising
/// factorials and the complete Bell polynomials of `|z u'| = 2|η||z − ε|z` and `|z² u''| = 2|η|z²` (Faà di Bruno in
/// `w = z − ε`, as the block and CoolProp evaluate it).
fn gaussian_side(p: f64, eta: f64, epsilon: f64, z: f64, k: usize) -> f64 {
    let a = [2.0 * eta.abs() * (z - epsilon).abs() * z, 2.0 * eta.abs() * z * z, 0.0, 0.0];
    (0..=k).map(|m| binomial(k, m) * rising(p, k - m) * bell(m, &a)).sum()
}

/// The `Term` scale of entry `(i, j)` of a Gaussian term `n τ^t δ^d e^(−η(δ − ε)² − β(τ − γ)²)`.
pub fn gaussian(term: &GaussianTerm, tau: f64, delta: f64, i: usize, j: usize) -> f64 {
    let (wt, wd) = (tau - term.gamma, delta - term.epsilon);
    let exponent = term.t * math::ln(tau) - term.beta * wt * wt - term.eta * wd * wd;
    let phi = term.n * math::exp(exponent) * math::powi(delta, i32::from(term.d));
    let tau_side = gaussian_side(term.t, term.beta, term.gamma, tau, i);
    phi.abs() * tau_side * gaussian_side(f64::from(term.d), term.eta, term.epsilon, delta, j)
}

/// The `Term` scale of entry `(i, j)` of a GaoB term `n τ^t δ^d e^(−η(δ − ε)² + 1/(β(τ − γ)² + b))`. The δ-side is a
/// Gaussian side; the τ-side's `h_k = τ^k v⁽ᵏ⁾` of `v = 1/q`, `q = β(τ − γ)² + b`, take the chain rule's summands with
/// `|q′| = 2|β||τ − γ|`, `|q″| = 2|β|`, `r = 1/|q|`: `|v′| = |q′|r²`, `|v″| = (2q′²r + |q″|)r²`,
/// `|v‴| = 6|q′|(|q″| + q′²r)r³`, `|v⁗| = (6q″² + 36q′²|q″|r + 24q′⁴r²)r³`.
pub fn gao_b(term: &GaoBTerm, tau: f64, delta: f64, i: usize, j: usize) -> f64 {
    let (wt, wd) = (tau - term.gamma, delta - term.epsilon);
    let q = term.beta * wt * wt + term.b;
    let exponent = term.t * math::ln(tau) + 1.0 / q - term.eta * wd * wd;
    let phi = term.n * math::exp(exponent) * math::powi(delta, i32::from(term.d));
    let (q1, q2, r) = (2.0 * term.beta.abs() * wt.abs(), 2.0 * term.beta.abs(), 1.0 / q.abs());
    let (r2, r3, q11) = (r * r, r * r * r, q1 * q1);
    let v = [
        q1 * r2,
        (2.0 * q11 * r + q2) * r2,
        6.0 * q1 * (q2 + q11 * r) * r3,
        (6.0 * q2 * q2 + 36.0 * q11 * q2 * r + 24.0 * q11 * q11 * r2) * r3,
    ];
    let a = [tau * v[0], tau * tau * v[1], tau * tau * tau * v[2], tau * tau * tau * tau * v[3]];
    let tau_side: f64 = (0..=i).map(|m| binomial(i, m) * rising(term.t, i - m) * bell(m, &a)).sum();
    phi.abs() * tau_side * gaussian_side(f64::from(term.d), term.eta, term.epsilon, delta, j)
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

    /// The τ-side mirrors the δ-side: a Lemmon2005 term with m = 2 at τ = 0.5 (y = 0.25) and t = 1 has the τ-scale
    /// 4.75 of the l = 2 power term above; l = 0 removes the δ exponential, so A_00 is |n|τ^t δ^d e^(−τ^m). A
    /// DoubleExponential term with g_t < 0 keeps |y|.
    #[test]
    fn tau_side_scales_by_hand() {
        let close = |a: f64, b: f64| ((a / b) - 1.0).abs() < 1e-15;
        let lemmon = Lemmon2005Term { n: -2.0, t: 1.0, d: 3, l: 0, m: 2.0 };
        let phi = 2.0 * 0.5 * math::exp(-0.25) * 0.125;
        assert!(close(lemmon2005(&lemmon, 0.5, 0.5, 0, 0), phi));
        assert!(close(lemmon2005(&lemmon, 0.5, 0.5, 2, 0), 4.75 * phi));
        assert!(close(lemmon2005(&lemmon, 0.5, 0.5, 0, 1), 3.0 * phi));
        // m = 0 removes the τ exponential, l = 1 keeps e^(−δ): A_00 = |n| τ^t δ^d e^(−δ).
        let flat = Lemmon2005Term { n: 1.0, t: 1.0, d: 1, l: 1, m: 0.0 };
        assert!(close(lemmon2005(&flat, 0.5, 0.5, 0, 0), 0.25 * math::exp(-0.5)));
        let double = DoubleExponentialTerm { n: 1.0, t: 0.0, d: 1, gd: 0.0, ld: 2, gt: -3.0, lt: 1.0 };
        let phi = 2.0 * math::exp(6.0);
        // τ-side, i = 1: t + m|y| = 0 + 6; δ-side, j = 1: d = 1 (g_d = 0).
        assert!(close(double_exponential(&double, 2.0, 2.0, 1, 1), 6.0 * phi));
    }

    /// A Gaussian side by hand at z = 2, ε = 1.5, η = 3: |z u'| = 6, |z² u''| = 24; with p = 1, k = 2:
    /// p^(2) + 2·p·6 + (6² + 24) = 2 + 12 + 60 = 74. The τ-side with β = 0 is the power's rising factorial.
    #[test]
    fn gaussian_scale_by_hand() {
        let close = |a: f64, b: f64| ((a / b) - 1.0).abs() < 1e-15;
        let term = GaussianTerm { n: -0.5, t: 1.5, d: 1, eta: 3.0, epsilon: 1.5, beta: 0.0, gamma: 7.0 };
        let phi = 0.5 * math::powf(2.0, 1.5) * 2.0 * math::exp(-0.75);
        assert!(close(gaussian(&term, 2.0, 2.0, 0, 0), phi));
        assert!(close(gaussian(&term, 2.0, 2.0, 0, 2), 74.0 * phi));
        assert!(close(gaussian(&term, 2.0, 2.0, 2, 0), 1.5 * 2.5 * phi));
        // β > 0 on the τ-side: τ = 2, γ = 1.5, β = 3 gives the same 74 for t = 1.
        let tau_term = GaussianTerm { n: 1.0, t: 1.0, d: 0, eta: 0.0, epsilon: 0.0, beta: 3.0, gamma: 1.5 };
        let phi = 2.0 * math::exp(-0.75);
        assert!(close(gaussian(&tau_term, 2.0, 0.5, 2, 0), 74.0 * phi));
    }

    /// A GaoB τ-side by hand at τ = 1, γ = 0, β = 1, b = 1 (q = 2, |q′| = |q″| = 2, r = 1/2): |v′| = 1/2,
    /// |v″| = (4 + 2)/4 = 3/2, so with t = 0 and i = 2: B_2(1/2, 3/2) = 1/4 + 3/2 = 7/4; φ = e^(1/2) with d = 0.
    #[test]
    fn gao_b_scale_by_hand() {
        let close = |a: f64, b: f64| ((a / b) - 1.0).abs() < 1e-15;
        let term = GaoBTerm { n: 1.0, t: 0.0, d: 0, eta: 0.0, epsilon: 0.0, beta: 1.0, gamma: 0.0, b: 1.0 };
        let phi = math::exp(0.5);
        assert!(close(gao_b(&term, 1.0, 0.5, 0, 0), phi));
        assert!(close(gao_b(&term, 1.0, 0.5, 1, 0), 0.5 * phi));
        assert!(close(gao_b(&term, 1.0, 0.5, 2, 0), 1.75 * phi));
        // i = 3: B_3 = a1³ + 3a1a2 + a3 with |v‴| = 6·2·(2 + 4/2)/8 = 6.
        assert!(close(gao_b(&term, 1.0, 0.5, 3, 0), (0.125 + 2.25 + 6.0) * phi));
        // τ = 2, γ = 1, β = b = 1 (w = 1, q = 2, |q′| = |q″| = 2, r = 1/2): |v′..v⁗| = 1/2, 3/2, 6, 33, so
        // h = (τ|v′|, τ²|v″|, τ³|v‴|, τ⁴|v⁗|) = (1, 6, 48, 528) and B_1..B_4 = 1, 7, 67, 865.
        let shifted = GaoBTerm { gamma: 1.0, ..term };
        for (i, bell) in [(1, 1.0), (2, 7.0), (3, 67.0), (4, 865.0)] {
            assert!(close(gao_b(&shifted, 2.0, 0.5, i, 0), bell * phi), "i = {i}");
        }
        // τ = 2.5 (w = 1.5, q = 13/4, |q′| = 3, |q″| = 2): exact fractions give B_1..B_4 = 120/169,
        // 141800/28561, 236196000/4826809, 528628500000/815730721; φ = e^(4/13).
        let phi = math::exp(4.0 / 13.0);
        let exact = [120.0 / 169.0, 141800.0 / 28561.0, 236196000.0 / 4826809.0, 528628500000.0 / 815730721.0];
        for (i, bell) in exact.into_iter().enumerate() {
            let got = gao_b(&shifted, 2.5, 0.5, i + 1, 0);
            assert!(((got / (bell * phi)) - 1.0).abs() < 1e-14, "i = {}", i + 1);
        }
        // η and d reach φ: d = 2, η = 3, ε = 0.5 at τ = 1, δ = 1.5 (q = 2): φ = δ² e^(1/2 − 3·1²) = 2.25 e^(−2.5).
        let full = GaoBTerm { n: 1.0, t: 0.0, d: 2, eta: 3.0, epsilon: 0.5, beta: 1.0, gamma: 0.0, b: 1.0 };
        assert!(close(gao_b(&full, 1.0, 1.5, 0, 0), 2.25 * math::exp(-2.5)));
        // β = 2 and δ − ε = 3/2 keep every factor visible: τ = 3/2, δ = 2, q = 11/2, |q′| = 6, |q″| = 4, r = 2/11:
        // φ = δ² e^(2/11 − 27/4), h₁ = 36/121, B₂ = h₁² + h₂ = 19908/14641.
        let wide = GaoBTerm { n: 1.0, t: 0.0, d: 2, eta: 3.0, epsilon: 0.5, beta: 2.0, gamma: 0.0, b: 1.0 };
        let phi = 4.0 * math::exp(2.0 / 11.0 - 27.0 / 4.0);
        assert!((gao_b(&wide, 1.5, 2.0, 0, 0) / phi - 1.0).abs() < 1e-14);
        assert!((gao_b(&wide, 1.5, 2.0, 1, 0) / (36.0 / 121.0 * phi) - 1.0).abs() < 1e-14);
        assert!((gao_b(&wide, 1.5, 2.0, 2, 0) / (19908.0 / 14641.0 * phi) - 1.0).abs() < 1e-14);
        // j = 1 on the Gaussian δ-side: d + 2η|δ − ε|δ = 2 + 18.
        assert!((gao_b(&wide, 1.5, 2.0, 0, 1) / (20.0 * phi) - 1.0).abs() < 1e-14);
    }
}
