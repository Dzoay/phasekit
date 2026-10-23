# Error remedies, reach and batch detail (2026-10-23)

Asked when the user took up issue #290, the four follow-ups of the review of M28.1's messages that EM11 left open
(the review asked whether each message is accurate, says what the caller can do, suits an agent and helps a simulator
searching at high call rates). The facts then: an `InvalidInput` that no state reaches says only "out of range",
naming neither the pair's other input nor the reachable range; `batch::evaluate` keeps a `Status` per cell and drops
the error; `NoConvergence` carries its strategy and iterations, not where the solver stopped, though
`FlashOptions::with_guess(T, ρ)` takes a warm start; `Error` is at most 48 bytes (`lib.rs`'s size assertion); and the
remedy for most refusals already exists in the API under a name that differs by facade (Rust's `FlashOptions`,
compat's imposed-phase strings, the browser package's options objects).

| # | Question | Decision | Notes |
|---|---|---|---|
| EM14 | Which of #290's follow-ups, and when? | **All four: remedy codes, the reachable bound and side, detail in batch results, and the wording fixes, as milestone M29** | "we want #290 fixes, all 4". The core is frozen from `m18-start` to M18's close (EM5), so M29's core steps land after `m18`. The wording is shown in full in the PRs for the user's review, as M28's was (EM2). |
| EM15 | How does core say what a caller can do? | **`Error::remedies()`, stable snake_case `Remedy` codes, which each facade writes in its own API's terms; for an unknown name, `Registry::nearest(name, k)`, which a facade calls to write "did you mean"** | The remedies: choose a root (`RootPolicy::Nearest`/`Stable`, or a phase hint), extrapolate (`DomainPolicy::Extrapolate`), warm-start (`FlashOptions::with_guess`), another input pair, the saturated phases' properties (or the quality), a pair the model declares, the nearest names. The error stays small and allocation-free: the nearest names are not in it. |
| EM16 | How is a value no state reaches reported? | **A new variant, `Error::Unreachable { quantity, value, side, bound, given }`, code `unreachable`, its batch `Status` `InvalidInput`; the bound is the extreme the solver reached at the given input (h_max at this T, say), and `given` the pair's other input as a quantity and a value** | `InvalidInput` keeps the values that break a rule (`Rule::OutOfRange` stays for a model's own refusals with no state to measure against). |
| EM17 | What does a batch keep for a refused point? | **Its `Error`: `batch::evaluate_detailed(…, errors: &mut [Option<Error>])`, one slot a point, no allocation on success; the C ABI and the browser package flatten each to a fixed record of its code, rule, side and up to four numbers (the value and its limit, or the roots)** | Not a flat record in core, which would drop what does not fit four numbers. |
| EM18 | Does `NoConvergence` say where the solver stopped? | **Yes: `last: Option<(T, ρ_molar)>` and `residual`, filled where the solver has an iterate, `None` (and NaN) where it has not; a retry passes `last` to `FlashOptions::with_guess`** | Across the 72 sites that build it. |
