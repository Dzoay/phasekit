# Allocation counting (2026-10-06)

Asked at M2.9: the plan's allocation tests (M2.9 `hot_get_allocates_nothing`, later M5.2a and M5.8) need a counting
`#[global_allocator]`, and implementing `GlobalAlloc` needs `unsafe impl`, which `unsafe_code = "forbid"` rules out in
every crate but `phasekit-capi`; no crate of the T3 list provides one.

| # | Question | Decision | Notes |
|---|---|---|---|
| AC1 | How allocations are counted | **allocation-counter as a dev-dependency of `phasekit-verify`** | allocation-counter 0.8 (MIT OR Apache-2.0), tier T3: dev-only, never shipped. It installs its own counting global allocator in the test binaries that use it, so the workspace stays free of `unsafe`. |
