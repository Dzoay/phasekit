# The computed critical point (2026-10-09)

Asked at M6.7. The model's own critical point is solved from the EOS (Newton on K1 = 2A01 + A02 = 0 and
K2 = 2A01 + 4A02 + A03 = 0) and compared with CoolProp's computed columns (fastchebpure's multiprecision metadata).
T agrees to 2e-14 for every fluid but Chlorine and ρc within `Flash`'s near-critical 1e-6 for all but two:
DimethylCarbonate, 2.0e-6 off on a very flat critical isotherm (∂K2/∂ln ρ = 3.7e-5; phasekit's point satisfies K1 and
K2 to 1e-15, while at CoolProp's ρc phasekit's EOS gives K2 = 7.5e-11), and Chlorine, whose critical region is
degenerate (K1 and K2 within 1e-12 of 0 from 7950 to 8153 mol/m³ within 5e-5 K; the two points 2.6 % apart in ρ).

| # | Question | Decision | Notes |
|---|---|---|---|
| CR1 | How the critical point is checked | **The criticality conditions first: every solved model critical point satisfies K1 = K2 = 0 within `Flash`; the oracle's computed columns within `Flash`'s near-critical bounds, but for a divergence entry, DIV-0017 (`SkipOracle`), on Chlorine's and DimethylCarbonate's columns** | DIV-0017's proof pins the evidence (K2 at the oracle's points). Rejected: raising `Flash`'s near-critical ρ bound to 1e-5 with an entry for Chlorine alone. |
