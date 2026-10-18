# SIMD and the transcendentals (2026-10-13)

Asked after a review of M12 against the code. M12 promised lanes bitwise equal to the scalar path, an in-house vector
exp/ln, and features that never change numerics (K13). The scalar path's exp and ln are std's, though (D12: `libm`
stays opt-in, M9.5), and a vector exp cannot reproduce the platform's bits portably. Measured on the i7-8700K with a
model of n-Propane's 18 power terms: 1 ln and 18 exp take about 61 ns of α^r's 316 ns. With std called per lane, the
speedup is at most 2.5× even if everything else vectorises perfectly 4 wide (about 2.2-2.4× realistically). With a
4-wide exp at 1-1.5 ns per lane it is about 3.2-3.5×.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL1 | How do SIMD lanes stay bitwise equal to the scalar path when std's exp and ln cannot be vectorised bit for bit? | **The `simd` feature requires `libm`** | `simd` enables `libm`. A SIMD build's scalar path therefore runs the `libm` crate, bit-identical across targets (M9.4, `fixtures/hash/libm.txt`), and its lanes are bitwise equal to that path. The lane exp and ln are ports of the locked `libm` crate's, operation for operation. Any other transcendental a laned kernel needs is ported the same way or called per lane through `math`. So `simd` changes numerics only as `libm` does, and `libm` stays the one exception to K13. The cost: the paths a SIMD build does not lane (the flashes) run at libm's 1.35-1.53× of std (D12). The gate (ARCHITECTURE.md §7) is unchanged: it still compares with the M9 record, std's scalar path. *Rejected:* an in-house exp/ln as everyone's reference (reopens M9.5); lanes as a tolerance-checked `Fast` policy (gives up K17's bitwise check); std called per lane (puts the gate out of reach). |
