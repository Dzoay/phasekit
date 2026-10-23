# Warm start where it is honoured (2026-10-23)

Asked while M29.3 was built. EM18 has `NoConvergence` carry the last iterate so that a retry can pass it to
`FlashOptions::with_guess`, and EM15 lists `warm_start` as a remedy for every `NoConvergence`. But the guess is used only
by PT's density solve (a pure fluid's and a mixture's); PH, PS, the D+X, T+X and Q-pairs, HS and SU and the VLE ignore
it, so a retry with it would change nothing there.

| # | Question | Decision | Notes |
|---|---|---|---|
| EM19 | Where is `warm_start` a remedy, given that only PT's density solve takes the guess? | **Only where the guess is honoured: `NoConvergence` keeps `last` and `residual` wherever its solver has an iterate, but `remedies()` lists `warm_start` only for a solve that takes the guess (PT's density Newton), and `other_pair` alone for every other** | Not teaching every solver to seed from the guess, which would change each search's structure. Amends EM15's remedy for `NoConvergence`. |
