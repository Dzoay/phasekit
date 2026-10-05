# 12 Docs, roadmap, known issues and rot history - CoolProp v8.0.0 map

> Scope: `README.md`, `FAQ.md`, `TTD.txt`, `docs/superpowers/` (27 files / 9,959 lines at v8.0.0; 35 on master incl. `specs/2026-09-06-refprop-coolprop-functionality-gaps.md`), `dev/agent-notes.md` (master only, 229 lines), `Web/develop/` (8 pages, 461 lines), `Web/coolprop/*.rst` (14 pages, 4,987 lines, skimmed), `.beads/issues.jsonl` (469 records at v8.0.0, 652 on master), GitHub issues (2,221: 2,162 closed, 59 open on 2026-10-04), git history (6,207 commits to v8.0.0, 151 after), plus cross-cutting measurements over `src/` + `include/` (102,978 lines, of which 17,716 are tests). ~115k lines surveyed. Part of the coolprop-rs port plan; cites the v8.0.0 source (ae81610e).

Conventions. `path:line` = v8.0.0 unless prefixed `master:`. `bd:<id>` = beads issue (`CoolProp-<id>`); the master copy of `.beads/issues.jsonl` is the complete archive (586 issues + 66 "memories"; all closed 2026-09-26 when tracking moved to Linear, survivors renamed `COO-n`). `#n` = GitHub issue/PR. `[oracle]` = run for this document against CoolProp==8.0.0. "(inference)" = not directly demonstrated. Keyword-classified counts are approximate and overlap. Peer docs are cited as `01 §6 R9` etc. and hold the subsystem-level detail; this doc is the cross-cutting register.

## 1. Purpose and concepts

This area is CoolProp's meta-layer: what maintainers wrote about intent, defects and direction, and what the history says about where the code breaks. It is mined for (a) a rot register, (b) recurring bug classes, (c) churn/fix hotspots, (d) stated direction and pain points, (e) user demand, (f) what a Rust design can kill by construction.

Headline findings:

1. **The architecture is the 2014 v5 design with 2025-26 bolt-ons.** One mutable object (`AbstractState`/`HelmholtzEOSMixtureBackend`) fuses model, state, memo cache and solver workspace; six friend classes write its internals directly. Most rot classes below derive from that fusion.
2. **Activity is bimodal.** 2014-16: 3,992 commits; 2017-24: 1,012 commits (a maintenance trough; v7.0.0 notes "Finally(!) got the Catch2 tests to all pass", `Web/coolprop/changelog.rst:565`); 2025-26: 759 commits to v8.0.0 + 151 after, with 349/759 (46%) and 113/151 carrying `Co-Authored-By: Claude`.
3. **v7.2.0 to master grew non-test source 66k to 106k lines** (+61%): `FlashRoutines.cpp` 2,311 to 5,154 at v8.0.0 (+123%) and 5,462 on master; `VLERoutines.cpp` 2,135 to 3,282 to 4,137. Most of the growth is fallback paths layered on the old structure (inference). Of 103 commits touching `src/Backends/Helmholtz` in v7.2.0..master, 56 (54%) have a fix-like subject line (87 if the whole commit message is matched, which over-counts feature commits that mention "fix" in their bodies).
4. **The oracle has known defects.** CoolProp 8.0.0 ships wrong reducing densities for 4 fluids, a missing R1233zd(E) viscosity, thermodynamically inconsistent cubic entropies, approximate virials and accepts invalid inputs (§6.3). TDD needs an errata registry from day 1.
5. **Data, not code, is the biggest churn surface** (`dev/fluids`: 4,673 file-changes in 424 commits), and data-provenance drift is a first-class bug class (fossil CoolProp 4.2 values in 57 fluids; transcription errors from papers).
6. **Users ask mostly about integration** (install, wrappers, Excel/MATLAB, platforms: ~23% of all issue titles; the 12 most-commented issues are almost all install/wrapper/performance), then mixtures (~11%), flash failures and transport gaps.
7. **Several structural fixes were deliberately not pursued in C++** at the 2026-09-26 triage (always-bracketed density solve, Chebyshev mixture density rootfinder, diagnostics API, error-returning PropsSI, worker-pool batch API). They are cheap to adopt from day one in a new design.

| Era | Tags | Commits | Character |
|---|---|---|---|
| 2014-05 v5 rewrite to 2016 | v5.0.0 (2014-12) to v6.0.0 (2016-05) | 3,992 | Architecture set: AbstractState, HEOS, FlashRoutines, JSON fluids |
| 2017-2024 | v6.1 to v6.8 | 1,012 | Low activity; red test suite until v7.0.0 |
| 2025 | v7.0.0 (2025-08) to v7.2.0 (2025-11) | 332 | Superancillaries, array cache, docs to GitHub Pages |
| 2026-01 to 06 | v8.0.0 (2026-06-27) | 427 | SVDSBTL, Qmass, Michelsen PT flash, nanobind, nlohmann, header reorg, thread-safety, code-quality gates |
| 2026-07 to 10 | master (151) | 151 | Validation centralisation, races, data provenance, transport DSL, GERG strict, beads to Linear |

## 2. Structure (key types/functions -> path:line)

### 2.1 Sources inventory

| Source | Size / dates | Content and value |
|---|---|---|
| `README.md` | 29 lines | Pointer page (PyPI, SourceForge, docs, Discussions, FAQ). No technical content |
| `FAQ.md` | 87 lines: 6 usage + 2 build Q&As; last touched 2020-07 | User pain: reference-state h/s offsets, REFPROP 9.0 segfaults, missing transport, missing BIPs, retired MATLAB wrapper, "major flaw" no mixing enthalpy in incompressibles |
| `TTD.txt` | 150 lines: Python 2 snippet + fluid checklist; last content edit 2014-08 | Obsolete REFPROP-fluid porting queue (D2/D2O "can't be done yet", now done). Drop |
| `docs/superpowers/specs` + `plans` + `derivations` | 13+13+1 files at v8.0.0; +4+4 on master | Agent-workflow design docs: Qmass, backend options string, RapidJSON to nlohmann (5 phases), symbol visibility, superancillary de-leak, SVDSBTL demo/docs, C# DLL rename, sponsor promotion, exact virials / `A_xy` derivation; master adds transport DSL, GERG strict, ATcT formation enthalpy, REFPROP 10.1 transport supersession audits, REFPROP gaps assessment |
| `.beads/issues.jsonl` | v8.0.0: 419 issues + 50 memories (2026-04-25..06-26); master: 586 + 66 (to 2026-09-12) | Richest defect record: 198 bugs, 112 features, 15 epics, with root-cause write-ups, reproducers, close reasons, Linear `COO-n` mapping (33 surviving items) |
| `dev/agent-notes.md` (master) | 229 lines | The 66 beads memories distilled: gotchas on CI, REFPROP, flash design, provenance, transport coupling, paper reading |
| `Web/develop/*.rst` | 461 lines | Thin and stale: `backends.rst` lists 4 backends (12 families are declared in `include/CoolProp/DataStructures.h:493-508`, TREND among them though it has no implementation), testing/release/cmake how-tos |
| `Web/coolprop/changelog.rst` | 1,893 lines | Per-release highlights plus "Issues closed"/"PRs merged" lists: an issue-to-fix map back to v5.0.0 |
| `dev/Tickets/` | 27 repro scripts, 2,541 lines | Historical bug reproducers (tabular phase spec, CO2/water stability, metastable water, SES36 enthalpy) |
| GitHub | 2,221 issues; 59 open | Labels sparse (82 `invalid`, 50 `bug`); open set is curated after mass triage |

### 2.2 God classes and giant functions (lizard 1.24.0 over `src/`+`include/`, tests excluded; 3,300 functions)

| Unit | Size | Evidence |
|---|---|---|
| `AbstractState` | 1,722-line header; 181 `virtual`, 138 `calc_*` hooks, 136 `NotImplementedError` mentions; 320 function bodies | `include/CoolProp/AbstractState.h` |
| `HelmholtzEOSMixtureBackend` | 196 functions / 3,670 NLOC, plus 6 friend classes with full access | friends at `src/Backends/Helmholtz/HelmholtzEOSMixtureBackend.h:164-175`; solver sub-states at `:68-79` |
| Friend "satellites" of HEOS | FlashRoutines 33 fns / 3,183 NLOC; MixtureDerivatives 181 / 1,360; SaturationSolvers 16 / 1,031 (39 / 1,900 including nested classes such as `PTflash_twophase`); TransportRoutines 48 / 939; PhaseEnvelopeRoutines 6 / 595 | effective HEOS surface about 11-12k NLOC |
| `FlashRoutines::HSU_D_flash` | 1,000 lines, 713 NLOC, CCN 159 | `src/Backends/Helmholtz/FlashRoutines.cpp:1710-2710` |
| `IF97Backend::fast_evaluate` | 283 NLOC, CCN 108 | `src/Backends/IF97/IF97Backend.h:677-1010` |
| `SVDSBTLBackend::resolve_point_` | 340 NLOC, CCN 93 | `src/Backends/SVDSBTL/SVDSBTLBackend.cpp:1087-1595` |
| `PTflash_twophase::solve_michelsen` | 282 NLOC, CCN 84 | `src/Backends/Helmholtz/VLERoutines.cpp:2677-3079` |
| `T_/p_phase_determination_pure_or_pseudopure` | 348 + 346 NLOC, CCN 83 + 82, cloned blocks | `HelmholtzEOSMixtureBackend.cpp:2177-2564`, `:1699-2115` |
| `REFPROPMixtureBackend::update` | 442 NLOC, CCN 83 | `src/Backends/REFPROP/REFPROPMixtureBackend.cpp:1448-2148` |
| `FlashRoutines::DHSU_T_flash` | 308 NLOC, CCN 83 | `FlashRoutines.cpp:3802-4204` |
| `TabularBackend::update` | 393 NLOC, CCN 78 | `src/Backends/Tabular/TabularBackends.cpp:1025-1484` |
| `HSU_P_flash_singlephase_Brent` | 346 NLOC, CCN 77 | `FlashRoutines.cpp:2802-3326` |
| `PCSAFTBackend::calc_fugacity_coefficients` | 445 NLOC, CCN 71 | `src/Backends/PCSAFT/PCSAFTBackend.cpp:863-1351` |
| `init_CoolProp` (nanobind) | 683 NLOC | `src/nanobind_interface.cxx:383-1190` |
| Totals | 58 functions with CCN >= 30; 39 with NLOC >= 150 | lizard CSV |

### 2.3 Churn and fix hotspots (`git log v8.0.0 --follow --no-merges`; fix-like = case-insensitive match of fix/bug/crash/wrong/regress/segfault/typo/incorrect/broken, against the subject line or against the whole commit message)

Caveat (verifier): matching the whole message inflates the count badly for 2025-26 commits, whose long AI-written bodies mention "fix" even in feature commits. For example, 25 of the 27 SVDSBTL commits match on the body but only 7 on the subject, and the subjects are mostly `feat(svdsbtl)`. Use the subject column for fix-proneness.

| File | All-time commits | Fix-like: subject / whole message | v7.2.0..v8.0.0 | v8.0.0..master |
|---|---|---|---|---|
| `src/Backends/Helmholtz/HelmholtzEOSMixtureBackend.cpp` | 386 | 83 / 108 | 35 | 14 |
| `src/Backends/REFPROP/REFPROPMixtureBackend.cpp` | 239 | 90 / 107 | 28 | 8 |
| `src/Backends/Helmholtz/FlashRoutines.cpp` | 204 | 71 / 94 | 33 | 9 |
| `src/CoolProp.cpp` | 159 | 38 / 51 | 17 | 4 |
| `include/CoolProp/AbstractState.h` | 149 | 26 / 40 | 11 | 5 |
| `src/AbstractState.cpp` | 141 | 24 / 44 | 15 | 6 |
| `src/HumidAirProp.cpp` | 134 | 50 / 62 | 27 | 0 |
| `src/Backends/Helmholtz/VLERoutines.cpp` | 131 | 34 / 45 | 16 | 6 |
| `src/Backends/Tabular/TabularBackends.cpp` | 91 | 26 / 39 | 11 | 3 |
| `src/Backends/Cubics/CubicBackend.cpp` | 70 | 18 / 31 | 19 | 4 |
| `src/Backends/SVDSBTL/SVDSBTLBackend.cpp` | 27 | 7 / 25 | 27 | 3 |

- Directory churn (file-changes, all time): `dev/fluids` 4,673; `src/Backends` 3,215; `dev/incompressible_liquids` 2,045; `wrappers/Python` 1,130; `Web/coolprop` 881.
- Recent era (v7.2.0..master, commits / fix-like by subject / fix-like by whole message): `.github` 128/22/60, `wrappers` 111/26/73, `src/Backends/Helmholtz` 103/56/87, `CMakeLists.txt` 69/19/58, `src/Backends/Cubics` 39/15/31, `src/Backends/SVDSBTL` 30/7/27, `src/SBTL` 29/8/27, `src/HumidAirProp.cpp` 27/15/22.
- Line age (blame, reformat revs ignored): `FlashRoutines.cpp` 63% from 2025-26; `HelmholtzEOSMixtureBackend.cpp` 37% from 2014-16; `TransportRoutines.cpp` 0% from 2025-26 (stable, transcription-bug-prone data code).

## 3. Algorithms and formulas (cite the papers CoolProp cites)

Only the algorithm families implicated in recurring defects; peer docs 02-08 hold the formulas.

| Algorithm | Where | Literature (CoolProp BibTeX key) | Defect history |
|---|---|---|---|
| Michelsen TPD stability + phase split | `VLERoutines.cpp:2094`, `:2677` | Michelsen-FPE-1982a/b | Unconverged splits published as success (bd:1tbe.22), knife-edge near dew (bd:ft05, P0), false-positive stability (#3171), fail-open on trial failure (bd:1tbe.8) |
| Legacy blind PT flash | `MIXTURE_STABILITY_ALGORITHM=0` (`VLERoutines.h:664`) | Gernert-FPE-2014 | Returned Q = 0.5 unconditionally (bd:1tbe.1) |
| Pure saturation | `VLERoutines.h:77` | Akasaka-2008 | Termination tautology (04 §6 #6); superseded by superancillaries |
| Density root rho(T,p) | `HelmholtzEOSMixtureBackend.cpp:2906` (`solver_rho_Tp`) | Bell & Alpert 2018, FPE, Chebyshev rootfinding (planned, bd:zkc) | Standalone failure on 25-65% of mixture (T,p) cells (agent-notes); unstable-branch roots (#3448) |
| Bracketed 1-D solves | `FlashRoutines.cpp:459` (TOMS748 fallback) | Alefeld-Potra-Shi, ACM TOMS 748 (via Boost) | Added 2026 to patch bracketing failures (about 4,860 PHSU failures, bd:r1w7) |
| GERG-2008 reducing/departure | `ReducingFunctions.cpp` | Kunz-JCED-2012 | NaN at both-zero or tiny mole fractions (bd:8psx, b0rb); XN_DEPENDENT derivatives wrong (COO-15) |
| ECS transport | `TransportRoutines.h:274,278` | Huber-IECR-2003 | Reference-fluid coupling moves other fluids (agent-notes); per-call reconstruction (bd:a7je) |
| Friction theory / rhosr-CS | `TransportRoutines.cpp` | QuinonesCisneros-FPE-2000; Bell 2016 entropy scaling | `x_crossover` ignored (5418cec5); R1233zd(E) saturated-liquid viscosity 40-75% off REFPROP 10 and measured data in CoolProp 6.2.1 (#1826 comments; refit post-v8 in 14da1f0d) |
| Zero-density `A_xy` derivatives | `docs/superpowers/derivations/virial-axy.md` | teqp convention | p at rho = 0 gives `0*Inf`; virials via delta = 1e-12 (`HelmholtzEOSMixtureBackend.cpp:1683-1693`) |

## 4. Data and configuration inputs

- **Inputs to this analysis (re-runnable):** `.beads/issues.jsonl` (JSONL; `_type` issue/memory; fields `id,title,description,status,priority,issue_type,close_reason,notes`); `gh issue list -R CoolProp/CoolProp --state all --json number,title,labels,createdAt`; `gh search issues --sort comments`; `git log --follow`, `git blame --ignore-revs-file .git-blame-ignore-revs`; lizard (`--csv`, `-Eduplicate`).
- **Configuration that drives rot:** 38 process-global keys (`include/CoolProp/detail/configuration_keys.h:10-99`), read at 141 non-test `get_config_*` sites; `COOLPROP_<KEY>` env overrides and ad-hoc `getenv` (e.g. `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY`, `src/Backends/Helmholtz/Fluids/FluidLibrary.cpp:47`). Algorithm choice (`MIXTURE_STABILITY_ALGORITHM`), numerics (`ENABLE_SUPERANCILLARIES`, `NORMALIZE_GAS_CONSTANTS`, `configuration_keys.h:11`) and validation (`DONT_CHECK_PROPERTY_LIMITS`) are all global.
- **Data that drives rot:** 136 fluid JSONs (137 on master), 888 binary pairs (`dev/mixtures/mixture_binary_pairs.json`; no ammonia-water row), incompressible JSON corpus (regenerated post-v8, 01342963), superancillary blocks stamped with `source_eos_hash` and fitted out-of-tree (fastchebpure); embedded as CBOR by `dev/generate_headers.py` (501 lines).

## 5. State, caching, globals, thread-safety, memory

Thread-safety history (v5 to master):

| When | Event | Evidence |
|---|---|---|
| 2014 (v5.0.x) | `thread_local` broken on OSX, portability attempts | changelog #501, #502 |
| 2026-04 | Fluid-library cold-start race fixed with `call_once` | #2787 / #2800; `src/Backends/Helmholtz/Fluids/FluidLibrary.cpp:40-44` |
| 2026-04 | HumidAir per-thread Water/Air/IF97 backends | #2831; `src/HumidAirProp.cpp:51-53` |
| 2026-05 | `deriv_counter` made atomic; ideal-container TSan report judged false positive | issue #2844, PR #2855; `HelmholtzEOSMixtureBackend.cpp:47`; agent-notes |
| 2026-06 | Error strings made `thread_local` (#3146), then reverted to a mutex-guarded global because Mathcad/Excel read errors from another thread (#3211) | `src/CoolProp.cpp:64-86`: "Two threads racing UNRELATED failures still clobber one another last-writer-wins" |
| post-v8 | Configuration + 5 DataStructures lookup tables under `call_once`: TSan found 83 races and a SEGV on concurrent first use | c17783e6 (#3422) |
| post-v8 | TabularDataLibrary map and table builds locked | 66ecd326 (#3423) |
| post-v8 | All REFPROP access behind one process-wide recursive mutex (2 threads on different fluids aborted the process) | 8214f28e (#3424); per-instance loading epic #3233 open |

Still structural at v8.0.0 (and mostly on master):
- **Contract is "one AbstractState per thread"** (agent-notes): sharing a backend races by design because every getter may write the cache.
- **Global mutable model data:** `static JSONFluidLibrary library` (`src/Backends/Helmholtz/Fluids/FluidLibrary.cpp:28`); reference-state setters mutate it unsynchronized (01 §6 R14, 09 §6 R4); BIP library insert-on-lookup and load-order poisoning (04 §6 #14-15).
- **Algorithm selection by global config:** even the test for the legacy flash must flip global config under an RAII guard (`VLERoutines.cpp:3238-3249`).
- **WASM:** `#define thread_local` (empty) under `__EMSCRIPTEN__` (`include/CoolProp/detail/tools.h:22-24`): per-thread HumidAir state becomes process-global in a threaded WASM build (inference).
- **Memory:** every instance deep-copies `CoolPropFluid` models (`HelmholtzEOSMixtureBackend.cpp:114`) plus SatL/SatV children (`:141-150`), and lazily TPD/critical/transient copies (`.h:68-100`); measured 87-135 KiB per state (09 §6 R5; verifier's [oracle] re-measurement: about 84 KiB for Propane and 147 KiB for Water per HEOS `AbstractState`). `PropsSI` builds a new backend per call (`src/CoolProp.cpp:254-272`; about 4x overhead, 01 §6 R8; verifier, from Python: `PropsSI("P","T",300,"D",1,"Propane")` 49 us vs a reused `AbstractState` update + `p()` 1.2 us, so the ratio seen by Python callers is far above 4x). The whole library is decoded eagerly from CBOR on first use (2.0 s, +67 MiB, 09 §6 R1; verifier: `import CoolProp.CoolProp` takes 1.8 s and +75 MiB RSS, and the first `AbstractState` is then free). The EOS containers hold an unkeyed derivative cache inside model data (`include/CoolProp/fluids/Helmholtz.h:747-763`).
- **Recompute instead of memoize:** mixture critical points re-traced per accessor (#3445: 1.45 s each for R449A); UNIFAC tables rebuilt per call because the T-cache guard is commented out (`src/Backends/Cubics/UNIFAC.cpp:135-140`; VTPR 270x slower than HEOS at N = 6, #3275); ECS reference backend rebuilt per transport call until #3141 (bd:a7je); `update()` overhead is 22-44% of a single DmolarT call (#2718).

## 6. Rot and bugs

### 6.1 Rot register (systemic)

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| R1 | Model, state, cache and solver workspace fused in one mutable object; satellites write internals | 6 friend classes (`HelmholtzEOSMixtureBackend.h:164-175`); FlashRoutines writes `HEOS._T/_p/_rhomolar/_Q/_phase` about 170 times (168-183 depending on the regex), calls `specify_phase` 45 times; 872 `SatL`/`SatV` references across Flash/VLE/HEOS | Non-transactional updates, phase-imposition leaks (bd:1tbe.8 #1, aeea05e4/#3243, bd:u9gn), poisoned states after failure (bd:i9t8), manual save/restore of `_T`/`_p` around a fallback (`FlashRoutines.cpp:990-998`) | `Arc<Model>` immutable; `State` a small value; flash = pure `fn(&Model, Inputs, &Opts) -> Result<State>`; scratch in a per-call/per-thread `Workspace` |
| R2 | God interface and giant functions | §2.2: 181 virtuals, 136 `NotImplementedError` mentions in the header; `HSU_D_flash` CCN 159; 58 functions CCN >= 30 | Capability discovered by catching; unreviewable control flow | Capability traits (`Thermo`, `Transport`, `Saturation`, `MixtureVle`, ...); one module per input-pair family |
| R3 | Exceptions as control flow; swallowed failures; sentinels | 237 non-test `catch (...)` (55 in FlashRoutines, 59 in CoolPropLib); dead handler in an empty `try {}` (`FlashRoutines.cpp:969-974`); `add_many` prints and continues (`src/Backends/Helmholtz/Fluids/FluidLibrary.cpp:57-61`, fixed post-v8 in 51e8c60f); `_HUGE` on 313 non-test lines (447 occurrences); GERG acentric left at `_HUGE` broke every GERG mixture VLE (bd:deut) | Silent wrong answers, opaque errors (format-arity bugs turned ValueError into fmt errors, bd:b7p8) | `Result<_, FlashError>`; `Option` for missing model constants; explicit fallback chain that records which strategy ran |
| R4 | Process-global mutable state and config | §5; 141 config reads; error outbox `src/CoolProp.cpp:64-86` | Cross-thread interference; results depend on unrelated callers; R134a Tc moves 1.97 mK with `ENABLE_SUPERANCILLARIES` (04 §6 #28) | No globals; `Options` passed by value; registry immutable after `OnceLock` init; per-call error values |
| R5 | Hand-written derivative code per term, duplicated | Each term hand-codes 20 outputs (`include/CoolProp/fluids/Helmholtz.h:40-60`; 20 `all()` bodies / 780 NLOC by lizard); a second multicomplex implementation for 7 term types, used only for checking and compiled only under `ENABLE_CATCH` (`one_mcx`, `src/Helmholtz.cpp:295,560,720,778,1067,1292,1344`; base stub `Helmholtz.h:302-306`); 15 scalar accessors run all 20 and return one (`Helmholtz.h:175-308`); MixtureDerivatives 28% clone lines with twin XN_DEPENDENT/INDEPENDENT branches (7 wrong derivatives, COO-15; bd:rgxn, 5ys2, 9trb, rpsm, zb5u); CP0PolyT 4th derivative wrong (10 §6) | Transcription bugs, latent wrong high-order derivatives | One generic term implementation over a number trait (f64, dual/Taylor jets, SIMD lanes); hand kernels only as side-by-side fast paths tested against it |
| R6 | Copy-paste duplication generally | `lizard -Eduplicate` clone rate 18.1% on core sources (the clone percentages in this cell were not re-run by the verifier); unique clone-line share: PC-SAFT 46%, IncompressibleFluid 45%, CoolPropLib 33%, MixtureDerivatives 28%, REFPROP 23%, Tabular 22%; 5 Rachford-Rice implementations (04 §6 #20); 4 fits of one saturation curve (08 §6 R23); reference-state code x3 (10 §6) | Fixes land in one copy only | Shared generic solvers; one saturation-curve abstraction |
| R7 | Scattered and inconsistent constants | 9 R literal spellings, 8 distinct values, in non-test src/include (8.314472 x10, 8.3144598 x6, 8.314510 x5 plus 8.31451 x2, 8.314371 x4, ...), e.g. `HelmholtzEOSMixtureBackend.cpp:496,517,2830`, `VLERoutines.cpp:1172,2654`, `src/Backends/Helmholtz/Fluids/FluidLibrary.h:1257`; name-keyed fudge `f = 1.00001` for Water/CO2, uncommented (`src/Backends/Helmholtz/Fluids/FluidLibrary.cpp:91`); `NORMALIZE_GAS_CONSTANTS` rescales p, alpha_ig, cv, w by about 1.1e-6 for mixtures (GERG spec) | Sixth-digit inconsistencies no structural test catches | R is per-model data; one `constants` module; no name-keyed special cases |
| R8 | Approximations presented as exact | Virials by delta = 1e-12 (`HelmholtzEOSMixtureBackend.cpp:1683-1693`; [oracle] Cvirial off by -6.8e-5 Propane, -7.1e-5 N2, +1.9e-5 Water); p(rho = 0) = `0*Inf` (virial-axy.md §1a; #1676 open since 2018); mixture eta = exp(sum x ln eta_i) and lambda = sum x lambda_i behind a warning string (`:816`, `:1060`) | Plausible numbers without accuracy | `A_xy` derivative representation; exact zero-density Taylor coefficients; "estimate" results typed or refused |
| R9 | Input validation scattered and late | Q/NaN guards missing on Cubic/SVDSBTL/INCOMP/PhaseEnvelope (bd:jvkr); NaN Q segfault (bd:uedc); Tmin not enforced (#3394, open); 5 post-v8 commits centralise it (aa7c6079, 15ede127, 0f978943, 4370e448, ff39b559) | Garbage in, garbage out; state cleared before the check | Validated newtypes at the boundary (`Finite`, `Quality`, `Temperature`); model validity domain; explicit `Extrapolate` opt-in |
| R10 | Data-provenance drift | Fossil 4.2 `STATES.critical` h/s in 57 fluids (bd:yr5d, worst R1336mzz(Z) 72%); reducing densities rebuilt through unit round-trips (2acbbc82); TRANSPORT block silently dropped in an EOS swap (#2768, fixed 14da1f0d); EOS vs superancillary hash coupling (`source_eos_hash` in 130 fluid JSONs; contract described at `dev/scripts/check_superanc_release_pin.py:19-27`; 66859efb); stale derived cubic listing (bd:y2ot); stale CBOR within a build (bd:my86); placeholder citations (520b8809); unverifiable ECS entries (767b1b3b) | The oracle itself drifts from literature; silent regressions | Build-time generation with schema checks; content hashes bind EOS to derived artifacts; mandatory DOI + check values per block; derived data regenerated in CI, never hand-edited |
| R11 | Transcription defects, in CoolProp and in the papers themselves | CoolProp-side: last-digit and ancillary typos (e6ba7dbe water, closes #2053; 3b510388 n-hexane rhoV ancillary); SF6 conductivity typo inherited from the Assael paper (3f449ab8, 2014); ECS `q_D` ignored (05 §6 R2). Paper-side (verifier correction: these are errors in the printed equations, NOT CoolProp bugs, and they concern correlations added after v8.0.0): R-161 Eq. 8 denominator sign, ethanol (IJT 2023) Eq. 12 `rho_r` vs `rho_r^2`, krypton Eq. 13 missing `exp` (f854d34b, 9415d9e8), xenon Eq. 6 implemented "as corrected" (f854d34b). They were caught only by evaluating the paper's own check values, and CoolProp ships the REFPROP-FLD form that reproduces those tables. The ethanol 2023 pole (bd:z94e) was in a correlation withdrawn before merge, so it never shipped | Wrong transport properties; a printed equation is not a reliable arbiter on its own | Every correlation ships with its paper's verification table as a test; record which printed equations are known to be wrong; Crossref `updated-by` erratum check |
| R12 | Hidden cross-model coupling | Changing R134a viscosity moved 7 ECS refrigerants by 1-2.6%; conductivity terms divide by or read viscosity (agent-notes; 05 §6 R6) | Edits have non-local effects | Explicit dependency graph in the registry; impact tests on change |
| R13 | Open/closed violations | New transport form = 4 core edits: struct+enum, parser, dispatch switch, routine (transport DSL spec, "Problem"); alternate EOS parsed but unreachable (`include/CoolProp/CoolPropFluid.h:540,543`; 23 fluids); backend registry special cases (01 §6 R24) | Friction for every new model | Data-driven correlation enum/expression; explicit model selection; registry of constructors |
| R14 | Dead code and legacy paths | `saturation_critical` never called, prints via `std::cout` (`VLERoutines.cpp:15-30`, `.h:152`); commented `allEigen` (`Helmholtz.cpp:37-137`); UNIFAC cache commented out (`UNIFAC.cpp:135-140`); legacy Gernert PT path; XN_DEPENDENT "dead but wrong" (bd:9trb); 54 `TEST_CASE`s inside 20 production files, behind `#if defined(ENABLE_CATCH)` (e.g. `VLERoutines.cpp:3189-3192`); 58 `throw()` specifications; 29 flat include shims "remove at v9"; wrappers untouched since 2015-22 (Android, Delphi, Julia, SMath, Fortran, Labview) | Maintenance drag, misleading code | Do not port; keep a "not ported" list (§9) |
| R15 | API legacy | 73 C exports (`EXPORT_CODE` declarations; a raw `grep -c EXPORT_CODE` gives 81 because it also counts 8 macro-definition lines) incl. kSI `Props`/`PropsS`/`Props1`, kSI `HAProps`, `F2K`/`K2F`, `redirect_stdout` (`include/CoolProp/CoolPropLib.h:225-299, 846-854`); enum values renumbered between releases (01 §6 R1); enum numbering hand-copied in 4 places (COO-37); global errstring outbox; per-call factory in PropsSI | ABI breaks, races, overhead | Explicit stable discriminants; `#[non_exhaustive]`; small cbindgen C ABI with per-call error buffers; string API as a thin parser over the typed API |
| R16 | Build and packaging complexity | `CMakeLists.txt` 2,607 lines, 23 options, 101 `COOLPROP_*` names; 12 `CPMAddPackage` calls fetched at configure, 9 unconditional plus Catch2, ExcelAddinInstaller and FindMathematica (`cmake/dependencies.cmake:18-131`); 22 workflows; a JSON symbol-visibility saga (RapidJSON ODR/version clashes, then a hidden-visibility pragma poisoned `__assert_fail`; the 14 specs/plans dated 2026-06-04..07 cover the RapidJSON-to-nlohmann migration and visibility); 21 wrapper directories | A large share of user issues (about 23% of titles, §6.5) and of recent churn (`.github`, `wrappers`, `CMakeLists.txt` are among the top recent-era paths, §2.3) concern building and packaging. Verifier downgrade: the earlier claim of "most commits and issues" is not supported | Cargo workspace; few deps; no header ABI; Cargo features for fluid subsets; generated bindings |
| R17 | Fail-open verification | Preflight ran zero tests (bd:8yrc); pipefail inverted gates (bd:onfy); malformed clang-tidy regex (bd:hq3z); REFPROP tests silently skip (bd:mriu); bit-exact tests break on arm64 FMA (bd:pb3j, abza); order-dependent tests (bd:dag6); suite red for years before v7.0.0 | False confidence | Zero tests run = failure; tolerance-based numeric asserts; randomized order; aarch64 + wasm in CI |
| R18 | Host FP and locale hazards | Delphi/Excel unmask FP exceptions, so NaN/`_HUGE` trap (#3012; `include/CoolProp/FPUGuard.h:29-72`, applied at `src/CoolPropLib.cpp:58-64`); under `setlocale(LC_NUMERIC, "de_DE")`, `string2double` threw on every decimal in HMX.BNC text, `MEG-20.5%` failed and `COOLPROP_*` env doubles silently read 0.25 as 0; separately the lax concentration parser turned a malformed `MEG-abc%` into 0 (b945eb4f, post-v8) | Host-specific crashes and wrong inputs | Rust parsing is locale-free and strict; FP-environment guard only in the C-ABI shim on Windows |
| R19 | Mixture flash fragility | `solver_rho_Tp` called standalone with no guess fails on 25-65% of binary-mixture (T,p) cells where the full PT flash succeeds; the flash survives only because `update_TP` falls through to the bracketed `solver_rho_Tp_global` (agent-notes); Michelsen unconverged splits (bd:1tbe.22); P0 knife-edge HSU_P where a 1.3e-7 change in N2 rho_c flips the result by 1.01 K (bd:ft05); spurious middle root (#3283, fixed b17661f6); interior-Q PQ solved as bubble/dew (b63ea865); trivial QT solution near critical (#3346); unstable-branch roots (#3448); DP/DQ/HQ/QS not implemented for mixtures (`FlashRoutines.cpp:500,568,606,639`) | Silent wrong states for mixtures | Enumerate all density roots on a bracket (Chebyshev or bracketed TOMS748) and select by Gibbs energy with stability postconditions; convergence gates are part of each solver's contract |

### 6.2 Recurring bug classes (counts: beads bugs n=198 | GitHub titles n=2,221 | fix-like commits to v8.0.0 n=1,332; keyword-classified, overlapping)

Verifier note: the per-class keyword counts below were not re-run, because the keyword lists were not recorded. The commit total is regex-dependent: 1,324-1,378 non-merge commits match on the subject line and 1,614 match on the whole message, so treat 1,332 as approximate. Per-file commit counts use the subject-line definition (§2.3).

| Class | beads / GH / commits | Representative evidence | Killed by construction? |
|---|---|---|---|
| Pure-fluid flash failures, incl. near-critical and two-phase edges | 62 / 179 flash + 124 sat + 59 crit / 42 + 48 + 31 | devdocs consistency report (7.2.1dev): 12,355 failures over 82 fluids, 71% exceptions, about 4,860 PHSU no-bracket, about 2,691 melting-line out-of-range (bd:r1w7); Air near-critical D+X, 108 failures (bd:br8q); #2773 multi-root saturation inputs | No: consistency sweeps + bracketed solvers |
| Mixture flash / VLE / stability | 35 / 252 / 51 | bd:ft05, 1tbe.22, #3283, #3346, #3448, #3171; PT-flash regression (bd:d08) | No: algorithm redesign + regression corpus |
| Input validation, NaN, sentinels, UB | 46 / 80 / 35 + 29 segfault | bd:uedc, jvkr, deut; IF97 NaN gives 273.15 K (4370e448); empty-list UB (538d4227); strcpy overflow (bd:tw7t); uninitialised `pseudo_pure` (bd:9h53) | Mostly yes: newtypes, `Option`, bounds checks, mandatory init |
| Transport correlation transcription and data | 11 / 87 / 20 + data typos | §6.1 R11; R1233zd(E) viscosity lost in v8.0.0 | No: paper check-value tests |
| Data provenance drift | 24 / n.a. / 424 commits in `dev/fluids` | §6.1 R10 | Partly: hash-bound generated data |
| Thread safety / global state | 14 / 26 / 7 | §5 | Yes: `Send`/`Sync`, no globals |
| Tabular (TTSE/BICUBIC/SVDSBTL) | 26 / 110 / 50 | #1301 BICUBIC near saturation (open since 2016); raw-pointer `erase` (08 §6 R6); index off-by-one (COO-19) | Partly: memory bugs yes, accuracy no |
| Wrapper / binding parity, packaging | 37 (+53 CI) / 518 / 323 | 12 bugs in the nanobind migration epic (bd:r9sq), 5 closed by one parity PR (#3120); C# CI OOM (bd:mo0y); Java artifact shipped without sources (bd:qrn3) | Mostly design: one API source, generated bindings |
| CI gates fail-open | 53 (beads only) | §6.1 R17 | Process: fail-closed tests |
| Platform, FP, locale | 8 / incl. above / 39 Windows + 29 macOS/ARM | #3012, b945eb4f, bd:anye (arm64 tolerance) | Locale yes; FP env needs a C-ABI guard |
| REFPROP backend | 15 / 184 / 63 | Not reentrant; upstream wrong answers (bd:51nk, tmvw) | Out of scope (drop) |
| Humid air / incompressible / IF97 | 16 / 101 + 109 + 70 / 40 + 27 | `kT` passes density as pressure (07 §6 H1; confirmed at `src/HumidAirProp.cpp:2344-2345`: `Water->update(PT_INPUTS, WaterIF97->rhomass(), T)`); PCL viscosity 100x (25c4f3a5); IF97 Region 3 (07 §6 R3) | No: tests; model redesign |
| Cubic / PC-SAFT / VTPR | 12 / 95 / 18 by subject, 31 by whole message (`CubicBackend.cpp` alone) | PR/SRK entropy chain-rule error (9b96b64b); Q = 5 accepted; VTPR O(N^2) recompute (#3275) | No: thermodynamic-identity tests |

### 6.3 Oracle errata: CoolProp 8.0.0 values known to be wrong or superseded

| Item | v8.0.0 behaviour | Correct / status | Evidence |
|---|---|---|---|
| Reducing densities: Nitrogen, Ethylene (+M), OrthoHydrogen (+M), n-Undecane | [oracle] N2 `rhomolar_reducing` = 11183.901464580624 | 11183.9 (Span et al. JPCRD 2000); fixed 2acbbc82, superancillaries refit 66859efb | commit messages |
| R1233zd(E) viscosity | [oracle] raises "Viscosity model is not available" | v7.2.0 had rhosr-CS; restored and refit 14da1f0d | #3330 |
| Two-phase eta/lambda | [oracle] Water 500 K, Q = 0.5: eta = 1.6048e-5 | Now throws (da48a3ee) | #3446 |
| PR/SRK entropy | [oracle] PR propane 400 K, 1 bar: T(ds/dT)p = 91.35 vs cp = 93.89 J/mol/K | Tc/Tr chain-rule restored 9b96b64b | #3287 |
| Cubic quality range | [oracle] SRK Propane QT with Q = 5 accepted (rho = 4.51 kg/m3) | Refused (aa7c6079) | commit message |
| Below Tmin | [oracle] Water `DmolarT_INPUTS` (55018.5 mol/m3, 250 K) gives p = -5.928 Pa without error; `Tmin()` = 273.16 | Open on master | #3394 |
| Virials | [oracle] Cvirial relative error -6.8e-5 / -7.1e-5 / +1.9e-5 (Propane 300 K / N2 300 K / Water 600 K). Verifier reproduced it (-6.7e-5 / -7.1e-5 / +1.9e-5) against a cubic extrapolation of `d2alphar_dDelta2` to delta = 0; Bvirial agrees to about 1e-12, so only C (second derivative at delta = 1e-12) is affected | Exact path not merged (COO-8) | §6.1 R8 |
| Mixture PT spurious middle root; interior-Q PQ/QT | Wrong state | b17661f6, b63ea865 | #3283, #3372 |
| Mixture near-dew HSU_P | Up to 2.97 K wrong T, half silently | Open (COO-5) | #3342 |
| `fugacity_coefficient` at Q = 0/1; 3rd-order mixture alpha0 | Throws | 552b4711, 1fca2b92 | #3258 |
| `STATES.critical` h/s (57 fluids) | Fossil 4.2 values | Open (COO-21) | bd:yr5d |
| IF97 NaN inputs; incompressible PCL viscosity | T = 273.15 K; 100x high | 4370e448; 25c4f3a5 | commit messages |
| Transport supersessions | Older correlations | REFPROP 10.1 audit: 15 viscosity + 6 conductivity supersessions, 12 conductivity gaps; many landed post-v8 (#3399-#3406, 93a41972) | master `docs/superpowers/plans/2026-08-29-*` |

### 6.4 Maintainers' stated direction and pain points

Direction (evidence):
- **REFPROP parity, evidence-first** (gaps spec): 13 gaps; triage kept COO-11 mixture transport (largest predictive gap: `TransportRoutines.cpp` throws "only for pure and pseudo-pure" in 15 places), COO-26 ammonia-water (`factory("HEOS","Ammonia&Water")` fails at construction, `MixtureParameters.cpp:349`; open since #341, 2014), COO-27 mixture DP/DQ/HQ/QS. Dropped as wishlist: mixture surface tension (`HelmholtzEOSMixtureBackend.cpp:711`), sublimation lines, dielectric constant, runtime model switching, BIP estimation (fail-loud default deliberately kept), choked flow, AGA8, heating values, 4th virial. Rejected claims: VLLE is not a REFPROP gap; SLE is not either.
- **Transport as data:** expression DSL (#3185, extended #3333/#3334/#3352), aiming to make a new parametric form "data, not code"; supersession audits against REFPROP 10.1 using open-access papers.
- **Strict models:** GERG-2004/2008 backends that "refuse to answer questions the model does not cover", with teqp test values as acceptance (gerg-strict spec; c7d6a1aa).
- **Provenance from source:** ATcT values regenerated by script, so "no number rests on a hand-curated spreadsheet"; literature over REFPROP as arbiter (2acbbc82: "confirmed against the original publications, not merely against REFPROP").
- **Speed:** superancillaries (v7), SVDSBTL and `fast_evaluate` (v8), SIMD post-v8 (#3046), extend `fast_evaluate` to HEOS (#3008), per-instance REFPROP (#3233).
- **Modernisation:** nanobind abi3; retire Cython and Python 2 (COO-30); remove flat headers at v9; CMake package (d9debbb5), FHS/OBS packaging (492753b7), Pyodide wasm32 wheels (4525f062); C99-clean C ABI headers (585270a5).
- **New physics in flight:** Bollengier Gibbs-explicit liquid water (6d788a7a), tensor B-splines (8a5e34fe), D2O IAPWS transport (93a41972), Beckmuller H2 binaries (3c15af82), Tkaczuk cryogenic mixtures (#3118).

Pain points that survived triage (Linear `COO-n`): wrong mixture HSU_P T (5), DmassT density mismatch and near-critical PQ (6), quality guards (7), exact virials (8), thread safety of config/tabular/REFPROP (13), CI gates (14), XN_DEPENDENT derivatives (15), GERG edge cases (16), D+{H,S,U} consistency (17), SVDSBTL error-path contract (18), tabular index bugs (19), fossil data (21), PC-SAFT load swallows errors (22), stale CBOR (23), `set_reference_stateS` no-op (24), transport recompute (25), enum numbering copied 4x (37).

Deliberately not pursued in C++ (2026-09-26 triage), so open for the Rust design: always-bracketed TOMS748 density solve over superancillary/melting bounds (bd:uqvr, "symptoms were patched instead"); Bell-Alpert Chebyshev mixture density rootfinder (bd:zkc, r5h; prototype 17 us/call methane, never merged); public diagnostics (bd:06j); error-returning PropsSI to retire the global slot (bd:z8wk); worker-pool batch API (bd:y5d); continuation tracer for phase envelopes (bd:2ta4, PR #3378).

Lessons recorded as "do not" (agent-notes and beads memories; the SLEEF item comes from #3046): P+X is a 1-D problem, and a 2-D homotopy regressed it and was reverted (bd:761); fast 2-D (h,s) inversion belongs to tabular methods; accept an HS root only if dp/drho|T > 0 and cv > 0; generate flash test points in (p,T), never density bands; never differentiate a fitted Chebyshev series when the derivative has a closed form; never diagnose an equation from PDF text extraction; do not use SLEEF u10 exp in VLE paths.

### 6.5 What users ask for most

| Demand | Evidence |
|---|---|
| Install / build / wrappers / platforms | about 518 of 2,221 titles (23%); Excel 54, MATLAB 56, install/pip 45, JS/WASM 15, Julia 14; most-commented after #1482 (cubic EOS, 64 comments): #1867 Raspberry Pi pip (59), #946 ACADO (52), #2119 Python bindings help (51), #1293 Julia package (49), #596/#246 macOS (43/41); 10 of the top 12 are build, install, wrapper or wrapper-performance issues (the others: #1482, #715 adding fluids) |
| Mixtures (more pairs, reliable flash, transport) | about 252 titles; open: ammonia-water #341 (2014), #1342, #1373, #1677 zero mole fractions, #2637, #2737 HELD, #2757, #3342, #3346, #3448; FAQ Q4 missing BIPs |
| Transport coverage / correctness | 87 titles; 6 open `new transport` issues (#1826, #1851, #2180, #2229, #2383, #2523) |
| Tabular reliability | 110 titles; 7 open `tabular backend` (#935, #936 TTSE DP with phase spec fails 90%+, #1301, #2247, #2266, #2318, #1830) |
| Performance | #411 (26 comments), #447, #538, #2199, #2718, #3008, #3046, #3445, #3275 |
| Reference-state confusion | FAQ Q1; 18 titles |
| New fluids / EOS | #715 (45 comments), #2675 CO2 2026 EOS, #3277 EOS-CG, #1360 formation enthalpy (2016, partly done post-v8) |
| Humid air and incompressibles | 101 and 109 titles; FAQ Q6 incompressible mixing flaw; #3302 24 fluids ship all-zero conversion fits |

Issue inflow per year: 2014 353, 2015 478, 2016 381, 2017-2024 61-170, 2025 106, 2026 148 (to October).

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

- **What blocked parallelism historically was state, not math:** globals (§5), per-instance deep copies, REFPROP's process-global library, and the "one AbstractState per thread" contract. Immutable shared models make thread-level parallelism over many requests on the same or different fluids nearly free (inference, design goal).
- **Measured single-call budget (#2718, Propane):** update + p() costs 367-516 ns, of which the residual Helmholtz evaluation is 282-284 ns; overhead is 22-44%. SIMD in the EOS kernel can only speed up the EOS share, so lean state construction comes first.
- **SIMD evidence (#3046, closed PR #3044):** the hottest loop is `ResidualHelmholtzGeneralizedExponential::all` (about 384 ns/call average over 24 fluids, 814 ns for Water with 54 terms). Per-term cost: about 30% in two `exp`, 60% in the multiply-add chain. NEON 2-wide gave 1.33x per call and 1.17x end to end. Estimated AVX2 4-wide 1.6-1.8x, AVX-512 2.0-2.2x with scalar `exp` and SIMD FMA chains. Prerequisite: sort terms into homogeneous chunks at model build (branch-free lanes). Constraint: VLE near the triple point needs 1-2 ULP `exp`, so the maintainer judged SLEEF u10 unacceptable unless it passes a precision stress test (branch `ihb/vle-triple-stress`). #3046 reports that Methane and CO2 pass while Propane and Methanol do not, and recommends scalar `exp` with SIMD FMA chains.
- **Side-by-side kernels must not be compared bit-exactly:** the delta-only vs full evaluation differ in low bits through FMA contraction (bd:pb3j, abza). Rust does not contract `a*b+c` implicitly, which helps a deterministic scalar reference; platform `libm` still differs, so pin a pure-Rust `libm` for the reference kernel if cross-platform bit stability matters (inference).
- **Data-parallel (good fit):** residual and ideal term sums per state; batch evaluation of many states of one fluid in SoA layout (the `fast_evaluate` idea generalised to HEOS, #3008); tabular/SVDSBTL interpolation; table and superancillary builds (SVDSBTL per-cell sampling threads, targeted 4-8x, bd:43h closed; `SVDSBTL_SAMPLING_THREADS`, `Web/coolprop/SVDSBTL.rst:434`); consistency sweeps and oracle comparisons (embarrassingly parallel).
- **Branchy / sequential (poor fit):** flash cascades (55 `catch (...)` fallbacks in FlashRoutines), phase determination (CCN 82-83), Michelsen stability/split, phase-envelope tracing, mixture critical-point tracing. Parallelise across requests, not inside. Lockstep SIMD Newton over uniform-phase batches is possible but speculative (inference).
- **Mixtures:** O(N^2) composition Jacobians and per-pair departure terms (evaluated twice, 04 §6 #16) suit term- or pair-level vectorisation once caches are keyed correctly; VTPR shows the cost of recomputation (#3275).
- **Recommended shape for side-by-side implementations (design proposal, not CoolProp evidence):** (1) at model build, compile each EOS into term-family chunks in SoA form (the "term sorting" prerequisite of #3046); (2) one kernel trait such as `HelmholtzKernel::eval(&self, tau, delta) -> Jet` with a scalar reference implementation that is generic over the number type, plus optional `x86_64-v3`, `aarch64-neon` and `wasm32-simd128` implementations selected at run time; (3) a batch entry point over many `(tau, delta)` states of one model (state-parallel lanes), which is the better SIMD target than term-parallel lanes because residual EOSs are short (12-56 terms, median 16, counted over `dev/fluids/*.json` EOS[0]); (4) every fast kernel is property-tested against the reference within a stated ULP budget, and solver paths that need 1-2 ULP `exp` (VLE near the triple point) keep a scalar `exp`; (5) branchy algorithms (flash, stability, tracers) stay scalar per state and are parallelised across requests with a thread pool.

## 8. Verification assets

- **Oracle errata registry (to build):** §6.3, each item with the literature value, CoolProp version, commit and tolerance policy.
- **Consistency sweep:** `wrappers/Python/CoolProp/Plots/ConsistencyPlots.py` (669 lines) + `_consistency_report.py` (181) + `Web/scripts/fluid_properties.Consistency.py`: lists 15 input pairs per fluid (`all_solvers`, `ConsistencyPlots.py:16`), of which 4 are marked not implemented (`:17`), so 11 are round-tripped, on 40x40 single-phase and 20x20 two-phase grids and writes per-point failure CSVs; the devdocs report (12,355 failures over 82 fluids at 7.2.1dev) is a baseline to beat.
- **Regression corpus:** 198 beads bugs with reproducers (e.g. bd:ft05 gives the 5-component mixture, z, p, T); open GitHub reproducers (#3342, #3346, #3394, #3445, #3448, #3171, #3275); `dev/Tickets/` (27 scripts); changelog issue-to-fix lists back to 5.0.0.
- **Baselines with numbers (beads memory "mixture-density-baseline", 60x60 PT grids):** standalone `solver_rho_Tp` failure rates of 25-65% for three binaries (nC10/CH4 25%, CO2/N2 49%, CH4/H2S 65%) and 39% for the 10-component "amarillo" AGA-8 gas; PT-flash p95 times of 8-44 ms for the binaries and 155 ms for amarillo (the worst, not CO2/N2 as previously stated); phase-envelope tracer failure cases (agent-notes); HS cascade validation (130 fluids, 191,492 points, 0 regressions; beads memory).
- **Literature anchors:** transport paper verification tables (the recorded way typeset errors were caught); GERG teqp test values; ATcT scraper; IAPWS releases (G13-15 SBTL, R17-20, R18-21); `virial-axy.md` sympy derivations with a reproduction script (`dev/derivations/virial_axy_derivations.py`).
- **Precision/perf harnesses:** on branches only (#3046): `CoolProp-Tests-HelmholtzInnerBench.cpp`, `CoolProp-Tests-VLETripleStress.cpp`; in the v8.0.0 tree: `src/Tests/CoolProp-Tests-TermCacheProfile.cpp` (per-fluid term-structure statistics, useful for SIMD chunking).
- **Thermodynamic identity checks** (catch whole classes the oracle cannot): T(ds/dT)p = cp (caught the cubic entropy bug), Maxwell relations, Gibbs-Duhem for mixtures, FD vs analytic derivatives (MixtureDerivatives tests found COO-15).

## 9. Port recommendation (units with priority; order; what to redesign)

| Unit | CoolProp paths | Priority | Rationale |
|---|---|---|---|
| Oracle harness + errata registry | Python oracle; §6.3 | P0-core | TDD against a known-imperfect oracle needs per-item overrides with literature values from day 1 |
| Consistency-sweep harness | `ConsistencyPlots.py`, `_consistency_report.py` | P0-core | Round-trip every supported input pair per fluid; fail closed; the CoolProp failure CSVs are the baseline |
| Typed error model + validated inputs | `src/CoolProp.cpp:60-110`, `include/CoolProp/Exceptions.h`, master `AbstractState::check_input_quality` | P0-core | Kills R3, R9 and the errstring race by construction |
| Explicit options instead of global config | `include/CoolProp/detail/configuration_keys.h`, `src/Configuration.cpp` | P0-core | 141 hot-path global reads; algorithm selection must be a parameter |
| Immutable model / value state / workspace split | `HelmholtzEOSMixtureBackend.*`, `FluidLibrary.*` | P0-core | Root of R1, R4 and memory rot; enables lazy per-fluid loading and parallel requests |
| Generic-number term evaluation (`A_xy`) | `src/Helmholtz.cpp`, `include/CoolProp/fluids/Helmholtz.h` | P0-core | Removes R5; zero-density-safe; one implementation feeds AD, SIMD and checks |
| Flash strategy chains with diagnostics | `FlashRoutines.cpp`, `VLERoutines.cpp` | P0-core (design), P1 (mixtures) | Replace `catch (...)` cascades and `specify_phase` mutation with explicit, ordered strategies and postconditions |
| Provenance pipeline | `dev/generate_headers.py`, `dev/scripts/check_superanc_*.py`, `inject_*.py` | P1-early | Hash-bound derived data, schema at build time, DOI + check values mandatory |
| Regression corpus from bug history | `.beads/issues.jsonl` (master), `dev/Tickets/`, open GitHub issues | P1-early | Seed tests for each recurring class (§6.2) |
| Concurrency tests | master `[threads]` tests (c17783e6, 66ecd326) | P1-early | Concurrent first use, many fluids at once; loom/miri where applicable |
| Paper check-value tests for transport | `dev/fluids/*.json` TRANSPORT blocks + papers | P1-early | The only proven guard against R11 |
| Bench suite + side-by-side SIMD kernels | #3046 artifacts, `TermCacheProfile` | P2-later | After the scalar reference is verified; ULP-budget comparisons |
| Ammonia-water (Tillner-Roth & Friend) | none (gap) | P2-later | Top user request since 2014; a dedicated model, not a BIP row |
| Mixture transport model | `HelmholtzEOSMixtureBackend.cpp:816,1060` | defer | Multi-month research item; until then return a typed estimate or an error |
| Other REFPROP gaps (sublimation, dielectric, AGA8, HHV, choked flow, D virial) | gaps spec | defer | Triaged as wishlist upstream |
| REFPROP backend, GUI, 18 of 21 wrappers | `src/Backends/REFPROP`, `wrappers/*` | drop | Not reentrant / out of scope; keep C ABI, Python, WASM only |
| Legacy and dead surfaces | kSI API, flat shims, `TTD.txt`, `Web/develop`, Gernert legacy PT path, XN_DEPENDENT branches, `saturation_critical`, empty try, `allEigen`, container caches, 15 scalar accessors, `one_mcx` duplicates, embedded `TEST_CASE`s, `throw()` | drop | §6.1 R14-R15 |

Order: P0 harnesses (errata, consistency) and error/options/model-state decisions first, then the generic term kernel, then pure-fluid flash with strategy chains, then provenance pipeline and regression corpus, then mixtures, then SIMD/batch.

What the Rust design removes by construction versus what still needs tests (assuming the P0 decisions hold):

| Bug class (CoolProp evidence) | Mechanism in Rust | Residual test need |
|---|---|---|
| Init and data races (c17783e6: 83 TSan races; #2787; #3211) | No mutable globals; `OnceLock` registries; `Send`/`Sync` checked by the compiler | Concurrent first-use smoke tests only |
| Stale caches, phase-imposition leaks, non-transactional `update` (bd:1tbe.8, #3243, bd:ek87) | `State` is a value; flash returns `Result<State>` and never mutates its input; caches keyed by inputs | None structural |
| Uninitialised fields, UB on empty containers, buffer overruns (bd:9h53, 538d4227, bd:tw7t) | Mandatory initialisation, bounds checks, no `strcpy` | Fuzz the C-ABI shim |
| Format-arity errors turning into opaque failures (bd:b7p8) | `format!` checked at compile time | None |
| Locale-dependent parsing (b945eb4f) | `str::parse` is locale-free and strict | None |
| NaN / out-of-range inputs, sentinel constants (bd:uedc, jvkr, deut; #3394) | Validated newtypes; `Option` instead of `_HUGE` | Domain-limit property tests |
| Enum ABI renumbering (01 §6 R1; COO-37) | Explicit discriminants, one source of truth, generated headers | ABI snapshot test |
| Header/ODR/symbol leaks (RapidJSON, `__assert_fail`) | Crates; explicit `extern "C"` surface | None |
| "Not implemented" discovered at run time (136 default throws) | Capability traits; unsupported combinations do not compile or are rejected at model build | None |
| Hand-derivative transcription (COO-15; CP0PolyT) | One generic implementation per term over a number trait | AD vs finite-difference vs paper values |
| Flash convergence, root selection, mixture stability (bd:r1w7, ft05, #3448) | Not removable; better structure only | Consistency sweeps, regression corpus, Gibbs/stability postconditions |
| Correlation transcription and data provenance (R10, R11) | Hash-bound generated data | Paper check values; recompute derived data in CI |
| SIMD vs scalar drift; platform `libm` differences (bd:pb3j) | Rust never contracts FMA implicitly | ULP-budget tests across x86_64, aarch64, wasm32 |

## 10. Open questions

1. Oracle policy where 8.0.0 is wrong: test against the literature value only, or also pin a second oracle (a master commit) for fixes that landed after the tag (reducing densities, cubic entropy, transport supersessions)?
2. Which transport generation to target first: v8.0.0 correlations (oracle-matchable) or REFPROP 10.1 supersessions (literature-matchable, 15 viscosity + 6 conductivity)? Proposal: oracle-matchable first, supersessions behind explicit model selection.
3. Adopt CoolProp's post-v8 transport expression DSL format as an input format, or define a typed Rust representation and convert at build time?
4. Expose alternate EOSs (23 fluids ship two) through explicit model selection in v1, or ship only `EOS[0]`?
5. Mixture scope for v1: PT/PQ/TQ with Gibbs-selected roots only, with DP/DQ/HQ/QS and mixture transport explicitly unsupported (typed errors)?
6. Incompressibles: port the known-flawed model (no mixing enthalpy, cp = cv; FAQ Q6, 07 §6 I7) for oracle parity, or redesign as a proper material-property module (this links to the future "materials" goal)?
7. Determinism target: bit-identical results across x86_64/aarch64/wasm32 (requires a pure-Rust libm in the reference path), or ULP-bounded agreement?
8. Should the Rust consistency sweep reproduce CoolProp's grid exactly so that failure counts are directly comparable with the devdocs report?
9. How much weight to give 2026 code paths that are heavily AI-co-authored and recently patched (e.g. SVDSBTL: 27 commits to v8.0.0, all from 2026, 7 with a fix-like subject; most are features stacked in quick succession) when using them as design references, versus going back to the papers?

## Verification log

- Date: 2026-10-04. Adversarial verification against the reference checkout at v8.0.0 (ae81610e), `origin/master`, the beads archives (`.beads/issues.jsonl` at v8.0.0 and on master), `gh` (2,221 issues re-fetched) and the CoolProp==8.0.0 oracle. lizard 1.24.0 was re-run and reproduced 3,300 functions, 58 with CCN >= 30, 39 with NLOC >= 150, and every row of §2.2.
- Claims checked: about 170. That covers every `path:line` citation (about 50), every count that could be re-run (about 45), all 39 cited commit hashes, all 51 cited beads IDs, about 45 GitHub issue numbers, titles, states and dates, and 10 oracle values (N2 reducing density, R1233zd(E) viscosity, two-phase eta, PR entropy, SRK Q = 5, below-Tmin pressure, three Cvirial errors, ammonia-water construction, mixture `fugacity_coefficient` at Q = 0, mixture third-order alpha0).
- No partial edit from an earlier verifier was found: no broken table, duplicated section or earlier "Verification log".
- Corrections:
  1. Wrong path: `FluidLibrary.cpp` / `FluidLibrary.h` are under `src/Backends/Helmholtz/Fluids/`. Fixed in §4, §5, R3 and R7.
  2. §2.3: the fix-like counts had been computed on the whole commit message although the header said "subject". Both definitions are now shown. Subject-based counts are much lower (SVDSBTL 7/27, not 25/27; FlashRoutines 71, not 94). The `AbstractState.h` n/a cells are filled in. The recent-era bullet was recomputed.
  3. §1 headline 3: "86 of 103 Helmholtz commits (83%) fix-like" becomes 56 (54%) by subject (87 by whole message). Open question 9 (SVDSBTL "25 of 27 fix-like") is corrected the same way.
  4. Era table: 2025 commits 330 becomes 332.
  5. §2.1: the GitHub `bug` label count 45 becomes 50. The "12 backend families" claim is now cited to `DataStructures.h:493-508`, including TREND, which has no implementation.
  6. §2.2: SaturationSolvers size now notes the nested-class total (39 fns / 1,900 NLOC).
  7. §3: "#1826 R1233zd(E) 50-70% high" becomes 40-75% off for CoolProp 6.2.1 saturated-liquid viscosity (issue comment), with the post-v8 refit noted.
  8. §5: deriv_counter now cites issue #2844 / PR #2855. Memory and overhead figures carry the verifier's oracle re-measurements: 84-147 KiB per state; 1.8 s and +75 MiB at import; PropsSI vs a reused AbstractState is about 40x from Python.
  9. R1: state-member writes 177 becomes about 170 (168-183 by regex).
  10. R2/R3: "136 default throws" becomes 136 `NotImplementedError` mentions. `catch (...)` 236 becomes 237. `_HUGE` 315 becomes 313 non-test lines (447 occurrences).
  11. R5: `all()` bodies 21/810 becomes 20/780. `one_mcx` exists for 7 term types, not 4, and is compiled only under `ENABLE_CATCH`.
  12. R6: clone-rate figures flagged as not re-run.
  13. R7: R literals are 9 spellings but 8 distinct values.
  14. R10: hash-coupling citation `check_superanc_release_pin.py:61` becomes `:19-27`.
  15. R11 rewritten. The krypton, R-161, ethanol and xenon equation defects are errors in the published papers, caught by check values in post-v8 work. They are not CoolProp transcription bugs, and CoolProp ships the REFPROP-FLD form. The ethanol pole (bd:z94e) was in a correlation withdrawn before merge.
  16. R12: 1.1-2.6% becomes 1-2.6% (agent-notes wording).
  17. R14: the embedded `TEST_CASE`s are behind `ENABLE_CATCH`, and the line citation is adjusted.
  18. R15: C exports 81 becomes 73 (81 had counted macro-definition lines).
  19. R16: CPM packages 10 becomes 12 (9 unconditional). `COOLPROP_*` names 102 becomes 101. The 14 specs/plans are characterised as the nlohmann migration and visibility series. The impact "most commits and issues are about building and packaging" is downgraded as unsupported.
  20. R18: the locale bug is described precisely (b945eb4f): `string2double` threw, env doubles silently read 0.25 as 0, and `MEG-abc%` became 0 through a lax parser independent of locale.
  21. R19 and §8: `solver_rho_Tp` failure rates are for a standalone call with no guess; the full flash survives through `solver_rho_Tp_global`. The worst PT-flash p95 is 155 ms (10-component amarillo), not CO2/N2 44 ms. Amarillo's 39% failure rate is added.
  22. §6.2: noted that the per-class keyword counts were not re-run. Total fix-like commits are 1,324-1,378 by subject (regex-dependent). The Cubic commit count is split into subject and whole-message counts.
  23. §6.3: the below-Tmin errata row now gives the density (55018.5 mol/m3), and the Cvirial errata row records the verifier's independent reproduction.
  24. §6.4: the SLEEF lesson is attributed to #3046. §7: the SLEEF "rejected" wording is softened to match #3046.
  25. §8: the consistency sweep lists 15 pairs, of which 11 are implemented.
  26. (added by verifier) §6.2: the HumidAir `kT` density-as-pressure bug is now cited directly at `src/HumidAirProp.cpp:2344-2345`.
- Refuted or downgraded rot: R16's "most commits/issues" impact (downgraded); R11's paper typos (re-attributed to the literature, not to CoolProp); bd:z94e (never shipped); SVDSBTL "fix-heavy" (an artefact of the whole-message regex).
- Not re-verified, and left as attributed: peer-doc references (01, 04, 05, 07, 08, 09, 10 §6 items), lizard `-Eduplicate` clone percentages, §6.2 keyword-class counts, and the #3046 performance numbers beyond the issue text.
