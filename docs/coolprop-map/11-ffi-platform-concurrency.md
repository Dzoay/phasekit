# 11 Wrappers, C ABI, WASM, platform code, concurrency and memory - CoolProp v8.0.0 map

> Scope: `wrappers/Rust/` (235 lines), `wrappers/Javascript/` (259), `src/emscripten_interface.cxx` (348), `src/nanobind_interface.cxx` (1,201), `src/CoolPropLib.cpp` (1,164), `include/CoolProp/CoolPropLib.h` (856), `src/CoolPropLib.def` (47), `include/CoolProp/FPUGuard.h` (91), `include/CoolProp/detail/{PlatformDetermination,filepaths,atomic_write,state_capi}.h` (157), `src/CPfilepaths.cpp` (331), the JS block `CMakeLists.txt:2055-2098`, and a second in-tree Rust consumer, `wrappers/GUI/src-tauri/` (627). About 5,300 lines. It also includes a whole-tree audit of global, static, `thread_local`, mutex and cache state in `src/` and `include/`, which follows references into about 20 more files. Part of the coolprop-rs port plan; cites the v8.0.0 source.

Conventions:
- `path:line` cites v8.0.0 (ae81610e).
- "Oracle" is the CoolProp==8.0.0 wheel (nanobind, manylinux2014 x86_64). Every oracle number below was measured for this document.
- "Inference" marks claims that were not demonstrated directly.
- Sibling maps: 01 (API/state), 03 (flash), 07 (humid air, REFPROP), 08 (tabular), 09 (data), 10 (tests). Their rot IDs are cited as "01 R23" etc. and are not repeated here.

## 1. Purpose and concepts

This area is CoolProp's boundary: every route by which a non-C++ program reaches the library, the platform shims beneath those routes, and the process-wide state they share. It contains no thermophysics. It does decide whether the thermophysics can run concurrently, how much memory it costs, and what a drop-in replacement must reproduce.

| Concept | v8.0.0 realisation | Where |
|---|---|---|
| C ABI | 71 `extern "C"` exports. Strings are NUL-terminated `char*`; arrays are `double*` plus a `long` length. Errors come back either as `long* errcode` plus a caller-owned message buffer, or as a `_HUGE` return plus the global `errstring` | `include/CoolProp/CoolPropLib.h`, `src/CoolPropLib.cpp` |
| Handle | A `long` key into a process-global `std::map<size_t, shared_ptr<AbstractState>>` | `CoolPropLib.cpp:478-510` |
| Export macros | `EXPORT_CODE` (`extern "C"`, plus `__declspec(dllexport)` on Windows) and `CONVENTION` (defaults to `__stdcall` on Windows). There are 32-bit stdcall/cdecl build variants and an opt-in `.def` alias file | `CoolPropLib.h:22-74`, `CMakeLists.txt:590-635`, `src/CoolPropLib.def` |
| FP guard | RAII. On entry it masks all IEEE exceptions. On exit it clears the status flags and restores the caller's mask | `include/CoolProp/FPUGuard.h:29-87` |
| Error outbox | One process-global `errstring` and one `warnstring`, guarded by a mutex and drained by `get_global_param_string("errstring")` | `src/CoolProp.cpp:64-106, 1047-1056` |
| Python | nanobind module that maps `CoolPropBaseError` to `ValueError`. Adds a State C-ABI capsule: a `void*` handle with a `thread_local` last error | `src/nanobind_interface.cxx:128-271, 394-400, 1193-1198`; `detail/state_capi.h:29-55` |
| JS/WASM | emscripten + embind ES6 module (`coolprop.js` + `coolprop.wasm`). `factory` returns a raw pointer that needs a manual `.delete()` | `src/emscripten_interface.cxx`; `CMakeLists.txt:2055-2098` |
| Rust consumers | (a) `coolprop-rs` 0.2.0: bindgen over `CoolPropLib.h`, linked against a system `libCoolProp`. (b) Tauri GUI: a hand-written `extern "C"` block over a CMake-built static library | `wrappers/Rust/`, `wrappers/GUI/src-tauri/src/` |
| Platform shims | `__ISWINDOWS__`/`__ISAPPLE__`/`__ISLINUX__`/`__ISPOWERPC__`; home directory; atomic write; `thread_local` macro'd away under emscripten | `detail/PlatformDetermination.h`, `src/CPfilepaths.cpp`, `detail/tools.h:22-24` |
| Concurrency model | "One AbstractState per thread". Singletons load under `std::call_once`. Everything else is unsynchronised or was retro-fitted | sec. 5 |

Headline findings:
1. **There is no shared immutable model.** A pure HEOS state deep-copies its fluid **6 times**, and every copy carries mutable evaluation caches.
   - Measured: 148 KiB malloc per HEOS Water state.
   - The 21 ECS-transport fluids reach about 300 KiB after their first transport call.
   - The oracle hides another ~400 KiB per state that every non-COW `std::string` platform pays: Windows, macOS, WASM.
2. **Thread-safety was retro-fitted.** It came only after the in-tree Rust GUI's parallel `cargo test` runner exposed races (16 of 17 tests failed; `0f639eab`, gh-2787).
   - v8.0.0 still has races that are fixed only on master: lookup tables, config, tabular, REFPROP.
   - It also has unfixed ones (sec. 5a).
3. **The C ABI has memory-safety holes beyond 01 R23:**
   - two string outputs are bounded by the *message* buffer's length;
   - the handle reference escapes its lock;
   - `long` handles overflow on Windows;
   - the stdcall `.def` file is stale.
4. **Errors pass through a process-global outbox that the library itself drains** (`Props1SI`). Some failures throw an `int`, which no layer can translate. Oracle: an empty `ValueError`, or a `SystemError`.
5. **The WASM build works but embeds and eagerly decodes all 136 fluids.** That is 4.65 MB of CBOR, 84 % of it superancillaries. The toolchain is pinned to emcc 4.0.12.
6. **The Python oracle holds the GIL for every call.** With 4 threads the speedup is 0.97x. Concurrency must therefore be verified natively, not through the oracle.

## 2. Structure (key types/functions -> path:line)

| Item | Role | path:line |
|---|---|---|
| `str2buf` | Copies a string into a caller buffer; throws if it is too small | `CoolPropLib.cpp:25-32` |
| `HandleException` | Rethrow-dispatch. `HandleError`/`CoolPropBaseError` give errcode 1 plus a message (2 if the message does not fit). Anything else gives **3 and no message** | `CoolPropLib.cpp:33-55` |
| `fpu_reset_guard` | Alias of `CoolProp::fpu_guard`. Constructed in 51 of the 71 exports | `CoolPropLib.cpp:64` |
| kSI layer | `convert_from_kSI_to_SI` / `_SI_to_kSI`: ×1000 for P, H, S, U, G, Cp, Cp0, Cv, λ | `CoolPropLib.cpp:65-120` |
| `AbstractStateLibrary` | `add` / `remove` / `get` under one `std::mutex`. `get` returns `shared_ptr&` | `CoolPropLib.cpp:478-509` |
| `handle_manager` | The static instance | `CoolPropLib.cpp:510` |
| Handle exports (36) | Pattern: `*errcode = 0`, guard, `get(handle)`, call, `catch(...)` → `HandleException` | `CoolPropLib.cpp:512-1143` |
| Batch exports | `update_and_common_out` / `_1_out` / `_5_out`: serial loop with a per-point `catch(...){}` | `CoolPropLib.cpp:784-862` |
| `.def` aliases | 44 undecorated stdcall names | `src/CoolPropLib.def:1-47` |
| State C-ABI | 9-entry function table: `make`, `destroy`, `update`, `keyed_output`, `first_partial_deriv`, `last_error`, `set_mole_fractions`, `specify_phase`, `unspecify_phase` | `state_capi.h:29-55`; impl. `nanobind_interface.cxx:128-271` |
| nanobind module | Exception translator (`CoolPropBaseError` only, `:394-400`); `_capi` capsule (`:1197`). There is no GIL release anywhere in the file | `nanobind_interface.cxx` |
| embind module | Marshalling helpers `:35-50`. Free functions `:55-62, 225, 235`. Enums `:64-221`: 86 parameters, 43 input pairs, 9 phases, 13 backend families. `AbstractState` class with 75 methods `:237-345` | `emscripten_interface.cxx` |
| JS build/test | Flags `CMakeLists.txt:2068-2079`; Docker pin `wrappers/Javascript/Dockerfile:3-12`; test `test_wasm.mjs:1-215` | |
| Rust crate | `build.rs:9` (link `CoolProp`), `:17-27` (bindgen); `lib.rs:13-48` (error type), `:50-100` (PropsSI/HAPropsSI), `:102-122` (tests) | `wrappers/Rust/` |
| GUI FFI | `coolprop_ffi.rs:6-63` (extern block), `:69-78` (`check`), `:80-178` (wrappers); `state_manager.rs:13-56` (global handle table); `build.rs:6-44` | `wrappers/GUI/src-tauri/` |
| Platform | `FPUGuard.h:29-39` (select), `:46-63` (ctor), `:68-79` (dtor); `PlatformDetermination.h:4-17`; `CPfilepaths.cpp:87-99` (binary read), `:115-163` (atomic write), `:226-266` (`get_home_dir`), `:319-331` (text read) | |

C ABI inventory (`CoolPropLib.h`), 71 exports:

| Group | n | Members | In-tree consumers |
|---|---|---|---|
| High-level SI | 8 | `PropsSI`, `PropsSImulti`, `Props1SI`, `Props1SImulti`, `PhaseSI`, `HAPropsSI`, `saturation_ancillary`, `cair_sat` | `PropsSI` appears in 19 of 21 wrapper directories |
| Legacy kSI / char-keyed | 4 | `Props`, `PropsS`, `Props1`, `HAProps` | Excel- and EES-era code |
| Fortran pointer-argument | 3 | `propssi_`, `hapropssi_`, `haprops_` | Fortran |
| Lookup / utility | 10 | `get_param_index`, `get_input_pair_index`, `get_global_param_string`, `get_parameter_information_string`, `get_fluid_param_string(_len)`, `C_is_valid_fluid_string`, `C_extract_backend`, `K2F`, `F2K` | |
| Global mutation | 10 | `set_config_string/double/bool`, `set_departure_functions`, `set_reference_stateS/D`, `add_fluids_as_JSON`, `redirect_stdout`, `set/get_debug_level` | |
| Handle API | 36 | `AbstractState_factory/free/update/keyed_output/...` | Fortran, Julia, GUI; master adds Mathcad (`afce86ff`) |

## 3. Algorithms and formulas

No thermophysics lives here; the humid-air caches touched in sec. 5 belong to 07. The boundary "algorithms" and their sources:

| Item | Definition | Where | Source |
|---|---|---|---|
| Fahrenheit | T_K = (T_F + 459.67)·5/9; T_F = 9/5·T_K − 459.67 | `CoolPropLib.cpp:325-330` | Exact definitions (NIST SP 811, App. B) |
| kSI layer | ×1000 for kPa, kJ/kg, kJ/(kg K), kW/(m K) | `CoolPropLib.cpp:65-120` | The CoolProp 4 API |
| FP masking | Snapshot the control word or enabled set; mask all; on exit clear the flags and restore | `FPUGuard.h:46-79` | IEEE 754-2019 §7; C99 `<fenv.h>`; glibc `fegetexcept`/`fedisableexcept`; MS CRT `_controlfp_s` |
| Atomic replace | Write `<target>.tmp.<64-bit salt>.<seq>`, optionally chmod 0600, then `rename` | `CPfilepaths.cpp:115-163` | POSIX `rename(2)`; Win32 replace-existing |
| Handle allocation | Monotonic `long next_handle`, then `std::map::insert` | `CoolPropLib.cpp:482-491` | none |
| JS marshalling | One JS call per element (`val::push`, `arr[i].as<double>()`) | `emscripten_interface.cxx:35-50` | embind |
| Data embedding | JSON → CBOR (RFC 8949) → `.incbin`, or a hex array under MSVC/emscripten | `FluidLibrary.cpp:8-23` | See 09 |

## 4. Data and configuration inputs

| Input | Values / effect | Where |
|---|---|---|
| Library kind | `COOLPROP_SHARED_LIBRARY` / `STATIC` / `OBJECT` (mutually exclusive) | `CMakeLists.txt:35-38, 637-641` |
| Calling convention | `COOLPROP_STDCALL_LIBRARY` / `CDECL` apply to 32-bit only and only warn on 64-bit; `COOLPROP_EXTERNC_LIBRARY` | `CMakeLists.txt:590-635, 819-823` |
| `COOLPROP_LIB` | Switches on `extern "C"` and `dllexport` | `CoolPropLib.h:38-60`; `CMakeLists.txt:732` |
| `COOLPROP_LIBRARY_EXPORTS` | Optional `.def`. CI does not pass it: "exports are opt-in via src/CoolPropLib.def" | `CMakeLists.txt:611-613, 705`; `.github/workflows/library_shared.yml:70` |
| Windows packaging | `/MT` static CRT; `DEBUG_POSTFIX d`; no `lib` prefix; VERSIONINFO `.rc`; output folders `32bit__stdcall` and `64bit__arm64` | `CMakeLists.txt:694-745` |
| JS build | `-sDISABLE_EXCEPTION_CATCHING=0`, `-DCOOLPROP_NO_INCBIN`; link `-lembind -s ASSERTIONS=1 -sALLOW_MEMORY_GROWTH=1 -s EXPORT_ES6=1 -s MODULARIZE=1`; Release is forced; MSVC is rejected | `CMakeLists.txt:2055-2098` |
| emsdk pin | `emscripten/emsdk@sha256:744fb6…` (emcc 4.0.12). emcc 5 dropped runtime embind dispatch, and `EMBIND_AOT` fails on this binding shape | `Dockerfile:3-12`; `.github/workflows/javascript_builder.yml:27` |
| Python wheel | nanobind; manylinux2014, which means the **old COW `std::string` ABI** (the wheel imports `_ZNSs*` symbols, e.g. `_M_leak_hard`) | `pyproject.toml:131-139`; `.github/workflows/python_cibuildwheel.yml:131-133` |
| Compile-time switches | `NO_ERROR_CATCHING` and `PROPSSI_ERROR_STDOUT` (`CoolProp.cpp:626-699`); `LAZY_LOAD_SUPERANCILLARIES` (`CoolPropFluid.h:444-446`); `COOLPROP_DEEP_DEBUG` (`Solvers.cpp:289-292`); `EIGEN_DONT_VECTORIZE`, only for MSVC90 (`CMakeLists.txt:795-802`) | |
| C++ dialect | `CMAKE_CXX_STANDARD 17` with extensions left on, i.e. `gnu++17`. GCC then defaults to `-ffp-contract=fast` (inference: aarch64 wheels may fuse multiply-adds that x86-64 baseline wheels cannot) | `CMakeLists.txt:104` |
| Environment | `HOME` on Linux/macOS (only Apple falls back to `getpwuid`); `USERPROFILE`, or `HOMEDRIVE`+`HOMEPATH`, on Windows. Feeds `~/.CoolProp/Tables/` and `~/.CoolProp/SVDTables` | `CPfilepaths.cpp:226-266`; `TabularBackends.h:1017`; `SVDSurfaceSerializer.cpp:546` |
| Config keys used at the boundary | `LIST_STRING_DELIMITER` (C API string lists, `CoolPropLib.cpp:212, 251, 532`), `ALTERNATIVE_TABLES_DIRECTORY`, `VTPR_UNIFAC_PATH`, `OVERWRITE_FLUIDS`, `ALLOW_SVDSBTL_IN_PROPSSI` (`configuration_keys.h:17, 52, 55, 81-85`) | |
| Embedded data | 4,648,555 B of CBOR for 136 fluids. Without superancillaries (SA) it is 751,531 B (measured). Plus about 0.45 MB of JSON text for mixtures, cubics, PC-SAFT and incompressibles | 09 |

## 5. State, caching, globals, thread-safety, memory

### 5a. Inventory of process-wide mutable state

Status codes:
- **OK**: no data race.
- **RACE**: data race possible in v8.0.0.
- **FIXED-m**: fixed on origin/master (commit given).
- **SERIAL**: race-free but serialising.
- **INERT**: mutable by declaration, never written.

| # | Object | Location | Purpose | Status (v8.0.0) | Evidence / notes |
|---|---|---|---|---|---|
| 1 | `static JSONFluidLibrary library` + `once_flag` | `FluidLibrary.cpp:28, 40-44` | All HEOS fluids | Load OK; **RACE** for writers | `add_many(string)` (`:130-138`) and `set_fluid_enthalpy_entropy_offset` (`:64-127`) mutate maps and fluids with no lock while factories copy out of them (`FluidLibrary.h:1229-1332`). Master `51e8c60f` fixes orphan names and swallowed load failures, but adds no lock |
| 2 | `static JSONIncompressibleLibrary` + `once_flag` | `IncompressibleLibrary.cpp:540-550` | INCOMP fluids | Load OK | Backends keep a raw `IncompressibleFluid*` into it (07) |
| 3 | `static PredefinedMixturesLibrary` | `MixtureParameters.cpp:16-51` | `.mix` presets | Eager static init (parses JSON before `main`); **RACE** with `set_predefined_mixtures` (`:72-75`) | |
| 4 | `static MixtureBinaryPairLibrary` | `MixtureParameters.cpp:81-283` (`call_once` `:89, 103`) | Binary pairs | Load OK; **RACE** for writers | `apply_simple_mixing_rule` (`:286-289`), `set_mixture_binary_pair_data` (`:353-371`), `set_interaction_parameters` (`:396-399`) |
| 5 | `static MixtureDepartureFunctionsLibrary` | `MixtureParameters.cpp:403-511` | Departure terms | Load OK; **RACE** | `get_departure_function` reads with `map[Name]`, which **inserts on a miss** (`:515`): a write on the read path. `set_departure_functions` (`:792`) is unsynchronised |
| 6 | Cubic and PC-SAFT libraries | `CubicsLibrary.cpp:160-163`; `PCSAFTLibrary.cpp:49-57` | Fluid DBs | Init OK (Meyers singleton); writers RACE | 06 |
| 7 | `static UNIFACParameterLibrary lib` | `VTPRBackend.cpp:9, 137-155` | VTPR groups | **RACE** | Check-then-populate with no lock. `VTPR_ALWAYS_RELOAD_LIBRARY` repopulates while other threads read. Disk I/O inside a factory |
| 8 | `static TabularDataLibrary library` | `TabularBackends.cpp:24` | Shared tables | **RACE**, plus a single-threaded use-after-free | `erase` on an NX/NY change while backends hold a raw `TabularDataSet*` (`TabularBackends.h:1142`). FIXED-m `66ecd326` (TSAN: 103 races); see 08 R6 |
| 9 | Melting-caloric cache: function-static `map` + `mutex` | `MeltingCaloric.cpp:247-264` | HS-flash leg 4 | **SERIAL** | **One mutex for every fluid.** The expensive `build(H)` runs under the lock and **mutates the caller's state** `H` via `H.update(PT_INPUTS, …)` (`:30-55`). Keyed by fluid name only |
| 10 | `SuperAncillary` object shared via `shared_ptr` by all instances of a fluid | `CoolPropFluid.h:407, 430-438`; mutex `superancillary.h:874` | Saturation, caloric SA | **SERIAL** on hot paths | `get_invlnp()` locks on **every** call (`superancillary.h:1078-1085`). It is reached from `get_T_from_p` (`:1272-1274`), which `PQ_flash` (`FlashRoutines.cpp:1169`) and `p_phase_determination_pure_or_pseudopure` (`HelmholtzEOSMixtureBackend.cpp:1795`) call. `ensure_HSU_under_lock` also locks on every call (`:1021-1041`). The unlocked readers `get_approx1d` / `has_variable` (`:971, 1059`) stay correct only because every caller locks first and `clear_caloric_variables` (`:1000`) is never called |
| 11 | `BackendLibrary` (Meyers singleton) | `AbstractState.cpp:36-62` | Backend registry | OK (populated by static initialisers, then read-only) | Generators at `AbstractState.cpp:108-160`, `HelmholtzEOSMixtureBackend.cpp:67`, `REFPROPMixtureBackend.cpp:200` |
| 12 | 5 lookup tables, `unique_ptr` with `if (!p) p = make_unique` | `DataStructures.cpp:163, 393, 456, 577, 846` | Name ↔ enum | **RACE** on first use | FIXED-m `c17783e6` (TSAN: "83 races and a SEGV") |
| 13 | `unique_ptr<Configuration> pconfig` | `Configuration.cpp:126-133` | Global config | **RACE** (lazy init, and `set_*` against readers) | Init fixed by `c17783e6`; set/get still unsynchronised (01 R15) |
| 14 | `std::atomic<int> debug_level` | `CoolProp.cpp:64` | Verbosity | OK | Read at 30+ hot-path sites that print to `std::cout` |
| 15 | `error_string`, `warning_string` + `message_mutex` | `CoolProp.cpp:83-85, 98-105, 1047-1056` | Error outbox | No UB, but last writer wins | `#3146` (`621ee098`) made it `thread_local`; `f5a70e23` (PR #3213, fixing issue #3211) reverted that because thread-pooled hosts (Mathcad, Excel/COM) read the error on another thread (`:67-82`). Drained internally by `Props1SI` (`:883-886`). Warnings are written from deep inside transport (`HelmholtzEOSMixtureBackend.cpp:816, 1060`) |
| 16 | `handle_manager` | `CoolPropLib.cpp:478-510` | C handles | Map OK, **objects RACE** | See 5b |
| 17 | `thread_local` Water/Air/IF97 backends and per-T caches | `HumidAirProp.cpp:51-53, 175-265` | Humid air | OK per thread | About 250 KiB per calling thread, never freed before thread exit (inference from 5f). Becomes process-global under emscripten (row 25) |
| 18 | `std::atomic<int>` model toggles | `HumidAirProp.cpp:114-116` | Humid-air correlations | OK; global behaviour switch | 07 H7 |
| 19 | `static double epsilon, R_bar` | `HumidAirProp.cpp:110` | Constants | INERT | Should be `const` |
| 20 | `std::atomic<int> deriv_counter` | `HelmholtzEOSMixtureBackend.cpp:47` | Instrumentation | OK | `#2844` |
| 21 | `kProcessSalt`, `static std::atomic counter` | `CPfilepaths.cpp:115-122` | Temp-file names | OK | `random_device` at static init |
| 22 | REFPROP globals (`LoadedREFPROPRef`, `dbg_refprop`, `instance_counter`, `_REFPROP_supported`) | `REFPROPMixtureBackend.cpp:67-69`; `.h:40-41` | DLL state | **RACE** | FIXED-m `8214f28e` (one process-wide lock); 07 P1 |
| 23 | `static std::vector xlog, flog` | `Solvers.cpp:290, 431` | Solver trace | RACE in `COOLPROP_DEEP_DEBUG` builds only | |
| 24 | Dead or inert statics | `CubicBackend.cpp:510, 524` (`static std::string errstr`, unused); `Ice.cpp:6-26`; `PCSAFTBackend.cpp:319-366, 612-667`; `TransportRoutines.cpp:786` (non-`const` coefficient arrays) | | INERT | Rot only |
| 25 | `#define thread_local` (empty) under `__EMSCRIPTEN__` | `detail/tools.h:22-24` | WASM | OK single-threaded | Redefines a keyword. Every `thread_local` in row 17 (and any added later) silently becomes shared if WASM is ever built with pthreads. Row 27 is nanobind-only and never built for WASM |
| 26 | Hidden `getenv` function statics | `FluidLibrary.cpp:47`, `FluidLibrary.h:393`, `FlashRoutines.cpp:1853, 2836-2841, 4743, 4825, 4901` | Kill switches | OK (read once) | 01 R16 |
| 27 | `thread_local std::string g_capi_error` | `nanobind_interface.cxx:132` | Capsule last error | OK | The only errno-style channel in the tree |
| 28 | `stdout` | `CoolPropLib.cpp:122-125` (`redirect_stdout` → `freopen`) | Logging | Process-global side effect | |

Per-instance mutable caches (not global, but they prevent sharing): `CacheArray<80>` plus 68 handles (01 5b); `BaseHelmholtzContainer::cache/is_cached` inside every **model** term container (`include/CoolProp/fluids/Helmholtz.h:750-751`); child states `SatL`/`SatV`, plus lazily created `transient_pure_state`, `TPD_state`, `critical_state` and the two ECS reference states (`HelmholtzEOSMixtureBackend.h:68-79`, `.cpp:851, 947`).

### 5b. The C API handle manager

- `add` inserts `(next_handle, AS)` under the lock and returns `next_handle - 1`. `remove` erases under the lock. `get` looks the handle up under the lock and **returns a `shared_ptr&` into the map** (`CoolPropLib.cpp:486-508`). Every export binds `shared_ptr<AbstractState>& AS = handle_manager.get(h)` (e.g. `:656`), so no reference count is held during the call.
  - A concurrent `AbstractState_free(h)` destroys the object mid-call: a use-after-free.
  - Concurrent calls on the same handle race inside the object; there is no per-handle lock.
  - This is code reading: the C exports are not in the wheel.
- The counter is a `long`, which is 32-bit on Windows (LLP64).
  - Inference: after 2^31 factories (a factory-per-request server at 10 k/s takes about 2.5 days), `next_handle++` is signed overflow, which is UB.
  - If the overflow wraps in practice, handles go negative, and `-1` (the factory's error sentinel, `CoolPropLib.cpp:522`) becomes a valid handle. The map key is `size_t`, so negative handles do not collide with the early ones until about 2^32 factories. From then on, `std::map::insert` does not overwrite an existing key, so a counter that hits a live handle returns that **old** state's handle while the new state is freed (`:489-491`). (Corrected by verifier: the aliasing needs ~2^32 factories, not 2^31.)
- Handles are never reused or evicted. Error results are `-1` for `factory` and `_HUGE` for doubles.
- Consumers re-invent locking and get it wrong. The GUI keeps its own `Mutex<HashMap<u64, c_long>>`, copies the handle out, and **releases the lock before calling CoolProp** (`state_manager.rs:28-45`). Two concurrent Tauri commands on one id therefore race inside CoolProp, and `free_state` can race `get_property` into the use-after-free above.

### 5c. Error channels (four, all inconsistent)

| Channel | Used by | Semantics | Defects |
|---|---|---|---|
| `_HUGE` (+inf) return + global `errstring` | `PropsSI`, `Props1SI`, `HAPropsSI`, `PhaseSI` (as an `"unknown: msg"` string) | Read-and-clear; last writer wins | Lost or misattributed under concurrency. `PropsSI`'s `catch(...)` returns `_HUGE` **without** setting a message (`CoolProp.cpp:697-699`). `Props1SI` drains and re-wraps another call's message (`:883-886`) |
| `errcode` + message buffer | 36 handle exports, `set_departure_functions`, `add_fluids_as_JSON` | 0 ok; 1 message; 2 message too long (dropped); 3 unknown | Any `std::exception` that is not a CoolProp type gives 3 with an empty buffer (`CoolPropLib.cpp:52-54`). Examples: the superancillary's `std::invalid_argument`, `std::bad_alloc`, nlohmann errors |
| Python | nanobind | `CoolPropBaseError` → `ValueError`; other `std::exception` types → nanobind's default mapping (mostly `RuntimeError`); `int` → untranslatable | Oracle: `AbstractState('SVDSBTL&HEOS','Water?@/nonexistent')` raises `SystemError: … exception could not be translated!`, and `PropsSI(…'HEOS::Water?@/nonexistent')` raises `ValueError('')` with an empty errstring. Root cause: `throw(errno)` (`CPfilepaths.cpp:98, 330`) via `FactoryOptions.cpp:45`. (Added by verifier: the same throw is reached from `VTPRBackend.cpp:146-151` when `VTPR_UNIFAC_PATH` is wrong; oracle `AbstractState('VTPR','Ethane&n-Propane')` with `/nonexistent/` also raises `SystemError`.) |
| Capsule | State shim | `thread_local` last error, cleared on success | Sound (`nanobind_interface.cxx:132-214`) |

### 5d. PropsSI and backend caching

- No layer caches backends: C++, C API, Python and JS all call `_PropsSI_initialize` → `AbstractState::factory` on every call (`CoolProp.cpp:241-299`). The config docs admit it (`configuration_keys.h:81-85`), which is also why tabular and SVDSBTL are banned from `PropsSI`.
- Oracle: `PropsSI("D","T",300,"P",1e5,"Water")` takes **76.5 µs**; reusing a state for `update(PT)` + `rhomass()` takes **13.9 µs**; constructing an HEOS Water state takes **55.9 µs**. About 80 % of a high-level call is construction (01 R8).

### 5e. "One AbstractState per thread" in practice

- **Reads are not safe either.** Getters fill caches lazily (`if (!_x) _x = calc_x()`), and even the model's term containers cache (5a note). Two threads cannot share an instance even for read-only use.
- **Each worker needs one instance per (backend, fluid, composition).** That costs 56 µs to build and 100-300 KiB to hold (5f). For example, 64 threads × 20 fluids × 150 KiB ≈ 190 MiB of duplicated model data (arithmetic).
- **Per-thread instances still meet shared state:**
  - row 1: read at construction, racing any writer;
  - row 10: an SA lock on `PQ`/`PH`/`PS`/`QH`/`QS`/`DH`/`HS` flashes; inference: contention grows with cores that use the same fluid;
  - row 9: one mutex for all fluids;
  - rows 13 and 15: config reads and errors on failure;
  - rows 8 and 22: tabular and REFPROP in v8.
- **HumidAir follows this pattern internally** with `thread_local` Water, Air and IF97 states (row 17).

### 5f. Memory

Per instance. Construct N live instances in a fresh process and divide; malloc is measured with glibc `mallinfo2` and RSS with `VmRSS`.

| Backend::fluid | malloc / instance | RSS / instance | Note |
|---|---|---|---|
| HEOS::Water | 147.6 KiB | 110.8 KiB | 6 fluid copies (code, below) |
| HEOS::R134a | 102.5 KiB | 65.7 KiB | |
| HEOS::Air (pseudo-pure, no SA) | 99.1 KiB | 62.4 KiB | |
| HEOS::R32&R125 | 199.7 KiB | 164.6 KiB | 12 fluid copies |
| HEOS::R143a / Propylene (ECS transport) | 93.0 → **297.1** / 99.2 → **303.7** KiB | | After the first `viscosity()` + `conductivity()`, two lazily built ECS reference backends are added, each with its own SatL/SatV (the constructor default, `HelmholtzEOSMixtureBackend.h:137`; built at `.cpp:851, 947`). Viscosity and conductivity build separate ones even when they share a reference fluid. 21 of 136 fluids use ECS |
| SRK::Propane | 39.1 → 78.6 KiB after the first update | 52.3 KiB | |
| IF97::Water | 2.1 KiB | 0.1 KiB | Shares function-static regions. IF97 and INCOMP (a raw pointer into its global library; 07) are the only backends that share model data; HEOS and the cubics copy it |
| HEOS::Water with SA disabled | | 107.5 KiB | Shows the oracle shares the SA strings (COW) |

Composition of a pure HEOS state, from the code:
- Three backends: self, SatL and SatV (`HelmholtzEOSMixtureBackend.cpp:141-150`). Each is about 2 KB of `AbstractState` (01) plus HEOS members.
- **Six `CoolPropFluid` copies:** `components` ×3 (`:114`), plus `GERG2008ReducingFunction::pFluids` ×3 (`ReducingFunctions.h:157`). `pFluids` is used only to compute `Yc_T`/`Yc_v` in the constructor (`:160-176`), yet it is re-copied on every `copy()` (`:179-181`, via `sync_linked_states` `HelmholtzEOSMixtureBackend.cpp:161-170`).
- Each fluid copy carries `superancillaries_str`, the SA JSON at about 66 KB median (`CoolPropFluid.h:406`, set at `FluidLibrary.h:395`).
- The wheel uses the COW ABI, so the oracle shares those strings. **On MSVC, libc++ (macOS, emscripten) and new-ABI Linux every copy is deep: about +400 KiB per pure HEOS state, and ≈1.5 MiB for an ECS fluid after transport.** This is an inference from the code plus the ABI check above; it was not measured on those platforms.

Library (fresh process: load the `.so` directly, then trigger the first library use):

| Measure | All 136 fluids | SA disabled |
|---|---|---|
| Load time | 1.98 s | 0.17 s |
| RSS growth | +68.9 MiB | +57.5 MiB |
| Retained malloc after load | **37.1 MiB** | 18.5 MiB |

- Of the retained 37 MiB, about 17.6 MiB is strings that duplicate already-parsed data:
  - `JSONstring_map` holds a full dump of every fluid, about 9.2 MiB (`FluidLibrary.cpp:338`);
  - `superancillaries_str` holds about 8.2 MiB (130 × ~66 KB median; verifier's compact re-serialisation, nlohmann's `dump()` may differ slightly).
- That averages ≈0.27 MiB per fluid (0.14 MiB without SA).
- RSS stays higher than retained memory because the whole-blob nlohmann DOM leaves an allocator high-water mark (inference).
- Lazy follow-ups: the first INCOMP use adds 0.3 MiB in 6.8 ms; the first mixture use (binary pairs + departure functions) 0.9 MiB in 25 ms; the first cubic use 0.1 MiB in 6.6 ms.

### 5g. Wrapper-specific concurrency

- **Python:** `nanobind_interface.cxx` never releases the GIL; it has no `gil_scoped_release` or `call_guard`. Oracle: 4 threads × 1000 updates gives a 0.97x speedup. The module cannot be used to test parallel behaviour, and real Python users get no parallelism.
- **JS/WASM:** built without `-pthread`, so it is single-threaded. `thread_local` is macro'd to nothing (row 25). Objects come from a raw pointer (`allow_raw_pointers`, `emscripten_interface.cxx:228-235`) and need `.delete()`; JS GC never frees them (`test_wasm.mjs:55, 98, 162, 210`).
- **Rust GUI:** `cargo test` is parallel by default, and that is what exposed gh-2787 (`0f639eab` message; `HumidAirProp.cpp:2661-2666`).

## 6. Rot and bugs

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| F1 | Handle reference escapes the lock; no per-handle exclusivity | `CoolPropLib.cpp:500-508` returns `shared_ptr&`; callers bind by reference (`:656`, …). GUI copies the handle and unlocks (`state_manager.rs:28-45`) | Use-after-free on a concurrent free; data races on a shared handle | Opaque `*mut CpState` (owned) for the new ABI. For compat, a generational slab: `u64` = index\|generation, with an `Arc<Mutex<State>>` per slot |
| F2 | Two string outputs have no length parameter and are bounded by the *message* buffer's length | `AbstractState_fluid_names` (`CoolPropLib.h:336`; check `CoolPropLib.cpp:533`), `AbstractState_backend_name` (`CoolPropLib.h:787`; `CoolPropLib.cpp:1066`) | Buffer overflow when `fluids`/`backend` is smaller than `message_buffer` | Every out-buffer gets its own `len`; return the required length; never write past it |
| F3 | Windows `long` handle counter: signed overflow, and `insert` will not overwrite | `CoolPropLib.cpp:482, 489-491` | UB after 2^31 factories; `-1` becomes ambiguous after wrap; stale-handle alias after ~2^32 (inference; see 5b) | `u64` generational ids; never `long` in an ABI |
| F4 | `long`/`int` in the ABI differ by platform (LP64 vs LLP64) | Lengths and handles are `long`; buffer sizes are `int` (`CoolPropLib.h:150`) | Different ABI per OS; bindgen emits `c_long` | `int32_t`/`int64_t`/`size_t` only; header compiled as C99 in CI (as master `585270a5`) |
| F5 | Global error outbox used as an internal channel | `Props1SI` drains `errstring` to build its own message (`CoolProp.cpp:883-886`); `PropsSI` reads it (`:671-676`); `PhaseSI` drains it into its return string (`:1195`, added by verifier). The `#3146` → `#3213`/#3211 flip-flop (`:67-82`) | Another thread's message is reported or swallowed | `Result<T, Error>`. In the ABI, status return + thread-local message; a process-global slot only in the compat shim |
| F6 | Non-`std::exception` throws and silent catch-alls | `throw(errno)` (`CPfilepaths.cpp:98, 330`; reached via `FactoryOptions.cpp:45` and `VTPRBackend.cpp:146-151`); `catch(...) { return _HUGE; }` (`CoolProp.cpp:697-699`); errcode 3 without a message (`CoolPropLib.cpp:52-54`). Oracle: `SystemError`, `ValueError('')` | Undiagnosable failures | No panics across FFI (`catch_unwind` at every export); errors carry a source chain |
| F7 | SA mutex on hot paths, shared by every instance of a fluid | `superancillary.h:1078-1085, 1272-1274`; callers `FlashRoutines.cpp:1169`, `HelmholtzEOSMixtureBackend.cpp:1795`; `ensure_HSU_under_lock` `:1021-1041` | Serialises same-fluid requests on PQ/PH/PS (contention is inference). Downgraded by verifier: an intentional, documented trade-off ("An uncontended mutex acquire is ~20 ns", `superancillary.h:1023-1026`); it is a scalability cost, not a bug | Build derived data in a per-model `OnceLock`; lock-free reads |
| F8 | Melting-caloric cache: one global mutex, build under the lock, caller mutated | `MeltingCaloric.cpp:247-264`; `build` calls `H.update` (`:30-55`) | Serialises every HS fallback across all fluids; side effects on the caller | Per-model `OnceLock`, built from the immutable model on a private scratch state |
| F9 | Writes to global maps on read paths, and unsynchronised writers | `departure_function_map()[Name]` inserts (`MixtureParameters.cpp:515`); VTPR check-then-populate (`VTPRBackend.cpp:138-152`); fluid-library writers (`FluidLibrary.cpp:64-148`) | UB under concurrent construction | Immutable `Registry`; changes produce a new `Arc<Registry>` (builder); lookups return `Option` |
| F10 | Thread-safety retro-fitted piecemeal; v8.0.0 still racy | `0f639eab` (#2800; GUI cargo tests 16/17 failing), `#2844`, `#3146`/`#3211`; master-only `c17783e6`, `66ecd326`, `8214f28e` | The oracle build itself has races | `Send + Sync` model types; test suites run multi-threaded; TSAN/Miri in CI |
| F11 | 6 deep fluid copies per state; models hold caches | `ReducingFunctions.h:157, 179-181`; `HelmholtzEOSMixtureBackend.cpp:114, 141-150`; `Helmholtz.h:750-751`. Measured 99-200 KiB; ECS ~300 KiB (also 02, 09 R5) | Memory × threads × fluids; 56 µs construction | `Arc<FluidModel>`; a state is a small value; reducing data stored as `Yc_T`/`Yc_v` only |
| F12 | Platform-dependent footprint hidden by the oracle | COW ABI in the wheel (`python_cibuildwheel.yml:131-133`) vs deep `std::string` elsewhere; `CoolPropFluid.h:406` | About +400 KiB per state on Windows, macOS and WASM (inference) | No retained source strings; a decoded model only |
| F13 | Eager whole-library load with duplicate strings | Measured 37 MiB retained, 2.0 s (SA 1.8 s); `JSONstring_map` (`FluidLibrary.cpp:338`) | Startup latency and memory; WASM-hostile | Per-fluid lazy decode; serialise on demand (09 R3, R10) |
| F14 | Stale and incomplete stdcall `.def` | `src/CoolPropLib.def`: `second_partial_deriv`, `second_two_phase_deriv` and `first_two_phase_deriv_splined` are listed as `@28`, but their signatures imply `@36`; 27 of 71 exports are absent; last edited for #2291. Checked by script against `CoolPropLib.h` | 32-bit stdcall builds that use it fail to link or lack symbols | Drop 32-bit stdcall unless required; a generated header plus a symbol-list test |
| F15 | Batch exports swallow per-point errors and leave outputs unchanged | `CoolPropLib.cpp:792-806, 820-830, 844-858` (01 R22). Swallowing is intentional and documented (code comment: "As documented in CoolPropLib.h, the output slot ... is left unchanged on error"); the defect is that there is no per-point status. In `_5_out` a throw from output k leaves outputs 1..k-1 of that point updated and k..5 stale (verifier) | Caller cannot tell bad points from stale data | A per-point `status[]`; NaN fill |
| F16 | Exceptions or UB at the C boundary | `AbstractState_set_fractions` builds a vector outside its `try` (`CoolPropLib.cpp:557`); `C_extract_backend` has no `try` (`:1149-1164`; only `std::bad_alloc` or a NULL input can escape, since `extract_backend` does not throw, `CoolProp.cpp:113-135`); `Props1` (01 R23) | `std::terminate` from a host process | `catch_unwind` plus validated lengths in every export |
| F17 | FP guard is partial and clears the caller's flags | Guard in 51 of 71 exports. Missing from `C_is_valid_fluid_string` and `get_fluid_param_string`, which construct states (`CoolPropLib.cpp:1145-1147, 390-401`). The destructor clears all sticky flags (`FPUGuard.h:68-79`); that part is intentional, for Excel/VBA polling hosts (`FPUGuard.h:16-19`) | Trapping hosts (Delphi) can fault in the two unguarded exports (inference); polling hosts lose flags they set before the call (by design) | One guard in every export: save, set default (mask all, round-nearest), restore |
| F18 | Platform macro rot | `#pragma error` is a no-op (`PlatformDetermination.h:16`, `CoolPropLib.h:35`); `__ISPOWERPC__` is unreachable on Linux PPC; a PowerPC `__assert` infinite loop sits in a public header (`CoolPropLib.h:76-84`); `#define thread_local` (`tools.h:22-24`); `windows.h` is included for `DBL_EPSILON` (`CoolProp.cpp:14-27`); an MSVC90 Eigen workaround (`CMakeLists.txt:795-802`); dead `HAS_MOVE_SEMANTICS`/`StringToWString` (`strings.h:23-56`) | Silent misconfiguration; UB (keyword redefinition) | `cfg!`/`#[cfg]`; no keyword games |
| F19 | Home-directory logic | Linux throws when `HOME` is unset (oracle: "Could not detect home directory."); Windows returns `""`, giving `"/.CoolProp/Tables/"` (inference); `getenv` ANSI paths (`CPfilepaths.cpp:226-266`). (Added by verifier.) Setting `ALTERNATIVE_TABLES_DIRECTORY` does not help: `get_home_dir()` is called before the override is checked (`TabularBackends.h:1017-1021`, also `TabularBackends.cpp:360`). Oracle: with `HOME` unset and the key set, `AbstractState('TTSE&HEOS','R245fa')` still raises "Could not detect home directory." Still present on origin/master. SVDSBTL does it right (`SVDSurfaceSerializer.cpp:545-546`) | Table backends fail in services and containers | Caller-supplied cache directory; no implicit home lookup in the core |
| F20 | WASM build hygiene | `ASSERTIONS=1` and JS-based exception catching in Release (`CMakeLists.txt:2068, 2078`); toolchain pinned to emcc 4.0.12 (`Dockerfile:3-12`); breaking JS API change on 2026-05-24 (`82d58c0d`, VectorDouble → arrays); guarded by legacy `#ifdef EMSCRIPTEN` (`emscripten_interface.cxx:8`; inference: non-STRICT only) | Size, speed and maintenance cost | wasm-bindgen; no exceptions; `wasm-opt`; SemVer-stable JS API |
| F21 | Legacy Rust crate unusable or unsafe | `as *const i8` (`wrappers/Rust/src/lib.rs:61-66`) does not compile on aarch64-linux/android, where `c_char = u8`; `Display` formats itself, so it recurses infinitely (`:40-48`); `catch_unwind` cannot see the errors, which arrive as `inf`, so failures return `Ok(inf)` (`:59-72`); no interior-NUL check; system-installed lib; bindgen 0.55 at build time; README paths invalid (`coolprop-rs::`, "coolplot-rs") | Wrong results and crashes on common targets | Pure Rust; `std::ffi::c_char`; `CString`; typed errors |
| F22 | GUI FFI signature drift | `get_global_param_string` is declared with 6 args (`coolprop_ffi.rs:45-52`) vs 3 in `CoolPropLib.h:150`. `errcode` is never written, so failures read as `Ok("")`. `vec![0i8; …]` (`:84`) has the same `c_char` portability bug. The comment claims 1e300 on failure (`:124`); it is `inf` | Silent wrong behaviour; would corrupt the stack under stdcall | Generate bindings or, better, no C++; ABI conformance tests |
| F23 | JS marshalling and coverage | One JS call per array element (`emscripten_interface.cxx:35-50`). No `specify_phase`, `phase()`, fugacity, BIP setters, `fast_evaluate` or reference-state API (absent from `:237-345`). Errors surface as `Infinity` (`PropsSI`) or as raw C++ exceptions (inference) | Slow batches; incomplete API | `Float64Array` views (zero-copy); a generated, complete binding surface |

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

| Piece | Character | Fit / plan |
|---|---|---|
| Single high-level calls (`PropsSI`) | About 80 % construction (5d), then a branchy flash | Embarrassingly parallel across calls **once the model is shared and immutable**. In CoolProp, construction cost and globals block it |
| C batch exports | Serial loop over one handle; `catch` per point | This is the natural data-parallel boundary. Split SoA ranges across workers, each with its own scratch; run SIMD lanes for the branch-free stages; report per-point status |
| Handle API | Sequential per handle | Parallel only across handles, so exclusivity is required (F1) |
| Library and SA build | One blob; SA build is 1.8 s of 2.0 s | Independent per fluid: lazy and on demand; can run in parallel for warm-up |
| Derived per-fluid data (caloric SA, inverse p(T), melting caloric, ECS reference) | Build once | `OnceLock`; reads lock-free and wait-free |
| FP guard | Two control-word writes per call; inference: `ldmxcsr`/`fldcw` serialise the pipeline | Once per batch, not per point |
| String parsing, enum lookup | Branchy, allocating | Hoist per batch into a compiled plan (01 sec. 7) |
| Error reporting | Exceptions plus a global outbox | Per-point status codes compose with SIMD masks |
| JS/WASM | Single thread. SIMD128 is in all current browsers. Threads need COOP/COEP + `SharedArrayBuffer` | `simd128` feature path; threading optional and outer only |
| Python oracle | GIL held | Generate fixtures with multiple processes (10) |
| Table builds (SVDSBTL sampling) | `std::thread` with per-thread `factory()` clones (`SVDSurfaceFactory.cpp:128-190`) | rayon over an immutable model (08) |

Design consequences for the "side-by-side implementations" goal:
- The batch boundary is where `scalar` (the reference), `simd` (`cfg(target_feature)`, `simd128` on WASM) and `par` (an optional rayon feature) plug in. All three share the same typed API and numerics and are tested against `scalar`.
- Branchy solvers (phase determination, saturation, two-phase flashes) stay scalar per point and parallelise only across points.
- CoolProp contains no SIMD or OpenMP. The only SIMD is Eigen's internals; `-msse4.1` is commented out (`CMakeLists.txt:867`).
- Reproducibility: Rust does not contract `a*b+c` implicitly; use explicit `mul_add` where wanted. Inference: CoolProp's `gnu++17` GCC builds may contract on aarch64, so oracle fixtures should record the oracle's platform and use ULP-aware tolerances.

## 8. Verification assets

- **JS test** (`wrappers/Javascript/test_wasm.mjs`):
  - Water normal boiling point in [373.124, 373.125] K (`:8-12`);
  - mixture `set_mole_fractions` vs `PropsSI` agree to 1e-8 relative (`:73-83`);
  - `get_mole_fractions` round trip to 1e-12 (`:87-96`);
  - phase envelope: at least 5 points, mid-point plausibility (`:136-160`);
  - dH/dT|P = cp_molar to 1e-6 relative (`:176-185`);
  - dp/dT|sat > 0 (`:188-195`);
  - the splined two-phase derivative is finite (`:199-208`);
  - enum-presence probes (`:109-129`).
  Port it to the Rust WASM package unchanged.
- **Rust crate tests** (`wrappers/Rust/src/lib.rs:107-115`): `PropsSI("H","T",300,"Q",1,"R134a") = 413265.6843372975` and `HAPropsSI("H","T",300,"P",1e5,"R",0) = 27013.112479771713`. The oracle reproduces both bit-for-bit. Exact-equality tests are brittle, so keep them as fixtures with tolerances.
- **GUI tests** (`coolprop_ffi.rs:180-250`): Water Tcrit 647.096 ± 0.01 K; Water at 1 atm, 300 K, ρ = 996 ± 2 kg/m³; humid-air W(20 °C, 50 % RH) = 0.00729 ± 1e-3; plus 5 error-path tests (invalid backend, fluid, pair, parameter).
- **Concurrency tests to mirror (semantics, not mechanism):**
  - `HAPropsSI is thread-safe under concurrent callers`: 16 threads × 50 iterations, 1e-9 relative (`HumidAirProp.cpp:2667-2712`);
  - SA lazy build: 8 threads, build count ≤ 1 (`src/Tests/CoolProp-Tests.cpp:6186-6237`);
  - `write_bytes_atomic` race test (`CoolProp-Tests-SVDSBTL.cpp:1289`);
  - cross-thread errstring handoff `#3211` (`CoolProp.cpp:1105-1130`). This one documents a host contract that the compat shim must honour.
- **FP guard:** `src/Tests/CoolProp-Tests-FPUGuard.cpp:27-82`, three cases: traps masked inside the scope, an already-masked environment left unchanged, flags cleared.
- **C ABI shape:** `dev/state_capsule/test_capsule.py` (capsule contract); `dev/ci/check-installed-headers.sh` (exists at v8.0.0; master `585270a5` adds the C99 compile and fixes two `()` prototypes); `dev/ci/check-json-symbols.sh` (no leaked symbols).
- **Oracle hooks:** the C exports are **not** in the wheel, so C-ABI behaviour can only be tested from a native build. Python error behaviour differs from the C ABI: Python raises, C returns `inf` + errstring.
- **Measurements made for this doc** (rerun for the Rust port's budgets):
  - per-instance malloc and RSS (5f);
  - library load: 1.98 s, 37 MiB retained;
  - PropsSI 76.5 µs vs reuse 13.9 µs vs construct 55.9 µs;
  - GIL scaling 0.97x;
  - error paths (F6);
  - HOME unset (F19).
- **New tests the port needs:**
  - `cargo test` multi-threaded stress: N threads × M fluids, results bit-identical to the serial run;
  - Miri on the FFI crate; TSAN (nightly) in CI;
  - a cbindgen header compiled as C99 with `-pedantic-errors`;
  - an exported-symbol list test;
  - batch vs scalar equality: bitwise for `par`, ULP-bounded for `simd`;
  - a memory-budget test: bytes per `State` and per loaded fluid.

## 9. Port recommendation

| Unit | CoolProp paths | Priority | Rust shape | ~LOC |
|---|---|---|---|---|
| U1 Concurrency contract | All of 5a | **P0-core** | Every model type `Send + Sync` and immutable; no `static mut`, no global config, no outbox; only `OnceLock` for lazily derived data; core is `#![forbid(unsafe_code)]`; tests run threaded | 150 (tests) |
| U2 Lazy registry | `FluidLibrary.cpp:28-62, 150-394` (09 D4) | **P0-core** | Static index to a per-fluid `OnceLock<Arc<FluidModel>>`. User fluids and reference states via a builder that returns a new `Arc<Registry>` / `Arc<FluidModel>` sharing coefficient slices | 400 |
| U3 Error model | `CoolPropLib.cpp:33-55`; `CoolProp.cpp:64-106, 697-699` | **P0-core** | `#[non_exhaustive] enum Error` with context; warnings returned as data (e.g. `TransportQuality::Approximate`) | 200 |
| U4 Batch API (scalar reference) | `CoolPropLib.cpp:784-862`; `fast_evaluate` (01) | **P0-core** | `eval_batch(&Model, Pair, &[f64], &[f64], &[Output], &mut [f64], &mut [Status])`; SoA; no allocation | 300 |
| U5 WASM-clean core gate | `tools.h:22-24`; `CMakeLists.txt:2055-2098` | **P0-core** | CI builds `wasm32-unknown-unknown` with no default features; core has no fs, env, threads or clock | 50 |
| U6 Oracle harness | 10 | **P0-core** | Multi-process fixture generation; empty `COOLPROP_*` env; record version, gitrevision and platform | 150 |
| U7 Derived-data caches | `superancillary.h:874-1085`; `MeltingCaloric.cpp:247-264`; `HelmholtzEOSMixtureBackend.h:68-79` | P1-early | Per-model `OnceLock`, built from the model on private scratch; ECS reference = `Arc` of the reference fluid's model | 200 |
| U8 Data provider | `FluidLibrary.cpp:8-23` (09) | P1-early | Per-fluid embedded blobs behind cargo features, or `&[u8]` at runtime (fetch on WASM); SA as a separable asset (84 % of the bytes) | 200 |
| U9 New C ABI crate (`cp_*`) | `CoolPropLib.h/.cpp`; `state_capi.h` | P1-early | `cdylib`/`staticlib`; cbindgen header; opaque `CpModel*` (Arc) and `CpState*` (owned); `size_t`/`int32_t`; status return + `cp_last_error(buf, len) -> needed` (thread-local); explicit lengths everywhere; `catch_unwind`; batch with `status[]` | 1,200 |
| U10 FP-environment guard | `FPUGuard.h` | P1-early (with U9) | Save, set (mask all, round-nearest), restore: `_controlfp_s` on Windows, fe* on glibc; no-op on wasm and macOS. Declare the externs directly (no deps) | 80 |
| U11 CoolPropLib.h compat shim | Tier A: `PropsSI`, `Props1SI`, `PhaseSI`, `HAPropsSI`, `get_global_param_string`, `get_fluid_param_string`, `get_param_index`, `get_input_pair_index`, `set_config_*`. Tier B: about 25 core `AbstractState_*` exports, including the batch and `checkedMemory` variants | P2-later | Same names and signatures; fixed buffer semantics (F2); generational handles and a per-handle `Mutex` (F1, F3); a **process-global** errstring slot (the `#3211` host contract); enum values pinned to v8.0.0 | 1,500 |
| U12 JS/WASM package | `emscripten_interface.cxx`; `test_wasm.mjs` | P2-later | wasm-bindgen; `free()` + FinalizationRegistry; `Float64Array` batch I/O; errors as JS `Error` | 600 |
| U13 SIMD and threaded batch back-ends | none in CoolProp | P2-later | `simd` (`target_feature`: AVX2 / NEON / simd128) and `par` (rayon) side by side behind U4, verified against scalar | 800 |
| U14 Python bindings | `nanobind_interface.cxx` | defer | PyO3; release the GIL (`allow_threads`) around kernel calls; free-threaded-ready | none |
| U15 Paths and atomic write | `CPfilepaths.cpp`; `atomic_write.h` | defer | Only for on-disk table caches; caller-supplied directory; temp + rename | 120 |
| U16 32-bit stdcall / `.def` / VERSIONINFO | `CoolPropLib.def`; `CMakeLists.txt:590-635, 694-702` | defer | Only if 32-bit Office VBA must be served | none |
| U17 Legacy and global-mutation exports | `Props`/`PropsS`/`Props1`/`HAProps`, `*_` Fortran exports, `redirect_stdout`, `set/get_debug_level`, `set_reference_stateS/D`, `add_fluids_as_JSON`, `set_departure_functions`, unchecked `get_phase_envelope_data` | drop | Replaced by builders and options (U2) | 0 |
| U18 Existing Rust wrappers | `wrappers/Rust/`, `wrappers/GUI/src-tauri/src/coolprop_ffi.rs` | drop | Lessons only (F21, F22) | 0 |
| U19 Platform shims | `PlatformDetermination.h`, PowerPC paths, MSVC90, `crtdbg`, `#define thread_local` | drop | `#[cfg]` | 0 |
| U20 HumidAir `thread_local` backend pattern | `HumidAirProp.cpp:51-265` | drop (pattern) | Pure functions over shared `Arc` water/air models (07 owns humid air) | 0 |

Order: U1 → U3 → U2 → U5 → U6 → U4 → U7 → U8 → U9 + U10 → U11 (tier A first) → U12 → U13; U14-U16 on demand.

What to redesign (not port):
- **State vs model.** CoolProp's AbstractState fuses model, state, cache and workspace (01). The Rust `State` is a small value (T, ρ, x reference, phase, derivative bundle) computed by `fn flash(&Model, …) -> Result<State>`. That gives no SatL/SatV objects, no copies and nothing to lock.
- **No global mutation.**
  - Reference states, user fluids, BIP changes and departure functions all produce new immutable models or registries (`Arc`), so concurrent readers never observe a change.
  - Config becomes a per-call `Options` value.
  - The process-global errstring survives only in the compat shim.
- **ABI hygiene.**
  - Fixed-width integer types only.
  - `#[repr(u32)]` enums with explicit, append-only discriminants pinned by a test. Never reuse CoolProp's renumbered enums (01 R1) except in the compat shim.
  - Every buffer has its own length.
  - No sentinels: status codes plus `NaN` fills.
  - Panics never cross FFI.
- **No C++ in the dependency chain.** Both Rust consumers needed a system `libCoolProp` or CMake plus per-OS `stdc++`/`c++` link lines (`wrappers/GUI/src-tauri/build.rs:35-43`). A pure-Rust core removes libclang, CMake and the C++ runtime for every user, including WASM.

## 10. Open questions

1. Is a drop-in `CoolPropLib.h`-compatible `cdylib` a deliverable (for Excel, LabVIEW, EES, Julia and Fortran users)? If so, which tiers, and must the integer enum values match v8.0.0 exactly, given that v7 → v8 renumbered them?
2. Must the compat shim keep CoolProp's **cross-thread** errstring semantics for thread-pooled hosts (`CoolProp.cpp:67-82`)? Those semantics conflict with per-thread error reporting.
3. Is 32-bit Windows (stdcall, 32-bit Office VBA) in scope?
4. WASM targets: browser (`wasm32-unknown-unknown` + wasm-bindgen), WASI, or both? Is single-threaded + SIMD128 the baseline, with threads (COOP/COEP) optional?
5. WASM data delivery: embed every fluid, or fetch per fluid? Are superancillaries mandatory, or an optional asset (84 % of the bytes, 1.8 s of build time in CoolProp)?
6. Should the C ABI FP guard also normalise the rounding mode and FTZ/DAZ, or only mask exceptions as CoolProp does?
7. Should the new ABI offer integer handles at all (VBA and LabVIEW prefer them), or opaque pointers only?
8. What are the memory and latency budgets: bytes per `State`, MiB per loaded fluid, cold-start time for one fluid?
9. Python: is a binding a deliverable, or is CoolProp only the oracle? If a binding ships, it must release the GIL and should target free-threaded CPython.
10. How much SA-lock contention exists in CoolProp under many cores on one fluid? Measuring it needs a native multi-threaded benchmark against a C++ build, which would be useful only as a baseline for the Rust port's scaling claims.

## Verification log

- Date: 2026-10-04. Adversarial verification against v8.0.0 (ae81610e) and the CoolProp==8.0.0 wheel. No partial earlier verifier edits were found in this doc.
- Claims checked: about 180. That covers the path:line citations in sections 1-9, every count, and every oracle number.
- Counts re-run and confirmed:
  - scope LOC (235, 259, 348, 1,201, 1,164, 856, 47, 91, 157, 331, 627; about 5,360 in total);
  - 71 exports (36 handle) and group sizes 8/4/3/10/10/36;
  - 51 of 71 exports guarded;
  - 44 `.def` entries, three with wrong `@28` (header implies `@36`);
  - embind enum sizes 86/43/9/13;
  - 21 ECS fluids;
  - 130 SA fluids;
  - CBOR 4,648,555 B; 751,531 B with every `SUPERANCILLARY` stripped;
  - `JSONstring_map` 9.19 MiB.
- Oracle re-runs, all reproduced within noise:
  - malloc per instance: Water 147.6, R134a 102.5, Air 99.0, R32&R125 199.3, R143a 92.6 → 296.6, SRK Propane 39.1 KiB;
  - import plus load 2.0 s, 37.5 MiB retained;
  - PropsSI 74 µs, reuse 14 µs, construct 52 µs;
  - 4-thread GIL speedup 0.99x;
  - `_ZNSs` imports present (41) and no C exports in the wheel;
  - `SystemError` and `ValueError('')` error paths;
  - "Could not detect home directory.";
  - both Rust-crate fixtures bit-exact.
- Corrections made:
  1. `CoolPropLib.h:38-52` → `:38-60` (COOLPROP_LIB block).
  2. `javascript_builder.yml:28` → `:27`.
  3. Config-key row: `configuration_keys.h:81-85` covered only `ALLOW_SVDSBTL_IN_PROPSSI`; now `:17, 52, 55, 81-85`.
  4. GUI FFI line ranges fixed (`coolprop_ffi.rs:6-63`, `:69-78`, `:80-178`).
  5. Row 15: the revert was `f5a70e23` (PR #3213, fixing issue #3211), not "#3211".
  6. Row 25: row 27 (nanobind) is never built for WASM; only row 17 is affected.
  7. 5b/F3: the stale-handle alias needs ~2^32 factories, not 2^31, because the key is `size_t`. Added that `-1`, the error sentinel, becomes a valid handle after a wrap.
  8. 5c: nanobind maps non-CoolProp `std::exception` types with its defaults, not always to `RuntimeError`.
  9. 5f: SA strings ≈8.2 MiB, not 8.4.
  10. FPU-guard test range `:27-66` → `:27-82`.
  11. `check-installed-headers.sh` exists at v8.0.0; master only adds C99 mode.
  12. kSI list now includes Cp0.
- Rot downgraded or qualified:
  - F7 is an intentional, documented trade-off (`superancillary.h:1023-1026`).
  - F15's swallowing is documented behaviour. The real gap is the lack of a status channel; partial-row update in `_5_out` added.
  - F16: only `bad_alloc` or NULL can escape `C_extract_backend`.
  - F17: clearing the caller's flags is by design for polling hosts; the trapping-host fault is inference.
- Added by verifier, with evidence:
  - F19: `ALTERNATIVE_TABLES_DIRECTORY` does not bypass `get_home_dir()` (`TabularBackends.h:1017`, `TabularBackends.cpp:360`). Oracle-confirmed, and still present on master.
  - F6 and 5c: VTPR with a bad `VTPR_UNIFAC_PATH` also throws `int` (`SystemError`).
  - F5: `PhaseSI` drains `errstring` (`CoolProp.cpp:1195`).
- Not re-verified: rot IDs in the sibling maps (01, 02, 06, 07, 08, 09, 10). The RSS figures in 5f were not re-measured; only malloc was.
