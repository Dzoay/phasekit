# The multiprecision check points' pressure (2026-10-08)

Asked at M6.3, when 50 of the 390 VLE answers missed class `SatMp` (1e-11) in p while their ρ′ and ρ″ matched the
check points to about 1e-14. The check points' p (CoolProp's `check_points`, and fastchebpure's `p(mp)` in the M6.1
files) is not multiprecision: at each point's own T and ρ″ it differs from the vapour side's pressure (p ≈ ρ″RT at low
pressure, rounding ε-relative) by up to 0.76·ε·ρ′RT·(2A01′ + A02′), the rounding of a liquid-side pressure evaluated in
double precision. In 49 rows that is more than 1e-11 of p, 3.7e-9 for MethylLinolenate at Θ = 0.5; at its triple point
(260 K, fastchebpure) the gap is 2 %. The densities are multiprecision, and ρ″ is well conditioned (the liquid side's
rounding enters it as ε·(2A01′ + A02′)), so the true saturation pressure is computable in double precision.

| # | Question | Decision | Notes |
|---|---|---|---|
| DP1 | How the VLE's p is compared with the multiprecision check points | **Derive p: the reference p is the vapour side's pressure at the check point's own (T, ρ″); `SatMp` stays 1e-11 for T, p, ρ′ and ρ″** | The VLE at a given p solves at that derived p. `check_point_pressures_carry_liquid_side_rounding` pins the evidence (every row within 1·ε·ρ′RT·max(1, \|2A01′ + A02′\|) of the derived p; 49 rows beyond 1e-11), so a release that fixes the column changes the count and reopens this. M6.4's fastchebpure comparisons use the same rule (830 of their 18 875 rows are beyond 1e-11). Rejected: a carried bound on the arbiter's p, max(1e-11, ε·ρ′RT·K′/p), which keeps p independent of the EOS but reaches 4e-9 on heavy fluids at Θ = 0.5. |
