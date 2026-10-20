# The fast feature's scope (2026-10-20)

Asked when M23.7 (the `fast` feature, user decision IM4) failed its gate in two ways the plan had not foreseen (issue
#244). The work in progress fuses a·b + c, through `Real`'s new method, in the in-house functions (Arm's FMA forms,
each matching Arm's C built with FMA bit for bit), the α^r term accumulators, Horner, the superancillary's Clenshaw sums
and α⁰'s lead term. Two of those broke checks that M23.7 requires to pass unchanged. M23.6 had just kept std's maths
as the default build's, because the in-house maths failed D12's rule at 1.056 (ARCHITECTURE.md D12).

| # | Question | Decision | Notes |
|---|---|---|---|
| IM7 | Does `fast` enable `libm`? | **Yes** | The user had thought that the in-house functions replace the `libm` crate's calls. They do, for `exp`, `ln`, `pow` and `expm1` (M23.2-M23.5). The `libm` feature now runs those four and the crate's `ln_1p`, `sqrt`, `sinh`, `cosh`, `atan`, `acos`, `sin`, `tan` and `cos`. With std the default, only a `fast` that enables `libm` gives IM4's one set of bits across targets, and with it M23.7's `fixtures/hash/fast.txt`. |
| IM8 | Do the superancillary's Clenshaw sums fuse under `fast`? | **No: they keep CoolProp's order, two roundings** | Fused, PropyleneGlycol's ρ_V at `coolprop-8.0.0/all/sat.csv:1501` (5.1e-15) moved by 3e-13 relative, outside `sa_coeff`'s 1e-14. Four fluids' p_min (1-Butene, DimethylEther, Ethane, R1234ze(E)) moved by an ulp, so the rows CoolProp wrote at p_min were refused as below it. Narrows IM4's "polynomial evaluations" to the EOS's own (Horner in the exponential terms). The term sums and the in-house functions still fuse. |
| IM9 | R411A's bubble point at 260.48 K no longer converges with the α^r term sums fused: what then? | **A step, M23.7a, to find out why it stalls, before M23.7** | `R411A.mix`, bubble, given T = 260.4842464590231 K: `NoConvergence { strategy: Vle, iterations: 60 }` with only the term accumulators fused; all of `tests/bubble_dew.rs` passes without them. IM4 says the solvers stay as they are: a fix that M23.7a finds in the solver comes back to the user. |
| IM10 | The name of `Real`'s a·b + c method? | **`mad`, for now** | `mul_add` is ruled out: `f64`'s own fused `mul_add` would win at every `f64` call site. |
