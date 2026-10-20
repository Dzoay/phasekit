# Mixtures: the scalar path under the two-phase kernel (2026-10-20)

Asked at M22.5, before it landed. Under user decision ML6 the two-phase kernel ships: in the default build a mixture's
QT flash under `ExecPolicy::Simd`, the mixture's default policy, takes 0.925-0.936× M22.4's time. The kernel makes the
composition derivatives generic over `Real`, and the scalar path they now share (`ExecPolicy::Reference`, which
`Mixture::with_exec` opts into) did not quite keep M22.4's speed. Measured on the quiet i7-8700K, default build, two
interleaved rounds, R32/R125, M22.5's `Reference` path against M22.4: QT 1.031× (dew), 1.033× (interior) and 1.004×
(bubble); PT 1.00× and 1.00×; DT 0.99× to 1.02×. The same code in two binaries differed by about 2 % on these benches.
Two causes were found and fixed before asking: the departures' jet inlined into `composition` (up to 8 %), and the
phases' τ and δ computed for the lanes under every policy. What remains is in the generic code's size *(inference
from callgrind: `composition` 15.3 → 17.9 KB, more I-cache misses)*.

| # | Question | Decision | Notes |
|---|---|---|---|
| ML7 | Does M22.5 land with its `Reference` path about 3 % slower than M22.4 on two QT cases? | **Yes** | The default path gains 6-8 % on QT; PT and DT are unchanged within the noise. M22.7's record measures the spike's kernels again, both builds. *Rejected:* recovering the 3 % first (near the noise, payoff uncertain); `Reference` as the mixture's default policy (nothing slower by default, but the gain opt-in). |
