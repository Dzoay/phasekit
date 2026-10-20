# Mixtures: which build decides a kernel (2026-10-19)

Asked at M22.5, the spike putting a flash's two phases in lanes. User decision ML4 ships a kernel if it is faster than
the scalar path in the same build, and does not say which build. Two builds run `ExecPolicy::Simd` differently. The
default build (std's maths, no features) runs it in two lanes at the target's baseline (M12.3). The `simd` build runs
it at the CPU's best level (four lanes a vector on AVX2) on `libm` maths (user decision SL1), whose `powf` runs per
lane through `math` until M23.4. M22.1-M22.4 were measured on the `simd` build. Measured on the quiet i7-8700K,
`mixture_flash` for R32/R125, µs per flash under `Reference` then `Simd`:

| Build | PT | QT | PQ |
|---|---|---|---|
| `simd` (`libm`, AVX2) | 5467 → 5485 | 653 → 658 | 686 → 690 |
| default (std, two lanes) | 3122 → 3117 | 411 → 369 (0.90×) | 463 → 419 (0.90×) |

The departures' `powf`, per lane in the `simd` build, would explain why that build gains nothing *(inference)*.

| # | Question | Decision | Notes |
|---|---|---|---|
| ML6 | Which build's measurement decides whether a kernel ships (ML4)? | **The default build's** | The two-phase kernel stays: the default build's QT and PQ flashes take 0.90× the scalar path's time, and the `simd` build's are unchanged within 1 %. M22.7's record measures the spike's kernels again, both builds, with M23.4's `pow` and its lane version if they have landed. *Rejected:* the `simd` build's, which M22.1-M22.4 were measured on (it would take the kernel out); leaving the choice to M22.7. |
