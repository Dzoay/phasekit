# Mixtures: the dev profile (2026-10-18)

Asked at M13's close: phasekit-mix built at opt-level 0 in the dev profile (SP1 had optimised phasekit-core, xtask and
the dependencies), and its debug mixture tests took 50-106 s each (`tests/mixture_family.rs`, M13.15).

| # | Question | Decision | Notes |
|---|---|---|---|
| SP2 | Build phasekit-mix optimised in the dev profile, as SP1 does phasekit-core? | **Yes, opt-level 2** ("don't bother with before and after timings for the build profile. Just look at the difference in CI after the fact. We can always revert.") | `Cargo.toml`'s `[profile.dev.package.phasekit-mix]`, and so `test` and `mutants`, which inherit it. Floating-point results do not depend on opt-level (SP1); overflow checks and debug assertions stay on. Judged by CI's job times after the merge; reverted if they are worse. |
