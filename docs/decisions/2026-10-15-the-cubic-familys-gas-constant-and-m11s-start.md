# The cubic family's gas constant, and M11's start (2026-10-15)

Asked when M12 landed. Decision 10 split the R of a cubic's borrowed ideal-gas part by dataset: `Parity` the cubic's
own R, as CoolProp 8.0.0 does; `Corrected` the source model's R, so that α⁰ equals its paper. CoolProp's cubics use
`R_U_CODATA` = 8.31446261815324 J/(mol·K) for everything, the borrowed α⁰ included (map 06 C6, map 15 §2.1). The
borrowed models' own R values differ from it by up to 1.96e-5 (CycloPropane and Propyne, R = 8.3143), 1.10e-5 for
Water (IAPWS-95), and 1.128e-6 for the 58 cubic fluids with R = 8.314472.

| # | Question | Decision | Notes |
|---|---|---|---|
| CB1 | Which R does the cubic family use? | **CoolProp's: 8.31446261815324 (CODATA 2018, exact since the 2019 SI redefinition), everywhere and in both datasets** | Replaces decision 10. A cubic model's one `gas_constant()` is this R under `Parity` and `Corrected`; its α^r and the borrowed HEOS α⁰ are both scaled by it. The borrowed α⁰ is the source model's dimensionless function, unchanged, so a cubic fluid's ideal-gas c_p⁰, h⁰ and s⁰ are its source model's times R_CODATA/R_source. The register records that as an accepted divergence from the source papers, with the oracle kept (`KeepOracle`, like DIV-0005). |
| CB2 | When is M11 built? | **Now, on the i7-8700K, before `v0.1.0`** | The user's word, replacing PLAN.md §4's trigger "v0.1 released". M11's steps are planned first, then built and merged in order when green, as M12's were (RL4). |
