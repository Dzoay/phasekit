# The superancillary against the VLE, and the VLE near Tc (2026-10-09)

Asked at M6.4, on fastchebpure's dense files for the core subset's 12 fluids (18 875 rows down to Θ = 0). The
superancillary evaluated here equals the files' own, misfit for misfit. The VLE matches the multiprecision densities to
about 1e-13 far from Tc (2.5e-13 at Θ ~ 0.1), which is above the 1e-14 floor of `SaFit`, so the planned direct check of
the superancillary against the VLE within `SaFit` fails through no fault of the superancillary. Near Tc, g′ = g″ is
solved on an almost flat Gibbs surface and the double-precision VLE's densities carry its rounding amplified as
Θ^−1.5: within 2.2·ε·Θ^−1.5 of the multiprecision values (p within 1.5·ε/Θ), inside `SatMp`'s 1e-11 down to Θ ≈ 1e-3
and inside the inferred near-critical 1e-6 down to Θ ≈ 3e-7, but 7.8e-5 at Θ ≈ 1e-7 and 2e-3 at Θ ≈ 1e-8, where the
superancillary itself misfits by up to 1e-4.

| # | Question | Decision | Notes |
|---|---|---|---|
| SV1 | How M6.4 compares the superancillary and the VLE | **Each against the multiprecision values: the superancillary within `SaFit` on every dense row, the VLE within `SatMp` (ρ′, ρ″, and p derived as DP1 decides)** | Their agreement follows within `SaFit` + `SatMp`; every class keeps its definition. Rejected: the direct comparison bounded by `SaFit` + `SatMp` (one check mixing two classes); `SaFit` with a 1e-12 floor for this comparison only. |
| NC1 | How `SatMp` treats near-critical rows | **A conditioning bound: ρ max(1e-11, 4·ε·Θ^−1.5), p and T max(1e-11, 4·ε/Θ), Θ = (Tc − T)/Tc, on every row; the measured constants (2.2, 1.5) pinned by a test** | Replaces the inferred 1e-6 for Θ < 1e-3 (VERIFICATION.md §5 said it would be measured at M6): tighter down to Θ ≈ 3e-7, looser closer in, like TC1 carries `Term` through cancelling relations. Below Θ = 1e-8 the VLE is not compared: the bound there exceeds 1e-3 of ρ, and over all 130 fluids it refuses some rows below Θ = 6e-9. Rejected: keeping 1e-6 and comparing only Θ ≥ 1e-6. Reaching multiprecision accuracy near Tc would take extended precision in the near-critical solve; PLAN.md §4 records it as an idea, at the user's request. |
