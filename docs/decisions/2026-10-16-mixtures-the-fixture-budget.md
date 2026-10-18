# Mixtures: the fixture budget (2026-10-16)

Asked at M13.4, whose mixture fixtures took the committed set to 17.17 MB, over `gates fixtures`' 16 MiB (16.78 MB;
M13.3 was at 16.60 MB). M13.5's GERG-2008 fixtures (AGA8's check state and binary set, teqp's vectors, GERG TM15's
tables) add about 1.5 MB, and M13.6-M13.12's kinds (fugacity columns, `mix_flash`, `mix_sat`) perhaps 2 MB more. PLAN.md
§0.4 counts a looser gate as a re-plan.

| # | Question | Decision | Notes |
|---|---|---|---|
| MX8 | The committed fixtures' budget | **24 MiB** | VERIFICATION.md §3.6 and `gates fixtures`; about 3 MB of headroom after M13's close (about 21 MB). *Rejected:* 16 MiB with shrunken grids (fewer states per pair, or a 400-row core `eos` grid: weaker coverage, with more cuts to come); a separate 8 MiB mixture budget beside the 16 MiB one. |
