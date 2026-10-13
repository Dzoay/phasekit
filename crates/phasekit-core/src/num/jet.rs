//! `Jet4`, the in-house bivariate jet (D2, S-07): exact derivatives to total order 4 for formulas written over
//! [`Real`]. The separable term kinds use precomputed factors instead (`helmholtz::power`); `Jet4` serves the
//! non-separable kinds (NonAnalytic, M4.1), families outside the core and the tests that check the fast path.
//! `Jet2`, crate-private, is the same code truncated at total order 2 (PLAN.md M9.2a, user decision NA1): a product's
//! entry of total order n sums only entries of order ≤ n, in the same order, so `Jet2`'s entries are bitwise `Jet4`'s.

use core::ops::{Add, Div, Mul, Neg, Sub};

use super::{Real, math, sealed};
use crate::derivs::{Derivs, Order, SLOTS, idx};

/// `n!` for n ≤ 4.
const FACTORIAL: [f64; 5] = [1.0, 1.0, 2.0, 6.0, 24.0];

/// The 70 slot triples `(k, a, b)` of a product: `c_k += x_a · y_b` whenever the exponents of a and b add up to k's.
/// Ordered by k's total order, so a jet of order n reads a prefix: 15 triples for n = 2, all 70 for n = 4.
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

/// A jet type of total order `$ord`, with `$slots` = (ord + 1)(ord + 2)/2 coefficients and products over the first
/// `$triples` entries of [`PRODUCT`]. Every order runs this one code, so a shorter jet's entries are the longer one's
/// bits.
macro_rules! jet {
    ($(#[$meta:meta])* $vis:vis struct $name:ident: $order:expr, $ord:literal, $slots:literal, $triples:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        $vis struct $name {
            /// `c_ij` at `idx(i, j)`: the derivative divided by `i! j!`.
            c: [f64; $slots],
        }

        impl $name {
            /// A constant.
            pub const fn constant(value: f64) -> Self {
                let mut c = [0.0; $slots];
                c[0] = value;
                Self { c }
            }

            /// The first variable at `tau`: `τ₀ + h`.
            pub const fn tau(tau: f64) -> Self {
                let mut c = [0.0; $slots];
                (c[0], c[idx(1, 0)]) = (tau, 1.0);
                Self { c }
            }

            /// The second variable at `delta`: `δ₀ + k`.
            pub const fn delta(delta: f64) -> Self {
                let mut c = [0.0; $slots];
                (c[0], c[idx(0, 1)]) = (delta, 1.0);
                Self { c }
            }

            /// The value at the expansion point.
            pub const fn value(&self) -> f64 {
                self.c[0]
            }

            /// `∂^(i+j)f / ∂τ^i ∂δ^j` at the expansion point, or `None` beyond the jet's total order.
            pub fn derivative(&self, i: usize, j: usize) -> Option<f64> {
                (i + j <= $ord).then(|| self.c[idx(i, j)] * FACTORIAL[i] * FACTORIAL[j])
            }

            /// The scaled derivatives `A_ij = τ^i δ^j ∂^(i+j)f/∂τ^i∂δ^j` of [`Derivs`] to the jet's order, with
            /// `(tau, delta)` the expansion point this jet was built at.
            pub fn derivs(&self, tau: f64, delta: f64) -> Derivs {
                Derivs::from_fn($order, |i, j| {
                    self.c[idx(i, j)]
                        * FACTORIAL[i]
                        * FACTORIAL[j]
                        * math::powi(tau, i as i32)
                        * math::powi(delta, j as i32)
                })
            }

            /// `f(self)` from `f` and its derivatives at the value, those beyond the jet's order unread: `Σ_n f⁽ⁿ⁾/n!
            /// εⁿ` by Horner, with `ε = self − value` nilpotent. The value part is `d[0]` itself. Each Horner product
            /// skips a zero factor, so an infinite derivative reaches only the slots its power of ε fills:
            /// `|δ − 1|^(10/3)` at δ = 1 has f⁗ = ∞ and ε⁴ = k⁴, and only `c_04` is infinite, where 0 · ∞ would have
            /// made every slot NaN (M4.6). Finite results are bitwise those of the plain product.
            fn compose(self, d: [f64; 5]) -> Self {
                let mut eps = self;
                eps.c[0] = 0.0;
                let mut r = Self::constant(d[$ord] / FACTORIAL[$ord]);
                for n in (0..$ord).rev() {
                    r = r.times_nilpotent(eps) + d[n] / FACTORIAL[n];
                }
                r.c[0] = d[0];
                r
            }

            /// `self · eps` for an `eps` whose value is zero, every product with an exact zero factor skipped.
            fn times_nilpotent(self, eps: Self) -> Self {
                let mut c = [0.0; $slots];
                for &(k, a, b) in &PRODUCT[1..$triples] {
                    let (x, y) = (self.c[usize::from(a)], eps.c[usize::from(b)]);
                    if x != 0.0 && y != 0.0 {
                        c[usize::from(k)] += x * y;
                    }
                }
                Self { c }
            }

            /// `x^y` with derivatives `(y)_k x^(y−k)` to the jet's order; a zero falling factorial gives an exact zero,
            /// so `x⁰`, `x¹` stay finite at x = 0.
            fn power(self, y: f64, pow: impl Fn(f64) -> f64) -> Self {
                let mut d = [0.0; 5];
                let mut falling = 1.0;
                for (k, slot) in d.iter_mut().enumerate().take($ord + 1) {
                    *slot = if falling == 0.0 { 0.0 } else { falling * pow(y - k as f64) };
                    falling *= y - k as f64;
                }
                self.compose(d)
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(mut self, o: Self) -> Self {
                self.c.iter_mut().zip(o.c).for_each(|(a, b)| *a += b);
                self
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(mut self, o: Self) -> Self {
                self.c.iter_mut().zip(o.c).for_each(|(a, b)| *a -= b);
                self
            }
        }

        impl Mul for $name {
            type Output = Self;
            fn mul(self, o: Self) -> Self {
                let mut c = [0.0; $slots];
                c[0] = self.c[0] * o.c[0];
                for &(k, a, b) in &PRODUCT[1..$triples] {
                    c[usize::from(k)] += self.c[usize::from(a)] * o.c[usize::from(b)];
                }
                Self { c }
            }
        }

        impl Div for $name {
            type Output = Self;
            fn div(self, o: Self) -> Self {
                let r = 1.0 / o.c[0];
                let inverse = [r, -r * r, 2.0 * r * r * r, -6.0 * r * r * r * r, 24.0 * r * r * r * r * r];
                let mut q = self * o.compose(inverse);
                q.c[0] = self.c[0] / o.c[0];
                q
            }
        }

        impl Neg for $name {
            type Output = Self;
            fn neg(mut self) -> Self {
                self.c.iter_mut().for_each(|a| *a = -*a);
                self
            }
        }

        impl Add<f64> for $name {
            type Output = Self;
            fn add(mut self, x: f64) -> Self {
                self.c[0] += x;
                self
            }
        }

        impl Mul<f64> for $name {
            type Output = Self;
            fn mul(mut self, x: f64) -> Self {
                self.c.iter_mut().for_each(|a| *a *= x);
                self
            }
        }

        impl sealed::Sealed for $name {}

        impl Real for $name {
            fn from_f64(x: f64) -> Self {
                Self::constant(x)
            }
            /// Composed with unit derivatives, then scaled by e^x₀ once: a subnormal e^x₀ (NonAnalytic terms far from
            /// the critical point) is rounded once, not at every Horner step.
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
    };
}

jet! {
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
    pub struct Jet4: Order::Four, 4, 15, 70
}

jet! {
    /// [`Jet4`] truncated at total order 2 (PLAN.md M9.2a): the NonAnalytic block's jet for bundles of order ≤ 2, its 6
    /// entries bitwise `Jet4`'s of order ≤ 2.
    pub(crate) struct Jet2: Order::Two, 2, 6, 15
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

    /// An infinite derivative stays in the slots its power of ε fills: |δ − 1|^(10/3) at δ = 1 has f⁗ = ∞ and ε⁴ = k⁴,
    /// so ∂⁴/∂δ⁴ is ∞ and every other derivative 0. e^(τ + that) at τ = 2 carries the ∞ through a second composition:
    /// e² in every pure τ-slot, ∞ in the δ⁴ one, 0 elsewhere. With plain products every slot of both was NaN.
    #[test]
    fn infinite_derivatives_stay_in_their_slots() {
        let x = (Jet4::delta(1.0) + -1.0).abs().powf(10.0 / 3.0);
        let y = (Jet4::tau(2.0) + x).exp();
        let e2 = math::exp(2.0);
        for n in 0..=4 {
            for i in 0..=n {
                let j = n - i;
                let infinite = (i, j) == (0, 4);
                assert_eq!(x.derivative(i, j), Some(if infinite { f64::INFINITY } else { 0.0 }), "x ({i}, {j})");
                let got = y.derivative(i, j).unwrap();
                if j == 0 {
                    assert!((got / e2 - 1.0).abs() < 1e-15, "y ({i}, 0): {got}");
                } else {
                    assert_eq!(got, if infinite { f64::INFINITY } else { 0.0 }, "y ({i}, {j})");
                }
            }
        }
    }

    /// PLAN.md M9.2a: `Jet2` is `Jet4` truncated, bit for bit. A formula through every `Real` method, the
    /// operators and an infinite fourth derivative (|δ − 1|^(10/3) at δ = 1), at points on both sides of 1: each entry
    /// of order ≤ 2 has the same bits on both jets, and `Jet2` has none above.
    #[test]
    fn jet2_entries_are_jet4_bitwise() {
        fn formula<R: Real>(tau: R, delta: R) -> R {
            let w = (delta + -1.0).abs();
            let a = (tau * delta).exp() + (tau + 0.5).ln() * delta.expm1() - (delta * 0.3).ln_1p() / tau.sqrt();
            let b = tau.powi(3) * w.powf(10.0 / 3.0) + (tau * 0.2).sinh() * delta.cosh() + delta.atan() * -2.0;
            a * b + w.powf(0.85) / (tau * tau + delta)
        }
        for (tau, delta) in [(1.5, 0.5), (0.7, 1.0), (1.0, 1.3), (2.5, 1e-3)] {
            let two = formula(Jet2::tau(tau), Jet2::delta(delta));
            let four = formula(Jet4::tau(tau), Jet4::delta(delta));
            for n in 0..=2 {
                for i in 0..=n {
                    let (a, b) = (two.derivative(i, n - i), four.derivative(i, n - i));
                    assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits), "({i}, {}) at ({tau}, {delta})", n - i);
                }
            }
            assert_eq!(
                (two.derivative(3, 0), two.derivative(0, 3), four.derivative(0, 3).is_some()),
                (None, None, true)
            );
            let (d2, d4) = (two.derivs(tau, delta), four.derivs(tau, delta));
            assert_eq!(d2.order(), Order::Two);
            assert_eq!(d2.get(2, 0).map(f64::to_bits), d4.get(2, 0).map(f64::to_bits));
        }
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
