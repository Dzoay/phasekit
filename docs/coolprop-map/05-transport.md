# 05 Transport properties, surface tension and the expression DSL - CoolProp v8.0.0 map

> Scope: `src/Backends/Helmholtz/TransportRoutines.{h,cpp}`, transport structs in `include/CoolProp/CoolPropFluid.h:116-383`,
> transport parsing in `src/Backends/Helmholtz/Fluids/FluidLibrary.h:459-1013`, `calc_*` dispatch in
> `src/Backends/Helmholtz/HelmholtzEOSMixtureBackend.cpp:703-1094`, `SurfaceTensionCorrelation` in
> `include/CoolProp/fluids/Ancillaries.h:25-65`, `dev/fluids/*.json` TRANSPORT blocks; ~2,950 lines at v8.0.0.
> The expression DSL (`src/expression/`, `include/CoolProp/expression/`, ~1,240 lines + 1,363 lines of tests), its
> design spec, the REFPROP-10.1 supersession plans and `dev/agent-notes.md` exist **only on origin/master** (DSL landed
> in 9587d5ad, 2026-08-22); they are cited as `master:<path>`. Part of the coolprop-rs port plan; cites the v8.0.0 source.

**Headline facts**
- The v8.0.0 oracle contains **zero** DSL blocks. Every v8.0.0 transport model is a closed set of parametric forms plus
  29 hardcoded per-fluid routines (27 in use, 2 dead). Oracle parity does not need a DSL.
- 136 fluids: 66 have viscosity, 63 conductivity, 108 surface tension; 70 have no transport model (calls throw).
  The wheel's embedded JSON matches the tree (checked via `get_fluid_param_string(f,'JSON')`).
- 19 fluids need another fluid (Propane, R134a or Nitrogen) loaded for transport (ECS). 54 of the 63 conductivity models
  read the same fluid's viscosity model. Both couplings are implicit and not pinned.
- Oracle defects found while mapping (details in §6): two-phase states give meaningless values; ECS `q_D` is never parsed
  (honouring it would shift near-critical λ by roughly −16 to +24 % for 5 fluids); ammonia's conductivity diverges at
  405.4 K at every density (6 W/m/K in single-phase vapour); methane's conductivity depends on a global config flag
  (~1e-7 relative). Added by verifier: R1234yf's `rhosr_critical` is stale against its own v8.0.0 EOS (R4), and the
  ECS conductivity of R124, R22, R245fa and R32 uses Chung-estimated Lennard-Jones parameters (R17).

## 1. Purpose and concepts

- **Viscosity η [Pa·s] and thermal conductivity λ [W/m/K]** for pure and pseudo-pure HEOS fluids, evaluated at an
  already-flashed `(T, ρ)` state. Most correlations use the reference-correlation decomposition
  `X = X0(T) + X1(T)·ρ + ΔX_r(T,ρ) + ΔX_c(T,ρ)`: dilute gas, initial density (viscosity only), residual or "higher
  order", and critical enhancement. CoolProp calls these *stages*.
- **Whole-fluid models** bypass the stages: extended corresponding states (ECS, conformal mapping onto a reference
  fluid), ρ·s_r corresponding states (rhosr-CS, Bell 2016), Chung 1988, and fluid-specific `*_hardcoded` routines.
- **Surface tension σ [N/m]** is a function of T alone along saturation: `σ = Σ a_i (1 − T/Tc)^{n_i}`. It is stored
  with the ancillaries and only accepted on a two-phase state.
- **Mixtures:** a non-predictive mixing rule only (§3.7). No mixture surface tension.
- **The expression DSL (master only)** turns new *parametric* stage forms into data: a formula string, constants,
  arrays and declared state variables, compiled once at fluid load and tree-walk evaluated (§3.9).
- **EOS coupling:** correlations take density in reducing units and also need EOS-derived quantities: α^r derivatives,
  p, ∂p/∂T|ρ, c_p, c_v, c_p⁰, ∂²α⁰/∂τ², residual entropy, and α^r derivatives at a second state (T_ref, ρ). Transport
  depends on the EOS, but the EOS does not depend on transport.

## 2. Structure (key types/functions -> path:line)

| Piece | Location (v8.0.0) | Notes |
|---|---|---|
| `TransportRoutines` (static class, 48 routines) | `src/Backends/Helmholtz/TransportRoutines.h:8-286` | each takes `HelmholtzEOSMixtureBackend&` and digs into `HEOS.components[0].transport` |
| Generic stage forms | `TransportRoutines.cpp:9-180, 358-401, 677-777, 850-863` | dilute, initial-density, higher-order, conductivity stages (incl. `eta0_and_poly` :850-863), Olchowy–Sengers |
| Hardcoded routines | `TransportRoutines.cpp:182-356, 403-629, 779-848, 865-1140` | 29 routines, 27 in use (§3.8) |
| ECS + conformal solver | `TransportRoutines.cpp:1142-1264, 1294-1373` | 2×2 Newton step with step halving, Eigen QR |
| rhosr-CS, Chung | `TransportRoutines.cpp:1266-1292, 630-675` | |
| Data structs (flag soup) | `include/CoolProp/CoolPropFluid.h:116-383` | `TransportPropertyData` has six bool flags and two "hardcoded" enums, and stores **every** variant's struct (not a union) |
| Stage enums | `CoolPropFluid.h:135-143, 163-170, 197-205, 233-244, 265-270, 289-301, 327-347` | `int type` for conductivity, typed enum for viscosity (inconsistent) |
| JSON → structs | `FluidLibrary.h:459-1013` (`parse_transport` :990, `parse_viscosity` :713, `parse_thermal_conductivity` :942, `default_transport` :1005) | string-compare dispatch, `assert` length checks |
| Surface tension | `include/CoolProp/fluids/Ancillaries.h:25-65`, factory `FluidLibraryFactories.h:26-36`, parse `FluidLibrary.h:1177` | |
| Dispatch | `HelmholtzEOSMixtureBackend.cpp:714-750` (dilute η), `:755-808` (background η), `:826-921` (η contributions), `:922-1036` (λ contributions), `:1038-1053` (background λ), `:810-825` & `:1054-1069` (totals + mixture rule), `:703-713` (σ), `:1070-1094` (conformal state API) | |
| Caching wrappers | `src/AbstractState.cpp:793-799, 810-813` | `CachedElement _viscosity/_conductivity/_surface_tension`, cleared on update |
| ECS reference cache | `HelmholtzEOSMixtureBackend.h:78-79`; built lazily at `.cpp:848-852, 944-948`; reset `:107-111` | one mutable reference backend **per AbstractState and per property** |
| DSL (master) | `master:include/CoolProp/expression/{Expression.h(97),ExpressionBlock.h(67),ExpressionCorrelation.h(54),detail/Lexer.h(49)}`, `master:src/expression/{Expression.cpp(872),ExpressionBlock.cpp(63),ExpressionCorrelation.cpp(40)}` | Lexer → Pratt parser → binder (slots) → `Program` (immutable, `shared_ptr<const ProgramData>`) |
| DSL hooks (master) | `master:include/CoolProp/CoolPropFluid.h` adds `*_EXPRESSION` enum values plus `ExpressionData` in 5 stage structs; parse `master:FluidLibrary.h:502-511`; dispatch `master:HelmholtzEOSMixtureBackend.cpp:743-746, 774-782, 819-822, 1057-1060, 1106-1109` | |

## 3. Algorithms and formulas (cite the papers CoolProp cites)

Notation: τ = T_r/T, δ = ρ/ρ_r, T* = T/(ε/k), N_A = 6.02214129e23 (hard-coded CODATA-2010 value).
"#" is the number of v8.0.0 fluids whose **default** model uses the form.

### 3.1 Viscosity stage forms

| Stage / JSON `type` | Formula (as coded) | Code | # | Source CoolProp cites |
|---|---|---|---|---|
| dilute `kinetic_theory` | η0 = 26.692e-9 √(M[kg/kmol]·T)/(σ[nm]² Ω22(T*)), Ω22 = 1.16145T*^−0.14874 + 0.52487e^−0.77320T* + 2.16178e^−2.43787T* | `.cpp:9-26` | 1 (+ECS, rhosr dilute) | Chapman–Enskog; Neufeld et al. JCP 57 (1972) |
| dilute `collision_integral` | η0 = C√(M T)/(σ² S*), S* = exp(Σ a_i (ln T*)^{t_i}) | `:28-55` | 20 | Vesovic 1990 style; per-fluid papers |
| dilute `powers_of_T` / `powers_of_Tr` | Σ a_i T^{t_i} / Σ a_i (T/T_r)^{t_i} | `:57-85` | 7 / 2 | Geller 2000 blends, Tanaka IJT 1996 (R123), Kiselev IECR 2005 (ethanol); f-theory papers |
| dilute `collision_integral_powers_of_Tstar` | C√T / Σ a_i T*^{t_i} | `:87-102` | 1 (H2S) | Quiñones-Cisneros JCED 2012 |
| initial `Rainwater-Friend` | routine returns B_η = N_A σ³ Σ b_i T*^{t_i}; **host** multiplies: η1 = η0·B_η·ρ (`HelmholtzEOSMixtureBackend.cpp:758-762`, "TODO: Check units") | `:140-160` | 19 | Vogel JPCRD 1998 |
| initial `empirical` | Σ n_i δ^{d_i} τ^{t_i} | `:162-180` | 1 (CycloHexane) | Tariq JPCRD 2014 |
| higher `modified_Batschinski_Hildebrand` | Σ a_i δ^{d1}τ^{t1}e^{γ_i δ^{l_i}} + (Σ f_i δ^{d2}τ^{t2})·(1/(δ0−δ) − 1/δ0), δ0 = Σg_iτ^{h_i}/Σp_iτ^{q_i} | `:103-138` | 22 | Vogel 1998 (propane), Lemmon–Jacobsen IJT 2004 |
| higher `friction_theory` | η_f = κ_a p_a + κ_r Δp_r + κ_i p_id + κ_aa p_a² + κ_drdr Δp_r² + κ_rr p_r² + κ_ii p_id² + κ_rrr p_r³ + κ_aaa p_a³; p_r = T∂p/∂T\|ρ, p_a = p − p_r, p_id = ρRT, Δp_r = p_r − p_id (bar); κ_x = (A_x0 + A_x1ψ1 + A_x2ψ2)τ^{N_x}, ψ1 = e^τ − c1, ψ2 = e^{τ²} − c2 | `:358-401` | 4 | Quiñones-Cisneros & Deiters JPCB 2006 |
| critical (viscosity) | always 0 (`HelmholtzEOSMixtureBackend.cpp:916-917`), except inside the hardcoded Water routine | — | — | — |

### 3.2 Whole-fluid viscosity models

- **ECS** (`:1203-1264`), 15 fluids. Huber, Laesecke & Perkins IECR 2003 (11 fluids); Klein et al. IJR 1997 (3);
  `Huber-RP912` (EthylBenzene). Shape factors are exact: the conformal state (T0, ρ0) solves
  α^r_ref(T0,ρ0) = α^r(T,ρ) and Z_ref(T0,ρ0) = Z(T,ρ) (`conformal_state_solver :1142-1201`, analytic Jacobian;
  step halving from 1 down to 1/512, i.e. 10 tries, because the loop stops at `frac > 0.001` (:1174); ≤ 50 iterations;
  absolute residual norm ≤ 1e-9). Then η = η*_kin(T) + Δη_ref,background(T0, ρ0·ψ_η)·F_η, with
  F_η = √f·h^(−2/3)·√(M/M0), f = T/T0, h = ρ0/ρ (molar) and ψ_η = Σ a_i (ρ/ρ_r)^{t_i}. The background is the reference fluid's initial-density
  plus higher-order stages (`calc_viscosity_background`). The target fluid gets no Rainwater–Friend term and no
  large-molecule correction (master `R1132a.json` note, quoted in the viscosity-supersessions plan).
- **rhosr-CS** (`:1266-1292`), 7 fluids. Bell, Purdue 2016 (`Bell-PURDUE-2016-ETA`).
  x = ρ R (τ∂α^r/∂τ − α^r)/ρs_r,crit; ψ_liq = 1/(1+e^{−100(x−2)}); ln η*_ref = ψ_liq·f_liq(x) + (1−ψ_liq)·f_vap(x),
  with cubics evaluated by Horner; η = η0_kin·[1 + C(η*_ref − 1)]. In all 7 fluids the dilute η0 uses
  **Chung-estimated** Lennard-Jones parameters (σ = 0.809·V_c^{1/3}, ε/k = T_c/1.2593 from the EOS reducing state,
  `FluidLibrary.h:1005-1013`), because the first list entry carries no `sigma_eta`. Verified bit-exact against the
  oracle for R1234yf, R32 and R22 at ρ→0 (verifier: bit-exact for all 7 fluids at 300 K and 1e-6 mol/m³; a full Python
  re-implementation is also bit-exact for R1234yf saturated liquid and vapour at 250–340 K).
- **Chung** (`:630-675`), 2 fluids: Cyclopentane, Isopentane. Chung et al. IECR 1988. η = η_k + η_p using
  Neufeld's Ω22 fit including its sine correction term (:659-660; polarity enters only through μ_r⁴ in F_c and A_i),
  F_c = 1 − 0.2756ω + 0.059035μ_r⁴ + κ, Y = ρV_c/6, G1, G2 and the A1..A10 tables. T_c, V_c, M, ω and μ come from the
  Chung JSON block, not from the EOS (`FluidLibrary.h:693-701`).
  κ is hard-coded to 0 (`:646`); the JSON `kappa` key is never read.
- **Hardcoded whole models** (8): see §3.8.

### 3.3 Conductivity stage forms

| Stage / JSON `type` | Formula | Code | # | Source |
|---|---|---|---|---|
| dilute `ratio_of_polynomials` | Σ A_i T_r^{n_i} / Σ B_i T_r^{m_i} | `:677-694` | 34 | Assael/Huber/Perkins papers |
| dilute `eta0_and_poly` | A_0·η0[µPa·s] + Σ_{i≥1} A_i τ_EOS^{t_i}; **calls `calc_viscosity_dilute()`** | `:850-863` | 4 (Air, Ar, N2, O2) | Lemmon & Jacobsen IJT 2004 |
| residual `polynomial` | Σ B_i τ^{t_i} δ^{d_i} (own T_r and ρ_r in mass units) | `:696-709` | 36 | idem; the (B1 + B2·T/T_c)(ρ/ρ_c)^i form maps onto it |
| residual `polynomial_and_exponential` | Σ A_i τ^{t_i} δ^{d_i} e^{−γ_i δ^{l_i}} using **EOS** τ and δ | `:711-725` | 4 | Lemmon & Jacobsen 2004 |
| critical `simplified_Olchowy_Sengers` | §3.4 | `:727-777` | 34 + all 18 ECS | Olchowy & Sengers IJT 1989 |
| critical `hardcoded: None / R123 / Ammonia` | 0 / a13·exp(a14(τ−1)⁴ + a15(δ−1)²) / Tufeu crossover | `:779-782, 972-1003` | 4 / 1 / 1 | Laesecke IJR 1996; Tufeu BBPC 1984 |

**ECS conductivity** (`:1294-1373`; McLinden, Klein & Perkins IJR 2000; Huber 2003):
λ = f_int·η*·(c_p⁰ − 2.5R) + (15/4)·R·η* + Δλ_ref,residual(T0, ρ0·ψ_λ)·F_λ + λ_c,OS(target),
with F_λ = √f·h^(−2/3)·√(M0/M). The data has f_int ≈ 0.9–1.7e-3 over 250–400 K (verifier re-evaluation of all 18
blocks), which is the McLinden convention with η* in µPa·s; the `/1e3` at `:1323` completes the unit conversion, so
f_int·1e3 ≈ 0.9–1.7 is the usual Eucken-type factor (checked by order of magnitude only).

### 3.4 Simplified Olchowy–Sengers critical enhancement (`:727-777`)

λ_c = ρ c_p R_D k T/(6π η ζ) · (Ω − Ω0)
- Ω = (2/π)[((c_p − c_v)/c_p)·arctan(q_D ζ) + (c_v/c_p)·q_D ζ]
- Ω0 = (2/π)[1 − exp(−1/((q_D ζ)⁻¹ + (q_D ζ ρ_c/ρ)²/3))]
- ζ = ζ0 (Δχ/Γ)^{ν/γ}, Δχ = (p_c ρ/ρ_c²)·[∂ρ/∂p|_T(T,ρ) − (T_ref/T)·∂ρ/∂p|_T(T_ref,ρ)]
- λ_c = 0 if Δχ < 10·DBL_EPSILON (Lemmon IJT 2004, p. 27).

Constants and defaults (`CoolPropFluid.h:179-193`): k = 1.3806488e-23 (CODATA 2010), R_D = 1.03, ν = 0.63, γ = 1.239,
Γ = 0.0496, ζ0 = 1.94e-10 m, q_D = 2e9 m⁻¹, T_ref = 1.5·T_c. The JSON can override `qD`, `zeta0`, `GAMMA`, `gamma`,
`R0` and `T_ref`, but not `nu` or `k` (`FluidLibrary.h:918-935`). T_c, ρ_c and p_c come from the **EOS reducing
state** (`:735-737`).

Cost: two uncached α^r-derivative calls at (τ_ref, δ) through `calc_alphar_deriv_nocache` (`:752-754`, orders (0,1)
and (0,2)), plus c_p, c_v and **the fluid's viscosity** (`:765-767`). Inference from the §3.5 timings: this term
dominates non-ECS conductivity cost.

Verifier check: a Python re-implementation of `:727-777` from the low-level API reproduces the oracle's `critical`
contribution bit-exactly for R134a at 4 states (375–450 K, 5,000–8,000 mol/m³).

### 3.5 Cost of model families (oracle, Python-level, warm, single state; indicative only)

| Family | +η | +λ (λ recomputes η) |
|---|---|---|
| Staged (R134a, N2) | 0.4–0.6 µs | 2.5–3.6 µs (O-S dominated) |
| rhosr-CS (R1234yf) / Chung / friction theory | 0.1–0.4 µs | 2.0–2.4 µs |
| Hardcoded water (critical enhancement with 2 no-cache α^r calls) | 6 µs | 13 µs |
| **ECS (R11 → R134a)** | **9 µs** | **18 µs** (two separate conformal solves) |

Verifier re-run (same method, best of 5 × 3,000 updates): staged 0.40–0.57 / 2.5–2.8 µs, R1234yf and Cyclopentane
0.15–0.34 / 2.0–2.7 µs, Water 9.1 / 15.7 µs, R11 9.5–9.9 / 20–22 µs. The ranking holds; absolute values are machine-dependent.

### 3.6 Surface tension (`Ancillaries.h:52-64`, `HelmholtzEOSMixtureBackend.cpp:703-713`)

σ = Σ a_i (1 − T/T_c,σ)^{n_i}. Coefficients come from 75 Mulero JPCRD 2012, 27 Mulero JPCRD 2014, 4 Okada IJT 1999,
1 IAPWS 1994 and 1 Kondou IJR 2015 correlations; 74 fluids use 1 term, 23 use 2 and 11 use 3.
It throws if T > T_c,σ, and it is only reachable when `_phase` is two-phase or the critical point. 28 fluids have no
correlation. In 18 fluids T_c,σ differs from T_c,EOS by more than 1 mK (§6 R13). At exact inequality the count is
20: CO2 and SF6 also differ, by 0.2 mK. T_c,σ < T_c,EOS (the throwing case) in 9 of the 20: Ammonia, CO2, Ethanol,
IsoButane, R1234ze(E), R404A, R407C, R507A and SF6. In the other 11, σ(T_c,EOS) > 0 instead (verifier, from
`dev/fluids/*.json` STATES.critical).

### 3.7 Mixtures (`HelmholtzEOSMixtureBackend.cpp:816-823, 1060-1067`)

η_mix = exp(Σ x_i ln η_i(T, ρ_mix)) and λ_mix = Σ x_i λ_i(T, ρ_mix). Each call builds a **new** `HelmholtzEOSBackend`
per component, which copies its `CoolPropFluid`, and flashes it at the mixture's (T, ρ). That state can lie inside
the pure component's dome. The only signal is a warning string. Master's gap spec calls this "the largest predictive
gap" (`master:docs/superpowers/specs/2026-09-06-refprop-coolprop-functionality-gaps.md:108-133`).

### 3.8 Hardcoded catalogue: what each routine does, and whether it is necessary or rot

| Routine (`TransportRoutines.cpp`) | Fluid / source | Verdict for the Rust port |
|---|---|---|
| `viscosity_water_hardcoded` :252-304 (+ `visc_Helper` :182-234) | Water; IAPWS 2008 (Huber JPCRD 2009) | **Necessary** (critical enhancement, Y(ξ) branch). Share the ξ code with conductivity. |
| `conductivity_hardcoded_water` :882-943 | Water; IAPWS 2011 (Huber JPCRD 2012) | **Necessary**. Duplicates the χ/ξ code of the viscosity routine (master be2bbd33 notes the pending dedupe). π = 3.141592654 (:912). |
| `viscosity/conductivity_heavywater_hardcoded` :235-251, :865-880 | D2O; IAPWS 2007 | Necessary, but superseded by IAPWS R17-20/R18-21 on master (93a41972). |
| `viscosity_helium_hardcoded` :403-438 / `conductivity_hardcoded_helium` :1005-1062 | He; Arp NIST 1998 / Hands Cryogenics 1981 | Necessary but **unverifiable**: "referring to REFPROP source code" (:410-411); "magical coefficients … not clear why" (:1056-1059). |
| `viscosity_methanol_hardcoded` :440-496 | Xiang JPCRD 2006 | Necessary (hard-sphere blend, logistic switch). |
| `viscosity_R23_hardcoded` :498-518 / `conductivity_hardcoded_R23` :945-970 | Shan ASHRAE 2000 | Same structure twice. Make it one typed "Shan" form. |
| `viscosity_{o,m}_xylene_hardcoded` :520-561, `p_xylene` :562-579 | Cao 2016, Balogun 2015 | **Duplicated code, not wrong data**: the three routines repeat identical dilute and initial-density constants (A0 = −1.4933, B0 = 473.2, …); only the leading constant and residual differ. Verifier: all 33 paper check values in `CoolProp-Tests.cpp:248-282` (including the ρ→0 and 40–49 mol/m³ points that isolate those terms) pass at < 6e-5, so the shared constants reproduce each paper. Make it a typed family; the cyclohexane dilute :593-598 has the same exp(A + B/T + C/T²) shape. |
| `viscosity_dilute_ethane` :581-592 | Friend JPCRD 1991 | Same Ω22 series as the methane routine :1070-1077. Make it typed. |
| `viscosity_dilute_CO2_LaeseckeJPCRD2017` :600-612, `viscosity_CO2_higher_order_…` :347-356 | Laesecke & Muzny JPCRD 2017 | Small closed forms. Typed or per-fluid fn. Uses `HEOS.Ttriple()` (EOS coupling). |
| `viscosity_{hydrogen,hexane,heptane,toluene,benzene,ethane}_higher_order_hardcoded` :305-345, :614-629 | Muzny 2013, Michailidou 2013/2014, Avgeri 2014/2015, Friend 1991 | Distinct rational functions of (T_r, ρ_r) per fluid; the master DSL now expresses this family as data. Keep as per-fluid fns or DSL. |
| `conductivity_dilute_hardcoded_CO2_HuberJPCRD2016` :830-838 | Huber JPCRD 2016 | Trivial: λ0 = τ^{−1/2}/Σ l_i τ^i in mW/m/K. Typed. |
| `conductivity_dilute_hardcoded_ethane` :840-848 | Friend 1991 | Needs η0 and ∂²α⁰/∂τ²; mixes correlation τ (305.33) with EOS τ (305.322). Keep but fix. |
| `conductivity_hardcoded_methane` :1063-1140 | Friend JPCRD 1989 | Necessary but rotten: recomputes its own viscosity, reads a saturation **ancillary** and config-dependent T_c/ρ_c (:1104-1105), "Looks like a typo in Friend" (:1103), NaN in the dome. |
| `conductivity_critical_hardcoded_R123` :779-782 | Laesecke IJR 1996 | Trivial; typed. |
| `conductivity_critical_hardcoded_ammonia` :972-1003 | Tufeu BBPC 1984 | **Defective** (§6 R3). Master default replaces it (Monogenidou JPCRD 2018). |
| `conductivity_dilute_hardcoded_CO2` :798-828, `conductivity_critical_hardcoded_CO2_ScalabrinJPCRD2006` :784-796 | Vesovic 1990, Scalabrin 2006 | **Dead**: no fluid selects them. Drop. |

### 3.9 Expression DSL (master only: what, why, how big)

- **Problem it solves** (`master:docs/superpowers/specs/2026-06-12-transport-expression-dsl-design.md:8-26`): a new
  correlation form costs four C++ edits (struct + enum, parser branch, dispatch case, routine) and a rebuild. The DSL
  makes "Tier A" parametric forms data. Tier B (O-S, friction theory, Chung) and Tier C (ECS) and all `*_hardcoded`
  routines stay in C++ "by explicit decision" (`:65-73`).
- **Language:** `let` bindings, `+ − * / ^` (right-associative, `^` = `std::pow`), 12 functions, and a single-level
  `sum(i: …)` over co-indexed arrays with lengths checked at compile time. All inputs and outputs are base SI. Each
  block declares `state_variables` from an allowlist of 8: T, P, Dmolar, Dmass, molar_mass, Smolar_residual, Bvirial,
  dBvirial_dT (`master:src/expression/Expression.cpp:545-551`). The host fills them with `keyed_output()`
  (`master:src/expression/ExpressionCorrelation.cpp:14-33`).
- **Evaluator:** a virtual-dispatch tree-walk over `unique_ptr` nodes. Each call heap-allocates a `scalars` vector
  (`Expression.cpp:814`) and an inputs vector (`ExpressionCorrelation.cpp:19`). The compiled `Program` is immutable and
  shared, so it is thread-safe. The only in-tree performance check asserts "< 5× the hardcoded routine"
  (`master:src/Tests/CoolProp-Tests-Expression.cpp:599-646`).
- **Adoption on master:** 69 blocks in 19 fluids (28 read T only, 40 read T and density, 1 reads Krypton's
  entropy-scaling inputs). Most are shapes the typed forms already have: ratio-of-polynomials dilute,
  Σ(B1 + B2·T_r)ρ_r^d residual, collision-integral dilute, Rainwater–Friend, Σnτ^tδ^d. A few shapes are new:
  rational-in-T dilute, exp(Σa_i(ln T/T_ref)^{p_i}) dilute, and the ρ_r^{2/3}√T_r·f(T_r, ρ_r) residual family that
  v8.0.0 hardcodes. One block is a one-off (Krypton entropy scaling).
- **Data duplication by design:** a stage cannot see another stage's output, so initial-density blocks recompute η0
  with duplicated arrays (spec §3a, commit 9415d9e8), and Krypton's higher-order block subtracts η0.
- **Verdict:** see §9. Use typed enums in the core. A DSL is optional and later, and should evaluate over batches.

## 4. Data and configuration inputs

**v8.0.0 default-model distribution (first list entry; per-fluid lists from `dev/fluids/*.json`).**

| Viscosity model | Fluids |
|---|---|
| CI + RF + BH (9) | Ammonia, IsoButane, R134a, n-Butane, n-Decane, n-Dodecane, n-Nonane, n-Octane, n-Propane |
| CI + BH (5) | Air, Argon, DimethylEther, Nitrogen, Oxygen |
| powers_of_T (+RF) + BH (6) | R123, R404A, R407C, R410A, R507A, Ethanol (+RF) |
| kinetic + RF + BH (1) | R125 |
| CI + RF + hardcoded HO (6) | Benzene, Hydrogen, ParaHydrogen, Toluene, n-Heptane, n-Hexane |
| hardcoded dilute + … (3) | CarbonDioxide (+RF + H-HO), CycloHexane (+empirical + BH), Ethane (+H-HO) |
| friction theory (4) | Methane, n-Pentane (powers_of_Tr), SulfurHexafluoride (powers_of_T), HydrogenSulfide (CI-T* + RF) |
| ECS (15) | → Propane: EthylBenzene, Propylene, R13, R141b, R142b, R218, R227EA, RC318; → R134a: R11, R116, R12, R143a, R236EA, R236FA; → Nitrogen: R14 |
| rhosr-CS (7) | R1234yf, R1234ze(E), R124, R152A, R22, R245fa, R32 |
| Chung (2) | Cyclopentane, Isopentane |
| hardcoded whole (8) | Water, HeavyWater, Helium, R23, Methanol, m-/o-/p-Xylene |

| Conductivity model | Fluids |
|---|---|
| ratio + polynomial + O-S (28) | Benzene, Cyclopentane, Ethanol, EthylBenzene, Hydrogen, IsoButane, Isopentane, Methanol, ParaHydrogen, R1234yf, R1234ze(E), R125, R134a, R152A, SF6, Toluene, the three xylenes, n-Butane, n-Decane, n-Dodecane, n-Heptane, n-Hexane, n-Nonane, n-Octane, n-Pentane, n-Propane |
| eta0_and_poly + poly&exp + O-S (4) | Air, Argon, Nitrogen, Oxygen |
| ratio + polynomial + None (4) / + R123 (1) / + Ammonia (1) | R404A, R407C, R410A, R507A / R123 / Ammonia |
| hardcoded dilute + polynomial + O-S (2) | CarbonDioxide (Huber 2016), Ethane (Friend 1991) |
| ECS (18) | → Propane: Propylene, R124, R13, R141b, R142b, R218, R227EA, R32, RC318; → R134a: R11, R116, R12, R143a, R22, R236EA, R236FA, R245fa; → Nitrogen: R14 |
| hardcoded whole (5) | Water, HeavyWater, Methane, R23, Helium |
| none (3 with viscosity only) | CycloHexane, DimethylEther, HydrogenSulfide |

- **JSON shape:** `TRANSPORT.viscosity` is an object or a list (only `[0]` is used, `FluidLibrary.h:715-718`).
  `TRANSPORT.conductivity` must be an object at v8.0.0 (no list support, `:942-944`). Stage dispatch is by
  `type`/`hardcoded` strings. `BibTeX` is required. `sigma_eta` and `epsilon_over_k` are optional; Chung defaults
  fill them (`:724-726`).
- **Ignored data:** the 7 rhosr-CS fluids carry a second list entry (ECS or Krauss 1996) that is never loaded,
  including the LJ parameters that 4 of them would need for their ECS conductivity (§6 R17).
  18 ECS conductivity blocks carry `q_D`, which is never read (§6 R2). `nu` is ignored (CO2). `kappa` is ignored
  (Chung, always 0 in the data). `x_crossover` was ignored until master 5418cec5. `_units` and `_note` keys are
  decoration (as is an unprefixed `note` in SF6's residual block). A verifier inventory of every TRANSPORT key against
  the parser found no other unread keys.
- **Derived constants stored as data:** `rhosr_critical` should be computed from the EOS critical point (agent-notes;
  master 14da1f0d found R1233zd(E)'s value stale after an EOS swap). At v8.0.0, R1234yf's is already 0.68 % off its
  own EOS (§6 R4).
- **EOS and config inputs read at evaluation time:** EOS reducing state (O-S, poly&exp, eta0_and_poly τ, R123 τ/δ,
  Huber-2016 CO2 dilute τ at `TransportRoutines.cpp:832`), `Ttriple()` (CO2), the saturation ancillary ρ_V (methane),
  and `ENABLE_SUPERANCILLARIES` through `T_critical()` and `rhomolar_critical()` (methane, ECS initial guess and
  Jacobian). **Read at load time:** the Chung LJ defaults are computed once from the EOS reducing state when the fluid is
  parsed (`FluidLibrary.h:1009-1012`), then frozen in `sigma_eta`/`epsilon_over_k`.
- **Drift v8.0.0 → master** (relevant to choosing the oracle): the default viscosity changed for 9 fluids (Ammonia,
  EthylBenzene, HeavyWater, Methane, Nitrogen, R1234yf, R1234ze(E), R245fa, R32) and was added for 10 (Ethylene,
  Krypton, Novec649, PropyleneGlycol, R1132a, R1233zd(E), R161, THF, Xenon, n-Undecane). The default conductivity
  changed for 5 (Ammonia, HeavyWater, Nitrogen, Propylene, R245fa) and was added for 9. `TransportRoutines.cpp` grew
  from 1,375 to 1,489 lines (verifier re-diffed all 137 master fluid files: counts and names confirmed).

## 5. State, caching, globals, thread-safety, memory

- **Global library:** a `static JSONFluidLibrary library` (`FluidLibrary.cpp:28`) loaded once with `std::call_once`
  (`:40-44`). The **first touch parses all 136 fluids**, transport included, and keeps a `dump()` JSON string per fluid
  (`:338`). `get_fluid` returns a full **copy** (`:376-379`). Runtime `add_fluids_as_JSON` mutates the global without
  a lock (inference from the code; out of this area).
- **Per-AbstractState copies:** each backend copies its `CoolPropFluid`s. `TransportPropertyData` stores all variant
  structs at once: exactly 57 `std::vector`s, mostly empty, plus 4 strings. `sizeof(TransportPropertyData)` is
  1,928 bytes before coefficients (verifier measurement: g++ / libstdc++ x86-64, `CoolPropDbl = double`;
  `sizeof(CoolPropFluid)` is 4,440).
- **ECS reference backends:** a mutable `shared_ptr<HelmholtzEOSMixtureBackend>` per AbstractState **per property**
  (`HelmholtzEOSMixtureBackend.h:78-79`). R11 therefore builds two independent R134a backends. Each call re-solves and
  overwrites their state (`TransportRoutines.cpp:1153, 1180, 1244, 1354`), so an AbstractState cannot be shared across
  threads and every thread pays for its own reference copies.
- **Mixture rule:** builds a fresh pure backend per component per call (§3.7). That means heap churn plus a full fluid
  copy on every call.
- **Caches:** `_viscosity`, `_conductivity` and `_surface_tension` CachedElements (`AbstractState.cpp:793-813`). O-S,
  water and helium conductivity call `HEOS.viscosity()`, so a cached η is reused, but λ alone computes η internally.
- **Hidden mutation in "data":** `SurfaceTensionCorrelation::evaluate` writes a scratch buffer `s` (`Ancillaries.h:61`),
  so it is non-const and the fluid data cannot be shared immutably. The Scalabrin routine has a non-const
  `static` array (`TransportRoutines.cpp:786`).
- **Global config dependence:** methane conductivity changes by up to ~1e-7 relative when `ENABLE_SUPERANCILLARIES` is
  toggled (oracle). Verifier scan of 75 single-phase states at 100–190.5 K: every state moves; the largest shift is 1.06e-7
  at 185 K, 80 kg/m³. The mapper's 1.5e-7 had no stated state. Cause: `T_critical()`/`rhomolar_critical()` change from
  190.564/10139.128 to 190.5640027/10139.1377.
- **DSL (master):** `Program` is immutable and shared, so concurrent evaluation is safe, but each evaluation makes two
  heap allocations.
- **Lazy-loading implications:** the transport closure of a fluid can include (a) the ECS reference fluid's EOS and
  background stages (Propane: 10 dependents, R134a: 8, Nitrogen: 1), (b) the fluid's own viscosity model for
  conductivity (54/63), (c) the fluid's ideal-gas and residual EOS (c_p, c_v, c_p⁰, two α^r states), and (d) for
  methane, a saturation ancillary. Today none of this is declared; it is discovered at first evaluation.

## 6. Rot and bugs (table: issue | evidence | impact | Rust remedy)

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| R1 | Two-phase states return meaningless η and λ | No phase check in `HelmholtzEOSMixtureBackend.cpp:810-814, 1054-1058`. Oracle: Water at 0.9·T_c, Q = 0.5 gives η = 2.078e-5 (liquid 8.24e-5, vapour 2.01e-5). Methane λ at Q = 0.5 gives NaN, and PropsSI gives an empty error. Fixed on master da48a3ee (#3446). | Silent garbage from the oracle | Transport takes a single-phase state type. 0 < Q < 1 → typed error. Explicit saturated-liquid/vapour API. Never compare against the oracle inside the dome. |
| R2 | ECS conductivity ignores per-fluid `q_D` | `parse_ECS_conductivity` (`FluidLibrary.h:668-680`) never reads `q_D`. R13, R14, R142b, R218 and RC318 have q_D ≠ 2e9, but the default (`CoolPropFluid.h:188`) is used. Verifier oracle proof: loading a copy of R218 with `q_D` = 1e7 through `add_fluids_as_JSON` gives a bit-identical λ at 3 states. My O-S re-implementation reproduces the oracle bit-exactly (R134a; verifier-confirmed, §3.4); honouring q_D would change λ by −17.6 % (R218) to +22.4 % (R14) near T_c (mapper, state not given). Verifier re-run at 0.98–1.05 T_c and 0.3–1.5 ρ_c: R218 −6.9 to −16.4 %, R14 +12.6 to +24.0 %, RC318 +5.8 to +10.5 %, R13 +4.9 to +10.3 %, R142b −2.7 to −5.5 %. All 5 blocks cite Huber-IECR-2003. Not fixed on master. | Probably wrong near-critical λ for 5 fluids (inference: the values are Huber 2003's; check the paper) | Strict schema: every key is consumed or rejected. Verify against Huber IECR 2003, then decide on parity (open question 2). |
| R3 | Ammonia critical term (Tufeu 1984) diverges at its own T_c = 405.4 K at **every** density | `TransportRoutines.cpp:972-1003`: t = \|T − 405.4\|/405.4, log(t), t^(−ν). EOS T_c = 405.56. Oracle, single-phase gas: λ(405.3999 K, 50 kg/m³) = 6.26 W/m/K and (405.3999, 100) = 24.9 W/m/K; at T = 405.4 it is NaN. | ~100× error in a valid vapour state | Treat as a known oracle defect. Adopt Monogenidou JPCRD 2018 (master default) or bound the term. Pin a regression test. |
| R4 | rhosr-CS correctness and performance | `x_crossover` is parsed (`FluidLibrary.h:708`) but ignored: the crossover is hard-coded at 2 (`:1278`, fixed 5418cec5). All 7 v8.0.0 files store exactly 2, so there is no numerical effect at v8.0.0; it is a latent data-contract defect. `cV` copied by value on every call (`:1281`, fixed be2bbd33). `rhosr_critical` is a stale derived constant (14da1f0d found this for R1233zd(E) on master). **Verifier: already stale at v8.0.0 for R1234yf.** Its EOS was swapped to Lemmon-IJT-2022 in cd69d4b5 (#2725), but the stored −54468.48 dates from 4542f9d0. The current EOS gives −54095.50 (−0.68 %); the other 6 fluids match to ≤ 7e-7. Recomputing it would raise R1234yf saturated-liquid η by +0.8 % (340 K) to +1.6 % (250 K); vapour moves < 0.01 %. Master keeps the stale value but demotes the block to a non-default list entry. Dilute LJ parameters silently come from Chung (verified bit-exact; "separate defect" in 14da1f0d). | Hidden data dependence on the EOS (1–2 % in R1234yf liquid η today); one heap allocation per call | Derive `rhosr_critical` from the EOS at load and check it against JSON. Require explicit LJ parameters, or name the Chung estimate explicitly as a typed choice. |
| R5 | Model lists half-supported | Only `viscosity[0]` is parsed; the alternatives are dead data (`FluidLibrary.h:715-718`). An empty list hits `front()` (UB; fixed 538d4227). Conductivity lists are not supported at v8.0.0 (`:942-944`). | Cannot select a model, which ECS pinning and literature upgrades both need | Load all entries lazily, give each a `ModelId`, default to `[0]`, and allow selection per state or per handle. |
| R6 | Implicit, unpinned cross-property coupling | O-S divides by the **current** η (`:767`). `eta0_and_poly` and the ethane dilute term read η0 (`:855, :845`). Water and helium call `viscosity()` (`:925, :1026`). That covers 54 of 63 conductivity models. Master agent-notes: "Changing one fluid's viscosity can move other fluids". The gap spec lists forced choices (f1ez, 3bpg). | Upgrading η silently changes λ, and changes λ of other fluids too | Each conductivity model declares the viscosity `ModelId` it was fitted with. ECS declares (reference fluid, reference model). Build a load-time DAG that rejects cycles and missing references. |
| R7 | ECS resolution and solver hygiene | Reference built by name on first evaluation (`HelmholtzEOSMixtureBackend.cpp:848-852, 944-948`). No check against ECS chains: a non-staged reference only fails at runtime in `calc_viscosity_background` (:802-804) or `calc_conductivity_background` (:1048-1050). `catch (...)` swallows everything (`TransportRoutines.cpp:1189`). Gas phase imposed "not checked" (`:1151`). `update_DmolarT_direct` at :1244 vs `update(DmolarT_INPUTS)` at :1354. Jacobian uses config-dependent `T_critical()` instead of the reducing T (`:1155-1156`). | Late failures, opaque errors, two reference copies | Evaluate the reference EOS directly at (τ, δ), which needs no phase logic. Resolve references at load. Typed solver errors carrying the iteration state. |
| R8 | Contributions API mislabels results | ECS, Chung, rhosr and hardcoded η totals are reported as `critical` (`:854, 861, 868, 876-897`). ECS and hardcoded λ totals are reported as `initial_density` ("Warning: not actually initial_density", `:950, 959-971`). Oracle: R11 η = {critical: 1.57e-5, others 0} (mapper, state not given). Verifier, R11 at 300 K and 101,325 Pa: η = {critical: 1.0209e-5, others 0} and λ = {initial_density: 8.525e-3, others 0}. | Wrong answers to "how big is the critical part?" | Typed `Breakdown::{Staged{..}, Monolithic(total)}`. |
| R9 | Dead branches and dead or uninitialised fields | No fluid uses CO2 Vesovic dilute (`:798-828`, whose comment admits it "does not yield exactly the correct values"), dilute `none`, or Scalabrin critical (`:784-796`). `CONDUCTIVITY_RESIDUAL_R123` is never set. `RESIDUAL_CO2` is parsed (`FluidLibrary.h:849-851`) but has no dispatch case, so it throws (`HelmholtzEOSMixtureBackend.cpp:1041-1051`). `p_reducing` (`CoolPropFluid.h:130`) and `AdrAdr` (:283) are unused. The O-S struct's `T_reducing` and `p_reducing` (:178) are also unused (added by verifier; no reader in `src/` or `include/`). poly&exp `T_reducing`/`rhomass_reducing` are uninitialised and unused (:152). | Maintenance noise; latent throws | Drop. Closed enums. `serde(deny_unknown_fields)`. |
| R10 | Constants frozen in the wrong place | Methane reads T_c and ρ_c through config-dependent accessors plus an ancillary (`:1104-1105`), giving up to ~1e-7 drift (oracle, §5). Ethane multiplies the correlation τ (305.33) by the EOS ∂²α⁰/∂τ² (EOS T_c = 305.322, `:843-845`), so τ² is off by 5.2e-5 relative in that term. Chung LJ defaults come from the EOS reducing state at load time (`FluidLibrary.h:1009-1012`). Lemmon–Jacobsen τ and δ come from the EOS (`:717`). Master f854d34b: "Reducing parameters belong to the correlation". | Results move when the EOS or config changes | Every correlation carries its published reducing constants. Values copied from the EOS are snapshotted at load and validated. Correlations never read config. |
| R11 | Validation only through `assert` (NDEBUG) | BH and friction-theory length checks (`FluidLibrary.h:603-617, 637-639`). `Ai[0..2]` and `Aii[0..2]` are read without checks (`TransportRoutines.cpp:367, 380`), and so are `Arrr`/`Aaaa` (:383-384, added by verifier; `FluidLibrary.h:656-661` has no assert). | OOB read on bad user JSON | Fixed-size `[f64; 3]` types and validated equal-length arrays at load. |
| R12 | Copy-paste and inconsistent constants | Xylene triplet (`:520-579`): duplicated code only; the shared constants reproduce all 33 paper check values (§3.8), so this is DRY, not wrong data. Water η and λ duplicate the ξ code (`:269-301` vs `:915-940`). R23 η and λ (`:498-518` vs `:945-970`). Friend Ω22 twice (`:581-592` vs `:1070-1077`). k_B = 1.3806504e-23 in ammonia (`:983`) vs 1.3806488e-23 in O-S. π = 3.141592654 (`:912, :983`). The numerical effects are tiny: k_B shifts ammonia's critical term by 1.2e-6 relative, and the truncated π shifts the water and ammonia terms by 1.3e-10. | Inconsistency; harder to audit against papers | Shared typed forms. Constants kept per correlation as published, replicated exactly for oracle parity. |
| R13 | Surface-tension design | Redundant derived members `N` and the mutable scratch buffer `s` (`Ancillaries.h:27-31, 52-64`) once silently zeroed every σ. A clang-tidy rewrite had initialised them before `n` was filled; the comment and guard test are at `CoolProp-Tests.cpp:6599-6619`. Requires a two-phase AbstractState (`HelmholtzEOSMixtureBackend.cpp:705-708`). T_c,σ ≠ T_c,EOS in 18 fluids (> 1 mK, §3.6). Ethanol at 514.2 K (two-phase per the EOS) throws "Must be saturated state : T <= Tc" (oracle, verifier-confirmed). | Avoidable failures | Pure `fn sigma(&self, T) -> Result`, with an explicit domain policy for T_c,σ < T ≤ T_c,EOS. |
| R14 | Friction theory reads cached `p()` instead of the EOS p(T, ρ) | `TransportRoutines.cpp:387`. Master da48a3ee: re-flash moved Methane/H2S mixture η by up to 7 %, and master deliberately kept the old values ("pinned to master"). This matters only where `p()` ≠ p_EOS(T, ρ): two-phase states (R1) and mixture-rule components inside their own dome. ∂p/∂T at :389 is EOS-based, so p_a mixes the two sources. | State-path dependence | Compute p and ∂p/∂T from the EOS at (T, ρ) inside the inputs. |
| R15 | Data regressions and stale tests | R1233zd(E) TRANSPORT was dropped by the v8 EOS swap (#2768), so the oracle throws (restored on master 14da1f0d). Commented-out R1234yf/ze λ checks now pass within 0.003 % (oracle; verifier max 0.0028 %). The disabled n-Decane λ check uses a molar density as `Dmass` (`CoolProp-Tests.cpp:419`, still commented "no viscosity" although n-Decane now has one). The Benzene 575 K point is 1.45 % off (disabled; verifier also finds the disabled 500 K, 32 kg/m³ point 0.106 % off, over its 1e-4 tolerance). The 7-point ParaHydrogen λ block is inside a `/* */` comment (:400-406). Wrong field in error text (`HelmholtzEOSMixtureBackend.cpp:804, 1031`). | Unknown coverage gaps | Data-pack tests assert per-fluid model presence. Re-curate the check tables. |
| R16 | DSL costs (master) | Two heap allocations and virtual node dispatch per evaluation (`master:src/expression/Expression.cpp:814`, `ExpressionCorrelation.cpp:19`). η0 recomputed per stage (9415d9e8). Golden tests not bit-exact (FMA; a849e8e1). | Interpreter overhead in the hot path; duplicated coefficients | Typed forms for the shapes that exist. If a DSL is kept: compile to a flat SSA/bytecode and evaluate over SoA batches. |
| R17 (added by verifier) | ECS conductivity of R124, R22, R245fa and R32 uses Chung-estimated LJ parameters | These 4 fluids use rhosr-CS for viscosity (first list entry, no `sigma_eta`) but ECS for conductivity. `default_transport` therefore fills σ and ε/k from Chung (`FluidLibrary.h:724-726, 1005-1013`), and `conductivity_ECS` uses them for η* in λ_int and λ_dilute (`TransportRoutines.cpp:1317, 1323, 1326`). Oracle: dilute-gas λ equals the ECS formula with Chung LJ to ~1e-10 at 250–400 K. The LJ parameters that ship with the same Huber-2003/Klein-1997 ECS model sit in the never-parsed second viscosity entry (e.g. R245fa σ = 0.5529 nm vs Chung 0.5151 nm). With them, dilute-gas λ would be lower by 5.4–5.7 % (R32), 8.1–8.3 % (R22), 9.6–10.5 % (R124) and 11.9–12.0 % (R245fa). The 15 ECS-viscosity fluids carry explicit LJ parameters and are unaffected. | Probably wrong low-density λ for 4 fluids (inference: the ECS papers pair each λ model with their tabulated LJ parameters; check Huber 2003 and McLinden 2000) | Each ECS model carries its own LJ parameters; never fill them from another model's absence. Decide parity and record it in the oracle-defect registry after the paper check. |

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

| Component | Fit | Notes |
|---|---|---|
| Stage sums (Σ a T^t, Σ n δ^d τ^t, exp(Σ a (ln T*)^t), ratio-of-polynomials, residual polynomials) | **SIMD across states** | 1–16 terms per stage (Batschinski–Hildebrand up to 16, Rainwater–Friend 7–13, most others 2–12), so vectorise across a batch (SoA). Classify exponents at load (integer → `powi`/Horner; real → exp(t·ln x) with ln τ and ln δ computed once per state). The fast path is not bit-exact against the oracle, so it is a side-by-side implementation cross-tested at about 1e-14. |
| Surface tension | SIMD across T | Trivial. |
| Olchowy–Sengers | SIMD across states if the EOS kernel batches | Dominated by the second α^r evaluation at (T_ref, ρ) plus c_p and c_v. The `Δχ < 10ε` cut becomes a mask. |
| rhosr-CS, Chung, friction theory, Rainwater–Friend, BH | SIMD-able closed forms | Need EOS scalars (α^r, τα^r_τ, p, ∂p/∂T). BH has a pole at δ0(τ) = δ. |
| Hardcoded water | Partly | Y(ξ) branch at ξ = 0.3817 (η) and y < 1.2e-7 (λ); two extra α^r calls each. Masked SIMD is possible but low value. |
| Hardcoded D2O (IAPWS 2007), R23, methanol | SIMD-able closed forms | Branch-free functions of (T, ρ): D2O uses only `abs`, methanol a smooth logistic switch. No EOS calls. (Corrected by verifier: the original grouped D2O with water and R23/methanol with the branchy routines.) |
| Hardcoded helium, methane, ammonia | Branchy, scalar | Helium is piecewise in T (≤ 100, ≤ 300, 3.5–12 K) and W; methane has branches plus a saturation-ancillary call; ammonia has one density select (cheap to mask). Thread-level only for helium and methane. |
| ECS | **Sequential per state** | 2-D Newton with step halving and data-dependent iteration counts. Parallel across states and threads; warm-start along a batch path. It is the most expensive family (9–18 µs in the oracle). |
| Mixture rule | Per-component fan-out | Trivially parallel, but the model itself should be redesigned. |
| DSL tree-walk | Poor | A batch interpreter (each op over N lanes) amortises dispatch. Build-time transpilation to Rust gives LLVM autovectorisation. |
| Multi-request concurrency | **Excellent once immutable** | Correlations are pure functions of (immutable coefficients, EOS-derived inputs). Today's blockers are the mutable ECS backends, AbstractState caches and the surface-tension buffer. |

**Side-by-side seam (proposal):** each typed form implements two methods.
- `eval(&Coeffs, &Inputs) -> f64` is the reference path. It keeps the oracle's operation order and is the TDD target.
- `eval_batch(&Coeffs, &InputsSoA, &mut [f64])` has a portable default that loops over `eval`, and may be overridden
  by SIMD or GPU back ends behind cargo features.

Requests on the **same** fluid share one `Coeffs` set, so they batch into SIMD lanes. Requests on **different**
fluids spread across threads, with no locks because the data is immutable. Mixed batches are grouped by
(fluid, model) first.

Not suited to SIMD: ECS (iterative), the hardcoded branchy routines and the DSL interpreter. These keep only the
scalar path and use thread-level parallelism.

Equivalence tests run every fast path against the reference path over the same grids, with an ULP or relative bound
per form.

## 8. Verification assets (tests, check values, paper tables, oracle hooks)

- **Published check values (v8.0.0):** `src/Tests/CoolProp-Tests.cpp:44-300` has 177 active viscosity points across
  36 fluids, each block annotated with its source paper (Vogel 1998, Huber 2004/2006/2009, Lemmon–Jacobsen 2004,
  Laesecke 2017, the IAPWS releases, Cao 2016/Balogun 2015, …), plus 16 disabled blend points.
  `:347-581` has **141 active conductivity points across 34 fluids, plus 36 disabled** (verifier recount: 29 `//` lines
  plus the 7-point ParaHydrogen `/* */` block at :400-406, which the mapper had counted as active). The fixture is at
  `:303-345` and `:583-600`. Water surface tension against IAPWS is at `:6604-6619`.
- **Not every check value is a literature value (added by verifier).** Several blocks are labelled as taken from
  REFPROP 9.1 "since … does not provide validation data" (`CoolProp-Tests.cpp:64, 70, 221` for viscosity;
  `:426` Marsh 2002, `:503` Tufeu ammonia and `:536` R134a for conductivity). `:76` is "From CoolProp v5 implementation",
  and the dense xylene λ values are "taken from the implementation in REFPROP 10.0" (`:542`). These are
  cross-implementation checks, not arbiters; tag them as such in the port's test data.
- **Oracle hooks (CoolProp 8.0.0 wheel):** `AbstractState.viscosity()`, `.conductivity()`, `.surface_tension()`,
  `.Prandtl()`, `.viscosity_contributions()` and `.conductivity_contributions()` (stage-level for staged fluids; beware
  R8), `.conformal_state(ref, T, ρ)` (tests the ECS solver alone), `saturated_{liquid,vapor}_keyed_output`,
  `PropsSI('V'|'L'|'I'|'PRANDTL', …)`, `get_BibTeXKey(f, 'VISCOSITY'|'CONDUCTIVITY'|'SURFACE_TENSION')`, and
  **`get_fluid_param_string(f, 'JSON')`**, which extracts the exact v8.0.0 data block from the wheel.
  `set_config_bool(ENABLE_SUPERANCILLARIES, …)` probes config sensitivity.
- **Literature-arbiter assets (master, usable without CoolProp):** paper verification points in
  `master:src/Tests/CoolProp-Tests-Expression.cpp`: Nitrogen 2024 Tables 7 and 8 (:1078-1193), Argon 2025 Table 9
  plus computer-verification points (:1194-1290), Krypton (:1291-1363), and golden Tier-A form tests at 1e-14 vs the
  hardcoded routines (:457-790, :999). Xenon Section 4, R-161 Table 11 and Ethylene Table 8 are quoted in the commit
  messages of f854d34b and 9415d9e8. IAPWS R17-20 and R18-21 Tables 3–4 are encoded in master 93a41972's tests.
  The R1233zd(E) refit has 61 Miyara points in `master:dev/scripts/fit_R1233zdE_viscosity.py`. Python DSL tests are in
  `master:wrappers/Python/pytest/test_expression.py`.
- **Release documents:** IAPWS R12-08 (H2O viscosity), R15-11 (H2O conductivity), R1-76(2014) (surface tension),
  R17-20 and R18-21 (D2O) each carry verification tables. Most JPCRD/IJT reference correlations publish
  computer-verification values. The REFPROP-10.1 plans (`master:docs/superpowers/plans/2026-08-29-refprop-101-*`)
  list open-access sources per fluid.
- **Measured facts to pin as regression tests:** R1–R3, R13 and the verifier-added R4 (R1234yf) and R17 (oracle
  numbers above). The O-S and rhosr-CS re-implementation scripts that matched the oracle bit-exactly are templates for
  stage-isolated tests.

## 9. Port recommendation (units with priority P0-core / P1-early / P2-later / defer / drop; order; what to redesign)

Numbers in parentheses are the C++ lines each unit replaces.

| Unit | Priority | C++ source | Design |
|---|---|---|---|
| U1 `TransportInputs` (state view) | **P0** | every `HEOS.*` call in `TransportRoutines.cpp` | Trait or struct supplying T, ρ (molar and mass), and the EOS-derived scalars (α^r and its δ/τ derivatives, p, ∂p/∂T\|ρ, c_p, c_v, c_p⁰, τ²α⁰_ττ, s_r, α^r at (T_ref, ρ)), computed lazily once per state. Implementable by the HEOS backend and by IF97, so one IAPWS water implementation can serve both. Today IF97 delegates to `calc_Flash(iviscosity)` (`src/Backends/IF97/IF97Backend.h:651-667`), which calls the external `IF97` library (`IF97::viscliq_p` etc., `:315-322`), fetched at build time (`cmake/dependencies.cmake:63-64`). Inference, since the library is not in this checkout: it carries a second copy of the IAPWS 2008/2011 water transport. |
| U2 Transport data schema + strict loader | **P0** | `CoolPropFluid.h:116-383`, `FluidLibrary.h:459-1013` (~820) | Sum types: `ViscosityModel::{Staged{dilute, initial: Option, higher}, Ecs, RhoSrCs, Chung, Hardcoded(Id)}` and `ConductivityModel::{Staged{dilute, residual, critical}, Ecs, Hardcoded(Id)}`. Fixed-size arrays, equal-length checks, `deny_unknown_fields`, all list entries kept with `ModelId`s. |
| U3 Staged viscosity forms | **P0** | `TransportRoutines.cpp:9-180, 358-401`; dispatch `HelmholtzEOSMixtureBackend.cpp:714-808` (~310) | Pure `fn eval(&coeffs, &inputs) -> f64`. Covers 34 fluids (9 of them also need a hardcoded dilute or higher-order part from U10). |
| U4 Staged conductivity forms + O-S | **P0** | `TransportRoutines.cpp:677-777, 850-863`; dispatch `HelmholtzEOSMixtureBackend.cpp:922-1053` (~250) | O-S is shared by 52 models. η is passed in explicitly. |
| U5 Surface tension | **P0** | `Ancillaries.h:25-65`, `HelmholtzEOSMixtureBackend.cpp:703-713` (~60) | `fn(T)`, immutable, explicit domain. 108 fluids. A quick early win for the TDD harness. |
| U6 Transport API | **P0** | `HelmholtzEOSMixtureBackend.cpp:810-1069`, `AbstractState.cpp:793-813` (~150) | Joint `{η, λ, Pr}` evaluation so η is reused. Single-phase guard. Typed `Breakdown`. Typed errors (`NoModel`, `TwoPhase`, `Domain`). |
| U7 Water IAPWS 2008/2011 | **P1** | `TransportRoutines.cpp:182-304, 882-943` (~180) | One shared ξ/Y module. First hardcoded target, rich IAPWS tables. |
| U8 ECS + conformal solver + dependency DAG | **P1** | `TransportRoutines.cpp:1142-1373`, `HelmholtzEOSMixtureBackend.cpp:843-856, 939-952, 1070-1094`, `FluidLibrary.h:668-691` (~260) | References are `Arc<FluidData>` plus a pinned `ModelId`. The reference EOS is evaluated directly at (τ, δ) with no backend mutation. Covers 19 fluids. |
| U9 rhosr-CS + Chung | **P1** | `TransportRoutines.cpp:630-675, 1266-1292`, `FluidLibrary.h:693-710, 1005-1013` (~110) | `rhosr_critical` derived at load. The Chung LJ estimate is an explicit, named choice. Deriving it at load deviates from the oracle for R1234yf (+0.8–1.6 % liquid η, R4), so it is a recorded parity decision, not a silent change. |
| U10 Remaining hardcoded models | **P1/P2** | `TransportRoutines.cpp:235-251, 305-356, 403-629, 779-782, 830-848, 865-880, 945-1140` (~530) | Typed families where shapes repeat (xylene/cyclohexane, Shan R23, Friend Ω22, Huber-2016 CO2, R123 crit). Per-fluid fns otherwise. Each is verified against its paper table first and the oracle second. Helium and ammonia are oracle-only or defect-flagged. |
| U11 Expression DSL | **P2** | `master:src/expression/*` and headers (~1,240) | Optional feature crate keeping master's JSON and grammar contract (for upstream data and user-defined correlations, later material properties). Compile to a flat program and evaluate over batches. Bind `state_variables` to `TransportInputs` fields, not string keys. |
| U12 Post-v8 literature models | **P2** | master `dev/fluids` and IAPWS D2O 2020/2021 (+120 C++) | Add as selectable non-default `ModelId`s, verified against paper tables. Switch defaults deliberately. |
| U13 Batch/SIMD fast paths | **P2** | — | Side-by-side with the scalar reference implementation; cross-tested to a tolerance. |
| U14 Mixture transport | **defer** | `HelmholtzEOSMixtureBackend.cpp:816-823, 1060-1067` (~20) | If parity is needed, implement the crude rule behind an explicit `ApproximateMixingRule` type that is never silent. A real ECS-mixture model is a separate project. |
| U15 Dead branches, two-phase evaluation, first-entry-only lists, mutable σ buffer | **drop** | `TransportRoutines.cpp:784-828`, enum members, R1/R5/R13 | — |

**Order:** U1 → U2 → U5 (σ: proves the data-pack and harness) → U3 (dilute first, tested stage by stage via
`viscosity_contributions`) → U4 → U6 → U7 → U8 → U9 → U10 → U11–U13.

**TDD protocol:**
1. Pull coefficients from the wheel JSON.
2. Sample single-phase (T, ρ) grids plus Q = 0/1 points per fluid.
3. Compare stages, not just totals.
4. Gate on 1e-14 relative where the operation order is reproduced; FMA contraction makes bit-exactness unportable
   (master a849e8e1).
5. Maintain a "known oracle defects" list (R1, R2, R3, R10) where the port deliberately deviates, each with a
   literature citation. Candidates after their paper checks: R4 (R1234yf `rhosr_critical`) and R17 (ECS LJ parameters).

## 10. Open questions

1. **Oracle vs newer literature defaults.** Which wins: v8.0.0 defaults (TDD parity), or master's 19 changed and added
   viscosity and 14 conductivity models (HeavyWater IAPWS 2020/21, Ammonia 2018, Nitrogen 2024/25, R1234yf/ze
   Huber–Assael 2016, …)? The proposal is parity first, then newer models as selectable alternatives.
2. **ECS `q_D`.** Honour it, deviating from the oracle by up to about −16 % / +24 % near T_c for R13, R14, R142b, R218
   and RC318? This needs a check of Huber IECR 2003's per-fluid q_D table. The same paper check should settle R17
   (which LJ parameters the ECS λ models of R124, R22, R245fa and R32 were fitted with).
3. **Ammonia Tufeu 1984 divergence.** Is it a transcription error or a limit of the correlation? Should the port ship
   it at all, given master's default is Monogenidou 2018?
4. **Surface tension between T_c,σ and T_c,EOS** (18 fluids differ by > 1 mK, §3.6): for the 9 with T_c,σ < T_c,EOS
   (which throw today), return 0, extrapolate, or error? For the 11 with σ(T_c,EOS) > 0, clamp to 0 at the EOS critical point?
5. **Runtime user-defined transport.** Is `add_fluids_as_JSON`-style loading a requirement? That decides whether the
   DSL is P2 or deferred.
6. **Mixtures.** Replicate the crude rule for parity, or expose no mixture transport until an ECS-mixture model is
   designed?
7. **Helium.** Its constants are "magical"/REFPROP-derived and cannot be checked against papers. Accept the oracle as
   the only reference?
8. **rhosr-CS fluids' dilute term** (Chung LJ estimates; vapour about 10 % high for R1233zd(E) per 14da1f0d, not yet
   measured for the 7 v8.0.0 rhosr fluids): keep for parity, or replace?
9. **Exact-constant policy.** Replicate legacy literals (π = 3.141592654, CODATA-2006/2010 k_B and N_A) for bit-level
   parity, or modernise with a documented tolerance?
10. **Model selection API.** REFPROP's SETMOD-like per-handle selection, or per-fluid-data-pack configuration? The same
    choice interacts with ECS reference pinning.

## Verification log

- **Date:** 2026-10-04. Adversarial pass against `reference/CoolProp` at v8.0.0 (ae81610e), origin/master 022b63e4
  (151 commits ahead), and the CoolProp==8.0.0 wheel, whose `gitrevision` is the same commit (ae81610e).
- **Claims checked: ≈330.** Every path:line citation in §2–§9 was opened. Every count and size was re-run with
  independent scripts over `dev/fluids/*.json`, `CoolProp-Tests.cpp` and the master tree. Every oracle number was
  re-measured. All 10 cited master commits and the 5 master documents were read.
- **Confirmed as written (highlights):** 136/66/63/108/70; both §4 model tables (each sums to 66 and 63); the "#"
  columns of §3.1/§3.3; 48 routines (`TransportRoutines.h:8-286`); 29 hardcoded routines (27 used, 2 dead); 19 ECS
  fluids with 10/8/1 dependents; 54/63 viscosity coupling; 57 vectors; drift 9 + 10 viscosity and 5 + 9 conductivity;
  DSL 1,242 + 1,363 lines, 69 blocks in 19 fluids (28/40/1), 12 functions, 8-name allowlist; R1 and R3 oracle numbers
  exact; wheel JSON = tree for all 136 fluids (TRANSPORT and surface tension); every listed oracle hook exists; the
  O-S re-implementation is bit-exact.
- **Corrections made:**
  1. §8: conductivity check values are 141 active points across 34 fluids, plus 36 disabled (was 147/35/29). The
     ParaHydrogen block at `:400-406` is inside `/* */`. Range is `:347-581`; the σ test is at `:6604-6619`.
  2. §9 LOC: U3 ~310 (was ~370), U4 ~250 (was ~200), U10 ~530 (was ~800).
  3. §5: `sizeof(TransportPropertyData)` measured at 1,928 B (was "roughly 1.5 KB").
  4. §1/§5/R10: methane config-toggle shift is up to ~1e-7 (verifier max 1.06e-7 over 75 states; the 1.5e-7 had no state).
  5. R2: the q_D default is at `CoolPropFluid.h:188` (was :187). Added an oracle-level proof and state-resolved ranges.
     §1 and Q2 now say "would shift" (≈ −16/+24 %) instead of "off by".
  6. R4: `x_crossover` has no numerical effect at v8.0.0 (all 7 files store 2), so it is downgraded to a latent defect.
  7. R8: the R11 value 1.57e-5 had no state; replaced with a stated, reproduced example.
  8. R12/§3.8: xylene "copy-paste rot" **refuted as a data error**. The shared constants pass all 33 paper check values
     (< 6e-5), so it is DRY only. π citation corrected `:982` → `:983`; k_B/π effects quantified (≤ 1.2e-6).
  9. R13: regression cause refined (derived `N` and `s` were initialised before `n`, not the buffer alone). The
     18-fluid T_c,σ count is qualified (> 1 mK; 20 at exact inequality; 9 on the throwing side).
  10. §3.2: ECS step halving stops at 1/512 (10 tries), not 1/1024; ECS sources include Huber-RP912 (EthylBenzene);
      Chung's "polar correction" is Neufeld's sine term; Chung inputs come from its JSON block.
  11. §3.3: f_int ≈ 0.9–1.7e-3 at 250–400 K (was 1.0–1.4e-3). §3.4: O-S makes two uncached α^r-derivative calls, not one.
  12. §2: `eta0_and_poly` (:850-863) moved from the hardcoded range to the generic-forms range.
  13. §4: Chung LJ defaults are read at load time, not evaluation time. Added the Huber-2016 CO2 τ read. The "transport
      code" growth figure is `TransportRoutines.cpp` only.
  14. §7: term counts are 1–16 per stage (was 3–12); D2O-2007, R23 and methanol re-classified as branch-free closed forms.
  15. §9 U1: IF97's second IAPWS copy labelled as inference (the library is fetched at build time and is not in the checkout).
  16. §10 Q8: "vapour ~10 % high" is measured for R1233zd(E) only. Q4 split into the 9 throwing and 11 σ > 0 cases.
  17. §8: Balogun's p-xylene paper is 2015 per its BibTeX. R14 scope qualified (only where `p()` ≠ p_EOS). R15: the
      n-Decane check is disabled; one more disabled Benzene point (500 K, 32 kg/m³) is 0.106 % off.
- **Added by verifier (each with code and oracle evidence):** R17 (ECS λ of R124/R22/R245fa/R32 uses Chung LJ; −5.4 to
  −12.0 % dilute λ with the published-entry LJ); R4's R1234yf `rhosr_critical` stale at v8.0.0 (−0.68 %; +0.8–1.6 %
  liquid η); R9 O-S `T_reducing`/`p_reducing` unused; R11 `Arrr`/`Aaaa` unchecked; R7 conductivity throw site;
  §8 check-value provenance (REFPROP 9.1/10.0 and CoolProp v5 rows); §4 full unread-key inventory; §3.6 T_c,σ split.
- **Residual doubts:** R2 and R17 hinge on Huber IECR 2003 and McLinden IJR 2000 tables, which were not available
  here. R3 (transcription error vs inherent divergence of Tufeu's form) is unresolved. §3.5 timings are
  machine-dependent. "O-S dominates non-ECS λ cost" remains an inference. The header's "~2,950 lines" depends on
  which ranges are counted: 2,920 for the six listed, ≈2,950 with the factory and caching wrappers. The master commit
  that added conductivity-list support (first entry only) was not identified.
