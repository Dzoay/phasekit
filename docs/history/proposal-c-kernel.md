# Proposal C: Kernel

Architect: proposal "Kernel" (emphasis: performance and concurrency first). Type sketch (compiles, tested):
`<session scratchpad>/sketch-kernel`.
Crate names use the placeholder prefix `cprs-` (D16). Evidence tags: "map NN §X" = `docs/coolprop-map/NN-*.md`;
K#/R#/P#/S# = recommendation IDs in `docs/research/{kernel-performance,dependencies,prior-art,materials-extensibility}.md`.
Anything not traceable to those is marked *(inference)*.

## 0. Thesis

Concurrency comes from immutability; speed comes from writing the math once. Each fluid is one immutable
`Arc<FluidModel>`, decoded lazily into a lock-free registry; a `State` is a 232-byte `Copy` value from a pure `flash`.
All model math is generic over one `Real` trait. Executors (scalar reference, `Lanes<W>`, ISA entry points, rayon chunks) hold
no math and match the scalar reference bit for bit, so SIMD and threads are tested exactly and never duplicate physics.
Four library crates, zero third-party runtime dependencies; library `unsafe` lives in one dispatch module (plus the C facade).

## 1. Layers and crate layout

```text
 facades (thin)        cprs-capi            cprs-wasm (web | wasi)        cprs-py (deferred)
                            \                      |                        /
 composition root           cprs   built-in Registry (LazyLock) · ExecPolicy::Auto -> ISA · compat::props_si
                           /      \                                   \
 bytes / dispatch    cprs-data     cprs-simd (only unsafe: 1 module)   |
 (no code deps)          ·             \                               |
                         ·              cprs-kernel  ◄─────────────────┘
                         ·   ┌──────────────────────────────────────────────────────────────────────┐
                         ·   │ registry  FluidId → OnceLock<Arc<FluidModel>>; DataSource; FluidRef  │ lazy, lock-free
                         ·   │ exec      LaneKernel; run_scalar | run_lanes<W> | run_parallel; Plan │ HOW (no math)
                         ·   │ flash     Input(19 pairs) · FlashOptions · State(Copy) · flash()     │ branchy, scalar
                         ·   │ fluid     FluidModel = EOS + lazy SatData + lazy Transport           │ package
                         ·   │ model     HelmholtzModel (object-safe) · Capabilities · Limits · Gauge │ OPEN seam
                         ·   │ solve     Root/Tol · TOMS748 · bracketed Newton · Clenshaw            │ toolbox
                         ·   │ relations p,h,s,u,g,cv,cp,w from A_xy (family-neutral)               │ data-parallel
                         ·   │ eos       Derivs(A_xy) · Vars · exp-of-jets term blocks · ad          │ WHAT (math)
                         ·   │ num       Real · Lane · Lanes<W> · Dual · powi · vmath                 │ numeric core
                         ·   └──────────────────────────────────────────────────────────────────────┘
 dev / tools (unpublished): cprs-verify (fixtures, tolerance classes, divergence register) · cprs-bench · xtask (datagen, oracle)
```

`cprs-data` holds only bytes and an index; `cprs` connects it to the kernel through `DataSource`, so the kernel never depends on data.

| Crate | Responsibility | Depends on | Features | MSRV |
|---|---|---|---|---|
| `cprs-kernel` | Everything hot: numeric core, Helmholtz terms, relations, solvers, flash, `State`, executors, lazy registry, errors, units | std only | `rayon` (adds `run_parallel`) | 1.85 |
| `cprs-simd` | `#[target_feature]` entry points that instantiate `exec::run_lanes`; one-time detection | kernel | none | 1.85 (an AVX-512 level would raise it to 1.89) |
| `cprs-data` | Generated per-fluid LE blobs (`include_bytes!`), sorted name/alias/CAS index, ECS dependency list, CoolProp MIT notice | none | `fluids-all` (default), `fluids-core`, `fluid-<name>` (enables the features of its ECS references) | 1.85 |
| `cprs` | What applications use: `fluids()`/`fluid(name)`, `ExecPolicy::Auto` → ISA dispatch, `compat::props_si` | kernel, data, simd? | `simd` (default), `rayon`, `compat`, fluid sets | 1.85 |
| `cprs-capi` | C ABI: cdylib/staticlib, cbindgen header, opaque handles, status + thread-local error, FP guard; opt-in `coolproplib` shim | cprs | `coolproplib` | 1.85 |
| `cprs-wasm` | wasm-bindgen browser package (`Float64Array` batches, baseline + `simd128` builds); WIT component | cprs | `web`, `wasi` | per binding |
| `cprs-py` | pyo3 abi3 wheel; releases the GIL (deferred) | cprs | none | per pyo3 |
| `cprs-verify` (dev) | Zero-dependency fixture reader, `Tolerance` classes, `Provenance`, typed `OVERRIDES` register | none | none | 1.85 |
| `cprs-bench` (unpublished) | criterion + gungraun benches; `GATES` table of perf targets | cprs | none | n/a |
| `xtask` (unpublished) | `datagen` (JSON → blobs), `oracle` (fixture generation), `check-deps` | serde_json etc. (T4) | none | n/a |

That is four library crates. Every boundary does real work. `cprs-data` keeps data separate from code, with its own versioning, licence notice
and feature subsets. `cprs-simd` isolates `unsafe` and ISA code. `cprs` is the composition root that keeps the kernel free of data and dispatch.
The kernel is the hot core. Development starts with the kernel, `cprs-verify` and `xtask` only; the other crates appear at M3, M8 and M10 (§6).

## 2. Core types and traits

These excerpts match the sketch (`crates/cprs-kernel/src/*.rs`, `crates/cprs-simd/src/lib.rs`). Doc comments are trimmed.

```rust
// num.rs: the math is written once over Real; f64 = reference, Lanes<W> = SIMD across states, Dual = AD oracle
pub trait Real: Copy + Add<Output=Self> + … + Mul<f64, Output=Self> + Send + Sync + 'static {
    type Mask: Copy;                                   // bool | [bool; W]
    fn splat(x: f64) -> Self;  fn exp(self) -> Self;  fn ln(self) -> Self;    // exp/ln = vmath (msun port, M1)
    fn sqrt(self) -> Self;  fn abs(self) -> Self;
    fn lt(self, rhs: Self) -> Self::Mask;  fn select(m: Self::Mask, a: Self, b: Self) -> Self;
    fn is_finite(self) -> Self::Mask;  fn and(a: Self::Mask, b: Self::Mask) -> Self::Mask;
}
pub trait Lane: Real { const WIDTH: usize; fn load(src: &[f64]) -> Self; fn store(self, dst: &mut [f64]);
                       fn mask_lane(m: Self::Mask, lane: usize) -> bool; }
pub struct Lanes<const W: usize>(pub [f64; W]);       // portable, #[inline(always)] element-wise, no unsafe
pub struct Dual { pub re: f64, pub eps: f64 }          // HyperDual / Jet<4> join at M2
pub fn powi<R: Real>(x: R, n: u32) -> R;               // deterministic (std powi is not)

// eos.rs: scaled bundle A_xy = τ^x δ^y ∂^{x+y}α; independent of the reducing state, finite at ρ→0
pub struct Derivs<R> { pub a: [R; 15] }                // idx(i,j) triangular, i+j ≤ 4; const ORD picks the work
pub fn b_factors<R: Real>(j: [R; 4]) -> [R; 5];        // log-jet [Φ',Φ'',Φ''',Φ''''] → [1,B1..B4] (Bell + Stirling)
impl PowerBlock {                                       // SoA: n, t, d, l, c (+ precomputed τ-side B-factors)
    pub fn value<R: Real>(&self, tau: R, delta: R) -> R;                       // value-only form → AD oracle
    pub fn accumulate<R: Real, const ORD: usize>(&self, v: &Vars<R>, acc: &mut Derivs<R>) {
        for k in 0..self.n.len() {                                             // control flow only on term data
            let lf = f64::from(self.l[k]);
            let x = v.delta_pow[usize::from(self.l[k])] * self.c[k];          // c·δ^l (c = 0: polynomial term)
            let phi = (v.ln_tau * self.t[k] + v.ln_delta * self.d[k] - x).exp() * self.n[k];  // ONE exp per term
            let l2 = lf * lf;
            let bd = b_factors([-(x * lf) + self.d[k], -(x * l2), -(x * (l2 * lf)), -(x * (l2 * l2))]);
            acc.add_outer::<ORD>(phi, &self.bt[k], &bd);                       // A_ij += φ·B^δ_j·B^τ_i
        }
    }
}
#[non_exhaustive] pub enum ResidualBlock { Power(PowerBlock), Gaussian(GaussianBlock),
                                           Generic(GenericSeparable), NonAnalytic(Box<[NonAnalyticTerm]>) }
impl ResidualEos { pub fn residual<R: Real, const ORD: usize>(&self, tau: R, delta: R) -> Derivs<R>; }
pub mod ad { pub trait AlphaValue { fn alpha<R: Real>(&self, tau: R, delta: R) -> R; }
             pub fn bundle<A: AlphaValue + ?Sized>(a: &A, tau: f64, delta: f64, ord: Order) -> Derivs<f64>; }

// model.rs: the OPEN seam between families (object-safe → Arc<dyn HelmholtzModel> works)
pub trait HelmholtzModel: Send + Sync {
    fn gas_constant(&self) -> f64;  fn molar_mass(&self) -> f64;  fn reducing(&self) -> Reducing;
    fn alphar(&self, tau: f64, delta: f64, ord: Order) -> Derivs<f64>;
    fn alpha0(&self, tau: f64, delta: f64, ord: Order) -> Derivs<f64>;
    fn limits(&self) -> &Limits;  fn capabilities(&self) -> Capabilities;   // declared pairs (bitset)
    fn native_gauge(&self) -> NativeGauge;  fn key(&self) -> ModelKey;
    fn saturation(&self) -> Option<&dyn SaturationCurve> { None }
    fn alphar_batch(&self, tau: &[f64], delta: &[f64], ord: Order, out: &mut [Derivs<f64>]) { /* scalar loop */ }
}

// exec.rs: WHAT (a LaneKernel) is separate from HOW (executors). Executors write only caller buffers
pub trait LaneKernel<const NI: usize, const NO: usize>: Sync {
    const FAIL: Status;
    fn eval<R: Real>(&self, x: [R; NI]) -> ([R; NO], R::Mask);         // pure, branch-free per point
}
pub fn run_scalar<K: LaneKernel<NI, NO>, const NI: usize, const NO: usize>(
    k: &K, input: [&[f64]; NI], out: [&mut [f64]; NO], status: &mut [Status]) -> Result<(), Error>;
#[inline(always)] pub fn run_lanes<const W: usize, K: LaneKernel<NI, NO>, const NI: usize, const NO: usize>(…);
#[non_exhaustive] pub enum ExecPolicy { Reference, #[default] Auto, Parallel { chunk: NonZeroUsize } }
pub enum WarmStart { #[default] Off, WithinChunk }
pub fn eval_batch<M: HelmholtzModel + ?Sized>(model: &M, gauge: Gauge, plan: &Plan, x: &[f64], y: &[f64],
    out: &mut [&mut [f64]], status: &mut [Status], policy: ExecPolicy,
    errors: Option<&mut Vec<(usize, Error)>>) -> Result<BatchReport, Error>;

// registry.rs: lock-free after first touch; handles BORROW (no Arc traffic per call)
pub struct Registry { index: Cow<'static, [IndexEntry]>, cells: Box<[OnceLock<Result<Arc<FluidModel>, LoadError>>]>,
                      source: Box<dyn DataSource>, opts: LoadOptions }
pub trait DataSource: Send + Sync { fn blob(&self, id: FluidId) -> Result<Blob, LoadError>; }  // cold path only
pub enum Blob { Static(&'static [u8]), Shared(Arc<[u8]>) }
#[derive(Clone, Copy)] pub struct FluidRef<'r> { model: &'r FluidModel, reg: &'r Registry, gauge: Gauge } // 32 B

// fluid.rs: one package per fluid; parts materialise on first use
pub struct FluidModel { info, eos: MultiParamEos, eos_source: Source, limits, crit: CriticalPoints, caps, key, blob: Blob,
                        sat: OnceLock<Option<SatData>>, transport: OnceLock<Result<Transport, LoadError>> }
pub enum ViscosityModel { Staged, Ecs { reference: Arc<FluidModel> } /* … */ }       // resolved via the registry

// flash.rs: a pure function; State has private fields, holds no Arc and no model pointer
pub fn flash<M: HelmholtzModel + ?Sized>(m: &M, input: Input, opts: &FlashOptions, gauge: Gauge) -> Result<State, Error>;
pub struct State { t, p, a0_tau: [f64; 3], sides: Sides /* Single(Side) | TwoPhase{q, liq, vap} */, ctx: Ctx, phase }

// lib.rs: D8 by construction
const _: () = { const fn shared<T: Send + Sync + 'static>() {}
    shared::<FluidModel>(); shared::<Registry>(); shared::<State>(); shared::<FluidRef<'static>>();
    shared::<Arc<dyn HelmholtzModel>>(); assert!(size_of::<State>() <= 256); };

// cprs-simd: the only unsafe; every level runs the same run_lanes body → bitwise = scalar
pub fn run_at<K: LaneKernel<NI, NO>, …>(level: Level, k: &K, …) -> Result<(), Error> {
    match level {
        #[cfg(target_arch = "x86_64")]
        Level::Avx2 if std::is_x86_feature_detected!("avx2") => x86::avx2(k, input, out, status), // unsafe inside
        Level::Neon | Level::Simd128 => run_lanes::<2, K, NI, NO>(k, input, out, status),
        _ => run_lanes::<4, K, NI, NO>(k, input, out, status),
    }
}
```

## 3. Decisions D1-D17

**D1 Crates and features.** *Decision:* the four library crates of §1, three thin facades and three dev tools. Features only add
capability: `rayon`, `simd`, `compat`, fluid sets. Numerics are chosen at run time through `ExecPolicy`. *Rationale:* feature unification
can then never change anyone's numbers (K13). The zero-dependency core is enforced by `xtask check-deps` (dependencies.md R1). The
`fluid-<x>` features encode ECS dependencies, so a subset build cannot miss a reference fluid. *Rejected:* one crate (that would put
`unsafe` and data in the core, and give facades and core a shared MSRV); a crate per layer (eos/flash/registry) as P11 warns, because no
second consumer exists yet and modules suffice; a numeric `fast-math` feature (it would break K13).

**D2 Numeric core and derivatives.** *Decision:* public API is `f64` SI. Internally one `Real` trait with three implementations:
`f64` (the reference), `Lanes<W>` (SIMD across states) and `Dual`/`HyperDual`/`Jet<4>` (AD). Separable terms use hand-derived log-jets
with shared B-factors, the "exp of jets" engine. The sketch implements it fully for `PowerBlock` and tests it against a hand derivation (≤1e-14) and
against `Dual` AD (≤1e-13). NonAnalytic terms, association and new families use value-only `ad::AlphaValue` with AD. `num-dual` is a dev oracle
only. *Rationale:* one exp per term, the cost centre (kernel-performance.md §2.4.1). 99.8 % of terms share one engine and differ only in a
4-entry log-jet, which removes CoolProp's ~1.8k hand-derivative lines (map 02 §3.3; map 12 R5). The same generic code runs on lanes, so SIMD
is not a second implementation (K8). *Rejected:* full AD on the hot path (slower than B-factor jets; P2); `num-dual` in core (the orphan rule
blocks lane types and the vector duals need nalgebra; dependencies.md §2.4); `std::autodiff` (nightly, no MSVC or WASM); generic
`[f64; ORD+1]` (needs `generic_const_exprs`, so the fixed 15-slot bundle uses a const `ORD`).

**D3 Model representation.** *Decision:* closed inside a family, open between families. Multiparameter term kinds form a
`#[non_exhaustive]` enum of SoA blocks, with `match` per block (≤4 blocks per fluid) and never per term. Families plug in through the
object-safe `HelmholtzModel` trait. Data-parallel work plugs into executors through `LaneKernel`. *Rationale:* closed term sets validated at
datagen (map 09 §6) and P1. The trait gives Open/Closed for new families (PC-SAFT, cubic, GERG at fixed z) with no core edits (§4c). Batches
are partitioned first, so `dyn` costs one call per chunk (materials-extensibility.md Q5). *Rejected:* `dyn Term` per term (a virtual call per term
blocks lanes); a generic `Fluid<M>` everywhere (monomorphisation explodes across facades); a plugin or dynamic loader (S10 deferral).

**D4 Pure vs mixture.** *Decision:* the v1 contract holds no composition. The pure fast path is the concrete `FluidModel`. Mixtures land
in P2 as a `Mixture` type: `Arc`-shared component models, a trimmed composition and departure terms, implementing `HelmholtzModel` *at fixed
composition*. That means one cheap value per composition sharing component `Arc`s *(inference: ~100 ns build)*. A `MixtureModel` extension trait
then adds mole-number derivatives through a single AD path. *Rationale:* CoolProp pays ~6 fluid copies and ~100 pure/mixture branch sites per pure
state (map 04 §9). Zero fractions must be trimmed at build (map 04). Composition is not always mole fractions (materials-extensibility.md §4).
*Rejected:* `x: Vec<f64>` in `State` (an allocation and a wrong universal contract); mixture-first design (penalises the 136-fluid core).

**D5 State and properties.** *Decision:* `State` is a `Copy` value of 232 B (asserted ≤256 B at compile time), built only by
flash. It holds T, p, phase, quality as an `Option` (no −1 sentinel), the δ-independent ideal-gas τ-jet, and one or two `Side{ρ, ln δ, A^r[6]}`.
The order-2 bundle is eager, because the flash already evaluated it at the converged point; higher orders are recomputed on demand. It also carries
`Ctx{R, M, gauge, ModelKey}`, so getters need no model and a foreign state is refused. Inputs use the 19-pair `Input` enum with validated newtypes
(`Temperature`, `Pressure`, `Density`/`Specific` with basis, `Quality` ∈ [0,1]) and one `to_molar(M)`. Outputs are typed getters; `Prop` is
`#[non_exhaustive]` and serves batch and compat. Two-phase cp/w are `Err(UndefinedInPhase)`. *Rationale:* map 01 §9 U1-U6; K1: an `Arc` in `State`
is a contended refcount write per state. map 02 §6: two-phase cp is meaningless in CoolProp. *Rejected:* lazy `OnceCell` caches in `State` (they
break `Copy` and `Sync`, and the flash cost dominates anyway; open question 7 of kernel-performance.md, settled by the M7 benchmark); `uom` in core
(R11; it is a facade concern).

**D6 Flash architecture.** *Decision:* `flash(&M, Input, &FlashOptions, Gauge) -> Result<State, Error>` runs these steps in order: capability
check → basis and gauge normalisation → domain check (per-call opt-out) → one kernel-defined phase rule from `SaturationCurve` → an ordered
strategy chain per pair inside solver brackets → a single `verify` gate. The toolbox returns `Root{x, f@x, iters, status}`. `Tol` is `Rel`, `Abs`
or `Log`. `RootPolicy` is `Strict` (error carrying the roots), `NearestT` or `Stable`. There is no global configuration. *Rationale:* map 03 §6
(sticky phase, the result/state mismatch, silent `max_iter`, the 6.1 % error from an absolute tolerance) and map 12 R1/R3/R4. *Rejected:* CoolProp's
mutable backend with `catch(...)` cascades (237 non-test sites, map 12 R3); a `Strategy` trait object registry (an ordered `match` per pair is enough).

**D7 Data pipeline.** *Decision:* the pinned v8.0.0 `dev/fluids` JSON (sha256 in `oracle.lock`) goes through `xtask datagen`: a strict
literal-kind-preserving parse, `deny_unknown_fields`, the logged Chlorine waiver and the FNV `source_eos_hash` gate. Datagen precomputes all derived
SA data, builds the corrections overlay and the ECS DAG (cycles rejected), and writes per-fluid LE blobs plus the index into `cprs-data`. The blobs
are uncompressed, committed and diffed in CI. At run time a `Blob::Static` (`include_bytes!`, zero-copy) or `Blob::Shared` (fetch or file) is
decoded on first `get`. The whole blob is validated up front; the EOS is materialised immediately, while the SA section (84 % of bytes) and
transport wait for first use. Transport resolves ECS references through the same registry. *Rationale:* map 09 §9 D1-D6 (22 KiB/fluid, no
compression, precomputed extrema); map 11 F13 (1.98 s and 37 MiB eager); map 05 §5 (19 ECS fluids depend on Propane, R134a or N2); R6/R7/R8.
*Rejected:* runtime serde/CBOR/bincode (R7; bincode is banned); `build.rs` codegen (it costs every adopter); runtime SA builds (34-51 ms first-use
spikes, map 03 §5); `Weak` caches (K2).

**D8 Concurrency kernel.** *Decision:* the registry is `[OnceLock<Result<Arc<FluidModel>>>]` indexed by `FluidId`, with an immutable sorted
index. After first touch a lookup is one acquire load. Handles are `FluidRef<'r>` (32 B, `Copy`, borrowing), so there is no `Arc` clone per call.
Models have no interior mutability except `OnceLock`, enforced by clippy `disallowed-types` (`Mutex`, `RwLock`, `RefCell`, `Cell`) in the kernel.
Scratch lives on the stack. Batches use caller buffers, so nothing is allocated. `rayon` is optional, with fixed chunks and no reductions.
`preload(&[FluidId])` serves servers. Send + Sync + 'static is asserted at compile time. *Rationale:* K1-K6; map 11 §9 U1/U2; the contention
analysis in kernel-performance.md §2.1.3. CoolProp has a mutex on every `get_T_from_p` (F7) and a `deriv_counter` RMW on every evaluation.
*Rejected:* `RwLock<HashMap>` (a CAS per read); `arc-swap` (no hot-swap need in v1); `thread_local!` scratch (hidden state, hostile to WASM and
async; K4).

**D9 Execution strategies.** *Decision:* "what" is a `LaneKernel` and "how" is an executor. `run_scalar` is the source of truth. `run_lanes<W>`
runs W states per lane and its remainder through scalar, which is **bitwise equal** to scalar. `cprs-simd` provides `#[target_feature]` entry points
(AVX2 now; NEON and simd128 run 2 lanes). `run_parallel` uses chunks of `CHUNK = 1024`. GPU is far future. Dispatch uses runtime detection on native,
cached in a `OnceLock`, and compile time on WASM (two builds). Kernels that qualify are the α^r/α⁰ bundles, relations, SA Clenshaw, ancillaries,
transport sums, and the lockstep ρ(T,p) and (T, ln ρ) Newton with masks and a scalar retry. Phase logic, P/T+X cascades, HS, VLE and mixtures never
get lanes; they parallelise across points (K14). Across-terms SIMD and an FMA `Fast` policy are later experiments with tolerance classes (K9, K12).
*Rationale:* K8-K17. The sketch test `every_level_matches_scalar_bitwise` passes, including the AVX2 entry point on the dev box. *Rejected:*
`std::simd` and `sleef` (nightly); `wide` (no runtime dispatch, unspecified precision); hand-written intrinsics before the portable lanes are
measured; a separate SIMD copy of the term math.

**D10 Material and states-of-matter seams.** *Decision:* build only these seams now. (1) A family-neutral `relations` layer on A_xy, which is
reducing-invariant. (2) The object-safe `HelmholtzModel` seam. (3) `#[non_exhaustive]` `Phase`, `Prop`, errors and `ResidualBlock`, and private
fields on `State` and `Limits`. (4) A `Source` provenance record and a `NativeGauge` declaration. (5) `LaneKernel`, so any family's data-parallel parts
get executors. Deferred, each with a trigger: `GibbsModel` plus `relations::gibbs` (IAPWS-06 ice), `PhaseModel`/`Substance`/min-Gibbs selection (two
phases of one substance), `Domain` with holes (IF97 or Bollengier), the `correlation` module (INCOMP), solids, tensors and plasma. *Rationale:*
materials-extensibility.md S1-S10 and §3.2 ("one implementation is a guess"). *Rejected:* a `Material` abstraction now (Cantera's deprecated lattice
classes are the cautionary tale).

**D11 Facades.** *Decision:* the Rust API is `cprs`. The PropsSI-style API is `cprs::compat` (feature): one strict grammar, no echo shortcut, no
sentinels, deviations recorded in the register. The C ABI is `cprs-capi`. WASM is `cprs-wasm`, where browser packaging uses wasm-bindgen and
`Float64Array` batches, and WASI either compiles `cprs` for wasip2 directly or uses a WIT component later. Python is `cprs-py`, deferred. The
CoolPropLib.h compat (tier A, v8.0.0 enum values, process-global errstring) is opt-in inside `cprs-capi`. *Rationale:* map 11 §9 U9-U12;
dependencies.md R15-R17; map 14 (legacy fill policies belong in facades). *Rejected:* `uniffi` (no C target, MPL); `safer-ffi` (0.2 not released);
emscripten (map 11 F20).

**D12 Errors, panics, NaN, determinism.** *Decision:* hand-written `#[non_exhaustive]` enums `Error`, `InputError`, `DomainError`, `FlashError` and
`LoadError`, implementing `core::error::Error`. Batches return a `u8` `Status` per point plus an optional sparse `(index, Error)` list. NaN is
rejected at the newtype boundary; NaN fill and `+inf` exist only in facades. No library path panics on any input; this is fuzzed with proptest.
`catch_unwind` guards the FFI. Determinism: the own `vmath` (msun port) is used everywhere, with no `mul_add` (clippy `disallowed-methods`) and no
std `powi`/`powf`, so the Rust results are **bitwise equal across Linux, Windows and WASM**. CI checks this with a result hash. *Rationale:* R10,
K10, K12, map 11 F5/F6/F15, map 14. *Rejected:* `thiserror` (proc-macro chain); std transcendentals (documented non-deterministic); `-ffast-math`
equivalents.

**D13 Verification architecture.** *Decision:* `cprs-verify` has zero dependencies. It provides a CSV fixture reader (`#` header recording oracle
version, git, wheel sha, full config and scrubbed env), the tolerance classes of map 10 §8.3 (`Exact`, `Term`, `Prop`, `Flash`, `Paper`, …),
`Provenance`, and a typed `OVERRIDES` divergence register (in the sketch: N2 reducing density, virial C, ice VI melting). Oracle fixtures are
generated multi-process by `xtask oracle` (uv-pinned CoolProp 8.0.0, fresh instance per case) on x86-64 Linux only. The core subset is
committed; the full set is tracked by a sha256 manifest. Property tests check identities (Maxwell relations, cp − cv, FD vs the analytic bundle).
L6 differential tests compare lanes, threads and batch against scalar **bitwise**. Performance gates:

| Gate (1 x86-64 core) | Target | CoolProp 8.0.0 (Python-level oracle; ~0.3 µs wrapper) | Measured by |
|---|---|---|---|
| α^r bundle O2, 16-20 terms, scalar | ≤ 0.3 µs | n/a (no C++-level figure; §2.10) | criterion; gungraun instr. count in CI |
| same, AVX2 across states | ≤ 0.1 µs/state, **≥ 2.5× or stop and re-plan** | — | criterion, forced levels |
| properties at (T, ρ): p, h, s, cp, w | ≤ 0.5 µs | `update(D,T)`+h+cp 1.5-10.7 µs | criterion |
| QT / PQ via superancillary | ≤ 0.1 µs | 0.45 / 0.64 µs (PQ first call +1.3-1.7 ms) | criterion |
| PT flash, single phase | ≤ 3 µs | 19-27 µs | criterion over the 40×40 (log p, T) grid |
| PH flash single / two-phase | ≤ 15 µs / ≤ 1 µs *(inference)* | 119-376 µs / 7 µs | criterion over the grid |
| batch overhead vs the same executor's single calls | ≤ 5 % *(inference)* | — | criterion |
| hot lookup by `FluidId` / by name | ≤ 5 ns / ≤ 50 ns *(inference)* | `PropsSI` rebuilds a backend: 76.5 µs vs 13.9 µs reused | criterion |
| first touch: EOS decode / SA section | ≤ 50 µs each *(inference)* | whole library 1.98 s at import | `Instant` in a fresh process |
| thread scaling, same and different fluids | ≥ 0.9·N up to physical cores | GIL: 0.97× on 4 threads | `thread::scope` harness, 1…N |
| memory per loaded fluid | EOS ≤ 25 KiB + SA ≤ 25 KiB; all 136 ≤ 8 MiB RSS *(inference)* | 65-164 KiB per state; +68.9 MiB RSS | counting allocator; `/proc` RSS |
| heap allocations per flash or batch point | 0 | — | counting `#[global_allocator]` test |
| `State` size | ≤ 256 B (232 B now) | 65-164 KiB per AbstractState | compile-time `const` assert |

Targets are K19 estimates until the M7 baseline. A C++-level CoolProp baseline is built in scratch, never in `reference/` (kernel-performance.md §3.3).
*Rejected:* bitwise asserts against CoolProp (its libm and FMA contraction differ; K18); widening tolerances to fit the oracle (map 10 §8.5 rule 3).

**D14 Licensing.** *Decision:* code is MIT OR Apache-2.0. CoolProp's MIT notice ships in `cprs-data` and in `LICENSE-THIRD-PARTY`. The NIST
disclaimer is kept if superancillary code is ported. Every model carries `Source` (bibkey, DOI, content hash). cargo-deny runs with the allow-list
of R19/§3.1. REUSE 3.3 and cargo-about cover releases. No GPL/LGPL code: outram-park-fork-coolprop, SeaFreeze and the GSW-C port are excluded;
GSW-C serves only as an oracle. Provenance must be cleared for Ethanol-Water (REFPROP 9.1), the DTU table and the INCOMP fits before they ship.
*Rationale:* map 09 §9 licensing; P13; R20. *Rejected:* single-licence MIT (Apache-2.0 adds a patent grant that Rust ecosystem norms expect).

**D15 Milestones and first release.** *Decision:* the ten always-green milestones of §6. **v0.1 = M0-M10**: thermodynamics for 136 pure and
pseudo-pure fluids, all 19 pairs, scalar + batch + SIMD + threads, Rust API, compat PropsSI, C ABI, browser + WASI. v0.2 adds transport and surface
tension (map 05 order U1→U5→U3…), reference states and melting lines. v0.3 adds mixtures (map 04), then IF97, ice and humid air. *Rationale:* the
kernel's performance claims need flashes and batches before transport. Transport is pure functions of `State` and plugs in without kernel changes
(S5). *Rejected:* transport in v0.1 (it delays the concurrency proof); mixtures before pure parity.

**D16 Naming.** *Decision:* `cprs-*` is a placeholder. The user chooses the final name; crates.io availability is checked at M0. Facade
names follow it (`<name>-capi`, `<name>-wasm`). The Python namespace must not shadow `CoolProp` by default (map 14).

**D17 Toolchain and lints.** *Decision:* edition 2024, `resolver = "3"`, develop on 1.99.0, `rust-version = "1.85"` for every library crate
(the AVX-512 level would raise `cprs-simd` alone to 1.89). Workspace lints: `unsafe_code = "forbid"`, except `deny` plus one `#[allow]` module in
`cprs-simd` and `cprs-capi`; `missing_docs`, `unreachable_pub`; clippy `-D warnings` (incl. `incompatible_msrv`, which enforces the MSRV) with
kernel `disallowed-types/methods`. CI: Linux x86-64 and Windows MSVC test; `cargo test --target wasm32-wasip2` under wasmtime; wasm32-unknown-unknown
build (baseline + `simd128`); aarch64; an MSRV job (`check --lib`); Miri on `cprs-capi` and the registry; forced dispatch levels; the
cross-platform result hash; `cargo deny`; `cargo-semver-checks` before releases; zero-tests-ran = failure (map 12 R17). *Rationale:* dependencies.md
§3.3-3.4. *Rejected:* a pinned `rust-toolchain.toml` (it pins only the bench job); nightly anywhere (R4).

## 4. Walkthroughs

**(a) One PT → h call for Water** (p = 101 325 Pa, T = 300 K; oracle `hmass` = 112 654.89965464505 J/kg).
1. `cprs::fluid("Water")` → `BUILTIN` (`LazyLock`, built once) → `resolve`: binary search, ASCII case-folded, no allocation → `FluidId`.
2. `get(id)` → `OnceLock::get_or_init`. Cold: `cprs_data::blob(id)` returns `&'static [u8]` (paged in by the OS). `FluidModel::decode` validates
   header, checksum and section table, materialises the EOS (Water: 56 residual terms incl. 2 NonAnalytic) and wraps it in `Arc`. Hot: one acquire
   load. The returned `FluidRef<'static>` is 32 bytes and `Copy`.
3. `water.flash(Input::pt(Pressure::new(101_325.0)?, Temperature::new(300.0)?), &FlashOptions::default())`. The newtypes have already rejected
   NaN and negative values.
4. `flash`: `Capabilities::supports(PT)` → `Limits::check_tp` (plus the melting line) → `water.saturation()` materialises `SatData` on first use →
   `at_t(300 K)` does one piece lookup and three Clenshaw chains, giving p_sat ≈ 3.5 kPa < p, so the state is liquid → density bracket
   [ρ'(T), ρ_max(T)] from SA and melting data → `newton_bracketed` on p(ρ) − p with an O3 δ-jet → `Root{x, f@x, status: Converged}` → `verify`
   (p reproduced, dp/dρ > 0, cv > 0).
5. `State::single` evaluates the O2 bundles once at the converged (τ, δ) and stores them with R, M, gauge and key: 232 B on the stack, no heap.
6. `s.enthalpy_molar()` computes `RT(1 + A^r_01 + A⁰_10 + A^r_10) + gauge.dh`; mass basis multiplies by 1/M. The target is ≤ 3 µs in total
   (CoolProp `PropsSI` is 76.5 µs, of which ~75 % is backend construction; map 11 §5d/§8).

**(b) 1,000,000 PH flashes over three fluids from 16 threads.** Water, R134a and R143a are submitted at the same instant; each thread holds
62 500 points of one fluid.
- **Load (once per fluid, concurrent).** The first `get` on each id runs one initializer, and other threads touching the same id block briefly
  (≤ 50 µs target) instead of decoding again; the sketch test `first_touch_initialises_once_under_contention` asserts exactly one source read
  across 16 threads. Different fluids initialise in parallel because each has its own `OnceLock`. PH needs SA brackets, so `SatData` is
  materialised once per fluid. R143a's ECS transport is **not** loaded, because PH needs no transport. If a later viscosity call arrives,
  `transport(reg)` resolves R134a through the same registry and shares the R134a model already in memory, with no second copy (CoolProp holds
  separate reference backends per state and per property: 93 → 297 KiB; map 11 §5f).
- **Memory.** 3 × (EOS ≤ 25 KiB + SA ≤ 25 KiB) ≤ 150 KiB shared read-only, so it stays L2-resident and unpoisoned by writes. Per thread: stack
  scratch (one `Vars` + bundles, < 1 KiB) plus one 1024-entry `u32` partition buffer per chunk (4 KiB). Caller-owned output columns of
  1 M × k × 8 B. There are no per-thread model copies, no per-point heap allocation and no shared writes; the CoolProp equivalent is 16 threads
  × 3 AbstractStates × 65-164 KiB plus its mutex on every `get_T_from_p` (F7).
- **Per call** `cprs::eval_batch(water.model(), gauge, &plan, &h, &p, &mut [t_out, rho_out], &mut status, ExecPolicy::Auto, None)`. Stages:
  (1) validate inputs as lanes, writing `InvalidInput` for NaN or out-of-range values; (2) classify against h'(p)/h''(p) using the datagen
  caloric curves and the T(ln p) inverse, with no runtime builds; (3) stable counting-sort partition per chunk: two-phase, liquid, gas,
  supercritical; (4) the two-phase group computes T_sat(p) and the lever rule in lanes; (5) single-phase groups run a lockstep 2-D Newton in
  (T, ln ρ) with a fixed iteration budget and masks, seeded inside SA and melting brackets; (6) lanes that miss the budget retry with the scalar
  bracketed strategy chain; (7) scatter results and write `Status`.
- **Execution.** `Auto` resolves once to AVX2 `Lanes<4>` on this hardware. Callers are already 16 threads, so they use `Auto`, not
  `Parallel`: the library never spawns threads and never nests pools. One caller with one big slice would use `Parallel{chunk: 1024}` instead.
  Results are **bitwise identical** to 1 M scalar `flash` calls with `WarmStart::Off`. With `WithinChunk` they still do not depend on the thread
  count, but they differ from cold-start results within the `flash` tolerance class.
- **Throughput target.** Single-phase PH costs ≤ 15 µs scalar, and roughly ≤ 5 µs/point amortised with lanes *(inference)*. Two-phase costs ≤ 1 µs.
  With ≥ 0.9·N scaling over 16 physical cores, 1 M mixed PH flashes take about 0.3-1 s wall time *(inference; confirmed by bench gate
  `flash_ph_*` and the scaling harness at M9)*. Every failure is a status byte plus an optional `(index, Error)`; a batch never aborts.

**(c) Adding a new EOS family (PC-SAFT), later a solid (ice Ih).**
- PC-SAFT lives in a new crate `cprs-pcsaft` (or a kernel module behind a feature) and needs **no edits** to existing kernel code.
  `struct PcSaft { params: Arc<PcSaftParams>, ideal: IdealGas, … }`.
  It implements `ad::AlphaValue::alpha<R: Real>(τ, δ)` **once** (hard chain, dispersion, association with a typed inner `X_A` solve that returns
  `Root`). `alphar` returns `ad::bundle(self, τ, δ, ord)`, so AD supplies every derivative and there are no hand-coded copies (CoolProp has four,
  ~1.45k lines; map 06 §9).
  `capabilities()` declares PT and DT first; QT/PQ are added once the generic VLE solver lands (P2). `saturation()` returns `None` and
  `native_gauge()` returns `ResidualOnly`; the ideal part is borrowed from the canonical substance entry (map 15 §9).
  `flash`, `eval_batch`, `ExecPolicy::Parallel`, `compat` (with an overlay registry) and the C ABI all work immediately through `&dyn HelmholtzModel`.
  Lanes are optional: a `LaneKernel` impl on the same generic `alpha` gets `cprs_simd::run` for free. Tests: AD vs finite differences, then teqp
  or Clapeyron cross-checks (P10). CoolProp's 8.0.0 PC-SAFT oracle is unreliable (map 06 §6).
- Ice Ih (IAPWS R10-06) is Gibbs-explicit, so it does not fit `HelmholtzModel`. The planned additive change (D10 deferral trigger) is a new
  `GibbsModel` trait with six derivatives (g, g_T, g_p, g_TT, g_Tp, g_pp), `relations::gibbs`, `Phase::Solid(SolidId)` (allowed because
  `Phase` is `#[non_exhaustive]`), and a `Sides::Solid` variant (allowed because `State` fields are private). Once ice must coexist with water,
  `PhaseModel` and `Substance` arrive with min-Gibbs selection and aligned gauges, checked by R10-06 g = g_L at the triple point
  (materials-extensibility.md §3.3). The Helmholtz code, executors and registry are untouched. *Honest cost:* these are new modules in the kernel,
  not zero lines, but they edit no existing function, and the `LaneKernel` path gives ice batches lanes on day one.

**(d) A CoolProp bug is found and the literature overrides CoolProp.** Example: the Nitrogen reducing density (map 12 §6.3, map 10 §8.4).
1. The M3 paper fixture `paper/Nitrogen/Span2000.csv` (K2 rows, printed strings kept) fails in the `Paper` class, while the CoolProp 8.0.0
   fixture passes. Disagreement is expected, because the Rust code ingests the same JSON (map 10 §9).
2. Triage follows map 10 §8.5 rules 1-4. Self-check the paper first: does its table reproduce with its own R, M and reducing values (map 13)?
   If yes, find the cause in the data: ρ_r = 11183.901464580624 against 11183.9 published, the unit round-trip fixed upstream in 2acbbc82.
3. Record it: add `Override { id: "N2-rho-reducing", policy: UsePaper, source: "Span et al. JPCRD 29 (2000)", evidence: "oracle …; 2acbbc82,
   66859efb" }` to `cprs_verify::OVERRIDES` (already seeded in the sketch). The CoolProp rows it covers stop being asserted. Tolerances are never
   widened.
4. Fix it: datagen adds a cited entry to the CORRECTIONS section of the N2 blob. `Dataset::Corrected` (default) applies it;
   `Dataset::CoolProp800` keeps bit-exact parity for migration users. The SA for N2 is refit offline (66859efb), and the content hash changes,
   which changes `ModelKey`, so caches keyed on it are invalidated by construction (map 08 §9).
5. When the oracle pin moves to a release containing the fix, the entry becomes `ResolvedUpstream`. An algorithmic bug follows the same path with
   `SkipCoolProp` and an independent arbiter: the virial C example (exact δ → 0 Taylor coefficient against CoolProp's δ = 1e-12, map 12 R8),
   decided by mpmath on the JSON coefficients.

## 5. CoolProp rot designed out

| Rot (evidence) | How this design prevents it |
|---|---|
| Model, state, cache and workspace fused; sticky `specify_phase`; failed updates poison instances (map 12 R1; map 01 R12/R13; map 03 §6) | Pure `flash`; `PhaseHint` per call; `State` is an immutable value built only by flash; nothing persists between calls |
| Per-state deep copies: 6 fluid copies, 65-164 KiB, 39-59 µs construction (map 11 F11; map 01 R9) | One `Arc<FluidModel>` per fluid per registry; `State` = 232 B with a compile-time ≤ 256 B assert; `FluidRef` borrows |
| Eager whole-library load: 1.98 s, 37 MiB retained, 17.6 MiB duplicate strings (map 11 F13; map 01 R10) | Per-fluid `OnceLock`; SA and transport sections lazy; no retained JSON; derived SA data precomputed |
| Hot-path mutex (SA `get_invlnp`), global melting-caloric mutex, `deriv_counter` RMW (map 11 F7/F8; map 03 §6) | No shared writes; clippy `disallowed-types` in the kernel; derived data in datagen or in `OnceLock` |
| 38 global config keys, env-var numerics, config toggled mid-computation (map 12 R4; map 01 R15/R16) | `FlashOptions`/`LoadOptions` by value; the kernel reads no env or files; `Dataset` and `ExecPolicy` are values |
| Exceptions as control flow (237 `catch(...)`), `_HUGE` sentinels, swallowed batch errors (map 12 R3; map 11 F5/F6/F15) | `Result` everywhere; ordered strategy chain with the strategy name in `NoConvergence`; `Status` per point |
| Hand-coded derivatives per term, a test-only multicomplex copy, wrong high-order derivatives (map 12 R5; map 02 §3.3) | One exp-of-jets engine plus value-only AD; jets tested against `Dual`/`Jet` at L1 |
| Fat interface, capability discovered by exception (map 01 R21; map 12 R2) | `Capabilities` bitset checked before work; small traits |
| 8 different R values plus `NORMALIZE_GAS_CONSTANTS` (map 12 R7; map 06 §9) | `gas_constant()` is per-model data with `Source`; no global normalisation |
| Virials at δ = 1e-12, p(ρ=0) = 0·∞ (map 12 R8) | A_xy scaling keeps p finite at ρ → 0; exact Taylor path for δ = 0 (partitioned before `ln δ`) |
| Solver returns x_{n+1} with the state at x_n; unchecked `max_iter`; absolute tolerances (map 03 §6) | `Root{x, f@x, iters, status}`; `MaxIter` is an error; typed `Tol::{Rel, Abs, Log}` |
| Non-unique T+X answered silently as two-phase (map 03 §6) | `FlashError::MultipleSolutions` with the roots, plus an explicit `RootPolicy` |
| Enums renumbered while used as the C ABI (map 01 R1; map 12 R15) | No discriminant crosses an ABI; `cprs-capi` uses pinned append-only values tested against a golden list |
| Thread safety retro-fitted, racy tests (map 11 F10; map 10 §5) | Compile-time `Send + Sync + 'static` asserts; multi-threaded `cargo test`; Miri; bitwise thread-invariance tests |
| ECS reference built by name at first evaluation, per state and per property; hidden coupling (map 05 R6/R7; map 12 R12) | Declared dependency DAG in the index; cycles rejected at datagen; one shared reference `Arc`; `fluid-x` features enable references |
| Data drift: fossil h/s, rebuilt reducing densities, stale cubic snapshot (map 12 R10; map 09 §6) | Hash-gated datagen regenerated in CI; cited corrections overlay; derived values computed, never stored |
| Validation scattered: NaN Q segfault, Q = 5 accepted, output == input echo (map 12 R9; map 01 R7) | Validated newtypes at the edge; `Limits` checked in flash; no echo shortcut in `compat` |
| Platform FP and locale hazards (map 12 R18; map 11 F17) | Own `vmath` with no FMA, bitwise across targets; locale-free parsing; FP guard only in the C ABI |
| TTSE/BICUBIC crash and garbage bugs (map 08 §6) | Not ported; a future surrogate tier keys its tables on `ModelKey` |

## 6. First 10 TDD milestones (each ends green; fixtures in `cprs-verify`)

| # | Scope | Oracle / check values | Green gate |
|---|---|---|---|
| M0 | Workspace, lints, CI matrix (4 targets + wasip2 under wasmtime), `xtask check-deps`, `cprs-verify` reader, `xtask oracle` + `oracle.lock` | Fixture header round trip; oracle reproduces `PropsSI("H","T",300,"Q",1,"R134a") = 413265.6843372975` (map 11 §8) | All targets build; zero-dependency guard; zero-tests-ran fails CI |
| M1 | `num`: `Real`, `Lane`, `Lanes<W>`, `Dual`/`HyperDual`/`Jet<4>`, `powi`, `vmath` exp/ln/ln_1p/exp_m1 (msun port) | Bitwise against `libm` 0.2.16 (dev-dep) over 10⁸ random + edge inputs (subnormals, ±709.78, NaN, ±∞) | Bitwise; lanes = scalar |
| M2 | `eos`: Derivs, Vars, b_factors, Power/Gaussian/Generic/NonAnalytic blocks, IdealGas, `ad` | Hand derivation (sketch: A00 = 0.18901064400954748); AD ≤ 1e-13; CoolProp block-isolation fixtures, ~300 (τ, δ) per block, 17 block types (map 10 §9 U3), `Term` class; IAPWS-95 Table 6 at 500 K, 838.025 kg/m³: α⁰ = 2.047977334795937, α^r = −3.4269320568155854, α^r_δ = −0.36436665036388133, α^r_τ = −5.8140343523841596 (oracle = paper ≤ 2.9e-9; map 13) | 5-fluid set (Water, N2, CO₂, R134a, Propane) passes `Term` |
| M3 | `xtask datagen`, blob format, `cprs-data`, `Registry`, `FluidModel::decode`, corrections overlay | All 136 fluids decode; FNV `source_eos_hash` parity for 130 stamps; JSON → blob → model equals JSON-direct bitwise; N2 `UsePaper` override | 16-thread first-touch test: one decode per fluid |
| M4 | `relations`, `HelmholtzModel` for `FluidModel`, properties at (T, ρ) | Oracle DmolarT with imposed phase, `Prop` class (1e-12; 1e-8 near critical); IAPWS-95 K2 row (Water 500 K, 838.025 kg/m³: oracle p = 10 000 385.800921902 Pa; paper agrees ≤ 2.6e-9, map 13); Lemmon 2016 Table 7 (4 fluids, ≤ 4.3e-7); Maxwell and cp − cv identities | Identities hold to 1e-12 over grids |
| M5 | `solve` (TOMS748, bracketed Newton, Clenshaw), `SatData`, QT/PQ, T(ln p) inverse | `sa-coeff` 1e-14 against CoolProp SA at the same T; mp check points `sa-fit`; Water NBP in [373.124, 373.125] K (map 11 §8) | No mutex; QT ≤ 0.1 µs (bench recorded) |
| M6 | `flash` PT, DT; phase rule; domain errors; `State`; `FluidRef::flash` | Round trips from (p, T) truth grids 40×40 + (T, Q) 20×20 per fluid, `Flash` class (map 10 §8.5 L4); known-suspect list excluded (map 10 §8.4); walkthrough (a) value 112 654.89965464505 J/kg | Every grid point verified or a typed error |
| M7 | `exec`: `run_scalar`, `eval_batch` (PT, DT), `Plan`, `Status`, `rayon` `run_parallel`; first benchmark baseline | L6: batch = single calls bitwise; 1/2/N threads = sequential bitwise; same vs different fluid stress (N × M) | Zero allocations per point; perf baseline committed; thread scaling ≥ 0.9·N |
| M8 | `run_lanes`, `cprs-simd` (AVX2; NEON and simd128 builds), lane kernels for α^r, relations, SA | Lanes = scalar **bitwise** at every forced level; cross-platform result hash identical on Linux, Windows, wasip2 and aarch64 | **≥ 2.5× α^r on AVX2, or stop and re-plan** (K-phase-1 gate) |
| M9 | Remaining pairs: T+X (`MultipleSolutions`), DP, P+X with lockstep single-phase batches, Q-pairs, D+X, HS; pseudo-pure fluids | `Flash` round trips; T+X compressed liquid arbitrated by EOS root enumeration, not by the oracle (map 03 §6); PH ≤ 15 µs gate | All 19 pairs × 136 fluids grid-green or typed errors |
| M10 | `cprs` facade, `compat::props_si`, reference states (gauge), `cprs-capi` (tier: new ABI), `cprs-wasm` browser | PropsSI parity fixtures excluding R7/R12/R13 behaviour (map 01 §8); cbindgen header compiles as C99 `-pedantic-errors`; `test_wasm.mjs` port (map 11 §8); IIR/ASHRAE/NBP offsets against the oracle | v0.1 release candidate |

## 7. Risks and deliberate deferrals

| Risk | Mitigation / trigger |
|---|---|
| SIMD payoff below the estimate (3-3.5× AVX2 is unverified; K9) | M8 gate: below 2.5×, ship scalar and threads and re-plan; nothing else depends on lanes |
| Bell/Stirling B-factor form loses accuracy against CoolProp's recurrence at high t or l (cancellation) *(inference)* | Measured at M2 in the `Term` class with a condition-aware bound; fallback is CoolProp's recurrence order inside the same block API |
| Own `vmath` costs speed (msun uses one division) | M1 bench; a SLEEF-style polynomial stays generic, so scalar still equals SIMD (kernel-performance.md Q1) |
| Lockstep Newton for PH is complex and masks diverge | Limited to ρ(T,p) and the (T, ln ρ) Newton; scalar retry is always available; batch = scalar bitwise is the gate |
| `OnceLock` re-entrancy deadlock via ECS cycles | Cycles rejected at datagen plus a load-time test; references resolve in another fluid's cell |
| WASM memory when all fluids are embedded (data segments copied per instance) | `fluids-core` browser build plus fetched `Blob::Shared`; never all-136 as the only build |
| Const-generic `LaneKernel<NI, NO>` ergonomics for third parties | Small, documented; `HelmholtzModel::alphar_batch` default gives scalar batches without it |
| Ten workspace crates look heavy to reviewers | Four are libraries; the others are thin facades or unpublished tools; crates appear only at their milestone |
| Cross-platform bitwise claim untested until aarch64 and Windows runners exist | CI result-hash job at M8; until then the claim is labelled *inference* |

**Deferred on purpose:** mixtures (v0.3); transport and surface tension (v0.2); humid air, IF97, INCOMP, ice (each after a
deferral trigger); PC-SAFT and cubics; tabular/SBTL surrogates; `PhaseModel`/`Substance`; GPU (K16); WASM threads (nightly; K15); AVX-512 level and
across-terms SIMD (benchmark-gated); the FMA `Fast` policy; the Python binding; CoolPropLib.h tier B and 32-bit stdcall; plotting, isolines and
cycle helpers (**CoolPropPlot deferred**; it can be built later on `eval_batch`); REFPROP (external second oracle only).

## 8. Self-assessment against the rubric

| Criterion (weight) | Score /5 | Honest note |
|---|---|---|
| Modularity and extensibility (3) | 4 | New Helmholtz families need zero core edits (`HelmholtzModel`, AD bundle, `LaneKernel`). Solids need *additive* kernel modules (`GibbsModel`, `relations::gibbs`, new `Phase` and `Sides` variants), not zero lines |
| Simplicity (3) | 3 | **Weakest.** Few runtime concepts (Real, HelmholtzModel, LaneKernel, Registry, State, flash), but const-generic executors, a dispatch crate and ten workspace crates are more than a scalar-only design needs before M8 pays them back. Staging limits the cost |
| Concurrency and memory (3) | 5 | Lock-free registry, no shared writes, borrowed handles, 232-B states, lazy sections and shared ECS references, all compile-time asserted and thread-tested in the sketch |
| Performance (3) | 4 | Clear path: exp-of-jets, lanes bitwise = scalar, ISA dispatch, partitioned batches, fixed-chunk threads. Targets are estimates until M7/M8 |
| Correctness and TDD (3) | 4 | Oracle + paper + mp + self (L6) fixtures, divergence register, always-green milestones; the corrections overlay keeps CoolProp parity selectable |
| Idiomatic Rust, DRY/SOLID, deps, types (3) | 4 | Zero runtime deps; math written once; validated newtypes; `unsafe` in one module. Const-generic arrays of slices are slightly unusual |
| Rot elimination (2) | 5 | 19 systemic rot items mapped to structural fixes (§5) |
| Cross-platform (2) | 4 | All four targets plus wasip2 pass check and clippy in the sketch; bitwise cross-platform output depends on `vmath` (M1) and CI runners |
| Migration path (1) | 3 | `compat::props_si`, C ABI and an opt-in CoolPropLib.h shim arrive at M10; there is no Python binding in v0.1 |

Sketch evidence (observed on 2026-10-04): `cargo check` (Linux all-targets, wasm32-unknown-unknown, x86_64-pc-windows-msvc), `cargo clippy
--workspace --all-targets -D warnings` and `cargo test --workspace` (13 tests) all pass with zero warnings. The tests include the
hand-derived power term, the Dual-AD cross-check, lanes = scalar bitwise, every SIMD level = scalar bitwise (AVX2 path exercised), and 16-thread
single initialisation.
