# Prior art: Rust and modern thermodynamic property libraries

Research input for design decisions **D1–D3, D5, D7–D9 and D11–D14** (numbering as in the design brief).
Date: 2026-10-04. Versions, licences and activity figures are as of that date.
Source links are pinned to the commits that were read: feos `246e563d`, teqp `b381682f`, Clapeyron.jl `56209597`, num-dual `ce0e152b`, rfluids `30801b93`, CoolProp/IF97 `7aaced02`.
Local CoolProp citations are paths under `reference/CoolProp` at v8.0.0 (`ae81610e`).
Anything that could not be confirmed from a primary source is marked *(unverified)*.

**Bottom line**

- **No permissively licensed, pure-Rust successor to CoolProp exists.**
  - **FeOs** is the closest Rust prior art. Since 0.9 it evaluates CoolProp's pure-fluid multiparameter equations of state (EOS). But it is built for SAFT models and density functional theory (DFT):
    - it uses molecular reduced units internally;
    - it has no ancillaries, superancillaries or transport for those fluids;
    - it has heavy generics.
  - The only pure-Rust translation of CoolProp is GPL-3.0-only and, by its own account, unvalidated.
- **The modern libraries agree on one shape. Copy it.**
  - Each model is an immutable value with one residual-Helmholtz function. Everything else is derived from that function.
  - Term kinds form a small closed set, loaded from CoolProp-format JSON. Seven residual kinds and ten ideal-gas kinds cover the default (first) EOS of all 136 CoolProp v8.0.0 fluids. The JSON also holds an eighth residual kind (`ResidualHelmholtzAssociating`, in Methanol's alternate EOS), which CoolProp parses but never evaluates.
  - States are values.
  - Who does this: teqp, FeOs, Clapeyron.jl, and the GPL translation.
- **Every Rust binding to C++ CoolProp pays the same costs.** A pure-Rust kernel removes all of them:
  - process-wide locks;
  - `&mut self` getters;
  - global error strings;
  - a C++ toolchain or 13 MB prebuilt binaries;
  - no `wasm32-unknown-unknown` target.
- **For SIMD, the best evidence is CoolProp's own unmerged May 2026 investigation of exactly our hot loop.**
  - 2-wide NEON SIMD of the per-term multiply-add chain gave 1.33× per call and 1.17× end to end.
  - Removing redundant evaluations gave more (1.21×). The two combine to 1.41×.
  - An allocating Eigen version was *slower*.
  - Vectorised `exp` approximations were rejected because solving vapour–liquid equilibrium (VLE) near the triple point needs errors of 1–2 ULP (units in the last place).
- **Automatic differentiation (AD) wins on development speed, but CoolProp has not adopted it for its core.**
  - teqp, FeOs and Clapeyron all use AD. FeOs reports 1.2–1.7× the plain `f64` cost for PC-SAFT *(paper figure, unverified 2026-10-04)*.
  - CoolProp closed issue #2754 ("Investigate the inclusion of teqp") on 2026-05-26 with no comment and no linked change, and v8.0.0 has no AD in its core. No stated rationale exists, so "declined" is an inference.
  - Recommendation: an analytic derivative bundle for the multiparameter (HEOS) hot path, with AD used as a test oracle and as the default for later model families.

---

## 1. Summary of recommendations

| # | Decision | Recommendation | Confidence | Main evidence (§) |
|---|---|---|---|---|
| P1 | D3 term representation | A closed `enum` of term **kinds** that mirrors CoolProp's JSON `"type"` tags (7 residual and 10 ideal-gas kinds in the default EOS; the serde mirror must also accept or explicitly skip the 8th residual kind, `ResidualHelmholtzAssociating`, used only by a non-default EOS). Build per-kind structure-of-arrays blocks once, in the constructor, and keep them immutable. There is no separate `finish()` step. Dispatch with one `match` per block, not one per term. | High (enum, immutable construction) / Medium (per-kind SoA blocks: benchmark against array-of-structs) | FeOs, teqp, Clapeyron, CoolProp #3044/#3046 (§2.1, §2.3, §2.4, §2.9) |
| P2 | D2 derivatives | Port CoolProp's analytic `all()`-style **derivative bundle** for the HEOS kernel, scaled as `a_xy = τ^x δ^y ∂^{x+y}α/∂τ^x∂δ^y` (finite at δ = 0, teqp-style naming). Compute up to 2nd order eagerly and 3rd–4th order on demand. Use AD (dual numbers) as the oracle that tests every term kind, and as the default path for later families (PC-SAFT, mixture composition derivatives). Keep AD types out of the public API. | Medium | FeOs Table 3, num-dual SIMD limits, CoolProp #2754, teqp rationale (§2.1–2.3) |
| P3 | D2/D17 AD dependency | If `num-dual` is used: `default-features = false` (it then needs only `num-traits`), pinned to one minor version, private, wrapped in an internal `Scalar` trait. Alternative: a small in-house dual / hyper-dual type for tests. | Medium | num-dual churn, no `no_std`, nalgebra by default (§2.2) |
| P4 | D5 state | `Fluid` is the immutable model. `State` is an immutable value with **private** fields, created only by a successful flash. Property methods take `&self`. The derivative bundle is stored by value, with no per-state lazy cache in v1. This gives the guarantee of rfluids' typestate without generics. | High (immutability, private fields) / Medium (no lazy cache) | FeOs public-fields pitfall, rfluids `&mut` getters (§2.1, §2.7) |
| P5 | D5 inputs and units | The kernel uses raw SI `f64`. The API boundary uses small in-crate SI newtypes. Common input pairs get typed constructors (or a `StateFrom<(A, B)>`-style trait) so pair validity is checked at compile time. A runtime `InputPair` enum (`FromStr`) serves the PropsSI and FFI facades. `uom`/`quantity` interop is optional, never required. | Medium | FeOs/quantity, twine/uom, rfluids, seuif97, iapws95, rust-steam (§2.1, §2.2, §2.7, §2.8) |
| P6 | D8 concurrency | `Arc<Fluid>` that is `Send + Sync` by construction, asserted at compile time. No globals, no `thread_local`, no locks on the hot path. Keep the default multi-threaded `cargo test` runner on: it finds races for free. | High | CoolProp #2787/#2800/#2831, IF97 #51, binding locks, teqp `const` models (§2.3, §2.7) |
| P7 | D7/D11 data | Keep a faithful serde mirror for importing and exporting CoolProp JSON, the de facto interchange format. The mirror must accept alternate EOS entries, including the `ResidualHelmholtzAssociating` kind. Pin the v8.0.0 snapshot and record a provenance hash. Build the name/alias index at build time. Load lazily per fluid. Allow fluid subsets for WASM. | High | teqp, FeOs, Clapeyron, luisbedoia/coolprop-rs, chemicals (§2.1, §2.3, §2.4, §2.6, §2.7) |
| P8 | D9 execution strategies | One scalar reference implementation. Batch, SIMD and threaded variants sit beside it behind one trait and are equivalence-tested against it. SIMD order of work: (1) remove redundant evaluations; (2) sort terms into uniform chunks; (3) vectorise multiply-add chains across terms with scalar libm `exp`; (4) for batch calls, put one state per lane ("shallow" vectorisation); (5) never allocate per call. No single-ISA-only paths. Threads (rayon) only as an optional outer layer. | High (structure) / Medium (expected gains) | CoolProp #3044/#3046, pyJac, seuif97, chemicals, refprop-rs, peritheos (§2.9) |
| P9 | D12 errors | `Result` everywhere. Batch calls return a status per point. No sentinels (`-9999`, NaN tuples, `HUGE_VAL`). A NaN mode only as an opt-in facade feature. The library never prints; diagnostics are returned as data or through an optional `tracing` feature. | High | seuif97, Clapeyron, sgolle wrapper, FeOs `println!`, GSW-rs (§2.1, §2.4, §2.7, §2.8) |
| P10 | D13 verification | Literature check values as first-class tests (IAPWS tables, as FeOs and iapws95 do). A documented divergence register; compatibility switches only where CoolProp 8.0.0 departs from the literature (GSW-rs `compat` pattern). Optional native-only differential fuzzing against `coolprop-sys = "=8.0.0"`. teqp as a second oracle for GERG and multifluid. | Medium | GSW-rs, FeOs tests, rfluids/coolprop-sys, teqp (§2.3, §2.7, §2.8) |
| P11 | D1 crate layout | Start with few crates (kernel, data, facades) and feature-gate model families. Do not copy FeOs's four crates plus derive macros until a second family exists. Keep generic parameters shallow to control compile time. | Medium | FeOs, teqp compile-time issues (§2.1, §2.3) |
| P12 | D11 facades | Pure Rust lets the browser build use `wasm32-unknown-unknown` + `wasm-bindgen` (as seuif97 does), which no CoolProp binding can, since they all use Emscripten. Also: self-describing catalogs of valid pairs and outputs; C ABI and Python as thin feature-gated layers; a PropsSI-style string facade for migration. | High (WASM target) / Medium (rest) | seuif97, GSW-rs, luisbedoia, FeOs pyodide, CoolProp JS (§2.7, §2.8) |
| P13 | D14 licensing | Never copy code from GPL, LGPL or MPL projects (list in §2.10). Code under the NIST notice (teqp, AGA8) may be ported if the notice is kept. Cantera needs its BSD-3 notice. Check databank data source by source. | High | §2.10 |
| P14 | D10 materials (deferred) | For correlation-only material properties later, borrow thermo's method ranking, validity ranges and explicit extrapolation policy, and Cantera's split of species, phase and transport. Build nothing now (consistent with `materials-extensibility.md`). | Medium | thermo, Cantera (§2.5, §2.6) |

---

## 2. Findings by subtopic

### 2.1 FeOs (feos-org/feos)

| Aspect | Finding | Source |
|---|---|---|
| Status, licence | Latest is 0.10.1 (2026-07-24). 0.9.0 (2025-11-08) and 0.10.0 (2026-07-15) both redesigned the core traits. MIT OR Apache-2.0, edition 2024, 20,760 downloads. | [crates.io](https://crates.io/crates/feos), [CHANGELOG](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/CHANGELOG.md), [Cargo.toml](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/Cargo.toml) |
| Crate structure | `feos-core`: traits, `State`, density iteration, phase equilibria, AD module. `feos-dft`: functionals, FFT convolutions. `feos-derive`: proc macros. `feos`: models behind features (`pcsaft`, `epcsaft`, `gc_pcsaft`, `pets`, `uvtheory`, `saftvrmie`, `saftvrqmie`, `multiparameter`, `dft`, `rayon`). `py-feos`: PyO3 bindings. The paper: features let one "deactivate all features (decreases compile time …)". | [crates/feos/Cargo.toml](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos/Cargo.toml), [paper, doi:10.1021/acs.iecr.2c04561](https://doi.org/10.1021/acs.iecr.2c04561) (CC-BY-NC-ND 4.0) |
| Core traits | `Residual<N: Dim = Dyn, D: DualNum<f64> + Copy = f64>` has one required evaluator, `reduced_helmholtz_energy_density_contributions`. Everything else is a provided method: virials, `p_dpdrho`, `dmu_drho` and so on. Associated types `Real` and `Lifted<D2>` support derivatives with respect to model parameters. `ResidualDyn` is a simpler shortcut. `IdealGas` is defined by `ln_lambda3(T)` "in units ln(A³)". `EquationOfState<I, R>` pairs an ideal-gas model with a residual model and derefs to the residual. Also `Total`, `Molarweight`, `Subset`, and optional `EntropyScaling` (transport). | [residual.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos-core/src/equation_of_state/residual.rs), [equation_of_state/mod.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos-core/src/equation_of_state/mod.rs) |
| `State` | `State<E, N, D>` has **public** fields `eos`, `temperature`, `molar_volume`, `total_moles`, `density`, `molefracs`, plus a private `Cache` of 11 `OnceLock` derivative slots. Its docs say: "`State` objects are meant to be immutable. If individual fields like `volume` are changed, the calculations are wrong". Constructors: `new_nvt`, `new_npt`, `new_nph`, `new_nps`, `new_nth`, `new_nts`, `new_nvu`. `Contributions` selects `IdealGas`, `Residual` or `Total`. | [state/mod.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos-core/src/state/mod.rs), [state/cache.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos-core/src/state/cache.rs) |
| Phase equilibrium | `PhaseEquilibrium<E, const P: usize, N, D>` carries the phase count as a const generic. `pure(eos, temperature_or_pressure, …)` dispatches on the unit type of its input (`TemperatureOrPressure`). Also `bubble_point`, `dew_point`, `tp_flash`, and `ph_flash`/`ps_flash` (added in 0.10). | [phase_equilibria/mod.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos-core/src/phase_equilibria/mod.rs), [vle_pure.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos-core/src/phase_equilibria/vle_pure.rs) |
| Derivatives | The Helmholtz function is written once, generic over num-dual types. Monomorphisation generates every derivative variant at compile time, so there is "no penalty (e.g., virtual table look-ups) at run-time". Paper Table 3 (PC-SAFT, methane/CO₂, i7-7700K): `f64` 0.883 µs; Dual 1.20×; Dual2 1.52×; HyperDual 1.61×; Dual3 1.73× *(unverified 2026-10-04, as above)*. | [paper](https://doi.org/10.1021/acs.iecr.2c04561) |
| Units | Every public signature uses `quantity`. Internally FeOs uses molecular reduced units (1 Å, 1 ps, mass fixed through k_B, 1/N_A; `REFERENCE_VALUES`). | [feos-core lib.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos-core/src/lib.rs) |
| Multiparameter EOS | Added in 0.9.0 ("Implement pure-component multiparameter equations of state from CoolProp", [#301](https://github.com/feos-org/feos/pull/301)). Pure fluids only: anything else hits `panic!("Multiparameter equations of state are only implemented for pure components!")`. 124 fluids in `parameters/multiparameter/coolprop.json`. Supports all 7 residual and 10 ideal-gas term kinds used by the default EOS of CoolProp v8.0.0 fluids (not `ResidualHelmholtzAssociating`). The ideal gas is squeezed into `ln_lambda3` ("bit of a hack to convert from phi^0 into ln Lambda^3"). Max density: "Not sure what value works well here". No ancillaries, superancillaries or transport for these fluids. | [multiparameter/mod.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos/src/multiparameter/mod.rs), [residual_function.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos/src/multiparameter/residual_function.rs), [ideal_gas_function.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos/src/multiparameter/ideal_gas_function.rs) |
| Python | PyO3 extension with wheels for Windows, Linux and macOS (`pip install feos`). Units in Python come from the `si-units` package. | [README](https://github.com/feos-org/feos), [quantity README](https://github.com/itt-ustutt/quantity) |
| Parallelism | Optional `rayon` feature. 0.10 adds parallel evaluation of properties with their gradients (`PropertyAD` "parallel variants"). Since 0.9.5 the Python package sets the rayon thread count with `FEOS_MAX_THREADS` (or `set_num_threads()`). | [CHANGELOG](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/CHANGELOG.md) |
| WASM | Pyodide wheels for `wasm32-unknown-emscripten` since 0.10.1 ([#368](https://github.com/feos-org/feos/pull/368)). No `wasm-bindgen` package was found *(unverified)*. | [wheels.yml](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/.github/workflows/wheels.yml) |
| Performance claims | Paper Table 2 (methane/CO₂ PC-SAFT with quadrupole, single core, LTO): critical point 0.76 ms, T,p flash 1.13 ms, bubble point 2.01 ms, 50-point phase diagram 49.7 ms *(unverified 2026-10-04: the paper is open access, CC-BY-NC-ND, but the publisher blocked automated retrieval)*. | [paper](https://doi.org/10.1021/acs.iecr.2c04561) |
| Dependencies | `feos-core` always pulls nalgebra 0.35, quantity (with nalgebra and num-dual), num-dual, serde_json (`preserve_order`), indexmap, csv, itertools and thiserror. | [feos-core Cargo.toml](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos-core/Cargo.toml) |
| Diagnostics | Iteration output goes to stdout through `println!` macros gated by `Verbosity`. | [feos-core lib.rs](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/crates/feos-core/src/lib.rs) |

**Borrow:**

- One required Helmholtz function per model, with every property derived from it.
- Ideal-gas and residual parts as separate values combined by a plain struct, so any residual model can be paired with any ideal-gas model.
- Explicit per-call `SolverOptions` (max_iter, tol), not global configuration.
- Typed dispatch on the kind of input (`TemperatureOrPressure`), and the phase count in the type.
- Model families and rayon behind features.
- Later, for fitting: parameter derivatives through implicit differentiation.

**Avoid:**

- Public fields on a type documented as immutable.
- Molecular reduced units inside a macroscopic multiparameter kernel. Conversion constants such as `6.02214076e-7` end up in model code.
- Forcing α⁰ into a SAFT-shaped ideal-gas trait.
- Three generic parameters (`E`, `N`, `D`) on every core type, plus nalgebra bounds (`DefaultAllocator: Allocator<N>`) in signatures.
- Breaking redesigns of core traits in successive minor releases.
- `println!` diagnostics, and `panic!` for unsupported input.
- Using FeOs as a parity oracle for CoolProp multiparameter fluids.

**Licence:** MIT OR Apache-2.0. Compatible; code can be adapted with notices.

### 2.2 num-dual (and the units crates `quantity` and `uom`)

| Aspect | Finding | Source |
|---|---|---|
| Status | 0.15.0 (2026-08-12). MIT OR Apache-2.0, MSRV 1.89, edition 2024. 314,826 downloads, of which 169,344 were recent. | [crates.io](https://crates.io/crates/num-dual), [Cargo.toml](https://github.com/itt-ustutt/num-dual/blob/ce0e152b970007d7bc9633ba0bfaa8f3b40049e5/Cargo.toml) |
| API churn | Breaking minor releases: 0.11 (2024-12), 0.12 (2025-09), 0.13 (2025-12), 0.14 (2026-06), 0.15 (2026-08). 0.15 changed the primitive type of `DualNum` "from a type parameter to an associated type". FeOs 0.10 is still on 0.14. | [CHANGELOG](https://github.com/itt-ustutt/num-dual/blob/ce0e152b970007d7bc9633ba0bfaa8f3b40049e5/CHANGELOG.md), [feos Cargo.toml](https://github.com/feos-org/feos/blob/246e563d9beedaba9a9f9838ff452a364573ba17/Cargo.toml) |
| Dependency tree | The only required dependency is `num-traits`. Since 0.15, `nalgebra` (0.35, plus simba and approx) is a **default** feature that users "can opt out" of. serde, ndarray and pyo3/numpy are optional. | [Cargo.toml](https://github.com/itt-ustutt/num-dual/blob/ce0e152b970007d7bc9633ba0bfaa8f3b40049e5/Cargo.toml), [CHANGELOG 0.15.0](https://github.com/itt-ustutt/num-dual/blob/ce0e152b970007d7bc9633ba0bfaa8f3b40049e5/CHANGELOG.md) |
| Types without nalgebra | Available: `Dual`, `Dual2`, `Dual3`, `HyperDual`, `HyperHyperDual`, `Real` and the implicit-differentiation helpers. `DualVec`, `Dual2Vec`, `HyperDualVec`, `Derivative` and `gradient`/`hessian`/`jacobian` are gated on nalgebra. So a full (τ, δ) second-order set without nalgebra takes three `HyperDual64` passes (ττ, δδ, τδ). The last point is an analysis of the type list. | [lib.rs](https://github.com/itt-ustutt/num-dual/blob/ce0e152b970007d7bc9633ba0bfaa8f3b40049e5/src/lib.rs) |
| `no_std` | Not supported. `lib.rs` uses `std` unconditionally and there is no `std` feature. | [lib.rs](https://github.com/itt-ustutt/num-dual/blob/ce0e152b970007d7bc9633ba0bfaa8f3b40049e5/src/lib.rs) |
| Semantics | `DualNum: PartialOrd + PartialOrd<Primitive>`. Comparisons use the real part "to ensure that the execution path does not depend on the dual number type". | [CHANGELOG 0.13.5](https://github.com/itt-ustutt/num-dual/blob/ce0e152b970007d7bc9633ba0bfaa8f3b40049e5/CHANGELOG.md) |
| SIMD fit | `type Primitive: DualNumFloat`, and `DualNumFloat` is implemented only for `f32` and `f64`. A dual number therefore cannot carry SIMD lanes. Vector duals add derivative directions, not states. | [lib.rs](https://github.com/itt-ustutt/num-dual/blob/ce0e152b970007d7bc9633ba0bfaa8f3b40049e5/src/lib.rs) |
| Performance | The paper reports no timings. It says dual numbers "can be more costly to evaluate compared to hand-written derivatives", but with "the right type of dual number in combination with caching prior results, this disadvantage can be compensated". Static (const-generic) sizes avoid overhead. Dedicated hyper-duals are "expected to be faster than the recursive version". Measured figures: FeOs Table 3 (§2.1). | [Rehner & Bauer 2021, doi:10.3389/fceng.2021.758090](https://doi.org/10.3389/fceng.2021.758090) |
| `quantity` 0.15.0 | Compile-time SI units: seven `const i8` exponents on `SIUnit`, with exponent arithmetic supplied by impls generated in a build script (`const_impls.rs`). Required dependencies: num-traits and document-features. MIT OR Apache-2.0, MSRV 1.89. | [crates.io](https://crates.io/crates/quantity), [repo](https://github.com/itt-ustutt/quantity) |
| `uom` 0.38.0 | The most widely used units crate: 13.6 M downloads, Apache-2.0 OR MIT. Used by twine-thermo and by the GPL CoolProp translation (§2.8). | [crates.io](https://crates.io/crates/uom) |

**Borrow:** generalised (hyper-)dual numbers as the reference for testing hand-written derivatives, and for families where hand derivatives are not worth the effort. **Avoid:** num-dual or nalgebra types in coolprop-rs's public API; tracking its minor versions; relying on it for SIMD. **Licence:** MIT OR Apache-2.0, compatible.

### 2.3 teqp (usnistgov/teqp, Ian Bell et al.)

| Aspect | Finding | Source |
|---|---|---|
| Status, licence | 0.23.2 (2026-07-22). Header-heavy C++17 with Python (pybind11) and C interfaces. NIST disclaimer licence: works of NIST employees are "not subject to copyright protection in the United States", and permission is granted abroad provided the notice is kept. | [releases](https://github.com/usnistgov/teqp/releases), [LICENSE.md](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/LICENSE.md) |
| Model contract | A model is any class with a templated `alphar(T, rhomolar, molefrac) const` and `R(molefrac)`. Every derivative comes from automatic differentiation (autodiff), complex step or multicomplex. The paper: "the implementation is a pure function with no side-effects and all arguments are immutable (are const)". | [C++ docs](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/doc/source/cpp/index.rst), [paper, doi:10.1021/acs.iecr.2c00237](https://doi.org/10.1021/acs.iecr.2c00237) |
| Derivative API | `get_Arxy` returns Λ_xy = (1/T)^x ρ^y ∂^{x+y}α^r/∂(1/T)^x∂ρ^y. Also `get_Ar01`, and `get_Ar06n` (all orders up to n in one call). From Python, "calling overhead is usually on the order of 1 microsecond". Virial coefficients via `get_B2vir`/`get_Bnvir`. Isochoric Ψ(T, ρ⃗) routines give fugacity coefficients and partial molar volumes. | [derivs.ipynb](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/doc/source/derivs/derivs.ipynb) |
| Object model | Templated methods cannot be virtual, so teqp uses type erasure (`AbstractModel`, `DerivativeAdapter`). `make_model(json)` returns `unique_ptr<AbstractModel>`. The stated reason: "re-compilation of the core of teqp is VERY slow". Release 0.23.0 added the option to disable models at compile time "for much faster builds". | [C++ docs](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/doc/source/cpp/index.rst), [README changelog](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/README.md) |
| Term sets | Closed `std::variant` lists, visited one term at a time: `EOSTermContainer<JustPowerEOSTerm, PowerEOSTerm, GaussianEOSTerm, NonAnalyticEOSTerm, Lemmon2005EOSTerm, GaoBEOSTerm, ExponentialEOSTerm, DoubleExponentialEOSTerm, GenericCubicTerm, PCSAFTGrossSadowski2001Term>`. A comment notes that hand-written `get_if` chains "doesn't seem to make a difference". | [multifluid_eosterms.hpp](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/include/teqp/models/multifluid_eosterms.hpp) |
| CoolProp JSON | `build_multifluid_model(names, datapath)` reads CoolProp-format `dev/fluids/*.json` and `dev/mixtures/*.json`. teqp ships 124 such fluid files. Water `alphar[0]` is identical to CoolProp v8.0.0 (diffed locally). Resolving aliases reads every file ("rather a lot slower"). Ideal-gas terms are rewritten in absolute T and ρ (`convert_CoolProp_idealgas`: a₁ = a₁* − ln ρ_r, a₂ = a₂*·T_r), so they combine in mixtures without per-component reducing states. | [multifluid.ipynb](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/doc/source/models/multifluid.ipynb), [IdealGas.ipynb](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/doc/source/models/IdealGas.ipynb), [fluiddata](https://github.com/usnistgov/teqp/tree/b381682ff6f521877c12a081b203a377772d09c7/teqp/fluiddata/dev/fluids) |
| Thread safety | Evaluation is `const`. JSON-schema validation inside the factory is commented "not thread-safe, needs a mutex". The 0.23.2 release skips free-threaded CPython wheels because "teqp is not yet audited for free-threading". | [teqp_impl_factory.cpp](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/interface/CPP/teqp_impl_factory.cpp), [v0.23.2 notes](https://github.com/usnistgov/teqp/releases/tag/v0.23.2) |
| Performance | Against REFPROP 10, propane, Lemmon EOS: "competitive with REFPROP, 1% slower for Λr02, and 38% faster for evaluation of αr". REFPROP takes about 0.5 µs/call. teqp's fugacity coefficients are "approximately two times faster". Through the C interface, PC-SAFT Λr01 takes 0.9 µs/call. *(These paper figures are unverified 2026-10-04: paywalled.)* **No direct teqp-vs-CoolProp timing was found** *(unverified)*. | [paper](https://doi.org/10.1021/acs.iecr.2c00237) |
| Why it departs from CoolProp | Hand-written derivatives are error-prone: "the implementation of these derivatives in CoolProp (just the mixture part) takes on the order of a thousand lines of code". Critical loci need high-order derivatives. Algorithms can be model-agnostic. The goal is to be "computationally efficient enough that it could replace existing optimized Fortran (REFPROP) and C++ (CoolProp) implementations". teqp is *not* "a feature-rich property library like NIST REFPROP". | [paper](https://doi.org/10.1021/acs.iecr.2c00237), [README](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/README.md) |
| Parallelism, SIMD | None in the kernels. A boost thread pool is used only for parameter-fitting cost functions. | [pure_param_optimization.hpp](https://github.com/usnistgov/teqp/blob/b381682ff6f521877c12a081b203a377772d09c7/include/teqp/algorithms/pure_param_optimization.hpp) |
| Relation to CoolProp | CoolProp #2754, "Investigate the inclusion of teqp in CoolProp": "Could be useful, but also costly for higher-order derivatives". Closed 2026-05-26 by its author (GitHub state reason "completed") with no comment and no linked change, and without adopting teqp. CoolProp v8.0.0 ported the cubic superancillary from teqp (`Web/coolprop/Cubics.rst:113`). CoolProp's in-tree design notes plan teqp-style `get_Ar(x,y)` accessors (`docs/superpowers/derivations/virial-axy.md`); these are not implemented on master as of 2026-10-03. Re-checked 2026-10-04 with GitHub code search on master (`022b63e4`): `get_Ar` appears only in docs, `dev/` scripts and a comment in the teqp-generated `GERGReferenceValues.h`. | [#2754](https://github.com/CoolProp/CoolProp/issues/2754), local `reference/CoolProp` |

**Borrow:**

- A single `alphar` contract.
- Λ/A_xy derivative naming, which stays finite at zero density.
- Closed sets of term variants.
- A JSON-driven factory for building models at runtime (C ABI, bindings).
- The absolute-variable ideal-gas form, for mixtures.
- Later, for mixtures: the isochoric formalism.

**Avoid:**

- A type-erasure layer. Rust enums give closed-set dispatch directly.
- Unbounded generic instantiation.
- Treating a kernel as a property library. coolprop-rs needs both the kernel and the property library around it.

**Licence:** NIST notice. Compatible if the notice is kept, but it is not an OSI-approved licence (§4).

### 2.4 Clapeyron.jl

| Aspect | Finding | Source |
|---|---|---|
| Status, licence | 0.6.29, tagged 2026-09-27; the pinned master commit is from 2026-10-01. MIT, Julia ≥ 1.10. Paper doi:10.1021/acs.iecr.2c00326. | [Project.toml](https://github.com/ClapeyronThermo/Clapeyron.jl/blob/562095977edabdd7b6f3a95b3ebab91d960086e9/Project.toml), [README](https://github.com/ClapeyronThermo/Clapeyron.jl) |
| Model zoo | Abstract types: `EoSModel` → `SAFTModel`, `CubicModel` (→ `DeltaCubicModel` → `ABCubicModel`), `EmpiricHelmholtzModel`, `IdealModel`, `ActivityModel`, `GibbsBasedModel` and others. Generic methods dispatch on the abstract type. A model outside the SAFT and cubic families implements three hooks for initial guesses: `lb_volume`, `T_scale`, `p_scale`. All properties come from `a_res(model, V, T, z)` through ForwardDiff. | [types.jl](https://github.com/ClapeyronThermo/Clapeyron.jl/blob/562095977edabdd7b6f3a95b3ebab91d960086e9/src/models/types.jl), [custom_model.md](https://github.com/ClapeyronThermo/Clapeyron.jl/blob/562095977edabdd7b6f3a95b3ebab91d960086e9/docs/src/user_guide/custom_model.md) |
| Parameters | `SingleParam{T}` (indexed by i), `PairParam{T}` (by ij) and `AssocParam{T}` (by ij and site ab), held in an `EoSParam` struct. `@newmodel Name AbstractType Param` generates the model struct. | [custom_model.md](https://github.com/ClapeyronThermo/Clapeyron.jl/blob/562095977edabdd7b6f3a95b3ebab91d960086e9/docs/src/user_guide/custom_model.md) |
| Solver methods | The method is an object: `saturation_pressure(model, T, method::SaturationMethod)`. The default comes from `init_preferred_method(saturation_pressure, model, kwargs)`. AD through solvers uses implicit differentiation (`saturation_pressure_ad`; dependency `IFTDuals`). Docs: "If the calculation fails, returns `(NaN, NaN, NaN)`". | [saturation.jl](https://github.com/ClapeyronThermo/Clapeyron.jl/blob/562095977edabdd7b6f3a95b3ebab91d960086e9/src/methods/property_solvers/singlecomponent/saturation/saturation.jl) |
| Multiparameter | `SingleFluid` reads CoolProp-format JSON. It prefers the JSON from a loaded CoolProp library (through a `ccall` with key `"JSON"`) and falls back to its own database. Terms are stored as structure-of-arrays vectors grouped by kind: `PolExpGaussTerm` holds index ranges for the polynomial, exponential and Gaussian groups and evaluates each group in its own loop. | [parser.jl](https://github.com/ClapeyronThermo/Clapeyron.jl/blob/562095977edabdd7b6f3a95b3ebab91d960086e9/src/models/EmpiricHelmholtz/SingleFluid/parser.jl), [polexpgauss.jl](https://github.com/ClapeyronThermo/Clapeyron.jl/blob/562095977edabdd7b6f3a95b3ebab91d960086e9/src/models/EmpiricHelmholtz/SingleFluid/terms/polexpgauss.jl) |
| Optional integrations | Julia package extensions (weak dependencies): CoolProp, CoolProp_jll, EoSSuperancillaries, Unitful, PythonCall and others. Unitful is also listed under the hard `[deps]`. | [Project.toml](https://github.com/ClapeyronThermo/Clapeyron.jl/blob/562095977edabdd7b6f3a95b3ebab91d960086e9/Project.toml) |

**Lessons for Rust traits.**

- Julia dispatches on the pair (model, method), and a concrete model can override a generic method. Stable Rust cannot do this: a blanket `impl` cannot be overridden for one type without specialisation, which is unstable.
- Use **trait default methods**, overridden per model. Use them for the hooks too (`lb_volume` becomes `fn max_density_guess(&self) -> f64`), and for "preferred method" selection.
- Solver choices become **explicit `enum`s or option structs**.
- Package extensions map to **optional features or adaptor crates**.
- Grouping terms by kind (structure of arrays) is the layout CoolProp's own SIMD plan needs (§2.9).

**Avoid:** signalling failure with NaN tuples; macro-generated model structs (use plain structs with derives). **Licence:** MIT, compatible.

### 2.5 Cantera

| Aspect | Finding | Source |
|---|---|---|
| Status, licence | 3.2.0 (2025-11-18). BSD-3-Clause; the licence text is 3-clause BSD with Caltech, Sandia and Cantera Developers copyrights. | [releases](https://github.com/Cantera/cantera/releases/tag/v3.2.0), [License.txt](https://github.com/Cantera/cantera/blob/main/License.txt) |
| Layers | Species thermo gives standard-state values: it "specifies how the reference enthalpy and entropy values for each species are calculated as a function of temperature". The phase model "describes how the species interact with one another". Some pairings are invalid: "one cannot pair certain non-ideal species thermodynamic models with an ideal phase model". Transport and kinetics are separate models, selected per phase. | [thermo reference](https://cantera.org/stable/reference/thermo/index.html), [YAML phases](https://cantera.org/stable/yaml/phases.html) |
| Phase models, including solids | `thermo:` keys include `ideal-gas`, `Peng-Robinson`, `Redlich-Kwong`, `pure-fluid`, `liquid-water-IAPWS95` ("for the liquid region only"), `fixed-stoichiometry`, `ideal-condensed`, `lattice`, `compound-lattice`, `plasma`, `electron-cloud`, `Debye-Huckel`, `HMW-electrolyte`, `Margules`, `Redlich-Kister` and others. **3.2 deprecated `lattice` and `compound-lattice`.** | [YAML phases](https://cantera.org/stable/yaml/phases.html), [phase thermo](https://cantera.org/stable/reference/thermo/phase-thermo.html) |
| Transport keys | `none`, `mixture-averaged(-CK)`, `multicomponent(-CK)`, `unity-Lewis-number`, `ionized-gas`, `high-pressure`, `high-pressure-Chung`, `water`. | [YAML phases](https://cantera.org/stable/yaml/phases.html) |
| Data format | YAML. Units can follow individual values ("1.45e9 cm^3/kmol"), and a `units` mapping "will set the default units for all values within the same YAML list or mapping, including any nested lists and mappings". | [YAML general](https://cantera.org/stable/yaml/general.html) |
| State | "class ThermoPhase stores internally the values of the *temperature*, the *mass density*, and the *mass fractions*". Setting the state mutates the object, so concurrent use needs one object per thread (inference). A search snippet attributes a "not thread safe" note to older ThermoPhase docs, but the current headers say nothing about threads *(unverified for 3.x)*. | [thermo reference](https://cantera.org/stable/reference/thermo/index.html) |

**Borrow:**

- Property models (transport, surface tension) as separate components attached to the phase model.
- Explicit units in data files, converted to SI once at load.
- A closed, string-keyed set of model kinds in the data.
- Deprecating abstractions that do not earn their keep, as 3.2 did with the lattice models.

**Avoid:** stateful phase objects; deep C++ inheritance. **Licence:** BSD-3-Clause, compatible if the notice is kept.

### 2.6 thermo and chemicals (Caleb Bell, Python)

| Aspect | Finding | Source |
|---|---|---|
| Status, licence | thermo 0.6.1 (2026-07-13); chemicals 1.5.2 (2026-06-07). Both MIT. | [PyPI thermo](https://pypi.org/project/thermo/), [PyPI chemicals](https://pypi.org/project/chemicals/) |
| Breadth | Constants; safety; temperature- and pressure-dependent correlations (vapour pressure, heat capacity, molar volume, thermal conductivity, surface tension, viscosity, enthalpy of vaporisation, permittivity); flash helpers (Rachford–Rice). "Data for over 20,000 chemicals … All databanks are loaded on-demand, saving loading time and RAM". There is one databank folder per property family. | [chemicals README](https://github.com/CalebBell/chemicals/blob/master/README.rst), [data folders](https://github.com/CalebBell/chemicals/tree/master/chemicals) |
| Execution variants | The same functions are exposed as `chemicals.numba` (JIT), `chemicals.vectorized` and `chemicals.units` (pint quantities). | [chemicals README](https://github.com/CalebBell/chemicals/blob/master/README.rst) |
| Method selection | `TDependentProperty` keeps per-method `T_limits` and an ordered `ranked_methods` list. `test_method_validity` and `test_property_validity` check results ("If the property is not reasonable, None is returned"). Extrapolation is configurable (`'linear'`, `'constant'`, `'Watson'`, …), and low and high ranges can differ. Changing `ranked_methods` in place affects new instances. `__call__` "caches previously calculated value, which is an overhead when calculating many different values". | [thermo.utils docs](https://thermo.readthedocs.io/thermo.utils.html) |
| Phases | "Phase objects are immutable and know nothing about bulk properties or transport properties". The README: "There are no solid models implemented in this interface at this time". | [phases docs](https://thermo.readthedocs.io/thermo.phases.html), [thermo README](https://github.com/CalebBell/thermo/blob/master/README.rst) |
| Provenance | chemicals describes itself as "a collection of cited and openly published data and equations", not as recommended values. | [chemicals README](https://github.com/CalebBell/chemicals/blob/master/README.rst) |

**Borrow:**

- On-demand loading per dataset.
- Ranked methods with validity ranges and an explicit extrapolation policy. This is the right pattern for future correlation-only material properties.
- One scalar source of truth exposed through several execution wrappers. This is the user's "side by side" idea, already in production.

**Avoid:** mutable global defaults; caching inside shared objects; importing databanks without checking each source's licence. **Licence:** MIT for the code; data provenance varies by source.

### 2.7 Rust bindings to CoolProp: ergonomics and pain points

| Crate or repo | Status | Shape |
|---|---|---|
| **rfluids** 0.6.0 + **coolprop-sys** 8.0.0 (portyanikhin) | 2026-08-02, MIT, MSRV 1.85. CoolProp v8.0.0 documents it as the "3-party" Rust wrapper ([PR #3220](https://github.com/CoolProp/CoolProp/pull/3220); `Web/coolprop/wrappers/Rust/index.rst`). | `Fluid<S: StateVariant = Defined>` typestate (`Fluid<Undefined>` → `in_state(a, b)?` → `Fluid<Defined>`); enums `Pure::Water`, `BinaryMixKind::MPG.with_fraction(0.6)?`; strum `FromStr`/`AsRef<str>` keys; a `bon` builder; an optional `serde` config; ships prebuilt CoolProp for 6 platforms ([README](https://github.com/portyanikhin/rfluids)) |
| **coolprop-rs** 0.2.0 (sgolle), the crate in `reference/CoolProp/wrappers/Rust` | 2020-10-27, MIT, edition 2018 | bindgen at build time against a system `libCoolProp`; only `PropsSI`/`HAPropsSI` ([repo](https://github.com/sgolle/coolprop-rs)) |
| ahjortland/coolprop-rs | 2026-02, MIT | vendored CMake build of CoolProp; `props_si`; `AbstractState` wrapper ([repo](https://github.com/ahjortland/coolprop-rs)) |
| mcurrie99/coolprop-rs | 2026-05, MIT | typed `InputPair`, `Parameter`, `StateUpdate` and `Phase` enums; `Result` instead of sentinels; `Drop` for handles; "Thread-safe default behavior through a process-wide CoolProp library lock"; an opt-in feature preloads the fluid library and removes that mutex ([repo](https://github.com/mcurrie99/coolprop-rs)) |
| luisbedoia/coolprop-rs | created 2026-10-01; README says MIT | safe API plus a WASM build through **Emscripten**. "Only a curated set of fluids is compiled in, chosen at build time with `COOLPROP_FLUIDS`. The npm package ships 30". It also describes its own API (inputs, valid pairs, properties, phases) and produces diagram data ([repo](https://github.com/luisbedoia/coolprop-rs)) |
| Pnex/pnex-coolprop-rs | 2026-08, MIT | REST server exposing all 71 functions of `CoolPropLib.h`. The first build fetches and compiles CoolProp, "~3–5 minutes" ([repo](https://github.com/Pnex/pnex-coolprop-rs)) |
| refprop-rs 0.3.1 (the REFPROP analogue) | 2026-02-17, MIT | "Thread-safe — global mutex with automatic fluid re-setup". `ParallelFluid` "duplicates the DLL/SO per CPU core". A `FluidApi` trait covers both the sequential and the parallel backend. A configurable `UnitSystem` ([README](https://github.com/math-dev-24/refprop-rs)) |

**Ergonomic ideas worth keeping:**

- Properties exist only after a successful state update. A pure-Rust design gets this with two types, the model `Fluid` and the value `State`, instead of a phantom type parameter.
- Substance enums with string round-trips, for the PropsSI facade.
- Value objects for inputs.
- Self-describing catalogs, useful for UIs and WASM.
- One trait over sequential and parallel back ends.
- `Result` errors everywhere.

**Pain points that a pure-Rust kernel designs out:**

- **Locks.**
  - coolprop-sys puts the whole library behind one process-wide `RwLock`. Shared access is used only for "operations known to support concurrent execution". Factories for backends other than HEOS, INCOMP, IF97, SRK, PR and PCSAFT, plus configuration, error handling, REFPROP and tabular backends, take exclusive access.
  - Reading CoolProp's single global `errstring` needs a dance: drop the shared guard, take the exclusive guard, clear the stale message, repeat the call, read the new message.
  - `AbstractState` is made deliberately `!Sync`.
  - Sources: [coolprop-sys lib.rs](https://github.com/portyanikhin/rfluids/blob/30801b93f8a27dfa7f93537151bc5d39630d3f24/coolprop-sys/src/lib.rs), [native/common.rs](https://github.com/portyanikhin/rfluids/blob/30801b93f8a27dfa7f93537151bc5d39630d3f24/rfluids/src/native/common.rs).
- **Mutability.** rfluids getters take `&mut self` and cache their outputs in per-instance `HashMap`s ([fluid/mod.rs](https://github.com/portyanikhin/rfluids/blob/30801b93f8a27dfa7f93537151bc5d39630d3f24/rfluids/src/fluid/mod.rs), [defined.rs](https://github.com/portyanikhin/rfluids/blob/30801b93f8a27dfa7f93537151bc5d39630d3f24/rfluids/src/fluid/defined.rs)).
- **Input pairs are checked only at runtime.** `in_state(p, p)` returns `Err(InvalidInputPair)` ([undefined.rs](https://github.com/portyanikhin/rfluids/blob/30801b93f8a27dfa7f93537151bc5d39630d3f24/rfluids/src/fluid/undefined.rs)).
- **Upstream thread-safety bugs that Rust's parallel test runner exposed.**
  - CoolProp [#2787](https://github.com/CoolProp/CoolProp/issues/2787): a race in fluid-library static initialisation. It was found because `cargo test` runs tests in parallel; the Tauri GUI tests saw lists like `"D4,D4,D4,D4,R125"`. Fixed by [#2800](https://github.com/CoolProp/CoolProp/issues/2800).
  - HumidAir moved to per-thread backends ([#2831](https://github.com/CoolProp/CoolProp/issues/2831)).
  - The IF97 header replaced a mutable `static` in `Region4::T_p` ([IF97 #51](https://github.com/CoolProp/IF97/pull/51)).
- **Build and distribution.**
  - Either a CMake + C++ toolchain, or prebuilt binaries: the `coolprop-sys-linux-x86-64` crate carries a 13 MB `libCoolProp.so`, and its README does not document where that binary comes from.
  - Regenerating bindings needs libclang.
  - **No `wasm32-unknown-unknown`.** CoolProp's own JavaScript module (`wrappers/Javascript/build.sh`) and luisbedoia both use Emscripten.
- **Errors in the in-tree wrapper** (code reading of `reference/CoolProp/wrappers/Rust/src/lib.rs`):
  - CoolProp's `PropsSI` catches every exception and returns `_HUGE` (`src/CoolProp.cpp:690-699`). The wrapper's `catch_unwind` therefore never fires, and failures come back as `Ok(HUGE)`.
  - `impl Display for CoolPropError` formats `CoolPropError` from inside itself (`write!(f, "CoolProp error: {}", CoolPropError)`). That recursion never ends.
  - Strings are passed as `format!(…).as_ptr() as *const i8`, which skips interior-NUL checks and assumes `c_char = i8`.

### 2.8 Pure-Rust property crates active in 2025–2026

Status figures are from crates.io API queries on 2026-10-04 (`https://crates.io/crates/<name>`).

| Crate (licence) | Version, last release | Scope | Notable | Lesson |
|---|---|---|---|---|
| **seuif97** (MIT) | 2.3.8, 2026-06-28; 18,489 downloads | IAPWS-IF97: 12 input pairs, 36 properties | C, Python and WASM (`wasm-bindgen`, npm) bindings from one crate through features (`cdecl`, `stdcall`, `python`, `wasm`). Speed from "profiling-guided loop tiling … to boost SIMD vectorization efficiency" and "shared-power scaling" ([preprint](https://doi.org/10.20944/preprints202606.0793.v1)). Own benchmark: 2.9–7.8× faster than CoolProp IF97 per property after correcting for FFI cost (case averages 4.6–7.3×; the README says "3-7x"; self-reported). Errors are reported as `INVALID_VALUE = -9999`. Units are MPa, °C and kJ/kg; outputs are selected by integer IDs. | Sharing power terms and computing all derivatives in one pass pays off; avoid sentinels and engineering units ([repo](https://github.com/thermalogic/RustSEUIF97)) |
| iapws95 (MIT on crates.io; GitHub detects no licence) | 0.2.5, 2026-05-19 | IAPWS-95 | No runtime dependencies. Takes temperature in °C. Saturation returns `Option` | Zero dependencies is achievable for one fluid ([repo](https://github.com/thermalogic/iapws95_rust)) |
| rust-steam (MIT) | 0.1.9, 2023-02-26 | Partial IF97 | Functions like `h_tp(t: f64, p: f64)`: two bare `f64`s that are easy to swap | A case for typed inputs ([repo](https://github.com/marciorvneto/rusteam)) |
| vle-thermo / vle-steam / vle-units (MIT) | 0.16.0, 2026-08-19 | Ports of legacy VB6 and Pascal code: 22+ cubic EOS, activity models, flashes, IF97 | Hand-written derivatives plus num-dual 0.11. An allocation-free `MixtureWorkspace`. Tested against the published tables of the source thesis. Says "What `0.x` means here": the API is not frozen | Combine hand-written and AD derivatives; test against published tables ([repo](https://github.com/miguelju/vle)) |
| **gsw** (BSD-3-Clause) | 0.2.3, 2025-08-02 | TEOS-10 seawater | `no_std` by default. Features: `compat` ("Reproduces the GSW-Matlab implementation"), `invalidasnan` (NaN instead of an error), `capi` | Oracle-compatibility and NaN modes as opt-in features; embedded targets are feasible ([repo](https://github.com/castelao/GSW-rs)) |
| aga8 (NIST notice) | 0.6.1, 2025-11-14 | AGA8 DETAIL and GERG-2008, ported from NIST's code | A mutable struct with public fields: set `p`, `t`, call `density()`, then `properties()` | Avoid order-dependent mutable APIs ([repo](https://github.com/royvegard/aga8)) |
| peritheos (MIT) | 0.11.0, 2026-09-28 (first release 2026-09-07) | Solid P–V–T EOS (BM3 and others), fitting, uncertainty, `.eosmat` material records | Every constructor returns `Result`. Batch extension traits use blanket impls (`impl<T: IsothermalEos + ?Sized> IsothermalEosBatch for T {}`), but a batch "Returns the first scalar model error" | Batch traits beside the scalar ones; return a result per point instead; a reference for future solids ([repo](https://github.com/CPrescher/peritheos)) |
| twine-thermo (MIT) | 0.4.2, 2026-01-23 | Fluid-property traits for a modelling framework | Capability traits (`HasPressure`, `HasEnthalpy`, …) and `StateFrom<(A, B)>`; a `State<Fluid>` holding `uom` quantities; optional rfluids backend | Input pairs checked at compile time; partial support expressed in types ([repo](https://github.com/isentropic-dev/twine)) |
| thermolib (MIT) | 0.9.8, 2026-07-01 | PC-SAFT (with PyO3) | — | — ([repo](https://github.com/shaw-yu2020/thermolib)) |
| reos (Apache-2.0 on crates.io) | 0.1.0, 2026-01-24 | CPA and cubics, Python-first | Analytic expressions for each model | — ([repo](https://github.com/Emanukka/reos)) |
| KiThe (MIT on crates.io; GitHub detects no licence) | 0.3.12, 2026-09-10 | Kinetics, combustion, thermochemistry | — | Out of scope ([repo](https://github.com/Gleb-Zaslavsky/KiThe)) |
| xsteamrs (MIT OR Apache-2.0 on crates.io; GitHub detects no licence); tpt-eng-props-water/air (MIT OR Apache-2.0) | 2026 | IF97 with an XSteam-style string API; IF97 and ASHRAE psychrometrics (`alloc`/`std` features) | — | String dispatch belongs only in a facade |
| **outram-park-fork-coolprop** (**GPL-3.0-only**) | 0.1.2, 2026-08-15 (first release 2026-07-16) | **Pure-Rust translation of CoolProp HEOS**: 137 fluids by its own count (v8.0.0 ships 136 enabled fluid files), flashes, partial transport, incompressibles | `Fluid` enum dispatch; coefficients generated into `const` Rust ("no runtime JSON", "~0.6 MB", Water "~3 KB"); `uom` API. The README states "No human V&V has been done yet". Transport covers 48 of 137 fluids, reported at "~1–3 %" against NIST/IAPWS. GPL because it ships OpenFOAM-derived code | **Do not copy or depend on it.** It does show that a complete Rust HEOS port is feasible ([crates.io](https://crates.io/crates/outram-park-fork-coolprop), [repo](https://github.com/theodoreOnzGit/outram-park-backend)) |
| tampines-steam-tables, outram-park-fork-dwsim-libs, ushma (all **GPL-3.0-only**) | 2026 | IF97; a DWSIM port (DWSIM is GPL-3.0); general thermodynamics | — | Off-limits (§2.10) |
| refprop-rs and chemapp_rs (MIT crates); refprop-sys (MIT, a GitHub repo, ahjortland/refprop-sys, not published on crates.io) | 2025–2026 | Bindings to proprietary REFPROP and ChemApp | — | Out of scope |

### 2.9 Parallelism and SIMD in the prior art (user follow-up)

| Project | Level | What was done | Result | Lesson |
|---|---|---|---|---|
| **CoolProp, May 2026, unmerged** ([PR #3044](https://github.com/CoolProp/CoolProp/pull/3044), [report](https://github.com/CoolProp/CoolProp/blob/939814c82a19e6635cec6d910330f73155de5286/docs/PERF-helmholtz-simd.md)) | Within one state, across the EOS terms of the residual-Helmholtz `all()` | (A) Apple `vvexp` batching of `exp` only; (B) the same plus a 2-wide NEON multiply-add chain (the "B-chain") and 15 SIMD accumulators | (A) about 3.5%. (B) 1.26–1.47× per call (1.33× over 24 fluids; Water with 54 terms: 814 → 597 ns); 1.167× end to end on a flash sweep, with "zero drift on T AND ρ". Per-term cost: about 30% `exp`, about 60% multiply-add chain. Separately, removing redundant `all()` calls ([#3034](https://github.com/CoolProp/CoolProp/pull/3034)) gave 1.211×, and both together 1.414× | Closed unmerged because it was Apple-only. "Don't ship NEON-only." Algorithmic savings were worth more than SIMD and multiply with it |
| CoolProp follow-up plan ([#3046](https://github.com/CoolProp/CoolProp/issues/3046), open) | Same | (1) Sort terms into uniform, branch-free chunks at construction; (2) keep scalar libm `exp` and vectorise only the multiply-add chains; (3) a thin portable SIMD layer | Estimated 1.6–1.8× per call with AVX2 and 2.0–2.2× with AVX-512. SLEEF `u10` (≤ 10 ULP) `exp` is "unacceptable": VLE near the triple point needs 1–2 ULP. AVX-512 is absent from consumer Intel since Alder Lake | Precision decides the SIMD design; AVX2 is the safe x86 target |
| CoolProp `allEigen` (2014, re-examined in 2026) | Across terms with Eigen arrays | 2014: "Disabled Eigen all function (so slow to compile)". 2026 recheck: 0.49–0.71× of scalar speed, caused by seven per-call `ArrayXd::Zero(N)` heap allocations; missing 4th order; wrong for R125 and Methanol ([RECON](https://github.com/CoolProp/CoolProp/blob/85fac56c0d55fc969c3ced147ca21cb62e80ea71/docs/RECON-alleigen.md)). The code is still present but commented out in v8.0.0 (`src/Helmholtz.cpp:38`, `include/CoolProp/fluids/Helmholtz.h:549`) | Slower | No allocation in the kernel. The term-array layout must be built in the constructor: a missing `finish()` once crashed the NEON path |
| CoolProp `powInt` (2016, local git log) | Scalar | Integer powers instead of `pow(double,int)` | "seems to save about 30% in speed" | Treat integer exponents as a separate fast case |
| CoolProp/IF97 [#54](https://github.com/CoolProp/IF97/commit/7aaced024a702f0985474bf293cdaae9c8d06521) | Within one evaluation | Rearranged polynomials into small arrays so the compiler can vectorise them | No numbers given | — |
| seuif97 | Within one evaluation | Loop tiling, shared powers | About 3–7× against CoolProp IF97 (self-reported) | Data-layout work comes before intrinsics |
| **pyJac** ([Curtis, Niemeyer & Sung 2018](https://arxiv.org/abs/1809.01029)) | Across states ("shallow") vs within one state ("deep") | Generated OpenCL kernels for chemical source terms and Jacobians | "Speedups of 3.40-4.08x … for shallow-vectorized OpenCL source-rate evaluation compared with a parallel OpenMP code" on AVX2. Deep vectorisation "requires synchronization between SIMD lanes" and suffers from lanes taking different branches. Data stored in chunks one vector wide | For batch calls, one state per lane in that chunked layout |
| Clapeyron.jl | Data layout | Structure-of-arrays term groups (polynomial, exponential, Gaussian) | No SIMD claim | Already the sorted layout that CoolProp #3046 wants |
| teqp | None | One `std::variant` visit per term | — | A layout built for AD, not SIMD |
| FeOs | Threads | `rayon` feature; parallel evaluation of properties and gradients; `FEOS_MAX_THREADS` | — | Threads as an optional outer layer |
| refprop-rs | Threads | Copies the REFPROP library once per core | — | What global mutable state forces on you |
| chemicals | Execution variants | `numba`, `vectorized` and `units` wrappers over one scalar code base | — | Side-by-side variants over one source of truth |
| num-dual | AD types | Only `f32`/`f64` can be the primitive inside a dual | — | Keep AD and SIMD separate |

The Rust side, briefly. `std::simd` is still unstable ([#86656](https://github.com/rust-lang/rust/issues/86656) open, last updated 2026-08-12). Stable crates exist: `wide` 1.7.1 (Zlib OR Apache-2.0 OR MIT), `pulp` 0.22.3 (MIT) and `multiversion` 0.9.0 (MIT OR Apache-2.0) ([crates.io](https://crates.io/crates/wide)). Which one to use belongs to D9 and is out of scope here.

### 2.10 Licence compatibility with an MIT OR Apache-2.0 project

| Licence | Projects | What coolprop-rs may do |
|---|---|---|
| MIT / Apache-2.0 / MIT OR Apache-2.0 | FeOs, num-dual, quantity, uom, Clapeyron.jl, thermo, chemicals, fluids, rfluids/coolprop-sys, the CoolProp Rust wrappers, seuif97, iapws95 (crates.io metadata only), rust-steam, vle-*, peritheos, twine-thermo, thermolib, KiThe, xsteamrs, tpt-eng-props-*, reos (Apache-2.0); thermopack (repo `LICENSE` is Apache-2.0, though its PyPI 2.2.3 classifier says MIT, [repo](https://github.com/thermotools/thermopack)); pycalphad (MIT on [PyPI](https://pypi.org/project/pycalphad/)); `wide` (Zlib OR Apache-2.0 OR MIT) | Adapt code, keeping attribution |
| BSD-3-Clause | Cantera, GSW-rs (and GSW-C) | Adapt code, keeping the notice |
| NIST disclaimer (US public domain; permission abroad with notice) | teqp, AGA8 / the aga8 crate | Adapt code, keeping the notice. Not OSI-approved, so add a cargo-deny clarification |
| **GPL-3.0(-only)** | outram-park-fork-coolprop, outram-park-fork-dwsim-libs, tampines-steam-tables, ushma, paspro/steam ([repo](https://github.com/paspro/steam)), DWSIM ([repo](https://github.com/DanWBR/dwsim)); BurnMan and SeaFreeze (see `materials-extensibility.md`) | **No code. Do not depend on them.** At most, high-level ideas already documented here |
| **LGPL-2.1** | Reaktoro ([repo](https://github.com/reaktoro/reaktoro)) | No code |
| **MPL-2.0** (file-level copyleft) | yaeos ([repo](https://github.com/ipqa-research/yaeos)) | No code copied into our files |
| Proprietary engines | REFPROP, ChemApp (via MIT-licensed bindings) | Out of scope |
| Data inside permissive repos | chemicals databanks ("cited and openly published data") | Check each source before importing any data |

---

## 3. Recommendations for coolprop-rs

### 3.1 Build like this

1. **P1: Term kinds form a closed enum, built once into arrays grouped by kind.** *(High / Medium)*
   - The default (first) EOS of the 136 CoolProp v8.0.0 fluids uses exactly **7 residual kinds**: Power 134 blocks, Gaussian 78, Exponential 10, NonAnalytic 2, Lemmon2005 1, GaoB 1, DoubleExponential 1.
   - It uses **10 ideal-gas kinds**: Lead, LogTau, PlanckEinstein, Power, EnthalpyEntropyOffset, CP0PolyT, PlanckEinsteinFunctionT, CP0Constant, CP0AlyLee, PlanckEinsteinGeneralized. These counts come from a local count over `reference/CoolProp/dev/fluids/*.json`.
   - **Correction (2026-10-04):** 23 fluid files carry a second, alternate EOS, and CoolProp's loader parses every entry (`parse_EOS_listing`, `FluidLibrary.h:453`) but evaluates only `EOSVector[0]`. Methanol's alternate EOS (Piazza 2013) uses an **8th residual kind, `ResidualHelmholtzAssociating`** (SAFT association, `FluidLibrary.h:149`). Counted over all EOS entries: Power 157, Gaussian 82, and the same 10 ideal-gas kinds. **Recommendation change:** the kernel enum still needs only the 7 kinds for v1. The serde mirror must, however, either model `ResidualHelmholtzAssociating` or skip alternate EOS entries explicitly, never fail on them, so that it can round-trip CoolProp JSON.
   - FeOs, teqp and the GPL port all use closed sets.
   - Build the kernel form in the constructor, sorted into uniform chunks. This is what Clapeyron does today and what CoolProp's SIMD plan asks for.
   - Keep the serde form separate and close to CoolProp JSON, for import and export.
   - Benchmark per-kind arrays against the plain array-of-structs (CoolProp's scalar loop) before committing.
2. **P2: An analytic derivative bundle in the HEOS hot path; AD as test oracle and as the default for other families.** *(Medium)*
   - The bundle uses the A_xy scaling (`τ^x δ^y ∂^{x+y}α`). That keeps p = ρRT(1 + A^r_01) finite at ρ = 0, which CoolProp's own derivation notes (`docs/superpowers/derivations/virial-axy.md`) show fails in v8.0.0.
   - Port CoolProp's `all()` arithmetic, including the `powInt` fast path. This gives near-bit-level oracle parity and a straight-line kernel that suits SIMD.
   - Every term kind gets a test that compares its hand-written derivatives with a dual-number evaluation. This is the safeguard teqp argues for.
   - Later families (PC-SAFT, mixture composition derivatives) may implement only `alphar<S: Scalar>`; a default method then derives the bundle with AD.
3. **P4: Two types instead of typestate.** *(High)*
   - `Fluid` is the immutable model, held in an `Arc`. `State` is a `Copy`-able value with private fields, built only by flash routines.
   - All getters take `&self`.
   - Do **not** copy FeOs's public fields or rfluids' `&mut` getters and output `HashMap`s.
4. **P5: SI `f64` in the kernel, newtypes at the edge.** *(Medium)*
   - Add typed constructors for the common pairs (`State::tp`, `ph`, `ps`, `dt`, `pq`, `tq`). A `StateFrom<(A, B)>` trait (twine pattern) is the generic version.
   - Add a runtime `InputPair` enum (`FromStr`, order-insensitive like rfluids) for the facades.
   - `uom`/`quantity` interop goes behind features.
   - Never engineering units in the kernel (the seuif97 and iapws95 mistake). Never two bare `f64` arguments in public signatures (the rust-steam mistake).
5. **P6: Shared immutable data with checks enforced by the compiler.** *(High)*
   - Assert `Send + Sync` at compile time, for example `const _: () = { fn f<T: Send + Sync>() {} f::<Fluid>(); };`.
   - No `static mut`, no `thread_local` caches, no global configuration. Pass options explicitly per call, as FeOs `SolverOptions` does.
   - Keep `cargo test` multi-threaded (CoolProp #2787 is the precedent).
6. **P7: CoolProp JSON stays the interchange format.** *(High)*
   - teqp's Water `alphar` (default EOS) is identical to CoolProp v8.0.0's, but the files are not byte-identical: teqp's copy lacks the `SUPERANCILLARY` block that v8.0.0 carries. FeOs and Clapeyron read the same format.
   - Both teqp and FeOs ship 124 fluids against v8.0.0's 136, but the sets differ. Both lack Chlorine, PropyleneGlycol, R1130(E), R1132(E), R1224yd(Z), R1336mzz(Z), Tetrahydrofuran, VinylChloride and the three n-perfluoroalkanes. teqp also lacks R1336mzz(E); FeOs has it but lacks R1123. **Pin our own v8.0.0 snapshot**; never take data from them.
   - Precompute the alias index (teqp's alias lookup reads every file).
   - Let WASM builds choose a subset of fluids (luisbedoia ships 30).
7. **P8: Side-by-side execution behind one trait.** *(High / Medium)*
   - Order: scalar reference first, then batch (slices in and out, a status per point), then SIMD across states for batches and SIMD across terms for single calls, each tested against the scalar path. Rayon only behind a feature.
   - Follow CoolProp's empirical order: remove redundant work first, then sort terms, then vectorise the multiply-add chains with libm `exp`.
   - Gate vectorised `exp` approximations on a VLE triple-point stress test.
   - No ISA-specific code without a portable fallback and tests for every target.
8. **P9: Errors as data.** *(High)* A structured `Result`; a status per point in batches; `log`/`tracing` only as optional features; a NaN mode only in facades (GSW-rs `invalidasnan`).
9. **P10: Several oracles.** *(Medium)*
   - Literature check tables.
   - CoolProp 8.0.0 fixtures from the pinned wheel.
   - Optionally, native-only fuzzing against `coolprop-sys = "=8.0.0"` behind a dev feature.
   - teqp 0.23.2 for GERG and multifluid; CoolProp master already generates its GERG reference values with it (see `docs/coolprop-map/06-eos-families.md`).
10. **P11/P12: Few crates, many features; thin facades.** *(Medium / High)*
    - Feature-gate model families; this is how both FeOs and teqp tame compile times.
    - The browser build uses `wasm32-unknown-unknown` + `wasm-bindgen`, as seuif97 does. Also provide WASI.
    - A C ABI (seuif97 `cdecl`, GSW-rs `capi`) and PyO3 (FeOs, num-dual) as later thin layers.

### 3.2 Illustrative sketch (not an API commitment)

```rust
/// Kernel form, built once from the serde mirror of CoolProp JSON (P1).
/// One block per term kind, coefficients as structure-of-arrays, sorted into uniform chunks.
pub(crate) enum ResidualBlock {
    Power { n: Box<[f64]>, d: Box<[f64]>, t: Box<[f64]>, l: Box<[f64]> },
    Gaussian { n: Box<[f64]>, d: Box<[f64]>, t: Box<[f64]>, eta: Box<[f64]>, eps: Box<[f64]>, beta: Box<[f64]>, gamma: Box<[f64]> },
    // Exponential, DoubleExponential, Lemmon2005, GaoB, NonAnalytic
}

/// A_xy-scaled residual bundle: a_xy = τ^x δ^y ∂^(x+y) αr / ∂τ^x ∂δ^y (P2). Finite at δ = 0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ar2 { pub a00: f64, pub a10: f64, pub a01: f64, pub a20: f64, pub a11: f64, pub a02: f64 }

impl ResidualBlock {
    /// Scalar reference. Batch and SIMD variants sit beside it and are tested against it (P8).
    pub(crate) fn add_ar2(&self, tau: f64, delta: f64, acc: &mut Ar2) { /* match self { … } */ }
}
```

---

## 4. Warnings

- **GPL near-miss.** A pure-Rust CoolProp translation already exists (outram-park-fork-coolprop). It is **GPL-3.0-only for the whole crate**, and by its own README unvalidated ("No human V&V has been done yet"). Do not copy code, port structure from its source, or add it as a dependency, even in tests. The same applies to tampines-steam-tables, outram-park-fork-dwsim-libs, ushma, paspro/steam, Reaktoro (LGPL) and yaeos (MPL).
- **num-dual stability.** It had 5 breaking minor releases (0.11–0.15) in the 20 months from 2024-12 to 2026-08. It does not support `no_std`. nalgebra is on by default. It cannot carry SIMD lanes. If it ever appears in a public signature, every num-dual bump becomes a coolprop-rs breaking change.
- **FeOs is not a CoolProp-parity oracle** for multiparameter fluids. Its ideal-gas mapping is a self-described "hack", its maximum density is a heuristic, it has no ancillaries or transport for these fluids, and its data snapshot is older (124 fluids).
- **The performance figures here are self-reported** on different hardware: Apple M-class NEON (CoolProp), i7-7700K (FeOs), a Docker container on Windows (teqp, *unverified*), and the authors' own benchmarks (seuif97). pyJac's figures come from chemical kinetics, not from EOS evaluation. Re-measure in coolprop-rs benchmarks before relying on any of them.
- **CoolProp's SIMD evidence may disappear.** It lives on an unmerged branch, and the author said the branch "will be deleted after issue is filed". The pinned raw links resolved on 2026-10-04. The persistent records are PR #3044 and issue #3046. This document keeps the numbers.
- **Floating-point portability.** Within-state SIMD changes the summation order, and vectorised `exp` changes results by a few ULP. CoolProp's lead reports that VLE near the triple point needs 1–2 ULP. Results will also differ between platform `libm` implementations: glibc on Linux, the MSVC CRT on Windows, and on `wasm32-unknown-unknown` the port of the `libm` crate (musl-derived) that `compiler-builtins` bundles for `exp`, `log`, `pow` and others (verified in `compiler-builtins/src/math/mod.rs`, `partial_availability`). Tolerance classes in D12/D13 must allow for this. Calling the `libm` crate (0.2.16, MIT) explicitly in the kernel would give the same transcendental results on every target, at a speed cost nobody has measured. That option belongs to D9/D13.
- **The NIST notice is not an OSI licence.** Licence scanners such as cargo-deny will flag teqp- or AGA8-derived code unless it is clarified explicitly.
- **Binary provenance.** coolprop-sys platform crates embed prebuilt CoolProp libraries (13 MB on Linux x86-64), and the README does not say how they are built. This matters if it is used even as a dev-dependency.
- **The Cantera thread-safety statement is unverified for 3.x.** Rely only on the documented fact that `ThermoPhase` stores its state internally.

## 5. Open questions

1. **How should the HEOS kernel get its derivatives?** This document recommends an analytic bundle with AD as oracle. `docs/coolprop-map/06-eos-families.md` proposes a single generic `alphar<S: Scalar>` entry point. Should we do both: a generic reference implementation, overridden by an analytic bundle for HEOS? Proposed tie-breaker: a one-day spike on Water (54 terms) and Propane (18 terms) comparing the ported `all()` against `Dual2Vec<2>` and 3 × `HyperDual64`, for speed and for agreement with CoolProp 8.0.0.
2. Depend on `num-dual` with no default features, or write a minimal in-house dual / hyper-dual just for tests and later families?
3. Units interop: support `uom`, `quantity`, both or neither behind features? This needs compile-time and ergonomics measurements.
4. Is a native-only `coolprop-sys = "=8.0.0"` dev-dependency acceptable for differential fuzzing, given the undocumented binary provenance? Or should all oracle data come from the pinned wheel?
5. Derivative naming: CoolProp names (`dalphar_dDelta`), or teqp-style A_xy (`ar(x, y)`), which CoolProp itself plans to adopt?
6. **Ecosystem "connectedness":** should a later adaptor crate implement `feos_core::ResidualDyn` for coolprop-rs fluids? That would let FeOs's critical-point and phase-equilibrium routines (and its future DFT) consume our models. Benefit and maintenance cost are unclear while FeOs keeps redesigning its traits.
7. Should coolprop-rs follow CoolProp issue #3046 (post-v8 SIMD with term sorting) so that any future change in CoolProp's summation order does not make oracle comparisons drift?

## Verification log

**Date:** 2026-10-04. **Method:** an adversarial re-check against primary sources:
- the crates.io API (versions, release dates, licences, MSRV, edition, downloads, dependency lists);
- GitHub, through `gh api` (pinned-commit sources, READMEs, issue and PR states and bodies, release dates, licence files);
- PyPI JSON;
- Cantera and thermo documentation;
- arXiv (pyJac) and Frontiers (num-dual paper);
- the local CoolProp v8.0.0 checkout (`ae81610e`).

**Claims checked:** about 150. Confirmed without change include:
- all crate versions and dates in §2.1, §2.2 and §2.8, and the `wide`, `pulp` and `multiversion` versions and licences;
- the FeOs `State` fields, its 11-slot `OnceLock` cache, `Residual` and `ResidualDyn`, the multiparameter `panic!`/"hack"/"Not sure" quotes, its 124 fluids and the feos-core dependency list;
- the num-dual default `nalgebra` feature, nalgebra-gated types, `DualNumFloat` for `f32`/`f64` only, no `no_std` and the 0.13.5 comparison semantics;
- `quantity`'s build-script `const_impls.rs`;
- the teqp 0.23.2 date, NIST licence, 124 fluid files, thread-safety comment, free-threading note and term-container list;
- the Clapeyron hooks, `PolExpGaussTerm` structure-of-arrays layout and NaN-tuple docs;
- the Cantera 3.2.0 date, BSD-3 text, deprecation of `lattice` and `compound-lattice`, and transport keys;
- the thermo 0.6.1 and chemicals 1.5.2 versions and quotes;
- the coolprop-sys `RwLock` and `!Sync` design, rfluids' `&mut self` getters and `HashMap` caches, and the 13 MB `libCoolProp.so` (12,991,816 bytes) with an undocumented build;
- CoolProp #3044, #3046, #3034, #2787, #2800, #2831 and #3220, and IF97 #51 and #54;
- the PERF and RECON report numbers (1.33×, 1.167×, 1.211×, 1.414×, Water 814 → 597 ns, 0.49–0.71×, seven `ArrayXd::Zero`);
- the pyJac quotes;
- the in-tree Rust wrapper defects, `PropsSI` returning `_HUGE`, and the `allEigen` code commented out at `Helmholtz.cpp:38` and `Helmholtz.h:549`;
- `std::simd` #86656 (open, updated 2026-08-12).

**Corrections made in place:**
1. **Term kinds (P1, P7; recommendation changed).** The "7 residual kinds cover all 136 fluids" claim holds only for the default (first) EOS. 23 files carry an alternate EOS, which CoolProp parses but does not evaluate. Methanol's alternate EOS uses an 8th kind, `ResidualHelmholtzAssociating`. The serde mirror must now handle or explicitly skip it. The kernel enum is unchanged.
2. **teqp Water data (P7).** The data is not byte-identical to CoolProp's. Only the default-EOS `alphar` is identical; teqp lacks v8.0.0's `SUPERANCILLARY` block.
3. **Missing fluids (P7).** teqp and FeOs both have 124 fluids, but the sets differ: FeOs has R1336mzz(E) and lacks R1123. The missing-fluid list now says so.
4. **num-dual churn.** "4 breaking minor releases in 20 months" became "5 (0.11–0.15) in 20 months".
5. **CoolProp #2754 and AD.** "Declined AD" was an inference. The issue was closed as "completed" with no comment or linked change. The wording now says AD was not adopted, with no stated rationale. The P2 recommendation is unchanged.
6. **seuif97 speed-up.** "4.1–6.4×" covered only test case 1. The full self-reported range is 2.9–7.8× per property (README: "3-7x").
7. **refprop-sys.** It is not on crates.io; it is the GitHub repo ahjortland/refprop-sys (MIT).
8. **FeOs threads.** `FEOS_MAX_THREADS` dates from 0.9.5 (Python), not 0.10.
9. **Clapeyron.** 0.6.29 was tagged 2026-09-27. Unitful is also a hard dependency.
10. **thermo quote.** Corrected to "If the property is not reasonable, None is returned".
11. **Licence and other nuances added:** mcurrie99 has an opt-in feature without the lock; KiThe and xsteamrs have no licence that GitHub detects; outram-park's "137 fluids" is its own count.
12. **WASM `libm`.** Previously unverified, now confirmed: on `wasm32-unknown-unknown`, `exp`, `log` and `pow` come from compiler-builtins' bundled `libm` port. Calling the `libm` crate explicitly for cross-target identical results is noted as a D9/D13 option.

**Still unverified:**
- FeOs paper Tables 2 and 3. The paper is open access, but the publisher blocked automated retrieval.
- The teqp paper's REFPROP comparison and its Docker/Windows hardware.
- Any direct teqp-vs-CoolProp timing.
- A FeOs `wasm-bindgen` package. None was found on npm; absence is not proof.
- The Cantera thread-safety note for 3.x. The current docs say nothing about threads.
