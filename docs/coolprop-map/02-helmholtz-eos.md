# 02 Helmholtz EOS terms, fluid data model and pure-fluid properties - CoolProp v8.0.0 map

> Scope: `src/Helmholtz.cpp`, `include/CoolProp/fluids/{Helmholtz,Ancillaries,IdealCurves,MeltingCaloric}.h`, `include/CoolProp/CoolPropFluid.h`, `src/Backends/Helmholtz/Fluids/{FluidLibrary.h,FluidLibrary.cpp,FluidLibraryFactories.h,Ancillaries.cpp}`, `src/Backends/Helmholtz/MeltingCaloric.cpp`, the pure-fluid parts of `src/Backends/Helmholtz/HelmholtzEOS{Mixture,}Backend.{h,cpp}` and `src/AbstractState.cpp`, `dev/fluids/*.json`, `dev/pseudo-pure/`. 14.0k lines of C++ in the listed files: 7.0k in the term/data/ancillary/melting/HEOS-wrapper files plus 7.0k in `HelmholtzEOSMixtureBackend.{h,cpp}` + `AbstractState.cpp`, of which only the pure-fluid parts are in scope; 136 active fluid JSON files (+2 `*.json_disabled`) = 9.64 MB compact JSON, of which 8.66 MB (90 %) is `SUPERANCILLARY`; 771 lines of Python/Cython in `dev/pseudo-pure`. Part of the coolprop-rs port plan; cites the v8.0.0 source (ae81610e).
>
> **Method.** Primary files read in full (the backend only for its pure-fluid parts); behaviour checked against the `CoolProp==8.0.0` wheel (the oracle). I wrote an independent, value-only Python evaluator of the JSON formulas in §3.1-3.2 (scratchpad, ~120 lines, easy to rebuild). With exact summation (`math.fsum`) it reproduces the oracle's α^r for all 136 default EOSs at 3 states each to ≤ 5.8e-14 relative (worst: R21 at δ = 0.05) and α⁰ to ≤ 9.3e-15. Generic 40-digit numerical differentiation (`mpmath.diff`) of that value-only α^r reproduces all 15 hand-coded derivatives (orders 0-4) for Water and CO2 (non-analytic terms), Ammonia (GaoB), Methanol (double exponential), R125 (Lemmon2005) and n-Propane to ≤ 3.4e-13. Inferences are marked *(inference)*.
>
> **File count.** v8.0.0 has 136 `*.json` (159 EOS blocks) + `AceticAcid.json_disabled` + `R407F.json_disabled` = 138 files; origin/master adds R-1132a (ec2e75d5), giving the brief's 139.

## 1. Purpose and concepts

- **Helmholtz-explicit multiparameter EOS.** α(τ,δ) = a/(RT) = α⁰(τ,δ) + α^r(τ,δ), τ = T_r/T, δ = ρ/ρ_r. Every equilibrium property is an algebraic function of α and its partial derivatives (§3.4).
- **Reducing state ≠ critical point.** (T_r, ρ_r) is an EOS parameter. 15 default EOSs have a reducing state different from JSON `STATES.critical` (e.g. Methanol T_r 513.38 K vs 512.5 K; Air T_r 132.6312 K vs 132.5306 K). 25 fluids have a JSON critical point that is not the EOS's own critical point (§3.6). CoolProp mixes the three notions; the port must type them separately.
- **Term families.** α^r and α⁰ are sums of typed term blocks (`"type"` in JSON). The loader folds them into a small, fixed set of C++ evaluator objects (one generalized-exponential container holds 99.7 % of all default residual terms, 2525 of 2532; GaoB has its own evaluator, and the separable families including GaoB are 99.8 %).
- **Derivatives.** Hand-coded per family up to 4th order (15 (i,j) pairs with i+j ≤ 4). The association term stops at 3rd order. Ideal-gas terms are τ-only plus ln δ.
- **Fluid record.** One JSON per fluid: `INFO` (identity), `STATES` (critical, triple), `EOS[]` (≥1; only `[0]` is reachable), `ANCILLARIES` (saturation, caloric, surface tension, melting) and optional `TRANSPORT` (doc 05). Each EOS carries its own gas constant (10 distinct values, 8.3143-8.31451 J/(mol·K)), molar mass, limits, reducing and sat-min states and an optional superancillary (doc 08).
- **Pure vs pseudo-pure.** Air, R404A, R407C, R410A, R507A and SES36 are blends fitted as single-component EOSs: separate bubble/dew ancillaries (`pL`, `pV`), `max_sat_T`/`max_sat_p` states, no superancillary, `is_pure()` false.
- **Reference states.** h and s are shifted by α⁰ += a1 + a2·τ: a JSON "core" offset (the published default) plus a user offset (IIR, ASHRAE, NBP, DEF, custom).
- **The central redesign.** In CoolProp a "fluid" is mutable data with embedded caches, deep-copied into every `AbstractState`. The port should compile an immutable model shared by `Arc`, with derivative values living in per-state values outside it.

## 2. Structure (key types/functions -> path:line)

| Item | Location | Role |
|---|---|---|
| `HelmholtzDerivatives` (15 derivatives, 5 unused `*_x_*` products, τ, δ, T_red, ρ_red; `get(itau,idelta)` switch) | include/CoolProp/fluids/Helmholtz.h:40-146 (`get` :91-145) | derivative value bag |
| `BaseHelmholtzTerm` (pure-virtual `all()`, 15 per-derivative virtual getters that each run `all()`, `one_mcx` test hook) | Helmholtz.h:175-307 | term interface |
| `ResidualHelmholtzGeneralizedExponential` ("GenExp": `add_Power/Exponential/Gaussian/GERG2008Gaussian/Lemmon2005/DoubleExponential`, `finish()`) | Helmholtz.h:309-554; `all()` src/Helmholtz.cpp:138-292 | n·δ^d·τ^t·e^u families |
| `ResidualHelmholtzNonAnalytic` | Helmholtz.h:556-598; Helmholtz.cpp:350-557 | IAPWS-95 / Span-Wagner critical terms |
| `ResidualHelmholtzGaoB` | Helmholtz.h:622-649; Helmholtz.cpp:629-717 | Gao et al. (2020) ammonia terms |
| `ResidualHelmholtzSAFTAssociating` | Helmholtz.h:673-745; Helmholtz.cpp:784-1064 | association term (3rd order only) |
| `ResidualHelmholtzGeneralizedCubic`, `ResidualHelmholtzXiangDeiters` | Helmholtz.h:600-620, 651-671; Helmholtz.cpp:579-603, 732-776 | cubic as α^r (`-SRK` suffix, `change_EOS`); generalized Lee-Kesler |
| `BaseHelmholtzContainer` (16-slot cache, cached getters) | Helmholtz.h:747-843 | per-container cache |
| `ResidualHelmholtzContainer` (6 fixed members), `IdealHelmholtzContainer` (10 fixed members, `_prefactor`, `set_Tred`) | Helmholtz.h:845-887, 1379-1455 | α^r / α⁰ aggregates |
| Ideal-gas terms: Lead, EnthalpyEntropyOffset, LogTau, Power, PlanckEinsteinGeneralized, CP0Constant, CP0PolyT, GERG2004Sinh/Cosh | Helmholtz.h:897-1246; Helmholtz.cpp:1083-1341 | α⁰ |
| `EquationOfState` (reduce, sat_min_{liquid,vapor}, hs_anchor, max_sat_{T,p}, limits, R_u, M, ω, Ttriple/ptriple, α^r, α⁰, superancillary string + `shared_ptr`, forwarding accessors) | include/CoolProp/CoolPropFluid.h:400-521 (forwarders :454-520) | one EOS |
| `CoolPropFluid` (`EOSVector`, `EOS()` = `[0]`, identity, ancillaries, transport, crit, triple states) | CoolPropFluid.h:527-573 (`EOS()` :539-545) | fluid record |
| `EOSLimits {Tmin,Tmax,rhomax,pmax}`; `CriticalRegionSplines` | CoolPropFluid.h:111-114; 40-108 | limits; legacy near-critical saturation fallback |
| `JSONFluidLibrary::parse_alphar/parse_alpha0/parse_EOS/parse_melting_line/parse_states/parse_ancillaries` | src/Backends/Helmholtz/Fluids/FluidLibrary.h:42-168, 171-334, 350-450, 1015-1068, 1071-1119, 1122-1174 | JSON → objects |
| `JSONFluidLibrary::get(key)` (by value; `-SRK`/`-PengRobinson` suffix) / `get(index)` | FluidLibrary.h:1229-1317 / 1323-1332 | lookup |
| global `static JSONFluidLibrary library`; `call_once` CBOR load; `add_one`; `set_fluid_enthalpy_entropy_offset` | Fluids/FluidLibrary.cpp:28-62, 150-369, 64-127 | registry |
| `make_saturation_ancillary`, `make_surface_tension_correlation` | Fluids/FluidLibraryFactories.h:26-73 | ancillary factories |
| `SaturationAncillaryFunction`, `SurfaceTensionCorrelation`, `MeltingLineVariables` | include/CoolProp/fluids/Ancillaries.h:25-290; Fluids/Ancillaries.cpp:26-262 | ancillaries, melting line |
| `MeltingCaloric` + process-global `get_melting_caloric_cached` | MeltingCaloric.h:20-106; src/Backends/Helmholtz/MeltingCaloric.cpp:18-265 | HS-flash seeds (doc 03) |
| `CurveTracer` + Ideal/Boyle/JouleInversion/JouleThomson tracers | include/CoolProp/fluids/IdealCurves.h:8-137 | ideal curves |
| `calc_all_alphar_deriv_cache`, `calc_alphar_deriv_nocache`, `calc_alpha0_deriv_nocache`, `calc_all_alpha0_derivs_nocache`, `calc_alphar/alpha0` accessors | src/Backends/Helmholtz/HelmholtzEOSMixtureBackend.cpp:3552-3831 | backend derivative plumbing |
| `ResidualHelmholtz::all` = `CorrespondingStatesTerm::all` + `ExcessTerm::all` | HelmholtzEOSMixtureBackend.h:849-876, 694-706 | pure fluids also go through the mixture path |
| `calc_pressure/hmolar/smolar/umolar/cvmolar/cpmolar/cpmolar_idealgas/speed_sound/gibbsmolar/helmholtzmolar/fugacity_coefficient/chemical_potential/PIP` | HelmholtzEOSMixtureBackend.cpp:3151-3532 | property formulas |
| `calc_Bvirial/dBvirial_dT/Cvirial/dCvirial_dT` | HelmholtzEOSMixtureBackend.cpp:1683-1698 | virials at δ = 1e-12 |
| `calc_T_critical/p_critical/rhomolar_critical`, `calc_Tmax/Tmin/pmax`, `calc_Ttriple/p_triple`, `calc_pmax_sat/Tmax_sat`, `get_fluid_constant` | HelmholtzEOSMixtureBackend.cpp:1197-1325, 1095-1108; .h:310-334 | constants and limits |
| `set_reference_stateS/D`, `set_fluid_enthalpy_entropy_offset` (member) | HelmholtzEOSMixtureBackend.cpp:4463-4597 (copies: src/CoolProp.cpp:946-1039; FluidLibrary.cpp:64-127) | reference states |
| `get_dT_drho`, `get_dT_drho_second_derivatives`, `calc_first/second_partial_deriv`, κ_T, α_p, Γ, `neff` | src/AbstractState.cpp:981-1151, 1152-1237, 1239-1260, 950-978, 728-735 | generic partial derivatives (Thorade & Saadat 2013) |
| `HelmholtzEOSBackend` (pure constructor; predefined mixtures) | src/Backends/Helmholtz/HelmholtzEOSBackend.h:30-68 (`.cpp` holds only includes) | factory target |

## 3. Algorithms and formulas (cite the papers CoolProp cites)

### 3.1 Residual term catalogue (α^r = Σ φ_k)

Counts: "default" = EOS[0] (the only one used at runtime); "all" = every EOS block in the 136 active files.

| JSON `type` | φ (as implemented) | Params | Default: fluids / terms | All: files | C++ evaluator |
|---|---|---|---|---|---|
| ResidualHelmholtzPower | n δ^d τ^t · e^(−δ^l) if l > 0; n δ^d τ^t if l = 0 | n, d, t, l | 134 / 1966 | 135 | GenExp (`add_Power`, c = 1 iff l > 0, Helmholtz.h:386-389) |
| ResidualHelmholtzGaussian | n δ^d τ^t e^(−η(δ−ε)² − β(τ−γ)²) | n, d, t, eta, epsilon, beta, gamma | 78 / 433 | 78 | GenExp |
| ResidualHelmholtzExponential | n δ^d τ^t e^(−g δ^l) (term absent if l = 0) | n, d, t, g, l | 10 / 100 (Fluorine, Methanol, Propyne, R1130(E), R114, R1243zf, R13, R14, R21, RC318) | 10 | GenExp |
| ResidualHelmholtzLemmon2005 | n δ^d τ^t e^(−δ^l − τ^m), each part omitted when its exponent is 0 (Lemmon & Jacobsen 2005) | n, d, t, l, m | 1 / 18 (R125; 15 terms m = 0, 5 terms l = 0) | 1 | GenExp |
| ResidualHelmholtzDoubleExponential | n δ^d τ^t e^(−g_d δ^(l_d) − g_t τ^(l_t)) (de Reuck & Craven 1993; g_t < 0) | n, d, t, gd, ld, gt, lt | 1 / 8 (Methanol) | 1 | GenExp |
| ResidualHelmholtzGaoB | n τ^t e^(1/(b + β(τ−γ)²)) · δ^d e^(+η(δ−ε)²); JSON η is negative, sign flipped vs the paper (Helmholtz.cpp:1435) | n, t, d, eta, beta, gamma, epsilon, b | 1 / 2 (Ammonia, Gao-JPCRD-2020) | 1 | GaoB |
| ResidualHelmholtzNonAnalytic | n Δ^b δ ψ; Δ = θ² + B[(δ−1)²]^a; θ = (1−τ) + A[(δ−1)²]^(1/(2β)); ψ = e^(−C(δ−1)² − D(τ−1)²) (Wagner & Pruß 2002; Span & Wagner 1996) | n, a, b, beta, A, B, C, D | 2 / 5 (Water 2, CO2 3) | 2 | NonAnalytic |
| ResidualHelmholtzAssociating | a·m·(ln X − X/2 + ½); X = 2/(1 + √(1 + 4Δ̄δ)); Δ̄ = g(η)(e^(ε̄τ) − 1)κ̄; g = ½(2−η)/(1−η)³; η = v̄n·δ | a, m, epsilonbar, vbarn, kappabar | **0** (Methanol EOS[1] `Piazza-FPE-2013`, disabled AceticAcid) | 1 | SAFTAssociating |
| (code only) GERG2008Gaussian | n δ^d τ^t e^(−η(δ−ε)² − β(δ−γ)) | | 0 (mixture departure functions, doc 04) | – | GenExp |
| (code only) GeneralizedCubic; XiangDeiters | SRK/PR α^r (doc 06); φ0 + ω φ1 + θ φ2, θ = (Z_c − 0.29)², fixed 14-term tables (Xiang & Deiters 2008, doi:10.1016/j.ces.2007.11.029) | – | 0 (`-SRK`, `change_EOS`) | – | Cubic / XiangDeiters |

GenExp's internal form (Helmholtz.h:344-351): φ = n δ^d τ^t e^u, u = −c δ^l − ω τ^m − η₁(δ−ε₁) − η₂(δ−ε₂)² − β₁(τ−γ₁) − β₂(τ−γ₂)².

Data facts that drive the design (default EOSs, 2532 residual terms):
- All d (0..15) and l (0..6) are integer-valued; 37 % of t are integers; t ∈ [−1, 50]; median 14 distinct t per fluid (on average 82 % of a fluid's t values are distinct, so there is little to share). Terms per fluid: min 12, median 16, max 54 separable (Water 56 incl. 2 non-analytic).
- *(added by verifier)* Integer-valued is not integer-typed: across all 159 EOS blocks 277 `d` and 161 `l` values are stored as JSON floats (`1.0`), the rest as ints. A serde `Vec<u8>` field rejects them, so the loader must accept integral floats and validate them.
- 930 pure polynomial terms (no e^u needed), 1164 other exponential terms (1036 Power with l > 0, 100 Exponential, 18 Lemmon2005, 8 DoubleExponential, 2 GaoB; ≤ 6 distinct l per fluid), 433 Gaussians, 5 non-analytic.
- **Separability:** every default term except the 5 non-analytic ones has the form n·F(τ)·G(δ) = n·exp(Φ_τ(τ) + Φ_δ(δ)). The association term is also non-separable.
- **"0 means absent" conventions:** l = 0 or m = 0 disables that exponent part (Helmholtz.cpp:169, 184); e^(−δ⁰) would otherwise inject a factor e^(−1). The independent evaluator only matches the oracle with this convention.
- d = 0 occurs in 57 terms (CycloPropane, Propyne, R114, R123, R13, R14, R152A, R21, RC318: MBWR conversions; Methane: 3 Gaussians). The MBWR pairs n τ^t(1 − e^(−γδ²)) are stored as two terms, so α^r at low δ is a small difference of O(10) numbers.
- All Gaussian η, β ≥ 0 (bounded); GaoB b > 0 (no pole).

### 3.2 Ideal-gas term catalogue (α⁰ = Σ ψ_k)

| JSON `type` | α⁰ contribution (as evaluated) | Default: fluids / terms | Notes |
|---|---|---|---|
| IdealGasHelmholtzLead | ln δ + a1 + a2 τ | 136 / 136 | the only δ-dependent α⁰ term |
| IdealGasHelmholtzLogTau | a ln τ | 136 / 136 | |
| IdealGasHelmholtzPlanckEinstein | Σ n ln(1 − e^(−θτ)) (JSON `t` = θ; loader flips sign into the generalized form, FluidLibrary.h:218-233) | 107 / 370 | |
| IdealGasHelmholtzPlanckEinsteinFunctionT | Σ n ln(1 − e^(−(v/T_crit)τ)) | 7 / 31 (Hydrogen, OrthoH2, ParaH2, Methane, Methanol, Nitrogen, R1336mzz(Z)) | `Tcrit` duplicates T_r (all equal in 8.0.0); keys `R`, `T0` ignored |
| IdealGasHelmholtzPlanckEinsteinGeneralized | Σ n ln(c + d e^(θτ)) | 2 / 2 (Air θ = +87.31, c = 2/3; Fluorine) | Air: e^(θτ) overflows below 16.3 K (T_min 59.75 K) |
| IdealGasHelmholtzEnthalpyEntropyOffset | a1 + a2 τ, tag `reference` | 55 / 55 (IIR 27, NBP 24, OTH 3, CUSTOM 1) | stored as the "core" offset (FluidLibrary.h:323-327) |
| IdealGasHelmholtzPower | Σ n τ^t | 17 / 36 | |
| IdealGasHelmholtzCP0PolyT (c_p⁰/R = Σ c T^t) | t = 0: c(1 − τ/τ₀ + ln(τ/τ₀)); t = −1: (cτ/T_c) ln(τ₀/τ) + (c/T_c)(τ − τ₀); else −c T_c^t τ^(−t)/(t(t+1)) − c T₀^(t+1) τ/(T_c(t+1)) + c T₀^t/t; τ₀ = T_c/T₀ | 21 / 60 (n-Undecane uses t = −2, −1) | Helmholtz.cpp:1188-1258; correct only if the block's `Tc` = T_r (R123 violates it, §6) |
| IdealGasHelmholtzCP0Constant | c(1 − τ/τ₀ + ln(τ/τ₀)) | 4 / 4 (HFE143m, R11, R123, R22) | |
| IdealGasHelmholtzCP0AlyLee (Aly & Lee 1981) | c_p⁰/R = A + B[(C/T)/sinh(C/T)]² + D[(E/T)/cosh(E/T)]² → A into CP0PolyT(t = 0); B → n = B, θ = −2C/T_c, c = 1, d = −1; D → n = −D, θ = −2E/T_c, c = 1, d = 1 | 2 fluids / 3 blocks (D6; n-Heptane has two) | converted at load, FluidLibrary.h:288-322; the comment at :305 contradicts the code |
| (code only) GERG2004Cosh / Sinh (Jaeschke & Schley 1995) | −n ln\|cosh(θ'τ)\|, n ln\|sinh(θ'τ)\|, θ' = θ·T_c/T_red | 0 | used by upstream GERG backends (c7d6a1aa) |

Global multiplier `_prefactor` (Helmholtz.h:1382, 1398-1407) comes from JSON `alpha0_prefactor`, which no fluid uses.

### 3.3 How derivatives are computed

**GenExp: one exp per term plus the "B-factor" recurrence** (Helmholtz.cpp:138-292). Write φ = n·exp(Φ), Φ = t ln τ + d ln δ + u_τ(τ) + u_δ(δ). Then δ^k ∂^k_δ e^(Φ_δ) = e^(Φ_δ)·B_δk with

  B_δ1 = d + δ u_δ′,  B_δ(k+1) = δ ∂_δ B_δk + (B_δ1 − k)·B_δk  (k = 1..3; code :230-240),

and the same in τ (:242-252). Separability gives all 15 scaled mixed derivatives as τ^i δ^j ∂^(i+j) φ = φ·B_τi·B_δj (:254-272). The loop then divides by τ^i δ^j (:274-289). Per term: 1 `exp` of the summed exponent (:228; avoids overflow of the separate factors), optional `powInt`/`exp` for δ^l and τ^m (:170, 185), ~60 flops. This is "exponential of a Taylor jet" (Faà di Bruno in log variables) and is the trick worth keeping.

**Everything else is closed-form by hand.** NonAnalytic: Δ^b and ψ derivative chains to 4th order (Helmholtz.cpp:370-555, colour-coded comments :482-490). GaoB: SymPy output to 4th order (:605-716). Association: 3rd order only; `all()` never writes 4th-order slots (:1031-1064), getters return `1e99` (Helmholtz.h:728-742), the test skips them (Helmholtz.cpp:1767-1769). Ideal-gas terms: τ-derivatives to 4th order; mixed α⁰ derivatives are identically 0.

**Evaluation path for one property** (pure fluid): `AbstractState::dalphar_dDelta()` (include/CoolProp/AbstractState.h:1611-1614) → `calc_dalphar_dDelta` → `calc_all_alphar_deriv_cache` (HelmholtzEOSMixtureBackend.cpp:3552-3572) → `ResidualHelmholtz::all` = `CS.all` (Σ x_i·EOS_i.alphar.all, ×1.0 for a pure fluid) + `Excess.all` (zero) + 5 unused `*_x_*` products (HelmholtzEOSMixtureBackend.h:866-876, 697-706) → `ResidualHelmholtzContainer::all` → six fixed member `all()`s (Helmholtz.h:864-871).

**Caching.**
- α^r: the first derivative request computes and caches all 15 at the AbstractState level (cpp:3552-3572).
- α⁰: **cached one value at a time, never as a set.** The AbstractState caches each α⁰ derivative after it is first requested (e.g. `alpha0()` include/CoolProp/AbstractState.h:1556-1558). But each distinct derivative's `calc_dalpha0_*` (cpp:3792-3831) runs `calc_alpha0_deriv_nocache` → container `dTau()` etc. → a full `all(…, false)` (Helmholtz.h:780-785), so h + c_p + w cost several full α⁰ passes. No caller ever passes `cache_values = true` to `alpha0.all`, so the α⁰ container cache is dead code. `calc_smolar` does one more full pass via `calc_all_alpha0_derivs_nocache` (cpp:3250).
- The container cache (Helmholtz.h:750-763) is not keyed on (τ, δ): `base(tau, delta)` returns `cache[i00]` regardless of its arguments when valid (:768-773). Correctness relies on `clear()` at every update (HelmholtzEOSMixtureBackend.h:154-162).
- No caching of τ/δ powers: ln τ, ln δ, 1/δ, 1/τ once per call (Helmholtz.cpp:146-147); a profiling harness for sharing pow(δ,l)/pow(τ,m) exists but was not adopted (src/Tests/CoolProp-Tests-TermCacheProfile.cpp:1-25).

**Orders actually consumed.** α^r: ≤ 2 for p, h, s, c_v, c_p, w; 3 for first/second partial derivatives (AbstractState.cpp:981-1237); 4th in δ for the p,T density solve (HelmholtzEOSMixtureBackend.cpp:2787-2819); mixed 4th order for mixture critical points (MixtureDerivatives.cpp:513-584). α⁰: ≤ 3; the backend throws above that (cpp:3615-3618), so the 4th-order α⁰ work done in every `all()` is never usable.

**Verification hook.** Value-only `one_mcx` (multicomplex step) exists for GenExp, NonAnalytic, GaoB, XiangDeiters, CP0PolyT, GERG2004 (e.g. Helmholtz.cpp:294-347); the fixture compares analytic vs mcx/FD at τ = 1.3, δ = 0.9 with tolerance 1e-9 (1e-7 for CP0PolyT/GERG2004) (:1761-1817). That value-only path, made primary, is the DRY design for Rust.

**Hand-written derivative code volume** (non-blank, non-comment lines, measured; the verifier reproduced GenExp 130, allEigen 91 and tests 390 exactly, but got NonAnalytic 182, GaoB 80, SAFT 269 and `get_dT_drho*` 221 with a plain blank/`//` filter, so treat the rest as ±15 %):

| Region | Lines |
|---|---|
| Per-term derivative math: GenExp 130, NonAnalytic 156, GaoB 62, SAFT 249, ideal-gas 227, cubic 20, XiangDeiters 36 | **880** |
| Plumbing: `HelmholtzDerivatives` 105, per-derivative virtual getters 87, container cached getters 93, container `all()`+cache 106, `EquationOfState` forwarders 60, backend accessors 100, nocache dispatch 149 | **700** |
| Hand-derived property partials `get_dT_drho*` (AbstractState.cpp:981-1237) | **201** |
| Multicomplex test hooks `one_mcx` | 138 |
| Dead/commented (allEigen 91, Aly-Lee 161, `kahanSum/wayToSort/ramp` 21) | 273 |
| Term-level tests (Helmholtz.cpp:1402-1819) | 390 |

≈ 1.8k lines (880 + 700 + 201) are replaceable by value-only term expressions (~250 lines for all families), one jet type (~300) and generic Jacobian property partials (~100). The verification above shows generic differentiation reproduces the hand-coded values to ≤ 3.4e-13.

### 3.4 Pure-fluid property formulas (R = EOS gas constant, M = molar mass)

Notation: subscripts are partial derivatives; X ≡ 1 + δα^r_δ − δτα^r_δτ, Y ≡ 1 + 2δα^r_δ + δ²α^r_δδ, W ≡ τ²(α⁰_ττ + α^r_ττ).

| Property | Formula | Where |
|---|---|---|
| p, Z | p = ρRT(1 + δα^r_δ); Z = 1 + δα^r_δ | HelmholtzEOSMixtureBackend.cpp:3151-3168; Z via AbstractState.cpp:971-973 → HelmholtzEOSMixtureBackend.h:657 |
| u, h | u = RTτ(α⁰_τ + α^r_τ); h = RT[1 + τ(α⁰_τ + α^r_τ) + δα^r_δ] | cpp:3267-3308, 3170-3216 |
| s | R[τ(α⁰_τ + α^r_τ) − α⁰ − α^r] | cpp:3217-3266 |
| g, a | g = RT(1 + α⁰ + α^r + δα^r_δ); a = RT(α⁰ + α^r) | cpp:3392-3428, 3447-3469 |
| c_v, c_p, c_p⁰ | c_v = −RW; c_p = c_v + R X²/Y; c_p⁰ = R(1 − τ²α⁰_ττ) | cpp:3309-3356 |
| w | w² = (RT/M)(Y − X²/W) | cpp:3357-3390 |
| (∂p/∂ρ)_T, (∂p/∂T)_ρ | RTY; ρRX | AbstractState.cpp:998-1003 |
| any (∂A/∂B)_C | (A_T C_ρ − A_ρ C_T)/(B_T C_ρ − B_ρ C_T) over closed-form A_T, A_ρ for p, h, s, u, g, c_v, c_p, w (Thorade & Saadat 2013, doi 10.1007/s12665-013-2394-z) | AbstractState.cpp:981-1260 |
| κ_T, α_p, κ_s | (1/ρ)(∂ρ/∂p)_T; −(1/ρ)(∂ρ/∂T)_p; (ρ/p)(∂p/∂ρ)_s | AbstractState.cpp:950-958 |
| Joule-Thomson | no dedicated output; via `first_partial_deriv(iT, iP, iHmolar)`. Closed form μ_JT = −(δα^r_δ + δ²α^r_δδ + δτα^r_δτ)/[ρR(X² − WY)] matches the generic path to 1 ulp (oracle, 3 fluids) | – |
| Γ (fundamental derivative) | 1 + (ρ/(2w²))(∂²p/∂ρ²)_s ("Colonna, FPE, 2010, Eq. 1") | AbstractState.cpp:975-978 |
| PIP (Venkatarathnam & Oellrich 2011) | 2 − ρ[(∂²p/∂ρ∂T)/(∂p/∂T)_ρ − (∂²p/∂ρ²)_T/(∂p/∂ρ)_T] | HelmholtzEOSMixtureBackend.h:603-608 (duplicate cpp:3527-3532) |
| B, C, dB/dT, dC/dT | B = α^r_δ/ρ_r, C = α^r_δδ/ρ_r² at **δ = 1e-12**, d/dT via dτ/dT = −T_r/T² | cpp:1683-1698 |
| neff | −3(δα^r_δ − τδα^r_δτ)/(τ²α^r_ττ) | AbstractState.cpp:728-735 |
| ln φ (pure) | α^r + δα^r_δ − ln(1 + δα^r_δ) (via mixture code) | cpp:3470-3482 |
| μ (chemical potential) | RT[α⁰(τ·T_c/T_r, δ·ρ_r/ρ_c) + 1 + ln x + ∂(nα^r)/∂n] with **critical** constants (bug, §6) | cpp:3502-3526 |
| residual / ideal-gas parts | h^r, s^r, g^r; h⁰ = RT(1 + τα⁰_τ), s⁰ = R(τα⁰_τ − α⁰) | AbstractState.cpp:700-726 |
| Ideal curves | Z = 1; Boyle (∂Z/∂v)_T = 0; Joule inversion (∂Z/∂T)_v = 0; JT inversion (∂Z/∂T)_p = 0; Brent on a circle in (ln T, ln p) from fixed start (1e5 Pa, 800 K) | IdealCurves.h:86-137; cpp:1128-1144 |
| Two-phase h, s, u, g, a | quality-weighted SatL/SatV values; w throws unless Q ∈ {0,1}; c_p, c_v are **not** phase-checked (bug, §6) | cpp:3188-3197, 3234-3243, 3358-3365, 3406-3409 |

General reference for these relations: Span (2000), *Multiparameter Equations of State*; CoolProp itself: Bell et al., IECR 53 (2014) 2498.

### 3.5 Reference states

- α⁰ = Lead(a1, a2) + core offset (JSON) + user offset (`IdealHelmholtzEnthalpyEntropyOffset`, Helmholtz.h:930-981). Changing h, s by Δh, Δs: Δa1 = Δs/R, Δa2 = −Δh/(R·T_r) (molar; mass-based codes use R/M).
- Targets (identical code in three places: HelmholtzEOSMixtureBackend.cpp:4463-4522, CoolProp.cpp:946-1026, plus the h/s recomputation in FluidLibrary.cpp:64-127 and cpp:4547-4597):
  - IIR: h = 200 kJ/kg, s = 1 kJ/(kg·K), saturated liquid at 273.15 K (requires "Ttriple" ≤ 273.15, which is really sat-min T, §3.6).
  - ASHRAE: h = s = 0, saturated liquid at 233.15 K. NBP: h = s = 0, saturated liquid at 101 325 Pa.
  - DEF: zero the user offset. RESET: `set(0,0)` adds 0 to an enabled offset → **no-op** (Helmholtz.h:944-961; oracle confirmed). Custom: `set_reference_stateD(T, ρ, h0, s0)`.
- Side effects: hs_anchor, reducing, critical and both triple-state h/s are recomputed; Water and CO2 use a name-based 1.00001 step-off from the non-analytic singularity (FluidLibrary.cpp:91; cpp:4565). Caloric superancillaries are not rebuilt; a closed-form shift is applied (#2773, Helmholtz.h:967-978).
- Oracle facts: default n-Propane gives h(273.15 K, Q=0) = 199 999.9939 J/kg, s = 999.99997 J/(kg·K) (published offsets ≠ exact IIR); after explicit IIR 200 000.0000. The global `set_reference_state` changes only states constructed afterwards; a pre-existing state keeps the old values (CoolProp-Tests.cpp:2328-2336 codifies this).

### 3.6 Constants, limits, critical and triple points

- **Two critical points coexist.** `T_critical()/p_critical()/rhomolar_critical()` return the superancillary's numerical critical point when `ENABLE_SUPERANCILLARIES` is on and the fluid is pure (cpp:1197-1253); `get_fluid_constant(iT_critical)`, `get_state("critical")`, the mixture α⁰ and μ use JSON `STATES.critical` (HelmholtzEOSMixtureBackend.h:315-322). Oracle, R13: `PropsSI('Tcrit')` = 303.04991 K, `get_fluid_constant` = 301.88 K, and turning superancillaries off changes `T_critical()` to 301.88 K. 25 fluids differ by > 0.01 K or > 0.1 % in ρ_c, e.g. R40 416.3 vs 418.63 K, R114 418.83 vs 420.61, MDM 564.09 vs 565.36, R21, R13, n-Heptane, DiethylEther (> 1 K), Methanol 512.5 vs 513.38, O2 ρ_c 13630 vs 13342.
- **"Ttriple" is sat-min.** `limits.Tmin`, `Ttriple`, `ptriple` = `sat_min_liquid` (FluidLibrary.h:384-386; `iT_triple`, `iP_triple` .h:327-330); the JSON EOS key `Ttriple` is ignored. 16 fluids differ: R114 273.15 vs 180.63 K, CycloPropane 273 vs 145.7, Propyne 273 vs 170.5, R124 120 vs 75, R21 200 vs 142.8.
- **Limits.** T_min = sat-min T; T_max, p_max from JSON; `EOSLimits.rhomax` never set or read (cpp:2984 comment: "unreliable"). Mixture limits are mole-fraction averages (cpp:1305-1325). Pseudo-pure T_max,sat / p_max,sat from `max_sat_T/p` (cpp:1254-1280).
- **Exact critical point of the EOS** (superancillary, doc 08): Water 647.0959999999873 K, 17873.72794440601 mol/m³ vs JSON 647.096, 17873.72799560906.

### 3.7 Ancillaries, melting, sublimation, pseudo-pure, identity

- **Saturation ancillaries** (Ancillaries.h:66-178; Ancillaries.cpp:43-113): `rhoLnoexp`: y = y_r(1 + Σ nθ^t); exponential types (`rhoV`, `pL`, `pV`, `pS`): y = y_r exp((T_r/T if `using_tau_r`)·Σ nθ^t), θ = 1 − T/T_r, NaN above T_r (:61-63). `rational_polynomial` (hL, hLV, sL, sLV): ΣA_iT^i/ΣB_iT^i via Eigen `Polynomial2D` (:47-48), stored **relative to hs_anchor** (VLERoutines.cpp:215, 276) so they survive reference-state changes; `max_abs_error` sets phase-determination error bands (cpp:1906-1944). `invert`: Brent on [T_min − 0.01, T_max], then extrapolating secant (:99-112). Coverage: rhoL/rhoV 136; pS 130; pL+pV 6; hL/hLV/sL/sLV 110; ~50 lines calling `ancillaries.{rhoL,rhoV,pL,pV}` in HelmholtzEOSMixtureBackend.cpp, FlashRoutines.cpp and VLERoutines.cpp (initial guesses, phase determination, pseudo-pure saturation). All 130 pure default EOSs also have superancillaries.
- **Surface tension** (108 fluids): σ = Σ a_i(1 − T/T_c)^(n_i) (Mulero et al. JPCRD 2012: 75, 2014: 27; Okada 4; Kondou 1; IAPWS 1994 1), throws above T_c (Ancillaries.h:52-64).
- **Melting line** (30 fluids): Simon (18) p = p₀ + a((T/T₀)^c − 1); polynomial_in_Tr (9) p = p₀(1 + Σ a((T/T₀)^t − 1)); polynomial_in_Theta (3) p = p₀(1 + Σ a(T/T₀ − 1)^t); piecewise segments; T(p) closed-form for Simon, Brent per segment otherwise (Ancillaries.cpp:152-262). Water and HeavyWater fold back below T_triple (excluded from the generic test, :321-328). `T_m` parsed, never read.
- **No sublimation curves** in the fluid model; only HumidAirProp.cpp hard-codes ice sublimation pressure (e.g. :566).
- **Pseudo-pure:** `pseudo_pure` flag, pL/pV pair, `pressure_max_sat`/`temperature_max_sat`; `is_pure()` false (HelmholtzEOSMixtureBackend.h:187-189), so superancillaries are rejected (cpp:314-319). `dev/pseudo-pure/fit_pseudo-pure_eos.py` imports the removed `Props` API and `matplotlib.mlab`, last touched 2019 (3b1eb503): dead.
- **Identity:** keys CAS, NAME, each ALIAS and UPPER(ALIAS) → one index (FluidLibrary.cpp:344-359); UPPER(NAME) is checked (:291) but never inserted. Oracle: "WATER", "R134A" resolve, "r134a" does not. 347 aliases, no collisions. CAS is the primary key but 10 are synthetic (`AIR.PPF`, `R404A.PPF`, `SES36.ppf`, `1333-74-0o/p`, `7782-39-0o/p`). `REFPROP_NAME` = "N/A" for 9 fluids; `ENVIRONMENTAL` missing for 11 (−1/`_HUGE` sentinels). Load order, hence `fluids_list` order, comes from an unsorted `glob` (dev/generate_headers.py:389); oracle list starts `R1234ze(E), CarbonDioxide, CycloHexane, R218, …`.

### 3.8 Numerical edge cases

| Case | CoolProp behaviour | Evidence |
|---|---|---|
| δ → 0 (virials) | GenExp works in ln δ and 1/δ, so δ = 0 is NaN/∞; virials use δ = 1e-12. In B_δ2 = … + (B_δ1 − 1)B_δ1, with B_δ1 = 1 + δu′ for d = 1, l = 1 terms, (1 + 1e-12·x) − 1 keeps ~4 digits. **C is wrong at 1e-5 level**: at 300 K, Nitrogen −7.1e-5, R32 −3.6e-5, Ethane −2.9e-5, Methane +9e-6 vs the δ→0 limit (verifier's 3-point Richardson extrapolation of the oracle's α^r_δδ at δ = 1e-4, 2e-4, 4e-4). B is fine (≤ ~1e-11). 45 fluids have d = 1, l = 1 terms. | cpp:1683-1698; Helmholtz.cpp:234-235; scratchpad virial check |
| τ, δ near 1 (non-analytic) | τ, δ within 10ε of 1 are moved to 1 + 10ε (Helmholtz.cpp:355-363). θ_δδδ is computed as pow(…)/(δ−1)³ (0/0 at δ = 1, :377); θ_δδδδ ∝ \|δ−1\|^(1/β − 4) = \|δ−1\|^(−2/3) for β = 0.3 (:378-379), so 4th δ-derivatives are genuinely infinite along δ = 1 for Water and CO2; Δ^(b−1) diverges at the critical point. Oracle at the exact reducing state of Water: c_p = 1.28e15 J/(mol·K), w = 14.8 m/s (nudge artefacts). | Helmholtz.cpp:350-557 |
| exp overflow | Summing the exponent first (`exp(t lnτ + d lnδ + u)`, :228) avoids overflow (Methanol g_t < 0 gives e^(+23τ)). PlanckEinsteinGeneralized with θ > 0 (Air) overflows only below 16.3 K. ParaHydrogen at T_min: θτ = 738 → e^(−738) is subnormal (slow arithmetic, harmless value). | Helmholtz.cpp:1161-1170 |
| ln(1 − e^(−x)), small x | no `log(-expm1(-x))`; smallest x within EOS range is 0.0225 (Dichloroethane at T_max) → ~1e-14 relative loss, negligible | Helmholtz.cpp:1164 |
| MBWR cancelling pairs (d = 0) | α^r at δ = 0.05 is a difference of O(10) terms; worst evaluator-vs-oracle gap 5.8e-14 (R21) | §3.1 |
| R123 ideal gas | cp⁰ off by −1.1…−1.4e-5 relative (250-450 K) vs the JSON polynomial (block `Tc` 456.82 ≠ T_r 456.831) | §6 |
| Summation order | bit-exact parity is not a realistic TDD target; agreement ~1e-14 is | evaluator |

## 4. Data and configuration inputs

- **Top-level keys** (136 files): `INFO` {NAME, CAS, ALIASES, REFPROP_NAME (136); FORMULA, INCHI_STRING, INCHI_KEY, SMILES, CHEMSPIDER_ID, 2DPNG_URL (126); ENVIRONMENTAL (125)}, `STATES` {critical, triple_liquid, triple_vapor; each T, p, rhomolar, hmolar, smolar plus `*_units` strings that are never read; empty triple state ⇒ −1/`_HUGE`, FluidLibrary.h:1086-1092}, `EOS[]`, `ANCILLARIES`, `TRANSPORT` (66).
- **EOS block keys** (159): gas_constant, molar_mass, acentric, pseudo_pure, T_max, p_max, Ttriple (**ignored**), BibTeX_EOS, BibTeX_CP0, `STATES` {reducing, sat_min_liquid, sat_min_vapor, hs_anchor (156), pressure_max_sat/temperature_max_sat (6)}, alphar[], alpha0[], SUPERANCILLARY (131), critical_region_splines (70), one `acentric_note`; every scalar has a `*_units` twin.
- **Multi-EOS files** (23; alternates unreachable): Ammonia, D4, D5, Helium, HydrogenChloride, MD2M, MD3M, MD4M, MDM, MM, Methanol, Neon, R11, R123, R1234yf, R1234ze(E), R1234ze(Z), R152A, R245fa, SulfurDioxide, n-Hexane, n-Octane, n-Pentane.
- **Semantic irregularities to encode explicitly:** "0 means absent" for l, m (§3.1); GaoB η stored negated; PlanckEinstein `t` sign flipped at load; per-block `Tc`/`Tcrit` duplicates of T_r (R123 and R11 EOS[1] differ); α⁰ block keys `R`, `T0` partly ignored; `Ttriple` ignored; reducing ≠ critical (15); JSON critical ≠ EOS critical (25).
- **No JSON schema for the 136 fluid files.** `dev/validate_fluid_schemas.py:27-32` validates only PC-SAFT, cubic and departure-function data.
- **Build:** all JSON merged into one CBOR blob (dev/generate_headers.py:371-420), embedded via incbin (FluidLibrary.cpp:9-24), decoded wholesale on first use; each fluid's full JSON is also kept as a string (`JSONstring_map`, FluidLibrary.cpp:338).
- **Config keys** (process-global, include/CoolProp/detail/configuration_keys.h): `NORMALIZE_GAS_CONSTANTS` (:11), `CRITICAL_WITHIN_1UK` (:12), `CRITICAL_SPLINES_ENABLED` (:14), `DONT_CHECK_PROPERTY_LIMITS` (:44), `R_U_CODATA` (:49), `OVERWRITE_FLUIDS` (:55), `ENABLE_SUPERANCILLARIES` (:72), `ENABLE_MELTING_CALORIC_HS` (:73), `LIST_STRING_DELIMITER` (:80). Env: `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY`, `COOLPROP_DISABLE_MELTING_CALORIC_HS`. Compile-time: `LAZY_LOAD_SUPERANCILLARIES`.
- **Upstream data fixes after 8.0.0** (oracle disagrees with literature there): 2acbbc82 reducing densities of Nitrogen, Ethylene (+M), OrthoHydrogen (+M), n-Undecane; 66859efb superancillaries refit to match; 520b8809 / d2a3a9fc EOS citations for ammonia, helium, n-octane, D4, dichloroethane; 3a052553 `C2H6` alias; ec2e75d5 new fluid R-1132a.

## 5. State, caching, globals, thread-safety, memory

| Shared / mutable state | Location | Hazard |
|---|---|---|
| `static JSONFluidLibrary library`, `call_once` load | FluidLibrary.cpp:28-44 | load is safe; later `add_fluids_as_JSON` (with `OVERWRITE_FLUIDS`), `set_reference_state`, `set_fluid_enthalpy_entropy_offset` mutate `std::map`s unsynchronized against concurrent `get()`: data race |
| Fluid returned **by value** and copied per state | FluidLibrary.h:1323-1332; cpp:114, 121, 142-149, 171-177; ReducingFunctions.h:157-162 | ~10 deep `CoolPropFluid` copies per pure `AbstractState` construction, 6 live (top/SatL/SatV `components` + 3 `GERG2008ReducingFunction::pFluids` that only need T_r, ρ_r). The user-declared `~CoolPropFluid() = default` (CoolPropFluid.h:538) suppresses the implicit move constructor, so even rvalue hand-offs copy. Each copy includes alternate EOSs, transport data and the 56-96 KB (median 66 KB) superancillary JSON string kept after parsing (CoolPropFluid.h:406, 441-447; the comment at :431-432 claims "copying O(1)"). Oracle: construction 49-54 µs vs 1.3-3.6 µs for update(D,T) + h + c_p |
| Caches and scratch inside model data | Helmholtz.h:750-763; Ancillaries.h:30, 120 (written in `evaluate`, Ancillaries.cpp:65-68) | parameters and per-state scratch mixed: the EOS cannot be shared across threads |
| Evaluation mutates the model | `E.alpha0.set_Tred(Tc)` cpp:3595, 3682; lazy `finish()` Helmholtz.cpp:144 | "nocache" reads write to the fluid |
| Lazy superancillary build | CoolPropFluid.h:433-438 | unsynchronized check-then-create (only with `LAZY_LOAD_SUPERANCILLARIES`) |
| `static std::atomic<int> deriv_counter` | cpp:45-47, 3554 | incremented on every cached α^r evaluation (`calc_all_alphar_deriv_cache`; the nocache path skips it), never read: one contended cache line across all threads |
| MeltingCaloric process cache | MeltingCaloric.cpp:247-265 | keyed by fluid *name* (stale after a fluid override, *inference*); global mutex held across a multi-flash build that mutates the caller's backend (:18-59, 206-245) |
| Global config singleton | src/Configuration.cpp:127-132 | lazy unsynchronized creation; read in hot paths (e.g. `ENABLE_SUPERANCILLARIES` in cpp:1207); `DONT_CHECK_PROPERTY_LIMITS` toggled temporarily by TabularBackends.cpp:129, 197 leaks to other threads |

**Memory and lazy loading (oracle, Python 3.12; verifier rerun: 2.05 s / +75.6 MB and 0.19 s / +64.3 MB):** importing CoolProp loads the whole library eagerly: 1.98 s and +75.6 MB RSS; with `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY` 0.20 s and +64.2 MB. Eager superancillary construction therefore costs ≈ 1.8 s and ≈ 11 MB for 130 fluids. The coefficients of all 2532 default residual terms are ≈ 0.2 MB as f64. Conclusion: load per fluid on first use, keep the superancillary behind a second lazy cell, never keep JSON strings after parsing, share via `Arc`.

## 6. Rot and bugs (table: issue | evidence | impact | Rust remedy)

| Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|
| μ ≠ g for pure fluids; mixture α⁰ uses critical instead of reducing constants | cpp:3517-3521 (μ), 3637-3638 and 3695-3696 (mixture α⁰), MixtureDerivatives.cpp:833-834; `iT_critical` → `crit.T` (.h:315-316). Oracle μ − g at T = 1.3T_r, ρ = 0.5ρ_r: Methanol +83.2, MDM −146.6, Air −183.7, R134a −26.2, Helium −2.36, IsoButane +1.03 J/mol (Water, Propane 0). Mixture at x = [1, 0] vs pure at the same state: Methanol Δh = −14.19 J/mol (Methanol&Water or &Ethanol), R134a +0.49 (R134a&R32); Δα^r = 0, so all of it is α⁰ (verifier rerun). Still on master (cpp:3684). | wrong chemical potential and mixture-vs-pure inconsistency for ≥ 15 fluids | distinct types `Reducing` vs `CriticalPoint`; α⁰ only ever sees its own EOS's (τ, δ); property test μ ≡ g |
| Two coexisting critical points, config-dependent public constants | cpp:1197-1253 vs .h:315-322. R13: 303.05 K vs 301.88 K; flipping `ENABLE_SUPERANCILLARIES` changes `T_critical()`; 25 fluids differ | silent 1-2 K disagreements between APIs; mixture rules use the "other" value | `EosCriticalPoint` (computed from the EOS, cached lazily), `PublishedCritical` (metadata), `Reducing` (parameter); no global flag changes constants |
| C virial inaccurate (δ = 1e-12 + cancellation) | cpp:1683-1698; Helmholtz.cpp:234-235. Oracle vs δ→0 limit at 300 K: N₂ −7.1e-5, R32 −3.6e-5, Ethane −2.9e-5 (R134a, no d = 1/l = 1 terms: 5e-12). Still on master. | wrong C and dC/dT at the 1e-5 level | exact δ → 0 Taylor coefficients per term (integer d, l make this trivial); a dedicated "virial jet" |
| c_p and c_v returned for two-phase states | cpp:3309-3344 have no phase check (w does, :3358-3365). Oracle Water 300 K, Q = 0.5: c_p 2069.6, c_v 1575.0 J/(kg·K) | physically meaningless values, silently | property API returns `Result`; two-phase rules in a separate layer |
| Hydrogen / OrthoHydrogen melting curve wrong at low T | Hydrogen.json melting (Datchi-PRB-2000, T₀ = 1, p₀ = −236200, a = 231000, c = 1.7627). Oracle: p_melt(T_triple = 13.957 K) = 23.6 MPa vs p_triple 7.36 kPa; p_melt(20 K) 44.9 MPa vs ParaHydrogen 22.7 MPa; OrthoH2 copies it with T_min 13.957 < its T_triple 14.008. *(inference: a high-T Simon fit applied down to the triple point)*. Still on master. | wrong melting pressures and limit checks | literature check (Datchi 2000 validity range); data validator: p_melt(T_triple) ≈ p_triple (Helium exempt: it has no solid-liquid-vapour triple point; oracle p_melt(T_min) = 2.2 MPa is physical) |
| Simon T(p) accepts T ≥ T₀ instead of T ≥ T_min | Ancillaries.cpp:199. Oracle T(0.5·p_min): Hydrogen 9.52 K (T_min 13.957), Helium 1.88 (2.18), O₂, Kr, p-H₂ also below T_min (by ≤ 0.03 K). `T_0` is the Simon reference temperature (1 K for H₂), not a range bound. Still on master. | silent extrapolation below the triple point | check `[T_min, T_max]` per segment; typed out-of-range error |
| R123 (and R11 EOS[1]) ideal gas uses `Tc` ≠ T_r | R123.json EOS[0] CP0 blocks `Tc` 456.82 vs T_r 456.831; formula Helmholtz.cpp:1188-1258 assumes τ = T_c/T. Oracle c_p⁰ −1.3e-5 relative vs the JSON's own polynomial in T. Still on master. | small systematic error in all caloric properties of R123 | c_p⁰-type terms take T_r from the model; validator rejects `Tc` ≠ T_r (or an explicit compat flag) |
| Fluid deep-copied into every state (~10 copies, 6 live; no move constructor) | FluidLibrary.h:1323-1332; cpp:114, 121, 142-149, 171-177; ReducingFunctions.h:157; CoolPropFluid.h:406, 538 | ~50 µs (oracle, R134a: 48 µs) and ~0.4-0.5 MB *(inference: 6 × ~66 KB median superancillary strings plus term data)* per AbstractState; no sharing | immutable `Arc<PureEos>`; states hold `Arc` + small value structs |
| α⁰ recomputed per derivative; 4th order always computed, never usable | cpp:3580-3626, 3792-3831, 3615-3618; no `alpha0.all(…, true)` anywhere | several full α⁰ passes (logs/exps) per property call | one α⁰ τ-jet per state (or per isotherm) |
| Reference states: tripled code, RESET no-op, only new states see changes, member `set_reference_stateD` uses the mixture h | CoolProp.cpp:946-1039; cpp:4463-4597 (4535-4536); FluidLibrary.cpp:64-127; Helmholtz.h:944-961; RESET is documented as "Remove the offset" (include/CoolProp/CoolProp.h:147, AbstractState.h:813) but `set(0,0)` only adds zero; oracle (§3.5: ASHRAE then RESET leaves h unchanged) | inconsistent semantics, races | `ReferenceState` value applied to an immutable model: `eos.with_reference(Ref::Iir) -> Arc<PureEos>` |
| Mutable caches/scratch in model data; container API ignores its arguments | Helmholtz.h:750-773, 883, 1451; Ancillaries.cpp:65-68; cpp:3595 | not thread-shareable; stale values if `clear()` is missed (latent) | pure functions `fn eval(&self, τ, δ) -> Derivs`; caches are explicit per-state values |
| CP0PolyT t = −1 4th derivative wrong | Helmholtz.cpp:1251: −3c/(τ³T_c); correct −2c/(τ³T_c) (derivative of c/(τ²T_c)). n-Undecane uses t = −1; test only covers t ∈ {0,1,2} (:1465-1471). Latent (order 4 never read). Still on master. | latent wrong value | AD removes the class of bug |
| Association term incomplete and unreachable | Helmholtz.cpp:1031-1064 (no 4th order; X and its derivatives recomputed ~10× per call, :1035-1048); Helmholtz.h:728-742 (`1e99`); test skip :1767-1769 | ~355 lines unused; wrong if enabled | defer; if needed, value-only + AD |
| Alternate EOSs unreachable | CoolPropFluid.h:539-545 (`EOS()` = `[0]`), 23 files | parsed, stored and copied for nothing | explicit selection by citation key, lazily compiled |
| Non-analytic: 10ε nudge, 0/0 forms, name-based fudge | Helmholtz.cpp:355-363, 377; FluidLibrary.cpp:91; cpp:4565; CoolProp-Tests.cpp:2406 | artefacts at (1,1) (c_p 1.28e15 J/(mol·K)); hidden special cases | rewrite with sign(δ−1)\|δ−1\|^p (no division); document singular orders; model flag `has_nonanalytic` instead of names |
| Loader robustness | unknown α⁰ type only printed (FluidLibrary.h:328-331); asserts only (:54-56 …, 1182-1186; CoolPropFluid.h:450-453); "Cannot add " truncated messages (:93, 151, 184). Fixed upstream: length checks 427c8aeb/ae54172f/681472bf; swallowed load errors and orphan names 51e8c60f; uninitialized `-SRK` EOS scalars 0f978943/ae54172f; `GERG2004Sinh::extend` self-append (Helmholtz.h:1193-1197) 427c8aeb; *(added by verifier)* `CP0PolyT`/`GERG2004*::extend` silently join terms with a different Tc/T0 (rejected upstream in ae54172f; latent at 8.0.0, since the only multi-block case, n-Heptane's two Aly-Lee blocks, shares Tc and T0) | silent corruption on bad data | serde schema with `deny_unknown_fields`, validation pass, typed errors |
| Hard-coded R = 8.3144598 | FluidLibrary.h:1257 (the commented-out `fluid.EOSVector[0].R_u` shows the intent); cpp:496, 517 (`change_EOS`). The same literal also appears in cpp:2830 and VLERoutines.cpp:1172, 2654 (outside this area, *added by verifier*). Still on master | inconsistent with the EOS R (10 distinct EOS R values, 8.3143-8.31451) | R always from the model |
| Identity/lookup quirks | FluidLibrary.cpp:291 vs 344-359 ("r134a" fails, "R134A" works); synthetic CAS; unsorted load order (generate_headers.py:389) | surprising lookups; non-reproducible `fluids_list` | generated, sorted registry with an explicit case-folding policy; `CasId` enum |
| "Ttriple" is sat-min; virial units wrong | FluidLibrary.h:384-386 (R114 273.15 vs 180.63 K); DataStructures.cpp:61-63 ("-" for B, C; B is m³/mol) | misleading constants and docs | separate `triple_point` and `eos_t_min`; typed units |
| Global hot-path counter and config | cpp:45-47, 3554; Configuration.cpp:127-132; TabularBackends.cpp:129, 197 | cross-core contention, cross-thread interference | no globals in kernels; per-call `Options` |
| Dead code and data | `*_x_*` fields (.h:869-874, never read); `EOSLimits.rhomax`; `T_m`; `alpha0_prefactor`; GenExp SoA arrays built in `finish()` (Helmholtz.h:504-546) read only by commented `allEigen` (Helmholtz.cpp:37-137); `kahanSum/wayToSort/ramp` (:14-35); Aly-Lee comments (Helmholtz.h:1248-1377); 53 `throw()` specifications (40 in Helmholtz.h, 13 in Helmholtz.cpp), including the 15 `BaseHelmholtzTerm` getters that call a virtual `all()` that may throw (e.g. GERG2004Sinh, Helmholtz.cpp:1271) → `std::terminate`; IdealCurves.h without include guard; dev/pseudo-pure | maintenance noise, 2× term memory | do not port |
| Inefficiencies | container-wide u-flags make every Power term evaluate zero Gaussian parts (Helmholtz.cpp:167-226); `IdealHelmholtzPower` calls pow 5× per term (:1116-1150); pure fluids pass through mixture CS + Excess + struct copies (.h:697-706, 866-876); `fluid_param_string` deep-copies the fluid (cpp:248) | wasted work in the hottest loop | homogeneous per-family SoA blocks; pure fast path |

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

| Kernel | Fit | Notes |
|---|---|---|
| Separable residual terms (99.8 % of terms) | **SIMD-friendly** | Same arithmetic for every term: n·exp(Φ_τ + Φ_δ) plus B-factor jets; integer d, l → shared δ^k table (k ≤ 15) and ≤ 6 shared e^(−δ^l). Two vector axes: (a) terms of one state (12-54, median 16 → 2-4 AVX2 iterations, exp-throughput bound, modest gain); (b) **states of one fluid in a batch** (lanes = states, identical term table, best fit for batch calls). |
| Isotherm reuse | data reuse | For fixed T (density solves, isotherm sweeps) the τ-factors F_k(τ) and their jets are computed once; each δ-iteration then needs only exps for distinct l (≤ 6) and Gaussians. Upstream's δ-only path (c6d414e4, ~3× for mixture density solves) still recomputes exp(t lnτ + d lnδ + u) per term *(inference from the master source)*. |
| Ideal-gas α⁰ | SIMD over states | τ-only, ≤ ~12 terms (log/exp), cache per isotherm |
| Property formulas | trivially vectorizable | pure arithmetic on 15-element derivative arrays; SoA over states |
| Non-analytic (Water, CO2; 5 terms) | scalar | fractional `pow`, sign handling, singular orders near δ = 1 |
| Association | scalar | deferred anyway |
| Ancillary / melting evaluation | cheap scalar; inversions branchy | Brent per call |
| Loading, validation, reference-state setup, superancillary build | sequential, one-time | parallel across fluids at most |

- **Side-by-side implementations (the user's request).** One `ResidualKernel` trait with a scalar reference backend (ordered summation, deterministic, also `no_std`/WASM) and SIMD backends (state-batch and term-batch) selected per model at compile time or by runtime CPU-feature detection (SSE2/AVX2/AVX-512/NEON/WASM simd128). All backends share one SoA term table, so concerns stay separated: data layout, math, and vectorization strategy. Differential tests assert fast vs reference ≤ 1e-14 relative per derivative. Non-separable terms stay scalar inside every backend.
- **Vector exp/ln** is the bottleneck: `std::simd` is nightly-only and has no vector exp/ln; options are `wide`/`pulp` plus a small polynomial exp/ln (≈1 ulp) to keep dependencies minimal. Its accuracy sets the fast-kernel tolerance.
- **Threads:** `Arc<PureEos>` is `Send + Sync`; no locks, counters or config reads in kernels; registry lookups are lock-free after a per-fluid `OnceLock`.
- **GPU (later):** all term tables fit in ~0.2 MB (constant memory) and the state-batch kernel is embarrassingly parallel, but consumer-GPU f64 throughput is poor: defer.

## 8. Verification assets (tests, check values, paper tables, oracle hooks)

- **Oracle hooks** (Python `AbstractState`): `alphar`, `alpha0`, all α^r derivatives up to `d4alphar_*`, α⁰ up to `d3alpha0_*`, `tau()`, `delta()`, `T_reducing()`, `rhomolar_reducing()`, `Bvirial/Cvirial`, `chemical_potential`, `fugacity_coefficient`, `melting_line`, `saturation_ancillary`, `first/second_partial_deriv`, `get_fluid_constant`, `get_state`. Use `specify_phase(iphase_gas)` + `update(DmolarT_INPUTS, …)` to hit any (T, ρ) directly.
- **Independent evaluator (scratchpad):** value-only α^r, α⁰ from JSON for all 136 default EOSs (≤ 5.8e-14 / 9.3e-15 vs oracle) + `mpmath.diff` derivatives (≤ 3.4e-13). Rebuild it as the golden-data generator, including the 23 alternate EOSs (oracle cannot reach them; check against papers).
- **Exact virial series** (scratchpad): B, C from per-term Taylor coefficients at δ = 0; exposes the oracle's C error.
- **CoolProp tests:** term derivatives vs FD/multicomplex (Helmholtz.cpp:1761-1817); paper validation tables (p, c_v, c_p, w) for 12 fluids (CoolProp-Tests.cpp:4787-4840); reference states (:2263, 2295); fixed states for all reference states (:2437); triple point (:2096); first/second partial derivatives (:1744, 1829); Methanol vs REFPROP 10 (:3361); superancillary `source_eos_hash` FNV-1a over the parsed EOS (:3598, useful as a data fingerprint); melting lines (Ancillaries.cpp:282-389); hs_anchor vs EOS (:391-414).
- **Literature arbiters:** IAPWS-95 Table 6 (α⁰, α^r and derivatives at 500 K, 838.025 kg/m³): oracle matches all 12 printed values to ≤ 3e-9 (CoolProp does not test this); IAPWS-95 Table 7; Span & Wagner 1996 (CO2); Lemmon & Jacobsen 2005 (R125); Gao et al. 2020 (NH3); each EOS's paper via `BibTeX_EOS`; IAPWS 2011 melting/sublimation release (Water); Datchi et al. 2000 (H2, He melting).
- **Identities usable as property tests** (all hold to ≤ 2e-15 in the oracle except μ): μ = g (pure); h − Ts = g; c_p − c_v = T(∂p/∂T)²_ρ/(ρ²(∂p/∂ρ)_T); μ_JT closed form = generic Jacobian; ln φ = α^r + δα^r_δ − ln(1 + δα^r_δ); B = lim (Z − 1)/ρ.
- **Known oracle-vs-literature divergences to whitelist:** μ for reducing ≠ critical fluids; mixture α⁰; C virial; two-phase c_p/c_v; R123 c_p⁰; H2/o-H2 melting; Simon extrapolation; 4 reducing densities fixed in 2acbbc82; values at the exact reducing state of Water/CO2.

## 9. Port recommendation (units with priority P0-core / P1-early / P2-later / defer / drop; order; what to redesign)

**Minimal data model** (covers all 159 EOS blocks; serde, no math):
```rust
// crate fluid-data: decode one fluid on demand; SUPERANCILLARY and TRANSPORT stay raw until asked for
pub struct FluidRecord { info: Info, states: PublishedStates, eos: Vec<EosRecord>, ancillaries: AncillaryRecords, transport: Option<RawJson> }
pub enum CasId { Registry { a: u32, b: u8, c: u8 }, Synthetic(Box<str>) }      // "AIR.PPF", "1333-74-0p"
pub struct EosRecord { gas_constant: f64, molar_mass: f64, acentric: f64, kind: EosKind /* Pure | PseudoPure{max_sat_t, max_sat_p} */,
    reducing: Reducing /* t, rho: the only (τ,δ) source */, sat_min: [StatePoint; 2], hs_anchor: Option<StatePoint>,
    t_max: f64, p_max: f64, t_triple_published: Option<f64>, alphar: Vec<ResidualBlock>, alpha0: Vec<IdealBlock>,
    superancillary: Option<RawJson>, cite_eos: Box<str>, cite_cp0: Box<str> }
#[serde(tag = "type", deny_unknown_fields)] pub enum ResidualBlock {
    Power { n: Vec<f64>, d: Vec<u8>, t: Vec<f64>, l: Vec<u8> },          // l == 0 ⇒ no exp factor
    Exponential { n, d, t, g: Vec<f64>, l: Vec<u8> }, Gaussian { n, d, t, eta, epsilon, beta, gamma },
    Lemmon2005 { n, d, t, l: Vec<u8>, m: Vec<f64> }, DoubleExponential { n, d, t, gd, ld, gt, lt },
    GaoB { n, t, d, eta, beta, gamma, epsilon, b }, NonAnalytic { n, a, b, beta, A, B, C, D }, Associating { a, m, epsilonbar, vbarn, kappabar } }
pub enum IdealBlock { Lead{a1,a2}, LogTau{a}, Power{n,t}, PlanckEinstein{n,theta}, PlanckEinsteinFunctionT{n,v_kelvin},
    PlanckEinsteinGeneralized{n,theta,c,d}, Cp0Constant{c,t0}, Cp0PolyT{c,t,t0}, Cp0AlyLee{a,b,c,d,e,t0}, Offset{a1,a2,tag: RefTag} }
```
**Compiled model and traits** (crate `helmholtz`, immutable, `Send + Sync`, shared by `Arc`):
```rust
pub struct PureEos { r: f64, molar_mass: f64, reducing: Reducing, residual: Residual, ideal: IdealGas, offset: (f64, f64) }
pub struct Residual { separable: SeparableTable /* SoA: n, ln n?, t, d:u8, exponent primitives; grouped by family */,
                      nonanalytic: Box<[NonAnalytic]>, assoc: Option<Assoc> }
pub struct Derivs { a: [f64; 15] }            // scaled a_ij = τ^i δ^j ∂^{i+j}α, i+j ≤ 4 (what every property formula uses)
pub enum Want { Order2, Order3, Order4, DeltaOnly4 }   // compute only what the caller needs
pub trait AlphaTerm { fn value<S: Scalar>(&self, tau: S, delta: S) -> S; }       // value-only reference; S = f64 | jet | dual
pub trait ResidualKernel: Send + Sync { fn eval(&self, tau: f64, delta: f64, want: Want) -> Derivs;
                                        fn eval_batch(&self, td: &[(f64, f64)], want: Want, out: &mut [Derivs]); }
pub trait HelmholtzModel { fn reducing(&self) -> Reducing; fn r(&self) -> f64; fn alphar(&self, tau: f64, delta: f64, want: Want) -> Derivs;
                           fn alpha0(&self, tau: f64, delta: f64, want: Want) -> Derivs; }   // pure, pseudo-pure, mixture (doc 04), cubic (doc 06)
```
- Store **scaled** derivatives a_ij; δα_δ, δ²α_δδ, τδα_τδ, τ²α_ττ are exactly what §3.4 needs, and the 1/δ^j division of Helmholtz.cpp:274-289 disappears. Provide exact δ → 0 Taylor coefficients separately for virials.
- Separable kernel = generalized "exp of jets": per term, univariate order-4 jets of Φ_τ (t lnτ, −ωτ^m, −β(τ−γ)², 1/(b+β(τ−γ)²)) and Φ_δ (d lnδ, −cδ^l, −η(δ−ε)²), then the B-recurrence and outer product. One code path covers Power, Exponential, Gaussian, Lemmon2005, DoubleExponential, GaoB and the GERG departure Gaussian.
- Fixed max order 4 with a `Want` mask (stable Rust cannot size `[f64; N+1]` from a const generic). Keep a generic `Scalar` path (jets/duals) for NonAnalytic, association and future material models.
- Materials later: keep `HelmholtzModel` for fluids; Gibbs-explicit solids (e.g. IAPWS-06 ice) get their own trait. Do not force everything through Helmholtz.

| Unit | Priority | CoolProp paths | ~LOC (C++) | Notes / redesign |
|---|---|---|---|---|
| Fluid JSON schema + typed loader (INFO, STATES, EOS, ANCILLARIES; SUPERANCILLARY/TRANSPORT raw and lazy) | P0-core | FluidLibrary.h:42-450, 1015-1186; FluidLibrary.cpp:150-369; FluidLibraryFactories.h | 900 | serde enums; validate lengths, integer d/l, "0 ⇒ absent", signs, cp⁰ `Tc` = T_r, triple/melting consistency; ignore `*_units` after checking them |
| Registry: identity, aliases, lazy per-fluid load | P0-core | FluidLibrary.cpp:28-62, 287-394; FluidLibrary.h:1206-1338; generate_headers.py | 280 | sorted generated index (name/alias/CAS → id) with explicit case folding; `OnceLock<Arc<PureEos>>` per fluid; second lazy cell for the superancillary; feature-gated fluid sets for WASM size; runtime additions return a new registry value |
| Separable residual families (+ GERG2008Gaussian for doc 04) | P0-core | Helmholtz.h:309-554, 622-649; Helmholtz.cpp:138-292, 629-717 | 520 | SoA per family, shared δ^k and e^(−δ^l), one exp per term, exp-of-jet B-recurrence; GaoB η normalized at load |
| Non-analytic term | P0-core | Helmholtz.h:556-598; Helmholtz.cpp:350-557 | 250 | value-only + AD; sign(δ−1)\|δ−1\|^p form; explicit behaviour at δ = 1 and (1,1) instead of the 10ε nudge |
| Ideal-gas terms (10 JSON types, converted at load) | P0-core | Helmholtz.h:897-1172, 1379-1455; Helmholtz.cpp:1083-1258; FluidLibrary.h:171-334 | 690 | τ-jet + ln δ; `ln(-expm1(-x))`, log-sum-exp for θ > 0; c_p⁰ conversions in one place using the model's T_r |
| Derivative engine (`Derivs`, `Want`, jets, generic `Scalar`, scalar reference kernel) | P0-core | Helmholtz.h:40-307, 747-887; CoolPropFluid.h:454-520; HelmholtzEOSMixtureBackend.cpp:3552-3831; AbstractState.h:1553-1679 | 880 | replaces ~1.8k lines of hand derivatives and plumbing; δ-only and isotherm-cached modes |
| Pure-fluid property functions (p, Z, u, h, s, g, a, c_v, c_p, c_p⁰, w, residual/ideal parts, ln φ, μ = g, exact B/C/dB/dT/dC/dT, ∂p/∂ρ, ∂p/∂T, generic (∂A/∂B)_C, κ_T, α_p, κ_s, Γ, μ_JT, PIP, neff) | P0-core | cpp:1683-1698, 3151-3532; AbstractState.cpp:696-760, 950-1260 | 770 | pure functions of (`Derivs`, R, T, ρ, M); two-phase handling in a separate layer returning `Result` |
| Constants and limits as distinct types (reducing, EOS critical, published critical, triple, EOS range) | P0-core | CoolPropFluid.h:111-114, 400-573; cpp:1095-1325 | 300 | EOS critical point computed lazily (or from the superancillary); no config flag changes constants |
| Reference states (core + user offset; IIR/ASHRAE/NBP/DEF/custom) | P1-early | Helmholtz.h:924-981; cpp:4463-4597; CoolProp.cpp:946-1039; FluidLibrary.cpp:64-127 | 350 | immutable `with_reference()`; needs the saturation solver (doc 04); anchor h/s computed lazily, not stored |
| Saturation ancillaries + surface tension | P1-early | Ancillaries.h:25-178; Ancillaries.cpp:26-113; FluidLibraryFactories.h:26-73 | 300 | `fn eval(&self, T) -> f64` without scratch; Horner rational polynomials (drop Eigen `Polynomial2D`); keep anchor-relative caloric ancillaries |
| Pseudo-pure support | P1-early | flags, pL/pV, max_sat states (FluidLibrary.h:415-435, 1129-1140; cpp:1254-1280) | 60 | `EosKind::PseudoPure` variant; no superancillary |
| Melting lines (Simon, polyTr, polyTheta) | P1-early | Ancillaries.h:186-290; Ancillaries.cpp:115-277; FluidLibrary.h:1015-1068 | 320 | fix the T_min bound; fix H2 data after the literature check; optional sublimation-curve type for future materials |
| Alternate EOS selection | P1-early | CoolPropFluid.h:539-545 | 30 | select by citation key; compile lazily |
| SIMD / batch kernels (state-batch, term-batch) | P2-later | – (new) | – | feature-gated, runtime dispatch, differential-tested against the scalar reference; SoA layout decided in P0 |
| GERG2004 Cosh/Sinh ideal terms | P2-later | Helmholtz.h:1176-1246; Helmholtz.cpp:1260-1362 | 175 | for strict GERG backends (doc 04/06) |
| Cubic-as-residual, XiangDeiters, `change_EOS` | P2-later | Helmholtz.cpp:579-603, 732-782; FluidLibrary.h:1236-1316; cpp:483-532 | 210 | belongs with doc 06 |
| Ideal-curve tracers | P2-later | IdealCurves.h; cpp:1128-1144 | 155 | built on property functions + a 1-D solver; start point from the model's range |
| MeltingCaloric | P2-later | MeltingCaloric.{h,cpp} | 375 | per-model lazy cell, not a name-keyed global (doc 03) |
| Association term | defer | Helmholtz.h:673-745; Helmholtz.cpp:784-1064 | 355 | only Methanol EOS[1] and disabled AceticAcid; value-only + AD if ever needed |
| Critical-region splines | defer | CoolPropFluid.h:40-108; FlashRoutines.cpp:925-944 | 90 | superseded by superancillaries |
| Multicomplex `one_mcx` hooks | drop | Helmholtz.cpp:294-347 and 6 more | 140 | replaced by generic `Scalar` AD |
| dev/pseudo-pure, commented code, `deriv_counter`, `*_x_*` fields, `rhomax`, `T_m`, `alpha0_prefactor` (reject if present), per-derivative virtual getters, container caches, name-based fudges, `kahanSum` & co. | drop | as cited in §6 | ~1100 | – |

**Order.** (1) Schema + loader + registry, golden data from the evaluator and the oracle. (2) Separable kernel with scalar reference + `Derivs`. (3) Ideal-gas terms. (4) Non-analytic term. (5) Property functions, TDD against the oracle on (T, ρ) inputs only. (6) Constants and limits. (7) Ancillaries, then reference states (after the saturation solver of doc 04). (8) Melting, pseudo-pure, alternate EOS. (9) SIMD kernels.

**Perf tricks to keep / add / drop.** Keep: single `exp` of the summed exponent per term; B-factor recurrence (generalized); `powInt` for integer exponents; one evaluation yields all needed derivatives. Add: scaled-derivative output; `Want` masks; isotherm τ-cache; shared δ^k and e^(−δ^l) tables; α⁰ jet per state; pure-fluid fast path; lazy per-fluid loading. Drop: per-instance deep copies, container caches, global counters, JSON strings kept in memory.

## 10. Open questions

1. **Divergence policy.** Where 8.0.0 is demonstrably wrong (μ ≠ g, mixture α⁰ constants, C virial, two-phase c_p/c_v, R123 c_p⁰, H2 melting, Simon extrapolation), does Rust match the oracle behind a compat flag or follow the literature by default? A divergence registry with evidence links is needed either way.
2. **Data snapshot.** v8.0.0 JSON for oracle parity, or origin/master (corrected reducing densities 2acbbc82, refit superancillaries 66859efb, R-1132a, citation fixes)? Proposal: ship master data, keep an `8.0.0-compat` data feature for TDD.
3. **Which critical point is public?** JSON `STATES.critical`, the EOS reducing state, or the EOS's numerical critical point (25 fluids differ, up to 2.3 K for R40)?
4. **Alternate EOSs.** Expose the 23 through an API? If yes, the association term (Methanol EOS[1]) moves from defer to P1.
5. **Reference-state semantics.** Per-model immutable values only, or also a process default mirroring `set_reference_state` (which CoolProp applies only to states created later)? Should defaults reproduce the published, not-exactly-IIR offsets?
6. **Non-analytic singularities.** At δ = 1 the 4th δ-derivative is infinite; at (T_r, ρ_r) c_p diverges. Return ±∞, NaN with an error, or CoolProp's 10ε nudge?
7. **Tolerance targets.** Is ≤ 1e-12 relative acceptable for EOS-level TDD (evaluator-vs-oracle gaps reach 5.8e-14 on α^r, 3.4e-13 on derivatives), with looser, documented bounds near cancellations?
8. **Pseudo-pure typing.** A pure fluid with a flag, or a distinct "fitted blend" kind ahead of the mixture/material generalization?
9. **Max derivative order.** Is 4 enough for future algorithms (critical-point analysis, higher-order solvers), or should the jet order be generic from day one?
10. **SIMD numerics.** Which vector exp/ln (dependency vs in-house) and which accuracy budget for the fast kernels relative to the scalar reference?

## Verification log

**2026-10-04, adversarial verifier.** About 185 claims checked: about 140 `path:line` citations opened at v8.0.0 (ae81610e), and about 45 counts and oracle values recomputed. The counts came from `dev/fluids/*.json` with my own scripts and from `CoolProp==8.0.0`. I compared against origin/master for the "still on master" and "fixed upstream" claims. No partial edits from an earlier verifier were found; the doc was structurally intact.

Confirmed unchanged (selection):
- every row of §2;
- the GenExp B-recurrence and the "0 means absent" convention;
- the NonAnalytic 10ε nudge and 0/0 forms;
- the test skip of the SAFT 4th order;
- the CP0PolyT t = −1 4th-derivative bug (−3c vs −2c; still at master Helmholtz.cpp:1448);
- the Aly-Lee comment/code contradiction;
- the GERG2004Sinh self-append (fixed in 427c8aeb);
- the `*_x_*` fields never read, the dead SoA arrays and the `deriv_counter`;
- the two critical points (25 fluids; R13 303.05 vs 301.88 K);
- μ − g for 8 fluids;
- two-phase c_p/c_v (Water 300 K, Q = 0.5: 2069.6 / 1575.0 J/(kg·K));
- H₂ melting 23.6 MPa at T_triple;
- Simon `T >= part.T_0` (Ancillaries.cpp:199, still on master);
- R123 `Tc` 456.82 vs T_r 456.831;
- RESET being a no-op;
- old states keeping their reference;
- the "r134a" lookup failure;
- the 1.00001 Water/CO2 fudge;
- IAPWS-95 Table 6 (≤ 2.9e-9);
- μ_JT closed form (1 ulp);
- the Water reducing-state artefact (c_p 1.28e15, w 14.8 m/s);
- construction 48 µs vs update+h+c_p 3.7 µs;
- all §3.1/§3.2/§4 family, term, key and ancillary counts (136/159/23/15/16/2532/1966/433/100/57/45/347/10/9/11/108/30).

No rot item was refuted.

Corrections:
1. Header: "~12.6k lines" → 14.0k lines across the listed files (7.0k term/data files + 7.0k backend/AbstractState, pure parts only in scope); `dev/pseudo-pure` 771 lines are Python + Cython.
2. §1: GenExp holds 99.7 % (2525/2532) of default residual terms, not 99.8 %. GaoB is a separate evaluator; 99.8 % is separable incl. GaoB.
3. §3.1: "~14 distinct t per fluid (no reuse)" → median 14, 82 % distinct (little reuse). "1164 δ^l-exponential terms" relabelled with its family breakdown.
4. §3.1 *(added)*: 277 `d` and 161 `l` values are JSON floats (`1.0`); the loader must accept integral floats.
5. §3.3: α⁰ *is* cached per derivative at AbstractState level (AbstractState.h:1556+). What is missing is computing the derivatives as a set (one full container pass per distinct derivative).
6. §3.4: Z location added (AbstractState.cpp:971-973 → HelmholtzEOSMixtureBackend.h:657); it is not in cpp:3151-3168.
7. §3.7: "46 call sites" → ~50 lines in three files.
8. §3.8/§6: C-virial errors at 300 K re-measured. R32 −4.4e-5 → −3.6e-5; Ethane −2.4e-5 → −2.9e-5; B "≤ 4e-12" → ≤ ~1e-11.
9. §3.8: R123 c_p⁰ error range −1.2…−1.4e-5 → −1.1…−1.4e-5 (250-450 K).
10. §5: superancillary strings are 56-96 KB (median 66 KB), not 65-90 KB. Per-state memory estimate adjusted to ~0.4-0.5 MB *(inference)*.
11. §5: `deriv_counter` counts only cached α^r evaluations, not every α^r evaluation.
12. §6 μ row: mixture-vs-pure Δh at the stated state (1.3 T_r, 0.5 ρ_r) re-measured as Methanol −14.19 J/mol and R134a +0.49, replacing the −29.46 / +0.70 that could not be reproduced. The claim itself stands.
13. §6 RESET: added the evidence that RESET is documented as "Remove the offset" (CoolProp.h:147, AbstractState.h:813).
14. §6 hard-coded R: added the other occurrences (cpp:2830; VLERoutines.cpp:1172, 2654) and confirmed it is still on master.
15. §6 dead code: "`throw()` on 53 functions that can throw" reworded. There are 53 specifications, of which the throwing path is the 15 base getters calling a virtual `all()`.
16. §6 loader *(added)*: `CP0PolyT`/`GERG2004*::extend` joining mismatched Tc/T0 is latent at 8.0.0 and fixed in ae54172f.
17. §6 Simon/H₂: overshoot magnitude added (≤ 0.03 K for O₂/Kr/p-H₂). The p_melt(T_triple) ≈ p_triple validator must exempt Helium (no s-l-v triple point).
18. §7: "no stable `std::simd` exp" → `std::simd` itself is nightly-only.
19. §3.3 code-volume table: flagged as approximate (±15 %) after partial reproduction.
