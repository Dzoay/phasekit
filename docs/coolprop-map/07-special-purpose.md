# 07 IF97, incompressibles / brines, Bollengier water, ice, humid air and REFPROP - CoolProp v8.0.0 map

> Scope: `src/Backends/IF97/` (+ external header `IF97.h` from CoolProp/IF97 @ `7aaced02` = v2.2.1, MIT, fetched by CPM in `cmake/dependencies.cmake:62-67`), `src/Backends/Incompressible/`, `include/CoolProp/fluids/IncompressibleFluid.h`, shared `include/CoolProp/numerics/PolyMath.h` + `src/PolyMath.cpp`, `dev/incompressible_liquids/`, `src/Ice.cpp`, `include/CoolProp/fluids/Ice.h`, `src/HumidAirProp.cpp`, `include/CoolProp/HumidAirProp.h`, `src/Backends/REFPROP/`, the Bollengier backend (post-v8, origin/master only) and the tests for all of these. ~11.0k lines in-tree at v8.0.0: IF97 backend 1.0k, INCOMP 3.4k, humid air 2.9k, ice 0.16k, REFPROP 3.5k. On top of that: 4.9k lines of external `IF97.h`, 2.3k of shared PolyMath, 126 JSON fits (337 KB as shipped, 162 KB compact), and a 6.6k-line Python fitting pipeline with 0.7 MB of raw data (311 files). Post-v8, Bollengier adds 1.2k lines plus a 0.4k-line spline evaluator. Part of the coolprop-rs port plan; cites the v8.0.0 source.

Conventions:
- Plain `path:line` means the v8.0.0 checkout.
- `IF97.h:N` means CoolProp/IF97 @ 7aaced02.
- `master:` means origin/master (2026-10-03).
- "Oracle" means a probe run against the CoolProp==8.0.0 wheel for this map. Measured numbers are quoted inline; Python timings include ~0.2 µs of call overhead.
- *(inference)* marks a conclusion that the code or oracle does not state directly.

## 1. Purpose and concepts

| Subsystem | Model form | Native vars | Phases | Basis | In v8 oracle | Port |
|---|---|---|---|---|---|---|
| IF97 | Piecewise: Gibbs γ(π,τ) in R1/R2/R5, Helmholtz φ(δ,τ) in R3, psat(T) in R4, plus non-iterative backward equations | (T,p) | L, V, SC; two-phase via Q | mass | yes | P1-early |
| INCOMP | Independent fits of ρ(T,x), c(T,x), μ, λ, psat, T_freeze; h and s come from integrating c | (T,p,x) | liquid only | mass; x is a mass or volume fraction | yes | P1-early |
| Humid air | Virial (B,C) mixture of dry air and water, enhancement factor f, ideal parts from IAPWS-95 and Lemmon air, ice below 273.16 K | (T,p,ψ_w) | gas in equilibrium with liquid water or ice | per kg dry air or per kg humid air | yes (`HAPropsSI`) | P1-early |
| Ice Ih | Gibbs g(T,p) with complex-log terms; sublimation correlation | (T,p) | solid | mass | indirectly (`HAProps_Aux`) | P1-early |
| Bollengier | Gibbs G(P,T) as a tensor B-spline (order 6×6, 80×40 coefficients) | (T,p) | liquid, including metastable liquid | mass | **no** (post-v8) | P2-later |
| REFPROP | FFI to the NIST Fortran library | any | any | molar | needs the proprietary DLL | drop from core; dev oracle deferred |

- **IF97**: the fast industrial water/steam formulation. `Web/fluid_properties/IF97.rst:16` notes it was first written as fast water for humid air. It is piecewise in (T,p) with five regions; R4 is the saturation line. The backward equations return T(p,h), T(p,s), p(h,s) and the R3 v(T,p) without iteration. CoolProp also uses IF97 as a tabulation source (`SVDSBTL&IF97`, `src/Tests/CoolProp-Tests-SVDSBTL.cpp:666`).
- **INCOMP**: secondary heat-transfer fluids.
  - 74 pure fluids: oils, HTFs, molten salts, liquid metals.
  - 52 solutions: glycols, alcohols, salts, LiBr, seawater and 3 ice slurries.
  - Every property is a 2D fit in (T−Tbase, x−xbase). There is no vapour phase: psat serves only as a validity floor and as the Q=0 input.
- **Humid air**: the ASHRAE RP-1485 real moist-air model (Herrmann, Kretzschmar & Gatley 2009, HVAC&R Res. 15(5):961-986; `Web/fluid_properties/HumidAir.rst:10-12`). It is a separate string API (`HAPropsSI`), not an `AbstractState`. Valid range: 10 Pa to 10 MPa, −143.15 to 350 °C, W ≤ 10 (`src/HumidAirProp.cpp:120-157`).
- **Ice Ih**: used only by humid air below 273.16 K. It supplies the sublimation pressure, ice h for the wet bulb, and ice v and κ_T for the enhancement factor.
- **Bollengier** (master only, `6d788a7a` #3426): liquid water at 240-500 K and 0-2300 MPa, including the metastable liquid inside ice fields where IAPWS-95 refuses. PT inputs only.
- **REFPROP**: a thin wrapper around NIST REFPROP. It contains no model of its own.

**Concepts these subsystems force on the future material / all-states-of-matter design**

| Observation | Evidence | Abstraction it implies |
|---|---|---|
| One substance has many models. Water is IAPWS-95 (HEOS), IF97, Bollengier, `INCOMP::Water` and ice Ih. One `HAPropsSI` call can touch IAPWS-95, IF97 and ice at once | `HumidAirProp.cpp:51-53, 838-840, 859-861, 1556` | Separate **Substance** (identity, M) from **Model** (formulation, phases covered, domain). Key the registry by (substance, model) |
| Gibbs-explicit models re-derive the same property map 3 times | `IF97.h:235-261`, `src/Ice.cpp:118-143`, master `BollengierBackend.h:322-332` | `GibbsPotential` → derivative bundle (g, g_T, g_p, g_TT, g_Tp, g_pp) → one shared property map, alongside the Helmholtz one |
| A liquid solution with a liquidus | INCOMP `Tfreeze(p,x)` (`IncompressibleFluid.cpp:216-234`) | Solution model with a typed composition basis plus a solid-liquid boundary |
| A solid+liquid mixture faked as a fluid. In the ice slurries, x is the *ice mass fraction* and latent heat is folded into c | oracle: IceEA cp = 81 kJ/kg/K at 255 K, x=0.2; master `DATA_AUDIT.md` (Ice* row) | A future `PhaseAssemblage` (solution + ice Gibbs) instead of a pseudo-fluid |
| A gas mixture in equilibrium with a condensed phase. The enhancement factor is the equality of water chemical potentials, including Henry's law and the condensed-phase compressibility | `HumidAirProp.cpp:848-951` | An equilibrium constraint between heterogeneous models (gas-mixture virial vs pure condensed Gibbs) |
| Validity domains with holes and seams | IF97 region map `IF97.h:3887-3927`; Bollengier excluded box master `BollengierBackend.h:144-145, 301-309`; INCOMP TminPsat / T_freeze | `Domain` as first-class data, with typed out-of-domain errors |
| The literature already has a consistent multiphase framework for humid air + ice + seawater | IAPWS G8-10 (humid-air Helmholtz function with cross-virials, check-value Tables 13-15), R10-06 ice, IAPWS-08 seawater | The target abstraction for the "all states" phase: per-phase potentials plus chemical-potential equilibrium. Defer, but leave room in the traits |

## 2. Structure (key types/functions -> path:line)

### 2.1 IF97
| Item | Location | Notes |
|---|---|---|
| `IF97Backend : AbstractState` (header-only) | `src/Backends/IF97/IF97Backend.h:13`; the `.cpp` is 4 lines | Mass-based. Extra CachedElements `_hmass,_rhomass,_smass,_reverse` (`:19-20`) |
| `clear()` | `:60-82` | Resets the whole cache registry. This fixes a stale-`speed_sound` bug, per the comment |
| `set_phase()` with ε = 3.3e-5 psat band | `:85-156` | Forward branch uses `psat97`. Reverse branch uses `BackwardRegion` + `Tsat97` (`:110-155`) |
| `update()` | `:167-282` | PT, PQ, QT, HmolarP/HmassP, PSmolar/PSmass, HmolarSmolar/HmassSmass via `case` fall-through |
| `calc_SatLiquid/SatVapor/calc_Flash` | `:292-448` | Two-phase blend with v linear in Q (`:381`). cp, cv, w, μ, λ and Pr throw (`:383-403`) |
| Reverse h(p,s), s(p,h) | `:468-491` | `IF97::hmass_psmass` / `smass_phmass` (= backward T, then forward; `IF97.h:4186-4199`) |
| `specify_phase` plumbing | `:563-568` | Added for SBTL. `PropsSI` still refuses phase strings for IF97 (`src/CoolProp.cpp:541-542`) |
| `fast_evaluate` (batch) | `:677-1010` | PT/HmassP/HmolarP only; per-row status. Ends with `clear()` (`:1009`) |
| Factory | `src/AbstractState.cpp:84-108` | Accepts only "Water"/"H2O" |
| `IF97::BaseRegion` | `IF97.h:203-485` | Coefficient vectors are copied per instance (`:206-233`). Property map `:235-261`. Each γ derivative is its own loop (`:337-418`). μ (R12-08) at `:262-268, 420-435`; λ (R15-11) at `:269-274, 436-478` |
| Regions 1 / 2 / 5 | `IF97.h:491-564`, `:570-644`, `:2721-2758` | B23 boundary `:646-665` |
| Region 3 φ(δ,τ) | `IF97.h:2186-2587` | Has its own copy of the transport code (`:2254-2271, 2344-2403, 2465-2504`) |
| Region 3 v(T,p) backward (SR5-05) | `IF97.h:672-2181` | 26 sub-regions `Region3a..z` (`:1647-1810`), dispatched by `char` in `Region3_v_TP` (`:1812-1870`); 12 dividing lines (`:1984-2066`) |
| Region 4 | `IF97.h:2592-2716` | `p_T`, `T_p`, `sigma_t` |
| Backward T(p,h), T(p,s), p(h,s), T(h,s) | `IF97.h:2765-3876`; dispatch `RegionOutputBackward` `:4041-4162`; HS `:4388-4539` | Clips the result to the correct side of Tsat (`:4058-4084`) |
| Region selection | `RegionDetermination_TP` `IF97.h:3887-3927`; `RegionOutput` `:3929-3962`; `RegionDetermination_pX` `:3965-4018`; `BackwardRegion` `:4021-4039` | Region objects are function-local `static const` |
| Public API | `IF97.h:4546-4760` | `rhomass_Tp`, `hmass_Tp`, `T_phmass`, `psat97`, `Tsat97`, `sigma97`, ... |
| Check data | `IF97.h:4792-4907` (`ENABLE_CATCH`) | SR5-05 Tables 5/11 and dividing-line Tables 3/11. They print mismatches but assert nothing |

### 2.2 INCOMP (incompressible liquids and solutions)
| Item | Location | Notes |
|---|---|---|
| `IncompressibleData {type enum, Eigen::MatrixXd coeffs}` | `include/CoolProp/fluids/IncompressibleFluid.h:29-47` | 5 fit types + NOT_SET |
| `IncompressibleFluid` | `IncompressibleFluid.h:59-431` | Name, limits, Tbase/xbase, 9 `IncompressibleData` members, `Polynomial2DFrac poly` (`:135`). Setters are public (`:186-249`). Always-throwing stubs: `cp/cv/s/u/h` (`:271-292`), `T_s..x_Tfreeze` (`:347-373`), `h_u/u_h` (`:384-395`) |
| Evaluators | `src/Backends/Incompressible/IncompressibleFluid.cpp:111-234` | Switch on fit type per property. Exponential forms ignore x (`:115-118, 151-154`) |
| Derivative/integral | `IncompressibleFluid.cpp:241-282` | Polynomial type only |
| Composition conversion | `IncompressibleFluid.cpp:287-395` | Everything except identity throws NotImplemented |
| Direct inverses `T_rho`, `T_c` | `IncompressibleFluid.cpp:403-429` | Brent on the polynomial |
| `checkT/checkP/checkX` | `IncompressibleFluid.cpp:441-490` | T_freeze, psat and x bounds |
| `IncompressibleBackend` | `IncompressibleBackend.h:14-308`, `.cpp:25-567` | Raw `IncompressibleFluid* fluid` pointing into the global library (`.h:30`) |
| `update` | `.cpp:59-152` | PT, DmassP, PSmass, HmassP, QT (Q=0 only) |
| Reference state | `.cpp:176-210` | `set_fractions` re-pins the reference at the new x (`:202-209`) |
| h, s, u | `.cpp:509-527` | `raw_calc_hmass/smass` + reference offsets |
| HmassP/PSmass flash | `.cpp:405-461` | Brent on [Tmin,Tmax] with `maxiter = 10` (`:426`, `:457`) |
| Partial derivatives | `.cpp:530-554` | Lookup table of 17 cases |
| Library | `IncompressibleLibrary.cpp:348-404` (parse), `:426-491` (`add_one`), `:540-583` (global + `call_once`) | |
| 2D centred-polynomial engine | `include/CoolProp/numerics/PolyMath.h:220-438`, `src/PolyMath.cpp:363-791` | `deriveCoeffs` `:363`; `evaluate` `:422-526`; `derivative` `:528-565`; `integral` `:567-637`; `fracIntCentral(Dvector)` `:757-791` |
| Fluid-string parsing (`MEG-30%`, `MEG[0.3]`) | `src/CoolProp.cpp:136-235` | The basis is implied by the fluid's `xid` |

### 2.3 Humid air (`namespace HumidAir`)
| Item | Location |
|---|---|
| Per-thread HEOS Water, HEOS Air and IF97 backends | `HumidAirProp.cpp:51-53`, created lazily in `check_fluid_instantiation` `:96-108` |
| Bounds | `check_bounds` `:120-157` |
| Virials from the EOS (δ = 1e-12 limit) + per-T cache | `:175-206` |
| α⁰ cache + reference offsets | `:208-329` |
| Cross virials B_aw, C_aaw, C_aww and their T-derivatives | `:654-725` |
| Mixture virials B_m, C_m | `:727-799` |
| Henry constant, k_T, enhancement factor f | `:804-951` |
| Transport (Tsilingiris) | `Viscosity :971-991`, `Conductivity :992-1015` |
| Molar volume, ideal-gas parts, h, u, s | `:1022-1261` |
| Dew point, wet bulb (solver classes) | `:1263-1481` |
| Name aliases | `Name2Type :1482-1536` |
| ψ_w from W, RH or Tdp; RH | `:1546-1608` |
| Input resolution, including the Brent/secant inverses | `_HAPropsSI_inputs :1760-2013`, `Brent_HAProps_W/T :384-545` |
| Outputs, including finite-difference cp, cv, w | `_HAPropsSI_outputs :2014-2135` |
| Entry points | `HAPropsSI :2136-2267`, `HAProps_Aux :2269-2423`, `cair_sat :2424`, `IceProps :2433`, legacy `HAProps :1678-1689` (still C-exported: `CoolPropLib.h:299`) |
| Tests | `:2447-2846`; `src/Tests/CoolProp-Tests.cpp:851-1465, 5439-5676` |

### 2.4 Ice Ih
- `src/Ice.cpp:1-143` defines free functions in the global namespace: `psub_Ice`, `g_Ice`, `dg_dp_Ice`, `dg2_dp2_Ice`, `dg_dT_Ice`, `h_Ice`, `s_Ice`, `rho_Ice`, `IsothermCompress_Ice`.
- They are declared in `include/CoolProp/fluids/Ice.h:4-12`. `include/Ice.h` is a deprecated shim.
- The only caller is humid air.

### 2.5 Bollengier (master only)
- `master:src/Backends/Bollengier/BollengierBackend.h` (535 lines):
  - immutable shared spline `surface()` `:76-82`
  - `domain()` `:176-200`
  - `update()` `:251-448`
- The coefficients (`kOrderP=kOrderT=6`, `kNP=80`, `kNT=40`, 621 lines) are generated into `BollengierWaterCoefficients.h` by `dev/scripts/extract_bollengier_coefficients.py`, which pins the sha256 of the paywalled supplement.
- Evaluator: `master:include/CoolProp/spline/TensorBSpline2D.h` (`8a5e34fe` #3400): `eval(x,y,dx,dy)` `:55`; `find_span/basis_funs/basis_ders` `:78-93, 219-288`.
- Tests: `master:src/Tests/CoolProp-Tests-Bollengier.cpp` (484 lines) and `-TensorBSpline.cpp` (419 lines).

### 2.6 REFPROP
- **Classes**: `REFPROPBackend` (pure) derives from `REFPROPMixtureBackend : AbstractState` (`REFPROPMixtureBackend.h:31`). The generator is at `REFPROPMixtureBackend.cpp:179-200`.
- **Library discovery and loading**: `:234-300`.
  - Searched in order: env `COOLPROP_REFPROP_ROOT`, then config `ALTERNATIVE_REFPROP_LIBRARY_PATH` / `ALTERNATIVE_REFPROP_PATH`, then the default `/opt/refprop` (`:77-83`).
- **Fluid (re)loading**: `set_REFPROP_fluids :327+`.
  - Alias resolution via `REFPROP_RESOLVE_COOLPROP_ALIASES` (`:354-388`) uses `INFO.REFPROP_NAME`, read at `FluidLibrary.cpp:177-180`. That name is present in 127 of the 136 `dev/fluids/*.json` (9 are "N/A"; see P4).
- **Units**: table at `:2-20`.
- **Headers**: come from `CoolProp/REFPROP-headers @ b4faab1b` (`cmake/dependencies.cmake:69-74`).

**Coupling**
- Humid air depends on HEOS Water and Air (another area), IF97, Ice, and even `CoolProp::PropsSI` (`HumidAirProp.cpp:1884`).
- INCOMP depends on PolyMath, `Solvers` (Brent), nlohmann JSON and PropsSI string parsing.
- IF97Backend depends on `IF97.h`. It is consumed by humid air and SVDSBTL (`src/SBTL/SurfacePresets.cpp`).
- REFPROP depends on a dynamic library and the FluidLibrary name map.

## 3. Algorithms and formulas (cite the papers CoolProp cites)

### 3.1 IF97: IAPWS R7-97(2012) + supplementary releases
- **Gibbs regions**: γ = γ° + γʳ.
  - Region 1: γ = Σ34 nᵢ(7.1−π)^Iᵢ(τ−1.222)^Jᵢ. CoolProp stores nᵢ·(−1)^Iᵢ in order to use (π−7.1) (`IF97.h:491-492`).
  - Region 2: 9 ideal + 43 residual terms. Region 5: 6 + 6 terms.
  - Property map: v = (RT/p)·π·γ_π, h = RTτγ_τ, s = R(τγ_τ − γ), cp = −Rτ²γ_ττ, plus cv and w from mixed partials (`IF97.h:235-261`). Region 1 overrides cv and w (`:536-548`).
  - Each derivative is a separate pass over the terms using `powi` (`:337-418`). `speed_sound` re-evaluates `dgammar_dPI` 3 times (`:259`).
- **Region 3**: Helmholtz φ = n₁lnδ + Σ_{i=2..40} nᵢδ^Iᵢτ^Jᵢ (`:2274-2281`).
  - ρ(T,p) comes from the SR5-05(2016) backward v(T,p) and is used **as is**: `REGION3_ITERATE` is defined nowhere in CoolProp (`IF97.h:2562-2569`). This is the IF97 library's documented default (CoolProp/IF97 `README.md:54`: "error on the order of 1E-6, but about 2.6 times faster"); the library's own check program defines the flag (`IF97.cpp:10`), so its printed tables are not what CoolProp computes.
  - Oracle against R7-97 Table 33: ρ error −4.15e-6 at (650 K, 25.58 MPa), +1.1e-7 at (650 K, 22.29 MPa), −1.4e-6 at (750 K, 78.31 MPa).
- **Region 4**: the quadratic saturation equation (`IF97.h:2622-2702`).
- **Range constants** (`IF97.h:76-111`): T 273.15-1073.15 K and p 611.213 Pa-100 MPa; R5 up to 2273.15 K at p ≤ 50 MPa. R7-97 itself states R2 and R5 down to 0 < p.
- **Backward equations**:
  - T(p,h) and T(p,s) for R1 and R2a/2b/2c come from R7-97.
  - R3a/3b T(p,h), T(p,s) come from SR3-03(2014).
  - p(h,s) for R1, R2 and R3, and T(h,s) on the R4 boundary, come from the 2014 supplementary releases (`IF97.h:3600-3625`; classes `:3654-3790`). R5 has no backward equation (`:4158`).
  - The result is clipped 1 µK to the correct side of Tsat (`IF97.h:4058-4084`; the comment cites ±25 mK uncertainty).
  - There is no forward refinement. Oracle: after `HmassP`, `hmass()` − h_in = −2.54 J/kg at 500 K / 1 MPa. `smass()` goes through `Y_pX`, but away from saturation it equals the forward s at the same backward T (oracle difference 0.0), so h and s are consistent with each other and with `T()`; only h ≠ h_in.
- **Transport**:
  - Viscosity: IAPWS R12-08 with μ̄₂ = 1, the industrial form without critical enhancement (`IF97.h:262-268`).
  - Conductivity: IAPWS R15-11, with the critical term λ̄₂ built from IF97 cp, cv and (∂ρ/∂p)_T plus the ζ(T_R) piecewise table (`IF97.h:189-196, 281-296, 452-478`).
  - Surface tension: IAPWS R1-76(2014) (`IF97.h:2703-2715`).
- **Measured cost**: `fast_evaluate` takes ~750 ns/point for (ρ,h). A PT update plus h takes 1.19 µs, against 19.6 µs for HEOS.

### 3.2 INCOMP
- **Fit forms** (`IncompressibleFluid.cpp:111-234`; `Web/fluid_properties/Incompressibles.rst:329-336`, whose prose says "four" forms while the code has five):
  - polynomial: Σᵢⱼ C_ij (T−Tbase)ⁱ (x−xbase)ʲ. Row index = T power, column index = x power; shipped fits are at most 4×6 (`PolyMath.cpp:469-526`).
  - exppolynomial: exp(polynomial).
  - exponential: exp(C₀/(T+C₁) − C₂). The pole is linearised in a ±2.2e-14 band (`IncompressibleFluid.cpp:38-61`).
  - logexponential: exp(C₁·ln(1/(T+C₀) + 1/(T+C₀)²) + C₂) (`:64-87`).
  - polyoffset: Σ C_{i+1}(y − C₀)ⁱ, where a row vector means a function of the second argument and a column vector a function of the first (`:89-108`).
- **Caloric relations** (`IncompressibleBackend.cpp:509-527, 560-567`):
  - h = ∫c dT + p·ρ⁻¹(1 + Tρ⁻¹ ∂ρ/∂T)
  - s = ∫(c/T) dT + p·ρ⁻² ∂ρ/∂T
  - u = h − p/ρ
  - cp = cv = c (`IncompressibleBackend.h:217-222`)
  - ∫c/T dT uses a binomial expansion of the centred polynomial (`PolyMath.cpp:757-791`; `Incompressibles.rst:315-323`).
- **Reference state**: h = s = 0 at 293.15 K and 101325 Pa, **at the current x** (`Incompressibles.rst:38-45`; re-pinned at `IncompressibleBackend.cpp:208`). As a result there is no enthalpy of mixing (master `NOTES_mixing_models.md`; issues #533, #781, #1690).
- **Inverses**: Brent on [Tmin,Tmax].
  - Termination: a T-bracket below 2ε|T| + 2.2e-13 K, |f| < 2.2e-16 J/kg, or `maxiter = 10` (`IncompressibleBackend.cpp:424-427`; `Solvers.cpp:522-640`).
  - Cost: HP flash 13.1 µs, against h(T) 1.97 µs and ρ(T) 0.67 µs (MEG, oracle; a rerun by the verifier gave 15.2 µs and 1.92 µs).
- **Data sources** (JSON `reference` field, 126 fluids):
  - SecCool/Skovrup: 34 alone, ~57 including co-sources.
  - Melinder (2010): 22 alone, plus 11 jointly with Skovrup.
  - ASHRAE 7; vendor sheets: Paratherm 7, Dynalene 6, Dow, Therminol, Hydro2000 and others.
  - Pátek & Klomfar 2006 (LiBr); Sharqawy 2010 (MITSW seawater).
  - 5 fits sampled from CoolProp's own EOS fluids (Air, Water, Ethanol, Hexane, Acetone).

### 3.3 Humid air (RP-1485; Herrmann et al. 2009)
- **Mixture virials**: B_m = (1−ψ)²B_aa + 2ψ(1−ψ)B_aw + ψ²B_ww; C_m is the cubic analogue (`:727-799`).
  - B_aa, C_aaa, B_ww and C_www are taken as lim αʳ/δ of the HEOS air and water EOS at δ = 1e-12 (`:189-198`). If `FlagUseVirialCorrelations` is set, 7th-order polynomials in T are used instead.
  - The cross terms are hard-coded correlations (`:654-725`). Their coefficients match IAPWS G8-10 Table 4, which attributes B_AW to Harvey & Huang (2007) and C_AAW and C_AWW to Hyland & Wexler (1983). The exceptions are in C_aww: d₂ = 0.347804e4, against 0.34780200×10²·100 = 3478.02 in G8-10 (`:702, 714`), and d₁ = −0.1072887e2, a truncation of G8-10's −0.10728876×10² (verifier checked against the G8-10 PDF, Table 4).
- **Volume and caloric properties**:
  - v̄ is found by a hand-rolled secant on p = RT/v̄·(1 + B_m/v̄ + C_m/v̄²) (`:1022-1070`).
  - h̄ = (1−ψ)h̄ₐ° + ψh̄_w° + RT[(B−TB′)/v̄ + (C−TC′/2)/v̄²] (`:1121-1149`).
  - s̄ is analogous, plus ideal mixing −R[(1−ψ)ln(1−ψ) + ψlnψ]. A third secant finds v̄ of pure dry air for the air ideal-gas term (`:1187-1247`).
  - Ideal-gas parts come from the α⁰ of IAPWS-95 water and Lemmon (2000) air at δ = 1. Hard-coded offsets and reference states reproduce the RP-1485 tables (`:267-329, 1077-1112`).
- **Enhancement factor f**: a secant on RP-1485 Eq. 3.25 (`:848-951`).
  - Inputs: liquid k_T from IAPWS-95 at the IF97 density, or ice k_T; Henry's constant (`:804-815`); the virials.
  - The Henry's-constant form and its N₂/O₂/Ar coefficients follow IAPWS G7-04 *(inference)*.
  - The result is clamped to f ≥ 1 (`:947-950`).
- **Saturation pressure p_ws**: IF97 `psat97` above 273.16 K and ice sublimation below, used in ψ_w, RH and dew point (`:1546-1608, 1263-1331`). Inside `f_factor` it is IAPWS-95 instead (`:859-861`).
- **Dew point**: secant on p_w = f·p_ws(T_dp) (`:1263-1331`).
- **Wet bulb**: an adiabatic-saturation energy balance with liquid h_w from IAPWS-95 at the IF97 density, or ice h. Solved by Brent with a bracket clamp (`:1333-1481`; #2255, #2690). The triple-point gap is detected and reported (#2906, `:1944-2011`).
- **Inverse inputs**: secant/Brent around the whole forward model (`:384-545, 1760-2013`).
- **Transport**: Wilke-type mixing after Tsilingiris.
  - The comment cites "2009 … 49, 1098-1010" (`:975, 996`). The paper is Energy Convers. Manag. 49 (2008) 1098-1110 *(inference; verify)*.
  - Dry air is evaluated at (T,p). Water vapour is evaluated as **saturated vapour at total p** (`:983-990`).
- **cp, cv**: central finite differences with dT = 1e-3. w and the isentropic exponent use finite differences of p(v̄) (`:2079-2126`).
- **Measured cost** per call: H(T,R) 7.5 µs; W(T,R) 6.8 µs; Tdp 132 µs; Twb 152 µs; T(H,R) 203 µs.

### 3.4 Ice Ih (`src/Ice.cpp`)
- **Gibbs function** (IAPWS R10-06, `:51-67`): g = g₀(p) − s₀T_tθ + T_t·Re Σ_{k=1,2} r_k[(t_k−θ)ln(t_k−θ) + (t_k+θ)ln(t_k+θ) − 2t_k ln t_k − θ²/t_k].
  - g₀ is quartic and r₂ quadratic in (π−π₀).
  - s₀ = −3327.33756492168 J/kg/K, the "IAPWS-95" choice of s₀.
  - Implemented: g, g_p, g_pp, g_T, h, s, ρ, κ_T. **Missing**: g_TT (cp), g_Tp (α), w.
- **Sublimation**: IAPWS R14-08(2011), ln(p/p_t) = θ⁻¹Σaᵢθ^bᵢ (`:36-49`). Oracle at 230 K: 8.947352740 Pa against the check value 8.94735 Pa.

### 3.5 Bollengier (master)
- **Source**: Bollengier, Brown & Shaw, J. Chem. Phys. 151, 054501 (2019), doi:10.1063/1.5097179.
- **Property map**: v = G_P·1e-6, s = −G_T, h = G + Ts, cp = −TG_TT, cv = cp + TG_PT²/G_PP, w² = −v²/(∂v/∂P)_s (`BollengierBackend.h:322-332`).
- **Spline evaluation**: Cox–de Boor in the Piegl & Tiller form (A2.2/A2.3). Six separate `eval` calls per state (`:313-318`).
- **Domain**: [240,500] K × [0,2300.6] MPa, minus the box {p ≥ 1500 MPa ∧ T ≤ 255 K} (`:114-145, 301-309`). An admissibility backstop rejects G_PP ≥ 0 or cv ≤ 0 (`:428-436`).
- **No reference-state shift**: at the triple point h = 71.23 J/kg and s = 0.258 J/kg/K (`:84-108`).

### 3.6 REFPROP
FFI only: SETUPdll, flash routines, PHIXdll and friends. REFPROP units are kPa, mol/L, J/mol and µPa·s (`REFPROPMixtureBackend.cpp:2-20`). The REFPROP version is decoded from SETUP's `ierr` (`:302-325`).

## 4. Data and configuration inputs

| Input | Location | Size / format | Notes |
|---|---|---|---|
| IF97 coefficients | `IF97.h`: namespace-scope `static const` arrays and `std::vector`s (`:198-201, 528, 627-628, 654, 2229, 2608, 2737-2738`) | ~600 rows | Header-only. Each including TU gets its own copy: 4 TUs (`AbstractState.cpp`, `IF97Backend.cpp`, `HumidAirProp.cpp`, `SBTL/SurfacePresets.cpp`) |
| INCOMP fits | `dev/incompressible_liquids/json/*.json` (337 KB) → `dev/all_incompressibles.json` (generated, not in the checkout) → string header `all_incompressibles_JSON.h` (`dev/generate_headers.py:60, 422-450`) | 126 fluids: 74 pure, 39 mass-based, 13 volume-based | Keys: name, description, reference, Tmin, Tmax, TminPsat, Tbase, xbase, xmin, xmax, xid, and 9 property blocks `{type, coeffs, NRMS}` |
| INCOMP fit-type census (measured) | json | ρ, c, λ: polynomial 126/126. μ: exppoly 68, exp 46, logexp 3, poly 2, none 7. psat: none 86, exp 20, exppoly 17, logexp 3. T_freeze: none 77, exppoly 29, poly 15, exp 4, polyoffset 1. Density shapes: 4×1 (52), 4×6 (45), 2×1 (21), ... | `mass2input` is present for all 13 volume fluids and `volume2input` for 16 mass fluids. Both are unused (I6). No solution uses an exponential-type μ, which matters because those forms ignore x (I15) |
| INCOMP pipeline + raw data | `dev/incompressible_liquids/CPIncomp/` (6.6k lines of Python) + `data/` (0.7 MB, 311 files) | numpy/scipy | Offline generator. Master adds data tests and `DATA_AUDIT.md` |
| Master-only INCOMP schema | `density_cheb`/`specific_heat_cheb` `{type:"chebyshev"}` with load-time d/dT, ∫c, ∫c/T | all 127 master JSON files (master adds one fluid) | master `IncompressibleLibrary.cpp:470-489` prefers them (`e8fff569`). **Not in the v8 oracle** |
| Fluid string | `src/CoolProp.cpp:136-235` | `INCOMP::AEG-30%`, `INCOMP::MEG[0.3]` | "30%" means volume % for AEG but mass % for MEG (oracle: AEG-30% equals AEG[0.3] as a volume fraction) |
| Humid-air constants | `HumidAirProp.cpp` literals | cross virials, reference offsets, ε = 0.621945, M_a = 0.028966, Air T_red 132.6312 | Four values of R appear (H4) |
| Ice constants | `src/Ice.cpp:6-26` | 6 complex + 9 real (T_t, p_t, p_0, g00-g04, s0) | g00 is the 2006 value (C1) |
| Bollengier coefficients | `master:.../BollengierWaterCoefficients.h` | 80×40 coefficients + 2 knot vectors (~25 KB binary) | Extracted from a paywalled supplement (sha256 pinned in the generator) and shipped under MIT |
| REFPROP | user DLL + FLUIDS/MIXTURES directories | proprietary | |
| Config / env | `DONT_CHECK_PROPERTY_LIMITS` (`HumidAirProp.cpp:122`); `ALTERNATIVE_REFPROP_PATH`, `..._LIBRARY_PATH`, `..._HMX_BNC_PATH`, `REFPROP_USE_GERG`, `REFPROP_USE_PENGROBINSON`, `REFPROP_RESOLVE_COOLPROP_ALIASES`; env `COOLPROP_REFPROP_ROOT`; humid-air toggles `UseVirialCorrelations` / `UseIsothermCompressCorrelation` / `UseIdealGasEnthalpyCorrelations` (`:363-383`) | process-global | Each of these changes model results for every thread |

## 5. State, caching, globals, thread-safety, memory

| Subsystem | State / caches | Globals | Thread-safety | Memory / loading |
|---|---|---|---|---|
| IF97 | Per-instance AbstractState + 4 CachedElements. `imposed_phase_index` is never initialised (`AbstractState.h:83`; ctor `:724-730` does not set it; IF97 has no ctor) | Function-local `static const` region objects (C++11-safe initialisation). Namespace `static const std::vector` in a header means one copy per TU | The `IF97::` library is pure. Each backend must be used by one thread only. The batch API mutates the instance (R5) | Tiny |
| INCOMP | Backend: 19 CachedElements (`IncompressibleBackend.h:23-28`, 7 of them reference values) + `_fractions`. Fluid objects hold no mutable state; `Polynomial2DFrac` has no data members (`PolyMath.h:220-401`) | `static JSONIncompressibleLibrary library` (`IncompressibleLibrary.cpp:540`), filled under `std::call_once` (`:546-550`) | Read-only after load in v8, but mutable refs escape: `get_incompressible_fluid` returns `IncompressibleFluid&` and the setters are public. Master adds **unlocked** runtime `add_fluids_as_JSON("INCOMP")` (master `src/CoolProp.cpp:716-724`) *(inference: data race with concurrent readers)* | The first INCOMP use parses all 126 fluids. A 2D evaluate of a 4×6 fit makes ~13 Eigen heap allocations (`PolyMath.cpp:439-466, 490-518`) *(inference from code)* |
| Humid air | `thread_local` HEOS Water, HEOS Air and IF97 backends (`:51-53`). `thread_local` per-T virial and α⁰ caches and reference offsets (`:175-265`). α⁰ evaluation mutates the EOS copy (`E.alpha0.set_Tred`, `:233-236`) | `std::atomic<int>` model flags (`:114-116`). One shared, mutex-guarded error slot where the last writer wins (`src/CoolProp.cpp:83-106`) | Tested with 16 threads (`:2667-2712`) | 3 backends are built per thread on first call (benchmark `:2717-2738`). HEOS water and air data are loaded globally |
| Ice | none | `static` doubles and complexes, never mutated | pure | trivial |
| Bollengier (master) | Per-instance cached properties | Function-local static immutable spline and Domain | safe | 25 KB static |
| REFPROP | Fortran COMMON state per process. Global `LoadedREFPROPRef` and `endings[]` (`REFPROPMixtureBackend.cpp:67, 72`). Static `instance_counter` / `_REFPROP_supported` (`:232-233`, `.h:40-41`) | DLL handle (REFPROP_lib.h) | **None in v8.** Two threads on different fluids abort the process; master `8214f28e` adds a process-wide recursive mutex | The DLL unloads when `instance_counter` reaches 0 (`:220-227`), which never happens once a pure instance has existed (P2). SETUP re-runs whenever interleaved instances use different fluids (`:228-230, 331-332`) |

## 6. Rot and bugs (table: issue | evidence | impact | Rust remedy)

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| R1 | IF97 `HmolarSmolar` falls through into `HmassSmass`, which overwrites the converted values with the raw molar inputs | `IF97Backend.h:257-264`. Oracle: (h,s) molar at 500 K / 1 MPa → T = 281.36 K, p = 18.08 MPa. Still on master (`IF97Backend.h:279-287`) | Silently wrong state | A separate input-normalisation layer; typed `MolarEnthalpy`/`MassEnthalpy`; exhaustive `match` with no fall-through |
| R2 | `imposed_phase_index` is never initialised. `new IF97Backend()` value-initialises it to 0 = `iphase_liquid`, so the 3.3e-5 band throw at `:189-196` never fires. A stack-constructed instance reads an indeterminate value, which is UB *(inference)* | `AbstractState.h:83, 724-730`; `DataStructures.h:183-194`. The SBTL code knows (`SurfacePresets.cpp:581-583`). Oracle at 373.15 K, p = 0.99998·psat: phase = liquid with ρ = 0.598 (vapour). `fast_evaluate` on the same points returns status 2 (`IF97Backend.h:822-829`) | Wrong phase labels; the scalar and batch paths disagree | Constructors make every field explicit; `Option<Phase>` |
| R3 | IF97 HmassP/PSmass throws for every **Region-3** state above p_crit, because the reverse `set_phase` (case 3) calls `Tsat97(p)`, which throws for p > 22.064 MPa (`IF97.h:2651-2652`). Region-1/2 states above p_crit are unaffected (verifier oracle: (30 MPa, 1.0 MJ/kg) → 503.7 K, (30 MPa, 3.5 MJ/kg) → 890.1 K). The SBTL builder misdiagnosed this as "no backward equation is wired up" and added a TOMS748 workaround | `IF97Backend.h:132-133`; `SurfacePresets.cpp:537-559`. Oracle: (30 MPa, 1.9 MJ/kg), (50, 2.0) and (100, 2.1) all give "Pressure out of range". `fast_evaluate` on the same points returns T = 658.89, 690.5718338 and 733.6163014 K, matching the SR3-03 values printed by `IF97.cpp:275`. Still on master (`:143`) | The supercritical backward domain is unusable in scalar calls | Phase classification as a pure function of (region, T, p) with an explicit supercritical branch; property tests across every region |
| R4 | IF97 accepted NaN inputs | Oracle: HP with p = NaN → T = 273.15 K, gas; HS with h = NaN → 273.15 K, two-phase; PT with p = NaN → "liquid". Fixed post-v8 in `4370e448` (#3421) | Garbage in the v8 oracle | `Finite<f64>` newtype at the API boundary |
| R5 | `fast_evaluate` breaks its contract ("does not touch any cached state", `AbstractState.h:893-894`; also its own comment `IF97Backend.h:673-676`) by calling `clear()` | `IF97Backend.h:1007-1009`. Oracle: after a batch call, `T()` = −inf. The test only compares values (`TabularBackends.cpp:1999-2023`). The comment at `:1007-1008` shows the intent was "state untouched". Still on master (`:1031`) | Batch calls destroy the caller's state | Batch APIs as `&self` free functions over an immutable model, with no state object |
| R6 | *Downgraded by verifier: a documented trade-off, not rot.* IF97 Region 3 density uses the SR5-05 backward v(T,p) without Newton refinement. The HP/PS flash uses backward T only | `REGION3_ITERATE` exists nowhere in the tree (`IF97.h:2562-2569`); the IF97 `README.md:54` documents the ~1e-6 error as the default speed trade-off; the ±25 mK backward-T uncertainty is acknowledged at `IF97.h:4058-4065`. Oracle: ρ error −4.15e-6 against R7-97 Table 33; HP round-trip error −2.5 J/kg | Not exact to the basic equation; the oracle and the IF97 check program (which defines the flag) disagree | `enum Region3Density {Backward, Iterated}` and an `HpRefine` flag. Default exact, plus a `CoolPropCompat` mode for oracle tests |
| R7 | IF97 rejects p < 611.213 Pa everywhere, although R7-97 defines R2 and R5 for 0 < p | `IF97.h:84, 3891, 3975`. Oracle: (400 K, 100 Pa) → "Pressure out of range" | Domain narrower than IAPWS | Domain taken from the release, with low-p R2/R5 included |
| R8 | IF97 kernels recompute: every derivative is its own term loop; w calls `dgammar_dPI` 3 times; λ₂ calls cp twice plus cv, which calls cp again (base `cvmass`; the Region 1 override does not); psat is computed twice in region selection. Backward kernels make 3 `std::pow` calls per term, and R3 v(T,p) recomputes the loop-invariant `pow(π−a,c)` and `pow(θ−b,d)` per term | `IF97.h:337-418, 259, 464-466, 3917-3919, 3592-3597, 1623-1628` | Wasted cycles in the hot path (oracle: `fast_evaluate` 750 ns/point for 2 outputs) | One pass builds the derivative bundle from shared power tables; integer powers by multiplication; loop-invariant hoisting; SIMD over states |
| R9 | A routine documented "for testing purposes only" sits in the hot path. Each HP/PS `update` runs the full `RegionDetermination_pX` (Tsat plus 2 forward evaluations) 2-3 times | `IF97.h:4021-4024`; callers `IF97Backend.h:113-115, 225, 247, 268`; `IF97.h:4099` | Slow flashes | Classify the region once and pass a typed `Region` through |
| R10 | Duplicated transport code in Region 3 has diverged from `BaseRegion`: π literal 3.141592654 vs `2*acos(0)`; a ζ clamp exists only in R3; the Δχ < 0 guard exists only in the base. `Region5::lambda2` *hides* the non-virtual `BaseRegion::lambda2` and is dead | `IF97.h:452-478` vs `2377-2403`; `:2746-2748`. Inert in practice: oracle λ = λ₀λ₁ to 2e-16 at 5 R5 points, and no NaN over 1600 R3 points | Latent divergence; misleading code | One transport module parameterised by the region's (cp, cv, ∂ρ/∂p) |
| R11 | `-_HUGE` sentinel returns and silent no-op setters. API inconsistency: `PropsSI` refuses IF97 phase strings that the backend now supports | `IF97Backend.h:327-328, 366-367, 51`; `src/CoolProp.cpp:541-542` vs `IF97Backend.h:563-568` | Silent garbage; confusing API | `Result`; capability traits |
| I1 | INCOMP HmassP/PSmass Brent: `maxiter = 10` with a near-machine-precision T-bracket tolerance | `IncompressibleBackend.cpp:424-427, 455-458`; `Solvers.cpp:561, 628, 639-640`. Oracle (verifier rerun, all 126 fluids, 12 T × 5 x at 1 MPa): 37 of 6226 PT→HP/PS round-trips fail with "Brent's method reached maximum number of steps of 10", 35 of them PSmass (IceEA 12, IcePG 10, IceNA 7, others 1 each). The original grid (31 of 2140) was not recorded. *(added by verifier)* In the same sweep, 31 HP round-trips at exactly T = Tmin or Tmax fail with "do not bracket the root", because \|f(endpoint)\| exceeds the 2.2e-13 endpoint tolerance (`Solvers.cpp:530-543`) | Random flash failures | Safeguarded Newton with analytic dh/dT = c > 0 inside a bracket |
| I2 | Unfitted placeholder coefficients shipped as data: LiBr/MITSW T_freeze = exp(700/(x−60)−10); LiBr μ = 1 Pa·s, λ = 0; Acetone μ/λ; 12 psat blocks, of which DSF's is reachable and crosses its pole in range | Oracle: LiBr T_freeze 3.7e-10 K, MITSW 3.9e-10 K, LiBr μ = 1.0, λ = 0.0; DSF psat 0.42 Pa at 400 K. Fixed post-v8 in `9f1b088c` | **The oracle is wrong here** | Build-time data gate (no placeholder, zero, non-finite or in-range-pole fits); register of known-bad oracle values |
| I3 | PCL viscosity is 100× too high | `25c4f3a5`. Oracle: 0.272 Pa·s at 0 °C | Wrong oracle | Sanity tests on Pr and ν per fluid |
| I4 | A `polyoffset` T_freeze is evaluated against **pressure**. 1D JSON coefficients load as a column (`MatrixMath.h:151-164`); a column means "function of the first argument"; for T_freeze that argument is p | `IncompressibleFluid.cpp:96-101, 216-227`; `IncompressibleBackend.h:229-233`. Oracle (`ExampleSecCool`, x = 0.2): T_freeze = −9.34e16 K at 1e5 Pa, where f(x) would give 258.06 K. The freezing check is therefore silently disabled | Latent; only an example fluid uses polyoffset | Typed axes (`Axis::T/X/P`) in fit descriptors |
| I5 | Six `Example*` test fixtures ship as production fluids | `dev/generate_headers.py:422-450` globs every JSON. The oracle lists ExamplePure, ExampleDigital, ExampleDigitalPure, ExampleSolution, ExampleMelinder and ExampleSecCool | Pollutes the public fluid list | Fixtures live only in the test crate |
| I6 | Mass↔volume conversion is unimplemented although the data exists | `IncompressibleFluid.cpp:287-395`. Oracle: `AEG.set_mass_fractions` → "Mass composition conversion has not been implemented." | Dead data; confusing API | `MassFraction`/`VolumeFraction` newtypes, with conversions where data supports them |
| I7 | Thermodynamically inconsistent model: cp ≡ cv; (∂ρ/∂p) = 0 whatever is held constant; the reported cp ≠ (∂h/∂T)_p at p ≠ 0; no mixing enthalpy; reference re-pinned per x | `IncompressibleBackend.h:217-222`; `.cpp:532, 522-527, 208`; master `NOTES_thermodynamic_consistency.md`. Oracle: MEG cp = cv = 3738.19; (∂ρ/∂p)_h = 0; `speed_sound` unimplemented | Energy balances across concentrations are wrong (#1690) | Gibbs form g(T,p,x) = g₀(T,x) + v(T,x)(p−p₀) behind a compat flag |
| I8 | Allocation-heavy polynomial engine: an Eigen matrix copy per call and per row; derivative and integral coefficients rebuilt on every call; the 0/0 at T = Tbase is approximated by an ε-band interpolation | `PolyMath.cpp:430-437, 439-466, 490-518, 528-565, 567-637`. The ε-band was replaced by the exact value post-v8 (`9f1b088c`). Oracle: h(T) 1.97 µs; HP flash 13.1 µs | Slow, and not SIMD-able | Precompute derivative and antiderivative tables at build time; allocation-free Horner/Clenshaw |
| I9 | Dead code and stubs | ~424 commented lines of LiBr (315 in `IncompressibleLibrary.cpp:12-326`, 109 in `.h:18-126`); 534 commented lines in `PolyMath.h:458-991`; `baseExponentialOffset` declared but never defined (`IncompressibleFluid.h:260`); always-throwing methods (`:271-292, 347-373, 384-395`); `validate()` is a no-op (`IncompressibleFluid.cpp:24-30`) | Noise | Do not port |
| I10 | Load failures are swallowed: a partial library, with the error printed to stdout. A failed fluid stays in `fluid_map` as a zombie entry | `IncompressibleLibrary.cpp:552-562`, `:433-438, 488-491` | Silently missing fluids | Fail at build time; `Result` at runtime |
| I11 | Tautological test: expected and actual are computed by the same call | `IncompressibleBackend.cpp:882-885` | False coverage | Golden data from source tables |
| I12 | `INCOMP::Air`, a gas, is treated as incompressible | `json/Air.json`; DATA_AUDIT "don't-touch" calls it a gas reference. Oracle: ρ = 1.176 kg/m³ at 0.1, 1 and 10 MPa, where HEOS gives 1.16, 11.6 and 116.9 | Trap for users | Drop it, or mark it fixed-pressure-only |
| I13 | An exponential-fit pole inside the valid range is "linearised" instead of rejected | `IncompressibleFluid.cpp:45-60, 71-86`; DSF psat (I2) | Hides bad fits | Reject at build time any fit whose pole lies in [Tmin,Tmax] |
| I14 | Fluid purity is inferred from the **shape** of the density matrix, not from `xid`. Exponential/logexponential fits ignore x | `IncompressibleFluid.cpp:32-35, 115-118, 151-154` | Latent misclassification and silent loss of x | Typed `FluidKind`; fit descriptors declare their arguments |
| H1 | `HAProps_Aux("kT")` passes **density as pressure** to `PT_INPUTS`; the correct `DmassT` is at `:839`. The test only asserts kT > 0 | `HumidAirProp.cpp:2345`; `CoolProp-Tests.cpp:1398-1409`. Oracle: kT = 1.00e-3 1/Pa, against 4.5e-10 | Wrong by 6 orders of magnitude, and CI is green | Typed state constructors, not positional `(f64,f64)`; value tests against tables |
| H2 | Water-vapour μ and λ are taken from saturated vapour **at total p**, which does not depend on T. Tsilingiris uses vapour at T | `HumidAirProp.cpp:985-987, 1008-1011`. Oracle at 300 K: μ_w = 1.223e-5 against 0.976e-5; mixture μ +0.35% at RH 0.5, growing with p (at 5 MPa μ_w is taken at 537 K) | Transport bias | Implement the paper, which is the arbiter; keep a compat flag for oracle tests |
| H3 | Four water formulations in one call: IF97 p_ws (`:564, 809, 1308, 1352, 1556, 1574, 1595`); IAPWS-95 saturation in `f_factor` (`:859-861`); IAPWS-95 at the IF97 density for k_T and wet-bulb h_w (`:838-840, 1366-1369`); HEOS `PropsSI` Tsat for T_max (`:1884`). The liquid/ice threshold is `>` in some places (`:819, 857, 1350, 2342`) and `>=` in others (`:562, 1306, 1554, 1573, 1593`) | Oracle: IF97 vs IAPWS-95 psat differ by up to −1.1e-4 (500 K) and −6e-5 (300 K) | Internal inconsistency | One injected water model per humid-air instance; one phase-boundary predicate |
| H4 | Magic constants: four gas constants (8.314472 `:110, 1025, 1072, 2129`; 8.314371 `:271, 849, 1088`; 8.314510 `:272, 1097, 1190`; 8.3145 `:2380`). ε = 0.621945 and M_a = 0.028966 sit alongside the EOS values 0.6219569 and 0.02896546. Air T_red 132.6312 is hard-coded (`:307, 320, 1100, 1108`). Offsets at `:281-326, 1083, 1101, 1110, 1195`. "Not clear why getting rid of this term yields the correct values" (`:1131`). *Partly downgraded by verifier:* three of the four R values are each source formulation's own constant (8.314371 = IAPWS-95 R·M, 8.314510 = Lemmon 2000 air, 8.314472 = RP-1485/CODATA 2006; comments at `:270-272`), and ε = 0.621945 = M_w/M_a with RP-1485's M_a = 28.966 g/mol. That is intentional, not rot. Only 8.3145 (`:2380`, `HAProps_Aux` debug output) is an outlier | code; oracle for the EOS values | Unauditable without provenance; fragile reproduction | Named constants with RP-1485 equation provenance, isolated in an `rp1485_compat` module + golden tests |
| H5 | Hand-rolled secants stop silently after 100 iterations: `f_factor` (`:900-946`, then f clamped ≥ 1 at `:947`), `MolarVolume` (`:1041-1068`), `MolarEntropy` (`:1208-1232`), `DewpointTemperature` (`:1293-1329`). The f ≥ 1 test only checks the clamp (`CoolProp-Tests.cpp:1340-1355`) | code | Silent non-convergence | One solver returning `Result<Converged>` |
| H6 | The dew-point initial guess is Tsat(**p**) − 1, while the comment says the guess should come from p_w | `HumidAirProp.cpp:1282-1288`. Replicating the secant: 13 vs 6 iterations (300 K, RH 0.5), 18 vs 8 (1 MPa), 17 vs 10. Each iteration calls `f_factor`. Oracle: D costs 132 µs against 6.8 µs for W | ~2× slower dew-point and T_dp-input paths | Guess from Tsat(p_w) (or the ice line); Newton with analytic df/dT |
| H7 | The `FlagUseIdealGasEnthalpyCorrelations` path in entropy prints "Not implemented" and uses s⁰ = 0 | `:1234-1235` | Silently wrong s | Drop the flags, or make them compile-time model variants |
| H8 | Global mutable model switches, a global error slot, and `HAPropsSI` returning `_HUGE`. `DONT_CHECK_PROPERTY_LIMITS` also skips the NaN check | `:114-116, 363-383, 2261-2266, 122-125`; `src/CoolProp.cpp:83-106` | Cross-thread interference; NaN accepted | Per-call config value; `Result<T, HumidAirError>` |
| H9 | String-API calls and finite differences in the hot path | `PropsSI("T","P",p,"Q",0,"Water")` (`:1884`); finite-difference cp, cv, w (`:2079-2126`). Oracle: T(H,R) 203 µs, Twb 152 µs | Slow inverses | Analytic derivatives of the virial mixture; direct model calls; precomputed B(T), C(T) |
| H10 | A "trivial" output (output key = input key) returns before any bounds check | `:2163-2172`. Oracle: `HAPropsSI('T','T',300,'P',101325,'R',5)` = 300; P = −1 returns −1 | Invalid inputs accepted | Validate inputs before dispatch |
| H11 | k_T switches to a polynomial correlation inside IF97's 3.3e-5 saturation band, to dodge the IF97 throw (#2690) | `:824-836` | Model discontinuity inside an iterated function | Fix IF97 phase handling (R2/R3) and evaluate liquid k_T from one consistent model |
| H12 | Legacy surface. `HAProps` (kSI) scales the **dimensionless** isentropic exponent and the speed of sound by 1000 and is still C-exported (`CoolPropLib.h:299`), although the Python wrapper removed it in v8. `HAProps_Aux` copies with C `strcpy` into an unsized `char*` (`:2283ff`). Global-namespace `strcmp`/`strcpy` overloads with external linkage return `size_t` (`:28-42`). `HAHelp` prints "Sorry, Need to update!" (`:952-954`). Dead solvers (`:590-651`). A commented test "incredibly slow ... Many of the tests also fail" (`:2809-2844`) | `:1625-1627, 1659-1661` | Maintenance burden; overflow-prone C API; wrong legacy values *(code evidence; the oracle Python cannot reach it)* | Do not port; typed diagnostics API |
| H13 | The C_aww coefficient differs from IAPWS G8-10 Table 4 in the 6th digit (d₂ 0.347804e4 vs 3478.02), and d₁ is truncated (−10.72887 vs −10.728876) | `:702, 714`; G8-10 Table 4 (verified against the IAPWS PDF) | ~7e-5 relative in C_aww from d₂ at 300 K, ~6e-6 from d₁ *(inference: transcription)* | The literature is the arbiter; take constants from one cited table |
| H14 | *(added by verifier)* The dry-air guard in `DewpointTemperature` tests `(1 - psi_w) < 1e-16` (pure water), while its comment says "Make sure it isn't dry air". For ψ_w = 0 the secant runs on p_w = 0 and returns a finite garbage temperature that passes the T bounds check | `HumidAirProp.cpp:1268-1271`. Oracle: `HAPropsSI('D','T',300,'P',101325,'W',0)` = 149.395 K (same for R = 0). Unchanged on master | Silently wrong dew point for dry air | Dew point as `Option<T>` / typed error for ψ_w = 0 |
| C1 | Ice g00 is the superseded IAPWS-06 (2006) value, not the 2009 revision (−0.632 020 233 335 886e6) | `src/Ice.cpp:21`. Oracle: g = 0.6116705 vs the R10-06(2009) Table 6 value 0.611784135; Δ = −1.136e-4 J/kg at all 3 check points. ρ, h, s and κ_T match to ≤ 3.4e-10 | μ_ice ≠ μ_liquid at the triple point (tiny, but stale source) | Use the revised constant; the literature is the arbiter; compat flag |
| C2 | Ice functions return 1e99 on PowerPC; no domain checks; global free functions; g_TT and g_Tp are missing; no unit tests | `src/Ice.cpp:2-12, 29-34, ...` | Platform-dependent garbage; cp and α unavailable | `IceIh: GibbsPotential` with a domain; portable complex arithmetic |
| P1 | REFPROP is not thread-safe in v8 | Fixed post-v8 by `8214f28e`; the commit records that 2 threads on different fluids abort the process | Crash | Use it only as an oracle, in a separate single-threaded process |
| P2 | `instance_counter` is incremented twice for pure fluids: the implicit base ctor does `++` (`REFPROPMixtureBackend.h:61-63`), then `construct()` does `++` again (`.cpp:214`). The dtor decrements once (`:222`) | `REFPROPBackend.cpp:27-31`; also on master | *(inference)* The DLL is never unloaded once a pure instance has existed | n/a (not ported) |
| P4 | *(added by verifier)* With alias resolution on, a CoolProp fluid whose `REFPROP_NAME` is "N/A" resolves to the literal name "N/A", because the code tests only `REFPROPname.empty()` | `REFPROPMixtureBackend.cpp:362, 377`; 9 of 136 `dev/fluids/*.json` carry "N/A". Fixed post-v8 by `0e8d00ef` (#3242) | Wrong REFPROP fluid lookup for those 9 fluids | n/a (not ported); the oracle harness must map names itself |
| P3 | Hygiene: test names promise "within 0.5%" but assert 5%; `#pragma error` is not an error directive; non-static globals; stdout diagnostics; the version is decoded from `ierr` | `REFPROPMixtureBackend.cpp:2833/2860, 2863/2891, 82, 67, 72, 279-287, 302-325` | Weak cross-check; fragile | Name tolerances in the harness |
| B1 | Bollengier: six independent spline evaluations, each redoing the span search and basis functions. A stale comment says "reference-shifted". Reference-state plumbing cannot re-anchor it (`set_reference_stateS` silently no-ops) | master `BollengierBackend.h:313-318, 208-209, 95-99` | ~6× redundant work; a cross-area reference-state gap | Fused `eval_derivs<2>()` returning the whole bundle; reference state as part of model config |

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

| Kernel | Shape | SIMD | Threads | Strategy |
|---|---|---|---|---|
| IF97 R1/R2/R5 forward (34 / 43+9 / 6+6 terms, integer exponents, e.g. R1 I∈[0,32], J∈[−41,17]) | Straight-line polynomial sums | Excellent across states (SoA) and across terms | trivial | Bucket a batch by region. Per lane, build π^I and τ^J tables by multiplication, then one fused pass yields (γ, γ_π, γ_ππ, γ_τ, γ_ττ, γ_πτ) |
| IF97 R3 (SR5-05 tree over 26 sub-regions with logs/sqrt in the dividing lines, then a 40-term φ) | Decision tree + polynomial with real exponents | Good after sub-region bucketing; branchy before it | trivial | Classify, compact, evaluate. Hoist `pow(π−a,c)` once per lane. Optional fixed-count masked Newton refinement |
| IF97 backward T(p,h), T(p,s), p(h,s) | Shifted polynomials; I/J mostly integer stored as double | Good per sub-region | trivial | Same bucketing; integer power tables instead of 3 `pow`/term |
| IF97 transport | exp/sqrt; piecewise ζ table; λ₂ branch at y < 1.2e-7 | Fair (masked lanes) | trivial | Compute alongside the bundle, reusing cp/cv/∂ρ/∂p |
| INCOMP forward | Small dense 2D polynomial (≤ 4×6); collapses to 1D for fixed x | Excellent | trivial | Group the batch by (fluid, x), collapse x once, then Horner/Clenshaw across lanes. Exp-type μ/psat via vector exp |
| INCOMP inverses (T from h, s, ρ) | Monotone 1D root | Masked Newton with a fixed iteration count | trivial | Analytic c (or c/T, ρ_T) gives quadratic convergence; bracket clamp per lane |
| Humid-air forward (T,p,ψ) | Virial polynomial + 2 small 1D solves (v̄, f) | Moderate: fixed-iteration Newton with masks; B and C(T) need the EOS | embarrassingly parallel | Precompute B_aa, C_aaa, B_ww, C_www(T) as Chebyshev at load to drop EOS calls *(estimate: µs → ~100 ns)*. Solve v̄ as a cubic with a Newton polish |
| Humid-air inverses (T from H/B/D; W from X) | Nested Brent/secant; data-dependent iteration counts; triple-point gap logic | Poor | Parallelise across states | Keep these scalar |
| Ice Ih | Closed-form complex logs | Good (complex math as re/im lanes) | trivial | Rarely hot |
| Bollengier | Binary span search + 6×6 contraction × 6 derivative orders | Good after span bucketing | trivial | Fuse the derivatives; contract in one pass |
| REFPROP | Global Fortran state | none | **none** (serialise) | Oracle only, in a separate process |

**Side-by-side architecture** (the user's goal):
- Each SIMD-able kernel gets a scalar reference plus a batch/SIMD twin with identical math, behind a common kernel trait.
- The twin is verified against the reference to ≤ N ULP by property tests. The tolerance must allow FMA contraction differences.
- Region, sub-region, fluid or composition bucketing is a separate pre-pass that returns index permutations.
- Solvers in the batch path have bounded iterations and per-lane status, like `fast_evaluate`'s status flags but without touching state.
- Branchy humid-air inverses, the IF97 R3 classifier before bucketing, and REFPROP do not suit SIMD. They parallelise only across requests.

## 8. Verification assets (tests, check values, paper tables, oracle hooks)

| Asset | Location | Pins |
|---|---|---|
| IAPWS R7-97(2012) Tables 5, 15, 33, 42 (forward), 35/36 (saturation), 7, 9, 24, 29 (backward); SR3-03 Table 5; SR5-05(2016) Tables 5/13; R12-08, R15-11 and R1-76 check values | Printed by `IF97.cpp` in CoolProp/IF97 @ 7aaced02 (536 lines, no assertions); `IF97.h:4792-4907` (Table 5/3 data); release PDFs | IF97 exactness. Oracle check: R1/R2/R5 ≤ 3e-9 (table rounding); R3 ρ 4e-6 (R6) |
| CoolProp IF97 tests | Benchmarks only (`CoolProp-Tests.cpp:3482-3565`); `fast_evaluate` vs `update` (`TabularBackends.cpp:1999-2026`); SVDSBTL comparisons | **Gap: no assertion tests against IAPWS tables in v8** |
| IF97 conformance generator | `Web/scripts/fluid_properties.IF97Conformance.py` (protocol of IAPWS G13-15) | Backend-vs-IF97 statistics; reusable for the Rust IF97 |
| INCOMP regression tests | `CoolProp-Tests.cpp:5244-5343` (#2209 psat, #1578 Tbase continuity, #1374 MPG2 vs the Melinder grid ±1%), `:6288` (molar throws); `IncompressibleFluid.cpp:556-758` (XLT, CH3OH SecCool points); `IncompressibleBackend.cpp:583-1079` | Fit evaluation + flashes (note I11) |
| INCOMP raw source tables (arbiter) | `dev/incompressible_liquids/CPIncomp/data/` (Melinder, SecCool, VDI grids) | Fit fidelity vs data; master `test_fitting_regression.py` |
| Master INCOMP data tests | `test_json_sanity.py`, `test_data_sanity.py`, `test_chebyshev_entries.py`, `DATA_AUDIT.md`. Don't-touch list: LiqNa λ up to 87 W/m/K, Air as a gas, Zitrec LC μ 59 Pa·s, Kelvin freeze tables | Data-gate design for the Rust build step |
| Humid air: RP-1485 tables | `HumidAirProp.cpp:2452-2627`: A.2.1 virials (1e-3), A.3 f, A.5 k_T (80 °C relaxed to 1e-2 because RP-1485 0.46009e-9 vs IAPWS-95 0.46150e-9, `:2506`), A.6 Henry, A.11/A.12/A.15 state points (1%; one A.12 entry commented "seems incorrect from report", `:2536`) | Model; loose tolerances |
| Humid air: physics and issue tests | `CoolProp-Tests.cpp:851-1027` (#2670, virial/α⁰ caches), `:1028-1465` (ASHRAE A.6.1/A.6.2/A.8/A.9, mostly finiteness and inequalities; aux tests at `:1336+` are plausibility-only, see H1), `:5439-5676` (#2255, #2690, #2906) | Inverse robustness |
| Humid air table generator | `Web/fluid_properties/Validation/HAValidation.py` | Re-creates the RP-1485 tables; compares nothing |
| IAPWS G8-10 (humid air, 2010) | Tables 13-15 check values for f^AV, f^A, f^V and f^mix | Arbiter for a consistent humid-air model; cross-virial constants (H13) |
| Ice | IAPWS R10-06(2009) Table 6; R14-08(2011) Eq. 6 check value. Verified for this map: ρ, h, s, κ_T ≤ 3.4e-10; psub(230 K) = 8.947352740 Pa | **Gap: no ice tests in CoolProp** |
| Bollengier | `master:CoolProp-Tests-Bollengier.cpp:127-195`: Supplementary Material E, 95 (p,T) points of ρ, cp and w at 2e-5 (a 6×17 grid minus the cold high-pressure corner the authors omit); Maxwell relations `:443`; IAPWS-95 agreement at T ≥ 300 K and p ≤ 50 MPa `:63` (the test comment says the models are *not* uniformly close below 100 MPa); SeaFreeze is the external reference implementation | **Not in the v8 oracle** |
| REFPROP | `REFPROPMixtureBackend.cpp:2830+` (5% tolerances); `INFO.REFPROP_NAME` map | Second oracle for HEOS-side fluids only |

**Register of known oracle divergences.** The TDD harness must mark these rather than match them:
- IF97: R1, R2 (labels), R3, R4, R6 (exact mode), R7.
- INCOMP: I2, I3, I4, I12, and all `Example*` fluids.
- Humid air: H1, H2, H6 (cost only), H10, H13, H14.
- Ice: C1.
- Divergences beyond v8: master's Chebyshev caloric path changes INCOMP ρ/c at ~1e-9 (basis conversion) or at fit level (refits); Bollengier is absent from v8.

**REFPROP as a second oracle.**
- **Useful only for HEOS-side inputs**: IAPWS-95, Lemmon air, pure fluids and mixtures.
- **No equivalents**: REFPROP has nothing comparable to INCOMP or RP-1485 humid air. IF97, Bollengier and the ice Gibbs function are not REFPROP formulations *(inference)*.
- **Cheapest path**: the v8 oracle wheel itself, through `AbstractState("REFPROP", name)` with `REFPROP_RESOLVE_COOLPROP_ALIASES`, provided a REFPROP install is present.
- **Constraints**: single-threaded, in a separate process, never linked into the crate. The licence restricts it to local use, not public CI.

## 9. Port recommendation (units with priority P0-core / P1-early / P2-later / defer / drop; order; what to redesign)

| Unit | CoolProp source | Priority | Rationale |
|---|---|---|---|
| `gibbs-potential` (trait + bundle g, g_T, g_p, g_TT, g_Tp, g_pp → v, s, h, u, cp, cv, w, κ, α) | `IF97.h:235-261`; `Ice.cpp:118-143`; master `BollengierBackend.h:322-332` | P0-core | ~150 LOC. Designed together with the Helmholtz map so that IF97, ice, Bollengier, Gibbs-INCOMP and future TEOS-10 share one implementation (DRY) |
| `if97-forward` (R1, R2, R3 φ, R4, R5, B23, region map) | `IF97.h:34-2758, 3887-3962` | P1-early | Fast water; pure; exact IAPWS check values; SIMD showcase. Fix R7, R8, R10; expose R6 as a strategy option |
| `if97-backward` (T(p,h), T(p,s), p(h,s), T(h,s), R3 v(T,p)) | `IF97.h:672-2181, 2765-3876, 4041-4539` | P1-early | Needed for HP/PS and R3. Table-heavy; generate the tables |
| `if97-transport` (R12-08, R15-11 IF97 variant, R1-76) | `IF97.h:117-196, 262-296, 420-478, 2703-2715` | P1-early | Small; deduplicate (R10) |
| `if97-state` (input pairs incl. molar; phase classification; Q blending) | `IF97Backend.h:85-448` | P1-early | Rewrite; fix R1, R2, R3, R5, R9, R11 |
| `if97-batch-simd` (bucketed SoA kernels beside the scalar ones) | `IF97Backend.h:677-1010` (contract only) | P2-later | The user's SIMD goal; the scalar path comes first |
| `incomp-data` (build-time JSON → static tables; data gate; known-bad register; no `Example*`) | `dev/incompressible_liquids/json`, `generate_headers.py:422-450` | P1-early | Lazy and per fluid, with no runtime JSON. Fix I2, I3, I5, I12, I13 |
| `incomp-eval` (5 fit types; precomputed d/dT, ∫c, ∫c/T; typed axes) | `IncompressibleFluid.cpp:38-282`; `PolyMath.cpp:363-791` | P1-early | Allocation-free; SIMD-friendly. Fix I4, I8, I14 |
| `incomp-backend` (PT, DmassP, HmassP, PSmass, QT(Q=0); limits; reference state; fraction basis) | `IncompressibleBackend.cpp:59-567`; `IncompressibleFluid.cpp:441-490`; `CoolProp.cpp:136-235` | P1-early | Fix I1, I6, I10. Expose cp/cv honestly, with I7 in compat mode |
| `incomp-chebyshev-caloric` | master `IncompressibleLibrary.cpp:470-489` + `_cheb` JSON | P2-later | Not in the v8 oracle; adopt when the oracle moves |
| `incomp-gibbs-solution` (single potential g(T,p,x); LiBr–H₂O per Pátek & Klomfar) | master `NOTES_mixing_models.md`, `NOTES_thermodynamic_consistency.md` | defer | Fixes I7 properly; a research item |
| INCOMP fitting pipeline (Python, 6.6k LOC) | `dev/incompressible_liquids/CPIncomp/` | drop | Keep as an offline data generator; consume its JSON |
| `ice-ih` (full R10-06(2009) bundle + R14-08 sublimation; domain) | `src/Ice.cpp` | P1-early | The first solid model and the template for future solids. Fix C1, C2 |
| `humid-air-core` (virial mixture at (T,p,ψ), f, k_T, Henry, h, s, u, v; injected water, air and ice models) | `HumidAirProp.cpp:110-1261` | P1-early | After HEOS water and air. Pure functions; per-T virial precompute. Fix H3-H5, H7, H11, H13 |
| `humid-air-inputs` (typed input combinations; T and W inverses; dew point; wet bulb including the triple-point gap) | `:384-545, 1263-1481, 1546-2013` | P1-early | `Result` API. Fix H6, H8, H9, H10, H14 |
| `humid-air-transport` (Tsilingiris, compat + paper modes) | `:971-1015` | P2-later | Fix H2 |
| `humid-air-iapws-g8-10` (consistent Helmholtz humid air + seawater/ice coupling) | none in CoolProp; IAPWS G8-10 | defer | The literature-grade successor; first step to "all states" |
| Humid-air legacy surface (`HAProps` kSI, `HAProps_Aux` strings, `cair_sat`, `IceProps`, correlation flags) | `:363-383, 1610-1689, 2269-2443` | drop | Replace with a typed diagnostics API (virials, f, k_T) |
| `bspline2d` (tensor B-spline with a fused derivative bundle) | master `TensorBSpline2D.h` | P2-later | Reusable numeric unit (also for tabular areas) |
| `bollengier-water` (PT only; domain + excluded box; no reference shift) | master `BollengierBackend.h` | P2-later | Not in the v8 oracle; verify against SM_E and SeaFreeze |
| `refprop-oracle` (dev-only harness: serialised, separate process, unit conversion, name map) | `REFPROPMixtureBackend.cpp:2-20, 234-300, 354-388` | defer | A separate tool, never in core or WASM; second oracle for HEOS areas |
| REFPROP backend port | `src/Backends/REFPROP/` | drop | Proprietary; process-global state |

**Order**:
1. `gibbs-potential`, designed together with the HEOS core.
2. `if97-forward`, then `if97-transport`, then `if97-backward`, then `if97-state`. TDD against IAPWS tables first and the oracle second, using the divergence register.
3. `ice-ih`.
4. `incomp-data`, `incomp-eval`, `incomp-backend`.
5. `humid-air-core` and `humid-air-inputs`; these need HEOS water and air.
6. Later: SIMD twins, Chebyshev, the transport fix, B-spline/Bollengier, G8-10.

**Redesign, not transliteration**:
- Model objects are immutable and `Send + Sync`. State and flash are a thin layer on top.
- Batch APIs never touch instance state.
- Every compat-vs-exact choice is an explicit strategy enum: Region 3 density, HP refinement, INCOMP caloric (polynomial / Chebyshev / Gibbs), humid-air vapour transport, ice g00, humid-air water model.
- Fraction basis, mass/molar basis and fit axes are typed.
- There are no global toggles, sentinels, error slots or thread-local caches. Precomputation happens once into immutable tables (`OnceLock`) or at build time.

## 10. Open questions

1. **INCOMP oracle baseline.** Match v8 (polynomial path, known-bad data, plus a register), or adopt master's scrubbed data and Chebyshev entries and accept the divergence?
2. **IAPWS-exact IF97 by default?** That means Region 3 Newton, HP refinement and low-p R2/R5. It diverges from the oracle by ~1e-6 and changes domain behaviour.
3. **Humid air defaults.** Fix the vapour-transport state (H2), unify the water model (H3) and correct C_aww (H13) by default, or keep RP-1485/CoolProp compatibility by default? Is IAPWS G8-10 the long-term arbiter instead of RP-1485?
4. **Ice g00.** Adopt the 2009 value (literature) by default? RP-1485 may have used the 2006 value *(inference)*.
5. **Bollengier scope and provenance.**
   - Is it in scope before the oracle includes it?
   - Is the SeaFreeze licence acceptable for a test-only oracle?
   - Is the provenance of the extracted coefficients (paywalled supplementary material, MIT-shipped by CoolProp) acceptable for the project's permissive licence?
6. **INCOMP fluids that duplicate EOS fluids.** Water, Air, Acetone, Ethanol and Hexane are sampled from CoolProp HEOS. Keep, mark, or drop them?
7. **REFPROP licence.** Does the user have one for local oracle runs? If so, which version (9.1 or 10)?
8. **Composition basis in the public API.** Should a volume-based fluid accept mass fractions once conversions exist (I6)? What syntax replaces `MEG-30%`?
9. **Ice slurries.** Keep them as pseudo-fluids until a `PhaseAssemblage` exists, or defer them?
10. **LiBr–H₂O.** Implement Pátek & Klomfar (2006) directly from the paper instead of the polynomial refit? The commented-out C code shows that intent (`IncompressibleLibrary.cpp:12-326`).
11. **RP-1485 vs IAPWS-95 table disagreements.** The A.5 k_T at 80 °C differs by 0.3%, and one A.12 entropy is flagged "incorrect from report". Should the Rust tests pin the report or the recomputed IAPWS-95 value?

## Verification log

- **Date:** 2026-10-04 (adversarial verifier).
- **Integrity:** no partial edits from an earlier verifier were found. All tables were intact, and there was no earlier verification log.
- **Claims checked:** about 190. That covers every `path:line` citation in sections 2-9 (v8.0.0 checkout; CoolProp/IF97 cloned at `7aaced02`; `origin/master` for Bollengier, Chebyshev and REFPROP fixes), every rot item, and every count.
- **Re-measured:**
  - LOC per subsystem and the IF97 term counts (R1 34 with I∈[0,32], J∈[−41,17]; R2 43+9; R3 40; R5 6+6; 26 sub-regions; 12 dividing lines).
  - INCOMP census (126 = 74/39/13; fit-type counts; density shapes).
  - The `REFPROP_NAME` coverage and the Bollengier sizes.
- **Re-run against the v8 oracle:** R1, R2, R3, R4, R5, the `fast_evaluate` cost, I1-I7, I12, H1, H10, H14 and the humid-air timings.
- **Checked against the source literature:** the G8-10 Table 4 coefficients, against the IAPWS PDF.

**Corrections**
1. Header and §4: the INCOMP JSON is 337 KB as shipped, not "592 KB pretty". The 592 figure was `du` block usage. The raw data is 0.7 MB in 311 files, not 1.7 MB.
2. §2.2: the partial-derivative lookup has 17 cases, not 18.
3. §5: the INCOMP backend has 19 CachedElements, not 17.
4. §2.6 and P4: `REFPROP_NAME` is present in 127 of 136 fluid JSONs, not 129 of 138.
5. §3.1: deleted the claim that IF97 HP-state s and h are "mutually inconsistent". The oracle shows s equals the forward s at the same backward T. Only h ≠ h_in.
6. R3: narrowed "HP/PS above p_crit always throws" to Region-3 states only. Region-1/2 states above p_crit work (oracle).
7. R6: downgraded to a documented trade-off. The IF97 README documents the ~1e-6 Region-3 error as the default, and the ±25 mK backward-T uncertainty is acknowledged in code. §9 now treats R6 as a strategy option rather than a fix.
8. R5: corrected the test range to `TabularBackends.cpp:1999-2023`.
9. I1: the original "31 of 2140" could not be reproduced on any recorded grid. It is replaced by a documented sweep (37 of 6226, mostly PSmass, ice slurries dominant), and the Brent citation is fixed to `Solvers.cpp:628`.
10. I9: the PolyMath commented block is 534 lines at `PolyMath.h:458-991`, not 544 at 440-993. The LiBr dead code totals ~424 lines (cpp + h).
11. §4: the ice constants are 6 complex + 9 real, not 7 real.
12. H4: partly downgraded. Three of the four R values are each source EOS's own constant, and ε/M_a are RP-1485's values. Only 8.3145 (`:2380`) is an outlier.
13. H13: added the d₁ truncation, and confirmed d₂ against the G8-10 PDF.
14. R8: λ₂'s extra cp call applies only to the base `cvmass`, not to Region 1.
15. §8 Bollengier: there are 95 SM_E points, not ~97. The `:63` test covers T ≥ 300 K and p ≤ 50 MPa, not "below 100 MPa".
16. §4: master has 127 INCOMP JSONs, all with `_cheb` entries, not "all 126".
17. §3.2: added the verifier's rerun timings next to the original ones.
18. §9: I12 is now listed under `incomp-data`.

**Added by verifier**
- I1 (extension): HP round-trips at T = Tmin or Tmax fail with "do not bracket".
- H14: the dry-air dew-point guard is inverted. HAPropsSI returns 149.4 K for W = 0.
- P4: at v8, a `REFPROP_NAME` of "N/A" resolves to the literal "N/A" (fixed post-v8 in `0e8d00ef`).

**Survived refutation attempts:** R1, R2, R4, R5, R7, R8, R9, R10, R11, I2-I8, I10-I14, H1-H3, H5-H12, C1, C2, P1-P3, B1.
