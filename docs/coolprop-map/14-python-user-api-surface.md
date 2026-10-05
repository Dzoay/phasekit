# 14 Python user-facing API surface and downstream contracts - CoolProp v8.0.0 map

> Scope: `wrappers/Python/_nanobind/` (`CoolProp.pyi` 1,584, `State.pyx` 505, `State.pxd` 52, `__init__.py` 93, `HumidAirProp.py` 21, `stub_patterns.txt` 54), the Python-shaping lambdas in `src/nanobind_interface.cxx` (`:24-127`, `:657-1198`), `src/CoolProp.cpp:300-700` (PropsSImulti semantics), `wrappers/Python/generate_constants_module.py` (111), `wrappers/Python/CoolProp/{CoolProp.pyx,BibtexParser.py}` (the legacy reference, 1,518), `dev/pdsim_cimport_contract/` (478), `wrappers/Python/pytest/` (1,784 in 11 `.py` files; 1,817 with README), `Web/coolprop/{HighLevelAPI,LowLevelAPI}.rst` (1,053). About 7,400 lines. Part of the coolprop-rs port plan; cites the v8.0.0 source.

Conventions:
- `path:line` cites v8.0.0 (ae81610e).
- "Oracle" means the PyPI `CoolProp==8.0.0` wheel, run through `uv run --no-project --python 3.12 --with CoolProp==8.0.0`. Every oracle result below was measured for this document. The probe scripts are `p1.py`-`p5.py` in the session scratchpad and are not committed.
- "Inference" marks claims that were not demonstrated directly.
- Sibling maps: 01 (API/state), 07 (humid air), 10 (tests/oracle), 11 (FFI, nanobind internals, GIL), 12 (rot). This map does not repeat them.

## 1. Purpose and concepts

Most users, and our oracle harness, reach CoolProp through Python. Python is therefore the de facto public contract. The vectorized `PropsSI` is the batch API people actually use. The legacy `State` class mixes unit systems, and PDSim compiles against that mix.

| Concept | v8.0.0 realisation | Where |
|---|---|---|
| Package layout | `CoolProp/` contains: the nanobind core `CoolProp.abi3.so` (imported as `CoolProp.CoolProp`); `_constants.abi3.so` + `constants.py`; `State.abi3.so` (a Cython shim); `HumidAirProp.py`; `.pxd`, `.pyi` and `py.typed`; `include/` (107 headers); `CoolPropBibTeXLibrary.bib`; `Plots/`, `GUI/`, `tests/`, `BibtexParser.py`. 143 files under `CoolProp/` in the wheel RECORD, plus 6 dist-info files (oracle, `importlib.metadata.files`) | `_nanobind/__init__.py:1-37` |
| Core module | 259 public names: 47 functions, 15 classes (4 of them `Py*` aliases), 6 `IntEnum`s with 190 exported values, and `_capi` (oracle) | `CoolProp.pyi`; `nanobind_interface.cxx:383-1198` |
| Two enum surfaces | `CoolProp.CoolProp.iT` is an `IntEnum` member, `parameters.iT`. `CoolProp.constants.iT` and `CoolProp.iT` are plain `int` 19. Both are accepted everywhere because the enums are `nb::is_arithmetic` | `nanobind_interface.cxx:453, 459, 552, 601, 615, 624`; `constants.py` (generated) |
| Errors | `CoolPropBaseError` → `ValueError` (the translator catches only this type). Any other `std::exception` falls through to nanobind's default → `RuntimeError` (oracle: `conformal_state` → `RuntimeError('std::exception')`, R21; `PropsSI(b'D',...)` → `RuntimeError('std::bad_cast')`). Length mismatch → `TypeError`. errno throws → `SystemError` (per 11 sec. 5; not re-verified here). `HAProps` → `NotImplementedError` | `nanobind_interface.cxx:394-400` |
| Legacy `State` | A Cython cdef class that forwards through the `_capi` PyCapsule. kPa/kJ getters, SI `Props()`/`.pAS` | `State.pyx:189-505`; `nanobind_interface.cxx:130-271, 1197` |
| PDSim contract | `cimport` of `State`, `.pAS`, and `constants_header`. It is link-free, resolving through the vtable capsule | `dev/pdsim_cimport_contract/SURFACE.md:13-93` |

## 2. Structure (key types/functions -> path:line)

### 2.1 Public name inventory (runtime, oracle-verified)

| Group | Names | Call semantics | Where |
|---|---|---|---|
| High-level | `PropsSI` (4 overloads), `Props1SI`, `PhaseSI`, `PropsSImulti`, `HAPropsSI` (2 overloads), `HAProps_Aux`, `cair_sat`, `saturation_ancillary` | Positional-only; `str` only. `bytes` in a name/fluid slot raises `TypeError` (no overload matches); `bytes` as `Output` falls into the sequence-of-outputs overload and raises `RuntimeError('std::bad_cast')` (oracle). Legacy accepted bytes (inference: Cython `std::string` coercion, `CoolProp.pyx:498-583`). See sec. 3 | `nanobind_interface.cxx:947-1144`; `.pyi:1440-1511` |
| Name/metadata | `FluidsList`, `get_aliases`, `get_REFPROPname`, `get_BibTeXKey`, `get_fluid_param_string`, `get_global_param_string`, `get_parameter_index`, `get_parameter_information`, `get_phase_index`, `is_trivial_parameter`, `extract_backend`, `extract_fractions`, `generate_update_pair` | Thin string lookups. The list results are split in C++ | `:936-946, 1079-1094, 1164-1168` |
| Global mutation | `set_config_{string,double,bool,int}`, `set_config_as_json_string`, `set_reference_state{,S,D}`, `set_debug_level`, `add_fluids_as_JSON`, `set_departure_functions`, `set_interaction_parameters`, `set_predefined_mixtures`, `set_mixture_binary_pair_{data,pcsaft}`, `apply_simple_mixing_rule` | Process-wide, with no scoping (01, 09, 11) | `:922-935, 1095-1098, 1146-1186` |
| Error outbox | `get_errstr()` | **Read-and-clear**: a second call returns `''` (oracle) | `:1168` |
| Classes | `AbstractState` (169 public methods), `SimpleState`, `CriticalState`, `GuessesStructure`, `PhaseEnvelopeData`, `SpinodalData`, `Py*` aliases of the last four, `ChebyshevExpansion`, `ChebyshevApproximation1D`, `SuperAncillary`, `IntervalMatch`, `MonotonicExpansionMatch` | Not picklable; `copy.copy` also fails (oracle) | `:402-430, 613-656, 657-920, 279-381` |
| Package attrs | `__fluids__` (136), `__incompressibles_pure__` (74), `__incompressibles_solution__` (52), `__version__` `'8.0.0'`, `__gitrevision__`; helpers `get(s)`, `test()`, `get_include_directory()`, `copy_BibTeX_library()` | Computed **eagerly at import** (R2) | `_nanobind/__init__.py:33-93` |
| Humid air module | `HumidAirProp.{HAPropsSI, HAProps_Aux, cair_sat, HAProps}`. `HAProps` is a stub that raises | `_nanobind/HumidAirProp.py:9-21` |
| Constants | `constants.py`: 190 enum values, plus a leaked `absolute_import` (also re-exported as `CoolProp.absolute_import` via `from .constants import *`, oracle). Generated by scraping the C++ headers for text; run by the build (`wrappers/Python/CMakeLists.txt:104`) | `generate_constants_module.py:11-56, 59-105` (leak written at `:94`) |

### 2.2 AbstractState members whose Python shape differs from C++

All 155 public methods of the legacy Cython `AbstractState` (`wrappers/Python/CoolProp/AbstractState.pyx`) are present; v8 has 169, i.e. 14 v8-only (`available_in_high_level`, `clear`, `dBvirial_dT`, `dCvirial_dT`, `dipole_moment`, `get_reducing_state`, `get_state`, `mole_fractions_{liquid,vapor}_double`, `p_triple`, `set_T`, `using_{mass,mole,volu}_fractions`) (oracle vs grep). The legacy `Py*` struct classes survive as aliases. At module level the only legacy name gone is the deprecated `Props` (`CoolProp.pyx:469`); `HAProps` survives only as a raising stub. The members below are reshaped for Python, so a Rust core needs typed return structs for them, not out-params.

| Method | Python return | Where |
|---|---|---|
| `AbstractState(backend, fluids)` | `__new__` → `factory`; `isinstance` works | `:657-658` |
| `criticality_contour_values()` | `(L1*, M1*)` tuple | `:711-716` |
| `true_critical_point()`, `ideal_curve(name)` | `(T, rho)` and `(T[], p[])` tuples | `:830-841` |
| `viscosity_contributions()`, `conductivity_contributions()` | dict with `dilute`, `initial_density`, `residual`, `critical`. For Water everything is reported under `critical` (oracle: 0.000854); inference: IAPWS is a product form, not a sum | `:856-880` |
| `conformal_state(ref, T, rho)` | dict with `T` and `rhomolar` (in/out args) | `:882-891` |
| `fast_evaluate(pair, v1, v2, outputs:int32[], out:(N,M), status:int32[N], imposed_phase)` | Fills caller buffers in place. Shape is validated. HEOS raises "not implemented". IF97 returns `nan` + status 1 for an out-of-range point | `:726-758` |
| `get_phase_envelope_data()`, `get_spinodal_data()`, `all_critical_points()` | Struct objects with list fields | `:423-430, 635-640, 708-710, 850` |
| Before any update | `T()` and `keyed_output(iT)` return `-inf`; `hmass()` raises `phase is invalid` (oracle) | sentinel leak, 01 R22 |

### 2.3 Legacy `State` shim (the unit split)

| Member | Unit returned | Note | Where |
|---|---|---|---|
| `State(Fluid, params=None, phase=None, backend=None)` | The input dict takes `T` [K], `D` [kg/m³], `Q` [-] as SI, and `P` [**kPa**], `H` [**kJ/kg**], `S` [**kJ/kg/K**], `U` [**kJ/kg**] | Exactly two keys; `'Dmass'` is rejected. Default backend is `HEOS`; legacy used `'?'` (`CoolProp.pyx:855-856`) | `State.pyx:202-232, 293-316` |
| `get_p`/`.p`, `p_` | kPa | | `:404-406, 290` |
| `get_h`/`u`/`s`/`cp`/`cp0`/`cv` | kJ/kg and kJ/kg/K | | `:407-427` |
| `get_dpdT` | kPa/K, computed as `∂p/∂T|ρmolar / 1000` | | `:431-433` |
| `get_MM`/`.MM` | **g/mol** (×1000) | **No value check anywhere** (R14). Same as legacy (`CoolProp.pyx:992-994`) | `:428-430` |
| `get_cond`/`.k` | **kW/m/K** (÷1000) | No value check anywhere (only the unit-free `Prandtl` ratio is tested). Same as legacy (`CoolProp.pyx:1084-1086`). The module docstring says "1000x the SI value" (`:16-17`), which is wrong for `k` | `:437-439` |
| `get_T`, `get_rho`, `get_visc`, `get_speed_sound`, `get_Q`; `T_`, `rho_` | SI | `Q` = **-1.0** in single phase (oracle) | `:401-445` |
| `Props(key)`, everything on `.pAS` | SI | `Props(iP)` = 101325.00008 while `p` = 101.32500008 (oracle) | `:361-365, 150-186` |
| `Prandtl` | cp[kJ]·μ/k[kW] is dimensionless and consistent | | `:503-505` |
| `Tsat`, `superheat`, `subcooling` | K, or `None` outside the two-phase p-range. Subcooled liquid reports `superheat` = **-73.1** (not `None`), the same as legacy | | `:371-399` |
| `update_Trho(T, rho)`, `update_ph(p_kPa, h_kJ)`, `copy()`, `set_Fluid`, `Phase()` | | `copy()` drops the `phase` string (oracle: `b''`); legacy kept it (`CoolProp.pyx:1288-1295`). v8 `copy()` does forward mixture fractions (`State.pyx:346-355`, GH #3151) | `:318-369` |

The PDSim contract (`SURFACE.md:23-50`) has three parts:
- the members above plus `.pAS.{keyed_output, rhomass, cpmass, cvmass, T, p, update, fluid_names, first_partial_deriv}`;
- the enum values `iT iP iHmass iSmass iUmass iDmass iDmolar iCpmass iCp0mass iCvmass ispeed_sound iconductivity iviscosity imolar_mass iQ iP_critical iT_critical PT_INPUTS`;
- the cimport paths `CoolProp.State`, `CoolProp.CoolProp` (re-exported through a `.pxd`) and `CoolProp.constants_header`.

`run_contract.py:41-105` checks the values only against `PropsSI`, and checks 11 of the 15 `get_*` getters (not `get_T`, `get_MM`, `get_visc`, `get_cond`; `pdsim_surface.pyx:56-59` collects them but nothing asserts on them). The check is opt-in through the environment variable `RUN_PDSIM_CIMPORT_CONTRACT` (`test_pdsim_contract.py:28-29`).

## 3. Algorithms and formulas (cite the papers CoolProp cites)

This area contains no thermophysics and cites no papers. The algorithms here are the dispatch and broadcasting rules, all measured with the oracle.

### 3.1 `PropsSI` dispatch (`nanobind_interface.cxx:947-1075`)

1. The 2-argument form `PropsSI(out, fluid)` calls `Props1SI`, which tolerates the arguments in either order. A non-finite result raises `ValueError`. `Props1SI` called directly returns **`inf`** instead (oracle).
2. The all-`float` 6-argument form calls C++ `PropsSI`. A non-finite result raises `ValueError(errstring)`, and the message carries a ` : PropsSI("D","T",-5,...)` suffix.
3. Any other 6-argument call takes the "object" overload. Each value is classified by `_to_vec` (`:59-81`):
   - an object with `ndim`: 0 means scalar, 1 means sequence, more than 1 raises `ValueError("...not one-dimensional")`. An `(n,1)` array is also rejected; legacy accepted it (`CoolProp.pyx:521-524`);
   - `list`/`tuple` → sequence;
   - anything else goes through `float()`. `range` and generators therefore raise `TypeError`.
4. Broadcast: `n = max(len)`. A length-1 operand is repeated; any other mismatch raises `TypeError` (`:114-127`). An empty sequence returns `array([])` before any validation (`:970-974`).
5. The work runs through C++ `PropsSImulti` on **one** `AbstractState`, evaluated sequentially. The result is a C-contiguous float64 ndarray, freshly allocated.
6. If every input was scalar (for example int literals), the result is a Python `float`. The message then lacks the `PropsSI(...)` suffix (oracle).
7. A first argument that is a sequence of outputs takes a separate overload (`:1013-1066`). It returns an `(n,m)` matrix with a squeeze rule: `(n,1)→(n,)`, `(1,m)→(m,)`, `(1,1)→(1,)`, and never a 0-d array.

### 3.2 Per-element error semantics (`CoolProp.cpp:404-529`)

| Case (oracle) | Result |
|---|---|
| `T=[300,-5,400]`, `P=101325` | `[996.6, inf, 0.555]`. A failed point is **`+inf`, not NaN**, and no message is recorded for it (`catch (...)` `:428-437`, `:498-501`) |
| `T=[300, nan]` | `[996.6, inf]`. An invalid input cannot be told apart from a failure |
| Multiple outputs, one point bad | That whole row is `inf` |
| Per output in one row | Each `(i,j)` cell is caught separately (`:498-501`), so one output can fail while another succeeds |
| All points fail | `ValueError('No outputs were able to be calculated')` (`:526-529`). The real cause is lost, e.g. two-phase `A` for every point |
| A single point and a single output (`[−5.]` or scalar) | Re-throws the real message (`one_input_one_output`, `:429-431`) |
| Bad fluid, bad output name, bad pair | `ValueError` for the whole call **before** evaluation (`:576-619`): "plan-time" errors |
| Output equals an input (`'T','T',[300,-5]`) | Passes the input through, `[300, -5]`, with no state update and no validation (`:442-452`) |
| Trivial output with a state vector | Broadcast constant, `[647.096, 647.096]` |
| `'D&H'` as Output | Scalar path: "output should be 1x1; error was  :". Vector path: "Output string is invalid [D&H]" (R9) |
| `PropsSImulti` called directly with all points failing | Returns `[]` **silently**; the cause sits in `get_errstr()` |
| Option `USE_GUESSES_IN_PROPSSI` | Point *i* seeds from point *i-1* (`:398, 419-426, 461-525`), which makes the results order-dependent. Default is off |
| Determinism | Vector and scalar loop agree **bitwise** (3,000 PT points, oracle) |

### 3.3 `HAPropsSI` (`nanobind_interface.cxx:1101-1144`)

- The classification and the length-1 broadcast are the same as for `PropsSI`, over 3 inputs. The output type depends on the input type:

  | Input | Output |
  |---|---|
  | All scalars | `float` |
  | Any ndarray | ndarray |
  | Lists or tuples only | **`list`** |

- It **raises on the first non-finite element**, so the whole batch is lost (`:1125-1130`). `PropsSI` fills `inf` instead. This is deliberate legacy parity (comment `:1126-1129`, CoolProp-1tbe.11; pinned by `test_parity.py:178-185`).
- An empty input raises `TypeError('vectorized input Input1 has an incompatible length')` (the other scalars make `n=1`, `:1119-1122`). `PropsSI` returns `[]` for the same input.
- There is no multi-output form. The work is a per-point loop over the scalar C++ function (07).

### 3.4 Cost of each path (oracle, Water PT, 20k points)

| Path | µs/pt (mapper) | µs/pt (verifier re-run) |
|---|---|---|
| Vectorized `PropsSI`, HEOS | 21.3 | 20.3 |
| Scalar `PropsSI` loop, HEOS | 76.5 | 83.1 |
| `AbstractState` loop, HEOS | 21.3 | 20.4 |
| 5-output vector, HEOS | 34.1 | 25.6 |
| Vectorized `PropsSI`, `IF97::Water` | - | 1.09 |
| `AbstractState` loop, IF97 | - | 0.86 |
| IF97 `fast_evaluate` | **0.33** | 0.40 |

The vector path saves the per-call construction cost (11 sec. 5d), but its per-point dispatch costs the same as a reused state. Most of the HEOS-vs-`fast_evaluate` gap is the backend (HEOS vs IF97), not the call path: on the same IF97 backend `fast_evaluate` is only ~2-3x faster than vectorized `PropsSI` or a reused-state loop.

## 4. Data and configuration inputs

| Input | Effect on the Python contract | Where |
|---|---|---|
| `get_global_param_string('fluids_list'/'incompressible_list_*'/'version'/'gitrevision')` | These feed the `__fluids__` etc. attributes at import | `__init__.py:33-37` |
| `LIST_STRING_DELIMITER` (config) | `get_fluid_list` joins with this delimiter (`src/Backends/Helmholtz/Fluids/FluidLibrary.h:1335-1337`), but Python splits on `','`. Setting `'\|'` makes `FluidsList()` return **1** element (oracle). `__fluids__` is computed at import with the default `','`, so it is unaffected in practice | `nanobind_interface.cxx:1164` |
| `aliases_bar` | `get_aliases` splits on `'\|'`, which is safe for `1,2-dichloroethane` (oracle). `INCOMP::` fluids raise | `:1165`; `HelmholtzEOSMixtureBackend.cpp:253` |
| `BibTeX-<KEY>` | `EOS`, `CP0`, `VISCOSITY`, `CONDUCTIVITY`, `SURFACE_TENSION`, `MELTING_LINE` return a key or `''`. `ECS_*` raises `ValueError('')`, which comes from a bare `NotImplementedError()`. An unknown key raises `Bad key`. Keys are case-sensitive | `HelmholtzEOSMixtureBackend.cpp:263-289` |
| `CoolPropBibTeXLibrary.bib` | Shipped; `copy_BibTeX_library` copies it. `BibtexParser` needs `pybtex` and `latexcodec`, which the wheel does not declare. The import fails on a plain install (oracle). Its default path is `'../../../CoolPropBibTeXLibrary.bib'`, relative to the working directory | `BibtexParser.py:6-8, 34`; `pyproject.toml:42-43` (only numpy is declared) |
| `include/` (107 headers) and the `.pxd` files | Exist only for downstream Cython builds (PDSim) | `__init__.py:55-66` |

## 5. State, caching, globals, thread-safety, memory

- **Import-time eager load (measured).**
  - Loading the bare `CoolProp.abi3.so` costs **5 ms and +6 MiB**.
  - `get_global_param_string('fluids_list')` then costs **1,890 ms and +67 MiB**.
  - The first `AbstractState` afterwards takes 0.1-0.3 ms.
  - **Verifier correction:** the C++ core is lazy only until the *first HEOS use*, and then decodes the whole library at once. In a fresh process with the bare `.so` and no `fluids_list` call, the first `AbstractState('HEOS','Water')` costs **1,944 ms and +67.5 MiB**. The first `AbstractState('IF97','Water')` costs ~0 ms, but the following HEOS `PropsSI` then pays 2,027 ms and +67.5 MiB (oracle, `v7.py`/`v8.py`). So `__init__.py:33` (`__fluids__`) only moves an all-or-nothing decode to import time. A PEP 562 module `__getattr__` would help only processes that never touch HEOS. The real rot is the monolithic decode (see 12 sec. 5, which measured "import 1.8 s").
- **Global error outbox.** Scalar and vector Python errors are built from `get_global_param_string("errstring")` (`nanobind_interface.cxx:104-111, 985-989`), a process-global value that reading also clears (11 F5). The slot is mutex-guarded and deliberately shared across threads (`CoolProp.cpp:65-85, 1047-1051`), so there is no UB, but a concurrent unrelated failure can still overwrite the message. In Python the write and the read happen in one call while the GIL is held, so this does not happen there (11 sec. 5). The `_capi` path uses a `thread_local` instead (`:132`).
- **No GIL release** (11 sec. 5): Python callers get no parallelism, either across threads or inside `PropsSI` vectors.
- **Per-call construction:** every `PropsSI` call builds a fresh backend (11 sec. 5d). A vector call amortises one construction over n points.
- **Global mutation** reachable from Python is listed in sec. 2.1, row "Global mutation". `set_reference_state` changes every later state of that fluid in the process (01).
- **Lifetimes.** `_AbstractStateView` holds a raw `void*` that it does not own and no reference to its `State` (`State.pyx:150-156, 282-285`, `State.pxd:6-9`). This leads to the use-after-free in R1.
- **No pickling.** `State` raises "no default __reduce__" and `AbstractState` "cannot pickle". `multiprocessing` fan-out therefore has to rebuild states from strings.

## 6. Rot and bugs

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| R1 | **Use-after-free in the `State.pAS` view** | `v = State('Water',{...}).pAS; v.p()` segfaults with **exit 139** (oracle, `faulthandler`). Verifier re-run: garbage `p() = -7.9e-05`, then SIGBUS (exit 135) after further allocations, so the symptom varies but the crash reproduces. The view stores the handle only (`State.pyx:282-285`), and `State.__dealloc__` destroys it (`:234-236`). Legacy `pAS` was a ref-counted `AbstractState` (`CoolProp.pyx:901`). Not fixed on master (`git log v8.0.0..origin/master -- wrappers/Python/_nanobind/State.pyx` is empty) | Memory unsafety in the shipped wheel. PDSim-style code that keeps `.pAS` past the `State`'s lifetime crashes or reads garbage (it returned 0.0 before the crash) | The view holds `Py<State>` or `Arc<Model>` plus an owned state. Lifetimes are enforced by the borrow checker or by PyO3 |
| R2 | All-or-nothing fluid-library decode, made eager at import | Sec. 5: the first HEOS use in a process costs ~1.9 s and +67 MiB whatever the fluid (a single `AbstractState('HEOS','Water')` pays it all). `__init__.py:33` moves the cost to `import CoolProp` (1.96 s, verifier) | Startup cost and memory for every user, against "load only what is needed" | Per-fluid lazy decode behind an immutable `Arc` cache. Name lists come from a manifest, not decoded fluids. Lazy `__fluids__` via module `__getattr__` in any Python layer |
| R3 | Four different per-element error policies | `PropsSI` vector fills `inf`; `HAPropsSI` vector raises on the first failure; `PropsSImulti` returns `[]` silently; `Props1SI` returns `inf`, while 2-arg `PropsSI` raises (all oracle). Each policy is individually intentional: legacy parity (`nanobind_interface.cxx:1126-1129, 1068-1070`; `CoolProp.cpp:633-652` sets the errstring and returns `{}`) or the C++ "never throws" contract (`CoolProp.cpp:646-648`). The rot is the inconsistency, not a bug | Users cannot write one error handler. Batches are lost or silently wrong | One core result: `values` plus a per-cell `Status`. The compat façade chooses fill/raise per legacy function |
| R4 | Failure causes are discarded | `catch (...)` without a message (`CoolProp.cpp:428-437, 498-501`). All-fail gives the generic "No outputs were able to be calculated" (`:528`) | The vector two-phase `A` error does not say why | A per-cell error kind (enum), with message text produced on demand |
| R5 | `inf` used for both "failed" and "input NaN" | `[300, nan]` → `[.., inf]` | Ambiguous. `inf` is also a legitimate value for some derivatives (inference) | `Status::InvalidInput` versus `Status::SolverFailed` |
| R6 | Shape and type depend on the container | `PropsSI(list)` gives an ndarray; `HAPropsSI(list)` gives a `list`; the stub claims `NDArray` for `HAPropsSI` (`.pyi:1478`) while `test_parity.py:165-167` locks the `list` | The stub lies to type checkers. `test_stub.py:46-50` checks symbol parity; `:53-67` checks return types with mypy (skipped if mypy is absent), but only for three `PropsSI` calls, never `HAPropsSI` | Always return ndarray. The compat façade keeps the legacy list behaviour only where it is pinned. Check stub return types at runtime |
| R7 | Empty input differs between functions | `HAPropsSI('H','T',[],...)` raises `TypeError`; `PropsSI` returns `[]` | Edge-case divergence | One broadcast helper shared by both |
| R8 | `(n,1)` arrays rejected, `range` and generators rejected | Oracle. Legacy accepted `(n,1)` (`CoolProp.pyx:521-524` checks `prod==max`) | Small compat breaks from v7 to v8 | Accept any 1-D-squeezable array and any `Sequence` |
| R9 | `'&'`-joined outputs are half-supported | The scalar C++ `PropsSI` splits on `&` (`CoolProp.cpp:669`) and then demands 1×1 (`:674-676`). The resulting message is "error was  :" | Confusing dead feature | Multiple outputs only through a list |
| R10 | `State.get_Tsat` broken for mixtures | `State('R32[0.5]&R125[0.5]',...).get_Tsat()` → "Mole fractions must be set" (oracle). `State.pyx:374` rebuilds from the bracket-stripped `_fluids`; legacy used `self.Fluid`, which kept the brackets (`CoolProp.pyx:860, 1098`). `State.Fluid` is also now `b'R32&R125'`. `copy()` was fixed for exactly this (`State.pyx:331-333, 346-349`) but `get_Tsat` was not, and `test_state_shim.py` has no mixture `Tsat` test. Not fixed on master | Regression for refrigerant-blend users; `Tsat`, `superheat` and `subcooling` all raise | Composition belongs to the model, so Tsat uses the same `Arc<Model>` |
| R11 | Torn state after a failed update | After `update({'T':-5,...})` fails: `T_` = 368.6 (stale) but `get_T()` = **-5.0** (oracle). The C++ state keeps the rejected input | Silent garbage if a caller catches the error and keeps reading | `update` is transactional: it returns a new `State` value or an `Err` and leaves the old one untouched |
| R12 | Sentinels leak into the API | `Q` = -1 for single phase; `T()` = -inf before an update; `PhaseSI` returns the string `'unknown: <msg>'` (oracle; 01 R22) | Magic numbers in user code | `Option<Quality>`; there is no unset state, because construction requires inputs |
| R13 | Docstrings contradict behaviour | `State.pyx:16-17` (the cond factor); the legacy `get_BibTeXKey` docstring promises `''`/"Bad key" returns and an `ECS_FITS` key (`CoolProp.pyx:645-667`), but the function raises and `ECS_FITS` is invalid | Misleading. The stub carries no docstrings at all | Generate docs from typed signatures. Doctests run in CI |
| R14 | Contract tests miss unit conversions | `get_MM` (g/mol) and `get_cond` (kW/m/K) have no value check in `run_contract.py:51-89` or `test_state_shim.py` (grep). The only `k` use is the unit-free `Prandtl` ratio (`test_state_shim.py:136-138`). `SURFACE.md:54-60` omits both. `get_T` and `get_visc` are also unchecked, but they are SI. The PDSim test is opt-in | The most dangerous ×1000 class of bug (`SURFACE.md:58-60`) is not guarded for the 2 scaled getters out of 15 `get_*` | A compat unit table with an exhaustive test over every getter |
| R15 | `LIST_STRING_DELIMITER` breaks the list APIs | Sec. 4 (oracle) | One global config knob silently corrupts `FluidsList()` (and any user split of `get_global_param_string`). `__fluids__` is built at import with the default, so it is unaffected | Return `Vec<&str>` from the core. Delimiters exist only in the C ABI |
| R16 | Two enum surfaces and header scraping | `constants.py` holds ints and `CoolProp.CoolProp` holds `IntEnum`s for the same 190 names. They are produced by text-parsing C++ (`generate_constants_module.py:11-56`), which also leaks `absolute_import` | Duplication; the generator is fragile | One `#[repr(i32)]` enum source; values pinned to v8 for compat |
| R17 | Self-deleting import (intentional; low severity) | `__init__.py:14-25` deletes a stale `constants` binary left by an older install and calls `quit()`. The comment documents it as a migration guard, and `__init__.py:4-10` copies it verbatim from legacy for parity. It does not fire on a clean v8 install | Surprising side effect at import, but only on polluted installs. A legacy leftover, not a bug | Drop |
| R18 | `HAProps_Aux` prints to stdout and returns a sentinel | `HAProps_Aux('Ha',...)` → `(-1.0, '')` plus "Sorry I didn't understand..." on stdout (oracle) | Silent failure | `Result`; drop from the compat surface |
| R19 | Optional submodules with undeclared dependencies | `BibtexParser` (pybtex) and `Plots` (matplotlib) both raise `ModuleNotFoundError` on a plain install (oracle) | Broken public names | Move them out of the core package |
| R20 | Unpicklable states | Sec. 5. Not a regression: legacy `State.__reduce__` is commented out (`CoolProp.pyx:877-883`) | No process-pool fan-out of states | `State` is a small `Copy`/`serde` value; `Model` is rebuilt by its key |
| R21 | Exception translation is incomplete | The translator maps only `CoolPropBaseError` (`nanobind_interface.cxx:394-400`). A bare `std::exception` (likely source, inference: the uncached-read `throw std::exception()` in `include/CoolProp/detail/CachedElement.h:69, 77, 124, 132`) reaches Python as `RuntimeError('std::exception')`. Oracle: `AbstractState('HEOS','Water').conformal_state('Water',300.,1000.)` on a state that was never updated. `test_parity.py:274-281` tolerates any exception here | `except ValueError` misses it, and the message carries no information | Typed `Error` enum; no untyped throws; the binding maps every variant |

## 7. Parallelism fit

| Surface | Fit | Note |
|---|---|---|
| Vectorized `PropsSI`/`HAPropsSI` | Natural batch boundary, run serially today | One plan per call (fluid, pair, outputs, phase), then N independent points. Bitwise equal to scalar evaluation, so scalar, SIMD and parallel backends can be cross-checked (11 U4, U13) |
| `USE_GUESSES_IN_PROPSSI` | **Sequential by construction** (path-dependent) | Keep only as an explicit "trace" mode, never in `par` |
| `fast_evaluate` | Caller-allocated `(N,M)` + `status[N]`. 65x faster than HEOS `PropsSI` per point, but only ~2-3x faster than IF97 vectorized `PropsSI` (sec. 3.4): most of the gain is the backend | The right ABI shape. Extend `status` to per-cell |
| PyO3 binding | GIL release around kernel calls is required (11 U14) | A Python `ThreadPoolExecutor` over `PropsSI` then scales; the oracle cannot show this (0.97x, 11) |
| Legacy `State` | Mutable, per-object | Single-threaded by design. Fine as a compat object |

## 8. Verification assets

| Asset | Covers | Gap |
|---|---|---|
| `wrappers/Python/pytest/test_parity.py` (360) | The scalar float return; list and ndarray returns; the multi-output squeeze shapes; the HAPropsSI list/ndarray rule | Does not cover per-element `inf`, multi-point all-fail, empty-HA, or the delimiter. Plan-time errors are covered only for bad fluid and bad output (`:56-62, 83-85, 141-146`). It pins HA raise-first (`:178-185`) |
| `test_state_shim.py` (159) | The widened constructor, `Tsat`, mixtures | Not mixture `Tsat`, not `MM`/`cond`, not view lifetime |
| `test_stub.py` (67) | Symbol parity between stub and runtime; mypy return types for 3 `PropsSI` calls (skipped without mypy) | Not `HAPropsSI` return types; no runtime return-type check |
| `test_doc_examples.py` (162) | Executes the RST examples | |
| `test_fast_evaluate.py` (63) | Buffer shapes and status | |
| `dev/pdsim_cimport_contract/*` | The compile-time surface; the kPa/kJ values against `PropsSI` | Opt-in; 2 getters missing |
| The oracle probes in this document | Sec. 3.2 table: broadcasting, shapes, error classes, messages | Turn them into fixtures (U1) |

## 9. Port recommendation

| Unit | Source | Priority | Rust shape | Est. LOC |
|---|---|---|---|---|
| U1 Python-contract fixtures | Sec. 3 probes; `test_parity.py` | **P0-core** | Feeds 10 U2. A JSON table of `(call, inputs) -> {shape, dtype/type, values with inf, error class, message prefix}`. This is the spec for U5 and for the batch `Status` mapping | 200 (py) |
| U2 Batch result model | `CoolProp.cpp:404-529`; `fast_evaluate` | **P0-core** (extends 11 U4 / 01 U15) | `plan(fluid, pair, &[Output], phase) -> Result<Plan, PlanError>`; `plan.eval(&[f64], &[f64], out: &mut [f64] /*N×M row-major*/, status: &mut [CellStatus])`. `CellStatus` = Ok, InvalidInput, OutOfRange, SolverFailed(kind), Undefined (e.g. two-phase `A`). Passthrough and trivial outputs are planned, not special-cased | 300 |
| U3 Typed metadata API | `get_aliases`, `get_REFPROPname`, `get_BibTeXKey`, `FluidsList` | P1-early | `Model::aliases() -> &[String]`; `bibtex(Topic) -> Option<&str>` with `enum Topic {Eos, Cp0, Viscosity, Conductivity, SurfaceTension, MeltingLine}`; `catalog::names()` read from a manifest, with no decoding | 150 |
| U4 PyO3 core module | `nanobind_interface.cxx` | P2-later (only if 11 Q9 = yes) | `coolprop_rs` (abi3). `PropsSI`/`HAPropsSI` take `Sequence`/ndarray, use the U2 broadcast, release the GIL, and return ndarray always. `AbstractState` keeps the Python-shaped returns of sec. 2.2. Enums are pinned to the v8 ints. Lazy module attrs. Generated stub with a return-type check | 1,200 |
| U5 `CoolProp` compat façade | Sec. 3 rules | P2-later | Pure Python over U4. Reproduces fill-`inf`, all-fail `ValueError`, `TypeError` on length mismatch, HA list-in/list-out and raise-first, the squeeze rule, `get_errstr` (thread-local), and the `constants` ints. It must pass U1 | 400 |
| U6 Legacy `State` (Python) | `State.pyx` | defer | Pure Python over U5, with an explicit `LEGACY_UNITS` table (sec. 2.3, including MM and cond) tested for every getter. Fixes R1, R10 and R11 by construction | 250 |
| U7 PDSim cimport contract | `SURFACE.md`, `State.pxd` | defer | PyO3 classes cannot be `cimport`ed. If it is needed, ship a Cython shim over the 11 U9 C ABI, the same capsule pattern as v8 | 300 |
| Drop | `HAProps`, `Props`, `HAProps_Aux`, `_capi`, `Py*` aliases, `get_include_directory`, `test()`, `BibtexParser`, `Plots`/`GUI` (separate package if wanted), the constants self-delete, `set_debug_level` (use `log`), process-global `set_*` mutators (replace with model builders, 01) | drop | Document them in a migration table | 0 |

**What the Rust batch API must be able to express** (so that U5 can be layered on it without kernel changes):
1. An SoA input pair with N points and **M outputs**, producing a row-major N×M result in caller-owned buffers. A length-1 operand broadcasts, but only at the binding layer.
2. **Per-cell status**, not just per-point: one output can fail while another succeeds (`CoolProp.cpp:498-501`). The binding can then reproduce row-`inf`, cell-`inf`, raise-first or all-fail.
3. **Plan-time errors are separate from point errors**: an unknown fluid, output or pair fails before any evaluation.
4. Input validation is distinct from solver failure (R5).
5. Trivial outputs and outputs that equal an input are resolved in the plan; the latter pass through even when the state is invalid (oracle `[300, -5]`).
6. Phase imposition per plan (`"T|gas"`); derivative output strings (`d(H)/d(T)|P`) as outputs; a fixed mixture composition per plan.
7. An optional ordered **trace mode** (the `USE_GUESSES_IN_PROPSSI` equivalent). The default is order-independent, with results bitwise equal to the scalar path.
8. SI-only kernel. Unit adapters (kPa/kJ, g/mol, kW/m/K) live only in the compat layer.
9. Message text is created on demand from a `(kind, point, output)` triple, never through a global outbox.

## 10. Open questions

1. 11 Q9: does a Python binding ship? If it does, is it a drop-in `CoolProp` namespace (U5 by default) or `coolprop_rs` with an opt-in façade?
2. Should the compat façade preserve the `HAPropsSI` raise-on-first and list-in/list-out behaviour, or align it with `PropsSI` (fill `inf`, return ndarray) and document the change?
3. Is `+inf` the fill value for the façade (CoolProp parity) while a native API returns `NaN` plus status? Or does the native API expose no fill at all?
4. Does PDSim compatibility matter to the user? If so, U7 needs the C ABI first. Should U6 also keep the stale-`p_`-free v8 behaviour, or legacy's `update_Trho`/`update_ph`, which never refreshed `p_` (`CoolProp.pyx:905-933`)?
5. Should the oracle harness record `errstring` text verbatim? The messages differ between the scalar path and the 1-element vector path (sec. 3.1, step 6), so exact-text fixtures are brittle. Matching on the error class plus a message prefix is the proposed default.
6. Is per-cell status (N×M bytes) acceptable for very large batches, or should it be per-point by default with per-cell opt-in?
7. Are `get_MM` in g/mol and `get_cond` in kW/m/K the legacy contract PDSim actually relies on? Partly answered: the legacy Cython `State` used the same units (`CoolProp.pyx:992-994, 1084-1086`), so v8 keeps the legacy contract. Whether PDSim calls them still needs a PDSim source check, because no test verifies them (R14).

## Verification log

- **Date:** 2026-10-04. Adversarial verifier pass against v8.0.0 (ae81610e) and the `CoolProp==8.0.0` oracle. The probes were `v1.py`-`v9.py` in the session scratchpad and are not committed.
- **Claims checked:** ~95 in total:
  - every `path:line` citation in sec. 1-6 and 8-10, opened in the reference checkout;
  - every oracle row in sec. 2.1-2.3, 3.1-3.4 and 4-5, re-run;
  - all 20 rot items, each checked against `git log v8.0.0..origin/master` (no fixes found for R1-R20).
- **Recounted:**
  - 259 public names = 47 functions + 15 classes + 6 `IntEnum`s (190 values) + `_capi`: confirmed.
  - `AbstractState` 169: confirmed.
  - `__fluids__`/pure/solution 136/74/52: confirmed.
  - `include/` 107: confirmed.
  - `CoolProp.pyx` + `BibtexParser.py` 1,518: confirmed.
  - PDSim 478: confirmed.
  - RST 1,053: confirmed.
- **Corrections:**
  - Counts:
    - pytest lines 1,756 → 1,784 (1,817 with README).
    - Wheel files 146 → 143 under `CoolProp/`, plus 6 dist-info.
    - "All 169 legacy Cython methods" → 155 legacy public methods, all present, plus 14 v8-only.
    - Legacy-only module name is `Props`, not the `Py*` names, which are aliases.
  - Sec. 5 / R2: the "C++ core is already lazy" conclusion was wrong. The first HEOS `AbstractState` in a fresh process costs ~1.94 s and +67.5 MiB, so `__init__.py:33` only moves an all-or-nothing decode to import time. A PEP 562 lazy attribute does not remove it.
  - Sec. 7 / 3.4: the "65x" figure compared HEOS `PropsSI` with IF97 `fast_evaluate`. On the same IF97 backend `fast_evaluate` is only ~2-3x faster; added IF97 rows.
  - `bytes` as `Output` gives `RuntimeError('std::bad_cast')`, not `TypeError`.
  - Exception translation covers `CoolPropBaseError` only. Added R21 (bare `std::exception` → `RuntimeError`).
  - Oracle value `0.548` → `0.555` (T=400 K, P=101325).
  - HA empty-input message text corrected.
  - Citations corrected:
    - `CoolProp.cpp:677-679` → `:674-676`;
    - legacy `CoolProp.pyx:1100` → `:860, 1098`;
    - enum citation `:601` → all six `nb::enum_` lines;
    - superancillary classes `283-381` → `279-381`;
    - `all_critical_points` at `:708`.
  - R6: `test_stub.py` does check `PropsSI` return types via mypy (`:53-67`); it is not names-only.
  - R14: the gap is 4 of 15 getters unchecked (`get_T`, `get_MM`, `get_visc`, `get_cond`), of which 2 are scaled. Legacy used the same g/mol and kW/m/K units (Q7 partly answered).
  - R15: `__fluids__` is not affected (built at import with the default).
  - Sec. 5 outbox: noted it is mutex-guarded and cross-thread by design (`CoolProp.cpp:65-85`).
- **Refutation outcomes:**
  - Survived with evidence: R1 (UAF reproduced, SIGBUS/SIGSEGV), R4-R12, R13, R16, R18, R19.
  - Survived but labelled intentional: R3 (each policy is deliberate legacy parity or a documented C++ contract; only the inconsistency is rot).
  - Downgraded: R17 (documented migration guard copied from legacy; not a bug). R20 (not a regression: legacy `__reduce__` is commented out).
  - Narrowed: R2 (root cause moved to the monolithic decode), R6, R14, R15.
  - Not verified here: the `SystemError` errno mapping and the cross-references to maps 01/07/11/12.
