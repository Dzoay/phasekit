# 09 Fluid data assets, schemas, embedding and licensing - CoolProp v8.0.0 map

> **Scope.**
> - **Data:** `dev/fluids/*.json` (136 loaded, plus 2 `*.json_disabled`), `dev/mixtures/`, `dev/cubics/`, `dev/pcsaft/`, `dev/incompressible_liquids/json/`.
> - **Pipeline:** `dev/generate_headers.py`, `dev/cbor_min.py`, `dev/check_cbor_min_vs_cbor2.py`, `dev/package_json.py`, `dev/validate_fluid_schemas.py`, `dev/scripts/inject_superanc_check_points.py`.
> - **Loader:** `src/Backends/Helmholtz/Fluids/FluidLibrary.{h,cpp}`, `FluidLibraryFactories.h`, `include/CoolProp/CoolPropFluid.h`, `src/superancillary.cpp`.
> - **JSON and schemas:** `include/CoolProp/detail/{json,msgpack}.h`, `src/SchemaValidation.cpp`, `include/CoolProp/schemas/`.
> - **Embedding and licensing:** `externals/{incbin,miniz-3.1.1}`; data embedding in `CMakeLists.txt`, `wrappers/Python/CMakeLists.txt` and `cmake/`; `LICENSE`, `CoolPropBibTeXLibrary.bib`.
> - **Size:** about 4,400 lines of pipeline and loader code (4,372 by `wc -l` over the files listed above, excluding `superancillary.h`), plus about 1,150 lines of side-dataset loaders shared with areas 04 and 06. There are 17.2 MB of fluid JSON and 0.79 MB of side datasets.
> - **Master-only paths:** `dev/gerg/` and `THIRD_PARTY_NOTICES.md` exist only on origin/master and are cited as `master:`.
> - Part of the coolprop-rs port plan; cites the v8.0.0 source.
>
> **Measurement setup.**
> - Oracle: the CoolProp==8.0.0 wheel (cp312-abi3, manylinux2014, GCC 10.2.1).
> - Machine: CPython 3.12, i7-8700K (12 threads), Linux 7.2.8.
> - Scripts are in the session scratchpad under `a09/`. "Oracle" below always means this wheel.

**Headline facts (all re-measured for this document)**

- **Size**
  - 136 fluid files: 16.37 MiB of pretty JSON, 9.19 MiB minified.
  - **The superancillary (SA) Chebyshev tables are 89.8 % of the bytes.** Everything else is about 7 KB of JSON per fluid (median 267 numbers).
  - As raw f64 the whole library is **2.81 MiB**: 1.9 to 30.9 KiB per fluid, median 21.8 KiB. This counts EOS[0] only and leaves out check points.
- **Embedding**
  - The `.so` holds **one uncompressed 4,648,555 B CBOR blob**, which is 51.4 % of the 9.05 MB `CoolProp.abi3.so`.
  - The blob matches v8.0.0 `dev/fluids` exactly: 136/136 fluids, including whether each number was written as an int or a float. Re-encoding the sources gives byte-identical output.
  - The wheel *also* installs the same data as a **27.9 MB hex header that nothing reads**. That is 71.5 % of the 39.0 MB (37.2 MiB) installed package, excluding `__pycache__`.
- **Load cost**
  - The first touch loads every fluid at once: **1.96 s and +67.6 MiB RSS**.
  - 91 % of that time is SA construction (171 ms with SAs disabled).
  - `import CoolProp` pays this cost: 1.98 s, with RSS going from 12 to 88 MiB.
  - The SA work is about 24.6k 11×11 eigenproblems. Across all 390 SA variables they find **only 7 interior extrema**, so the whole step can be precomputed.
- **Validation**
  - The 136 fluid files have **no schema**. Their checks are `assert`s, which are compiled out of the wheel.
  - PC-SAFT schema failures are **ignored unless the debug level is above 0**.
  - Offline, the 3 side schemas are run only by a local preflight script, in no CI workflow. The cubic and PC-SAFT schemas are also checked at runtime on first use (§6 R15).
- **State and memory**
  - The fluid library is global and mutable, and its writers take no locks.
  - Each live HEOS state holds deep copies of its fluid: **78–126 KiB per state**.
- **Oracle data defects (confirmed in the wheel)**
  - **31 of 147** predefined mixtures cannot be constructed.
  - `Ttriple` returns T_min: CycloPropane gives 273 K, while the data say 145.7 K.
  - An ODP sentinel of −1 makes `iODP` throw for 100 fluids, R134a among them.
  - INCOMP LiBr returns placeholder values η = 1 Pa·s and λ = 0.
  - PCL viscosity is 100× too high.
  - 4 reducing densities are wrong.
  - The cubic table has not been regenerated since 2016 and drifts by up to 7.6 % in p_c.
- **Licensing**
  - Code and data are MIT (CoolProp). The v8.0.0 wheel ships only `LICENSE`.
  - Third-party notices were added after 8.0.0, and they cover code only.
  - Provenance still to clear:
    - Ethanol–Water, marked "From REFPROP 9.1 with permission";
    - the DTU environmental table, which was built from REFPROP `.fld` files;
    - the fastchebpure SA outputs;
    - incompressible fits from manufacturer data sheets.

## 1. Purpose and concepts

- **What the files contain.** Fluid data are *model parameters*, not property tables. Each fluid has:
  - identity: `NAME`, CAS, aliases, REFPROP name, InChI/SMILES;
  - EOS: α⁰ and αʳ term lists, the reducing state, R, M and limits;
  - cheap ancillaries used for initial guesses;
  - **SA**: piecewise Chebyshev expansions of p_sat, ρ′ and ρ″ versus T, which serve as the saturation solution;
  - transport and σ correlations (05-transport);
  - the melting line;
  - environmental metadata: GWP, ODP, ASHRAE 34.
- **Source of truth.**
  - Hand-edited JSON in `dev/fluids`, *mutated in place* by about 12 injection scripts. Examples: `package_json.py`, `dev/scripts/inject_superancillary.py`, `inject_superanc_check_points.py`, `inject_states.py` and `inject_InChI.py`. (`dev/scripts/set_reference_state.py` only prints offsets and does not write sources.)
  - The SA blocks come from an external fitter, **fastchebpure**. Each is stamped with a structural hash of the EOS it was fitted against (§3).
- **Pipeline.**
  - Build time: JSON → CBOR (`cbor_min`) → `.incbin`, or a C hex array on MSVC, WASM and Catch2 → linked into the binary.
  - Run time: blob → one nlohmann DOM → one `CoolPropFluid` per fluid → the process-global `JSONFluidLibrary`.
  - The side datasets (mixtures, cubic, PC-SAFT, incompressible) are embedded as JSON *text*. Each has its own singleton parser.
- **The data format is public API.** Users can read and extend it through these entry points:
  - `get_fluid_param_string(f, "JSON" | "aliases" | "CAS" | "REFPROP_name" | "BibTeX-*" | …)` (`src/CoolProp.cpp:1134`, `HelmholtzEOSMixtureBackend.cpp:251-307`);
  - `add_fluids_as_JSON(backend, json)` (`src/CoolProp.cpp:703-716`), with the `OVERWRITE_FLUIDS` switch (`Web/coolprop/HighLevelAPI.rst:455-471`);
  - `set_reference_state` (`src/CoolProp.cpp:946-1040`), which rewrites library entries.

## 2. Structure (key types/functions -> path:line)

| Piece | Location | Role / notes |
|---|---|---|
| Header generator | `dev/generate_headers.py:55-72` (tables), `:93-207` `TO_CPP`, `:343-369` `DependencyManager`, `:371-451` `combine_json` | Globs `dev/fluids/*.json` (`:389`). Writes `dev/all_fluids.{cbor,json}` + `all_fluids_verbose.json`, then the incbin-template hex header `include/all_fluids_CBOR.h` (`:74-90`). Side datasets become `static constexpr char X_binary[]` + `std::string_view` (`:193-195`). |
| CBOR codec | `dev/cbor_min.py:1-153`; parity `dev/check_cbor_min_vs_cbor2.py` | Stdlib-only RFC 8949 encoder. Floats always 0xFB f64 (`:57-61`); non-finite values rejected. The generator round-trips its own output (`generate_headers.py:405-408`). |
| Legacy packager | `dev/package_json.py:91-406` | Injects Mulero σ (`:8-88`, `:213-249`) and DTU environmental data, merges ancillaries, provides `combine_json`. `__main__` mutates sources (`:404-406`). |
| Embedding | `FluidLibrary.cpp:9-24` | `INCBIN(all_fluids_CBOR, "all_fluids.cbor")` (`:22`), via the `dev/` include dir (`CMakeLists.txt:375`). MSVC (`:18-19`) and `COOLPROP_NO_INCBIN` include the hex header instead. `COOLPROP_NO_INCBIN` is set for emscripten (`CMakeLists.txt:2069`) and Catch2 (`:2354`). |
| Wheel packaging | `wrappers/Python/CMakeLists.txt:60-95` (header regeneration), `:453-461` (header install) | The install rule excludes `*_JSON.h` / `*_JSON_z.h` but **not `*_CBOR.h`**, so the 27.9 MB hex header ships. |
| Global library | `FluidLibrary.cpp:28` `static JSONFluidLibrary library`; `:40-44` `std::call_once` | One per process. Load is triggered by any HEOS lookup, `FluidsList` or JSON echo (`:371-394`). |
| Load | `FluidLibrary.cpp:46-62` | `getenv` → `cpjson::from_cbor` (the whole blob into one DOM, `:56`) → `add_many`. Failures are printed to stdout and swallowed (`:57-61`). |
| Container | `FluidLibrary.h:31-37` | `std::map<size_t,CoolPropFluid> fluid_map`, `std::map<size_t,std::string> JSONstring_map`, `vector name_vector`, `std::map<string,size_t> string_to_index_map` |
| Per-fluid parse | `FluidLibrary.cpp:150-369` `add_one`. In `FluidLibrary.h`: `:42-169` αʳ, `:171-334` α⁰, `:350-450` EOS, `:453-457` all EOS entries, `:459-1003` transport, `:1005-1013` default transport, `:1015-1070` melting, `:1071-1121` states, `:1122-1176` ancillaries | Hand-written `.at()` / `cpjson::get_*` walkers (`include/CoolProp/detail/json.h:62-171`) |
| Factories | `FluidLibraryFactories.h:26-36` (σ), `:41-73` (saturation ancillary) | Plain `Values` structs keep nlohmann out of installed headers. **Any unknown ancillary `type` falls through to "exponential"** (`:56-60`). A missing `Tmin`/`Tmax` is swallowed by `catch (...)` (`:52`). |
| Identifier index | `FluidLibrary.cpp:287-305` (duplicate detection), `:344-359` (insert CAS, name, alias, `upper(alias)`) | No `upper(name)` and no REFPROP name in the index |
| Lookup | `FluidLibrary.h:1229-1317` `get(string)` (synthesises `-SRK` / `-PengRobinson`), `:1323-1331` `get(size_t)` **by value** | Deep copy to every backend (`HelmholtzEOSBackend.h:50,53`) |
| JSON echo | `FluidLibrary.h:1206-1223` | Re-parses the stored dump, wraps it in an array and re-dumps on every call (2.2 ms for Water) |
| SA holder | `CoolPropFluid.h:406` `std::string superancillaries_str`, `:433-438` `get_superanc()`, `:441-447` setter | Eager construction unless `LAZY_LOAD_SUPERANCILLARIES` is defined (`:444-446`) |
| SA build | `src/superancillary.cpp:51-68` (re-parses the SA string); `include/CoolProp/superancillary/superancillary.h:508-550` (extrema by companion-matrix eigenvalues), `:557-630` (monotonic intervals), `:637-642` (constructor), `:909-918` | All extrema and intervals are computed at load. The inverse T(p) is built lazily under a mutex (`:1078-1085`; dyadic split `:923-951`). |
| JSON wrapper | `json.h:39-55` (parse / from_cbor → `ValueError`), `:186-233` `validate_schema` (Valijson) | Not installed. Symbols are hidden at link time (`cmake/CoolPropJSONVisibility.cmake:15-35`); that helper is fail-open on a missing target (fixed later in `6b548e4e`). |
| Generic schema helpers | `src/SchemaValidation.cpp:10-23`, `include/CoolProp/SchemaValidation.h` | String in / string out. The only in-tree schema is `include/CoolProp/schemas/SVDSBTLOptions.h` (backend options, draft-07), **not fluids** |
| Side-file schemas | `dev/cubics/cubic_fluids_schema.json`, `dev/pcsaft/pcsaft_fluids_schema.json`, `dev/mixtures/mixture_departure_functions_schema.json` (all draft-04) | Cubic and PC-SAFT are validated **at first use in every process** (`CubicsLibrary.cpp:133-146`, `PCSAFTLibrary.cpp:31-46`). Departure functions are checked only by the script. |
| Mixture data | `MixtureParameters.cpp:16-51` (predefined; static init at `:51`), `:81-283` (binary pairs, `call_once` at `:103`), `:403-511` (departure, `call_once` at `:503`), `:589-591` (missing pair → throw) | Keyed by sorted CAS pair or by name |
| miniz / msgpack | `src/Backends/Tabular/TabularBackends.cpp:9,54-99`; `src/SBTL/SVDSurfaceSerializer.cpp:14,28`; `include/CoolProp/detail/msgpack.h:12` (a bare `#include "msgpack.hpp"` wrapper) | **Not used for fluid data.** They serve only the tabular and SVD-SBTL caches (area 08). |

## 3. Algorithms and formulas (cite the papers CoolProp cites)

- **CBOR (RFC 8949).**
  - A definite-length encoding of the JSON data model. Floats are always 0xFB doubles, so the encoding is lossless (`cbor_min.py:10-17`).
  - Before v8 the data were zlib-compressed JSON parsed with RapidJSON, about 37 ms. The migration gate measured CBOR + nlohmann at 35 ms decode (`dev/json_migration_bench/README.md:40-63`).
  - **The gate never measured the SA build**, which costs about 50× the decode (§5).
- **SA freshness hash (FNV-1a-64).**
  - Input: a type-tagged walk of `EOS[0]` minus `SUPERANCILLARY`, with keys sorted by UTF-8 bytes (`dev/scripts/inject_superanc_check_points.py:111-222`; C++ twin `src/Tests/CoolProp-Tests.cpp:3598-3830`).
  - Tags: `n`/`f`/`t`; `i` + LE int64; `d` + LE IEEE bits; `s`/`a`/`o` + LE u64 length.
  - Two consequences for any re-encoder:
    - **It distinguishes JSON `1` from `1.0`.** EOS[0] minus `SUPERANCILLARY` has 5,826 integer literals and 11,723 floats, so a re-encoder must keep the literal kind to reproduce the 130 stamps. All 130 recompute correctly here.
    - **It covers strings too** (BibTeX keys, `*_units`). A citation-only edit therefore forced a restamp after 8.0.0 (`d2a3a9fc`).
- **SA construction.**
  - For each Chebyshev piece, CoolProp builds the companion matrix of the derivative series, balances it, and takes the real eigenvalues in [−1, 1] as extrema (`superancillary.h:508-550`). These feed the monotonic-interval table (`:557-630`). This runs in the constructor (`:637-642`).
  - Scale: 24,486 pieces in EOS[0] plus 171 in R1234yf's unused `EOS[1]`, all of degree 12.
  - Measured result (numpy `chebroots` on the same data): **7 interior extrema in all 390 SA variables.**
    - Water ρ′ at 277.15 K and HeavyWater ρ′ at 284.77 K are the physical density maxima.
    - Three are *inferred artefacts*: PropyleneGlycol p_sat at 216.5 K, where p ≈ 2.7×10⁻⁸ Pa; DimethylCarbonate ρ″ (3 extrema); and m-Xylene ρ″.
    - The ρ″ ones lie within 1e-4 K of T_c, inside the last dyadic pieces of width 1.7e-8·T_c, where the critical anchor takes over.
  - *Inference:* the eigen work dominates the 1.79 s SA share. numpy needs 0.68 s for 24,657 random 11×11 eigenproblems.
- **Bibliography.** The bib cites the cubic SA paper (`Bell-IECR-2021`, `CoolPropBibTeXLibrary.bib:540`). **The multiparameter SA blocks carry no BibTeX key, and the bib has no entry for them.**
- **Surface tension (Mulero).**
  - σ = Σ aᵢ(1−T/T_c)^{nᵢ}, from `Mulero-JPCRD-2012` and `Mulero-JPCRD-2014` (`package_json.py:192-197, 294-299`).
  - Sources of the 108 σ fits: Mulero 2012 (75), Mulero 2014 (27), Okada-IJT-1999 (4), Kondou-IJR-2015 (1), IAPWS-1994 (1).
- **Default transport scales when `TRANSPORT` is absent** (Chung et al., IECR 1988; `FluidLibrary.h:1005-1013`):
  - σ_η = 0.809/ρ_c^{1/3}, with ρ_c in mol/L and the result in nm;
  - ε/k = T_c/1.2593.
- **Cubic synthesis from a name suffix** (`FluidLibrary.h:1236-1311`).
  - For `X-SRK` / `X-PengRobinson`, CoolProp keeps α⁰ and replaces αʳ with the cubic, using T_c, p_c and ω, with a hard-coded **R = 8.3144598** (`:1257`).
  - The cubic-library branch uses `R_U_CODATA` for the cubic (`:1277`), but it builds a fresh `EquationOfState E` whose `R_u` (and `molar_mass`) is never assigned. At v8.0.0 these members had no initialiser, so the value was indeterminate. Default `= 0` was added in `0f978943`, and `R_u` was set properly in `ae54172f`, both after 8.0.0. The oracle returns `gas_constant() = 0.0` and `p = 0` for `HEOS::R1233ZD(E)-SRK` and `-PengRobinson`. Only R1233ZD(E) reaches this branch among the 116 built-in cubic names; the rest resolve through the HEOS index (verified).
  - When ρ_c is missing it uses Kazakov's fit (`:1287-1289`): v_c [L/mol] = 2.14107171795·(T_c/p_c·1000) + 0.00773144012514.

## 4. Data and configuration inputs

### 4.1 Fluid files (v8.0.0, measured)

| Metric | Value |
|---|---|
| Files | 136 `*.json` + `AceticAcid.json_disabled`, `R407F.json_disabled` = 138 entries |
| Pretty JSON | 17,163,124 B (16.37 MiB). Per file: min 11,080 (SES36), median 129,640, max 248,274 (R1234yf) |
| Minified | 9,639,664 B (9.19 MiB). Per fluid: min 6,716, median 72,776, max 138,557 |
| Core JSON (everything except SA and alternate EOS) | 3,844 / 7,028 / 9,375 B per fluid (min / median / max); 0.93 MB total. 37,840 numbers = 0.29 MiB as f64, median 267 per fluid |
| Embedded CBOR | 4,648,555 B; sha256 `b60ca6fcc49454380cb90cc9821759a735325d6d868bfcc023114d79fbb39c9d` |
| CBOR compressed | gzip-9 3,075,416 (2.93 MiB); zstd-19 2,774,279 (2.65 MiB); xz-9e 2,565,340 (2.45 MiB); brotli-11 2,494,797 (2.38 MiB) |
| SA numbers (`EOS[0]`) | 373,268 values = 2.85 MiB f64. `check_points` account for 2,730 values (97,207 B JSON). `crit_anc` + `meta` take 98 KB JSON |
| SA shape | 130 fluids; every fluid except Air, R404A, R407C, R410A, R507A and SES36 (the pseudo-pure ones). 3 variables (p, ρ′, ρ″), 53–91 pieces each, **all degree 12** (13 coefficients). **p, ρ′ and ρ″ share identical, contiguous breakpoints in 130/130 fluids.** Pieces shrink dyadically toward T_c, to a relative width of 1.7e-8. |
| Raw f64 layout | EOS[0] only, SA with one breakpoint array, no check points: **2,941,584 B (2.81 MiB)**. Core 0.29 MiB + SA 2.52 MiB; SA per fluid 16.8 / 19.9 / 28.6 KiB. Per fluid in total: 1,968 B (SES36) / 22,348 B median / 31,664 B (Methanol). INFO strings add 25.8 KB. |
| Raw f64 compressed | zlib-9 2,546,206 (−13 %), zstd-19 2,478,708 (−16 %), xz-9e 2,378,920 (−19 %). f64 mantissas barely compress. |
| Alternate EOS | 23 fluids carry `EOS[1]`, 108,053 B minified. Examples: Colonna 2006/2008 siloxanes, Richter 2011 R1234yf, Span 2003 alkanes. Only `EOSVector[0]` is ever read (`CoolPropFluid.h:539-545`; no other `EOSVector` reader in `src/`). R1234yf's alternate SA (no check points, no hash) is still built at load. |

Byte share of the minified total: `EOS[].SUPERANCILLARY` **89.84 %**, `ANCILLARIES` 3.86 %, `EOS[].STATES` 1.56 %, `STATES` 1.00 %, `alphar` 0.93 %, `TRANSPORT` 0.67 %, `INFO` 0.63 %, `alpha0` 0.52 %, `critical_region_splines` 0.29 %, and the rest < 0.2 %.

### 4.2 Top-level keys (count of fluids)

| Key | Content | Findings |
|---|---|---|
| `INFO` (136) | Required by the loader: `NAME`, `CAS`, `REFPROP_NAME`, `ALIASES` (`FluidLibrary.cpp:163-230`). Optional, in 126 fluids: `FORMULA`, `INCHI_STRING`, `INCHI_KEY`, `SMILES`, `CHEMSPIDER_ID`, `2DPNG_URL`. Optional, in 125: `ENVIRONMENTAL{ASHRAE34, GWP20/100/500, ODP, HH, FH, PH, Name}`. | **Sentinels:** `REFPROP_NAME = "N/A"` (9 fluids); ODP −1 (100); GWP100 −1 (59); ASHRAE34 `"UNKNOWN"` (67) or `"?"` (13); ChemSpider −1 (3). Loader defaults `"N/A"` / −1 (`:186-217`). **CAS is overloaded in 10 fluids:** `AIR.PPF`, `R404A.PPF`, `R407C.PPF`, `R410A.PPF`, `R507A.PPF`, `SES36.ppf`, `1333-74-0p/o`, `7782-39-0p/o`. |
| `STATES` (136) | `critical`, `triple_liquid`, `triple_vapor` {T, p, ρ, h, s + `*_units`} | `triple_liquid.T == EOS.sat_min_liquid.T` in 136/136. It is the EOS minimum state, not the triple point. |
| `EOS[]` (136; 159 entries) | Scalars: `gas_constant`, `molar_mass`, `acentric`, `T_max`, `p_max`, `Ttriple` (**never read**), `pseudo_pure`, `BibTeX_EOS/CP0`. Term lists: `alpha0[]`, `alphar[]`. `STATES{reducing, sat_min_liquid/vapor, hs_anchor, pressure_max_sat / temperature_max_sat (6)}`. `critical_region_splines` (63 in EOS[0]). `SUPERANCILLARY` (130 + 1). | `gas_constant` has **10 distinct values**: 8.314472 ×63, 8.3144621 ×17, 8.31451 ×15, 8.314462618 ×15, 8.3144598 ×14, … αʳ block types: Power 134, Gaussian 78, Exponential 10, NonAnalytic 2, Lemmon2005 1, GaoB 1, DoubleExponential 1. 12–56 coefficients per fluid (median 16). **All `d` and `l` exponents are integers** (d ≤ 15, l ≤ 6). α⁰ types: Lead and LogTau 136 each, PlanckEinstein 107, EnthalpyEntropyOffset 55, CP0PolyT 21, Power 17, PlanckEinsteinFunctionT 7, CP0Constant 4, CP0AlyLee 3, PlanckEinsteinGeneralized 2. |
| `ANCILLARIES` (136) | `rhoL`, `rhoV` (136); `pS` (130: type `pV` ×68, `pL` ×62), or `pL` + `pV` (6 pseudo-pure); `hL/hLV/sL/sLV` (110, `rational_polynomial`); `surface_tension` (108); `melting_line` (30: Simon 18, polynomial_in_Tr 9, polynomial_in_Theta 3) | `_note`, `description` and every `*_units` field are documentation only. **No `*_units` field is read anywhere in the loader.** |
| `TRANSPORT` (66) | `viscosity` (66), `conductivity` (63). 28 `"hardcoded"` name hooks dispatch to C++, plus 4 `"None"`. | See 05-transport |

### 4.3 FluidsList: why 136 names but "139 files"

- **v8.0.0 has 138 entries in `dev/fluids`.** There are 136 `*.json` files plus 2 `*.json_disabled` files.
  - The disabled files were switched off on 2014-11-29 (`fd993bd3`). They use the pre-`INFO` layout (top-level `NAME`/`CAS`/`ALIASES`), and AceticAcid uses `ResidualHelmholtzAssociating`.
  - `glob('*.json')` (`generate_headers.py:374, 389`) skips them, so 136 fluids are embedded and **FluidsList has exactly 136 names**.
- **The figure 139 is origin/master.** There, `R1132a.json` was added (`ec2e75d5`; Akasaka, Low & Lemmon, IJT 2026), giving 137 + 2.
- **Names come from `INFO.NAME`, not file names.** `R1224yd(Z).json` defines `R1224YDZ`, and the oracle rejects `R1224yd(Z)` with "key … not found". This is the only fluid where file stem ≠ NAME.
- **FluidsList order is the build host's `glob.glob` order**, which is unsorted (`:389`). The wheel's list starts `R1234ze(E), CarbonDioxide, CycloHexane, R218, ParaDeuterium, …`. Compare lists as sets.

### 4.4 Other embedded datasets

| File(s) | Size | Content and findings | Load |
|---|---|---|---|
| `mixtures/mixture_binary_pairs.json` | 207,579 B | 888 pairs keyed `CAS1/CAS2`, no duplicate pairs. Sources: Bell-JCED-2016 582, Kunz-JCED-2012 194, Bell-JCED-2025 74, Gernert-Thesis-2013 15, … **48 pairs reference fluids not in the library:** R1216, C14, IOCTANE, C1CC6, C3CC6, RE347MCC. | JSON text, `call_once`; 7.5 ms on first use |
| `mixtures/mixture_departure_functions.json` | 10,377 B | 28 functions: Exponential 16, GERG-2008 9, Gaussian+Exponential 3. Ethanol–Water's `BibTeX` is **`"From REFPROP 9.1 with permission"`**. | `call_once` |
| `mixtures/predefined_mixtures.json` | 32,972 B | 154 entries, 147 names; 7 identical duplicates (R429A–R438A). **31/147 fail to construct in the oracle:** 27 lack a binary pair ("Could not match the binary pair … for now this is an error", `MixtureParameters.cpp:589-591`; R401A/B/C, R402A/B, R403A/B, R405A, R406A, R408A, R409A/B, R412A, R413A, R414A/B, R416A, R424A, R426A, R429A, R435A, R437A, R438A, R446A, R453A, R458A, R461A), and R468A/B/C and R473A need R1132a, which is not in v8.0.0. The injector hard-codes R1132a's molar mass (`dev/mixtures/inject_ASHRAE_2026.py:13-26`). | **Static init at `.so` load** (`MixtureParameters.cpp:51`). `emplace` keeps the first duplicate (`:45-47`). |
| `cubics/all_cubic_fluids.json` | 115,014 B | 116 fluids with UPPERCASE names, all of them HEOS fluids by CAS. **Last regenerated 2016-11-29** (`59399842`) by running CoolProp (`dev/cubics/generate_cubics_listing.py:4-18`). **14 HEOS pure fluids are missing** (Chlorine, R1123, R1336mzz(E/Z), n-Perfluoro*, …). **21 entries drift** from current HEOS constants: Neon ω 8.3 %, D5 p_c 7.6 %, MD2M 7.3 %, MD4M 5.9 %, Methanol 1.6 %. *Inference:* the siloxanes were generated from the Colonna EOS that are now only alternates. | Meyers singleton, Valijson at runtime; 6.3 ms |
| `pcsaft/all_pcsaft_fluids.json` + `mixture_binary_pairs_pcsaft.json` | 61,585 + 21,512 B | 180 fluids, 26 of them ions. 170 names are all-uppercase (16 ions such as `H3O+` and `OH-` among them); the other 10 are mixed-case ions (`Li+`, `Cl-`, `Mg2+`, …). 222 aliases. The oracle accepts `PCSAFT::METHANE` and the CAS, but rejects `PCSAFT::Methane`. | Meyers singleton, Valijson at runtime, **fail-open** (§6 R15); 5.4 ms |
| `incompressible_liquids/json/*.json` | 126 files, 336,736 B | 74 pure, 39 mass-based and 13 volume-based fits. Sources: Melinder2010, SecCool (Skovrup2013), ASHRAE2006, and manufacturer sheets (Dow, Therminol, Paratherm, Dynalene, …); references are free text, not bib keys. **Defects in the v8.0.0 data:** 16 files ship unfitted placeholder coefficients (oracle `INCOMP::LiBr-20%`: η = 1.0 Pa·s, λ = 0.0; scrubbed in `9f1b088c`), and PCL viscosity is 100× too high (oracle 0.154 Pa·s at 300 K; fixed in `25c4f3a5`). | `call_once`, 5.8 ms |
| Raw tables (build-only) | `KunzWagner2012_TableA6/A7/A8.txt`, `Bell2016Coefficients.txt`, `ASHRAE_predefined_2026.tsv`, `old_BIP.json` | Transcriptions of primary sources, plus inject scripts | — |
| `CoolPropBibTeXLibrary.bib` | 3,622 lines, 298 entries (plus one `@Comment`) | The data reference 194 distinct keys. 3 are missing: `BELL-PERSONAL-2015`, `Huber-RP912`, and the REFPROP-permission string. | Shipped in the wheel |

### 4.5 Identifiers (input to a resolver)

- **Built-in counts.** The built-in HEOS set has 136 names, 347 aliases, 136 CAS, 127 REFPROP names and 126 InChIKeys. That is 872 tokens and **556 distinct case-folded keys, 6.9 KB in total**.
  - **Case-folding creates no cross-fluid collisions.** All keys are ASCII.
  - Every REFPROP name is already reachable as a name or alias when case is ignored.
- **Each backend resolves identifiers differently today.**
  - **HEOS** indexes `NAME`, CAS, aliases and `upper(alias)`. It is case-sensitive otherwise: `water` and `WATER` work, but `r134a` fails.
  - **Cubic** upper-cases the input and has no CAS index: `PR::74-82-8` fails.
  - **PC-SAFT** indexes name, CAS, aliases and `upper(alias)`, as HEOS does (`PCSAFTLibrary.cpp:181-191`), but not `upper(name)`: `METHANE` works and `Methane` fails.
- **List outputs are comma-joined strings, and the delimiter is a global config.** The delimiter is `LIST_STRING_DELIMITER = ","` (`include/CoolProp/detail/configuration_keys.h:80`). The output is ambiguous: `aliases(Dichloroethane)` = `"DICHLOROETHANE,1,2-dichloroethane,1,2-DICHLOROETHANE"`. The workaround for HEOS is `aliases_bar`, which is pipe-joined (`HelmholtzEOSMixtureBackend.cpp:253-254`).

### 4.6 Configuration and environment

- `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY`: an environment variable read in `load()` (`FluidLibrary.cpp:47`) and again per EOS (`FluidLibrary.h:393`).
- `LAZY_LOAD_SUPERANCILLARIES`: a compile macro, on by default only for the Debug Catch2 runner (`CMakeLists.txt:2368-2372`).
- `COOLPROP_NO_INCBIN`.
- `OVERWRITE_FLUIDS`: a global config bool (`FluidLibrary.cpp:312`).
- Debug-level prints to stdout (`:222, :238, :362`).
- **There is no build option to subset fluids or omit data.** No `option()` in `CMakeLists.txt` touches data.

## 5. State, caching, globals, thread-safety, memory

**Load path.** The first call that needs any HEOS fluid runs `ensure_library_loaded()`, which:

1. decodes the whole CBOR blob into one DOM;
2. runs `add_one` for all 136 fluids;
3. for each EOS, `dump()`s the SA subtree to a string (kept for good) and re-parses it into Chebyshev objects (`FluidLibrary.h:392-396`, `superancillary.cpp:53`);
4. computes all eigen extrema and monotonic intervals;
5. stores a full JSON dump of each fluid (`FluidLibrary.cpp:338`).

The DOM is then freed. `import CoolProp` triggers all of this through `__fluids__ = get_global_param_string('fluids_list')` (`wrappers/Python/_nanobind/__init__.py:33-35`).

| Measurement (wheel) | Time | RSS |
|---|---|---|
| Load the `.so`, bypassing the package `__init__` | 4.0 ms | +5.5 MiB (to about 19 MiB) |
| **First `PropsSI(D,T,P,Water)`, which loads all 136 fluids** (3 runs) | **1,959–1,969 ms** | **+67.6 MiB** (to 86.6) |
| The same after `malloc_trim(0)` | — | +52.3 MiB live |
| The same with `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY` | 171–176 ms | +56.5 MiB; +26.4 MiB after trim |
| First `FluidsList` call | 1,950 ms | +67.2 MiB |
| `import CoolProp` | 1,981–1,983 ms | 12.1 → 87.7 MiB |
| Per fluid, re-adding the oracle's own JSON | min 0.29 ms (SES36), **median 15.8 ms**, max 31.4 ms (R1234yf, 2 SAs); sum 2.09 s. Without SA: median 2.07 ms, sum 0.28 s | — |
| First `PropsSI` after load / warm `PropsSI` / warm `AS.update(PT)` | 1.6 ms / 68.8 µs / 13.8 µs | — |
| `AbstractState('HEOS', f)` | 49–68 µs | **Per live state:** Air 78, Propane 81, R1234yf 117, Water 126 KiB (N = 1000) |
| First use of binary pairs / PC-SAFT / PR / incompressible | 7.5 / 5.4 / 6.3 / 5.8 ms | ≤ 2 MiB. These do **not** load the HEOS library. |

| Shared state | Where | Init | Mutation and risk |
|---|---|---|---|
| `JSONFluidLibrary library` | `FluidLibrary.cpp:28` | `call_once` (`:40-44`) | These write the shared maps and fluids in place **without locks**, while `get()`, `get_JSONstring()` and `get_fluid_list()` (`FluidLibrary.h:1335`) read them concurrently (a data race, so UB): <br>• `add_many(string)`, a static member that writes the global (`:130-138`); <br>• `add_one` (`:150-369`); <br>• `set_fluid_enthalpy_entropy_offset` (`:64-127`). <br>The reference state is process-global. |
| Fluid copies | `FluidLibrary.h:1323-1331` → `HelmholtzEOSBackend.h:50,53` → `HelmholtzEOSMixtureBackend.cpp:114, 141-149` (SatL/SatV) | — | Each state holds several deep copies of the model. Only the SA object is shared (`shared_ptr`). |
| `superancillaries_str` | `CoolPropFluid.h:406` | Set at parse | About 67 KB per fluid, kept for good and copied with every `CoolPropFluid`. The wheel uses the **old copy-on-write `std::string` ABI**: it imports 41 `_ZNSs*` symbols, including `_Rep::_S_create`, and no new-ABI `basic_string` imports, so copies share one buffer. *Inference:* on libc++, MSVC and new-ABI builds every copy duplicates it, despite the "copying O(1)" comment at `:431-432`. |
| `get_superanc()` | `CoolPropFluid.h:433-438` | Unlocked lazy init when `LAZY_LOAD_*` is defined | In lazy builds each copy builds its own SA, about 14 ms each. |
| SA lazy members | `superancillary.h:874, 1078-1085` | A mutex is taken **on every** `get_invlnp()` | Lock on a hot path (area 03) |
| Predefined mixtures | `MixtureParameters.cpp:16-51` | Static constructor | `set_predefined_mixtures` (`:72-75`) is unsynchronised. |
| Binary pairs, departure functions | `MixtureParameters.cpp:103, 503` | `call_once` | `binary_pair_map()` returns a mutable reference; the `set_*` functions are unsynchronised. |
| Cubic, PC-SAFT libraries | `CubicsLibrary.cpp:160-163`, `PCSAFTLibrary.cpp:49-57` | Meyers singletons | `add_fluids_as_JSON` mutates them. |

**Memory accounting.** About 3.1 MiB of numbers (2.85 MiB SA + 0.29 MiB core) occupy about 52 MiB live, roughly 17× overhead. The contributors are:

- **With SAs disabled: 26.4 MiB live.**
  - Per-fluid JSON dumps in `JSONstring_map`, about 9.2 MiB. These include the SA text whether or not SAs are enabled.
  - 136 `CoolPropFluid` objects, about 17 MiB. *Inference:* that is about 125 KiB each, which matches the per-state figures above. 23 of them carry unused alternate EOS entries.
- **SAs add 25.9 MiB live** (on minus off, after trim).
  - `superancillaries_str`, about 8.3 MiB.
  - Parsed expansions with extrema and intervals, about 17.6 MiB.
- **glibc keeps another 15.3 MiB** of freed DOM until `malloc_trim`.

**WASM.** With `COOLPROP_NO_INCBIN` the same 4.6 MB blob is compiled from a 27.9 MB hex source into the module. *Inference (not measured):* the heap cost after load is the same as native.

## 6. Rot and bugs (table: issue | evidence | impact | Rust remedy)

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| R1 | The whole library loads eagerly, including SA eigen work for all fluids | `FluidLibrary.cpp:40-62`; `superancillary.h:508-550, 637-642`; measured 1.96 s / +67.6 MiB; `_nanobind/__init__.py:33` | Latency on first call or import; memory spent on fluids never used | Lazy per-fluid decode via `OnceLock`. **Datagen precomputes extrema and intervals: 7 numbers in total.** |
| R2 | The SA exists in 4 forms: CBOR → DOM → `dump()` string (kept) → re-parse → object | `FluidLibrary.h:392-396`; `CoolPropFluid.h:406, 441-447`; `superancillary.cpp:51-68` | CPU and RAM waste; copy cost depends on the ABI | One binary section read straight into the final struct |
| R3 | `JSONstring_map` keeps a full dump of every fluid, and the echo re-parses it on each call | `FluidLibrary.cpp:338`; `FluidLibrary.h:1206-1223`; 2.2 ms per call | About 9.2 MiB resident for a debug API | Serialise on demand behind a `json` feature |
| R4 | Mutable global library; unsynchronised writers; reference state rewrites the shared fluid | `FluidLibrary.cpp:28, 64-127, 130-138, 150-369`; `FluidLibrary.h:1335`; `CoolProp.cpp:703-716, 946-1040` | UB under concurrency. One caller's `set_reference_state` changes h and s for every state constructed afterwards in the process; existing states keep their copies. | Immutable `Arc<FluidModel>`; `ReferenceState` is a value; user fluids go into an explicit registry overlay |
| R5 | Every state deep-copies the model several times | `FluidLibrary.h:1323-1331`; `HelmholtzEOSBackend.h:50,53`; `HelmholtzEOSMixtureBackend.cpp:114, 141-149`; 78–126 KiB per state | Poor scaling with many concurrent states | Share the `Arc`; per-state caches only |
| R6 | Load failures swallowed; orphan names in FluidsList | `FluidLibrary.cpp:57-61, 167`; fixed after 8.0.0 in `51e8c60f` | The library can come up partial, with no error | Validate at build time so shipped data cannot fail; `Result` for user data |
| R7 | No schema for fluid files; checks are `assert`s; unknown types accepted | Asserts at `FluidLibrary.h:54-56, 1182-1186`, `CoolPropFluid.h:450-453`; **wheel imports no `__assert_fail`**. Unknown α⁰ type printed and ignored (`FluidLibrary.h:327-329`). Any ancillary `type` other than `rational_polynomial` or `rhoLnoexp` becomes "exponential" (`FluidLibraryFactories.h:56-60`). That default is also how the legitimate types `rhoV`, `pL` and `pV` are dispatched, so a typo is accepted silently. A missing `Tmin`/`Tmax` on rational polynomials is hidden by `catch(...)` (`:52`). Later out-of-bounds / UB fixes: `427c8aeb`, `681472bf`, `ae54172f`, `538d4227`, `0f978943` | Malformed (user) JSON gives UB or silently wrong properties | serde closed `enum`s at datagen; arrays-of-structs make length mismatches unrepresentable |
| R8 | `Ttriple` / `ptriple` / `T_min` are conflated; `EOS.Ttriple` is never read | `FluidLibrary.h:383-386` (`\todo`). 16 fluids have JSON `Ttriple` ≠ T_min. Oracle: CycloPropane 273 vs 145.7, Propyne 273 vs 170.5, R124 120 vs 75, R21 200 vs 142.8 | Wrong physical answers | Separate `t_min` and `t_triple: Option`; a parity shim reproduces v8 |
| R9 | Literature-mismatched and duplicated constants | `2acbbc82`: reducing ρ of Nitrogen, Ethylene (+M), OrthoHydrogen (+M) and n-Undecane were rebuilt through unit conversion; SA refit in `66859efb`. 28 fluids have `STATES.critical` ≠ `reducing` by only 1–20 ULP. | Oracle ρ_r wrong by 1.3e-7 (Nitrogen) to 3.0e-5 (OrthoHydrogen) relative for 4 fluids; duplicated constants drift | Store each published constant once, verbatim; known-defects list with citations |
| R10 | Dead and stale data | 23 alternate EOS entries (108 KB) parsed but unused; R1234yf's alternate SA built at load. `.json_disabled` since 2014 (`fd993bd3`). `old_BIP.json`. `check_points` loaded at runtime but used only by tests (`superancillary.h:868, 1108`). 48 orphan binary pairs; 7 duplicate mixtures. | Parse cost; misleading lists | Datagen drops or flags them; fixtures move to tests |
| R11 | Predefined mixtures are never checked against pair coverage | 31/147 fail to construct in the oracle (27 missing pairs at `MixtureParameters.cpp:589-591`; 4 need R1132a); `inject_ASHRAE_2026.py:13-26` | The advertised list includes unusable names | Datagen cross-reference check; explicit fallback mixing rule per entry, or exclusion |
| R12 | Cubic table: a committed derived copy produced by running CoolProp in 2016 | `59399842` (2016-11-29); `2acbbc82` calls it a file the build "never regenerates". The generator is broken: `generate_cubics_listing.py:16` opens `NAME + '.json'`, which fails for R1224YDZ, and `:18` splits on `', '` while the delimiter is `','`. 14 fluids missing; 21 drift by up to 8.3 %. | DRY violation; `PR::` / `SRK::` diverge from HEOS constants | Derive cubic constants from the HEOS model at load; keep the v8 table only as a parity fixture |
| R13 | Fragile, non-reproducible build pipeline | `generate_headers.py`: <br>• `:374` with `:357-363`: a generator is consumed, so the cache stores `[]` and deleted fluids go undetected; <br>• `:425`: a tuple is always truthy, so incompressibles always repack; <br>• `:316`: undefined `rev`; <br>• `:389`: unsorted glob, so FluidsList order depends on the host. <br>The generator-consumption bug was fixed after 8.0.0 (`e47acc2e`, `list(sources)`); `:425`, `:316` and `:389` are unchanged on master. <br>`.incbin` is invisible to compiler depfiles (*inference*); `CMakeLists.txt:2354` records stale embedded data, though its comment still says "zlib-compressed". The legacy `package_json.py` (not invoked by the build) writes top-level `ENVIRONMENTAL` at `:326`, while the loader reads `INFO.ENVIRONMENTAL` (`FluidLibrary.cpp:221-227`); `:344` and `:385` read directories that do not exist. About 12 scripts mutate sources in place. | Stale or non-reproducible binaries; no audit trail from paper to data | `cargo` + `include_bytes!` (tracked); deterministic, sorted datagen; sources never mutated, corrections are an overlay with citations |
| R14 | The wheel ships a 27.9 MB hex copy of the data | `wrappers/Python/CMakeLists.txt:453-461` excludes `*_JSON.h` / `*_JSON_z.h`, but not `*_CBOR.h` (still so on master). The `gall_fluids_CBOR*` symbols are exported in `.dynsym`. | 73 % of the installed package is dead weight | Ship one binary dataset; a `cargo package` content test |
| R15 | Side-data validation is fail-open or out of CI | `PCSAFTLibrary.cpp:31-46`: a schema failure throws only if the debug level is above 0, and `add_many` errors are printed (`:39`); still so on master. `validate_fluid_schemas.py:36-39` returns 0 on SKIP. The schemas run only in local `dev/ci/preflight.sh:385-405`, in no GitHub workflow. Shipped data is re-validated at runtime (`CubicsLibrary.cpp:133-146`). | Bad user data silently ignored; startup cost | Validate once in CI datagen; typed parse with `Result` for user input |
| R16 | Identifier semantics differ by backend | CAS overloaded in 10 fluids. File stem ≠ NAME. HEOS case behaviour depends on hand-listed aliases (`FluidLibrary.cpp:344-359`). PC-SAFT rejects `Methane`; PR rejects CAS. Comma-joined lists are ambiguous (`1,2-dichloroethane`); only HEOS offers the pipe-joined `aliases_bar`. The `"N/A"` sentinel versus an `.empty()` check (`REFPROPMixtureBackend.cpp:362`) *likely* passes `N/A` to REFPROP for 9 fluids (*inference*). | Surprising lookup failures; divergence between backends | One shared resolver: case-folded keys (0 collisions in 556), `Option<Cas>`, list outputs as slices |
| R17 | Sentinel values in the environmental data | ODP −1 in 100 fluids, so `keyed_output(iODP)` throws "ODP value is not specified or invalid" for R134a (an HFC, ODP 0). GWP100 −1 in 59. ASHRAE34 `UNKNOWN` (67) or `?` (13). 11 fluids have no `ENVIRONMENTAL` at all. | Unusable or misleading metadata | `Option<f64>` with source; never a sentinel |
| R18 | Name-keyed and constant hacks | `FluidLibrary.cpp:91` (Water and CO₂ ×1.00001); `FluidLibrary.h:1257` (hard-coded R) versus `:1277`; `-SRK` cubic-library path with an unassigned `R_u` (oracle reads 0; fixed in `0f978943` and `ae54172f`) | Hidden special cases | Explicit data flags or enum variants |
| R19 | Duplicate JSON key | `dev/fluids/Chlorine.json:3796-3797`: `source_eos_hash` appears twice (removed incidentally in `9b35f538`) | Lenient parsers keep the last value silently; **serde-derived structs reject the v8.0.0 file** | Datagen detects duplicates and pre-cleans with a logged waiver |
| R20 | Licence notices missing from the v8.0.0 wheel | `dist-info/licenses/` holds only `LICENSE`, yet the wheel bundles Eigen (MPL-2.0), Valijson (BSD-2), nanobind (BSD-3), NIST SA code and a Beer-ware spline. Fixed by `master:THIRD_PARTY_NOTICES.md` (`f23e69cb`), which covers code only: no data provenance. | Compliance gap | Generated notices (`cargo about` or `cargo deny`) plus a `DATA-NOTICE` with per-dataset provenance |

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

- **Loading is decoding, not compute.** With a plain f64 format a fluid materialises in microseconds: about 22 KiB, essentially a `memcpy`, because extrema are precomputed. Parallel loading is unnecessary. Datagen (136 fluids, about 2 s of extrema work in Python) is embarrassingly parallel but fast enough run sequentially.
- **Concurrency comes from immutability.** A materialised fluid is read-only behind `Arc`, so it is `Send + Sync` with no locks.
  - CoolProp takes a mutex on every inverse-SA access (`superancillary.h:1078-1085`) and mutates shared fluids.
  - The port avoids both: build the inverse in datagen, or behind a per-fluid `OnceLock`.
- **SIMD-friendly facts in the data:**
  - **SA.**
    - Every piece is degree 12, so Clenshaw has a fixed trip count.
    - **One breakpoint array serves p, ρ′ and ρ″ in 130/130 fluids.** A single interval search therefore feeds three evaluations: one `f64x4` Clenshaw with lanes p, ρ′, ρ″ and a pad.
    - For batches over many T, bin inputs by piece and vectorise across T.
    - Pieces are dyadically refined near T_c, so piece lookup should be a branch-free search over a short sorted array, not a uniform grid.
  - **αʳ.**
    - Power terms: 0–51 per fluid (median 12), with integer `d ≤ 15` and `l ≤ 6` (47 % have `l = 0`). Only 38 % of `t` values are integral.
    - Gaussian terms: 2–14 (median 5) in 78 fluids.
    - Layout: structure-of-arrays per term family, with `n` and `t` as f64 and `d` and `l` as small integers.
    - Grouping by `l` lets one δ^l and one exp(−δ^l) serve many terms.
    - *Inference:* lanes across states (the same fluid, many (τ, δ)) beat lanes across terms, because a median of 12 terms fills only 3 AVX2 iterations.
- **Branchy or sequential parts** are all offline or once per fluid:
  - identifier resolution;
  - JSON import and validation;
  - mixture lookup per binary pair;
  - ancillary and transport type dispatch (a closed `match`);
  - extrema root-finding (datagen).
- **Side-by-side kernels (user goal).**
  - Keep the dataset *layout-neutral*: contiguous per-variable arrays, documented endianness and alignment.
  - Let each kernel (scalar, SIMD width N, a future GPU) derive its packed view once per fluid, cached next to the `Arc<FluidModel>`.
  - Examples: interleaved `[piece][13][4]` SA coefficients, or term arrays padded to the lane width.
  - This keeps data concerns separate from architecture concerns and lets unsuitable algorithms stay scalar.

## 8. Verification assets (tests, check values, paper tables, oracle hooks)

- **Data identity.**
  - Wheel blob sha256 `b60ca6fcc4945438…`, 4,648,555 B.
  - All 136 embedded records equal `dev/fluids` at v8.0.0, including int-versus-float literal kind; `cbor_min` re-encoding is byte-identical.
  - Oracle hook: `get_fluid_param_string(f, "JSON")`.
  - Round-trip test: parse into the Rust model, compare every number bit for bit, then compare against the oracle's JSON.
- **SA freshness contract.** The FNV-1a fixture is `8e75626511d00b5c` (`CoolProp-Tests.cpp:3788`). All 130 stamped `source_eos_hash` values match when recomputed here. In CI, the pinned-release check runs in `dev_checks.yml:608-637`.
- **Extended-precision saturation `check_points`.**
  - 3 per fluid × 130, at Θ = 0.5 / 0.3 / 0.1, from fastchebpure multiprecision (`inject_superanc_check_points.py:14-31`).
  - The C++ test is `CoolProp-Tests.cpp:3834-3868`: tolerance 4 × |SA/mp − 1| with a 1e-14 floor. It reads the data through the JSON echo.
  - **For p_sat, ρ′ and ρ″ this is a better oracle than CoolProp itself.** Move it to test fixtures.
- **Encoding tests.** The `[cbor]` test is `src/Tests/CoolProp-Tests-CBOR.cpp:17`. `dev/check_cbor_min_vs_cbor2.py` runs in CI (`test_catch2.yml:108`).
- **Side schemas.** All 3 pass at v8.0.0 (re-run here: 180 PC-SAFT, 116 cubic, 28 departure functions). They are reusable as fixtures for user-JSON import.
- **Oracle metadata endpoints:**
  - `FluidsList` (136, compare as a set);
  - `predefined_mixtures` (294 = 147 × 2 case variants);
  - `get_fluid_param_string` for `aliases`, `CAS`, `REFPROP_name`, `formula`, `pure`, `BibTeX-*`, `INCHI` and `SMILES`;
  - `Props1SI` for `Tcrit`, `Ttriple`, `molemass` and `GWP100`.
- **Primary-source transcriptions** for arbitration:
  - `dev/mixtures/KunzWagner2012_TableA6/A7/A8.txt` (GERG-2008);
  - `Bell2016Coefficients.txt`;
  - the Mulero 2012 and 2014 tables (`package_json.py:8-88, 213-249`);
  - `CoolPropBibTeXLibrary.bib` (194 keys used).
  - On master, GERG data move into C++ headers and are verified against teqp (`master:dev/gerg/README.md`, `verify_transcription.py`).
- **Known oracle defects to encode as expected deviations, each with its citation:**
  - R8 (`Ttriple`).
  - `2acbbc82` (4 reducing densities) and the `66859efb` SA refit.
  - 31 unusable predefined mixtures.
  - Cubic table drift.
  - ODP sentinels.
  - INCOMP placeholders and PCL ×100 (`9f1b088c`, `25c4f3a5`).
  - The `-SRK` R_u = 0 path (`ae54172f`).
  - After 8.0.0:
    - `14da1f0d`: R1233zd(E) viscosity restored;
    - `767b1b3b`: unverifiable ECS entries removed;
    - `520b8809`: citation fixes;
    - `3a052553`: `C2H6` alias;
    - `93a41972`: D₂O IAPWS transport;
    - `c7d6a1aa`: GERG reducing-function NaN when two mole fractions are 0.

## 9. Port recommendation (units with priority P0-core / P1-early / P2-later / defer / drop; order; what to redesign)

| Unit | Priority | Replaces (approx. C++/Python lines) | Design |
|---|---|---|---|
| D1 `fluid-model` types | **P0** | `CoolPropFluid.h` model structs; `FluidLibrary.h:42-457` (~600) | Immutable plain structs and enums: `Identity`; `Eos{r_u, m, reducing, limits{t_min, t_triple: Option}, alpha0: Vec<Alpha0Term>, alphar: ResidualTerms}`; `SatStates`; `Superancillary`. No sentinels; integer exponents; units normalised. |
| D2 `datagen` (xtask, not shipped) | **P0** | `generate_headers.py`, `cbor_min.py`, `package_json.py`, `eos_fnv1a_hex` (~1,100) | Reads the pinned v8.0.0 JSON with a literal-kind-preserving parser and rejects duplicate keys except the logged Chlorine waiver. Strict typed validation; units checked, then dropped. Cross-reference checks for mixtures and pairs. Recomputes the FNV gate, **precomputes SA extrema, intervals and (optionally) inverse tables**, sorts deterministically, and emits the versioned binary plus the index. |
| D3 Binary dataset format and reader | **P0** | `json.h` get-helpers + CBOR decode (~240) | Little-endian, 8-byte-aligned sections; `f64::from_le_bytes` (optionally zero-copy). Versioned header and checksum. `no_std`-friendly, **no dependencies**. About 22 KiB per fluid; 2.8–3.0 MiB for all fluids. |
| D4 `Registry` + identifier resolver | **P0** | `FluidLibrary.cpp:28-62, 150-394`; `FluidLibrary.h:1229-1337` (~450) | A static sorted table, or a build-time perfect hash, of 556 case-folded keys → `FluidId(u16)` → `OnceLock<Arc<FluidModel>>`. The same resolver serves every model family through availability flags. No global mutation. |
| D5 Data crate + embedding + `DataSource` | **P0** | incbin, hex headers, CMake targets (~120) | `include_bytes!` (cargo-tracked). Cargo features select fluid subsets. A `DataSource` trait reads embedded bytes, a file, or a fetch (WASM). No compression by default, since f64 gains only 13–19 %; use HTTP brotli/zstd on the web. |
| D6 SA dataset section | **P0** | `superancillary.cpp`; SA constructor parts (~150) | One breakpoint array, 3 coefficient arrays `[piece][13]`, `crit_anc`, and the precomputed extrema (7 in total) and intervals. `check_points` go to test fixtures, not the runtime dataset. |
| D7 Transport, σ and melting data | **P1** | `FluidLibrary.h:459-1070` (~610) | Closed enums; `Hardcoded(Id)` checked at build (05-transport) |
| D8 Mixture data | **P1** | `MixtureParameters.cpp` loaders (~400); 3 JSON files | Binary pairs keyed by an ordered `(Cas, Cas)`. Predefined mixtures validated against the registry and pair coverage. Duplicates and orphans reported. |
| D9 Dataset IDs, corrections overlay, known defects, citations API | **P1** | `2acbbc82`-style fixes; the bib | `DatasetId::CoolProp800` (bit-exact oracle data) versus `Corrected`, where every change carries a citation. Citations exposed per model part. |
| D10 Licence and notice bundle | **P1** | `LICENSE`, `master:THIRD_PARTY_NOTICES.md` | See below |
| D11 Cubic, PC-SAFT and incompressible data | **P2** | `CubicsLibrary.cpp`, `PCSAFTLibrary.cpp`, `IncompressibleLibrary.cpp` loaders (~1,150) | Same pipeline. Cubic constants derived from HEOS records; the 2016 table kept only as a parity fixture. Incompressible fits checked for placeholders and all-zero blocks at datagen. |
| D12 CoolProp-JSON import/export | **P2** | `add_fluids_as_JSON`, `get_fluid_param_string(...,"JSON")` | Optional `serde_json` feature. User fluids go into a `Registry` overlay built by a builder, never into global state. A JSON Schema is generated from the Rust types. |
| D13 Alternate EOS selection, environmental metadata, URLs / ChemSpider | **defer** | 23 `EOS[1]` entries; `INFO.ENVIRONMENTAL` | Keep them in the source dataset; do not load by default. Resolve provenance first. |
| D14 Drop | **drop** | CBOR, incbin, hex headers, miniz/msgpack for data, nlohmann/Valijson at runtime, `JSONstring_map`, `superancillaries_str`, runtime schema checks, env-var and macro switches, `OVERWRITE_FLUIDS`, injection-in-place scripts, `.json_disabled`, `old_BIP.json`, `LIST_STRING_DELIMITER` | Replace the switches with an explicit `LoadOptions { superancillary: bool, … }` |

**Order.**

1. D1 → D2 → D3 → D6 → D4 → D5, with a 5-fluid TDD set: Water, Nitrogen, CO₂, R134a and Propane. Together they cover non-analytic terms, Gaussians, density-maximum extrema, a `2acbbc82` fluid, and ancillaries.
2. Then all 136 fluids.
3. Then D7.
4. Then D8–D10.
5. Then P2.

**Shape (sketch).**

- `Registry::builtin().get("r134a")? -> Arc<FluidModel>`.
- `Superancillary { t_breaks: Box<[f64]>, coef: [Box<[[f64;13]]>; 3], extrema: Box<[f64]>, crit: CritAnchor }`.
- `ReferenceState` is passed to state constructors, never stored in the fluid.

**Licensing and attribution for coolprop-rs** (facts plus recommendations; not legal advice):

- **CoolProp notice.**
  - The data and the translated algorithms derive from CoolProp, which is MIT-licensed: "Copyright (c) 2012-2018 Ian H. Bell and other CoolProp developers" (`LICENSE:3`).
  - Ship that notice in `LICENSE-THIRD-PARTY` and inside the data crate. MIT is compatible with a permissive coolprop-rs licence.
- **Ported SA code.** If the SA code is ported, keep the **NIST disclaimer**, which must appear in all copies (`superancillary.h:1-28`).
- **Spline.** Re-implement the Beer-ware spline clean-room (`master:THIRD_PARTY_NOTICES.md:1172-1181`).
- **Provenance to clear before redistribution:**
  - the Ethanol–Water parameters ("From REFPROP 9.1 with permission");
  - the DTU environmental table: `dev/environmental_data_from_DTU/build_DTU_JSON.py` reads REFPROP `.fld` files and states no source or licence;
  - the licence of fastchebpure outputs;
  - incompressible manufacturer-sheet fits;
  - after 8.0.0, the ATcT `HFORMATION` data (`9b35f538`).
- **Standards tables.** CoolProp's own maintainers treat edition-locked standard tables (ISO 6976) as "copyright-encumbered against an MIT repo" (`9b35f538`). Do not embed normative standard tables. Equations and coefficients from papers and IAPWS releases are cited, not copied.
- **Scientific attribution.**
  - Ship BibTeX keys per model part, plus the bib.
  - Add the 3 missing keys and a key for the multiparameter SA.
  - Expose citations through the API.

## 10. Open questions

1. **Default dataset.** Bit-exact v8.0.0 (oracle parity, including its known errors) or literature-corrected? Proposal: both, as explicit `DatasetId`s, with parity tests pinned to v8.0.0 and corrections cited against the primary literature.
2. **Triple-point parity.** Should parity mode reproduce v8's `Ttriple = T_min` and `triple_liquid = sat_min_liquid`, while the physical triple point is exposed separately?
3. **Cubic parity.** For `PR::` / `SRK::`, keep the 2016 cubic table for parity, or derive from HEOS and record the deviations (up to 8.3 %)?
4. **Licence clearance.** REFPROP-permission Ethanol–Water, the DTU environmental data, fastchebpure outputs, manufacturer incompressible data, and ATcT (post-8.0.0). Who signs off?
5. **SA extrema and inversion.**
   - May precomputed extrema (numpy/Rust root-finding) differ from CoolProp's Eigen ones in the last bits? This needs a tolerance policy for saturation inversions.
   - Should the 5 spurious extrema be kept for parity or dropped?
   - Should the inverse T(p) be precomputed (roughly doubling SA size) or built per fluid on first use?
6. **WASM packaging.** Embed all 136 fluids (about 2.8 MiB f64), or ship per-fluid fetchable files plus a curated default subset?
7. **Alternate EOS.** Are the 23 entries needed (for example Richter 2011 for R1234yf, Colonna for siloxanes)? If so, select them by citation key?
8. **Case-insensitivity.** Case-folding is collision-free today, so should all identifiers be case-insensitive? Parity tests should compare successes, not failure sets.
9. **Schema namespacing.** How should the schema be namespaced and versioned for materials and phases beyond fluids? Proposal: a substance identity layer (CAS / InChIKey) plus one dataset per model family, without designing solids now.
10. **Expression variant.** master stores transport correlations as a DSL in JSON, and GERG as C++ headers. Should the Rust schema reserve an `Expression` variant now, or wait until 05-transport decides?
11. **Unusable predefined mixtures.** For the 31 mixtures, omit them, mark them unavailable, or supply a documented default mixing rule? The oracle offers none, so parity means "error".

## Verification log

- **Date:** 2026-10-04 (adversarial verification against the v8.0.0 checkout `ae81610e` and the CoolProp==8.0.0 wheel).
- **Document state found:** complete. There was no partial edit, broken table, duplicated section or earlier verification log to repair.
- **Claims checked:** 214, as follows.
  - **Code citations:** every `path:line` citation was opened and checked.
  - **Commits:** all 23 cited commits were checked for their v8.0.0 ancestry and their message bodies.
  - **Counts re-run with independent scripts:**
    - file sizes, byte shares, SA shape and breakpoints;
    - term and type counts, sentinels, identifiers, BibTeX keys;
    - mixtures, cubic drift, PC-SAFT and incompressibles;
    - FNV hashes (130/130), int and float literals, and the 7 interior extrema via numpy.
  - **Oracle re-runs:**
    - blob sha256 and offset in the `.so`, and the compressed sizes;
    - import and first-load time and RSS, with and without SAs (1.94 s / +67.7 MiB; 171 ms / +56.5 MiB);
    - the `Ttriple`, `iODP`, INCOMP and PC-SAFT/PR lookups;
    - the 31/147 failing predefined mixtures.
  - **Not re-measured:** the per-state KiB figures, the side-dataset first-use timings and the per-fluid re-add timings.
- **Rot items:** all 20 survived refutation, some with narrowed wording. R6, R13 (partly), R19 and R20 are fixed after 8.0.0, but they stay because the oracle is v8.0.0.
- **Corrections:**
  1. Scope LOC: from "about 4,500" to about 4,400 (4,372 measured).
  2. Hex header share: from "73 % of the 38 MB package" to 71.5 % of 39.0 MB (37.2 MiB).
  3. Headline on validation: the side schemas also run at runtime for cubic and PC-SAFT, not "only by preflight".
  4. §1: `set_reference_state.py` does not mutate sources. It was replaced by `inject_InChI.py` as an example.
  5. §3 and R18: the `-SRK` cubic-library `R_u` was *uninitialised* at v8.0.0, not "set to 0". The oracle reads 0.0. A default was added in `0f978943` and a proper value in `ae54172f`. Only R1233ZD(E) reaches this path.
  6. §3: the literal-kind counts apply to EOS[0] *minus* `SUPERANCILLARY`.
  7. §4.4 PC-SAFT: 26 ions, not 10. The 10 are the mixed-case ions; 170 names are all-uppercase. 222 aliases.
  8. §4.5 and R16: PC-SAFT also indexes aliases and `upper(alias)`. HEOS offers a pipe-joined `aliases_bar` that removes the list ambiguity.
  9. gzip-9 size: 3,075,416 B with CLI `gzip -9`. The header bytes vary.
  10. R7: the "exponential" default is also the dispatch for the legitimate types `rhoV`/`pL`/`pV`. `catch(...)` covers only a missing `Tmin`/`Tmax` on rational polynomials.
  11. R9: the reducing-density error ranges from 1.3e-7 to 3.0e-5 relative, not "1e-5 level".
  12. R4: a reference-state change affects states constructed afterwards; existing states hold copies.
  13. R13: the generator-consumption bug was fixed after 8.0.0 in `e47acc2e`, while `:425`, `:316` and `:389` persist on master. `package_json.py` is legacy and not invoked by the build.
  14. §5: the WASM heap-cost statement is now labelled as an inference.
