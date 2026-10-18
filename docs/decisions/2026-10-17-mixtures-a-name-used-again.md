# Mixtures: a name used again (2026-10-17)

Asked at M13.13, which measured `PropsSI` at (T, p) on `R410A.mix` at 73 ms a call against v8.0.0's 4.4 ms (161 ms
against 8.4 on `R407C.mix`). Each call resolves the name to a new mixture, and the label of its single phase needs the
mixture's critical points (M13.8, ROT-009; v8.0.0 compares ρ with ρ_r(z) instead), which M13.7's search finds in 65 ms;
a fluid held across calls flashes the state in 0.78 ms. D8 had kept the library's interior mutability to one-shot
`OnceLock` and `LazyLock`.

| # | Question | Decision | Notes |
|---|---|---|---|
| MX9 | A mixture name used again | **A bounded name→fluid cache** ("the compatibility api containing a bounded name-fluid cache would be fine") | Kept by phasekit-mix's mixture-name resolver (`with_mixtures`), which compat hands every name the registry does not hold, so `PropsSI`, the C ABI, its shim and the browser package all use it (PLAN.md M13.14): the models of the last 64 names, the least recently used dropped first, a refusal never kept. Its one lock is the exception to D8 this decision grants; a model is immutable, so a cached one gives a fresh one's values bit for bit. *Not chosen now:* a cheaper search for the critical point the label needs. |
