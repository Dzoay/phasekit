# The datagen tests' build (2026-10-11)

Asked after a review of the M7 loop's speed on the laptop: the 26 datagen tests in phasekit-xtask regenerate all 136
fluids each in an unoptimised build: 831 of the native suite's 1 018 test CPU-seconds (134 s wall on the i7-1165G7,
M7.5 stack).

| # | Question | Decision | Notes |
|---|---|---|---|
| SP1 | Build phasekit-xtask and the dependencies optimised in the dev profile (and so `test` and `mutants`)? | **Yes, opt-level 2, phasekit-core too, as a standalone PR before the next stacked step's gates** | Approved as small and low-risk for xtask and the dependencies; measured, that gave 134 s → 117 s, as the datagen tests spend their time in phasekit-core, so the user added phasekit-core: 44 s (335 test CPU-seconds). Floating-point results do not depend on opt-level; datagen's byte-for-byte tests and `gates datagen` check the generated files; debug assertions and overflow checks stay on. A core edit now rebuilds core optimised (a 20 s build against 11 s). The other workspace crates stay at opt-level 0. |
