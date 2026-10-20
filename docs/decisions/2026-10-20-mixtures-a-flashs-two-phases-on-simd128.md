# Mixtures: a flash's two phases on simd128 (2026-10-20)

Asked at M22.8, the browser build. Under `ExecPolicy::Simd` core's `batch::run_kernel` runs a batch at the CPU's best
level, its short last group padded with copies of its first point. M22.5's kernel hands it a flash's liquid and vapour,
two points. In the `simd` build the two fill a group of 4 lanes on AVX2, and in the browser's simd128 module a group of
8. Measured under Node on the quiet i7-8700K, the browser modules of main at 7495bbd, CO2/n-Propane at 40/60 (two of the
five embedded fluids): QT and PQ flashes at Q = 0, 0.5 and 1 from 230 to 270 K, PT inside and outside the dome, and QT +
h as one `Fluid.evaluate` call. Each figure is the mixture's default policy, `Simd`, against `Reference`, means of three
rounds:

| Module | QT | PQ | PT | QT batch |
|---|---|---|---|---|
| baseline (two lanes, as the default build) | 0.96× | 0.93× | 1.01× | 0.97× |
| simd128 (8 lanes, padded) | 1.15× | 1.13× | 1.01× | 1.16× |
| simd128, at most two points at two lanes | 1.08× | 1.04× | 1.00× | 1.05× |

The same rule in the `simd` build (two interleaved rounds of `mixture_flash`, R32/R125) takes its QT flash under
`Simd` from 1.01× to 0.92× of its `libm` scalar path, and PQ to 0.90×. On simd128 two lanes stay slower than `f64`
*(inference: the lane code the simd128 target vectorises; not profiled)*.

| # | Question | Decision | Notes |
|---|---|---|---|
| ML8 | How does M22.8 handle the simd128 module, where M22.5's kernel is slower than the scalar path? | **Scalar on simd128** | Under `Simd` core runs a batch of at most two points at two lanes, as a build without `simd` runs every batch, and on simd128 point by point (about `Reference`'s time), so the browser's flashes stop losing there; the `simd` build gains. Most browsers run the simd128 module. *Rejected:* two lanes everywhere, simd128 at 1.04-1.08× (the baseline module deciding, as ML6 has the default build decide); no core change, simd128 at 1.13-1.16× and the `simd` build at 1.01×. |
