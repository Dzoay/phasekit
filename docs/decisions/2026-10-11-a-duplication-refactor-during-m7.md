# A duplication refactor during M7 (2026-10-11)

Asked after a duplication audit of the workspace at `08a7226` (#90; jscpd 5.4.1 and similarity-rs 0.5.0, generated.rs
excluded): 1.3 % of lines are exact copies and 5.1 % of code differs only in names or literals, the hotspot being
phasekit-verify's tests (8.3 %). Six areas: the verify integration tests share no helper module (`label()` three times,
the pinned-generator-environment test four times, the paper-formula AD tests); the check routines of verify's `eos.rs`
and `term.rs`; the blob and pack framing in core's `blob.rs`; the per-family EOS term scaffolding in core, xtask
datagen and verify; parallel mapping tables (compat's CoolProp names, the Prop-to-state mapping); and two pairs of
xtask functions.

| # | Question | Decision | Notes |
|---|---|---|---|
| DR1 | Where does the refactor go? | **M7.9a, after the M7.2-M7.9 stack lands and before M7.10** | Mid-stack it would conflict with every stacked step; M7.10 adds verify check routines and test files, which would otherwise copy the duplicated preambles again. A refactor has no failing test: its gate is behaviour unchanged (PLAN.md M7.9a). |
| DR2 | Is the EOS term scaffolding (area 4) part of it? | **No: its own step, M7.9b, right after M7.9a** | The largest area, across three crates; a PR of its own keeps each reviewable. |
| DR3 | Split phasekit-core's `flash.rs` tests module, where every stacked M7 step appended its tests and rebases conflicted? | **Yes, in M7.9a** | One file per input-pair family under `flash/tests/` and the shared fixtures in `flash/tests.rs`; the tests unchanged. The paper-formula AD tests' scaffolding in verify's `tests/terms.rs` is area 4's, so M7.9b's (DR2). |
