# 03 Flash routines, phase determination, solvers, superancillaries and numerics - CoolProp v8.0.0 map

> Scope: `src/Backends/Helmholtz/FlashRoutines.{h,cpp}` (5,464 lines); the dispatcher, phase determination and density solvers in `src/Backends/Helmholtz/HelmholtzEOSMixtureBackend.cpp` (lines 1327-3149, ~1,820 of 4,599); `src/Solvers.cpp`, `include/CoolProp/numerics/*`, `src/{CPnumerics,PolyMath,MatrixMath,ODEIntegrators}.cpp` (5,386); `include/CoolProp/superancillary/*`, the deprecated 7-line shims `include/superancillary/*`, `src/superancillary.cpp` (2,469); `MeltingCaloric` (375); flash tests `src/Tests/CoolProp-Tests-{PXFlash,PXcdj,HS,HS-prototypes,HSU_D,AirCritical}.cpp` (3,431). About 19k lines. `dev/agent-notes.md`, `CoolProp-Tests-PTFlashMiddleRoot.cpp` and `CoolProp-Tests-DeltaOnly.cpp` exist only on origin/master and are cited as master-only. Part of the coolprop-rs port plan; cites the v8.0.0 source.

Aliases: **FR** `src/Backends/Helmholtz/FlashRoutines.cpp`, **FH** `.../FlashRoutines.h`, **HB** `src/Backends/Helmholtz/HelmholtzEOSMixtureBackend.cpp`, **SV** `src/Solvers.cpp`, **SA.h** `include/CoolProp/superancillary/superancillary.h`, **VLE** `src/Backends/Helmholtz/VLERoutines.cpp`, **T.cpp** `src/Tests/CoolProp-Tests.cpp`.
"Oracle" = the CoolProp 8.0.0 wheel (rev ae81610e) run with `uv` on this machine. Timings are Python-level medians over 400 random Water states, reused `AbstractState`, ~0.2 µs call overhead included; indicative only. "Verified" means reproduced in the oracle; "inference" marks reasoning not directly tested.

## 1. Purpose and concepts

- **Flash**: map an input pair to the EOS state `(T, rho)`, a phase label and, if two-phase, `Q` plus the saturated liquid/vapor sub-states. Every other property is then evaluated from alpha at `(T, rho)` (area 02).
- **Dispatcher**: `HelmholtzEOSMixtureBackend::update` (HB:1455) clears caches and converts mass inputs (`pre_update`, HB:1436), validates `Q` for Q-pairs, switches 18 molar input pairs onto 11 static `FlashRoutines::*` functions (PT, DHSU_T, DP, HSU_D, HSU_P, HS, QT, PQ, QS, HQ, DQ) and runs `post_update` (HB:1647). `post_update` checks only that `p`, `T`, `rho` are finite and `rho >= 0`, plus finite `Q` and a known phase. It does no range or consistency check. `FlashRoutines` is a friend class whose static functions read and write the backend's private fields (FH:21-29).
- **Phase determination** happens before solving. It is p-based (`p_phase_determination_pure_or_pseudopure`, HB:1699) or T-based (`T_phase_determination_pure_or_pseudopure`, HB:2177). For two-phase inputs it also does the lever-rule solve, so for many pairs "phase determination" *is* the two-phase flash.
- **Superancillaries (SA)**: piecewise Chebyshev expansions of the EOS's own VLE solution (`rho'(T)`, `rho''(T)`, `p(T)`), fitted offline in multiprecision by fastchebpure. They replace iterative VLE for pure fluids: QT/PQ cost ~0.6 µs. They give exact saturation brackets to every other flash. **Ancillaries** are the older empirical fits (1e-3 to 1e-6). The **EOS VLE solvers** (Maxwell -> Akasaka -> 1-D, VLE:756-776) are used when SA is disabled or absent.
- **Two code generations in one file.** v8.0.0 layers SA "happy paths" (2025-2026) over legacy ancillary-seeded "sad paths". Each flash is a cascade of strategies glued by `try { } catch (...)`, usually ending in an "accept only if the state reproduces the inputs" check.
- **Imposed phase** (`specify_phase`) is a mutable flag on the backend. Flashes read it, and some set or clear it as a side effect (§6).
- **Strict mode (#2773)**: a Q-pair inversion with several T-roots (water `rho'(T)` near 4 °C, `h''(T)` around 540 K, R1234ze(E) `s''(T)`) throws `MultipleSolutionsError` unless `update_with_guesses` supplies `guess.T`.
- **Non-unique pairs are not flagged elsewhere.** T+H is ambiguous across the compressed-liquid region, and T+S and T+U are ambiguous for water below ~277 K: a two-phase state and a compressed-liquid state share `(T, X)` wherever `(dX/drho)_T > 0` in the liquid. CoolProp silently returns the two-phase state (verified, §6).
- Mixtures share the dispatcher (stability, Rachford-Rice, SS + Newton, phase envelope). They are summarized only; see area 04.

## 2. Structure (key types/functions -> path:line)

| Component | Location | Role / notes |
|---|---|---|
| `update`, `update_with_guesses`, `pre_update`, `post_update` | HB:1455, :1594, :1436, :1647 | Pair dispatch; mixture Q-mass pairs diverted at HB:1459; unsupported pairs throw at HB:1577 (`SmolarUmolar`; there is no H+U pair in the enum, `include/CoolProp/DataStructures.h:292-343`) |
| Primitive setters | `update_QT_pure_superanc` HB:1327, `update_TDmolarP_unchecked` :1359, `update_DmolarT_direct` :1372, `update_TP_guessrho` :1422 | Used inside residuals; each calls `clear()` and `post_update` |
| `recalculate_singlephase_phase` | HB:207 | Label from (p vs pc, T vs Tc, rho vs rhoc); mixtures use rho vs rho_reducing |
| p-based determination | HB:1699 (p>psat_max :1717-1780; SA :1784-1857; ancillary :1862-2017; full VLE on temp backend :2024-2096; p<ptriple :2097-2111) | p given plus T/D/H/S/U |
| T-based determination | HB:2177 (T==Tc :2200-2228; SA :2231-2316; ancillary :2319-2442; VLE :2447-2507; T>Tc :2508-2560) | T given plus P/D/H/S/U |
| Density solvers | `solver_rho_Tp` HB:2906, `solver_rho_Tp_SRK` :3078, `solver_rho_Tp_global` :2835, `solver_dpdrho0_Tp` :2656, `SolverTPResid` :2787 | Single-phase rho(T,p) |
| `FlashRoutines` class | FH:29; Rachford-Rice helpers FH:33-51 | All static |
| PT | `PT_flash` FR:298, `PT_flash_mixtures` FR:24, `PT_flash_with_guesses` FR:1429 | |
| DP | `DP_flash` FR:409, `solver_DP_resid` :344, `T_DP_PengRobinson` :385 | |
| Q-pairs | `DQ_flash` FR:566, `HQ_flash` :604, `QS_flash` :637, `resolve_T_via_superancillary` :712, `sat_superanc_path_applies` :558, `alpha0_offset_total` :542, `*_with_guesses` :847/:865/:877 | |
| QT, PQ | `QT_flash` FR:888, `PQ_flash` :1157, `PQ/QT_flash_with_guesses` :1369/:1396, `PT_Q_flash_mixtures` :1454 | |
| D+X | `HSU_D_flash` FR:1710 (SA :1842-2113, legacy :2114-2493, mixtures :2494-2709), `HSU_D_flash_twophase` :1671 | |
| P+X | `HSU_P_flash` FR:3329, `HSU_P_flash_singlephase_Brent` :2802 (direct-EOS warm Newton :2896-2958; outer solve :3139-3325) | Name is legacy: it uses TOMS748/Halley/2-D Newton |
| T+X | `DHSU_T_flash` FR:3802, `solver_for_rho_given_T_oneof_HSU` :3609 | |
| HS | `HS_flash` FR:4817, `hs_cascade` :4716, legs :4419/:4488/:4522/:4698, `hs_corrector` :4338, `hs_accept` :4656, `hs_two_phase_likely` :4776, `HS_flash_twophase` :4205, `HSGasGuard` :4317 | |
| Dead code | FR:2712-2801 (`HSU_P_flash_singlephase_Newton`), :4239-4291 (`HS_flash_singlephase`), :4292-4297 (random guess with `rand()`), FH:252-276 (`solver_TP_resid`), FH:280-307 (`PY_singlephase_flash_resid`) | No callers in `src/` (grep) |
| Solver API | `include/CoolProp/numerics/Solvers.h:17-65` (`FuncWrapper1D` with `Dictionary options`, `errstring`, `iter`; `WithDeriv/TwoDerivs/ThreeDerivs`; `FuncWrapperND`) | Virtual-call residual objects |
| Solvers | SV: FD Jacobian :13, `NDNewtonRaphson_Jacobian` :49, `Newton` :108, `Halley` :159, `Householder4` :227, `Secant` :288, `BoundedSecant` :373, `ExtrapolatingSecant` :429, `Brent` :522 | Plus boost `toms748_solve`, called directly in FR/HB and through `detail::toms748` (`src/superancillary.cpp:31-36`) |
| Small numerics | `src/CPnumerics.cpp`: `MatInv_2` :71, `solve_cubic` :82, `solve_quartic` (Eigen Polynomials) :134, `SplineClass` :161; `include/CoolProp/numerics/numerics.h` (`ValidNumber` :33, `is_in_closed_range` :561, interpolation helpers) | |
| Linear algebra | `include/CoolProp/numerics/MatrixMath.h` (Gauss-Jordan `linsolve` :593-731, Eigen<->vector converters); Eigen QR SV:69, FR:4255, VLE:1392/1624/2380/3096; LDLT + SelfAdjointEigenSolver VLE:2941-2949 | All systems in pure flashes are 2x2 or 3x3 |
| PolyMath, ODE, finite differences | `include/CoolProp/numerics/PolyMath.h` (~800 of 993 lines are comments: 746 `//` lines plus doc blocks), `src/PolyMath.cpp`; `src/ODEIntegrators.cpp` (`AdaptiveRK54`); `include/CoolProp/numerics/finite_diff.h` (`romberg_diff`) | PolyMath serves incompressibles and rational ancillaries (`src/Backends/Helmholtz/Fluids/Ancillaries.cpp:46-48`); ODE and Romberg have **no users** |
| SA classes | `ChebyshevExpansion` SA.h:270, `ChebyshevApproximation1D` :489, `SuperAncillary` :817; JSON parse `src/superancillary.cpp:43-73` | Header-only templates |
| Cubic SA | `include/CoolProp/superancillary/cubicsuperancillary.h:6-720` | Universal SRK/PR curves in reduced T~, degree 18, header constants; duplicates Clenshaw (:15-28) |
| SA ownership | `include/CoolProp/CoolPropFluid.h:403-447`, `src/Backends/Helmholtz/Fluids/FluidLibrary.h:392-396` | `shared_ptr` per EOS, built from a retained JSON string; greedy unless `LAZY_LOAD_SUPERANCILLARIES` |
| SA access | `get_superanc` HB:314 (throws for pseudo-pure/mixtures), `ensure_caloric_superancillaries` HB:321 | |
| Melting caloric | `src/Backends/Helmholtz/MeltingCaloric.cpp:247-265` (`static std::map` + mutex) | HS leg 4 |

## 3. Algorithms and formulas

### 3.1 Pure-fluid flash per input pair (SA enabled, the default)

| Pair (entry) | Phase determination | Seeds / bracket | Solver chain -> fallbacks | Tol / max iter | Imposed phase, metastable | Oracle µs |
|---|---|---|---|---|---|---|
| **PT** (FR:298) | Critical short-circuit if \|T/Tc-1\| and \|p/pc-1\| <= 1e-10 (FR:303-313). Not imposed: `T < 0.9 Ttriple + 0.1 Tmax_sat` (FR:319) -> p-based with `Tsat = T(ln p)`, liquid if `T < Tsat - 100 eps` (absolute K, HB:1801); exactly at Tsat it falls into `default:` and throws "bad input for other" (HB:1815-1836). Else T-based with SA `psat(T)`; **throws if \|psat/p-1\| < 1e-6** (HB:2245). Two-phase -> throw (FR:329) | Liquid: **ancillary** `rhoL(T)` even when SA exists (HB:2934). Gas: SRK root (HB:3078) or ideal gas | Liquid: Halley -> Brent `[0.9,1.3] rhoL_anc`. Supercritical liquid: Brent `[0.99 rhoL_anc, 4 rhoc]` -> Brent `[.., 1.1 rhoL_anc(Ttriple)]` -> bracket walk + TOMS748 48 bits (HB:2954-3034). Gas/supercritical: Householder4 + branch-sign checks -> restart from 3 rho_r or 1e-6 -> Brent `[1e-10, 3-5 rho_r]` (HB:3037-3076) | Residual (p-p_spec)/p; ftol 1e-8; 20 (H4) / 100 | Honored (skips determination). Imposed two-phase -> "Bad phase to solver_rho_Tp_SRK" (HB:3145). Metastable only via imposition | 19.4 |
| **DmolarT** (`DHSU_T_flash` FR:3802) | T-based, SA lever rule on v (HB:2258-2275); T>Tc by rho vs rhoc_num. **No T<Ttriple check**: SA extrapolates (§6) | none | Direct `p(T,rho)` (FR:4176-4195) | - | Honored. Imposed two-phase runs VLE on a temporary backend but loads SatL/SatV at *ancillary* densities (FR:3815-3867) | 3.5 (1φ) / 6.7 (2φ) |
| **HT, ST, TU** (FR:3802 -> FR:3609) | T-based: SA densities + EOS calorics at them (HB:2278-2315). T>Tc labels only (HB:2508-2560). T==Tc±10 eps throws (HB:2226-2227). **Non-unique** in compressed liquid (§6) | Liquid: linear interpolation between triple-liquid and SA-liquid states (FR:3758). Gas: `(1e-14+rhoV)/2` (FR:3789) | T>Tc: Brent `[rhoc,1e-10]`, rhoc grown x1.1 for at most 31 steps, i.e. up to ~19x (FR:3637-3693). Liquid: Halley -> Secant (FR:3760-3764). Gas: Halley -> Brent `[1e-14, rhoV]` (FR:3788-3796) | **Absolute rho tol 1e-9 mol/m³** (FR:3666, :3690); Halley ftol 1e-8 | Honored (skips determination); T>Tc label honored (FR:3697-3703) | HT 33.9, ST 62.4 |
| **DmolarP** (FR:409) | p-based (as PT, other=D) | Liquid: `SatL.T` or `_TLanc` (= SA Tsat, HB:1844). Supercritical liquid: 1.1 Tc. Gas: closed-form Peng-Robinson `T(rho,p)` (FR:385-407) | Halley in T; accept only if finite, 0<T<=1.5 Tmax, \|r\|<1e-7 (FR:445-453) -> TOMS748 `[Tmin, 1.5 Tmax]` (FR:458-489) | 1e-10 / 100; 30 bits / 100 | **Ignored** (FR:410-413, :503-504 "TO DO"). Mixtures: NotImplemented | 13.1 |
| **HmolarP, PSmolar, PUmolar** (FR:3329) | p-based: p>psat_max -> supercritical by X vs X(Tc,rhoc) (HB:1717-1780); SA Tsat(p) + EOS calorics at SA densities, Q outside [-1e-9, 1+1e-9] is single phase (HB:1784-1857); p<0.9999 ptriple -> gas (HB:2097-2111) | T bracket: gas `[Tsat(p), 1.5 Tmax]`; liquid `[Tmelt(p)-1e-3 or Tmin-1e-3, Tsat(p)]`; supercritical `[Tmelt or Tmin, 1.5 Tmax]` (FR:3355-3431) | Outer: TOMS748 if endpoints bracket, else Halley in T. Inner rho(T,p): full `update(PT)` on cold probes, or direct-EOS Householder3 warm start (<=15 iter, skipped for p>pc) (FR:2896-2958, :2982-3067). On throw: near-critical -> 2-D Newton (T,rho); p>pc -> TOMS748 `[Tmin, 1.05 Tc]` -> 2-D Newton (FR:3195-3300) | TOMS748 30 bits/100; Halley 1e-12; inner 1e-12 rel; 2-D Newton 1e-12/20, accept residual <=1e-6 | Determined phase imposed inside (FR:2874-2882); **cleared on exit** (FR:3183, :3192, :3306) | 153-167 (1φ) / 7.1 (2φ) |
| **HmolarSmolar** (FR:4817) | (0) screen: 40-point scan of `Qh(T)-Qs(T)` on caloric SA + TOMS748 (FR:4776-4807) | Leg anchors: saturated state with `s_sat = s` (SA S-inversion, FR:4451), `T = Tmax` isotherm (FR:4488-4517), ideal-gas limit lambda=0 (FR:4570-4590), melting line (FR:4698-4710) | (1) cascade of homotopies; corrector = 2x2 Newton in (T, ln rho) with dp/drho>0 backtracking, N = 1..128 subdivisions, <=40 iterations per step (FR:4338-4400). (2) two-phase: legacy Brent on `Qh-Qs` over QT flashes. Pseudo-pure: same cascade without dome veto (FR:4900-4944). Legacy: 50-point ST-flash scan + TOMS748 -> Brent (FR:4945-5043) | Corrector norm 1e-11; accept \|Δh\|<=1e-6 R Tc, \|Δs\|<=1e-6 R, cv>0, dp/drho>0, outside dome (FR:4656-4680, :4717) | Imposes gas internally (`HSGasGuard`), clears user imposition; rejects in-dome metastable roots | 61.9 (1φ) / 78.8 (2φ, p90 328) |
| **DmolarH/S/U** (FR:1710) | SA: all T with `rhosat(T) = rho` on both branches (FR:2064) -> intervals `[Tmin_SA, roots..., Tc_num, 1.5 Tmax]`, each classified by its midpoint (FR:2072-2102) | interval edges | 1φ: TOMS748 on `X(rho,T)-X`, reject in-dome roots (FR:1944-1973). 2φ: TOMS748 on `Qo-Qd` with caloric SA, <=4 secant EOS polish, reject `Qd` outside [0,1]±1e-8 (FR:1976-2050). Commit check (FR:1927-1932). Legacy: supercritical fast path (FR:2128-2183); triple-density regions; `saturation_D_pure` with 7 damped retries (FR:2230-2251); `HSU_D_flash_twophase` (Brent, **20 iter, Tmax_sat-0.01 cap**, FR:1705); sub-triple liquid via melting-line minimum scan (FR:2386-2488) | 44 bits / 100; accept rho 1e-7 rel, X 1e-6 rel + 1e-3 abs | Legacy residual imposes gas and clears on destruction (FR:1794, :1802-1811) | 36.9 (1φ and 2φ) |
| **QT** (FR:888) | - | - | SA: `rho'`, `rho''`, `p` at T (FR:893-911). Throws only if T > Tc_num (FR:898-902): **no lower-domain check**. No SA: within 1 µK of Tc -> rhoc; range check; critical splines (FR:935-942); **Maxwell only** (FR:943-951, no Akasaka fallback); pseudo-pure: ancillary p and rho, Q ∈ {0,1} only (FR:952-967) | - | - | 0.60 |
| **PQ** (FR:1157) | - | - | SA: `T = T(ln p)` (lazy inverse expansion) -> densities (FR:1160-1181); throws only if p > pmax_num. No SA: `saturation_PHSU_pure` (Newton, ω = 1, 0.6, 0.2) -> `saturation_P_pure_1D_T` (FR:1236-1260). Pseudo-pure: invert pL/pV ancillaries, `T = Q Tdew + (1-Q) Tbub` (FR:1186-1199) | - | - | 0.65 (first call +2.2-2.7 ms) |
| **DmolarQ** (FR:566) | - | - | Q ∈ {0,1} + SA: enumerate all SA roots, dedup 1e-6 K, 0 roots -> OutOfRange, >1 -> MultipleSolutions (FR:817-840), then QT. Fractional Q: Brent `[Tmin+0.1, Tc-0.1]` on `Q(T)` over full QT flashes (FR:589-597) | Brent 1e-10 / 100 | **Sets imposed two-phase, never cleared** (FR:570) | ~1-5 (unsourced; not re-timed) |
| **HmolarQ** (FR:604) | - | - | **Q must be 1** (FR:608), checked before the SA gate. SA strict path, else `saturation_PHSU_pure` IMPOSED_HV | - | Sets two-phase (FR:611) | - |
| **QSmolar** (FR:637) | Shortcut \|s - s_reducing\| < 0.001 J/mol/K -> critical point, any Q (FR:641-647) | - | SA strict path, else `saturation_PHSU_pure` IMPOSED_SL/SV; fractional Q throws (FR:660-678) | - | Sets two-phase (FR:648) | - |
| `*_with_guesses` (HB:1594) | - | user guesses | DQ/HQ/QS: SA monotonic interval containing guess.T -> TOMS748 64 bits (FR:776-813). PQ/QT: `newton_raphson_saturation` from user T or p, x, y, rho', rho'' (FR:1369-1427). PT: `solver_rho_Tp(guess)` **with the return value discarded** (FR:1430); label from p/T/rho vs critical; no saturation or stability check (FR:1431-1451) | - | Two-phase leak as above | - |

Mixtures, summary only: PT runs stability (Michelsen/Gernert) plus a Wilson cross-check -> `PTflash_twophase`, else SRK-root Gibbs selection (FR:24-297); the envelope path calls `specify_phase`/`unspecify_phase` and wipes a user imposition (FR:43-53; fixed on master aeea05e4). PQ/QT use the envelope when built, else SS + Newton (FR:978-1050). P+X: TOMS748 in T over full PT flashes (FR:3441-3607). T+X and D+X: scans of nested PT flashes (FR:3894-4171, :2494-2709). DP: NotImplemented.

### 3.2 Phase-determination details

- **SA classification** compares the input with `(rho'(T), rho''(T), psat(T))` or `Tsat(p)`. Saturated calorics come from the EOS at SA densities (HB:1819-1834, :2278-2296): a hybrid of SA densities and EOS calorics. Ambiguity bands are tight: `Q < -1e-9` / `> 1+1e-9` (p-based, HB:1839-1849), `Q < 0` / `> 1` (T-based, HB:2298-2309), 100 DBL_EPSILON on T (HB:1801, absolute K).
- **Legacy classification** (no SA, pseudo-pure) uses fixed bands: 2% on pressure (HB:2326-2327), 5% on density (HB:2001-2002, :2359-2360), `max_abs_error` bands for h/s/u ancillaries (HB:1905-1990), and the test `value > 0.95*rho_liq || value < 1.05*rho_vap` commented "Definitely single-phase" although it marks the ambiguous strip (HB:2374-2390). Anything ambiguous runs a full VLE on a **temporary backend** (HB:2029, :2447).
- **Phase also lives in `_Q` sentinels**: -1, ±1000 with opposite signs in different branches (liquid = -1000 at HB:1809, :2044, :2263; liquid = +1000 at HB:1900, :1928, :2253, :2335), 1e9 supercritical (HB:1718, :2510), 10000 (D+X and HS single phase, FR:1916, :2268, :4863), ±1 (HB:2494-2498). Oracle `Q()` after single-phase flashes: -1 for PT/PH/PS/DT/ST/DP/HT and **10000** for DH/DS/HS.

### 3.3 Superancillaries

- **Concept.** Chebyshev expansions fitted to the EOS's own multiprecision VLE solution with dyadic interval splitting (`detail::dyadic_splitting`, SA.h:185-255; split when the norm of the last M=3 coefficients over the first exceeds tol). Literature: Bell & Deiters, IECR 60 (2021) 9983 doi:10.1021/acs.iecr.1c00847 (`CoolPropBibTeXLibrary.bib:540`, cubic SA). `Web/coolprop/SuperAncillary.ipynb` cites three titles without bibliographic details: "Exceptionally reliable density-solving algorithms for multiparameter mixture models from Chebyshev expansion rootfinding", "Efficient and Precise Representation of Pure Fluid Phase Equilibria with Chebyshev Expansions", "Superancillary Equations for the Multiparameter Equations of State in REFPROP 10.0". Journal details are not in the repo. Balancing follows arXiv:1401.5766 Alg. 3 (SA.h:67); derivative coefficients follow Mason & Handscomb p.34 Eq. 2.52 (SA.h:416).
- **Data layout** (`EOS[0].SUPERANCILLARY` in `dev/fluids/*.json`; numbers measured over all files):
  - `jexpansions_{rhoL,rhoV,p}`: lists of `{xmin, xmax, coef[13]}`. All pieces are **degree 12**, with 53-91 pieces per curve (median 63; Water 65). Pieces cluster toward Tc: Water's last piece is 2.2e-5 K wide, its widest 39.4 K.
  - **All 130 fluids use identical piece boundaries for the three curves.** Of 8,032 interior boundaries, all but 60 are dyadic (`Tmin + k ΔT/2^m`, max depth 24); all 60 exceptions are Chlorine's, consistent with its shortened domain below (added by verifier). 1,386 pieces have an exactly-zero top coefficient.
  - `meta`: `Tcrittrue` (numerical critical point), `rhocrittrue`, `Ttriple`, `Treducing`, `gas_constant`, `Brho{L,V}`. `crit_anc` (near-critical power law) is **unused by the C++** (grep). `check_points`: 3 per fluid with multiprecision `p`, `rho'`, `rho''` and fastchebpure's SA/mp ratios. `source_eos_hash` (FNV-1a) ties the fit to the EOS (T.cpp:3598).
  - Coverage: **130 of 136** fluids. The six without are the pseudo-pure Air, R404A, R407C, R410A, R507A, SES36.
  - Size: 58.8-100.6 KB of JSON per fluid (9.05 MB total); 318,318 coefficients = 2.55 MB as f64 (~19.6 KB per fluid).
  - Domain: `xmin` = meta `Ttriple` for all 130. `xmax` = `Tcrittrue` for 129; **Chlorine's last piece ends 1.0e-4 K below `Tcrittrue`**, so `T_critical()` (meta) and `p_critical()` (= p at `xmax`, SA.h:917) describe slightly different points.
- **Evaluation**: Clenshaw after an affine map to [-1,1] (SA.h:306-321). The piece index comes from an unchecked binary search over piece starts (SA.h:697-713), so **x outside [xmin, xmax] extrapolates silently**. CoolProp performs three separate searches for QT although the boundaries are shared. `eval_sat_many`: 42-43 ns/eval (oracle, C++ loop).
- **Monotonic intervals** are built at construction. For each piece the derivative is trimmed of trailing zeros, a transposed companion matrix is formed and balanced, Eigen general eigenvalues with \|Im\| < 1e-15 inside [-1,1] are taken as extrema (SA.h:508-551), and intervals carry per-piece y-ranges (SA.h:557-631). This runs for every fluid at library load (§5).
- **Inversion**: `get_x_for_y` visits every interval whose y-range contains the target and runs TOMS748 per piece (SA.h:755-775). `get_all_intersections` merges the L and V branches (SA.h:1329-1341). `T(p)` uses a lazily built inverse expansion in ln p (degree 12, tol 1e-12, <=26 passes, SA.h:923-952), built under a mutex (SA.h:1078-1086).
- **Caloric SA** (h, s, u for L and V; SA.h:1127-1193) is built lazily: sample `h/s/u(T, rho_SA)` from the EOS at the 13 Chebyshev-Lobatto nodes of every rho piece, then multiply by the L matrix. The degree is hard-coded to 12 (SA.h:1129) and there is no fit-error check. The result is stamped with the first caller's alpha0 offset `(a1, a2)`; other reference states shift the target value (`Δh = R T_red Δa2`, `Δs = -R Δa1`, u takes the h shift; FR:747-774, SA.h:876-893). First build costs 45-63 ms per fluid (oracle). Precision is ~3e-9 relative (open bead CoolProp-05w).
- **Accuracy.** Over the 390 check points (3 per fluid, none near Tc), the worst `p` ratio is 4.7e-9 (MethylStearate, 387.9 K) and the median 4.8e-14. rho' and rho'' are better than 4.5e-13. The full-domain figures quoted for the four fluids refit in master 66859efb are worse: worst p 1.9e-12 to 7.5e-10, except n-Undecane at 2.5e-7, and **rho'/rho'' at 1.4e-5 to 4.5e-5, set by the near-critical end**. Treat ~1e-5 as the near-Tc density accuracy. Against CoolProp's SA-off Maxwell VLE (oracle, 120 T per fluid), the median p difference is 1e-13 to 3e-12. Two outliers turn out to be EOS-VLE deficiencies, not SA errors:
  - R134a at Tc-1.76 K: the SA-off path uses critical splines and is off by 3.2-3.5% in rho'' (3.2% in a verifier rerun at Tc_num-1.76 K) with Δg/RT = 6e-4. The SA point satisfies Δg/RT ~ 1e-15.
  - Propane at 86 K (p ≈ 2e-4 Pa): p differs by 1e-3 while both satisfy Δg/RT ~ 1e-13. Double-precision liquid-side p cannot resolve psat there, so the multiprecision-fit SA is the better reference.
- **Numerical critical point**: with SA enabled, `T_critical()`, `p_critical()` and `rhomolar_critical()` return SA values (HB:1207-1211, :1226-1230, :1245-1249). Water: 647.0959999999873 K, so **QT at the published 647.096 K throws** (oracle). T.cpp:3888-3901 asserts that the two critical points differ.
- **Cubic SA** holds universal reduced curves for SRK/PR (degree 18) and returns -1/-2 sentinels for unknown codes (`cubicsuperancillary.h:676-705`). It belongs to area 06 but should share the Chebyshev kernel.

### 3.4 Numerics actually used

| Routine | Where | Used by | Semantics / defects |
|---|---|---|---|
| TOMS748 (Alefeld-Potra-Shi, via boost) | `boost/math/tools/toms748_solve.hpp`; `src/superancillary.cpp:31-36` | P+X, D+X, DP, HS, SA inversion, rho fallback, mixture sweeps | Returns the final bracket; callers take the midpoint and re-evaluate. **Exhaustion is silent**: no call site inspects `max_iter` (grep FR/HB/SA) |
| Brent (Brent 1973) | SV:522 | ancillary inversion, legacy flashes, rho(T,p) | `t` is an x tolerance but is also compared with \|f(b)\| (SV:530, :536); \|f\| is compared with `macheps` (SV:609) and with `2 macheps \|b\|` (SV:642). Can return `a` after evaluating `b` (SV:536-537) |
| Halley / Householder4 / Newton | SV:159 / :227 / :108 | rho(T,p), DP, T+X, legacy D+X | Return `x_{n+1}` while the backend sits at `x_n` (SV:196-199, :263-266). Stop on \|dx/x\| below `xtol_rel` (default 1e-12, `Solvers.h:79-80`; Newton hard-codes 1e-11, SV:131) regardless of residual. Newton's loop condition `iter < maxiter \|\| \|f\| > ftol` (SV:115) makes ftol matter only at maxiter. ω read from a string `Dictionary` (SV:168) |
| Secant, ExtrapolatingSecant, BoundedSecant | SV:288, :429, :373 | T+X liquid fallback; ancillary inversion fallback (`Ancillaries.cpp:111`); cubic/PC-SAFT | ExtrapolatingSecant returns an extrapolated guess when f is NaN (SV:472-478). BoundedSecant has no NaN check |
| `NDNewtonRaphson_Jacobian` (Eigen colPivQR) | SV:49-97 | P+X 2-D fallback | Returns on a tiny step (absolute 100 eps or relative 1e-12) before checking the residual (SV:82-88). At maxiter it sets `x0[0] = _HUGE` and **keeps looping** (SV:90-93). Default FD Jacobian uses step `0.001*x[i]`, which is zero at x=0 (SV:22) |
| 2x2 Cramer inline | FR:4371-4374, :4618-4621; `MatInv_2` CPnumerics.cpp:71 | HS corrector; dead Newton | `MatInv_2` has no det==0 guard |
| Gauss-Jordan `linsolve` | MatrixMath.h:593 | `saturation_PHSU_pure` 3x3 (VLE:508), `saturation_D_pure` 2x2 (VLE:703) | Partial pivoting; singularity test is an absolute pivot < 10 eps (MatrixMath.h:623), which is scale-dependent |
| `solve_cubic` (trig/hyperbolic) | CPnumerics.cpp:82 | SRK seed (HB:3113), critical splines, cubic backend | Quadratic branch has no discriminant guard (:94-95). `acos` argument can exceed 1 by rounding near a double root (DELTA≈0, :124-126) -> NaN |
| Chebyshev Clenshaw, derivative, companion eigenvalues, dyadic split | SA.h:306, :415, :508, :185 | SA | Eigen general eigensolver at load only; see latent defects in §6 |
| Rachford-Rice, Wilson, SS, Michelsen TPD, LDLT | FH:33-51, VLE | mixtures | Area 04 |
| `AdaptiveRK54`, `romberg_diff` | ODEIntegrators.cpp, finite_diff.h | **none** | Dead |

Eigen appears in: QR solves, the SA companion eigenvalues (SA.h:539), the LU/Chebyshev matrices (SA.h:151-169), `solve_quartic` (`unsupported/Eigen/Polynomials`, CPnumerics.cpp:4, :138), PolyMath. Every pure-fluid flash solve is 1-D, or 2-D with 2x2/3x3 systems: **no general dense linear algebra is needed for pure fluids**.

### 3.5 Literature cited in code

Akasaka, J. Therm. Sci. Tech. 3(3), 2008 (VLE:781-787). The Maxwell solver's docstring is truncated ("implements the method of", VLE:935-939). Brent 1973 (SV:509-510). Halley via Wikipedia (SV:150); Householder via numbers.computation.free.fr (SV:218). TOMS748 via the boost docs link (FR:3157); not otherwise cited. SRK and PR constants are used uncited (HB:3083-3087, FR:392-394). Fernández-Prini et al., JPCRD 2003, doi:10.1063/1.1564818, for mixture Henry's-law guesses (FR:1054). The HS cascade derivation is in `Web/coolprop/HSFlash.ipynb`, with no external paper.

## 4. Data and configuration inputs

- **Fluid data** (area 02 / 09): SA blocks (§3.3). Ancillaries `pL, pV, rhoL, rhoV, hL, hLV, sL, sLV` with `max_abs_error`, stored relative to `hs_anchor` (HB:1909-1913). Triple-point liquid/vapor states (FR:2186-2192). Melting line and limits (FR:2407-2439). `critical_region_splines` (`CoolPropFluid.h:427-428`). EOS limits `Tmin`, `Tmax`, `pmax`, `ptriple`. Note `EOS.Ttriple = EOS.limits.Tmin` at parse (`FluidLibrary.h:386`).
- **Config keys** (`include/CoolProp/detail/configuration_keys.h`). These are global and read on hot paths through `get_config_bool` (an `unordered_map::find`, `Configuration.h:150-160`); FR has 16 `get_config_bool` call sites and HB 14:

| Key | Default | Effect |
|---|---|---|
| `ENABLE_SUPERANCILLARIES` (:72) | true | SA vs legacy for QT/PQ/phase determination/D+X/HS **but not the Q-pair strict path** (§6); also changes the reported critical point |
| `HSU_D_TWOPHASE_EOS_POLISH` (:76) | true | Secant EOS polish after the SA two-phase D+X solve |
| `ENABLE_MELTING_CALORIC_HS` (:73) | true | HS leg 4 |
| `CRITICAL_WITHIN_1UK` (:12), `CRITICAL_SPLINES_ENABLED` (:14) | true | QT near Tc when SA is off |
| `DONT_CHECK_PROPERTY_LIMITS` (:44) | false | Melting-line and Tmin range checks |
| `HENRYS_LAW_TO_GENERATE_VLE_GUESSES` (:46) | false | Mixture PQ; calls `PropsSI(...,"Water")` inside the flash (FR:1304) |

- **Environment kill-switches** are read once into function statics: `COOLPROP_DISABLE_SUPERANC_HSU_D` (FR:1853), `PXFLASH_DIRECT_EOS`, `PXFLASH_INNER_NEWTON` (FR:2836-2841), `COOLPROP_DISABLE_MELTING_CALORIC_HS` (FR:4743), `COOLPROP_DISABLE_SUPERANC_HS` (FR:4825, :4901). `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY` is checked at library load (`FluidLibrary.h:393`, `FluidLibrary.cpp:47`). The compile-time `LAZY_LOAD_SUPERANCILLARIES` (`CoolPropFluid.h:444`) also affects loading.
- **Inputs from area 02**: alphar/alpha0 derivative bundles (`residual_helmholtz->all`, `calc_all_alpha0_derivs_nocache`), `calc_*_nocache`, `first/second_partial_deriv`. `calc_hmolar_nocache` runs the full alphar bundle twice, and `calc_smolar_nocache` twice plus alpha0 twice, to read one or two entries (HB:3177-3179, :3224-3227). Phase determination, the HS/D+X probes and the caloric SA build pay this repeatedly.

## 5. State, caching, globals, thread-safety, memory

- **Stateful backend.** A flash mutates the backend's private fields, `CachedElement`s and child backends `SatL`/`SatV` (HB:141-150). Residuals call `update_DmolarT_direct`, or even the public `update(PT_INPUTS)`/`update(QT_INPUTS)` re-entrantly on the same object (FR:3063, :1686, :4216, :514). So every probe clears and refills the cache and overwrites the caller's inputs. Comments warn that failed inner solves leave `_T`/`_p` at -inf or garbage (FR:38-40, :105-107, :2810-2817), and several paths snapshot and restore inputs by hand (FR:1864-1876, :2171-2182, :4830-4834; FR:4875-4879 records a past bug where a missed restore corrupted the target).
- **The documented contract is one AbstractState per thread** (master `dev/agent-notes.md`, Thermodynamics section). `PropsSI` builds a fresh state per call (`src/CoolProp.cpp:241-273`), so it pays construction (52-58 µs, oracle) and never sees the imposed-phase leak. The low-level API reuses states and does.
- **The imposed phase is cross-call mutable state**: set by DQ/HQ/QS, cleared by PH/DH/HS (§6).
- **Shared per-fluid SA** (`shared_ptr` in `EquationOfState`, `CoolPropFluid.h:406-438`):
  - rho'/rho''/p curves are immutable after construction.
  - The lazily built `invlnp` and caloric curves sit behind one per-SA `std::mutex`. That mutex is taken on **every** `get_T_from_p` (SA.h:1272-1274 -> :1078-1086), so every PQ, PH, PS, PU and SA p-based determination locks it. It is also taken on every `get_caloric_alpha0_stamp` (SA.h:1043-1046) and `ensure_HSU_under_lock` (SA.h:1021-1030), which run several times per HQ/QS/D+X/HS call.
  - `has_variable` and `get_approx1d` read the optionals without the lock (SA.h:1059-1073, :971-993). This is safe only because callers call `ensure_*` first.
  - `get_superanc()` returns the `shared_ptr` by value: an atomic refcount RMW on a control block shared by every thread using the fluid (HB:314-319; 19 call sites in FR and HB, including every SA-enabled `T_critical()`/`p_critical()`/`rhomolar_critical()` call, HB:1208, :1227, :1246).
- **Process globals in the hot path**:
  - `deriv_counter.fetch_add` on every *cached* alphar bundle evaluation (`calc_all_alphar_deriv_cache`); it is never read (HB:47, :3554; grep). The `_nocache` path and the P+X direct-EOS path do not touch it. This is a single contended cache line across all threads.
  - The config map is unsynchronized against `set_config_*` and was lazily constructed without `call_once` in v8.0.0 (`src/Configuration.cpp:126-133`; master c17783e6).
  - The melting-caloric cache is a `static std::map` behind a global mutex, keyed by fluid **name only** and built with the first caller's EOS (`MeltingCaloric.cpp:247-265`). After `change_EOS` it could serve a stale model (inference).
  - Minor: `rand()` in dead code (FR:4294); debug `static` vectors in Secant under `COOLPROP_DEEP_DEBUG` (SV:290).
- **Memory and load cost** (oracle unless noted):
  - Each AbstractState copies `CoolPropFluid` three times (self, SatL, SatV; HB:114, :141-150). The SA itself is a shared `shared_ptr`. The code also keeps the SA JSON string in each `EquationOfState` (`CoolPropFluid.h:406`, 68 KB compact for Water), but the verifier measured **no per-state cost from it**: glibc heap per state is the same to within 0.12 KiB with and without `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY` (Water 147 KiB, n-Propane 102 KiB). Why the string copy does not show up is unexplained. Measured RSS is **~73-135 KiB per AbstractState** (n-Dodecane, Propane, Nitrogen, Water). Flashes build further temporary full backends (HB:2029, :2447; FR:3827).
  - **Eager SA construction for all fluids at load**: `import CoolProp` takes 1.97-2.01 s, and 0.195 s with `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY=1`. A verifier rerun on a busier machine gave 2.6-4.4 s against 0.34 s: same ratio, so SA construction still dominates. About 1.8 s, or ~14 ms per fluid, goes to dumping each SA block back to JSON (`FluidLibrary.h:395`), re-parsing it (`src/superancillary.cpp:53`) and running ~190 companion eigensolves per fluid.
  - First-use builds: `T(ln p)` inverse 2.2-2.7 ms; calorics H/S/U 45-63 ms (Water 62.6, Propane 45.3).
- **Non-const evaluation of shared data**: `SaturationAncillaryFunction::evaluate` writes the member scratch vector `s` (`Ancillaries.cpp:65-68`, `Ancillaries.h:120`). `calc_alpha0_deriv_nocache` calls `E.alpha0.set_Tred(Tc)` on every evaluation (HB:3595). Both are per-copy today, so they are races only if the data is ever shared (inference).

## 6. Rot and bugs

| Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|
| **Imposed-phase leak**: DQ/HQ/QS (+ `_with_guesses`) call `specify_phase(iphase_twophase)` and never clear it | FR:570, :611, :648, :851, :869, :881; still on origin/master (FR:726, :768, :806, :1009, :1027, :1039 there). Oracle, reused Water state after QS: PT -> "Bad phase to solver_rho_Tp_SRK"; **PH at (30 MPa, h(700 K)) -> T = 270.79 K**; after DQ, DT(10 mol/m³, 700 K) is labeled two-phase | Silent wrong answers on the low-level (performance) API | Flash is a pure function; the phase hint is a per-call argument |
| **Imposed phase handled inconsistently** | Honored by PT/DT/T+X. Ignored by DP (FR:410-413, :503-504; oracle: imposed gas at a two-phase (d,p) -> two-phase). **Cleared** by PH (FR:3183, :3192, :3306), legacy DH (FR:1802-1811), the D+X SA `restore_inputs` (FR:1875), HS (`HSGasGuard`, FR:4325-4327). Oracle: impose gas -> PH -> PT(1 bar, 300 K) returns liquid. Bead CoolProp-1tbe.8 ("phase-imposition leak"), CoolProp-9ie | Unpredictable metastable control | `PhaseHint` per call, a per-pair support matrix, `Err(Unsupported)` |
| **T+X pairs non-unique; CoolProp silently picks two-phase** | Oracle Water 300 K: HT for compressed liquid at 1/10/50 MPa returns two-phase at psat = 3536.8 Pa with Q = 0.00038/0.0038/0.0185. Propane 250 K/5 MPa likewise. Water 275 K: **ST and UT** also return two-phase (`(ds/drho)_T > 0` below ~277 K). Mechanism: T-based SA lever rule tested before any liquid branch (HB:2281-2315). `ConsistencyPlots.py:17` excludes HmolarT/TUmolar as "not implemented" | Wrong state, wrong p by orders of magnitude, no error | Detect `(dX/drho)_T > 0` on the liquid branch at saturation; return `Err(Ambiguous{..})` or use `RootPolicy` / `PhaseHint` |
| **Q-pair strict path ignores `ENABLE_SUPERANCILLARIES`** | `sat_superanc_path_applies` and `resolve_T_via_superancillary` never read the config (FR:558-564, :712-841); `EquationOfState::get_superanc` builds regardless (`CoolPropFluid.h:433-438`). Oracle with SA off: HQ still throws the SA MultipleSolutions error; DQ uses SA roots then Maxwell QT. Same on master (FR:706-712) | Config does not mean what it says; mixed SA/EOS states | `SatSource` chosen explicitly per call or model; one code path per source |
| **`PT_flash_with_guesses` discards the solver result and validates nothing** | `HEOS.solver_rho_Tp(HEOS.T(), HEOS.p(), guess.rhomolar);` (FR:1430; master FR:1666). Oracle: Water 10 MPa/300 K with guess 0.9 rho lands on **dp/drho = -2.95e5 < 0**, rho -14.2%, h -1104 J/mol, labeled liquid. The gas case leaves rho one step stale (-6.5e-10) | Silent unphysical states | Guess = seed only; result passes the same `verify()` (stability, phase) |
| **Imposed two-phase T+X is broken** | Loads SatL/SatV at ancillary densities and uses stale SatL/SatV for D (FR:3822-3866). Oracle: fresh state, impose two-phase, DT(1000, 400 K) -> "rhomolar is less than zero"; HT -> rho off 1.1e-4 vs QT. Same on master (FR:4135-4141) | Errors or inconsistent states | Two-phase states come only from SA/VLE lever rule |
| **Exception-driven cascades over mutable state** | Dozens of `catch (...)` legs; manual restores (FR:1866, :4830); empty try/catch (FR:969-974). Inner solves corrupt `_T`/`_p` (FR:38-40, :2810-2817) | Fragile ordering, masked errors | Ordered `Result` strategies over an immutable model plus one `verify()` |
| **Solver result vs state mismatch** | Halley/Householder return `x+dx` with state at `x` (SV:196-199). Brent can return `a` after evaluating `b` (SV:536-537). TOMS748 midpoints are never evaluated, so callers re-evaluate ("stale alphar derivatives (#1907)", FR:2263-2267, :3172-3177, :477-478). PT reports p ≠ input: oracle Nitrogen 10 MPa/200 K gives 3.3e-9 relative (master 3f06ccf7) | Stale-cache bug class; oracle p after PT is not the input | Solvers return `Root{x, f, iters}`; the state is built from a pure `state_at(T, rho)` at the returned x |
| **Silent non-convergence** | No TOMS748 call site checks `max_iter` (grep). NDNewton keeps looping at maxiter with `x0[0] = _HUGE` and exits on a tiny step with a large residual (SV:82-93; acknowledged at FR:3271-3274) | False convergence, misleading errors | `Err(NoConvergence{..})`; converge on residual and step together |
| **Absolute tolerances over 10+ decades** | T>Tc rho solve uses Brent `t = 1e-9 mol/m³` (FR:3666, :3690). Oracle Water 1.3 Tc: **ST round-trip rho error 6.1% at rho=1e-8**, 8.9e-4 at 1e-6; HT 1.2e-4 at 1e-8 (verifier rerun at T = 1.3 Tc_num; the first pass reported 5.6e-4). Fixed on master e33bf27a (log-rho axis, ftol 1e-12) | Wrong low-density states | Relative / log-space tolerances as types; log axis for entropy |
| **SA domain not enforced; DT below the triple point extrapolates** | Upper check only (FR:898-902, :1164-1168); lookup extrapolates (SA.h:697-721); T<Tc never checks Ttriple in T-based SA (HB:2229-2316). Oracle: QT 250 K -> p = 95.25 Pa, 200 K -> "rhomolar is less than zero", PQ 100 Pa -> 250.55 K; **DT(55000 mol/m³, 250 K) labeled two-phase at the extrapolated psat**. SA off: QT 250 K and PQ 100 Pa give range errors, but DT(55000, 250 K) is *still* labeled two-phase (p = 1.1e-4 Pa, Q ≈ 2e-24; verifier oracle run). So the missing Ttriple check in T-based determination is independent of the config. Test only checks the upper bound (T.cpp:3870-3874) | Fabricated states in both modes; the QT/PQ contract depends on config | Domain-checked `eval -> Result`; extrapolation opt-in |
| **Inconsistent range policy** | PT accepts T = 3000 K and 1e5 K for Water (Tmax 2000 K, oracle). DP rejects T > 1.5 Tmax (FR:493-495). P+X brackets to 1.5 Tmax. PT at 5 GPa fails inside the melting-line evaluator instead of a pmax error | Unpredictable validity | One `Domain` check per model with explicit extrapolation flags |
| **Config-dependent physics, global config** | Critical constants switch between SA numerical and published values (HB:1207-1249; T.cpp:3888-3901). QT at the published Tc throws (oracle). Tests flip the global map via RAII fixtures (T.cpp:3430-3460) | Results depend on process state; tests serialize | `CriticalPoint{published, numerical}`; explicit `FlashOptions` |
| **PT near saturation depends on T** | Switch at FR:319: below `0.9 Ttriple + 0.1 Tc` the p-based path accepts \|p/psat-1\| = 1e-7, above it HB:2245 throws within 1e-6. Exactly at psat the p-based branch throws "bad input for other" (HB:1815-1836). Oracle Water: 305 K liquid, 315 K throws | Inconsistent API | One rule and an explicit `AtSaturation` error |
| **Q-pair asymmetries** | HQ rejects Q≠1 before the SA gate (FR:608) while QS accepts Q=0. DQ fractional-Q bracket `Tc-0.1` (FR:589-590): oracle fails at Tc-0.05 K. QS critical shortcut ignores Q and compares to the *reducing* entropy (FR:641). Legacy two-phase D+X/HS: Brent capped at 20 iterations and `Tmax_sat-0.01` (FR:1705, :4235) | Valid inputs rejected | Symmetric SA-based Q-pairs bracketed to Tc_num |
| **Strict-mode cliff** | Default HQ (Q=1) on Water throws for any rising-branch h (oracle 400 K: roots 400 K and 587.912 K). R1234ze(E) `s''` has 3 roots (oracle error lists 267.3, 275.6, 363.7 K; extrema 271/338 K) | Usability | `RootPolicy {Strict, Nearest(T), All}`, roots returned as data |
| **Format bugs throw untyped errors** | FR:494 (`%g` with no argument), FR:3692 (4 specs, 3 args). Oracle `MethylOleate ST(502.386, 789.75)` -> `RuntimeError: argument not found`. Fixed on master 42bc0667 | Lost diagnostics; docs build aborted | `format!` is compile-checked; error enums |
| **NaN-blind quality checks** | `(Q<0)\|\|(Q>1)` (HB:1547 etc.), `\|Q-1\|>1e-10` (FR:608). Master 0f978943: HmolarQ Q=NaN SIGSEGV (Propane); `update_HmolarQ_with_guessT` has no check (HB:1398); master aa7c6079 centralizes the check | Crash or misleading error | `Quality::new(f64) -> Result` |
| **Hot-path locks, refcounts, atomics** | SA mutex per `get_T_from_p` / stamp / ensure (SA.h:1021-1086, :1272); `shared_ptr` copies (HB:314); `deriv_counter` RMW per evaluation (HB:3554) | Poor many-thread scaling on one fluid (inference, not measured) | Precomputed data, borrowed `&SatCurves`, no global counters |
| **Eager load and first-use latency** | ~1.8 s of import is SA construction for 130 fluids (oracle). Lazy caloric build 45-63 ms, `invlnp` 2.2-2.7 ms per fluid (SA.h:1127-1193, :923-952) | Startup and tail latency; violates "load only what is needed" | Offline binary SA with precomputed extrema, inverse and calorics; per-fluid lazy load |
| **Heavyweight state** | 3 fluid copies per state (HB:114, :141-150); ~73-135 KiB/state measured (the retained SA JSON string adds no measurable per-state cost, §5); temporary backends in flashes (HB:2029, :2447; FR:3827) | Memory, allocation, construction 52-58 µs | Shared `Arc<FluidModel>`; `State` is ~100 B of plain data |
| **SA latent defects** | Derivative trim keeps `head(cd, ilastnonzero)` and drops the last nonzero coefficient (SA.h:515-524), reached by the 1,386 zero-top pieces; verified to change no extrema in shipped data. Balanced matrix computed then discarded (eigenvalues of the unbalanced matrix, SA.h:536-539). Degree-0 Clenshaw returns `c0(1+x)` (SA.h:309-318). Degenerate 1-2 coefficient expansions segfault the constructor (bead CoolProp-1tbe.21). Caloric degree hard-coded 12 (SA.h:1129). Chlorine domain/Tc mismatch (§3.3) | Latent wrong extrema / crash on new data | Property tests on the Chebyshev module; validate SA data at build time |
| **Dead or broken code** | FR:2712-2801, :4239-4297, FH:252-307. `iP` branches of `HSU_D_flash` are unreachable (callers HB:1509-1519 pass only H/S/U; FR:2211-2215, :2273-2289, :2317-2319, :2363-2365). `SuperAncillary::solve_for_Tq_DX` is never instantiated and intersects the wrong property (SA.h:1419), writes `Lrho.xmin * 0.999` without calling the member function, which would not compile if the template were ever instantiated (:1426), `std::swap(y2, y2)` (:1430). `if (false)` (SA.h:566). A 176-line commented `flash()` (SA.h:1462-1645). Unused ODE/Romberg. 25 of 26 `HS-prototypes` tests hidden | Maintenance load, traps | Port only reachable behavior |
| **Misleading diagnostics** | "Pressure to PQ_flash" raised from PH/PS brackets (FR:3370, :3401) and determination (HB:1792-1793). "I should never get here" (FR:436). Maxwell docstring truncated (VLE:935-939) | Debugging cost | Contextual error enums |
| **Recomputed derivatives** | `SolverTPResid` calls `calc_d3/d4alphar` (full re-evaluation) instead of cached values (HB:2813, :2818): 6 evaluations per solve where 2 suffice (master 3f06ccf7). `calc_*_nocache` evaluate full bundles 2-4x (HB:3177-3179, :3224-3227) | Slow PT, phase determination, SA build | One derivative bundle per (T, rho) |
| **Legacy heuristics** | 2%/5% bands and the `0.95/1.05` test (HB:2326, :2359-2390); ExtrapolatingSecant fallback on ancillary inversion (`Ancillaries.cpp:106-112`); liquid seed uses the ancillary even with SA (HB:2934) | Misclassification for non-SA fluids | SA for pure fluids; explicit uncertainty band -> EOS VLE |
| **Oracle defects (mixtures)** | Blind mixture PT returns spurious middle roots, e.g. CH4/C2H6/C3H8 [.05,.9,.05] at 2 MPa, 212.5 K: rho 6854 vs 16869 (master b17661f6, #3283; `CoolProp-Tests-PTFlashMiddleRoot.cpp` master-only) | Oracle wrong at these points | Area 04 |

**Known-fragility baseline.** At v8.0.0 the open epic CoolProp-r1w7 records 12,355 flash-consistency failures across 82 fluids (devdocs report, CoolProp 7.2.1dev, 2026-06-10). 71% are exceptions; the largest bucket is ~4,860 "PHSU fails to bracket". The near-v8 137-fluid sweep had 3,614 -> 3,192 inconsistent points and 795 -> 780 exceptions (master 3f06ccf7 message). Open beads at v8.0.0: CoolProp-br8q (Air near-critical D+X), -l34 (Air HS), -j3n.2 (OrthoHydrogen D+X at GPa), -uqvr (always-bracketed density solve), -r1w7.3 (Water/HeavyWater SmolarT), -sxc (QS_flash segfault), -0nx (P+X solver); -1tbe.8 (imposed-phase leak, silent non-convergence) is in_progress, not open.

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

| Algorithm | Fit | Structure and side-by-side design |
|---|---|---|
| SA evaluation, one T, many curves | **SIMD, intra-call** | All curves share pieces (verified for all 130 fluids), and the caloric curves are built on the same pieces (SA.h:1156-1174). One piece lookup then up to 9 independent 13-term Clenshaw chains (rho', rho'', p, h', h'', s', s'', u', u'') in lockstep: 3 AVX2 or 2 AVX-512 vectors, 5 NEON/WASM-SIMD128 vectors. Store per-piece SoA blocks `coef[13][K]`. Helps every single call (QT, PQ, classification) |
| SA evaluation, many T | **SIMD / data-parallel** | AoSoA gather or sort-by-piece. Branch-free lookup: Eytzinger search, or a dyadic index (boundaries are dyadic, depth <= 24). CoolProp runs 42 ns/eval scalar |
| QT, PQ (precomputed `T(ln p)`), DT/PT classification, caloric lookups | **SIMD** | Pure evaluation, no iteration once the inverse and calorics exist |
| rho(T,p) inside SA brackets | **Lockstep SIMD with masks** (same fluid) | Fixed-budget Newton/Halley per lane inside `[rhoV, ...]` / `[rhoL, rho_max]` brackets; converged lanes masked; failed lanes retry on the scalar TOMS748 path. Needs the EOS SoA derivative kernel (area 02), the dominant cost |
| DP (Halley in T), T+X single phase | Lockstep feasible | Same pattern; HT/UT need log axes and ambiguity checks first |
| P+X | Branchy, nested (outer T, inner rho) | Task-parallel across requests; SIMD only inside the EOS kernel. A 2-D (T, rho) Newton batch is possible for warm starts but CoolProp reverted a 2-D P+X approach (master agent-notes) |
| D+X interval method, SA root enumeration | Branchy | Precompute inverse expansions per monotonic interval offline, so inversions become evaluations (SIMD-able) |
| HS homotopy cascade (4 legs, adaptive substeps) | **Sequential, branchy** | Scalar only; rayon across requests. Not worth a SIMD variant |
| TOMS748 / Brent | Sequential, data-dependent branches | Scalar reference; batch via per-lane scalar loops. Divergent iteration counts make lockstep wasteful |
| Legacy VLE (Maxwell/Akasaka), mixture stability, SS | Branchy; small dense linear algebra | Per request; N <= 20 |
| SA building (dyadic split, companion eigenvalues, caloric sampling) | Build-time, parallel per fluid | Offline tool, not runtime |

- **Side-by-side implementations.** Put numerical kernels (Chebyshev curve bundle, EOS derivative bundle, root step) behind a small trait with a **scalar reference implementation** that is the TDD oracle target, plus optional SIMD variants (`std::simd`, a small crate, or `core::arch` per ISA) selected by feature or runtime detection. Flash *logic* (classify, bracket, verify) stays scalar and generic over the kernel.
- **Bit-parity caveat.** Bit-level parity with the oracle needs CoolProp's operation order without FMA contraction (Clenshaw, SA.h:313). SIMD/FMA variants will differ in the last ULPs, so test them against the scalar Rust reference with ULP bounds, not against CoolProp bits (inference: assumes the x86-64 wheel is built without FMA).
- **Threads.** CoolProp's only parallelism is one backend per thread. The SA mutex, `shared_ptr` refcounts and the global `deriv_counter` are the shared-write points for many concurrent requests on one fluid (inference, not benchmarked). An immutable `Arc<FluidModel>` with precomputed SA has no shared writes.

## 8. Verification assets

- **Unit tests (v8.0.0)**:
  - `CoolProp-Tests-PXFlash.cpp:34`: N2 supercritical-cold PS at 86.35 K/4.754 MPa and 90.20 K/7.768 MPa, 1e-5 relative. `:64`: PH/PS/PU round trips. `:124`: quantum fluids at high pressure.
  - `CoolProp-Tests-PXcdj.cpp:186`: named regressions for Water/CO2/R134a/Propane/N2/MM at 1e-5. The sweep `:234` is hidden.
  - `CoolProp-Tests-HS.cpp`: `:39` two-phase 25 T x 5 Q for 6 fluids; `:86` single-phase 20x20 (log p, T) grid for 8 fluids; `:149` pseudo-pure at 2e-4.
  - `CoolProp-Tests-HSU_D.cpp`: issue-mined cases #2486, #2157, #1698, #1054, #2154, #2173, #1965, #2022, #2685, #2426; the water anomaly `:417`; the polish toggle `:462`; `balance_matrix` termination `:607`. The dense sweep `:514` is hidden.
  - `CoolProp-Tests-AirCritical.cpp:33` (pseudo-pure D+X).
  - T.cpp: PT solvers `:1495`; PT at the critical point `:1530`; PY `:1557`; flashdups `:1694/:1723`; consistency `:768` (loose: T 1e-2 K, p 1e-2%, `:727-729`); SA fixtures `:3477-3970`; multiprecision check points `:3834`; `source_eos_hash` `:3598`; water flash regressions `:5181-5823`; #2773 branch/strict/thread tests `:5896-6248`.
- **Master-only**: `CoolProp-Tests-PTFlashMiddleRoot.cpp` (mixture wrong root); `CoolProp-Tests-DeltaOnly.cpp`; `dev/agent-notes.md`. The agent notes say: HS acceptance needs dp/drho>0 **and** cv>0; generate flash test points in (p,T), never in rho bands; P+X is a 1-D problem; do not differentiate fitted Chebyshev expansions when a closed form exists.
- **Reference data**:
  - SA `check_points` (3 x 130 multiprecision p, rho', rho'').
  - fastchebpure releases (pinned 2026.08.29 on master).
  - `Web/coolprop/SuperAncillary.ipynb` and `HSFlash.ipynb`.
  - `wrappers/Python/CoolProp/Plots/ConsistencyPlots.py`: 15-pair round-trip grids; skips SU, HU (no such pair), TU, HT.
  - `dev/scripts/check_superanc_freshness.py`.
- **Oracle hooks**:
  - `AbstractState.update`, `specify_phase`, `update_with_guesses` (`PyGuessesStructure`), `update_QT_pure_superanc`.
  - `CP.SuperAncillary(json).eval_sat/eval_sat_many`, for bit-level SA checks.
  - `CP.ChebyshevApproximation1D([...]).monotonic_intervals()/get_x_for_y`.
  - `set_config_bool(ENABLE_SUPERANCILLARIES, False)` for the EOS-VLE path (but see the Q-pair caveat).
- **Oracle caveats**:
  - Use a **fresh AbstractState per query**.
  - `p()` after PT is not exactly the input (~1e-9).
  - Saturation is SA, not EOS VLE: use per-fluid tolerances from the check-point ratios, ~1e-5 for rho near Tc.
  - SA-off near Tc uses critical splines (3.5% errors, R134a), so it is not an EOS reference there.
  - T+X results in the liquid are two-phase by construction (ambiguous pair).
  - ST at low density is wrong at v8.0.0.
  - DT below Ttriple uses extrapolated SA, and with SA off it is still labeled two-phase.
  - Mixture PT middle roots.
  - Four reducing densities were corrected after v8 (master 2acbbc82: Nitrogen, Ethylene, OrthoHydrogen, n-Undecane; SA refit 66859efb). For those fluids the literature arbitrates.
- **Equilibrium self-check**: for any SA point, `p(T, rho') = p(T, rho'')` and `g' = g''` from the EOS give a source-independent check (Δg/RT ~ 1e-13 to 1e-15 observed).

## 9. Port recommendation

| Unit | CoolProp source | Priority | Est. Rust LOC | Notes |
|---|---|---|---|---|
| `roots`: native TOMS748, bracketed (rtsafe-style) Newton/Halley/Householder, secant, bracket expansion; `Result<Root>` with status/iterations; value+derivative tuples | SV; boost TOMS748 | P0-core | 450 | No boost; relative/abs/log tolerances; always return the evaluated point |
| `cheb`: `ChebPiece<13>`, curve bundles with shared piece index, scalar Clenshaw (no FMA, oracle order), extrema and monotonic intervals, all-roots, inverse expansions | SA.h:56-800 | P0-core | 450 | Guard degree < 2; correct the trim; balance-or-not decided by tests |
| `superancillary`: rho'/rho''/p, Tc_num/pc_num, `T(ln p)`, caloric H/S/U + offset shift, check points, domain-checked eval, `RootPolicy` | SA.h:801-1450, `src/superancillary.cpp`, FR:530-841 | P0-core | 450 | Binary data with precomputed extrema/inverse/calorics; per-fluid lazy load |
| `flash::types`: `Spec`, `PhaseHint`, `PhaseState`, `State`, `FlashOptions`, `FlashError`, `Quality`, `Tol`, `verify()` | HB:1455-1700; FR acceptance helpers | P0-core | 350 | Single acceptance gate per pair |
| `flash::sat`: QT, PQ, DT/PT classification | FR:888-912, :1157-1181; HB:1784-1857, :2231-2316 | P0-core | 200 | First oracle TDD target |
| `flash::density`: rho(T,p) bracketed by SA densities and rho_max/melting (CoolProp-uqvr); Householder as accelerator; SRK seed only without SA | HB:2787-3149 | P0-core | 350 | Root uniqueness by construction |
| `flash::pt` (pure) | FR:298-341 + determination subset | P0-core | 150 | One saturation rule |
| `flash::tx`: HT/ST/UT with log-rho axis for S and ambiguity detection | FR:3609-3800, :3802-4204 (pure) | P1-early | 300 | Adopt master e33bf27a; return `Ambiguous` |
| `flash::dp` | FR:344-505 | P1-early | 120 | Honor `PhaseHint` |
| `flash::px`: PH/PS/PU, outer T TOMS748 with warm-started inner density | FR:2802-3440 | P1-early | 450 | Brackets from SA and melting line |
| `flash::qx`: DQ/HQ/SQ for any Q | FR:507-886 | P1-early | 200 | Symmetric Q support; roots as data |
| `flash::dx`: D+H/S/U interval method | FR:1710-2113 | P1-early | 350 | The interval classification is sound; keep it |
| `flash::hs`: two-phase screen + legs 1-3 + corrector | FR:4205-4238, :4298-4944 | P1-early | 600 | Port the cascade; drop the legacy scan |
| `vle::pure_eos`: Maxwell/Akasaka | VLE:80-1128 | P2-later | 450 | Fallback and verification against SA check points |
| Pseudo-pure ancillary saturation + null-SA HS cascade | FR:952-967, :1183-1199, :4890-4944 | P2-later | 250 | 6 fluids |
| Melting legs (sub-triple liquid D+X, HS leg 4) | FR:2386-2488, :4684-4710, `MeltingCaloric.cpp` | P2-later | 250 | Keyed by model identity, not name |
| Batch / SIMD kernels (curve bundles, masked lockstep density) | new | P2-later | 400 | Scalar is the reference |
| SA builder (dyadic split, companion extrema, caloric sampling) | SA.h:151-255, :923-952, :1127-1193 | defer | 400 | Offline data tool |
| Mixture flashes | FR:24-297, :1454-1670, mixture branches | defer | - | Area 04 |
| Cubic SA, PolyMath, critical splines | `cubicsuperancillary.h`, PolyMath, `CoolPropFluid.h:427` | defer | - | Areas 06/07; splines superseded by SA |
| Dead code, ODE, Romberg, ExtrapolatingSecant, Gauss-Jordan, env switches, `Q` sentinels, mutable imposed phase, global SA flag | §6 | drop | 0 | |

**Order:** `roots` -> `cheb` (bit-check against `SuperAncillary.eval_sat` and check points) -> `superancillary` -> `types`/`verify` -> QT/PQ/DT -> `density` + PT -> T+X -> DP -> P+X -> Q-pairs -> D+X -> HS -> P2.

**Redesign sketch:**
```rust
pub fn flash(m: &PureFluid, spec: Spec, o: &FlashOptions) -> Result<State, FlashError>;
pub fn flash_batch(m: &PureFluid, specs: &[Spec], o: &FlashOptions, out: &mut [Result<State, FlashError>]); // rayon chunks; SIMD kernels inside
pub enum Spec { PT{p,t}, DT{rho,t}, HT{h,t}, ST{s,t}, UT{u,t}, DP{rho,p}, HP{h,p}, SP{s,p}, UP{u,p}, HS{h,s}, DH{rho,h}, DS{rho,s}, DU{rho,u}, QT{q,t}, PQ{p,q}, DQ{rho,q}, HQ{h,q}, SQ{s,q} } // molar SI newtypes
pub struct FlashOptions { phase: PhaseHint, guess: Option<Guess>, roots: RootPolicy, sat: SatSource, tol: Tolerances, allow_extrapolation: bool }
pub enum PhaseState { Liquid, Gas, SupercriticalLiquid, SupercriticalGas, Supercritical, CriticalPoint, TwoPhase { q: Quality, liq: SatPoint, vap: SatPoint } }
pub enum FlashError { OutOfDomain{..}, AtSaturation{..}, MultipleSolutions{roots: Vec<Candidate>}, Ambiguous{..}, NoConvergence{..}, Unsupported{..} }
trait Strategy { fn try_solve(&self, ctx: &Ctx, spec: &Spec) -> Result<Candidate, Rejection>; } // ordered per pair; verify() gates
trait CurveKernel { fn eval_bundle(&self, piece: &PieceBlock, t: f64, out: &mut [f64]); } // scalar reference + SIMD variants
```

## 10. Open questions

1. SA-first saturation (oracle parity, bit-compatible Clenshaw) with EOS VLE kept for verification, or EOS-VLE-first with SA as accelerator? This document recommends SA-first; the SA is also the better reference at very low pressure (§3.3).
2. Where are calorics, the `T(ln p)` inverse and the monotonic intervals produced: offline in a Rust fastchebpure-like data tool, or once at load under `OnceLock`?
3. Policy for metastable states and SA extrapolation below `Tmin` (supercooled liquid; DT below the triple point): allowed only through an explicit hint?
4. Is PT exactly at saturation an error (CoolProp) or an explicit "saturated, ambiguous" result?
5. Default for ambiguous inputs (Q-pairs with several roots; T+H in compressed liquid; T+S/T+U for cold water): strict error, all roots, or a phase-preference default? Parity with CoolProp would silently return two-phase.
6. Tolerance targets per pair and region. CoolProp acceptance is often 1e-6 relative (+1e-3 absolute), SA claims ~1e-12 away from Tc but only ~1e-5 for rho near Tc, and caloric SA precision is ~3e-9.
7. Pseudo-pure fluids: port the ancillary path, or fit SA-like bubble/dew curves?
8. Which post-v8.0.0 master fixes become the spec when the oracle disagrees (log-rho ST, PT p restore, format fixes, Q validation, mixture root selection), and how are deviations recorded with literature evidence?
9. SIMD route under the minimal-dependency goal: `std::simd` (nightly), a small crate, or `core::arch` per ISA; and how to keep WASM (SIMD128) builds first-class.
10. Keep the 4-leg HS homotopy, or can SA-bracketed 1-D formulations along isentropes replace legs 1-2?
11. Which critical point is exposed by default (published vs numerical), given that QT at the published Water Tc fails in CoolProp?

## Verification log

- Date: 2026-10-04. Adversarial verification against the v8.0.0 checkout (ae81610e) and the CoolProp==8.0.0 wheel.
- Claims checked: about 240. That is ~200 path:line citations opened in source (every function anchor in §2, every rot-table citation, and the §3-§5 solver, phase-determination, SA and config lines), 12 recounts and 30 oracle reproductions.
- Recounts confirmed: FR+FH 5,464 lines; HB 4,599; numerics 5,386; SA 2,469; MeltingCaloric 375 (266 cpp + 109 header); flash tests 3,431. 136 fluids, 130 with SA; the 6 without are Air, R404A, R407C, R410A, R507A, SES36. All pieces are degree 12 with 53-91 pieces per curve (median 63); boundaries are identical for all 130 fluids; 8,032 interior boundaries; 1,386 zero-top pieces; 318,318 coefficients; 58.8-100.6 KB of JSON per fluid (9.05 MB total); 390 check points (worst p 4.7e-9 MethylStearate, median 4.8e-14, rho better than 4.5e-13); Chlorine `xmax` is 1.02e-4 K below Tcrittrue; cubic SA is degree 18 (99 pieces); 25 of 26 HS-prototype tests are hidden; FR has 16 `get_config_bool` call sites.
- Oracle reproductions confirmed: QS leak -> PT "Bad phase to solver_rho_Tp_SRK" and PH(30 MPa) -> 270.79 K; DQ leak -> DT two-phase; impose gas -> PH -> PT liquid; DP ignores the imposed phase; HT Water 300 K two-phase at 3536.8 Pa with Q 0.00038/0.0038/0.0185; ST/UT Water 275 K two-phase; QT 250 K = 95.25 Pa; QT 200 K throws; PQ 100 Pa -> 250.55 K; DT(55000, 250 K) two-phase; QT at 647.096 K throws (Tc_num 647.0959999999873); PT at 305 K liquid vs 315 K throws; HQ Water roots 400/587.912 K; R1234ze(E) s'' has three roots; MethylOleate ST "argument not found"; PT_with_guesses -14.2% with dp/drho = -2.95e5; N2 PT p off by 3.3e-9; ST at 1.3 Tc 6.1% and 8.9e-4; imposed two-phase DT "rhomolar is less than zero" and HT 1.07e-4; DQ fails at Tc-0.05 K; HQ with SA off still raises the SA MultipleSolutions error; PT at 3000 K and 1e5 K accepted; PT at 5 GPa fails in the melting line; first PQ costs 2.2 ms.
- Master commits aeea05e4, e33bf27a, 66859efb, 2acbbc82, c17783e6, 0f978943, aa7c6079, b17661f6, 42bc0667 and 3f06ccf7 exist and are not ancestors of v8.0.0. Their messages match the claims. The cited beads exist with the stated topics.
- No partial edit from an earlier verifier was found: there was no prior log section, and all tables had consistent columns.
- Corrections:
  1. §1: the dispatcher reaches 11 `FlashRoutines` functions from 18 molar pairs, not 13.
  2. §2: PolyMath.h has ~800 comment lines (746 `//` lines plus doc blocks), not 746 in total.
  3. §3.1 T+X: the rhoc bracket growth is at most 31 x1.1 steps (~19x), not 30x.
  4. §3.1 DQ: the timing "~1-5 (§5)" pointed to a section with no such data. It is now marked unsourced.
  5. §3.3: added that all 60 non-dyadic boundaries belong to Chlorine (verified).
  6. §3.3: R134a spline error is 3.2% in a rerun (doc said 3.5%); the stated range is now 3.2-3.5%.
  7. §3.4: Halley/Householder4 stop at `xtol_rel` (default 1e-12); Newton hard-codes 1e-11.
  8. §4: HB has 14 `get_config_bool` call sites, not 15.
  9. §5: `get_superanc()` has 19 call sites in FR+HB, not 23. The bullet now notes that the SA-enabled critical-point getters pay it too.
  10. §5: `deriv_counter` is bumped only on the cached alphar bundle path, not on every bundle evaluation.
  11. §5 and §6 "Heavyweight state": the claim that each state deep-copies the retained SA JSON string was refuted by measurement. Heap per state is the same with SA disabled (Δ < 0.12 KiB for Water and n-Propane), so the claim was removed and the cause marked unexplained. The RSS range is widened to ~73-135 KiB.
  12. §5: added a rerun note on import time (2.6-4.4 s vs 0.34 s on a loaded machine; the ratio holds).
  13. §6 "SA domain not enforced": "SA off: range errors" was only partly true. With SA off, DT(55000, 250 K) is still labeled two-phase (p = 1.1e-4 Pa), so the defect does not depend on the config. The impact text and the §8 oracle caveat were updated.
  14. §6 "Absolute tolerances": HT error at rho = 1e-8 reproduces as 1.2e-4, not 5.6e-4. The ST figures were confirmed.
  15. §6 known-fragility: CoolProp-1tbe.8 is in_progress, not open.
  16. §6 dead code: clarified the `Lrho.xmin * 0.999` defect (the member function is named without being called, so the template would not compile if instantiated).
- Rot items that survived refutation unchanged: imposed-phase leak (still on master), inconsistent phase imposition, T+X non-uniqueness, Q-pair strict path ignoring the config (still on master), PT_with_guesses (still on master), broken imposed two-phase T+X (still on master), exception cascades, solver/state mismatch, silent non-convergence, inconsistent range policy, config-dependent critical point, PT saturation rule depending on T, Q-pair asymmetries, strict-mode cliff, format bugs (fixed on master), NaN-blind quality checks (fixed on master), hot-path locks, eager load, SA latent defects, dead code, misleading diagnostics, recomputed derivatives, legacy heuristics.
- Not re-verified: the per-pair µs timings in §3.1, `eval_sat_many` ns/eval, the SA-vs-Maxwell median p differences (120 T per fluid), the Propane 86 K outlier, the caloric build times, and the 52-58 µs construction cost (the verifier saw 80-180 µs on a loaded machine). Treat these as indicative.
