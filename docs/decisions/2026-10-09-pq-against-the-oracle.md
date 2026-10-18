# PQ against the oracle (2026-10-09)

Asked at M6.8. CoolProp 8.0.0's PQ takes T straight from its T(ln p) inverse, with no polish on the forward p(T)
(FlashRoutines.cpp:1169, superancillary.h:1272-1274; map 03 §3.3), so its PQ states sit off its own superancillary:
over the 130 pure fluids (3 248 rows, p from its own QT), up to 2.0e-11 in T, 1.1e-6 in ρ and 1.3e-7 in h at
Θ ~ 1e-7 (R245fa), and still 4.5e-14 in T, 6.3e-13 in ρ and 7.4e-12 in h at Θ ~ 0.1. That is beyond `SaCoeff`'s 1e-14.
phasekit's PQ solves the curve's own p(T) = p to rounding (TOMS 748 within the inverse's 1e-9 bracket).

| # | Question | Decision | Notes |
|---|---|---|---|
| PQ1 | How M6.8 checks PQ | **Polish, and a divergence entry: DIV-0018 (`SkipOracle`) on the oracle's PQ cells (T, ρ′, ρ″, h′, h″, s′, s″); each PQ row is checked against the oracle's QT row at the temperature that generated its p, T within `SaCoeff`, and its state equals phasekit's QT at its T** | DIV-0018's proof pins CoolProp's PQ off its own curve. Rejected: copying CoolProp's unpolished inverse (phasekit's inverse is built independently to the same 1e-12 target, so PQ would still need a class of about 2e-11 in T and 1e-6 in ρ near Tc, a tolerance widened to fit CoolProp); keeping the polish and comparing at a new class sized to CoolProp's inverse error (also widened). |
