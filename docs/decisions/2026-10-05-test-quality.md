# Test quality (2026-10-05)

Asked during M0.4: does anything check for useless tests, such as tautological tests that test nothing, before the
test-driven steps begin? Findings: `gates counts` catches tests that stop running and clippy catches `assert!(true)`
and `assert_eq!(x, x)`, but nothing caught a test without an assertion or `assert_eq!(f(), f())`, and VERIFICATION.md
§8.6 had deferred mutation testing past v0.1.

| # | Question | Decision | Notes |
|---|---|---|---|
| TQ1 | Checks for tests that cannot fail | **Both: a static test-shape gate and mutation testing** | `cargo xtask gates assertions`: every `#[test]` asserts (or is `#[should_panic]`) and no `assert_eq!`/`assert_ne!` compares an expression with itself; clippy's tautology lints are probed. `cargo-mutants` is a T4 tool (dependencies §3.1), not a crate dependency. Reverses VERIFICATION.md §8.6. PLAN.md M0.4a; ROT-294. |
| TQ2 | How strict mutation testing is | **Gate PRs on changed code** | `cargo xtask gates mutants` runs `cargo mutants --in-diff` over each PR's Rust changes: every mutant is caught, or excluded in `.cargo/mutants.toml` with a written reason. A full run weekly, report only (M0.6). |
