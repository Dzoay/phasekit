# Order-3 outputs (2026-10-12)

Asked before M7.10, after a read of ARCHITECTURE.md §3.7, the oracle and the bench records.

| # | Question | Decision | Notes |
|---|---|---|---|
| OD1 | Which order-3 outputs does M7.10 implement? | **PLAN.md's: second partials over the 12 first-order variables, the fundamental derivative, κ_T and β; Cv, Cp, w, τ and δ as derivative variables deferred** | ARCHITECTURE.md §3.7's M7 row also listed those five; CoolProp's second derivatives refuse Cv, Cp and w (no oracle), and τ and δ need the model trait to expose its reducing constants, an API change of its own. PLAN.md §6 holds the follow-up, and ARCHITECTURE.md's row follows this decision. |
| OD2 | Their tolerances? | **Class `Prop`, with the order-3 bundle's `Term` scales carried through each relation; floors \|X\|/(\|Y\|·\|W\|), 1, 1/\|p\| and 1/T; the identities in class `Identity`** | No new class: the carried bound is absolute, from summed magnitudes, so a second partial that crosses zero needs no special case; the near-critical window as defined. A disagreement beyond it goes through the divergence register; no widening. |
| OD3 | Regenerate the committed fixtures for the new `eos` columns? | **Yes, through the nightly's `regenerate` dispatch (K18)** | The change to gen.py re-hashes every committed fixture, which only the pinned image may write. |
| OD4 | M7's exit gate asks for PT and PH benches: is this machine's record enough? | **Yes** | `results/M7-11th-gen-intel-core-i7-1165g7.csv` holds PT and PH (M7.3); the i7-8700K, the reference machine (RM1), recorded PT only (M7.2). The §7 targets are checked on the dev box from M9. |
| OD5 | Thorade & Saadat (2013), which VERIFICATION.md §8.1 named for M7's identities? | **Fetch it if it is open; otherwise the identities §8.1 itself lists** | It is not: Springer's copy is paywalled and the one public repository copy refuses an anonymous fetch. M7.10 checks the mixed second partial, Maxwell's relation differentiated once more, (∂c_v/∂ρ)_T, (∂c_p/∂p)_T and three κ_T and β relations. |
