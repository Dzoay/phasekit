# 10 Test suite, verification assets and the oracle - CoolProp v8.0.0 map

> Scope: `src/Tests/` (Catch2, 31 files incl. `TestObjects.*`, 17.7k lines), the `#if ENABLE_CATCH` blocks in 21 library files (20 `.cpp` + `include/CoolProp/fluids/Helmholtz.h`; 4,656 lines, 51 active TEST_CASEs), `Web/fluid_properties/Validation/`, `Web/scripts/`, `dev/scripts/`, `dev/ci/`, `dev/reference/`, the superancillary check points in `dev/fluids/*.json`, `wrappers/Python/{pytest,CoolProp/tests}` and `wrappers/Python/CoolProp/Plots/ConsistencyPlots.py`; ~36k lines in total (verifier recount of `.cpp/.h/.py/.sh/.yml` in these paths). Part of the coolprop-rs port plan; cites the v8.0.0 source (ae81610e).
>
> Oracle runs (all "measured" values below): PyPI `CoolProp==8.0.0`, `__gitrevision__` ae81610e7d23efc57f9d051c8e70a4d66e87537f, wheel tag `cp312-abi3-manylinux_2_17_x86_64`, `CoolProp.abi3.so` 9,050,856 B sha256 `05d85591871524e8...`, Python 3.12.14, Linux x86_64, i7-8700K (6 cores / 12 threads), via `uv run --no-project --python 3.12 --with CoolProp==8.0.0`. Upstream fixes are cited as commits in `v8.0.0..origin/master` (151 commits to 2026-10-03). `dev/agent-notes.md` exists only on origin/master and is cited as `master:dev/agent-notes.md:<line>`.

## 1. Purpose and concepts

CoolProp verifies itself in three layers. Only the first can fail a build.

| Layer | Where | Runs in CI? | Asserts numbers? |
|---|---|---|---|
| Catch2 `CatchTestRunner`: 495 TEST_CASEs (464 visible, 31 hidden `[.]`), 58 `[slow]`, 8 `[!benchmark]` + 10 `[benchmark]`, 28 REFPROP-tagged, 86 linked to a GitHub issue | `src/Tests/*.cpp` (444 cases) + `#if ENABLE_CATCH` blocks in `src/**`, `include/**` (51 cases) | Yes: Release build, whole suite, no tag filter (`.github/workflows/test_catch2.yml:14,128-135`); ASan job (`.github/workflows/dev_checks.yml:40-107`). Hidden cases never run. | Yes |
| Docs-time sweeps: consistency plots, REFPROP / IF97 / superancillary deviation plots | `Web/scripts/*.py`, `wrappers/Python/CoolProp/Plots/ConsistencyPlots.py` | Docs build only; outputs PNG/RST | No, they only report |
| pytest for the Python binding | `wrappers/Python/pytest/*.py` (11 files), `wrappers/Python/CoolProp/tests/` | Wheel CI | Mostly API shape (`test_parity.py:17-18` says so); `test_superancillary.py:49-51` compares SA to PropsSI at 1e-6, which is circular because PropsSI uses the SA |

Provenance classes used below for every expected value:

- **P-paper / P-IAPWS**: computer-verification values printed in the EOS or correlation paper, or an IAPWS release (5-9 significant digits). This is the arbiter.
- **P-mp**: extended-precision values computed from the same EOS, i.e. the superancillary `check_points` from fastchebpure. They are exact for the EOS but say nothing about whether its coefficients are right.
- **R-other**: REFPROP 9.1/10 values, a different implementation of the same model. We cannot redistribute or regenerate them.
- **R-self**: values computed by CoolProp itself (regression pins). They detect change, not error.
- **C**: consistency checks: round-trip flashes, FD vs analytic derivatives, thermodynamic identities, equilibrium residuals. They need no external data.
- **S**: smoke checks ("does not throw", "is finite").

The oracle (CoolProp 8.0.0 as a black box) is an implementation, not the truth. Its outputs depend on 38 global config keys plus environment variables (section 4), and they are wrong in known places (section 8.4).

## 2. Structure (key types/functions -> path:line)

### 2.1 Runners and gates

| Item | path:line | Notes |
|---|---|---|
| `CatchTestRunner` target | `CMakeLists.txt:2293-2374` | 25 test TUs listed by hand plus every library source (glob `src/*.cpp` at `:340`, backends at `:356-360`) compiled with `ENABLE_CATCH`. `COOLPROP_NO_INCBIN` (`:2354`) loads the fluid data through a generated header instead of incbin. Its comment about "out-of-date zlib-compressed fluid information" is stale, since the data is CBOR now (`FluidLibrary.cpp:9-21`). `LAZY_LOAD_SUPERANCILLARIES` applies to Debug only by default, unless `COOLPROP_LAZY_LOAD_SUPERANCILLARIES` is set (`:2368-2373`); CI tests Release. |
| Stale exclusions | `CMakeLists.txt:363-365` | `REMOVE_ITEM .../src/Tests/Tests.cpp` is a no-op, because the glob at `:340` is not recursive. |
| CTest discovery | `CMakeLists.txt:2360-2366` | CI does not use it: the ctest steps are commented out (`test_catch2.yml:110-125`) and CI runs one `./CatchTestRunner` process, so global state carries across tests. |
| Legacy ASan runner whose `main` always returns success | `CMakeLists.txt:2508-2526`, `src/Tests/catch_always_return_success.cxx:10-14` | Used only by `dev/asan/run_asan.sh`. The modern ASan CI job uses the normal runner (`dev_checks.yml:78,107`). |
| Dead runner helpers that discard the session result | `src/Tests/Tests.cpp:30-47,49-68` (`return 1;` at `:43,:64`) | No callers (only the `Tests.h:8-10` declarations). |
| Local pre-push gate | `dev/ci/preflight.sh:225-245` | Default filter `"[!slow][!benchmark]"` (`:237`) means "tagged `!slow` AND `!benchmark`" in Catch2 v3 syntax; no test has those tags (verifier: 0 TEST_CASEs carry `[!slow]`). The Helmholtz arm `"[Helmholtz],[REFPROP],[!benchmark]"` (`:234`) and the SBTL arm (`:230`) OR benchmarks in. Pass/fail is decided by grepping the last 3 output lines (`:240`). This is a local helper only; the CI Catch2 job runs the whole suite and uses the exit code. See rot R1. |
| REFPROP gating | `src/CoolProp.cpp:719-725` (`Skip_if_No_REFPROP`, declared in the public header `include/CoolProp/CoolProp.h:196`) | REFPROP tests SKIP when REFPROP is absent; fork PRs never build it (`test_catch2.yml:62`, `master:dev/agent-notes.md:91-94`). |

### 2.2 Test inventory by area

| Area | Files (TEST_CASEs) | How it tests | Kind | Typical tolerance |
|---|---|---|---|---|
| Helmholtz term derivatives | `src/Helmholtz.cpp:1402-1819` (1) | 19 term instances x 14 derivatives at ONE point, tau=1.3, delta=0.9 (`:1771`). Multicomplex step where `one_mcx` exists (11 of 19 instances), otherwise central FD h=1e-5 (`:1772`; 8 instances: Lead, LogTau, IGPower, PlanckEinstein, CP0Constant, SAFT, SRK, PengRobinson). SAFT skips 5 fourth-order derivatives (`:1767-1770`). | C | 1e-9 derivative / 1e-14 value; 1e-7/1e-10/1e-12 for CP0PolyT, GERG2004, GaoB (`:1797-1807`) |
| Mixture composition derivatives | `src/Backends/Helmholtz/MixtureDerivatives.cpp:1105-1685` (3: HEOS, PR, SRK; VTPR commented out at `:1678-1683`) | FD on 16 single-variable perturbed copies plus 10 per-component perturbed vectors (`:1150-1154`); n-Pentane/Ethane/n-Propane/n-Butane = 0.1/0.12/0.18/0.6 at rho=300 mol/m^3, T=300 K (`:1157-1172`); `XN_INDEPENDENT` only | C | 5e-6 default, relaxed to 1e-4 (HEOS) and 1e-3 (cubics) (`:1667,1671,1675`) |
| Property, saturation and two-phase derivatives | `CoolProp-Tests.cpp:1744-1970, 2441-2703`; `AbstractState.cpp:1335` | FD vs `first/second_partial_deriv`, `*_saturation_deriv`, `*_two_phase_deriv(_splined)` | C | 1e-4..1e-7 |
| Pure-fluid flash | `CoolProp-Tests.cpp:1495-1743, 5181-6110`; `-HS.cpp` (3); `-HSU_D.cpp` (17, 3 hidden); `-PXFlash.cpp` (3); `-PXcdj.cpp` (2, sweep hidden); `-AirCritical.cpp` (1); `-HS-prototypes.cpp` (26, 25 hidden) | Round trip from PT or QT truth states; issue-reported states | C, S | HS 1e-5 rel T plus 1e-4 abs Q two-phase (`-HS.cpp:63`), 1e-5 rel T/rho single-phase (`:120`), 2e-4 for pseudo-pure (`:162`); HSU_D effectively 1e-3 (R4); many cases only NOTHROW / `ValidNumber` (R3) |
| Superancillary (SA) | `CoolProp-Tests.cpp:3431-4033` | mp check points, `source_eos_hash`, availability, timing | P-mp, C | `4*max(abs(ratio-1),1e-14)` per point (`:3834-3868`) |
| EOS paper check values | `CoolProp-Tests.cpp:4393-4502, 4781-4840` | p, cv, cp, w at (T, rho); critical constants | P-paper: 18 states for 13 fluids, plus R1336mzz(Z) constants | 1e-4 / 1e-5 / 1e-6 |
| Mixture alphar check values | `CoolProp-Tests.cpp:4505-4740, 2777-2856` | alphar at z1=0.4, tau≈delta≈0.8; Tkaczuk p and reducing values | Mixed: P-paper and R-self (section 8.1) | 1e-10 |
| Mixture VLE, flash and stability | `CoolProp-Tests-Michelsen.cpp` (53), `FlashRoutines.cpp:5048-5160` (5), `VLERoutines.cpp:3189-3262` (2) | Literature compositions (Michelsen 1982 / MM07, `-Michelsen.cpp:94-104`); phase label; equilibrium residual `max abs(ln(fV/fL))` (`:19-46`, used `:122,131`) | C, S | residual < 1e-6; trace components skipped (`:40`) |
| Transport | `CoolProp-Tests.cpp:30-600` (2 cases, 318 active rows = 177 eta + 141 lambda, 41 fluids); `:6604-6619` (surface tension) | eta/lambda at (T, rho or p or Q) vs tabulated values | P-paper, P-IAPWS, R-other, R-self (section 8.1) | 1e-5..1e-1 per row |
| Melting / triple / ancillaries | `Ancillaries.cpp:282-417`; `CoolProp-Tests.cpp:2096-2240, 4089-4232`; `-AirMelting.cpp`; `-NeonMelting.cpp` | IAPWS R14-08 inverse points, Herrig 2018 D2O, Air and Neon (both obtained by inverting CoolProp's own fitted curve; Air's fit reproduces REFPROP's Lemmon-2000 curve to <0.02%, `-AirMelting.cpp:19-20`) | P-IAPWS, P-paper, R-self | 1e-6..1 K |
| Reference states, fixed states | `CoolProp-Tests.cpp:2263-2439` | IIR/ASHRAE/NBP through the global `set_reference_stateS`; stored h/s of 7 named states vs EOS for 4 reference states | C | 1e-10..1e-8 J/kg; 1e-2 J/mol for fixed states (`:2418-2419`) |
| Humid air | `HumidAirProp.cpp:2452-2846` (10 active; an 11th at `:2812` sits in a block comment), `CoolProp-Tests.cpp:808-1465` | ASHRAE RP-1485 tables, physics bounds, round trips, thread safety | P-paper, C | 1e-3..1e-2 |
| PC-SAFT / cubic / VTPR / UNIFAC | `CoolProp-Tests.cpp:2749-3160, 3377-3430, 4233-4398`; `-CubicU/Alpha.cpp`; `UNIFACLibrary.cpp:141` | 16-digit densities and pressures with no stated source | R-self | 1e-5 / 1e-3 |
| Tabular / SBTL / SVDSBTL | `-SVDSBTL*.cpp`, `-SVDComponents.cpp`, `-SBTLAdapter.cpp` (127), `TabularBackends.cpp:1736-2035` (3) | Table vs HEOS/IF97/REFPROP | C | various; 57 of the 58 `[slow]` cases (47 in `-SVDSBTL.cpp`, 8 in `-SBTLAdapter.cpp`, 2 in `-SVDSBTLCriticalPatch.cpp`) |
| API, strings, config, JSON, schema, CBOR, FPU | `CoolProp.cpp:727-1160` (5), `-FactoryOptions/PropsSIOptions/JSONHelpers/SchemaValidation/CBOR/FPUGuard.cpp` (48), `DataStructures.cpp:901-935` (3) | Parsing, error paths | S | - |

### 2.3 Harness classes

| Class | path:line | Role and defects |
|---|---|---|
| `TransportValidation::vel`, `TransportValidationFixture` | `CoolProp-Tests.cpp:33-42, 303-326` | Row = (fluid, in1, v1, in2, v2, out, expected, tol). The constructor never stores `out` (`:38-40`); the loops hard-code eta and lambda (`:340,594`). Hence the IAPWS D2O *conductivity* rows are labelled `"V"` (`:570-572`). `set_pair` shadows the member `pair` (`:320`). |
| `ConsistencyFixture` | `CoolProp-Tests.cpp:625-766` | PT truth, then re-flash. Only `DmolarT_INPUTS` is active (`:603-622`). T within 1e-2 K and p within 1e-4 rel are enforced by throwing (`:726-729`); optional REFPROP rho check at 1e-3. |
| `AncillaryFixture` (disabled), `SatTFixture`, `FixedStateFixture` | `:2010-2093, 2195-2240, 2353-2439` | Loops over all fluids. The disabled one's `check_pV` reads the *liquid* p (`:2057`). `FixedStateFixture` checks h/s to an absolute 1e-2 J/mol (`:2418-2419`). |
| `SuperAncillaryOn/OffFixture`, `PropertyLimitsFixture`, `NormalizeGuard`, `SuperancGuard`, `DebugLevelGuard`, `WaterRefStateGuard` | `:3431-3475, 2791-2800, 5149-5175, 6041-6123`, `-HSU_D.cpp:59-72` | RAII save/restore of process-global config |
| `HelmholtzConsistencyFixture` | `src/Helmholtz.cpp:1408-1748` | Hand-built term instances, not loaded from fluid JSON |
| `DerivativeFixture<backend>` | `MixtureDerivatives.cpp:1147-1665` | 17 perturbed backend copies |
| `HAPropsConsistencyFixture` | `HumidAirProp.cpp:2548-2627` | RP-1485 tables A.11, A.12, A.15 at 1% |
| `equilibrium_residual()` | `CoolProp-Tests-Michelsen.cpp:19-46` | Re-solves each phase at (p, T) with imposed phase and compares fugacities |
| `ConsistencyFigure` / `ConsistencyAxis` (Python) | `ConsistencyPlots.py:79-133, 246-620` | 40x40 (log p, T) single-phase grid + 20x20 (T, Q) two-phase grid per input pair. GOOD if drho and dp < 1e-3 rel and dT < 1e-3 K (`:443-447, 543`). 4 of 15 pairs are "not implemented" (`:17`). |

### 2.4 Scripts, Python tests and docs assets

| Script | path | Status |
|---|---|---|
| Consistency-plot driver (threads, each driving an isolated subprocess per fluid) | `Web/scripts/fluid_properties.Consistency.py` (201) | Live (docs) |
| REFPROP comparison along T = min(1.01 Tc, Tmax), rho in (0, 2 rhoc) | `Web/scripts/fluid_properties.REFPROPcomparison.py:51-58` | Needs REFPROP |
| Superancillary deviation vs the pinned fastchebpure release `2026.06.02-v2` (downloads a GitHub archive) | `Web/scripts/fluid_properties.Superancillary.py:15-27` | Live |
| IF97 conformance per IAPWS G13-15, for `SVDSBTL&IF97` vs `IF97` | `Web/scripts/fluid_properties.IF97Conformance.py:7-20,59-65` | Live |
| Humid-air tables, printed only | `Web/fluid_properties/Validation/HAValidation.py`, `NelsonValidation.py` | Nothing asserted |
| SA check-point injector and FNV-1a EOS hash | `dev/scripts/inject_superanc_check_points.py:78,111-222` | Live |
| SA freshness and release-pin gates | `dev/scripts/check_superanc_freshness.py`, `check_superanc_release_pin.py` | Live |
| Golden C++ plot tests printed from Python CoolProp | `dev/scripts/generate_Plot_test_data.py:20-30` | R-self |
| `derivative_tester.py`, `check_hs.py` | `dev/scripts/` | Dead: they import `Props`/`DerivTerms` (`derivative_tester.py:2`, `check_hs.py:3`), which the v8 module lacks |
| Schema gate | `dev/validate_fluid_schemas.py:28-39` | Covers PC-SAFT, cubic and departure files only. The 136 pure-fluid JSONs and `mixture_binary_pairs.json` have no schema, and a missing file prints SKIP and passes (`:37-39`). |
| `dev/reference/HX.py` | | Heat-exchanger paper script (Bell 2014), not reference data |

## 3. Algorithms and formulas (cite the papers CoolProp cites)

- **Finite differences.** Central first differences, h = 1e-5. A higher derivative is the FD of the next-lower analytic derivative (`Helmholtz.cpp` `HelmholtzConsistencyFixture::call`). Mixture steps: dtau = ddelta = dz = dn = 1e-6, dT = drho = 1e-3, dp = 1 Pa (`MixtureDerivatives.cpp:1157`).
- **Multicomplex-step differentiation.** Uses `usnistgov/multicomplex` @39bf9ca5 (`cmake/dependencies.cmake:83-88`) through `mcx::diff_mcxN` (`Helmholtz.cpp:1732-1740`). It is exact to rounding at any order. Only GeneralizedExponential, NonAnalytic, GaoB, XiangDeiters, CP0PolyT, GERG2004Sinh and GERG2004Cosh implement `one_mcx` (`Helmholtz.cpp:295,560,720,778,1067,1292,1344`). Because the fixture builds 5 GeneralizedExponential instances (Gaussian, Lemmon2005, Power, Exponential, GERG2008), 11 of the 19 test instances use mcx and 8 fall back to FD (the base virtual throws `NotImplementedError`, `Helmholtz.h:302-306`). The virtual exists only under `ENABLE_CATCH` (`include/CoolProp/fluids/Helmholtz.h:302-306`, plus 7 more blocks at `:551,595,646,668,1169,1207,1243`).
- **Round-trip consistency.** Build a truth state with PT (single phase) or QT (two phase), read (x1, x2) for another input pair, re-flash, then compare T, rho, p (ConsistencyPlots), T/rho/Q (HS) or rho and the caloric input (HSU_D).
- **Equilibrium residual.** `max_i abs(ln(f_i^V/f_i^L))` < 1e-6 (`CoolProp-Tests-Michelsen.cpp:19-46`). A correctness check that needs no reference values. Michelsen FPE 9 (1982) 1-19 and 21-40; Michelsen & Mollerup 2007 (`:94-104`).
- **Superancillary acceptance.** The Chebyshev evaluation must reproduce the mp value within 4x fastchebpure's own reported error (`CoolProp-Tests.cpp:3834-3868`). Check points sit at Theta = (Tc_num - T)/Tc_num = 0.5, 0.3, 0.1 (`inject_superanc_check_points.py:78`), so nothing within 10% of Tc is checked.
- **EOS freshness hash.** FNV-1a 64 (offset `0xcbf29ce484222325`, prime `0x100000001b3`, `inject_superanc_check_points.py:168,222`) over a typed byte stream of the parsed JSON tree of `EOS[0]` minus `SUPERANCILLARY`: type tags, little-endian u64 lengths, sorted keys, IEEE bits for doubles. Self-test vector `8e75626511d00b5c` (`CoolProp-Tests.cpp:3788`). Text is deliberately not hashed because "nlohmann's and Python's shortest-round-trip float formatters occasionally disagree" and CoolProp's runtime JSON parse "historically rounded some doubles 1 ULP away" (`:3608-3621`).
- **Papers and releases cited by the assets:**
  - EOS: Lemmon & Akasaka IJT 43:119 (2022) Table 7 (R1234yf, `CoolProp-Tests.cpp:4393-4396`); McLinden & Akasaka JCED 65:4201 (2020); and the batch at `:4798-4815`: Akasaka & Lemmon IJT 2023/2024/2025 and JPCRD 2022, Fiedler et al. IJT 2023, Eisenbach et al. JPCRD 2021, Thol, Fenkl & Lemmon IJT 2022, Akasaka et al. IJR 2020, Gao et al. IECR 2022, Huber, Kazakov & Lemmon IJT 2025.
  - Mixtures: Bell JPCRD 51:013103 (2022) Table XI and 52:013101 (2023) Table XIII; McLinden et al. NIST IR 8570 (2025); Tkaczuk et al. JPCRD 49:023101 (2020) Table 8.
  - Melting: Herrig et al. JPCRD 47:043102 (2018) (D2O melting); IAPWS R14-08 (water melting).
  - Transport: Huber et al. JPCRD 2009/2012 (IAPWS water eta/lambda), IAPWS D2O; about 40 transport papers named in comments at `CoolProp-Tests.cpp:44-301, 347-581`; Mulero JPCRD 2012 (surface tension).
  - Other: ASHRAE RP-1485 (`HumidAirProp.cpp:2452-2627`); IAPWS G13-15 (IF97 conformance); Bell & Deiters IECR 60:9983 (2021) (cubic SA, `CoolPropBibTeXLibrary.bib:540-549`).

## 4. Data and configuration inputs

**The oracle's data is the repo data.** For all 136 fluids, `get_fluid_param_string(f, "JSON")` from the wheel (a one-element JSON array) equals the corresponding `dev/fluids/*.json` document as a whole (measured, re-confirmed by verifier; `R1224YDZ` lives in `R1224yd(Z).json`). The Rust port should ingest `dev/fluids/*.json` at v8.0.0 so that oracle and port share identical coefficients.

**Outputs depend on 38 global config keys.** Defaults come from `get_config_as_json_string()`, and every key can be overridden by an environment variable `COOLPROP_<KEY>` read at first use (`include/CoolProp/Configuration.h:174-176,253-256`).

| Key (default) | Effect on oracle outputs (measured where stated) |
|---|---|
| `ENABLE_SUPERANCILLARIES` (true) | `T/p/rhomolar_critical` return the EOS numerical critical point from the SA, not the JSON constants. Measured over 136 fluids: 34 deviate > 1e-3 rel and 111 > 1e-6. Worst: R40 (Tc +0.56%, pc +3.9%, rhoc -5.9%), MDM (+0.23%, +2.0%, +4.5%), Helium (rhoc -4.1%), MM (rhoc +4.0%). With the flag off the values equal JSON exactly. Pure-fluid QT becomes a Chebyshev evaluation (0.5 us vs 17-69 us). ECS transport reads the state's Tc and rhoc (`TransportRoutines.cpp:1205-1206,1296-1297`), so it shifts by up to 1.5e-6 (section 8.4). |
| `NORMALIZE_GAS_CONSTANTS` (true) | Mixture R becomes CODATA instead of the mole-fraction average (`HelmholtzEOSMixtureBackend.cpp:603-612`, whose comment says "mass fraction"). Measured: Tkaczuk Table 8 pressures are off by -2.68e-6 (Ar pairs) and +3.4e-7 (He/Ne) with true, and by at most 2.2e-14 with false. |
| `CRITICAL_WITHIN_1UK` (true), `CRITICAL_SPLINES_ENABLED` (true) | Within 1 uK of Tc both phases are set to `rhomolar_critical()`. Without an SA (or with the SA off), T > `splines.T_min` uses critical-region spline densities, with p = mean of the two phase pressures (`FlashRoutines.cpp:927-942`). 63 fluids carry splines; the widest window is R134a at 0.70% of T_red (measured). |
| `HSU_D_TWOPHASE_EOS_POLISH` (true), `ENABLE_MELTING_CALORIC_HS` (true), `MIXTURE_STABILITY_ALGORITHM` (1), `ASSUME_CRITICAL_POINT_STABLE` (false), `SPINODAL_MINIMUM_DELTA` (0.5), `DONT_CHECK_PROPERTY_LIMITS` (false), `USE_GUESSES_IN_PROPSSI` (false), `R_U_CODATA` (8.31446261815324) | Select or perturb algorithm branches |
| Hidden env switches read once (at the one-time library load, or into function statics): `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY` (load-time, `FluidLibrary.cpp:47`, `FluidLibrary.h:393`), `COOLPROP_DISABLE_SUPERANC_HSU_D` (`FlashRoutines.cpp:1853`), `PXFLASH_DIRECT_EOS`, `PXFLASH_INNER_NEWTON` (`:2837,2841`), `COOLPROP_DISABLE_MELTING_CALORIC_HS` (`:4743`), `COOLPROP_DISABLE_SUPERANC_HS` (`:4825,4901`) | Not in the config system; cannot be changed after first use. Fixture generators must scrub them. |
| Reference state (global per fluid, `set_reference_stateS`) | Mutates the library entry. Existing instances keep the old offsets and new instances get the new ones (asserted at `CoolProp-Tests.cpp:2329-2344`). |

Other inputs:
- **Per-EOS gas constant** (measured across the 136 JSONs): 8.314472 (63 fluids), 8.3144621 (17), 8.31451 (15), 8.314462618 (15), 8.3144598 (14), 8.314471 (6), 8.3143 (2), 8.31448 (2), 8.31434 (1), 8.314371357587 (Water). Fixtures and the port must use each EOS's own R.
- **Block types in use** (`EOS[0]`, measured): alphar = Power (134 fluids), Gaussian (78), Exponential (10), NonAnalytic (2), GaoB, DoubleExponential, Lemmon2005 (1 each). alpha0 = Lead/LogTau (136), PlanckEinstein (107), EnthalpyEntropyOffset (55), CP0PolyT (21), Power (17), PlanckEinsteinFunctionT (7), CP0Constant (4), PlanckEinsteinGeneralized (2), CP0AlyLee (2). The loader converts AlyLee and PlanckEinsteinFunctionT into CP0PolyT/PlanckEinsteinGeneralized (`FluidLibrary.h:234-322`). DoubleExponential and EnthalpyEntropyOffset have their own code paths (`:122-137, 323-327`).
- **Stored derived states.** `STATES.critical` and `EOS[0].STATES.{reducing,hs_anchor,sat_min_*}` hold h, s and p. They are read at load and refreshed only by `set_fluid_enthalpy_entropy_offset` (`FluidLibrary.cpp:86-101`), which evaluates "reducing" and "critical" h/s at 1.00001 x (T, rho) for Water and CO2 (`:91`).

## 5. State, caching, globals, thread-safety, memory

- **Fluid library.** A process-global `static JSONFluidLibrary library` is loaded under `std::call_once` (`FluidLibrary.cpp:27-43`) by decoding the whole CBOR blob. `add_many` errors are printed to stdout and swallowed (`:57-61`; fixed upstream 51e8c60f). SAs are built eagerly unless `LAZY_LOAD_SUPERANCILLARIES` (`include/CoolProp/CoolPropFluid.h:433-447`). The lazy getter is unsynchronized; inference: it is a data race under concurrent first use, which only Debug test builds enable.
- **Measured costs.** `import CoolProp` takes 2.0 s. An `AbstractState("HEOS","Water")` costs 49 us. The extension is 9.05 MB. SA blocks are 8.6 MB of the 9.6 MB compact fluid JSON (89%; 130 fluids x ~66 kB).
- **Global mutable state the tests touch:**
  - config keys (RAII guards, section 2.3);
  - per-fluid reference state (no guard at `CoolProp-Tests.cpp:2295-2350`, so an exception leaks the state into later tests);
  - the process-global `errstring`/`warnstring`. `CoolProp.cpp:1105-1135` *asserts* that thread A's error is visible to thread B, which means concurrent callers overwrite each other's errors;
  - binary-pair tables (`set_mixture_binary_pair_pcsaft` at `CoolProp-Tests.cpp:3068-3073` throws when another test set the pair first; order dependence fixed upstream 42a848f0);
  - the global caloric-SA cache with a build counter exposed through `get_fluid_parameter_double(0,"SUPERANC::caloric_build_count")` (`CoolProp-Tests.cpp:6186-6247`).

  Isolation rests on guards restoring state; running tests in parallel threads in one process would race.
- **Thread contract.** One `AbstractState` per thread (`master:dev/agent-notes.md:147-151`). REFPROP is never thread-safe (`:95-96`). After v8.0.0 upstream still had to: put config/DataStructures init under `call_once` (c17783e6: "settings made through a losing Configuration were silently dropped"), make TabularDataLibrary thread-safe (66ecd326), and serialize REFPROP (8214f28e). Treat v8.0.0 as safe only for per-thread instances after warm-up.
- **Oracle determinism (measured).** Results are bit-identical regardless of call order or instance reuse: 2,000 PT+PH points each for Water, n-Propane and CO2, compared forward vs reverse vs fresh instances. The HEOS oracle has no warm-start path dependence for PT and PH. `add_fluids_as_JSON` mutates the global library, so use it only in generator processes.
- **Fixture memory (measured).** A 10k-row x 23-column (T, rho) fixture is 4.37 MB as CSV (437 B/row) and 2.11 MB gzipped. Python `repr` round-trips exactly for every finite value; 431 of the 10k rows (4.3%) carry `nan` speed of sound (unstable states), so fixtures need an explicit NaN token.

## 6. Rot and bugs (table: issue | evidence | impact | Rust remedy)

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| R1 | Fail-open gates | `dev/ci/preflight.sh:230-240`: the tag DSL selects nothing or ORs benchmarks in, and the verdict comes from a grep. Upstream: 85fc178d ("make preflight actually run these tests"), d8d95547 ("touching a Helmholtz file shrank the sweep from ~560 cases to 69"), ada47fbe ("clang-format and build gates could never fail" under `pipefail`). Runners discard results: `Tests.cpp:43,64` (dead code, no callers), `catch_always_return_success.cxx:11-13` (legacy ASan target only). Scope (verifier): the CI Catch2 job (`test_catch2.yml:128-135`) runs the whole suite and honours the exit code, so the fail-open affects the local pre-push gate and the legacy ASan runner, not CI. | A green local gate with zero tests run | `cargo test`/nextest exit codes; no tag DSL; required CI checks; a CI step asserting the executed-test count |
| R2 | The dense sweeps are hidden, so never run | `[.]` on `-HSU_D.cpp:514` (PT sweep over all SA fluids), `-PXcdj.cpp:234` (broad sweep), 25 of 26 in `-HS-prototypes.cpp`, `-TermCacheProfile.cpp:235,256` | Broad-coverage regressions surface only when someone runs them by hand | Nightly `--ignored` job with published report; sweeps sized to a time budget |
| R3 | Assertions weaker than their names | "5 times PH ... yields same results every time" asserts only NOTHROW (`CoolProp-Tests.cpp:1694-1721`), and a near-duplicate follows (`:1723-1742`). P,Y flash round trips check `ValidNumber(T2)`, never T2 == T (`:1557-1685`). `[saturation]` asserts nothing (`:796-806`). "Test all input pairs ... all valid backends" runs DmolarT on HEOS only (`:603-622,768-794`). Of the 10 "Github issue #..." cases at `:3160-3328`, 6 assert only no-throw (#2622 asserts nothing at all, `:3229-3244`) and 2 only finiteness. "Surface tension" checks `ValidNumber` (`Ancillaries.cpp:416-420`). | Value regressions pass | Every round trip asserts the recovered values against a tolerance class; mutation testing (cargo-mutants) on kernels |
| R4 | Tolerances looser than written | HSU_D `Approx(x).epsilon(1e-5).margin(1e-3*abs(x)+1e-6)`: the "margin" is a 1e-3 *relative* bound (`-HSU_D.cpp:130`; the comment at `:128-129` says it guards zero crossings of the caloric value, but it also dominates the 1e-5 epsilon everywhere else). REFPROP sections titled "0.5%" check 5% (`REFPROPMixtureBackend.cpp:2833,2860,2863,2891`). Water ice VI uses 1 K vs 0.01 K for the other ices (`Ancillaries.cpp:311`) and hides a 0.965 K error (R10). "matches IAPWS" surface tension tolerates 1% (`CoolProp-Tests.cpp:6613-6614`) while the model is Mulero 2012. | False confidence | Tolerances come from provenance (printed digits) or a central class table; no ad-hoc margins |
| R5 | Tolerances tuned to CoolProp's output, not the paper (inference from the near-bound pattern; explicit only for Heptane) | Measured (re-run by verifier), all 318 rows pass, but 7 sit within 10% of their bound: DME eta 2.99e-3 vs 3e-3 (`CoolProp-Tests.cpp:82`), Heptane lambda 2.94e-3 vs 3e-3 (`:360`, "Relaxed tolerance because conductivity was fit using older viscosity correlation"), R123 (`:442`), Helium (`:517`), m-Xylene (`:544`), EthylBenzene 4.6e-2 vs 5e-2 (`:546`), n-Pentane (`:579`). Helium near 5.2 K is 7-8% off at a 1e-1 bound (`:521-522`). 32 of 318 rows deviate by > 0.1%. | Real model or implementation differences frozen in as "pass" | Override registry with evidence (section 8.5); never widen a tolerance to fit |
| R6 | Self-referential "reference" values | Rows "From REFPROP 9.1" and "From CoolProp v5 implementation" (`CoolProp-Tests.cpp:64-78, 221-227, 426-428, 503-505, 536-546`). Hydrogen lambda replaced by CoolProp's own number (`:397`). Bell-2022 R1234yf+R1234ze(E) pinned to CoolProp's value with a rationale its sibling pair contradicts (section 8.4) (`:4530-4539`). NIST-IR-8570 alphar values computed by CoolProp (`:4657-4717`). Tkaczuk reducing values "computed ... with CoolProp's pure reducing constants" (`:2834-2856`). `p == Approx(291215)` (`:1687-1692`). PC-SAFT 16-digit values with no source (`:2749-3160`). Plot goldens printed from Python CoolProp (`generate_Plot_test_data.py:20-30`). | Tests detect change, not error | A provenance field on every expected value; regression pins stored apart from verification data |
| R7 | Test-only code in production classes and headers | Virtual `one_mcx` only under `ENABLE_CATCH` in an installed header (`Helmholtz.h:26-28, 302-306` + 7 blocks); 4,656 test lines in 21 library sources; `Skip_if_No_REFPROP` in the public `CoolProp.h:196` | Tested binary differs from shipped (vtable layout, data path via `COOLPROP_NO_INCBIN`, Debug-only lazy SA) | `#[cfg(test)]` and `tests/`; terms generic over a scalar type so autodiff exercises production code; one loader |
| R8 | Thin term-level coverage; silent loader gaps | One (tau, delta) point; FD fallback for 8 of 19 term instances; SAFT 4th derivatives skipped (`Helmholtz.cpp:1767-1770`). The `DoubleExponential` (Methanol) and `EnthalpyEntropyOffset` (55 fluids) paths and the AlyLee/PlanckEinsteinFunctionT conversions have no term test (the fixture only calls `add_Gaussian/Lemmon2005/Power/Exponential/GERG2008Gaussian`); Methanol's only EOS cross-check is REFPROP-gated (`CoolProp-Tests.cpp:3361-3375`), so it skips in fork CI. Unknown ideal-gas block types are printed and skipped (`FluidLibrary.h:329-330`, throw commented out); unknown residual types throw (`:160`). (added by verifier) Coefficient-vector length checks in `parse_alphar` are bare `assert`s (`FluidLibrary.h:54-83`, `:131-136`), compiled out in the Release and `-DNDEBUG` ASan builds that CI tests; upstream ae54172f: "parse_alphar only had asserts, compiled out in Release". | Edge cases (delta to 0, the non-analytic term near tau=delta=1) unguarded; a typo drops a term silently | Generic term code + dual/hyper-dual autodiff on hundreds of points per block + oracle block isolation (8.2); unknown block type = parse error |
| R9 | Paper coverage gap | 13 fluids have state check values (`CoolProp-Tests.cpp:4393-4502, 4787-4840`); no IAPWS-95 table test for Water (grep finds none); SA mp points are circular; `source_eos_hash` detects change, not correctness | A coefficient transcription error in ~120 fluids goes undetected | Paper-check corpus built fluid by fluid as each EOS is ported (P0) |
| R10 | Data bug: Water melting curve, ice VI | `dev/fluids/Water.json:98` `p_0: 623400000.0`; IAPWS R14-08 uses p* = 632.4 MPa. Measured: T(1356.76 MPa) = 320.965 K vs the IAPWS check value 320 K, and p(320 K) = 1337.45 MPa; with 632.4 MPa, p(320 K) = 1356.757 MPa. Still present on origin/master. | Ice-VI melting T off by ~1 K | Paper wins; override entry; Rust data uses 632.4 MPa |
| R11 | Stale derived data in fluid JSON, masked by the test that should catch it | Measured: 57 fluids store `STATES.critical` and `EOS.STATES.reducing` at identical (T, rho) but different h/s. In 56 of them the *reducing* h disagrees with the EOS (1-Butene 26144.47 stored vs 26139.13 EOS; CO2 14625.96 vs 14622.61), loaded verbatim at `FluidLibrary.h:373-374`. These values date from 2014 (`git log -S` -> 655e8f14) and are still on master. R1234yf reducing h = s = 0 (`dev/fluids/R1234yf.json:191-196`). `FixedStateFixture` should catch this but cannot: its cleanup `set_reference_stateS(fluid,"DEF")` (`CoolProp-Tests.cpp:2421`) runs after the first state in the list, `hs_anchor` (`:2429`), and recomputes reducing/critical h/s from the EOS (`FluidLibrary.cpp:86-101`) before they are checked (inference from code reading). `update_states()` has no callers (`HelmholtzEOSMixtureBackend.cpp:545-563`). Upstream 2acbbc82: "stale reducing enthalpies break a five-component mixture flash". | Wrong values reachable via `get_state` and mixture guesses in a fresh process; test results depend on in-process history | Never store derived values; compute at load; tests run in fresh, immutable models |
| R12 | Critical-point accessors change meaning with a global flag | Section 4 measurements. A test was commented out after "Whoah, actually quite a few change meaningfully" (`CoolProp-Tests.cpp:3330-3341`). The R1336mzz(Z) fixed-point test loosens pc/rhoc to 1e-3 so that the SA numerical critical point also passes (comment at `:4486-4496`); verifier: both fixed-point tests pass with SA on and off (with SA off the accessors return the JSON constants, which equal the papers' values). Upstream f854d34b: "Reducing parameters belong to the correlation". | Correlations, grids and tests keyed on Tc depend on config | Separate typed `PublishedCritical` and `NumericalCritical`; correlations own their reducing constants |
| R13 | `viscosity_contributions()` misreports | ECS, Chung, rho*sr and hard-coded models put the whole value into `critical` (`HelmholtzEOSMixtureBackend.cpp:853-890`) | Contribution fixtures meaningless for those models | Per-model typed breakdown; `Option` for components that do not apply |
| R14 | Disabled instead of fixed | `AncillaryFixture` disabled with a muddled reason that does not cover the fluids without an SA (`CoolProp-Tests.cpp:2089-2093`). Commented transport rows: ParaHydrogen block (`:400-406`; measured 1.5% off at `:404`), Toluene, Benzene, R1234yf/ze, Geller blends. HS solver test (`:4742-4779`). VTPR tests (`VTPRBackend.cpp:171`, `MixtureDerivatives.cpp:1678-1683`). An RP-1485 value dropped as "seems incorrect" with no erratum (`HumidAirProp.cpp:2536`). | Known failures lose visibility | `#[ignore = "reason + issue"]` plus a CI report of ignored tests; overrides need evidence |
| R15 | Dangling or dead assets | `dev/fixtures/pxcdj_master_baseline.csv.gz` is referenced (`-PXcdj.cpp:15,225`) but exists only on `origin/ihb/pxcdj-testinfra-precond`. `TestObjects.cpp` is compiled by no target, yet its header is included (`IncompressibleFluid.cpp:503`, `IncompressibleBackend.cpp:581`); `IncompressibleFluid.cpp:505` defines its own global `makeMatrix` duplicating `CoolPropTesting::makeMatrix` (`TestObjects.cpp:15`). `derivative_tester.py`/`check_hs.py` use the v4 API. `NelsonValidation.py:21-25` converts Tdb with `+ 273.13` (Twb uses 273.15). | Misleading inventory; the Nelson table is computed 0.02 K off | Fixture manifest with hashes checked in CI; delete dead code |
| R16 | Bit-exact or over-tight asserts across different code paths | Upstream 5b9c32ac: `all_deltaonly` vs `all` differ by 1-3 ulp from FMA contraction; `VLERoutines.cpp:3211` demands 1e-10 from a solver that stops at 1e-7 (and the REQUIRE aborted before the second half ran). Upstream a849e8e1 drops toolchain-dependent bit-exact transport checks. | Flaky "real" failures; untested code after an aborting REQUIRE | Bitwise only for literally the same code path; otherwise ulp/relative classes derived from solver tolerances |
| R17 | Unvalidated inputs | NaN quality passed `(Q<0)\|\|(Q>1)` guards into the flash (upstream 0f978943). Locale-dependent number parsing (db0c599f, b945eb4f). | Crashes and environment-dependent results | `Quality` newtype (finite, in [0,1]); locale-free parsing (Rust `str::parse` already is) |
| R18 | Transport and caloric outputs returned for physically invalid states | Measured: Water QT(0.5, 373.15 K) eta = 1.2129e-5 Pa s, lambda = 0.02499 W/m/K (upstream da48a3ee makes this throw). `speed_sound` returns NaN on unstable (T, rho) states (4.3% of a Water grid); cv < 0 on 9% of a uniform n-Propane (T, rho) grid. | Fixtures could bake in nonsense | Types forbid transport on two-phase states; unstable-state evaluation is explicit and flagged |

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

| Activity | Shape | Fit and evidence |
|---|---|---|
| Fixture generation (oracle) | Embarrassingly parallel over fluids and points; order-independent (measured, section 5) | Process pool with fork (children inherit the loaded library, so no 2 s import per worker). Measured, Water (T, rho) + 6 properties: 1 proc 252k states/s; 6 procs 1.48M/s (5.9x); 12 procs 1.67M/s (6.6x, hyper-threading adds 13%); mixed fluids on 12 procs 2.95M/s. |
| Term and EOS verification in Rust | Pure functions of (tau, delta) or (T, rho) over SoA columns | Data-parallel and SIMD-friendly. Fixture columns double as batch/SIMD benchmark inputs. |
| Flash round-trip sweeps | Branchy per point (phase determination, root finding) but independent | Parallel across points (rayon); no SIMD inside a solve |
| Saturation | SA = interval lookup + Clenshaw (branch-light); VLE = Newton (branchy) | Batch SA evaluation is SIMD-friendly; VLE is not |
| Transport | Dilute/background terms are polynomial/exp; ECS has an inner conformal solve that fails on ~7% of states (8.4) | Direct correlations SIMD-friendly; ECS branchy |
| Test execution | `cargo test` runs tests on parallel threads by default | Forces a library with no global mutable state. The test runner enforces the user's concurrency goal; CoolProp could only survive this with RAII guards and still had order dependence (42a848f0). |
| Side-by-side kernels (scalar / batch / SIMD / threads) | Same math, different evaluation order | Differential tests: the scalar f64 path is the reference; every accelerated variant is compared against it on property-based random inputs with a ulp budget, never bitwise across code paths (5b9c32ac measured 1-3 ulp from FMA alone). Thread-count invariance is a test in its own right. |

## 8. Verification assets (tests, check values, paper tables, oracle hooks)

### 8.1 Reusable reference data in-repo, with provenance

| Asset | Location | Format | Size | Provenance and measured agreement |
|---|---|---|---|---|
| Transport check rows | `CoolProp-Tests.cpp:44-301` (eta), `:347-581` (lambda) | `vel(...)` initializers; citation in the preceding comment | 318 active rows, 41 fluids | Mostly P-paper/P-IAPWS. About 28 rows are R-other (REFPROP 9.1/10, incl. Mylona "taken from the implementation in REFPROP 10.0", `:542-546`) or R-self (`:76-78, 397`); 6 rows "Some of these don't work" carry no source (`:408-414`). Measured: Water vs IAPWS 3.3e-8 (eta) and 2.7e-9 (lambda); D2O <= 5.3e-11; Lemmon-Jacobsen N2/Ar/O2/air <= 1.2e-5; median row 7.8e-6. |
| Pure-fluid EOS verification states | `CoolProp-Tests.cpp:4399-4474` (R1234yf, 6 states), `:4798-4815` (12 fluids, 1 row each) | Rows (T, rho, p, cv, cp, w, rtol, "Paper, Table, Row") | 18 states, 68 values | P-paper. Measured: 67 of 68 values within half a unit of the last printed digit (relative errors 5e-14..4.9e-6). Exception: R1224YDZ p (8.4). |
| Fixed-point constants | `:4476-4502` | Tc, pc, rhoc, Ttriple, M | 2 fluids | P-paper; pass with SA on and off (tolerances loosened to admit the SA numerical critical point) |
| Mixture alphar values | `:4523-4740`, Tkaczuk `:2777-2856` | (pair, z, T, rho, alphar) | 11 pairs + 3 reducing triples | Bell Table XIII (5 pairs) and R134a+R1234zeE (Table XI): P-paper at 1e-10 (measured <= 1.1e-14, with tau within 0.0008 of 0.8). R1234yf+R134a: table vs pin differ by 6e-9. R1234yf+R1234zeE: R-self, 0.89% from Table XI. NIST IR 8570: R-self. Tkaczuk p: P-paper but generated by the authors' CoolProp script (`:2784-2789`), so it needs `NORMALIZE_GAS_CONSTANTS=false`. |
| Superancillary mp check points | `dev/fluids/*.json` -> `EOS[0].SUPERANCILLARY.check_points` | Keys `"T / K"`, `"p(mp) / Pa"`, `"rho'(mp) / mol/m^3"`, `"rho''(mp) / mol/m^3"` and `*(SA)/*(mp)` ratios | 390 points (3 x 130 fluids), 97 kB | P-mp. Measured SA error at these points: p median 4.8e-14, p95 4.1e-11, max 4.7e-9 (MethylStearate/Linolenate/Oleate near 386-391 K); rho <= 4.4e-13. Not representative near Tc (8.4). |
| `source_eos_hash` | Same block | 16-hex FNV-1a | 130 | Integrity stamp tying each SA to the exact EOS (section 3) |
| fastchebpure dense `outputcheck/{fluid}_check.json` | External GitHub release (docs pin `2026.06.02-v2`) | JSON on a dense log-Theta grid with mp columns | All SA fluids | P-mp; the best saturation oracle, including near-critical points. The release must match the v8.0.0 EOS hashes; license unverified. |
| Melting check values | `Ancillaries.cpp:282-312` (IAPWS R14-08, 4 points); `CoolProp-Tests.cpp:4165-4193` (D2O, Herrig 2018 Sec. 3.4, 4 points, 1e-6); `-AirMelting.cpp:21-25`; `-NeonMelting.cpp:22-26` | Literals | 12 | Water and D2O: IAPWS / paper. Air and Neon: R-self, "obtained by inverting the fitted curve" (Air's fit reproduces REFPROP's Lemmon-2000 curve to <0.02%; Neon is CoolProp's own TDE fit). |
| Humid-air RP-1485 tables | `HumidAirProp.cpp:2452-2627`; `CoolProp-Tests.cpp:1028-1215` | Literals | ~120 values | P-paper, 4-5 printed digits |
| Robustness corpus | 86 issue-linked TEST_CASEs (`CoolProp-Tests.cpp` 57, `-HSU_D.cpp` 11, `-Michelsen.cpp` 8, ...); `-PXcdj.cpp:186-215`; `-PXFlash.cpp:34-166`; `-AirMelting.cpp:30-52` | Literal states tagged with issue numbers | ~100 states | S/C; high value as "must not fail" inputs for the Rust flashes |
| Consistency-plot recipe | `ConsistencyPlots.py:79-600` | Code | 40x40 + 20x20 per pair per fluid | C; the recipe for L4 sweeps |

### 8.2 The oracle in practice

`CoolProp.CoolProp.AbstractState` has 169 public names (measured):

| Group | Methods | Notes |
|---|---|---|
| alpha^r | `alphar` + all 14 partials `d{1..4}alphar_d*` up to 4th order | Total over all terms only |
| alpha^0 | `alpha0`, `dalpha0_*`, `d2alpha0_*`, `d3alpha0_*` (10) | Up to 3rd order; no 4th order exposed |
| Per-block isolation (a technique, not an API) | `CP.add_fluids_as_JSON('HEOS', json)` with a clone of the fluid whose `alphar` list holds one block (rename NAME/ALIASES/CAS/REFPROP_NAME, keep `EOS[0]` only, drop `SUPERANCILLARY`) | Measured on Water (Power + Gaussian + NonAnalytic) at 4 states including near-critical: the sum of parts equals the full value to <= 2.2e-13 relative over all 15 derivatives. Gives term-level fixtures for every block type. |
| Reduced variables, phase | `tau`, `delta`, `T_reducing`, `rhomolar_reducing`, `get_reducing_state`, `specify_phase`/`unspecify_phase` | Impose a phase to evaluate the EOS at any (T, rho), including metastable and unstable states |
| Properties | p, h, s, u, g, a, cv, cp, cp0, w, Z, `isothermal_compressibility`, `isobaric_expansion_coefficient`, `fundamental_derivative_of_gas_dynamics`, `PIP`, `*_residual`, `*_idealgas`, `*_excess`, `Bvirial`, `Cvirial`, `dBvirial_dT`, `dCvirial_dT`, `fugacity(_coefficient)`, `chemical_potential`, `tangent_plane_distance` | |
| Derivatives | `first/second_partial_deriv`, `first/second_saturation_deriv`, `first/second_two_phase_deriv`, `first_two_phase_deriv_splined` | |
| Saturation | `update(QT/PQ)`, `saturated_{liquid,vapor}_keyed_output`, `saturation_ancillary`, `update_QT_pure_superanc`; module classes `SuperAncillary` (`eval_sat`, `eval_sat_many` only), `ChebyshevExpansion`, `ChebyshevApproximation1D` | SA on/off via config |
| Critical and limits | `T/p/rhomolar_critical` (SA-dependent), `true_critical_point`, `all_critical_points`, `criticality_contour_values`, `Ttriple`, `p_triple`, `Tmin`, `Tmax`, `pmax`, `melting_line`, `has_melting_line`, `build_spinodal`/`get_spinodal_data`, `ideal_curve`, `get_state(name)` | Python `SimpleState` exposes only T, p, rhomolar (no h/s) |
| Transport | `viscosity`, `conductivity`, `*_contributions` (unreliable, R13), `conformal_state`, `surface_tension`, `Prandtl` | 70 of 136 fluids have no viscosity model and 73 no conductivity model (measured) |
| Data, config, hooks | `get_fluid_param_string(f,"JSON")`, `get_fluid_constant`, `get_fluid_parameter_double`, `get/set_binary_interaction_double` (key `"Fij"`, not `"F"`), `change_EOS`, `get_config_as_json_string`, `set_config_*`, `set_reference_state*`, `get_BibTeXKey`, `__version__`, `__gitrevision__` | |

Throughput, single thread, random points per row: 10k, except 2k for QT SA-off and 3k for HP/HS/PropsSI (measured; states/s; Water us/state in brackets):

| Workload | Water | n-Propane | Nitrogen | R134a |
|---|---|---|---|---|
| DmolarT, imposed phase, + alphar and 1st/2nd partials | 289k [3.5] | 925k | 615k | 906k |
| + all 25 alphar/alpha0 values and derivatives (term-fixture row) | 162k [6.2] | 285k | 173k | 234k |
| + p, h, s, cv, cp, w | 246k [4.1] | 618k | 373k | 534k |
| PT flash + rho, h | 46k [22] | 171k | 104k | 159k |
| PT + viscosity + conductivity | 29k [34] | 126k | 84k | 117k |
| QT, SA on | 1.98M [0.5] | 2.30M | 2.33M | 2.23M |
| QT, SA off (iterative VLE) | 14.6k [68] | 58k | 49k | 56k |
| HmolarP flash | 5.2k [193] | 20k | 11k | 22k |
| HmolarSmolar flash | 11.2k [90] | 23k | 15k | 23k |
| `PropsSI` scalar (rebuilds the state per call) | 13k [76] | 16k | 16k | 17k |

A full 10k-row, 23-column EOS fixture including region labels and CSV formatting takes 0.19 s (52k rows/s, measured). The whole fixture set (136 fluids x ~6 kinds) takes minutes on one core and well under a minute on a 6-core pool.

### 8.3 Proposed fixture format

Layout:
- `fixtures/oracle.lock`: package, version 8.0.0, git ae81610e..., wheel tag, sha256 of `CoolProp.abi3.so`, Python version, generator sha256, full config JSON.
- `fixtures/paper/<fluid>/<bibkey>.csv`: transcribed values, printed strings kept.
- `fixtures/mp/<fluid>.csv`: SA check points plus fastchebpure dense points.
- `fixtures/coolprop-8.0.0/{term,eos,sat,flash,transport,crit}/<fluid>.csv`
- `fixtures/overrides.toml` (section 8.5).

Encoding:
- Plain CSV with a `#` metadata header, so no parser dependency in Rust tests.
- Floats as Python `repr`, the shortest round trip. Rust `f64::from_str` is correctly rounded, so values survive bit-exact (measured for all finite values).
- `nan` is an explicit token, plus a `status` column (`ok` | `err:<class>`) that records oracle failures instead of dropping rows.

```
# fixture: eos/v1
# oracle: CoolProp 8.0.0 git=ae81610e7d23efc57f9d051c8e70a4d66e87537f wheel=cp312-abi3-manylinux_2_17_x86_64 so_sha256=05d85591...
# config: {"ENABLE_SUPERANCILLARIES":true,"NORMALIZE_GAS_CONSTANTS":true,...}   # full get_config_as_json_string()
# env: COOLPROP_* and PXFLASH_* scrubbed; LC_ALL=C
# fluid: Water json_sha256=... source_eos_hash=... R=8.314371357587 refstate=DEF
# grid: T~U[Ttriple,Tmax] rho~logU[1e-6*rhoL(Ttriple),rhoL(Ttriple)] n=10000 seed=1 phase=imposed:gas
# columns: T,rhomolar,region,status,p,hmolar,...       # SI molar units
# tol: in,in,label,label,prop,prop,...                 # tolerance class per column
# source: coolprop | paper:<bibkey>/<table>/<row> | mp:fastchebpure@<tag>
```

Columns by kind:
- **term**: `block_idx, block_type, tau, delta, a, a_t, a_d, a_tt, a_td, a_dd, a_ttt, a_ttd, a_tdd, a_ddd, a_tttt, a_tttd, a_ttdd, a_tddd, a_dddd`
- **eos**: `T, rho, region{stable|metastable|unstable}, status, p, h, s, u, cv, cp, w, Z, alphar..., dpdrho_T, dpdT_rho, B, C`
- **sat**: `T|p, Q, p, rhoL, rhoV, hL, hV, sL, sV, path{superanc|vle|crit_spline|ancillary}`
- **flash**: `pair, x1, x2, T, rho, p, h, s, u, Q, phase, truth{PT|QT}, status`
- **transport**: `T, rho, eta, lambda, model_kind{direct|ecs|chung|rhosr|hardcoded}, status`
- **crit**: `Tc_pub, pc_pub, rhoc_pub, Tc_num, pc_num, rhoc_num`

Tolerance classes (relative unless stated):

| Class | Bound | Applies to | Rationale |
|---|---|---|---|
| `exact` | bitwise | Constants copied from data (M, R, published Tc) | Same data |
| `term` | `abs(a-b) <= 1e-13 * sum_i abs(contribution_i)` (cancellation-aware), floor 1e-300 | Term values and derivatives at (tau, delta) | Sum-of-blocks vs total measured <= 2.2e-13 relative to the result near the critical point; FMA/libm ulps (5b9c32ac) |
| `prop` | 1e-12; 1e-8 within abs(T/Tc-1) < 1e-3 and abs(rho/rhoc-1) < 0.1 | Direct EOS properties | Cancellation near the critical point (cp, w) |
| `sa-coeff` | 1e-14 | Rust SA vs CoolProp SA at the same T (same Chebyshev coefficients) | Same expansion |
| `sat-mp` | p 1e-11, rho 1e-11; Theta < 1e-3: rho 1e-6 (inference, to be tightened by measurement) | Rust VLE solver vs mp points | mp is exact for the EOS; near-critical conditioning |
| `sa-fit` | 4 x fastchebpure's own ratio, as CoolProp does | SA vs mp | SA fit error reaches 4.7e-9 (p) at the check points and ~3-4.5e-5 (rho) near Tc (66859efb) |
| `flash` | T, rho 1e-9; Q absolute 1e-8; relax near Tc | Round trips | Iterative solvers; derive from the Rust solver's own stopping rule |
| `tr-direct` / `tr-ecs` | 1e-12 / 1e-8, with the SA flag pinned | Transport at the same (T, rho) | ECS inner solve and Tc/rhoc dependence |
| `paper` | Half a unit in the last printed digit, computed from the stored string | P-paper rows | What the authors guarantee; flags R1224YDZ (8.4) |
| `smoke` | finite / documented error class | Robustness lists | |

Storage: 136 fluids x 6 kinds x 10k rows is about 3.5 GB raw. Commit a core subset (~10 representative fluids x 500 rows per kind, single-digit MB) plus a sha256 manifest of the full set. A nightly CI job regenerates the full set from `oracle.lock` and fails on manifest drift.

### 8.4 Known-suspect CoolProp 8.0.0 outputs: do not bake these into fixtures as truth

| Output | Evidence | Policy |
|---|---|---|
| Water melting T on the ice VI branch | R10 (`Water.json:98`; dT = +0.965 K at 1356.76 MPa) | IAPWS R14-08 wins |
| R1234yf+R1234ze(E) mixture alphar | Measured -0.464679 vs Bell 2022 Table XI -0.460595 (+0.89%). At the table's T = 469 K, CoolProp's tau = 0.80266 (T_red = 376.45 K), whereas tau should be about 0.80, so the reducing-T parameters differ from those behind Table XI (inference). Every other Bell pair lands at tau within 0.0008 of 0.8. The "pre-publication R1234yf EOS" rationale (`CoolProp-Tests.cpp:4524-4527`) is contradicted by R1234yf+R134a, which matches Table XI to 6e-9. | Paper check needed; exclude from `paper`-class fixtures until resolved |
| R1224YDZ p at (400 K, 8000 mol/m^3) | Measured 21.1790735 MPa vs the transcribed "21.17909" (`CoolProp-Tests.cpp:4798`): 7.8e-7 relative, 3.3x half a unit in the last digit, while cv, cp, w match. Hidden by the 1e-5 tolerance. | Re-check against Akasaka & Lemmon IJT 2023 Table 7 before using |
| `T/p/rhomolar_critical` with SA on | Section 4 (up to 5.9%) | Two fixtures: published and numerical |
| Near-critical SA saturated densities | fastchebpure fit error up to 3.3e-5..4.5e-5 for rho' / rho'' near Tc (upstream 66859efb's fit-quality table); the in-file check points stop at Theta = 0.1 | `sa-fit` class; prefer fastchebpure dense mp |
| ECS transport | Measured over 19 ECS fluids x 200 random single-phase (p, T): 7.3% throw ("Not able to get a solution" 215, "Conformal state solver failed" 47, "too many iterations" 15). R141b fails 20% and R14 19%. The SA flag shifts values by up to 1.5e-6 (via Tc/rhoc, section 4). | Record the failure as `status=err`; paper points only for ECS fluids |
| Missing or regressed transport models | R1233zd(E) viscosity dropped in 8.0.0 ("It answered 6.3689e-4 Pa-s in v7.2.0", upstream 14da1f0d); Xenon viscosity absent (added f854d34b); R11 viscosity fails at dilute 1.2 T_red (measured) | Paper verification points only |
| PR/SRK entropy | Measured `T*(ds/dT)_p / cp` = 1.356 for Methane under PR and SRK (HEOS 1.000); verifier at (250 K, 5 MPa): PR 1.383, SRK 1.382, HEOS 1.000 (state-dependent); fixed upstream 9b96b64b | No cubic s or S-based flash fixtures from 8.0.0 |
| Nitrogen, Ethylene, OrthoHydrogen, n-Undecane | Reducing density (and M for Ethylene and OrthoHydrogen) differ from the publications; saturation densities shift by 1.3e-7 (N2), 2.1e-5, 3.0e-5 and 1.1e-5 (upstream 2acbbc82; SAs refit in 66859efb) | Paper constants win; flag these fluids |
| Water surface tension | Mulero-JPCRD-2012 two-term fit (`Water.json` ANCILLARIES), not IAPWS R1-76(2014). Measured vs IAPWS: +0.12% at 300 K, +0.015% at 373 K, -0.34% at 450 K, -0.90% at 600 K. | Label as "CoolProp model"; IAPWS comparison separate |
| eta/lambda for two-phase states; w for unstable states | R18 | Exclude; Rust returns an error |
| Pseudo-pure (Air, R404A, R407C, R410A, R507A) QT | p comes from the pL/pV ancillary; rho is the EOS root at (T, p_anc) seeded by the rho ancillary (`FlashRoutines.cpp:953-967`); 0 < Q < 1 throws | `path=ancillary`; never compare to an EOS VLE |
| Near-critical saturation without SA | Spline densities and averaged p (`FlashRoutines.cpp:935-942`); within 1 uK of Tc both phases are rhoc | `path` column; `sat-mp` only |
| Mixtures | Default `NORMALIZE_GAS_CONSTANTS` shifts p by up to 2.7e-6 vs the published models; 3rd-order mixture alpha0 throws (1fca2b92); `XN_INDEPENDENT` fugacity derivative and spurious PT roots (014c8186, b17661f6) | Pin config; defer mixture fixtures to P2 |
| Transport rows with R-other or R-self provenance, and the 32 rows deviating > 0.1% | R5, R6 | The paper value wins once the paper is checked (crossref "updated-by" erratum check, `master:dev/agent-notes.md:203-204`) |
| `*_contributions()` for non-decomposed models | R13 | Exclude |

### 8.5 Recommended test pyramid for the Rust TDD

| Level | Unit under test | Truth sources (priority order) | Grid | Tolerance |
|---|---|---|---|---|
| L0 data | Parse `dev/fluids/*.json` into typed structs: vector-length checks (upstream 681472bf, 427c8aeb, ae54172f), unknown block type = error, FNV-1a `source_eos_hash` parity | The JSON itself; hash self-test `8e75626511d00b5c` and the 130 stored hashes | All 136 fluids | exact |
| L1 term | Each alphar/alpha0 block type: value + 14 derivatives (+ 4th-order alpha0, which the oracle does not expose) | (a) autodiff of the same generic code (dual/hyper-dual); (b) oracle block isolation (8.2); (c) an independent mpmath evaluation from the JSON coefficients as tie-breaker (inference: cheap to write) | ~300 (tau, delta) per block: delta log-spaced down to 1e-8, tau in [Tc/Tmax, Tc/Tmin], plus tau = delta = 1 neighbourhoods for NonAnalytic | term |
| L2 EOS at (T, rho) | p, h, s, u, cv, cp, w, virials, Z, derivatives; identities (Maxwell, cp - cv, Gibbs-Helmholtz, `first_partial_deriv` algebra) | Paper verification tables, then oracle with imposed phase | (T, rho) grid with region labels. Measured share of a uniform 60x60 grid (T in [Tt, Tmax], rho up to rhoL(Tt)) inside the dome / with dp/drho < 0: Water 18.0% / 13.3%, n-Propane 37.7% / 21.8%, CO2 4.2% / 2.8%, H2 2.5% / 1.5%, R134a 54.4% / 37.8%. Fine for direct evaluation, never for flash truth. | prop / paper |
| L3 saturation | SA evaluation; VLE solver; ancillaries; numerical critical point | mp check points + fastchebpure dense, then CoolProp SA | Theta = 1 - T/Tc log-spaced 1e-7..(1 - Tt/Tc), both Q | sa-coeff / sat-mp / sa-fit |
| L4 flash | PT, DT phase determination, PH, PS, PU, D+{H,S,U}, HS, Q-pairs | Round trip from truth states built in (p, T) for single phase and (T, Q) for two phase: "Generate flash test points in (p,T), not density bands" (`master:dev/agent-notes.md:114-116`); plus the ~100 issue states | 40x40 (log p, T) + 20x20 (T, Q) per fluid (consistency-plot recipe). Accept a root only if it reproduces the inputs, lies in [Tmin, Tmax], and has dp/drho > 0 and cv > 0 (`:110-113`). Validate speed on whole grids (`:132-135`). | flash |
| L5 transport | Dilute, initial-density, residual and critical parts; ECS conformal state; surface tension | Paper/IAPWS points first ("Validate a transport correlation against its paper's own verification points", `:187-190`), then oracle at the same (T, rho) | Stable single phase only; ECS couples fluids (`:175-182`) | paper / tr-* |
| L6 API and kernel | Scalar = batch = SIMD = N threads x {same, different} fluids; typed errors; no globals | Self: the scalar f64 path is the reference implementation | Random, property-based | exact (same path) or term (different path) |

How paper values override CoolProp:
1. Every CoolProp-derived expectation is provisional. Paper, IAPWS and mp values are authoritative and stored with their printed strings.
2. When Rust and CoolProp disagree beyond the class tolerance, look for a paper or IAPWS value. If Rust matches it within `paper` while CoolProp does not, add an `overrides.toml` entry `{id, fluid, quantity, region, policy = "use-paper"|"skip-coolprop", source = "<bibkey> Table x row y", evidence = "<path:line / upstream commit / measurement>", erratum_checked = true}`; the CoolProp rows it covers stop being asserted. With no paper value, break the tie with an independent evaluation (autodiff identity or mpmath on the JSON coefficients) before deciding.
3. Never widen a tolerance to fit CoolProp (R5). Never "fix" a paper value without a published erratum (the anti-pattern at `HumidAirProp.cpp:2536`).
4. When the oracle pin moves (e.g. to a release with 2acbbc82, 9b96b64b or da48a3ee), re-run all fixtures. Overrides that CoolProp now agrees with are marked `resolved-upstream`, which keeps the audit trail.

Seed `overrides.toml` with: Water ice VI (R10), the four reducing-density fluids, PR/SRK entropy, R1233zd(E) viscosity, two-phase transport, plus the two open paper checks (R1234yf+R1234zeE, R1224YDZ) as `policy = "investigate"`.

## 9. Port recommendation (units with priority P0-core / P1-early / P2-later / defer / drop; order; what to redesign)

| Unit | CoolProp source | Priority | What to do or redesign |
|---|---|---|---|
| U1 `verify` dev-crate: fixture reader, provenance enum, tolerance classes, ulp/rel/condition-aware comparators | Analogues: `vel` (`CoolProp-Tests.cpp:33-42`), Catch `Approx` usage | P0-core | Zero dependencies (hand-rolled CSV and `#` headers); per-fluid lazy loading; failures print row, column, class and provenance |
| U2 Oracle generator (Python, uv-pinned) + `oracle.lock` | `ConsistencyPlots.py`, `generate_Plot_test_data.py`, `inject_superanc_check_points.py` | P0-core | Scrub `COOLPROP_*`/`PXFLASH_*`, set every config key explicitly, assert 136 fluids loaded, fork pool, record error classes, write the manifest |
| U3 Term fixtures + autodiff harness | `Helmholtz.cpp:1402-1819`, `MixtureDerivatives.cpp:1105-1685` | P0-core | Block isolation via `add_fluids_as_JSON`; generic-scalar terms so autodiff tests production code; cover all 17 block types used by the 136 JSONs (7 residual + 10 ideal-gas; the loader accepts 20) incl. DoubleExponential and EnthalpyEntropyOffset |
| U4 Paper corpus (EOS) + override registry | `CoolProp-Tests.cpp:4393-4502, 4787-4840`, `Ancillaries.cpp:282-312`, `CoolProp-Tests.cpp:4165-4193` | P0-core | Transcribe the paper verification tables, keeping printed strings; start with IAPWS-95, CO2 (Span-Wagner), N2, R134a, propane |
| U5 EOS (T, rho) fixtures + identity property tests | New (oracle) | P0-core | Region labels; imposed phase; NaN-aware |
| U6 Saturation verification | `CoolProp-Tests.cpp:3477-3910`, `test_superancillary.py` | P1-early | mp points + fastchebpure dense import; published vs numerical critical point |
| U7 Flash round-trip sweeps + robustness corpus | `-HS.cpp`, `-HSU_D.cpp`, `-PXFlash.cpp`, `-PXcdj.cpp`, `CoolProp-Tests.cpp:1495-1743, 5181-6110`, `ConsistencyPlots.py` | P1-early | (p, T) and (T, Q) truth; HS acceptance rule; the 86 issue cases as must-not-fail inputs; nightly full sweep (never hidden) |
| U8 Transport verification | `CoolProp-Tests.cpp:30-600, 6604-6619` | P1-early | 318 rows with a provenance column; per-model contribution fixtures; ECS failure handling |
| U9 Data integrity: FNV-1a tree hash, pure-fluid schema | `CoolProp-Tests.cpp:3598-3832`, `check_superanc_freshness.py`, `dev/validate_fluid_schemas.py` | P1-early | Byte-compatible with fastchebpure; add the missing pure-fluid and binary-pair schemas (serde `deny_unknown_fields`) |
| U10 Parity and concurrency tests for side-by-side kernels | Analogues `CoolProp-Tests.cpp:6186-6247`, `HumidAirProp.cpp:2667-2716` | P1-early | Scalar reference vs batch/SIMD/threads with ulp budgets; thread-count invariance; the foundation for later SIMD work |
| U11 Mixture verification (Bell / NIST-IR / Tkaczuk, Michelsen invariants, composition derivatives) | `CoolProp-Tests.cpp:2777-2856, 4505-4740`, `-Michelsen.cpp` | P2-later | Gas-constant convention as a typed per-mixture option; equilibrium-residual checks with the trace-component rule (`-Michelsen.cpp:25-27,40`) stated explicitly |
| U12 Consistency report CLI (nightly HTML/MD) | `Web/scripts/fluid_properties.Consistency.py`, `_consistency_report.py` | P2-later | Rust binary + CSV, replacing matplotlib |
| U13 Humid air RP-1485 | `HumidAirProp.cpp:2447-2846`, `CoolProp-Tests.cpp:808-1465`, `Web/fluid_properties/Validation/*` | P2-later | Fix the 273.13 typo when transcribing |
| U14 PC-SAFT, cubic, VTPR, UNIFAC tests | `CoolProp-Tests.cpp:2749-3160, 4233-4398`, `-Cubic*.cpp` | defer | Values lack provenance; rebuild from papers when those backends are ported |
| U15 Incompressible tests | `IncompressibleFluid.cpp:499-758`, `IncompressibleBackend.cpp:576-1079` | defer | |
| U16 SVDSBTL, SBTL and tabular tests (~130 cases) | `-SVD*.cpp`, `-SBTLAdapter.cpp`, `TabularBackends.cpp:1713-2035` | defer | Only if a tabular backend is ported; keep the mid-cell probe idea (`master:dev/agent-notes.md:215-218`) |
| U17 REFPROP comparison tests (28 tagged) | `REFPROPMixtureBackend.cpp:2826-3051`, `CoolProp-Tests.cpp:6391-6600` | drop | Proprietary, not regenerable; replaced by paper data |
| U18 Catch2 plumbing, preflight tag logic, always-success ASan runner, `Tests.cpp`, `TestObjects.*`, prototype/benchmark tests | `src/Tests/Tests.cpp`, `catch_always_return_success.cxx`, `-HS-prototypes.cpp`, `-TermCacheProfile.cpp`, `dev/ci/preflight.sh` | drop | `cargo test`/nextest + criterion benches |

Order: U1, U2, U3, then U4/U5 (TDD of terms and EOS properties), then U6, U9, U7, U8, U10, then the P2 units.

## 10. Open questions

1. Licensing and redistribution: (a) fastchebpure dense outputs (license unverified here); (b) generated CoolProp-derived fixtures (CoolProp is MIT); (c) transcribed paper tables. Are these OK to commit in a permissive OSS repo?
2. Stay pinned to 8.0.0 with the override registry, or also run a second "upstream-fixed" oracle built from origin/master (in scratch, never in the reference checkout) to confirm overrides such as 2acbbc82, 9b96b64b and da48a3ee?
3. Who checks the two open paper discrepancies: Bell 2022 Table XI R1234yf+R1234ze(E) (reducing parameters?) and Akasaka & Lemmon 2023 R1224YDZ p (transcription?)?
4. Rust public API default: should `critical_point()` mean the published constants or the EOS numerical point, and should both be exposed as distinct types?
5. libm and FMA policy: std (platform libm) or the `libm` crate for bit-identical Windows/Linux/WASM results? This decides whether cross-platform parity can be `exact` or must be `term`.
6. Paper transcription order and verification (double entry?). Which ~10 fluids form the committed core fixture set?
7. Should the mixture gas-constant convention (`NORMALIZE_GAS_CONSTANTS`) be a typed per-mixture option in Rust, with the paper's convention as default?
8. Should the oracle also be sampled from Windows/macOS/Pyodide wheels to measure cross-platform ulp drift before fixing the `term`/`prop` classes?
9. Is an independent mpmath evaluator from the JSON coefficients (L1/L2 tie-breaker) worth maintaining alongside the CoolProp oracle?

## Verification log

- Date: 2026-10-04. Adversarial verification against the reference checkout at v8.0.0 (ae81610e) and the PyPI `CoolProp==8.0.0` oracle. No partial edits from an earlier verifier were found (no broken tables, duplicate sections or earlier log).
- Claims checked: about 190 (every path:line citation, every count and every rot item R1-R18; measured values re-run where cheap).
- Re-confirmed by recount or re-measurement: 495 TEST_CASEs (444 in `src/Tests`, 51 in library files), 31 hidden, 58 `[slow]`, 8 `[!benchmark]` + 10 `[benchmark]`, 28 REFPROP-tagged, 86 issue-linked (57/11/8); 25 hand-listed TUs; 4,656 `ENABLE_CATCH` lines in 21 files; 318 transport rows (177 + 141, 41 fluids), the 7 near-bound rows, 32 rows > 0.1%, median 7.8e-6; 136 fluids, gas-constant and block-type histograms, 130 SAs, 390 check points (97 kB), 63 spline fluids, SA = 89% of compact JSON; 38 config keys and defaults; JSON parity for 136/136 fluids; SA critical shifts (34 > 1e-3, 111 > 1e-6, R40/MDM/Helium/MM); 70/73 fluids without viscosity/conductivity; 57/56 stale reducing h/s fluids; Water ice VI (T = 320.965 K, p(320 K) = 1337.45 MPa); R1224YDZ p = 21.1790735 MPa; R1234yf+R1234zeE tau = 0.80266; two-phase Water eta/lambda returned; 169 AbstractState names; water surface tension vs IAPWS R1-76; extension size and sha256; spot throughput (QT SA 0.46 us, PropsSI 74 us). All cited upstream commits confirmed to be after v8.0.0 except 655e8f14 (2014), as the doc states.
- Not re-measured (left as the original author's measurements): ECS failure statistics, full throughput and parallel-scaling tables, determinism sweep, region shares of the (T, rho) grids, fixture size, block-isolation sum test, near-critical spline window.
- Corrections:
  1. Scope: "21 library sources" -> 20 `.cpp` + `Helmholtz.h`; total ~34k -> ~36k lines.
  2. `catch_always_return_success.cxx:9-13` -> `:10-14` (and `:12-13` -> `:11-13` in R1).
  3. Preflight: the SBTL arm (`:230`) also ORs benchmarks in; R1 scoped to the local gate and the legacy ASan runner (the CI Catch2 job honours the exit code).
  4. Term test: FD fallback is 8 of 19 instances, not 12 (5 GeneralizedExponential instances use mcx); ranges `Helmholtz.cpp:1402-1816` -> `1402-1819`, fixture `1408-1745` -> `1408-1748`, SAFT skip `1767-1771` -> `1767-1770`.
  5. Mixture derivatives: range `1105-1680` -> `1105-1685`; "17 perturbed copies" -> 16 single-variable copies plus 10 per-component vectors.
  6. Humid air: `HumidAirProp.cpp` has 10 active TEST_CASEs, not 11 (one at `:2812` is in a block comment); block ends at `:2846`, not `:2830`.
  7. `[slow]` tabular share: 57 of 58, not 47.
  8. HS tolerance citation: `:63` is T and Q (two-phase), `:120` is T and rho.
  9. Air melting check values are R-self (inverted CoolProp fit), not R-other.
  10. Transport loop line `:593` -> `:594`.
  11. R12 and 8.1: "fixed-point tests pass only with SA on" was false; they pass with SA on and off (measured).
  12. R4: ice VI cross-reference R11 -> R10; HSU_D margin rationale comment noted.
  13. R3: GitHub-issue cases quantified (6 of 10 no-throw only, #2622 asserts nothing, 2 finiteness only).
  14. R5: labelled as an inference (tuning is explicit only for Heptane).
  15. R14: "self-contradictory reason" softened to "muddled reason".
  16. R15: `makeMatrix` is a duplicate global definition, not a redefinition; second include site `IncompressibleBackend.cpp:581` added.
  17. Wrong cross-references "(R13)" for the SA-flag ECS shift (section 4 and 8.4) fixed.
  18. `FlashRoutines.cpp:926-942` -> `927-942`; `951-967` -> `953-967`; `REFPROPMixtureBackend.cpp:2826-3060` -> `2826-3051`.
  19. U3: "18 JSON block types" -> 17 in use (20 accepted by the loader). U11: dropped the unsupported "without the trace-component skip" (the skip is documented at `-Michelsen.cpp:25-27,40`).
  20. Section 4: `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY` is read at the one-time load, not into a function static; noted that the JSON string is a one-element array.
  21. 8.4 PR/SRK entropy: verifier value at (250 K, 5 MPa) added (1.383/1.382); the original 1.356 was not reproduced because its state was not recorded.
  22. Added by verifier (R8): loader length checks are bare `assert`s, compiled out in the Release/NDEBUG builds CI tests (upstream ae54172f). Also noted that Methanol's only EOS cross-check is REFPROP-gated.
  23. Escaped the `||` in the R17 row, which split the table cell.
