# Caloric curves (2026-10-06)

Asked at the start of M2 (M2.3 under way): ARCHITECTURE.md §8 step 5 and ROT-027 say datagen precomputes the caloric
curves (h, s, u along both saturation branches on the superancillary's pieces; CoolProp builds them lazily at first use,
45-63 ms per fluid behind a mutex, map 03 §6), but no PLAN.md step built them. The user asked to fill the gap as part of
M2. Computing the curves needs every residual and ideal-gas kind (M4.4) and the superancillary evaluator (M5.2), so the
numbers cannot exist in M2.

| # | Question | Decision | Notes |
|---|---|---|---|
| CC1 | Where the caloric-curve work goes | **Contract in M2, filled at M5.2a** | PLAN.md M2.11 (closes M2): the blob v1 `Caloric` section and layout, `FluidRecord::caloric`, the stamp that binds the curves to the EOS and the α⁰ gauge (exact R rescale and gauge shift, stale otherwise) and the datagen hook, tested on synthetic curves. PLAN.md M5.2a: datagen computes the curves for 130 fluids from the compiled EOS at the SA densities and fills the section; tested against the EOS between nodes, the oracle's `sat` rows and the gauge shift. M7.7 and M7.8 consume them. |
