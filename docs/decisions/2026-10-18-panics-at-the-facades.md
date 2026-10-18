# Panics at the facades (2026-10-18)

Asked at M13's close: issue #219, raised at M13.14, had left two pieces of panic hardening unscheduled, fault injection
at every C export and the browser package's panics, which wasm cannot catch.

| # | Question | Decision | Notes |
|---|---|---|---|
| PH1 | When to do issue #219 | **Before `v0.1.0`, now** ("do the fault injection and harden the C export / wasm code"), and the lints on phasekit-mix and phasekit-core too ("Yes, mix then core") | PLAN.md M24: fault injection along the C exports' paths, the browser package's panic hook, then clippy's `indexing_slicing` and `arithmetic_side_effects` denied in compat, capi and wasm, then phasekit-mix, then phasekit-core, which M13's close unfroze. |
