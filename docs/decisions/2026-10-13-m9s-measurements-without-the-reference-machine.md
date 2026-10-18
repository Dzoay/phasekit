# M9's measurements without the reference machine (2026-10-13)

Asked at the start of M9: M9.2 (thread scaling), M9.5 (the `libm` cost rule) and M9.6 (the §7 targets) measure on the
reference machine, the i7-8700K (P3, RM1), which cannot be reached from the laptop (i7-1165G7) where M9 is worked.

| # | Question | Decision | Notes |
|---|---|---|---|
| RM2 | Where are M9's measurements taken? | **Ratios on the laptop, the §7 targets on the i7-8700K** | M9.2's scaling and M9.5's libm/std ratios are relative measurements within one build on one machine, so the laptop records them (N = its 4 physical cores). M9.6's absolute check against ARCHITECTURE.md §7 waits for one command the user runs on the i7-8700K. RM1 otherwise stands. |
