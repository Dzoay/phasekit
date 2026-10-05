# 06 Cubic, PC-SAFT and GERG-2008 backends - CoolProp v8.0.0 map

> Scope: `src/Backends/Cubics/` (4,561 lines), `src/Backends/PCSAFT/` + `include/CoolProp/fluids/PCSAFTFluid.h` (3,772 + 96 = 3,868), `dev/cubics/` (5,619, almost all JSON), `dev/pcsaft/` (4,355, JSON), cubic/PC-SAFT tests (`src/Tests/CoolProp-Tests-Cubic*.cpp`, blocks of `CoolProp-Tests.cpp`, `CoolProp-Tests-Michelsen.cpp`). GERG is **not in v8.0.0**: it landed in c7d6a1aa (#3314) after the tag, so `src/Backends/GERG/` (13,357 lines, of which 10,029 are generated test vectors), `dev/gerg/` (3,802), `src/Tests/CoolProp-Tests-GERG.cpp` (2,592) and the spec/plan (2,319) are cited from origin/master `022b63e4` and marked **[master]**. About 8.4k lines of hand-written C++ at v8.0.0 (4,561 + 3,868). Part of the coolprop-rs port plan; cites the v8.0.0 source.

## 1. Purpose and concepts

All three families are alternative **residual Helmholtz models** αr(T, ρ, x). CoolProp wires each one into the library differently, and those differences account for most of the rot in this area.

| | Cubic (SRK, PR, VTPR) | PC-SAFT | GERG-2004/2008 [master] |
|---|---|---|---|
| Model | one-fluid generalized cubic recast as αr(τ,δ,x) (Bell & Jäger 2016) | Gross–Sadowski PC-SAFT: hard chain + dispersion + Gross–Vrabec dipole + Wertheim association + Debye–Hückel ions | Kunz–Wagner multi-fluid: Σxᵢαr_oi + ΣΣxᵢxⱼFᵢⱼαr_ij, with GERG's own short pure EOS |
| Class / base | `AbstractCubicBackend : HelmholtzEOSMixtureBackend` | `PCSAFTBackend : AbstractState` (standalone) | `GERGMixtureBackend : HelmholtzEOSMixtureBackend` |
| Adds beyond HEOS | a cheap closed-form EOS for any fluid with (Tc, pc, ω); analytic PT roots; kij; Soave/Mathias–Copeman/Twu alpha; volume translation; cubic superancillary; gE mixing (VTPR) | associating, polar and electrolyte systems; 180 fluids incl. 26 ions; kij(T) | the reference natural-gas model; 18/21 components; parameters fixed by the publication; R = 8.314472 |
| HEOS reuse | all property evaluation, every input pair other than PT/QT/PQ/DT, mixture VLE, Michelsen TPD/flash, critical points, envelopes | **none**: its own flashes, density solver and phase logic | everything; GERG itself adds only data, its own R, and guards |
| Inputs | all HEOS pairs (pure PT/QT/PQ/DT are special-cased) | PT, QT, PQ, DmolarT only (every other pair throws, `PCSAFTBackend.cpp:1858-1871`) | all HEOS pairs; pure HmolarP/PSmolar/PUmolar fail (pinned test) |
| Missing outputs | viscosity, conductivity and surface tension throw (8.0.0 oracle, PR::Methane; corrected by verifier); `rhomolar_critical` is a heuristic | h, s, u, cp, cv, w, transport, `fugacity_coefficient(i)` | transport and surface tension (refused on purpose) |
| Oracle in CoolProp==8.0.0 | yes, except entropy (#3287), volume translation (both broken) and pure QT saturation beyond ~1e-6 (C14) | partial (bugs P1-P4, §6) | **absent** |

Key concepts:
- **Reducing-coordinate coupling.** HEOS works in τ = T_r/T and δ = ρ/ρ_r. The cubic sets T_r = 1 K and ρ_r = 1 mol/m³ (`GeneralizedCubic.cpp:198-199`, `CubicBackend.cpp:28`). Its ideal-gas α0, however, is the HEOS fluid's α0, written in (Tc/T, ρ/ρc). HEOS therefore has to apply chain-rule scaling, and the one place it did not caused the PR/SRK entropy bug (#3287, `HelmholtzEOSMixtureBackend.cpp:3684`).
- **The cubic is also infrastructure.** HEOS uses SRK/PR as seeds: the SRK covolume as the density bound (`HelmholtzEOSMixtureBackend.cpp:2821-2833`), `solver_rho_Tp_SRK` (`:3078`, used at `VLERoutines.cpp:1160-1161,2638-2642`) and `T_DP_PengRobinson` (`FlashRoutines.h:167`). A cubic can also replace a fluid's residual part: the `HEOS::X-SRK` / `X-PengRobinson` fluids (`src/Backends/Helmholtz/Fluids/FluidLibrary.h:1236-1312`) and `change_EOS` (`calc_change_EOS`, `HelmholtzEOSMixtureBackend.cpp:483-525`) both go through `ResidualHelmholtzGeneralizedCubic` (`include/CoolProp/fluids/Helmholtz.h:600-621`).
- **PC-SAFT has no ideal-gas part.** Only residual h/s/g exist (`PCSAFTBackend.h:129-138`), so it cannot report absolute h or s, cp, cv or w (all confirmed missing in the oracle).
- **GERG is data plus strictness.** The spec calls it "a wiring-and-data exercise, not new thermodynamics" (`docs/superpowers/specs/2026-07-25-gerg-strict-backend-design.md:58-60`). CoolProp's HEOS mixture model already *is* the Kunz–Wagner formalism. What differs is the pure EOS (reference EOS vs GERG's short forms), 16 of the 210 binaries, and R (spec `:26-51`).

## 2. Structure (key types/functions -> path:line)

### 2.1 Cubic family (`src/Backends/Cubics/`)

| Unit | Location | Notes |
|---|---|---|
| Alpha functions: `BasicMathiasCopeman` (Soave), `MathiasCopeman`, `Twu` | `GeneralizedCubic.h:25-106`, `.cpp:10-194` | `term(τ, n≤4)` plus `calc_all_terms`; a `m_version` counter feeds cache invalidation (`.h:32-63`) |
| `AbstractCubic` (model core) | `GeneralizedCubic.h:108-681`, `.cpp:196-767` | Tc/pc/ω vectors, Δ1/Δ2, kij matrix, scalar `cm`, alpha vector, **mutable caches** (`.h:124-154`); about 600 lines of hand-coded composition derivatives up to 3rd order (`.h:400-672`, `.cpp:402-767`) |
| `SRK`, `PengRobinson` | `GeneralizedCubic.h:683-720`, `.cpp:769-800` | exact Ωa/Ωb ("Bell and Deiters, IECR, 2021") |
| `AbstractCubicBackend` | `CubicBackend.h:36-351`, `.cpp:10-919` | HEOS subclass; overrides αr derivatives, reducing state, R, limits, PT/QT/PQ/DT |
| `SRKBackend`, `PengRobinsonBackend` | `CubicBackend.h:353-437` | read `CubicLibrary`; `get_copy` + `copy_internals` (`.cpp:734-762`) |
| `CubicResidualHelmholtz` | `CubicBackend.h:439-581` | adapter into HEOS `ResidualHelmholtz`; factored `all()` (`:459-509`) |
| PT density | `CubicBackend.cpp:390-412`, `:563-685` | Cardano (`solve_cubic`); the root is picked by imposed phase, by Gibbs energy, or by comparison with p_sat |
| Pure saturation | `CubicBackend.cpp:414-562` | Secant/BoundedSecant on Δg |
| Superancillary | `CubicBackend.cpp:828-919`; `include/CoolProp/superancillary/cubicsuperancillary.h` (720 lines) | Chebyshev in T̃ = RTb/a |
| `CubicLibrary` | `CubicsLibrary.h:14-50`, `.cpp:15-180` | Meyers singleton over embedded JSON |
| VTPR | `VTPRBackend.h:27-104`, `.cpp:11-162`, `VTPRCubic.h:17-227` | PR + UNIFAC residual gE mixing |
| UNIFAC | `UNIFAC.h:18-104`, `UNIFAC.cpp`, `UNIFACLibrary.{h,cpp}` | groups and interaction parameters; τ-derivatives by finite differences |
| Registration | `AbstractState.cpp:109-160`, `DataStructures.cpp:804-819` | static `GeneratorInitializer` objects |

### 2.2 PC-SAFT (`src/Backends/PCSAFT/`)

| Unit | Location | Notes |
|---|---|---|
| `PCSAFTBackend` | `PCSAFTBackend.h:22-167` | standalone `AbstractState` with its own `SatL`/`SatV`; physical constants at namespace scope in the header (`:16-20`) |
| Two near-identical constructors | `PCSAFTBackend.cpp:57-123`, `:125-189` | set the ion/polar/assoc/water flags; load kij |
| αr | `.cpp:247-515` | hs, hc, disp, polar, assoc, ion |
| ∂αr/∂T | `.cpp:517-845` | complete re-derivation of every term |
| Fugacity vector | `.cpp:863-1351` | complete re-derivation |
| Z | `.cpp:1367-1703` | complete re-derivation |
| h_res, s_res, g_res | `.cpp:847-861`, `:1353-1365` | g_res moved to the (T,V) basis by the #1943 fix |
| `update`, phase logic | `.cpp:1733-1884`, `:1886-1994` | |
| QT/PQ flash | `.cpp:1996-2573` | inside-out (Watson et al. 2017), with brute-force sweeps as fallback |
| Initial estimates | `.cpp:2575-2746` | |
| Density solver | `.cpp:2749-2863` | 40-point grid scan + Brent + Gibbs root choice (Privat et al. 2010) |
| Association helpers | `.cpp:2898-3061` | `XA_find`, `dXAdt_find`, `dXAdx_find`, site matrix |
| `dielc_water` | `.cpp:3063-3090` | fit to Archer & Wang 1990 |
| `PCSAFTFluid` | `include/CoolProp/fluids/PCSAFTFluid.h:12-93`, `PCSAFTFluid.cpp:9-17` | getters; `calc_water_sigma` mutates the parameters |
| Library | `PCSAFTLibrary.h:19-60`, `.cpp:17-389` | Meyers singleton; binary pairs keyed by `vector<string>` of CAS numbers |

### 2.3 GERG [master] (`src/Backends/GERG/`)

| Unit | Location | Notes |
|---|---|---|
| `GERGMixtureBackend` | `GERGBackend.h:44-330` | 65 code lines, 243 comment lines (mostly a catalogue of the mutation routes it must guard) |
| Construction, linked states | `GERGBackend.cpp:22-95` | rebuilds SatL/SatV so they are GERG-typed |
| `update` range guard (T only, 60-700 K) | `.cpp:97-175` | |
| `calc_gas_constant` = 8.314472 | `.cpp:177-183` | overrides HEOS's global `NORMALIZE_GAS_CONSTANTS` behaviour |
| `set_mixture_parameters` | `.cpp:191-287` | β/γ matrices, Fᵢⱼ, departure functions; a zero-coefficient dummy departure for every non-departure pair (`:274`) |
| Coefficient tables | `.cpp:289-1110`, `GERGData.h` (438) | function-local `static const std::map`s |
| `resolve_component` | `.cpp:1112-1145` | goes through CoolProp's global fluid library (`get_fluid_param_string(.., "CAS")`, `:1130`) |
| `make_gerg_fluid` | `.cpp:1388-1784` | ideal gas re-expressed as generalized Planck–Einstein terms (`:1590-1632`); R*/R folded in |
| Generated data | `GERGAncillaries.h` (669), `GERGAcentric.h` (78) | fitted against teqp's GERG pure EOS |
| `GERGReferenceValues.h` | 10,029 lines | test vectors only (see §8) |

## 3. Algorithms and formulas (cite the papers CoolProp cites)

### 3.1 Generalized cubic (Bell & Jäger, J. Res. NIST 121 (2016) 238, cited at `CubicBackend.h:1-12`)
- αr = ψ⁻ − τ·a_m(τ)/(R·T_r)·ψ⁺ (`GeneralizedCubic.cpp:733-735`). With τ = T_r/T this is
  αr = −ln[1 − (b_m − c_m)ρ] − a_m/(R T b_m(Δ1 − Δ2)) · ln[(1 + (Δ1b_m + c_m)ρ)/(1 + (Δ2b_m + c_m)ρ)]
  (ψ⁻ `.cpp:376-401`, ψ⁺ `.cpp:553-575`, A-term `.h:619-623`).
- SRK has Δ1 = 1, Δ2 = 0; PR has Δ1,2 = 1 ± √2 (`.h:689,709`). a_ii = Ωa R²Tc²/pc·α(T) and b_i = Ωb R Tc/pc, with exact Ω values from Bell & Deiters 2021 (`.cpp:769-795`).
- Alpha: Soave form [1 + m(1 − √(T/Tc))]², with m(ω) from Soave 1972 (SRK, `.cpp:779-784`) and PR 1976 (`.cpp:796-800`). Mathias–Copeman uses the 3-term polynomial **at all T** (`.cpp:36-80`). Twu: (T/Tc)^{N(M−1)}·exp[L(1 − (T/Tc)^{MN})] (`.cpp:82-123`). Neither MC nor Twu is cited in the code; canonical sources are Mathias & Copeman, FPE 13 (1983) 91 and Twu et al., FPE 69 (1991) 33 (inference from domain knowledge).
- Mixing (vdW1f): a_m = ΣΣxᵢxⱼ(1 − kᵢⱼ)√(aᵢᵢaⱼⱼ) (`.cpp:232-245,344-375`); b_m = Σxᵢbᵢ (`.cpp:277-292`). At v8.0.0, c_m is one mixture-wide scalar (`.h:122,353-360`).
- Derivatives: all 15 τ/δ derivatives up to 4th order are built from three 5-element intermediate arrays (`CubicBackend.h:459-509`). Composition derivatives are hand-coded per factor (b_m, PI_12, A, c, ψ±, τa).
- PT density: the cubic in ρ (`CubicBackend.cpp:397-406`) is solved by Cardano. With three roots: imposed phase, or Gibbs comparison (mixtures, `:576-590`), or a full PQ flash of a "transient" state to get ρ_V (pure, `:650-667`).
- Pure saturation: secant on Δg = ln(δV/δL) + αr_V − αr_L + δV·αr_δ,V − δL·αr_δ,L (`:452-460`). Seeded by log10(p/pc) = (7/3)(1 + ω)(1 − Tc/T) (`:507,530-531`).
- Superancillary: Chebyshev expansions of p̃, ρ̃_L, ρ̃_V in T̃ = RTb/a(T), with p = p̃·a/b² and ρ = ρ̃/b (`.cpp:862-881`). Bell & Deiters, IECR 60 (2021) 9983, cited at `Web/coolprop/Cubics.rst:113`. Valid for any alpha function, because a(T) only enters through T̃.
- Pure saturation tolerances differ by direction: PQ uses `Secant(..., ftol 1e-10)` (`CubicBackend.cpp:511`), QT uses `BoundedSecant(..., ftol 1e-5)` on Δg (`:544,547`). The superancillary path (`update_QT_pure_superanc`, `:884-919`) is a separate entry point; `update(QT_INPUTS)` still goes through the secant (`:351-355` → `saturation()`) (added by verifier).
- "Critical density" for a pure cubic is a Kazakov curve fit, v_c[L/mol] = 2.14107·(Tc/pc·1000) + 0.00773 (`CubicBackend.h:166-174`; duplicated at `.cpp:149` and `src/Backends/Helmholtz/Fluids/FluidLibrary.h:1288`). It is **not** the EOS critical point (Zc = 1/3 for SRK, 0.30740 for PR).
- VTPR: a_m = b_m[Σxᵢaᵢᵢ/bᵢᵢ + g^{E,R}/(−0.53087)]; b_ij = ((bᵢ^{3/4} + bⱼ^{3/4})/2)^{4/3}; b_m = ΣΣxᵢxⱼb_ij (`VTPRCubic.h:113-161`). g^{E,R} comes from the UNIFAC residual part (`UNIFAC.cpp:135-220`). No literature is cited; VTPR is Ahlers & Gmehling, FPE 191 (2001) 177 (inference). The UNIFAC test reproduces a Poling example (`UNIFACLibrary.cpp:141-181`).

### 3.2 PC-SAFT (references listed at `PCSAFTBackend.cpp:13-53`)
- dᵢ = σᵢ(1 − 0.12·exp(−3εᵢ/kT)) (`:251`); ions use a T-independent d (Held 2014, `:253-260`). ζₙ = (π/6)ρ_N Σxᵢmᵢdᵢⁿ, η = ζ3 (`:264-274`).
- Hard sphere (BMCSL): a_hs = (1/ζ0)[3ζ1ζ2/(1−ζ3) + ζ2³/(ζ3(1−ζ3)²) + (ζ2³/ζ3² − ζ0)·ln(1−ζ3)] (`:315-317`). Chain: a_hc = m̄·a_hs − Σxᵢ(mᵢ − 1)·ln gᵢᵢ^hs (`:310-311,348`). Gross & Sadowski, IECR 40 (2001) 1244.
- Dispersion: −2πρ·I1·m²εσ³ − πρ·m̄·C1·I2·m²ε²σ³ (`:349`). I1 and I2 are degree-6 polynomials in η with m̄-dependent coefficients (`:319-338`); C1 at `:339-341`. Combining rules: σᵢⱼ = (σᵢ + σⱼ)/2, εᵢⱼ = √(εᵢεⱼ)(1 − kᵢⱼ − kᵢⱼ,T·T) (`:289-305`).
- Dipolar (Gross & Vrabec, AIChE J 52 (2006) 1194): a_polar = A2/(1 − A3/A2), with J2/J3 correlations; A3 is O(N³) (`:351-421`).
- Association (Huang–Radosz 1990/1991; Gross–Sadowski 2002): Δᴬᴮ = gᵢⱼ^hs·(e^{εᴬᴮ/kT} − 1)·σᵢⱼ³·κᴬᴮᵢⱼ (`:451-454`). X_A is found by damped successive substitution (`:458-477`), and a_assoc = Σxᵢ Σ_A(ln X_A − X_A/2 + 1/2) (`:479-482`).
- Ions (Cameretti 2005; Held 2008, 2014): Debye–Hückel κ (`:497-498`), χᵢ (`:505`), a_ion (`:509`). The dielectric constant comes from `dielc_water` (Archer & Wang 1990); water σ(T) from `PCSAFTFluid.cpp:16`, which cites no source.
- p = Z·kT·ρ_N (`:239-245`); R is never a named constant, it is written `kb * N_AV` inline (`:851,859,1363`); h_res = (−T∂a/∂T + Z − 1)RT (A.46, `:847-852`); s_res = R(−T∂a/∂T − a) (`:855-860`); g_res = (a + Z − 1)RT (`:1353-1365`).
- VLE: inside-out flash (Watson et al., IECR 56 (2017) 960; `:2084-2573`). Density roots by grid scan with min-Gibbs selection (Privat et al., FPE 295 (2010) 76; `:2749-2863`).

### 3.3 GERG-2004/2008 [master] (Kunz et al., GERG TM15 (2007); Kunz & Wagner, JCED 57 (2012) 3032; teqp is the reference implementation)
- α = α° + Σxᵢαr_oi(δ,τ) + ΣΣ_{i<j} xᵢxⱼFᵢⱼαr_ij(δ,τ). The reducing T_r and ρ_r use GERG β/γ functions (HEOS `GERG2008ReducingFunction`, `ReducingFunctions.h:144`). Departure terms use `GERG2008DepartureFunction`/`ExcessTerm` (`ExcessHEFunction.h:104,200`).
- Pure residual: Σn·δ^d·τ^t + Σn·δ^d·τ^t·exp(−δ^c), as 12/24-term shared exponent sets (`GERGBackend.cpp:304-389`). Departure: polynomial + exp[−η(δ−ε)² − β(δ−γ)] (spec `:143-147`: 15 pairs, 7 of them generalized with Fᵢⱼ ≠ 1).
- Ideal gas: α°_oi = ln(ρ/ρ_ci) + (R*/R)[n1 + n2τ + n3·ln τ + Σn·ln|sinh θτ| − Σn·ln cosh θτ], with R* = 8.314510 and R = 8.314472 (spec `:182-197`). The integration constants n1 and n2 are **recomputed** so that h = s = 0 for the ideal gas at 298.15 K / 101325 Pa (spec `:199-213`, `GERGBackend.cpp:1029`).
- Validity range 60-700 K, applied verbatim to every component (Kunz & Wagner 2012 §4.1; commit 6dc22bf2). As pure fluids, He and H2 are supercritical-only.

## 4. Data and configuration inputs

| Dataset | File | Size | Provenance | Loading |
|---|---|---|---|---|
| Cubic fluids (116) | `dev/cubics/all_cubic_fluids.json` | 115 KB | **generated snapshot** of HEOS Tc/pc/ω/ρc/M/α0 (`dev/cubics/generate_cubics_listing.py:5-19`); every alpha type is "default"; the `*_units` fields are ignored | whole file schema-validated and parsed on first use (`CubicsLibrary.cpp:133-153`) |
| Cubic schema | `dev/cubics/cubic_fluids_schema.json` | 2.4 KB | | embedded |
| Cubic superancillary | `cubicsuperancillary.h` | 720 lines | Bell & Deiters 2021 (ported from teqp) | compiled-in constants |
| PC-SAFT fluids (180) | `dev/pcsaft/all_pcsaft_fluids.json` | 62 KB | Gross 2001 (63), Kleiner 2007 (66), Held 2014 (26 ions), Gross 2002 (17), Ghosh 2003 (7), Fuchs 2006 (water); water `"sigma": -1` is a sentinel | whole file parsed on first use (`PCSAFTLibrary.cpp:59-68`) |
| PC-SAFT pairs (140) | `dev/pcsaft/mixture_binary_pairs_pcsaft.json` | 22 KB | Held 2014 (100), Gross 2001/2002, Ghosh; only 2 pairs have kijT | parsed into a map keyed by sorted CAS pair |
| UNIFAC groups/params/decompositions | **not shipped** | n/a | user must set `VTPR_UNIFAC_PATH` (`VTPRBackend.cpp:137-155`) | read from disk on first VTPR use |
| GERG tables [master] | `GERGBackend.cpp:289-1110`, `GERGData.h` | ~1.3k lines | transcribed from teqp `GERG.hpp`, cross-checked by `dev/gerg/verify_transcription.py` | function-local static maps |
| GERG ancillaries/ω [master] | `GERGAncillaries.h`, `GERGAcentric.h` | 747 lines | fitted/derived offline with teqp 0.23.2 (`dev/gerg/fit_ancillaries.py`, `compute_acentric.py`) | static |

Configuration keys read: `R_U_CODATA` (= 8.31446261815324, cubic R, `CubicBackend.h:115,366,407`), `OVERWRITE_FLUIDS`, `OVERWRITE_BINARY_INTERACTION`, `VTPR_UNIFAC_PATH`, `VTPR_ALWAYS_RELOAD_LIBRARY` (`include/CoolProp/detail/configuration_keys.h:49,52,69`), `LIST_STRING_DELIMITER`, and for GERG `DONT_CHECK_PROPERTY_LIMITS`. GERG must override `calc_gas_constant` because `NORMALIZE_GAS_CONSTANTS` would otherwise rescale mixture R (`GERGBackend.h:262-275`).

## 5. State, caching, globals, thread-safety, memory

- **Mutable model objects.** `AbstractCubic` memoizes aᵢᵢ(τ) and bᵢ in `mutable` members and validates them with τ bit-compare plus alpha version counters (`GeneralizedCubic.h:124-154`). The model is therefore not shareable across threads. On master, every setter needs invalidation and rollback (`set_Tci` → `refresh_after_critical_change` / `rollback_critical_unless_translation_fits`, master `GeneralizedCubic.h:316-333`).
- **Hand-propagated mutation.** Every cubic setter loops over `linked_states` (`CubicBackend.cpp:712-714,734-748,785-788,799-808`). When a copy misses a field, the copy silently runs a different model: `cm` was not copied into TPD/critical/transient states until master 1ace9311. VTPR's `get_copy` shares alpha-function `shared_ptr`s with its parent (`VTPRBackend.h:64-70` → `CubicBackend.cpp:734-740`).
- **PC-SAFT changes its own parameters per T.** `calc_water_sigma(T)` overwrites σ in `components` and `dielc` is a member. Both are reset from flash loops at trial temperatures (`PCSAFTBackend.cpp:1786-1846,2138-2144,2297-2306,2606-2614`). The density residual mutates `_rhomolar` through `update_DmolarT` (`:2758-2765`).
- **Global singletons.** `CubicLibrary` (`CubicsLibrary.cpp:160-163`) and `PCSAFTLibrary` (`PCSAFTLibrary.cpp:49-57`) are Meyers singletons, so first construction is thread-safe. `add_fluids_as_JSON` and `set_mixture_binary_pair_pcsaft` (`PCSAFTLibrary.cpp:21-23`) then mutate the maps **without locks** while backend constructors read them: a data race. The global BIP mutation already caused test-order dependence (master 42a848f0, COO-62). VTPR uses a file-scope `static UNIFACParameterLibrary lib` that is populated lazily without a lock (`VTPRBackend.cpp:9,137-155`). `VTPR_ALWAYS_RELOAD_LIBRARY` clears it (`UNIFACLibrary.cpp:11-15`) while other `UNIFACMixture`s hold `const&` to it (`UNIFAC.h:22`).
- **GERG [master]** has immutable static tables, which is the good pattern. It still inherits HEOS's mutable state and public members; its own header lists the bypasses (`GERGBackend.h:198-253`).
- **Memory.** Each cubic or PC-SAFT `AbstractState` holds two more full backends (SatL/SatV). Cubic mixtures add TPD, critical and transient states on demand. Each copy duplicates its component vectors, including α0 containers and PC-SAFT strings. Datasets are parsed whole at first use: 115 KB of cubic JSON; 83 KB of PC-SAFT JSON (62 + 22) plus schema validation. Resolving a GERG name that is not one of the canonical lowercase teqp spellings (e.g. "Methane" rather than "methane") goes through `get_fluid_param_string(.., "CAS")`, i.e. CoolProp's HEOS fluid library (`GERGBackend.cpp:1119-1130`); that this loads the whole library is an inference.

## 6. Rot and bugs (table: issue | evidence | impact | Rust remedy)

IDs: C = cubic, V = VTPR/UNIFAC, P = PC-SAFT, G = GERG. "8.0.0 oracle" means verified with `uv run ... CoolProp==8.0.0`.

| Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|
| C1 PR/SRK **entropy values wrong** | `HelmholtzEOSMixtureBackend.cpp:3684` returns α0 derivatives without the Tc/T_r chain rule; fixed on master 9b96b64b (#3287). 8.0.0 oracle: PR::Methane at 1 MPa, 200→300 K gives ΔS(value) = 1361.07 vs ∫cp/T dT = 910.33 (ratio 1.495) | s values wrong, and S-input flashes (PS, HS, ST) interpret s on that wrong scale (inference). **g and a are not affected** (verifier, 8.0.0 oracle, PR::Methane 1 MPa 250 K: −∂g/∂T = (h − g)/T = 81.284 to 1e-10, while `smolar` = 38.098; u − Ts − a equals h − Ts − g), because they need no ∂α0/∂τ. 8.0.0 is unusable as an entropy oracle for cubics | exchange reducing-invariant derivatives (A_nm = τⁿδᵐ∂αⁿ⁺ᵐ, identical for any reducing pair); never share τ/δ between models with different reducing states |
| C2 saturation converts δ with **T_r instead of ρ_r** | `CubicBackend.cpp:513-514,551-552`; master 1ace9311 calls it a "latent bug" | masked only because T_r = ρ_r = 1 | dimensional (T, ρ) model API |
| C3 `calc_pressure_nocache(T, ρ)` ignores its arguments in the prefactor and hides the non-virtual HEOS version | `CubicBackend.cpp:197-202` vs `HelmholtzEOSMixtureBackend.h:470`; still present on master (`CubicBackend.cpp:208`) | latent: the only off-state call is in dead spinodal code (`:525-536`) | pure functions of (model, T, ρ, x); no name hiding |
| C4 `throw -1` (int) at 18 sites; the cubic HEOS term is `throw()` | `GeneralizedCubic.cpp:32,78,121,315,341,...,698`; `src/Helmholtz.cpp:579` | latent: every site is the `default:` of a derivative-order switch (order > 4), which no caller requests. If reached, the int escapes `CoolPropBaseError`/`std::exception` handlers and lands only in `catch (...)` (e.g. `CoolPropLib.cpp:52`) with no message; through the `throw()` term it is `std::terminate` (verifier downgraded "uncatchable") | typed `Result` errors; derivative order checked by type (const generics) |
| C5 **stale duplicated cubic dataset** | `generate_cubics_listing.py` snapshot: Ammonia Tc 405.4 vs HEOS 405.56; D5 619.15 vs 618.3; R1233zd(E), n-octane, D5 α0 ≠ current HEOS JSON. 8.0.0 oracle: cp0(PR) vs cp0(HEOS) differs >0.1% for 11/116 fluids (D5 16%, MD4M 13.5%, R1233zd(E) 2.3%) | PR::X and HEOS::X silently use different critical constants and ideal gases | derive cubic inputs from the canonical fluid DB; no generated copies |
| C6 **three gas-constant literals plus per-fluid R** across families (verifier: "four" overstated; k_B·N_A = 1.380649e-23 × 6.02214076e23 is bit-identical to `R_U_CODATA` = 8.31446261815324 in double) | R = 8.3144598 (CODATA 2014) hard-coded at `src/Backends/Helmholtz/Fluids/FluidLibrary.h:1257`, `HelmholtzEOSMixtureBackend.cpp:496,517,2830`; `R_U_CODATA` (2018) in cubics; k_B·N_A recomputed inline in PC-SAFT (`PCSAFTBackend.h:16,18`, `.cpp:851,859,1363`); GERG 8.314472; HEOS mixtures switch to `R_U_CODATA` under `NORMALIZE_GAS_CONSTANTS` (master `GERGBackend.h:262-275`). 8.0.0 oracle: cp0 of PR vs HEOS differs by >1e-6 for 98/116 fluids; the most common offset (54 of 97 checked) is 1.128e-6 = 8.314472/8.31446262 − 1, i.e. HEOS fluid files written with R = 8.314472 | ~1e-6 inconsistencies, hard to diagnose | R as explicit model data; one documented constant per model |
| C7 `rhomolar_critical` for pure cubics is a heuristic | `CubicBackend.h:166-174`. 8.0.0 oracle: PR::Methane reports 10369 vs model 9443 mol/m³ (+9.8%; SRK +19%); ∂p/∂ρ there = 13.2, not 0 | reported critical point inconsistent with the model | compute the model's critical point (closed form for cubics) |
| C8 volume translation broken in 8.0.0 | 4 paths ignore `cm`; `cm` lost in `get_copy`; d(A)/dxᵢ misses the (1 + c·ρ) factor (`GeneralizedCubic.h:631-634`). All fixed in master 1ace9311 (PR methane/decane bubble point 10.20 → 6.87 MPa error) | wrong p_sat, fugacities, negative pressures when c ≠ 0 | port master semantics: per-component cᵢ with linear mixing (Privat–Jaubert 2016), tested against the Jaubert 2016 invariances |
| C9 `HEOS::<cubic-lib fluid>-SRK` path | R_u never set and only Twu alpha applied (`src/Backends/Helmholtz/Fluids/FluidLibrary.h:1291-1311`); master ae54172f: R_u "defaulted to 0 and zeroed p" | wrong or zero pressures | drop the name-suffix trick; compose an explicit model |
| C10 fabricated metadata to satisfy HEOS | Tmin = 0.3·Tc, Tmax = 10·Tc, pmax = 100·pc, p_triple = 0.01·pc (`CubicBackend.h:175-219`); placeholder `CoolPropFluid`s carrying only α0 (`CubicBackend.cpp:73-89`); T_triple read from that placeholder (`CubicBackend.h:107-110`) | flash bracketing depends on invented numbers | flash needs only the model trait and explicit `ValidityRange` |
| C11 dead or orphaned code | "Robust PT flash" doc blocks with no code (`CubicBackend.h:45-61`, `.cpp:921-936`); spinodal disabled as "VERY slow" (`.cpp:525-527`); unused `static std::string errstr` (`:510,524`); VTPR tests commented out (`VTPRBackend.cpp:171-195`, `MixtureDerivatives.cpp:1679`) | misleading docs, untested code | — |
| C12 two assemblies of the same cubic αr | factored (`CubicBackend.h:459-509`) vs 15 separate `alphar()` calls (`src/Helmholtz.cpp:579-604`) | DRY; the HEOS-embedded cubic is slower | one kernel |
| C13 inconsistent naming | 8.0.0 oracle: `PCSAFT::Methane` fails (UPPERCASE only) while `PR::methane` works; PR rejects CAS `74-82-8`, PC-SAFT accepts it (`CubicsLibrary.cpp:104-120` looks up name and alias only) | user-facing inconsistency | one identity registry for all families |
| C14 pure cubic QT saturation converged loosely (added by verifier) | `update(QT_INPUTS)` → `saturation()` → `BoundedSecant(..., ftol 1e-5, ...)` on Δg (`CubicBackend.cpp:352-355,544,547`). 8.0.0 oracle, SRK::Propane 300 K: the QT-flash phases disagree on p and Δg/RT by 2.7e-7 / 2.1e-7, while the superancillary densities give equal p to 1e-13 and Δg/RT 6e-15; over 150-365 K the flash p_sat differs from the superancillary by up to 1.0e-6 (SRK) and 1.8e-8 (PR) | 8.0.0 is only a ~1e-6 oracle for pure-cubic saturation via QT | superancillary + Newton polish (U4); explicit tolerance in the API |
| V1 VTPR unusable out of the box | no UNIFAC data shipped; requires `VTPR_UNIFAC_PATH` (`VTPRBackend.cpp:137-155`) | dead feature | defer/drop |
| V2 ln γ^R τ-derivatives by nested central finite differences (1% step) | `UNIFAC.cpp:216-218`; 2ⁿ evaluations, each mutating via `set_temperature` (`:135-203`) | inaccurate 3rd/4th derivatives, slow, not reentrant | AD |
| V3 wrong VTPR composition derivatives | `VTPRCubic.h:134-140` uses ∂/∂xᵢ twice and drops b·∂²S/∂xᵢ∂xⱼ (same pattern at `:141-148`); tests commented out | wrong VTPR fugacity derivatives and flashes | AD |
| V4 VTPR misc | `// TODO` ignores translation in ln φ (`VTPRCubic.h:220`); magic −0.53087 repeated 10× (`:114,131-147,222`) | | named constant |
| P1 **DmolarT two-phase quality wrong** | `PCSAFTBackend.cpp:1969,1983` interpolate linearly in ρ. 8.0.0 oracle: methane 150 K at the midpoint ρ gives Q = 0.5, lever rule 0.0431 (QT(0.0431) reproduces that ρ). Mixtures also get p = p_bubble (`:1968,1982`) instead of a TV flash. Not fixed on master | wrong Q and p for two-phase (ρ,T) | shared generic flash |
| P2 **PT fails for every supercritical T** | `estimate_flash_p` (`:2641-2746`) throws, and is called outside any try (`:1901`). 8.0.0 oracle: methane at 300 K, 1 bar or 50 bar → "an estimate for the VLE pressure could not be found"; verifier reproduced it for methane 200 K, CO2 350 K, propane 400 K / 10 bar and equimolar methane/benzene at 700 K (propane 300 K / 1 bar works). Not fixed on master (`estimate_flash_p(*this)` still unguarded at master `:1903`) | the most basic PT call fails unless the phase is imposed | stability-test based phase determination |
| P3 `dielc_water(t)` evaluates the member `_T` instead of `t` | `PCSAFTBackend.cpp:3083,3085`, called with trial temperatures (`:2142-2144,2197-2200,2303-2306,2611-2614`); `_T = -inf` after `clear()` (`AbstractState.cpp:311`). 8.0.0 oracle (re-run by verifier, Q = 0, 298-340 K): QT→PQ round trip drifts +0.05 to +0.07 K for Na+/Cl−/water at x_ion = 0.0106 each, +0.57 to +0.81 K at x_ion = 0.091 each; 1.3e-8 K for methanol/cyclohexane and ≤1.6e-7 K for pure water (σ(T) but no ions), so the drift is ion-specific. Attributing all of it to this bug is an inference. Still on master | electrolyte PQ flashes wrong | pure functions of T |
| P4 no ideal gas; few inputs | `PCSAFTBackend.h:129-138` (`// TODO implement these heat capacity functions` at `:136`). 8.0.0 oracle: h/s/cp/cv/w and `fugacity_coefficient(i)` throw NotImplemented; HmolarP/PSmolar unsupported; `get_fluid_constant` always throws (`:101-117`) | barely usable outside VLE/density | model = residual + ideal gas from the fluid DB |
| P5 4× copy-paste of the whole model | αr, ∂αr/∂T, ln φ and Z each re-derive every term (`:247-515,517-845,863-1351,1367-1702`). Coefficient tables repeated (`:319,612,943,1444`). The XA loop is repeated with drifted tolerances (`1e-15` at `:467,788,1280` vs `1e-14` at `:1646`). Constructors duplicated (`:57-123` vs `:125-189`) | the four copies span ~1.45k of the file's 3,092 lines (`:247-1703`; verifier corrected "3.1k LOC"); slightly inconsistent derivatives | one αr kernel + AD |
| P6 hard-sphere cancellation at low density | 1/ζ0 form (`:315-317`). 8.0.0 oracle (propane, 300 K): αr/ρ relative error 5e-5 at 1e-8 mol/m³, 1e-3 at 1e-10, NaN at ρ = 0 ("p is not a valid number") | B2/virial limit unreliable; ρ = 0 crashes | series expansion for small ζ (teqp is said to do this at `pcsaft.hpp:109`; not verified, teqp is not checked out locally) |
| P7 association solver | damped successive substitution, 100 iterations, no failure signal; a non-finite closed-form initial guess is replaced by XA = 0.02 (`:458-477`). Unqualified `abs(double)` (`:472,793,1285,1651`, also `:2854-2855` in the density solver) binds to `int abs` with only `<cmath>/<cstdlib>`; with `<Eigen/Dense>` included it resolves to the double overload (verifier re-ran both scratch compiles with g++ and clang++), so it works only through the transitive Eigen include | silent non-convergence; latent portability bug | Newton with an error result; closed forms for pure 1A/2B |
| P8 density solver | 40-point grid scan + Brent per bracket (`:2770-2802`). Fallback returns the best grid point unconverged (`:2847-2858`). Returns `_HUGE` (+inf) for unhandled phases (`:2804-2830`) | slow (≥41 full evaluations per call) and silently inaccurate | packing-fraction Newton + stability check |
| P9 flash fallbacks are brute force; loose acceptance | log₁₀p sweep −6..9 in 0.1 steps (`:2012-2026`), T sweep 800→1 K in 10 K steps (`:2055-2072`); squared residual for root-finding (`:2115,2377`); outerPQ iterates to 1e-8 but accepts maxdif ≤ 1e-3 (`:2087,2338`); outerTQ iterates to 1e-8 but accepts maxdif ≤ 0.1 (`:2348,2568`, added by verifier) | up to ~150 flashes per call; answers may be converged only to 1e-3 (PQ) or 0.1 (QT) | shared VLE with explicit tolerances |
| P10 library integrity | OVERWRITE path erases `end()` (UB) and reuses `index = size()`, which collides with an existing fluid (`PCSAFTLibrary.cpp:141-149,173-176`). Schema failures ignored unless debug > 0 and `add_many` errors printed and swallowed (`:31-46`). kij round-tripped through `%0.16g` + `atof` (`:229-234`, `PCSAFTBackend.cpp:99-102`). A missing pair throws instead of defaulting to kij = 0 (8.0.0 oracle: `METHANE&ARGON`) | silent corruption; mixtures without a pair cannot be built | typed immutable DB; explicit default-kij policy |
| P11 data and units rot | water `sigma: -1` sentinel + CAS check (`PCSAFTBackend.cpp:74-77`); range messages disagree with checks (473.16 vs "473.15" and 273 vs "273.15", `PCSAFTFluid.cpp:10-13`); water σ(T) correlation has no cited source (`PCSAFTFluid.cpp:16`); σ documented as "1/Angstrom" (`PCSAFTFluid.h:15`); κᴬᴮ labelled `Angstrom^3` in the JSON; CODATA 2014 e and ε0 mixed with 2019 k_B/N_A (`PCSAFTBackend.h:16-20`); NaN-blind `(Q<0) \|\| (Q>1)` (`:1807,1827`, fixed on master 0f978943) | | enum `Sigma::{Const, TempDependent}`; unit newtypes |
| P12 weak tests | electrolyte bubble pressure passes at 23% (`CoolProp-Tests.cpp:3109`); three tests disabled with "doesn't pass yet" (`:3054,3081,3129`); the two-phase DT test checks p and phase but not Q (`:2769-2775`) | bugs P1-P3 escaped | literature- and AD-based tests |
| G1 GERG absent from the oracle | not in v8.0.0 (c7d6a1aa is after the tag) | TDD needs another oracle | teqp + AGA8 vectors (§8) |
| G2 strictness by guarding inherited mutation | `GERGBackend.h:91-260`: overrides of update, setters and change_EOS, plus "KNOWN BYPASSES": public `Reducing`/`residual_helmholtz`; `update_DmolarT_direct` accepts T = 900 K | complexity; holes remain | immutable model ⇒ strict by construction |
| G3 pure GERG HP/PS/PU flashes fail | HEOS flash depends on `p_triple` (`_HUGE`, `GERGBackend.cpp:1428`) and a 1.5·Tmax bracket; pinned in `CoolProp-Tests-GERG.cpp:2363-2410` | missing functionality | flash independent of fluid metadata |
| G4 tabular wrappers build tables, then reject every lookup | `CoolProp-Tests-GERG.cpp:2450-2487` (cache dir left behind) | wasted work, confusing | capability checks at construction |
| G5 dummy departure for each F = 0 pair | `GERGBackend.cpp:274` + `ExcessHEFunction.h:287-299` evaluates all N(N−1)/2 pairs | 195/210 wasted evaluations for 21-component gases | sparse departure list |
| G6 semantic overloading | triple-point fields set to the 60 K saturation end state (`GERGBackend.cpp:~1689`); ideal gas contorted into generalized Planck–Einstein form (`:1590-1632`) | fragile | GERG ideal gas as its own term type |
| G7 doc rot and size | stale `hs_anchor` justification (`GERGBackend.h:317-327`, removed in 6dc22bf2); 243 comment vs 65 code lines; a 10k-line generated header under `src/` | | data files under `tests/` |

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

| Kernel | CoolProp location | Shape | Fit and side-by-side variant |
|---|---|---|---|
| Cubic αr + 15 derivatives | `GeneralizedCubic.cpp:733`, `CubicBackend.h:459-509` | closed form (1 sqrt per component, 2 logs, divisions) | **SIMD-excellent** across states (SoA batches); scalar reference + lane batch; AD with dual numbers over lanes |
| Cubic mixing a_m, b_m | `.cpp:232-292` | dense O(N²) quadratic form | vectorize over j for large N, otherwise across states; precompute √aᵢᵢ(T) once per state |
| Cubic composition derivatives | `.cpp:402-767` | hundreds of calls, each recomputing b_m and PI_12 | poor as written; one AD gradient pass replaces it |
| Cardano roots | `CubicBackend.cpp:390-412` | closed form with a discriminant branch | SIMD with masked select; root choice (Gibbs) as a separate branchy pass |
| Cubic superancillary | `cubicsuperancillary.h` | binary search over intervals + Clenshaw | evaluation SIMD-friendly; the search is a small branch (could be a fixed-depth search) |
| Pure cubic saturation | `CubicBackend.cpp:414-562` | secant iterations | sequential; replace by superancillary + one Newton polish (O(1), nearly branch-free) |
| PC-SAFT hc + disp | `PCSAFTBackend.cpp:247-349` | O(N²) closed form, degree-6 polynomials | SIMD-good (Horner, fixed tables) |
| PC-SAFT polar | `:351-421` | O(N³) for A3 | SIMD-good per state; cost grows as N³ |
| PC-SAFT association | `:423-483`, `:2898-2981` | fixed-point iteration with data-dependent count | **branchy**; side-by-side: closed forms for pure 1A/2B (branch-free) vs general Newton; masked-lane iteration possible |
| PC-SAFT ion | `:485-511` | closed form with a κ = 0 branch | SIMD with select |
| PC-SAFT density / flashes | `:2749-2863`, `:2084-2573` | grid scans, nested iterations, sweeps | sequential; the grid is embarrassingly parallel but should be replaced, not parallelized |
| GERG pure terms / departures | HEOS GenExp; `ExcessHEFunction.h:287-299` | Σ n·δᵈ·τᵗ·exp(−δᶜ) per component; O(N²) pairs | **SIMD-excellent** over terms (SoA coefficients) and across states; sparse departures (15 pairs) |
| GERG reducing functions | `ReducingFunctions.h:144` | O(N²) with cube roots | precompute pair constants at model build; SIMD over pairs |

Rules for the "side-by-side architectures" goal:
- Write each model formula once, branch-free, over a numeric trait implemented by `f64`, a lane type and dual numbers. Every `if` in a kernel (`kappa != 0`, `A2 != 0`, `x.size()==1`) becomes a select.
- Iterations (XA, roots, flashes) live outside kernels and are parallelized across requests, not within one.
- Stable Rust lacks `std::simd` (nightly-only as far as I know), so start with auto-vectorizable SoA loops. Add explicit SIMD behind a feature flag (x86 AVX2/AVX-512, aarch64 NEON, wasm simd128) and keep a scalar reference implementation for bit-tolerance tests.

## 8. Verification assets (tests, check values, paper tables, oracle hooks)

| Asset | Location | Use |
|---|---|---|
| Alpha `term()` vs `calc_all_terms()` | `src/Tests/CoolProp-Tests-CubicAlpha.cpp` (98 lines) | alpha-function derivative cross-checks |
| Factored `all()` bit-exact vs 15 `alphar()` calls, plus a timing benchmark | `CoolProp-Tests-CubicU.cpp` (116 lines) | kernel regression; performance baseline |
| Numerical vs analytic mixture derivatives for PR/SRK | `MixtureDerivatives.cpp:1670-1677` (`DerivativeFixture`) | port as AD-vs-FD tests |
| Helmholtz term consistency (Soave/PR terms) | `src/Helmholtz.cpp:1409-1432` fixture | derivative checks with T_r = 300, ρ_r = 4000 |
| Superancillary vs EOS flash; DmolarT round trips | `CoolProp-Tests.cpp:4233-4391` | cubic saturation |
| Literature flash benchmarks (Michelsen 1982 I/II, Michelsen & Mollerup 2007, SRK vs PR) | `CoolProp-Tests-Michelsen.cpp` (2,087 lines, 53 cases) | cubic-driven VLE acceptance |
| PC-SAFT p, ρ, h_res, s_res, g_res, p_sat, bubble P/T, kij, phase, association indices | `CoolProp-Tests.cpp:2749-2776, 2858-3159, 3377-3444` (the gaps are unrelated HEOS tests) | values mostly generated by CoolProp/pcsaft itself; DIPPR/experimental checks at 1-2% |
| UNIFAC Poling example (acetone/n-pentane) | `UNIFACLibrary.cpp:141-181` | if UNIFAC is ever revived |
| [master] entropy consistency: ΔS = ∫(∂S/∂T)_P and T(∂S/∂T)_P = cp | `CoolProp-Tests-CubicEntropy.cpp` (78) | catches C1 |
| [master] volume-translation invariances (Jaubert et al. 2016) + FD scaffold for every composition derivative | `CoolProp-Tests-CubicVolumeTranslation.cpp` (1,118) | port wholesale |
| [master] 66 GERG cases (tables, strictness, saturation, reordering invariance) | `CoolProp-Tests-GERG.cpp` | behaviour spec |
| [master] **`GERGReferenceValues.h`**: 288 + 336 pure (T, ρ) points (18/21 fluids × 16) and 153 + 397 mixture points (all 153/210 binaries + 187 AGA8 gases), each with αr, α°, p, cv, w from teqp 0.23.2; NaN only in w on mechanically unstable branches | `src/Backends/GERG/GERGReferenceValues.h` | the GERG golden set. It is 10k lines only because clang-format puts each element of the 21-element name/z vectors on its own line (`dev/gerg/README.md:105-120`). Convert to CSV/JSON test data |
| [master] AGA8 published table (187 rows; P agrees with teqp to a median 3.5e-12 and worst 6.8e-4, a table-provenance issue) | `dev/gerg/_validation_data.py` | independent check |
| [master] table transcription check against teqp `GERG.hpp` | `dev/gerg/verify_transcription.py` | data-entry guard |
| External oracles | teqp (github.com/usnistgov/teqp, NIST public domain; `include/teqp/models/{cubics/,pcsaft.hpp,association/,GERG/GERG.hpp}`; 0.23.2 generated the GERG vectors); FeOs PC-SAFT (MIT/Apache, from memory); Gross & Sadowski 2001/2002 parameter and AAD tables; Kunz & Wagner 2012 | arbiter where CoolProp is wrong |
| Closed-form invariants | at the cubic's own critical point, ∂p/∂ρ = ∂²p/∂ρ² = 0 and Z_c = 1/3 (SRK), 0.30740 (PR); Péneloux invariances; αr/ρ → B2 as ρ → 0 | oracle-free TDD |

Oracle caveats for CoolProp==8.0.0:
- **Cubics.** αr, p, h, u, g, a, cp, cv, w and fugacity are fine for c = 0. Entropy and S-input flashes are **not** (C1; verifier found g and a unaffected). Pure QT saturation is good only to ~1e-6 (C14); use the superancillary (`saturation_ancillary`) as the tighter reference. Do not use 8.0.0 for anything with volume translation (C8) or for `rhomolar_critical` (C7). No transport or surface tension.
- **PC-SAFT.** p(T, ρ), residual h/s/g, the fugacity vector and non-electrolyte VLE are fine. Not usable: two-phase Q (P1), supercritical PT (P2), electrolyte PQ (P3), ρ < 1e-8 mol/m³ (P6), and cp/cv/w (absent).
- **GERG.** Not available; use the teqp vectors and AGA8.

## 9. Port recommendation (units with priority P0-core / P1-early / P2-later / defer / drop; order; what to redesign)

| Unit | CoolProp source | Priority | ~Rust LOC | Rationale |
|---|---|---|---|---|
| U1 Residual-model trait contract for all families: αr(T, ρ, x) generic over the scalar; derivatives exchanged as reducing-invariant A_nm; composition derivatives via moles and AD; explicit R, validity range and reference state | `CubicBackend.h:439-581`; the #3287 site `HelmholtzEOSMixtureBackend.cpp:3684` | P0-core | 300 | the plug-in point; prevents the C1/C2 bug class |
| U2 Cubic seed toolkit: Cardano roots, SRK covolume, PR T(ρ, p) | `CubicBackend.cpp:390-412`; `HelmholtzEOSMixtureBackend.cpp:2821-2833,3078`; `VLERoutines.cpp:1160,2638`; `FlashRoutines.h:167` | P0-core | 200 | the HEOS density and flash seeds depend on it |
| U3 Generic cubic model (vdW/SRK/PR via Δ1/Δ2; Soave/MC/Twu alpha; vdW1f + kij; R per model) | `GeneralizedCubic.*`, `CubicBackend.*` | P1-early | 500 | small and closed-form: the first non-HEOS implementor and SIMD/AD pilot |
| U4 Cubic superancillary (Bell & Deiters 2021) | `cubicsuperancillary.h`, `CubicBackend.cpp:828-919` | P1-early | 150 + data | fast pure-cubic saturation; verifier measured equal-p to 1e-13 and Δg/RT 6e-15 for SRK propane at 300 K (CoolProp's own test only asserts < 1e-3, `CoolProp-Tests.cpp:4261-4263`); replaces the loose secant of C14 |
| U5 Cubic parameter source: Tc, pc, ω, M and ideal gas from the canonical fluid DB, plus user-defined cubic-only components | `CubicsLibrary.*`, `dev/cubics/*` | P1-early | 150 | removes the stale snapshot (C5) |
| U6 GERG-2004/2008 as immutable **datasets** on the shared multi-fluid model (short pure EOS, β/γ, sparse departures, sinh/cosh ideal gas with recomputed constants, R = 8.314472, 60-700 K) | [master] `src/Backends/GERG/*`; HEOS `ReducingFunctions.h:144`, `ExcessHEFunction.h:104-200` | P1-early | 300 + ~1.5k data | key natural-gas use case; little code once the multi-fluid model exists |
| U7 GERG golden-vector fixture + AGA8 check | [master] `GERGReferenceValues.h`, `dev/gerg/_validation_data.py` | P1-early (with U6) | data | 1,174 points from teqp |
| U8 Péneloux translation, per component, master semantics | master 1ace9311 | P2-later | 100 | v8.0.0 is broken; test against the Jaubert invariances |
| U9 PC-SAFT hard chain + dispersion (+ kij(T)) on AD | `PCSAFTBackend.cpp:247-349` | P2-later | 350 | after the trait, AD and generic flash are proven; cross-check with teqp/FeOs |
| U10 Association (Wertheim schemes) with Newton + closed forms | `:423-483,2898-3061` | P2-later | 300 | robust convergence and error reporting |
| U11 Gross–Vrabec dipolar term | `:351-421` | P2-later | 200 | 66 of 180 PC-SAFT fluids use it |
| U12 ePC-SAFT ions, water σ(T), dielectric | `:485-511,3063-3090`; `PCSAFTFluid.cpp` | defer | 250 | niche; re-derive from Held 2014 when electrolytes are in scope |
| U13 VTPR + UNIFAC (gE-based cubic mixing) | `VTPR*`, `UNIFAC*` | defer | 600 | no data shipped, wrong derivatives (V1-V3); re-implement from the literature with AD if activity models come with the "materials" scope |
| D1 `HEOS::X-SRK` / `-PengRobinson` names and `change_EOS` residual swapping | `FluidLibrary.h:1236-1312`; `HelmholtzEOSMixtureBackend.cpp:483-525` | drop | — | replaced by explicit model composition |
| D2 PC-SAFT bespoke flash, density solver and phase-by-exception logic | `PCSAFTBackend.cpp:1886-2863` | drop | — | replaced by the shared VLE |
| D3 GERG guard machinery (range/update overrides, setter throws, bypass catalogue) | `GERGBackend.h:91-260`; `.cpp:97-175` | drop | — | immutability makes it unnecessary; keep validity as data |
| D4 Cubic coordinate hack (T_r = 1, ρ_r = 1), fake components, heuristic limits, Kazakov ρc | `CubicBackend.cpp:73-89`, `CubicBackend.h:166-219` | drop | — | causes C1, C2, C7 and C10 |
| D5 Hand-coded composition derivatives; PC-SAFT 4× derivative copies | `GeneralizedCubic.h:400-672`, `.cpp:402-767`; `PCSAFTBackend.cpp:517-1702` | drop | — | AD; keep at most hand-optimized fast paths verified against AD |

Order: U1 → U2 → U3 + U4 (TDD vs 8.0.0 for αr/p/cp/w/fugacity; entropy vs master or ∫cp/T) → U5 → U6 + U7 (after the HEOS multi-fluid core) → U8 → U9 → U10 → U11 → deferred units.

What to redesign:
- **Trait shape (teqp/FeOs style).** Make `fn alphar<S: Scalar>(&self, t: S, rho: S, x: &[S]) -> S` the only formula entry point. Generic methods are not object-safe, so dispatch built-in families through a closed `enum Residual { MultiFluid, Cubic, PcSaft, .. }`; offer an object-safe facade (`fn derivs(&self, t, rho, x, max_order) -> Derivs`) for plugins. Model = residual + ideal gas + R + validity range + reference state, all held in an immutable `Arc`. States are small values that reference the model.
- **Reducing-invariant exchange.** Exchange A_nm = τⁿδᵐ·∂ⁿ⁺ᵐαr/∂τⁿ∂δᵐ (equal to (1/T)ⁿρᵐ·∂/∂(1/T)ⁿ∂ρᵐ). Each model reduces internally, so no other layer ever needs another model's T_r or ρ_r.
- **One identity registry.** Names, aliases and CAS, case-insensitive, for all families. Per-family parameter tables are keyed by canonical id and loaded per component.
- **Flash and VLE generic over the trait.** They must not read fluid metadata such as triple points or "Tmax·1.5"; bracketing comes from the model (e.g. the covolume or packing-fraction limit).
- **Typed errors and convergence.** Return `Result` with a non-convergence variant; no `throw -1`, no sentinels (`_HUGE`, σ = −1), no silent acceptance at 1e-3.

## 10. Open questions

1. Oracle policy where 8.0.0 is wrong (cubic s/g/a, translation, PC-SAFT Q/PT/electrolytes): use CoolProp master as a secondary oracle, or only analytic consistency plus teqp/FeOs?
2. Should GERG be a user-visible family name (`GERG2008::...`) or a named dataset of the multi-fluid model? Should leaving the validity range be a hard error (CoolProp master) or a flagged result?
3. AD dependency: adopt `num-dual` (MIT/Apache, used by FeOs) or write a minimal in-house dual/hyper-dual to keep dependencies small? What maximum derivative order is needed (CoolProp computes up to 4th in τ/δ)?
4. Should PC-SAFT parameters be ported verbatim from CoolProp's 180-fluid JSON, or taken from FeOs/teqp parameter sets (provenance, licensing, the water σ(T) correlation which has no cited source)?
5. Which ideal gas should attach to PC-SAFT and cubic components when the canonical fluid has several (reference-EOS α0 vs GERG α0)? Precedence rules are needed.
6. Is the Mathias–Copeman supercritical form intended? CoolProp applies the 3-term polynomial above Tc (`GeneralizedCubic.cpp:36-80`); my recollection of the 1983 paper is that only c1 is used above Tc. Verify against the paper.
7. Are electrolytes (ePC-SAFT) and VTPR/UNIFAC wanted at all in the first releases, or should they wait for the "materials" scope?
8. Reference states differ per family: GERG recomputes integration constants for h = s = 0 at 298.15 K / 1 atm, while cubics inherit HEOS α0 offsets. Should the Rust model carry an explicit `ReferenceState`?
9. Accuracy targets per family for TDD: GERG vs teqp at 1e-12 for αr/α° and 1e-10 for p, cv, w (CoolProp's targets, master `CoolProp-Tests-GERG.cpp:586-590` for pure points); cubic vs CoolProp at ~1e-12 except pure QT saturation (~1e-6, C14); PC-SAFT vs CoolProp at ~1e-8, given the iterative association solve.

## Verification log

Date: 2026-10-04. Adversarial verification against the v8.0.0 checkout (ae81610e), origin/master `022b63e4` for [master] items, and the CoolProp==8.0.0 wheel. No partial earlier verification edits were found in this doc (no broken tables, duplicate sections or prior log).

Claims checked: 112 (every path:line citation in §1-§9, every rot item C1-C13, V1-V4, P1-P12, G1-G7, every count in the scope line and in §4, and the oracle numbers for C1, C5, C6, C7, C13, P1, P2, P3, P4, P6 and P10, which were re-run).

Corrections made:
1. Scope LOC: PC-SAFT is 3,772 + 96 = 3,868 (the doc double-counted the header); hand-written total 8.4k, not 8.5k.
2. §1 cubic "Missing outputs: none" was wrong: viscosity, conductivity and surface tension throw for PR::Methane in the 8.0.0 oracle.
3. C1: g and a are not affected by the entropy bug (oracle: −∂g/∂T = (h − g)/T while `smolar` is off); only s and S-input flashes are. The §8 oracle caveat was corrected to match.
4. C4 downgraded: the 18 `throw -1` sites are latent (derivative order > 4 only), and `catch (...)` handlers exist (`CoolPropLib.cpp:52`), so "uncatchable" was overstated.
5. C6 rewritten: k_B·N_A equals `R_U_CODATA` exactly in double, so there are three gas-constant literals (8.3144598, 8.31446261815324, 8.314472) plus per-fluid R, not four constants. The 1.128e-6 offset is the most common one (54 of 97), not "typically exact".
6. V4: −0.53087 appears 10 times, not 7.
7. P3: oracle numbers re-run and made explicit (0.05-0.07 K at x_ion = 0.0106, 0.57-0.81 K at x_ion = 0.091; pure water and a non-electrolyte mixture round-trip to ≤1.6e-7 K). The attribution to the `_T` bug is labelled an inference.
8. P5: the four copies span ~1.45k lines, not "3.1k LOC" (that is the whole file).
9. P7: the XA = 0.02 is a fallback for a non-finite initial guess, not a convergence fallback; added the `abs` calls at `:2854-2855`; the Eigen-include claim was re-verified with g++ and clang++.
10. P9: added outerTQ's acceptance of maxdif ≤ 0.1 (`:2568`).
11. P11: added the 273 vs "273.15" message mismatch and the uncited water σ(T) correlation.
12. P2: strengthened with four more failing cases and its master status.
13. P6: the teqp `pcsaft.hpp:109` citation is marked as unverified (teqp is not checked out locally).
14. Paths fixed: `FluidLibrary.h` → `src/Backends/Helmholtz/Fluids/FluidLibrary.h`; `Helmholtz.cpp` → `src/Helmholtz.cpp`; `configuration_keys.h` → `include/CoolProp/detail/configuration_keys.h` (added `:49` for `R_U_CODATA`).
15. Line ranges fixed: Z `.cpp:1367-1703`; `make_gerg_fluid` `.cpp:1388-1784`; PC-SAFT tests are `CoolProp-Tests.cpp:2749-2776, 2858-3159, 3377-3444`, not one contiguous block to 3428.
16. §5: PC-SAFT JSON is 83 KB. GERG name resolution goes through the HEOS library only for non-canonical spellings (`GERGBackend.cpp:1119-1130`).
17. Added (by verifier) C14, with evidence: pure cubic QT saturation uses `BoundedSecant` with ftol 1e-5 on Δg (`CubicBackend.cpp:544,547`) and is only ~1e-6 accurate (SRK) in the 8.0.0 oracle, while the superancillary is accurate to ~1e-13. Propagated to §1, §3.1, §8 oracle caveats, U4 and open question 9.
18. Open question 9: CoolProp's GERG targets are 1e-12 (αr, α°) and 1e-10 (p, cv, w), not a flat 1e-10.
19. P11 table row: escaped the `||` inside the code span, which split the row into extra cells.

Refuted or downgraded: C4 (severity), C6 (the count of constants), the g/a part of C1, P5's LOC figure. Confirmed unchanged: C2, C3, C5, C7, C8, C9, C10, C11, C12, C13, V1, V2, V3, P1, P2, P4, P8, P10, P12, G1-G7 (G-items checked against `022b63e4`).
