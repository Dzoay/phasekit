# c_p, c_v and w of a saturated phase (2026-10-13)

Asked after M8.10, which reads η and λ at Q = 0 and 1 from the saturated phase, while c_p, c_v and w stayed refused for
every two-phase state (M5.3). CoolProp gives the saturated phase's: its w reads that phase at Q = 0 and 1, and its c_p
and c_v evaluate the EOS at the mean density, which there is that phase's (Water, CO₂ and Nitrogen identical to the
single phase at the saturated density).

| # | Question | Decision | Notes |
|---|---|---|---|
| SQ1 | Do c_p, c_v and w at Q = 0 and 1 come from the saturated phase? | **Yes, to match CoolProp** | The liquid's at Q = 0, the vapour's at Q = 1, at its own T (E3). Strictly inside the dome they stay `Undefined`, where CoolProp's c_p and c_v are the EOS at the mean density (Water at QT(0.5, 500 K): 35 464 J/(kg K)) (ROT-010, DIV-0004). Asked for these three only: the partial derivatives and the order-3 outputs stay refused at Q = 0 and 1. |
| SQ2 | Do the partial derivatives and the order-3 outputs at Q = 0 and 1 follow c_p, c_v and w? (asked after SQ1 landed) | **Yes, the saturated phase's** | The first and second partials, κ_T, β and Γ read the liquid at Q = 0 and the vapour at Q = 1, the order-3 bundle at that phase's T and ρ. CoolProp's are the single phase's there, bit for bit (Water at 500 K, CO₂ at 280 K, Air at 101 325 Pa). Strictly inside the dome they stay `Undefined`, where CoolProp evaluates the EOS at the mean density (CO₂ at QT(0.5, 280 K): κ_T −1.71e-7 1/Pa) and refuses Γ (ROT-010, DIV-0004). Supersedes SQ1's note that they stay refused. |
| SQ3 | Do c_p⁰ and the residual h, s and g at Q = 0 and 1 follow too? (asked on SQ2's pull request) | **Yes, the same way** | The residual h, s and g read the saturated phase at Q = 0 and 1, CoolProp's there bit for bit, and stay `Undefined` strictly inside the dome, where CoolProp's are the EOS at the mean density (Water at QT(0.5, 500 K): h^r −6 256 J/mol, against its phases' −35 254 and −2 340) (ROT-010, DIV-0004). c_p⁰, a function of T alone, is given in the dome too, at its T, as CoolProp's. |
