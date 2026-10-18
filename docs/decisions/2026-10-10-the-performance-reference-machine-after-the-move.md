# The performance reference machine after the move (2026-10-10)

Asked when work moved from the dev box to another, less powerful machine. P3 named the dev box (i7-8700K) as the
machine on which ARCHITECTURE.md §7's absolute targets are checked from M9; the C++ baseline and the recorded benches
are both keyed by CPU model, so the two machines' files never mix.

| # | Question | Decision | Notes |
|---|---|---|---|
| RM1 | Does the new machine replace the dev box as the performance reference? | **No: the i7-8700K dev box stays the reference machine, for now** | "this box can remain the reference for now". The new machine records its own C++ baseline (`cargo xtask baseline`, `coolprop-8.0.0-<machine>.csv`) in its own PR before it records any bench; its benches go to `results/<milestone>-<machine>.csv`, and a PR compares them with that machine's baseline only. The §7 targets stay absolute and are checked on the dev box at each milestone close from M9. To be revisited if the dev box goes. |
