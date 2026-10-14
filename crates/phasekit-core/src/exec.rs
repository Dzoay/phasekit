//! L0 exec: how a batch runs (D9). [`ExecPolicy`] and its executors hold no physics (ARCHITECTURE.md §7), so a batch
//! hook on any layer can take a policy without pointing up (D1). Public as `batch::ExecPolicy` (S-03). A
//! [`LaneKernel`] says what a batch computes; [`run_scalar`], [`run_lanes`] and [`run_parallel`] say how (PLAN.md
//! M12.3).

use core::num::NonZeroUsize;

#[cfg(feature = "rayon")]
use rayon::iter::{IndexedParallelIterator, ParallelIterator};
#[cfg(feature = "rayon")]
use rayon::slice::{ParallelSlice, ParallelSliceMut};

#[cfg(feature = "simd")]
use fearless_simd::Level;

use crate::error::Error;
use crate::num::Lanes;

/// How a batch is executed. Chosen at run time; never changes results (features only add capability, K13).
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
    /// States side by side in lanes, on the calling thread (user decision SL5), where the batch has a lane kernel:
    /// α^r through the built-in EOS's `HelmholtzModel::residual_batch`. Other batches run the scalar path. With the
    /// `simd` feature the lanes run at the CPU's widest level, detected once per batch (8 on AVX-512, 4 on AVX2, 2
    /// below; PLAN.md M12.4); without it, two lanes at the target's baseline, a measured win over the scalar path
    /// (M12.3). Each lane is its state's scalar evaluation, and a short last group is padded with copies of one of its
    /// states (K17), so the bits are `Reference`'s.
    Simd,
}

/// Points per chunk, at most `n`: `Reference` and `Simd` take the batch as one chunk; `Parallel` rounds its `chunk` up
/// to a multiple of 16 (K6), saturating at the largest multiple below `usize::MAX`.
pub(crate) fn chunk_points(exec: ExecPolicy, n: usize) -> usize {
    match exec {
        ExecPolicy::Parallel { chunk } => chunk.get().checked_next_multiple_of(16).unwrap_or(usize::MAX - 15).min(n),
        ExecPolicy::Reference | ExecPolicy::Simd => n,
    }
}

/// What a batch computes ("what", ARCHITECTURE.md §7): one point, or W points side by side, where lane `k` of
/// [`lanes`](Self::lanes) is, bit for bit, [`point`](Self::point) at `(x[k], y[k])`.
pub(crate) trait LaneKernel: Sync {
    /// One point's result.
    type Out: Send;
    /// The point `(x, y)`.
    fn point(&self, x: f64, y: f64) -> Self::Out;
    /// W points side by side.
    fn lanes<const W: usize>(&self, x: Lanes<W>, y: Lanes<W>) -> [Self::Out; W];
}

/// `kernel` at every point `(x[i], y[i])` into `out[i]`, under `exec`: `Simd` through [`run_lanes`], the others
/// through [`run_scalar`] in [`run_parallel`]'s chunks. The bits do not depend on the policy (K6, K17). Unequal lengths
/// are [`Error::Shape`].
pub(crate) fn run<K: LaneKernel>(
    exec: ExecPolicy,
    kernel: &K,
    x: &[f64],
    y: &[f64],
    out: &mut [K::Out],
) -> Result<(), Error> {
    if !shape(x, y, out.len())? {
        return Ok(());
    }
    match exec {
        // With the `simd` feature, the CPU's best level, detected once per batch (PLAN.md M12.4); without it, two lanes
        // at the target's baseline, a measured win over the scalar path (M12.3).
        #[cfg(feature = "simd")]
        ExecPolicy::Simd => run_simd(Level::new(), kernel, x, y, out),
        #[cfg(not(feature = "simd"))]
        ExecPolicy::Simd => run_lanes::<K, 2>(kernel, x, y, out),
        ExecPolicy::Reference | ExecPolicy::Parallel { .. } => {
            run_parallel(chunk_points(exec, x.len()), x, y, out, |x, y, out| run_scalar(kernel, x, y, out));
        }
    }
    Ok(())
}

/// Writes `f(x[i], y[i])` into `out[i]` for every point, for a batch with no lane kernel: in [`chunk_points`]' chunks
/// through [`run_parallel`]; `Simd` runs it as `Reference`. Each point is evaluated alone, so the bits do not depend on
/// the policy (K6). Unequal lengths are [`Error::Shape`].
pub(crate) fn points<O: Send>(
    exec: ExecPolicy,
    x: &[f64],
    y: &[f64],
    out: &mut [O],
    f: impl Fn(f64, f64) -> O + Sync,
) -> Result<(), Error> {
    if !shape(x, y, out.len())? {
        return Ok(());
    }
    run_parallel(chunk_points(exec, x.len()), x, y, out, |xs, ys, out| {
        for ((x, y), o) in xs.iter().zip(ys).zip(out) {
            *o = f(*x, *y);
        }
    });
    Ok(())
}

/// Whether a batch of `x`, `y` and `out` (its length) has points to run: `Ok(false)` when it is empty (E12: no
/// zero-sized chunks), [`Error::Shape`] when the lengths differ.
fn shape(x: &[f64], y: &[f64], out: usize) -> Result<bool, Error> {
    for (expected, found) in [(x.len(), y.len()), (x.len(), out)] {
        if found != expected {
            return Err(Error::Shape { expected, found });
        }
    }
    Ok(!x.is_empty())
}

/// Every point through [`LaneKernel::point`], in turn.
pub(crate) fn run_scalar<K: LaneKernel>(kernel: &K, x: &[f64], y: &[f64], out: &mut [K::Out]) {
    for ((x, y), o) in x.iter().zip(y).zip(out) {
        *o = kernel.point(*x, *y);
    }
}

/// W points at a time through [`LaneKernel::lanes`]. A short last group is padded with copies of its first point
/// (K17), so every lane holds a state of the batch, and the copies' results are dropped.
#[inline(always)]
pub(crate) fn run_lanes<K: LaneKernel, const W: usize>(kernel: &K, x: &[f64], y: &[f64], out: &mut [K::Out]) {
    for ((xs, ys), out) in x.chunks(W).zip(y.chunks(W)).zip(out.chunks_mut(W)) {
        let (Some(&x0), Some(&y0)) = (xs.first(), ys.first()) else { continue };
        let lanes = |s: &[f64], pad: f64| Lanes::<W>(core::array::from_fn(|k| s.get(k).copied().unwrap_or(pad)));
        for (o, r) in out.iter_mut().zip(kernel.lanes(lanes(xs, x0), lanes(ys, y0))) {
            *o = r;
        }
    }
}

/// [`run_lanes`] under `level`'s target features (fearless_simd's `vectorize`, as its `dispatch!` calls it), as wide as
/// the level's vectors: 8 lanes on AVX-512, 4 on AVX2, 2 on SSE2, SSE4.2, NEON and WASM's simd128, and 2 without
/// target features at a fallback level (PLAN.md M12.4). The kernel is `#[inline(always)]` down to the lane
/// operations, so the level's instructions reach them; transcendentals still run per lane through `math`.
#[cfg(feature = "simd")]
pub(crate) fn run_simd<K: LaneKernel>(level: Level, kernel: &K, x: &[f64], y: &[f64], out: &mut [K::Out]) {
    use fearless_simd::Simd;
    match level {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        Level::Avx512(simd) => simd.vectorize(
            #[inline(always)]
            || run_lanes::<K, 8>(kernel, x, y, out),
        ),
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        Level::Avx2(simd) => simd.vectorize(
            #[inline(always)]
            || run_lanes::<K, 4>(kernel, x, y, out),
        ),
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        Level::Sse4_2(simd) => simd.vectorize(
            #[inline(always)]
            || run_lanes::<K, 2>(kernel, x, y, out),
        ),
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        Level::Sse2(simd) => simd.vectorize(
            #[inline(always)]
            || run_lanes::<K, 2>(kernel, x, y, out),
        ),
        #[cfg(target_arch = "aarch64")]
        Level::Neon(simd) => simd.vectorize(
            #[inline(always)]
            || run_lanes::<K, 2>(kernel, x, y, out),
        ),
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        Level::WasmSimd128(simd) => simd.vectorize(
            #[inline(always)]
            || run_lanes::<K, 2>(kernel, x, y, out),
        ),
        _ => run_lanes::<K, 2>(kernel, x, y, out),
    }
}

/// `run` on the batch in `chunk`-point pieces (`chunk` > 0): on rayon's current pool when the `rayon` feature is on and
/// there is more than one piece, else in turn on the calling thread with the same boundaries.
pub(crate) fn run_parallel<O: Send>(
    chunk: usize,
    x: &[f64],
    y: &[f64],
    out: &mut [O],
    run: impl Fn(&[f64], &[f64], &mut [O]) + Sync,
) {
    #[cfg(feature = "rayon")]
    if chunk < x.len() {
        x.par_chunks(chunk)
            .zip(y.par_chunks(chunk))
            .zip(out.par_chunks_mut(chunk))
            .for_each(|((x, y), o)| run(x, y, o));
        return;
    }
    x.chunks(chunk).zip(y.chunks(chunk)).zip(out.chunks_mut(chunk)).for_each(|((x, y), o)| run(x, y, o));
}

#[cfg(test)]
pub(crate) mod tests {
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
        assert_eq!(chunk_points(ExecPolicy::Simd, 5), 5);
    }

    /// Every dispatch level this CPU has, from [`Level::new`] down to the target's baseline (PLAN.md M12.4: CI forces
    /// each level its runners have).
    #[cfg(feature = "simd")]
    pub(crate) fn levels() -> Vec<Level> {
        let best = Level::new();
        let mut levels = vec![best];
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        levels.extend(
            [best.as_avx2().map(Level::Avx2), best.as_sse4_2().map(Level::Sse4_2), best.as_sse2().map(Level::Sse2)]
                .into_iter()
                .flatten(),
        );
        levels.dedup_by(|a, b| format!("{a:?}") == format!("{b:?}"));
        levels
    }

    /// A kernel whose result names its point, and which fails the test if a lane holds a state from outside the
    /// batch.
    struct Tagged<'a> {
        batch: &'a [f64],
    }

    impl LaneKernel for Tagged<'_> {
        type Out = (u64, u64);
        fn point(&self, x: f64, y: f64) -> (u64, u64) {
            (x.to_bits(), y.to_bits())
        }
        fn lanes<const W: usize>(&self, x: Lanes<W>, y: Lanes<W>) -> [(u64, u64); W] {
            for x in x.0 {
                assert!(self.batch.contains(&x), "lane state {x} is not in the batch");
            }
            core::array::from_fn(|k| (x.0[k].to_bits(), y.0[k].to_bits()))
        }
    }

    /// `run_lanes::<W>` over a batch of n points.
    fn lanes_of<const W: usize>(kernel: &Tagged<'_>, n: usize) -> Vec<(u64, u64)> {
        let (x, y): (Vec<f64>, Vec<f64>) = (0..n).map(|i| (kernel.batch[i], -kernel.batch[i])).unzip();
        let mut out = vec![(0, 0); n];
        run_lanes::<_, W>(kernel, &x, &y, &mut out);
        out
    }

    /// K17 (PLAN.md M12.3): `run_lanes` writes every point's own result, for W = 2, 4 and 8 and every batch length up
    /// to 17, so every remainder; a short last group's padding lanes hold states of the batch, never zeros or garbage.
    #[test]
    fn lanes_write_every_point_and_pad_with_batch_states() {
        let batch: Vec<f64> = (1..=17).map(|i| f64::from(i) * 1.5).collect();
        let kernel = Tagged { batch: &batch };
        for n in 0..=batch.len() {
            let want: Vec<(u64, u64)> = (0..n).map(|i| kernel.point(batch[i], -batch[i])).collect();
            assert_eq!(lanes_of::<2>(&kernel, n), want, "W = 2, n = {n}");
            assert_eq!(lanes_of::<4>(&kernel, n), want, "W = 4, n = {n}");
            assert_eq!(lanes_of::<8>(&kernel, n), want, "W = 8, n = {n}");
        }
    }

    /// PLAN.md M12.4: at every dispatch level of this CPU (8 lanes on AVX-512, 4 on AVX2, 2 below), `run_simd` writes
    /// every point's own result for every batch length up to 17, padding lanes with states of the batch.
    #[cfg(feature = "simd")]
    #[test]
    fn every_level_writes_every_point() {
        let batch: Vec<f64> = (1..=17).map(|i| f64::from(i) * 1.5).collect();
        let neg: Vec<f64> = batch.iter().map(|x| -x).collect();
        let kernel = Tagged { batch: &batch };
        for level in levels() {
            for n in 0..=batch.len() {
                let mut out = vec![(0, 0); n];
                run_simd(level, &kernel, &batch[..n], &neg[..n], &mut out);
                let want: Vec<(u64, u64)> = (0..n).map(|i| kernel.point(batch[i], neg[i])).collect();
                assert_eq!(out, want, "{level:?}, n = {n}");
            }
        }
    }

    /// A kernel whose result is the number of lanes it ran in.
    #[cfg(feature = "simd")]
    struct Width;

    #[cfg(feature = "simd")]
    impl LaneKernel for Width {
        type Out = usize;
        fn point(&self, _: f64, _: f64) -> usize {
            1
        }
        fn lanes<const W: usize>(&self, _: Lanes<W>, _: Lanes<W>) -> [usize; W] {
            [W; W]
        }
    }

    /// PLAN.md M12.4: `run_simd` runs each level as wide as its vectors, 8 lanes on AVX-512, 4 on AVX2 and 2 below. The
    /// widths change no bits (every lane is its state's scalar evaluation), so only this test can see them.
    #[cfg(feature = "simd")]
    #[test]
    fn each_level_runs_its_own_width() {
        for level in levels() {
            let name = format!("{level:?}");
            let want = if name.starts_with("Avx512") {
                8
            } else if name.starts_with("Avx2") {
                4
            } else {
                2
            };
            let mut out = [0; 9];
            run_simd(level, &Width, &[1.0; 9], &[1.0; 9], &mut out);
            assert_eq!(out, [want; 9], "{name}");
        }
    }

    /// PLAN.md M12.3: `run` under each policy writes every point's own result, `Simd` as the others; unequal lengths are
    /// a `Shape` error and an empty batch is `Ok`.
    #[test]
    fn every_policy_runs_the_kernel_at_every_point() {
        let batch: Vec<f64> = (1..=37).map(f64::from).collect();
        let kernel = Tagged { batch: &batch };
        let neg: Vec<f64> = batch.iter().map(|x| -x).collect();
        let want: Vec<(u64, u64)> = batch.iter().map(|x| kernel.point(*x, -x)).collect();
        let parallel = ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() };
        for exec in [ExecPolicy::Reference, parallel, ExecPolicy::Simd] {
            let mut out = vec![(0, 0); batch.len()];
            run(exec, &kernel, &batch, &neg, &mut out).unwrap();
            assert_eq!(out, want, "{exec:?}");
            assert!(matches!(run(exec, &kernel, &batch, &neg[1..], &mut out), Err(Error::Shape { .. })));
            assert!(matches!(run(exec, &kernel, &batch, &neg, &mut out[1..]), Err(Error::Shape { .. })));
            assert!(run(exec, &kernel, &[], &[], &mut []).is_ok());
        }
    }
}
