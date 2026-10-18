# Mixtures in lanes (2026-10-16)

Asked during M13 ("what part of the mixture calculation pipeline takes advantage of simd work?", then "lets plan this
as a follow-up"). Facts: no part does. Core's batch takes its lane path only for a `PureFluid` (`batch.rs`), and
`phasekit-mix` overrides neither `residual_batch` nor `ideal_batch`, so a mixture's batch runs point by point. Its
numerics are scalar: the multi-fluid residual (each component through its own `HelmholtzModel::residual` at the
mixture's τ and δ, then the departures), the composition derivatives on multivariate Taylor numbers, and the flashes'
density solves and Newton iterations. `Lanes<W>` and the lane `exp`, `ln` and `expm1` are crate-private in core
(ARCHITECTURE.md §7); `Real`, which `Lanes<W>` implements, is public and sealed and has no comparisons (§3.1).
Measured in release on the i7-8700K at a load average of about 28, so as proportions only (M22.1 records the baseline
on the quiet machine):

| Mixture | α^r, order 2 | its components' own α^r | composition derivatives, order 2 / 3 | PT flash |
|---|---|---|---|---|
| R32/R125 | 4.4 µs | 1.5 µs | 24 / 45 µs | 2.5 ms |
| Methane/Ethane | 7.3 µs | 3.4 µs | 16 / 34 µs | 19 ms |
| a five-component natural gas | 20.8 µs | 6.5 µs | 115 / 387 µs | 59 ms |

A flash's time goes to the composition derivatives, which its stability test and Newton iterations evaluate many times,
not to α^r alone; and of α^r much is the reducing functions and the departures, which a batch at fixed z computes once
and lanes respectively *(inference)*.

| # | Question | Decision | Notes |
|---|---|---|---|
| ML1 | Which parts of the mixture pipeline go into lanes? | **Batches in lanes, then a time-boxed spike on a flash's inner kernels** | A mixture's α^r and α⁰ batches at fixed z through the components' own lane kernels (M12), the reducing state once per batch and the departures in lanes; DT batches under a phase hint (`Strategy::Direct`); the `simd128` browser build. Then the spike: a flash's two phases, and its stability test's trial phases, evaluated in lanes in the composition derivatives, each kept only if faster (ML4). *Rejected:* batches only (flashes stay scalar); a scalar profiling step first, lanes decided from its baseline. |
| ML2 | When? | **After M13 closes (the `m13` tag)** | Core is frozen through M13 (MX2), and two steps change it: a runner for a family's kernels over `Real` (ML3) and a group hook on `ThermoModel` (ML5). Planned now as milestone M22, which does not move `MILESTONE`. *Rejected:* after `v0.1.0`; starting the parts that touch only `phasekit-mix` beside M13. |
| ML3 | How does a model crate reach core's lane math? | **Through core: the family writes its kernel over the public, sealed `Real`, and core runs it in its lanes, which stay crate-private** | Revised 2026-10-17, before this plan merged ("I want to avoid making lane public"). Beside `ExecPolicy` in `batch`, core adds one public trait whose one method is generic over `R: Real`, a pure function from a few numbers to a few, and one function that runs it over columns of `f64` under an `ExecPolicy`, at `Lanes<W>` inside core (D9: the family's math is the "what", core's executors the "how"). `Real` has no comparisons, so such a kernel cannot branch on a value, and each of its operations on lanes is `f64`'s bit for bit (K17, SL1): a kernel is bitwise its scalar evaluation by construction. The semver promise is that trait and that function, a minor version (D9); `Lanes<W>`, its widths, the dispatch and the lane `exp`, `ln` and `expm1` stay private, and `Real` stays sealed, so methods can still be added. The cubic family's α^r, already over `Real`, can use it (PLAN.md §4). *Rejected:* a public, semver-stable `phasekit_core::lanes` (the first answer, 2026-10-16); under `phasekit_core::internal`, doc-hidden with no semver promise (a family crate uses core's public API only, and a third-party family could not); kept private, the departures staying scalar. |
| ML4 | What decides whether a kernel ships? | **It ships if it is faster (D9)** | Measured on the i7-8700K against M22.1's scalar baseline, the same build; a kernel that is not faster is taken out and its measurements recorded. Targets are recorded, not enforced. Lanes stay bitwise equal to the scalar path (K17), so no result changes either way. *Rejected:* a required speedup that the gate enforces, as SL3 did for M12. |
| ML5 | How does core's batch reach a family's lanes? | **A group hook on `ThermoModel`: the flash of a group of points that core's batch has validated, by default each point's `flash`; core's batch calls it for every model under `ExecPolicy::Simd`** | Decided 2026-10-17 after a review of M22.4; replaces ML2's first answer, a batch lane path for any Helmholtz model under a phase hint. Core cannot build a mixture's state: a hinted DT on a `Mixture` builds it in `phasekit-mix` (its own limits, `State::from_total`, an external solve path), a `PureFluid`'s in core (a pressure check, the melting line), and `ThermoModel` has no method that takes a bundle. The hook keeps what a state is in the family (D3) and replaces the batch's `PureFluid` downcast; `PureFluid`'s hook is its M12 lane path, so its batches do not change. A provided method, so a minor version. *Rejected:* a narrower method, a single-phase state from a total bundle, that core's lane path and the family's own hinted DT both call. |
