# 01 Public API, AbstractState and the state model - CoolProp v8.0.0 map

> Scope: `include/CoolProp/{AbstractState,DataStructures,CoolProp,CoolPropLib,Configuration,FactoryOptions,Exceptions,Hash}.h`, `include/CoolProp/detail/{CachedElement,configuration_keys,state_capi}.h`, `src/{AbstractState,DataStructures,CoolProp,CoolPropLib,Configuration,FactoryOptions}.cpp`, `src/qmass_conversions.h`, `src/l10n/`, specs `docs/superpowers/specs/2026-05-16-backend-options-string-design.md` and `2026-04-30-qmass-support-design.md`. ~10,000 lines (9,206 code + 19 in `src/l10n/english.h` + 774 spec). Followed into `src/Backends/Helmholtz/{HelmholtzEOSMixtureBackend,FlashRoutines}.*`, `src/Backends/Helmholtz/Fluids/FluidLibrary.*`, `include/CoolProp/fluids/Helmholtz.h` where the API contract is decided there. Part of the coolprop-rs port plan; cites the v8.0.0 source (ae81610e).

Conventions: `path:line` cites v8.0.0. "Oracle" = CoolProp==8.0.0 wheel (nanobind build) driven from Python; all oracle results quoted were run for this document. "Inference" marks claims not directly demonstrated.

## 1. Purpose and concepts

`AbstractState` is CoolProp's single abstraction: one mutable C++ object that is at once (a) the fluid **model** (EOS, ancillaries, transport, constants, deep-copied per instance), (b) the current thermodynamic **state** (T, rho, p, Q, phase), (c) a **memo cache** of ~68 derived values, and (d) a **solver workspace** (saturated child states, guesses, imposed phase). Everything else in this area is a front-end onto it.

| Concept | What it is in v8.0.0 | Where |
|---|---|---|
| Input pair | `enum input_pairs` (43 valid + INVALID); a flash is `update(pair, v1, v2)` | `include/CoolProp/DataStructures.h:292-343` |
| Output parameter | `enum parameters` (85 valid + 2 sentinels), metadata table, string aliases | `DataStructures.h:64-178`, `src/DataStructures.cpp:18-153` |
| Phase | `enum phases` (7 physical + `unknown` + `not_imposed`) | `DataStructures.h:183-194` |
| Backend family | `enum backend_families` (12 + INVALID), string table, static-init registry | `DataStructures.h:493-526`, `src/AbstractState.cpp:36-160` |
| Lifecycle | `factory()` -> `set_*_fractions` -> [`specify_phase`] -> `update()` -> lazy getters / `keyed_output()` | `AbstractState.h:740-1679` |
| High-level API | `PropsSI`, `PropsSImulti`, `Props1SI`, `PhaseSI`: string-keyed, build a fresh AbstractState per call | `src/CoolProp.cpp:241-701, 866-934, 1191-1203` |
| C API | `CoolPropLib.h`: 70+ `extern "C"` functions, integer handles, errcode + message buffer | `include/CoolProp/CoolPropLib.h`, `src/CoolPropLib.cpp` |
| Configuration | 38 process-global typed keys, `COOLPROP_<KEY>` env override, JSON get/set | `include/CoolProp/detail/configuration_keys.h:10-99`, `src/Configuration.cpp` |
| Factory options | `"BACKEND::Fluid?{json}"` / `?@file.json` suffix, opt-in per backend (only SVDSBTL in v8.0.0) | `src/FactoryOptions.cpp:21-51`, spec 2026-05-16 |
| Qmass | Mass-basis quality: output `iQmass` + 8 `*Qmass*_INPUTS` pairs (new in v8) | `AbstractState.cpp:317-352, 818-926`, `src/qmass_conversions.h` |
| Reference state | IIR / ASHRAE / NBP / DEF / RESET / custom offsets of alpha0 (mutates the global fluid library) | `src/CoolProp.cpp:946-1040` |
| Batch kernel | `fast_evaluate`: cache-bypassing, allocation-free, per-point status; opt-in (IF97, Tabular, SVDSBTL only) | `AbstractState.h:889-924` |

Units: strict SI (K, Pa, mol/m^3, kg/m^3, J/mol, J/kg, J/mol/K, J/kg/K, Pa s, W/m/K, N/m). The canonical internal basis is **molar**; mass inputs are converted before the flash (`mass_to_molar_inputs`, `AbstractState.cpp:317-450`) and mass outputs are molar / M at the getter (`AbstractState.h:536-580`). `Q` is molar (`DataStructures.h:91`). IF97 and INCOMP are internally mass-based. A legacy kSI layer (kPa, kJ) survives in the C API (`src/CoolPropLib.cpp:65-120, 152-190`).

## 2. Structure (key types/functions -> path:line)

| Item | Role | path:line |
|---|---|---|
| `class AbstractState` | God-interface: 176 virtual member functions (134 default-throw `NotImplementedError`, 8 pure), of which 138 are `calc_*` hooks (117 default-throw); ~180 public methods (185 declarations, 178 distinct names; verifier parse) | `include/CoolProp/AbstractState.h:77-1680` |
| protected state | `_phase`, `imposed_phase_index`, `CacheArray<80> cache`, `_critical`, `_reducing`, bulk `_rhomolar,_T,_p,_Q`, 68 `CAE` cache handles | `AbstractState.h:79-150` |
| `factory(backend, fluids)` | String dispatch: `?options` stripping, registry lookup, inline TTSE/BICUBIC/SVDSBTL, `"?"` recursion | `src/AbstractState.cpp:168-289` |
| `BackendLibrary` / `register_backend` / `GeneratorInitializer` | Static-initialiser registry of generators | `AbstractState.cpp:36-62`, `AbstractState.h:1689-1719` |
| generators | IF97, SRK, PR, INCOMP, VTPR, PCSAFT (`AbstractState.cpp:84-160`), HEOS (`src/Backends/Helmholtz/HelmholtzEOSMixtureBackend.cpp:51-67`), REFPROP (`src/Backends/REFPROP/REFPROPMixtureBackend.cpp:179-200`) | |
| `clear()` / `clear_comp_change()` | Invalidate cache + bulk values / composition-dependent values | `AbstractState.cpp:293-316` |
| `mass_to_molar_inputs` | Pair + value rewrite to molar basis; Qmass->Q for pure | `AbstractState.cpp:317-450` |
| `trivial_keyed_output` / `keyed_output` | `switch` from `parameters` to getters | `AbstractState.cpp:451-644` |
| lazy getters | `if (!_x) _x = calc_x(); return _x;` pattern | `AbstractState.cpp:646-930`, `AbstractState.h:1555-1679` |
| `calc_Qmass`, `update_Qmass_pair` | Qmass output; TOMS748 on Qmolar for mixtures | `AbstractState.cpp:818-926` |
| `get_dT_drho`, `get_dT_drho_second_derivatives` | (dX/dT)_rho, (dX/drho)_T tables for 19 / 12 variables | `AbstractState.cpp:981-1238` |
| `calc_first_partial_deriv`, `calc_second_partial_deriv` | Generic Jacobian-ratio derivatives | `AbstractState.cpp:1239-1291` |
| `SimpleState`, `CriticalState`, `SsatSimpleState`, `GuessesStructure`, `SpinodalData` | POD helpers; `_HUGE` = "unset" | `DataStructures.h:16-55`, `AbstractState.h:22-57` |
| `generate_update_pair<T>` | (key1,val1,key2,val2) -> ordered pair; 35 of 43 pairs | `DataStructures.h:380-469` |
| `parameter_info_list`, `ParameterInformation` | name / IO / units / description / trivial; 24 legacy aliases | `src/DataStructures.cpp:18-169` |
| `is_valid_first_derivative` etc. | `"d(P)/d(T)\|Dmolar"`, `"d(P)/d(T)\|sigma"`, `"d(d(P)/d(T)\|Dmolar)/d(T)\|Dmolar"` | `DataStructures.cpp:227-358` |
| `input_pair_list`, `split_input_pair` | Pair name tables / pair -> two parameters | `DataStructures.cpp:504-787` |
| `PropsSI` -> `_PropsSImulti` -> `_PropsSI_initialize` / `StripPhase` / `output_parameter::get_output_parameters` / `_PropsSI_outputs` | High-level pipeline | `src/CoolProp.cpp:654-701, 576-620, 241-299, 532-574, 301-341, 343-530` |
| `extract_backend`, `extract_fractions` | `"BACKEND::"`, `"A[0.5]&B[0.5]"`, `"MEG-20%"`, legacy `REFPROP-` prefixes | `CoolProp.cpp:113-239` |
| `Props1SI`, `is_valid_fluid_string` | State-independent outputs; arg order auto-detected by constructing backends | `CoolProp.cpp:866-934` |
| `set_reference_stateS/D` (free + per-instance) | Mutates fluid library offsets | `CoolProp.cpp:946-1040`, `HelmholtzEOSMixtureBackend.cpp:4463-4560` |
| `get_global_param_string` | version, errstring (read-and-clear), lists, schemas | `CoolProp.cpp:1042-1082` |
| `Configuration`, `ConfigurationItem` | union + string item, env override, unordered_map | `include/CoolProp/Configuration.h:48-259`, `src/Configuration.cpp:62-191` |
| `CoolPropBaseError` + 14 typed aliases (9 direct + 5 `ValueErrorSpec`) | Exception hierarchy, `ErrCode` enum (14) | `include/CoolProp/Exceptions.h:11-78` |
| `CachedElement`, `CacheArrayElement<T>`, `CacheArray<N>` | Flag+value cache; handles hold references into the array | `include/CoolProp/detail/CachedElement.h:35-171` |
| `parse_factory_options` | Split on first `?`; `@path` reads file | `src/FactoryOptions.cpp:21-51` |
| `fnv1a_64`, `to_hex16` | Cache-key hash of canonical option JSON | `include/CoolProp/Hash.h:24-50` |
| `CoolProp_StateCAPI` | C function table for the Python `State` shim (PDSim) | `include/CoolProp/detail/state_capi.h:29-55` |
| `Qmolar_to_Qmass`, `Qmass_to_Qmolar` | Pure algebra | `src/qmass_conversions.h:9-20` |
| `AbstractStateLibrary handle_manager` | C-API handle registry | `src/CoolPropLib.cpp:478-510` |
| `l10n/english.h` | 4 message strings; never included anywhere (dead) | `src/l10n/english.h:13-16` |

## 3. Algorithms and formulas (cite the papers CoolProp cites)

**Basis conversion** (`AbstractState.cpp:378-441`): with M = `molar_mass()` of the current composition, rho_molar = rho_mass / M; h,s,u_molar = (h,s,u)_mass * M. Pure fluids: Qmass == Qmolar so the 8 Qmass pairs are rewritten to their molar siblings (`:323-352`).

**Qmass** (`src/qmass_conversions.h:9-20`): Qmass = Q M_V / (Q M_V + (1-Q) M_L); inverse nV/(nV+nL) with n = Q/M. Output path `calc_Qmass` (`AbstractState.cpp:822-832`) needs phase molar masses (`calc_phase_molar_masses`, `:833-841`; mixtures override). Mixture *input* path `update_Qmass_pair` (`:842-926`): TOMS748 (Boost) on Qmolar in [1e-12, 1-1e-12], 48 bits, max 50 iterations; **every residual evaluation is a full mixture flash**, then a final re-flash; spec estimates 5-8x a normal flash (`qmass spec:249`). `_Qmass` is then cached as the *target*, not the recomputed value (`:925`).

**Ideal-gas and derived properties** (base-class formulas usable by any Helmholtz backend):
- h0 = R T (1 + tau a0_tau); s0 = R (tau a0_tau - a0); u0 = R T tau a0_tau (`AbstractState.cpp:704-753`).
- kappa_T = (1/rho)(drho/dp)_T; beta = -(1/rho)(drho/dT)_p; kappa_s = (rho/p)(dp/drho)_s (`:950-958`, defs `AbstractState.h:208-219`).
- Fundamental derivative Gamma = 1 + rho_mass (d2p/drho2)_s / (2 w^2) - Colonna et al., FPE 2010, Eq. 1 (`:975-978`).
- PIP - Venkatarathnam & Oellrich (cited in `AbstractState.h:1302`), implemented per backend.
- neff = -3 (Ar01 - Ar11) / Ar20 (`:728-735`); code carries no citation (inference: the effective-hardness quantity of Bell et al. used for entropy scaling).
- Prandtl = cp mu / lambda, computed every call, uncached (`AbstractState.h:1530-1532`).
- Tangent plane distance with fugacities, citing GERG-2004 monograph Table 7.3 (`AbstractState.h:1063-1088`).

**Generic partial derivatives** (`AbstractState.cpp:1239-1291`; docs `AbstractState.h:1327-1360`): with every variable expressed as X(T, rho),
(dA/dB)_C = [A_T C_rho - A_rho C_T] / [B_T C_rho - B_rho C_T]. `get_dT_drho` (`:981-1151`) supplies (X_T, X_rho) from alpha0/alphar derivatives for T, Dmolar, Dmass, P, H, S, U, G (molar+mass), Tau, Delta, Cv, Cp, speed of sound (19 keys). Second derivatives differentiate N/D again (`:1248-1291`) with `get_dT_drho_second_derivatives` (`:1152-1238`) covering only T, Dmass, Dmolar, Tau, Delta, P, H, S, U - so second derivatives of G, Cp, Cv, w throw (oracle: `d(d(Gmass)/d(T)|P)/d(T)|P` -> "input to get_dT_drho_second_derivatives[Gmass] is invalid"). Second-derivative derivations credit Thorade & Saadat, Environ. Earth Sci. 70 (2013) 3497-3503, DOI 10.1007/s12665-013-2394-z (`:1156-1157`). This engine is backend-agnostic and branch-free once (Of, Wrt, Const) are fixed: a clean port target.

**Saturation / two-phase derivatives** (API only here; implemented in HEOS): Clausius-Clapeyron (dT/dp)_sigma = T (v''-v')/(h''-h'), Thorade & Saadat 2013 (`AbstractState.h:1362-1449`); splined two-phase drho/dh, drho/dp per Quoilin, Bell, Desideri, Dewallef, Lemort, Energies 7(3) 1621-1640 (`:1451-1473`).

**Reference states** (`CoolProp.cpp:946-1040`): IIR h=200 kJ/kg, s=1 kJ/kg/K sat. liquid 0 C; ASHRAE h=s=0 sat. liquid -40 C; NBP h=s=0 sat. liquid 1 atm; DEF; RESET; custom (T, rho, h0, s0). Offsets on alpha0: Delta a1 = Delta s / (R/M), Delta a2 = -Delta h / ((R/M) T_red) (`:978-981`; doc `AbstractState.h:815-819`).

**PropsSI pipeline** (`CoolProp.cpp:654-701`): `extract_backend` (`::`) -> `extract_fractions` (`[x]` or `-x%`) -> `strsplit('&')` -> factory -> set fractions -> `StripPhase` (`"T|liquid"`) -> `is_valid_parameter` x2 -> `generate_update_pair` -> parse outputs (incl. derivative grammar) -> `update` -> `keyed_output` / derivative calls -> 1x1 check; on error `set_error_string` and return `+inf` (`_HUGE`, `include/CoolProp/numerics/numerics.h:17-22`).

## 4. Data and configuration inputs

### 4a. Output parameters (85 = 87 enum members - INVALID - iundefined; 14 flagged "IO", 71 "O", 28 trivial)

| Group | Count | Members (short names) | Notes |
|---|---|---|---|
| Trivial / state-independent | 28 | gas_constant, molar_mass, acentric, rhomolar/rhomass_reducing, rhomolar/rhomass_critical, T_reducing, T_critical, p_critical, p_reducing, T_triple, p_triple, T_min, T_max, P_max, P_min, dipole_moment; fraction_min/max, T_freeze (INCOMP); GWP20/100/500, FH, HH, PH, ODP | `P_min` silently = p_triple (`AbstractState.cpp:469-471`); `rhomass_reducing` unreachable (sec. 6) |
| Bulk state | 6 | T, P, Q, Qmass, Tau, Delta | Tau/Delta flagged "IO" (`DataStructures.cpp:36-37`) but no input pair uses them |
| Molar thermodynamic | 15 | Dmolar, Hmolar, Smolar, Umolar, Gmolar, Helmholtzmolar, Cpmolar, Cvmolar, Cp0molar; H/S/G_molar_residual; H/S/U_molar_idealgas | residual only molar |
| Mass thermodynamic | 12 | Dmass, Hmass, Smass, Umass, Gmass, Helmholtzmass, Cpmass, Cvmass, Cp0mass; H/S/U_mass_idealgas | no mass residuals |
| Transport | 4 | viscosity, conductivity, surface_tension, Prandtl | |
| Derivative-based | 5 | speed_of_sound, isothermal_compressibility, isobaric_expansion_coefficient, isentropic_expansion_coefficient, fundamental_derivative_of_gas_dynamics | |
| Helmholtz terms | 8 | alphar, dalphar_dtau, dalphar_ddelta, alpha0, dalpha0_dtau, dalpha0_ddelta, d2alpha0_ddelta2, d3alpha0_ddelta3 | arbitrary subset; 2nd-4th alphar derivs only via low-level API |
| Other | 6 | Bvirial, Cvirial, dBvirial_dT, dCvirial_dT, Z, PIP | virial units "-" in table (wrong, they are m^3/mol, m^6/mol^2) |
| Phase | 1 | Phase (index as double) | |
| Derivative strings | open | `d(X)/d(Y)\|Z` (19 vars), `d(d(X)/d(Y)\|Z)/d(W)\|V` (X, Y, Z from 12 vars; W, V from 19), `d(X)/d(Y)\|sigma` | parsed at call time |

Not reachable as parameters at all: excess properties (`hmolar_excess` etc.), fugacity/chemical potential, mole fractions, phase envelopes, critical points, spinodal, contributions of transport. Aliases (`DataStructures.cpp:130-153`, 24): D,H,S,U,G->mass; C->Cpmass; O->Cvmass; M,molemass,molarmass; V,L,A,I; pcrit, Pcrit, Tcrit, Ttriple, ptriple, rhocrit, Tmin, Tmax, pmax, pmin. Names are case-sensitive but every name's all-uppercase form is also registered (`:157-160`): "DMASS" works, "dmass"/"t" fail (oracle).

### 4b. Input pairs (43) and support

Physical pairs (basis-free): QT, PQ, QS, HQ, DQ (quality, 5) and PT, DT, HT, ST, TU, DP, HP, PS, PU, HS, SU, DH, DS, DU (14) = 19. Basis variants expand them to 43 (8 Qmass + 8 Q + PT + 8 x-T + 8 x-P + 4 H/S/U + 6 D-x).

| Backend (v8.0.0) | Native arms in `update()` | Notes |
|---|---|---|
| HEOS | 18 molar (`HelmholtzEOSMixtureBackend.cpp:1475-1578`) + 13 mass via conversion + 8 Qmass = 39 | Unsupported: HmassT, TUmass (conversion commented out `AbstractState.cpp:356,383,388`), SmolarUmolar, SmassUmass (no arm). QS/HQ only at Q in {0,1} for pure fluids (oracle: "non-unity quality not currently allowed for HQ_flash"). Oracle: 31 pairs pass a generic Water probe; the 12 failures are exactly these. Probe with fresh instances: reusing one instance after a failed QS update makes later SmolarT/TUmolar/PT updates fail too (R12). |
| IF97 | 9: PT, QT, PQ, HP (mass/molar), PS (mass/molar), HS (mass/molar) | `src/Backends/IF97/IF97Backend.h`; oracle-confirmed |
| Cubics SRK/PR/VTPR | 18 molar case labels; PT/QT/PQ/DmolarT native, the other 14 delegate to `HelmholtzEOSMixtureBackend::update` | `src/Backends/Cubics/CubicBackend.cpp:342-385` (`AbstractCubicBackend::update`) |
| PCSAFT | 4 molar: PT, QT, PQ, DmolarT. The other 12 molar case labels fall through to the default "not yet supported" throw (`src/Backends/PCSAFT/PCSAFTBackend.cpp:1858-1871`). Oracle: SmolarT/HmolarP/PSmolar/DmolarP throw | corrected by verifier (was "16 molar") |
| INCOMP | PT, DmassP, HmassP, PSmass, and QT at Q=0 only. The PUmass arm is commented out (`src/Backends/Incompressible/IncompressibleBackend.cpp:99-133`) | mass-based |
| TTSE/BICUBIC | 9 molar arms (+ mass conversion) | `src/Backends/Tabular/TabularBackends.cpp` |
| SVDSBTL | PT, DT, HP, PS (mass and molar) | |
| REFPROP | 31 incl. HmassT, TUmass, SU | not a port target |

`generate_update_pair` (`DataStructures.h:380-459`) has no branch for QS, HQ (molar or mass, Q or Qmass): **8 pairs cannot be reached from PropsSI** (oracle: `PropsSI("T","Q",0.5,"Hmass",1e6,"Water")` -> "Input pair variable is invalid").

### 4c. Phases

`iphase_liquid, supercritical (p>pc,T>Tc), supercritical_gas (p<pc,T>Tc), supercritical_liquid (p>pc,T<Tc), critical_point, gas, twophase, unknown, not_imposed` (`DataStructures.h:183-194`); strings `phase_*` (`DataStructures.cpp:366-376`) and bare names (`CoolProp.cpp:1168-1190`). Single-phase states report `Q() == -1` (oracle, HEOS/IF97/SRK) - a sentinel.

### 4d. Configuration (38 keys; 21 bool, 8 string, 5 int, 4 double) and environment

| Concern | Keys | Read sites |
|---|---|---|
| HEOS flash numerics (change results) | CRITICAL_WITHIN_1UK, CRITICAL_SPLINES_ENABLED, DONT_CHECK_PROPERTY_LIMITS (12 sites), HENRYS_LAW_TO_GENERATE_VLE_GUESSES, ENABLE_SUPERANCILLARIES (14 sites), ENABLE_MELTING_CALORIC_HS, HSU_D_TWOPHASE_EOS_POLISH, ASSUME_CRITICAL_POINT_STABLE, MIXTURE_STABILITY_ALGORITHM, SPINODAL_MINIMUM_DELTA, PHASE_ENVELOPE_STARTING_PRESSURE_PA | FlashRoutines, VLERoutines, HEOS backend |
| Model construction | R_U_CODATA (8.31446261815324; 11 sites incl. generators `AbstractState.cpp:113,122,144`), NORMALIZE_GAS_CONSTANTS | |
| Library mutation policy | OVERWRITE_FLUIDS, OVERWRITE_DEPARTURE_FUNCTION, OVERWRITE_BINARY_INTERACTION | |
| High-level/wrapper | USE_GUESSES_IN_PROPSSI, FLOAT_PUNCTUATION, LIST_STRING_DELIMITER, ALLOW_SVDSBTL_IN_PROPSSI | |
| REFPROP | 9 keys (paths, GERG/PR switches, error threshold, alias resolution) | |
| Tabular / SVDSBTL | SAVE_RAW_TABLES, ALTERNATIVE_TABLES_DIRECTORY, ALTERNATIVE_SVDTABLES_DIRECTORY, MAXIMUM_TABLE_DIRECTORY_SIZE_IN_GB, SVDSBTL_SAMPLING_THREADS, TABULAR_NX, TABULAR_NY | |
| VTPR | VTPR_UNIFAC_PATH, VTPR_ALWAYS_RELOAD_LIBRARY | |

Environment: each key is overridden at first config access by `COOLPROP_<KEY>` (`Configuration.h:172-258`; bool accepts only True/true/False/false; int/double via `std::stoi/stod`). Six **hidden** switches bypass the config system as function-local statics read once: `COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY` (`FluidLibrary.cpp:47`, `FluidLibrary.h:393`), `COOLPROP_DISABLE_SUPERANC_HSU_D` (`FlashRoutines.cpp:1853`), `PXFLASH_DIRECT_EOS` and `PXFLASH_INNER_NEWTON` (`FlashRoutines.cpp:2836-2841`, no COOLPROP_ prefix), `COOLPROP_DISABLE_MELTING_CALORIC_HS` (`:4743`), `COOLPROP_DISABLE_SUPERANC_HS` (`:4825, 4901`).

### 4e. Backend-options string (spec 2026-05-16)

Grammar `<backend-and-fluid>[?<json>|?@<path>]`, split on the first `?` (`FactoryOptions.cpp:24`); empty/whitespace tail = no options. Options are raw JSON validated by the backend (only SVDSBTL opts in; `SVDSBTLBackend.cpp:229-237` validates against `kSVDSBTLOptionsSchemaJson`, canonicalises via sorted-key `dump()`). Default generator rejects non-empty options (`AbstractState.cpp:69-82`). `build_options_json()` returns the canonical form (`AbstractState.h:787-789`); FNV-1a 64 prefix keys caches (`Hash.h`). Design goals are good (immutable, per-instance, strict, canonical, reproducible: spec:38-62) but the string transport is fragile (sec. 6 R20). Master extends it to BICUBIC/TTSE (10416845).

## 5. State, caching, globals, thread-safety, memory

### 5a. Lifecycle (HEOS, the reference backend)

1. `factory("HEOS","Water")` -> `new HelmholtzEOSBackend(name)`: copies `CoolPropFluid` out of the global library (`HelmholtzEOSMixtureBackend.cpp:77-80`, `:114`), builds reducing function, then **two more full backends SatL/SatV** with their own component copies (`:141-150`).
2. Mixtures: `set_mole_fractions` -> `clear_comp_change()` (resets molar mass, critical, reducing, R; `AbstractState.cpp:293-302`).
3. Optional `specify_phase(p)` sets `imposed_phase_index` **and overwrites `_phase`** (`HelmholtzEOSMixtureBackend.h:246-249`); sticky until `unspecify_phase()`.
4. `update(pair, v1, v2)`: Qmass on mixture -> `update_Qmass_pair` (`HelmholtzEOSMixtureBackend.cpp:1459-1462`); else `pre_update` = `clear()` (base cache + per-component EOS term caches, `.h:154-162`) -> composition check -> `mass_to_molar_inputs` -> `gas_constant()` -> `calc_reducing_state()` (`.cpp:1436-1453`); then the pair switch writes the input fields and calls a `FlashRoutines::*_flash(*this)` that mutates the object in place (`:1475-1578`); `post_update` validates finiteness of p, T, rho, sets tau/delta and, for mixtures, mutates the excess-term model (`residual_helmholtz->Excess.update`, `:1647-1681`).
5. Getters: `if (!_hmolar) _hmolar = calc_hmolar();` (`AbstractState.cpp:696-699`). Invalidation is purely "every mutation path calls `clear()` first"; the cached flag is not tied to (T, rho). `set_T()` (`AbstractState.h:770-772`) breaks this.

Failure semantics: an exception mid-`update` leaves the object half-mutated (cache cleared, some inputs written). Quality range checks run *after* `pre_update` cleared the state (`.cpp:1546-1575`) and in `update_with_guesses` *after* `_Q` is assigned (`:1623-1639`).

### 5b. Per-instance state (measured)

- `sizeof(AbstractState)` = **2000 B** (compiled probe against the v8.0.0 headers): `CacheArray<80>` 728 B, 68 `CacheArrayElement` handles x 16 B = **1088 B of pure indirection**, 2 x `SimpleState` 112 B, bulk scalars. 68 of 80 cache slots used.
- Whole instance (oracle, 1000 instances, fresh process): HEOS Water ~111 KiB, CO2 ~90 KiB, R134a/n-Propane ~66 KiB, R32&R125 ~164 KiB; SRK Propane ~17 KiB; IF97/INCOMP ~0 KiB (share globals).
- Construction (oracle, Python-bound): HEOS Water 59 us, R32&R125 205 us. `PropsSI("D","T",...,"P",...,"Water")` 81 us vs. `update()+rhomass()` on a reused instance 20 us: ~75 % of a PropsSI call is object construction. (Verifier re-run on another load: 64 us vs 14 us, construction 49 us; Water ~111 KiB/instance reproduced. Timings depend on the machine.)
- Root cause of the copies: the model objects double as evaluation caches. `BaseHelmholtzContainer` holds `std::array<double,16> cache` + `is_cached` flags keyed by nothing (`include/CoolProp/fluids/Helmholtz.h:747-763`), cleared by every `update` (`HelmholtzEOSMixtureBackend.h:154-162`). The EOS cannot be shared between states or threads.

### 5c. Global and shared state

| Global | Where | Mutated by | Thread-safety in v8.0.0 |
|---|---|---|---|
| Backend registry `BackendLibrary` | `AbstractState.cpp:55-62` | static initialisers before main | read-only after init; static-init registration pattern |
| Lookup tables (parameter, phase, scheme, input pair, backend) | `DataStructures.cpp:163-169, 393-399, 456-462, 577-583, 846-852` | lazy `if (!p) p = make_unique` | **racy first use** (fixed on master c17783e6: "83 races and a SEGV" under TSAN) |
| `Configuration pconfig` | `Configuration.cpp:126-133` | `set_config_*`, env at init | racy init (fixed c17783e6); get/set of `std::string`/map items remain unsynchronized |
| Fluid library `JSONFluidLibrary` | `FluidLibrary.cpp:28-62` | `call_once` load of **all** fluids; `add_fluids_as_JSON`; `set_reference_stateS` | load is safe; later mutation unsynchronized vs. concurrent factory reads |
| errstring / warnstring | `CoolProp.cpp:83-85, 98-105, 1047-1056` | every failing high-level call | mutex since #3211; still one slot, last-writer-wins across threads (comment `:79-82`) |
| `debug_level` | `CoolProp.cpp:64` | `set_debug_level` | atomic; 30+ hot-path reads print to stdout |
| C-API handles | `CoolPropLib.cpp:478-510` | factory/free | map guarded, but `get()` returns a reference after unlocking (`:500-508`); no per-handle lock; handles never reused, no eviction |
| HumidAir states | `src/HumidAirProp.cpp:44-53` | per call | `thread_local` HEOS/IF97 instances: one per thread, never evicted |
| Tabular data | `src/Backends/Tabular/TabularBackends.cpp:24` | table builds | out of area (Tabular map) |
| SVDSBTL surfaces | master 313b8108 adds a process-wide bounded LRU | | out of area |
| Hidden env statics | `FlashRoutines.cpp:1853, 2836-2841, 4743, 4825, 4901` | first read | immutable after first read |

PropsSI has **no** AbstractState cache: every call constructs (`CoolProp.cpp:255, 263, 273`), as the config docs admit ("PropsSI rebuilds the AbstractState on every call", `configuration_keys.h:81-85`). `Props1SI` makes three factory attempts: two in `is_valid_fluid_string` (`CoolProp.cpp:867-868`, `:921-934`; normally one of them fails), then one in `PropsSI` (`:883`).

An AbstractState instance is not shareable: concurrent use of one instance (or one C handle) races on cache, bulk fields, SatL/SatV and EOS term caches. The only safe parallel pattern is one instance per thread (what HumidAir does).

### 5d. Memory / lazy loading

- First library use decodes **every** fluid from embedded CBOR (`FluidLibrary.cpp:46-62`). The Python package forces it at import (`wrappers/Python/_nanobind/__init__.py:33`, `fluids_list`): measured `import CoolProp` ~2.0 s, after which the first factory costs 0.1 ms. "Load only what is needed" is absent.
- Every instance then holds 1-3 private copies of each component's full fluid record.

## 6. Rot and bugs

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| R1 | Integer enums renumbered between releases while used as the C ABI | v7.2.0->v8.0.0: 64/86 `parameters` and 34/36 `input_pairs` renumbered (Qmass inserted mid-enum; scripted diff of `include/DataStructures.h` vs `include/CoolProp/DataStructures.h`). qmass spec:34-49 orders mid-enum insertion yet :337 claims "existing enum values ... unchanged". Master inserts `iHmolar_formation` after `idipole_moment` (renumbers iT...) while appending backend families "because these enums are serialized by value" | C/Excel/LabVIEW callers using `AbstractState_update(long pair)`/`keyed_output(long param)` compiled against v7 silently get wrong properties | Rust enums are never the ABI. Stable string names at boundaries; any C ABI uses explicit `#[repr(u16)]` append-only discriminants pinned by a test |
| R2 | Input-pair name table collides | `DataStructures.cpp:513-520`: QSmolar/QSmass both "QS_INPUTS", HmolarQ/HmassQ both "HQ_INPUTS", same for Qmass variants; `index_map.emplace` keeps the first (`:572`); `:521` labels DmassQ "Molar density kg/m^3" | `get_input_pair_index` cannot name HmassQ/QSmass; name round-trip maps HmassQ -> HmolarQ (J/kg read as J/mol) | Names generated from the enum by one macro; exhaustive round-trip test |
| R3 | 8 quality pairs unreachable from PropsSI | `generate_update_pair` lacks Q+S, Q+H branches (`DataStructures.h:380-459`); oracle "Input pair variable is invalid" | Documented inputs silently unusable in the high-level API | Pair construction from a single (Var, Var) table; test all 19 physical pairs x bases |
| R4 | Advertised pairs unsupported by HEOS | HmassT/TUmass conversions commented "NOT CURRENTLY IMPLEMENTED" (`AbstractState.cpp:356,383,388`); no SmolarUmolar arm (`HelmholtzEOSMixtureBackend.cpp:1475-1578`); oracle "This pair of inputs [HmassT_INPUTS] is not yet supported" while HmolarT works | Basis asymmetry; users cannot predict support | Normalise basis once for every pair; backends declare supported physical pairs (capability), compile-time where possible |
| R5 | `rhomass_reducing` unreachable; dead dispatch | trivial param without `trivial_keyed_output` case (`AbstractState.cpp:451-512`); oracle Props1SI fails; `keyed_output` cases for molar_mass, T_reducing, rhomolar_reducing are dead (`:583-588`) | Advertised output fails | One exhaustive `match` over an `Output` enum (compiler-checked) |
| R6 | `USE_GUESSES_IN_PROPSSI` breaks most outputs | `CoolProp.cpp:461-481`: default arm throws "Don't understand this parameter"; oracle with key set: `PropsSI("Dmass",...)` fails, `Dmolar` works | Global flag silently changes which outputs work | Guesses are an explicit per-call option; no global |
| R7 | Output==input shortcut skips validation | `CoolProp.cpp:368-389, 441-454`; oracle `PropsSI("T","T",-5,"P",101325,"Water")` -> -5; `("P","T",1e9,"P",-3)` -> -3; `("Q","T",300,"Q",7)` -> 7 | Physically impossible inputs echoed as results | Validated newtypes (`Temperature::new -> Result`), quality in [0,1] at construction |
| R8 | PropsSI rebuilds the backend every call | `CoolProp.cpp:255,263,273`; `configuration_keys.h:81-85`; measured 81 us vs 20 us; Props1SI builds 3 (`:867-883`) | 4x overhead on the dominant usage path; SVDSBTL/tabular disabled in PropsSI because of it | `Arc<Model>` cache keyed by canonical fluid spec; `State` is a cheap value |
| R9 | Model data copied per instance; EOS holds evaluation caches | `HelmholtzEOSMixtureBackend.cpp:77-80,114,141-150`; `Helmholtz.h:747-763`; measured 65-164 KiB/instance | Memory x concurrency; no sharing across threads | Immutable `Arc` model, `Send + Sync`; derivative bundle returned by value |
| R10 | Eager decode of the whole fluid library | `FluidLibrary.cpp:46-62`; Python import ~2.0 s (`_nanobind/__init__.py:33`) | Startup cost, memory for unused fluids, WASM-hostile | Per-fluid lazy decode behind `OnceLock` from an index |
| R11 | Cache machinery hazards | `CacheArrayElement` stores references into the owner (`CachedElement.h:96-97`): implicit copy aliases the source (latent; no copy path found in src); `next()` checks `inext > N` (off-by-one, `:163-165`); `clear()` memsets values to 0 not `_HUGE` (`:156-159`); uncached read throws bare `std::exception()` (`:69,124`); dead `_fugacity_coefficient` (`AbstractState.h:131`); `_cp0molar` slot never filled (`AbstractState.cpp:782-784`); `set_T` bypasses invalidation (`AbstractState.h:770-772`) | 1088 B overhead per object; stale-value bugs depend on discipline | No flag caches: compute a derivative bundle once per state (eager or `OnceCell`); `State` immutable |
| R12 | Sticky phase imposition mutates the solved state | `HelmholtzEOSMixtureBackend.h:246-249`; oracle: after `specify_phase(gas)` a liquid state reports gas (rho 996.6); imposed gas at 1 atm/372 K returns metastable vapour 0.5996 kg/m^3 silently; `specify_phase(not_imposed)` called after an update makes `phase()` return not_imposed, and then `hmass()` throws "phase is invalid in calc_hmolar" (oracle; warned in `state_capi.h:45-50`). (added by verifier) The flash routines impose phase themselves: `QS_flash` calls `HEOS.specify_phase(iphase_twophase)` (`FlashRoutines.cpp:648`) and never lifts it, and the same holds at `:726, 768, 806, 881, 1009, 1027, 1039`. Oracle: after a *successful* `update(QSmolar_INPUTS, 0, s_L)` on Water, a later `update(PT_INPUTS, 101325, 300)` on the same instance throws "Bad phase to solver_rho_Tp_SRK". `QS_flash` on origin/master still imposes the phase this way | Wrong roots persist across later updates; phase() lies; one QS update permanently breaks an instance | Per-call `PhaseHint` in flash options; result `Phase` never unknown/not_imposed; report metastability |
| R13 | Non-transactional update, inconsistent input validation | `HelmholtzEOSMixtureBackend.cpp:1436-1453` then Q checks `:1546-1575`; `update_with_guesses` assigns `_Q` first (`:1623-1639`); cubic arms had no Q check: oracle SRK Propane `QT(5, 300)` -> rho 102.18, Q 5 (fixed master aa7c6079). (added by verifier) The `update_with_guesses` PQ and QT arms have no quality check at all (`HelmholtzEOSMixtureBackend.cpp:1608-1617`) | Half-mutated objects after errors; garbage accepted | `fn flash(&Model, Input) -> Result<State>`; prior state untouched; validation in one place |
| R14 | Reference state is global mutable library state | `CoolProp.cpp:946-1027` -> `FluidLibrary.cpp:64-127` unsynchronized; oracle: after `set_reference_state("R134a","ASHRAE")` a pre-existing instance gives h=148144 J/kg, a new one ~0; silent no-op for SRK/PR/INCOMP... (no else at `CoolProp.cpp:1026`; fixed master 80dd7beb); per-instance copy duplicates the code (`HelmholtzEOSMixtureBackend.cpp:4463-4522`); magic `f = 1.00001` for Water/CO2 (`FluidLibrary.cpp:91`) | Inconsistent results across instances and threads | `Model::with_reference_state(RefState) -> Arc<Model>` sharing coefficients; no globals |
| R15 | Global config read on hot paths, unsynchronized | `Configuration.cpp:126-191`; ENABLE_SUPERANCILLARIES 14 sites, DONT_CHECK_PROPERTY_LIMITS 12; `set_config_as_json_string` applies partially on type error (two-pass, `:170-191`); `set_config_string` side-effect unloads REFPROP (`:146-148`). (added by verifier) Library code writes the global as well: the Tabular table build sets `DONT_CHECK_PROPERTY_LIMITS` to true and then unconditionally to false (`src/Backends/Tabular/TabularBackends.cpp:129, 197`). That races with other threads and drops the user's setting | Changing config mid-run changes numerics for other threads; hash lookups per call | Typed immutable `FlashOptions`/`ModelOptions` passed explicitly; `Default` = CoolProp defaults |
| R16 | Hidden env-var numerics + lax parsing | six getenv statics (sec. 4d); `std::stoi/stod` accept trailing junk (`Configuration.h:194,208`): oracle `COOLPROP_R_U_CODATA=8.314xyz` -> 8.314, `COOLPROP_TABULAR_NX=250abc` -> 250; locale-dependent (master b945eb4f: de_DE reads 0.25 as 0) | Oracle results depend on the environment; typos silently accepted | Kernel never reads env; optional outer layer parses strictly |
| R17 | Divergent, lax fluid-string grammars | fractions/concentration parsed only in PropsSI (`CoolProp.cpp:136-239`), `?options` only in factory (`AbstractState.cpp:183-219`): oracle `AbstractState("INCOMP","MEG-20%")` fails, PropsSI accepts; `MEG-abc%` silently = 0 % (unchecked `strtod`, `:224-231`); `MEG-0.2`, suggested by the error text at `:220`, fails (detector requires '%', `:140-143`); zero fractions drop components (`:196-204`; intentional per the comment, listed only because the other entry points behave differently) | Same string means different things per entry point | One strict `FluidSpec` parser shared by every entry point |
| R18 | Options suffix breaks on '&' and '[' | factory splits on '&' before stripping '?' (`AbstractState.h:740-742`, `AbstractState.cpp:187-219`): oracle `("SVDSBTL&HEOS", 'Water?{"critical_patch":{"mode":"a&b"}}')` -> "pure-fluid only"; PropsSI's `extract_fractions` rejects JSON arrays: oracle `...Water?{..."bbox":[600,650,1e7,3e7]}}` -> "must end with ']'". Spec claims verbatim pass-through (spec:97-104, 428-431); `&` URL case tested only at string level (`CoolProp-Tests-FactoryOptions.cpp:87-91`) | The spec's own flagship example (bbox) is unusable via PropsSI | Typed options struct; if a string form is kept, strip `?` first, then split |
| R19 | Spec drift | Options spec: "JSON library: RapidJSON" (spec:391-393) vs nlohmann + Valijson (`src/SchemaValidation.cpp:12-22`); "No ? -> {}" (spec:299-301) vs "" (`FactoryOptions.cpp:25-27`, `:33-35`); qmass spec "calc_Qmass non-virtual" vs virtual (`AbstractState.h:271`) | Design docs are not a reliable oracle | Docs generated from code / ADRs kept with tests |
| R20 | Derivative-key grammar lax, errors late | `DataStructures.cpp:245-257` only checks a '(' after index 0: oracle `x(P)/y(T)\|Dmolar` and `zz(P)junk/q(T)more\|Dmolar` == `d(P)/d(T)\|Dmolar`; `d(Phase)/d(T)\|P` parses then fails in `get_dT_drho` (`AbstractState.cpp:1148-1149`) | Typos return numbers | `ThermoVar` enum of differentiable state functions; strict parser |
| R21 | Fat interface, capability by exception | 176 virtuals / 134 default `NotImplementedError` (`AbstractState.h:77-1680`); PCSAFT overrides 22, IF97 48 | Clients discover support by catching; violates ISP | Small capability traits (`Helmholtz`, `Transport`, `Saturation`, `Mixture`, ...) |
| R22 | Error model: globals and sentinels | errstring outbox (`CoolProp.cpp:83-85,1047-1056`); PropsSI returns +inf (`:696-698`); PhaseSI returns "unknown: <msg>" (`:1191-1203`); batch C APIs swallow per-point errors and leave those outputs unchanged (`CoolPropLib.cpp:792-806,820-830,844-858`; intentional and documented in the comments at `:800-805`, but no per-point status is reported); `-_HUGE` "unset" bulk values (`AbstractState.cpp:310-313`); Q=-1 single phase | Errors lost or misattributed under concurrency; sentinels leak | `Result<_, Error>` with a structured `#[non_exhaustive] enum`; `Option<Quality>`; per-point status in batch |
| R23 | C-API memory safety | `Props1SImulti` bounds-checks rows (`_result.size()`, always 1) but writes `_result[0].size()` columns (`CoolPropLib.cpp:228-233`; unchanged on master); `handle_manager.get` returns `shared_ptr&` after unlock (`:500-508`: free on another thread = use-after-free); `Props1` has no try and `convert_from_SI_to_kSI` throws for e.g. Tcrit (`:94-120,152-161`): exception crosses `extern "C"`; `PropsS` keeps only `Name1[0]` (`:162-164`). (Code reading; C exports are not in the wheel.) | Buffer overflow, UAF, terminate | Deferred C-ABI crate: `Arc` clones, explicit lengths, catch-all, no kSI |
| R24 | Static-init registry with hard-coded special cases | `AbstractState.cpp:55-62,108-160`; TTSE/BICUBIC/SVDSBTL inline because "the generator API ... can't carry the source-backend slot" (`:162-166, 236-268`); `"?"` recursion (`:270-285`); `StripPhase` compares `backend_name()` strings (`CoolProp.cpp:538-548`) | Adding a backend touches several files; life-before-main | Explicit `enum Backend`/registry built at first use; composition via typed spec |
| R25 | Qmass mixture input = nested flashes | `AbstractState.cpp:886-925` | 5-8x cost (spec:249). Caching `_Qmass` as the target (`:925`) is intentional per spec:251 and accurate to the 48-bit solver tolerance; downgraded by the verifier, not a defect | Mass quality as a native VLE spec variable (co-solve) in the mixture flash; warm start otherwise |
| R26 | Phase label is backend-defined | oracle: SRK/PR Water at 300 K, 1 atm -> rho 754.7/847.6 kg/m^3 labelled `iphase_gas` (cubic area; verify there) | `phase()` is not a reliable contract across backends | Phase classification rule defined once in the kernel and tested per backend |
| R27 | Doc/test rot | `_molar_mass` "[mol/kg]" (`AbstractState.h:103`); d2alpha0 docs swapped (`:371-376`); the base `set_reference_stateS/D` errors (`:821-834`) point users to the free `CoolProp::set_reference_stateD`, which always builds a HEOS backend (`CoolProp.cpp:1028-1040`), so the advice is wrong for non-HEOS backends (verifier rewording; the message does not recommend itself); fast_evaluate doc omits SVDSBTL (`:897-898`); virial units "-" (`DataStructures.cpp:61-64`); dead `CoolPropDbl` long-double switch (`detail/tools.h:45-50`); dead `l10n/english.h`; test "Test all input pairs for Water using all valid backends" runs only DmolarT (`src/Tests/CoolProp-Tests.cpp:603-624, 768-794`) | Misleading comments; weak verification | Doc tests, units in types, honest test names |

## 7. Parallelism fit

| Piece | Character | Fit / plan |
|---|---|---|
| String parsing (fluid spec, pair names, outputs, derivative keys) | Branchy, allocating | Once per request/batch, hoisted out of loops ("compiled output plan", as IF97 `fast_evaluate` validates outputs up front, `IF97Backend.h:710-729`) |
| Model load | Decode/IO once per fluid | Lazy `OnceLock`; parallel-safe; never on the hot path |
| Basis normalisation, Qmolar<->Qmass, ideal-gas parts, Z, Prandtl | Branch-free arithmetic | SIMD-trivial over points |
| Property assembly from an alpha-derivative bundle (h, s, u, cp, cv, w, g, a, kappa, beta) | Branch-free | SIMD-friendly with SoA bundles |
| Generic partial-derivative algebra | Branch-free once (Of, Wrt, Const) fixed | Monomorphise per triple; SIMD over points |
| Single-phase flash (PT, DT direct) | Iterative, data-dependent iteration counts | Candidate for masked lock-step Newton (investigate; keep scalar reference) |
| Phase determination, saturation, two-phase flashes, HS/HQ, mixtures, Qmass-mixture loop | Deeply branchy, nested solves | Scalar per point; thread-parallel across points/fluids |
| Error handling | Exceptions today | Per-point status codes (already the `fast_evaluate` contract, `DataStructures.h:197-205`) compose with SIMD masks |

Thread-level parallelism is blocked in CoolProp by the mutable AbstractState (one 65-164 KiB instance per thread, R9) and global config/library mutation (R14, R15). With an immutable `Arc<Model>` (Send + Sync) and value `State`, many concurrent requests on the same or different fluids need no locks. Recommended structure (user requirement): three side-by-side implementations behind one typed API - `scalar` (reference, oracle parity), `batch` (slices in/out, SoA, no allocation, per-point status), `simd` (opt-in feature, same numerics, tested against `batch`); rayon-style threading as an optional outer layer only.

## 8. Verification assets

- Oracle hooks (nanobind, `src/nanobind_interface.cxx`): `AbstractState.update`, `keyed_output`, `trivial_keyed_output`, alphar derivatives to 4th order and alpha0 to 3rd, `first/second_partial_deriv`, `first/second_saturation_deriv`, `first/second_two_phase_deriv(_splined)`, `Qmass`, `fast_evaluate` (IF97/Tabular/SVDSBTL), `generate_update_pair`, `get_parameter_index/information`, `is_trivial_parameter`, `get/set_config_*`, `get_config_as_json_string`, `PropsSI`, `PropsSImulti`, `Props1SI`, `PhaseSI`, `set_reference_state`, `specify_phase`. Not exposed: `get_input_pair_index`, per-instance `set_reference_stateS`, any C-API symbol.
- Oracle hygiene: run with an empty environment for `COOLPROP_*` and `PXFLASH_*` (R16); store `get_config_as_json_string()`, version and gitrevision (ae81610e) with fixtures; reset reference states (global, R14) between cases; expect a ~2 s import.
- Do **not** encode as expected values (oracle defects): R3, R4 (HmassT), R5, R6, R7 echoes, R12 metastable roots, R13 SRK Q=5, R17 `MEG-abc%`, R26 cubic phase labels. Note `Props1SI("Water","Tcrit")` = 647.0959999999873 (HEOS) vs the IAPWS-95 constant 647.096 (`T_reducing` = 647.096): "trivial" constants are computed, so compare with tolerance and arbitrate with the paper.
- CoolProp tests worth porting: FD check of `get_dT_drho` for 14 of its 19 variables: P, H, S, U, G, Cv, Cp (molar and mass) and w (`AbstractState.cpp:1335-1460`, tolerance 1e-3, tighten; T, D, Tau, Delta untested); first/second derivatives via PropsSI (`CoolProp-Tests.cpp:1744, 1829`); reference-state check values IIR/ASHRAE/NBP for n-Propane, R134a, R124 at 1e-8 (`:2263-2436`); phase flags for 5 Water PT points (`:2704-2730`); Qmass round-trips pure and R32/R125 (`:4842-4960`); parser cases for options (`CoolProp-Tests-FactoryOptions.cpp:52-178`, `CoolProp-Tests-PropsSIOptions.cpp:58-153`); PropsSI input-validation cases (`CoolProp.cpp:729-858`), Props1SI (`:902-918`), `get_global_param_string` (`:1084-1132`).
- Identity tests needing no oracle (TDD seeds): (dG/dT)_p = -S (oracle: -393.0620684404547 vs -393.0620684404549 J/kg/K for Water at 300 K, 1 atm; ~5e-16 relative), (dH/dT)_p = cp, g = h - Ts, a = u - Ts, h = u + p/rho, Z = p/(rho R T), w^2 = (dp/drho)_s, cp - cv = T beta^2/(rho kappa_T), Prandtl = cp mu/lambda, Qmass(Qmolar) inverse identity, mass/molar ratios = M exactly, reference-state invariance of cp, w, Z.
- Paper constants for arbitration: IAPWS-95 Tc = 647.096 K, pc = 22.064 MPa, rhoc = 322 kg/m^3 (oracle `rhomolar_reducing` 17873.72799560906 mol/m^3); R = 8.31446261815324 J/mol/K (exact SI); reference-state definitions above.

## 9. Port recommendation

Order: units/basis -> errors -> Output/Input/Phase enums with metadata -> State + Model traits -> property assembly -> first derivatives -> (flash areas plug in here) -> second derivatives -> FluidSpec parser + high-level compat with model cache -> options -> Qmass -> reference states -> batch API -> later C ABI.

| Unit | CoolProp source | Priority | Redesign |
|---|---|---|---|
| U1 Quantities and basis (`Temperature`, `Pressure`, `Density<Molar/Mass>`, `Fraction`, `Quality::{Molar,Mass}`) | SI comments, `DataStructures.h:292-343`, `CoolPropDbl` (`detail/tools.h:45-50`) | P0-core | Validated constructors; f64 only (generic later for AD/SIMD) |
| U2 `Input` enum (19 physical pairs, per-field basis) + one `to_molar(M)` normaliser | `DataStructures.h:292-469`, `DataStructures.cpp:504-787`, `AbstractState.cpp:317-450` | P0-core | Order-independent constructors; unsupported pair = typed error from capability |
| U3 `Output` enum + single metadata table (name, aliases, unit, basis, trivial) + exhaustive dispatch | `DataStructures.h:64-178`, `DataStructures.cpp:11-225`, `AbstractState.cpp:451-644` | P0-core | Groups: Const, State(basis), Transport, Deriv; `FromStr`/`Display`; no integer ABI |
| U4 `Phase` + `PhaseHint` | `DataStructures.h:183-194`, `DataStructures.cpp:360-430`, `CoolProp.cpp:1168-1190` | P0-core | Result phase never Unknown/NotImposed; hint per call |
| U5 `Error` enum + `Result` | `Exceptions.h:11-78` | P0-core | Structured fields (input, value, limit, iterations) |
| U6 `State` value + `Model` capability traits (replaces AbstractState lifecycle and caching) | `AbstractState.h`, `CachedElement.h`, `HelmholtzEOSMixtureBackend.cpp:1436-1681` | P0-core | `flash(&self, Input, &FlashOptions) -> Result<State>`; State holds T, rho, p, phase, `Option<Quality>`, optional saturated L/V sub-states, derivative bundle |
| U7 Property assembly from alpha0/alphar bundle (ideal, residual, mass variants, kappa, beta, Gamma, neff, Z) | `AbstractState.cpp:646-978` | P0-core | Pure functions of (bundle, R, M, T, rho) |
| U8 First-partial-derivative engine | `AbstractState.cpp:981-1151, 1239-1247` | P0-core | Table per `ThermoVar`; FD + identity tests |
| U9 Second partial derivatives (extend to G, Cp, Cv, w) | `AbstractState.cpp:1152-1238, 1248-1291` | P1-early | Same table, second order |
| U10 `FluidSpec` parser + `props_si`/`props1_si`/`phase_si` compat with `Arc<Model>` cache | `CoolProp.cpp:107-701, 866-934, 1191-1203` | P1-early | One strict grammar; cache bounded or explicit `clear`; no errstring |
| U11 Derivative-key parser | `DataStructures.cpp:227-358` | P1-early | Strict; `ThermoVar` only |
| U12 Typed options (`FlashOptions`, `ModelOptions`) with CoolProp defaults; env/JSON loader outside kernel | `configuration_keys.h:10-99`, `Configuration.*` | P1-early | Port only the 13 numerics/model keys; immutable |
| U13 Qmass (pure rewrite; mixture solve) | `AbstractState.cpp:818-926`, `qmass_conversions.h` | P1-early | Co-solve in mixture VLE when that area lands |
| U14 Reference states as model variants | `CoolProp.cpp:946-1040`, `HelmholtzEOSMixtureBackend.cpp:4463-4560`, `FluidLibrary.cpp:64-127` | P1-early | `with_reference_state()`; no global mutation |
| U15 Batch evaluate API (fast_evaluate contract) | `AbstractState.h:889-924`, `DataStructures.h:197-205` | P1-early | Implemented for HEOS too; scalar/batch/simd side by side |
| U16 Guesses + phase hint per call | `AbstractState.h:30-57, 885-887`, `CoolProp.cpp:532-574` | P1-early | Fields of `FlashOptions` |
| U17 Saturation and two-phase derivative API | `AbstractState.h:1362-1473` | P2-later | Implementations live in the HEOS area |
| U18 Metadata outputs (GWP, ODP, hazards, dipole, fraction limits) | `AbstractState.cpp:486-507` | P2-later | Fluid metadata, not state |
| U19 C ABI crate | `CoolPropLib.h/.cpp`, `state_capi.h` | P2-later | `Arc` handles, explicit lengths, string keys or pinned discriminants |
| U20 Factory-string JSON options, canonical JSON, FNV hash | `FactoryOptions.*`, `Hash.h`, spec 2026-05-16 | defer | Only if tabular/SBTL backends are ported; typed struct first |
| U21 Legacy and global machinery: kSI API, errstring/warnstring, debug_level printing, CacheArray/CachedElement, `set_T`, sticky `specify_phase`, Props1SI arg swap, `REFPROP-`/`REFPROP-MIX:` prefixes, FLOAT_PUNCTUATION/LIST_STRING_DELIMITER, USE_GUESSES_IN_PROPSSI, `l10n/english.h`, long-double switch | as cited in sec. 6 | drop | |

Shape of the replacement (sketch, not final):

```rust
pub trait Model: Send + Sync {                       // immutable, Arc-shared, lazily loaded
    fn flash(&self, input: Input, opts: &FlashOptions) -> Result<State, Error>;
    fn eval(&self, st: &State, out: Output) -> Result<f64, Error>;
    fn eval_batch(&self, kind: InputKind, x: &[f64], y: &[f64], outs: &[Output],
                  out: &mut [f64], status: &mut [Status]);   // no alloc, per-point status
}
pub enum Input { PT { p: Pressure, t: Temperature }, DT { d: Density, t: Temperature },
                 HP { h: Enthalpy, p: Pressure }, /* ... 19 physical pairs ... */
                 QT { q: Quality, t: Temperature } }        // Density/Enthalpy/Quality carry their basis
```

## 10. Open questions

1. Composition: is a mixture composition part of the `Model` (one Arc per composition, cheap if coefficients are shared) or of the `State`/`Input` (one model per component set)? Affects caching keys and batch APIs over composition.
2. Eager vs lazy derivative bundle in `State`: compute alphar/alpha0 derivatives to order 2 (or 3) at flash time, or `OnceCell` per state? Needs a benchmark against the flash cost.
3. Which CoolProp defects must the high-level compat layer reproduce for parity (R3 unreachable pairs, R7 echo shortcut, Q = -1 sentinel), if any? Proposed: none; document deviations.
4. Should mass-basis outputs live on the State (needs M) or require the Model at call time? (`state.h_mass(&model)` vs. storing M in State.)
5. Keep string-encoded backend options (`?{...}`) for the compat API at all, or only typed builders?
6. Required C ABI consumers (Excel, Fortran, LabVIEW, Modelica) and therefore whether U19 can move earlier.
7. Phase classification contract for cubic/PC-SAFT backends (R26): define once in the kernel or per model?
8. Model cache policy for the compat API under memory pressure (bounded LRU vs unbounded with explicit clear), given WASM targets.

## Verification log

Date: 2026-10-04. Adversarial verification against the v8.0.0 checkout (ae81610e) and the CoolProp==8.0.0 wheel (oracle reports version 8.0.0, gitrevision ae81610e). I checked about 125 claims: every cited `path:line`, every count, every rot item R1-R27, and 30 oracle statements, all re-run. I found no partial edits from an earlier verifier.

Re-measured and confirmed:
- `sizeof(AbstractState)` = 2000 B (clang probe), with `CacheArray<80>` 728 B, 68 `cache.next()` handles and `SimpleState` 56 B.
- 85 parameters: 14 IO, 28 trivial, 24 aliases. 43 input pairs; `generate_update_pair` covers 35 of them.
- 38 config keys (21 bool, 8 string, 5 int, 4 double). `ENABLE_SUPERANCILLARIES` has 14 read sites, `DONT_CHECK_PROPERTY_LIMITS` 12 and `R_U_CODATA` 11.
- v7.2.0 -> v8.0.0 enum renumbering: 64 of 86 parameters and 34 of 36 input pairs (R1).
- All cited post-v8 master commits exist and are absent from v8.0.0.
- Oracle confirmations: R3, R4, R5, R6, R7, R12, R13, R14, R16, R17, R18, R20 and R26; second-derivative G error; DMASS/dmass; Tcrit 647.0959999999873; Q = -1; ~2 s import; Water ~111 KiB/instance.

Corrections:
1. `FactoryOptions.cpp:63-93` -> `:21-51` (the file has 53 lines). `:66` -> `:24`. `:67-69` -> `:25-27`/`:33-35`.
2. AbstractState counts: "179 virtuals, 134 default-throw calc_* hooks, 227 public method names" -> 176 virtual functions (134 default-throw, 8 pure), of which 138 are `calc_*` (117 default-throw); ~180 public methods.
3. `get_dT_drho_second_derivatives` covers 12 variables, not 13 (sec. 2 and 4a).
4. Exceptions: 14 typed aliases, not 13.
5. `numerics.h:17-18` -> `include/CoolProp/numerics/numerics.h:17-22`.
6. PCSAFT supports 4 molar input pairs, not 16. The other 12 case labels fall through to the throw (code plus oracle).
7. INCOMP: the PUmass arm is commented out; QT works only at Q=0.
8. Cubics: 14 of the 18 arms delegate to the HEOS update; cited the lines.
9. CoolProp's FD test checks 14 of the 19 `get_dT_drho` variables, not 19.
10. Props1SI wording: three factory attempts (one usually fails), not three constructions.
11. R12: narrowed the `specify_phase(not_imposed)` claim to "after an update" (oracle: on a fresh instance the phase is detected correctly).
12. R19: added the spec line (299-301) for "No ? -> {}".
13. R22: noted that per-point error swallowing in the batch C API is intentional and documented.
14. R25: downgraded "`_Qmass` cached as target". It is intentional (qmass spec:251) and accurate to the solver tolerance.
15. R27: the base `set_reference_stateD` message does not "recommend itself". It recommends the free function, which is HEOS-only. Reworded. d2alpha0 lines 371-376.
16. R17: marked zero-fraction dropping as intentional.
17. Section 5b: added the verifier's timing re-run (64 us vs 14 us; the ratio holds, absolute values depend on the machine).

Added by verifier (evidenced):
- R12: the flash routines leave a sticky `iphase_twophase` imposition (`FlashRoutines.cpp:648` and 7 more sites). After a *successful* QSmolar update, a later PT update on the same instance throws (oracle). This is unchanged on origin/master.
- R13: the `update_with_guesses` PQ/QT arms have no quality check.
- R15: Tabular table builds toggle the global `DONT_CHECK_PROPERTY_LIMITS` (`TabularBackends.cpp:129, 197`).

Residual doubts:
- The public-method count is a regex parse, so it is approximate.
- The 31/43 HEOS pair probe depends on the state and instance chosen. With fresh instances, the 12 failures listed are the ones the code predicts.
