# 04 Saturation / VLE, critical points, mixtures and phase envelopes - CoolProp v8.0.0 map

> Scope: `src/Backends/Helmholtz/{VLERoutines,MixtureDerivatives,ReducingFunctions,MixtureParameters,PhaseEnvelopeRoutines}.{h,cpp}`, `ExcessHEFunction.h`, `include/CoolProp/fluids/PhaseEnvelope.h`, the mixture, critical-point and stability parts of `HelmholtzEOSMixtureBackend.{h,cpp}`, the VLE entry points of `FlashRoutines.cpp` (QT, PQ, PT, the mixture branch of DHSU_T), `dev/mixtures/`, `Web/mixtures/` and `src/Tests/CoolProp-Tests-Michelsen.cpp`. About 15k lines: 10.8k in the dedicated files, about 1.3k in the backend, about 1.4k in FlashRoutines, plus 2.1k lines of Michelsen tests. Part of the coolprop-rs port plan; cites the v8.0.0 source (ae81610e, tagged 2026-06-27).
>
> Markers: **[oracle]** means measured with the PyPI wheel `CoolProp==8.0.0` on the planning machine (Python, single thread). The scripts live in the session scratchpad and are not part of the plan. *(inference)* means reasoned from the code but not executed. "Upstream X" means commit X in `v8.0.0..origin/master` (151 commits up to 2026-10-03).

---

## 1. Purpose and concepts

| Concept | What CoolProp v8.0.0 does |
|---|---|
| Pure-fluid VLE | Given T (or p, h, s, u, ρ'), find ρ', ρ'' and p such that p' = p'' and g' = g''. The default path for pure fluids is the **superancillary**: Chebyshev expansions of ρ'(T), ρ''(T) and p(T), built in extended precision. Six iterative EOS solvers remain in use: Maxwell, Akasaka, PHSU, D and two 1-D fallbacks. They cover the non-T inputs, fluids without a superancillary, and runs with `ENABLE_SUPERANCILLARIES=false`. |
| Pseudo-pure (Air, R404A, R407C, R410A, R507A, SES36) | There is no true VLE. QT accepts only Q=0 or Q=1 and takes pL/pV from ancillaries (`FlashRoutines.cpp:952-975`). PQ returns T = Q·T_V + (1−Q)·T_L from two ancillary inversions (`:1183-1199`). |
| Multi-fluid mixture model (GERG type) | αʳ(τ,δ,x) = Σᵢ xᵢ αʳ₀ᵢ(τ,δ) + Σᵢ<ⱼ xᵢxⱼFᵢⱼ αʳᵢⱼ(τ,δ), with τ = T_r(x)/T and δ = ρ/ρ_r(x). T_r and v_r come from GERG reducing functions (β_T, γ_T, β_v, γ_v); Lemmon ξ/ζ pairs are converted to GERG form at load. The ideal part is Σ xᵢ(Rᵢ/R)[α⁰₀ᵢ + ln xᵢ] (Kunz & Wagner 2012, Table B5). **"HEOS mixture" is not GERG-2008**: the library mixes GERG-2008, EOS-CG (Gernert 2013), Bell 2016 fitted pairs, Lemmon 2004 and newer refrigerant work (§4). |
| Composition derivatives | ln φᵢ, n∂lnφᵢ/∂nⱼ, ∂lnφᵢ/∂T at constant p, ∂lnφᵢ/∂p at constant T, partial molar volume, and the 3rd-order tensors needed for critical points. All are hand-coded across 181 static functions. A runtime flag selects between two conventions: `XN_INDEPENDENT`, where all xᵢ are free, and `XN_DEPENDENT`, where x_N = 1−Σ. The dependent convention is only partly implemented (§2.3). |
| Mixture VLE | Bubble and dew points (Q = 0 or 1), interior-Q two-phase states, and the blind PT flash: Michelsen TPD stability test, then a phase split, then single-phase root selection. |
| Critical points and spinodal | Heidemann–Khalil criticality: det L* = 0 and det M* = 0. The code traces the L1* = 0 contour (the spinodal) in (τ,δ) and brackets critical points by sign changes of M1*. |
| Phase envelope | A dew-type continuation. The bulk vapour density ρ'' is imposed and marched multiplicatively. The unknowns are the incipient composition x, T and ρ'. The trace starts at 100 Pa. |
| Architecture | **One class, `HelmholtzEOSMixtureBackend`, serves both pure fluids and mixtures.** A pure fluid is N=1: a GERG reducing function with all-ones parameters, a corresponding-states sum over one component and an empty excess term. `HelmholtzEOSBackend` adds only a name-based constructor that also expands predefined `.mix` names (`HelmholtzEOSBackend.h:30-68`). |

**Key takeaways for the port.**
1. Mixture VLE in v8.0.0 is slow, ranging from milliseconds to hundreds of milliseconds per call [oracle].
2. It is under heavy repair upstream. Since the tag, 8 `fix(flash|mixtures|HEOS)` commits have touched this area (3f16d5fb, b63ea865, b17661f6, 014c8186, 42bc0667, 1fca2b92, 552b4711, aeea05e4). Two further fixes, ae54172f and 15ede127, also touch it, as do 3 perf commits (c6d414e4, 3f06ccf7, d6e52450).
3. The oracle is demonstrably wrong at several states (§6, §8).
4. The mixture *model* is sound apart from a few bugs, and it fixes the shape of every core trait. Port the model early and the VLE algorithms later, and rebuild those from the literature.

---

## 2. Structure (key types/functions -> path:line)

### 2.1 Pure-fluid saturation (`SaturationSolvers`, `VLERoutines.h:8-656`)

| Symbol | Location | Role / callers |
|---|---|---|
| `saturation_T_pure` | `VLERoutines.cpp:756-777` | Chain: Maxwell, then Akasaka, then `saturation_T_pure_1D_P`. Callers: T phase determination, which first **constructs a whole new backend** (`HelmholtzEOSMixtureBackend.cpp:2447-2449`), and DHSU_T two-phase (`FlashRoutines.cpp:3827-3829`). |
| `saturation_T_pure_Akasaka` | `VLERoutines.cpp:778-923` | 2×2 Newton in (δ',δ''). Akasaka, JTST 3(3) 2008 (cited at `:781-789`). |
| `saturation_T_pure_Maxwell` | `VLERoutines.cpp:933-1127` | Quadratic step in volume from the Helmholtz equal-area condition. **No citation**: the comment is truncated at `:935-939`. QT_flash calls it **directly, without the fallback chain** (`FlashRoutines.cpp:943-951`). |
| `saturation_PHSU_pure` | `VLERoutines.cpp:166-602` | 3×3 Newton in (τ,δ',δ'') plus one specification row (pL, pV, hL, hV, sL or sV). Callers: `FlashRoutines.cpp:631` (HQ), `:664`/`:672` (QS), `:1246` (PQ), `:2284` (DP two-phase). |
| `saturation_D_pure` | `VLERoutines.cpp:603-755` | ρ' or ρ'' imposed. Callers: `FlashRoutines.cpp:2234,2239`. Documented as "numerically unstable near ρ_c" (`FlashRoutines.cpp:2118-2121`, `Tests/CoolProp-Tests-AirCritical.cpp:1-10`). |
| `saturation_T_pure_1D_P`, `saturation_P_pure_1D_T` | `VLERoutines.cpp:80-123`, `:125-164` | Secant or Brent on g'−g''. The second is called from PQ_flash `:1259`. |
| `saturation_critical` | `VLERoutines.cpp:15-78` | **Dead code**: it has no callers, prints to stdout (`:27,:52`) and returns p instead of a residual (`:70`). |
| Superancillary QT/PQ | `FlashRoutines.cpp:893-913`, `:1160-1181`; duplicated in `HelmholtzEOSMixtureBackend::update_QT_pure_superanc` `:1327-1357`; phase determination `:2231-2238` | Evaluates ρ'(T), ρ''(T) and p(T) and calls `update_TDmolarP_unchecked`. `Tcrit_num` bounds the input. |
| Critical-region splines, the at-Tc branch, pseudo-pure | `FlashRoutines.cpp:927-942`, `:952-975` | The at-Tc and spline branches report p = ½(p'+p'') (`:932,:941`). |

### 2.2 Mixture model

| Symbol | Location | Notes |
|---|---|---|
| `ReducingFunction` (abstract base) | `ReducingFunctions.h:32-137`, defaults `.cpp:5-142` | About 40 virtuals: T_r and ρ_r with 1st–3rd x-derivatives, β/γ derivatives, n∂/∂nᵢ forms and the Ψ functions. `factory` is declared at `:48` but **never defined**. |
| `GERG2008ReducingFunction` | `ReducingFunctions.h:144-538`, `.cpp:144-611` | `Yr` `.cpp:244-261`, `c_Y_ij` `:561-564`, `f_Y_ij` `:565-568`. It **stores N full `CoolPropFluid` copies** (`.h:157`) that are read only to compute 2N numbers in the constructor and in `copy()` (`.h:160-181`). |
| `ConstantReducingFunction` | `ReducingFunctions.h:545-663` | Used by the cubic backends. |
| `LemmonAirHFCReducingFunction::convert_to_GERG` | `ReducingFunctions.h:688-707` | Converts ξ/ζ to β=1 and γ_T, γ_v (Lemmon 2000/2004). |
| `DepartureFunction` and subclasses | `ExcessHEFunction.h:20-198` | GERG-2008 (`:104`), Gaussian+Exponential (`:143`), Exponential (`:182`). Each object carries a mutable `derivs` cache. `copy_ptr()` slices the object to the base class (harmless: all state lives in `phi`). |
| `ExcessTerm` | `ExcessHEFunction.h:200-739` | An N×N matrix of `shared_ptr<DepartureFunction>` plus F. `update()` evaluates **both triangles** (`:239-248`). The cached αʳ sums use only i<j (`:302-313`), but the composition derivatives read `[i][k]` for every k≠i, so they also read the lower triangle (`:369-371`). Both orientations hold the same function (`HelmholtzEOSMixtureBackend.cpp:472-473` sets `[i][j]` and `[j][i]` together), so the second evaluation is redundant work rather than dead work. |
| `CorrespondingStatesTerm`, `ResidualHelmholtz` | `HelmholtzEOSMixtureBackend.h:693-847`, `:849-947` | Σ xᵢ αʳ₀ᵢ. Composition derivatives call `components[i].EOS().baser(τ,δ)` and rely on an unkeyed cache (`:707-716`). |
| Mixture α⁰ | `HelmholtzEOSMixtureBackend.cpp:3629-3665` (one derivative), `:3685-3727` (all) | Implemented up to 2nd order only; `:3661` is a bare `throw ValueError()` (fixed upstream in 1fca2b92). It **uses critical, not reducing, constants** (`:3637-3638`, `:3695-3696`); see §3.2 and rot #4. It mutates component EOS objects via `alpha0.set_Tred(Tr)` (`:3645,:3703`). |
| Gas constant | `HelmholtzEOSMixtureBackend.cpp:599-614` | When `NORMALIZE_GAS_CONSTANTS` is set (the default), R is CODATA. Otherwise it is the mole-fraction average, although the comment says "mass fraction". |
| Parameter assembly | `MixtureParameters.cpp:556-644` | Loops over all i≠j: sorted-CAS lookup, β inversion on swap, one departure object `new`-ed per ordered pair, and a dummy zero function when F = 0 (`:629-633`). |
| Global libraries | `MixtureParameters.cpp:16-51` (predefined), `:81-283` (BIP), `:403-511` (departure) | See §5. |

### 2.3 Mixture derivative machinery (`MixtureDerivatives`, a friend class)

| Group | Location | Notes |
|---|---|---|
| Size | `MixtureDerivatives.h` (1012 lines) | 181 static members. 95 are defined out of line in `MixtureDerivatives.cpp:6-1100`; the rest are inline (L*/M* builders, the ψ family and test-signature shims). |
| Fugacity primitives | `MixtureDerivatives.cpp:6-53` | `fugacity_i` `:12-14`; `ln_fugacity_coefficient` `:15-17` (= αʳ + n∂αʳ/∂nᵢ − ln(1+δαʳ_δ)); T/p/ρ derivatives follow GERG eqs. 7.29–7.32. |
| x-derivatives of ln f | `:54-97` | `dln_fugacity_dxj__constT_p_xi` adds the ideal term **in XN_DEPENDENT form regardless of the flag** (`:58-64`). The `_constT_rho_` variant throws for XN_INDEPENDENT (`:69-71`). |
| n∂αʳ/∂nᵢ family | `:209-1100` | Each scalar loops over k and calls `residual_helmholtz->dalphar_dxi(k)` plus reducing derivatives that loop again. ln φᵢ costs O(N²); a full Hessian costs O(N⁴) *(inference)*. |
| Dual-convention gaps | `ExcessHEFunction.h:453-466`, `HelmholtzEOSMixtureBackend.h:789-822` | Several 3rd/4th-order XN_DEPENDENT branches throw `"xN_flag is invalid"`, so the two conventions are not interchangeable. |
| L*, M* and their τ/δ derivatives | `MixtureDerivatives.h:165-318` | Built from `adjugate` and Jacobi's formula. M* needs, for each i, an N×N block of `n2Aijk` (O(N³) entries, each O(N²)). Measured scaling is roughly N^5 [oracle, §5]. |
| FD test fixture | `MixtureDerivatives.cpp:1105-1685` | Fixed `xN = XN_INDEPENDENT` (`:1157`) on a 4-component alkane gas at 300 mol/m³. Tolerances are relaxed to 1e-4 (HEOS) and 1e-3 (PR/SRK) (`:1666-1676`). The mole-number perturbation states are built incorrectly and never used (`:1261-1284`). |

### 2.4 Mixture VLE, stability, flash

| Symbol | Location | Notes |
|---|---|---|
| Wilson K, `WilsonK_resid`, `saturation_preconditioner`, `saturation_Wilson` | `VLERoutines.h:59-73,197-336` | Wilson K plus an ln p vs T interpolation between mole-fraction-averaged triple and critical points. |
| `successive_substitution` (bubble/dew in T or p) | `VLERoutines.cpp:1138-1252` | Seeds liquid density from SRK plus a Peneloux shift (Horstmann 2005, `:1163-1176`) with hard-coded R. Exits silently at `Nstep_max` (`:1241`) and writes through references into `SatL`/`SatV` (`:1147`). |
| `newton_raphson_saturation` | `VLERoutines.h:527-582`, `.cpp:1253-1578` | Bubble/dew Newton (Gernert FPE 2014). Unknowns are the N−1 incipient x plus T or p, or (T, ρ') with ρ'' imposed for the envelope. Uses XN_DEPENDENT (`:1488`). |
| `newton_raphson_twophase` | `VLERoutines.h:403-465`, `.cpp:1580-1791` | Interior Q in (x, y, T or p) with a β-ratio closure. Carries the GH #3192 hardening (`:1614-1701`). Reached **only** through the envelope path (`FlashRoutines.cpp:1593`). |
| Rachford–Rice | `VLERoutines.h:215-227` (`WilsonK_resid`), `VLERoutines.cpp:1204` (in `successive_substitution`), `VLERoutines.cpp:1793-1806` (`RachfordRiceResidual`, which wraps `FlashRoutines.h:33-50`), `VLERoutines.cpp:1815-1828` (`rachford_rice_beta_bisect`), `VLERoutines.cpp:2712-2756` (`solve_rachford_rice` lambda) | The RR sum is coded **five** times. Two of them solve for T or p at fixed β; the other three solve for β. None supports negative flash. |
| `StabilityEvaluationClass` | `VLERoutines.h:666-778`, `.cpp:1944-2659` | Michelsen 1982a (default) or legacy Gernert, chosen by `MIXTURE_STABILITY_ALGORITHM`, which is read in the constructor (`.h:697`). |
| `minimize_tpd` | `VLERoutines.cpp:2288-2476` | α = 2√Y variables with a heuristic diagonal-shift "Hebden" trust region. |
| `PTflash_twophase::solve_michelsen` / `solve_legacy` | `VLERoutines.cpp:2677-3079` / `:3081-3186` | Phase 1 is SS plus GDEM. Phase 2 is a second-order Gibbs step with an eigenvalue PD shift and a trust region. A convergence gate follows (`:3058-3078`). |
| `PT_flash_mixtures` | `FlashRoutines.cpp:24-297` | Order of attempts: envelope-guided path (`:25-95`), blind stability test (`:169-170`), Wilson cross-check `guess_split_from_wilson` (`:185-199`), split, single-phase root selection (`:104-166`). An imposed phase skips all of this (`:288-296`) **unless an envelope was built**. |
| QT/PQ mixture | `FlashRoutines.cpp:978-1050`, `:1268-1366` | With an envelope, `PT_Q_flash_mixtures` (`:1454-1670`). Otherwise Wilson, then SS, then `newton_raphson_saturation` with `bubble_point = (Q<0.5)` (`:1022`, `:1342`). |
| Mixture DT/HT/ST/UT | `FlashRoutines.cpp:3930-3952`, `:4149` | Explicit (T,ρ) inputs run a full stability test and fall back to a P-sweep of PT flashes. |

### 2.5 Critical points, spinodal, TPD, envelope

| Symbol | Location |
|---|---|
| `calc_critical_point`: 2-D Newton on (det L*, det M*), analytic Jacobian via adjugates, stability check of each point | `HelmholtzEOSMixtureBackend.cpp:4113-4196` |
| `OneDimObjective` (Halley on L1*(τ) at δ₀) and `L0CurveTracer` (300 circle-steps, stops at p > 500 MPa or δ>5 or τ>5) | `:4201-4229`, `:4233-4383`; stop rule `HelmholtzEOSMixtureBackend.h:284-286` |
| `_calc_all_critical_points`, `calc_build_spinodal`, `calc_criticality_contour_values` | `HelmholtzEOSMixtureBackend.cpp:4396-4434`, `:4457-4461`, `:4385-4390` |
| `calc_tangent_plane_distance` (on the `TPD_state` child) | `:4436-4455` |
| Mixture `calc_T/p/rhomolar_critical`, which **reruns the full search on every call** | `:1197-1253` |
| `PhaseEnvelopeRoutines::build/refine/evaluate/finalize/find_intersections/is_inside` | `PhaseEnvelopeRoutines.cpp:14-357/359-439/440-513/514-655/657-693/694-826` |
| `PhaseEnvelopeData` (X-macro structure of arrays, component-major K/lnK/x/y) | `include/CoolProp/fluids/PhaseEnvelope.h:7-138` |

### 2.6 How pure fluids are routed through the mixture backend, and what it costs

- **Same object graph.** A pure state owns a `components` vector, a GERG reducing function with 1×1 all-ones matrices (`HelmholtzEOSMixtureBackend.cpp:118-127`), an empty `ExcessTerm` and two child backends `SatL`/`SatV` (`:141-150`).
- **Six live deep copies per pure state.** These are the `components` vector and `Reducing.pFluids`, each in the parent, `SatL` and `SatV`. Nine copies are made during construction, because the children rebuild their reducing function and `sync_linked_states` then replaces it with another copy (`:114,:121,:142-148,:161-170`; `ReducingFunctions.h:179-181`).
- **Construction cost.** [oracle] About 50 µs to construct a pure `AbstractState`, against 1.7 µs for a DT update. RSS is about 10–30 KiB per instance, a noisy allocator measurement.
- **Branching.** There are about 100 runtime pure-vs-mixture branch sites. `is_pure_or_pseudopure`, `is_pure()` and `size()==1` occur 96 times: 46 in `HelmholtzEOSMixtureBackend.cpp`, 32 in FlashRoutines, 15 in TransportRoutines and 3 in the envelope code. Counting `size()!=1` as well gives 106 (54 in the backend, 34 in FlashRoutines).
- **Type confusion.** Pure-only solvers silently use `components[0]` (`VLERoutines.cpp:195,313`). The code itself flags this: "TODO: is it a bug that this branch can be accessed for mixtures?" (`FlashRoutines.cpp:3821`).
- **Evaluation overhead is small but not zero.** Each αʳ update runs `CS.all` over one component, adds `HelmholtzDerivatives` temporaries and evaluates **all 15 derivatives up to 4th order** even when only p is needed (`HelmholtzEOSMixtureBackend.cpp:3552-3572`, `HelmholtzEOSMixtureBackend.h:866-877`). Upstream c6d414e4 added a δ-only path for density solves and reports a 283 → 90 ms improvement on a natural-gas PT flash.
- **Hidden reconstruction.** Phase determination builds a fresh backend whenever ancillaries are inconclusive (`:2447`).

---

## 3. Algorithms and formulas (cite the papers CoolProp cites)

### 3.1 Pure-fluid saturation
- **Akasaka** (JTST 3(3), 2008). Residuals: J'−J''=0 and K'−K''=0, with J=δ(1+δαʳ_δ) and K=δαʳ_δ+αʳ+lnδ. The Newton step uses Δ = dJ''dK' − dJ'dK'' (`VLERoutines.cpp:880-891`) and geometric damping until δ'>1>δ''>0 (`:897-904`).
  - The loop exit `|stepL| > 10ε·|stepL|` (`:912`) is a **tautology**: it is true for every finite non-zero step and false for NaN. So only `error > 1e-10` controls termination, and a NaN state exits the loop.
  - The post-check accepts |p'−p''|/p' ≤ **1e-3** (`:914-922`). It is NaN-transparent.
- **Maxwell** (uncited). Mean pressure p_M = (a'−a'')/(v''−v'), then a quadratic in Δv' (`:1064-1083`).
  - It exits on error ≤ 1e-10 **or** after 4 small steps **or** after 6 error-increasing steps (`:1123`), with no residual check afterwards.
  - The "sqrt scaling" at `:1076-1082` is a mathematical no-op: √(a·k)/√k = √a. It yields NaN when the argument is exactly 0.
- **PHSU / D**: Akasaka-type Newton extended with a specification row and optional ln δ variables.
  - With log-δ, the Jacobian entry `J[1][2]` has an extra `+1` (`:418`; compare the correct `:685`). Every caller disables log-δ (`FlashRoutines.cpp:588,629,663,671,1234,2281`) although the default is true (`VLERoutines.h:100`).
  - The HV/SV seeds read `HEOS.hmolar()`/`smolar()` instead of the `specified_value` argument (`VLERoutines.cpp:219,282`), which is temporal coupling.
  - A stalled Newton (step < 1e-10 with a large residual) is accepted silently (`:542-544` together with `:599-601`).
  - The residual norm mixes dimensionless rows with J/mol rows (`:536`).
- **Superancillary.** Chebyshev evaluation (Bell et al.; the evaluator is in another area). QT reports the superancillary p directly. `T_critical()` and `p_critical()` return the **numerical** critical point `Tcrit_num` (`HelmholtzEOSMixtureBackend.cpp:1207-1212`).
- **Reported pressure.** On the at-Tc and spline paths, saturation p = ½(p'+p'') (`FlashRoutines.cpp:932,941`). On the Maxwell path it is also the mean (`:950`). PQ reports Q·p''+(1−Q)·p' (`:1264`).

### 3.2 Mixture model (Kunz et al., GERG-2004 monograph 2007; Kunz & Wagner, JCED 2012; Lemmon, JPCRD 2000/2004)
- **Reducing function.** Y_r = Σ xᵢ²Y_c,i + Σᵢ<ⱼ 2β_Y,ijγ_Y,ij Y_c,ij · xᵢxⱼ(xᵢ+xⱼ)/(β²_Y,ij xᵢ+xⱼ).
  - For Y=T, T_c,ij = √(T_c,iT_c,j); for Y=v, v_c,ij = ⅛(v_c,i^⅓+v_c,j^⅓)³ (`ReducingFunctions.h:169-176`).
  - Y_c uses `EOS().reduce`, not `crit`. The header comment at `:381` misprints xᵢY²_c,i.
  - β_ij = 1/β_ji when the CAS order is swapped (`MixtureParameters.cpp:134,159-166,600-606`); γ is symmetric.
- **Departure functions** (`ExcessHEFunction.h`):
  - GERG-2008: Σ n δ^d τ^t + Σ n δ^d τ^t exp[−η(δ−ε)²−β(δ−γ)].
  - "Gaussian+Exponential": power-exponential terms plus a standard Gaussian. Its docstring describes a single combined exponent that the code does not implement (`:135-141` vs `:146-170`).
  - Exponential: Σ n δ^d τ^t exp(−δ^l).
- **Ideal part, Kunz & Wagner Table B5.** α⁰ = Σ xᵢ(Rᵢ/R)[α⁰₀ᵢ(τᵢ,δᵢ)+ln xᵢ], with τᵢ = T_c,i·τ/T_r and δᵢ = δ·ρ_r/ρ_c,i.
  - CoolProp takes T_c,i and ρ_c,i from `crit` (`HelmholtzEOSMixtureBackend.cpp:3637-3638`). Each `α⁰₀ᵢ` is defined in its fluid's **reducing** variables, and the pure branch correctly uses `iT_reducing` (`:3592`). In GERG-2008 the critical and reducing parameters coincide; in CoolProp they differ for 15 fluids (§6 #4).
  - `calc_chemical_potential` repeats the critical-constant choice and also drops Rᵢ/R (`:3516-3520`).
- **Estimation for missing pairs: none automatic.** A missing CAS pair throws (`MixtureParameters.cpp:588-591`). Callers can opt in to `"linear"` or `"Lorentz-Berthelot"` (all ones), but there are **two inconsistent implementations**:
  - The per-instance `apply_simple_mixing_rule` uses file `crit` values (`HelmholtzEOSMixtureBackend.cpp:394-402`).
  - The global `add_simple_mixing_rule` uses `T_critical()`/`rhomolar_critical()`, which are superancillary-dependent (`MixtureParameters.cpp:240-246`).
  - Neither is exactly linear in T_r unless the critical and reducing values coincide *(inference)*.

### 3.3 Derivative machinery (GERG-2004 monograph ch. 7, eqs. 7.29–7.64; Gernert thesis 2013, eqs. 3.115–3.134, cited in the code)
- **Residual chemical potential, all xᵢ independent:** n(∂αʳ/∂nᵢ) = δαʳ_δ(1−ρ_r⁻¹ n∂ρ_r/∂nᵢ) + ταʳ_τ T_r⁻¹ n∂T_r/∂nᵢ + αʳ_xᵢ − Σ_k x_k αʳ_x_k. In XN_DEPENDENT form the sum stops at N−1 (`MixtureDerivatives.cpp:230-245`).
- **Projection identity used throughout:** n∂F/∂nⱼ = ∂F/∂xⱼ − Σ_k x_k∂F/∂x_k (`ReducingFunctions.cpp:71-111`). Solvers re-derive it ad hoc (`VLERoutines.cpp:2891-2899`).
- **Critical conditions** (Heidemann & Khalil, AIChE J. 1980, cited only in a test comment, `Tests/CoolProp-Tests-Michelsen.cpp:259`; formulated as in the GERG monograph):
  - L*ᵢⱼ = n∂lnfᵢ/∂nⱼ at constant T,V (`nAij`).
  - M* is L* with its last row replaced by tr(adj(L*)·n∂L*/∂nᵢ).
  - The Jacobian uses tr(adj(A)·∂A/∂X) (`HelmholtzEOSMixtureBackend.cpp:4135-4143`).
  - The contour-tracing method matches Bell & Jäger, FPE 433 (2017), which the code does not cite *(inference)*.

### 3.4 Bubble/dew Newton (Gernert, Jäger, Span, FPE 2014; Gernert thesis)
- **Residuals:** ln fᵢ(T,p,x) − ln fᵢ(T,p,y). The unknowns are the N−1 incipient mole fractions plus T or p. When ρ'' is imposed (envelope), the unknowns are (x, T, ρ') with an extra p'−p'' row (`VLERoutines.cpp:1490-1518`).
- **Termination** uses `minCoeff` of the relative changes (`:1425,:1431`). That stops as soon as *any* variable stalls; the twin solver notes that `maxCoeff` is correct (`:1685-1687`).
- **No post-loop residual check.** The returned h and s come from the pre-step `SatL`/`SatV` while T and x are post-step (`:1433-1447`).
- **dT/dp along saturation (Gernert 3.96–3.97).** `build_arrays` weights by x (`:1567-1577`). The public `calc_first_saturation_deriv` weights by the incipient phase (`HelmholtzEOSMixtureBackend.cpp:3839-3867`). They agree only for dew-type calls, which is the only place the first one is consumed *(inference)*.

### 3.5 Stability and PT phase split (Michelsen, FPE 9, 1982a,b; Michelsen & Mollerup 2007 ch. 12; Wilson K; Rachford & Rice 1952)
- **Stability, tangent-plane distance (TPD):**
  - Trial phases z·K and z/K from Wilson (`VLERoutines.cpp:2124-2131`). Feed fugacities dᵢ = ln zᵢ + ln φᵢ(z).
  - If the global feed-density solve fails, the code falls back to a **gas-imposed** root (`:2104-2113`).
  - Up to 4 × (2 SS + 1 GDEM) loops on ln Yᵢ = dᵢ − ln φᵢ(y), with the modified TPD tm = 1+ΣYᵢ(ln Yᵢ+ln φᵢ−dᵢ−1) (`:2141-2244`).
  - `minimize_tpd` then runs **unconditionally**, even when SS has already decided (`:2250`; skipped upstream in d6e52450).
- **Split:**
  - Rachford–Rice in ln K: Newton plus bisection, with ln K clamped at 350 (`:2712-2756`).
  - SS plus GDEM. The GDEM ratio is clamped to 0.95, which caps the factor at 19 (`:2811-2818`).
  - Second order: reduced gradient gᵢ = β(1−β)(ln fᵢ''−ln fᵢ'), Hessian per Michelsen 1982b App. B, scaling √(zᵢ/(xᵢyᵢ)), min-eigenvalue PD shift, Gibbs-decrease acceptance (`:2828-3036`).
  - Final gate: max|Δln f| ≤ 1e-5, spread ≥ 1e-4 and β interior (`:3058-3078`).
- **Hessian bugs.** In both second-order Hessians, `dln_fugacity_dxj__constT_p_xi` (ln f, not ln φ) is fed into formulas that add the ideal term themselves. In `minimize_tpd` it is also left unprojected (`:2359`, `:2887-2902`). The documentation gives the correct form (`Web/fluid_properties/Mixtures.rst:333-337`).
- **Legacy algorithm** (Gernert 2014; the sole option up to v7.x per `Mixtures.rst:349-351`). SS stability with an L1 "diffbulk" heuristic (`:2478-2602`) and a Newton on iso-fugacity plus mass-balance ratios (`:3081-3186`).
- **Density seeds** use SRK plus Peneloux (Horstmann, FPE 2005). `solve_trial_rho_warm` warm-starts and falls back to `solver_rho_Tp_global`, the lowest-Gibbs root (`:2063-2092`). That global solver **assumes at most two stationary points** per isotherm (`HelmholtzEOSMixtureBackend.cpp:2835-2904`). Multi-fluid isotherms can have more; upstream b17661f6 documents a mechanically stable spurious loop root.

### 3.6 PQ/QT routing for mixtures
- **With an envelope built:** `PT_Q_flash_mixtures` runs first (`FlashRoutines.cpp:984-999`). Upstream b63ea865 measured it **silently falling back on 42% of interior-Q states**, seeded with negative densities from `CubicInterp`.
- **Blind path:** Wilson, then `successive_substitution` with β=Q, then the bubble/dew solver with `bubble_point=(Q<0.5)` (`:1022`, `:1342`). That solver holds the "bulk" phase at the SS composition and never imposes z=(1−Q)x+Qy, so it is exact only at Q=0 and Q=1.
  - [oracle] max|z−(1−Q)x−Qy| reaches 2.9e-6 for N2/C1/C2/nC4/nC5 at 0.3 MPa and 7e-8 for C1/nC4 at 2 MPa.
  - Upstream reported 1.2e-4 and fixed it in b63ea865 by reformulating in ln K.
- **Water dew points** can use a Henry's-law seed (Fernández-Prini 2003, `FlashRoutines.cpp:1053-1156`). It is gated by `HENRYS_LAW_TO_GENERATE_VLE_GUESSES`, which defaults to false. The seed:
  - calls high-level `PropsSI` from inside the flash (`:1304`);
  - sets the water liquid fraction to Σx_gas instead of 1−Σ (`:1328`);
  - defaults `iWater=0` when no water is present (`:1303`);
  - throws for any unlisted gas (`:1154`).

### 3.7 Critical points and spinodal
- **Search:**
  - Start at δ₀ = `SPINODAL_MINIMUM_DELTA` (0.5), τ₀ = 0.66, bumped by ×1.1 up to 3 times.
  - Halley on det L*(τ) (`HelmholtzEOSMixtureBackend.cpp:4404-4418`).
  - Trace L1*=0 with circle steps of radius (R_τ, R_δ) = (0.1, 0.025) (`:4392-4395`). The first step finds the angle by Brent; later steps use Newton (`:4305-4343`).
  - A sign change of M1* triggers a 2-D Newton at the midpoint (`:4362-4370`).
  - M1* is computed on every step, even for spinodal-only traces (`:4346`).
- **Stability of found points.** Points with p<0 are marked unstable but still returned. Otherwise a full Michelsen stability test runs unless `ASSUME_CRITICAL_POINT_STABLE` is set (`:4184-4194`).
- **"Stop" that does not stop.** Twice the tracer comments "Stopping the search" and then executes `continue` without updating τ, δ or θ (`:4319-4327`, `:4332-4339`). The loop therefore retries the same failing step until it reaches 300 iterations; the bug is still present on master.

### 3.8 Phase envelope (GERG-2004 monograph; Venkatarathnam, I&ECR 2014, both discussed in `Web/mixtures/phase_envelope.ipynb`)
- **The notebook and the code disagree.** The notebook derives the GERG ln K / ln T / ln p formulation with a specified variable. The C++ instead does ρ''-marching with the dew-type `newton_raphson_saturation`.
- **Start:** a dew point at `PHASE_ENVELOPE_STARTING_PRESSURE_PA` = 100 Pa (`PhaseEnvelopeRoutines.cpp:116-156`).
- **Continuation:**
  - ρ'' is multiplied by a factor that starts at 1.05 and adapts to the Newton step count (÷10, ÷3 or ×2 on the excess over 1). The factor has a floor of 1.01. It is capped at 1.1 only near the critical point, when |ρ'/ρ''−1| < 4 (`:318-334`). Elsewhere it can grow without bound.
  - Extrapolation is linear, then quadratic, then cubic splines **rebuilt from all previous points on every step** (`:179-229`).
  - Rejection rules: the trivial solution |ρ'−ρ''|<1e-3, p<0, or ΔT>100 K (`:243-254`).
- **Stop:** p below the start pressure, or a near-pure incipient phase (`:338-351`). There is **no pressure or temperature cap**.
- **Refinement and maxima:** `refine` inserts points (`:359-439`). Maxima come from spline extrema plus a Brent solve on dT/dp=0, with Type-I detection by end pressure (`:514-655`).
- **`is_inside`** assumes exactly 2 crossings of the primary variable (`:719-727`).

### 3.9 Heuristics that are not paper methods (do not port as-is)
- SRK covolume and Peneloux density seeds with R = 8.3144598 (`VLERoutines.cpp:1165-1176,2649-2657`; `HelmholtzEOSMixtureBackend.cpp:2825-2834`).
- `saturation_preconditioner`: ln p linear in T between averaged triple and critical points (`VLERoutines.h:229-255`).
- Mole-fraction-averaged mixture limits T_max, T_min, p_max (`HelmholtzEOSMixtureBackend.cpp:1305-1325`).
- Gas/liquid labelling of a mixture by ρ vs ρ_r (`:207-219`; `FlashRoutines.cpp:56,165`).

---

## 4. Data and configuration inputs

| Input | Source | Facts (verified) |
|---|---|---|
| Binary pairs | `dev/mixtures/mixture_binary_pairs.json` (207 KB), embedded as a string by `dev/generate_headers.py:61-63` | **888 pairs over 116 CAS numbers; no duplicates.** By source: 582 Bell-JCED-2016 (automatically fitted, F=0), 194 Kunz-JCED-2012 (49 of them all-ones by design), 74 Bell-JCED-2025, 15 Gernert-Thesis-2013, 6 Lemmon-JPCRD-2004 (ξ/ζ), 8 Bell-JPCRD-2022/2023, 3 NIST IR 8570, 3 Tkaczuk-JPCRD-2020, 1 REFPROP 9.1, 1 Akasaka-Purdue-2014, 1 Bell-IJT-2020. **40 pairs have F≠0.** |
| Superseded BIPs | `dev/mixtures/old_BIP.json` (18 records) via `remove_dupes.py` | The **air/water/CO₂ pairs use Gernert/EOS-CG values instead of GERG-2008**. All 15 pairs in force are Gernert-Thesis-2013: H₂O–{N₂, O₂, Ar, CO, CO₂}, CO₂–{N₂, Ar, CO, O₂}, N₂–{O₂, Ar, CO}, Ar–O₂, Ar–CO and O₂–CO. They replace 15 Kunz-JCED-2012 records and 3 Lemmon-JPCRD-2000 air records (N₂–O₂, N₂–Ar, Ar–O₂). HEOS results for dry air, humid air, CCS and similar mixtures therefore are **not** GERG-2008 (and not Lemmon 2000). Strict GERG backends exist only upstream (c7d6a1aa). |
| Departure functions | `mixture_departure_functions.json` plus schema | **28 functions**: 16 Exponential, 9 GERG-2008, 3 Gaussian+Exponential. The aliases KW0–KW8, KWR, KWS, KWT and LJ6 are REFPROP model codes. Generalized functions (GeneralizedAlkane, GeneralizedHFC, GeneralizedAirWater) serve several pairs with different F. `GeneralizedAirComponents` is used by no pair. |
| Predefined mixtures | `predefined_mixtures.json` | **154 mixtures** with 2–10 components; all fractions sum to 1. The ASHRAE entries come from `inject_ASHRAE_2026.py`, which converts the TSV **mass** fractions to mole fractions using CoolProp's own molar masses, with overrides for R143b, RE170, R1132a and R1132(E). The stored mole fractions are therefore derived data. |
| Raw paper tables | `KunzWagner2012_TableA6/A7/A8.txt`, `Bell2016Coefficients.txt`, `Bell2016fluids.txt`, `Table*_to_JSON.py`, `inject_*.py` | Traceable sources for regenerating the JSON. `JSON_to_C++.py` is **stale**: it reads `mixture_excess_term.json` and `mixture_reducing_parameters.json`, which no longer exist. |
| REFPROP HMX.BNC | `parse_HMX_BNC` `MixtureParameters.cpp:646-790`, `set_departure_functions` `:792-855` | Optional import of REFPROP-format files. |
| Runtime mutators (process-global) | `set_interaction_parameters` (`:396-399`), `set_departure_functions` (`:792`), `set_predefined_mixtures` (`:72-75`), `set_mixture_binary_pair_data` (`:353-378`), global `apply_simple_mixing_rule` (`:286-289`) | They mutate process-wide maps without locks (§5). Per-instance overrides go through `set_binary_interaction_double/string` and propagate to `linked_states` (`HelmholtzEOSMixtureBackend.cpp:413-481`). |
| Cubic replacement of a component | `calc_change_EOS` (`HelmholtzEOSMixtureBackend.cpp:483-532`) | The `SRK`, `Peng-Robinson` and `XiangDeiters` substitutions use a hard-coded R (`:496,:517`). |

Configuration keys that change VLE or mixture behaviour or results (`include/CoolProp/detail/configuration_keys.h`):

| Key | Default | Effect |
|---|---|---|
| `NORMALIZE_GAS_CONSTANTS` | true | Mixture R becomes CODATA 8.31446261815324. Paper check values need `false` (Tkaczuk; `Tests/CoolProp-Tests.cpp:2784-2799`). [oracle] For methane the pure EOS R is 8.31451, a 5.7e-6 ratio. |
| `ENABLE_SUPERANCILLARIES` | true | Selects the superancillary QT/PQ path **and changes `T_critical()`/`p_critical()`** for pure fluids. [oracle] R134a gives T_critical() = 374.211967 K vs 374.21 K (file). |
| `MIXTURE_STABILITY_ALGORITHM` | 1 | Selects Michelsen or legacy, for both the stability test and the split (`VLERoutines.h:697`, `.cpp:2671`). |
| `PHASE_ENVELOPE_STARTING_PRESSURE_PA` | 100 | Envelope start pressure. |
| `SPINODAL_MINIMUM_DELTA` | 0.5 | Start of the L1* search. |
| `ASSUME_CRITICAL_POINT_STABLE` | false | Skips the TPD check of critical points. |
| `OVERWRITE_BINARY_INTERACTION` / `OVERWRITE_DEPARTURE_FUNCTION` | false | Duplicate-key policy of the global libraries. |
| `HENRYS_LAW_TO_GENERATE_VLE_GUESSES` | false | Water dew-point seed (§3.6). |
| `CRITICAL_WITHIN_1UK`, `CRITICAL_SPLINES_ENABLED`, `DONT_CHECK_PROPERTY_LIMITS` | true, true, false | Near-critical handling and limit checks for pure saturation. |

---

## 5. State, caching, globals, thread-safety, memory

**Per-instance object graph.** Each `AbstractState("HEOS", …)` owns the following:
- **`components`**: a full `CoolPropFluid` copy per component, holding EOS terms, ancillaries and transport data. The superancillary itself is `shared_ptr`-shared (`include/CoolProp/CoolPropFluid.h:430-438`).
- **`Reducing`**, which holds **another set of copies** (`ReducingFunctions.h:157`).
- **`residual_helmholtz`**, with N(N−1) heap departure objects.
- **Child backends `SatL` and `SatV`**, full `get_copy`s created at construction (`HelmholtzEOSMixtureBackend.cpp:141-150`).
- **Lazily created children:** `TPD_state`, `critical_state` (with its own SatL/SatV) and `transient_pure_state` (`HelmholtzEOSMixtureBackend.h:81-102`). All live in `linked_states`, so BIP edits propagate. `sync_linked_states` deep-copies the reducing function and the residual again (`.cpp:161-170`); `set_mixture_parameters` copies the components once more (`MixtureParameters.cpp:558`).

**Hidden mutable caches and evaluation-time mutation:**
- **Unkeyed component cache.** `BaseHelmholtzContainer::base(τ,δ)` returns the cached value whenever `is_cached` is set, whatever (τ,δ) is passed (`include/CoolProp/fluids/Helmholtz.h:768-773`). Correctness depends on `clear()` running on every component at every update (`HelmholtzEOSMixtureBackend.h:154-162`). The corresponding-states composition derivatives call `EOS().baser(τ,δ)` and rely on that cache (`:707-716`).
- **Departure caches.** `DepartureFunction::derivs` is a per-object cache filled by `ExcessTerm::update`.
- **Model data written during evaluation.** `alpha0.set_Tred(Tr)` writes into component EOS objects while α⁰ is evaluated (`HelmholtzEOSMixtureBackend.cpp:3595,3645,3682,3703`).
- **Solvers communicate by mutating `HEOS.SatL`/`SatV`.** `VLERoutines.cpp` contains about 400 occurrences of `SatL`/`SatV` and 39 calls to `set_mole_fractions`.
  - `successive_substitution` writes through `get_mole_fractions_ref()` (`:1147`).
  - `check_stability_legacy` updates the **caller's own** state (`:2523-2524`).
  - Akasaka and Maxwell clear the **caller's** imposed phase (`:825-827`, `:1025-1027`); in Maxwell's case the phase was set on the wrong object.
  - `successive_substitution_guessrho` reads T and p from `HEOS.T()`/`HEOS.p()`, so callers have to poke `HEOS._T`/`_p` first (`FlashRoutines.cpp:1664-1666`).
- **The phase envelope is cached in the state and changes later algorithms.** PT/PQ/QT check `PhaseEnvelope.built` (`FlashRoutines.cpp:25,985,1273`).

**Globals:**
- **BIP and departure libraries** are file-scope `static` objects whose maps are filled lazily under `std::call_once` (`MixtureParameters.cpp:89,103,283,409,503,511`).
- **`PredefinedMixturesLibrary`** is parsed **eagerly at static-initialisation time** (`:20-22,51`).
- **Unlocked mutation.** The mutators take no lock, and `binary_pair_map()` hands out a mutable reference (`:92-95`).
- **A lookup that writes.** `get_departure_function` uses `map[Name]`, which **inserts an empty entry on a miss** (`:515`). Concurrently that is a data race. [oracle] Sequentially, the insert permanently blocks later registration of that name ("already loaded").
- **Global `Configuration`** is a lazily created `unique_ptr` with no synchronisation (`src/Configuration.cpp:126-160`). The dedicated files read it 8 times. The whole of `HelmholtzEOSMixtureBackend.cpp` reads it 16 times and `FlashRoutines.cpp` another 16, some of them on every call (for example `ENABLE_SUPERANCILLARIES` in QT/PQ, `FlashRoutines.cpp:893,1160`).
- **Tests** flip global config without isolation, or with RAII guards (`Tests/CoolProp-Tests-Michelsen.cpp:406-418`; `VLERoutines.cpp:3237-3249`).
- `static std::atomic<int> deriv_counter` is benign (`HelmholtzEOSMixtureBackend.cpp:47`).
- **Net effect:** an `AbstractState` must never be shared between threads. Parallel use needs one full object graph per thread, and the global mutators are unsafe at any time.

**Measured costs [oracle]** (medians or best of 5; Python call overhead about 0.3 µs):

| Operation | Pure (methane) | C1/C2 (0.9/0.1) | Larger mixtures |
|---|---|---|---|
| PT, phase not imposed | 8.8 µs | **13.9 ms** (5 MPa, 250 K) | 6-component natural gas: **38–168 ms** |
| PT, phase imposed | — | 28 µs | — |
| DT (explicit input) | 1.7 µs | **8.6 ms** blind vs 4.5 µs imposed | — |
| QT | 0.5 µs superancillary; 20.6 µs Maxwell | 0.65 ms at Q=0; 1.03 ms at Q=0.5 | — |
| `T_critical()` | data lookup | **0.1–0.25 s, then throws.** At 0.9/0.1 the message is "found 8 critical points"; equimolar C1/C2 gives "found 3" | — |
| `all_critical_points` / `build_spinodal` | — | 75–111 ms / about 50 ms (binaries) | **1.86 s / 1.47 s** (N2/C1/C2/C3), roughly N^5 scaling |
| `build_phase_envelope` | — | 8–13 ms | 126–190 ms (4–6 components) |
| Construction / RSS per instance | about 50 µs / 10–30 KiB | about 170 µs / about 120 KiB | 6 components: about 900 µs / about 630 KiB |

**Lazy loading.** The binary-pair and departure JSON are parsed in full on the first mixture use, once per process. Predefined mixtures are parsed at library load. Each new mixture `AbstractState` re-creates its departure objects from dictionaries and re-copies every component. There is no per-pair laziness and no sharing of immutable model data between instances.

---

## 6. Rot and bugs

Ordered roughly by severity. "Upstream" gives the fix commit where one exists; otherwise the item is still present on master or was not checked.

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| 1 | **Mixture PT flash returns a spurious middle-branch root** (classified as gas, h ≈ −666 kJ/mol) | [oracle] C1/C2/C3 at z=(0.05,0.90,0.05), 2 MPa, 223.15 K gives ρ=6750.9; at 222.5 and 225 K the result is liquid with ρ≈16300. Cause: `FlashRoutines.cpp:115-143` treats a single SRK root returned twice as two roots. Upstream b17661f6. | A silent wrong answer from the oracle | Enumerate all EOS roots on the isotherm. Select by mechanical **and** material stability (TPD), not by assuming two stationary points (`HelmholtzEOSMixtureBackend.cpp:2835-2904`). |
| 2 | **Interior-Q PQ/QT solved by a bubble/dew solver** | `FlashRoutines.cpp:1022,1342`; the residual has no mass balance (`VLERoutines.cpp:1490-1561`). [oracle] mass-balance error up to 2.9e-6; upstream measured 1.2e-4 and fixed it in b63ea865. | Wrong T or p and wrong phase compositions at 0<Q<1 | Newton in ln K with x=z/(1+β(K−1)) and y=Kx: mass balance holds by construction, with a Rachford–Rice closure. |
| 3 | **Ideal term double-counted (and, in the TPD case, unprojected) in both second-order Hessians, plus the XN_INDEPENDENT ideal term wrong in its last row** | `VLERoutines.cpp:2359`, `:2887-2902`, `MixtureDerivatives.cpp:58-64`. Upstream 014c8186 fixed all three. Its comment reads "off-diagonals become '… − 2' instead of '… − 1'", and it reports that the unprojected TPD Hessian flipped a methanol/benzene verdict on MSVC once the ideal-term fix landed. | Linear convergence and stalls, which push results onto fallbacks; wrong stability verdicts are possible | Hessians built from one AD-derived n∂lnφ/∂n. Test symmetry plus FD agreement. |
| 4 | **Mixture α⁰ evaluates components at critical, not reducing, constants** (also in `calc_chemical_potential`) | `HelmholtzEOSMixtureBackend.cpp:3637-3638,3695-3696,3516-3520` vs the pure branch `:3592`. Affects the 15 fluids whose crit≠reduce, including R134a, Methanol, MM, MDM, R1234ze(Z), SO₂, Helium and HCl. [oracle] At 450 K and 5 mol/m³, methanol in a (1,0) binary vs pure methanol: cp⁰ +1.1e-3, h −29.5 J/mol, s −0.17 J/(mol·K). R134a: cp⁰ −5.1e-5, h +0.69 J/mol. The mixture α⁰ equals the pure α⁰ evaluated at (T_c/T, ρ/ρ_c). **Not fixed on master.** | Wrong ideal-gas h, s, cp, μ for mixtures containing these fluids; the invariant "binary at z=(1,0) equals pure" is broken | Evaluate each α⁰₀ᵢ in its own reducing variables. Add a property test "trace mixture equals pure, up to the R policy". Record a known-deviation fixture for oracle comparisons. |
| 5 | **BIP library load-order poisoning** | `set_interaction_parameters` bypasses `load_defaults_if_needed` (`MixtureParameters.cpp:396-399`). The default load then throws inside `call_once` (`:102-110,184-189`), so every retry fails. [oracle] After one call with a default pair, **every** mixture, including N2/Ar, fails with "CAS pair(354-33-6,75-10-5) already in binary interaction map". Upstream 3c15af82. | Mixtures disabled process-wide | Immutable compiled-in data. User overrides go through a builder that produces a new model value. |
| 6 | **A departure-function lookup inserts an empty entry** | `MixtureParameters.cpp:515` (`map[Name]`). [oracle] After a failed `set_binary_interaction_string(…,"function","MyDep")`, registering `MyDep` throws "already loaded". | A data race under threads, and permanent name poisoning | Lookups return `Option`; no global map. |
| 7 | **0/0 when two mole fractions are zero** | `f_Y_ij` `ReducingFunctions.cpp:565-568`. [oracle] C1/C2/C3 at z=(1,0,0) fails with "p is not a valid number"; blind DT reports a misleading "P-sweep" error. Upstream c7d6a1aa guards the value and the first x-derivatives, so `fugacity()` matches the trimmed mixture on master. Its message shows that second and higher x-derivatives of f_Y,ij have **no limit** at the both-zero corner, because they depend on the direction of approach. Envelopes therefore still fail there. | NaN for natural-gas-style component lists | Drop zero components when the model instance is built. A limit-based guard cannot work beyond first order. Property test: zero components equal the trimmed mixture. |
| 8 | **No composition validation** | `set_mole_fractions` checks only the size (`HelmholtzEOSMixtureBackend.cpp:152-160`). [oracle] z=(0.3,0.3) is accepted with T_r = 90 K; (−0.1,1.1) is accepted; NaN surfaces as an unrelated P-sweep error. | Silent garbage | A validated `Composition` newtype that is finite, non-negative, normalized, with an explicit zero policy. |
| 9 | **Maxwell exits unconverged without error, and QT reports p = ½(p'+p'')** | `VLERoutines.cpp:1118-1123`, `FlashRoutines.cpp:948-950`. [oracle] With superancillaries off, R134a at T_c−1 K: p'=3.977160 vs p''=3.983020 MPa, (g'−g'')/RT=−5e-4, reported p 0.07% high. In a 162-point sweep, 3 states were silently unconverged. At the MD4M triple point: p'=7.8e-7 vs p''=6.0e-7 Pa. | Silent wrong pure saturation on the non-default path; the mean pressure is meaningless at low reduced pressure | One Newton solver with a residual-checked exit. Take p_sat from the vapour side or the superancillary. |
| 10 | **Akasaka termination is a tautology and its NaN exit passes; PHSU accepts a stalled Newton** | `VLERoutines.cpp:912` (`abs(s) > 10*eps*abs(s)`), `:916` (NaN passes `>`); `:542-544` with `:599-601` | Non-converged or NaN saturation states can be returned *(inference for NaN)* | Typed convergence criteria; `Result` with finite checks; mandatory final residual. |
| 11 | **Bubble/dew Newton terminates on `minCoeff` with no post-loop residual check; outputs come from mixed iterates** | `VLERoutines.cpp:1425-1447`; the twin solver states that `maxCoeff` is correct (`:1685-1687`). The legacy PT solver has the same flaw (`:3109`). | Premature, silently "converged" bubble/dew points and envelope points | Norm-based criteria plus a mandatory final verification. |
| 12 | **GDEM clamp (ratio→0.95 gives factor 19) collapses near-dew splits onto the wrong Rachford–Rice branch** | `VLERoutines.cpp:2236,2814`. Upstream 014c8186 (#3342) reports HSU_P temperatures wrong by 0.11–0.67 K. It names the clamp as one of **several** causes. The others are the second-order stage's β(1−β)-scaled reduced gradient, which vanishes as β→1 so steps die at the dew boundary, and the Hessian bug #3. | Silent wrong two-phase states | Try-then-revert GDEM: keep the extrapolation only if the SS error decreases. For the second-order stage, use minority-phase variables with an unscaled Gibbs gradient. |
| 13 | **Legacy PT Newton Jacobian wrong for N≥3; identical if/else branches** | `VLERoutines.cpp:3164-3170` (identical branches) and `:3174-3184` (wrong numerator, no δᵢⱼ, missing x_N chain rule). Legacy tests are binary only. β was never set before CoolProp-1tbe.1 (`:3117-3125`). | The legacy algorithm is broken for ternaries | **Drop** the legacy path. |
| 14 | **Mixture critical properties are recomputed on every call, throw for ordinary binaries and report garbage points as stable; the tracer's "stop" is a `continue`** | `HelmholtzEOSMixtureBackend.cpp:1197-1253`, `:4319-4339`. [oracle] C1/C2 `T_critical()` takes 0.1–0.25 s and throws: 8 points at z=(0.9,0.1), 3 equimolar, 5 at (0.1,0.9). For equimolar C1/C2, `all_critical_points` returns points at −20.2 and −4.7 MPa, which are flagged unstable. Equimolar C1/C3 returns a "stable" critical point at 70 K, 162 MPa. | An unusable API, latency, and misleading data | Compute once and cache in an immutable result. Classify points as physical, stable or spinodal-only. Use explicit continuation with termination reasons. |
| 15 | **Envelope tracer fails silently or runs away** | Silent `return` after 5 failures without `built` (`PhaseEnvelopeRoutines.cpp:173-177`). Fixed 100 Pa start (`:117`). No pressure or temperature cap (`:339`). NaN slips through `x<0‖x>1` (`:221`; fixed upstream in 15ede127). `goto` at `:171,:226`. Size_t underflow at `iTmax−1` (`:557-561,613-617`). [oracle] CO₂/water (0.9/0.1) fails at the start (T=214 K, p=100 Pa). C1/nC10 traces to 6837 MPa, H₂/C1 to 21227 MPa, and N2/C1/C2/C3 to 23136 MPa and 7135 K, all without an error. | Missing or garbage envelopes; the envelope-guided flash silently degrades | Re-implement the continuation with validity bounds, a robust start (critical point or Wilson), typed failure reasons and incremental extrapolation. |
| 16 | **Algorithm choice depends on whether `build_phase_envelope()` was called earlier on the same object** | `FlashRoutines.cpp:25,985,1273`. Upstream aeea05e4: the imposed phase was ignored, giving a metastable liquid root of about 14000 mol/m³ instead of about 20. Upstream b63ea865: 42% silent fallbacks with negative seed densities. | Non-reproducible results that depend on the object's history | Envelopes are separate immutable values. Flash APIs take an explicit, optional `&Envelope` hint. |
| 17 | **Explicit (T, ρ, z) inputs run a full stability analysis plus a P-sweep of PT flashes** | `FlashRoutines.cpp:3930-3952,4149`. [oracle] 8.6 ms vs 4.5 µs with the phase imposed. | About 1900× latency on an explicit Helmholtz input | (T, ρ, n) evaluation is primitive and never implies phase analysis; stability is opt-in. |
| 18 | **Model data copied about 6× per instance; the reducing function holds `CoolPropFluid` copies only to compute 2N numbers** | `HelmholtzEOSMixtureBackend.cpp:79,114,121,141-150,161-170`; `ReducingFunctions.h:157-181`; `MixtureParameters.cpp:558`. [oracle] costs in §5. | Memory, construction time and cache misses | `Arc<PureModel>` per component; packed reducing constants only. |
| 19 | **Evaluation mutates model data; component caches are unkeyed** | `HelmholtzEOSMixtureBackend.cpp:3645,3703` (`set_Tred`); `Helmholtz.h:768-773`; `HelmholtzEOSMixtureBackend.h:154-162` | Temporal coupling, which forces per-instance copies and blocks sharing | Pure functions of (T, ρ, n); results returned in stack `Derivs` structs. |
| 20 | **Solvers communicate through mutable child backends and even the caller's state** | About 400 `SatL`/`SatV` uses in `VLERoutines.cpp`; `:1147`, `:2523-2524`, `:825-827`, `:1025-1027`; `HelmholtzEOSMixtureBackend.cpp:2447` (a new backend per phase determination) | Re-entrancy and aliasing bugs, allocation in hot paths | Solvers take `&Model` plus a workspace and return values with diagnostics. |
| 21 | **Departure functions duplicated per ordered pair and evaluated twice; F=0 dummies are evaluated** | `MixtureParameters.cpp:567-640` (dummy at `:629-633`); `ExcessHEFunction.h:239-248`. *Verifier note:* the lower-triangle values are read by the composition derivatives (`ExcessHEFunction.h:369-371`), so they are not dead. They are redundant, though: α_ij = α_ji, and both entries are set to the same function (`HelmholtzEOSMixtureBackend.cpp:472-473`). | Up to about 2× departure work plus N(N−1) heap objects per backend | A sparse upper-triangular list of (i, j, F, `&'static` term table) for F≠0 only, indexed symmetrically. |
| 22 | **Global configuration changes algorithms and results** | `VLERoutines.h:697`, `.cpp:2671`; `HelmholtzEOSMixtureBackend.cpp:599-614,1207-1212` | Irreproducible results; tests have to mutate globals | Explicit, typed per-call or per-model options. |
| 23 | **DRY violations**: the Rachford–Rice sum coded five times (three β solvers), two superancillary QT paths, two dT/dp_sat formulas, two inconsistent "linear" mixing rules | §2.4; `FlashRoutines.cpp:893-913` vs `HelmholtzEOSMixtureBackend.cpp:1327-1357`; `VLERoutines.cpp:1567-1577` vs `HelmholtzEOSMixtureBackend.cpp:3839-3867`; `HelmholtzEOSMixtureBackend.cpp:394-402` vs `MixtureParameters.cpp:240-246` | Divergent fixes and inconsistent results | One module per concept. |
| 24 | **Hard-coded R = 8.3144598** (CODATA 2014) in 5 places, next to configurable CODATA 2018 and per-fluid R values | `VLERoutines.cpp:1172,2654`; `HelmholtzEOSMixtureBackend.cpp:496,517,2830` | Inconsistent constants: guess quality for the seeds, but a **model value** in `change_EOS` | One constants module plus an explicit `RPolicy`. |
| 25 | **Stability test feed-density fallback forces the gas branch** | `VLERoutines.cpp:2104-2113` | For a liquid feed where the global solver fails, d_i is evaluated on the wrong root and the stability verdict is invalid *(inference)* | Root selection with explicit failure; never a phase-imposed guess. |
| 26 | **XN_DEPENDENT reducing derivative is suspected to miss a factor 2; `dYr_dbeta` aborts on the first both-zero pair** | `ReducingFunctions.cpp:49` (−1×) vs `:37,:69` (−2×; derivation in §3.3 gives −2 in both conventions); `:288-290` (`return 0` instead of `continue`). The FD fixture tests XN_INDEPENDENT only. | Latent: these are not on production paths *(inference)* | Mooted by mole-number AD. |
| 27 | **PHSU log-δ Jacobian has an extra +1** behind a default-true option that every caller turns off | `VLERoutines.cpp:418` vs `:685`; `VLERoutines.h:100`; callers in §3.1 | A latent bug and dead weight | Drop the option; derive the Jacobian by AD. |
| 28 | **Dead and debug code** | `saturation_critical` (`VLERoutines.cpp:15-78`); `check_Jacobian` (`:1270-1349`); `catch(NotImplementedError&){ throw; // ??? }` (`:336-338,645-647`); empty `try{}catch` (`FlashRoutines.cpp:969-974`); the dead QT store (`:1042-1043`, overwritten at `:1047-1048`); the commented-out critical-point call and "critical point jump" blocks (`PhaseEnvelopeRoutines.cpp:100-108,280-315`); the undefined `ReducingFunction::factory` (`ReducingFunctions.h:48`); the no-op sqrt scaling (`VLERoutines.cpp:1076-1082`); 33 `std::cout` lines in `VLERoutines.cpp`; an include guard in a `.cpp` (`PhaseEnvelopeRoutines.cpp:1-2`); `throw ValueError(format(""))` (`:62`) | Noise and misleading APIs | Do not port. |
| 29 | **Parsing and error-path bugs** | `find('/') > 0` is true even when `'/'` is absent (`MixtureParameters.cpp:689,699`). The error path reads `"Name1"` but the dictionary key is `"name1"` (`:613` vs `:141`), so building the error throws a different error. Malformed BIP records print to stdout and are skipped (`:168-172`). | HMX.BNC lines are mis-parsed; errors are misleading | serde-typed records; tested error paths. |
| 30 | **Interface holes in the oracle** | Mixture α⁰ 3rd derivatives throw a bare `ValueError()` (`HelmholtzEOSMixtureBackend.cpp:3661`; fixed in 1fca2b92). `fugacity_coefficient` throws at Q=0 and Q=1 (`:3470-3482`; fixed in 552b4711). Departure length checks were missing (fixed in ae54172f). The Henry seed has bugs (§3.6). | Feature gaps in the oracle | Complete derivative orders by construction (AD); no high-level calls from the kernel. |
| 31 | **Stale docs and weak tests** | `Mixtures.rst:17-21` lists only PQ/TQ/TP. The line-search and 30-iteration text at `:340-342` disagrees with the code. The Michelsen "paper benchmark" tests use paper compositions but assert only the phase, 0<Q<1 and the fugacity residual, never the paper's numbers (`Tests/CoolProp-Tests-Michelsen.cpp:107-147`). *(added by verifier)* The benchmark scenarios at `:107-341` are also mostly cubic: 24 sections use SRK or PR and only 2 use HEOS (`:166`, `:203`), so they barely exercise the multi-fluid model. The Bell-JPCRD-2022 Table XI R1234yf check values were recomputed with CoolProp itself (`Tests/CoolProp-Tests.cpp:4524-4527`). `SatTFixture` checks only that nothing throws (`:2195-2236`). | False confidence | Tests assert values with provenance (paper or independent code), not self-consistency alone. |

---

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

| Computation | Parallel axis | SIMD fit | Notes for side-by-side implementations |
|---|---|---|---|
| Superancillary Chebyshev (ρ'(T), ρ''(T), p(T); p→T inversion) | Many T per call. Within one call, the 3–5 series share the same x. | **Excellent**: fixed-length Clenshaw recurrences, with a branch only for the interval lookup | First SIMD experiment. Lanes are either states, or the three series of one state. The scalar version stays the reference. |
| Pure EOS VLE Newton (Akasaka/PHSU) | The liquid and vapour evaluations within one iteration (same T, different δ); many T in a batch | **2-lane** within a solve; lock-step across T is feasible away from T_c (uniform iteration counts) | The kernel evaluates two (τ,δ) points per call; control flow stays scalar. |
| Mixture αʳ at one (T, ρ, x) | All component and departure terms share (τ,δ): flatten them into one structure-of-arrays (SoA) term table with per-term weights (xᵢ, or xᵢxⱼFᵢⱼ) | **Good even for small N**: one fused reduction per state. A segmented reduction yields ∂αʳ/∂xᵢ (per-component partial sums) for free. | This replaces CoolProp's repeated per-component `baser()` calls. Two-phase solvers evaluate liquid and vapour as 2 lanes. |
| Reducing function and its x-derivatives | Pairs i<j | Fair: O(N²) tiny loops | Precompute packed c_Y,ij and β² per orientation. Scalar or AD. |
| Composition derivatives (ln φ, n∂lnφ/∂n, L*, M*) | Derivative directions (multi-dual lanes = N+2); states | Good via AD: dual-number arithmetic is short-vector arithmetic | Replaces about 180 hand-coded functions. The same generic code runs scalar or SIMD. M* needs 3rd-order AD (nested or Taylor). |
| Rachford–Rice | Independent flashes | The inner Σ over N is SIMD-able but tiny | The safeguarded Newton is branchy per state. Batch across states with masks only if profiling demands it. |
| Successive substitution and GDEM | Per state, sequential | Elementwise over N; tiny | Parallelise across states only. |
| Density root solves (T,p→ρ) | Per state | **Poor**: bracketing plus Newton/Halley, root enumeration | Keep scalar; batch with threads. |
| Stability test | Two independent trial phases | 2 tasks or 2 lanes | Not worth threads for one state; 2-lane kernel evaluation is cheap. |
| Second-order split (N×N eigen/LDLT) | Per state | Small dense linear algebra | Fixed-size matrices for small N; batch across states. |
| Critical points and spinodal tracing | **Sequential continuation**; parallel across compositions (critical loci) | None within one trace | The L*/M* assembly is O(N^5) scalar in CoolProp; AD shrinks it. Threads pay off only across compositions. |
| Phase envelope | **Sequential continuation**; parallel across compositions and across refinement segments | None within one trace | Keep the predictor and corrector scalar; extrapolate incrementally (no spline rebuild). |
| Batch PT/PQ/QT flashes (process simulation, tables) | **Embarrassingly parallel** across states and fluids | — | Needs an immutable `Send + Sync` model and per-thread workspaces. CoolProp needs a full object graph per thread (§5). |

**Side-by-side plan.** Separate four layers:
- **K: kernels.** αʳ/α⁰ and their derivatives, implemented as (a) a scalar reference generic over `S: Scalar`, which also makes it AD-capable, (b) a batch-over-states SIMD variant, and (c) a flattened-terms or 2-lane variant.
- **P: primitives.** ln φ, n∂lnφ/∂n and ∂p/∂ρ, all built by AD on K(a).
- **A: algorithms.** Scalar, generic over the model trait, deterministic, and returning diagnostics.
- **D: drivers.** Single, batch (an optional rayon feature), and later accelerators for K only.

Every K variant is property-tested against K(a) to within a few ulp. Do **not** vectorise the control flow of Michelsen, trust-region, continuation or root enumeration; those algorithms do not suit SIMD and stay scalar per state.

---

## 8. Verification assets

**In-repo tests at v8.0.0:**
- `src/Tests/CoolProp-Tests-Michelsen.cpp` (2087 lines):
  - Scenarios use paper compositions: the 7-component natural gas of Michelsen 1982a,b (`:107`), CH₄/CO₂ and CH₄/C₂H₆/CO₂ from M&M 2007 (`:147,:185`), CH₄/nC₁₀ (`:222`), CH₄/H₂S (`:255`), near-critical CH₄/C₂H₆ (`:277`) and N₂/CH₄ (`:321`). Assertions are qualitative: phase, 0<Q<1, and the equal-fugacity residual from `equilibrium_residual` < 1e-6 (`:28-45`). Most sections run on the SRK/PR backends, not HEOS (24 against 2).
  - The same file covers blind-flash sweeps (`:421-722`), envelope-guided PT/PQ, the zgpy sweep and HSU_P/DHSU_T/HSU_D round trips.
- **Mixture αʳ check values at 1e-10:**
  - Bell JPCRD 2022 Table XI (`Tests/CoolProp-Tests.cpp:4523-4587`). Its R1234yf pairs use **CoolProp-recomputed** values; only R134a+R1234zeE is a true paper value.
  - Bell JPCRD 2023 Table XIII (`:4588-4656`). All five pairs are paper values, agreeing to about 1e-10 (`:4505-4521`).
  - NIST IR 8570 (`:4657-4718`).
  - Tkaczuk et al. JPCRD 2020 Table 8, which needs `NORMALIZE_GAS_CONSTANTS=false` (`:2777`).
  - Inverted-order sections exercise the β inversion.
- **Other in-repo checks:**
  - Composition-derivative FD fixture for HEOS/PR/SRK (`MixtureDerivatives.cpp:1105-1685`; XN_INDEPENDENT only, loose tolerances).
  - Saturation-derivative FD (`Tests/CoolProp-Tests.cpp:2441,2482,5363`); reducing-parameter sensitivity of critical points (`:2732`); predefined mixtures (`:2254`).
  - Superancillary "matches extended-precision check points for all fluids" (`:3834`).
  - The `PTflash_twophase` residual test (`VLERoutines.cpp:3192-3225`).

**Upstream assets after v8.0.0** (they target post-8.0.0 behaviour; usable with care):
- Strict GERG-2004/2008 backends with `dev/gerg/` generators and about 10k lines of reference values **cross-checked against teqp** (c7d6a1aa, `src/Tests/CoolProp-Tests-GERG.cpp`).
- Beckmüller 2021 H₂ binaries with Table S6 checks at 2e-5 (`CoolProp-Tests-H2Mixtures.cpp`, 3c15af82). In v8.0.0 the H₂ pairs are still Kunz–Wagner.
- `CoolProp-Tests-WideBoilingSplit.cpp` (3f16d5fb), `CoolProp-Tests-PTFlashMiddleRoot.cpp` (b17661f6), the interior-Q gates (b63ea865) and the near-dew HSU_P band (014c8186).

**Literature (the arbiter):**
- Kunz & Wagner JCED 2012 and the GERG monograph 2007. Tables A6–A8 are in `dev/mixtures/`; locate the computer-implementation check tables.
- Akasaka JTST 2008.
- Michelsen FPE 1982a,b and M&M 2007, which have worked flash and stability examples.
- Gernert, Jäger and Span FPE 2014, and the Gernert thesis.
- Heidemann & Khalil 1980 and Bell & Jäger FPE 2017 (critical loci, e.g. C1/C2).
- Venkatarathnam I&ECR 2014.
- Lemmon JPCRD 2000/2004 (air and HFC check values).

**Oracle hooks** [oracle]:
- Python `AbstractState` exposes `update`, `alphar`, `alpha0` and the d…alpha derivatives, `fugacity(i)`, `fugacity_coefficient(i)`, `chemical_potential(i)`, `mole_fractions_liquid/vapor`, `saturated_liquid/vapor_keyed_output`, `build_phase_envelope`/`get_phase_envelope_data` (`built` and `closed` are not exposed), `build_spinodal`/`get_spinodal_data`, `all_critical_points`, `criticality_contour_values`, `tangent_plane_distance`, `set/get_binary_interaction_double`, `set_binary_interaction_string`, `apply_simple_mixing_rule`, `first_saturation_deriv`, `T_reducing` and `rhomolar_reducing`.
- `MixtureDerivatives` internals are not exposed, so Rust derivative tests check FD consistency against oracle ln φ rather than oracle derivatives.
- teqp (Bell; MIT licence, autodiff multi-fluid) is a plausible second oracle; upstream already uses it (c7d6a1aa).

**Invariants that do not depend on the oracle:**
- Σᵢ xᵢ n∂lnφᵢ/∂nⱼ = 0 (Gibbs–Duhem), and symmetry of n∂lnφᵢ/∂nⱼ.
- Euler homogeneity of nα.
- Invariance under β swaps; zero components equal the trimmed mixture.
- **z=(1,0) equals the pure fluid up to the R policy.** CoolProp 8.0.0 violates this for α⁰ (#4).
- Equal fugacities plus mass balance at every published two-phase state, and a Gibbs decrease relative to the feed.
- Pure saturation satisfies p'=p'' and g'=g'', with p_sat checked on the vapour side at low p_r.

**Known-bad oracle states**, to exclude or flag in a fixture file:
- C1/C2/C3 at (0.05, 0.90, 0.05), 2 MPa, 223.15 K (#1).
- Any composition with two zeros (#7).
- Interior-Q states of wide-boiling mixtures (#2).
- Near-dew states of the #3342 mixture (#12).
- CO₂/water near-pure condensation in PT (3f16d5fb).
- Envelopes for CO₂/water, C1/nC10, H₂/C1 and N2/C1/C2/C3 (#15).
- Mixture `T_critical()` (#14).
- Mixture α⁰-derived properties (h, s, cp, μ) for the 15 crit≠reduce fluids (#4).
- Pure saturation with superancillaries disabled (#9).
- MD4M near the triple point (§10 Q4).

---

## 9. Port recommendation (units, priority, order, redesign)

| Unit | CoolProp paths | Priority | Rationale / redesign |
|---|---|---|---|
| U1 Composition and state types (design-only) | `HelmholtzEOSMixtureBackend.h:23-60`, `.cpp:152-197` | **P0-core** | A validated `Composition` newtype and the kernel taking mole numbers. `Pure` and `Mixture` are distinct models behind one trait. This fixes every trait signature now (#8). |
| U2 Pure saturation API, superancillary first | `FlashRoutines.cpp:888-913,1157-1181`; `HelmholtzEOSMixtureBackend.cpp:1197-1253` (pure branch), `:1327-1357` | **P0-core** | About 0.5 µs per QT and SIMD-friendly. Report `Tc_eos` (numerical) and `Tc_published` separately. Optional EOS polish step. |
| U3 One robust EOS pure-VLE Newton for T, p, h, s, u or ρ specified | `VLERoutines.cpp:80-1127` | **P0-core** | Needed for fluids without a superancillary, user-modified EOS and superancillary validation. Newton in (τ, ln δ', ln δ'') with an AD Jacobian, superancillary or ancillary seeds, a bracketed 1-D fallback, a mandatory residual check and `Result`. **It replaces 6 solvers.** |
| U4 Pseudo-pure (ancillary-defined) saturation | `FlashRoutines.cpp:952-975,1183-1199` | **P1-early** | Six common fluids (Air, R410A, …). Make the "not EOS-consistent" semantics explicit in the type. |
| U5 Multi-fluid model evaluation: reducing, departures, CS, ideal mixing, R policy | `ReducingFunctions.{h,cpp}`, `ExcessHEFunction.h`, `HelmholtzEOSMixtureBackend.h:693-947`, `.cpp:599-614,3534-3728` | **P1-early** | Immutable `Arc<MultiFluid>`, sparse pairs, zero components trimmed at build time (#7), α⁰ in reducing variables (#4), no copies of pure EOSs. Gives predefined blends single-phase properties early. |
| U6 Mixture data and lookup | `MixtureParameters.{h,cpp}`, `dev/mixtures/*.json` | **P1-early** | build.rs or codegen into static tables keyed by sorted CAS. β pre-oriented; provenance and model family per record; parity set "CoolProp-8.0.0"; opt-in estimators. Overrides through a builder, no globals. ASHRAE **mass** fractions are the source of truth. |
| U7 Composition derivatives (ln φ, n∂lnφ/∂n, ∂/∂T, ∂/∂p, partial molar volume, L*, M*) | `MixtureDerivatives.{h,cpp}` (about 2.7k lines) | **P1-early** | Generic AD (dual and hyper-dual) over (T, ρ, n), roughly 300–500 Rust lines in place of 181 functions and the XN flag. FD and invariant tests. |
| U8 Rachford–Rice (one module, negative-flash capable) | `VLERoutines.h:197-228`, `.cpp:1793-1828,2712-2756`, `FlashRoutines.h:33-50` | **P2-later** (first VLE unit) | Small, shared by every VLE algorithm. Bracketed Newton in β with ln K inputs. |
| U9 Bubble/dew points (Wilson init, SS, Newton) and interior Q | `VLERoutines.h:59-336`, `.cpp:1129-1791`, `FlashRoutines.cpp:978-1050,1268-1366` | **P2-later** | Newton in ln K with T or p imposed; mass balance by construction; AD Jacobian; final residual gate. |
| U10 Stability (Michelsen TPD) | `VLERoutines.cpp:1944-2476` | **P2-later** | Port the *method*, not the v8.0.0 code: a Hessian from n∂lnφ/∂n (#3), try-then-revert GDEM (#12), skip the minimiser when SS decides (d6e52450), near-pure trial seeds (3f16d5fb). |
| U11 PT phase split and single-phase root selection | `VLERoutines.cpp:2661-3079`, `FlashRoutines.cpp:24-297` | **P2-later** | Michelsen 1982b second order with minority-phase variables (014c8186 lessons); full root enumeration (#1); an outcome enum `{Single(root), TwoPhase(split), Failed(reason)}`. |
| U12 Generalised Clapeyron dT/dp_sat for mixtures | `HelmholtzEOSMixtureBackend.cpp:3832-3887` | **P2-later** | One implementation weighted by the incipient phase. |
| U13 Critical points and spinodal | `HelmholtzEOSMixtureBackend.cpp:4113-4461`, `MixtureDerivatives.h:165-318` | **P2-later** | L*/M* via AD. Classify and cache points; bound the search; parallelise over compositions. |
| U14 Phase envelope | `PhaseEnvelopeRoutines.{h,cpp}`, `PhaseEnvelope.h` | **P2-later** | A new continuation (GERG/Michelsen ln K–ln T–ln p with a specified variable, as in the notebook) with validity bounds, typed SoA output named incipient/bulk, and n-crossing `is_inside`. |
| U15 Envelope-guided flashes | `FlashRoutines.cpp:25-95,1454-1670` | **defer** | Only as an explicit, optional hint (#16). |
| U16 HMX.BNC import | `MixtureParameters.cpp:646-855` | **defer** | An optional `refprop-import` feature with a real parser. |
| U17 BIP fitting (parameter derivatives `dTr_dbetaT`, …) | `ReducingFunctions.h:53-82`, `.cpp:263-300` | **defer** | Free with AD over parameters, if wanted. |
| U18 Legacy Gernert stability and legacy PT Newton | `VLERoutines.cpp:2478-2602,3081-3186` | **drop** | Broken for N≥3 (#13); superseded. |
| U19 Dead and diagnostic routines (`saturation_critical`, `check_Jacobian`, the 1-D fallbacks as separate APIs, `use_logdelta`) | `VLERoutines.cpp:15-164,1270-1349` | **drop** | The fallback idea survives inside U3. |
| U20 SRK/Peneloux seeding with hard-coded R; Henry's-law seed | `VLERoutines.cpp:1160-1178,2634-2659`, `HelmholtzEOSMixtureBackend.cpp:2825-2834`, `FlashRoutines.cpp:1053-1156,1301-1330` | **drop** | Use the generic root finder from area 03 with documented initialisation. |
| U21 Runtime-mutable global BIP, departure and predefined libraries | `MixtureParameters.cpp:16-555` | **drop (redesign)** | Replaced by U6's immutable data plus a builder. |

**Order:**
1. U1, U2, U3, U4: pure VLE done, with TDD against the superancillary, Akasaka's examples and the oracle.
2. U5, U6, U7 together: single-phase mixtures and predefined blends, with TDD against the αʳ check tables, FD and the invariants.
3. U8, U9, U10, U11, U12: mixture VLE, with TDD against invariants, literature examples and the oracle, excluding known-bad states.
4. U13, U14.

**When to port mixtures.** The **model** (U5–U7) belongs early (P1). It fixes the composition argument of every core trait, unlocks predefined blends, and is mostly correct in v8.0.0, apart from #4 and #7. The **VLE algorithms** (U8–U14) come later (P2). They are the least mature part of v8.0.0: Michelsen became the default only in v8.0, about 10 flash/VLE fixes have followed the tag (§1), and the oracle is wrong in several places. Build them from Michelsen, Gernert and GERG rather than transliterating the C++.

**What the trait design must anticipate now:**
- **Kernel signature.** Evaluate `a_res(T, ρ, n: &[S]) -> S` generic over the scalar, with the reducing function inside so that AD sees the composition. Pure fluids get a specialised implementation with no composition overhead, and a 1-component mixture must agree with it.
- **Composition representation.** Dynamic N with inline small storage (no heap allocation for N ≤ about 8), plus an SoA batch view for many states.
- **Results.** Values, never mutated child states.
- **Options.** Typed per-call or per-model options, never globals.

---

## 10. Open questions

1. **Fix or parity?** For #4 (α⁰ evaluated at critical constants), #1, #2 and #9, implement the literature-correct behaviour and record the oracle deviation in a fixture, or keep an opt-in `compat_8_0_0` flag? The recommendation is to fix and fixture.
2. **R policy for mixtures.** CoolProp's CODATA normalisation (oracle parity), or per-model conventions (GERG 8.314472; paper tables use a weighted R)? The recommendation is an explicit `RPolicy` per model that defaults to parity.
3. **Model family selection.** HEOS uses EOS-CG (Gernert) values for the air/water/CO₂ pairs and GERG elsewhere, and upstream now ships several models per pair. Since 3c15af82 the last record in document order is in force, as the `load_from_JSON` comment in master's `MixtureParameters.cpp` states. What is the selection API, and which default reproduces 8.0.0?
4. **Low-pressure saturation oracle.** [oracle] For MD4M at T_triple, the superancillary gives p = 6.278e-7 Pa, but the EOS vapour-side p at the superancillary ρ'' is 6.048e-7 Pa (3.8% lower), while (g'−g'')/RT ≈ −3e-13 at the superancillary densities. Is the superancillary *pressure* expansion inaccurate at extreme low p_r? This decides which value is the pure-VLE oracle near triple points.
5. **First mixture milestone scope.** Which input pairs (PT, PQ, TQ only? HSU?) and which N (dynamic N with a small inline buffer of 8 or 16)?
6. **AD implementation.** Hand-rolled duals (no dependencies, SIMD-friendly) or a crate? The user prefers minimal dependencies. M* needs 3rd order.
7. **Second oracle.** Adopt teqp (MIT, C++ and Python) as an additional mixture oracle in the test harness?
8. **Envelope and critical scope.** VLE only, or also LLE, VLLE and type III systems (H₂/C1, C1/C3 garbage points)? Which validity bounds (p_max, T range) stop a trace?
9. **Explicit (T,ρ,n) inputs.** Should they ever run phase analysis (CoolProp does, taking 8.6 ms)? Or should two-phase classification be a separate opt-in call?
10. **Pseudo-pure fluids.** Keep ancillary-only saturation semantics, or route the predefined blend (`R410A.mix`) through the true mixture model by default?
11. **The XN_DEPENDENT factor at `ReducingFunctions.cpp:49`.** Moot under mole-number AD. Should it be confirmed numerically (C++ FD) before any XN_DEPENDENT oracle value is compared?

---

## Verification log

**Date:** 2026-10-04. **Verifier:** an adversarial pass against `reference/CoolProp` at v8.0.0 (ae81610e), `origin/master`, and the `CoolProp==8.0.0` wheel.

**Scope.** About 210 claims were checked: about 175 path:line citations, 25 counts and sizes, and 10 oracle re-runs. The re-runs covered #1, #4, #7, #8, #9, #14 and #15, the PQ mass balance, and DT latency. I found no partial edits from an earlier verifier: tables were intact and the doc had no previous log.

**Counts re-run and confirmed:**
- Line counts: 10.8k in the dedicated files; 2087 in the Michelsen tests.
- 151 upstream commits.
- 181 static members in `MixtureDerivatives`, 95 of them defined out of line.
- 888 pairs over 116 CAS numbers, with no duplicates; the per-source counts match; 40 pairs have F≠0; 49 Kunz all-ones pairs.
- 28 departure functions (16/9/3).
- 154 predefined mixtures; 18 superseded BIPs.
- 15 fluids with crit≠reduce.
- 402 `SatL`/`SatV` uses, 33 `std::cout` lines and 39 `set_mole_fractions` calls.
- 5 hard-coded R values; 15 cached derivatives.

**Corrections made:**
1. §1 takeaway 2 and §9: "at least 13 flash/VLE fixes" was not supported. Replaced with the commit list: 8 `fix(...)` commits, plus ae54172f and 15ede127, plus 3 perf commits.
2. §2.2 `ExcessTerm` and rot #21: the lower-triangle departure values are read by the composition derivatives (`ExcessHEFunction.h:369-371`). Rot #21 is downgraded from "evaluated twice, wasted" to "redundant because α_ij = α_ji".
3. §2.4 Rachford–Rice row: repaired a garbled citation that attributed `:1815-1828` to `FlashRoutines.h`. Clarified that two of the five sums solve for T or p, not β. #23 is reworded to match.
4. §2.6 branching counts: "54 + 32" mixed two search patterns. Restated as 96 occurrences (46/32/15/3), or 106 if `size()!=1` is included.
5. §3.8 envelope step factor: it is not bounded to [1.01, 1.1]. The 1.1 cap applies only when |ρ'/ρ''−1| < 4.
6. §4 superseded BIPs: completed the list of the 15 Gernert pairs in force, including N₂–O₂, N₂–Ar, Ar–O₂, N₂–CO and N₂–CO₂. Noted that they also replace Lemmon-2000 air records.
7. §4 `calc_change_EOS`: the EOS names are `SRK`, `Peng-Robinson` and `XiangDeiters`, not "-SRK" and "-PengRobinson".
8. §5: `CoolPropFluid.h` path corrected to `include/CoolProp/CoolPropFluid.h`.
9. §5: the BIP and departure libraries are file-scope static objects, not function-scope statics.
10. §5: "38 config reads" was not reproducible. Replaced with 8 (dedicated files), 16 (backend .cpp) and 16 (FlashRoutines.cpp).
11. §5 cost table and rot #14 [oracle]: C1/C2 at z=(0.9, 0.1) throws "found **8** critical points". "3 points at −20.2 and −4.7 MPa" holds for the **equimolar** mixture. Latency in this run was 0.1–0.25 s, not 82 ms.
12. Rot #7: c7d6a1aa also fixes first-order derivatives and `fugacity()`. Second-order derivatives have no limit at the both-zero corner. The remedy is restricted to trimming zero components, and U5 is updated to match.
13. Rot #12: per the 014c8186 message, the GDEM clamp is one of several causes of the 0.11–0.67 K error. The vanishing β(1−β) gradient and Hessian bug #3 are added.
14. Rot #15: the NaN-composition fix is attributed to 15ede127.
15. Rot #28: the commented-out envelope blocks are described accurately.
16. Rot #31 and §8: the Michelsen paper benchmarks run on SRK/PR in 24 of 26 sections (added by verifier). The CoolProp-recomputed R1234yf values apply only to Bell 2022 Table XI. The 2023 Table XIII values are paper values.
17. §10 Q3: added the evidence for "last record in force" from master's `MixtureParameters.cpp` (3c15af82).

**Rot items tried and kept, with the refutation attempt:**
- #1 [oracle] reproduced exactly; the root cause is confirmed by b17661f6.
- #3: the three Hessian defects are confirmed by 014c8186.
- #4: still present on master (master `HelmholtzEOSMixtureBackend.cpp:3819-3820,3911-3912`); [oracle] reproduced.
- #5: 3c15af82 does fix it on master, because run-time loads now merge into the fully loaded library.
- #8: still present on master.
- #9 [oracle] reproduced.
- #10: the tautology was confirmed by reading `:912`.
- #13: the legacy tests are binary only (`CoolProp-Tests-Michelsen.cpp:406,723`; `VLERoutines.cpp:3227`), and N=2 has no mass-balance rows.
- #14: the "stop" `continue` is still on master (`:4592-4607`).
- #26: the −2 coefficient is re-derived by the verifier and still holds.

**Residual doubts:**
- The oracle timings are machine-dependent; this run measured DT blind at 16.6 ms against the doc's 8.6 ms.
- The C1/nC10 envelope maximum depends on composition (906 MPa at 0.5/0.5 in this run).
- The MD4M open question (§10 Q4) was not re-run.
