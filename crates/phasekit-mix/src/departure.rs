//! A binary pair's departure function α^r_ij(τ, δ) in one kernel on [`Series`] over any `Real` (its τ, δ derivatives;
//! ROT-218's pattern; f64, or core's lanes through `batch::run_kernel`, PLAN.md M22.3), its terms as CoolProp 8.0.0 evaluates them (`ExcessHEFunction.h:104-196`,
//! `Helmholtz.h:377-457`; PLAN.md M13.3):
//! - Exponential: Σ n δ^d τ^t exp(−δ^l), without the exponential where l = 0;
//! - GERG-2008: `power` terms n δ^d τ^t, then n δ^d τ^t exp(−η(δ − ε)² − β(δ − γ)): β multiplies δ − γ, not its square;
//! - Gaussian + Exponential: `power` terms n δ^d τ^t exp(−δ^l), then n δ^d τ^t exp(−η(δ − ε)² − β(τ − γ)²).
//!
//! Every term is separable, n T(τ) D(δ), so its derivatives are products of two univariate series: the jet to order k
//! costs O(k²) a term, not a bivariate composition's (PLAN.md M13.7: the critical search evaluates it some ten thousand
//! times a mixture).

use core::ops::{Mul, Sub};

use phasekit_core::batch::RealKernel;
use phasekit_core::{Derivs, Order, Real};

use crate::data::{DepartureData, Kind, Term};
use crate::slot::at;

/// n! for n ≤ 4.
const FACTORIAL: [f64; 5] = [1.0, 1.0, 2.0, 6.0, 24.0];

/// A series' last slot: its coefficients up to the fourth order.
const LAST: usize = 4;

/// A univariate Taylor series truncated at `order` ≤ 4: `c[k]` is the k-th derivative over k!. Its value slot is
/// computed as the plain kernel computes it, so a jet's value is the plain evaluation's, bit for bit. `variable` marks
/// the variable itself, whose composition takes the derivatives directly: `Real` has no comparisons to find it by its
/// coefficients (PLAN.md M22.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Series<R> {
    order: usize,
    c: [R; 5],
    variable: bool,
}

impl<R: Real> Series<R> {
    /// The variable at `x`.
    #[inline(always)]
    fn variable(x: R, order: usize) -> Series<R> {
        let mut c = [R::from_f64(0.0); 5];
        (c[0], c[1]) = (x, R::from_f64(if order >= 1 { 1.0 } else { 0.0 }));
        Series { order, c, variable: true }
    }

    /// The constant `x`, of order 0: an operand of any order raises it.
    #[inline(always)]
    fn constant(x: R) -> Series<R> {
        let mut c = [R::from_f64(0.0); 5];
        c[0] = x;
        Series { order: 0, c, variable: false }
    }

    /// f(self) from f's derivatives `d` at the value: Σ_n d_n/n! εⁿ by Horner, ε = self − value; for the variable itself
    /// (ε the unit monomial) the coefficients d_n/n! directly, which is what Horner's products with it give exactly.
    #[inline(always)]
    #[allow(clippy::arithmetic_side_effects, reason = "Series products: IEEE arithmetic on truncated series, no panic")]
    fn compose(self, d: [R; 5]) -> Series<R> {
        if self.variable {
            let mut c = [R::from_f64(0.0); 5];
            for ((c, d), f) in c.iter_mut().zip(&d).zip(&FACTORIAL).take(self.order.saturating_add(1)) {
                *c = *d / R::from_f64(*f);
            }
            return Series { order: self.order, c, variable: false };
        }
        let mut eps = self;
        eps.c[0] = R::from_f64(0.0);
        let last = |d: &[R; 5]| d.get(self.order).copied().unwrap_or(R::from_f64(f64::NAN));
        let mut r = Series::constant(last(&d) / R::from_f64(at(&FACTORIAL, self.order))).with_order(self.order);
        let below = self.order.min(d.len());
        for (d, f) in d.get(..below).unwrap_or_default().iter().zip(FACTORIAL.get(..below).unwrap_or_default()).rev() {
            r = r * eps;
            r.c[0] = *d / R::from_f64(*f);
        }
        r.c[0] = d[0];
        r
    }

    #[inline(always)]
    fn with_order(mut self, order: usize) -> Series<R> {
        self.order = order;
        self
    }

    /// The truncated product; the value slot is the values' product alone. A constant's product is a scale, the general
    /// sum's terms with its zero slots being exact zeros. Each slot sums its terms in index order, from 0. Plain index
    /// loops, up to the order or the last slot if that is lower, so every index lies in the five slots: an iterator
    /// chain here cost the GERG gas's α^r a fifth more instructions (PLAN.md M24.4).
    #[inline(always)]
    #[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects, reason = "k ≤ last ≤ 4 and a ≤ k: in the slots")]
    fn product(&self, other: &Series<R>) -> Series<R> {
        let order = self.order.max(other.order);
        let last = order.min(LAST);
        let mut c = [R::from_f64(0.0); 5];
        c[0] = self.c[0] * other.c[0];
        // Equivalent (M13.7): the fast path gives the general path's bits (`truncation_keeps_the_lower_orders`,
        // `the_value_is_the_jet_s`), so taking the general path instead changes nothing.
        // equivalent mutant: replace || with &&
        if self.order == 0 || other.order == 0 {
            let (series, k0) = if self.order == 0 { (other, self.c[0]) } else { (self, other.c[0]) };
            let mut k = 1;
            while k <= last {
                c[k] = k0 * series.c[k];
                k += 1;
            }
            return Series { order, c, variable: false };
        }
        let mut k = 1;
        while k <= last {
            let (mut sum, mut a) = (R::from_f64(0.0), 0);
            while a <= k {
                sum = sum + self.c[a] * other.c[k - a];
                a += 1;
            }
            c[k] = sum;
            k += 1;
        }
        Series { order, c, variable: false }
    }
}

impl<R: Real> Sub for Series<R> {
    type Output = Series<R>;
    #[inline(always)]
    #[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
    fn sub(mut self, other: Series<R>) -> Series<R> {
        self.order = self.order.max(other.order);
        self.variable = false;
        for (x, y) in self.c.iter_mut().zip(other.c) {
            *x = *x - y;
        }
        self
    }
}

impl<R: Real> Mul for Series<R> {
    type Output = Series<R>;
    #[inline(always)]
    fn mul(self, other: Series<R>) -> Series<R> {
        self.product(&other)
    }
}

impl<R: Real> Series<R> {
    /// The constant `x`, of order 0.
    #[inline(always)]
    fn from_f64(x: f64) -> Series<R> {
        Series::constant(R::from_f64(x))
    }

    /// x^n's derivatives n (n − 1) ... (n − k + 1) x^(n − k), 0 where the falling factorial is (x = 0 at δ = 0).
    #[inline(always)]
    #[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
    fn powi(self, n: i32) -> Series<R> {
        let mut d = [R::from_f64(0.0); 5];
        let mut falling = 1.0;
        for (k, slot) in d.iter_mut().enumerate().take(self.order.saturating_add(1)) {
            *slot =
                if falling == 0.0 { R::from_f64(0.0) } else { self.c[0].powi(n.saturating_sub(k as i32)) * falling };
            falling *= f64::from(n) - k as f64;
        }
        self.compose(d)
    }

    #[inline(always)]
    #[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
    fn powf(self, t: f64) -> Series<R> {
        let mut d = [R::from_f64(0.0); 5];
        let mut falling = 1.0;
        for (k, slot) in d.iter_mut().enumerate().take(self.order.saturating_add(1)) {
            *slot = self.c[0].powf(t - k as f64) * falling;
            falling *= t - k as f64;
        }
        self.compose(d)
    }

    #[inline(always)]
    fn exp(self) -> Series<R> {
        self.compose([self.c[0].exp(); 5])
    }
}

/// A term's τ part: τ^t, times exp(−β(τ − γ)²) for a Gaussian + Exponential function's Gaussian terms.
#[inline(always)]
#[allow(clippy::arithmetic_side_effects, reason = "Series arithmetic on any Real: IEEE or lane arithmetic, no panic")]
fn tau_part<R: Real>(kind: Kind, gaussian: bool, term: &Term, tau: Series<R>) -> Series<R> {
    let power = tau.powf(term.t);
    match (kind, gaussian) {
        (Kind::GaussianExponential, true) => {
            let t = tau - Series::from_f64(term.gamma);
            power * (Series::from_f64(0.0) - Series::from_f64(term.beta) * t * t).exp()
        }
        _ => power,
    }
}

/// A term's δ part: δ^d times its exponential in δ, if it has one.
#[inline(always)]
#[allow(clippy::arithmetic_side_effects, reason = "Series arithmetic on any Real: IEEE or lane arithmetic, no panic")]
fn delta_part<R: Real>(kind: Kind, gaussian: bool, term: &Term, delta: Series<R>) -> Series<R> {
    let power = delta.powi(term.d as i32);
    let c = Series::from_f64;
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

/// α^r_ij at (τ, δ): Σ (n T(τ)) D(δ), the plain evaluation a jet's value is checked against, on series of order 0
/// (which carry the values alone).
#[cfg(test)]
fn alpha(f: &DepartureData, tau: f64, delta: f64) -> f64 {
    let (tau, delta) = (Series::variable(tau, 0), Series::variable(delta, 0));
    let mut sum = 0.0;
    for (k, term) in f.terms.iter().enumerate() {
        let gaussian = k >= f.power;
        sum += term.n * tau_part(f.kind, gaussian, term, tau).c[0] * delta_part(f.kind, gaussian, term, delta).c[0];
    }
    sum
}

/// α^r_ij's Taylor coefficients at (τ, δ) to total order `order` ≤ 4: `c[i][j]` is ∂^(i+j)α^r_ij/∂τ^i∂δ^j over i! j!.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Jet<R = f64> {
    order: usize,
    c: [[R; 5]; 5],
}

impl<R: Real> Jet<R> {
    /// ∂^(i+j)α^r_ij/∂τ^i∂δ^j, or `None` beyond the order kept.
    #[inline(always)]
    #[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
    pub(crate) fn derivative(&self, i: usize, j: usize) -> Option<R> {
        let c = self.c.get(i).and_then(|row| row.get(j))?;
        (i.saturating_add(j) <= self.order).then(|| *c * at(&FACTORIAL, i) * at(&FACTORIAL, j))
    }
}

/// α^r_ij's jet at (τ, δ) to total order `order` (≤ 4), finite at δ = 0 (the virials).
pub(crate) fn jet(f: &DepartureData, tau: f64, delta: f64, order: usize) -> Jet {
    jet_on(f, tau, delta, order)
}

/// [`jet`] on any `Real`: each term's two univariate series multiplied out, the value slot summed as [`alpha`] sums it.
#[inline(always)]
#[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
fn jet_on<R: Real>(f: &DepartureData, tau: R, delta: R, order: usize) -> Jet<R> {
    let order = order.min(4);
    let mut c = [[R::from_f64(0.0); 5]; 5];
    let (t0, d0) = (Series::variable(tau, order), Series::variable(delta, order));
    for (k, term) in f.terms.iter().enumerate() {
        let gaussian = k >= f.power;
        let (t, d) = (tau_part(f.kind, gaussian, term, t0), delta_part(f.kind, gaussian, term, d0));
        for (i, (row, ti)) in c.iter_mut().zip(t.c).enumerate().take(order.saturating_add(1)) {
            let nt = ti * term.n;
            for (cell, dj) in row.iter_mut().zip(d.c).take(order.saturating_add(1).saturating_sub(i)) {
                *cell = *cell + nt * dj;
            }
        }
    }
    Jet { order, c }
}

/// α^r_ij's scaled derivatives A_ij = τ^i δ^j ∂^(i+j)α^r_ij/∂τ^i∂δ^j at (τ, δ), to `order`.
pub(crate) fn derivs(f: &DepartureData, tau: f64, delta: f64, order: Order) -> Derivs {
    derivs_on(f, tau, delta, order)
}

/// [`derivs`] on any `Real`.
#[inline(always)]
#[allow(clippy::arithmetic_side_effects, reason = "on any Real: IEEE or lane arithmetic, no panic")]
fn derivs_on<R: Real>(f: &DepartureData, tau: R, delta: R, order: Order) -> Derivs<R> {
    let jet = jet_on(f, tau, delta, order.get());
    Derivs::from_fn(order, |i, j| {
        jet.derivative(i, j).unwrap_or(R::from_f64(f64::NAN)) * tau.powi(i as i32) * delta.powi(j as i32)
    })
}

/// Departure functions as one kernel over `Real` for core's runner (PLAN.md M22.3; user decision ML3): (τ, δ) in, and
/// out each function's A_ij to `order`, in [`Derivs::from_fn`]'s order (`slots(order)` of them a function).
pub(crate) struct Departures<'a> {
    pub(crate) functions: Vec<&'a DepartureData>,
    pub(crate) order: Order,
}

/// The A_ij of a bundle to `order`: (order + 1)(order + 2)/2.
pub(crate) fn slots(order: Order) -> usize {
    let n = order.get().saturating_add(1);
    n.saturating_mul(n.saturating_add(1)) / 2
}

/// Each A_ij of `d` with i + j ≤ `order`, in [`Derivs::from_fn`]'s order.
#[inline(always)]
fn entries<R: Real>(d: &Derivs<R>, order: Order) -> impl Iterator<Item = R> + '_ {
    (0..=order.get())
        .flat_map(|n| (0..=n).map(move |i| (i, n.saturating_sub(i))))
        .map(|(i, j)| d.get(i, j).unwrap_or(R::from_f64(f64::NAN)))
}

impl RealKernel for Departures<'_> {
    #[inline(always)]
    fn eval<R: Real>(&self, inputs: &[R], outputs: &mut [R]) {
        let nan = R::from_f64(f64::NAN);
        let (tau, delta) = (inputs.first().copied().unwrap_or(nan), inputs.get(1).copied().unwrap_or(nan));
        for (f, out) in self.functions.iter().zip(outputs.chunks_mut(slots(self.order))) {
            let d = derivs_on(f, tau, delta, self.order);
            for (o, v) in out.iter_mut().zip(entries(&d, self.order)) {
                *o = v;
            }
        }
    }
}

/// The bundle of function `f` (its index) at point `k` from [`Departures`]' output columns, as [`derivs`] builds it.
pub(crate) fn from_columns(columns: &[Vec<f64>], f: usize, k: usize, order: Order) -> Derivs {
    let first = f.saturating_mul(slots(order));
    let mut values = columns.iter().skip(first).map(|c| c.get(k).copied().unwrap_or(f64::NAN));
    Derivs::from_fn(order, |_, _| values.next().unwrap_or(f64::NAN))
}

#[cfg(test)]
mod tests {
    use num_dual::{DualNum, HyperDual};
    use phasekit_core::batch::{ExecPolicy, run_kernel};
    use phasekit_core::{HelmholtzModel, Order, math};

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

    /// τ and δ for [`departure_lanes_are_scalar_bitwise`]: 65 points of τ in [0.5, 2] and δ in [0, 3], δ → 0 among them
    /// (0, 1e-300, 1e-12 and 1e-6 lead).
    fn lane_states() -> (Vec<f64>, Vec<f64>) {
        let tau = (0..65).map(|k| 0.5 + 1.5 * f64::from(k * 37 % 65) / 64.0).collect();
        let small = [0.0, 1e-300, 1e-12, 1e-6];
        let delta = (0..65).map(|k| small.get(k).copied().unwrap_or(3.0 * (k * 29 % 65) as f64 / 64.0)).collect();
        (tau, delta)
    }

    /// Every entry's bits, NaN canonical.
    fn bits(d: &Derivs) -> Vec<Option<u64>> {
        let canonical = |v: f64| if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() };
        (0..=4).flat_map(|n| (0..=n).map(move |i| (i, n - i))).map(|(i, j)| d.get(i, j).map(canonical)).collect()
    }

    /// `functions` as one [`Departures`] kernel through core's runner under `Simd` at the states, against [`derivs`]
    /// point by point, bit for bit; returns the bundles compared.
    fn assert_lanes_are_scalar(functions: &[&DepartureData], order: Order, (tau, delta): (&[f64], &[f64])) -> usize {
        let kernel = Departures { functions: functions.to_vec(), order };
        let mut columns = vec![vec![f64::NAN; tau.len()]; functions.len() * slots(order)];
        let mut outputs: Vec<&mut [f64]> = columns.iter_mut().map(Vec::as_mut_slice).collect();
        let ran = run_kernel(&kernel, &[tau, delta], &mut outputs, ExecPolicy::Simd);
        assert!(ran.is_ok(), "{order:?}, {} points: {ran:?}", tau.len());
        for (f, function) in functions.iter().enumerate() {
            for (k, (t, d)) in tau.iter().zip(delta).enumerate() {
                let (got, want) = (from_columns(&columns, f, k, order), derivs(function, *t, *d, order));
                assert_eq!(
                    bits(&got),
                    bits(&want),
                    "{} {order:?} at τ {t}, δ {d}, {} points",
                    function.name,
                    tau.len()
                );
            }
        }
        functions.len() * tau.len()
    }

    /// PLAN.md M22.3 (K17; user decision ML3): every library and GERG-2008 departure function, as one kernel each set,
    /// through core's runner under `Simd` at every order over batches of 1 to 65 points (so a short last group at every
    /// width), is its scalar kernel point by point, bit for bit, δ → 0 included; and so is each GERG-2008 component's
    /// `residual_batch`, the same kernel on its own equation.
    #[test]
    fn departure_lanes_are_scalar_bitwise() {
        let (tau, delta) = lane_states();
        let library: Vec<&DepartureData> = DEPARTURES.iter().collect();
        let gerg: Vec<&DepartureData> = crate::gerg::DEPARTURES.iter().map(|(_, f)| f).collect();
        let mut compared = 0;
        for order in [Order::One, Order::Two, Order::Three, Order::Four] {
            for n in 1..=65 {
                compared += assert_lanes_are_scalar(&library, order, (&tau[..n], &delta[..n]));
                compared += assert_lanes_are_scalar(&gerg, order, (&tau[..n], &delta[..n]));
            }
        }
        // Every function at every order, over batches of 1 to 65 points (2145 points in all).
        assert_eq!(compared, (library.len() + gerg.len()) * 4 * 2145);
        assert_eq!(assert_gerg_components_are_scalar(&tau, &delta), 21 * 4 * 65);
    }

    /// Each GERG-2008 component's `residual_batch` under `Simd` at every order, at (T_c/τ, δ ρ_c), against its
    /// `residual` point by point, bit for bit; returns the bundles compared.
    fn assert_gerg_components_are_scalar(tau: &[f64], delta: &[f64]) -> usize {
        let mut compared = 0;
        let registry = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
        let bound = registry.get("Methane").unwrap();
        for c in &crate::gerg::COMPONENTS {
            let eos = crate::gerg::Pure::new(c, bound.clone());
            let reducing = eos.reducing().unwrap();
            let t: Vec<f64> = tau.iter().map(|tau| reducing.t / tau).collect();
            let rho: Vec<f64> = delta.iter().map(|delta| delta * reducing.rho).collect();
            for order in [Order::One, Order::Two, Order::Three, Order::Four] {
                let mut out = vec![Derivs::zero(order); t.len()];
                eos.residual_batch(&t, &rho, order, &mut out, ExecPolicy::Simd).unwrap();
                for (k, got) in out.iter().enumerate() {
                    let want = eos.residual(t[k], rho[k], order);
                    assert_eq!(bits(got), bits(&want), "{} {order:?} at T {}, ρ {}", c.name, t[k], rho[k]);
                }
                compared += out.len();
            }
        }
        compared
    }

    /// A bundle to order k has (k + 1)(k + 2)/2 entries, the columns [`Departures`] writes for each function.
    #[test]
    fn slots_count_a_bundle_s_entries() {
        let orders = [Order::One, Order::Two, Order::Three, Order::Four];
        assert_eq!(orders.map(slots), [3, 6, 10, 15]);
        assert!(orders.iter().all(|&o| slots(o) == entries(&Derivs::<f64>::zero(o), o).count()));
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
