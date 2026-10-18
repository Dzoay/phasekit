# Starting M12 before v0.1 (2026-10-13)

Asked when the user wanted the SIMD build started. M12's entry trigger is "v0.1 released", D9 rejected SIMD in v0.1,
and only M10.9 (the release) was left. ARCHITECTURE.md §7 also stated the SIMD gate twice: "≥ 2.5×" in its list, and
"≤ 0.1 µs/state and ≥ 2.5×" in its table. From n-Propane's 315.7 ns on the i7-8700K, the second needs about 3.2×.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL2 | When is M12 built? | **Now, landed after v0.1** | M12 is worked on the i7-8700K in its own worktree, its steps stacked. Its PRs merge to `main` only after `v0.1.0` is tagged, so the release carries no SIMD code and D9's rejection of SIMD in v0.1 stands. |
| SL3 | Which gate decides whether `simd` ships? | **≥ 2.5× decides; ≤ 0.1 µs/state is a §7 target** | ≥ 2.5× α^r batch throughput over the scalar path, on AVX2 on the reference machine, is the ship-or-stop gate (D9). ≤ 0.1 µs/state is checked by `gates perf --targets` like the other §7 targets: a miss names its re-plan issue in `ci/perf-exemptions.txt`, as #142-#144 do. |
