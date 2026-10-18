# Caloric curves are starting points (2026-10-07)

Asked at M5.2a (PLAN.md §0.4 stop: the step's tests could pass only with another tolerance class). Degree-12 fits of
h, s, u along both saturation branches on the superancillary's pieces miss the EOS between nodes by up to 1.1e-6 in
the last piece below Tc (h, s, u go like (Tc − T)^β there) and by up to 1e-11 elsewhere (wide low-T pieces, where the
EOS's own rounding is that size); all 130 fluids exceed `SaCoeff` (1e-14) and 129 exceed 1e-12 (measured). The curves'
only consumers are the superancillary-based HQ, SQ, UQ and HS flashes (M7.7, M7.8).

| # | Question | Decision | Notes |
|---|---|---|---|
| CC2 | What accuracy the caloric curves carry | **Starting points on the shared pieces: class `CaloricFit` (2e-6), every answer polished with the EOS** | CC1 and the M2.11 contract stand (one set of pieces with the superancillary). Datagen refuses a fit that misses the EOS at a midpoint by more than `CaloricFit` (the measured 1.1e-6, rounded up); M7.7 and M7.8 polish with the EOS at (T, ρ_SA(T)), so their answers stay EOS-exact. A QT state's h′, s′, u′ come from the EOS at the SA densities, as the oracle computes them (class `Prop`). Rejected: own dyadic pieces to an exact class (amends M2.11, more bytes, deep splits near Tc); no fitted curves (undoes CC1). |
