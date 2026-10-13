//! L6 batch: one driver for every facade (map 11 U4). Caller-owned buffers, one status per cell, no
//! allocation on success, fixed chunks. "What" is the model's flash; "how" is [`ExecPolicy`] (D9). Every
//! point goes through `Fluid::flash`, so batch and scalar results are the same code path.
#![deny(clippy::indexing_slicing)] // E12: no panicking index on the batch path

use core::num::NonZeroUsize;

#[cfg(feature = "rayon")]
use rayon::iter::{IndexedParallelIterator, ParallelIterator};
#[cfg(feature = "rayon")]
use rayon::slice::{ParallelSlice, ParallelSliceMut};

use crate::error::Error;
use crate::flash::FlashOptions;
use crate::fluid::Fluid;
use crate::input::{Input, Pair};
use crate::prop::Prop;
use crate::state::State;
use crate::units::Basis;

/// How a batch is executed. Chosen at run time; never changes results (features only add capability, K13).
/// A SIMD executor would be a new variant behind the post-0.1 gate (§7), additive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecPolicy {
    /// The scalar reference path: the source of truth.
    #[default]
    Reference,
    /// Fixed chunks, run on rayon's current pool (the global one, or the pool `ThreadPool::install` runs the
    /// caller in) when the `rayon` feature is on, and in turn on the calling thread otherwise; a batch of one
    /// chunk always runs on the calling thread. Chunks are independent and cold-started, so results are bitwise
    /// identical for any thread count (K6).
    Parallel {
        /// Points per chunk, rounded up to a multiple of 16 (K6: whole lanes for a later SIMD executor).
        chunk: NonZeroUsize,
    },
}

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
            Error::InvalidInput { .. } => Status::InvalidInput,
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
}

impl<'a> BatchRequest<'a> {
    /// `x[i]`, `y[i]` are point `i` in pair order; `basis` applies to D, H, S and U. Default options and
    /// the `Reference` executor.
    pub fn new(pair: Pair, basis: Basis, x: &'a [f64], y: &'a [f64], outputs: &'a [Prop]) -> Self {
        Self { pair, basis, x, y, outputs, flash: FlashOptions::new(), exec: ExecPolicy::Reference }
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
    #[cfg(feature = "rayon")]
    if chunk < n {
        let (points, outs) = (req.x.par_chunks(chunk).zip(req.y.par_chunks(chunk)), out.par_chunks_mut(chunk * m));
        let chunks = points.zip(outs.zip(status.par_chunks_mut(chunk * m)));
        let failed = chunks.map(|((xs, ys), (out, status))| run_chunk(fluid, req, xs, ys, out, status)).sum();
        return Ok(BatchSummary { points: n, failed_cells: failed });
    }
    // One chunk, or no rayon: the chunks in turn on the calling thread, with the boundaries rayon's would have.
    let (points, outs) = (req.x.chunks(chunk).zip(req.y.chunks(chunk)), out.chunks_mut(chunk * m));
    let chunks = points.zip(outs.zip(status.chunks_mut(chunk * m)));
    let failed = chunks.map(|((xs, ys), (out, status))| run_chunk(fluid, req, xs, ys, out, status)).sum();
    Ok(BatchSummary { points: n, failed_cells: failed })
}

/// Points per chunk, at most `n`: `Reference` takes the batch as one chunk; `Parallel` rounds its `chunk` up to a
/// multiple of 16 (K6), saturating at the largest multiple below `usize::MAX`.
fn chunk_points(exec: ExecPolicy, n: usize) -> usize {
    match exec {
        ExecPolicy::Parallel { chunk } => chunk.get().checked_next_multiple_of(16).unwrap_or(usize::MAX - 15).min(n),
        ExecPolicy::Reference => n,
    }
}

/// One chunk, point by point. Returns the failed-cell count.
fn run_chunk(
    fluid: &Fluid,
    req: &BatchRequest<'_>,
    xs: &[f64],
    ys: &[f64],
    out: &mut [f64],
    status: &mut [Status],
) -> usize {
    let m = req.outputs.len();
    let mut failed = 0;
    let rows = out.chunks_exact_mut(m).zip(status.chunks_exact_mut(m));
    for ((x, y), (out, status)) in xs.iter().zip(ys).zip(rows) {
        // The single gate (validation, basis, gauge), then the model: exactly the scalar path.
        let state = Input::new(req.pair, *x, *y, req.basis).and_then(|input| fluid.flash(input, &req.flash));
        let ok = if state.as_ref().is_ok_and(State::is_extrapolated) { Status::Extrapolated } else { Status::Ok };
        for ((o, s), prop) in out.iter_mut().zip(status.iter_mut()).zip(req.outputs) {
            let value = state.as_ref().map_err(Clone::clone).and_then(|st| fluid.prop(st, *prop));
            (*o, *s) = match value {
                Ok(v) => (v, ok),
                Err(e) => (f64::NAN, Status::from(&e)),
            };
            failed += usize::from(!matches!(*s, Status::Ok | Status::Extrapolated));
        }
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

    /// K6 (PLAN.md M9.1): `Parallel` chunks are a multiple of 16 points, rounded up from the policy's `chunk` and
    /// saturating below `usize::MAX`, and never longer than the batch; `Reference` is one chunk of the whole batch.
    #[test]
    fn parallel_chunks_are_a_multiple_of_16() {
        let parallel = |chunk| ExecPolicy::Parallel { chunk: NonZeroUsize::new(chunk).unwrap() };
        let cases = [(1, 16), (15, 16), (16, 16), (17, 32), (1000, 1008), (1024, 1024), (usize::MAX, usize::MAX - 15)];
        for (chunk, points) in cases {
            assert_eq!(chunk_points(parallel(chunk), usize::MAX), points, "chunk {chunk}");
        }
        assert_eq!((chunk_points(parallel(64), 40), chunk_points(parallel(1), 5)), (40, 5));
        assert_eq!(chunk_points(ExecPolicy::Reference, 5), 5);
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
            let parallel = ExecPolicy::Parallel { chunk: NonZeroUsize::new(chunk).unwrap() };
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
