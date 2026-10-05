# 08 Tabular surrogates (TTSE / BICUBIC, SVDSBTL, region atlas, SBTL, SVD, splines) - CoolProp v8.0.0 map

> Scope: `src/Backends/Tabular/` (4217), `src/Backends/SVDSBTL/` + `include/CoolProp/Backends/SVDSBTL/` +
> `include/CoolProp/schemas/SVDSBTLOptions.h` (2913), `src/SBTL/` + `include/CoolProp/sbtl/` (4331), `src/SVD/` +
> `include/CoolProp/svd/` (644), `src/Region/` + `include/CoolProp/region/` (2188): ~14.3k lines of library code, plus
> ~3.9k lines of tests (`src/Tests/CoolProp-Tests-{SVDSBTL*,SBTLAdapter,SVDComponents}.cpp`), the `[Tabular]` tests
> inside `TabularBackends.cpp`, and dev tools (`dev/bench_svdsbtl_ph.cpp`, `dev/svd_sbtl_e2e.cpp`, `dev/TTSE/`).
> `include/CoolProp/spline/` does not exist at v8.0.0; splines live in `include/CoolProp/region/` and `src/SVD/`.
> `dev/agent-notes.md` exists only on origin/master and is cited as `master:dev/agent-notes.md`. Part of the
> coolprop-rs port plan; cites the v8.0.0 source unless marked otherwise.

## 1. Purpose and concepts

A fast-evaluation tier is a **surrogate** of an exact ("source", "truth") backend: built by sampling the exact model,
cached on disk, served through the same `AbstractState` interface. v8.0.0 ships three generations:

| Tier | Grid / coordinates | Stored per node or cell | Evaluation | Non-native inputs | Two-phase | Status |
|---|---|---|---|---|---|---|
| `TTSE&<src>` | Two rectangular tables: (h, log-spaced p) and (T, log-spaced p), default 200x200 | f, f_x, f_y, f_xx, f_yy, f_xy for 6 state variables; μ, λ values | 2nd-order Taylor about the **nearest node** | Solve the Taylor quadratic | 1000-point log-p saturation table (pure); phase envelope (mixtures) | Legacy ("backward compatibility", `Web/coolprop/Tabular.rst:15`) |
| `BICUBIC&<src>` | Same tables (one in-process dataset shared with TTSE) | 16 coefficients per cell from corner f, f_x, f_y, f_xy | Horner in normalized cell coords (x̂, ŷ) | `solve_cubic` in one cell coordinate | Same as TTSE | Legacy; #1301 inversion failures; "the fix is the SVDSBTL DT-indexed surface" (`master:dev/agent-notes.md:220-223`) |
| `SVDSBTL&{HEOS,REFPROP,IF97}` | **Region atlas**: curved regions in (p,h), (p,T), (p,s), (T,ρ) whose edges are the saturation curve, melting/T_min floor, EOS limits; each region maps to a unit square (ξ, η) | Per (region, property): rank-r SVD of a dense NT x NR = 200 x 800 sample grid (r = 20) plus cubic-Hermite slopes per mode | locate, Hermite basis, `Σ_k u_k(η)·v_k(ξ)` (optionally `exp`) | Each pair tabulated natively (PH, PT, PS, DT); no inversion | Lever rule on superancillary (SA) endpoints; near-critical "patch" falls back to the exact source | New in v8 (`kRevision = 19` at release; 18 revision entries documented, rev 15's entry missing, `SVDSurfaceSerializer.h:64-230`) |

Concepts the port must keep: native input pair vs inversion; validity domain + error budget per (region, property);
phase boundaries as region edges; exact fallback where the surrogate cannot serve (critical point, out of domain);
batch evaluation with per-point status (`fast_evaluate` contract `include/CoolProp/AbstractState.h:889-924`: row-major
`out[k*N_out+o]`, NaN row + nonzero status on failure; status enum `include/CoolProp/DataStructures.h:196-205`;
`SVDSBTLBackend.h:259-318` `PointEvaluation::Kind` = SinglePhase / DomeBlend / Patched / OutOfRange /
TwoPhaseDisallowed / InternalError).

## 2. Structure (key types/functions -> path:line)

**Tabular (TTSE/BICUBIC):**

| Item | Location | Role |
|---|---|---|
| `LIST_OF_MATRICES` (38), `LIST_OF_SATURATION_VECTORS` (28) X-macros | `src/Backends/Tabular/TabularBackends.h:19-91` | Declare/pack/unpack every table field |
| `PureFluidSaturationTableData` (N = 1000) | `TabularBackends.h:225-581` (`is_inside` :259, `evaluate` :407, `first_saturation_deriv` :523) | Log-p saturation table, 4-point Lagrange cubic (`CubicInterp`, `include/CoolProp/numerics/numerics.h:528`) |
| `SinglePhaseGriddedTableData`, `LogPHTable`, `LogPTTable` | `TabularBackends.h:587-895` | Grid, `set_limits`, node search (`find_native_nearest_neighbor` :699, `find_nearest_neighbor` :726), `make_good_neighbors` :641, `deserialize` checks :828-847, :875-894 |
| `CellCoeffs` | `TabularBackends.h:899-978` | 6 `std::vector<double>` + alternate-cell index per cell |
| `TabularDataSet`, `TabularDataLibrary`; process-static `library` | `TabularBackends.h:981-1026`; `TabularBackends.cpp:24` | One dataset per path, shared by all TTSE/BICUBIC instances of a fluid |
| `TabularBackend` base | `TabularBackends.h:1034-1350`; `update` `cpp:1025-1484`; `fast_evaluate` `cpp:866-1023`; `check_tables` `h:1315-1349` | Dispatch, calc_*, build/load |
| Builds | `cpp:112-223` (saturation), `cpp:225-351` (single-phase: 1 flash + 12 first + 18 second derivatives per node) | Sampling of the exact model |
| `build_coeffs` (`Ainv` 16x16) | `cpp:14-22`, `cpp:1570-1711` | Bicubic coefficients + alternate-cell remap |
| `load_table` / `write_table` (msgpack + miniz) | `cpp:35-108` | `.bin.z` files |
| `TTSEBackend` | `TTSEBackend.h:11-86`; eval `TTSEBackend.cpp:168-201`, invert :47-166, transport :15-45 | Taylor evaluation |
| `BicubicBackend` | `BicubicBackend.h:63-176`; eval `BicubicBackend.cpp:89-145`, derivative :147-196, invert :199-305 | Bicubic evaluation |
| `bisect_vector`, `bisect_segmented_vector_slice` | `include/CoolProp/numerics/numerics.h:232-299, 311-380` | Hand-rolled bisection with NaN "holes" (used only by tabular) |

**SVDSBTL stack (top to bottom):**

| Layer | Location | Role |
|---|---|---|
| `SVDSBTLBackend` (AbstractState subclass) | `include/CoolProp/Backends/SVDSBTL/SVDSBTLBackend.h:55-517`; `src/Backends/SVDSBTL/SVDSBTLBackend.cpp` | ctor :289-358, `build_critical_patch_` :395-538, `auto_calibrate_critical_bbox_` :540-771, critpatch sidecar :773-842, `ensure_surface_` :979-1025, `resolve_point_` :1087-1595, `evaluate_property_` :1653-1819, `update` :1821-1912, `fast_evaluate` :2019-2288 |
| Presets `ph_/pt_/ps_/dt_subcritical`, `PresetOptions` | `include/CoolProp/sbtl/SVDSurfaceFactory.h:50-103`; `src/SBTL/SurfacePresets.cpp:292, 729, 1024, 1356` | Region geometry, property lists, sampling closures (IF97 branches inside) |
| `SurfaceSpec`, `RegionSpec`, `PropertySpec` | `include/CoolProp/sbtl/SurfaceSpec.h:25-122` | Build recipe (`update_state`/`read_property` closures) |
| `build_surface`, `sample_grid`, `make_grid_axes` | `src/SBTL/SVDSurfaceFactory.cpp:49-323` | Sampling (optionally threaded), NaN fill, SVD per (region, property) |
| `SVDSurface` | `include/CoolProp/sbtl/SVDSurface.h:48-162`; `src/SBTL/SVDSurface.cpp` (`resolve` :123, `eval_with_region_multi` :156-182) | Immutable after `seal()` (`SVDSurface.cpp:54-94`) |
| `SVDSurfaceSerializer` (msgpack + zlib, `kRevision = 19`) | `SVDSurfaceSerializer.h:61-277`; `src/SBTL/SVDSurfaceSerializer.cpp` (cache path :566-604) | Disk cache |
| `SatBoundaryFactory`, `SaturationSurrogate` | `include/CoolProp/sbtl/SatBoundaryFactory.h:43-190`; `SaturationSurrogate.h:56-96` | Boundary curves; 96-knot spline saturation fallback when no SA |
| `build_svd` (Eigen BDCSVD), slopes | `src/SVD/SVDBuilder.cpp:19-221` | Offline factorization |
| `SVDDecomposition`, `SVDEvaluator`, `Hermite1D` | `include/CoolProp/svd/SVDDecomposition.h:54-83`, `SVDEvaluator.h:51-227`, `Hermite1D.h:25-54` | Hot kernel |
| `AxisTransform`, `BoundaryCurve` (+ `ConstantCurve`, `CubicSplineCurve`, `PiecewiseChebyshevCurve`, `Superancillary{,Temperature}BoundaryCurve`), `Region`, `RegionAtlas` | `include/CoolProp/region/*.h`, `src/Region/*.cpp` | Classification and (ξ, η) normalization |

## 3. Algorithms and formulas (with the sources CoolProp cites)

- **TTSE** (IAPWS TTSE guideline, linked at `Web/coolprop/Tabular.rst:73`):
  `z = z_ij + Δx z_x + Δy z_y + ½Δx² z_xx + ½Δy² z_yy + ΔxΔy z_xy`, Δ from the nearest node (`TTSEBackend.cpp:173-178`);
  analytic HEOS partials at the nodes (`TabularBackends.cpp:315-348`). Transport: bilinear on the cell *starting at the
  nearest node*, so x < x_i extrapolates (`TTSEBackend.cpp:28-31`; inference). Inversion: Taylor quadratic, root by a
  "within one spacing" rule (`TTSEBackend.cpp:55-90`).
- **Bicubic** (Wikipedia "Bicubic interpolation", `BicubicBackend.h:13-61`): `α = Ainvᵀ F`,
  `F = [f, f_x·Δx, f_y·Δy, f_xy·ΔxΔy]` at the corners (`TabularBackends.cpp:1637-1665`); Horner in (x̂, ŷ)
  (`BicubicBackend.cpp:114-122`). Inversion builds the cubic in one cell coordinate, calls `solve_cubic`, and keeps the
  root with **smallest |x̂|**, not a root in [0,1], with no containment check (`BicubicBackend.cpp:219-235, 275-291`).
- **Saturation table** (`TabularBackends.cpp:112-223`): N = 1000 = 999 points log-spaced from p_triple toward
  0.9999 p_c (the loop stops at i = N-2, so 0.9999 p_c itself is not sampled) plus a node at p_c; 4-point Lagrange
  cubic in ln p. Quality weighting: 1/ρ for density, harmonic for viscosity, linear for
  cp, cv, w (`TabularBackends.h:445-510`).
- **SVD surrogate** (CoolProp cites Eckart & Young 1936, `Web/coolprop/SVDComponents.ipynb:461`):
  - Sample matrix M (NT x NR; `ln f` when `EXP`) -> thin BDCSVD truncated to r; `U` (NT x r), `V_S = V·diag(σ)`
    (NR x r), row-major rank-contiguous (`SVDBuilder.cpp:151-196`, layout rationale `SVDDecomposition.h:29-53`).
  - Per-mode slopes: natural cubic spline (default), central FD, or PCHIP (Fritsch & Carlson 1980,
    `SVDBuilder.cpp:51-102`).
  - Evaluation `f = T(Σ_k ũ_k(η)·ṽ_k(ξ))`, ũ/ṽ cubic Hermite (`Hermite1D.h:7-20`), T = id or exp
    (`SVDEvaluator.h:158-184`). Per property: r·(8 loads + ~10 FMA); locate + basis shared across outputs
    (`make_context`, `SVDEvaluator.h:140-152`; `SVDSurface.cpp:156-182`).
  - Grid: η Chebyshev-cosine spaced with 0.001 pad (crowds at the dome), ξ uniform (`SVDSurfaceFactory.cpp:49-62`);
    hence `locate` is a binary search on both axes (`SVDEvaluator.h:201-220`).
- **Region normalization** (`Region.cpp:77-117`, `AxisTransform.h:73-95`): ξ = LINEAR / LOG / POWER
  `1 - cbrt((a_hi-a)/(a_hi-a_lo))` / POWER_LO (cube-root crowding toward p_c, "Ising β ≈ 0.326 ≈ 1/3",
  `AxisTransform.h:16-31`); η = `(g(b) - g(b_lo(a))) / (g(b_hi(a)) - g(b_lo(a)))`, g = id or log.
- **Default HEOS regions (PH/PT/PS):** LIQUID, VAPOR on [p_triple, 0.9 p_c] (LOG); NC_LIQUID, NC_VAPOR on
  [0.9 p_c, (1-1e-10) p_c] (POWER); NC_SUPER on [(1+1e-10) p_c, 1.1 p_c] (POWER_LO); SUPER above (LOG)
  (`SurfacePresets.cpp:317-515`). IF97 source: SUPER split at the R1/R3 isotherm, B23 curve, plus an R5 slab
  (`SurfacePresets.cpp:109-290, 420-515`). DT preset: T-primary, ρ-secondary, LOG η for VAPOR/SUPER, water density-
  anomaly split of LIQUID (`SVDSurfaceFactory.h:82-103`, `SurfacePresets.cpp:1356-1574`).
- **Boundaries:** SA-backed no-refit curves (`SuperancillaryBoundaryCurve.h:17-56`) with a 1024-point linear
  `eval_fast` table (~1e-6, `BoundaryCurve.h:43-59`); 64-knot natural splines otherwise (`SatBoundaryFactory.h:43-54`);
  melting/T_min floor walk-up.
- **Properties:** ρ (EXP), T or h, s, u, w, plus μ, λ (EXP) if a transport probe at (0.85 T_c, 0.5 p_c) succeeds
  (`SurfacePresets.cpp:35-106`). No cp, cv or partial derivatives are served.
- **Dispatch** (`resolve_point_`, ~500 lines): PQ/QT via SA -> spline surrogate -> source flash (`:1094-1149`);
  critical-patch box (+TOMS748 polish for IF97, `:1229-1325`); DT dome pre-check (`:1340-1395`); atlas resolve +
  η-band dome reclassification for #3190 (`:1400-1435`); atlas miss -> lever-rule dome (`:1437-1568`); PT within
  3.3e-5·T_sat -> `TwoPhaseDisallowed` (`:1570-1590`); else OutOfRange.
- **Critical patch** (`:395-771`): default mode "auto" calibrates per fluid; the Water-sized box
  [0.95,1.05]T_c x [0.75,1.15]p_c (`:405`) is the fallback for mode "fixed" or a failed calibration. Auto-calibration
  bisects each axis 6 times on 12-probe strips just outside the box with budgets 1% (ρ, h, s) / 5% (w), tolerating 2
  outliers per strip (`:572-575, 656-658, 738`); (p,h)/(p,s) boxes from a perimeter walk of 25 points per edge
  (`kPerimeterSamples = 24`, `:490-527`); 44-byte sidecar file.
- **Literature status:** accuracy targets reference IAPWS G13-15 (SBTL guideline, "Kunick et al., 2015",
  `Web/scripts/fluid_properties.IF97Conformance.py:7`). G13-15's SBTL is a spline method with analytic inverse
  functions on transformed coordinates; CoolProp's `src/SBTL/` implements SVD + Hermite instead (the name is
  historical; inference from the code). BICUBIC/TTSE have no paper-level validation beyond the TTSE guideline link.

## 4. Data and configuration inputs

| Key / input | Default | Used by | Note |
|---|---|---|---|
| `TABULAR_NX`, `TABULAR_NY` | 200 | `TabularBackends.h:601-607` (read at table construction) | Introduced by cf1c7785 (#2894); non-square grids crash (R2) |
| `ALTERNATIVE_TABLES_DIRECTORY` | "" | `TabularBackends.cpp:360-365` | String concatenation: needs a trailing `/` |
| `SAVE_RAW_TABLES` | false | `write_table` (`cpp:104-107`) | |
| `MAXIMUM_TABLE_DIRECTORY_SIZE_IN_GB` | 1.0 | `check_tables` (`h:1326-1338`) | Measures only the per-fluid directory (~16 MB), so effectively inert (inference) |
| `DONT_CHECK_PROPERTY_LIMITS` | false | **Flipped by the saturation build** (`cpp:128-130, 196-198`) | Global side effect (R7) |
| `ALTERNATIVE_SVDTABLES_DIRECTORY` | "" -> `~/.CoolProp/SVDTables` | `SVDSurfaceSerializer.cpp:539-548` | |
| `SVDSBTL_SAMPLING_THREADS` | 1 | `SVDSurfaceFactory.cpp:128-139` | Per-thread `factory()` clones; REFPROP forced serial |
| `ALLOW_SVDSBTL_IN_PROPSSI` | false | `SVDSBTLBackend.cpp:360-369` | `PropsSI` re-constructs per call (~80 ms load) |
| Options JSON `SVDSBTL&HEOS::Water?{...}` | `{}` | schema `include/CoolProp/schemas/SVDSBTLOptions.h:16-96` | `prebuild`, `pmin`, `grid{NT,NR,rank}`, `properties.transport`†, `critical_patch{mode,source,tolerance†,metric†,bbox}` (†never read, R18) |

**Disk caches and invalidation:**
- Tabular: `<dir>/<wrapped backend_name>(<fluid>[x.10f]&...)/{single_phase_logph,single_phase_logpT,pure_saturation,
  phase_envelope}.bin.z` (`cpp:352-380`), msgpack maps of named matrices, zlib. Checks: revision ints (0 or 1, never
  bumped: `h:101, 231, 602`), grid size, axis limits recomputed from the live model (`h:828-847`). No EOS/library hash.
- SVDSBTL: `<dir>/<Fluid>.<Source>.<PAIR>.<opthash16>.svd.bin.z` (msgpack array + zlib; atomic write) plus
  `<Fluid>.<Source>.critpatch.<hash>.bin`. Checks: magic `"SVDS"`, manual `kRevision` (`SVDSurfaceSerializer.cpp:497-505`),
  FNV-1a of canonical options. No EOS/library hash; SA boundary handles re-acquired from the *current* HEOS at load
  (`SVDSurfaceSerializer.cpp:361-401`).

## 5. State, caching, globals, thread-safety, memory

- **Tabular**
  - Process-static `TabularDataLibrary library` (`cpp:24`): `std::map<path, TabularDataSet>`, **no lock** at v8.0.0.
    Post-v8 66ecd326 adds `data_mutex`/`build_mutex`; its message reports TSAN "103 races in
    SinglePhaseGriddedTableData::build without the fix".
  - Instances hold raw `TabularDataSet* dataset` (`h:1142`); `get_set_of_tables` can `erase` an entry on a
    TABULAR_NX/NY change (`cpp:1541-1552`) -> use-after-free for other live instances (called "a latent
    use-after-free" in post-v8 10416845).
  - The shared dataset stores the `shared_ptr<AbstractState>` of the *first* instance that loaded it
    (`cpp:1494-1497`; members `h:229, 593`) and calls it later (`AS->molar_mass()` in `first_saturation_deriv`,
    `h:559-573`): immutable-looking shared data aliases one instance's mutable backend.
  - Per-instance mutable cursors (`cached_single_phase_i/j`, `z/dzdx...` raw pointers, `h:1045-1053`); evaluators
    write results into the instance cache (`BicubicBackend.cpp:124-143`). One instance per thread is the contract.
- **SVDSBTL**
  - Each instance owns its surfaces (`unique_ptr`, `SVDSBTLBackend.h:420`) and re-reads them from disk; no in-process
    sharing at v8.0.0 (post-v8 313b8108 adds a process-wide LRU, 16 entries / 512 MB, `shared_ptr<const SVDSurface>`).
  - Lazily created mutable per-instance state: `source_`, `patch_source_`, SA handle, `SaturationSurrogate`
    (`h:410-466`). Not thread-safe per instance by design (`h:162-174`); patched rows in `fast_evaluate` re-update the
    shared `patch_source_`.
  - `SVDSurface` is immutable after `seal()` (const `eval*`, `SVDSurface.cpp:123-182`): safe to share read-only.
- **Lazy loading:** tabular builds/loads everything in the ctor (PH, PT, saturation, both coefficient sets; TTSE builds
  the full bicubic coefficient sets, `TTSEBackend.h:21-28`, but reads only their valid/alternate flags in the PT bump
  code, `TabularBackends.cpp:1148-1160, 1229-1240`; its own lookups ignore `coeffs`, `TTSEBackend.h:54-62`). SVDSBTL builds PH + PT + critical-patch calibration
  eagerly and DT/PS lazily **inside the first `update()`/`fast_evaluate()`** for that pair (`cpp:330-356, 1825-1829,
  2040-2044`).

**Measured** (oracle: CoolProp 8.0.0 wheel, Water, 12-thread x86-64 Linux, Python 3.12, caches redirected to scratch;
HEOS is the reference):

| Quantity | BICUBIC / TTSE &HEOS (200x200) | SVDSBTL&HEOS (defaults) |
|---|---|---|
| Cold build | 10.0 s (both tables + saturation + coefficients) | 46.7 s for PH + PT + patch calibration with 12 sampling threads (verifier rerun: 51.6 s); **5.4 s stall on the first DmassT query** (lazy DT build) |
| Warm construction | 0.25 s | 0.20 s **per instance** |
| Disk | 16.0 MB (7.08 PH + 8.74 PT + 0.20 sat) | 14.0 (PH) + 13.6 (PT) + 16.2 (DT) MB + 44 B |
| RSS | +67 MB (fresh build) / +111 MB (load); TTSE shares (+0) | +63 MB 1st instance; **+42 MB and +25 MB for 2nd/3rd instances** (no sharing) |
| Single-phase native, random cell centres | PT ρ p99 7.1e-7, max 1.0e-4; PH T max 9.0e-5 (TTSE PT ρ p99 3.1e-5) | Vapour PT ρ p99 8e-9, max 9e-5; liquid max 4e-7; PH T max 3.5e-5; **PT supercritical max 3.3e-2** (669 K, 27.5 MPa, just outside the patch; verifier rerun at the rounded point: ρ 3.0e-2) |
| Exactly on a grid node | **PT ρ wrong phase (99.8% error)**, DmolarT p down to -1.0e8 Pa (R3b) | n/a (no exact-hit bug; 1/400 node probes at p = p_triple: NaN + phase "twophase") |
| Non-native inputs | DmolarT: 19/600 cell-centre probes >1% (p99 0.95, max 2.1); TTSE p99 28, max 116. Nitrogen 60x60 PSmolar: 41/399 >1% (verifier rerun with other random (T,p) probes: 20/400 with T error >1%) | Native tables (DT, PS) |
| Near saturation | Liquid 1.2 K subcooled at 3.4 bar: ρ error 2.3% | Exact SA edges; η-band reclassification |
| Transport (2000 random PT) | μ: med 6e-6, p99 1.6e-2, **max 4.0** (TTSE 5.6) | Tabulated (EXP) |
| Cross-pair consistency | n/a | PT→h then (h,p)→T: p99 2.8 mK, max 0.17 K (697 K, 25.5 MPa) |
| Python loop, update + 1 output | 0.71-0.74 µs (HEOS: PT 19 µs, HmolarP 149 µs) | 1.61 µs (HmassP) |
| `fast_evaluate` per probe | 0.157 µs (2 outputs, HmolarP) | 0.75 µs (4 outputs, HmassP); 0.35 µs (2 outputs, PT); **47 µs inside the critical box** (exact flash) |
| cp, cv, partial derivatives | Yes (table derivatives) | **No**: `cpmass()` raises; `first_partial_deriv` raises "calc_reducing_state is not implemented" |

Application-level: the HX demo spec measures ~20x per run (5.38 ms HEOS vs 0.26 ms SVDSBTL; ~7 s first build)
(`docs/superpowers/specs/2026-06-01-svdsbtl-hx-demo-notebook-design.md:20-28`; measured on CoolProp 7.2.1dev, not 8.0.0).

**Memory anatomy:** BICUBIC: 38 node matrices x 200² x 8 B = 12.2 MB per table, kept twice (members + msgpack
`matrices` map, `h:614, 667-690`); `CellCoeffs` ≈ 760 B per cell (6 vector headers + 4 heap arrays of 16 doubles)
x 39,601 cells ≈ 30 MB per table: ≈105 MB total, matching the 111 MB measured. SVD at defaults: one (region, property)
= 2·r·(NT+NR) + grids ≈ 328 KB; 6 regions x 7 properties ≈ 14 MB per pair, ≈55 MB per fluid with all four pairs
(4x smaller than the raw grid, 64x smaller than per-cell bicubic coefficients at the same resolution).

## 6. Rot and bugs

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| R1 | **TTSE segfaults in the process that built the tables** | `make_good_neighbors` runs only in `unpack()` (`TabularBackends.h:681-690`); after a build, `check_tables` "Load[s] the tables back ... as a consistency check" (`h:1343`) but `get_set_of_tables` returns the in-memory set (`cpp:1535-1547`); TTSE indexes the empty `nearest_neighbor_i` (`h:756-768`). Oracle: 40x40 fresh dir, near-dome HmolarP queries -> exit 139; same tables reloaded -> 160 OK (verifier reproduced: Water 40x40, 80 near-dome probes, exit 139 cold, 80/80 OK on reload). Still on master (code inference: `make_good_neighbors` still only in `unpack`, master `TabularBackends.h:691`; 66ecd326 only adds locking) | SIGSEGV on cold cache | One constructor establishes every invariant; immutable table; build -> serialize -> load -> equality test |
| R2 | **Non-square grids segfault** in slice bisection | `bisect_segmented_vector_slice` takes `N = mat[j].size()` (row length) as the row count (`numerics.h:314`), reached via `find_nearest_neighbor` (`h:726-753`). Oracle: Nitrogen 40x80 PSmolar -> exit 139; 60x60 OK (verifier reproduced with 400 random probes) | Crash / OOB read | Typed 2-D arrays with explicit shape; no ragged `Vec<Vec>` |
| R3 | **Non-native inversion unreliable** (#1301) | min-\|x̂\| root, no containment, no neighbour retry (`BicubicBackend.cpp:219-235, 275-291`); TTSE spacing heuristics (`TTSEBackend.cpp:55-90`). Test `CoolProp-Tests-SVDSBTL.cpp:1919` asserts BICUBIC fails the #1301 sweep. Oracle §5: silent >1% errors, negative pressures | Silent garbage | Native table per supported pair; any residual inversion bracketed in-cell with failure status |
| R3b | **`bisect_vector` exact-hit bug** (new) | When a probe equals a midpoint node (`rM == 0`) the `else` branch sets `L = M`, `rL = 0`; from then on `rL*rM < 0` can never pass, so L walks right to R-1 (`numerics.h:285-296`; same pattern in the slice variant :366-375); every tabular lookup uses it (`h:303-307, 700-773`). Oracle (verifier re-run, BICUBIC&HEOS Water 200x200, node T_10 = 410.0866331658292 K): 3.4 bar returns vapour ρ 103 mol/m³ (99.8% error) vs 2.3% at T ± 1 ulp (that residual is R4); 5 bar is unaffected (4e-15); 7 bar 2.7% vs 4e-15 at ± 1 ulp. Mapper also reports p exactly on a node: 0.6% error (not re-run). Unchanged on master (`git diff v8.0.0 origin/master -- include/CoolProp/numerics/numerics.h` is empty) | Wrong phase/value for inputs that coincide with grid nodes (e.g. sweeps on the grid, round numbers) | `partition_point`-style locate with tested exact-hit, endpoint and NaN semantics; property tests |
| R4 | PT near saturation: cell straddles, sentinel arithmetic | Bump only handles i-direction straddles (`cpp:1205-1243`); `Ts = evaluate(iT, _p, _Q)` with `_Q = -1000` (`cpp:1050, 1206`) computes 1001·TL − 1000·TV (`h:425-428`), exact only if TL == TV. Oracle: 2.3% ρ error 1.2 K subcooled | Percent errors in single phase | Dome as a region edge; `Option`/enum quality, no sentinels |
| R5 | Imposed-phase PT bump indices inconsistent | Liquid tests `rhomolar[i+1][j]`; gas tests `rhomolar[i][j+1]` while doing `i++` without upper bound, comment "Bump to lower temperature" (`cpp:1163-1199`) | Wrong cell / OOB | As R4 |
| R6 | Unsynchronized static library, raw pointers, `erase` | `cpp:24`, `h:1142`, `cpp:1541-1552`; fixed post-v8 (66ecd326, 10416845) | Data races, use-after-free | `Arc<Table>` registry (`OnceLock`/`RwLock`) keyed by content hash; entries never mutated |
| R7 | Process-global config flipped during build | `set_config_bool(DONT_CHECK_PROPERTY_LIMITS, true)` at i == 0, reset only if point 0 fully succeeds (`cpp:128-130, 148-153, 179-184, 196-198`); still on master | Leaked global flag; races | Explicit options passed to the exact model; no global flags |
| R8 | **Surrogate invents physics in the dome** | Harmonic μ blend, linear λ/w/cp/cv Q-blend (`h:456-510`), routed for two-phase states (`cpp:539-619`); post-v8 da48a3ee makes TTSE/BICUBIC throw for two-phase μ and λ only; the w/cp/cv blends remain on master (master `TabularBackends.cpp:570, 583, 660`) | Surrogate domain ≠ exact model domain | Surrogate returns what the exact model would, or `Undefined` |
| R9 | `fast_evaluate` ignores the dome (new) | Only `native_inputs_are_in_range` + alternate cell (`cpp:951-966`); invalid cells map to a neighbour (`BicubicBackend.cpp:13-26`). Oracle, 1 bar (verifier reproduced): Q = 0.01 -> status **ok**, T 378.1 K (true 372.76), ρ 52995 (true 3089 mol/m³); Q = 0.99 -> status **ok**, T 361.96 K; Q in [0.02, 0.98] -> status `out_of_range` | Silent wrong batch results near saturation | Classification pass shared by single and batch paths; distinct `TwoPhase` status |
| R10 | Mixture and composition paths | Saturated liquid/vapour keyed outputs both return the Q-blend (`cpp:392-413`, `h:1278-1283`); HmolarP mixture two-phase uses `hL = hV = 0` from the pure path so Q = ±inf -> always throws (`cpp:1071, 1078-1090`; inference); TTSE lacks the `set_mole_fractions` override (`BicubicBackend.h:79-90`) | Wrong or failing mixture values | Defer mixture surrogates; composition in the artifact key |
| R11 | Stale `_phase` on T-input two-phase | (T,ρ)/(T,s) two-phase branch never sets `_phase` (`cpp:1383-1408`); fixed post-v8 da48a3ee | Wrong `phase()` | Phase is part of the returned state value |
| R12 | Memory waste and layout | Duplicated `matrices` map (`h:614, 667-690`); `vector<vector<double>>`; 6 heap vectors per `CellCoeffs` (`h:908`); TTSE builds full coefficient sets (`TTSEBackend.h:25-26`) but uses only their validity flags (`cpp:1148, 1229`). Measured +111 MB/fluid | RAM, cache misses | Contiguous SoA arrays, one owner |
| R13 | Tabular cache trust | Revisions are hard-coded 0/1/0 and not tied to content (`h:101, 231, 602`); limit check divides by signed `xmin` (`h:838-840, 885-887`), never fires for negative h_min; `compress()` result ignored, output buffer sized to input (`cpp:97-101`); non-atomic writes; no EOS hash (inference: stale after EOS edits) | Stale/corrupt tables used silently | Content-addressed key (model data hash + builder version + options + format); checked atomic I/O |
| R14 | SVD cache trust | Manual `kRevision = 19` (history entries for revs 1-14, 16-19; rev 15's entry missing) and the format comment still says revision 1 (`SVDSurfaceSerializer.h:18-26, 64-230`); stale "integer value of the enum" filename note (`h:56-59`; the code uses the pair's short name, `SVDSurfaceSerializer.cpp:597-604`); raw-int `parameters`/`input_pairs` stored in the stream (`SVDSurfaceSerializer.cpp:337, 384, 393`), and post-v8 9b35f538 had to bump to rev 20 because an enum insertion would make old caches "silently decode to the WRONG" property (master `SVDSurfaceSerializer.h:235`); critpatch hash is a self-described "intentionally a tiny duplicate" of the serializer's FNV-1a and lacks zero padding (`SVDSBTLBackend.cpp:240-255, 283` vs `to_hex16` at :997; verifier observed `Water.HEOS.critpatch.8f44b07b5901a25.bin` next to `Water.HEOS.PT_INPUTS.08f44b07b5901a25.svd.bin.z`) | Stale or misdecoded surfaces; human-managed invalidation | As R13; stable string ids for enums; one hash/format function |
| R15 | Per-instance reload, no sharing | `unique_ptr` surfaces (`SVDSBTLBackend.h:420`), ~80 ms load per construction (`h:95-110`); measured +42 MB per extra instance; fixed post-v8 by 313b8108 | RAM x instances | `Arc` artifacts from day one |
| R16 | The "pure" kernel mutates and builds in the request path | `resolve_point_` documented "without mutating any backend state" (`cpp:1077-1078`) yet lazily builds SA/surrogate (`cpp:859-941`), flashes `source_` (unconditionally for DT dome p_sat `cpp:1362-1365`; as a no-SA fallback at `cpp:1357-1360, 1466-1475, 1533-1542`) and `patch_source_` (`cpp:1267-1325`); DT/PS surfaces built inside `update()` (`cpp:1825-1829`) unless the opt-in `prebuild` option is set (`cpp:313-327, 349-355`); tabular `check_tables()` in `update()`/`fast_evaluate()` (`cpp:916, 1041`). Measured 5.4 s first-DT stall | Not thread-safe even for reads; latency cliffs | Classification pure; builds explicit or async; exact fallback a separate pass |
| R17 | Failed samples silently filled before SVD | Row-wise linear fill, then median fill (`SVDSurfaceFactory.cpp:192-258`); history of R3 cells "silently masked ... T residuals ~470 K" (`SurfacePresets.cpp:537-546`) | Invisible contamination of modes | Failure masks; shrink domain or fail the build; record coverage |
| R18 | Options schema accepts dead keys; spec drift | `properties.transport`, `critical_patch.tolerance`, `critical_patch.metric` never read (grep of `src/`; `SVDSBTLOptions.h:49-60, 76-85`); `bbox` documented "only when fixed" but honoured in auto (`SVDSBTLBackend.cpp:434-437`, where a comment calls it an intentional "escape hatch": the schema text, not the code, is stale); design spec says calibration uses tolerance/metric and stores the box in the cache header (`docs/superpowers/specs/2026-05-16-backend-options-string-design.md:270-290`) vs hard-coded budgets + sidecar (`cpp:572-575, 773-842`). Dead keys still change the opthash | Duplicate caches; false control | Typed option structs; unused fields unrepresentable |
| R19 | Partial interface, misleading errors | No cp, cv, Gibbs, partial derivatives (oracle: `cpmass()` "calc_cpmolar is not implemented"; `first_partial_deriv` "calc_reducing_state is not implemented") although `hermite_eval_deriv` (`Hermite1D.h:45-54`) and `dxi_da` (`AxisTransform.h:107-120`) exist; HmassSmass/PU pairs unsupported | Unusable in Newton solvers/simulators | Capability query on the property trait; analytic chain-rule derivatives or typed `Unsupported` |
| R20 | Classification and normalization use different boundary evaluations (downgraded: documented speed trade-off, `BoundaryCurve.h:43-56`, `Region.cpp:66-71`) | `curve_contains` uses `eval_fast` (~1e-6), `to_normalized` uses `eval` (`Region.cpp:65-98`). The #3190 η-band reclassification (`SVDSBTLBackend.cpp:1031-1075, 1400-1431`, `kDomeQTol` :71) is attributed in code to the boundary being "an interpolated sat curve whose fit error makes it overshoot the true bubble/dew line" (`cpp:1402-1404`); linking it specifically to `eval_fast` is an inference | Edge misclassification; extra branches | One exact boundary representation for both |
| R21 | Accuracy claims vs evidence (narrowed by verifier: the docs page itself is candid) | `SVDSBTL.rst:13` scopes its headline to T(p,h) for water and states "single-digit percent accuracy ceiling" for HEOS presets, and the accuracy envelope discloses that v, w, s, μ, λ exceed G13-15 (`SVDSBTL.rst:527-543`) and HEOS "~10^-3 ... ~1%" (`:544-548`). The overstatement is in the v8.0.0 tag message ("IAPWS-G13-15 conformant for water", unqualified; `git show v8.0.0`) and in the test gate: HEOS-comparison tests assert 5e-3 or 1e-3 (`CoolProp-Tests-SVDSBTL.cpp:114-117`; 24x `epsilon(5e-3)`, 27x `epsilon(1e-3)` across `CoolProp-Tests-SVDSBTL*.cpp`) and the G13-15 fail-map cases are `[!benchmark]` with zero `REQUIRE`/`CHECK` (`CoolProp-Tests-SVDSBTLFailMap.cpp:159-171`; added by verifier). Measured PT supercritical 3.3%. The mid-cell probe "never reached master" (`master:dev/agent-notes.md:215-219`) | Overstated accuracy | Budgets stored in the artifact; adversarial probes in CI |
| R22 | Critical-patch heuristics and latency cliff | Calibration is per fluid, but any calibrator exception is swallowed and silently falls back to the Water-sized box (`cpp:402-405, 443-458`); calibration probes sample only the Water-default outer box (`kT_outer_lo..kp_outer_hi` = 0.95-1.05 T_c x 0.75-1.15 p_c, `cpp:664-675`), and the bisection only shrinks from the Water box ("No widening here", `cpp:740-768`), so a fluid needing a larger patch keeps the Water box; DT not patched (`cpp:529-537`); exact flash per in-box probe: 47 µs vs 0.35 µs (measured) | Fluid-specific surprises; 135x per-probe latency jumps | Build-time validity mask from measured error; batched exact-fallback pass |
| R23 | Exact flash on the DT dome path despite SA | `src->update(QT_INPUTS, 0, T)` for p_sat on every DT dome hit (`cpp:1362-1365`) | µs per two-phase DT probe | p_sat(T) from SA |
| R24 | Four fits of the same saturation curve | `PureFluidSaturationTableData` (`h:225-581`), `SaturationSurrogate` (96 knots), SA + 1024-point `eval_fast`, 64-knot boundary splines (`SatBoundaryFactory.h:45`). The surrogate and splines are used only when no SA is available (REFPROP/IF97 sources, or the 6 of 136 `dev/fluids/*.json` without a `SUPERANCILLARY` block: Air, R404A, R407C, R410A, R507A, SES36; verifier count) | Divergent dome edges | One `SaturationCurve` (SA-backed) used everywhere |
| R25 | Source-specific (IF97) logic in generic presets and query path; duplication | B23/R1R3/R5 curves and polish (`SurfacePresets.cpp:109-290, 420-697`), query-time polish twins `polish_patch_state_`/`_s_` (`SVDSBTLBackend.cpp:90-174`), IF97 ε-band applied to all sources (`:1581`), `backend_name() == "IF97Backend"` string tests (`SurfacePresets.cpp:317, 746, 1040`), NC constants copied x3 (`:318-326, 747-755, 1041-1047`) | SoC/DRY violations | Exact model exposes internal seams via a trait; generic atlas builder |
| R26 | `fast_evaluate` contract violated by every override | Contract: "no heap allocations ... does not touch any cached state" (`AbstractState.h:893-894`), overrider list omits SVDSBTL (`:898`). Tabular writes `_hmolar/_p/_T`, may build tables, then `clear()`s the caller's state (`cpp:916, 972-978, 1019-1022`); SVDSBTL allocates per call (a documented choice, `cpp:2109-2112`) and ignores `imposed_phase` (`SVDSBTLBackend.cpp:2021, 2113-2117, 2227`); IF97 also `clear()`s at the end (`IF97Backend.h:1007-1009`) | Callers cannot rely on thread-safety or real-time guarantees | `&self` batch API on immutable data, caller-owned slices, allocation-free checked by test |
| R27 | Out-of-range labelled two-phase; NaN quality | OOB HmassP sets `_phase = iphase_twophase` "legacy back-compat" (`cpp:1891-1905`; oracle: p = p_triple vapour probe -> NaN + "twophase"); `Q < 0.0 or Q > 1.0` check lets NaN through (`cpp:1105`; fixed post-v8 15ede127) | Misleading state | Status enum separate from phase; validated `Quality` newtype |
| R28 | Wrapped `AbstractState` left with an imposed phase | `specify_phase(iphase_twophase)` with no unspecify (`TabularBackends.cpp:767-777`, "TODO: We cheat here") | Later uses see an imposed phase (inference) | No shared mutable inner state |
| R29 | Bicubic internals | Validity flags and alternates recomputed per parameter, last parameter wins (`cpp:1585-1709`); derivative eval lacks the validity guard (`BicubicBackend.cpp:147-196` vs :101-103 for #1950) | Latent empty-vector dereference (inference) | Drop BICUBIC |
| R30 | Dead, duplicate, stale code and docs | `PiecewiseChebyshevCurve` (508 LOC) used only by serializer/tests/dev; `dev/svd_bench.cpp` referenced (`SVDDecomposition.h:25, 32`) but absent; `svd_sbtl_e2e.cpp:9` says Chebyshev, code builds splines (:160); `SVDSBTLBackend.h:29-54` says "two-phase NaN, no critical fallback"; `SVDSurfaceFactory.h:37-42` "2-region atlas"; stale TODO `SatBoundaryFactory.h:31-41`; `SaturationSurrogate.cpp:3` "log-uniform" vs Chebyshev (:45-66); `BoundaryCurve.h:17-19` "never a finite-difference shim" vs FD `eval_da` (`SuperancillaryBoundaryCurve.h:45-56, 109-120`); unreachable `set_T` (`TabularBackends.h:1291-1292`); duplicated `path_to_tables` (`h:1009-1023`, `cpp:352-366`); TTSE `xvec[j]` (`TTSEBackend.cpp:77`); "x" error text in `invert_single_phase_y` (`BicubicBackend.cpp:290, 303`); test computes `actual_BICUBIC` from `ASTTSE` (`TabularBackends.cpp:1751-1752`) | Maintainer confusion | Do not port |

## 7. Parallelism fit (data-parallel / SIMD-friendly vs branchy / sequential)

| Kernel | Where | Fit | Notes for Rust |
|---|---|---|---|
| Exact-model sampling (build) | `TabularBackends.cpp:225-351`; `SVDSurfaceFactory.cpp:73-190` | **Embarrassingly parallel**; CoolProp needs per-thread `factory()` clones, REFPROP forced serial | Dominant build cost (46.7 s with 12 threads). `rayon` over nodes with per-thread scratch from an immutable `Send + Sync` model |
| SVD factorization | `SVDBuilder.cpp:118-221` | Parallel across (region, property); BLAS-3 inside | Build-only; feature-gated dependency |
| Atlas classification | `RegionAtlas.cpp:50-67`, `Region.cpp:65-75` | **Branchy**: first-match AABB loop, virtual curve evaluation | Pass 1: classify into SoA `(region, ξ, η, status)`; enum-dispatched boundaries |
| Normalization | `Region.cpp:77-98`, `AxisTransform.h:73-95` | log/cbrt + 2 curve evaluations; vectorizable within a region bucket | Inline SA Chebyshev evaluation |
| SVD evaluation | `SVDEvaluator.h:140-193` | **Ideal SIMD over rank k**: unit-stride rows, r = 20 = 5 f64x4 (AVX2) / 3 f64x8; multi-output shares the context; a sorted batch maps to small GEMMs | O(1) locate by making grids uniform in transformed coordinates (fold the Chebyshev crowding into the η transform); interleave U, dU (and all properties) per node |
| Bicubic / TTSE evaluation | `BicubicBackend.cpp:89-145`, `TTSEBackend.cpp:168-201` | 16 / 6 FMAs; SIMD across points needs gathers; memory-bound | Not ported |
| Inversion (cubic/quadratic roots) | `BicubicBackend.cpp:199-305`, `TTSEBackend.cpp:47-166` | Branchy, wrong-root prone | Avoided by native tables |
| Dome lever rule | `TabularBackends.h:407-514`; `SVDSBTLBackend.cpp:1753-1803` | Vectorizable once endpoints are known (SA endpoints are polynomial evaluations) | Separate bucket |
| Exact fallback (critical patch, out of domain) | `SVDSBTLBackend.cpp:1267-1325` | **Sequential, iterative**: 47 µs per in-box Water PT probe (measured) | Pass 3 on the exact model, parallel across points; never inline in a SIMD loop |

- The surrogate value kernels are the most SIMD-friendly code in the whole library: branch-free, unit-stride,
  FMA-dense. Classification, dome detection and exact fallback are not. Structure batches as
  **classify -> bucket by (region, cell) -> evaluate -> fallback**.
- Side-by-side implementations: scalar reference, portable SIMD (`std::simd`/`wide`/`pulp`, wasm32 `simd128`), later
  GPU, all behind one kernel trait over the *same* artifact layout; equivalence tested to a few ULP, not bit-exact
  (FMA contraction differs; `SVDDecomposition.h:44-53` already notes this for nvcc).
- Kernel budget at defaults (inference from `SVDEvaluator.h:158-184`): ~180 FMA (20 modes x 9) + optional `exp` per property per
  point; ~1.3 KB touched per property (8 rows x 20 x 8 B). CoolProp's own fit (Apple Silicon) is ~170 ns per-probe
  setup + ~35 ns per extra output (`Web/coolprop/SVDSBTL.rst:259-264`), so locate/dispatch, not the rank loop, is the
  first optimization target; sorting by cell matters as much as vector width.
- Thread-level parallelism over many requests is trivially available once artifacts are immutable `Arc`s and the
  per-query context is a stack value; only the exact-fallback path needs per-thread scratch state.

## 8. Verification assets (tests, check values, paper tables, oracle hooks)

- **Unit/analytic:** `src/Tests/CoolProp-Tests-SVDComponents.cpp` (SVD round trip on analytic 2-D functions :470,
  spline vs FD slopes :536, EXP transform :565, `AxisTransform` Jacobians vs FD :69-151, region round trip :407, NaN
  rejection :328, :374); `CoolProp-Tests-SBTLAdapter.cpp` (presets vs HEOS :134-262, serializer bit-identical
  round trip :263, path traversal :310, corrupt input :334).
- **Backend:** `CoolProp-Tests-SVDSBTL.cpp` (vs HEOS at 5e-3; PQ/QT blends :266-319; #3189 :124, #3190 :344-427,
  #2247 :586, #1301 sweep :1919; multi-fluid :533; REFPROP/IF97 sources); `CoolProp-Tests-SVDSBTLCriticalPatch.cpp`;
  `CoolProp-Tests-SVDSBTLOptions.cpp`; `[Tabular]` tests `TabularBackends.cpp:1736-2034` (saturation derivatives 1e-6,
  cp 1e-4, isentropic T 1e-2, `fast_evaluate` buffers); #1950 regression `CoolProp-Tests.cpp:6365`.
- **Fail maps/benchmarks:** `CoolProp-Tests-SVDSBTLFailMap.cpp:159-171` (vs IF97 per IAPWS region);
  `dev/bench_svdsbtl_ph.cpp`, `dev/svd_sbtl_e2e.cpp`, `dev/profile_svdsbtl.cpp`, `dev/svdsbtl_sizing_harness.py`,
  `dev/TTSE/*.py`, `Web/coolprop/_gen/gen_DT_validation_fig1301.py`, `Web/coolprop/SVDSBTLValidation.ipynb`.
- **External arbiters:** IAPWS G13-15 budget tables (T(p,h) 25/10/10 mK in R1/R2/R5; ~1e-5 relative for v, w, s, μ,
  λ, per `SVDSBTL.rst:527-543`); IAPWS TTSE guideline; Kunick et al. (G13-15) for the SBTL method itself; above all the
  exact model: a surrogate is verified against the exact model, never against another surrogate.
- **Oracle hooks (8.0.0 wheel):** `AbstractState("BICUBIC&HEOS" | "TTSE&HEOS" | "SVDSBTL&HEOS" | "SVDSBTL&IF97", f)`,
  `.fast_evaluate(pair, v1, v2, outs, out, status)` (outs `int32` array, out a 2-D `(N, N_out)` float64 array, status `int32`); redirect caches with `ALTERNATIVE_TABLES_DIRECTORY` (trailing
  `/`) and `ALTERNATIVE_SVDTABLES_DIRECTORY`; `SVDSBTL_SAMPLING_THREADS = 0` for auto threads. The §5 table is the
  baseline the Rust surrogate must beat.
- **Method:** adversarial mid-cell vs at-node probes (`master:dev/agent-notes.md:215-219`; reproduced here: SVDSBTL
  vapour cell centres T p99 4.8e-9 vs nodes of the same order, so the interpolation is real); exact-node probes (R3b);
  boundary probes (η -> 0/1, p -> p_c, T -> T_sat ± mK, p = p_triple); cross-pair round trips; dome-edge batch probes
  (R9); cold-build-then-query in one process (R1); non-square grids (R2).

## 9. Port recommendation

| Unit | Priority | CoolProp source | Rationale / redesign |
|---|---|---|---|
| U1. Fast-tier contract in the core API: batch evaluation on `&self`, SoA or row-major caller-owned buffers, per-point status (Ok / OutOfDomain / TwoPhase / Undefined / NeedsFallback), capability query (pairs x outputs) | **P0-core** | `AbstractState::fast_evaluate` (`AbstractState.h:889-924`); `SVDSBTLBackend.h:259-318` | Exact and surrogate models share one property trait; retrofitting later breaks the API. No exceptions or NaN-only signalling in hot loops |
| U2. Model-data content hash + immutable `Arc` fluid model + SA saturation curves as first-class objects | **P0-core** | n/a (missing: R13, R14, R24) | Prerequisite for cache keys, boundary curves and dome endpoints; also needed by exact flashes |
| U3. 1-D numerics kit: Hermite basis/derivative, natural-spline/PCHIP slopes, robust O(1)/binary locate with exact-hit tests | P1-early | `Hermite1D.h`, `SVDBuilder.cpp:19-114`, `CubicSplineCurve.cpp` | Shared with superancillary, transport, boundaries. Generic over f32/f64 and SIMD lanes. Fixes R3b class of bug |
| U4. Surrogate verification harness: random, mid-cell, node, boundary and cross-pair probes; per-property budgets; oracle fixtures; G13-15 tables | P1-early | `CoolProp-Tests-SVDSBTLFailMap.cpp`, `dev/bench_svdsbtl_ph.cpp` | Write before any surrogate; reusable for flash verification |
| U5. Region geometry: `AxisTransform` (LINEAR/LOG/POWER/POWER_LO + Chebyshev-as-transform), `BoundaryCurve` enum (SA-backed, spline, constant, floor), `Region`, `RegionAtlas` | P2-later | `include/CoolProp/region/`, `src/Region/` (~2.2k) | ~700 LOC; one boundary evaluation for classify + normalize (R20); uniform grids in transformed coordinates |
| U6. Surrogate builder: parallel sampling, failure masks, SVD (feature-gated), slopes, adaptive rank by budget, recorded coverage/error | P2-later | `SVDSurfaceFactory.cpp`, `SurfacePresets.cpp`, `SVDBuilder.cpp` | ~600 LOC; exact-model seams via trait, no IF97 branches (R25); never in the request path (R16) |
| U7. Evaluation kernels: scalar reference + SIMD variant on the same layout, equivalence tests | P2-later | `SVDEvaluator.h`, `SVDSurface.cpp:123-182` | ~400 LOC; side-by-side implementations; `Arc`-shared immutable data |
| U8. `SurrogateModel`: PH and PT first, then PS and DT; dome via SA lever rule; exact-fallback composition; analytic derivatives | P2-later | `SVDSBTLBackend.cpp` (2.3k) | ~900 LOC; classify -> bucket -> evaluate -> fallback; exact-model semantics (R8); derivatives or typed `Unsupported` (R19) |
| U9. Artifact format + content-addressed registry (`OnceLock` per fluid/pair, LRU), optional disk cache (off in WASM), optional embedded prebuilt artifacts | P2-later | `SVDSurfaceSerializer.*`, `TabularBackends.cpp:35-108` | ~400 LOC; header (magic, format version, model hash, builder version, options, enum string ids) + raw LE f64 arrays (mmap/zero-copy); atomic writes; no msgpack/zlib in the hot path |
| U10. Critical-region policy: POWER-axis NC regions + build-time validity mask | P2-later | `SVDSBTLBackend.cpp:395-771`, `SurfacePresets.cpp` NC regions | Replaces calibration with Water defaults (R22) |
| U11. Alternative kernel: G13-15-style SBTL (spline + analytic inverse) or dense Hermite bicubic, behind the same trait | defer | n/a (literature: G13-15) | Only if SVD fails budgets or forward/backward consistency is required (§5 cross-pair 0.17 K) |
| U12. IF97-structured presets (B23, R1/R3, R5) | defer | `SurfacePresets.cpp:109-290, 420-697` | Only with an IF97 source, via exact-model seams |
| U13. `SaturationSurrogate` for sources without SA | defer | `src/SBTL/SaturationSurrogate.cpp` | 130 of 136 HEOS fluid files ship SA; Air, R404A, R407C, R410A, R507A and SES36 do not (R24), so a no-SA saturation path (generated SA, or exact flash) is still needed for them (corrected by verifier) |
| U14. Mixture tables (phase envelope) | defer | `TabularBackends.cpp` mixture branches | Buggy (R10); needs a composition axis |
| U15. TTSE, BICUBIC backends, saturation table, inversion | **drop** | `TTSEBackend.*`, `BicubicBackend.*`, `TabularBackends.*` | R1-R5, R9, R12, R29; superseded by atlas + native pairs |
| U16. msgpack/zlib formats, X-macros, `~/.CoolProp/Tables`, directory cap, JSON options schema, `PiecewiseChebyshevCurve` | **drop** | `TabularBackends.h:19-91`, `SVDSBTLOptions.h`, `PiecewiseChebyshevCurve.*` | Replaced by typed config and U9; dead code (R18, R30) |

**Order:** U1 + U2 with the core. U3 + U4 alongside superancillary/flash work. Then U5 -> U6 + U7 (scalar) -> U8 for
single-phase PH/PT + dome -> U9 -> U10 -> SIMD U7 variant -> PS/DT pairs -> HS/DU, GPU later.

**Redesign summary:** the surrogate is a derived, immutable, content-addressed artifact of an exact model, implementing
the same trait with an explicit capability set and stored error budget; each input pair has native coordinates;
phase boundaries are exact region edges; builds are explicit (or shipped), never triggered by a query; classification,
evaluation and exact fallback are separate passes; scalar and SIMD kernels live side by side over one data layout.

## 10. Open questions

1. Is a surrogate tier needed in v1? It depends on the Rust exact (h, p) flash cost: CoolProp HEOS Water HmolarP costs
   ~149 µs per call (Python loop) vs 0.75 µs per probe for SVDSBTL `fast_evaluate` (4 outputs). Strong case for
   (p, h)-driven simulators; weak for (T, ρ), which is explicit in the EOS.
2. Accuracy target: G13-15 budgets for water, or "≤ 1e-6 relative to the exact model" for all fluids? Which outputs
   must be covered (derivatives, transport, metastable/imposed phase)?
3. Kernel choice: SVD (≈14 MB per pair, ~200 FMA per property) vs dense spline/bicubic (64x the memory, fewer flops)
   vs G13-15 SBTL with analytic inverses (forward/backward consistency). Should a consistency bound (§5: 0.17 K) be a
   requirement?
4. Is a build-time SVD dependency acceptable under "minimal deps" if feature-gated, or should prebuilt artifacts ship
   as data crates (required for WASM, where a ~47 s build is unacceptable)?
5. Memory policy for many concurrent fluids: f32 mode storage, adaptive rank, per-region lazy materialization, LRU
   eviction (CoolProp's post-v8 default: 16 entries / 512 MB)?
6. Can the POWER-axis NC regions (claimed 1e-9 to 1e-7 off-grid, `AxisTransform.h:22-25`; "≲ 1e-7" for NC_SUPER in `SurfacePresets.cpp` ph_subcritical) shrink the critical patch to nothing
   and remove the exact-fallback latency cliff (47 µs vs 0.35 µs)?
7. How should exact-model "internal seams" (IF97 region boundaries, melting curve, EOS validity envelope) be exposed
   generically so atlas builders need no source-specific code?
8. Is there demand for fixed-composition mixture surrogates, given the CoolProp mixture tabular path is broken (R10)?

## Verification log

Date: 2026-10-04. Adversarial verifier pass against the v8.0.0 checkout (ae81610e) and the CoolProp 8.0.0 wheel.
No partial earlier verification edits were found (no prior log, tables intact).

Claims checked: about 180 (code citations across all ten sections, every rot item R1-R30, the LOC/count figures, and
the oracle measurements behind R1, R2, R3b, R9, R19 and the SVDSBTL build, cache-file name and PT-supercritical error).
Counts re-run: the scope LOC (4217 / 2913 / 4331 / 644 / 2188, 14.3k total), the 38 matrices and 28 saturation vectors,
`PiecewiseChebyshevCurve` 508 LOC, the test LOC (3862), NT/NR/rank 200/800/20, and the memory arithmetic. All match.

Corrections:
- §1: "19 cache-format revisions" changed to `kRevision = 19` with 18 documented entries (rev 15's entry is missing).
- §2: `seal()` range attributed to `SVDSurface.cpp:54-94`.
- §3: the saturation table has 999 log-spaced points (the 0.9999 p_c endpoint is not sampled) plus the p_c node. The
  critical patch defaults to per-fluid auto-calibration; the Water box is only the fallback. The perimeter walk has 25
  points per edge.
- §5: TTSE does read the bicubic coefficient validity flags in the PT bump code, so "never uses" was wrong. Added the
  verifier rerun numbers (cold build, PSmolar, PT supercritical). The R3b error is 99.8%, not 99.7%. The HX spec was
  measured on 7.2.1dev.
- R1, R2, R9: reproduced with the oracle. R1 is still present on master (code inference). R9 now also covers
  Q = 0.99 (status ok, T wrong).
- R3b: corrected the mechanism (a midpoint exact hit, not only `rL == 0`). Re-measured: wrong phase at 3.4 bar, 2.7%
  at 7 bar, none at 5 bar. "T ± 1 ulp is correct" was wrong at 3.4 bar, where ±1 ulp still has the 2.3% R4 error.
- R8: post-v8 da48a3ee throws only for μ and λ. The w/cp/cv two-phase blends remain on master.
- R12, R13: TTSE coefficient use corrected. The revision fields are reworded as hard-coded 0/1/0 rather than "never
  bumped".
- R14: rev count fixed. The "WRONG" quote is sourced to master `SVDSurfaceSerializer.h:235`. The duplicate FNV is
  self-described. The critpatch hash without zero padding was observed directly.
- R16: separated the unconditional DT p_sat flash from the no-SA fallbacks, and noted the `prebuild` opt-out.
- R18: the bbox drift is in the schema text. Code comments call it an intentional escape hatch.
- R20: downgraded to a documented trade-off. Its link to #3190 is labelled inference; the code blames the
  interpolated-curve fit error.
- R21: narrowed. `SVDSBTL.rst` does disclose its limits; the overstatement is in the v8.0.0 tag message and in weak
  test gates. Added (verifier): the G13-15 fail-map tests are `[!benchmark]` with zero assertions.
- R22: rewritten. The Water box applies only on swallowed calibrator failure. Calibration samples only the Water outer
  box and never widens it (`cpp:664-675, 740-768`).
- R24: the surrogate and spline fits apply only without SA. 6 of 136 fluid files lack SA.
- R26: IF97 also `clear()`s. SVDSBTL's per-call allocation is a documented choice.
- R30: fixed the `BoundaryCurve.h` line (263-265 changed to 17-19; the file is 73 lines long).
- §7: kernel budget expressed as ~180 FMA. §8: added oracle `fast_evaluate` buffer types.
- §9 U13: "All HEOS pure fluids ship SA" was false. Air, R404A, R407C, R410A, R507A and SES36 have no SA.
- §10 Q6: the NC accuracy claim is sourced to `AxisTransform.h:22-25`.

Not re-run (left as the mapper reported them): the BICUBIC/TTSE timing and RSS figures, the DmolarT -1e8 Pa node
result, the SVDSBTL per-probe timings and the 47 µs critical-box cost, and the cross-pair 0.17 K figure.
