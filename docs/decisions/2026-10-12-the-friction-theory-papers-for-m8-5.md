# The friction-theory papers for M8.5 (2026-10-12)

Asked during M8.5: its failing tests name friction-theory paper rows for Methane, n-Pentane, SF₆ and H₂S. SF₆'s and
H₂S's papers print check values, and the step checks them; Methane's and n-Pentane's correlation is
Quiñones-Cisneros and Deiters, J. Phys. Chem. B 110, 12820 (2006), which is paywalled, and CoolProp's tests have no
rows for them.

| # | Question | Decision | Notes |
|---|---|---|---|
| FT1 | Hold M8.5 for that paper, or land it without Methane's and n-Pentane's paper rows? | **Land M8.5; the two fluids' rows wait for the paper** | Both fluids still match the oracle stage by stage. The paper is also the arbiter of n-Pentane's viscosity itself: CoolProp replaced two of its coefficients (`Aa[1]`, `Ai[0]`) with REFPROP 9.1's, and at Vassiliou et al. 2015's check point its η is 27.15 µPa s where that correlation used 49.465 (M8.6, DIV-0025). A copy that reaches `reference/papers/` adds the rows. |
