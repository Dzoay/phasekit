# The perf gate and the npm licence (2026-10-19)

Asked at M24's close. The reference machine's record for M24.6 had found M24.4's first version 8 % slower on every
mixture bench (natural gas α^r 15 %), and `gates perf` had not seen it: its gungraun workloads were the pure fluids'
alone. Still open since MX10: the npm package's `license` field reads `MIT OR Apache-2.0`, though the package, like the
C library, builds phasekit-mix in, and with it GERG-2008's coefficients from NIST's AGA8 code.

| # | Question | Decision | Notes |
|---|---|---|---|
| PG1 | Should `gates perf` count a mixture workload? | **Yes** ("Add a mixture bench in gates perf.") | `benches/instructions.rs` gains `mixture_alphar` (R32/R125 and the GERG-2008 gas, 16 states each of `benches/mixture.rs`'s grid about fixed critical points), `mixture_pt` and `mixture_qt` (R32/R125's single-phase states at 250 K and its bubble points from 230 to 270 K), held to the same +5 % as the rest (VERIFICATION.md §12, enforcement item 1). M24.4's first `departure.rs` fails all four. |
| MX11 | Should the npm package's `license` field name `LicenseRef-NIST-GERG`, since the package bundles phasekit-mix? | **No, it stays `MIT OR Apache-2.0`** ("Consumers don't have to take mixtures and when they do the get the AND NIST lic.") | phasekit-mix's own `license` keeps MX10's expression. The npm package and the C library ship `LICENSE-THIRD-PARTY`, which credits NIST, AGA8 and teqp and reproduces NIST's notice. |
