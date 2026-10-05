# Proposal B: Extensible

Type sketch (compiles; 18 tests): `<session scratchpad>/sketch-extensible`.
Citations: "map NN §X / rot ID" = `docs/coolprop-map/NN-*.md`; "research Kx/Rx/Px/Sx" = `docs/research/{kernel-performance,dependencies,prior-art,materials-extensibility}.md`. Anything else is labelled *inference*.

## 0. Thesis

**Close what is known, open what will grow.** Inside a family, everything is closed: term kinds, transport forms and ancillary forms are enums compiled to SoA blocks with `match` dispatch. Across families, a std-only core offers exactly **four small object-safe traits**: `HelmholtzModel` (an EOS), `ThermoModel` (anything you can flash), `Transport` and `Catalog`. They exchange one reducing-invariant currency, `Derivs2` (CoolProp's scaled `A_ij`, the same as teqp's Λ_ij), held in a `Copy` `State`.
**One family = one crate = code + data + catalog.** Cubic, PC-SAFT, GERG (as data), IF97, INCOMP, ice Ih and later solids land without a single core edit. The sketch proves this in `crates/cprs/tests/open_closed.rs`: a value-only Helmholtz family and a Gibbs solid, both defined outside every first-party crate, flash, batch and take reference states through the unchanged core.

## 1. Layers and crate layout

```text
 facades   cprs-capi (C ABI)        cprs-wasm (browser: wasm-bindgen | WASI: wit-bindgen)      [cprs-py later]
                 \___________________________|______________________________________/
 facade    cprs ── Library{ Arc<[Arc<dyn Catalog>]> } · "FAMILY::A&B" resolver · compat::props_si
                 │ registers catalogs: 1 line per first-party family (Cargo feature); 3rd parties: LibraryBuilder
 families  cprs-heos (v1) ┆ cprs-cubic ┆ cprs-pcsaft ┆ cprs-iapws (IF97, ice Ih) ┆ cprs-incomp ┆ cprs-humidair
           code + data + catalog; closed enums + SoA inside          (┆ = future crate, same shape)
                 │ impl HelmholtzModel / ThermoModel / Transport / Catalog      (dependencies point down only)
 core      cprs-core:  num (Real, Jet2) ─ math ─ derivs (Derivs2) ─ relations ─ state (State, Phase)
                       model::HelmholtzModel ─ flash (generic) ─ fluid::{Fluid, Gauged} ─ thermo::{ThermoModel,
                       Transport, Catalog} ─ batch (ExecPolicy) ─ input/units/composition/gauge/error/provenance
 dev       cprs-verify (fixtures, tolerance classes, divergence register, conformance suite) · cprs-bench · cprs-xtask
```

| Crate | Responsibility | Depends on | Features |
|---|---|---|---|
| `cprs-core` | Kernel contracts and generic algorithms: `Real`/`Jet2`, the `math` choke point, `Derivs2`, `relations`, `State`, `Input`→`Spec`, the 4 traits, the generic Helmholtz `flash`, `Fluid`, `Gauged` and the `batch` executor. No globals, locks, env or fs. | std only (R1) | planned, additive: `rayon` (Parallel executor), `libm` (bit-identical math), `serde` (derives) |
| `cprs-heos` | The CoolProp multiparameter family: term enums + SoA, ideal gas, superancillary, ancillaries, melting, CoolProp transport forms, `MultiFluid` (GERG-2008 = data), the embedded v8.0.0 data, `HeosCatalog` | core | `all-fluids` (default), `tdd5`, `fluid-<name>` (generated; WASM subsets). Dataset is chosen at runtime (`DatasetId`) |
| `cprs` | Rust facade: `Library`, fluid-string grammar, `compat` (PropsSI-style), re-exports | core, heos (optional) | `heos` (default); later `cubic`, `pcsaft`, `iapws`, `incomp`, `humidair`; `rayon` passthrough |
| `cprs-capi` | C ABI: opaque `Arc` handles, status codes, thread-local last error, string keys | cprs | `coolprop-compat` (CoolPropLib.h-named shims) |
| `cprs-wasm` | Browser (wasm-bindgen; baseline + `simd128` builds) and WASI component (wit-bindgen) | cprs | `browser`, `wasi` |
| `cprs-verify` *(unpublished)* | Fixture CSV reader, `Provenance`, `ToleranceClass`, divergence register, family **conformance suite** | core | none (zero third-party deps, R18) |
| `cprs-bench` *(unpublished)* | criterion + gungraun benches and perf gates | cprs | none |
| `cprs-xtask` *(unpublished)* | `datagen`, `oracle`, `deps-guard`, `notices` | tools only (serde_json, nalgebra: tier T4) | none |
| *future* `cprs-cubic`, `cprs-pcsaft` | Each implements `HelmholtzModel` + `Catalog`, borrowing identity and ideal gas from HEOS via `Fluid::eos_shared` | core, heos | none |
| *future* `cprs-iapws` | IF97 (region map; Gibbs + Helmholtz region 3), IAPWS-06 ice Ih, Gibbs→`Derivs2` transform | core | none |
| *future* `cprs-incomp`, `cprs-humidair`, `cprs-simd`, `cprs-py` | INCOMP correlations; humid air with injected models; lanes (MSRV 1.89, R3); pyo3 abi3 | core (+ injected models) | — |

**Crate rule (anti-sprawl):** a family gets a crate only if it brings its own data, provenance or dependency. Otherwise it is data or a module: GERG-2008 is data on `MultiFluid` (map 06 U6), pseudo-pure fluids are data, and VTPR is deferred (map 06 U13). v1 ships 5 published crates and 3 unpublished ones.

## 2. Core types and traits (excerpts, in sync with the sketch)

```rust
// cprs-core::derivs: the currency. A_ij = tau^i delta^j d^(i+j)alpha = teqp Lambda_ij for ANY reducing pair.
pub struct Derivs2<R = f64> { pub a00: R, pub a10: R, pub a01: R, pub a20: R, pub a11: R, pub a02: R }
pub struct AlphaSplit { pub ideal: Derivs2, pub residual: Derivs2 }      // total() = ideal + residual
#[non_exhaustive] pub enum Parts { Residual, Ideal, Both }

// cprs-core::num: write a formula once; f64 = reference, Jet2 = exact 2nd derivatives, lanes later.
pub trait Real: Copy + Add<Output=Self> + Sub<Output=Self> + Mul<Output=Self> + Div<Output=Self> + Neg<Output=Self>
              + Send + Sync + 'static {
    type Mask: Copy; const LANES: usize;
    fn cst(x: f64) -> Self; fn exp(self) -> Self; fn ln(self) -> Self; fn powi(self, n: i32) -> Self;
    fn powf(self, c: f64) -> Self; fn sqrt(self) -> Self;
    fn lt(self, rhs: Self) -> Self::Mask; fn select(m: Self::Mask, a: Self, b: Self) -> Self;
}
pub struct Jet2 { pub v: f64, pub t: f64, pub d: f64, pub tt: f64, pub td: f64, pub dd: f64 } // impl Real

// cprs-core::model: extension point 1, Helmholtz families (HEOS, MultiFluid/GERG, cubic, PC-SAFT, IF97-R3).
pub trait HelmholtzModel: Send + Sync + 'static {
    fn info(&self) -> &EosInfo;                                  // r (own gas constant), components, limits, Source
    fn alpha(&self, t: f64, rho: f64, x: &Composition, parts: Parts) -> Result<AlphaSplit, EvalError>; // unchecked
    fn alpha_batch(&self, t: &[f64], rho: &[f64], x: &Composition, parts: Parts,
                   out: &mut [Result<AlphaSplit, EvalError>]) { /* default: scalar loop = reference */ }
    fn rho_max(&self, t: f64, x: &Composition) -> f64;          // bracketing from the model, not metadata
    fn saturation(&self, _at: SatAt) -> Option<SatEstimate> { None }   // SA: Exact | ancillary: Guess | Definition
    fn melting_temperature(&self, _p: f64) -> Option<f64> { None }
    fn critical_point(&self) -> Option<CriticalPoint> { None }        // typed: Numerical | Published
}

// cprs-core::thermo: extension point 2 (anything flashable) and its two satellites.
pub trait ThermoModel: Send + Sync + 'static {
    fn info(&self) -> &ModelInfo;                // family: FamilyId(&'static str), pairs: PairSet, native_gauge, Source
    fn flash(&self, spec: Spec, x: &Composition, opts: &FlashOptions) -> Result<State, Error>;   // pure
    fn transport(&self) -> Option<&dyn Transport> { None }
}
impl dyn ThermoModel { pub fn state(&self, input: Input, x: &Composition, o: &FlashOptions) -> Result<State, Error> }
pub trait Transport: Send + Sync + 'static {
    fn source(&self) -> &Source;
    fn viscosity(&self, _s: &State) -> Result<f64, PropError> { Err(PropError::Unsupported(Prop::Viscosity)) }
    fn conductivity(&self, _s: &State) -> Result<f64, PropError> { /* same */ }
    fn surface_tension(&self, _t: f64) -> Result<f64, PropError> { /* same */ }
}
pub trait Catalog: Send + Sync + 'static {
    fn family(&self) -> FamilyId;                                   // "HEOS", "PR", "PCSAFT", "INCOMP", "IF97"
    fn resolve(&self, name: &str) -> Option<ModelKey>;              // case-folded name/alias/CAS
    fn load(&self, key: ModelKey) -> Result<Arc<dyn ThermoModel>, LoadError>;   // OnceLock slot per model
    fn load_mixture(&self, _keys: &[ModelKey]) -> Result<Arc<dyn ThermoModel>, LoadError> { Err(LoadError::Unsupported) }
    fn names(&self) -> Vec<Arc<str>>;
}

// cprs-core::fluid: two family-agnostic ThermoModels built from the extension points.
pub struct Fluid { info: ModelInfo, eos: Arc<dyn HelmholtzModel>, transport: Option<Arc<dyn Transport>> }
pub struct Gauged { inner: Arc<dyn ThermoModel>, per_component: Box<[Gauge]>, reference: ReferenceState }
impl Gauged { pub fn new(inner: Arc<dyn ThermoModel>, r: ReferenceState) -> Result<Self, Error> } // IIR/ASHRAE/NBP/Custom for ANY family

// cprs-core::state: Copy, 192 bytes (asserted <= 256), no Arc, no cache.
pub struct State { t: f64, rho: f64, p: f64, r: f64, molar_mass: f64, phase: Phase, gauge: Gauge, body: Body }
enum Body { Single { total: Derivs2, residual: Option<Derivs2> }, Two { q: f64, liquid: SatPoint, vapour: SatPoint } }
impl State { pub fn single_phase(p: SinglePhaseParts) -> Result<State, EvalError>  /* public family constructor */
             pub fn h_mass(&self) -> f64;  pub fn cp_molar(&self) -> Result<f64, PropError>;  /* ... */ }
#[non_exhaustive] pub enum Phase { Liquid, Gas, TwoPhase, Supercritical, SupercriticalGas, SupercriticalLiquid,
                                   CriticalPoint, Solid }

// cprs-heos::terms: closed inside the family; hand fast path + AD route side by side (D2).
#[non_exhaustive] pub enum ResidualTerm { Power{..}, Exponential{..}, Gaussian{..}, Lemmon2005{..},
                                          DoubleExponential{..}, NonAnalytic{..} }   // fn value<R: Real>(tau, delta) -> R
impl PowerBlock { pub fn accumulate(&self, tau: f64, delta: f64, out: &mut Derivs2) {
    let (ln_tau, ln_delta) = (math::ln(tau), math::ln(delta));
    for i in 0..self.n.len() {                               // SoA, branch-free on per-term data
        let (t, d, l, c) = (self.t[i], self.d[i], self.l[i], self.c[i]);
        let lf = f64::from(l);
        let e = c * math::powi(delta, l);                    // c = 0 when l = 0 ("0 means absent", map 02)
        let a = self.n[i] * math::exp(t * ln_tau + d * ln_delta - e);   // one exp per term
        let k = d - lf * e;
        out.a00 += a; out.a10 += t * a; out.a01 += k * a;
        out.a20 += t * (t - 1.0) * a; out.a11 += t * k * a; out.a02 += (k * (k - 1.0) - lf * lf * e) * a;
    } } }

// cprs-core::batch: "what" (model) vs "how" (policy). SoA in, columns out, u8 status per point.
#[non_exhaustive] pub enum ExecPolicy { Reference, Auto, Parallel { chunk: usize } }
pub fn run(model: &dyn ThermoModel, req: &BatchRequest<'_>, out: &mut BatchOutput<'_>, policy: ExecPolicy) -> Result<(), Error>;

// D8, compile-time (cprs-core/src/lib.rs; also in cprs-heos and cprs):
const _: () = { const fn shared<T: ?Sized + Send + Sync + 'static>() {}
    shared::<dyn HelmholtzModel>(); shared::<dyn ThermoModel>(); shared::<dyn Catalog>(); shared::<Fluid>(); /* ... */ };
```

## 3. Decisions D1-D17

**D1 Workspace / crates / features.**
- *Decision:* the §1 layout: a core contract crate, one crate per family (code + data + catalog), thin facades, and three unpublished dev crates. Inside `cprs-core`, modules rather than crates. Features only add capability (fluids, executors, families).
- *Rationale:* crate boundaries are placed where change is independent. A new family ships, versions and carries its provenance without recompiling or re-releasing the core (map 12 R13: a new transport form needed 4 core edits). The T0 zero-dependency rule is enforced per crate (R1). Modules inside core follow K §3.1: "split only when a second consumer appears".
- *Rejected:*
  - One crate with feature-gated family modules: Open/Closed by convention only, mixed provenance, every family change re-releases the kernel.
  - A crate per layer (num, relations, flash, state): 10+ crates for one consumer (P11).
  - A separate data crate: the data and its decoder change together, and datasets are a runtime `DatasetId`.

**D2 Numeric core and derivatives.**
- *Decision:* the kernel uses `f64`. Formulas are written once over the in-house `Real` trait (`f64`, `Jet2`, later lanes). Derivatives come by two routes, side by side:
  - (a) Hand-derived fast paths per hot block, e.g. `PowerBlock::accumulate`, the generalised one-`exp` B-factor style of map 02 §9.
  - (b) The AD route: each term kind defines only `value<R: Real>`, and `Jet2` gives exact order-2 derivatives. (b) is correct on day one for every kind and is the oracle for (a) (term class, map 10 §8.3). New families (PC-SAFT, GERG departures, user EOS) write only (b).
- *Rationale:* map 12 R5 (hand derivatives, 780 NLOC; wrong XN_DEPENDENT/CP0PolyT); map 06 P5 (PC-SAFT copied 4×, ~1.45k lines); map 02 (exp-of-jets covers 99.8% of terms); research R5, K8, P2. The sketch's unit test checks the power block against 50-digit hand values and against `Jet2` (≤1e-14).
- Order 3 (critical point, spinodal, fundamental derivative) gets a `Jet3` and a default trait method when the critical-point solver lands. That is additive.
- *Rejected:*
  - Hand-coded only (the CoolProp rot above).
  - `num-dual` in the core: its base type is limited to f32/f64, so it cannot carry lanes, and it is an extra dependency (K §2.6, R5). It stays a dev-oracle candidate.
  - `std::autodiff`: nightly-only (R4).
  - A public API generic over the scalar: it leaks type parameters into every signature.

**D3 Model representation (the Open/Closed decision).**
- *Decision:* closed inside a family, open across families. Term kinds, transport forms and ancillary forms are `#[non_exhaustive]` enums compiled to SoA, with `match` dispatch. Families sit behind the object-safe traits of §2.
  - Dynamic dispatch happens once per `alpha` call or per batch, never per term.
  - `FamilyId` is an open `&'static str` token, not a core enum.
  - Optional capabilities are default methods returning `None`, never NotImplemented (map 12 R2: 181 virtuals, 136 `NotImplementedError`s).
- *Rationale:* map 06 §9 proposes a closed `enum Residual {MultiFluid, Cubic, PcSaft}` plus an object-safe plugin facade. We keep only the object-safe path, so first-party and third-party families are equal citizens and the core never depends on a family. That also keeps WASM binaries free of unused families.
  - Cost (*inference*): one indirect call (~1-3 ns) per ~300 ns α evaluation (K §3.4 target), so ≤1%. M8 gates this at ≤2%.
  - Escape hatch: `flash::helmholtz` can become generic over `M: HelmholtzModel + ?Sized`. It is a free function, so that change is local.
- *Rejected:*
  - A closed family enum in core: core edits per family, and core links every family.
  - A fully generic `State<M>`/`flash<M>`: monomorphisation bloat, and a heterogeneous registry needs `dyn` anyway.
  - `dyn` per term: a virtual call per term destroys SoA.

**D4 Pure vs mixture.**
- *Decision:* composition is in the contract now (`&Composition` per call). Pure fluids pass the static `[1.0]` and their models ignore it.
  - A mixture is one `Arc<MultiFluid>` per **trimmed** component set: zero fractions are rejected, because GERG reducing functions have no second-order limit there (map 04, c7d6a1aa).
  - Composition per call lets one `Arc` serve many compositions concurrently.
  - GERG-2008 = data on `MultiFluid` (map 06 U6). Phase compositions of two-phase mixtures come later as a separate `MixtureFlash` value (it allocates), not inside `State`.
- *Rationale:* map 04 counts about 100 runtime pure-vs-mixture branches and about 6 deep copies per pure state. Our pure path is a different type, and the mixture path costs nothing until used. Adding mixtures later breaks no signature.
- *Rejected:*
  - Composition inside the model: one `Arc` per composition, and caches keyed by floats.
  - A pure-only contract now: a breaking change later.

**D5 State, properties, inputs, units.**
- *Decision:* `State` is `Copy` and 192 bytes, as in §2.
  - It holds the model's own `r` and `M`, so every thermo property is a pure function of the `State` and the same code serves Helmholtz and Gibbs families.
  - The bundle is computed eagerly by the flash (it already evaluated it). Properties are a few flops on demand. There are no lazy caches.
  - Two-phase cp/cv/w and transport return `Err(TwoPhaseUndefined)`.
  - Typed methods are primary. `Prop` (`#[non_exhaustive]`, CoolProp names via `FromStr`, `category()`) serves the batch, PropsSI and C-ABI paths.
  - `Input` has 19 typed pairs whose quantities carry their basis. One `to_spec(M)` turns it into the kernel's molar `Spec`.
  - Validated `repr(transparent)` newtypes live at the edge; the kernel uses raw SI `f64`.
- *Rationale:* map 01 U2/U6, R3/R4 (unreachable QS/HQ pairs, HmassT asymmetry); map 07 R1/H1 (positional mix-ups); research P4, P5, R11, S5, S6.
- *Rejected:*
  - `OnceCell` caches in `State`: they break `Copy`, and the bundle already exists.
  - `uom` in core: generic signatures (R11).
  - CoolProp's 43 basis-specific pairs.
  - An `Arc` inside `State` (K1).

**D6 Flash architecture.**
- *Decision:* pure functions over `&dyn HelmholtzModel`, one module per pair family (no 159-CCN `HSU_D_flash`, map 12 R2).
  - Phase comes, in order, from: the per-call hint; the superancillary (`Exact`); ancillary guesses plus VLE (`Guess`); pseudo-pure ancillaries (`Definition`); later, stability tests for mixtures.
  - The shared `solve` toolbox (`Root{x,f,iterations,status}`, typed `Tolerance`, TOMS748 with an enforced `max_iter`) also serves non-Helmholtz families.
  - `RootPolicy::{Unique, Stable}`: with `Unique`, multiple roots return `FlashError::Ambiguous{roots}`.
  - A single verify gate checks: finite, residual, in domain, dp/dρ > 0 for stable roots.
  - `FlashOptions` is `Copy` and immutable. Families with their own inverses (IF97 backward equations) implement `ThermoModel::flash` and reuse `solve`.
- *Rationale:* map 03 (sticky `specify_phase`; solver returns x_{n+1} with the state of x_n; exhausted `max_iter` never checked; non-unique T+H); map 06 P2/P8/P9 (PC-SAFT's bespoke flash fails supercritical PT); map 12 R3 (237 `catch(...)`).
- *Rejected:* per-family flashes, exception cascades, global config, Q sentinels.

**D7 Data pipeline.**
- *Decision:* pinned v8.0.0 JSON → `xtask datagen`, which:
  - parses preserving literal kinds and checks the FNV `source_eos_hash` gate (map 09);
  - maps everything into closed enums and validates as real errors, not debug asserts (map 10);
  - applies the cited `Corrected` overlay;
  - precomputes superancillary extrema.
  It emits LE binary sections (~22 KiB/fluid, 2.81 MiB total) plus a sorted, case-folded name/alias/CAS index (556 keys), committed. Loading:
  - `include_bytes!` per fluid behind features;
  - one `OnceLock<Result<Arc<Fluid>, LoadError>>` slot per fluid inside `HeosCatalog`; the superancillary sits in a second lazy cell;
  - ECS reference fluids resolve lazily through a `Weak` catalog handle (no `Arc` cycle), with the DAG checked at datagen;
  - runtime bytes (WASM fetch, user JSON) go behind a `json` feature (serde_json, tier T1) that builds a **user `Catalog`**, never a global mutation.
- Two datasets: `CoolProp800`, bit-identical to the oracle, used for parity, and `Corrected`, the default.
- *Rationale:* map 09 §9 D1-D9; map 02 (lazy superancillary: eager costs 1.8 s and 11 MB); map 14 (first use decodes every fluid: 1.9 s, +67 MiB); R6-R8.
- *Rejected:* JSON at runtime in core, CBOR or bincode (RUSTSEC-2025-0141), compression (13-19% gain), `build.rs` codegen, a global mutable library.

**D8 Concurrency kernel.**
- *Decision:* immutable `Arc<dyn ThermoModel>` with no locks on the hot path.
  - `clippy.toml` `disallowed-types` (Mutex, RwLock, Cell, RefCell) in core (K3). `OnceLock` is the only cell.
  - Compile-time `Send + Sync + 'static` assertions in every crate.
  - `State` is a `Copy` value. Scratch lives on the stack.
  - The batch API is SoA with caller-owned buffers and a `u8` `Status` per point. Failed cells are NaN, never stale.
  - `rayon` is optional, with fixed chunks independent of the thread count. Warm starts are opt-in (K6/K7).
  - `Library::global()` is an opt-in `LazyLock`, read-only.
  - The only lock in the system is the compat facade's mixture-string cache, outside the kernel.
- *Rationale:* map 11 (6 fluid copies per state, 100-300 KiB, 52-56 µs to construct); map 07 (HumidAir `thread_local` backends); K1-K7. The sketch test runs 16 threads bitwise-equal to sequential, and `Parallel{64}` matches `Reference` bitwise.
- *Rejected:* per-thread model copies, `RwLock<HashMap>` registries (K2), `thread_local!`.

**D9 Execution strategies.**
- *Decision:* *what* = model math generic over `Real`, plus an overridable `HelmholtzModel::alpha_batch`. *How* = `ExecPolicy` executors in `core::batch`.
  - `Reference` (scalar) is the source of truth.
  - Lanes come in steps:
    1. a portable `[f64; 4]` lane type in core (auto-vectorised);
    2. `cprs-simd` (core::arch AVX2 / NEON, or fearless_simd 1.0), only if (1) misses the ≥2.5× gate;
    3. lanes across states first, bitwise equal to scalar (K9, K17).
  - Dispatch is detected once per process (`OnceLock` fn table). WASM `simd128` is chosen at compile time (K11, K15). Features never change numerics (K13).
  - Qualifying kernels: α term sums, relations, superancillary Clenshaw, ancillaries, transport, and lockstep ρ(T,p) with a scalar retry.
  - Never vectorised: flash cascades, VLE, stability, association iteration. These run in parallel across requests only (K14).
- Every family inherits the scalar `alpha_batch` and can add lanes later without a core edit.
- *Rejected:* `std::simd` (nightly), single-ISA paths, GPU (K16), FMA in deterministic kernels (K12).

**D10 Material / states-of-matter seams.**
- *Decision:* build nine cheap seams now:
  1. the `Derivs2` currency, which needs no reducing state (S1);
  2. family-neutral `relations`;
  3. `ThermoModel` with a public `State::single_phase` and `Phase::Solid`. A Gibbs model converts g-derivatives to `Derivs2` by an exact Legendre transform, proven in `open_closed.rs`: cp = −T g_TT to 1e-10;
  4. `Gauge` + `NativeGauge` + `Gauged`, so aligned zeros across models are possible (map 15);
  5. `DomainError::BelowMeltingLine` and private `Limits`, so domains can later have holes (S4);
  6. per-family key enums (S6);
  7. `Source`/`DataTerms` (S8);
  8. `#[non_exhaustive]` everywhere (S9);
  9. `Catalog`, an open registry.
- *Deferred:*
  - `Substance` / multi-phase selector. Min-Gibbs across `Arc<dyn ThermoModel>` needs no new trait, because g comes from any `State`.
  - The Gibbs transform stays in `cprs-iapws` until a second Gibbs crate exists.
  - `Domain` with holes, the RegionAtlas port, the `correlation` module (lands with INCOMP), solid EOS, tensors, plasma, CALPHAD, the `Formation` gauge, marker-type keys.
- *Deviation from research R1 (no `PhaseModel` in v1), with reason:* `ThermoModel` is not a speculative material trait. It is the CoolProp backend contract (map 01 U6 sketches the same `Model::flash`), and four planned CoolProp ports implement it: `Fluid`, IF97, INCOMP and ice Ih (humid air needs ice). `Substance`/`Selector` stay deferred exactly as R1 says.

**D11 Facades.**
- *Decision:*
  - `cprs`: native Rust API plus `compat::props_si`, which returns `Result`, and `props_si_or_inf`, which uses CoolProp's +inf policy. Legacy policies live only in the facade (map 14).
  - `cprs-capi`: `extern "C"`, cbindgen CLI, `catch_unwind`, status + thread-local error, string keys. `Status` is the only `repr(u8)` enum crossing the ABI, append-only (map 01: v8 renumbered 64/86 parameters). An optional `coolprop-compat` shim keeps the CoolPropLib.h names (map 11).
  - `cprs-wasm`: two builds (baseline, `simd128`), plus a wasip2 component.
  - `cprs-py`: later (pyo3 0.29 abi3, R16).
- A network server is an outer facade, never core. Plotting, isolines and cycle helpers (CoolPropPlot) are deferred.
- *Rejected:* uniffi/safer-ffi (R15); enum discriminants as ABI; a C ABI inside core.

**D12 Errors, panics, NaN, FP determinism.**
- *Decision:*
  - Hand-written `#[non_exhaustive]` error enums: `Error` ⊃ Input / Domain / Flash / Eval / Prop / Load / Lookup.
  - No panics on input. `debug_assert!` is for internal invariants only. The C ABI catches unwinds.
  - NaN: rejected by newtypes at the boundary, and per point in batches (`Status::InvalidInput`). A non-finite raw evaluation is rejected when the `State` is built (`EvalError::NonFinite`).
  - FP: no `mul_add`; one `math` choke point (std by default; `libm` feature for bit-identical Linux/Windows/WASM; in-house vector exp/ln when lanes land); fixed summation order.
  - Fixtures are generated on x86-64 Linux and checked by tolerance class, never bitwise against CoolProp (K17/K18). CI compares an output hash across targets under `libm`.
- *Rejected:* sentinels (`_HUGE`, −1), a global error slot in core, `thiserror` (R10).

**D13 Verification architecture.**
- *Decision:* `cprs-verify` (zero-dep) implements:
  - the map 10 pyramid L0-L6;
  - CSV fixtures with a `#` header (oracle SHA `ae81610e`, all 38 config keys, scrubbed env, fresh AbstractState per case);
  - `Provenance` priority: paper > multiprecision > CoolProp8 > self;
  - `ToleranceClass`: exact, term, prop, paper (half a unit in the last printed digit), flash, and so on;
  - `divergences.csv` with policies `use-paper` / `skip-coolprop` / `investigate` / `resolved-upstream`;
  - a **family conformance suite**: FD consistency in the dimensional variables, part additivity, thread and batch invariance. Every family runs it and gets it for free.
  - Property tests (proptest, dev-only): Maxwell relations, gauge invariance, the mixture x=[1,0] equals the pure fluid, admissibility (cv > 0, κ_T > 0).
  - Differential tests: AD vs fast path, lanes vs scalar, batch vs scalar, N threads vs 1.
  - Benchmarks: criterion plus gungraun instruction-count gates.
- *Rationale:* map 10 §8; map 13 (free arbiters: IAPWS-95 ≤2.9e-9, Lemmon 2016 ≤4.3e-7); K17/K18.
- *Rejected:* trusting CoolProp's tests (map 10 R3/R4: assertions weaker than their names); widening tolerances to fit the oracle.

**D14 Licensing and attribution.**
- *Decision:* code is `MIT OR Apache-2.0`. CoolProp's MIT notice ships in `cprs-heos` and in `LICENSE-THIRD-PARTY`.
  - Every model and dataset carries a `Source` (BibTeX key, DOI, `DataTerms`). A CI rule rejects restricted terms in default features.
  - cargo-deny allow-list (R19); REUSE 3.3; cargo-about notices for wheels and WASM.
  - The NIST disclaimer is kept if superancillary code is ported.
  - No GPL code (SeaFreeze, BurnMan, outram-park-fork-coolprop). GSW-C is an oracle only.
  - Provenance to clear before redistribution: Ethanol–Water "from REFPROP 9.1 with permission", the DTU table, fastchebpure outputs, INCOMP data-sheet fits (map 09).
- *Rejected:* bundling NIST SRD or SESAME data.

**D15 Milestone order and first useful release.**
- *Decision:* v0.1 = M1-M10 of §6: all 136 HEOS pure fluids, all 19 pairs, transport and σ, reference states, batch + rayon, PropsSI compat, C ABI, WASM.
- Next comes M11, the cubic family, as the **Open/Closed gate**: its PR must show a zero-line diff in `cprs-core`. Then SIMD lanes, mixtures + GERG, IF97 + ice, INCOMP, humid air, PC-SAFT, Python.
- *Rationale:* "CoolProp pure fluids first"; the second family is proven early, before the traits ossify.
- *Rejected:* mixtures before the pure-fluid flash is green (map 10: defer mixture fixtures to P2).

**D16 Naming.**
- *Decision:* the placeholder is `cprs-*`. The final name is the user's call.
- Constraints (*inference*): the name must not imply official CoolProp endorsement, must be free on crates.io and PyPI at decision time, and must be usable as a C prefix (`cprs_` today).

**D17 Edition, MSRV, tooling, CI, lints, unsafe.**
- *Decision:*
  - Edition 2024 and `resolver = "3"`. MSRV 1.85 for core and family crates; 1.89 for `cprs-simd` (R3). Develop on 1.99.
  - Workspace lints: `unsafe_code = "forbid"`, overridden only in `cprs-capi` and `cprs-simd`, with `SAFETY:` comments; `missing_docs = "warn"`; `clippy::all`; core `disallowed-types`.
  - CI matrix:
    - tests on x86_64 Linux and Windows MSVC;
    - the full suite on `wasm32-wasip2` under wasmtime;
    - `wasm32-unknown-unknown` builds, baseline and `+simd128`;
    - an MSRV job, rustfmt, `clippy -D warnings`, cargo-deny, the zero-dependency guard (xtask), cargo-semver-checks before releases, cargo-shear.
- *Rejected:* nightly features in published crates (R4); a pinned `rust-toolchain.toml` (except in the bench job).

## 4. Walkthroughs

**(a) One PT → h call for Water.**
```rust
let water = cprs::Library::global().model("Water")?;                       // default family "HEOS"
let st = water.state(Input::PT(Pressure::new(101_325.0)?, Temperature::new(300.0)?),
                     &Composition::pure(), &FlashOptions::default())?;
let h = st.h_mass();                                                        // oracle: 112654.89965464505 J/kg
```
1. `LazyLock` builds `Library::standard()`: `HeosCatalog::new(Corrected)` allocates 136 empty `OnceLock` slots and decodes nothing.
2. `"Water"` has no `::`, so the default family applies. The catalog's `resolve` folds the name to "water" and binary-searches the generated index, giving a `ModelKey`.
3. Slot `get_or_init`: decode the Water section (~22 KiB). Power terms go into a `PowerBlock`; Gaussian and NonAnalytic terms go into their blocks; then the ideal terms, ancillaries, melting line and `HeosTransport`. The result is `HeosPure` → `Fluid` → `Arc<dyn ThermoModel>`. The superancillary stays raw.
4. `state()`: M comes from `ModelInfo`; `to_spec` normalises the basis (nothing to convert for PT); `PairSet` capability check; then `Fluid::flash` → `flash::helmholtz`, PT module:
   - limits: T ≥ T_min = 273.16 K;
   - first saturation call: the superancillary is decoded into its lazy cell;
   - p_sat(300 K) = 3536.8067523441227 Pa (oracle) < p, so the phase is `Liquid`;
   - ρ′ from the superancillary seeds a bracketed Newton on p(ρ), using `alpha(.., Parts::Residual)` within `[ρ_v, rho_max]`;
   - verify gate;
   - one `alpha(.., Parts::Both)` builds the `State`: ρ = 996.5569352651672 kg/m³ (oracle; `prop` class 1e-12).
5. `h_mass()` = r·T·(Λ01 + Λ10)/M + gauge (0 here).
6. Cost after warm-up: no allocation, about 6 indirect calls, no locks. Expected values are those of fixture M6.

**(b) 1,000,000 PH flashes over 3 fluids from 16 threads at once.**
```rust
let lib = cprs::Library::global();
let fluids = ["Water", "R134a", "CO2"].map(|f| lib.model(f).unwrap());     // 3 Arc clones, shared
std::thread::scope(|s| for k in 0..16 { let m = &fluids[k % 3]; s.spawn(move || {
    let req = BatchRequest { pair: Pair::HP, a: &h[k], b: &p[k], x: &Composition::pure(),
                             outputs: &[Prop::T, Prop::Dmass, Prop::Cpmass], options: &FlashOptions::default() };
    batch::run(&**m, &req, &mut out[k], ExecPolicy::Reference)              // 62,500 points per thread
})});
```
- **Cold start:**
  - The first touch of each slot races; `OnceLock` runs exactly one decode per fluid and the other threads wait only during that init (tens of µs, *estimate*).
  - The same holds for each superancillary cell.
  - After that, a read is one atomic load. No other shared write exists (asserted by `disallowed-types` and the Send/Sync consts).
- **Per point:**
  - `spec_at` validates the inputs (a NaN gives `Status::InvalidInput`).
  - The PH module asks the superancillary for (T_sat, ρ′, ρ″) at p and computes h′, h″ through `relations`.
  - Two-phase points: exact via the lever rule.
  - Single-phase points: Newton in (T, ρ) on (h, p), with the Jacobian from `relations::{dp_dt_rho, dp_drho_t, cp}` on `Derivs2`.
  - The resulting `State` writes its outputs into the columns. A failure gives that point's `Status` and NaN cells; the other points are unaffected.
- **Memory:** 3 models of about 22 KiB plus about 20 KiB of superancillary each, shared. Per thread: one 192-byte `State` on the stack plus the caller's buffers.
  - CoolProp needs one `AbstractState` per thread with 100-300 KiB deep copies (map 11), and its first use decodes every fluid (1.9 s, +67 MiB, map 14).
- **Determinism:** every point starts cold (K7), so results do not depend on the thread count or chunking. The sketch tests `Parallel{64}` against `Reference` bitwise.
  - With a single caller and the `rayon` feature, `ExecPolicy::Parallel{chunk: 4096}` gives the same bits. The library never spawns threads under `Reference`.
- **Throughput target:** ≤15 µs per single-phase PH (K §3.4, *estimate*), about 0.94 s wall-clock on 16 cores. M8 benchmarks confirm or re-plan.
- **Later:** PH stays scalar per point (branchy, K14). Lanes speed up the α evaluations inside lockstep ρ(T,p) solves through `alpha_batch`.

**(c) Adding a new EOS family (PC-SAFT), then a solid (ice Ih).** Core diff in both cases: **0 lines**.

*PC-SAFT*, new crate `cprs-pcsaft` (deps: `cprs-core`, `cprs-heos` for identity and the ideal gas):
1. `struct PcSaft { info: EosInfo, comps: Box<[Segment]>, kij: SparseKij, ideal: Box<[Arc<dyn HelmholtzModel>]> }`. Each `ideal` entry comes from `HeosCatalog::fluid(key)?.eos_shared()`, evaluated with `Parts::Ideal`. This fixes CoolProp's missing h/s/cp/w (map 06 P4) without a stale copy (C5).
2. Write once: `fn alphar<R: Real>(&self, t: R, rho: R, x: &[f64]) -> Result<R, EvalError>`. It covers hard chain, dispersion and (later) polar terms.
   - The association inner Newton returns `EvalError::InnerNoConvergence` instead of 100 silent iterations (map 06 P7).
   - A small-ζ series avoids the ρ → 0 cancellation (P6).
3. `impl HelmholtzModel`:
   - `alpha()` evaluates `alphar` on `Jet2` seeds with any reference (Λ is invariant), so there is no fake reducing state (C10).
   - `rho_max` comes from the packing-fraction limit.
   - `saturation` keeps its default `None`, so the generic flash iterates. This is the same path GERG pure fluids use, and it is stability-based, not CoolProp's failing supercritical PT (P2).
4. `PcSaftCatalog: Catalog` under `"PCSAFT"`. Names go through the HEOS resolver (one identity registry; C13). The parameters are its own (provenance check: map 06 open question).
5. The facade gets one feature-gated line: `b = b.with_catalog(Arc::new(PcSaftCatalog::new(heos.clone())))`.
6. Tests:
   - `cprs_verify::conformance::*`;
   - oracle 8.0.0 for p, residual h/s/g and fugacity only (map 06 §8 caveats);
   - teqp/FeOs and Gross–Sadowski tables as arbiters.

*Ice Ih*, new crate `cprs-iapws` (deps: `cprs-core`):
1. `struct IceIh { info: ModelInfo, g00: G00 }`, where `G00::{R10_2009 (default), R10_2006 (compat)}`. It holds the R10-06(2009) Gibbs function with its own small complex type.
2. `GibbsDerivs {g, g_t, g_p, g_tt, g_tp, g_pp}` and `to_derivs2(t, p, r)`, the same algebra as `gibbs_to_derivs2` in `open_closed.rs`:
   - Λ00 = (g−pv)/(rT), Λ01 = pv/(rT), Λ10 = (g−pv−T g_T)/(rT);
   - Λ20 = T(g_TT g_pp − g_Tp²)/(r g_pp), Λ11 = pv/(rT) + v g_Tp/(r g_pp), Λ02 = −v²/(g_pp rT) − 2pv/(rT).
3. `impl ThermoModel`:
   - PT is direct; PH and PS use a safeguarded 1-D Newton in T from core `solve` (analytic cp).
   - `pairs = {PT, PH, PS}`, `Phase::Solid`, `residual: None`.
   - `NativeGauge{aligned_with: "IAPWS-95"}`, from R10-06's "IAPWS-95" s0 (research §4).
   - `Gauged` reference states work unchanged.
4. Tests: R10-06 Table 6 values (paper class); oracle only via `HAProps_Aux` g/h of ice, with divergence row `Ice-g00` (map 07 C1: Δg = −1.136e-4 J/kg).
5. Melting and sublimation by g-equality with IAPWS-95 (R14-08 checks) belong to the deferred `Substance`. It is a new type in a new module that uses only `ThermoModel::flash(PT)` and `State::g_molar()`.

**(d) A CoolProp bug is found and the literature overrides CoolProp.** Example: Nitrogen's reducing density.
1. Symptom:
   - L2/L3 rows for N2 pass the oracle (`prop` class) under `DatasetId::CoolProp800`.
   - The paper-class row from the Span et al. 2000 check table fails by more than half a unit in the last digit.
   - Saturation densities sit 1.3e-7 off (map 10 §8.4).
2. Diagnosis: CoolProp 8.0.0 stores ρ_r = 11183.901464580624 while the paper has 11183.9 (map 12 §6.3, fixed upstream in 2acbbc82). It is a data defect, not an algorithm bug.
3. Fix:
   - A datagen `Corrected` overlay entry `{Nitrogen, reducing.rho, 11183.9, Source{Span-JPCRD-2000}, evidence: map 12 §6.3 / 2acbbc82}`.
   - The N2 superancillary is either refit from upstream (66859efb, with its provenance) or marked `SatAccuracy::Guess`, so the flash polishes it. The choice is recorded.
4. Register: `divergences.csv` gets `N2-rhor,Nitrogen,rhomolar_reducing,use-paper,Span-JPCRD-2000,...`.
   - Oracle rows for N2 run against `CoolProp800` (parity build); paper rows run against `Corrected` (the default). Both stay green.
   - When the oracle pin moves, the row becomes `resolved-upstream`.
5. Variants:
   - *Algorithm bug* (PR/SRK entropy, map 06 C1): no data change. A `skip-coolprop` row plus identity tests (∫cp/T dT, T(∂s/∂T)_p = cp).
   - *Paper wrong* (map 12 R11: four post-v8 transport papers print wrong equations): the paper's own check table with its own constants arbitrates (map 13), and the erratum form is stored with its provenance.
   - *Both legitimate* (ice g00 2006/2009, IF97 region-3 iteration): an explicit model option enum; literature by default, the CoolProp value as compat.
   - Never widen a tolerance to fit CoolProp, and never "fix" a paper value without a published erratum (map 10 §8.5).

## 5. CoolProp rot designed out

| Rot (evidence) | How this design prevents it |
|---|---|
| Model, state, cache and solver workspace fused; 6 friend classes write about 170 fields (map 12 R1) | Immutable `Arc` model, `Copy` `State`, stack scratch; `flash` is a pure function returning `Result<State>` |
| God interface: 181 virtuals, 136 `NotImplementedError`s; capability found by catching (map 12 R2, map 01 R4) | 4 small traits with `Option` defaults; declared `PairSet` capability checked before flashing (sketch test) |
| New transport form = 4 core edits; backend registry special cases (map 12 R13) | Closed forms inside `cprs-heos`, so a new form is one variant + one function; open `Catalog` + `LibraryBuilder` |
| Exceptions as control flow, `_HUGE` sentinels, swallowed failures (map 12 R3; map 03) | Typed `Result` everywhere, `Root` with residual-at-x, `Ambiguous{roots}`, per-point `Status` with NaN cells |
| 38 global config keys; library toggles config mid-computation (map 12 R4, map 01) | `FlashOptions` / model options passed by value; core reads no env (clippy-enforced cells) |
| Sticky `specify_phase` leaks across calls (map 03, map 01) | Phase hint is a per-call `FlashOptions` field; no mutable model or state exists |
| Hand-written derivatives per term, plus a second test-only implementation (map 12 R5; map 06 P5, V3) | Value-only generic formulas + `Jet2`; hand fast paths are tested against AD (sketch test) |
| Mixed reducing states: PR/SRK entropy off by 1.495× (map 06 C1, C2) | Only reducing-invariant `Derivs2` crosses model boundaries; reducing state is private to `HeosPure` |
| Gas constant: 9 spellings, 8 values (map 12 R7, map 06 C6) | `EosInfo.r` per model; `State` carries the model's own `r`; one named CODATA constant |
| Stale duplicated cubic dataset; X-SRK pseudo-fluids (map 06 C5, C9; map 15 X6) | Cubic and PC-SAFT borrow ideal gas and identity through `Fluid::eos_shared`; no name-suffix tricks |
| Fabricated metadata for bracketing (Tmin = 0.3·Tc…) (map 06 C10, G3) | Flash brackets with `HelmholtzModel::rho_max`; no invented limits |
| `set_reference_state` silent no-op except HEOS; six gauge mechanisms (map 15 X3, X7) | One `Gauged` wrapper for every `ThermoModel` (tested on a Gibbs solid); typed errors |
| Mixture α0 uses the wrong τ for component offsets (map 15 X12) | Gauge is applied outside the EOS; property test: mixture x=[1,0] equals the pure fluid under any gauge |
| Wet bulb mixes gauges; tables bake the gauge (map 15 X1, X2) | Composite models run sub-models in native gauge (injection); future surrogates keyed by model fingerprint + gauge |
| IF97 molar HS falls through; humid-air kT passes density as pressure (map 07 R1, H1) | Typed `Input` with basis-carrying quantities; exhaustive `match`, no fall-through |
| `fast_evaluate` clears the caller's state (map 07 R5, map 08) | Batch takes `&dyn ThermoModel`, caller-owned buffers, no model mutation |
| Humid air mixes 4 water formulations; `>` vs `>=` at 273.16 K (map 07 H3) | Future HA crate takes `Arc<dyn ThermoModel>` water, air and ice by injection; one boundary predicate |
| Ice: global free functions, 1e99 on PowerPC, superseded g00 (map 07 C1, C2) | Ice as a `ThermoModel` with `Phase::Solid`, a typed domain and a `G00` option (2009 default) |
| Eager import: 1.8-1.9 s, +67-75 MiB (map 14, map 09) | Per-fluid `OnceLock` slot, superancillary in a second cell, features for subsets |
| Per-state deep copies, 100-300 KiB; mutable term caches (map 11) | One shared model; the 192-byte `State` holds no `Arc` |
| Unknown JSON types silently accepted; asserts compiled out (map 09, map 10) | Closed enums at datagen; validation returns real errors; `Corrupt` `LoadError` |
| Hidden cross-model coupling (ECS) (map 12 R12) | ECS references explicit (`EcsReference`, `Weak` catalog), DAG checked at datagen |
| Locale-dependent parsing; enum renumbering breaks ABI (map 12 R18, map 01) | Rust `FromStr` is locale-free; string keys; only `Status` crosses the ABI, append-only |
| Q = 5 accepted; NaN Q segfault; below-Tmin negative pressure (map 12 §6.3, R9) | `Quality` newtype in [0, 1]; finiteness at the boundary; `DomainError::BelowTmin` |
| Two-phase cp/cv/transport silently returned (map 02, map 10 §8.4) | `Err(PropError::TwoPhaseUndefined)` |
| TTSE/BICUBIC crash or garbage bugs (map 08 R1-R9) | Not ported. A future surrogate is just another `ThermoModel` wrapping an exact one |

## 6. First TDD milestones (each always green; oracle = CoolProp 8.0.0 `ae81610e` unless a paper is named)

| # | Scope | Oracle / check values | Green gate |
|---|---|---|---|
| M0 | This workspace: traits, `Jet2`, `PowerBlock`, `Fluid`, `Gauged`, batch, CI skeleton | 50-digit hand values for 2 power terms; `Jet2` = fast path ≤1e-14; 16 threads bitwise | the 5 cargo commands on host/wasm/msvc (done: 18 tests) |
| M1 | `cprs-verify` reader and classes; `xtask oracle` (uv, CoolProp==8.0.0, scrubbed env, 38 keys pinned, fresh state per case); L1 fixtures for Water, N2, CO2, R134a, Propane by block isolation | IAPWS-95 Table 6 at 500 K, 838.025 kg/m³: φr = −3.42693206 (oracle −3.4269320568155854) | comparator self-tests; fixture headers carry the SHA and config |
| M2 | All 7 residual and 10 ideal term kinds; NonAnalytic with explicit δ=1 behaviour | Table 6: φr_δ = −0.364366650, φr_τ = −5.81403435, φ0_τ = 9.04611106 (oracle reproduces ≤2.9e-9, map 13); `term` class vs oracle | AD = fast path for every kind (term class) |
| M3 | `relations` + (T, ρ) states, imposed phase, L2; conformance suite; Maxwell/identity property tests | IAPWS-95 Table 7 at 300 K, 996.556 kg/m³: p = 0.0992418352 MPa, cv = 4.13018112 kJ/(kg K), w = 1501.51914 m/s, s = 0.393062643 kJ/(kg K); Lemmon 2016 Table 7 (≤4.3e-7) | `prop` 1e-12 vs oracle; `paper` class vs tables |
| M4 | Datagen → LE binary + index; `HeosCatalog` lazy slots; L0 for 136 fluids | coefficients bit-identical to JSON (`exact`); FNV `source_eos_hash` recomputes (130/130) | loading Water decodes one blob; a 16-thread first-touch race yields one decode |
| M5 | Superancillary, ancillaries, QT/PQ; pseudo-pure `Definition` path | Water p_sat(300 K) = 3536.8067523441227 Pa; `sa-coeff` 1e-14 vs CoolProp superancillary; `sa-fit` vs fastchebpure | no extrapolation below T_triple without opt-in |
| M6 | PT and DT flashes, phase determination, domain errors, `solve` toolbox | Water 300 K / 101325 Pa: h = 112654.89965464505 J/kg, ρ = 996.5569352651672 kg/m³; 40×40 (log p, T) round trips (`flash` 1e-9) | Water DT at 250 K errors (oracle gives p = −5.928 Pa: divergence row) |
| M7 | PH, PS, PU, HS, D+X, T+X (`Ambiguous`), Q+X incl. QS/HQ; `Gauged` IIR/ASHRAE/NBP | round trips; reference states for n-Propane, R134a, R124 at 1e-8 (map 01 §8); 19 × basis matrix vs `PairSet` | T+X compressed liquid arbitrated by EOS root enumeration, not the oracle |
| M8 | Batch executor, `rayon` feature, benches | batch = scalar, `Parallel{chunk}` = `Reference`, N threads × {same, different} fluids = sequential (bitwise); criterion baselines | `dyn` overhead ≤2% vs a monomorphised flash; RSS for 1/3/136 fluids recorded |
| M9 | σ, staged η/λ (stage by stage), IAPWS water transport, ECS DAG | paper/IAPWS tables first; `tr-direct` 1e-12 / `tr-ecs` 1e-8 vs oracle | two-phase transport is an error (divergence row) |
| M10 → **v0.1** | `Library`, `compat::props_si` parity corpus, C ABI (cbindgen, `catch_unwind` test), WASM baseline + `simd128`, Windows MSVC tests | PropsSI corpus from the oracle (including PhaseSI strings); full suite on `wasm32-wasip2` under wasmtime | Linux, Windows, wasip2 green; zero-dependency guard |

After v0.1:
- **M11, cubic:** zero-line core diff (the Open/Closed gate). Checks: the 8.0.0 oracle for αr/p/h/cp/w/fugacity with c = 0; entropy vs ∫cp/T; Zc = 1/3 (SRK) and 0.30740 (PR) at the model's own critical point.
- **M12:** SIMD lanes (≥2.5× gate).
- **M13:** mixtures + GERG (1,174 teqp vectors).
- **M14:** IF97 + ice.
- **M15:** INCOMP.
- **M16:** humid air.
- **M17:** PC-SAFT; Python.

## 7. Risks and what this proposal deliberately defers

| Risk | Mitigation |
|---|---|
| The trait shape is wrong for a family we have not ported | M11 (cubic) is gated on a zero core diff. `open_closed.rs` already exercises a value-only EOS and a Gibbs solid. Pre-1.0 semver; `#[non_exhaustive]` options |
| `dyn` overhead on hot paths | One call per α or per batch; M8 gate ≤2%; the generic-flash escape hatch is a local change |
| AD route slower than hand paths for non-power terms | Fast paths are added per block after benchmarks, side by side and differential-tested; correctness never waits for them |
| Λ-scaled exchange vs CoolProp rounding (research §4 warning) | M2/M3 gates on 5 fluids; the `term` and `prop` classes absorb it. A drift beyond them is a measurement result that re-plans the bundle |
| Gibbs→Λ transform loses precision when g_pp is tiny (*inference*) | Exact algebra; checked against R10-06 tables at M14. Fallback: a `relations::gibbs` sibling (research R2) |
| Crate sprawl from "one family = one crate" | The crate rule in §1; GERG, pseudo-pure and alternate EOS are data, not crates |
| `get_or_try_init` is unstable, so load errors are cached | Embedded data is validated at datagen; a cached error is deterministic. Runtime catalogs return fresh errors |
| Compat mixture-string cache needs a lock | Lives in the facade only; native users hold `Arc` handles |

**Deferred deliberately:**
- Plotting, isolines and cycle helpers (CoolPropPlot).
- Tabular TTSE/BICUBIC/SBTL. Do not port TTSE/BICUBIC (map 08); a surrogate is a future `ThermoModel` decorator.
- Mixtures (the contract is ready) and Qmass for mixtures.
- `Substance` / min-Gibbs selector, RegionAtlas, `Domain` with holes, the `correlation` module, solid EOS, tensors, plasma, CALPHAD, the `Formation` gauge.
- Marker-type property keys, the expression DSL (map 05 U11), ePC-SAFT, VTPR + UNIFAC.
- REFPROP: oracle only, in a separate process; never a backend (map 07).
- Python, GPU, WASM threads.

## 8. Self-assessment against the rubric

| Criterion (weight) | Score /5 | Honest note |
|---|---|---|
| Modularity and extensibility (3) | 5 | The emphasis. A Gibbs solid and a value-only EOS are proven out-of-tree; reference states, batch and conformance come free to any family |
| Simplicity (3) | 3 | **Weakest.** 4 traits + `Real`/`Jet2` + `Fluid`/`Gauged` wrappers means more concepts than a single closed-enum design; a reader follows `dyn` layers (Library → Catalog → Fluid → HelmholtzModel → blocks). Mitigation: each trait has ≤6 methods with defaults, and v1 has 5 published crates |
| Concurrency and memory (3) | 4.5 | Lock-free reads, per-fluid lazy slots, 192-byte `Copy` `State`, bitwise thread tests. The compat mixture cache is the one lock |
| Performance (3) | 4 | The scalar fast path follows map 02; lanes have a clean seam (`alpha_batch`, `ExecPolicy`). `dyn` and AD-route costs are real but gated; no benchmarks yet |
| Correctness and TDD-ability (3) | 4.5 | Paper > oracle arbitration, two datasets, divergence register, conformance kit, always-green milestones with concrete check values |
| Idiomatic Rust, DRY/SOLID, minimal deps, strong types (3) | 4.5 | Zero third-party deps in published crates, typed inputs and errors, Open/Closed by traits. `Spec`/`Input` duplicate 19 arms (deliberate edge/kernel split) |
| Rot elimination (2) | 4.5 | 26 rot items mapped to mechanisms (§5); gauge and reference-state rot removed for all families, not just HEOS |
| Cross-platform (2) | 4.5 | The sketch checks host, wasm32-unknown-unknown and MSVC; wasip2 tests and the libm determinism hash are planned, not yet run |
| Migration path (1) | 3.5 | PropsSI compat and C ABI in v0.1; a drop-in CoolPropLib.h only behind `coolprop-compat`; no Python until after v0.1 |
