//! The derivative currency between model families (D2, D3): reducing-invariant scaled derivatives.

use core::ops::Add;

use crate::num::{Lanes, Real};

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

/// Triangular index of `(i, j)`, ordered by total order, then by `i`. The arithmetic saturates (PLAN.md M24.5): an index
/// past the slots stays past them, where `get` finds nothing. In the unrolled kernels it folds to constants.
pub(crate) const fn idx(i: usize, j: usize) -> usize {
    let n = i.saturating_add(j);
    (n.saturating_mul(n.saturating_add(1)) / 2).saturating_add(i)
}

/// The slots of the order-2 entries and of `A_0j`, j = 1..4, as constants, so that indexing them is checked when built.
const A00: usize = idx(0, 0);
const A10: usize = idx(1, 0);
const A01: usize = idx(0, 1);
const A20: usize = idx(2, 0);
const A11: usize = idx(1, 1);
const A02: usize = idx(0, 2);
const A03: usize = idx(0, 3);
const A04: usize = idx(0, 4);

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
                let j = n.saturating_sub(i);
                let value = f(i, j);
                if let Some(slot) = d.a.get_mut(idx(i, j)) {
                    *slot = value;
                }
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
        if i.saturating_add(j) <= self.order.get() { self.a.get(idx(i, j)).copied() } else { None }
    }

    /// `A_ij += φ · bt[i] · bd[j]` for `i + j ≤ ORD`: the outer product of separable τ- and δ-factors, each addition
    /// [`Real::mad`]'s (fused with the `fast` feature).
    #[inline(always)]
    #[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
    pub(crate) fn add_outer<const ORD: usize>(&mut self, phi: R, bt: &[f64; 5], bd: &[R; 5]) {
        for (i, &bti) in bt.iter().enumerate().take(ORD.saturating_add(1)) {
            let e = phi * bti;
            for (j, &bdj) in bd.iter().enumerate().take(ORD.saturating_add(1).saturating_sub(i)) {
                if let Some(a) = self.a.get_mut(idx(i, j)) {
                    *a = e.mad(bdj, *a);
                }
            }
        }
    }
    /// `A_ij += φ · bt[i] · bd[j]` for `i + j ≤ ORD`, with state-dependent τ-factors (terms with an exponential
    /// on the τ-side too).
    #[inline(always)]
    #[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
    pub(crate) fn add_separable<const ORD: usize>(&mut self, phi: R, bt: &[R; 5], bd: &[R; 5]) {
        for (i, &bti) in bt.iter().enumerate().take(ORD.saturating_add(1)) {
            let e = phi * bti;
            for (j, &bdj) in bd.iter().enumerate().take(ORD.saturating_add(1).saturating_sub(i)) {
                if let Some(a) = self.a.get_mut(idx(i, j)) {
                    *a = e.mad(bdj, *a);
                }
            }
        }
    }
}

impl<const W: usize> Derivs<Lanes<W>> {
    /// Lane `k` as a bundle of its own, or `None` past the last lane.
    pub(crate) fn lane(&self, k: usize) -> Option<Derivs> {
        let mut a = [0.0; SLOTS];
        for (slot, lanes) in a.iter_mut().zip(&self.a) {
            *slot = *lanes.0.get(k)?;
        }
        Some(Derivs { a, order: self.order })
    }

    /// Writes `d` into lane `k`; past the last lane it does nothing.
    pub(crate) fn set_lane(&mut self, k: usize, d: &Derivs) {
        for (lanes, v) in self.a.iter_mut().zip(d.a) {
            if let Some(slot) = lanes.0.get_mut(k) {
                *slot = v;
            }
        }
    }

    /// One bundle per lane.
    pub(crate) fn split(&self) -> [Derivs; W] {
        core::array::from_fn(|k| self.lane(k).unwrap_or(Derivs::zero(self.order)))
    }
}

impl Derivs<f64> {
    /// The ρ-part every ideal-gas α⁰ shares: `ln δ` contributes `A_0j = (−1)^(j−1)(j−1)!` and nothing else.
    /// `residual + IDEAL_DELTA` therefore gives the exact total `A_0j` (j ≥ 1), so density solves (p, dp/dρ)
    /// never evaluate α⁰. `A00` and every `A_ij` with `i ≥ 1` of the sum are NOT the total ones.
    pub const IDEAL_DELTA: Derivs = {
        let mut a = [0.0; SLOTS];
        a[A01] = 1.0;
        a[A02] = -1.0;
        a[A03] = 2.0;
        a[A04] = -6.0;
        Derivs { a, order: Order::Four }
    };

    /// The order-2 slice a `State` stores, or `None` if this bundle stops at order 1.
    pub fn bundle(&self) -> Option<Bundle> {
        (self.order >= Order::Two).then(|| Bundle {
            a00: self.a[A00],
            a10: self.a[A10],
            a01: self.a[A01],
            a20: self.a[A20],
            a11: self.a[A11],
            a02: self.a[A02],
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

/// Exact zero-density virial coefficients and their first two T-derivatives at one temperature (E4), molar SI:
/// `Z = 1 + Bρ + Cρ² + …`.
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
    /// d²B/dT², m³/(mol K²): humid air's c_p, c_v and w take it (PLAN.md M14.1, user decision HA6).
    pub d2b_dt2: f64,
    /// d²C/dT², m⁶/(mol² K²).
    pub d2c_dt2: f64,
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
