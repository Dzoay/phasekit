# Every test run through cargo-nextest (2026-10-07)

Asked during the M5 landing, after PR #62's CI spent 4 hours in `gates mutants` (353 mutants, one job on a 4-vCPU
runner). Every mutant ran the whole workspace through `cargo test`, which runs the test binaries one after another: a
mutant caught by a late binary, or missed, paid for every earlier one, xtask's 51 s datagen tests included. Measured
here (i7-8700K, quiet, pre-built): the host suite 24.3 s with `cargo test`, 17.2 s with nextest and `cargo test --doc`;
wasip2 22.9 s and 10.9 s; M5.11's 7 mutants 8 min 17 s and 2 min 21 s (16-21 s of tests per mutant instead of
166-168 s), the same 7 caught.

| # | Question | Decision | Notes |
|---|---|---|---|
| TQ3 | Which test runner the gates use | **cargo-nextest for everything: G3, G4, `gates counts`, `gates mutants` (fail-fast), CI and the nightly sweep; doctests with `cargo test --doc`** | `.config/nextest.toml`: profile `default` stops at the first failure (the mutants gate), `ci` runs everything (G3, G4, CI), `counts` adds the JUnit report `gates counts` reads. The tool is pinned in CI (0.9.146) and checked by `scripts/check-toolchain.sh`. The mutants gate ends every running test at the first failure, and its test timeout is never shorter than a full workspace run (cargo-mutants timed its baseline on the mutated packages only, so uncaught mutants ran out of time and counted as caught). |
