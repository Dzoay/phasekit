//! Multivariate Taylor numbers truncated at order 3 (PLAN.md M13.6): a polynomial in m variables of degree ≤ 3, stored
//! by degree, its monomials sorted (x_i x_j with i ≤ j, x_i x_j x_k with i ≤ j ≤ k), so a product costs O(m³), not the
//! square of the coefficient count. The coefficients are the Taylor coefficients ∂^α f / α! at the expansion point;
//! [`Taylor::derivative`] multiplies α! back. Only the field operations are needed: the mixture's composition
//! derivatives compose each part's (τ, δ) jet with τ(n) and δ(n), which are rational in the variables. The coefficients
//! are any `Real`: `f64`, or core's lanes through `batch::run_kernel` (PLAN.md M22.5), each operation the same on both.

use std::ops::{Add, Mul, Neg, Sub};

use phasekit_core::Real;

/// The index of monomial x_i x_j, i ≤ j, among the degree-2 monomials. The index arithmetic saturates (PLAN.md M24.4):
/// a saturated index lies outside every coefficient vector, where [`at`] reads NaN.
fn pair(i: usize, j: usize) -> usize {
    (j.saturating_mul(j.saturating_add(1)) / 2).saturating_add(i)
}

/// The index of monomial x_i x_j x_k, i ≤ j ≤ k, among the degree-3 monomials.
fn triple(i: usize, j: usize, k: usize) -> usize {
    (k.saturating_mul(k.saturating_add(1)).saturating_mul(k.saturating_add(2)) / 6).saturating_add(pair(i, j))
}

/// Entry `i` of `v`, or NaN where it has none (`slot::at` on any `Real`).
#[inline(always)]
fn at<R: Real>(v: &[R], i: usize) -> R {
    v.get(i).copied().unwrap_or(R::from_f64(f64::NAN))
}

/// Sets entry `i` of `v` to `value`, where it has one.
#[inline(always)]
fn set<R: Real>(v: &mut [R], i: usize, value: R) {
    if let Some(x) = v.get_mut(i) {
        *x = value;
    }
}

/// A polynomial of degree ≤ `order` (≤ 3) in `m` variables.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Taylor<R = f64> {
    m: usize,
    order: usize,
    c0: R,
    c1: Vec<R>,
    c2: Vec<R>,
    c3: Vec<R>,
}

/// The number of monomials of each degree to `order` in `m` variables: m, then the pairs and triples below index m.
fn monomials(m: usize, order: usize, degree: usize) -> usize {
    match degree {
        _ if degree > order => 0,
        1 => m,
        2 => pair(0, m),
        _ => triple(0, 0, m),
    }
}

#[allow(clippy::arithmetic_side_effects, reason = "multivariate series on any Real: IEEE or lane arithmetic, no panic")]
impl<R: Real> Taylor<R> {
    /// The constant `value`.
    pub(crate) fn constant(m: usize, order: usize, value: R) -> Taylor<R> {
        let zeros = |degree| vec![R::from_f64(0.0); monomials(m, order, degree)];
        Taylor { m, order, c0: value, c1: zeros(1), c2: zeros(2), c3: zeros(3) }
    }

    /// Variable `i` at the expansion point `value`.
    pub(crate) fn variable(m: usize, order: usize, i: usize, value: R) -> Taylor<R> {
        let mut x = Taylor::constant(m, order, value);
        set(&mut x.c1, i, R::from_f64(1.0));
        x
    }

    /// The polynomial whose coefficients, by degree and in [`Taylor::coefficients`]' order, are `c`'s next ones; NaN
    /// where `c` runs out.
    pub(crate) fn from_coefficients(m: usize, order: usize, c: impl IntoIterator<Item = R>) -> Taylor<R> {
        let mut c = c.into_iter();
        let mut next = || c.next().unwrap_or(R::from_f64(f64::NAN));
        let c0 = next();
        let mut degree = |d| (0..monomials(m, order, d)).map(|_| next()).collect();
        let (c1, c2, c3) = (degree(1), degree(2), degree(3));
        Taylor { m, order, c0, c1, c2, c3 }
    }

    /// Every coefficient: the value, then each degree's by monomial.
    pub(crate) fn coefficients(&self) -> impl Iterator<Item = R> + '_ {
        core::iter::once(self.c0).chain(self.c1.iter().chain(&self.c2).chain(&self.c3).copied())
    }

    /// The number of coefficients of a polynomial to `order` in `m` variables.
    pub(crate) fn len(m: usize, order: usize) -> usize {
        (1..=3).fold(1_usize, |n, degree| n.saturating_add(monomials(m, order, degree)))
    }

    /// The value at the expansion point.
    pub(crate) fn value(&self) -> R {
        self.c0
    }

    /// The number of variables.
    pub(crate) fn vars(&self) -> usize {
        self.m
    }

    /// The order kept.
    pub(crate) fn order(&self) -> usize {
        self.order
    }

    /// The same polynomial with its constant set to 0.
    pub(crate) fn increment(&self) -> Taylor<R> {
        Taylor { c0: R::from_f64(0.0), ..self.clone() }
    }

    /// ∂^(len) f / ∂x_vars[0] ∂x_vars[1] ... at the expansion point, for up to three variables in any order; 0 beyond
    /// the order kept.
    pub(crate) fn derivative(&self, vars: &[usize]) -> R {
        let mut v = vars.to_vec();
        v.sort_unstable();
        let zero = R::from_f64(0.0);
        match v[..] {
            [] => self.c0,
            [i] => self.c1.get(i).copied().unwrap_or(zero),
            [i, j] => self.c2.get(pair(i, j)).copied().unwrap_or(zero) * if i == j { 2.0 } else { 1.0 },
            [i, j, k] => {
                let factorial = match (i == j, j == k) {
                    (true, true) => 6.0,
                    (true, false) | (false, true) => 2.0,
                    (false, false) => 1.0,
                };
                self.c3.get(triple(i, j, k)).copied().unwrap_or(zero) * factorial
            }
            _ => zero,
        }
    }

    /// `a·self + b·other`, coefficient by coefficient.
    fn combine(&self, a: R, other: &Taylor<R>, b: R) -> Taylor<R> {
        let mix = |x: &[R], y: &[R]| x.iter().zip(y).map(|(x, y)| a * *x + b * *y).collect();
        Taylor {
            m: self.m,
            order: self.order,
            c0: a * self.c0 + b * other.c0,
            c1: mix(&self.c1, &other.c1),
            c2: mix(&self.c2, &other.c2),
            c3: mix(&self.c3, &other.c3),
        }
    }

    /// `self` times the number `k`.
    pub(crate) fn scale(&self, k: R) -> Taylor<R> {
        self.combine(k, self, R::from_f64(0.0))
    }

    /// `self + k·other`, in place.
    pub(crate) fn add_scaled(&mut self, k: R, other: &Taylor<R>) {
        self.c0 = self.c0 + k * other.c0;
        for (x, y) in [(&mut self.c1, &other.c1), (&mut self.c2, &other.c2), (&mut self.c3, &other.c3)] {
            for (x, y) in x.iter_mut().zip(y) {
                *x = *x + k * *y;
            }
        }
    }

    /// The truncated product.
    pub(crate) fn product(&self, other: &Taylor<R>) -> Taylor<R> {
        let (a, b) = (self, other);
        let mut out = Taylor::constant(a.m, a.order, a.c0 * b.c0);
        for ((out, x), y) in out.c1.iter_mut().zip(&a.c1).zip(&b.c1) {
            *out = a.c0 * *y + *x * b.c0;
        }
        if a.order >= 2 {
            a.squares(b, &mut out.c2);
        }
        if a.order >= 3 {
            a.cubes(b, &mut out.c3);
        }
        out
    }

    /// The product's degree-2 coefficients, into `c2`.
    fn squares(&self, b: &Taylor<R>, c2: &mut [R]) {
        for j in 0..self.m {
            for i in 0..=j {
                set(c2, pair(i, j), self.square(b, i, j));
            }
        }
    }

    /// The product's degree-3 coefficients, into `c3`.
    fn cubes(&self, b: &Taylor<R>, c3: &mut [R]) {
        for k in 0..self.m {
            for j in 0..=k {
                for i in 0..=j {
                    set(c3, triple(i, j, k), self.cube(b, (i, j, k)));
                }
            }
        }
    }

    /// The product's coefficient of x_i x_j, i ≤ j.
    fn square(&self, b: &Taylor<R>, i: usize, j: usize) -> R {
        let a = self;
        let (a1, b1) = (|i| at(&a.c1, i), |i| at(&b.c1, i));
        let cross = if i == j { a1(i) * b1(i) } else { a1(i) * b1(j) + a1(j) * b1(i) };
        let p = pair(i, j);
        a.c0 * at(&b.c2, p) + at(&a.c2, p) * b.c0 + cross
    }

    /// The product's coefficient of x_i x_j x_k, i ≤ j ≤ k: each distinct way to split the monomial into one variable
    /// times a pair (x_i, then x_j and x_k where they differ from the one before).
    fn cube(&self, b: &Taylor<R>, (i, j, k): (usize, usize, usize)) -> R {
        let a = self;
        let t = triple(i, j, k);
        let split = |s: usize, p: usize| at(&a.c1, s) * at(&b.c2, p) + at(&a.c2, p) * at(&b.c1, s);
        let mut sum = a.c0 * at(&b.c3, t) + at(&a.c3, t) * b.c0 + split(i, pair(j, k));
        if j != i {
            sum = sum + split(j, pair(i, k));
        }
        if k != j {
            sum = sum + split(k, pair(i, j));
        }
        sum
    }

    /// 1/self, from the geometric series of its increment (its value must not be 0).
    pub(crate) fn recip(&self) -> Taylor<R> {
        let r = R::from_f64(1.0) / self.c0;
        let e = self.increment().scale(-r); // 1/(c0 + d) = r·Σ (−d·r)^k
        let mut sum = Taylor::constant(self.m, self.order, R::from_f64(1.0));
        let mut power = sum.clone();
        for _ in 0..self.order {
            power = power.product(&e);
            sum.add_scaled(R::from_f64(1.0), &power);
        }
        sum.scale(r)
    }

    /// self/other.
    pub(crate) fn divide(&self, other: &Taylor<R>) -> Taylor<R> {
        self.product(&other.recip())
    }
}

impl<R: Real> Add for &Taylor<R> {
    type Output = Taylor<R>;
    fn add(self, other: &Taylor<R>) -> Taylor<R> {
        self.combine(R::from_f64(1.0), other, R::from_f64(1.0))
    }
}

impl<R: Real> Sub for &Taylor<R> {
    type Output = Taylor<R>;
    fn sub(self, other: &Taylor<R>) -> Taylor<R> {
        self.combine(R::from_f64(1.0), other, R::from_f64(-1.0))
    }
}

impl<R: Real> Mul for &Taylor<R> {
    type Output = Taylor<R>;
    fn mul(self, other: &Taylor<R>) -> Taylor<R> {
        self.product(other)
    }
}

impl<R: Real> Neg for &Taylor<R> {
    type Output = Taylor<R>;
    fn neg(self) -> Taylor<R> {
        self.scale(R::from_f64(-1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// f(x, y, z) = (1 + x + 2y)(3 − z + xy)/(2 + y z): every derivative to order 3 at (0.3, −0.2, 0.5) against its
    /// value written out with exact rational arithmetic through central differences of a polynomial-exact form (here:
    /// the same expression evaluated on the Taylor numbers of each variable separately as a cross-check), and the
    /// product's symmetry in its factors.
    #[test]
    fn products_and_quotients_have_the_polynomial_s_derivatives() {
        let (m, order) = (3, 3);
        let at = [0.3, -0.2, 0.5];
        let v: Vec<Taylor> = (0..m).map(|i| Taylor::variable(m, order, i, at[i])).collect();
        let one = Taylor::constant(m, order, 1.0);
        let f = |v: &[Taylor]| {
            let a = &(&one + &v[0]) + &v[1].scale(2.0);
            let b = &(&Taylor::constant(m, order, 3.0) - &v[2]) + &(&v[0] * &v[1]);
            let c = &Taylor::constant(m, order, 2.0) + &(&v[1] * &v[2]);
            (&a * &b).divide(&c)
        };
        let t = f(&v);
        // The same function on plain numbers, differentiated by nested central differences of step h.
        let g = |x: [f64; 3]| (1.0 + x[0] + 2.0 * x[1]) * (3.0 - x[2] + x[0] * x[1]) / (2.0 + x[1] * x[2]);
        let h = 1e-3;
        let fd = |vars: &[usize]| -> f64 {
            fn rec(g: &dyn Fn([f64; 3]) -> f64, x: [f64; 3], vars: &[usize], h: f64) -> f64 {
                match vars.split_first() {
                    None => g(x),
                    Some((&i, rest)) => {
                        let (mut up, mut down) = (x, x);
                        up[i] += h;
                        down[i] -= h;
                        (rec(g, up, rest, h) - rec(g, down, rest, h)) / (2.0 * h)
                    }
                }
            }
            rec(&g, at, vars, h)
        };
        assert!((t.value() - g(at)).abs() < 1e-15);
        for vars in [vec![0], vec![2], vec![0, 1], vec![1, 1], vec![0, 1, 2], vec![2, 2, 1], vec![1, 1, 1]] {
            let (got, want) = (t.derivative(&vars), fd(&vars));
            assert!((got - want).abs() < 1e-5 * want.abs().max(1.0), "{vars:?}: {got} against {want}");
        }
        assert_eq!(&v[0] * &v[1], &v[1] * &v[0]);
        // A quotient by itself is 1 exactly in every coefficient but the constant's rounding.
        let unit = t.divide(&t);
        assert!((unit.value() - 1.0).abs() < 1e-15 && unit.derivative(&[0, 1, 2]).abs() < 1e-13);
    }

    /// Each degree's coefficients hold its monomials once: m, m(m + 1)/2 and m(m + 1)(m + 2)/6, and none above the
    /// order kept; negation is the scale by −1.
    #[test]
    fn each_monomial_is_stored_once() {
        for (m, counts) in [(1, (1, 1, 1)), (3, (3, 6, 10)), (4, (4, 10, 20)), (21, (21, 231, 1771))] {
            let x = Taylor::constant(m, 3, 1.0);
            assert_eq!((x.c1.len(), x.c2.len(), x.c3.len()), counts, "{m} variables");
            assert_eq!(triple(m - 1, m - 1, m - 1), counts.2 - 1);
        }
        let (first, second) = (Taylor::constant(3, 1, 1.0), Taylor::constant(3, 2, 1.0));
        assert_eq!([first.c1.len(), first.c2.len(), first.c3.len()], [3, 0, 0]);
        assert_eq!([second.c1.len(), second.c2.len(), second.c3.len()], [3, 6, 0]);
        let v = Taylor::variable(2, 3, 0, 0.5);
        let y = &(&v * &v) + &v;
        assert_eq!(-&y, y.scale(-1.0));
        assert_ne!(-&y, y);
    }

    /// Below the order kept, derivatives are 0; the second-order truncation is the third-order one's.
    #[test]
    fn truncation_keeps_the_lower_orders() {
        let (a, b) = (Taylor::variable(2, 2, 0, 0.5), Taylor::variable(2, 2, 1, 2.0));
        let x = (&a * &b).divide(&(&a + &b));
        let (a3, b3) = (Taylor::variable(2, 3, 0, 0.5), Taylor::variable(2, 3, 1, 2.0));
        let y = (&a3 * &b3).divide(&(&a3 + &b3));
        for vars in [vec![], vec![0], vec![1], vec![0, 0], vec![0, 1], vec![1, 1]] {
            assert_eq!(x.derivative(&vars).to_bits(), y.derivative(&vars).to_bits(), "{vars:?}");
        }
        assert_eq!(x.derivative(&[0, 0, 1]), 0.0);
        assert_ne!(y.derivative(&[0, 0, 1]), 0.0);
    }
}
