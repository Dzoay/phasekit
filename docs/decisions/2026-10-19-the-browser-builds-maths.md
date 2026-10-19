# The browser builds' maths (2026-10-19)

Asked when M23.2 (#238, core's own `exp`) failed CI's `site` job (issue #239). The browser package's baseline build
(`phasekit-wasm` without features) ran std's maths, which on wasm32-unknown-unknown is the `libm` crate's
(compiler-builtins). Its `simd128` build enables `simd` and so `libm` (user decision SL1), which since M23.2 runs core's
own `exp`. The two builds had given the same bits (user decision SL8) only because std on wasm is the `libm` crate.
User decision IM1 keeps the in-house functions out of builds without `libm` until M23.6, and for good if they fail
M9.5's rule. So from M23.2 on, the site's benchmark found 129 of n-Propane's 1600 DT cells differing by about an ulp
between the builds.

| # | Question | Decision | Notes |
|---|---|---|---|
| IM6 | Which maths does the browser package's baseline build run while the in-house functions are only the `libm` builds'? | **The `libm` build's: `phasekit-wasm` enables `phasekit-core/libm`** | Both browser builds run core's own `exp`, `ln`, `pow` and `expm1` as M23.2-M23.5 land them, and the `libm` crate's for the rest, which is what std ran on wasm. So SL8's same bits hold as written, and IM1's outcome at M23.6 does not change the browser package. Measured on #238: the site's bench test passes, and the baseline `.wasm` grows from 1 689 190 to 1 724 227 B (+35 kB), within its budget (1 913 287 B, MX5). *Rejected:* the builds differing by up to an ulp a call until M23.6, or for good if IM1's rule fails, with SL8 and the bench test changed to "equal where both run the same maths" (weakens a check); holding M23.2-M23.5 back until M23.6 (delays them, and comes back if the rule fails). |
