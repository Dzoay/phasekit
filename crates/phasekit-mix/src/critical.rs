//! A mixture's critical points at fixed composition (PLAN.md M13.7; Heidemann and Khalil 1980, as GERG TM15 §7.6
//! formulates them): with Q_ij = ∂²(A/RT)/∂n_i∂n_j at (T, V), the stability limit det Q = 0 and the cubic form
//! C = Σ_ijk ∂³(A/RT)/∂n_i∂n_j∂n_k Δn_i Δn_j Δn_k = 0 along Q's null vector Δn. A/RT = Ψ + Ψ⁰: Ψ's derivatives are
//! M13.6's, Ψ⁰'s in closed form from the mixture's own ideal part (ln ρ + Σ x_i w_i (c_i + ln x_i), w_i = R_i/R):
//! ∂²Ψ⁰/∂n_i∂n_j = (1 − w_i − w_j + w̄)/n + w_i δ_ij/n_i and ∂³Ψ⁰/∂n_i∂n_j∂n_k = (w_i + w_j + w_k − 2w̄ − 1)/n² −
//! w_i δ_ijk/n_i², w̄ = Σ x_k w_k.
//!
//! Every critical point in the model's range is found: the stability limit is located on a grid
//! in (ln T, ln ρ), the cubic form's sign is sampled where the limit crosses the grid's edges, and each sign change is
//! polished to the conditions' root. The null vector comes from complete pivoting, not from Q's first N − 1 rows,
//! which v8.0.0's det M* relies on: where those rows are singular among themselves det M* vanishes with them and
//! v8.0.0 reports a critical point that is not one (DIV-0056). CoolProp computes them on every call, throws for
//! ordinary binaries and reports garbage points as stable (ROT-187); here they are found once, in the immutable
//! mixture.

use phasekit_core::{Error, Order, math};

use crate::slot::{at, at2, set};
use crate::{Composition, MixtureModel, MultiFluid, Var};

/// A critical point at the mixture's composition: T (K), ρ (mol/m³) and p (Pa).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CriticalState {
    /// Temperature, K.
    pub t: f64,
    /// Molar density, mol/m³.
    pub rho: f64,
    /// Pressure, Pa (negative on a liquid branch of the stability limit).
    pub p: f64,
}

/// Heidemann and Khalil's conditions at a state, each with the scale `Identity` bounds it on (VERIFICATION.md §5): the
/// largest term of its sum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Criticality {
    /// det(D Q D), D = diag(√x_i): 1 for an ideal gas.
    pub det: f64,
    /// Hadamard's bound on |det(D Q D)|, the product of its rows' Euclidean norms.
    pub det_scale: f64,
    /// The cubic form along Q's D-normalised null vector.
    pub cubic: f64,
    /// The sum of its terms' magnitudes.
    pub cubic_scale: f64,
}

/// Q and the third-derivative tensor of A/RT at one mole of composition `x`, from `c`, with ideal weights `w`.
struct Conditions {
    q: Vec<Vec<f64>>,
    c: Option<Composition>,
    w: Vec<f64>,
    x: Vec<f64>,
}

impl Conditions {
    fn new(c: Composition, x: &[f64], w: &[f64]) -> Conditions {
        let n = x.len();
        let w_bar: f64 = x.iter().zip(w).map(|(x, w)| x * w).sum();
        let mut q = vec![vec![0.0; n]; n];
        for (i, row) in q.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let ideal = 1.0 - at(w, i) - at(w, j) + w_bar + if i == j { at(w, i) / at(x, i) } else { 0.0 };
                *cell = c.psi(&[Var::N(i), Var::N(j)]) + ideal;
            }
        }
        Conditions { q, c: Some(c), w: w.to_vec(), x: x.to_vec() }
    }

    /// D Q D with D = diag(√x_i): the identity for an ideal gas, so the stability limit's scale does not depend on x.
    fn scaled(&self) -> Vec<Vec<f64>> {
        self.q
            .iter()
            .enumerate()
            .map(|(i, row)| {
                row.iter().enumerate().map(|(j, q)| q * math::sqrt(at(&self.x, i) * at(&self.x, j))).collect()
            })
            .collect()
    }

    /// det(D Q D).
    fn det(&self) -> f64 {
        determinant(self.scaled())
    }

    /// Hadamard's bound on |det(D Q D)|.
    fn det_scale(&self) -> f64 {
        self.scaled().iter().map(|row| math::sqrt(row.iter().map(|q| q * q).sum())).product()
    }

    /// The null vector Δn of D Q D's nearest singular matrix, mapped back by D and D-normalised (Σ Δn_i²/x_i = 1): by
    /// elimination with complete pivoting, the column of the smallest pivot left free. Taking the last component as
    /// free instead fails wherever Q's first N − 1 rows are singular among themselves (DIV-0056). Its sign is
    /// arbitrary: the cubic form is odd in it, so `oriented` compares two of them.
    fn null_vector(&self) -> Option<Vec<f64>> {
        let n = self.x.len();
        let mut a = self.scaled();
        let mut cols: Vec<usize> = (0..n).collect();
        for k in 0..n.saturating_sub(1) {
            let (r, c) = largest(&a, k);
            if at2(&a, r, c) == 0.0 {
                return None;
            }
            a.swap(r, k);
            for row in a.iter_mut() {
                row.swap(c, k);
            }
            cols.swap(c, k);
            eliminate(&mut a, k);
        }
        // Back-substitution of the first N − 1 pivots' rows with the free column's entry 1.
        let mut y = vec![0.0; n];
        if let Some(free) = y.last_mut() {
            *free = 1.0;
        }
        for r in (0..n.saturating_sub(1)).rev() {
            let row = a.get(r).map_or(&[][..], Vec::as_slice);
            let s: f64 = row.iter().zip(&y).skip(r.saturating_add(1)).map(|(a, y)| a * y).sum();
            set(&mut y, r, -s / at(row, r));
        }
        // y is D Q D's null vector in permuted order; Δn = D y.
        let mut v = vec![0.0; n];
        for (&y, &c) in y.iter().zip(&cols) {
            set(&mut v, c, y * math::sqrt(at(&self.x, c)));
        }
        let norm = math::sqrt(v.iter().zip(&self.x).map(|(v, x)| v * v / x).sum());
        norm.is_finite().then(|| v.iter().map(|v| v / norm).collect())
    }

    /// The cubic form along Δn, for the third-order composition derivatives.
    fn cubic(&self) -> f64 {
        self.cubic_terms().0
    }

    /// The cubic form and the sum of its terms' magnitudes.
    fn cubic_terms(&self) -> (f64, f64) {
        match self.null_vector() {
            Some(dn) => self.cubic_along(&dn),
            None => (f64::NAN, f64::NAN),
        }
    }

    /// The cubic form along `dn` and the sum of its terms' magnitudes.
    fn cubic_along(&self, dn: &[f64]) -> (f64, f64) {
        let Some(c) = &self.c else { return (f64::NAN, f64::NAN) };
        let n = self.x.len();
        let w_bar: f64 = self.x.iter().zip(&self.w).map(|(x, w)| x * w).sum();
        let (mut sum, mut scale) = (0.0, 0.0);
        let triples = (0..n).flat_map(|i| (0..n).flat_map(move |j| (0..n).map(move |k| (i, j, k))));
        for (i, j, k) in triples {
            let (w, x) = (|i| at(&self.w, i), |i| at(&self.x, i));
            let ideal =
                w(i) + w(j) + w(k) - 2.0 * w_bar - 1.0 - if i == j && j == k { w(i) / (x(i) * x(i)) } else { 0.0 };
            let term = (c.psi(&[Var::N(i), Var::N(j), Var::N(k)]) + ideal) * at(dn, i) * at(dn, j) * at(dn, k);
            (sum, scale) = (sum + term, scale + term.abs());
        }
        (sum, scale)
    }
}

/// The row and column of the largest |a_ij| with i, j ≥ k: complete pivoting's choice.
fn largest(a: &[Vec<f64>], k: usize) -> (usize, usize) {
    let n = a.len();
    let mut best = (k, k);
    for i in k..n {
        for j in k..n {
            if at2(a, i, j).abs() > at2(a, best.0, best.1).abs() {
                best = (i, j);
            }
        }
    }
    best
}

/// The determinant of `a` by Gaussian elimination with partial pivoting.
fn determinant(mut a: Vec<Vec<f64>>) -> f64 {
    let n = a.len();
    let mut det = 1.0;
    for col in 0..n {
        let pivot = (col..n).max_by(|&r, &s| at2(&a, r, col).abs().total_cmp(&at2(&a, s, col).abs())).unwrap_or(col);
        if at2(&a, pivot, col) == 0.0 {
            return 0.0;
        }
        if pivot != col {
            a.swap(pivot, col);
            det = -det;
        }
        det *= at2(&a, col, col);
        eliminate(&mut a, col);
    }
    det
}

/// Subtracts row `col` from the rows below it to zero column `col`.
fn eliminate(a: &mut [Vec<f64>], col: usize) {
    let Some((pivot, rest)) = a.get_mut(col..).and_then(<[Vec<f64>]>::split_first_mut) else { return };
    for row in rest.iter_mut() {
        let f = at(row, col) / at(pivot, col);
        for (x, p) in row.iter_mut().zip(pivot.iter()).skip(col) {
            *x -= f * p;
        }
    }
}

/// The grid of the search, in ln T and ln ρ: fine enough that the stability limit crosses a cell's edges at most
/// twice per branch, and that the cubic form's sign is sampled on it about once a cell.
const TEMPERATURES: usize = 100;
const DENSITIES: usize = 56;

/// A point on the stability limit, with the cubic form there along its null vector.
#[derive(Clone)]
struct OnLimit {
    t: f64,
    rho: f64,
    cubic: f64,
    dn: Vec<f64>,
}

/// How often a cell is split in four, at most, where the cubic form keeps its sign between two of the limit's crossings
/// but falls to under half its size: two roots between them, as for Nitrogen/Methane's at 142.8 K.
const SPLITS: u32 = 3;

/// A cell of the search in (ln T, ln ρ), with det at its corners (u0 v0, u1 v0, u0 v1, u1 v1).
#[derive(Clone, Copy)]
struct Cell {
    u: (f64, f64),
    v: (f64, f64),
    det: [f64; 4],
}

impl Cell {
    /// Its edges, as pairs of corner indices.
    const EDGES: [(usize, usize); 4] = [(0, 1), (2, 3), (0, 2), (1, 3)];

    fn corner(&self, k: usize) -> (f64, f64) {
        (if k.is_multiple_of(2) { self.u.0 } else { self.u.1 }, if k < 2 { self.v.0 } else { self.v.1 })
    }

    /// The cell's four quarters, det at their new corners from `det`.
    fn quarters(&self, det: &dyn Fn((f64, f64)) -> f64) -> [Cell; 4] {
        let (um, vm) = ((self.u.0 + self.u.1) / 2.0, (self.v.0 + self.v.1) / 2.0);
        let [d00, d10, d01, d11] = self.det;
        let (dm0, d0m, dmm) = (det((um, self.v.0)), det((self.u.0, vm)), det((um, vm)));
        let (d1m, dm1) = (det((self.u.1, vm)), det((um, self.v.1)));
        [
            Cell { u: (self.u.0, um), v: (self.v.0, vm), det: [d00, dm0, d0m, dmm] },
            Cell { u: (um, self.u.1), v: (self.v.0, vm), det: [dm0, d10, dmm, d1m] },
            Cell { u: (self.u.0, um), v: (vm, self.v.1), det: [d0m, dmm, d01, dm1] },
            Cell { u: (um, self.u.1), v: (vm, self.v.1), det: [dmm, d1m, dm1, d11] },
        ]
    }
}

/// Whether two of the cubic form's samples of one sign leave room for two roots between them: the smaller under half
/// the larger.
fn halves(a: f64, b: f64) -> bool {
    a.abs().min(b.abs()) < 0.5 * a.abs().max(b.abs())
}

/// A grid node: its density index and temperature index.
type Node = (usize, usize);

/// The model the search evaluates.
struct Search<'a> {
    model: &'a MultiFluid,
    x: &'a [f64],
    w: &'a [f64],
}

impl Search<'_> {
    fn conditions(&self, t: f64, rho: f64, order: Order) -> Option<Conditions> {
        self.model.composition(t, rho, self.x, order).ok().map(|c| Conditions::new(c, self.x, self.w))
    }

    fn det_at(&self, (u, v): (f64, f64)) -> f64 {
        self.conditions(math::exp(u), math::exp(v), Order::Two).map_or(f64::NAN, |c| c.det())
    }

    /// The D-inner product of two null vectors.
    fn along(&self, a: &[f64], b: &[f64]) -> f64 {
        a.iter().zip(b).zip(self.x).map(|((p, q), x)| p * q / x).sum()
    }

    /// The limit's point on the segment from `p` to `q` in (ln T, ln ρ), where det is `ends`, of opposite signs.
    fn limit_between(&self, p: (f64, f64), q: (f64, f64), ends: (f64, f64)) -> Option<OnLimit> {
        if !(ends.0.is_finite() && ends.1.is_finite()) || ends.0.signum() == ends.1.signum() {
            return None;
        }
        let at = |s: f64| (p.0 + s * (q.0 - p.0), p.1 + s * (q.1 - p.1));
        let (u, v) = at(regula_falsi(&|s| self.det_at(at(s)), ends));
        let (t, rho) = (math::exp(u), math::exp(v));
        let c = self.conditions(t, rho, Order::Three)?;
        let dn = c.null_vector()?;
        let (cubic, _) = c.cubic_along(&dn);
        cubic.is_finite().then_some(OnLimit { t, rho, cubic, dn })
    }

    /// The limit's crossings of `cell`'s edges.
    fn crossings(&self, cell: &Cell) -> Vec<OnLimit> {
        Cell::EDGES
            .iter()
            .filter_map(|&(a, b)| {
                self.limit_between(cell.corner(a), cell.corner(b), (at(&cell.det, a), at(&cell.det, b)))
            })
            .collect()
    }

    /// Newton's method from between `a` and `b`, whose cubic forms differ in sign, its null vectors oriented as a's: the
    /// root where it reaches one.
    fn polish(&self, a: &OnLimit, b: &OnLimit) -> Option<CriticalState> {
        let both = |t: f64, rho: f64| -> [f64; 2] {
            let Some(c) = self.conditions(t, rho, Order::Three) else { return [f64::NAN; 2] };
            let Some(dn) = c.null_vector() else { return [f64::NAN; 2] };
            [c.det(), self.along(&dn, &a.dn).signum() * c.cubic_along(&dn).0]
        };
        let (t, rho) = newton(&both, (math::sqrt(a.t * b.t), math::sqrt(a.rho * b.rho)))?;
        self.critical(t, rho)
    }

    /// (T, ρ) as a critical point where both conditions vanish there to 1e-9 and p is finite.
    fn critical(&self, t: f64, rho: f64) -> Option<CriticalState> {
        let c = self.conditions(t, rho, Order::Three)?;
        let p = c.c.as_ref().map_or(f64::NAN, |c| c.pressure());
        (c.det().abs() <= 1e-9 && c.cubic().abs() <= 1e-9 && p.is_finite()).then_some(CriticalState { t, rho, p })
    }

    /// The roots in `cell`, whose edges the limit crosses at `on`: polished between each pair of crossings whose cubic
    /// forms differ in sign (a root Newton's method reaches outside the cell is kept: both conditions hold there), and
    /// the cell split in four, up to `splits` times, where a pair's keep their sign but one is under half the other.
    fn roots(&self, cell: &Cell, on: &[OnLimit], splits: u32) -> Vec<CriticalState> {
        let mut roots = Vec::new();
        let mut doubtful = false;
        for (k, a) in on.iter().enumerate() {
            for b in on.iter().skip(k.saturating_add(1)) {
                let cb = b.cubic * self.along(&a.dn, &b.dn).signum();
                if a.cubic.signum() != cb.signum() {
                    roots.extend(self.polish(a, b));
                } else {
                    doubtful |= halves(a.cubic, cb);
                }
            }
        }
        if doubtful && splits > 0 {
            for q in &cell.quarters(&|at| self.det_at(at)) {
                roots.extend(self.roots(q, &self.crossings(q), splits.saturating_sub(1)));
            }
        }
        roots
    }
}

/// The critical points of `model` at composition `x` (one per component, summing to 1), with ideal weights `w` (R_i/R)
/// within [t_min, t_max] and below `rho_max`, in increasing density. The stability limit is located on a grid in
/// (ln T, ln ρ), det on every node (order 2), and refined to its crossing of every edge det changes sign on. The cubic
/// form is evaluated there, on the limit (order 3). Where it changes sign between two crossings of one cell, its null
/// vectors oriented alike, Newton's method in (ln T, ln ρ) polishes the cell's root, which is kept only where both
/// conditions vanish to 1e-9.
pub(crate) fn critical_points(
    model: &MultiFluid,
    x: &[f64],
    w: &[f64],
    (t_min, t_max): (f64, f64),
    rho_max: f64,
) -> Result<Vec<CriticalState>, Error> {
    let (_, rho_r) = model.reducing_state(x);
    // No critical point of these mixtures lies above twice their components' highest critical temperature.
    let t_c = model.fluids().filter_map(|f| f.info().published_critical().map(|c| c.t)).fold(0.0, f64::max);
    let search = Search { model, x, w };
    let (ln_t, ln_rho) = (
        log_axis(t_min, t_max.min(2.0 * t_c), TEMPERATURES),
        log_axis(0.02 * rho_r, rho_max.min(6.0 * rho_r), DENSITIES),
    );
    let det: Vec<Vec<f64>> = ln_rho.iter().map(|&v| ln_t.iter().map(|&u| search.det_at((u, v))).collect()).collect();
    // Each grid edge's crossing, by its nodes (density index, temperature index), shared by the cells either side.
    let mut edges: std::collections::HashMap<(Node, Node), Option<OnLimit>> = std::collections::HashMap::new();
    let mut found: Vec<CriticalState> = Vec::new();
    for (i, j) in (1..DENSITIES).flat_map(|i| (1..TEMPERATURES).map(move |j| (i, j))) {
        let (i0, j0) = (i.saturating_sub(1), j.saturating_sub(1));
        let nodes = [(i0, j0), (i0, j), (i, j0), (i, j)];
        let (u, v) = ((at(&ln_t, j0), at(&ln_t, j)), (at(&ln_rho, i0), at(&ln_rho, i)));
        let cell = Cell { u, v, det: nodes.map(|(a, b)| at2(&det, a, b)) };
        let mut crossing = |(a, b): (usize, usize)| {
            let between = || search.limit_between(cell.corner(a), cell.corner(b), (at(&cell.det, a), at(&cell.det, b)));
            let (&na, &nb) = (nodes.get(a)?, nodes.get(b)?);
            edges.entry((na, nb)).or_insert_with(between).clone()
        };
        let on: Vec<OnLimit> = Cell::EDGES.iter().filter_map(|&edge| crossing(edge)).collect();
        for c in search.roots(&cell, &on, SPLITS) {
            if fresh(&c, &found, (t_min, t_max)) {
                found.push(c);
            }
        }
    }
    found.sort_by(|a, b| a.rho.total_cmp(&b.rho));
    Ok(found)
}

/// Whether `c` lies in [t_min, t_max] and is none of `found`: no point there within 1e-8 of it in both T and ρ.
fn fresh(c: &CriticalState, found: &[CriticalState], (t_min, t_max): (f64, f64)) -> bool {
    let known = found.iter().any(|f| (f.t / c.t - 1.0).abs() < 1e-8 && (f.rho / c.rho - 1.0).abs() < 1e-8);
    !known && (t_min..=t_max).contains(&c.t)
}

/// `n` ≥ 2 points evenly spaced in ln from ln `lo` to ln `hi`, both included.
fn log_axis(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n).map(|k| math::ln(lo) + (math::ln(hi) - math::ln(lo)) * k as f64 / n.saturating_sub(1) as f64).collect()
}

/// A root in (0, 1) of `f`, whose values at 0 and 1 are `ends` (of opposite signs), by the Illinois variant of the
/// regula falsi: until the bracket is under 1e-3 of the interval, or 12 steps. That is close enough to sample the
/// cubic form on the limit and to start Newton's method. The bracket, not the estimate's last move, decides: the plain
/// form creeps from one side of a curved det in steps smaller than that (exp(8s) − 2 from 0 and 1: 3.4e-4, then
/// 3.3e-4), which the Illinois halving then breaks.
fn regula_falsi(f: &dyn Fn(f64) -> f64, (mut fa, mut fb): (f64, f64)) -> f64 {
    let (mut a, mut b) = (0.0, 1.0);
    let mut side = 0;
    for _ in 0..12 {
        let s = (a * fb - b * fa) / (fb - fa);
        if !s.is_finite() || b - a < 1e-3 {
            return s;
        }
        let fs = f(s);
        if !fs.is_finite() {
            return s;
        }
        if fs.signum() == fa.signum() {
            (a, fa) = (s, fs);
            if side == -1 {
                fb /= 2.0;
            }
            side = -1;
        } else {
            (b, fb) = (s, fs);
            if side == 1 {
                fa /= 2.0;
            }
            side = 1;
        }
    }
    (a * fb - b * fa) / (fb - fa)
}

/// Newton's method on (det, cubic) in (ln T, ln ρ) from `start`, its Jacobian by forward differences of relative
/// step 1e-7: up to 60 steps, kept while the residual falls, so the point returned is the best the conditions' rounding
/// allows; `None` where it leaves the finite.
fn newton(f: &dyn Fn(f64, f64) -> [f64; 2], (t, rho): (f64, f64)) -> Option<(f64, f64)> {
    let at = |u: f64, v: f64| f(math::exp(u), math::exp(v));
    let size = |r: [f64; 2]| r[0].abs() + r[1].abs();
    let (mut u, mut v) = (math::ln(t), math::ln(rho));
    let mut r = at(u, v);
    let (mut best, mut stalled) = ((u, v, size(r)), 0_u32);
    for _ in 0..60 {
        if !(r[0].is_finite() && r[1].is_finite()) {
            return None;
        }
        let Some((du, dv)) = newton_step(&at, (u, v), r) else { break };
        (u, v) = (u - du, v - dv);
        r = at(u, v);
        if size(r) < best.2 {
            (best, stalled) = ((u, v, size(r)), 0);
        } else {
            stalled = stalled.saturating_add(1);
        }
        if stalled >= 3 || size(r) == 0.0 {
            break;
        }
    }
    best.2.is_finite().then(|| (math::exp(best.0), math::exp(best.1)))
}

/// One Newton step at (u, v), where `f` is `r`: its Jacobian by forward differences of step 1e-7, the step cut to 0.1
/// in length; `None` where the Jacobian is singular or not finite.
fn newton_step(f: &dyn Fn(f64, f64) -> [f64; 2], (u, v): (f64, f64), r: [f64; 2]) -> Option<(f64, f64)> {
    let h = 1e-7;
    let (tu, vu) = (f(u + h, v), f(u, v + h));
    let j = [[tu[0] - r[0], vu[0] - r[0]], [tu[1] - r[1], vu[1] - r[1]]].map(|row| row.map(|d| d / h));
    let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let (du, dv) = ((r[0] * j[1][1] - r[1] * j[0][1]) / det, (r[1] * j[0][0] - r[0] * j[1][0]) / det);
    let step = math::sqrt(du * du + dv * dv);
    let scale = if step > 0.1 { 0.1 / step } else { 1.0 };
    Some((scale * du, scale * dv))
}

/// Heidemann and Khalil's conditions at (T, ρ), with their scales.
pub(crate) fn conditions_at(model: &MultiFluid, x: &[f64], w: &[f64], t: f64, rho: f64) -> Result<Criticality, Error> {
    let c = Conditions::new(model.composition(t, rho, x, Order::Three)?, x, w);
    let (cubic, cubic_scale) = c.cubic_terms();
    Ok(Criticality { det: c.det(), det_scale: c.det_scale(), cubic, cubic_scale })
}

#[cfg(test)]
mod tests {
    use phasekit_core::{DataSet, Registry};

    use super::*;

    /// Conditions on a given Q at fractions `x` (no composition derivatives, so no cubic form).
    fn on(q: Vec<Vec<f64>>, x: &[f64]) -> Conditions {
        Conditions { q, c: None, w: vec![1.0; x.len()], x: x.to_vec() }
    }

    /// The null vector of a singular Q by complete pivoting is Q's null vector, D-normalised (Σ Δn_i²/x_i = 1), where
    /// Q's leading entry is 0 (DIV-0056's L*, whose first row vanishes), where it has rank one, and where the first
    /// pivot lies off the diagonal of a 3 × 3.
    #[test]
    fn null_vectors_take_the_largest_pivot() {
        let cases = [
            (vec![vec![0.0, 0.0], vec![0.0, 2.0]], vec![0.5, 0.5], vec![1.0, 0.0]),
            (vec![vec![1.0, 2.0], vec![2.0, 4.0]], vec![0.25, 0.75], vec![2.0, -1.0]),
            (
                vec![vec![0.0, 1.0, 1.0], vec![1.0, 0.0, 1.0], vec![1.0, 1.0, 2.0]],
                vec![0.2, 0.3, 0.5],
                vec![1.0, 1.0, -1.0],
            ),
        ];
        for (q, x, direction) in cases {
            let v = on(q.clone(), &x).null_vector().unwrap();
            let norm = |v: &[f64]| v.iter().zip(&x).map(|(v, x)| v * v / x).sum::<f64>();
            assert!((norm(&v) - 1.0).abs() < 1e-14, "{v:?}");
            let sign = v.iter().zip(&direction).map(|(a, b)| a * b).sum::<f64>().signum();
            for (a, d) in v.iter().zip(&direction) {
                assert!((a - sign * d / math::sqrt(norm(&direction))).abs() < 1e-14, "{q:?}: {v:?}");
            }
            for row in &q {
                assert!(row.iter().zip(&v).map(|(q, v)| q * v).sum::<f64>().abs() < 1e-14, "{q:?}: {v:?}");
            }
        }
    }

    /// det(D Q D) is the 2 × 2 determinant x₁x₂(q₁₁q₂₂ − q₁₂²), Hadamard's bound the product of D Q D's row norms, and
    /// the D-inner product Σ a_i b_i/x_i.
    #[test]
    fn the_determinant_its_scale_and_the_inner_product_are_their_formulas() {
        let (q, x) = (vec![vec![3.0, -1.0], vec![-1.0, 2.0]], [0.25, 0.75]);
        let c = on(q, &x);
        let scaled = [[3.0 * 0.25, -math::sqrt(0.25 * 0.75)], [-math::sqrt(0.25 * 0.75), 2.0 * 0.75]];
        assert!((c.det() - (0.25 * 0.75) * (3.0 * 2.0 - 1.0)).abs() < 1e-15);
        let rows: f64 = scaled.iter().map(|r| math::sqrt(r[0] * r[0] + r[1] * r[1])).product();
        assert!((c.det_scale() - rows).abs() < 1e-15);
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let model = MultiFluid::new(&registry, &["Methane", "Ethane"], DataSet::Parity).unwrap();
        let search = Search { model: &model, x: &x, w: &[1.0, 1.0] };
        assert_eq!(search.along(&[1.0, 2.0], &[3.0, 4.0]), 3.0 / 0.25 + 8.0 / 0.75);
    }

    /// The grid's axes run from ln lo to ln hi in even steps, both ends included.
    #[test]
    fn axes_span_their_range() {
        let axis = log_axis(10.0, 1000.0, 5);
        assert_eq!(axis.len(), 5);
        assert_eq!((axis[0], axis[4]), (math::ln(10.0), math::ln(1000.0)));
        assert!((axis[2] - math::ln(100.0)).abs() < 1e-15 && (axis[1] - axis[0] - axis[4] + axis[3]).abs() < 1e-15);
    }

    /// A cell's quarters tile it: their corners are its corners, edge midpoints and centre, with det there; two samples
    /// leave room for two roots where the smaller is under half the larger.
    #[test]
    fn cells_split_into_their_quarters() {
        let det = |(u, v): (f64, f64)| 10.0 * u + v;
        let cell = Cell { u: (1.0, 2.0), v: (5.0, 7.0), det: [15.0, 25.0, 17.0, 27.0] };
        let quarters = cell.quarters(&det);
        assert_eq!(
            quarters.map(|q| (q.u, q.v)),
            [((1.0, 1.5), (5.0, 6.0)), ((1.5, 2.0), (5.0, 6.0)), ((1.0, 1.5), (6.0, 7.0)), ((1.5, 2.0), (6.0, 7.0))]
        );
        for q in &quarters {
            assert_eq!(q.det, [0, 1, 2, 3].map(|k| det(q.corner(k))), "{:?}", (q.u, q.v));
        }
        assert!(halves(1.0, -3.0) && halves(-3.0, 1.0) && !halves(1.0, 2.0) && !halves(2.0, 1.5));
    }

    /// A point on the stability limit whose cubic form does not vanish is no critical point; Methane/Ethane's at 50/50
    /// is one.
    #[test]
    fn only_both_conditions_make_a_critical_point() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let model = MultiFluid::new(&registry, &["Methane", "Ethane"], DataSet::Parity).unwrap();
        let (x, w) = ([0.5, 0.5], [1.0, 1.0]);
        let search = Search { model: &model, x: &x, w: &w };
        // The isotherm at 240 K, below the critical point, crosses the limit: its first sign change in ln ρ.
        let nodes: Vec<(f64, f64)> =
            (0..60).map(|k| (math::ln(240.0), math::ln(500.0) + 0.06 * f64::from(k))).collect();
        let (lo, hi) = nodes
            .windows(2)
            .map(|w| (w[0], w[1]))
            .find(|(a, b)| search.det_at(*a).signum() != search.det_at(*b).signum())
            .unwrap();
        let on = search.limit_between(lo, hi, (search.det_at(lo), search.det_at(hi))).unwrap();
        assert!(on.cubic.abs() > 1e-6 && search.critical(on.t, on.rho).is_none(), "{} K: {}", on.t, on.cubic);
        let found = critical_points(&model, &x, &w, (91.0, 610.0), 30_000.0).unwrap();
        let c = found.iter().find(|c| (c.t - 262.5).abs() < 0.1).unwrap();
        assert_eq!(search.critical(c.t, c.rho).map(|c| c.p), Some(c.p));
        // Each point once (no two within 1e-6 in both ln T and ln ρ), in increasing density, within the range.
        assert!(found.windows(2).all(|w| w[0].rho <= w[1].rho), "{found:?}");
        for (k, a) in found.iter().enumerate() {
            for b in &found[k + 1..] {
                let apart = math::ln(a.t / b.t).abs().max(math::ln(a.rho / b.rho).abs());
                assert!(apart > 1e-6, "{a:?} and {b:?}");
            }
        }
        assert!(found.iter().all(|c| (91.0..=610.0).contains(&c.t)) && found.len() >= 5, "{found:?}");
    }

    /// A sign change of the cubic form between two crossings is judged with their null vectors oriented alike: the
    /// limit's two crossings of Methane/Ethane's 261.5 K isotherm straddle its critical point, and with the second's null
    /// vector (and so its cubic form) flipped, the pair still polishes to that point.
    #[test]
    fn crossings_are_compared_with_their_null_vectors_oriented_alike() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let model = MultiFluid::new(&registry, &["Methane", "Ethane"], DataSet::Parity).unwrap();
        let (x, w) = ([0.5, 0.5], [1.0, 1.0]);
        let search = Search { model: &model, x: &x, w: &w };
        let nodes: Vec<(f64, f64)> =
            (0..60).map(|k| (math::ln(261.5), math::ln(500.0) + 0.06 * f64::from(k))).collect();
        let changes: Vec<OnLimit> = nodes
            .windows(2)
            .filter_map(|n| search.limit_between(n[0], n[1], (search.det_at(n[0]), search.det_at(n[1]))))
            .collect();
        let (a, b) = (changes.first().unwrap(), changes.last().unwrap());
        assert!(a.cubic.signum() != b.cubic.signum() && search.along(&a.dn, &b.dn) > 0.0, "{} {}", a.cubic, b.cubic);
        let flipped = OnLimit { cubic: -b.cubic, dn: b.dn.iter().map(|v| -v).collect(), ..b.clone() };
        let cell = Cell { u: (math::ln(261.5), math::ln(261.5)), v: (math::ln(a.rho), math::ln(b.rho)), det: [1.0; 4] };
        for pair in [[a.clone(), b.clone()], [a.clone(), flipped]] {
            let roots = search.roots(&cell, &pair, 0);
            assert!(roots.len() == 1 && (roots[0].t - 262.5).abs() < 0.1, "{roots:?}");
        }
    }

    /// The regula falsi finds a root of a strongly curved function, where its plain form creeps from one side: exp(8s) −
    /// 2 at ln 2/8, to 1e-5 in its 12 evaluations, and a line's in one step, either way round. The Illinois halving moves
    /// whichever end is kept twice running: exp(ks) − 2 and 2 − exp(ks) keep the right end, exp(k(1 − s)) − 2 the left.
    /// A milder curve, k = 2, stops after 7 evaluations, its bracket under 1e-3, within 1e-11 of its root.
    #[test]
    #[allow(clippy::disallowed_types, reason = "a test-only evaluation counter")]
    fn regula_falsi_finds_curved_roots() {
        const CALLS: [(usize, f64); 6] = [(12, 1e-5), (12, 1e-5), (12, 1e-5), (7, 1e-11), (7, 1e-11), (7, 1e-11)];
        type Curve = (fn(f64) -> f64, f64);
        let functions: [Curve; 6] = [
            (|s| math::exp(8.0 * s) - 2.0, math::ln(2.0) / 8.0),
            (|s| 2.0 - math::exp(8.0 * s), math::ln(2.0) / 8.0),
            (|s| math::exp(8.0 * (1.0 - s)) - 2.0, 1.0 - math::ln(2.0) / 8.0),
            (|s| math::exp(2.0 * s) - 2.0, math::ln(2.0) / 2.0),
            (|s| 2.0 - math::exp(2.0 * s), math::ln(2.0) / 2.0),
            (|s| math::exp(2.0 * (1.0 - s)) - 2.0, 1.0 - math::ln(2.0) / 2.0),
        ];
        for ((f, root), (calls, within)) in functions.iter().zip(CALLS) {
            let count = std::cell::Cell::new(0);
            let counted = |s: f64| {
                count.set(count.get() + 1);
                f(s)
            };
            let found = regula_falsi(&counted, (f(0.0), f(1.0)));
            assert!((found - root).abs() < within && count.get() == calls, "{found} after {} calls", count.get());
        }
        assert_eq!(regula_falsi(&|s| s - 0.25, (-0.25, 0.75)), 0.25);
    }

    /// Newton's method in (ln T, ln ρ) reaches a root its full steps would overshoot: arctan(ln T − 1) and
    /// arctan(ln ρ − 2), from 2 and 2.3 away in ln, whose plain Newton steps grow (x → x − (1 + x²) arctan x diverges
    /// beyond |x| ≈ 1.39); its steps are cut to 0.1 until near. It judges a point by the sum of its residuals' sizes,
    /// so it leaves a start where one of them is already 0; it gives up (`None`) where a step leaves the finite, and stops
    /// three steps after the residual last fell: |ln T − 1| + 1e-6 never falls below 1e-6.
    #[test]
    #[allow(clippy::disallowed_types, reason = "a test-only evaluation counter")]
    fn newton_cuts_its_steps() {
        let f = |t: f64, rho: f64| [math::atan(math::ln(t) - 1.0), math::atan(math::ln(rho) - 2.0)];
        let (t, rho) = newton(&f, (math::exp(-1.0), math::exp(-0.3))).unwrap();
        assert!((t / math::exp(1.0) - 1.0).abs() < 1e-9 && (rho / math::exp(2.0) - 1.0).abs() < 1e-9, "{t} {rho}");
        let (t, rho) = newton(&f, (math::exp(-1.0), math::exp(2.0))).unwrap();
        assert!((t / math::exp(1.0) - 1.0).abs() < 1e-9 && (rho / math::exp(2.0) - 1.0).abs() < 1e-9, "{t} {rho}");
        let g = |t: f64, rho: f64| [t - 3.0 * rho, rho - 2.0];
        let (t, rho) = newton(&g, (1.0, 1.0)).unwrap();
        assert!((t - 6.0).abs() < 1e-9 && (rho - 2.0).abs() < 1e-9, "{t} {rho}");
        assert_eq!(newton(&|_, _| [f64::NAN, 0.0], (1.0, 1.0)), None);
        let edge = |t: f64, rho: f64| {
            let u = math::ln(t);
            [if u < 0.4 { u - 1.0 } else { f64::NAN }, math::ln(rho) - 2.0]
        };
        assert_eq!(newton(&edge, (math::exp(0.35), math::exp(2.0))), None);
        let count = std::cell::Cell::new(0);
        let floor = |t: f64, rho: f64| {
            count.set(count.get() + 1);
            [(math::ln(t) - 1.0).abs() + 1e-6, math::ln(rho) - 2.0]
        };
        let (t, rho) = newton(&floor, (math::exp(1.05), math::exp(2.05))).unwrap();
        assert!((math::ln(t) - 1.0).abs() < 1e-5 && (math::ln(rho) - 2.0).abs() < 1e-9, "{t} {rho}");
        assert!(count.get() <= 40, "{} calls", count.get());
    }

    /// One Newton step: the solution of J Δ = r with J by forward differences, its length cut to 0.1 where longer, the
    /// direction kept; `None` where J is singular.
    #[test]
    fn newton_steps_are_cut_to_a_tenth() {
        let f = |u: f64, v: f64| [u - 1.0, v - 2.0];
        let (du, dv) = newton_step(&f, (0.0, 0.0), f(0.0, 0.0)).unwrap();
        assert!((math::sqrt(du * du + dv * dv) - 0.1).abs() < 1e-12 && (dv / du - 2.0).abs() < 1e-6, "{du} {dv}");
        let (du, dv) = newton_step(&f, (0.99, 1.995), f(0.99, 1.995)).unwrap();
        assert!((du + 0.01).abs() < 1e-9 && (dv + 0.005).abs() < 1e-9, "{du} {dv}");
        let singular = |u: f64, v: f64| [u + v, u + v];
        assert_eq!(newton_step(&singular, (0.0, 0.0), singular(0.0, 0.0)), None);
    }

    /// The search splits a cell as many times as it is told, no more: a cell around Methane/Ethane's critical point
    /// (262.5 K, 9017 mol/m³ at 50/50), 0.5 wide in ln T and 0.2 in ln ρ, holds it only for a second split.
    #[test]
    fn splits_go_as_deep_as_asked() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let model = MultiFluid::new(&registry, &["Methane", "Ethane"], DataSet::Parity).unwrap();
        let (x, w) = ([0.5, 0.5], [1.0, 1.0]);
        let search = Search { model: &model, x: &x, w: &w };
        let (u0, v0) = (math::ln(262.528_642) + 0.45 * 0.25, math::ln(9016.626) - 0.45 * 0.1);
        let (u, v) = ((u0 - 0.25, u0 + 0.25), (v0 - 0.1, v0 + 0.1));
        let cell = Cell { u, v, det: [(u.0, v.0), (u.1, v.0), (u.0, v.1), (u.1, v.1)].map(|at| search.det_at(at)) };
        let on = search.crossings(&cell);
        let found: Vec<Vec<CriticalState>> = (0..3).map(|splits| search.roots(&cell, &on, splits)).collect();
        assert!(found[0].is_empty() && found[1].is_empty() && found[2].len() == 1, "{found:?}");
        assert!((found[2][0].t - 262.5).abs() < 0.1, "{found:?}");
    }

    /// A segment whose ends' det share a sign, or where either is not finite, crosses no limit.
    #[test]
    fn a_crossing_needs_det_to_change_sign() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let model = MultiFluid::new(&registry, &["Methane", "Ethane"], DataSet::Parity).unwrap();
        let (x, w) = ([0.5, 0.5], [1.0, 1.0]);
        let search = Search { model: &model, x: &x, w: &w };
        let (p, q) = ((math::ln(240.0), math::ln(2000.0)), (math::ln(240.0), math::ln(16000.0)));
        for ends in [(1.0, 2.0), (-1.0, -2.0), (f64::NAN, -1.0), (1.0, f64::INFINITY)] {
            assert!(search.limit_between(p, q, ends).is_none(), "{ends:?}");
        }
    }

    /// A point found is kept once: one within 1e-8 of a kept point in both T and ρ is that point, one further in either
    /// is another; and only in [t_min, t_max], its ends included.
    #[test]
    fn each_point_is_kept_once_and_in_range() {
        let at = |t: f64, rho: f64| CriticalState { t, rho, p: 1.0 };
        let found = [at(250.0, 9000.0)];
        let range = (100.0, 300.0);
        assert!(!fresh(&at(250.0 * (1.0 + 5e-9), 9000.0 * (1.0 - 5e-9)), &found, range));
        assert!(fresh(&at(250.0 * (1.0 + 2e-8), 9000.0), &found, range));
        assert!(fresh(&at(250.0, 9000.0 * (1.0 - 2e-8)), &found, range));
        assert!(fresh(&at(250.0, 18000.0), &found, range) && fresh(&at(125.0, 9000.0), &found, range));
        assert!(fresh(&at(100.0, 1.0), &found, range) && fresh(&at(300.0, 1.0), &found, range));
        assert!(!fresh(&at(99.999, 1.0), &found, range) && !fresh(&at(300.001, 1.0), &found, range));
    }

    /// The cubic form along a direction Δn is Σ_ijk (∂³Ψ/∂n_i∂n_j∂n_k + ∂³Ψ⁰/∂n_i∂n_j∂n_k) Δn_i Δn_j Δn_k with the ideal
    /// part's in closed form, (w_i + w_j + w_k − 2w̄ − 1) − w_i δ_ijk/x_i², at a state off the stability limit; the
    /// conditions' cubic form is it along their null vector, and its scale the sum of the terms' magnitudes.
    #[test]
    fn the_cubic_form_is_its_sum() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let model = MultiFluid::new(&registry, &["Methane", "Ethane"], DataSet::Parity).unwrap();
        let (x, w) = ([0.4, 0.6], [1.000_002, 0.999_997]);
        let c = model.composition(250.0, 4000.0, &x, Order::Three).unwrap();
        let psi: Vec<f64> =
            [[0, 0, 0], [0, 0, 1], [0, 1, 1], [1, 1, 1]].iter().map(|ijk| c.psi(&ijk.map(Var::N))).collect();
        let conditions = Conditions::new(c, &x, &w);
        let w_bar = x[0] * w[0] + x[1] * w[1];
        let third = |i: usize, j: usize, k: usize| {
            let ideal =
                w[i] + w[j] + w[k] - 2.0 * w_bar - 1.0 - if i == j && j == k { w[i] / (x[i] * x[i]) } else { 0.0 };
            psi[i + j + k] + ideal
        };
        let dn = [0.3, -0.7];
        let (mut sum, mut scale) = (0.0, 0.0);
        for (i, j, k) in (0..2).flat_map(|i| (0..2).flat_map(move |j| (0..2).map(move |k| (i, j, k)))) {
            let term = third(i, j, k) * dn[i] * dn[j] * dn[k];
            (sum, scale) = (sum + term, scale + term.abs());
        }
        let (got, got_scale) = conditions.cubic_along(&dn);
        assert!(
            (got - sum).abs() <= 1e-13 * scale && (got_scale - scale).abs() <= 1e-13 * scale,
            "{got} against {sum}"
        );
        let along = conditions.null_vector().map(|v| conditions.cubic_along(&v));
        assert_eq!(along, Some(conditions.cubic_terms()));
        assert_eq!(conditions.cubic(), conditions.cubic_terms().0);
        assert!(conditions.cubic().abs() > 1e-3, "{}", conditions.cubic());
    }
}
