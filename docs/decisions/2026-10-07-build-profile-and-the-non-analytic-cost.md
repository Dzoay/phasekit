# Build profile and the non-analytic cost (2026-10-07)

Asked at M5.11, after the DT bench. Water's α^r (order 2) took 5.65 µs against C++ CoolProp's 2.7 µs; its 2
non-analytic terms took 4.65 µs of it (the other 54 terms 1.02 µs), because `NonAnalyticBlock` evaluates them on `Jet4`
at order 4 whatever order is asked (order 2 and order 4 cost the same). The workspace had no `[profile]`: opt-level 3,
16 codegen units, no LTO. Measured on the i7-8700K: fat LTO with one codegen unit takes the 54 ordinary terms from
1.02 to 0.71 µs and leaves the non-analytic ones at 4.6 µs; `target-cpu=native` adds about 10 % on those.

| # | Question | Decision | Notes |
|---|---|---|---|
| BP1 | The workspace's build profile | **`[profile.release]` and `[profile.bench]`: `lto = "fat"`, `codegen-units = 1`; no `target-cpu`** | Applies to what the workspace builds itself (benches, and the C library, wheel and wasm module from M10); dependents use their own profile. Bench results from M5.11 on are measured with it; the M2 and M3 rows were not. |
| NA1 | When to make the non-analytic block order-aware | **M9 (step M9.2a), not M5** | Before the `libm` cost rule (M9.5) and the performance gates (M9.6) measure the kernel. |
