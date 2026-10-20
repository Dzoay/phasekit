//! L0 exec: how a batch runs (D9). [`ExecPolicy`] and its executors hold no physics (ARCHITECTURE.md §7), so a batch
//! hook on any layer can take a policy without pointing up (D1). Public as `batch::ExecPolicy` (S-03). A
//! [`LaneKernel`] says what a batch computes; [`run_scalar`], [`run_lanes`] and [`run_parallel`] say how (PLAN.md
//! M12.3), and `dispatch` picks the lane widths. A family's [`RealKernel`] reaches the same lanes through
//! [`run_kernel`], public as `batch::RealKernel` and `batch::run_kernel`, while `Lanes<W>` and the executors stay
//! crate-private (PLAN.md M22.2; user decision ML3).

use core::num::NonZeroUsize;

#[cfg(feature = "rayon")]
use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
#[cfg(feature = "rayon")]
use rayon::slice::{ParallelSlice, ParallelSliceMut};

#[cfg(feature = "simd")]
use fearless_simd::Level;

use crate::error::Error;
use crate::num::{Lanes, Real};

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
    /// α^r and α⁰ through the built-in EOS's `HelmholtzModel::residual_batch` and `ideal_batch`, the superancillary's
    /// saturations through `SaturationCurve::at_t_batch` and `at_p_batch`, and, through `batch::evaluate`, the total
    /// bundles of a DT batch and the saturations and phases' bundles of a QT or PQ batch of a Helmholtz fluid (PLAN.md
    /// M12.8, M12.9), what a model's `ThermoModel::flash_many` evaluates side by side (a mixture's hinted DT, M22.4),
    /// and a family's [`RealKernel`] through `batch::run_kernel` (M22.2). Other batches, and the rest of each point's
    /// flash, run the scalar path. With the `simd` feature
    /// the lanes run at the CPU's widest level, detected once per batch, four vectors to a lane operation (32 lanes on
    /// AVX-512, 16 on AVX2, 8 below; PLAN.md M12.4, M12.7); without it, two lanes at the target's baseline, a measured
    /// win over the scalar path (M12.3). Each lane is its state's scalar evaluation, and a short last group is padded
    /// with copies of one of its states (K17), so the bits are `Reference`'s.
    Simd,
}

/// A family's kernel: a pure function from a few numbers to a few, written once over the sealed [`Real`], which
/// [`batch::run_kernel`](crate::batch::run_kernel) runs on `f64` point by point or, under [`ExecPolicy::Simd`], on
/// states side by side in core's lanes (PLAN.md M22.2; user decision ML3). `Real` has no comparisons, so a kernel cannot
/// branch on a value, and each of its operations on lanes is `f64`'s on every lane: lane k is the kernel's `f64` result
/// at point k, bit for bit (K17). Write it `#[inline(always)]` down to its `Real` operations, as core's kernels are, so
/// that the lanes' target features reach them.
pub trait RealKernel: Sync {
    /// The kernel at one point, or at a group of points side by side: reads `inputs` and writes `outputs`, one value per
    /// input and output column `run_kernel` is given, so their counts are the kernel's own, set at run time.
    fn eval<R: Real>(&self, inputs: &[R], outputs: &mut [R]);
}

/// `kernel` at every point k of the columns: `inputs[i][k]` in, `outputs[j][k]` written, under `exec`. `Reference`
/// evaluates it on `f64` point by point, `Parallel` the same in fixed chunks, and `Simd` in core's lanes at the CPU's
/// widest level, a short last group padded with copies of its first point; the bits do not depend on the policy (K6,
/// K17). The working columns are allocated once per call. Columns of unequal lengths are [`Error::Shape`].
pub fn run_kernel<K: RealKernel>(
    kernel: &K,
    inputs: &[&[f64]],
    outputs: &mut [&mut [f64]],
    exec: ExecPolicy,
) -> Result<(), Error> {
    let n = column_length(inputs, outputs)?;
    if n == 0 {
        return Ok(());
    }
    let mut columns = Columns { kernel, inputs, outputs };
    match exec {
        // As `run`: the CPU's best level with the `simd` feature, else two lanes at the target's baseline.
        #[cfg(feature = "simd")]
        ExecPolicy::Simd => dispatch(Level::new(), &mut columns),
        #[cfg(not(feature = "simd"))]
        ExecPolicy::Simd => columns.padded::<2>(),
        ExecPolicy::Reference | ExecPolicy::Parallel { .. } => columns.points(chunk_points(exec, n)),
    }
    Ok(())
}

/// The length of every column, or [`Error::Shape`] naming the first that differs from the first column's; 0 when there
/// are none.
fn column_length(inputs: &[&[f64]], outputs: &[&mut [f64]]) -> Result<usize, Error> {
    let mut lengths = inputs.iter().map(|c| c.len()).chain(outputs.iter().map(|c| c.len()));
    let expected = lengths.next().unwrap_or(0);
    match lengths.find(|found| *found != expected) {
        Some(found) => Err(Error::Shape { expected, found }),
        None => Ok(expected),
    }
}

/// [`run_kernel`]'s batch: a kernel and its columns, all of one length.
struct Columns<'a, 'b, K> {
    kernel: &'a K,
    inputs: &'a [&'a [f64]],
    outputs: &'a mut [&'b mut [f64]],
}

impl<K: RealKernel> Columns<'_, '_, K> {
    /// The kernel on `f64` point by point, in `chunk`-point pieces (`chunk` > 0): on rayon's current pool when the
    /// `rayon` feature is on and there is more than one piece, else in turn on the calling thread with the same
    /// boundaries, as [`run_parallel`] runs a pair of columns.
    fn points(&mut self, chunk: usize) {
        let mut pieces: Vec<Vec<&mut [f64]>> = Vec::new();
        for column in self.outputs.iter_mut() {
            for (k, piece) in column.chunks_mut(chunk).enumerate() {
                match pieces.get_mut(k) {
                    Some(piece_k) => piece_k.push(piece),
                    None => pieces.push(vec![piece]),
                }
            }
        }
        let (kernel, inputs) = (self.kernel, self.inputs);
        let run = |(k, mut outputs): (usize, Vec<&mut [f64]>)| {
            let from = chunk.saturating_mul(k);
            let inputs: Vec<&[f64]> = inputs.iter().map(|c| c.get(from..).unwrap_or_default()).collect();
            run_points(kernel, &inputs, &mut outputs);
        };
        #[cfg(feature = "rayon")]
        // Equivalent (M22.2): with `>=` a batch of one piece goes to rayon, which runs it inline on the calling thread:
        // the same thread and bits.
        // equivalent mutant: replace > with >=
        if pieces.len() > 1 {
            pieces.into_par_iter().enumerate().for_each(run);
            return;
        }
        pieces.into_iter().enumerate().for_each(run);
    }
}

impl<K: RealKernel> Widths for Columns<'_, '_, K> {
    #[cfg(any(test, all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))))]
    #[inline(always)]
    fn run<const W: usize, const V: usize>(&mut self) {
        let n = column_length(self.inputs, self.outputs).unwrap_or(0);
        let whole = whole_groups::<W>(n);
        run_groups::<K, W>(self.kernel, self.inputs, self.outputs, (0, whole));
        run_groups::<K, V>(self.kernel, self.inputs, self.outputs, (whole, n));
    }

    #[inline(always)]
    fn padded<const W: usize>(&mut self) {
        let n = column_length(self.inputs, self.outputs).unwrap_or(0);
        run_groups::<K, W>(self.kernel, self.inputs, self.outputs, (0, n));
    }
}

/// The points of `n` that fill whole groups of `W`.
#[cfg(any(test, all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))))]
#[inline(always)]
fn whole_groups<const W: usize>(n: usize) -> usize {
    n.saturating_sub(n.checked_rem(W).unwrap_or(0))
}

/// `kernel` on `f64` at every point of `outputs`' columns, in turn, from the same points of `inputs`' (columns at least
/// as long): the working values allocated once, the outputs zeroed before each point.
fn run_points<K: RealKernel>(kernel: &K, inputs: &[&[f64]], outputs: &mut [&mut [f64]]) {
    let n = outputs.first().map_or(0, |c| c.len());
    let (mut x, mut y) = (vec![0.0; inputs.len()], vec![0.0; outputs.len()]);
    for k in 0..n {
        for (x, column) in x.iter_mut().zip(inputs) {
            *x = column.get(k).copied().unwrap_or(f64::NAN);
        }
        y.fill(0.0);
        kernel.eval(&x, &mut y);
        for (column, y) in outputs.iter_mut().zip(&y) {
            if let Some(out) = column.get_mut(k) {
                *out = *y;
            }
        }
    }
}

/// `kernel` at `W` lanes over the points `from..to` of the columns, W at a time, a short last group padded with copies
/// of its first point (K17), whose results are dropped: the working lanes allocated once, the outputs zeroed before
/// each group.
#[inline(always)]
fn run_groups<K: RealKernel, const W: usize>(
    kernel: &K,
    inputs: &[&[f64]],
    outputs: &mut [&mut [f64]],
    (from, to): (usize, usize),
) {
    let (mut x, mut y) = (vec![Lanes([0.0; W]); inputs.len()], vec![Lanes([0.0; W]); outputs.len()]);
    for start in (from..to).step_by(W) {
        let group = (start, start.saturating_add(W).min(to));
        gather(inputs, group, &mut x);
        y.fill(Lanes([0.0; W]));
        kernel.eval(&x, &mut y);
        scatter(&y, group, outputs);
    }
}

/// The points `start..end` (at most W) of each input column into its lanes, the rest padded with the first point.
#[inline(always)]
fn gather<const W: usize>(inputs: &[&[f64]], (start, end): (usize, usize), x: &mut [Lanes<W>]) {
    for (lanes, column) in x.iter_mut().zip(inputs) {
        let group = column.get(start..end).unwrap_or_default();
        let first = group.first().copied().unwrap_or(f64::NAN);
        *lanes = Lanes(core::array::from_fn(|k| group.get(k).copied().unwrap_or(first)));
    }
}

/// Each output's lanes into the points `start..end` of its column; the padding lanes are dropped.
#[inline(always)]
fn scatter<const W: usize>(y: &[Lanes<W>], (start, end): (usize, usize), outputs: &mut [&mut [f64]]) {
    for (lanes, column) in y.iter().zip(outputs.iter_mut()) {
        for (out, value) in column.get_mut(start..end).unwrap_or_default().iter_mut().zip(lanes.0) {
            *out = value;
        }
    }
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

/// A batch whose lane widths the CPU's level picks at run time: its whole groups of `W` points side by side, then the
/// rest `V` at a time, so a short batch does not evaluate `W` lanes for a few states (PLAN.md M12.7). Every lane is its
/// state's scalar evaluation, whatever its width.
pub(crate) trait Widths {
    /// The batch at `W` lanes over its whole groups of W points, then at `V` over the rest: on the levels of x86 and
    /// aarch64 with `simd`, which pick W and V (simd128 and the fallback run [`padded`](Self::padded)), and in core's
    /// tests at widths of their own.
    #[cfg(any(test, all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))))]
    fn run<const W: usize, const V: usize>(&mut self);
    /// The batch at `W` lanes throughout, its short last group padded: one copy of the kernel's code, where
    /// [`run`](Self::run) inlines two.
    fn padded<const W: usize>(&mut self);
}

/// [`run`]'s batch under `Simd`: a lane kernel at the points `(x[i], y[i])` into `out[i]`.
#[cfg(feature = "simd")]
struct Pairs<'a, K: LaneKernel> {
    kernel: &'a K,
    x: &'a [f64],
    y: &'a [f64],
    out: &'a mut [K::Out],
}

#[cfg(feature = "simd")]
impl<K: LaneKernel> Widths for Pairs<'_, K> {
    /// [`run_lanes`] at `W` lanes over the whole groups, then at `V` over the rest.
    #[cfg(any(test, all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))))]
    #[inline(always)]
    fn run<const W: usize, const V: usize>(&mut self) {
        // Not compiled in the gate's run (M12.7): it needs `simd`; with it, `every_level_writes_every_point` catches
        // this.
        // equivalent mutant: replace <impl Widths for Pairs<'_, K>>::run with ()
        let whole = whole_groups::<W>(self.x.len());
        let (Some((xw, xr)), Some((yw, yr)), Some((ow, or))) =
            (self.x.split_at_checked(whole), self.y.split_at_checked(whole), self.out.split_at_mut_checked(whole))
        else {
            return;
        };
        run_lanes::<K, W>(self.kernel, xw, yw, ow);
        run_lanes::<K, V>(self.kernel, xr, yr, or);
    }

    /// [`run_lanes`] at `W` lanes over the whole batch.
    #[inline(always)]
    fn padded<const W: usize>(&mut self) {
        // Not compiled in the gate's run (M22.2): it needs `simd`. With it, only the fallback level and simd128 run it,
        // and `every_level_writes_every_point` catches it on simd128 (CI's wasip2 lane step).
        // equivalent mutant: replace <impl Widths for Pairs<'_, K>>::padded with ()
        run_lanes::<K, W>(self.kernel, self.x, self.y, self.out);
    }
}

/// [`run_lanes`] over the batch at the widths of `level` ([`dispatch`]).
#[cfg(feature = "simd")]
pub(crate) fn run_simd<K: LaneKernel>(level: Level, kernel: &K, x: &[f64], y: &[f64], out: &mut [K::Out]) {
    // Not compiled in the gate's run (M12.4, M12.7): this needs core's `simd` feature, which the run does not enable,
    // so no test there can fail. With the feature, `each_level_runs_its_own_width` and
    // `every_level_writes_every_point` catch it.
    // equivalent mutant: replace run_simd with ()
    dispatch(level, &mut Pairs { kernel, x, y, out });
}

/// `job` under `level`'s target features (fearless_simd's `vectorize`, as its `dispatch!` calls it), four vectors to a
/// lane operation: 32 lanes on AVX-512, 16 on AVX2, 8 on SSE2, SSE4.2, NEON and WASM's simd128 (measured on the i7-8700K
/// at M12.7: independent multiply-add chains fill the pipelines that one vector's latency leaves idle; eight vectors run
/// out of registers), and one vector over the rest. simd128 runs 8 lanes throughout, its short last group padded: under
/// Node that module is 77 KB smaller than with a one-vector remainder, and faster (M12.10, user decision SL8). A
/// fallback level runs 2 lanes without target features. The batch is `#[inline(always)]` down to the lane operations,
/// so the level's instructions reach them; transcendentals other than `exp` and `ln` still run per lane through `math`.
#[cfg(feature = "simd")]
pub(crate) fn dispatch(level: Level, job: &mut impl Widths) {
    // Not compiled in the gate's run (M22.2): this needs core's `simd` feature. With the feature,
    // `each_level_runs_its_own_width` catches it.
    // equivalent mutant: replace dispatch with ()
    use fearless_simd::Simd;
    match level {
        // AVX-512 only, which neither the reference machine nor CI's runners have.
        // equivalent mutant: delete match arm Level::Avx512(simd)
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        Level::Avx512(simd) => simd.vectorize(
            #[inline(always)]
            || job.run::<32, 8>(),
        ),
        // Caught with `simd` (`each_level_runs_its_own_width`), which the gate's run does not enable.
        // equivalent mutant: delete match arm Level::Avx2(simd)
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        Level::Avx2(simd) => simd.vectorize(
            #[inline(always)]
            || job.run::<16, 4>(),
        ),
        // Caught with `simd` (`each_level_runs_its_own_width`), which the gate's run does not enable.
        // equivalent mutant: delete match arm Level::Sse4_2(simd)
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        Level::Sse4_2(simd) => simd.vectorize(
            #[inline(always)]
            || job.run::<8, 2>(),
        ),
        // Caught with `simd` (`each_level_runs_its_own_width`), which the gate's run does not enable.
        // equivalent mutant: delete match arm Level::Sse2(simd)
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        Level::Sse2(simd) => simd.vectorize(
            #[inline(always)]
            || job.run::<8, 2>(),
        ),
        // NEON exists only on aarch64, where the width test catches it (the aarch64 runner).
        // equivalent mutant: delete match arm Level::Neon(simd)
        #[cfg(target_arch = "aarch64")]
        Level::Neon(simd) => simd.vectorize(
            #[inline(always)]
            || job.run::<8, 2>(),
        ),
        // simd128 exists only on wasm32, where the width test catches it (wasip2 with simd128).
        // equivalent mutant: delete match arm Level::WasmSimd128(simd)
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        Level::WasmSimd128(simd) => simd.vectorize(
            #[inline(always)]
            || job.padded::<8>(),
        ),
        _ => job.padded::<2>(),
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
    // Equivalent (M12.1, M12.3): with `<=` a batch of one chunk goes to rayon, which runs it inline on the calling
    // thread: the same thread and bits.
    // equivalent mutant: replace < with <=
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

    /// PLAN.md M12.4, M12.7: at every dispatch level of this CPU, `run_simd` writes every point's own result for every
    /// batch length up to 40 (whole groups of 16 on AVX2 and the rest 4 wide), padding lanes with states of the batch.
    #[cfg(feature = "simd")]
    #[test]
    fn every_level_writes_every_point() {
        let batch: Vec<f64> = (1..=40).map(|i| f64::from(i) * 1.5).collect();
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

    /// PLAN.md M12.4, M12.7: `run_simd` runs each level four vectors to a lane operation (32 lanes on AVX-512, 16 on
    /// AVX2, 8 below) over the batch's whole groups, and one vector wide (8, 4, 2) over the rest; WASM's simd128 runs 8
    /// lanes throughout, its short last group padded (M12.10, user decision SL8). The widths change no bits (every lane
    /// is its state's scalar evaluation), so only this test can see them.
    #[cfg(feature = "simd")]
    #[test]
    fn each_level_runs_its_own_width() {
        for level in levels() {
            let name = format!("{level:?}");
            let (wide, vector) = match name.split('(').next() {
                Some("Avx512") => (32, 8),
                Some("Avx2") => (16, 4),
                Some("WasmSimd128") => (8, 8),
                Some("Fallback") => (2, 2),
                _ => (8, 2),
            };
            let mut out = [0; 43];
            run_simd(level, &Width, &[1.0; 43], &[1.0; 43], &mut out);
            let whole = 43 - 43 % wide;
            let want: Vec<usize> = (0..43).map(|i| if i < whole { wide } else { vector }).collect();
            assert_eq!(out.to_vec(), want, "{name}");
        }
    }

    /// A kernel over `Real` whose every output is the number of lanes it ran in: 1 on `f64`.
    struct RealWidth;

    impl RealKernel for RealWidth {
        fn eval<R: Real>(&self, _: &[R], outputs: &mut [R]) {
            for out in outputs {
                *out = R::from_f64((core::mem::size_of::<R>() / 8) as f64);
            }
        }
    }

    /// The lanes `job` ran each of 43 points in, through [`RealWidth`] into two output columns, which must agree.
    fn real_widths(job: impl FnOnce(&mut Columns<'_, '_, RealWidth>)) -> Vec<f64> {
        let (x, mut a, mut b) = ([1.0; 43], [0.0; 43], [0.0; 43]);
        job(&mut Columns { kernel: &RealWidth, inputs: &[&x], outputs: &mut [&mut a, &mut b] });
        assert_eq!(a, b, "the two output columns differ");
        a.to_vec()
    }

    /// PLAN.md M22.2: a family's kernel over `Real` runs its whole groups of W points at W lanes and the rest at V, as
    /// `run_simd` does a lane kernel, here at 4 and 2 lanes, or at W throughout, padded; on `f64` point by point under
    /// `Reference` and `Parallel`; and at every dispatch level of this CPU at its own widths
    /// (`each_level_runs_its_own_width`).
    #[test]
    fn real_kernels_run_whole_groups_then_the_rest() {
        let want = |wide: usize, vector: usize| -> Vec<f64> {
            (0..43).map(|i| if i < 43 - 43 % wide { wide as f64 } else { vector as f64 }).collect()
        };
        assert_eq!(real_widths(|job| job.run::<4, 2>()), want(4, 2));
        assert_eq!(real_widths(|job| job.padded::<4>()), vec![4.0; 43]);
        let parallel = ExecPolicy::Parallel { chunk: NonZeroUsize::new(16).unwrap() };
        for exec in [ExecPolicy::Reference, parallel] {
            let got = real_widths(|job| job.points(chunk_points(exec, 43)));
            assert_eq!(got, vec![1.0; 43], "{exec:?}");
        }
        #[cfg(feature = "simd")]
        for level in levels() {
            let (wide, vector) = match format!("{level:?}").split('(').next() {
                Some("Avx512") => (32, 8),
                Some("Avx2") => (16, 4),
                Some("WasmSimd128") => (8, 8),
                Some("Fallback") => (2, 2),
                _ => (8, 2),
            };
            assert_eq!(real_widths(|job| dispatch(level, job)), want(wide, vector), "{level:?}");
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
