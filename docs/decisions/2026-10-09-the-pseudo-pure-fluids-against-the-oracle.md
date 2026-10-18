# The pseudo-pure fluids against the oracle (2026-10-09)

Asked at M6.9, on the six pseudo-pure fluids' QT and PQ rows (path `ancillary`). CoolProp inverts a pressure ancillary
by Brent, which stops at about 1e-10 K (`Ancillaries.cpp:82-113`), so near the top of the range its PQ sides sit at
that T: 63 of 282 PQ rows miss `Prop` in density, by up to 4.1e-11 within Θ ≈ 2e-3 of the top, while its PQ T matches
phasekit's (an exact inversion) to 7.6e-13. Within Θ ≈ 1e-3 of the top the bubble-pressure ancillaries of Air and
R407C turn over, so some QT pressures have no root in the ancillary's fitted range (CoolProp then extrapolates by a
secant beyond it), and SES36's own QT fails there.

| # | Question | Decision | Notes |
|---|---|---|---|
| PP1 | How the pseudo-pure PQ sides are checked | **At the oracle's side temperature: PQ's T within `Prop`; at Q = 0 and Q = 1 the side's density, h and s within `Prop` from phasekit's EOS at the oracle's T and the given p; at Q = 0.5 T alone** | Like with like, no tolerance widened and no register entry. Rejected: a divergence entry exempting the side cells. |
| PP2 | The rows near the top where the definition breaks down | **Refuse and count: phasekit refuses (D6, a fit is never evaluated outside its range) with a `DomainError`, the test counts those rows, and the rows where the oracle itself errs are counted, not compared** | As M6.8 counts the rows outside the model's limits. Rejected: a divergence entry; a grid that stops at Θ = 1e-2. |
