# Proposal A: Lean

Type sketch (compiles; 5 gates green, see §8): `<session scratchpad>/sketch-lean`
(6 workspace members, ~4.2k lines, 20 passing tests, zero third-party dependencies).
Citations: "map NN §X / R#" = `docs/coolprop-map/NN-*.md`; "dependencies R#", "kernel-performance K#", "prior-art P#",
"materials S#/R#" = `docs/research/*.md`. "Oracle (observed)" = re-run by me on CoolProp 8.0.0 (ae81610e) for this document.

## 0. Thesis

One std-only core crate holds the whole kernel and API: one concrete model type (`Fluid`), one extension trait
(`Residual`), one derivative mechanism (truncated-Taylor jets), one value type (`State`) and one batch function.
There are zero public generic parameters. Each future direction gets a seam that is one item wide and already exercised
by a test: new families implement `Residual`, new states of matter call `State::from_total`, faster execution overrides
`Residual::eval_batch`, and mixtures arrive as a crate. Everything else waits for a benchmark or a second user.

## 1. Layers and crate layout

```text
  Rust users ─────────────────────────────┐        C / Excel / Fortran        browsers (JS)      WASI servers
                                           │              │                         │                │
                                           │        cprs-capi (C ABI,         cprs-wasm (wasm-    (no facade: cprs
                                           │        legacy PropsSI shim)      bindgen, 2 builds)   itself on wasip2)
                                           ▼              ▼                         ▼                │
 ┌─ cprs (std only, unsafe forbidden, 0 third-party deps; arrows = "uses") ───────────────────────────┴──
 │  compat (PropsSI strings) ─► batch (slices, Status, Exec) ─► flash (pure fns, FlashOptions, verify) ─► roots
 │       │                                                            │
 │       ▼                                                            ▼
 │  registry (alias index, OnceLock per fluid) ─► data (blob reader) ─► fluid (Arc<Core>: info, limits, critical)
 │                                                                      │   ├─► sat (SA Chebyshev, melting)
 │                                                                      │   ├─► transport (public API only)
 │                                                                      ▼   ▼
 │  state (Copy, total bundle) ─► relations (family-neutral)       eos (Residual trait, SoA blocks, IdealGas)
 │                                      │                                │
 │                                      ▼                                ▼
 │                                derivs (A_xy) ◄──────────────── jet (Jet, Jet2) ─► math (exp/ln/powi: one file)
 └───────────────────────────────────────────┬─────────────────────────────────────────────────────────────────
                                             │ optional feature `bundled` (default)
                      cprs-data: generated blobs + alias index, no code (CoolProp MIT notice)
 dev/tools (never shipped): cprs-verify (fixtures, tolerance classes, divergence register) · xtask (datagen, oracle, deps guard)
```

Module dependencies point downwards, with two deliberate convenience edges: `Fluid::state`/`Fluid::flash` delegate to
`flash`, and `State` is built from a `&Fluid` evaluation. Both are inside one crate and carry no state. Modules are not
crates: a module becomes a crate only when it gets a second consumer or must not ship with the core (kernel-performance
§3.1: "start as modules of one crate").

| Crate | Responsibility | Depends on | Features |
|---|---|---|---|
| `cprs` | Everything that computes: jets, terms, ideal gas, `Residual`, registry, decoding, superancillary, roots, flash, `State`, relations, transport, batch, `compat` strings | `cprs-data` (optional) | `bundled` (default), `rayon`, `libm`, `json` (all additive, off by default except `bundled`; only `libm` changes bits, and that is documented) |
| `cprs-data` | Generated per-fluid binary blobs (`include_bytes!`) + 556-key alias table + `DATASET` id + CoolProp MIT NOTICE. No logic. | none | `all` (default) = one feature per fluid (generated; e.g. `water`, `r134a`) |
| `cprs-capi` | `cp_*` C ABI (cdylib/staticlib, cbindgen CLI header, cargo-c), pinned status codes, FP guard; opt-in CoolPropLib.h shim | `cprs` | `coolproplib-compat` (M9) |
| `cprs-wasm` | Browser facade (wasm-bindgen; baseline and `+simd128` builds; fetch-only-needed fluids) | `cprs` | none (subset via `cprs-data` features) |
| `cprs-verify` (unpublished) | Zero-dependency harness: CSV fixtures with `#` headers, `Provenance`, tolerance `Class`, `Divergence` register, later criterion benches | none | none |
| `xtask` (unpublished) | `datagen` (JSON to blobs), `oracle` (uv + CoolProp 8.0.0), `deps` (zero-dependency guard) | `cprs[json]`, serde_json (T4) | none |

Later crates are created only when their trigger fires: `cprs-mix` (first mixture milestone), `cprs-py` (a Python user
asks; pyo3 abi3), `cprs-simd` (only if the M10 auto-vectorization gate fails), and a solids crate (first Gibbs solid).
Plotting, isolines and cycle helpers (CoolPropPlot) are deferred and not mapped (00-inputs-digest, coverage note).

**Does each layer earn its place?** (the user: "Layers will be important but we must not over engineer this")

| Layer (module) | Earns its place because | Not built (and why) |
|---|---|---|
| `jet` + `derivs` | Replaces about 1.8k lines of hand derivatives and plumbing (map 02 §9), and is the only reason new families need no derivative code | A generic numeric trait (no second numeric type yet) |
| `eos` (`Residual`, blocks, `IdealGas`) | The only open/closed seam for families; SoA blocks are the SIMD unit | A per-term trait, a family enum, a `HelmholtzModel` super-trait |
| `relations` | Written once for every potential, including Gibbs solids after transform (materials S1) | A `relations::gibbs` copy (it will be a converter, not formulas) |
| `fluid` + `registry` | Immutability, laziness and sharing live in exactly these two types (D7/D8) | `Substance`/`PhaseModel` (materials R1 trigger), a model cache separate from the registry |
| `sat` | SA evaluation is the saturation source for 130 of 136 fluids (map 08 §9) and brackets every density solve | Runtime SA builders (datagen precomputes, map 03 §9) |
| `flash` + `roots` | The single place where selection happens (materials S10); one `verify()` gate | `Strategy` objects, a solver framework, a workspace type (stack scratch suffices) |
| `state` | The only value users hold; private fields keep it growable | Typestate per phase, lazy caches |
| `batch` | Defines the no-allocation, per-cell-status contract that C, WASM and Python all need (map 11 U4) | A second batch API per facade |
| `transport` | Its form is set by data (closed enums); it is written against the public API to prove the seam (S5) | The expression DSL (map 05 U11, P2) |
| `compat` | One string grammar shared by three facades (DRY) | Per-facade parsers |

## 2. Core types and traits (excerpts, identical to the sketch)

```rust
// derivs.rs: the ONE derivative currency. A_xy = τ^x δ^y ∂^(x+y)α/∂τ^x∂δ^y = Λ_xy in (1/T, ρ):
// invariant to reducing constants, finite at ρ = 0 (map 06 §9; prior-art P2).
pub enum Order { Two, Three, Four }                  // `Want` mask of map 02 §9
pub struct Derivs { a: [f64; 15] }                   // get(x, y); ZERO; IDEAL_DELTA (ρ-part of every α0)

// eos.rs: THE extension point (D3). Object-safe; Send + Sync is a supertrait (D8).
pub trait Residual: Send + Sync + core::fmt::Debug {
    fn gas_constant(&self) -> f64;                   // explicit model data (map 12 R7: 8 distinct R values)
    fn source(&self) -> &Source;                     // provenance on every model (materials S8)
    fn eval(&self, t: f64, rho: f64, order: Order) -> Derivs;           // reducing-invariant α^r bundle
    fn eval_batch(&self, t: &[f64], rho: &[f64], order: Order, out: &mut [Derivs]) { /* scalar loop */ }
}
pub(crate) enum Block { Power(PowerBlock), Gaussian(GaussianBlock), NonAnalytic(NonAnalyticBlock) /* +3 in M2 */ }
pub enum IdealTerm { Lead{..}, LogTau{..}, TauLogTau{..}, Power{..}, PlanckEinstein{..} } // 10 JSON kinds normalised

// jet.rs: how derivatives are obtained (D2). Crate-private univariate scaled jets for separable terms;
// public bivariate Jet2 for non-separable terms and new families (ordinary arithmetic -> all 15 A_xy).
pub struct Jet2 { c: [f64; 15] }   // var_x, var_y, constant, + - * , exp, ln, powf, powi, derivs(x0, y0)

// fluid.rs: one immutable package per fluid (materials S2/S3); cloning = one Arc increment.
pub struct Fluid { core: Arc<Core>, offset: EnergyOffset }
impl Fluid {
    pub fn builder(info: FluidInfo, molar_mass: f64) -> FluidBuilder;   // user fluids, new families, decoder
    pub fn state(&self, input: Input) -> Result<State, Error>;          // flash with default options
    pub fn flash(&self, input: Input, opts: &FlashOptions) -> Result<State, Error>;
    pub fn alpha(&self, t: f64, rho: f64, order: Order) -> Alpha;       // raw, unchecked (metastable OK)
    pub fn supports(&self, pair: Pair) -> bool;                         // declared capability (map 01 R21)
    pub fn with_offset(&self, o: EnergyOffset) -> Fluid;                // gauge per handle, O(1)
    pub fn with_reference(&self, r: ReferenceState) -> Result<Fluid, Error>;
    pub fn ideal_gas(&self) -> &IdealGas;                               // cubic / PC-SAFT borrow it
}
pub struct Registry { /* entries: name, bytes, OnceLock<Result<Fluid, Error>>; sorted case-folded index */ }
impl Registry {
    pub fn builtin() -> &'static Registry;                               // feature `bundled`
    pub fn from_owned(blobs: Vec<(String, Vec<u8>)>) -> Result<Registry, Error>; // WASM fetch, files
    pub fn with_fluid(self, f: Fluid) -> Registry;                       // new value, never global mutation
    pub fn get(&self, name_alias_or_cas: &str) -> Result<Fluid, Error>;  // lazy decode, once
}

// input.rs: 19 physical pairs; the basis lives in the quantity; ONE validation gate `to_molar`.
pub struct Temperature(pub f64);  pub struct Pressure(pub f64);  pub struct Quality(pub f64);
pub enum Density { Molar(f64), Mass(f64) }           // same for Enthalpy, Entropy, InternalEnergy
pub enum Input { PT(Pressure, Temperature), DT(Density, Temperature), HP(Enthalpy, Pressure), /* … 19 */ }

// state.rs: Copy value (≤ 256 B, asserted), private fields, made by flash or `from_total`.
impl State {
    pub fn from_total(t: f64, rho: f64, r: f64, m: f64, phase: Phase, total: &Derivs) -> State; // D10 seam
    pub fn t(&self) -> f64;  pub fn p(&self) -> f64;  pub fn phase(&self) -> Phase;  pub fn path(&self) -> Path;
    pub fn rho(&self, b: Basis) -> f64;  pub fn h(&self, b: Basis) -> f64;  /* s, u, g */
    pub fn cp(&self, b: Basis) -> Result<f64, Error>;    // Err(TwoPhase) in the dome; cv, w likewise
    pub fn get(&self, prop: Prop) -> Result<f64, Error>; // dynamic path (batch, strings, C, JS)
}

// flash.rs: pure function; options are a Copy value; no globals anywhere.
pub fn flash(fluid: &Fluid, input: Input, opts: &FlashOptions) -> Result<State, Error>;
FlashOptions::new().with_phase(Phase::Gas).with_roots(RootPolicy::Nearest(t)).without_limit_checks().with_guess(g)

// batch.rs: SoA in, caller-owned SoA out, one Status per cell, no allocation.
pub enum Exec { Auto, Reference, Parallel { chunk: usize } }
pub struct Request<'a> { pub kind: InputKind, pub x: &'a [f64], pub y: &'a [f64],
                         pub outputs: &'a [Prop], pub flash: FlashOptions, pub exec: Exec }
pub fn evaluate(f: &Fluid, req: &Request<'_>, out: &mut [f64], status: &mut [Status]) -> Result<(), Error>;

// lib.rs: D8 by construction. Adding a Cell, Rc or cache to any of these breaks the build.
const _: () = { const fn shared<T: Send + Sync + 'static>() {}
    shared::<Fluid>(); shared::<Registry>(); shared::<State>(); shared::<Arc<dyn Residual>>();
    shared::<FlashOptions>(); shared::<Error>(); assert!(size_of::<State>() <= 256); };
```

Concept count for a user: `Registry`, `Fluid`, `Input` (with 7 quantity types and `Basis`), `FlashOptions`, `State`, `Phase`,
`Prop`, `Error`. An implementor also sees `Residual`, `Derivs`/`Order` and `Jet2`.

Where each piece lives in the sketch (`crates/cprs/src/`). Bodies are complete unless marked `todo!`:

| File | Complete | `todo!` (milestone) |
|---|---|---|
| `jet.rs`, `derivs.rs`, `math.rs`, `relations.rs` | All of it | — |
| `eos.rs` | `Residual`, `Multiparameter::eval` + `eval_batch`, Power and Gaussian blocks, `IdealGas` (Lead, LogTau, TauLogTau, Power) | NonAnalytic (M2), PlanckEinstein (M3) |
| `fluid.rs`, `registry.rs`, `data.rs` | Builder, accessors, lazy registry, alias index, blob reader | `from_owned` (M3), SA/ancillary sections (M5), named references (M6) |
| `input.rs`, `state.rs`, `props.rs`, `error.rs` | All of it | — |
| `flash.rs`, `roots.rs`, `sat.rs` | DT, PT, QT, PQ, classification, `verify`, rtsafe, Clenshaw | TOMS748 and `T(ln p)` (M5), P+X/T+X/DP (M6), D+X/HS/SU/Q-pairs/melting (M7) |
| `batch.rs`, `compat.rs` | All of it (Reference executor) | Transport keys (M8) |
| `transport.rs` | Types and guards | Correlations (M8) |

## 3. Decisions D1-D17

| # | Decision | Rationale (evidence) | Rejected alternatives |
|---|---|---|---|
| D1 | 4 shipped crates (`cprs`, `cprs-data`, `cprs-capi`, `cprs-wasm`) + 2 unpublished (`cprs-verify`, `xtask`). Features are additive and few (table §1). | `cprs-data` earns its place: data churn (corrections overlay) is versioned apart from the API, the CoolProp MIT notice has a clean boundary (map 09 §9 licensing), and WASM users can skip it and fetch blobs. Facades must not leak deps into the core (dependencies T2). FeOs and teqp tame size with "few crates, features" (prior-art P11). | Crate per layer (eos/flash/registry/state crates: crate boundaries without a second consumer, so semver friction for nothing); a monolith with data inside (no data/API version split, 3 MB for blob-fetching users); a `verify` folded into `tests/` (would block reuse by capi parity tests and the future `cprs-mix`). |
| D2 | `f64` only, everywhere. Derivatives come from **truncated-Taylor jets**: separable terms (99.8 % of default terms, map 02 §9) use univariate scaled jets `J_k = x^k f^(k)/f` plus one outer product, with **one `exp` per term** (CoolProp's B-factor recurrence, generalised). Non-separable terms and new families use bivariate `Jet2`. `Jet2` is also the in-house AD oracle in tests (`power_and_gaussian_jets_match_bivariate_taylor`). Max order 4 with an `Order` mask. | Map 12 R5: CoolProp hand-codes 20 outputs per term, keeps a test-only multicomplex copy and ships wrong CP0PolyT / XN_DEPENDENT derivatives. Jets mean no per-term derivative formula exists anywhere. The fixed order 4 follows map 02 §9 (no stable const-generic array sizing). Fully implemented and tested in the sketch: `PowerBlock` against hand-computed δe^-δ values and closed forms, and Power/Gaussian jets against `Jet2` at 1e-13. | Hand-derived `all()` port (prior-art P2: bit-closer to the oracle, but it is the rot R5 itself); `num-dual` in core (5 breaking releases in 20 months, cannot carry SIMD lanes, prior-art §4); a generic `Real` trait now (no second numeric type exists in v1; it would be a crate-private refactor of two files later, triggered only by D9's gate); `std::autodiff` (nightly, dependencies R4). |
| D3 | Families are **open**: one object-safe trait `Residual`, stored as `Box<dyn Residual>` in `Fluid`. Term kinds within the multiparameter family are a **closed enum of SoA blocks** with `match` dispatch. Ideal-gas kinds are a closed public enum that every family shares. | A closed term set is what FeOs, teqp and CoolProp's data use: 7 residual + 10 ideal kinds (prior-art P1). SoA blocks are SIMD-ready. One vtable call per evaluation is about 1 ns against about 300 ns of work (inference), and batches amortise it through `eval_batch`. A new family needs no core edit: proven by `tests/new_family.rs`, where van der Waals is written in `Jet2`, then built, flashed, batched and shared by 16 threads from outside the crate. | Trait object per term (blocks SoA and inlining); closed enum of families (a core edit per family, rot R13 map 12); generic `Fluid<M: Model>` (public generic parameters, monomorphisation per family, harder FFI). |
| D4 | Composition is **not** in the core contract, and the pure-fluid path is the only path in v1. Mixtures arrive in `cprs-mix`: a `Mixture` holds the `Fluid`s of its components (`Arc`) and a composition trimmed at build time (map 04: zero fractions trimmed, not guarded). At fixed z it **implements `Residual`**, so relations, `State` and single-phase flashes are reused. Its ideal part needs ONE additive core constructor, `IdealGas::weighted(&[(x_i, &IdealGas_i)])`, which evaluates each component in its own reducing variables (map 04 §9, map 15 §9). Composition derivatives use a mole-number jet inside `cprs-mix` (map 04: "one mole-number AD path"). VLE and stability (enumerate every density root, then Gibbs selection, map 12 R19) live there. A two-phase result is `MixtureState { beta, liquid: State, vapour: State }`. | Map 04 §9: about 100 pure-vs-mixture branch sites and 6 fluid copies per pure state are what the pure path must not pay. Materials §4 warns that composition is not always mole fractions, so a universal `x: Vec<f64>` must stay out of the `State` contract. | `x: &[f64]` in every `Residual::eval` (pays on 136 pure fluids); composition in `State` (breaks `Copy`, wrong for INCOMP and humid air); mixtures inside `cprs` behind a feature (code mass on every build; the seam should be proven from outside). |
| D5 | `State` is a `Copy` value (≤ 256 B, compile-time assert) with private fields. Each phase point holds the **total** order-2 bundle (A00, A10, A01, A20, A11, A02) plus residual A00, A10, A01 (for ln φ). A two-phase state adds Q and the vapour point; every state also holds R, M, the gauge, `Phase` and `Path`. The bundle is computed eagerly at the end of the flash; higher orders come from `Fluid::alpha`. Inputs: `Input` has 19 pairs, and a quantity newtype carries its basis (`Density::Mass(..)`). ONE gate, `Input::to_molar`, enforces finite values, T, p and ρ > 0, and Q in [0, 1], and converts basis and gauge for every entry point. Outputs: SI `f64` getters taking `Basis`; `Result` where two-phase makes them undefined; `Prop` for dynamic paths. | Map 01 §9: a single Input enum with the basis in the quantity removes R2/R3/R4 by construction, and a typed `Prop` table with one exhaustive match removes R5. Map 01 §10 / kernel-performance §5 Q7: eager order 2, no cell. The total bundle is family-neutral (materials S1), so a Gibbs solid fills the same six numbers; `tests/gibbs_seam.rs` checks this at 1e-12. | Newtypes with validating constructors (`Temperature::new -> Result`): a second gate, because batch, strings and C carry raw f64 anyway (DRY); kept as a conscious trade-off (§7). `OnceCell` caches in `State` (interior mutability, no `Copy`, rot R11 map 01); `Arc` in `State` (shared refcount writes, kernel-performance §4); `uom` in core (dependencies R11). |
| D6 | `flash(&Fluid, Input, &FlashOptions) -> Result<State, Error>` is a pure function. Options are `Copy` (phase hint, `RootPolicy`, limit check, guess) and per call. There is one private function per pair family (dt, pt, px, tx, dp, dx, hs, su, qx); fallbacks are ordered `?`/`or_else` chains. **One acceptance gate, `verify()`**: the residual at the returned point and dp/dρ > 0 unless a phase was imposed. Solvers return `Root{x, f}` evaluated at `x` and raise `NoConvergence` on exhaustion. Non-unique pairs return `MultipleSolutions { roots }`. Phase classification is defined once, on the numerical critical point. The strategy that ran is recorded in `State::path()`. | Map 03 §6: sticky `specify_phase`, the "x_{n+1} with the state at x_n" mismatch, unchecked TOMS748 `max_iter`, 6.1 % ST error from absolute tolerance, T+X silently two-phase (oracle observed: Water h(300 K, 10 MPa) gives Q = 0.00376 at 3536.8 Pa). Map 12 R3: 237 `catch (...)`. Map 01 R26: phase labels differ per backend. | A `Strategy` trait with registered strategy objects (map 03 §9 sketch: the order per pair is code, not configuration; YAGNI); flash methods on a mutable state object (rot R1 map 12); global or env config (map 01 R15/R16). |
| D7 | Pinned v8.0.0 JSON plus `corrections.csv` go through `xtask datagen`, which uses ONE strict serde mirror (`cprs::json`, `deny_unknown_fields`, a logged Chlorine duplicate-key waiver, every JSON quirk resolved: "0 means absent", GaoB η sign, integral-float exponents, cp0 T_c = T_r; map 02 §9, map 09 §9). Datagen also precomputes SA extrema, the `T(ln p)` inverse and the caloric curves (map 03 §6). It writes one versioned LE blob per fluid (about 22 KiB; finite-checked reader, no deps) and the alias table into `cprs-data`. The registry decodes lazily, one `OnceLock` per fluid. Runtime blobs come through `Registry::from_owned`, user fluids through `FluidBuilder` + `with_fluid` (a new value). ECS references are decoded first and held as `Fluid` inside the transport model (an acyclic graph checked by datagen). | Map 09 §9 (D1-D6): the binary format beats CBOR/JSON, and compression gains only 13-19 %. Map 14 §9: CoolProp decodes every fluid on first use (about 1.9 s, +67 MiB). Map 11 F13: 37 MiB retained. Map 12 R10: stale derived data. Map 05 R7: ECS references are built by name on first evaluation. | Runtime JSON parsing by default; CBOR, `bincode` (RUSTSEC-2025-0141) or `rkyv` (dependencies R7); `build.rs` codegen (dependencies R6); a separate lazy cell for the SA (decoding is a memcpy once extrema are precomputed; added privately only if first-use latency measures badly); compression (dependencies R8). |
| D8 | `Fluid` = `Arc<Core>`, immutable, `Send + Sync` asserted. `Registry::builtin()` is a `LazyLock` static of per-fluid `OnceLock`s: the first concurrent request for a fluid decodes it once; different fluids never contend. Hot paths take `&Fluid`: no locks, no refcount traffic, no shared writes, scratch on the stack. The batch API is `batch::evaluate` (SoA, per-cell `Status`, no allocation). `Exec::Parallel { chunk }` runs rayon over fixed chunks, so results never depend on the thread count. Warm starts are opt-in (`Guess`); the cold-start default is bitwise reproducible. | kernel-performance K1-K7 (static `OnceLock` registry, no `Arc` in `State`, rayon fixed chunks, warm starts opt-in); map 11 U1/U2/U4. Verified in the sketch: `sixteen_threads_share_one_fluid_bitwise`. | `RwLock<HashMap>` lookup per call (a shared write per read, kernel-performance §4); `thread_local` backends (map 11 U20); per-thread model copies (map 11 F11); core spawning threads (breaks `wasm32-unknown-unknown`, dependencies §4). |
| D9 | "What is computed" is per-term and per-state scalar functions (`PowerBlock::term`, `relations::*`, `clenshaw3`). "How it is executed" is loop shape: scalar `eval`; chunked `eval_batch` with terms outer, **states inner**, 64-state stack chunks, **bitwise equal** to scalar (tested: `batch_is_bitwise_equal_to_scalar`); rayon across points. Dispatch: features only add capability; the executor is a runtime `Exec` value; WASM simd128 is a second compile-time build. The upgrade path is benchmark-gated (M10): (1) an inlinable in-house `exp`/`ln` in `math.rs` (one file) lets the inner loop auto-vectorize; gate at ≥ 2.5× on AVX2 for α^r batches. (2) Only if (1) fails: a crate-private `Real` trait with `[f64; 4]` lanes (K8) or `cprs-simd` on fearless_simd with runtime dispatch (dependencies R13), overriding `eval_batch` only. Qualifies: α^r/α0 sums, relations, SA Clenshaw (3 curves in lockstep), later lockstep ρ(T,p) with masks. Never: the HS cascade, nested P+X, the D+X interval method, VLE/mixture flashes (scalar per point, parallel across points). | kernel-performance K8, K9, K13-K15, §3.2 phase gates; dependencies R13 step order; map 03 §7: branchy flashes stay scalar. | Explicit SIMD now (no benchmark yet; `std::simd` is nightly, `wide` has no runtime dispatch); across-terms SIMD first (changes summation order and so loses bit identity, K9); a GPU backend (K16). |
| D10 | Build only these seams: (1) `Derivs` + family-neutral `relations` on the total bundle (S1); (2) a public `State::from_total`, so any potential, including a Gibbs solid after its algebraic second-order Legendre transform, produces states (tested); (3) `FluidInfo` / `Residual`+`Source` / `Fluid` kept apart (S2); (4) `EnergyOffset` per handle (S3); (5) `DomainError::BelowMeltingLine` and private-field `Limits` (S4); (6) `#[non_exhaustive]` on `Phase`, `Prop`, `Error`, `IdealTerm` and `Path` (S9); (7) transport attached per fluid and written against the public API (S5/S7). Deferred with triggers: `PhaseModel`/`Substance`/min-Gibbs selection (when ice must coexist with water); a `relations::gibbs` converter (first Gibbs model; prototype in `tests/gibbs_seam.rs`); `Domain` with holes (IF97); a `correlation` module (INCOMP); `Phase::Solid` (additive variant). | materials R1 ("one implementation is a guess"), §3.2 deferral triggers, S1-S10 cost "close to nothing". | `Material`/`PhaseModel` traits now (Cantera deprecated its speculative lattice phases, materials §4); a universal `Vec<f64>` composition or tensor-valued properties (materials §4). |
| D11 | Rust API = `cprs`. A PropsSI-style string API = `cprs::compat`, in core because all three facades reuse it (one strict grammar, `Result` instead of +inf, no echo shortcut). C ABI = `cprs-capi`: `cp_*` exports with generational `u64` handles, fixed-width ints, a length on every buffer, a status return plus thread-local last error, `catch_unwind`, an FP guard; the `coolproplib-compat` feature adds legacy `PropsSI`/`Props1SI`/`PhaseSI` with CoolProp's process-global errstring (#3211 hosts). Browser = `cprs-wasm` (wasm-bindgen). WASI = `cprs` itself on `wasm32-wasip2` (the whole test suite runs under wasmtime); a WIT component waits for a consumer and wasip3. Python = `cprs-py` later (pyo3 abi3, GIL released). A network server is never core. | map 11 §9 U9-U12 and F1-F4/F16; map 14 §9 (legacy policies belong in the facade); dependencies R15-R17; prior-art P12 (no CoolProp binding ships a browser build). | uniffi (MPL, no C target), safer-ffi (0.2 not released) (dependencies R15); Emscripten (map 11 F20); reproducing CoolProp's integer enums in the new ABI (map 01 R1). |
| D12 | One `#[non_exhaustive] enum Error` (+ `DomainError`), `Clone`, `core::error::Error`, no `thiserror`. No panic on user input; `catch_unwind` at FFI. NaN is never a typed result (only NaN plus a `Status` in batch/C cells). FP: every transcendental goes through `math.rs` (std by default, `libm` feature for cross-target bit identity); integer powers by own `powi`; `f64::mul_add` banned by clippy `disallowed-methods`; fixed summation order. Tolerance classes live in `cprs-verify`: `Exact` (exec variants, threads), `Term` 1e-13·Σ\|c\|, `Prop` 1e-12 (1e-8 near critical), `SaCoeff` 1e-14, `Flash` 1e-9, `Paper` (half a unit in the last printed digit). Fixtures are generated on x86-64 Linux only. | dependencies R10/R14; kernel-performance K10/K12/K17/K18; map 02 §3 (agreement ~1e-14 is the realistic target, not bits); map 10 §9 U1; map 14 §9 (bindings map variants exhaustively). | Error per module (users juggle 5 types); NaN mode in core (prior-art P9: facade-only); bit-exact oracle parity (FMA and libm differences, map 05 §9 item 4). |
| D13 | Layers of truth: L0 hand-computed unit values; L1 Jet vs Jet2 (two derivative mechanisms); L2 paper tables, self-checked first (map 13: confirm the paper reproduces its own table with its own R, M and reducing values); L3 CoolProp 8.0.0 fixtures from `xtask oracle` (uv-pinned wheel, a fresh `AbstractState` per case, scrubbed `COOLPROP_*`/`PXFLASH_*`, all 38 config keys recorded, `oracle.lock`, CSV with shortest round-trip floats); L4 identities and round trips (own SplitMix64 sampler); L5 nightly consistency sweep over all pairs × fluids (fail closed; CoolProp baseline 12,355 failures, map 12 §6.2); L6 exec tests (batch, lanes, threads, all bitwise). Divergence register `fixtures/divergences.csv`: tests take the literature value AND assert the oracle still differs, so stale entries fail. Benchmarks: criterion under `cprs-verify` (dev-only). Perf gates follow kernel-performance §3.4 once the M4 baselines exist. | map 10 §9 U1-U10; map 12 §9 (errata registry from day 1); map 13 §9; prior-art P10. | proptest/approx (std-only alternatives are a few lines); a REFPROP oracle (proprietary, map 12 §9 drop); fixtures that hide failures (map 12 R17). |
| D14 | Code is `MIT OR Apache-2.0`. `cprs-data` ships CoolProp's MIT notice, and every model carries a `Source` (BibTeX, DOI, `DataTerms`). The NIST disclaimer is kept if `superancillary.h` algorithms are ported. No GPL/LGPL code (outram-park-fork-coolprop and others, prior-art §4); GSW-C is used as an oracle only (materials). Data whose provenance is not yet cleared (Ethanol-Water "from REFPROP with permission", the DTU table, fastchebpure outputs, INCOMP sheets; map 09 §9) stays out of default features. cargo-deny allow-list/bans, REUSE 3.3, cargo-about for C/WASM bundles. | dependencies R19/R20, map 09 §9, materials licensing row. | Copying normative standard tables (map 09: ISO 6976); MPL in T0/T1. |
| D15 | First useful release v0.1 = M0-M9 (§6): all 136 pure/pseudo-pure fluids, all 19 pairs, the staged + IAPWS + ECS transport and surface tension, `compat`, C ABI, browser WASM. Then M10 (SIMD gate), the remaining transport (rhosr-CS, Chung, the U10 hardcoded models), mixtures (`cprs-mix`, GERG via teqp vectors), cubic/PC-SAFT, IF97 and ice (materials §3.3 order), Python. | Users ask first for install/wrappers/WASM (23 % of issue titles, map 12 §6.5), then mixtures, then transport. Pure fluids are the user's stated start. | Mixtures in v0.1 (that doubles scope before the kernel is proven). |
| D16 | Placeholder `cprs` (crates) / `cp_` (C symbols). The user decides; candidates, availability unchecked: `thermoprop`, `fluxprop`, `phasekit`. Avoid a bare `coolprop` prefix: `coolprop-sys` and `coolprop-rs` already exist as bindings (prior-art §2.7). | D16 is the user's call. | — |
| D17 | Edition 2024, resolver 3, `rust-version = "1.85"` for published crates (dependencies R3); developed on 1.99.0; no nightly (R4). Workspace lints: `unsafe_code = "forbid"` (`cprs-capi` alone overrides to `deny`, with a per-function `#[allow]` and a `// SAFETY:` note), `missing_docs = "warn"`, clippy `all` + `disallowed-methods`; rustfmt width 120. CI: test on x86-64 Linux and Windows MSVC; `wasm32-wasip2` tests under wasmtime; `wasm32-unknown-unknown` build (baseline and `+simd128`); aarch64 macOS test; MSRV `check --lib`; clippy `-D warnings`; fmt; cargo-deny; `xtask deps` zero-dependency guard; nightly L5 sweep; Miri on the blob reader. Tests stay multi-threaded. | dependencies §3.3/§3.4; kernel-performance K17; map 11 U5 (WASM-clean gate); map 12 R17. | A pinned `rust-toolchain.toml` (dependencies §3.3); 32-bit stdcall (map 11 U16, deferred). |

## 4. Walkthroughs

**(a) One PT → h call for Water.**
```rust
let water = Registry::builtin().get("Water")?;                                   // 1-2
let st = water.state(Input::PT(Pressure(101_325.0), Temperature(300.0)))?;     // 3-7
let h = st.h(Basis::Mass);                                                       // 8: 112654.89965464505 J/kg (oracle, observed)
```
1. `get`: a case-folded binary search over the 556-key alias index gives a `FluidId`; `OnceLock::get_or_init` runs.
2. First use only: decode the Water blob (about 22 KiB) into `Arc<Core>`: info; ideal gas (Lead, LogTau, Planck-Einstein); default `EnergyOffset`; residual = 3 SoA blocks (51 Power, 3 Gaussian, 2 NonAnalytic, from the pinned JSON); the superancillary with precomputed extrema. Later calls cost one atomic load plus one `Arc` increment.
3. `Input::to_molar`: finite and > 0 checks; mass to molar; gauge to native.
4. `limits.check_tp`.
5. `classify_tp`: T is below Tc_num, so `Superancillary::eval(300 K)` (one `partition_point` and a 3-lane Clenshaw) gives psat ≈ 3536.8 Pa < p, so the phase is `Liquid`.
6. `density_bracket` = [ρ'_sat, ρ_max]. Safeguarded Newton on p(ρ) uses `alpha_r + Derivs::IDEAL_DELTA` (α^r only, no α0 work).
7. `State::single`: one α0 + α^r evaluation at order 2 fills the total bundle. `verify()` checks the residual and dp/dρ > 0. Result: `Path::DensitySolve`; p is returned exactly as given.
8. `h` = RT(A10 + A01) + Δh, divided by M.

No lock, no global read and no allocation after the first decode. ρ = 996.5569352651672 kg/m³ (oracle, observed) is the M5 fixture.

**(b) 1,000,000 PH flashes over 3 fluids, submitted from 16 threads at once.**
```rust
let reg = Registry::builtin();                                      // shared &'static
std::thread::scope(|s| for job in jobs /* 16 × (fluid name, h[], p[]); error handling elided */ { s.spawn(move || {
    let fluid = reg.get(job.fluid).unwrap();                        // 1 Arc clone per job, not per point
    let req = Request { kind: InputKind { pair: Pair::HP, basis: Basis::Mass }, x: &job.h, y: &job.p,
                        outputs: &[Prop::T, Prop::Dmass], flash: FlashOptions::new(), exec: Exec::Reference };
    batch::evaluate(&fluid, &req, &mut job.out, &mut job.status)    // or one caller with Exec::Parallel{chunk: 1024}
}); });
```
- **First use.** Racing first requests for one fluid block on that fluid's `OnceLock` for one decode (tens of µs, inference). The 3 fluids decode in parallel. Nothing else ever blocks.
- **Hot path.** `&Fluid` reads only: no refcount traffic, no lock, no shared write, scratch on the stack, outputs owned by the caller. So scaling is limited by cores, not contention (target ≥ 0.9·N, kernel-performance §3.4; confirmed by an M4 benchmark).
- **Memory.** 3 decoded fluids (about 3 × 50 KiB) shared by all threads; 0 bytes per point. The error path allocates only for `MultipleSolutions` roots and unknown names.
- **Per point.** Two-phase screen against the precomputed caloric curves at p. Otherwise TOMS748 on T, bracketed by Tsat(p) and the melting line, with an inner PT density solve. The target is ≤ 15 µs single-phase (oracle 119-376 µs, kernel-performance §3.4), so about 1 s wall time on 16 cores (estimate).
- **Failures.** A failed cell gets NaN plus a `Status`; no point hides another (map 14 §6 R3/R4).
- **Determinism.** Results are bitwise identical for any thread count or chunk size (cold start; `sixteen_threads_share_one_fluid_bitwise`).
- **SIMD.** PH is branchy, so it runs scalar per point and in parallel across points (D9).

**(c) A new EOS family (PC-SAFT), later a solid (ice Ih).**

PC-SAFT: new crate `cprs-pcsaft`, no core edit.
1. `struct PcSaft { m, sigma, eps_k, assoc: Option<Assoc>, source }` implements `Residual`. `eval` writes α^r(τ = 1/T, δ = ρ) for the hard chain, dispersion and association in `Jet2` arithmetic, as `VanDerWaals` does in `tests/new_family.rs`. The association X_A is solved in f64 with a typed `NoConvergence` (map 06: CoolProp silently accepts 100 unconverged iterations); one final Newton step in `Jet2` then yields the derivatives (implicit-function rule; inference, to be validated by finite differences).
2. The ideal part is `water.ideal_gas().clone()` from the canonical entry (map 15 §9: 24 of 116 cubic α0 blocks drifted), with the model's own R.
3. `Fluid::builder(..).ideal(..).residual(PcSaft{..}).limits(..).critical(..).build()`, then `Registry::empty().with_fluid(f)`.
4. PT, DT, D+X and single-phase P+X work immediately through the core flash. `Fluid::supports(QT)` is false until either the generic Maxwell VLE fallback lands in core (map 03 `vle::pure_eos`, a one-time addition that serves every family) or the crate ships a superancillary fitted offline.
5. If `Jet2` is too slow, the crate hand-optimises `eval` and overrides `eval_batch`, behind the same trait.
6. Oracles: Clapeyron/teqp/FeOs (CoolProp 8.0.0 PC-SAFT lacks h/s/cp/w, map 06 §9) plus paper tables.

Ice Ih: new crate.
1. Write the IAPWS-06 g(T, p) in `Jet2` (x = T, y = p), with the R10-06(2009) g00. CoolProp's superseded value becomes a divergence entry (materials §4).
2. A ~20-line algebraic Legendre converter (later `relations::gibbs`) produces the six totals. `State::from_total(t, 1/g_p, R, M, Phase::Solid, &total)` then makes a state, so h, s, cp, cv, w and ρ are reused unchanged. `tests/gibbs_seam.rs` runs exactly this path on a closed-form g(T, p) and checks it at 1e-12.
3. The only core edit is the additive `Phase::Solid` variant (`#[non_exhaustive]`).
4. When ice must coexist with liquid water (sublimation, frost, humid air), the materials R1 trigger fires: a `PhaseModel` trait, `Substance { phases }` and min-Gibbs selection go in a new crate, and core stays unchanged.
5. Oracles: R10-06 Table 6 check values, Clapeyron's `IAPWS06` (MIT), GSW-C as an external oracle only.

**(d) A CoolProp bug is found and the literature overrides CoolProp.**

Data bug: the oracle's N2 `rhomolar_reducing` is 11183.901464580624 (observed), but Span et al. 2000 gives 11183.9 (map 12 §6.3; fixed upstream in 2acbbc82).
1. Detection: an L2 paper fixture disagrees with L3 beyond tolerance, or an audit finds it.
2. Arbitration: first check that the paper reproduces its own table with its own constants (map 13). If it does not, record the paper's residual as the tolerance.
3. Fix: add a `corrections.csv` row (JSON path, old, new, citation, `DV-0001`). Datagen applies it, `cprs-data::DATASET` becomes `…+corrections.N`, and the blob changes.
4. Register: add a `divergences.csv` row (affected fluid and quantities, oracle value, literature value, citation, tolerance for oracle comparisons).
5. Tests: N2 oracle fixtures compare at the DV tolerance; paper fixtures stay at `Paper` class; a guard test asserts the oracle still differs, so the entry fails as stale if the pin moves.
6. Record it in the CHANGELOG and in the model's `Source`.

Algorithm bug: for Water, H+T at h(300 K, 10 MPa) = 2193.0757016361854 J/mol, the oracle returns two-phase with Q = 0.0037621392030636916 at 3536.8067523441227 Pa (observed; map 03 §6).
1. `tx` enumerates both branches and returns `MultipleSolutions`, or the liquid root under `RootPolicy::Nearest`.
2. The fixture cell becomes `err:MultipleSolutions`, with provenance "EOS root enumeration" plus a DV entry.

Literature errors: some papers print wrong equations (R-161, ethanol, krypton, xenon; map 12 R11). Every correlation is therefore tested against its paper's own check table, and a known erratum is stored with its provenance.

## 5. CoolProp rot designed out

| Rot (evidence) | How this design prevents it |
|---|---|
| Model, state, cache and workspace fused; 6 friend classes write about 170 fields (map 12 R1; map 01 R9/R11) | `Fluid` is immutable behind `Arc`; `State` is a `Copy` value with private fields; flash is a pure function. Nothing can write into a model. |
| Sticky `specify_phase`; one QS update breaks an instance (map 01 R12; map 03 §6) | The phase hint is a field of a per-call `Copy` options value. Test: `phase_hint_is_per_call_and_never_sticky`. |
| Non-transactional update; a rejected input stays in the state (map 01 R13; map 14 §9) | A flash returns a new `State` or an `Err`, so there is nothing to half-mutate. |
| 38 global config keys, 141 reads, env-var numerics, library code writing globals (map 12 R4; map 01 R15/R16) | No `static mut`, no config, no `std::env` in `cprs` (CI grep). `FlashOptions` is passed by value. |
| `catch (...)` cascades, `_HUGE` sentinels, errstring outbox (map 12 R3; map 01 R22) | One `Error` enum, `Option` for absent constants, a per-cell `Status`, ordered `Result` chains, `Path`. |
| Hand-written derivatives ×20 per term, duplicate multicomplex copy, wrong high orders (map 12 R5) | Jets: no per-term derivative formula exists. Jet vs Jet2 differential test. |
| Per-state deep copies of 65-164 KiB; eager decode of every fluid (2 s, +67 MiB) (map 01 R9/R10; map 11 F11/F13; map 14 §9) | One decoded `Arc` per fluid, lazy per fluid; `State` ≤ 256 B (compile-time assert). |
| SA mutex per call; global melting-cache mutex; caller mutated while building (map 11 F7/F8) | Derived data is precomputed by datagen. Core holds no lock besides first-use `OnceLock` initialisation. |
| Enum values renumbered while used as the C ABI (map 01 R1; map 12 R15, COO-37) | Rust enums never cross FFI. `cprs-capi` has pinned, append-only codes with a test. |
| Colliding pair names, 8 unreachable Q pairs, HmassT unsupported (map 01 R2-R4; oracle observed "[HmassT_INPUTS] is not yet supported") | One `Input` enum for 19 pairs, the basis inside the quantity, one `to_molar`, one (Var, Var) table in `compat`. |
| Output equals input echoes invalid values (map 01 R7; oracle observed `PropsSI("T","T",-5,…)` = -5) | No shortcut: every input passes the gate, so this is `Err(InvalidInput)`. |
| PropsSI builds a backend per call, 4× overhead (map 01 R8) | The registry is the model cache; `State` is cheap. |
| Lax, locale-dependent strings; `MEG-abc%` = 0 % (map 01 R17/R18; map 12 R18) | One strict, locale-free grammar (`compat::fluid_name`); mixtures and options are refused, not half-parsed. |
| 8 different R values; config-dependent critical point (map 12 R7; map 06 §9; map 10 §4: `ENABLE_SUPERANCILLARIES` moves ρc by up to 5.9 % for fluid R40) | R is model data (`Residual::gas_constant`). `Critical { published, numerical }` and the reducing state are three distinct notions. |
| Approximations shown as exact: C virial at δ = 1e-12, silent two-phase cp/cv/η/λ (map 12 R8; map 05 R1) | A_xy is finite at δ = 0 (test `finite_at_zero_density`); exact Taylor virials. cp, cv, w and transport return `Err(TwoPhase)` in the dome. |
| Solver returns x_{n+1} with the state at x_n; unchecked `max_iter`; absolute tolerances (map 03 §6) | `Root { x, f }` is evaluated at `x`; `NoConvergence` on exhaustion; typed `Tol` (relative now, log-ρ added with T+X). |
| T+X silently two-phase; strict-mode cliffs with 2-3 roots (map 03 §6) | `MultipleSolutions { roots }` plus `RootPolicy`. |
| SA extrapolated below T_triple in both modes (map 03 §6) | `Superancillary::eval` is domain-checked (unit test); extrapolation only per call. |
| Data provenance drift, stale derived h/s, fossil critical states (map 12 R10; map 10 §9) | Everything derived is regenerated by datagen in CI. The FNV `source_eos_hash` gate, the cited corrections overlay and the `DATASET` id. |
| Hidden cross-model coupling: ECS reference built by name on first evaluation (map 12 R12; map 05 R6/R7) | ECS holds its reference `Fluid`, resolved at decode, in an acyclic graph; dependencies are visible data. |
| Fat interface: 176 virtuals, capability discovered by exception (map 01 R21) | One 4-method trait; `Fluid::supports(pair)`; `Unsupported` is a typed error. |
| C ABI UAF, unbounded strings, `long` handles, exceptions crossing `extern "C"` (map 11 F1-F4, F16) | Generational `u64` handles, a length on every buffer, `catch_unwind`, fixed-width ints (M9). |
| TTSE/BICUBIC crashes and silent garbage (map 08 R1-R9) | Not ported. A future surrogate sits beside the exact model with exact-model accuracy gates. |
| Fail-open CI, order-dependent tests, bit-exact asserts break on arm64 (map 12 R17) | Tolerance classes, multi-threaded tests, Windows + WASM + aarch64 in CI, zero tests = failure. |
| Dead and legacy code, 2,607-line CMake, 12 fetched deps (map 12 R14/R16) | Only reachable behaviour is ported; one Cargo workspace; zero dependencies in the core. |

## 6. First TDD milestones (each ends green; check values marked "obs." were observed by me on the 8.0.0 oracle)

| M | Scope | Oracle / check values (gate) |
|---|---|---|
| M0 | Workspace, lints, CI matrix (§D17), `xtask deps` guard, Send/Sync and size asserts | Builds on 4 targets; clippy `-D warnings`; zero deps; tests run multi-threaded |
| M1 | `math`, `Jet`, `Jet2`, `Derivs`, `PowerBlock`; `cprs-verify`; `xtask oracle` (term fixtures from `AbstractState` α^r and its derivatives); `cprs::json` mirror | Hand values: δe^-δ gives A00 = 1/e, A02 = −1/e, A03 = 2/e, A04 = −3/e. R134a (21 Power terms only) at τ = 1.2472666666666667, δ = 2.362204724374703: α^r = −2.9019138848233386, δα^r_δ = −0.9742403422822072 (obs.). Gate: `Term` class on a 5-fluid (τ, δ) grid |
| M2 | Gaussian, NonAnalytic (via `Jet2`), Lemmon2005, DoubleExponential, GaoB; δ = 1 behaviour made explicit | IAPWS-95 Table 6 (500 K, 838.025 kg/m³): φr = −0.342693206e1 (`Paper` class); oracle −3.4269320568155854 (obs.). All 7 residual kinds at `Term` class |
| M3 | Ideal-gas kinds (10 JSON kinds → 5), datagen + blob format + `cprs-data` for Water, N2, CO2, R134a, Propane; registry; `relations`; DT flash; `Prop` table | Water DT(838.025 kg/m³, 500 K): p = 10000385.800921902 Pa, cv = 3221.062186740414 J/kg/K, w = 1271.2844091476063 m/s (obs.), plus IAPWS-95 Table 7 rows (`Paper`). Oracle DT grids at `Prop` class |
| M4 | All 136 fluids; `batch::evaluate`; `rayon` and `libm` features; criterion baselines; lazy-load and concurrency tests | 136-fluid DT fixtures; `is_loaded` only for touched fluids; 16-thread first use; batch == scalar and Parallel == Reference bitwise; wasip2 suite under wasmtime |
| M5 | `roots` (rtsafe, TOMS748), superancillary + `T(ln p)`, QT/PQ, PT + classification, pseudo-pure ancillaries | psat(Water, 400 K) = 245769.3455657737 Pa (obs.); PT(300 K, 101325 Pa): ρ = 996.5569352651672 kg/m³, h = 112654.89965464505 J/kg (obs.); SA check points at `SaCoeff` |
| M6 | P+X (PH, PS, PU), T+X with `MultipleSolutions`, DP, `with_reference` | Oracle PH(101325 Pa, h(300 K)) → 300.00000000002524 K (obs.; our round trip ≤ `Flash`); Water HT case of §4(d) → DV entry; R134a default gauge h'(273.15 K) = 199999.98852614488 J/kg (obs.) vs `with_reference(Iir)` = 200000 exactly |
| M7 | D+X, HS, SU, DQ/HQ/SQ for any Q, melting lines | Round trips from (p, T) truth (map 12 §6.4); HQ is unreachable in the oracle's PropsSI ("Input pair variable is invalid", obs.), so the truth is QT then H; R1234ze(E) s'' has 3 roots (map 03 §6) → `MultipleSolutions` |
| M8 | Transport: σ first, then staged η/λ by stage, IAPWS 2008/2011 water, ECS (map 05 order) | Water 300 K, 1 atm: η = 0.0008537424862859407 Pa·s, λ = 0.6094998584855923 W/m/K; σ(300 K) = 0.07176932405246211 N/m (obs.); paper check tables first (map 12 R11); R1233zd(E) η gap as a DV entry |
| M9 | Full `compat` alias table; `cprs-capi` (`cp_*`, FP guard, header) + `coolproplib-compat`; `cprs-wasm` package; **v0.1** | PropsSI parity sweep against the oracle minus registered divergences; `PropsSI("T","T",-5,…)` is -5 in the oracle (obs.) and `Err(InvalidInput)` here; C status codes pinned; wasip2 output identical to Linux bits under `libm` |

M10 (first after v0.1): the in-house `exp`/`ln` and the auto-vectorization gate of D9.

## 7. Risks and what this proposal deliberately defers

| Risk | Mitigation |
|---|---|
| Jets may be slower than CoolProp's hand-tuned `all()`, because they compute 15 entries when 6 suffice | `Order` masks the outer product. If M4 benchmarks lag, specialise an order-2 path inside `PowerBlock` (a private change, same tests). |
| Operation order differs from CoolProp, so last-bit parity is impossible | The `Term`/`Prop` classes (1e-13/1e-12) absorb it; bit parity is a non-goal (map 05 §9 item 4). |
| `Box<dyn Residual>` blocks cross-call inlining | Batches go through `eval_batch` (one dispatch per chunk); measured at M4. |
| Newtype fields are public, so `Temperature(-5.0)` is constructible | It is rejected at the single gate before any math. Validating constructors can be added later without a break. |
| One large core crate (estimated 25-30k LOC) may compile slowly | Modules already depend downwards only, so splitting is mechanical when a second consumer appears. |
| `FluidId` from one registry used with another indexes the wrong entry | Make `Registry::fluid` return `Err` on an out-of-range id (M3). |
| SA extrema computed without Eigen may differ in the last bits from CoolProp (map 09 §10) | `SaCoeff`/`Flash` tolerance classes; check-point fixtures. |
| `IdealGas::weighted` (D4) and `Phase::Solid` (D10) are core edits | Both are additive one-item changes, named here in advance. |
| MSRV 1.85 not verified locally | CI MSRV job (M0). |

**Deferred on purpose** (the trigger is in brackets):
- mixtures, GERG and ammonia-water (M11+, `cprs-mix`);
- cubic and PC-SAFT (a user, or the Maxwell VLE);
- IF97, INCOMP and humid air (materials §3.3 order);
- ice and the `PhaseModel` (coexistence);
- explicit SIMD crate (M10 gate fails);
- tabular, SBTL and SVDSBTL surrogates (a measured need);
- Python (a user);
- WIT component (wasip3 Tier 2);
- runtime JSON import beyond datagen (P2);
- alternate EOS selection (map 09 D13);
- mixture transport (map 12 §9 defer);
- plotting, isolines and cycle helpers (CoolPropPlot; deferred, not mapped);
- REFPROP backend (dropped);
- network server (never core).

## 8. Self-assessment against the rubric

| Criterion (weight) | Score /5 | Honest reason |
|---|---|---|
| Modularity and extensibility (3) | 4 | New families and new states of matter need no core edit, and both are proven from outside the crate (`new_family.rs`, `gibbs_seam.rs`). New term kinds and ideal kinds are closed enums: an edit to `eos.rs`/`data.rs`, by design. Mixtures need one additive constructor. |
| Simplicity (3) | 5 | 1 computing crate, 1 public trait, 0 public generic parameters, 1 derivative mechanism, 1 error type, 1 batch function, no strategy objects. |
| Concurrency and memory (3) | 4.5 | Immutable `Arc`, lazy per-fluid `OnceLock`, no hot-path locks, 16-thread bitwise test. The SA is not separately lazy (a deliberate choice pending measurement). |
| Performance (3) | 3 | **Weakest.** The batch seam is real and bitwise-tested, but the speed of jets against CoolProp is unmeasured, explicit SIMD is only a gated plan, and lockstep ρ(T,p) with masks is not designed beyond K14. |
| Correctness and TDD-ability (3) | 4.5 | 6 verification layers, a self-checking divergence register, observed oracle values per milestone. Risk: jets reorder operations, so parity is at 1e-13, not bitwise. |
| Idiomatic Rust, DRY/SOLID, minimal deps, strong types (3) | 4.5 | Zero dependencies, `non_exhaustive`, typed pairs and bases, one gate. Public newtype fields are a pragmatic softening of "invalid states unrepresentable". |
| Rot elimination (2) | 4.5 | 25 rot items designed out structurally (§5). Flash robustness still needs sweeps. |
| Cross-platform (2) | 4 | `cargo check` passes for Linux, Windows MSVC and wasm32-unknown-unknown (observed). wasip2 tests and bit identity under `libm` are planned, not yet run. |
| Migration path (1) | 3.5 | `compat` strings in Rust now; C legacy shim and WASM at M9; Python later. |

**Sketch gate results (observed, run from `sketch-lean` with `CARGO_TARGET_DIR=…/scratchpad/target-lean`):**
- `cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu`: OK, 0 warnings.
- `cargo check --workspace --target wasm32-unknown-unknown`: OK.
- `cargo check --workspace --target x86_64-pc-windows-msvc`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 20 passed, 0 failed.

The tests cover:
- the power term against hand values and closed forms;
- Jet vs Jet2 (Power, Gaussian, τ ln τ);
- batch == scalar bitwise;
- finiteness at δ = 0;
- Clenshaw plus the SA domain check;
- the `Prop` round trip;
- the vdW family from outside the crate: pressure, PT flash, registry + compat, batch status, 16 threads, the phase hint;
- the Gibbs seam;
- IAPWS-95 Table 6 against the oracle;
- fixtures;
- pinned C codes;
- doctests.
