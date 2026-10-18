# Baseline gaps (2026-10-05)

Asked whether the plan compares speed, memory use and parallel execution: it does, but the CoolProp side of memory and
parallelism came only from Python-level figures (the wheel's per-state bytes, 0.97× on 4 threads under the GIL), and
the M1.15 C++ baseline timed one thread only.

| # | Question | Decision | Notes |
|---|---|---|---|
| BG1 | Close the gaps in the CoolProp baseline now? | **Yes: "close what gaps you can now"** | PLAN.md M1.15a: the C++ harness also records native memory (library first use, bytes per state, every fluid loaded) and thread scaling (1, 2, 4, 6, 12 threads; same fluid and mixed), beside the timing file; VERIFICATION.md §12. |
