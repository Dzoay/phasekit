//! The derivative currency between model families (D2, D3): reducing-invariant scaled derivatives.

use core::ops::Add;

use crate::num::Real;

/// Highest derivative order requested (CoolProp's `Want` mask, map 02 §9). Order 3 is needed for second
/// partial derivatives and the fundamental derivative (map 01, map 02); order 4 for critical points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Order {
    /// A00, A10, A01.
    One = 1,
    /// ... + A20, A11, A02: every first-order property plus cp, cv, w and first partial derivatives.
    Two = 2,
    /// ... + order-3 terms.
    Three = 3,
    /// ... + order-4 terms.
    Four = 4,
}

impl Order {
    /// The order as a number.
    pub const fn get(self) -> usize {
        self as usize
    }
}

/// Number of `(i, j)` slots with `i + j ≤ 4`.
pub(crate) const SLOTS: usize = 15;

/// Triangular index of `(i, j)`, ordered by total order, then by `i`.
pub(crate) const fn idx(i: usize, j: usize) -> usize {
    let n = i + j;
    n * (n + 1) / 2 + i
}

/// `A_ij = τ^i δ^j ∂^(i+j)α / ∂τ^i ∂δ^j` for `i + j ≤ order ≤ 4`. Because `τ∂/∂τ = −T∂/∂T` and
/// `δ∂/∂δ = ρ∂/∂ρ`, the values do not depend on the reducing constants: bundles of different models, or the
/// ideal part of one with the residual part of another, add safely (map 06 C1, §9; materials S1).
/// Generic over the sealed [`Real`] so `Jet4` (and lanes, if ever) accumulate into the same layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Derivs<R = f64> {
    a: [R; SLOTS],
    order: Order,
}

impl<R: Real> Derivs<R> {
    /// All zero, valid up to `order`.
    pub fn zero(order: Order) -> Self {
        Self { a: [R::from_f64(0.0); SLOTS], order }
    }

    /// Builds a bundle from `f(i, j)` for every `i + j ≤ order` (families with closed forms use this).
    pub fn from_fn(order: Order, mut f: impl FnMut(usize, usize) -> R) -> Self {
        let mut d = Self::zero(order);
        for n in 0..=order.get() {
            for i in 0..=n {
                d.a[idx(i, n - i)] = f(i, n - i);
            }
        }
        d
    }

    /// The order this bundle was computed to.
    pub const fn order(&self) -> Order {
        self.order
    }

    /// `A_ij`, or `None` above the computed order.
    pub fn get(&self, i: usize, j: usize) -> Option<R> {
        (i + j <= self.order.get()).then(|| self.a[idx(i, j)])
    }

    /// `A_ij += φ · bt[i] · bd[j]` for `i + j ≤ ORD`: the outer product of separable τ- and δ-factors.
    pub(crate) fn add_outer<const ORD: usize>(&mut self, phi: R, bt: &[f64; 5], bd: &[R; 5]) {
        for (i, &bti) in bt.iter().enumerate().take(ORD + 1) {
            let e = phi * bti;
            for (j, &bdj) in bd.iter().enumerate().take(ORD + 1 - i) {
                let k = idx(i, j);
                self.a[k] = self.a[k] + e * bdj;
            }
        }
    }
}

impl Derivs<f64> {
    /// The ρ-part every ideal-gas α⁰ shares: `ln δ` contributes `A_0j = (−1)^(j−1)(j−1)!` and nothing else.
    /// `residual + IDEAL_DELTA` therefore gives the exact total `A_0j` (j ≥ 1), so density solves (p, dp/dρ)
    /// never evaluate α⁰. `A00` and every `A_ij` with `i ≥ 1` of the sum are NOT the total ones.
    pub const IDEAL_DELTA: Derivs = {
        let mut a = [0.0; SLOTS];
        a[idx(0, 1)] = 1.0;
        a[idx(0, 2)] = -1.0;
        a[idx(0, 3)] = 2.0;
        a[idx(0, 4)] = -6.0;
        Derivs { a, order: Order::Four }
    };

    /// The order-2 slice a `State` stores, or `None` if this bundle stops at order 1.
    pub fn bundle(&self) -> Option<Bundle> {
        (self.order >= Order::Two).then(|| Bundle {
            a00: self.a[idx(0, 0)],
            a10: self.a[idx(1, 0)],
            a01: self.a[idx(0, 1)],
            a20: self.a[idx(2, 0)],
            a11: self.a[idx(1, 1)],
            a02: self.a[idx(0, 2)],
        })
    }
}

/// Sum of two bundles (ideal + residual), valid to the lower of the two orders.
impl Add for Derivs<f64> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let mut a = self.a;
        for (x, y) in a.iter_mut().zip(rhs.a) {
            *x += y;
        }
        Self { a, order: self.order.min(rhs.order) }
    }
}

/// The order-2 TOTAL bundle (ideal + residual) of one phase point: everything first-order properties,
/// cp, cv, w and first partial derivatives need. Plain, family-neutral data: a Gibbs model fills it through
/// an exact Legendre transform ([`crate::bundle_from_gibbs`]).
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)] // A_ij of the total α, as in `Derivs`
pub struct Bundle {
    pub a00: f64,
    pub a10: f64,
    pub a01: f64,
    pub a20: f64,
    pub a11: f64,
    pub a02: f64,
}

/// What a model knows about one phase point beyond the order-2 bundle a `State` stores (E1): any order up
/// to 4, and, for families that separate them, the ideal-gas and residual parts (Cp0, residual h/s/g,
/// CoolProp's α-term outputs, second partial derivatives, the fundamental derivative).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointDerivs {
    total: Derivs,
    parts: Option<(Derivs, Derivs)>,
}

impl PointDerivs {
    /// A Helmholtz family: ideal and residual parts, evaluated at the same (T, ρ).
    pub fn split(ideal: Derivs, residual: Derivs) -> Self {
        Self { total: ideal + residual, parts: Some((ideal, residual)) }
    }
    /// A family with no ideal/residual split (a Gibbs solid: its order-3 Legendre transform).
    pub fn from_total(total: Derivs) -> Self {
        Self { total, parts: None }
    }
    /// The total α bundle.
    pub fn total(&self) -> Derivs {
        self.total
    }
    /// The ideal-gas part α⁰, if the family separates it.
    pub fn ideal(&self) -> Option<Derivs> {
        self.parts.map(|(i, _)| i)
    }
    /// The residual part α^r, if the family separates it.
    pub fn residual(&self) -> Option<Derivs> {
        self.parts.map(|(_, r)| r)
    }
}

/// Exact zero-density virial coefficients at one temperature (E4), molar SI: `Z = 1 + Bρ + Cρ² + …`.
/// From the δ → 0 Taylor coefficients of α^r, never from a small-δ evaluation (map 12 §6.3, R8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Virials {
    /// Second virial coefficient B, m³/mol.
    pub b: f64,
    /// Third virial coefficient C, m⁶/mol².
    pub c: f64,
    /// dB/dT, m³/(mol K).
    pub db_dt: f64,
    /// dC/dT, m⁶/(mol² K).
    pub dc_dt: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangular_index_is_dense() {
        let mut seen = [false; SLOTS];
        for n in 0..=4 {
            for i in 0..=n {
                seen[idx(i, n - i)] = true;
            }
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn get_refuses_above_order() {
        let d = Derivs::<f64>::zero(Order::Two);
        assert_eq!(d.get(1, 1), Some(0.0));
        assert_eq!(d.get(2, 1), None);
    }
}
