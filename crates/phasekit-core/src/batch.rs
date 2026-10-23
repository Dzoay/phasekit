//! L6 batch: one driver for every facade (map 11 U4). Caller-owned buffers, one status per cell, no
//! allocation on success, fixed chunks. "What" is the model's flash; "how" is [`ExecPolicy`] (D9). Every
//! point goes through `Fluid::flash`'s gate, so batch and scalar results are the same code path; under
//! `ExecPolicy::Simd` the points it passes go to the model a group at a time (`ThermoModel::flash_many`, PLAN.md M22.4),
//! where a Helmholtz fluid's DT, QT or PQ evaluates its points' bundles, and a QT or PQ batch its saturations, in lanes
//! first (M12.8, M12.9), and a mixture its hinted DT's. [`run_kernel`] runs a family's [`RealKernel`] over columns under
//! the same policies (M22.2).
#![deny(clippy::indexing_slicing)] // E12: no panicking index on the batch path

#[cfg(feature = "rayon")]
use rayon::iter::{IndexedParallelIterator, ParallelIterator};
#[cfg(feature = "rayon")]
use rayon::slice::{ParallelSlice, ParallelSliceMut};

use crate::error::Error;
use crate::exec::chunk_points;
pub use crate::exec::{ExecPolicy, RealKernel, run_kernel};
use crate::flash::FlashOptions;
use crate::fluid::Fluid;
use crate::input::{Input, NativeInput, Pair};
use crate::prop::Prop;
use crate::state::State;
use crate::units::Basis;

/// Outcome of one cell. NaN appears in an output cell only with a non-`Ok` status (map 11 F15).
/// Discriminants are not an ABI: `phasekit-capi` maps each variant to a pinned, append-only code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(missing_docs)] // one per `Error` family
pub enum Status {
    #[default]
    Ok,
    /// A value computed outside the validated domain (only under `DomainPolicy::Extrapolate`); not a failure.
    Extrapolated,
    InvalidInput,
    Unsupported,
    Domain,
    NoConvergence,
    Ambiguous,
    Undefined,
    NoModel,
    Load,
    Other,
}

impl From<&Error> for Status {
    fn from(e: &Error) -> Self {
        match e {
            Error::InvalidInput { .. } | Error::Unreachable { .. } | Error::InvalidArgument { .. } => {
                Status::InvalidInput
            }
            Error::Unsupported { .. } => Status::Unsupported,
            Error::Domain(_) => Status::Domain,
            Error::NoConvergence { .. } => Status::NoConvergence,
            Error::Ambiguous { .. } => Status::Ambiguous,
            Error::Undefined { .. } => Status::Undefined,
            Error::NoModel { .. } => Status::NoModel,
            Error::Load(_) => Status::Load,
            Error::ForeignState | Error::InvalidState { .. } | Error::Shape { .. } => Status::Other,
        }
    }
}

/// A batch: N points of one input pair, M outputs each. Private fields; build with [`BatchRequest::new`].
#[derive(Clone, Copy, Debug)]
pub struct BatchRequest<'a> {
    pair: Pair,
    basis: Basis,
    x: &'a [f64],
    y: &'a [f64],
    outputs: &'a [Prop],
    flash: FlashOptions,
    exec: ExecPolicy,
    quality_basis: Basis,
}

impl<'a> BatchRequest<'a> {
    /// `x[i]`, `y[i]` are point `i` in pair order; `basis` applies to D, H, S and U. Default options and
    /// the `Reference` executor.
    pub fn new(pair: Pair, basis: Basis, x: &'a [f64], y: &'a [f64], outputs: &'a [Prop]) -> Self {
        let (flash, exec, quality_basis) = (FlashOptions::new(), ExecPolicy::Reference, Basis::Molar);
        Self { pair, basis, x, y, outputs, flash, exec, quality_basis }
    }
    /// Flash options for every point.
    pub const fn with_flash(mut self, flash: FlashOptions) -> Self {
        self.flash = flash;
        self
    }
    /// Executor.
    pub const fn with_exec(mut self, exec: ExecPolicy) -> Self {
        self.exec = exec;
        self
    }
    /// The basis of the quality, for a pair with Q (`Input::with_quality_basis`): a mass quality reaches the model as
    /// one.
    pub const fn with_quality_basis(mut self, basis: Basis) -> Self {
        self.quality_basis = basis;
        self
    }

    /// Point `(x, y)` as an input: the raw gate, then the quality's basis.
    fn input(&self, x: f64, y: f64) -> Result<Input, Error> {
        Ok(Input::new(self.pair, x, y, self.basis)?.with_quality_basis(self.quality_basis))
    }
    /// Number of points.
    pub const fn points(&self) -> usize {
        self.x.len()
    }
    /// Number of cells (`points × outputs`), or `None` on overflow.
    pub const fn cells(&self) -> Option<usize> {
        self.x.len().checked_mul(self.outputs.len())
    }
}

/// Counts of a finished batch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatchSummary {
    /// Points evaluated.
    pub points: usize,
    /// Cells with an error status (neither `Ok` nor `Extrapolated`).
    pub failed_cells: usize,
}

/// Evaluates a batch into caller-owned, point-major buffers: `out[i * M + k]` is output `k` of point `i`,
/// `status` has the same layout. Point-major chunks are contiguous, so executors split them without
/// copying or `unsafe` (and a NumPy `(N, M)` array maps directly).
pub fn evaluate(
    fluid: &Fluid,
    req: &BatchRequest<'_>,
    out: &mut [f64],
    status: &mut [Status],
) -> Result<BatchSummary, Error> {
    run(fluid, req, (out, status), &mut [])
}

/// [`evaluate`], keeping each point's error in `errors`, one slot a point (PLAN.md M29.4; user decision EM17): the
/// error of its flash, else of its first refused output, as the scalar path returns it, and `None` for a point without
/// one. Under every [`ExecPolicy`] the same errors; nothing is allocated for a point without one.
pub fn evaluate_detailed(
    fluid: &Fluid,
    req: &BatchRequest<'_>,
    out: &mut [f64],
    status: &mut [Status],
    errors: &mut [Option<Error>],
) -> Result<BatchSummary, Error> {
    if errors.len() != req.points() {
        return Err(Error::Shape { expected: req.points(), found: errors.len() });
    }
    errors.fill(None);
    run(fluid, req, (out, status), errors)
}

/// [`evaluate`]'s and [`evaluate_detailed`]'s driver; `errors` is empty where they are not kept.
fn run(
    fluid: &Fluid,
    req: &BatchRequest<'_>,
    (out, status): (&mut [f64], &mut [Status]),
    errors: &mut [Option<Error>],
) -> Result<BatchSummary, Error> {
    let (n, m) = (req.x.len(), req.outputs.len());
    let cells = req.cells().ok_or(Error::Shape { expected: usize::MAX, found: out.len() })?;
    for (expected, found) in [(n, req.y.len()), (cells, out.len()), (cells, status.len())] {
        if found != expected {
            return Err(Error::Shape { expected, found });
        }
    }
    if cells == 0 {
        return Ok(BatchSummary { points: n, failed_cells: 0 }); // E12: no zero-sized chunks
    }
    // At most `n` points per chunk, so `chunk * m` is at most `cells`.
    let chunk = chunk_points(req.exec, n);
    let width = chunk.saturating_mul(m);
    #[cfg(feature = "rayon")]
    // Equivalent (M9.1): with `<=` a batch of one chunk goes to rayon, whose parallel iterator runs a single piece
    // inline on the calling thread: the same thread and bits.
    // equivalent mutant: replace < with <=
    if chunk < n {
        let rows = req.x.par_chunks(chunk).zip(req.y.par_chunks(chunk));
        let rows = rows.zip(out.par_chunks_mut(width).zip(status.par_chunks_mut(width)));
        let failed = if errors.is_empty() {
            rows.map(|(points, cells)| run_chunk(fluid, req, points, cells, &mut [])).sum()
        } else {
            let rows = rows.zip(errors.par_chunks_mut(chunk));
            rows.map(|((points, cells), errors)| run_chunk(fluid, req, points, cells, errors)).sum()
        };
        return Ok(BatchSummary { points: n, failed_cells: failed });
    }
    // One chunk, or no rayon: the chunks in turn on the calling thread, with the boundaries rayon's would have.
    let rows = req.x.chunks(chunk).zip(req.y.chunks(chunk));
    let rows = rows.zip(out.chunks_mut(width).zip(status.chunks_mut(width)));
    let mut failed = 0_usize;
    for (k, (points, cells)) in rows.enumerate() {
        let from = k.saturating_mul(chunk);
        let errors = errors.get_mut(from..from.saturating_add(chunk).min(errors.len())).unwrap_or_default();
        failed = failed.saturating_add(run_chunk(fluid, req, points, cells, errors));
    }
    Ok(BatchSummary { points: n, failed_cells: failed })
}

/// One chunk, point by point, or, under `ExecPolicy::Simd`, through the model's group hook ([`run_groups`]). Returns
/// the failed-cell count.
fn run_chunk(
    fluid: &Fluid,
    req: &BatchRequest<'_>,
    (xs, ys): (&[f64], &[f64]),
    (out, status): (&mut [f64], &mut [Status]),
    errors: &mut [Option<Error>],
) -> usize {
    if req.exec == ExecPolicy::Simd {
        return run_groups(fluid, req, (xs, ys), (out, status), errors);
    }
    let m = req.outputs.len();
    let mut failed = 0_usize;
    let rows = out.chunks_exact_mut(m).zip(status.chunks_exact_mut(m));
    for (k, ((x, y), (out, status))) in xs.iter().zip(ys).zip(rows).enumerate() {
        // The single gate (validation, basis, gauge), then the model: exactly the scalar path.
        let state = req.input(*x, *y).and_then(|input| fluid.flash(input, &req.flash));
        failed = failed.saturating_add(write_row(fluid, req, &state, (out, status), errors.get_mut(k)));
    }
    failed
}

/// Points the batch hands a model's group hook at a time (PLAN.md M22.4). A hook pays for each call as well as for each
/// point: a mixture's evaluates its group's bundles through several lane kernels, whose code and set-up come afresh
/// each call. At 64 points a group (one point in two refused by the domain) R32/R125's DT batch under a phase hint
/// took 1.15× its scalar path's time in the default build, at 256 0.93×, at 1024 no less. A pure fluid's hook runs its
/// lanes `flash::GROUP` points at a time inside it.
const HOOK_GROUP: usize = 256;

/// A chunk [`HOOK_GROUP`] points at a time: each point through the single gate as `Fluid::flash` takes it (validation,
/// basis, gauge), then the points it passes, in order, through the model's `ThermoModel::flash_many` (PLAN.md M22.4;
/// user decision ML5), whose results are the model's flash at each point, stamped with the handle's gauge as
/// `Fluid::flash` stamps them. A point the gate refuses keeps its refusal and is not handed to the model. Returns the
/// failed-cell count.
fn run_groups(
    fluid: &Fluid,
    req: &BatchRequest<'_>,
    (xs, ys): (&[f64], &[f64]),
    (out, status): (&mut [f64], &mut [Status]),
    errors: &mut [Option<Error>],
) -> usize {
    let m = req.outputs.len();
    let mut failed = 0_usize;
    // The hook writes each group's states here and the rows read them in place: one array for the chunk, so that no
    // group moves an array of states (moving them cost a mixture's DT batch up to 17 % in the default build's record).
    let mut states: [Result<State, Error>; HOOK_GROUP] =
        core::array::from_fn(|_| Err(Error::Shape { expected: 0, found: 0 }));
    let groups = xs
        .chunks(HOOK_GROUP)
        .zip(ys.chunks(HOOK_GROUP))
        .zip(out.chunks_mut(HOOK_GROUP.saturating_mul(m)).zip(status.chunks_mut(HOOK_GROUP.saturating_mul(m))));
    for (g, ((xs, ys), (out, status))) in groups.enumerate() {
        let points: [Result<NativeInput, Error>; HOOK_GROUP] = core::array::from_fn(|k| {
            let (x, y) = xs.get(k).zip(ys.get(k)).ok_or(Error::Shape { expected: HOOK_GROUP, found: k })?;
            fluid.native(req.input(*x, *y)?)
        });
        flash_passed(fluid, req, &points, &mut states);
        let mut states = states.iter();
        let rows = out.chunks_exact_mut(m).zip(status.chunks_exact_mut(m));
        for (k, ((point, (x, y)), cells)) in points.into_iter().zip(xs.iter().zip(ys)).zip(rows).enumerate() {
            // A model's refusal in the caller's terms, as `Fluid::flash` writes it.
            let caller = |e: Error| match req.input(*x, *y) {
                Ok(input) => fluid.caller_error(input, e),
                Err(_) => e,
            };
            let state = point.and_then(|_| gauged(fluid, states.next()).map_err(caller));
            let slot = errors.get_mut(g.saturating_mul(HOOK_GROUP).saturating_add(k));
            failed = failed.saturating_add(write_row(fluid, req, &state, cells, slot));
        }
    }
    failed
}

/// A passed point's result from the group hook, its state stamped with the handle's gauge as `Fluid::flash` stamps it.
fn gauged(fluid: &Fluid, state: Option<&Result<State, Error>>) -> Result<State, Error> {
    match state {
        Some(Ok(state)) => Ok(state.with_gauge(fluid.gauge())),
        Some(Err(error)) => Err(error.clone()),
        None => Err(Error::Shape { expected: HOOK_GROUP, found: 0 }),
    }
}

/// The model's flash of the points of `points` the gate passed, in order, through its group hook, into the first of
/// `states`; the rest keep what they held.
fn flash_passed(
    fluid: &Fluid,
    req: &BatchRequest<'_>,
    points: &[Result<NativeInput, Error>; HOOK_GROUP],
    states: &mut [Result<State, Error>; HOOK_GROUP],
) {
    let Some(first) = points.iter().find_map(|p| p.as_ref().ok().copied()) else { return };
    let mut passed = [first; HOOK_GROUP];
    let mut n = 0_usize;
    for (slot, input) in passed.iter_mut().zip(points.iter().filter_map(|p| p.as_ref().ok())) {
        *slot = *input;
        n = n.saturating_add(1);
    }
    if let (Some(passed), Some(out)) = (passed.get(..n), states.get_mut(..n)) {
        fluid.model().flash_many(passed, &req.flash, out);
    }
}

/// One point's row of cells from its flash: each output and its status. Returns the failed-cell count.
fn write_row(
    fluid: &Fluid,
    req: &BatchRequest<'_>,
    state: &Result<State, Error>,
    (out, status): (&mut [f64], &mut [Status]),
    slot: Option<&mut Option<Error>>,
) -> usize {
    let mut failed = 0_usize;
    let ok = if state.as_ref().is_ok_and(State::is_extrapolated) { Status::Extrapolated } else { Status::Ok };
    let mut first = None;
    for ((o, s), prop) in out.iter_mut().zip(status.iter_mut()).zip(req.outputs) {
        let value = state.as_ref().map_err(Clone::clone).and_then(|st| fluid.prop(st, *prop));
        (*o, *s) = match value {
            Ok(v) => (v, ok),
            Err(e) => {
                let cell = Status::from(&e);
                first = first.or(Some(e));
                (f64::NAN, cell)
            }
        };
        failed = failed.saturating_add(usize::from(!matches!(*s, Status::Ok | Status::Extrapolated)));
    }
    if let Some(slot) = slot {
        *slot = first;
    }
    failed
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::data::FluidRecord;

    /// ROT-022 (map 11 F15, map 14 R4): a NaN input is an `InvalidInput` cell with NaN beside it, never a stale
    /// value or +inf, and the cells around it are still evaluated (CoolProp gives [.., inf] for [300, nan]).
    #[test]
    fn nan_input_is_invalid_input_status() {
        let fluid = Fluid::new(Arc::new(FluidRecord::synthetic("X").unwrap().compile().unwrap()));
        let liquid = FlashOptions::new().with_phase(crate::state::Phase::Liquid);
        let (x, y) = ([5000.0, f64::NAN, 5000.0], [300.0, 300.0, f64::NAN]);
        let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &[Prop::T, Prop::P]).with_flash(liquid);
        let (mut out, mut status) = ([7.0; 6], [Status::Other; 6]);
        let summary = evaluate(&fluid, &req, &mut out, &mut status).unwrap();
        let invalid = Status::InvalidInput;
        assert_eq!((status, summary.failed_cells), ([Status::Ok, Status::Ok, invalid, invalid, invalid, invalid], 4));
        assert_eq!(out.first(), Some(&300.0));
        assert!(out.iter().skip(2).all(|v| v.is_nan()), "{out:?}");
        assert!(matches!(Input::new(Pair::DT, f64::NAN, 300.0, Basis::Molar), Err(Error::InvalidInput { .. })));
    }

    /// K6, K13 (PLAN.md M9.1): built without `rayon`, `Parallel` runs its chunks in turn on the calling thread, every
    /// cell and status bitwise the `Reference` path's, at any chunk size, a short last chunk included. G4 runs this:
    /// wasm builds core without rayon, while the native workspace enables it through phasekit-verify's thread tests.
    #[cfg(not(feature = "rayon"))]
    #[test]
    fn parallel_without_the_feature_is_sequential_and_identical() {
        let fluid = Fluid::new(Arc::new(FluidRecord::synthetic("X").unwrap().compile().unwrap()));
        let liquid = FlashOptions::new().with_phase(crate::state::Phase::Liquid);
        let x: Vec<f64> = (0..100).map(|i| if i == 7 { f64::NAN } else { 4000.0 + 20.0 * f64::from(i) }).collect();
        let y: Vec<f64> = (0..100).map(|i| if i == 50 { -1.0 } else { 250.0 + f64::from(i) }).collect();
        let outputs = [Prop::T, Prop::P, Prop::Cpmolar];
        let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &outputs).with_flash(liquid);
        let run = |exec| {
            let (mut out, mut status) = (vec![0.0; 300], vec![Status::Other; 300]);
            let summary = evaluate(&fluid, &req.with_exec(exec), &mut out, &mut status).unwrap();
            (out.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), status, summary)
        };
        let reference = run(ExecPolicy::Reference);
        assert!(reference.2.failed_cells == 6 && reference.1.iter().filter(|s| **s == Status::Ok).count() == 294);
        for chunk in [1, 16, 64, 1024] {
            let parallel = ExecPolicy::Parallel { chunk: core::num::NonZeroUsize::new(chunk).unwrap() };
            assert!(run(parallel) == reference, "chunk {chunk}");
        }
    }

    /// E12: every buffer shape is either evaluated or refused with `Shape`; nothing panics (zero outputs and
    /// zero points included). `batch_request_shapes_never_panic` (tests/properties.rs) widens this to any shape.
    #[test]
    fn batch_shapes_never_panic() {
        let fluid = Fluid::new(Arc::new(FluidRecord::synthetic("X").unwrap().compile().unwrap()));
        let (xs, ys, props) = ([5000.0, f64::NAN, -1.0, 1.0], [300.0; 4], [Prop::P, Prop::Cpmolar]);
        for n in 0..=3 {
            for m in 0..=2 {
                let (x, outputs) = (xs.get(..n).unwrap(), props.get(..m).unwrap());
                let run = |y_len: usize, cells: usize| {
                    let req = BatchRequest::new(Pair::DT, Basis::Molar, x, ys.get(..y_len).unwrap(), outputs);
                    let (mut out, mut status) = (vec![0.0; cells], vec![Status::Ok; cells]);
                    evaluate(&fluid, &req, &mut out, &mut status).map(|s| s.points)
                };
                assert_eq!(run(n, n * m), Ok(n));
                assert!(matches!(run(n + 1, n * m), Err(Error::Shape { .. })));
                assert!(matches!(run(n, n * m + 1), Err(Error::Shape { .. })));
            }
        }
    }
}
