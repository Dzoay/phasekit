# Seams for materials and all states of matter

Research input for design decision **D10** (material and states-of-matter seams). It also feeds D2 (numeric core), D3 (model representation), D5 (state and properties), D9 (execution strategies) and D14 (licensing). The D-numbers are the decisions listed in the design brief.
Date: 2026-10-04. Versions and statuses are as of that date.
Local citations: `path:line` is `reference/CoolProp` at v8.0.0 (`ae81610e`). `master:path` is CoolProp origin/master at `022b63e4` (2026-10-03).

**Bottom line.** v1 gets **no material abstraction**: no `Material` or `PhaseModel` trait and no multi-phase container. It gets eight cheap choices that a later material layer will need:

- a potential-to-properties layer that never imports fluid types, fed by a derivative bundle that needs no reducing (critical) state
- separate types for identity, model and assembled fluid
- the reference state as a value, never a global
- typed domain errors, including "below the melting line", checked outside the evaluator
- a public read-only state view that transport is written against
- property keys owned by each model family, plus typed methods
- a provenance record on every model and dataset
- `#[non_exhaustive]` and private fields on every public type that will grow

Adding a public item, a trait, or a variant of a `#[non_exhaustive]` enum later is a minor (non-breaking) change ([Cargo SemVer guide](https://doc.rust-lang.org/cargo/reference/semver.html)). The guide notes that a new public item can, in rare cases, clash with a downstream glob import.
So waiting until a second model family exists costs nothing, while guessing the trait now risks getting it wrong. Cantera 3.2 (PR merged 2025-09-08, released 2025-11-18) deprecated its speculative solid-lattice classes (§2.2.2).

---

## 1. Summary of recommendations

| # | Decision | Recommendation | Confidence |
|---|---|---|---|
| R1 | Material abstraction in v1 | **None.** Build concrete Helmholtz `Fluid` types only. Add `PhaseModel` and `Substance` when a second potential family has to coexist with a fluid (IAPWS-06 ice for humid air, or Jäger–Span dry ice). | High |
| R2 | Potential → properties | `relations::helmholtz`: pure, branch-free functions of `(T, ρ, r, &HelmholtzDerivs)`. The bundle holds **total** teqp-style derivatives Λ_xy = (1/T)^x ρ^y ∂^(x+y)α/∂(1/T)^x∂ρ^y. These equal τ^x δ^y ∂^(x+y)α/∂τ^x∂δ^y for *any* reducing pair, so a solid, plasma or tabular model can fill the bundle without a critical point or an ideal-gas split (§2.1). Add a `relations::gibbs` sibling (g, g_T, g_p, g_TT, g_Tp, g_pp) together with the first Gibbs model. | High (separate layer) / Medium (Λ convention; confirm the parity tolerance) |
| R3 | Identity vs model vs package | `FluidInfo` (identity, `Option` fields, no `-1` sentinels), `HelmholtzEos` (one formulation plus its `Source`), `Fluid` (the assembled pure-fluid package). Reserve the names `Substance` and `Material` for the multi-phase future. | High |
| R4 | Reference state | A value, `EnergyOffset { dh, ds }`, on the fluid handle. Never mutate shared data. Later it belongs to the substance and is applied to every phase model. | High (value) / Medium (lifting it out of the alpha0 term list) |
| R5 | Domain and metastability | Raw evaluation is unchecked. Validity is enforced in flash and selection, with per-call options. Use `#[non_exhaustive] enum DomainError { BelowMeltingLine {..}, .. }`. `Limits` has private fields so it can later grow into a domain with holes. Batch calls return one result per point. | High |
| R6 | State view | `State` has private fields and getters (T, ρ, p, phase, derivative bundle). Transport uses only the public API, so later property models can live in other crates. | High |
| R7 | Property-request API | Typed methods are the main API. A per-family `#[non_exhaustive] enum FluidProp` (unit variants, `Copy + Eq + Hash`, `FromStr` with CoolProp names, `category()`) serves the dynamic, batch and PropsSI paths. Missing data returns `Err(Unsupported)`. Tensor-valued and parameterised properties go through typed methods only. No marker-type keys yet. | Medium-High |
| R8 | Where transport and interfacial models attach | To the phase model (in v1, the `Fluid`), never to a substance-level object. | High |
| R9 | Provenance and data terms | `Source { key (BibTeX), doi, terms, optional source-artifact hash }` on every model and dataset. Restricted or copyleft data is never in default features. | High |
| R10 | Future-proof public types | `#[non_exhaustive]` on `Phase`, `FluidProp`, input pairs, errors and metadata structs. Private fields with constructors (C-STRUCT-PRIVATE). | High |
| R11 | Selection vs evaluation (the SIMD link) | Keep "which model, region or phase applies" (branchy, varies per point) apart from "potential → derivatives → properties" (straight-line arithmetic). Batches are partitioned first, then evaluated. SIMD kernels sit beside the scalar reference. If D2 adopts a scalar trait, make `relations` generic over it. | High (split) / Medium (SIMD payoff) |
| R12 | Correlation-only models | When INCOMP is ported, put its fit forms in a family-neutral `correlation` module, with a validity range and an explicit extrapolation policy per property. NIST-cryogenic solids then reuse it with two more fit forms. | Medium-High |
| R13 | First non-Helmholtz models | IAPWS-06 ice Ih (Gibbs), then IF97 (Gibbs plus Helmholtz plus a region map), then INCOMP correlations. Bollengier comes later: it is on CoolProp master only, so the 8.0.0 oracle cannot check it. | Medium |
| R14 | Explicitly deferred | Multi-phase `Substance` and selectors; solid EOS (Debye, Einstein, MGD, SLB, ice polymorphs); SESAME/LEOS readers; Saha plasma; CALPHAD; tensors and anisotropy; non-equilibrium (two-temperature) states; marker-type keys; plugins; non-SI units. | High |

---

## 2. Findings by subtopic

### 2.1 Thermodynamic potential families and where each fits

| Family | Natural variables | Where it is used (evidence) | How properties follow | Consequence |
|---|---|---|---|---|
| **Helmholtz a(T, ρ, x)**, fluids | T, ρ | All CoolProp HEOS fluids. CoolProp cubics subclass `HelmholtzEOSMixtureBackend` (`src/Backends/Cubics/CubicBackend.h:36`). PC-SAFT has `calc_alphar` (`src/Backends/PCSAFT/PCSAFTBackend.h:130`). Clapeyron "obtains all the properties of a model by differentiating the total Helmholtz energy" ([custom model guide](https://clapeyronthermo.github.io/Clapeyron.jl/dev/user_guide/custom_model/)). feos `Residual`: "A residual Helmholtz energy model" (`crates/feos-core/src/equation_of_state/residual.rs:98`, [feos-org/feos](https://github.com/feos-org/feos)). teqp models are α^r(T, ρ, z) ([teqp derivs](https://teqp.readthedocs.io/en/latest/derivs/derivs.html)). IF97 region 3 is f(ρ, T) ([R7-97](https://iapws.org/technical-guidance/release/IF97-Rev)). IAPWS G8-10 humid air is a Helmholtz energy "as a function of air mass fraction, temperature and density" ([G8-10](https://iapws.org/technical-guidance/release/SeaAir)). | p, s, u, h, cv, cp, w and so on from α and its derivatives | **The v1 currency** |
| Helmholtz, solids and wide-range EOS | T, ρ (isotropic); T plus a strain tensor (anisotropic) | SESAME splits the free energy into "the cold curve (or zero Kelvin isotherm), the nuclear vibrational excitations, and the thermal electronic excitations" ([LA-14503, McHardy 2018](https://www.osti.gov/biblio/1487368)). Stixrude & Lithgow-Bertelloni (2005) use Eulerian finite strain plus "an effective Debye temperature", with relations "generalized to anisotropic strain" ([GJI 162:610](http://kgblab.epss.ucla.edu/publication/stixrude-gji-162-610-2005/)). The Debye model gives U = 9NkT(T/T_D)³∫x³/(eˣ−1)dx, a T³ law at low T and Dulong–Petit at high T; T_D depends on volume ([Debye model](https://en.wikipedia.org/wiki/Debye_model)). BurnMan's EOS classes include MGD2/3, SLB2/3, BM3/4, Vinet, HP_TMT and DKS_S ([`burnman/eos/__init__.py`](https://raw.githubusercontent.com/geodynamics/burnman/main/burnman/eos/__init__.py)). | Same relations in the isotropic case | Reuse `relations::helmholtz`. Tensor variables are deferred. |
| Mie–Grüneisen | V, e (an incomplete EOS) | p − p₀ = (Γ/V)(e − e₀), with the reference state usually at 0 K and estimated from the Hugoniot. "It is used to determine the pressure in a shock-compressed solid" ([Wikipedia](https://en.wikipedia.org/wiki/Mie%E2%80%93Gr%C3%BCneisen_equation_of_state)). | Gives only p(ρ, e) unless a thermal model (Debye) completes it into a(T, V) | The hydrocode form. A complete MGD (BurnMan `mgd3`) fits the row above. |
| **Gibbs g(T, p, x)** | T, p | Ice Ih: "a fundamental equation for the Gibbs energy as a function of temperature and pressure" ([R10-06(2009)](https://iapws.org/technical-guidance/release/Ice-2009)). IF97 regions 1, 2 and 5 ([R7-97](https://iapws.org/technical-guidance/release/IF97-Rev)). Supercooled liquid water: a two-state Gibbs g(T, p) ([G12-15](https://iapws.org/technical-guidance/release/Supercooled)). TEOS-10 seawater (IAPWS-08 for the saline part, [TEOS-10 software](https://www.teos-10.org/software.htm)). Dry ice (Jäger & Span 2012, [doi:10.1021/je2011677](https://doi.org/10.1021/je2011677), as cited in [Clapeyron](https://raw.githubusercontent.com/ClapeyronThermo/Clapeyron.jl/master/src/models/CompositeModel/SolidModel/JagerSpanSolidCO2.jl)). CALPHAD: one Gibbs energy per phase ([pycalphad API](https://pycalphad.org/docs/latest/api/pycalphad.html)). | R10-06 Table 3 gives ρ, s, cp, h, u, f, α, κ_T, κ_s and more from g and its first and second derivatives | Add `relations::gibbs` together with the first Gibbs model |
| Gibbs as a spline or local basis functions (LBF) | T, p | Bollengier, Brown & Shaw (2019): liquid water, 240–500 K, up to 2300 MPa, tensor B-splines ([arXiv:1903.11730](https://arxiv.org/abs/1903.11730); J. Chem. Phys. 151, 054501, doi:10.1063/1.5097179, as cited in `master:src/Backends/Bollengier/BollengierBackend.h`). SeaFreeze: the README describes "Gibbs Local Basis Functions parametrization … for each phase". Its material list cites Journaux et al. 2020 for ice II, III, V and VI, Bollengier 2019 (`water1`) and Brown 2018 (`water2`) for liquid water, and NaCl(aq) as in preparation. Ice Ih cites **Feistel and Wagner 2006** (IAPWS-06) and ice VII/X cites **French and Redmer 2015**, so not every phase is an original LBF fit ([README](https://raw.githubusercontent.com/Bjournaux/SeaFreeze/master/README.md)). | Same Gibbs relations; the evaluator is a spline | The relations must be separate from the term evaluators. **Gibbs does not mean solid**: G12-15 and Bollengier are liquids. |
| Standard-state g°(T) at a reference pressure | T | Cantera species thermo (NASA-7, NASA-9, Shomate, constant-cp, piecewise-Gibbs) gives the reference h and s as functions of T; "The phase model then describes how the species interact with one another" ([species thermo](https://cantera.org/stable/reference/thermo/species-thermo.html), [thermo overview](https://cantera.org/stable/reference/thermo/index.html)). | Integrate cp° | Chemistry and combustion layer. Deferred. |
| Tabular wide-range EOS (solid → plasma) | ρ, T grids | SESAME: "about 150 materials", "from 10⁻⁶ to 10⁴ gm/cc and 0 to 10⁵ ev", with separate opacity and conductivity libraries ([LANL](https://www.lanl.gov/engage/organizations/aldsct/theoretical/pcm/sesame)). Table 301 = 304 + 305 + 306 (electron + ion + cold curve) and 303 = 305 + 306; each holds energy, free energy and pressure. Tables 601–605 hold mean ion charge, electrical and thermal conductivity, thermoelectric coefficient and electron conductive opacity ([USim reference](https://txcorp.com/images/docs/usim/latest/reference_manual/source_sesameVariables.html)). Tables 501–505 hold the T–ρ boundary, Rosseland opacity, conductive opacity, mean ion charge and Planck opacity ([LANL opacity docs](https://aphysics2.lanl.gov/static/opacdocs/sesame.html)). "Some materials also have vaporization, melt and shear tables" ([LANL](https://www.lanl.gov/engage/organizations/aldsct/theoretical/pcm/sesame); their 401/411/412/431 numbering is unverified). LEOS is LLNL's tabular library (tables L110 and L2240 in [LLNL-TR-705163](https://www.osti.gov/biblio/1330135)). | Interpolated P, E and A (A is the Helmholtz free energy) | (ρ, T) is still the natural pair. Deferred because of licensing (§2.5). |
| Saha ionization | T, n_e | n_{i+1}n_e/n_i = (2/λ_th³)(g_{i+1}/g_i)exp[−(ε_{i+1}−ε_i)/k_BT]. It assumes local thermodynamic equilibrium and an ideal gas, and "is only valid for dilute gases". It needs ionization energies and statistical weights ([Wikipedia](https://en.wikipedia.org/wiki/Saha_ionization_equation)). | A nonlinear solve per state (charge and mass balance) | Composition becomes an internal variable: a small equilibrium solve inside the model. Deferred. |
| Single-property correlations | T (sometimes x or RRR) | NIST cryogenic data use "a few simple types of polynomial or logarithmic polynomial equations" for k, cp, linear expansion, CTE and Young's modulus, 4–300 K ([2006 paper](https://trc.nist.gov/cryogenics/Papers/Material_Properties/2006-Cryogenic_Material_Properties_Database-Update_2006-ICEC21_Prague.pdf)). OFHC copper: log₁₀ k is a rational function in T^0.5 for each RRR from 50 to 500; fit errors are 1–2 % for k and 10 % for cp (T ≥ 15 K) and for expansion (T ≥ 50 K) ([OFHC page](https://trc.nist.gov/cryogenics/materials/OFHC%20Copper/OFHC_Copper_rev1.htm)). CoolProp INCOMP has five fit forms (§2.2.1). thermo ranks correlation methods by validity range, with selectable extrapolation ([thermo.utils](https://thermo.readthedocs.io/thermo.utils.html)). | Each property fitted on its own | **Not thermodynamically consistent by construction.** One correlation mechanism serves INCOMP and solids (R12). |

Two currencies fall out of this table:

- **(T, ρ) with a Helmholtz derivative bundle** evaluates a single-phase state in every state of matter: fluids, isotropic solids, SESAME-type tables and ideal plasma.
- **g(T, p)** compares phases, and it is the native form of the Gibbs models.

v1 needs the first. The second arrives with the first Gibbs model.

**Why the bundle should not depend on a reducing state (refines R2).**
teqp writes every property in "concise derivatives" Λ_xy = (1/T)^x ρ^y ∂^(x+y)α/∂(1/T)^x∂ρ^y.
For example p/(ρRT) = 1 + Λ^r_01, h/(RT) = 1 + Λ^r_01 + Λ^tot_10 and s/R = Λ^tot_10 − Λ^tot_00 ([teqp derivs](https://teqp.readthedocs.io/en/latest/derivs/derivs.html)).
Because τ ∝ 1/T and δ ∝ ρ, Λ_xy = τ^x δ^y ∂^(x+y)α/∂τ^x∂δ^y for every choice of reducing pair (a derivation, not a quote).
Every fluid ideal-gas part has the form α⁰ = ln δ + f(τ), so Λ⁰_01 = 1, Λ⁰_02 = −1 and Λ⁰_11 = 0.
The relations can therefore take the **total** Λ alone (r is the gas constant on the chosen basis):

| Property | From total Λ |
|---|---|
| p | ρ r T Λ01 |
| u; h | r T Λ10; r T (Λ01 + Λ10) |
| s; a; g | r (Λ10 − Λ00); r T Λ00; r T (Λ00 + Λ01) |
| (∂p/∂ρ)_T; (∂p/∂T)_ρ | r T (2Λ01 + Λ02); ρ r (Λ01 − Λ11) |
| cv; cp | −r Λ20; r [−Λ20 + (Λ01 − Λ11)² / (2Λ01 + Λ02)] |

What this buys:

- A model with no critical point (a solid, a plasma, a table) needs no invented reducing constants.
- A solid needs no ideal/residual split.
- Parts evaluated in different reduced variables add up after conversion (for example, mixture α⁰ per component and α^r on the mixture reducing functions).
- The bundle is a small `Copy` struct, which is the natural lane layout for SIMD (§2.6).

Fluid-only outputs (residual properties, virial coefficients, fugacity) stay in the fluid module.

### 2.2 How existing libraries structure this

#### 2.2.1 CoolProp's own non-Helmholtz parts (local source)

| Component | Form | Phase / validity logic | Lesson for the port |
|---|---|---|---|
| IF97 backend (`src/Backends/IF97/IF97Backend.h`, wrapping `IF97.h` from github.com/CoolProp/IF97, fetched in `cmake/dependencies.cmake:62-67`) | Regions 1, 2 and 5 use Gibbs γ(π, τ). Region 3 uses Helmholtz φ(δ, τ). Region 4 is psat(T). Backward equations as well. | `set_phase()` (`IF97Backend.h:85-86`) classifies from psat97, Tc and pc with ε = 3.3e-5 ("IAPWS-IF97 RMS saturated pressure inconsistency"). | The canonical case of **one substance, several potentials and an explicit region map**. The right moment to introduce a region map. |
| Incompressible (`src/Backends/Incompressible/*`) | `IncompressibleData` forms POLYNOMIAL, EXPPOLYNOMIAL, EXPONENTIAL, LOGEXPONENTIAL and POLYOFFSET (`include/CoolProp/fluids/IncompressibleFluid.h:29-38`), used for ρ, c, η, λ, psat and T_freeze. Composition basis `IFRAC_MASS / MOLE / VOLUME / PURE` (`include/CoolProp/DataStructures.h:272-276`). h and s come from integrals of c plus p terms (`IncompressibleBackend.cpp:522-526`). | No phases. The "melting line" is T_freeze(p, x) (`IncompressibleBackend.cpp:501-507`). | The same machinery as the NIST cryogenic solid fits. Composition is not always mole fractions. |
| Ice (`src/Ice.cpp`) | IAPWS-06 g(T, p) with complex logs, written as free functions `g_Ice`, `dg_dp_Ice`, `dg2_dp2_Ice`, `dg_dT_Ice`, `h_Ice`, `s_Ice`, `rho_Ice` and the sublimation curve `psub_Ice` (`include/CoolProp/fluids/Ice.h`). | Not a backend; only HumidAir calls it. There is no g_TT or g_Tp, so no ice cp or α. Returns `1e99` on PowerPC (`Ice.cpp:2`, `:28-34`). `g00` is the superseded 2006 value (§4). | Port the full R10-06 equation with all derivatives and the Table 6 check values. |
| HumidAir (`src/HumidAirProp.cpp`) | Composes IAPWS-95 water, Lemmon air, IAPWS-06 ice, IF97 (psat97, and a PT density for k_T) and virial terms. | **Hard switch at T = 273.16 K**: liquid above, ice below (`:819`, `:857-868`). The call sites disagree at exactly 273.16 K: `:819` and `:857` use `T > 273.16`, while `:562`, `:1554` and `:1593` use `T >= 273.16`. Backends are per-thread because "The HelmholtzEOSBackend instances are mutated on every HAPropsSI call" (`:44-53`). | Composition of models already exists. Immutable models remove the need for `thread_local` copies. |
| Melting line (fluid JSON `ANCILLARIES.melting_line`) | Piecewise Simon, polynomial_in_Tr or polynomial_in_Theta. 30 of 136 fluids have one (Simon 18, Tr 9, Theta 3). | HEOS throws "For now, we don't support T [..] below Tmelt(p)" with 1 mK slack (`HelmholtzEOSMixtureBackend.cpp:1726-1729`, `:2189-2193`, also `:1805`, `:1878`). The opt-out is the global `DONT_CHECK_PROPERTY_LIMITS` (`include/CoolProp/detail/configuration_keys.h:44`). | **This is already the solid seam.** Make it a typed error, and make the opt-out a per-call option. |
| Water melting line | BibTeX `IAPWS-Melting-2011`, 4 parts starting at 273.16, 251.165, 256.164 and 273.31 K. These are the R14-08 ice Ih, III, V and VI melting curves ([R14-08](https://iapws.org/technical-guidance/release/MeltSub)). | Each segment implicitly names the solid on the other side. | Possible later data field: an optional `solid_phase` label per segment. |
| `MeltingCaloric` (`src/Backends/Helmholtz/MeltingCaloric.cpp`) | Chebyshev fits of T, ρ, h and s along the liquid side of the melting curve, used to seed HS flashes. | Process-global `static std::map` plus a `std::mutex` (`:247-250`). | Derived per-fluid data belongs in a `OnceLock` inside the immutable model, not in a global map. |
| `RegionAtlas` (`include/CoolProp/region/RegionAtlas.h:12-29`, `Region.h:13-20`) | Generic regions in (a, b) bounded by boundary curves. A bounding-box (AABB) check runs first, then a curve check; "the first match wins". | Used by the SBTL/SVD tables. | A reusable region-map design. Port it with its first consumer (IF97 or SBTL). |
| Reference state (`src/CoolProp.cpp:946-1039`) | `set_reference_stateS` computes offsets and then does "Change the value in the library for the given fluid" (`:982-983`). 55 of 136 fluid JSONs carry an `IdealGasHelmholtzEnthalpyEntropyOffset` alpha0 term in their default EOS (`EOS[0]`); 65 carry one in some EOS entry. | Mutates process-global state. | Make it a per-handle value (R4). |
| Model multiplicity (`include/CoolProp/CoolPropFluid.h:545`) | `EOSVector` holds "The equations of state that could be used for this fluid". 23 fluids list more than one EOS (for example Ammonia: Gao-JPCRD-2020 and TillnerRoth-DKV-1993). Index 0 is used. | — | One substance already has several models. Keep identity apart from model (R3). |
| Parameters enum (`include/CoolProp/DataStructures.h:64-178`) | One flat enum mixing constants, state, thermo, transport, surface tension, α derivatives, incompressible limits, environmental indices and `iPhase`. `INFO.ENVIRONMENTAL` uses `-1.0` for unknown GWP and ODP (`dev/fluids/Water.json`). | — | Give properties a category, use `Option` instead of sentinels, give each family its own keys (R7). |
| Phases enum (`DataStructures.h:182-194`) | 7 real labels plus `iphase_unknown` and `iphase_not_imposed`. | — | A `#[non_exhaustive] Phase` with the 7 labels; the two sentinels become `Option`/`Result`. |
| Batch API | `fast_evaluate` with a status code per point (`DataStructures.h:196-205`, `AbstractState.h:917-923`). | — | Per-point domain errors in batch calls have a precedent. |
| **Bollengier**: absent from v8.0.0, on master since 2026-10-03 (`6d788a7a`, #3426, `master:src/Backends/Bollengier/BollengierBackend.h`) | Gibbs G(P, T) as a tensor B-spline. Liquid only, including the metastable liquid below the melting line that IAPWS-95 refuses. PT inputs only: "one surface evaluation plus five partials yields every property with no iteration". | The domain is the paper's rectangle (240–500 K, 0–2300.6 MPa) **minus an excluded box** (p ≥ 1500 MPa and T ≤ 255 K). Inside it, the fit is inadmissible over p 1833.5–2300.6 MPa and T 240.0–249.6 K (cv ≤ 0 or (∂v/∂P)_T ≥ 0). Out to 1526.4 MPa and 251.6 K, cv collapses toward 0.03 J/(kg·K). The box rounds that union outward. The reference state is deliberately left as published: at the triple-point liquid, h = 71.23 J/kg and s = 0.258 J/(kg·K), versus about 0.61 and 0 for IAPWS-95. `set_reference_stateS` "silently no-ops" for this backend. The coefficients were extracted from a paywalled supplement and committed as "numerical facts" with sha256 hashes of the source files (`master:dev/scripts/extract_bollengier_coefficients.py`). | Domains need holes. Models of one substance have different reference states. Provenance can record the hash of the source artifact. |
| The Gibbs property map is written three times | `IF97.h:235-261`, `master:.../BollengierBackend.h:322-332` and `src/Ice.cpp:118-143` (from [`docs/coolprop-map/07-special-purpose.md`](../coolprop-map/07-special-purpose.md)) | — | One `relations::gibbs` module (DRY). |

#### 2.2.2 Other libraries

| Library (version, date, licence) | Structure | Lesson |
|---|---|---|
| **Cantera 3.2.0** (published 2025-11-18, [GitHub API](https://api.github.com/repos/Cantera/cantera/releases/tags/v3.2.0); BSD-3-Clause, [License.txt](https://raw.githubusercontent.com/Cantera/cantera/main/License.txt)) | Two levels: species thermo gives reference h and s as functions of T, and "The phase model then describes how the species interact" ([thermo](https://cantera.org/stable/reference/thermo/index.html)). `ThermoPhase` subclasses are chosen by the `thermo:` key: `IdealGasPhase`, `IdealSolidSolnPhase` ("an ideal liquid or solid solution"), `StoichSubstance` (fixed stoichiometry), `PureFluidPhase` (liquid, vapor, two-phase and supercritical), `WaterSSTP` (IAPWS-95, liquid only), `PlasmaPhase` (handles "the electron energy distribution and electron temperature"), `MetalPhase` (electron cloud), Peng–Robinson, Debye–Hückel, HMW and others ([phase thermo](https://cantera.org/stable/reference/thermo/phase-thermo.html)). `transport:` and `kinetics:` are separate fields of each phase ([YAML phases](https://cantera.org/stable/yaml/phases.html)). `Mixture.equilibrate` finds "the composition that minimizes the total Gibbs free energy of the mixture, subject to element conservation constraints" ([Python thermo](https://cantera.org/stable/python/thermo.html)). | Transport is separate, and multi-phase equilibrium is a separate object. **3.2 deprecated `LatticePhase` and `LatticeSolidPhase`** ([PR #1959](https://github.com/Cantera/cantera/pull/1959), merged 2025-09-08). `LatticePhase` "doesn't do anything (correctly) that isn't handled by" `IdealSolidSolnPhase`; `LatticeSolidPhase` has "numerous serious consistency issues" and "does not appear to be significantly used". Speculative solid abstractions decay. |
| **Clapeyron.jl 0.6.29** (tagged 2026-09-27, [GitHub API](https://api.github.com/repos/ClapeyronThermo/Clapeyron.jl/releases); same as master [Project.toml](https://raw.githubusercontent.com/ClapeyronThermo/Clapeyron.jl/master/Project.toml), MIT, [repo](https://github.com/ClapeyronThermo/Clapeyron.jl)) | Root type `EoSModel`, with `EmpiricHelmholtzModel`, `CubicModel`, `ActivityModel` and `GibbsBasedModel` among its subtypes ([types.jl](https://raw.githubusercontent.com/ClapeyronThermo/Clapeyron.jl/master/src/models/types.jl)). `GibbsBasedModel` arrived in v0.6.16 ("EoS that use the gibbs energy as the main function"), together with `IAPWS06` and `JagerSpanSolidCO2` ([HISTORY](https://raw.githubusercontent.com/ClapeyronThermo/Clapeyron.jl/master/HISTORY.md)). `CompositeModel` (fields `components`, `fluid`, `solid`, `mapping`) routes liquid and vapour calculations to `fluid` and solid ones to `solid`. With the phase unknown, only `volume` compares candidates by Gibbs energy, and only when **both** models are residual-Helmholtz (`a_res`) models. Otherwise it, and every bulk `PT_property` call, throws "automatic phase detection not implemented" ([CompositeModel.jl](https://raw.githubusercontent.com/ClapeyronThermo/Clapeyron.jl/master/src/models/CompositeModel/CompositeModel.jl), `volume_impl` and `PT_property`). So for a Gibbs-based solid such as `IAPWS06`, the caller must name the phase. Gibbs equality is used for the melting and sublimation curves and for the triple point. Melting and sublimation curves and `triple_point(model::CompositeModel)` exist since v0.5.9. Reference states are aligned per model pair: `gibbsmodel_reference_state_consts(ice::IAPWS06, water::EmpiricHelmholtzModel)` returns `:zero`, while standalone IAPWS06 returns `:dH, 101325.0, 273.15, 6010.0` ([IAPWS06.jl](https://raw.githubusercontent.com/ClapeyronThermo/Clapeyron.jl/master/src/models/CompositeModel/SolidModel/IAPWS06.jl)). | **This is the path recommended here.** A Helmholtz-first library added a Gibbs family later without breaking anything. Its first solids were IAPWS-06 and Jäger–Span, and reference states are aligned per pair of models. |
| **thermo 0.6.1** (2026-07-13) and **chemicals 1.5.2** (2026-06-07), both MIT ([thermo](https://pypi.org/pypi/thermo/0.6.1/json), [chemicals](https://pypi.org/pypi/chemicals/1.5.2/json)) | chemicals holds the correlation functions. thermo wraps them in `TDependentProperty` and `TPDependentProperty` objects that rank methods with temperature limits and selectable low-T and high-T extrapolation ([thermo.utils](https://thermo.readthedocs.io/thermo.utils.html)). "Phase objects are immutable and know nothing about bulk properties or transport properties" ([phases](https://thermo.readthedocs.io/thermo.phases.html)). `FlashPureVLS(constants, correlations, gas, liquids, solids)`: "The phase with the lowest Gibbs energy is the most stable if there are multiple solutions". On ice polymorphs: "It is less common for there to be published, reliable, thermodynamic models for these different phases" ([flash](https://thermo.readthedocs.io/thermo.flash.html)). | Immutable phases, transport kept apart, min-Gibbs selection for pure substances, solids passed as a list. Correlations carry validity ranges and an explicit extrapolation choice. |
| **pycalphad 0.11.2** (2026-06-05, MIT, [PyPI](https://pypi.org/pypi/pycalphad/0.11.2/json)) | `Database` reads and writes TDB (the Thermo-Calc format). `Model` builds the Gibbs energy of one phase from contributions: reference, ideal mixing, Redlich–Kister excess, magnetic (Inden–Hillert–Jarl), two-state, Einstein, volume and atomic ordering. `calculate()` evaluates; `equilibrium()` finds the stable phase assemblage ([API](https://pycalphad.org/docs/latest/api/pycalphad.html)). | Keep the per-phase model separate from the equilibrium solver. The full CALPHAD stack is a project of its own. |
| **BurnMan 2.1.0** (2024-11-28, GPL-2.0-or-later, [PyPI](https://pypi.org/pypi/burnman/2.1.0/json)) | `Material` → `Mineral`, `Solution`, `Composite` and anisotropic variants ([docs](https://burnman.readthedocs.io/en/latest/materials.html)). `set_state(pressure, temperature)` stores the state on the object and resets its caches. Shear modulus and seismic velocities (`G`, `v_s`, `v_p`) are first-class ([material.py](https://raw.githubusercontent.com/geodynamics/burnman/main/burnman/classes/material.py)). | Solids need shear modulus and tensors. A mutating `set_state` is what this project avoids. Copyleft, so ideas only. |
| **SeaFreeze 1.1.3** (2026-05-27, GPL-3.0, [PyPI](https://pypi.org/pypi/SeaFreeze/1.1.3/json)) | A Gibbs model per phase: LBF for liquid water and ices II–VI; ice Ih cites Feistel and Wagner 2006; ice VII/X cites French and Redmer 2015. `SF_WhichPhase`: "Determine which phase is thermodynamically stable at given (P,T)". Inputs in MPa and K ([README](https://raw.githubusercontent.com/Bjournaux/SeaFreeze/master/README.md)). | Per-point phase selection across Gibbs models. Copyleft code. |
| **TEOS-10** (GSW-C: a BSD-style licence, © 2011 SCOR/IAPSO WG127, that permits redistribution "in source and binary forms, **without modification**". It is **not** BSD-3-Clause; [LICENSE](https://raw.githubusercontent.com/TEOS-10/GSW-C/master/LICENSE)) | Seawater Gibbs function. The SIA library covers seawater (IAPWS-08), ice Ih (IAPWS-06), moist air and IAPWS-95 water, in basic SI units. GSW uses oceanographic units ([software](https://www.teos-10.org/software.htm)). That page says GSW does not provide ice properties, but GSW-C's source does contain IAPWS-06 ice functions (`gsw_gibbs_ice` and others in `gsw_oceanographic_toolbox.c`). IAPWS G8-10 couples a humid-air Helmholtz function with seawater and ice Gibbs functions ([G8-10](https://iapws.org/technical-guidance/release/SeaAir)). | A working example of a consistent Helmholtz plus Gibbs family. Because of the "without modification" clause, use it as an **external oracle only**. Port nothing from it; re-implement from the IAPWS releases. |
| **TREND 2.0** (Bochum) | Combines "highly accurate equations of state (EoS) in form of the Helmholtz energy for fluid phases and with Gibbs energy models for pure solid phases", plus hydrates. The quote matches the abstract as indexed by a search engine; the publisher and catalogue pages blocked automated access ([Jäger et al. 2016, FPE 429:55-66](https://fis.tu-dresden.de/portal/de/publications/model-for-gas-hydrates-applied-to-ccs-systems-part-iii-results-and-implementation-in-trend-20(7a10a7e1-0357-46a0-ad49-fe3008eea7cd).html)). | Helmholtz fluid plus Gibbs solid is established practice in reference-quality software. |
| **feos 0.10.1** (2026-07-24, MIT OR Apache-2.0, [crates.io](https://crates.io/crates/feos)) | Traits `Residual<N, D: DualNum<f64>>`, `IdealGas`, `Total` and `EntropyScaling`; an immutable `State`. No solid or Gibbs trait (`crates/feos-core/src/equation_of_state/{mod,residual}.rs` in [feos-org/feos](https://github.com/feos-org/feos), checked at `246e563d`). | Rust prior art for immutable states, scalars generic for automatic differentiation, and transport kept apart. Fluids only. |
| **teqp** (NIST; "NIST Disclaimer of Copyright and Warranty", 17 U.S.C. §105, [licence](https://api.github.com/repos/usnistgov/teqp/license)) | Models are α^r(T, ρ, z); properties are written in Λ_xy ([derivs](https://teqp.readthedocs.io/en/latest/derivs/derivs.html)). | Source of the reducing-state-free bundle (R2). |
| **rfluids 0.6.0** (MIT, [crates.io](https://crates.io/crates/rfluids)) | A binding to CoolProp. `Substance` is an enum of `Pure`, `IncompPure`, `PredefinedMix`, `BinaryMix` and `CustomMix`; names come from strum `EnumString`. | String and enum keys are the accepted Rust pattern for a CoolProp-style API. |
| Rust landscape ([crates.io "iapws"](https://crates.io/api/v1/crates?q=iapws&per_page=10)) | IF97 and IAPWS crates exist (`seuif97` 2.3.8, `iapws95` 0.2.5, `vle-steam` 0.16.0). A search for "calphad" returns no crates ([search](https://crates.io/api/v1/crates?q=calphad&per_page=10)). Materials crates are tiny, for example `tpt-eng-materials` 0.1.0 with string keys, provenance and a licence gate ([docs.rs](https://docs.rs/tpt-eng-materials/latest/tpt_eng_materials/)). `outram-park-fork-coolprop` 0.1.2 is a **GPL-3.0-only** Rust translation of CoolProp ([crates.io](https://crates.io/api/v1/crates/outram-park-fork-coolprop)). | There is no ecosystem standard for materials to align with. A GPL translation exists and must not be copied. |

#### 2.2.3 Material → phases → models: the vocabulary across libraries

| Concept | CoolProp v8 | Cantera | Clapeyron | thermo | pycalphad | coolprop-rs v1 | Later |
|---|---|---|---|---|---|---|---|
| Identity | fluid info (name, CAS, aliases) | species | `components` | constants | elements and species | `FluidInfo` | `SubstanceInfo` |
| Phase model | backend (`HEOS`, `IF97`, `INCOMP`) | `ThermoPhase` subclass | `EoSModel` (Helmholtz or `GibbsBasedModel`) | immutable `Phase` | `Model` per phase | `Fluid` (one model) | `impl PhaseModel` |
| Multi-phase container | none (HumidAir is hand-composed) | `Mixture` | `CompositeModel{fluid, solid}` | `FlashPureVLS(.., gas, liquids, solids)` | `Database` + `equilibrium()` | none | `Substance { phases, selector }` |
| Transport | inside `AbstractState` | separate `Transport` | not reviewed | separate property objects | — | attached to `Fluid` | attached to each phase model |
| Selection | flash phase logic; IF97 region map; melting-line throw | `equilibrate` (Gibbs minimization) | caller names the phase; automatic only for `volume` across two `a_res` models; Gibbs equality for melting, sublimation and triple point | lowest Gibbs energy | Gibbs minimization with mass balance | `flash` | `Selector` |

### 2.3 Phase stability across models

| Strategy | Who uses it | Pros | Cons | SIMD and batch |
|---|---|---|---|---|
| Explicit region map | IF97 (boundaries plus the B23 equation, [R7-97](https://iapws.org/technical-guidance/release/IF97-Rev)); CoolProp `RegionAtlas` | Fast and deterministic | Hand-made and specific to one formulation; consistent at boundaries only within a tolerance (IF97 ε = 3.3e-5) | Branchy. Partition the batch, then run SIMD per region. |
| Boundary curves (ancillaries) | CoolProp melting line; HumidAir's 273.16 K switch; R14-08 melting and sublimation curves | Cheap; already in v1 | Only as consistent as their data. R14-08 had to refit the ice Ih curves to make them "consistent with the IAPWS-95 formulation and the ice Ih equation of state" ([R14-08](https://iapws.org/technical-guidance/release/MeltSub)). | A cheap scalar test before evaluation. |
| Minimum Gibbs energy across phase models at (T, p) | thermo `FlashPureVLS`; SeaFreeze `SF_WhichPhase`; TREND (a Gibbs-minimum stability analysis). Clapeyron `CompositeModel` only partly: automatic for `volume` across two `a_res` models, and Gibbs equality for its phase-boundary curves. R10-06 notes that equating ice and IAPWS-95 Gibbs energies allows "consistent computation of the melting-pressure and sublimation-pressure curves" ([R10-06](https://iapws.org/technical-guidance/release/Ice-2009)). | General and consistent | Every phase model is evaluated. Needs **aligned reference states**. A Helmholtz fluid needs a density solve to give g(T, p). | Evaluate all candidates on all lanes, then a masked argmin. The extra work grows with the number of phases. |
| Constrained Gibbs minimization with variable phase sets | pycalphad `equilibrium()`; Cantera `Mixture.equilibrate` | Handles multicomponent, multiphase and reacting problems (and Saha-type ionization) | A heavy solver | Not suited to SIMD. Parallelise across conditions instead. |

Production pattern (an inference; it mirrors how CoolProp handles vapor–liquid equilibrium: ancillary guess, then exact solve):

1. Use the boundary curve for a fast guess.
2. Near the boundary, confirm with a Gibbs comparison.
3. When no solid model exists, return `DomainError::BelowMeltingLine` (as v1 does).

**Minimal future abstraction (a sketch only; do not build in v1):**

```rust
// Introduced only when the 2nd family (e.g. IAPWS-06 ice) is ported. Additive: no v1 API breaks.
pub trait PhaseModel: Send + Sync {
    fn source(&self) -> &Source;
    fn domain(&self) -> &Domain;   // rectangle(s) minus excluded boxes, plus boundary curves
    /// Molar Gibbs energy of this phase's stable branch at (T, p): the selection currency.
    /// A Helmholtz fluid solves for density internally and picks its stable root.
    fn gibbs_molar(&self, t: f64, p: f64) -> Result<f64, DomainError>;
}
pub enum Selector { Single, Curves(/* boundary curves */), RegionMap(/* RegionAtlas port */), MinGibbs }
pub struct Substance {
    info: SubstanceInfo,               // identity (R3)
    offset: EnergyOffset,              // ONE per substance, applied to every phase (R4)
    phases: Vec<Arc<dyn PhaseModel>>,  // or a closed enum, per D3; each phase loaded lazily
    selector: Selector,
}
```

The v1 `Fluid` becomes one `impl PhaseModel` without changing its own API.
Each solid model declares which fluid source it is aligned with (for example, IAPWS-06 ice with `Wagner-JPCRD-2002`), as Clapeyron does for each pair.
Coexistence across models (solid–liquid, triple point) becomes a separate assemblage type. It is not added as more `Phase` variants.

### 2.4 Property taxonomy and keeping the request API open

| Category | Examples | Inputs | Source | Value type | In v1? |
|---|---|---|---|---|---|
| Constants and metadata | Tc, pc, M, ω, CAS, GWP, ODP, safety class | none | identity or EOS data | scalar, string or enum (`Option`, never `-1`) | yes |
| Thermodynamic, derived from the potential | p, ρ, u, h, s, g, a, cp, cv, w, κ_T, α_p, Γ, μ_JT, fundamental derivative, virial coefficients | state | potential plus derivatives | scalar | yes |
| Thermodynamic, from correlations | cp(T) of metals, ΔL/L(T), incompressible ρ(T, x) | T (x, RRR) | independent fits (NIST cryogenic, INCOMP) | scalar; **may be inconsistent** between properties | no (INCOMP later) |
| Phase boundaries and ancillaries | psat(T), ρ′, ρ″, T_melt(p), p_sub(T) | one variable | ancillary fits or equilibrium solves | scalar | yes (saturation, melting) |
| Transport | η, λ, diffusivity; electrical σ (metals, plasma; SESAME 602) | state (λ's critical enhancement also needs the EOS at a reference temperature) | separate models | scalar; tensor for anisotropic solids | yes (η, λ) |
| Interfacial | vapor–liquid σ; solid–liquid interfacial energy | T on saturation | separate correlation (Water: `Mulero-JPCRD-2012` in `Water.json`) | scalar, defined **between two phases** | yes |
| Mechanical (solids) | K_T and K_S (from the potential); shear modulus G, E, ν, C_ij | state, orientation | potential plus separate models (SESAME shear tables, SLB, BurnMan) | **tensor** (6×6 Voigt) | no |
| Electromagnetic and optical | static ε_r(T, ρ) for water, 238–873 K up to 1000 MPa ([R8-97](https://iapws.org/technical-guidance/release/Dielec)); refractive index n(λ, T, ρ), −12 to 500 °C, λ from 0.2 to 1.1 µm ([R9-97](https://iapws.org/technical-guidance/release/Rindex)); opacities (SESAME 50x) | state **plus extra parameters** (λ, frequency) | separate models | scalar with parameters; complex if frequency-dependent | no |

Options for the request API:

| Option | Open to extension without editing core? | Type safety | Batch and FFI use | Verdict |
|---|---|---|---|---|
| Closed `enum Prop` → `f64` | Only by editing the defining crate; `#[non_exhaustive]` makes additions non-breaking ([Rust Reference](https://doc.rust-lang.org/reference/attributes/type_system.html)) | Units are lost; invalid combinations are caught only at runtime | Excellent (`Copy` key, `match`, `FromStr`) | **Use one per family** (`FluidProp`), unit variants only |
| String keys | Fully open | None | Needed for PropsSI and the C ABI | Only at the facade, parsed into the enum |
| Typed methods (`state.cp()`) plus extension traits in downstream crates | Yes | Strongest; each property has its own return type, so tensors and parameters are possible | Needs the enum layer for batch use | **Use it as the main API** |
| Marker-type keys (`get::<Cp>()`, `trait Property { type Value }`) | Yes | Strong | Generic batch functions | Defer until generic code across families needs it |
| `TypeId`/`Any` maps | Yes | Downcasts at runtime | Poor | Reject |

Rules:

- Keep `FluidProp` `Copy + Eq + Hash`; this helps de-duplication and caching in batches.
- Parameterised requests (refractive index at a given λ) therefore go through typed methods or a request struct, not through enum variants carrying an `f64`.
- Key sets belong to a family. CoolProp already keeps PropsSI and HAPropsSI keys apart, and humid-air `H` is per kg of dry air, not the same quantity as fluid `H`.

### 2.5 Data provenance and licensing

Not legal advice. Facts as published by each source.

| Source | Terms (evidence) | Implication |
|---|---|---|
| CoolProp code and fluid JSON | MIT (`reference/CoolProp/LICENSE`). Every JSON block carries BibTeX keys (for example `Wagner-JPCRD-2002`, `Mulero-JPCRD-2012`, `IAPWS-Melting-2011`). | Carry the keys into `Source` (R9). Attribution is handled under D14. |
| CoolProp master's Bollengier coefficients | Extracted from a paywalled journal supplement. Committed as "numerical facts, not a creative work", with sha256 hashes of the supplement files; none of the authors' MATLAB code is used (`master:dev/scripts/extract_bollengier_coefficients.py`). | A precedent for coefficient provenance: cite the paper and hash the source artifact (R9). The legal position is CoolProp's, not verified here. |
| IAPWS releases | "Publication in whole or in part is allowed in all countries provided that attribution is given to the International Association for the Properties of Water and Steam" (cover pages of R10-06, R7-97, R14-08, R9-97, G8-10, G12-15). Release list: [iapws.org](https://iapws.org/technical-guidance/release). | Free to implement with attribution. Each release has check-value tables (R10-06 Table 6; R7-97 Tables 5, 15, 33 and others) that serve as test oracles. |
| NIST works (non-SRD), for example the cryogenic material fits | Works of NIST employees "are not subject to copyright protection in the United States" but "may be subject to foreign copyright". Users should acknowledge NIST, and modified works should say what changed ([NIST licence](https://www.nist.gov/open/license)). The cryogenic pages show no terms of their own ([index](https://trc.nist.gov/cryogenics/materials/materialproperties.htm)). | Usable with attribution and a note of modifications. |
| NIST Standard Reference Data (SRD) | The Secretary "may secure copyright … in all or any part of any standard reference data" (15 U.S.C. 290e). NIST: "None of our SRD may be reproduced … without prior permission" ([SRD law](https://www.nist.gov/srd/public-law)). The Chemistry WebBook is SRD 69, "© 2026 by the U.S. Secretary of Commerce … All rights reserved" ([WebBook](https://webbook.nist.gov/chemistry/)). | Never bulk-embed WebBook or other SRD data. Go to the primary literature. |
| teqp (NIST) | NIST disclaimer, 17 U.S.C. §105 ([licence](https://api.github.com/repos/usnistgov/teqp/license)). | Free to consult and port with acknowledgement. |
| SESAME (LANL) | "The Library is presently being offered to all interested users free of charge", after the user signs an agreement and submits a request form ([LANL](https://www.lanl.gov/engage/organizations/aldsct/theoretical/pcm/sesame)). Export-control terms are unverified. | Never bundle it. At most, an opt-in reader for files the user supplies. |
| LEOS (LLNL) | LLNL's tabular library ([LLNL-TR-705163](https://www.osti.gov/biblio/1330135)); access policy unverified. | Out of scope. |
| CALPHAD databases | "The open elements database is freely available based on the paper by Dinsdale" (SGTE unary, [Wikipedia](https://en.wikipedia.org/wiki/Computational_thermodynamics)). The NIST solder TDB is a direct download, COST507 is "available for free", and the NIMS database requires registration ([OpenCalphad](https://www.opencalphad.com/databases.php)). Thermo-Calc offers "over 40 … databases for use in our software", sold under licence ([Thermo-Calc](https://thermocalc.com/products/databases/)). | Mostly proprietary. If CALPHAD ever comes into scope, treat it as interop with user-supplied TDB files. |
| TEOS-10 GSW code | **Not BSD-3-Clause.** It is a custom BSD-style licence: "Redistribution and use, in source and binary forms, without modification, is permitted provided that …" ([GSW-C](https://raw.githubusercontent.com/TEOS-10/GSW-C/master/LICENSE); GitHub reports it as `NOASSERTION`). | **Corrected:** a translated or modified port is not clearly permitted. Run it as an external test oracle only, and implement IAPWS-06 and IAPWS-08 from the releases. |
| Cantera | BSD-3-Clause ([License.txt](https://raw.githubusercontent.com/Cantera/cantera/main/License.txt)). | Portable with the notice kept. |
| Clapeyron, thermo, chemicals, pycalphad, feos, rfluids, IF97.h | MIT; MIT; MIT; MIT; MIT OR Apache-2.0; MIT; MIT, "Copyright (C) 2015 Ian H. Bell" ([IF97 licence](https://api.github.com/repos/CoolProp/IF97/license)); sources in §2.2.2 | Consulting or porting with attribution is fine. |
| SeaFreeze (includes the Bollengier 2019 LBF); BurnMan; `outram-park-fork-coolprop` | GPL-3.0 ([repo](https://github.com/Bjournaux/SeaFreeze)); GPL-2.0-or-later ([repo](https://github.com/geodynamics/burnman)); GPL-3.0-only ([crates.io](https://crates.io/api/v1/crates/outram-park-fork-coolprop)) | **Do not copy code** into a permissively licensed crate. Running them as external oracles is fine: "copyright law does not give you any say in the use of the output people make from their data using your program" ([GPL FAQ](https://www.gnu.org/licenses/gpl-faq.html#GPLOutput)). Re-implement from the papers. |

### 2.6 Link to the user's request for side-by-side parallel and SIMD versions

The materials seam and the SIMD seam are the same cut: **selection** (branchy, varies per point) versus **evaluation** (straight-line arithmetic over a derivative bundle).
`std::simd` is still unstable. Its tracking issue [#86656](https://github.com/rust-lang/rust/issues/86656) is open (last updated 2026-08-12). The stable docs for Rust 1.99.0 (released 2026-10-01) still mark `std::simd` "nightly-only experimental API" (`portable_simd`).
So SIMD kernels live beside the scalar reference and are checked against it with differential tests (D9).

| Model family | Shape of the inner loop | SIMD fit (analysis) | Thread-level fit |
|---|---|---|---|
| Helmholtz multiparameter, cubic, SAFT | sums of exp, pow and log terms in (τ, δ) | good across states (structure-of-arrays lanes) or across terms | excellent |
| `relations` (Λ bundle → properties) | a few multiplies and one divide | excellent; no branches | excellent |
| Gibbs (IAPWS-06, IF97 regions 1, 2, 5) | polynomial and log sums; complex logs for ice | good | excellent |
| Spline Gibbs (Bollengier, SeaFreeze LBF) | knot search, then a small tensor product | moderate (gather) | excellent |
| Region maps and boundary curves | comparisons and branches | poor within lanes, so partition first | excellent |
| Minimum-Gibbs selection | evaluate every phase, then argmin | acceptable (masked select) | excellent |
| Density and flash solves | iterative, data-dependent iteration counts | moderate (masked lanes with an iteration cap) | excellent |
| Tables (SESAME, TTSE, bicubic, SBTL) | search, gather, small polynomial | limited (gather- and memory-bound) | excellent |
| Correlations (NIST cryogenic, INCOMP) | Horner polynomials | excellent | excellent |
| Saha, CALPHAD equilibrium | nonlinear solve or minimization | moderate or poor | good (across conditions) |

---

## 3. Recommendations for coolprop-rs

### 3.1 Build now: the minimum seams

All of these are needed for CoolProp parity anyway, or cost close to nothing. None adds a dependency.
Module names are placeholders; D1 decides the crate layout.

| Seam | What exactly | Why (evidence) | Conf. |
|---|---|---|---|
| **S1 Family-neutral relations** | `relations::helmholtz`: pure, branch-free functions (p, s, u, h, a, g, cv, cp, w, κ_T, α_p, μ_JT, …) of `(T, ρ, r, &HelmholtzDerivs)`. The bundle holds total Λ_xy up to second order; third-order terms go in a separate type when the fundamental derivative or flash Jacobians need them. The module imports no fluid, JSON or ancillary types. EOS term code computes in its own (τ, δ) and converts with τ^x δ^y. Derivatives can come from hand-coded terms or dual numbers, as D2 decides. | Reused by multiparameter, cubic, PC-SAFT and IF97 region 3 now, and by SESAME-style, SLB and plasma models later, none of which has a natural reducing state (§2.1). The obvious first SIMD target (§2.6). | High / Med (Λ) |
| **S2 Identity / model / package** | `FluidInfo` (`#[non_exhaustive]`; `Option` fields for CAS, formula, InChI, GWP and so on). `HelmholtzEos` (terms, reducing state, `Source`). `Fluid` (info, eos, ancillaries, melting line, transport, surface tension, limits, offset): immutable and shared through `Arc`. | CoolProp's `EOSVector` already holds several EOS per fluid. Alloys and polymers have no CAS number or formula. Avoids renames later. | High |
| **S3 `EnergyOffset { dh, ds }`** | The loader turns the alpha0 offset term (`IdealGasHelmholtzEnthalpyEntropyOffset`, a1 + a2·τ) into Δs = −R·a1 and Δh = Δu = R·T_r·a2. User reference states (IIR, ASHRAE, NBP, DEF) return a new `Fluid` value. Applied as Δa = Δu − TΔs and Δg = Δh − TΔs, it works for any potential (in Λ terms only Λ00 and Λ10 change). | CoolProp mutates the global library (`CoolProp.cpp:982-983`). R10-06 has two s0 values and only the "IAPWS-95" one matches IAPWS-95. Bollengier differs from IAPWS-95 by about 70 J/kg in h at the triple point. Clapeyron aligns reference states per pair. | High / Med |
| **S4 Typed domain errors; checks outside the evaluator** | `#[non_exhaustive] enum DomainError { BelowTmin, AboveTmax, AbovePmax, BelowMeltingLine { t, p, t_melt }, … }`, returned by flash and phase determination. Raw α evaluation stays callable for metastable states. "Don't check limits" is a per-call option, not the global `DONT_CHECK_PROPERTY_LIMITS`. Batch calls return one `Result` per point. `Limits` has private fields so it can grow into a `Domain` (rectangles minus boxes, plus curves) without a break. | Matches CoolProp's behaviour (it throws below Tmelt). Gives a future `Substance` something to delegate on. Min-Gibbs selection and supercooled water (IAPWS G12-15) need metastable evaluation. Bollengier's excluded box shows domains with holes. | High |
| **S5 Read-only state view** | `State` has private fields and getters: `temperature()`, `density_molar()`, `pressure()`, `phase()`, `helmholtz() -> &HelmholtzDerivs`, `fluid_info()`. `Fluid` can evaluate other (T, ρ) states. **Transport is written only against this public API.** | A downstream crate can then add, say, `DielectricExt` (R8-97 needs only T and ρ) without editing core (Open/Closed). Writing transport this way proves the seam works. | High |
| **S6 Per-family property keys plus typed methods** | `fluid::FluidProp`: `#[non_exhaustive]`, unit variants, `FromStr` and `Display` with CoolProp names and aliases, `category() -> Category { Constant, State, Thermo, Transport, Interfacial, Metadata }`. Typed methods are the main API. Missing data returns `Err(PropError::Unsupported)`. | CoolProp keeps PropsSI and HAPropsSI keys apart. Categories make lazy loading of transport and surface-tension data possible (D7). | Med-High |
| **S7 Transport and interfacial models attach to the phase model** | `Fluid { transport: Option<…>, surface_tension: Option<…> }`. The surface-tension doc comment says it is vapor–liquid. | Ice and liquid conductivities differ, so in the future each phase has its own transport model (Cantera attaches `transport:` per phase). | High |
| **S8 Provenance** | A `Source` (constructor plus `#[non_exhaustive]`, owned or interned strings) on every EOS, ancillary, transport model and dataset: BibTeX key, DOI, a `DataTerms` class, and an optional hash of the source artifact. A CI check rejects restricted or copyleft terms in default features. | §2.5. CoolProp already carries BibTeX keys, and master hashes the Bollengier supplement. | High |
| **S9 `#[non_exhaustive]` and private fields wherever types will grow** | `Phase` (CoolProp's 7 real labels; the `unknown` and `not_imposed` sentinels become `Option` and `Result`), `InputPair`, `FluidProp`, errors, metadata structs. `Phase` labels where a state sits on one fluid model. A later `Phase::Solid(SolidId)` is additive. | Non-breaking growth: adding variants to a `non_exhaustive` enum and private fields to a struct that already has one are minor changes ([Cargo SemVer](https://doc.rust-lang.org/cargo/reference/semver.html); [API guidelines C-STRUCT-PRIVATE, C-SEALED](https://rust-lang.github.io/api-guidelines/future-proofing.html)). | High |
| **S10 Selection apart from evaluation** | Phase determination and region choice live in `flash`. Evaluation lives in `eos` and `relations`. The batch API partitions points before evaluating them. | §2.6. The same cut later hosts region maps and min-Gibbs selection. | High |

Module boundaries (dependencies point downwards only):

```text
relations/   S1   no imports from any module below
eos/              Helmholtz terms -> HelmholtzDerivs (uses relations types only)
fluid/       S2, S3, S7, S8   FluidInfo, HelmholtzEos, Fluid, EnergyOffset, Source
state/       S5, S9   State (read-only view), Phase
flash/       S4, S10  selection and domain checks, DomainError
props/       S6   FluidProp, Category, FromStr/Display
transport/   S7   written against the public fluid/ and state/ API only
```

v1 shape (illustrative only; D2, D3 and D5 decide the concrete types):

```rust
/// Total Λ_xy = (1/T)^x ρ^y ∂^(x+y)α/∂(1/T)^x∂ρ^y of α = a/(rT): identical for any reducing pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HelmholtzDerivs { pub l00: f64, pub l10: f64, pub l01: f64, pub l20: f64, pub l11: f64, pub l02: f64 }

pub fn pressure(t: f64, rho: f64, r: f64, d: &HelmholtzDerivs) -> f64 { rho * r * t * d.l01 }
pub fn cp(r: f64, d: &HelmholtzDerivs) -> f64 {
    r * (-d.l20 + (d.l01 - d.l11).powi(2) / (2.0 * d.l01 + d.l02))
}

pub struct Fluid {                       // immutable, Arc-shared, Send + Sync
    info: FluidInfo,                     // identity (S2)
    eos: HelmholtzEos,                   // formulation + Source (S2, S8)
    ancillaries: Ancillaries,            // saturation curves, superancillaries
    melting: Option<MeltingLine>,        // domain boundary (S4); 30 of 136 fluids
    transport: Option<Transport>,        // attached to this phase model (S7)
    surface_tension: Option<SurfaceTension>,
    limits: Limits,                      // private fields: can grow into a Domain (S4)
    offset: EnergyOffset,                // a value, not a global (S3)
}
// DomainError::BelowMeltingLine { t, p, t_melt }   (S4)
```

### 3.2 Do not build now: deferral triggers

| Deferred | Build it when | Why wait |
|---|---|---|
| `PhaseModel` trait, `Substance`, `Selector` | The first non-Helmholtz phase must coexist with a fluid (IAPWS-06 ice for HumidAir or frost; Jäger–Span dry ice) | One implementation is a guess. Adding a trait later is non-breaking. Cantera's lattice classes show the cost of guessing. |
| `relations::gibbs` and `GibbsDerivs` | The same port (IAPWS-06 or IF97) | Small. The specification is R10-06 Table 3: every property comes from six quantities (g, g_T, g_p, g_TT, g_Tp, g_pp). |
| `Domain` with holes | IF97 or Bollengier | `Limits` keeps private fields, so the change is internal. |
| Region map (port of `RegionAtlas`) | IF97 or the SBTL/SVD port | Needs one real consumer to fix the interface. |
| Min-Gibbs selection and aligned offsets per phase pair | Two or more phase models of one substance | Needs a consistent model pair and test oracles (R10-06 g = gL). |
| `correlation` module (polynomial, exp-polynomial, exponential, log-exponential, poly-offset; later log₁₀-rational in T^0.5 and log₁₀ T polynomials) | The INCOMP port. Name it for the mechanism, not for "incompressible": no psat requirement, a validity range and an explicit extrapolation policy (default: none) per property. | One mechanism then serves brines and NIST-cryogenic solids. Mark it as non-potential (possibly inconsistent) data. |
| Bollengier liquid water | After IF97, if high-pressure liquid water is wanted | Master-only. The 8.0.0 oracle lacks it, so check against the paper's supplementary tables and run SeaFreeze as an external oracle only. |
| Solid EOS (Debye, Einstein, MGD, SLB, Birch–Murnaghan, Vinet), ice polymorphs | An explicit request for solids | Larger scope. The isotropic forms reuse S1 unchanged. |
| Tensor values and anisotropy (C_ij, k_ij), strain variables | The first anisotropic material | Changes value types. Only through typed APIs. |
| SESAME and LEOS readers | A user who holds licensed files | Licensing and access agreements; at most an opt-in feature. |
| Plasma: Saha, two-temperature states | An explicit plasma request | Needs internal equilibrium solves and extra state variables (Cantera's plasma phase has its own electron temperature). |
| CALPHAD (TDB, sublattices, equilibrium) | An explicit request; consider interop rather than a port | A whole project; the data are mostly proprietary. |
| Marker-type property keys, plugins or dynamic model loading, non-SI unit systems | Generic code across families needs it | The enum plus typed methods cover v1. Units belong at the facade (D11). |

### 3.3 Suggested order for the first non-Helmholtz models (confidence: medium)

1. **IAPWS-06 ice Ih (Gibbs).**
   - About 20 coefficients, with Table 6 check values.
   - CoolProp's HumidAir needs it (`psub_Ice`, `dg_dp_Ice`, `h_Ice` for the wet bulb below freezing, and the `HAProps_Aux` outputs).
   - Its "IAPWS-95" s0 is aligned with IAPWS-95 by design. `Water.json` alpha0 has no offset term (only Lead, LogTau and Planck–Einstein), so CoolProp water uses IAPWS-95's own reference.
   - It exercises S1–S5 end to end, including min-Gibbs melting and sublimation checked against R14-08.
2. **IF97.** Gibbs regions plus a Helmholtz region 3 plus an explicit map. This is where the `RegionAtlas` port and the `Domain` type come in. HumidAir also uses `psat97`.
3. **INCOMP correlations**, built as the family-neutral `correlation` module that NIST-cryogenic solids reuse later.
4. **Bollengier**, only if wanted (see §3.2).

Test oracles for these families are not CoolProp alone. Use the IAPWS check tables, TEOS-10 GSW-C (run as an external oracle only; its licence forbids modified redistribution, see §2.5), and Clapeyron's `IAPWS06` and `JagerSpanSolidCO2` (MIT) as cross-checks, with the papers as the final arbiter.

---

## 4. Warnings

- **The Bollengier backend is not in CoolProp v8.0.0.** The task brief assumed `src/Backends/Bollengier`; that path exists only on master, added on 2026-10-03 (`6d788a7a`, #3426). The 8.0.0 wheel therefore cannot serve as its oracle. The same model ships in SeaFreeze, which is **GPL-3.0**.
- **CoolProp's ice uses a superseded coefficient.** `src/Ice.cpp:21` has `g00 = -0.632020233449497e6`. R10-06(2009) Table 2 gives `−0.632 020 233 335 886 × 10⁶`, because "a minor adjustment of the coefficient g00 … improves numerical consistency" with IAPWS-95. The difference is about 1.1×10⁻⁴ J/kg in g, h and u of ice. It reaches `h_Ice` in the wet-bulb path and the `HAProps_Aux("g_Ice"/"h_Ice")` outputs. Record it in the divergence register and follow the 2009 release.
- **Reference states must match across phase models**, or computed phase boundaries are silently wrong. R10-06 gives an "IAPWS-95" s0 and an "absolute" s0; only the first is consistent with IAPWS-95 liquid at the triple point. Bollengier as published sits about 70 J/kg away from IAPWS-95 in h at the triple point, and CoolProp's `set_reference_stateS` silently ignores that backend. Never port the global mechanism.
- **Fitted surfaces can be thermodynamically inadmissible inside their own published range.** Bollengier has cv ≤ 0 or (∂v/∂P)_T ≥ 0 in a corner of its rectangle; CoolProp excludes a box. Property tests must check admissibility (cv > 0, κ_T > 0) over each declared domain, and domains must be able to carry holes.
- **Ancillary boundaries are fits.** CoolProp allows 1 mK of slack (`_T < Tm - 0.001`). Near triple points, a fitted boundary can disagree with g-equality. Once real solid models exist, use curves only as a first guess.
- **Correlation data is internally inconsistent.** NIST cryogenic fits have 1–10 % fit error, and cp, expansion and k are fitted separately. Keep them out of Maxwell-relation and other consistency property tests. Never extrapolate outside the fitted range by default.
- **"Every property is an f64" breaks** for crystals (tensors) and for parameterised properties such as n(λ) or frequency-dependent ε. Keep the dynamic enum scalar-only by design.
- **A state of just (T, ρ) breaks** for non-equilibrium plasma (a separate electron temperature, as in Cantera's plasma phase) and for anisotropic solids (strain). This is why the fields must be private (S5, S9).
- **Composition is not always mole fractions.** INCOMP uses mass, mole or volume fraction (`IFRAC_*`), TEOS-10 uses absolute salinity, and humid air uses the humidity ratio. Do not put a universal `x: Vec<f64>` into the state contract (D4).
- **Thread-local or global mutable caches are a smell.** HumidAir keeps `thread_local` backends and `MeltingCaloric` keeps a mutex-guarded global map. Use immutable models with `OnceLock`-built derived data (D8).
- **Do not build speculative solid classes.** Cantera 3.2 deprecated `LatticePhase` and `LatticeSolidPhase` for redundancy, consistency bugs and low use.
- **Licences.** SESAME needs a signed agreement and a request form (export-control terms unverified). NIST SRD (for example WebBook SRD 69) is copyrighted. CALPHAD databases are mostly commercial. SeaFreeze, BurnMan and `outram-park-fork-coolprop` are GPL; do not copy their code. TEOS-10 GSW-C is **not** BSD-3-Clause: it permits redistribution only "without modification", so use it as an oracle, never as a source to port.
- **Λ convention and exact parity.** Converting CoolProp's (τ, δ) derivatives to Λ adds multiplications by τ^x δ^y. That should stay within the oracle tolerance, but confirm it on the 5-fluid TDD set before fixing the bundle layout.
- **Versions as of 2026-10-04:** Cantera 3.2.0, Clapeyron 0.6.29 (tagged 2026-09-27), thermo 0.6.1, chemicals 1.5.2, pycalphad 0.11.2, feos 0.10.1, rfluids 0.6.0, SeaFreeze 1.1.3, BurnMan 2.1.0.

## 5. Open questions

1. Will HumidAir (HAPropsSI) be ported? If so, IAPWS-06 ice and IF97 are part of the CoolProp parity work, not speculation, and §3.3 steps 1–2 move into the main milestone order.
2. For parity, HumidAir switches between liquid and ice at exactly 273.16 K at every pressure. It is not even consistent at the boundary point: some call sites use `>` and others `>=` (§2.2.1). Keep that switch as is (parity) and record a min-Gibbs version as a later, documented divergence?
3. What basis should materials without a well-defined molar mass (alloys, polymers) use: mass basis throughout, or molar per formula or repeat unit? This sets the `r` passed to S1.
4. Should the bundle carry total Λ only (recommended), or also the residual split for fluid-only outputs and low-density precision? D2 or D3 should settle this from fixture tolerance results.
5. Should a future `Substance` hold phases as `dyn PhaseModel` (open to other crates) or as a closed enum (faster and simpler, but requires editing core)? This depends on D3. Because batches are partitioned first, per-group dynamic dispatch costs very little.
6. Scope: is CALPHAD meant as interop (TDB files plus an external solver) or as a native port? Is "plasma" equilibrium only (Saha), or also non-equilibrium?
7. Should melting-line segments get optional `solid_phase` labels in the fluid data now (data only, no code)? Recommended as a later data migration, not for v1.
8. Is Bollengier (CoolProp master, after 8.0.0) in scope at all, given that its oracle must be the paper and SeaFreeze rather than CoolProp 8.0.0?

## Verification log

**Date:** 2026-10-04. Adversarial fact-check against primary sources: crates.io API, PyPI JSON, the GitHub API (releases, PRs, issues, licence files, source at the cited commits), the Rust stable channel manifest and docs, IAPWS release PDFs, NIST, LANL, OSTI, TEOS-10, Cantera, thermo, pycalphad and teqp docs, and the local CoolProp checkouts `ae81610e` (v8.0.0) and `022b63e4` (master).

**Claims checked: 118.**
- **Local source (about 45 claims).** Line citations in §2.2.1: CubicBackend, PCSAFT, the IF97 fetch and ε, the INCOMP enums, Ice.cpp, HumidAir, the Tmelt throws and the opt-out, MeltingCaloric, RegionAtlas, `set_reference_stateS`, `EOSVector`, the parameters, phases and `fast_evaluate` enums, and the Bollengier header and extractor script. Counts in the fluid JSON, recomputed: 136 fluids; 30 with a melting line (Simon 18, Tr 9, Theta 3); 23 with more than one EOS; Water's alpha0 terms and four melting segments.
- **Versions, dates and licences (about 30 claims).** Cantera 3.2.0 (2025-11-18, BSD-3-Clause) and PR #1959 (merged 2025-09-08). Clapeyron 0.6.29 (MIT). thermo 0.6.1, chemicals 1.5.2 and pycalphad 0.11.2 (MIT). BurnMan 2.1.0 (2024-11-28, still the latest; GPLv2+). SeaFreeze 1.1.3 (GPLv3). feos 0.10.1 (MIT OR Apache-2.0; traits checked at `246e563d`). rfluids 0.6.0 (MIT; the `Substance` variants and strum `EnumString`). seuif97 2.3.8, iapws95 0.2.5 and vle-steam 0.16.0. tpt-eng-materials 0.1.0. outram-park-fork-coolprop 0.1.2 (GPL-3.0-only). "calphad" returns zero crates. IF97 is MIT. teqp carries the NIST disclaimer. GSW-C (see corrections).
- **Rust (5 claims).** The Cargo SemVer rules for new items, `non_exhaustive` variants and private fields. API guidelines C-STRUCT-PRIVATE and C-SEALED. `std::simd` is still unstable on Rust 1.99.0.
- **Literature and quotes (about 38 claims).** IAPWS: the R10-06 g00 values (2006 value confirmed against Feistel and Wagner 2006), the two s0 values, Tables 3 and 6, the attribution clause on six releases, R14-08 segment temperatures and wording, the R8-97 and R9-97 ranges, G8-10 and G12-15. The teqp Λ definitions and formulas; the §2.1 Λ table was re-derived by hand. Quotes from the Clapeyron HISTORY, types, IAPWS06 and Jäger–Span sources. Cantera phase, thermo and Python-doc quotes. thermo phases, flash and utils quotes. pycalphad model contributions. BurnMan EOS classes and `set_state`. NIST licence, SRD law and WebBook notice. LANL SESAME page. SESAME table numbers (USim). LANL opacity tables. OSTI LA-14503 and LLNL-TR-705163. OpenCalphad and Thermo-Calc. GPL FAQ. Stixrude & Lithgow-Bertelloni. arXiv:1903.11730. The Saha and Mie–Grüneisen pages.

**Corrections made:**
1. **TEOS-10 GSW-C licence (changes a recommendation).** It is not BSD-3-Clause. It is a custom licence that permits redistribution only "without modification". §2.2.2, §2.5, §3.3 and §4 now say: use it as an external oracle only and port nothing from it. Also noted that GSW-C's source does contain IAPWS-06 ice functions.
2. **Clapeyron `CompositeModel`.** It does not generally "compare Gibbs energies when the phase is unknown". Only `volume` does so, and only when both models are `a_res` models. Bulk `PT_property` with an unknown phase throws "automatic phase detection not implemented". §2.2.2, §2.2.3 and §2.3 are updated. The recommendation (follow Clapeyron's additive Gibbs family) is unchanged, but Clapeyron is no longer cited as doing automatic min-Gibbs selection.
3. **SeaFreeze.** Not every phase is an LBF fit. Ice Ih cites Feistel and Wagner 2006 (IAPWS-06) and ice VII/X cites French and Redmer 2015 (§2.1, §2.2.2).
4. **NIST OFHC copper fit errors.** cp and expansion are 10 % (for T ≥ 15 K and T ≥ 50 K respectively), not "5–10 %".
5. **Alpha0 offset count.** 55 of 136 is for the default `EOS[0]`; 65 fluids carry the term in some EOS entry.
6. **Bollengier excluded box.** The inadmissible sub-region is p 1833.5–2300.6 MPa and T 240–249.6 K, and a near-zero-cv margin surrounds it. The box covers both; it is not all inadmissible.
7. **HumidAir 273.16 K switch.** Call sites disagree at the boundary point (`>` vs `>=`). Added to §2.2.1 and open question 2.
8. Smaller precision fixes:
   - Cantera's deprecation is dated (3.2, 2025) instead of "just".
   - Clapeyron 0.6.29 is a tagged release (2026-09-27), not only master.
   - Added the SemVer glob-import caveat.
   - Clarified the R10-06 Table 3 "six numbers".
   - Added the Rust 1.99.0 status to the `std::simd` statement.

**Still unverified:**
- SESAME 401/411/412/431 table numbering. A search confirms only that a "401 table" exists.
- SESAME export-control terms.
- LEOS access policy.
- The TREND 2.0 abstract quote. It was matched only through a search-engine index, because the publisher and catalogue pages blocked automated access.
- The Debye-model formula, which was not re-fetched; it is a textbook result.
- The claim that Thermo-Calc databases are "sold under licence" (an inference from the product page).
- That a Gibbs-based Clapeyron solid (`IAPWS06`) lacks `a_res`. Inferred from `has_a_res` being method-based; not executed.
