# The pseudo-pure dome against the nightly grid (2026-10-09)

Asked at M6.9, when the nightly DT grid (40 × 40 PT and 20 × 20 `dome` truths per fluid) first reached the six
pseudo-pure fluids below their critical temperature. Each blend's EOS critical point lies below its published one
(Air 131.87 K against 132.53 K, R407C 356.60 K against 359.35 K), so between them no VLE splits, where CoolProp labels
states by its ancillary density bands; and at SES36's lowest temperatures CoolProp's in-dome p changes with Q at a fixed
T, 1e-8 from phasekit's converged VLE.

| # | Question | Decision | Notes |
|---|---|---|---|
| PP3 | Which rule a pseudo-pure fluid's DT follows below its published Tc | **CoolProp's: gas below 0.95 of the dew density ancillary, liquid above 1.05 of the bubble one, liquid in the 0.9975 strip where the ancillary quality is below 0.01, p is above 1.05 of the bubble pressure ancillary and the state is stable; otherwise the EOS's VLE from the ancillary densities** | For the six pseudo-pure fluids only: their saturation is CoolProp's construct, and decision 4 asks for its rules from oracle fixtures; pure fluids keep the band-free rule (ROT-088). Rejected: the EOS's VLE then the ancillary densities as boundaries; the EOS alone with a divergence entry. |
| — | SES36's in-dome p at its lowest temperatures | **A divergence entry: DIV-0019 (`SkipOracle`) on those p cells** | Its proof pins CoolProp's variation with Q and phasekit's single VLE p. Rejected: an `Investigate` entry with the measured residual as the tolerance. |
