//! `Jet4`, the in-house bivariate jet (D2, S-07): exact derivatives to total order 4 for formulas written over
//! [`Real`]. The separable term kinds use precomputed factors instead (`helmholtz::power`); `Jet4` serves the
//! non-separable kinds (NonAnalytic, M4.1), families outside the core and the tests that check the fast path.

use core::ops::{Add, Div, Mul, Neg, Sub};

use super::{Real, math, sealed};
use crate::derivs::{Derivs, Order, SLOTS, idx};

/// A truncated bivariate Taylor polynomial `f(τ₀ + h, δ₀ + k) = Σ_(i+j≤4) c_ij h^i k^j` around a point (τ₀, δ₀):
/// arithmetic on it carries every partial derivative to total order 4 exactly, up to rounding. Value parts are
/// computed with the same operations as on `f64`, so a formula's value is bitwise the same on both.
///
/// ```
/// use phasekit_core::{Jet4, Real};
/// let (tau, delta) = (Jet4::tau(1.5), Jet4::delta(0.5));
/// let f = tau.powi(2) * delta.exp(); // τ² e^δ
/// assert_eq!(f.derivative(2, 1), Some(2.0 * phasekit_core::math::exp(0.5))); // ∂³f/∂τ²∂δ = 2e^δ
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Jet4 {
    /// `c_ij` at `idx(i, j)`: the derivative divided by `i! j!`.
    c: [f64; SLOTS],
}

/// `n!` for n ≤ 4.
const FACTORIAL: [f64; 5] = [1.0, 1.0, 2.0, 6.0, 24.0];

/// The 70 slot triples `(k, a, b)` of a product: `c_k += x_a · y_b` whenever the exponents of a and b add up to k's.
const PRODUCT: [(u8, u8, u8); 70] = {
    let mut table = [(0u8, 0u8, 0u8); 70];
    let mut n = 0;
    let mut total = 0;
    while total <= 4 {
        let mut i = 0;
        while i <= total {
            let j = total - i;
            let mut a = 0;
            while a <= i {
                let mut b = 0;
                while b <= j {
                    table[n] = (idx(i, j) as u8, idx(a, b) as u8, idx(i - a, j - b) as u8);
                    n += 1;
                    b += 1;
                }
                a += 1;
            }
            i += 1;
        }
        total += 1;
    }
    assert!(n == 70);
    table
};

impl Jet4 {
    /// A constant.
    pub const fn constant(value: f64) -> Self {
        let mut c = [0.0; SLOTS];
        c[0] = value;
        Self { c }
    }

    /// The first variable at `tau`: `τ₀ + h`.
    pub const fn tau(tau: f64) -> Self {
        let mut c = [0.0; SLOTS];
        (c[0], c[idx(1, 0)]) = (tau, 1.0);
        Self { c }
    }

    /// The second variable at `delta`: `δ₀ + k`.
    pub const fn delta(delta: f64) -> Self {
        let mut c = [0.0; SLOTS];
        (c[0], c[idx(0, 1)]) = (delta, 1.0);
        Self { c }
    }

    /// The value at the expansion point.
    pub const fn value(&self) -> f64 {
        self.c[0]
    }

    /// `∂^(i+j)f / ∂τ^i ∂δ^j` at the expansion point, or `None` for `i + j > 4`.
    pub fn derivative(&self, i: usize, j: usize) -> Option<f64> {
        (i + j <= 4).then(|| self.c[idx(i, j)] * FACTORIAL[i] * FACTORIAL[j])
    }

    /// The scaled derivatives `A_ij = τ^i δ^j ∂^(i+j)f/∂τ^i∂δ^j` of [`Derivs`], with `(tau, delta)` the expansion
    /// point this jet was built at.
    pub fn derivs(&self, tau: f64, delta: f64) -> Derivs {
        Derivs::from_fn(Order::Four, |i, j| {
            self.c[idx(i, j)] * FACTORIAL[i] * FACTORIAL[j] * math::powi(tau, i as i32) * math::powi(delta, j as i32)
        })
    }

    /// `f(self)` from `f` and its first four derivatives at the value: `Σ_n f⁽ⁿ⁾/n! εⁿ` by Horner, with `ε = self −
    /// value` nilpotent (ε⁵ = 0). The value part is `d[0]` itself.
    fn compose(self, d: [f64; 5]) -> Self {
        let mut eps = self;
        eps.c[0] = 0.0;
        let mut r = Self::constant(d[4] / FACTORIAL[4]);
        for n in (0..4).rev() {
            r = r * eps + d[n] / FACTORIAL[n];
        }
        r.c[0] = d[0];
        r
    }

    /// `x^y` with derivatives `(y)_k x^(y−k)`; a zero falling factorial gives an exact zero, so `x⁰`, `x¹` stay
    /// finite at x = 0.
    fn power(self, y: f64, pow: impl Fn(f64) -> f64) -> Self {
        let mut d = [0.0; 5];
        let mut falling = 1.0;
        for (k, slot) in d.iter_mut().enumerate() {
            *slot = if falling == 0.0 { 0.0 } else { falling * pow(y - k as f64) };
            falling *= y - k as f64;
        }
        self.compose(d)
    }
}

impl Add for Jet4 {
    type Output = Self;
    fn add(mut self, o: Self) -> Self {
        self.c.iter_mut().zip(o.c).for_each(|(a, b)| *a += b);
        self
    }
}

impl Sub for Jet4 {
    type Output = Self;
    fn sub(mut self, o: Self) -> Self {
        self.c.iter_mut().zip(o.c).for_each(|(a, b)| *a -= b);
        self
    }
}

impl Mul for Jet4 {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        let mut c = [0.0; SLOTS];
        c[0] = self.c[0] * o.c[0];
        for &(k, a, b) in &PRODUCT[1..] {
            c[usize::from(k)] += self.c[usize::from(a)] * o.c[usize::from(b)];
        }
        Self { c }
    }
}

impl Div for Jet4 {
    type Output = Self;
    fn div(self, o: Self) -> Self {
        let r = 1.0 / o.c[0];
        let mut q = self * o.compose([r, -r * r, 2.0 * r * r * r, -6.0 * r * r * r * r, 24.0 * r * r * r * r * r]);
        q.c[0] = self.c[0] / o.c[0];
        q
    }
}

impl Neg for Jet4 {
    type Output = Self;
    fn neg(mut self) -> Self {
        self.c.iter_mut().for_each(|a| *a = -*a);
        self
    }
}

impl Add<f64> for Jet4 {
    type Output = Self;
    fn add(mut self, x: f64) -> Self {
        self.c[0] += x;
        self
    }
}

impl Mul<f64> for Jet4 {
    type Output = Self;
    fn mul(mut self, x: f64) -> Self {
        self.c.iter_mut().for_each(|a| *a *= x);
        self
    }
}

impl sealed::Sealed for Jet4 {}

impl Real for Jet4 {
    fn from_f64(x: f64) -> Self {
        Self::constant(x)
    }
    /// Composed with unit derivatives, then scaled by e^x₀ once: a subnormal e^x₀ (NonAnalytic terms far from the
    /// critical point) is rounded once, not at every Horner step.
    fn exp(self) -> Self {
        let e = math::exp(self.c[0]);
        let mut r = self.compose([1.0; 5]) * e;
        r.c[0] = e;
        r
    }
    fn expm1(self) -> Self {
        let e = math::exp(self.c[0]);
        self.compose([math::expm1(self.c[0]), e, e, e, e])
    }
    fn ln(self) -> Self {
        let r = 1.0 / self.c[0];
        self.compose([math::ln(self.c[0]), r, -r * r, 2.0 * r * r * r, -6.0 * r * r * r * r])
    }
    fn ln_1p(self) -> Self {
        let r = 1.0 / (1.0 + self.c[0]);
        self.compose([math::ln_1p(self.c[0]), r, -r * r, 2.0 * r * r * r, -6.0 * r * r * r * r])
    }
    fn powi(self, n: i32) -> Self {
        let x = self.c[0];
        self.power(f64::from(n), |e| math::powi(x, e as i32))
    }
    fn powf(self, y: f64) -> Self {
        let x = self.c[0];
        self.power(y, |e| math::powf(x, e))
    }
    fn sqrt(self) -> Self {
        let (x, s) = (self.c[0], math::sqrt(self.c[0]));
        self.compose([s, 0.5 / s, -0.25 / (s * x), 0.375 / (s * x * x), -0.9375 / (s * x * x * x)])
    }
    fn sinh(self) -> Self {
        let (s, c) = (math::sinh(self.c[0]), math::cosh(self.c[0]));
        self.compose([s, c, s, c, s])
    }
    fn cosh(self) -> Self {
        let (s, c) = (math::sinh(self.c[0]), math::cosh(self.c[0]));
        self.compose([c, s, c, s, c])
    }
    /// atan′ = 1/u, atan″ = −2x/u², atan‴ = (6x² − 2)/u³, atan⁗ = −24x(x² − 1)/u⁴ with u = 1 + x².
    fn atan(self) -> Self {
        let x = self.c[0];
        let r = 1.0 / (1.0 + x * x);
        let d3 = (6.0 * x * x - 2.0) * r * r * r;
        self.compose([math::atan(x), r, -2.0 * x * r * r, d3, -24.0 * x * (x * x - 1.0) * r * r * r * r])
    }
    fn abs(self) -> Self {
        if self.c[0].is_sign_negative() { -self } else { self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// x³ at x = 2 along τ: 8, 12, 12, 6, 0; along a mixed variable τδ at (2, 3): ∂τ∂δ(τδ)³ = 9τ²δ² = 324.
    #[test]
    fn jet4_cubes_by_hand() {
        let x = Jet4::tau(2.0);
        let y = x * x * x;
        let want = [8.0, 12.0, 12.0, 6.0, 0.0];
        for (i, w) in want.into_iter().enumerate() {
            assert_eq!(y.derivative(i, 0), Some(w));
        }
        assert_eq!(y.derivative(0, 1), Some(0.0));
        let p = Jet4::tau(2.0) * Jet4::delta(3.0);
        let p3 = p.powi(3);
        assert_eq!((p3.value(), p3.derivative(1, 1), p3.derivative(2, 2)), (216.0, Some(324.0), Some(216.0)));
        assert_eq!((p3.derivative(3, 2), p3.derivative(4, 1), p3.derivative(0, 5)), (None, None, None));
        // A_ij: τ^i δ^j ∂: A_11 = 2·3·324.
        assert_eq!(p3.derivs(2.0, 3.0).get(1, 1), Some(1944.0));
    }

    /// The product table has every pair whose exponents add up, and only those.
    #[test]
    fn product_table_is_complete() {
        let mut count = [0usize; SLOTS];
        for &(k, a, b) in &PRODUCT {
            count[usize::from(k)] += 1;
            assert!(a < SLOTS as u8 && b < SLOTS as u8);
        }
        for n in 0..=4 {
            for i in 0..=n {
                assert_eq!(count[idx(i, n - i)], (i + 1) * (n - i + 1), "({i}, {})", n - i);
            }
        }
    }

    /// Value parts are bitwise those of `f64`, so the generic fast path gives the scalar bits (D2).
    #[test]
    fn value_parts_are_bitwise_f64() {
        let (a, b) = (0.7310585786300049_f64, -1.2345678901234567_f64);
        let (ja, jb) = (Jet4::tau(a), Jet4::delta(b));
        let pairs: [(Jet4, f64); 13] = [
            (ja * jb, a * b),
            (ja / jb, a / b),
            (ja + jb, a + b),
            (ja - jb, a - b),
            (ja.exp(), math::exp(a)),
            (ja.expm1(), math::expm1(a)),
            (ja.ln(), math::ln(a)),
            (jb.ln_1p(), math::ln_1p(b)),
            (jb.powi(-3), math::powi(b, -3)),
            (ja.powf(2.5), math::powf(a, 2.5)),
            (ja.sqrt(), math::sqrt(a)),
            (jb.atan() * 3.0 + 0.5, math::atan(b) * 3.0 + 0.5),
            (jb.abs().sinh() + jb.cosh(), math::sinh(b.abs()) + math::cosh(b)),
        ];
        for (k, (jet, scalar)) in pairs.into_iter().enumerate() {
            assert_eq!(jet.value().to_bits(), scalar.to_bits(), "pair {k}");
        }
    }

    /// x^n at x = 0 for n ≥ 0 stays finite: the derivatives with a zero falling factorial are exact zeros, not 0·∞.
    #[test]
    fn integer_powers_are_finite_at_zero() {
        let x = Jet4::delta(0.0);
        for n in 0..=5 {
            let y = x.powi(n);
            for (j, factorial) in FACTORIAL.into_iter().enumerate() {
                let want = if j == n as usize { factorial } else { 0.0 };
                assert_eq!(y.derivative(0, j), Some(want), "x^{n}, order {j}");
            }
        }
        assert_eq!(Jet4::tau(-2.0).abs().derivative(1, 0), Some(-1.0));
        assert_eq!(Jet4::tau(-0.0).abs().value().to_bits(), 0.0f64.to_bits());
    }
}
