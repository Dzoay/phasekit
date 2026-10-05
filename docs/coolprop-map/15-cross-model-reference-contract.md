# 15 Cross-backend reference-state and energy-zero contract - CoolProp v8.0.0 map

> Scope: `src/CoolProp.cpp:946-1040`, `src/Backends/Helmholtz/HelmholtzEOSMixtureBackend.cpp:4463-4600`, `src/Backends/Helmholtz/Fluids/FluidLibrary.cpp:64-127`, `include/CoolProp/fluids/Helmholtz.h:930-981,1386-1431`, `src/Backends/Helmholtz/FlashRoutines.cpp:536-552,712-780`, `src/Backends/Helmholtz/MeltingCaloric.cpp:229`, `src/Backends/IF97/IF97Backend.h`, `src/Backends/Incompressible/IncompressibleBackend.{h,cpp}` (reference parts), `src/Backends/Cubics/CubicBackend.cpp:73-89`, `src/Backends/PCSAFT/PCSAFTBackend.h:125-140`, `src/HumidAirProp.cpp:45-108,170-329,1077-1112,1333-1380`, `src/Ice.cpp`, `src/Backends/Tabular/TabularBackends.{h,cpp}` (cache keys and load checks), `src/Backends/SVDSBTL/SVDSBTLBackend.cpp:236-290`, `src/SBTL/SVDSurfaceSerializer.cpp:538-604`, plus `dev/fluids/*.json` and `dev/cubics/all_cubic_fluids.json` offset terms. About 1.1k lines are relevant. Part of the coolprop-rs port plan; cites the v8.0.0 source.

Conventions: plain `path:line` means the v8.0.0 checkout. `master:` means origin/master. "Oracle" means a probe against the CoolProp==8.0.0 wheel (gitrevision ae81610e), with caches redirected to the scratchpad. *(inference)* marks a conclusion that is neither stated by the code nor measured. This map does not repeat the following, which are covered elsewhere: the HEOS offset algebra and the IIR/ASHRAE/NBP targets (02 §3.5); the global-mutation and thread-safety rot of `set_reference_state` (01 R14, 09 R4, 11 U2/U17); the cubic entropy bug (06 C1); the stale cubic dataset (06 C5); the HA and INCOMP model details (07).

## 1. Purpose and concepts

- **Energy zero (gauge).** Only h and s carry an arbitrary additive pair (Δh, Δs) per component. The quantities that follow from them are:
  - u' = u + Δh;
  - g' = g + Δh − TΔs and a' = a + Δh − TΔs;
  - for a mixture, h' = h + Σxᵢ·Δhᵢ (molar basis); the same sum applies to s.
  - Everything else is **gauge-invariant**: p, ρ, cp, cv, w, residual and departure properties, every Δh or Δs along a process, and μᵢ differences between phases of one component.
  - The gauge only matters when values from different models or different datasets are combined. Examples: energy balances across models, inverse flashes with h or s inputs on surrogate tables, and reactions (formation enthalpies).
- **What CoolProp does.** Each backend picks its own zero (table §2.1). The only user control is `set_reference_state`, and it works for HEOS only. HEOS stores the gauge *inside* α⁰ as an `IdealHelmholtzEnthalpyEntropyOffset` term (`Helmholtz.h:930-981`), and that term is evaluated on every α⁰ call (`:1430-1431`). Other backends either ignore the call or never receive it. Nothing defines when h or s from two models of one substance may be compared.
- **Why it decides the Rust design early.** 07 §1 requires "one substance, many models". Composite models already mix sub-models internally: HA uses IAPWS-95, IF97 and ice; the wet bulb uses HEOS liquid; tables are built from a source model. The gauge therefore has to be a value outside the EOS coefficients that every model declares, otherwise composites and caches silently mix zeros (bugs X1 and X2 in §6).

## 2. Structure (key types/functions -> path:line)

### 2.1 Per-backend zero convention (the table this map exists for)

| Backend | Native h/s/u zero | Source of the zero | `set_reference_state` effect (8.0.0) | Tables/surrogates stale after a change? |
|---|---|---|---|---|
| HEOS pure | Per-fluid **paper convention**, baked into the `IdealGasHelmholtzLead` a1/a2 terms. 65/136 fluid files add a JSON core offset `EnthalpyEntropyOffsetCore`: NBP 33, IIR 27 (some fluids have two EOS entries), OTH 4, CUSTOM 1. 71 have none (census of `dev/fluids/*.json`). Water: IAPWS-95, u = s = 0 for the saturated liquid at the triple point. R134a: IIR from the Lead term, oracle h(0 °C, Q=0) = 199 999.9885 J/kg. Air: "OTH", h(273.15 K, 1 atm) = 399 287.16 J/kg | `FluidLibrary.h:323-327` parses the core offset; user offset `Helmholtz.h:1386`; set via `CoolProp.cpp:969-1024` → `FluidLibrary.cpp:64-127` | Yes, but **only for states constructed afterwards** (02 §3.5, 01 R14). Spellings `HEOS?::X` and `HelmholtzEOSBackend::X` are silent no-ops (oracle) | n/a |
| HEOS mixture | *Intended* Σxᵢ of the component gauges, but α⁰ = Σxᵢ(Rᵢ/R)(α⁰ᵢ(τᵢ, δᵢ) + ln xᵢ) with τᵢ = **T_c,i**·τ/T_r (JSON critical T, not the component's reducing T), so a component offset a2 shifts the mixture h by R·T_c,i·a2 instead of R·T_r,i·a2. The mixture gauge is **not** Σxᵢ·(pure gauge) for the ≥15 fluids with T_c ≠ T_r (§6 X12) | `HelmholtzEOSMixtureBackend.cpp:3640, 3649` (and `:3698` in the all-derivatives path) | Per component via the library | n/a |
| IF97 | IAPWS R7-97: u = s = 0 for the saturated liquid at the triple point (`Web/fluid_properties/IF97.rst:81`). Oracle at 273.16 K: h = 0.61178, u = 0, s = −6.2e-5 J/kg/K | IF97 coefficients | **None**. The free function returns silently (no `else`, `CoolProp.cpp:1026`); the OO base throws (`AbstractState.h:821-834`) | SVDSBTL&IF97: no (the gauge is fixed) |
| INCOMP | h = s = 0 at 293.15 K, 101 325 Pa, **at the current composition x** | `IncompressibleBackend.h:89` defaults; ctor `.cpp:32,47`; re-pinned on every x change (`:208`); applied at `:513-518`. Documented: "the reference state gets updated each time you change the composition" (`Web/fluid_properties/Incompressibles.rst:49-50`) | **None** from the API. The 5-argument member `set_reference_state` (`.h:89`) neither overrides nor is reachable through `set_reference_stateS/D`. The free function is a silent no-op | n/a |
| PR / SRK (and VTPR) | **Ideal-gas anchored**: residual(cubic) + α⁰ copied from the `all_cubic_fluids.json` snapshot, including any snapshot core offset. Evaluated at τ* = Tc,cubic/T with R = CODATA 2018 | `CubicsLibrary.cpp:50-51`; placeholder `CoolPropFluid` carrying only α⁰ (`CubicBackend.cpp:73-89`); `HelmholtzEOSMixtureBackend.cpp:3684` | **None.** `set_reference_state("PR::X", …)` is a silent no-op that does not even validate the string: "BOGUS" is accepted (oracle). HEOS changes do not propagate (separate data). Fixed to raise on master `80dd7beb` | n/a |
| PC-SAFT | **No gauge at all**: no ideal-gas part, so h, s, cp and w throw. Only residual h, s, g exist, and those are gauge-free (oracle: `hmolar_residual` = −389.9 J/mol for methane at 1000 mol/m³, 300 K) | `PCSAFTBackend.h:129-138` | n/a (silent no-op) | n/a |
| Humid air (`HAPropsSI`) | RP-1485 (Herrmann et al. 2009). Dry air: h = 0 at 273.15 K, 101 325 Pa (oracle −2.7e-6 J/kg; s there = 0.00143 J/kg/K). Water vapour: IAPWS-95 ideal gas, **h with R̄ = 8.314472** (`:110, 1083`) but **s with 8.314371** (IAPWS-95's own R, `:271, 1088`); dry air with Lemmon R = 8.314510 (`:272, 1097, 1104`). Each is pinned at 473.15 K to a hard-coded molar value plus a legacy constant (`:281-326` + `:1083, 1101, 1110`). Liquid and ice: IAPWS-95 / R10-06 | Hard-coded `href`/`sref` + offsets `HumidAirProp.cpp:267-329`; constants `:1083,1101,1110` | **Mixture h/s/u are invariant**: the offsets are recomputed against the per-thread backend (`:281-326`), so any user offset cancels (oracle, bit-identical). **Wet bulb is not** (§6 X1) | n/a |
| Ice Ih | IAPWS R10-06: s₀ = −3327.34 J/kg/K is the "IAPWS-95" absolute-entropy choice, so ice is anchored to the IAPWS-95 liquid zero (g00 is the 2006 value, a 1.1e-4 J/kg mismatch, 07 C1) | `src/Ice.cpp:20-26,51-63,118-121` | None (pure constants) | n/a |
| TTSE / BICUBIC&HEOS | Whatever the source HEOS gauge was **at build time** | Tables store absolute h, s, u nodes (`TabularBackends.cpp:225-351`) | Set calls on `BICUBIC&HEOS::X` are silent no-ops. Changes made through `HEOS::X` are not seen | **In memory: yes (stale).** Disk: rebuilt only by accident (§6 X2) |
| SVDSBTL&HEOS | Source gauge at build time | SVD modes of absolute h/s | As above | **Yes, via the disk cache**: surfaces are per instance (`surfaces_`, `SVDSBTLBackend.cpp:983`), but every new instance reloads the gauge-blind file (`:997-1003`), even in a fresh process (§6 X2) |
| GERG (master only) | **Ideal-gas** h = s = 0 at 298.15 K, 101 325 Pa (integration constants recomputed; 06 §3 GERG bullet and open question 8, `master:GERGBackend.cpp:1029`) | `master:GERGBackend.cpp` | n/a in the oracle | — |

### 2.2 Mechanisms

| Mechanism | Location | Note |
|---|---|---|
| Gauge as an α⁰ term (a1 + a2·τ), two slots: core (JSON) + user | `Helmholtz.h:930-981, 1386, 1430-1431` | `set()` **increments** when enabled, "DEF" zeroes; the doc formula at `:170` divides by T where the code uses T_r |
| Free `set_reference_stateS/D` | `CoolProp.cpp:946-1040` | dispatches on the literal backend token; `D` builds `HEOS(FluidName)` from the raw string |
| Library write + recomputation of the hs_anchor, reducing, critical and triple h/s | `FluidLibrary.cpp:64-127` | silently ignores names absent from `string_to_index_map` (the outer `if` at `:67` has no `else`). Reachable from the free function only via the `X-SRK` / `X-PengRobinson` pseudo-names, which `JSONFluidLibrary::get` resolves (`FluidLibrary.h:1229-1254`) but the offset writer does not; any other unknown fluid throws earlier, in the `HEOS` constructor (oracle). Still present on master (§6 X13) |
| Per-instance copy | `HelmholtzEOSMixtureBackend.cpp:4463-4600` | duplicate of the free function; edits only `this->components` |
| Frame translation for cached caloric surrogates: "stamp" (a1, a2) at build, shift the target at query | `FlashRoutines.cpp:542-552, 757-775`; `MeltingCaloric.cpp:229` | **The one place CoolProp does it right** (#2773). Tables and SVDSBTL do not use it |
| HA re-anchoring | `HumidAirProp.cpp:267-329` (`ensure_ref_offsets`, `thread_local s_ref`) | Re-anchors the mixture h/s at fixed molar values, so user gauge changes cancel |
| Table cache keys | `TabularBackends.cpp:352-366`, `.h:1009-1020` (`backend(fluid[x])`); `SVDSurfaceSerializer.cpp:566-604` (`fluid.source.pair.opthash`) | Neither key includes the gauge or a model fingerprint |

## 3. Algorithms and formulas (cite the papers CoolProp cites)

- **Gauge algebra** (Span 2000; CoolProp IECR 53 (2014) 2498). Adding a1 + a2·τ to α⁰ gives Δs = −R·a1 and Δh = R·T_r·a2 (molar). The code computes Δa1 = Δs/(R/M) and Δa2 = −Δh/((R/M)·T_r) (`CoolProp.cpp:978-981`). The algebra is exact and T-independent, which is why the #2773 stamp shift works (`FlashRoutines.cpp:762-775`).
- **Shared-α⁰ models are not equal-gauge models.** h⁰ = RT(1 + τα⁰_τ) and s⁰ = R(τα⁰_τ − α⁰). If two models share the α⁰ coefficients but differ in R or T_r, they agree only at the anchor, and **h and s scale with R** (a model or constant difference, not a constant shift). Oracle, ideal gas at 1 Pa:
  - PR vs HEOS Water: dh/h = +1.10e-5 at every T (300-1000 K). That equals 8.31446262/8.31437136 − 1, the CODATA 2018 / IAPWS-95 R ratio.
  - Methane: −5.70e-6. Propane: −1.13e-6.
  - R134a: dh = +15.8 → −35.4 J/kg over 250-1000 K and dcp⁰/cp⁰ = −5.2e-5, because Tc,cubic = 374.21 ≠ T_r,HEOS = 374.18.
- **HA ideal parts** (RP-1485 §3, `HumidAirProp.cpp:1077-1112`): h̄_w = c + Δ_w + R̄·T(1 + τα⁰_τ) with R̄ = 8.314472. Δ_w is fixed by h̄(473.15 K) = 51 885.58 J/mol (`:281-288`). The resulting **enthalpy** is the IAPWS-95 ideal gas scaled by 8.314472/8.314371 − 1 = 1.21e-5; the **entropy** uses 8.314371 (`:1088`) and is not scaled. Oracle: HA dH/dW − h_HEOS,ig = +30.6, +31.0, +31.5 and +33.4 J/kg at 280, 300, 320 and 400 K (100 Pa, where the virial contributions are negligible).
- **INCOMP** (07 §3.2): h(T,p,x) = h_ref + [∫c dT + p·v(1 + Tρ⁻¹∂ρ/∂T)](T,p,x) − [same](T_ref,p_ref,x). x_ref is always the current x (`IncompressibleBackend.cpp:208`).
- **IF97** (IAPWS R7-97(2012) §5): its zero is the IAPWS-95 zero, so IF97 and HEOS differences are fit differences.

### 3.1 Measured cross-model offsets (oracle; Δ = model − HEOS)

| Substance, state | Model | h [J/kg] | Δh | Δs [J/kg/K] | Class |
|---|---|---|---|---|---|
| Water 300 K, 1 atm | HEOS (IAPWS-95) | 112 654.90 | 0 | 0 | reference |
| | IF97 | 112 665.04 | +10.14 | +0.0346 | **model difference**. Same zero (both 0.61178 J/kg at the triple point). Over the IF97 range Δh runs from −0.17 (275 K) to +235 (1000 K, 50 MPa) |
| | INCOMP | 28 589.39 | −84 065.51 | −296.66 | **constant shift** (−h_HEOS(293.15 K, 1 atm) = −84 007.30, s −296.463) **plus a model difference**: after removing the shift, Δh = +309.8 / −58.2 / −341.9 / −772.9 at 275 / 300 / 330 / 360 K |
| | PR | 18 481.53 | −94 173.37 | −8270.08 | Ideal-gas anchored. Liquid Δh is the **residual-model difference** (a cubic cannot represent liquid water); Δs is dominated by the **06 C1 bug**. At the ideal gas (600 K, 1 Pa) Δh = +34.37 (R ratio), Δs_raw = −4749.18, while Δs from (h−g)/T = +0.155 (R ratio) |
| | SRK | −53 298.76 | −165 953.66 | −8451.63 | as for PR |
| | HA internal water vapour | — | +31.0 (ideal gas, 300 K) | — | **scale mismatch** (RP-1485 R̄ in h only): Δh ≈ 1.21e-5 × h_ig, so it grows with T |
| R134a 300 K, 1 atm | HEOS | 426 102.82 | 0 | 0 | reference (IIR) |
| | PR / SRK | 426 562.91 / 426 582.04 | +460.1 / +479.2 | −1339.3 / −1339.4 | residual difference + C1. At 1 Pa: Δh = +13.76 (Tc/R), Δs_raw −1340.7, (h−g)/T: +0.56 |
| R134a, saturated liquid at 0 °C | PR / SRK | 199 831.83 / 197 511.32 | −168.2 / −2488.7 | −1402.2 / −1411.6 | residual difference + C1 |
| Dry air 273-400 K, 1 atm | HA vs HEOS::Air | — | −399 287.2 (273.15 K) to −399 289.5 (400 K) | −3792.391 to −3792.398 | **constant shift** (different reference states) plus a ~2 J/kg drift (model and R differences) |

**All 116 cubic fluids, PR vs HEOS in the ideal-gas limit** (ρ = 1e-6 mol/m³; 1.2·Tc, 1.8·Tc and 2.4·Tc with a 300 K floor; s from (h−g)/T to bypass C1):
- **~96 fluids: |Δh| small** (median ≈ 1.2-1.4 J/kg), mostly the R ratio. The bucket is threshold-dependent (verifier rerun with a |Δh| < 200 J/kg cut: 97/4/15) and is **not pure R ratio**: Neon +101.8 and Helium +98.5 J/kg are small **constant shifts** from snapshot offsets that differ from HEOS (Neon: snapshot has an offset block, HEOS none; Helium: different a1/a2), and MM drifts −69 → −139 J/kg (T-dependent).
- **4 fluids: large constant shifts.** Δh / Δs: HCl −536 161 / −2743.45; R40 −471 782 / −1874.35; OrthoHydrogen −444 925 / −17 050.5; DiethylEther −349 203 / −1129.49. For R40, OrthoHydrogen and DiethylEther the HEOS JSON has an offset block the snapshot lacks; for HCl neither has one and the `IdealGasHelmholtzLead` a1/a2 differ (7.9567/−3.2171 vs −4.0690/4.0258), with Tc 324.55 vs T_r 324.68.
- **15-16 fluids: T-dependent** α⁰ differences (06 C5), several with a large shift on top: R1234ze(Z) −441 680 → −414 115; HeavyWater −408 525 → −412 836; R1233zd(E) +36 205 → +52 592; D5, MD4M, Ammonia from −15 804 to +131 516.
- By CAS, 24 of the 116 snapshot α⁰ blocks differ from HEOS EOS[0] (rerun: 24). For Methanol the whole snapshot α⁰ is HEOS **EOS[1]** (Piazza 2013: offset a1/a2 = −18.6556/−3195.19 and Tc 512.5 match EOS[1], not EOS[0] de Reuck, T_r 513.38). Both are NBP-anchored, so the effect is only −550 → +698 J/kg (T-dependent), not a large shift.

## 4. Data and configuration inputs

| Input | Location | Note |
|---|---|---|
| Core offsets | `dev/fluids/*.json` `IdealGasHelmholtzEnthalpyEntropyOffset` {a1, a2, reference} | 65 files. Labels are strings (IIR/NBP/OTH/CUSTOM) with no machine-checkable target state |
| Cubic α⁰ snapshot | `dev/cubics/all_cubic_fluids.json` `alpha0` | Duplicates HEOS data; 24/116 drifted (above) |
| HA anchors | `HumidAirProp.cpp:281-326` (href 51 885.582451893446, sref 141.18297895840303, 13 782.240592933371, 212.22365283759311 at 473.15 K); `:1083` (−0.01102303806), `:1101` (−7914.149298), `:1110` (−196.1375815) | The code cites "ASHRAE RP-1485 §3" for the water enthalpy offset (`:279`) and "Lemmon 2000" for air (`:301`), but no equation or table numbers, and nothing for the three legacy constants |
| INCOMP reference | `IncompressibleBackend.h:89` default arguments (293.15 K, 101 325 Pa, 0, 0) | Lies **outside** the fit range for 9 fluids (oracle): pure LiqNa (Tmin 400 K), NaK (573.15 K), PBB, ExamplePure; solutions IceEA, IcePG, IceNA (Tmax ≤ 270 K), VMA, ExampleSolution. Their zero is a polynomial extrapolation (NaK h(Tmin) = 411 744 J/kg) |
| Table cache dirs | `ALTERNATIVE_TABLES_DIRECTORY` (needs a trailing `/`, 08 §4), `ALTERNATIVE_SVDTABLES_DIRECTORY` | Cache keys omit the gauge |
| Superancillary stamp | `superanc.get_caloric_alpha0_stamp()` (`FlashRoutines.cpp:757`) | Recorded at first build |

## 5. State, caching, globals, thread-safety, memory

- **The gauge lives in mutable model data.** The user slot sits in each `CoolPropFluid` copy, so its lifetime is that of the copy:
  - library copy: process-global, written without a lock (09 R4);
  - per-AbstractState copy: frozen at construction (01 R14);
  - SatL/SatV copies: frozen at their own construction. `sync_linked_states` syncs only the residual and reducing terms (`HelmholtzEOSMixtureBackend.cpp:161-170`), so a **per-instance** `set_reference_stateS/D` leaves SatL/SatV on the old gauge *(inference from code; the per-instance API is not exposed in Python, so this is unverified by oracle)*;
  - HA `thread_local` Water/Air backends (`HumidAirProp.cpp:51-53, 96-108`): frozen at the **first HA call in each thread**.
- **The result depends on the thread** (oracle, X1). `HAPropsSI('Twb', T=300 K, 1 atm, R=0.3)` gives 288.874899 K. After `set_reference_state('Water','NBP')` it gives 288.874899 K in the same thread but **288.248082 K in a new thread**. After 'DEF', a new thread gets 288.874899 K again.
- **Tabular memory caches.** `TabularDataLibrary::data` (process-global `static` library, `TabularBackends.cpp:24`, path key) outlives any gauge change. SVDSBTL surfaces are per instance (`SVDSBTLBackend.cpp:983`); their staleness comes from the gauge-blind disk cache that every new instance reloads.
- **Memory.** A gauge costs 2 doubles per component. Because CoolProp keeps it inside α⁰, every reference change copies whole fluids and rebuilds them.

## 6. Rot and bugs (table: issue | evidence | impact | Rust remedy)

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| X1 | **Wet bulb mixes gauges.** The HA mixture h is re-anchored (invariant), but liquid h_w comes from the thread-local HEOS Water, which carries the user offset | `HumidAirProp.cpp:1364-1370` vs `:281-326`. Oracle: Twb (300 K, R=0.3) 288.8749 → 288.2481 K (−0.63 K) after `set_reference_state('Water','NBP')`; H, S, Hha, U and T(H, R) unchanged; the shift appears only in threads created after the call (§5). Refutation attempt: the docs say to change the reference state only at program start (`Web/coolprop/HighLevelAPI.rst:390`); doing exactly that (fresh process, `set_reference_state` before any HA call) still gives Twb = 288.248082 K (verifier). So the wet bulb is gauge-dependent under documented use; the thread dependence is an extra symptom of mid-run changes | Wrong wet bulb, thread-dependent results | Composite models hold sub-models **in their native gauge**; the user gauge is applied only at the output boundary |
| X2 | **Surrogate tables bake the gauge and are not keyed by it** | Keys `TabularBackends.cpp:352-366` and `SVDSurfaceSerializer.cpp:604` have no gauge. Oracle (R134a, 1 bar, 300 K): after `set_reference_state('R134a','ASHRAE')`, HEOS gives 277 982.72 J/kg, while new BICUBIC, TTSE and SVDSBTL instances in the same process give 426 126.77 (stale by 148 144 J/kg). An `HmassP` update at the new-gauge h returns **T = 246.79 K instead of 300 K** (BICUBIC, TTSE and SVDSBTL; verifier rerun). In a fresh process (ASHRAE first, tables from disk, i.e. the documented init-time use), SVDSBTL is still stale (426 126.77; its `HmassP` at the new h returns NaN). BICUBIC rebuilds, but only because the h-axis limit check at `.h:838-840` happens to fire (08 R13: the check divides by signed `xmin`) | Silently wrong states | Tables store **native-gauge** data under a key that fingerprints the model; gauge translation of inputs and outputs is the #2773 stamp pattern, made general |
| X3 | `set_reference_state` is a **silent no-op** for every backend except the literal tokens `HEOS`, `?` and `REFPROP`, and the reference-state string is not validated on that path (also in 01 R14; IF97's silence is documented at `Web/fluid_properties/IF97.rst:81`). Unknown *fluid* names on the HEOS path throw (oracle); only the `-SRK`/`-PengRobinson` pseudo-names are silently ignored (X13) | `CoolProp.cpp:949, 969, 1026` (no `else`). Oracle: `PR::`, `SRK::`, `IF97::`, `INCOMP::`, `BICUBIC&HEOS::`, `SVDSBTL&HEOS::`, `HEOS?::` and `HelmholtzEOSBackend::` return without error and change nothing; `PR::R134a` + "BOGUS" is accepted. Fixed on master `80dd7beb` | Users believe models are aligned when they are not | `ReferenceState` is a field of the model spec; unsupported means a typed error at build time |
| X4 | **INCOMP gauge is per composition**, not per component. *Documented design limitation, not a bug*: "we regard mixtures with different compositions as independent fluids" (`Web/fluid_properties/Incompressibles.rst:49-50, 229-232`); it survives here only as a design constraint | `IncompressibleBackend.cpp:203-209`. Oracle: MEG h(293.15 K) = 0 and s = 0 for x = 0.1, 0.3, 0.5, while h(313.15 K) = 81 062 / 74 938 / 67 261 J/kg | Mixing or concentration-change energy balances are meaningless (07: no h_mix) | `Gauge` must be affine in composition; INCOMP solutions get a `PerCompositionGauge` compat mode that refuses composition-changing operations |
| X5 | INCOMP reference T outside the fit range for 9 fluids (verifier rerun: 9, same list). Low severity: the zero is arbitrary anyway and is still a deterministic constant; the docs warn only about T_ref = T_base (`Incompressibles.rst:50-55, 305-309`), not about the fit range | §4 | Zero defined by extrapolation; not reproducible from the literature | Per-fluid native anchor inside the range, or an explicit `Extrapolated` flag in compat mode |
| X6 | Stale cubic α⁰ snapshot carries **different reference offsets / Lead constants** | §3.1: 4 large constant shifts up to 536 kJ/kg (3 missing offset blocks, HCl a different Lead term), 2 small ones (Neon, Helium ≈ 100 J/kg), 15-16 T-dependent, Methanol α⁰ wholesale from EOS[1] | "PR::X vs HEOS::X" h differs by hundreds of kJ/kg even in the ideal gas | Cubics borrow the ideal-gas part (with its gauge) from the canonical substance entry (06 C5) |
| X7 | One gauge, six mechanisms (the doc previously said five but listed six): free S/D, per-instance S/D (a duplicate, including its own copy of the h/s recomputation at `HelmholtzEOSMixtureBackend.cpp:4540-4597`), library recomputation of 5-7 cached states, superancillary stamp, melting stamp, HA re-anchoring. The S/D triplication is already in 02 §6 | `CoolProp.cpp:946-1040`; `HelmholtzEOSMixtureBackend.cpp:4463-4597`; `FluidLibrary.cpp:64-127`; `FlashRoutines.cpp:542-552`; `MeltingCaloric.cpp:229`; `HumidAirProp.cpp:267-329` | Inconsistent semantics (X1, X2), DRY violation | One `Gauge` applied at one boundary; no cached h/s inside models |
| X8 | Per-instance `set_reference_stateD` on a mixture evaluates `this` (the mixture) rather than the component (duplicate of 02 §6; C++-only, not in `CoolPropLib.cpp` or Python) | `HelmholtzEOSMixtureBackend.cpp:4531-4536` builds `HEOS` for the component but calls `calc_hmolar_nocache` on `this` *(inference from code; not exposed in Python)* | Wrong component offsets for mixtures | — (does not exist in the design) |
| X9 | Per-instance S/D on a cubic iterates the HEOS-base `components`, which are α⁰-only placeholders (`CubicBackend.cpp:73-89`), and builds a HEOS from them | `HelmholtzEOSMixtureBackend.cpp:4464-4465` *(inference: the flash runs on an EOS with no residual and no states)* | Undefined results | — |
| X10 | Incomplete preconditions (low severity) | IIR checks only "Ttriple" (`CoolProp.cpp:971-973`; "Ttriple" is really sat-min T, 02 §3.5-3.6), not T_c > 273.15 K; oracle: `set_reference_state('Methane','IIR')` fails inside QT_flash ("may not be above the numerical critical point") rather than with a precondition message | Confusing errors | `ReferenceState::resolve(model) -> Result<Gauge, RefStateError::{BelowTriple, AboveCritical, …}>` |
| X11 | Gas-constant mismatches presented as "the same model" (overlaps 06 C6) | §3: PR vs HEOS h scales by 1.1e-5 for water; HA water-vapour h by 1.21e-5 | ~1e-5 relative disagreement; looks like a gauge problem but is not | R is explicit model data (06 C6); conformance tests separate shift from scale |
| X12 | **Mixture α⁰ applies component gauges at the wrong τ.** τᵢ = T_c,i·τ/T_r uses the JSON critical T, so a component offset contributes R·T_c,i·a2, not R·T_r,i·a2 (the gauge part of 02 §6's "mixture α⁰ uses critical instead of reducing constants") | `HelmholtzEOSMixtureBackend.cpp:3640, 3649, 3698`. Verifier oracle, Methanol (T_r 513.38, T_c 512.5) vs Methanol&Ethanol x = [1, 0], 600 K, 10 mol/m³: h_mix − h_pure = −19.18 J/mol by default, −33.47 after `set_reference_state('Methanol','ASHRAE')`. The extra −14.29 equals (T_c/T_r − 1) × the pure shift of 8338.64 J/mol; Δs is unchanged (−0.1538), since a1 is τ-free | A user gauge makes the mixture disagree with the pure fluid at x → 1, for ≥15 fluids | The gauge is applied outside the EOS (G1), per component, in molar units; α⁰ only ever sees its own EOS's τ |
| X13 | `set_reference_state('X-SRK' or 'X-PengRobinson', …)` is accepted and does nothing | `FluidLibrary.h:1236-1254` resolves the pseudo-name for the `HEOS` constructor; `FluidLibrary.cpp:66-67` then fails the lookup with no `else`. Verifier oracle: `set_reference_state('Water-SRK','NBP')` returns, and h(300 K, 1 atm) of `Water-SRK` stays −53 298.174 J/kg. Not fixed by `80dd7beb`, which adds an `else` only in `CoolProp.cpp`; master `FluidLibrary.cpp:90` (the outer `if`) still has no `else` *(inference from code)* | Silent no-op (like X3) | Same as X3: typed error |

## 7. Parallelism fit

- Applying a gauge is h += Δh and s += Δs (molar: Σxᵢ). It is branch-free and SIMD-trivial, with one load per batch because every point shares the model's gauge.
- Resolving a gauge (IIR/ASHRAE/NBP/Custom) costs one saturation or (T,ρ) evaluation per (model, reference state). Memoize it in a `OnceLock` on the gauged model handle. It is not on the hot path.
- Native-gauge tables and caches can be shared across every user gauge: build once, then share as `Arc` across threads and reference states. This removes CoolProp's need to rebuild or re-stamp, and removes the X1/X2 thread and lifetime hazards.

## 8. Verification assets

- **CoolProp tests**:
  - `src/Tests/CoolProp-Tests.cpp:2264-2340`: IIR/ASHRAE/NBP targets to 1e-8 absolute; "existing instance unchanged"; new instance updated.
  - `CoolProp-Tests-HS-prototypes.cpp:1121-1142, 1560-1565, 1717-1765`: superancillary stamp under reference changes.
  - `IncompressibleBackend.cpp:583+` (in-file tests, `set_reference_state` at `:761, 784, 806`).
- **Oracle probes** (this map). Scripts kept in the scratchpad, `p15/{base,ig,ha,ha2,ha3,inc,tab1,cub,cub2,misc,names,thr}.py`. Fixtures to capture:
  1. §3.1 table at 1e-6 relative;
  2. the ideal-gas R-ratio law per cubic fluid;
  3. HA invariance (bit-identical H/S/U before and after a user gauge change);
  4. INCOMP h(T_ref, x) = 0.
  - Known-bad values for the parity exclusion list: Twb after a user gauge change, stale tables, cubic s (C1).
- **Literature anchors**: IAPWS-95 and R7-97 (u = s = 0 for the triple-point liquid); R10-06 ice s₀; RP-1485 Tables (HA h, s); paper reference states for each HEOS fluid; ASHRAE/IIR definitions (Handbook Fundamentals).
- **Gauge-invariance property tests** (cheap and strong):
  - for random states, any two gauges must give identical p, ρ, cp, w and process Δh/Δs;
  - h, s, u, g and a must shift by exactly (Δh, Δs, Δh, Δh − TΔs, Δh − TΔs);
  - an h- or s-input flash under gauge G must return the same (T, ρ) as the native flash at h − Δh.

## 9. Port recommendation (units with priority P0-core / P1-early / P2-later / defer / drop)

**Proposed contract**
1. Every model declares a **native gauge**. This is metadata giving the anchor state, its provenance and the R it uses (for example `IAPWS95: u=s=0 at triple liquid`; `RP1485`; `IncompPerComposition{293.15 K, 1 atm}`; `ResidualOnly` for PC-SAFT).
2. The kernel computes **only in the native gauge**. Nothing gauge-dependent is cached inside a model.
3. `Gauge { dh_molar, ds_molar }` exists per component and composes linearly. It is applied **at the API boundary**: outputs add the gauge, h/s inputs subtract it, and g/a and their T-derivatives get the extra (Δh − TΔs) term.
4. `ReferenceState = Native | Iir | Ashrae | Nbp | Custom{T, ρ or p, h0, s0} | Aligned{to: ModelId, at: AnchorState}` (later also `Formation{298.15 K, 1 bar}`). It resolves to a `Gauge` once per `(Arc<Model>, ReferenceState)` and gives `GaugedModel { model: Arc<Model>, gauge }`. Different reference states share the same coefficient `Arc`. 01 U14 / 11 U2 are the API side of this.
5. **Should cross-model results be comparable by default? No.**
   - **Default = `Native`**, because oracle parity requires it and the native zeros are literature conventions users expect.
   - **Mixing models must be explicit.** Any API that combines values from two models (energy balance helpers, composite models, `Aligned`) checks the `GaugeId`s and refuses mismatches with a typed error. This is a runtime tag, not phantom types, to avoid over-engineering.
   - `Aligned` is the opt-in "connected" mode. It shifts a model so that it matches a substance's designated reference model at an anchor state:
     - ideal-gas anchor for gas-capable models;
     - liquid anchor for liquid-only models.
   - The residual model difference stays visible and documented (water: IF97 ≤ ~235 J/kg; INCOMP after alignment ≤ ~770 J/kg at 360 K).
6. **Oracle-parity mode must reproduce:**
   - HEOS native = Lead + JSON core offset; IIR/ASHRAE/NBP/DEF/Custom with the CoolProp algorithm (02 §3.5), including the published-offset residue (199 999.9885);
   - IF97 fixed;
   - INCOMP per-composition zero at 293.15 K/1 atm, including the extrapolated zeros (X5);
   - cubic α⁰ from the 8.0.0 snapshot (X6) with CODATA-2018 R;
   - HA RP-1485 constants;
   - ice R10-06 (2006 g00).
   - **It must not reproduce:** X1 (compute Twb in the native gauge; parity cases only with the default gauge), X2 (no stale tables), C1 (cubic s; exclude it from the oracle or use master 9b96b64b), or silent no-ops (X3).

| Unit | Source | Priority | Notes |
|---|---|---|---|
| G1 `Gauge` value + boundary application (outputs, h/s inputs, g/a terms, mixture Σxᵢ) | `Helmholtz.h:930-981` replaced | **P0-core** | ~150 LOC + property tests (§8). Enables everything else. Fixes X12 by construction; add a property test "mixture at x = [1, 0] equals the pure fluid under any gauge" |
| G2 Native-gauge declaration per model family (anchor, provenance, R) | §2.1 table | **P0-core** | Metadata only; required by the `Model` trait |
| G3 `ReferenceState` resolver for Helmholtz models (Iir/Ashrae/Nbp/Custom/Native) with typed preconditions | `CoolProp.cpp:946-1040`, `FluidLibrary.cpp:64-127` | P1-early | Replaces X7's five copies; no recomputed hs_anchor or critical/triple h/s caches |
| G4 Cross-model conformance matrix (Water: HEOS/IF97/INCOMP/PR/SRK/HA/ice; R134a: HEOS/PR/SRK; all cubic fluids at the ideal-gas limit) | §3.1 | P1-early | Separates shift vs scale vs model difference; guards X6/X11 |
| G5 Composite-model rule: sub-models injected in their native gauge (HA, wet bulb, ECS references, transport) | `HumidAirProp.cpp:1364-1370` | P1-early (with HA) | Fixes X1 by construction |
| G6 Surrogate tables: native gauge + model fingerprint key; gauge translation at the boundary | `TabularBackends.cpp:352-366`, `SVDSurfaceSerializer.cpp:566-604`, `FlashRoutines.cpp:757-775` | P2-later (with 08) | Fixes X2 |
| G7 INCOMP gauge: `PerCompositionGauge` compat; per-component gauge when a real solution model exists | `IncompressibleBackend.cpp:176-210` | P1-early (compat) / defer (proper) | X4/X5 |
| G8 `Aligned` reference state | new | P2-later | Opt-in connected mode |
| G9 `Formation` gauge (ATcT ΔfH°, third-law S°) for reacting systems and materials | master `9b35f538` (HFORMATION metadata) | defer | Reaches across states of matter (ice ↔ liquid ↔ vapour already share the IAPWS anchor) |
| — Global `set_reference_stateS/D`, the user offset term inside α⁰, per-instance copies, cached h/s in fluid states | X3, X7-X9 | **drop** | Replaced by G1-G3 |

## 10. Open questions

1. Should `Aligned` anchor gas-capable models at an ideal-gas state (exact apart from the R ratio) or at a liquid state (better for liquids)? Should the substance registry name one "reference model" per substance, for example IAPWS-95 for water?
2. Is an R-ratio scaling (X11) acceptable within "same model, same gauge", or should a cubic built on a HEOS ideal gas reuse that model's R for the ideal part (which breaks parity with 8.0.0 at ~1e-5)?
3. For INCOMP solutions, is there literature (Melinder, SecCool) giving per-component zeros, or are mixing enthalpies simply unavailable? If unavailable, G7 compat stays permanent.
4. Should the HA RP-1485 water-vapour R̄ = 8.314472 scaling (+31 J/kg at 300 K vs IAPWS-95) be kept in the default HA model or only in a compat mode? The 07 open questions on IAPWS G8-10 apply.
5. Should a gauge mismatch at a composite boundary be an error or an automatic native-gauge conversion? The proposal is an error for user-facing APIs and native-by-construction for internal composites.
6. Should a `Formation` gauge become the default once materials and reactions are in scope? It changes every h value users see, so it needs a user decision.

## Verification log

Date: 2026-10-04. Adversarial verifier pass against the v8.0.0 checkout (ae81610e) and the CoolProp==8.0.0 wheel (gitrevision ae81610e7d23…), with HOME and the table caches redirected to the scratchpad (`v15/{a..g}.py`).

**Claims checked: 84.** Every `path:line` in the scope line and §2-§6 and §8 was opened. The JSON census was rerun (136 files, 65 with an offset block, 71 without; files per label NBP 33 / IIR 27 / OTH 4 / CUSTOM 1; 71 blocks in total, with 6 files carrying two EOS entries). The cubic snapshot diff was rerun (24/116 differ). The INCOMP out-of-range count was rerun (9, same list). The ideal-gas sweep of all 116 cubic fluids was rerun. Every oracle number in §2.1, §3 and §3.1 was rerun except the HA dH/dW series at 280-400 K, which is consistent with the 1.21e-5 R ratio but was not rerun. X1 (both thread and init-time variants), X2 (in-process and fresh process), X3, X4 and X10 were reproduced.

**Confirmed as written:** the free and per-instance S/D code and the `CoolProp.cpp:949/969/1026` dispatch; `Helmholtz.h:170, 930-981, 1386, 1430-1431`; the `FluidLibrary.h:323-327` parsing; the `set()` increment/DEF semantics; `sync_linked_states` (`:161-170`); the cubic placeholder (`CubicBackend.cpp:73-89`) and `:3684`; the PC-SAFT throws and h_res = −389.916; the IF97 triple-point values and `IF97.rst:81`; the HA `thread_local` backends and offsets and the invariance of H/S/Hha/U; the ice constants (with g00 = 2006, per 07 C1); the cache keys of both table families; the #2773 stamp code; the test locations; and master commits 80dd7beb, 9b96b64b and 9b35f538.

**Corrections made:**
1. HEOS mixture row: α⁰ carries Rᵢ/R and τᵢ = T_c,i·τ/T_r, so the mixture gauge is not Σxᵢ·(pure gauge). Added **X12** (oracle: Methanol −14.29 J/mol extra after ASHRAE).
2. HA: water-vapour *entropy* uses 8.314371, not 8.314472 (`:271, 1088`); only h is scaled. Fixed in §2.1, §3 and §3.1. The HA water row is now classed as a "scale", not a "constant", mismatch.
3. §4: the code *does* cite "ASHRAE RP-1485 §3" (`:279`) and "Lemmon 2000" (`:301`). The "provenance is inference" claim was removed.
4. SVDSBTL: in-memory surfaces are per instance (`:983`), and staleness comes from the disk cache. Fixed in §2.1 and §5. `TabularDataLibrary` is confirmed as a process `static` (`TabularBackends.cpp:24`).
5. §2.2 / X3: unknown *fluid* names throw (oracle); only `X-SRK` / `X-PengRobinson` reach the silent no-`else` at `FluidLibrary.cpp:67`. Added **X13**, still on master.
6. §3.1 cubic sweep: Neon (+101.8) and Helium (+98.5 J/kg) are small constant shifts, not R ratio, and MM is T-dependent. HCl's shift comes from a different Lead term, not an offset block. Methanol takes its whole α⁰ from EOS[1] (Tc 512.5), which is a JSON fact rather than an inference, and the impact is only −550 → +698 J/kg. The bucket counts are threshold-dependent (rerun 97/4/15).
7. Dry air HA vs HEOS: the stated "± 1.5" range was replaced with the measured range (−399 287.2 to −399 289.5 J/kg).
8. Line fixes: INCOMP applied at `:513-518` (not 514-519); table limit check at `.h:838-840`; mixture α⁰ at `:3640, 3649, 3698`. GERG master: *ideal-gas* h = s = 0 at 298.15 K, 101 325 Pa (06 GERG bullet / open question 8; "06 Q8" does not exist as a label).
9. X7: "five implementations" listed six. Corrected to six, noting the duplicated h/s recomputation (`:4540-4597`) and the overlap with 02 §6.
10. X2: TTSE is also wrong (T = 246.79 K). In a fresh process, SVDSBTL `HmassP` returns NaN.

**Rot items, refutation results:**
- X1 **survives**: the docs require init-time changes (`HighLevelAPI.rst:390`), and even then Twb = 288.248 K, not 288.875 K.
- X2 **survives** for the disk cache: the fresh-process SVDSBTL result is stale under documented use. The in-memory part only arises from mid-run changes, which the docs advise against.
- X3 **survives**: it is fixed on master and overlaps 01 R14. IF97's silence is documented.
- X4 is **reclassified** as a documented design limitation (`Incompressibles.rst:49-50, 229-232`), not a bug.
- X5 is **downgraded** to low severity.
- X8 **duplicates** 02 §6 and is C++-only.
- X9 remains *inference* (C++-only API).
- X10 is low severity.
- X11 overlaps 06 C6.
- RESET being a no-op is real, but it belongs to 02 §3.5/§6 and is not repeated here.
