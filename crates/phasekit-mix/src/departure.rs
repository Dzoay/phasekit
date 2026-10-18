//! A binary pair's departure function α^r_ij(τ, δ) in one kernel over [`Scalar`] (f64, and [`Series`] for its τ, δ
//! derivatives; ROT-218's pattern), its terms as CoolProp 8.0.0 evaluates them (`ExcessHEFunction.h:104-196`,
//! `Helmholtz.h:377-457`; PLAN.md M13.3):
//! - Exponential: Σ n δ^d τ^t exp(−δ^l), without the exponential where l = 0;
//! - GERG-2008: `power` terms n δ^d τ^t, then n δ^d τ^t exp(−η(δ − ε)² − β(δ − γ)): β multiplies δ − γ, not its square;
//! - Gaussian + Exponential: `power` terms n δ^d τ^t exp(−δ^l), then n δ^d τ^t exp(−η(δ − ε)² − β(τ − γ)²).
//!
//! Every term is separable, n T(τ) D(δ), so its derivatives are products of two univariate series: the jet to order k
//! costs O(k²) a term, not a bivariate composition's (PLAN.md M13.7: the critical search evaluates it some ten thousand
//! times a mixture).

use core::ops::{Mul, Sub};

use phasekit_core::{Derivs, Order, math};

use crate::data::{DepartureData, Kind, Term};
use crate::slot::at;

/// n! for n ≤ 4.
const FACTORIAL: [f64; 5] = [1.0, 1.0, 2.0, 6.0, 24.0];

/// A series' last slot: its coefficients up to the fourth order.
const LAST: usize = 4;

/// What the kernel computes on: a number, or a univariate series.
pub(crate) trait Scalar: Copy + Sub<Output = Self> + Mul<Output = Self> {
    fn from_f64(x: f64) -> Self;
    fn powi(self, n: i32) -> Self;
    fn powf(self, t: f64) -> Self;
    fn exp(self) -> Self;
}

impl Scalar for f64 {
    fn from_f64(x: f64) -> f64 {
        x
    }
    fn powi(self, n: i32) -> f64 {
        math::powi(self, n)
    }
    fn powf(self, t: f64) -> f64 {
        math::powf(self, t)
    }
    fn exp(self) -> f64 {
        math::exp(self)
    }
}

/// A univariate Taylor series truncated at `order` ≤ 4: `c[k]` is the k-th derivative over k!. Its value slot is
/// computed as the f64 kernel computes it, so a jet's value is the plain evaluation's, bit for bit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Series {
    order: usize,
    c: [f64; 5],
}

impl Series {
    /// The variable at `x`.
    fn variable(x: f64, order: usize) -> Series {
        let mut c = [0.0; 5];
        (c[0], c[1]) = (x, if order >= 1 { 1.0 } else { 0.0 });
        Series { order, c }
    }

    /// f(self) from f's derivatives `d` at the value: Σ_n d_n/n! εⁿ by Horner, ε = self − value; for the variable itself
    /// (ε the unit monomial) the coefficients d_n/n! directly, which is what Horner's products with it give exactly.
    #[allow(clippy::arithmetic_side_effects, reason = "Series products: IEEE arithmetic on truncated series, no panic")]
    fn compose(self, d: [f64; 5]) -> Series {
        if self.c[1] == 1.0 && self.c[2..].iter().all(|c| *c == 0.0) {
            let mut c = [0.0; 5];
            for ((c, d), f) in c.iter_mut().zip(&d).zip(&FACTORIAL).take(self.order.saturating_add(1)) {
                *c = d / f;
            }
            return Series { order: self.order, c };
        }
        let mut eps = self;
        eps.c[0] = 0.0;
        let mut r = Series::from_f64(at(&d, self.order) / at(&FACTORIAL, self.order)).with_order(self.order);
        let below = self.order.min(d.len());
        for (d, f) in d.get(..below).unwrap_or_default().iter().zip(FACTORIAL.get(..below).unwrap_or_default()).rev() {
            r = r * eps;
            r.c[0] = d / f;
        }
        r.c[0] = d[0];
        r
    }

    fn with_order(mut self, order: usize) -> Series {
        self.order = order;
        self
    }

    /// The truncated product; the value slot is the values' product alone. A constant's product is a scale, the general
    /// sum's terms with its zero slots being exact zeros. Each slot sums its terms in index order, from 0. Plain index
    /// loops, up to the order or the last slot if that is lower, so every index lies in the five slots: an iterator
    /// chain here cost the GERG gas's α^r a fifth more instructions (PLAN.md M24.4).
    #[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects, reason = "k ≤ last ≤ 4 and a ≤ k: in the slots")]
    fn product(&self, other: &Series) -> Series {
        let order = self.order.max(other.order);
        let last = order.min(LAST);
        let mut c = [0.0; 5];
        c[0] = self.c[0] * other.c[0];
        if self.order == 0 || other.order == 0 {
            let (series, k0) = if self.order == 0 { (other, self.c[0]) } else { (self, other.c[0]) };
            let mut k = 1;
            while k <= last {
                c[k] = k0 * series.c[k];
                k += 1;
            }
            return Series { order, c };
        }
        let mut k = 1;
        while k <= last {
            let (mut sum, mut a) = (0.0, 0);
            while a <= k {
                sum += self.c[a] * other.c[k - a];
                a += 1;
            }
            c[k] = sum;
            k += 1;
        }
        Series { order, c }
    }
}

impl Sub for Series {
    type Output = Series;
    fn sub(mut self, other: Series) -> Series {
        self.order = self.order.max(other.order);
        for (x, y) in self.c.iter_mut().zip(other.c) {
            *x -= y;
        }
        self
    }
}

impl Mul for Series {
    type Output = Series;
    fn mul(self, other: Series) -> Series {
        self.product(&other)
    }
}

impl Scalar for Series {
    /// A constant, of order 0: an operand of any order raises it.
    fn from_f64(x: f64) -> Series {
        let mut c = [0.0; 5];
        c[0] = x;
        Series { order: 0, c }
    }

    /// x^n's derivatives n (n − 1) ... (n − k + 1) x^(n − k), 0 where the falling factorial is (x = 0 at δ = 0).
    fn powi(self, n: i32) -> Series {
        let mut d = [0.0; 5];
        let mut falling = 1.0;
        for (k, slot) in d.iter_mut().enumerate().take(self.order.saturating_add(1)) {
            *slot = if falling == 0.0 { 0.0 } else { falling * math::powi(self.c[0], n.saturating_sub(k as i32)) };
            falling *= f64::from(n) - k as f64;
        }
        self.compose(d)
    }

    fn powf(self, t: f64) -> Series {
        let mut d = [0.0; 5];
        let mut falling = 1.0;
        for (k, slot) in d.iter_mut().enumerate().take(self.order.saturating_add(1)) {
            *slot = falling * math::powf(self.c[0], t - k as f64);
            falling *= t - k as f64;
        }
        self.compose(d)
    }

    fn exp(self) -> Series {
        self.compose([math::exp(self.c[0]); 5])
    }
}

/// A term's τ part: τ^t, times exp(−β(τ − γ)²) for a Gaussian + Exponential function's Gaussian terms.
#[allow(clippy::arithmetic_side_effects, reason = "on the kernel's scalars, f64 or Series: IEEE arithmetic, no panic")]
fn tau_part<S: Scalar>(kind: Kind, gaussian: bool, term: &Term, tau: S) -> S {
    let power = tau.powf(term.t);
    match (kind, gaussian) {
        (Kind::GaussianExponential, true) => {
            let t = tau - S::from_f64(term.gamma);
            power * (S::from_f64(0.0) - S::from_f64(term.beta) * t * t).exp()
        }
        _ => power,
    }
}

/// A term's δ part: δ^d times its exponential in δ, if it has one.
#[allow(clippy::arithmetic_side_effects, reason = "on the kernel's scalars, f64 or Series: IEEE arithmetic, no panic")]
fn delta_part<S: Scalar>(kind: Kind, gaussian: bool, term: &Term, delta: S) -> S {
    let power = delta.powi(term.d as i32);
    let c = S::from_f64;
    let exponent = match (kind, gaussian) {
        (Kind::Gerg2008, false) => return power,
        (Kind::Exponential, _) | (Kind::GaussianExponential, false) if term.l == 0.0 => return power,
        (Kind::Exponential, _) | (Kind::GaussianExponential, false) => c(0.0) - delta.powi(term.l as i32),
        (Kind::Gerg2008, true) => {
            let shifted = delta - c(term.epsilon);
            c(0.0) - c(term.eta) * shifted * shifted - c(term.beta) * (delta - c(term.gamma))
        }
        (Kind::GaussianExponential, true) => {
            let shifted = delta - c(term.epsilon);
            c(0.0) - c(term.eta) * shifted * shifted
        }
    };
    power * exponent.exp()
}

/// α^r_ij at (τ, δ): Σ (n T(τ)) D(δ), the plain evaluation a jet's value is checked against.
#[cfg(test)]
fn alpha(f: &DepartureData, tau: f64, delta: f64) -> f64 {
    let mut sum = 0.0;
    for (k, term) in f.terms.iter().enumerate() {
        let gaussian = k >= f.power;
        sum += term.n * tau_part(f.kind, gaussian, term, tau) * delta_part(f.kind, gaussian, term, delta);
    }
    sum
}

/// α^r_ij's Taylor coefficients at (τ, δ) to total order `order` ≤ 4: `c[i][j]` is ∂^(i+j)α^r_ij/∂τ^i∂δ^j over i! j!.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Jet {
    order: usize,
    c: [[f64; 5]; 5],
}

impl Jet {
    /// ∂^(i+j)α^r_ij/∂τ^i∂δ^j, or `None` beyond the order kept.
    pub(crate) fn derivative(&self, i: usize, j: usize) -> Option<f64> {
        let c = self.c.get(i).and_then(|row| row.get(j))?;
        (i.saturating_add(j) <= self.order).then(|| c * at(&FACTORIAL, i) * at(&FACTORIAL, j))
    }
}

/// α^r_ij's jet at (τ, δ) to total order `order` (≤ 4), finite at δ = 0 (the virials): each term's two univariate
/// series multiplied out, the value slot summed as [`alpha`] sums it.
pub(crate) fn jet(f: &DepartureData, tau: f64, delta: f64, order: usize) -> Jet {
    let order = order.min(4);
    let mut c = [[0.0; 5]; 5];
    let (t0, d0) = (Series::variable(tau, order), Series::variable(delta, order));
    for (k, term) in f.terms.iter().enumerate() {
        let gaussian = k >= f.power;
        let (t, d) = (tau_part(f.kind, gaussian, term, t0), delta_part(f.kind, gaussian, term, d0));
        for (i, (row, ti)) in c.iter_mut().zip(t.c).enumerate().take(order.saturating_add(1)) {
            let nt = term.n * ti;
            for (cell, dj) in row.iter_mut().zip(d.c).take(order.saturating_add(1).saturating_sub(i)) {
                *cell += nt * dj;
            }
        }
    }
    Jet { order, c }
}

/// α^r_ij's scaled derivatives A_ij = τ^i δ^j ∂^(i+j)α^r_ij/∂τ^i∂δ^j at (τ, δ), to `order`.
pub(crate) fn derivs(f: &DepartureData, tau: f64, delta: f64, order: Order) -> Derivs {
    let jet = jet(f, tau, delta, order.get());
    Derivs::from_fn(order, |i, j| {
        jet.derivative(i, j).unwrap_or(f64::NAN) * math::powi(tau, i as i32) * math::powi(delta, j as i32)
    })
}

#[cfg(test)]
mod tests {
    use num_dual::{DualNum, HyperDual};
    use phasekit_core::{Order, math};

    use super::*;
    use crate::generated::DEPARTURES;

    /// Four ε directions (outer: 1 and 2, inner: 3 and 4): one evaluation gives one mixed derivative up to order 4.
    type D4 = HyperDual<HyperDual<f64>>;

    fn var(x: f64, seeds: [bool; 4]) -> D4 {
        let one = |seeded: bool| if seeded { 1.0 } else { 0.0 };
        let constant = |v: f64| HyperDual::new(v, 0.0, 0.0, 0.0);
        let inner = HyperDual::new(x, one(seeds[2]), one(seeds[3]), 0.0);
        HyperDual::new(inner, constant(one(seeds[0])), constant(one(seeds[1])), constant(0.0))
    }

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

    /// A departure function's terms, written again on num-dual from the formulas of `ExcessHEFunction.h`.
    fn reference(f: &DepartureData, tau: &D4, delta: &D4) -> D4 {
        let mut sum = D4::from(0.0);
        for (k, t) in f.terms.iter().enumerate() {
            let power = delta.powi(t.d as i32) * tau.powf(t.t) * t.n;
            let exponent = match (f.kind, k >= f.power) {
                (Kind::Gerg2008, false) => D4::from(0.0),
                (Kind::Exponential, _) | (Kind::GaussianExponential, false) => {
                    if t.l == 0.0 {
                        D4::from(0.0)
                    } else {
                        -delta.powi(t.l as i32)
                    }
                }
                (Kind::Gerg2008, true) => -((*delta - t.epsilon).powi(2) * t.eta) - (*delta - t.gamma) * t.beta,
                (Kind::GaussianExponential, true) => {
                    -((*delta - t.epsilon).powi(2) * t.eta) - (*tau - t.gamma).powi(2) * t.beta
                }
            };
            sum += power * exponent.exp();
        }
        sum
    }

    /// ROT-218's pattern: every departure function's A_ij to order 4, from the one kernel on `Jet4`, against num-dual at
    /// three states, within 1e-12 of the larger of |A_ij| and 1.
    #[test]
    fn departure_kinds_match_ad() {
        let seeds = |lo: usize, hi: usize| [0, 1, 2, 3].map(|k| (lo..hi).contains(&k));
        for f in &DEPARTURES {
            for (tau, delta) in [(0.8, 0.8), (1.25, 1.6), (0.5, 0.2)] {
                let d = derivs(f, tau, delta, Order::Four);
                for (i, j) in (0..=4).flat_map(|n| (0..=n).map(move |i| (i, n - i))) {
                    let ad = reference(f, &var(tau, seeds(0, i)), &var(delta, seeds(i, i + j)));
                    let want = part(&ad, i + j) * math::powi(tau, i as i32) * math::powi(delta, j as i32);
                    let got = d.get(i, j).unwrap();
                    assert!(
                        (got - want).abs() <= 1e-12 * want.abs().max(1.0),
                        "{} A{i}{j}: {got} against {want}",
                        f.name
                    );
                }
            }
        }
    }

    /// The f64 kernel is the jet's value, bit for bit, at every order, so α^r_ij evaluated alone costs no derivatives.
    #[test]
    fn the_value_is_the_jet_s() {
        for f in &DEPARTURES {
            for order in 0..=4 {
                let value = jet(f, 0.9, 1.1, order).derivative(0, 0).unwrap();
                assert_eq!(alpha(f, 0.9, 1.1).to_bits(), value.to_bits(), "{} at order {order}", f.name);
            }
        }
    }

    /// A shorter jet's derivatives are the longer one's, bit for bit.
    #[test]
    fn truncation_keeps_the_lower_orders() {
        for f in &DEPARTURES {
            let full = jet(f, 0.8, 0.7, 4);
            for order in 1..4 {
                let short = jet(f, 0.8, 0.7, order);
                for (i, j) in (0..=order).flat_map(|n| (0..=n).map(move |i| (i, n - i))) {
                    let (a, b) = (short.derivative(i, j).unwrap(), full.derivative(i, j).unwrap());
                    assert_eq!(a.to_bits(), b.to_bits(), "{} ({i}, {j}) at order {order}", f.name);
                }
                assert_eq!(short.derivative(order + 1, 0), None);
            }
        }
    }
}
