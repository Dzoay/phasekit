//! A mixture's α^r and α⁰ batches at its composition (PLAN.md M22.1). The reducing state is computed once per batch,
//! as z is fixed. Each component's own batch hook runs over a block of points, at its scaled (T_i, ρ_i) for α^r and at
//! the mixture's (T, ρ) for α⁰, so a built-in component runs core's lanes under `ExecPolicy::Simd`. The departures run
//! point by point, and each point's parts are summed by the code that sums a single point's. So every point has the
//! bits of its own `residual` and `ideal` under any policy (K17), as each component's hook has its point's.

use phasekit_core::batch::ExecPolicy;
use phasekit_core::{Derivs, Error, Order, math};

use crate::departure;
use crate::mixture::ideal_sum;
use crate::multifluid::{Component, MultiFluid, not_a_number, summed};

/// Points per block: each component's batch runs over one block at a time, so the block's parts stay in cache and a
/// batch's memory does not grow with its length. A multiple of every lane width (32 on AVX-512).
const BLOCK: usize = 64;

/// Whether a batch of `t`, `rho` and `out` (its length) has points to run: `Ok(false)` when it is empty, and
/// [`Error::Shape`] when the lengths differ, as core's batch hooks report them.
fn shape(t: &[f64], rho: &[f64], out: usize) -> Result<bool, Error> {
    for (expected, found) in [(t.len(), rho.len()), (t.len(), out)] {
        if found != expected {
            return Err(Error::Shape { expected, found });
        }
    }
    Ok(!t.is_empty())
}

/// A batch's working columns, allocated once per call and reused block by block: the block's τ and δ, one component's
/// scaled (T_i, ρ_i), every component's parts (component-major, a column of the block's points each) and one point's
/// departures.
struct Columns {
    tau: Vec<f64>,
    delta: Vec<f64>,
    t: Vec<f64>,
    rho: Vec<f64>,
    parts: Vec<Derivs>,
    departures: Vec<Derivs>,
}

impl Columns {
    fn new(components: usize, departures: usize) -> Columns {
        let points = || Vec::with_capacity(BLOCK);
        Columns {
            tau: points(),
            delta: points(),
            t: points(),
            rho: points(),
            parts: Vec::with_capacity(components.saturating_mul(BLOCK)),
            departures: Vec::with_capacity(departures),
        }
    }

    /// `components` columns of `points` bundles to `order`, to be overwritten.
    fn reset_parts(&mut self, components: usize, points: usize, order: Order) {
        self.parts.clear();
        self.parts.resize(components.saturating_mul(points), Derivs::zero(order));
    }
}

/// Point `k`'s parts in component-major `parts` of `points` (> 0) points per column: one per component, in order.
fn point(parts: &[Derivs], k: usize, points: usize) -> impl Iterator<Item = &Derivs> + Clone {
    parts.iter().skip(k).step_by(points)
}

impl Component {
    /// Its own model's `residual_batch` at its scaled states (T_r,i/τ, δ ρ_r,i), which the caller writes into `t` and
    /// `rho` as [`Component::residual`] scales a point.
    fn residual_batch(
        &self,
        (t, rho): (&[f64], &[f64]),
        order: Order,
        out: &mut [Derivs],
        exec: ExecPolicy,
    ) -> Result<(), Error> {
        match self.fluid.model().helmholtz() {
            Some(model) => model.residual_batch(t, rho, order, out, exec),
            None => {
                out.fill(not_a_number(order));
                Ok(())
            }
        }
    }
}

impl MultiFluid {
    /// [`residual_at`](Self::residual_at) at every point `(t[k], rho[k])` into `out[k]`, at mole fractions `x` of the
    /// components' number, under `exec`: the bits of `residual_at` at each point whatever the policy. Unequal lengths are
    /// [`Error::Shape`].
    pub(crate) fn residual_batch_at(
        &self,
        x: &[f64],
        (t, rho): (&[f64], &[f64]),
        order: Order,
        out: &mut [Derivs],
        exec: ExecPolicy,
    ) -> Result<(), Error> {
        if !shape(t, rho, out.len())? {
            return Ok(());
        }
        let reducing = self.reducing_state(x);
        let weights: Vec<f64> = self.departures.iter().map(|d| d.weight(x)).collect();
        let mut columns = Columns::new(self.components.len(), self.departures.len());
        for ((t, rho), out) in t.chunks(BLOCK).zip(rho.chunks(BLOCK)).zip(out.chunks_mut(BLOCK)) {
            columns.tau.clear();
            columns.tau.extend(t.iter().map(|t| reducing.0 / t));
            columns.delta.clear();
            columns.delta.extend(rho.iter().map(|rho| rho / reducing.1));
            self.residual_block(x, &weights, &mut columns, order, out, exec)?;
        }
        Ok(())
    }

    /// One block of [`residual_batch_at`](Self::residual_batch_at), its τ and δ in `columns`: each component's batch,
    /// then each point's departures and sum.
    fn residual_block(
        &self,
        x: &[f64],
        weights: &[f64],
        columns: &mut Columns,
        order: Order,
        out: &mut [Derivs],
        exec: ExecPolicy,
    ) -> Result<(), Error> {
        let points = out.len().max(1);
        columns.reset_parts(self.components.len(), points, order);
        for (c, part) in self.components.iter().zip(columns.parts.chunks_mut(points)) {
            columns.t.clear();
            columns.t.extend(columns.tau.iter().map(|tau| c.t_r / tau));
            columns.rho.clear();
            columns.rho.extend(columns.delta.iter().map(|delta| delta * c.rho_r));
            c.residual_batch((&columns.t, &columns.rho), order, part, exec)?;
        }
        for (k, ((tau, delta), o)) in columns.tau.iter().zip(&columns.delta).zip(out).enumerate() {
            columns.departures.clear();
            columns
                .departures
                .extend(self.departures.iter().map(|d| departure::derivs(d.function, *tau, *delta, order)));
            let parts = x.iter().copied().zip(point(&columns.parts, k, points));
            *o = summed(order, parts.chain(weights.iter().copied().zip(&columns.departures)));
        }
        Ok(())
    }
}

/// α⁰ at every point `(t[k], rho[k])` into `out[k]`, under `exec`, of a mixture of `fluid`'s components whose weights
/// are `weights` (each component's x_i R_i/R and ln x_i): each component's own `ideal_batch` at the mixture's (T, ρ), then
/// each point's sum through [`ideal_sum`], so each point has the bits of `Mixture::ideal_at`. Unequal lengths are
/// [`Error::Shape`].
pub(crate) fn ideal_batch(
    fluid: &MultiFluid,
    weights: &[(f64, f64)],
    (t, rho): (&[f64], &[f64]),
    order: Order,
    out: &mut [Derivs],
    exec: ExecPolicy,
) -> Result<(), Error> {
    if !shape(t, rho, out.len())? {
        return Ok(());
    }
    let mut columns = Columns::new(fluid.components.len(), 0);
    for ((t, rho), out) in t.chunks(BLOCK).zip(rho.chunks(BLOCK)).zip(out.chunks_mut(BLOCK)) {
        let points = out.len().max(1);
        columns.reset_parts(fluid.components.len(), points, order);
        for (c, part) in fluid.components.iter().zip(columns.parts.chunks_mut(points)) {
            match c.fluid.model().helmholtz() {
                Some(model) => model.ideal_batch(t, rho, order, part, exec)?,
                None => part.fill(not_a_number(order)),
            }
        }
        for (k, (rho, o)) in rho.iter().zip(out).enumerate() {
            let parts = weights.iter().zip(point(&columns.parts, k, points)).map(|(&(w, ln_x), d)| (w, ln_x, d));
            *o = ideal_sum(order, math::ln(*rho), parts);
        }
    }
    Ok(())
}
