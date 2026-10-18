# Ammonia's critical conductivity term (2026-10-12)

Asked during M8.9: Tufeu et al. 1984's critical term, as CoolProp codes it, diverges at its own T_c, 405.4 K, at every
density (CoolProp gives 6.26 W/(m K) at 405.3999 K and 50 kg/m³, NaN at 405.4 K). PLAN.md asked only for λ finite and
continuous there; the step chose how.

| # | Question | Decision | Notes |
|---|---|---|---|
| AM1 | How is Ammonia's λ kept finite at 405.4 K? | **Keep the step's floor: the term's reduced temperature is floored at the gap to the EOS's T_c, (405.56 − 405.4)/405.4** | CoolProp's value outside 405.24-405.56 K, finite and continuous inside (DIV-0030, ROT-111). The Corrected model (Monogenidou et al. 2018) remains the alternative after the paper check (map 05 Q3). |
