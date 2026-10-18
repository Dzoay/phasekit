# The near-critical bound across all 130 fluids, and an arbiter divergence (2026-10-09)

Asked at M6.4, when the nightly sweep over all 130 fluids (204 050 rows) ran locally. NC1's constant had been measured
on the 12 core fluids (2.2); 129 fluids stay within 3.5, but R22 needs 22.5 in ρ and 23.5 in p (7e-8 at Θ = 6e-6):
its EOS carries far more rounding in g near Tc. And PropyleneGlycol's dense file disagrees with the v8.0.0 EOS below
227.6 K, the first interval of fastchebpure's fit: ρ″ 0.7 % off at 213 K. phasekit's VLE and CoolProp 8.0.0's own (with
the superancillaries off) agree with each other there to 6e-15.

| # | Question | Decision | Notes |
|---|---|---|---|
| NC2 | How NC1's bound covers fluids whose EOS rounds more | **Carry the `Term` scale: ρ max(1e-11, 4·ε·Θ^−1.5·μ), p and T max(1e-11, 4·ε·μ/Θ), μ = max(1, (M00′ + M01′)/(1 + \|g′/RT\|)), the liquid's `Term` majorant of g relative to g** | As TC1 carries `Term` for properties. μ is 404 for R22 near Tc, 233 for Methanol, 2 to 3 for simple fluids; measured, every fluid is within 0.39·ε·Θ^−1.5·μ in ρ and 0.38·ε·μ/Θ in p. Rejected: a constant of 32 for every fluid (looser for the well-behaved ones); an R22 exception. |
| — | PropyleneGlycol's dense rows below 227.6 K | **A divergence entry: DIV-0016 (`SkipOracle`)** | The check skips the p and ρ″ cells of those rows by citing it; its proof pins CoolProp's VLE values and the remaining gap. Rejected: skipping the rows without an entry. |
