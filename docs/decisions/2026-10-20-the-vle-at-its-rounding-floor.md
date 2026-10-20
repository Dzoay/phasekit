# The VLE at its rounding floor (2026-10-20)

Asked when M23.7a (user decision IM9) found why fused α^r term sums stopped R411A's bubble point at 260.48 K converging
(issue #244): the stall is in every build, at the residual's rounding floor, and `fast` only changes which seed gets
lucky.

| # | Question | Decision | Notes |
|---|---|---|---|
| IM11 | M23.7a found the stall in every build, at the residual's rounding floor: how should the solve end there? | **When Newton's iterates stall at the floor, return the best of them if it meets the full fugacity gate** | R411A's feed liquid at 260.48 K and 3.035 bar has Z ≈ 0.009, so its p is the 1 % of 1 + δα^r_δ that the α^r_δ sum leaves: over ρ's next eight ulps the evaluated p moves by up to 6e-11 relative, against 8e-14 for its true slope (dln p/dln ρ = 670). The fugacity residual rounds to about 5e-13, the size of the solve's own gate (half of `FUGACITY_GATE`). From Wilson's seed every build cycles at 5.3e-13 for 60 steps. std and `libm` pass only because the split's seed lands at 3.9e-13, and under `fast` all three seeds cycle. The full gate (1e-12) is the one ROT-185 and the tests judge by; the half gate stays for every solve that does not stall. *Rejected:* stepping to the cycle's midpoint (a heuristic); compensated sums (every build's bits move); leaving the solver and unfusing `fast`'s sums. |
