//! L0 exec: how a batch runs (D9). [`ExecPolicy`] and its chunking hold no physics (ARCHITECTURE.md §7), so a batch
//! hook on any layer can take a policy without pointing up (D1). Public as `batch::ExecPolicy` (S-03).

use core::num::NonZeroUsize;

#[cfg(feature = "rayon")]
use rayon::iter::{IndexedParallelIterator, ParallelIterator};
#[cfg(feature = "rayon")]
use rayon::slice::{ParallelSlice, ParallelSliceMut};

use crate::error::Error;

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

/// Points per chunk, at most `n`: `Reference` takes the batch as one chunk; `Parallel` rounds its `chunk` up to a
/// multiple of 16 (K6), saturating at the largest multiple below `usize::MAX`.
pub(crate) fn chunk_points(exec: ExecPolicy, n: usize) -> usize {
    match exec {
        ExecPolicy::Parallel { chunk } => chunk.get().checked_next_multiple_of(16).unwrap_or(usize::MAX - 15).min(n),
        ExecPolicy::Reference => n,
    }
}

/// Writes `f(x[i], y[i])` into `out[i]` for every point, in [`chunk_points`]' chunks: on rayon's current pool when the
/// `rayon` feature is on and there is more than one chunk, else in turn on the calling thread with the same boundaries.
/// Each point is evaluated alone, so the bits do not depend on the policy (K6). Unequal lengths are [`Error::Shape`].
pub(crate) fn points<O: Send>(
    exec: ExecPolicy,
    x: &[f64],
    y: &[f64],
    out: &mut [O],
    f: impl Fn(f64, f64) -> O + Sync,
) -> Result<(), Error> {
    for (expected, found) in [(x.len(), y.len()), (x.len(), out.len())] {
        if found != expected {
            return Err(Error::Shape { expected, found });
        }
    }
    if x.is_empty() {
        return Ok(()); // E12: no zero-sized chunks
    }
    let chunk = chunk_points(exec, x.len());
    let run = |((xs, ys), out): ((&[f64], &[f64]), &mut [O])| {
        for ((x, y), o) in xs.iter().zip(ys).zip(out) {
            *o = f(*x, *y);
        }
    };
    #[cfg(feature = "rayon")]
    if chunk < x.len() {
        x.par_chunks(chunk).zip(y.par_chunks(chunk)).zip(out.par_chunks_mut(chunk)).for_each(run);
        return Ok(());
    }
    x.chunks(chunk).zip(y.chunks(chunk)).zip(out.chunks_mut(chunk)).for_each(run);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
