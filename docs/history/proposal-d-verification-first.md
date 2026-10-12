# Proposal D: Verification-first

Type sketch (compiles on 4 targets, clippy and 15 tests clean):
`<session scratchpad>/sketch-verification-first`.
Citations: "map NN §X" = `docs/coolprop-map/NN-*.md`; research codes (R5, K8, P2, S1...) = `docs/research/*.md`
summary tables. Anything else is labelled *(inference)*.

## 0. Thesis

The scalar f64 reference path is the specification. Every layer exposes a typed seam where an independent check attaches:
AD against hand-derived jets, identities against relations, printed tables against the EOS, multiprecision points against
saturation, round trips against flashes, and the reference policy against every accelerated policy. CoolProp 8.0.0 is a
provisional oracle and printed check tables arbitrate. Every deliberate difference is a typed, cited, proof-tested entry in a
compiled divergence register. Declared capabilities grow milestone by milestone, so every milestone stays green.

## 1. Layers and crate layout

```text
 facades (thin)     cprs-py (after 0.1)     cprs-wasm            cprs-capi
                           \                    |                    /
                            +--------------- cprs-compat -----------+   CoolProp names, FluidSpec, fill policies
                                                |
 kernel             cprs-core   std only, #![forbid(unsafe)], one crate, layers = modules, deps point down
   L6 exec          batch        ExecPolicy, Status per cell        <- differential: every policy == Reference
   L5 transport     transport    uses only the public Fluid/State API <- paper rows, stage by stage
   L4 solve         flash, saturation, state  Root, SolvePath, Ambiguous <- round trips, mp points, capability matrix
   L3 package       data, fluid, registry  FluidRecord -> Arc<FluidData> <- L0 parse, FNV-1a hash, patch proofs
   L2 model         helmholtz, model  closed term set, 2 traits     <- term fixtures, AD vs jets, K1 tables
   L1 relations     relations    Bundle -> p, h, s, cp, w            <- identities on synthetic bundles
   L0 numerics      num, units   Real, HyperDual, math, roots, SI newtypes <- unit tests
                                                |
 data               cprs-data    generated LE blobs + name index; features = fluid sets
 dev / tools        cprs-verify  kit (lib) + corpus (tests/) + fixtures/ + benches/   -> cprs-core
                    cprs-xtask   datagen, oracle driver, manifest/register/zero-dep/test-count gates -> cprs-core[json]
```

| Crate | Responsibility | Depends on | Features | Ships |
|---|---|---|---|---|
| `cprs-core` | Kernel L0-L6: numerics, relations, EOS terms, model traits, records, registry, flash, saturation, transport, batch | `cprs-data` (optional) | `fluids-all` (default), `fluids-core10`, `embedded`, `rayon`, `libm`, `json` | crates.io |
| `cprs-data` | Generated per-fluid blobs (v8.0.0 parity data) + static index; CoolProp MIT notice | none | `all`, `core10`, `water`, ... (additive sets) | crates.io |
| `cprs-compat` | `props_si`-style strings, CoolProp names/aliases, strict `FluidSpec`, `FillPolicy`, `Session` | core | none | crates.io |
| `cprs-capi` | `extern "C"` ABI, opaque handles, thread-local last error, optional CoolPropLib.h shim | compat | `coolproplib-shim` | cdylib/staticlib + header |
| `cprs-wasm` | wasm-bindgen browser API (baseline and `+simd128` builds); WASI packaging later | core, compat | `browser`, `wasi` | npm |
| `cprs-py` | pyo3 + maturin abi3 (after 0.1) | compat | `numpy` | PyPI |
| `cprs-verify` | Zero-dependency verification kit + the conformance corpus + fixtures + benches | core | none (dev-deps: proptest, criterion) | crates.io once stable |
| `cprs-xtask` | Datagen (JSON to blobs), oracle orchestration, CI gates | core[`json`] | none | never |

Eight crates, of which three are libraries with logic, three are thin facades and two are dev/tool crates. The kernel is
one crate because its layers form one algorithm chain. Splitting them costs semver and compile overhead and buys no
second consumer (research kernel-performance §3.1: "Start as modules of one crate and split them only when a second
consumer appears").

## 2. Core types and traits (excerpts, in sync with the sketch)

### 2.1 Test seams in the types

| Layer | Seam (type) | Independent check | Arbiter / oracle | Class (map 10 §8.3) |
|---|---|---|---|---|
| L0 | `units::{Quality, Temperature, ...}::new -> Result` | NaN, Q=5 and T<0 unrepresentable | map 12 R9, map 01 R7 | exact |
| L0 | `num::Real`, `HyperDual`, `math::powi` | AD identities; bit-exact integer powers | itself | exact |
| L1 | `relations::Bundle` (plain data) + pure fns | Ideal-gas limit, h=u+p/rho, cp-cv, Maxwell on synthetic bundles | identities | 1e-14 |
| L2 | `PowerBlock::value::<R>` vs `accumulate` | Jets vs hyper-dual AD at random (tau, delta) | AD of the paper formula | term |
| L2 | `HelmholtzModel` at (T, rho) -> reducing-invariant `Derivs` | FD vs analytic for ANY model (`conformance::fd_first_order`) | identities | FD 1e-7 |
| L2 | per-block `ResidualTerms::power()` etc. | Oracle block isolation via `add_fluids_as_JSON` (map 10 §8.2) | CoolProp 8.0.0 | term |
| L3 | `data::FluidRecord` (mutable, plain) -> `FluidData::compile` | Swap one constant for the paper's value and re-run the paper's table | map 13 §3 protocol | paper |
| L3 | `DataSet::{Corrected, Parity}` + `CORRECTIONS` | Parity matches oracle; Corrected matches paper; the difference is exactly the patch | register proof tests | paper / prop |
| L4 | `num::roots::Root{x, f, iterations, status}` | Residual at returned x; exhaustion is a status | map 03 §6 | flash |
| L4 | `State::path() -> SolvePath{strategy, iterations}` | Tests assert WHICH strategy ran | map 12 R3 | exact |
| L4 | `Capabilities` (declared) + `FlashError::Unsupported` | 19 pairs x 2 bases matrix: declared pairs must round-trip, undeclared must refuse | map 01 R3/R4/R21 | flash |
| L4 | `FlashError::Ambiguous{roots}` + `RootPolicy` | Root enumeration on both branches | EOS / literature, not oracle (map 03) | flash |
| L4 | `SatPoint.source: SatSource` | Superancillary vs VLE vs mp points | P-mp (map 10 §8.1) | sa-coeff / sat-mp |
| L5 | `transport::viscosity_breakdown -> Breakdown{Option...}` | Stage-by-stage | paper rows, then oracle | tr-direct / tr-ecs |
| L6 | `ExecPolicy::Reference` vs others | Bitwise for lanes and threads; bound for reordered sums | Reference path | exact |
| all | `Gauge` applied at the boundary only | Gauge invariance properties (map 15 §8) | identities | exact shift |

### 2.2 Numerics and the term kernel (D2)

```rust
pub trait Real: Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self>
    + Neg<Output = Self> + Add<f64, Output = Self> + Mul<f64, Output = Self> {
    type Mask: Copy;                                  // bool for scalars, lane mask for SIMD (D9)
    fn from_f64(x: f64) -> Self;
    fn exp(self) -> Self;  fn ln(self) -> Self;       // via num::math: one algorithm for every impl
    fn powi(self, n: i32) -> Self;                    // fixed multiplication chain, never powi intrinsic
    fn powf(self, y: f64) -> Self;
    fn lt(self, rhs: Self) -> Self::Mask;  fn select(mask: Self::Mask, a: Self, b: Self) -> Self;
}   // impls: f64 (reference), HyperDual (AD), later lane types in a side-by-side crate

/// A_ij = tau^i delta^j d^(i+j)alpha/dtau^i ddelta^j, i+j <= 4: reducing-INVARIANT.
pub struct Derivs { a: [[f64; 5]; 5], order: Order }
impl Derivs { pub fn get(&self, i: usize, j: usize) -> Option<f64> /* None above the computed order */ }

impl PowerBlock {
    /// The paper formula = the specification; with R = HyperDual it is the AD test oracle.
    pub fn value<R: Real>(&self, tau: R, delta: R) -> R { /* sum n tau^t delta^d exp(-delta^l) */ }
    /// Fast path: one exp per term + univariate log-jets -> Bell polynomials -> Stirling numbers.
    pub fn accumulate(&self, tau: f64, delta: f64, out: &mut Derivs) {
        let (ln_tau, ln_delta) = (math::ln(tau), math::ln(delta));
        for (((&n, &t), &d), &l) in self.n.iter().zip(&self.t[..]).zip(&self.d[..]).zip(&self.l[..]) {
            let (df, lf) = (f64::from(d), f64::from(l));
            let x = if l > 0 { math::powi(delta, l) } else { 0.0 };
            let e = n * math::exp(t * ln_tau + df * ln_delta - x);
            let lx = lf * x;  let l2x = lf * lx;  let l3x = lf * l2x;
            let bd = scaled_from_log_jet([df - lx, -l2x, -l3x, -lf * l3x]);  // d/du of d*u - e^(l u)
            out.add_outer(e, &power_law_jet(t), &bd);
        }
    }
}
```

The unit test checks one term (n=0.5, t=1.5, d=2, l=1 at tau=2, delta=0.5) against hand-derived values. alpha =
0.21444097124017672, and the delta-scaled factors are 1.5, 0.25, -1.625 and 2.0625. All 15 `A_ij` agree to 4e-16. A corpus
test checks a 5-term block against `value::<HyperDual>` at 300 random points (class `term`, scaled 1e-13).

### 2.3 Model contracts (D3, D4, D10)

```rust
pub trait HelmholtzModel: Send + Sync {                          // object-safe; any family implements it
    fn gas_constant(&self) -> f64;                               // explicit per model (map 12 R7)
    fn residual(&self, t: f64, rho_molar: f64, order: Order) -> Derivs;
    fn ideal(&self, t: f64, rho_molar: f64, order: Order) -> Derivs;
    fn residual_batch(&self, t: &[f64], rho: &[f64], order: Order, out: &mut [Derivs]) { /* loop */ }
}
pub trait PureFluidModel: HelmholtzModel {
    fn info(&self) -> &FluidInfo;  fn molar_mass(&self) -> f64;  fn limits(&self) -> &Limits;
    fn published_critical(&self) -> Option<CriticalPoint>;        // paper metadata
    fn eos_critical(&self) -> Result<CriticalPoint, SatError>;    // this EOS; never config-switched
    fn saturation(&self) -> Option<&dyn SaturationCurve>;         // superancillary, or None
    fn capabilities(&self) -> Capabilities;                       // declared, grows per milestone
    fn gauge(&self) -> Gauge;
    fn viscosity(&self, s: &State) -> Result<f64, PropError> { Err(PropError::NoModel { prop: "viscosity" }) }
    fn conductivity(&self, s: &State) -> Result<f64, PropError> { /* same default */ }
}
const IMPLEMENTED: Capabilities = Capabilities::none().with(InputKind::QT);  // in fluid.rs: grows M6 -> M7
```

### 2.4 Data, registry, state, flash, batch (D5-D8)

```rust
pub struct FluidRecord { pub info: FluidInfo, pub eos: EosRecord, pub melting: Vec<MeltingSegment>, pub applied: Vec<&'static str> }
pub enum DataSet { Corrected /* default */, Parity /* bit-exact v8.0.0 for oracle fixtures */ }
pub static CORRECTIONS: &[Patch] = &[
    Patch { divergence: "DIV-0001", fluid: "R1234ze(E)", edit: Edit::GasConstant(8.314_462_1) },
    Patch { divergence: "DIV-0002", fluid: "Water", edit: Edit::MeltingP0 { segment: 2, p0: 632.4e6 } },
    Patch { divergence: "DIV-0003", fluid: "Nitrogen", edit: Edit::ReducingDensity(11_183.9) },
];
pub struct Registry { entries: Box<[Entry]>, options: RegistryOptions }   // Entry.cell: OnceLock<Result<Arc<FluidData>, LoadError>>
impl Registry {
    pub fn embedded() -> &'static Registry;                     // LazyLock over the static index only
    pub fn from_embedded(o: RegistryOptions) -> Registry;       // e.g. DataSet::Parity for fixtures
    pub fn from_bytes(pack: Arc<[u8]>, o: RegistryOptions) -> Result<Registry, LoadError>;  // WASM fetch
    pub fn get(&self, name: &str) -> Result<Fluid, LoadError>;  // decodes this fluid once, then atomic loads
    pub fn preload(&self, ids: &[FluidId]) -> Result<(), LoadError>;
    pub fn loaded(&self) -> impl Iterator<Item = &str>;
}
#[derive(Clone)] pub struct Fluid { data: Arc<FluidData>, gauge: Gauge }   // handle; FluidData immutable
#[derive(Clone, Copy)] pub struct State { t, rho, p, phase: Phase, body: Body /* Single(Bundle) | TwoPhase{q, liquid, vapour} */,
    r, molar_mass, gauge: Gauge, path: SolvePath }               // <= 256 B, no Arc, private fields
pub fn flash<M: PureFluidModel + ?Sized>(model: &M, input: Input, opts: &FlashOptions) -> Result<State, FlashError>;
pub struct FlashOptions { pub phase_hint: Option<PhaseHint>, pub roots: RootPolicy, pub domain: DomainPolicy,
    pub solve: SolveOptions, pub guess: Option<(f64, f64)> }    // #[non_exhaustive], by value, no globals
pub enum FlashError { Quantity(..), Unsupported { pair }, Domain(..), NoConvergence { strategy, root },
    Ambiguous { roots: Roots }, Saturation(..), Load(..) }
pub fn evaluate_batch<M: PureFluidModel + ?Sized>(model: &M, pair: InputKind, x: &[f64], y: &[f64],
    outputs: &[Prop], out: &mut [f64] /* column-major */, status: &mut [Status] /* per cell */,
    opts: &BatchOptions /* exec: ExecPolicy, basis, flash */) -> Result<BatchSummary, BatchError>;
const _: () = { const fn assert_shared<T: ?Sized + Send + Sync + 'static>() {}
    assert_shared::<FluidData>(); assert_shared::<Fluid>(); assert_shared::<Registry>(); assert_shared::<State>();
    assert_shared::<PureEos>(); assert_shared::<dyn PureFluidModel>(); assert_shared::<dyn SaturationCurve>(); };
const _: () = assert!(core::mem::size_of::<State>() <= 256);
```

### 2.5 Verification kit (D13)

```rust
pub enum Provenance { Paper { citation, table, row }, Iapws { release, table }, MultiPrecision { source },
    Oracle { version }, OtherImplementation { name }, SelfReferential, Identity }   // is_arbiter(): first three only
pub enum ToleranceClass { Exact, Term, Prop, SaCoeff, SatMp, Flash, TransportDirect, TransportEcs, Paper, Smoke }
pub fn from_printed(printed: &str) -> Option<Tolerance>;      // "21.17909" -> Absolute(5e-6)
pub struct Divergence { pub id, pub fluid, pub part: Part, pub oracle, pub arbiter: Option<Citation>,
    pub policy: Policy /* UsePaper | SkipOracle | Investigate */, pub evidence, pub status: Status }
pub static DIVERGENCES: &[Divergence] = &[ /* DIV-0001..0005: R1234ze(E) R, Water ice VI, N2 rho_r, two-phase transport, Helium */ ];
pub fn fd_first_order<M: HelmholtzModel + ?Sized>(m: &M, t: f64, rho: f64, step: f64, tol: f64) -> Result<(), Mismatch>;
pub fn gauge_invariance<M: PureFluidModel + ?Sized>(native: &M, shifted: &M, pts: &[(f64, f64)]) -> Result<(), Mismatch>;
pub fn capability_matrix<M: PureFluidModel + ?Sized>(m: &M, truth: &[(f64, f64)]) -> Vec<(InputKind, bool)>;
pub fn policy_equivalence<M: PureFluidModel + ?Sized>(m: &M, pair: InputKind, x: &[f64], y: &[f64],
    outs: &[Prop], candidate: ExecPolicy, base: &BatchOptions) -> Result<(), (usize, f64, f64)>;
```

## 3. Decisions D1-D17

**D1 Workspace, crates, features.** *Decision:* the 8 crates of §1. The kernel is one crate with module layers. Features
only add capabilities: fluid sets, `embedded`, `rayon`, `libm`, `json`. *Why:* the boundaries that matter are dependency
tiers and release cadences. Std-only core (research R1). Separately versioned data with its own licence notice (map 09).
Legacy policies outside the kernel (map 14). Binding crates kept away from core (research T2). Dev-deps and fixtures kept
out of core's graph: `cprs-verify` depends on core, never the reverse, so there are no dev-dependency cycles and no
duplicate-crate type clashes in tests *(inference)*. *Rejected:* one crate per layer (about 10 crates for one call chain;
semver churn); one crate including data (no runtime-loading split, mixed licence, WASM size); a separate transport crate
(`Fluid` would then hold trait objects; the module already uses only public API, materials S5).

**D2 Numeric core and derivatives.** *Decision:* the public API and the kernel use f64. Each term family is written once as
generic `value::<R: Real>` (the paper formula). The hot path is hand-derived univariate log-jets in tau and delta: one
`exp` per term, Bell-polynomial and Stirling conversion, `Want`-style `Order` mask (research R5a, map 02 §3.3).
`HyperDual` (now) and `Jet4` (M3) live in `cprs-core::num`. They are production code for non-separable terms
(NonAnalytic, future PC-SAFT) and the test oracle for the jets. *Why:* CoolProp hand-codes 20 outputs per term plus a
test-only multicomplex copy (map 12 R5, map 10 R7). Generic differentiation reproduced CoolProp's hand-coded values to
<= 3.4e-13 (map 02 §3.3). A 3-line paper formula is reviewable against the paper; 20 hand derivatives are not.
*Rejected:* AD on the hot path (nested duals cost more, research §2.4); num-dual in core (orphan rule against SIMD lanes,
research R5); `std::autodiff` (nightly, no MSVC/WASM); a generic numeric type in the public API (P2/P3).

**D3 Model representation.** *Decision:* a closed set of term kinds, compiled into per-family SoA blocks with a
match-free hot loop. Unknown kinds are a `LoadError`. Model families plug in through two object-safe traits,
`HelmholtzModel` and `PureFluidModel`. Flash and batch are generic (monomorphised), and facades may hold
`Arc<dyn PureFluidModel>`. *Why:* six separable kinds cover 99.8% of default terms (map 02). CoolProp silently skips
unknown ideal blocks (map 10 R8, map 09 R7). New families must not edit core (map 12 R13, rubric). *Rejected:* a trait
object per term (vtable per term per evaluation, CoolProp's virtual `all()`); generics over term types (fluids are data,
not types); open term plug-ins (unvalidatable data).

**D4 Pure vs mixture.** *Decision:* pure fluids first. Composition is fixed inside a model instance (one `Arc` per
trimmed composition), so the pure path pays no mixture cost and `HelmholtzModel` already fits a `Mixture` later.
Pseudo-pure fluids are `PureFluidModel`s with `SatSource::Ancillary`. Mixtures land after 0.1 as one more implementor plus
a `MixtureModel` trait (composition derivatives by one mole-number AD path, map 04). *Why:* about 6 fluid copies per pure
state and about 100 pure/mixture branch sites in CoolProp (map 04). Zero fractions must be trimmed at build time (map 04,
c7d6a1aa). *Verification:* invariant tests that `x = [1, 0]` equals the pure fluid under any gauge (map 04, map 15 G1);
GERG golden vectors from teqp (map 06, P10). *Rejected:* composition in `Input`/`State` (every pure call carries a vector;
cache keys unclear, map 01 Q1).

**D5 State and properties.** *Decision:* `State` is a `Copy` value of at most 256 B, asserted at compile time. It holds T,
rho, p, phase, an order-2 bundle (two for two-phase), R, M, gauge and `SolvePath`, with private fields and getters. The
bundle is computed eagerly at the end of the flash. Properties are relations on demand. Order 3 and above come from the
model on demand. Inputs are a 19-variant `Input` whose fields carry their basis; `to_molar(M)` normalises once. Outputs
use typed getters plus `Prop` for dynamic and batch paths. Units are `#[repr(transparent)]` SI newtypes at the edge
(research R11). *Why:* map 01 U1-U6, research K1/P4; the flash evaluates the bundle anyway *(inference)*. A `OnceCell`
cache would end `Copy` and add interior mutability (map 01 Q2). *Rejected:* `Arc` in `State` (refcount traffic, K1); a
CoolProp-style cached mutable state (map 01 R11-R13); uom in core (generic leakage, R11).

**D6 Flash.** *Decision:* the flash is a pure `flash(&model, Input, &FlashOptions)`. Each pair has a fixed ordered list
of strategies, each returning `Result`. The winner is recorded in `State::path`. Every solver returns
`Root{x, f, iterations, status}` with typed `Stop` rules (relative, absolute or log-axis). Superancillary first for
Q-pairs and phase boundaries, VLE second. Non-unique pairs return `Ambiguous{roots}` under the default
`RootPolicy::Strict`. Before any root is returned, the acceptance gate checks that it reproduces the inputs, lies in
[Tmin, Tmax], and has dp/drho > 0 and cv > 0 (map 10 §8.5 L4). *Why:* sticky phase imposition (map 01 R12, map 03 §6),
237 `catch(...)` and `_HUGE` sentinels (map 12 R3), unchecked TOMS748 exhaustion, and a 6.1% density error from absolute
tolerances (map 03 §6). T+H in compressed liquid returns a wrong two-phase answer in CoolProp, so EOS root enumeration
arbitrates (map 03). *Rejected:* a mutable state with `update()` (CoolProp R1); exception-style fallback cascades; global
`DONT_CHECK_PROPERTY_LIMITS` (replaced by per-call `DomainPolicy`).

**D7 Data pipeline.** *Decision:* `xtask datagen` reads v8.0.0 JSON. It uses a literal-kind-preserving reader with a
logged Chlorine duplicate-key waiver, recomputes the FNV-1a `source_eos_hash` and resolves citations. It writes a
versioned LE blob per fluid plus a committed static index (case-folded names, aliases and CAS; 556 keys, no collisions,
map 09). At run time, `decode -> FluidRecord -> apply_corrections(DataSet) -> FluidData::compile -> Arc`, one `OnceLock`
per fluid. The superancillary and transport decode lazily in inner `OnceLock`s. `Registry::from_bytes` handles runtime
packs, and the `json` feature imports user JSON. ECS references are `FluidId`s, and datagen proves the dependency graph is
acyclic (map 12 R12). A patch that changes EOS constants changes the EOS hash, so that fluid's v8.0.0 superancillary is
marked stale and saturation uses VLE until a refit lands (the hash gate, map 09). *Why:* 2.81 MiB raw for 136 fluids;
compression gains only 13-19% (map 09); eager load costs 1.8-2 s (map 12, map 14). *Rejected:* runtime JSON (dependency,
speed), CBOR/bincode (R7), build.rs codegen (cost to adopters, R6), compressed blobs (R8).

**D8 Concurrency kernel.** *Decision:* the registry has an immutable index and a `OnceLock<Result<Arc<FluidData>>>` per
fluid. A failed load is cached, because data are immutable, so there are no retry storms. `Fluid` is an `Arc` plus a
`Gauge`; `State` is `Copy` without an `Arc`. The hot path has no locks: clippy `disallowed-types` bans
`Mutex`/`RwLock`/`RefCell`/`Cell`, and `disallowed-methods` bans `std::env::var` and `f64::mul_add`. The batch API takes
caller-owned SoA slices with a status per cell and allocates nothing. `rayon` is optional with fixed chunks; warm starts
are opt-in. Send + Sync + 'static is asserted at compile time. *Why:* map 11 F7-F11, research K1-K7. CoolProp needed RAII
guards and still had order dependence under parallel tests (map 10 §7). *Rejected:* `RwLock<HashMap>` caches, `Weak`
eviction, `thread_local!` scratch (K2/K4), and per-thread model copies (CoolProp).

**D9 Execution strategies.** *Decision:* "what" is L1-L5; "how" is `ExecPolicy { Reference, Auto, Parallel{chunk} }`,
chosen at run time (K13). `Reference` is the scalar f64 path. Lane types arrive in a separate `cprs-simd` crate (M11,
research T1b) that implements `Real` for lanes and overrides `HelmholtzModel::residual_batch`. Dispatch is detected once
into a `OnceLock` function table on native targets; wasm `simd128` is chosen at compile time with two builds (K11, K15).
*Qualifies:* term sums, relations, superancillary Clenshaw, ancillaries, direct transport, and a lockstep rho(T,p) with
masks plus a scalar retry. *Never:* phase determination, VLE, the HS cascade, the ECS conformal solve, mixture flashes
(K14, map 03 §7). *Verification:* `policy_equivalence` is bitwise for lanes-across-states and threads, and uses a
summation bound for across-terms or a later FMA "Fast" policy (K17, map 10 R16). Each accelerated kernel must also pass
the K1/K2 paper rows (map 13 §7). *Rejected:* `std::simd` (nightly), per-ISA code without a scalar reference, and
features that change numerics.

**D10 Material and states-of-matter seams.** *Build now:* reducing-invariant `Bundle` relations (materials S1); identity,
model and package split with `Substance`/`Material` reserved (S2); `Gauge` as a value (S3, map 15); `DomainError`
including `BelowMeltingLine`, with private `Limits` (S4); read-only `State` (S5); `#[non_exhaustive]` on `Phase`,
`Prop`, `InputKind` and errors (S9); selection kept apart from evaluation (S10); `Source` provenance (S8); and the generic
conformance kit, so a new family inherits its checks. *Deferred, with triggers (materials §3.2):* the `GibbsModel` trait
and `relations::gibbs` come with ice Ih; `PhaseModel`/`Substance` when a second family must coexist with a fluid; domains
with holes; region maps; min-Gibbs selection; the `correlation` module (INCOMP); solid EOS, plasma, CALPHAD and tensors.
*Rejected:* a `Material` abstraction now (one implementation is a guess, R1).

**D11 Facades.** *Decision:* the Rust API lives in core. The PropsSI-style API and legacy policies (inf fill,
HAPropsSI raise-first) live in `cprs-compat`. The C ABI lives in `cprs-capi`: fixed-width ints, a length for every
buffer, `catch_unwind` at every export, generational `u64` handles, thread-local last error, string keys rather than enum
integers, and an optional CoolPropLib.h shim (map 11 F1-F5, F16, map 01 R1). The browser and WASI facade is `cprs-wasm`;
Python comes after 0.1. *Why:* map 14 says legacy behaviours belong in the facade. *Rejected:* reproducing CoolProp
defects for parity (map 01 Q3); uniffi/safer-ffi (R15).

**D12 Errors, panics, NaN, determinism, tolerances.** *Decision:* hand-written `#[non_exhaustive]` error enums per layer
plus an umbrella `Error`; Display renders structured fields. No panic on user input: `unwrap_used` and `expect_used` are
denied, internal invariants use `debug_assert`, and `todo!` exists only in unreachable-by-capability code. NaN: inputs are
validated at construction; batch cells hold NaN only with a non-Ok `Status`; compat may map that to +inf. Determinism: no
FMA in the reference path, fixed summation order, all transcendentals through `num::math`, integer powers by a fixed
multiplication chain, and the `libm` feature for bit-identity across targets. Fixtures are generated on x86-64 Linux
only (K18). CI compares an FNV-1a hash of a canonical output grid on Linux, Windows and wasip2 with `libm` on. Without
`libm`, cross-platform comparisons use the `term`/`prop` classes. Tolerances are the map 10 classes, or half a unit in the
last printed digit for paper values; never widen one to fit CoolProp (map 10 R4/R5). *Rejected:* NaN-as-error mode in core
(P9); global error outbox (map 01 R22).

**D13 Verification architecture.** *Decision:*
- *Oracle generator:* `scripts/oracle/gen.py` under `uv run --no-project --python 3.12 --with CoolProp==8.0.0`. One fresh
  `AbstractState` per case, `COOLPROP_*`/`PXFLASH_*` scrubbed, `LC_ALL=C`, all 38 config keys recorded, fork pool,
  `oracle.lock` with the wheel's `.so` sha256 (map 10 §8.3, map 01 §8).
- *Fixtures:* `crates/cprs-verify/fixtures/{coolprop-8.0.0/<kind>, paper/<fluid>/<bibkey>.csv, mp/<fluid>.csv,
  MANIFEST.sha256}`. CSV with a `#` header, floats as Python repr (bit-exact into Rust), and a `status` column. The kinds
  are term, eos, sat, flash, transport, crit and gauge. A core subset of about 10 fluids x 500 rows per kind (single-digit
  MB) is committed; the full set (~3.5 GB raw) is regenerated nightly with a manifest drift gate (map 10 §8.3). Test
  binaries embed fixtures with `include_str!`, so the corpus runs on wasm32-wasip2 under wasmtime without filesystem
  mapping *(inference)*.
- *Arbiters:* `ARBITERS` records each paper's stated R, M and reducing constants. A table is first checked against
  itself, which NIST IR 8474 fails (map 13 R3). Transcription is double-entry with printed strings kept.
- *Divergence register:* compiled `DIVERGENCES`, cross-checked by id with `CORRECTIONS`. Each entry has a three-part proof
  test (§4d). When the oracle pin moves, entries the oracle now agrees with fail until marked
  `ResolvedUpstream{commit}` (map 10 §8.5 item 4).
- *Property tests:* identities (map 01 §8 list), gauge invariance (map 15 §8), FD vs analytic, round trips from (p,T) and
  (T,Q) truth, thread-count invariance, the capability matrix, and later the mixture x->1 limit. proptest (std only) adds
  shrinking on top of `SplitMix64` grids.
- *Differential tests:* jets vs AD; superancillary vs VLE; every policy vs `Reference`.
- *Benchmarks and perf gates:* criterion and gungraun in `cprs-verify/benches/` (R18). The research §3.4 targets are
  recorded at M9, and gungraun instruction-count regressions gate Linux CI after that. A C++-level CoolProp baseline is
  built in scratch (K19).
- *CI gates:* zero tests run is a failure (executed count >= manifest, map 10 R1); every `#[ignore]` needs a register or
  issue id and is reported; nightly `--ignored` sweeps with a published report (map 10 R2).

*Rejected:* TOML overrides (parser dependency, untyped); oracle-only TDD (oracle errata, map 12 §6.3); REFPROP as an
oracle (proprietary, process-global; map 07).

**D14 Licensing.** Code is MIT OR Apache-2.0 (R20). `cprs-data` ships CoolProp's MIT notice. Every model carries a `Source`
with `DataTerms`, and `Unpublished` sources (Propylene, SES36, Neon; map 13 R7) get labelled oracle-only fixtures.
CoolProp-generated fixtures are MIT. Committing printed check values and fastchebpure outputs waits for user clearance
(map 10 Q1, map 13 Q6). No GPL/LGPL/MPL code is copied (SeaFreeze, BurnMan, GSW-C is oracle only; materials). NIST-notice
code keeps its notice (P13). Tooling: cargo-deny with an allow-list and bans (bincode, serde_cbor), REUSE 3.3, and
cargo-about for bundles (R19).

**D15 Milestone order and first release.** M1-M10 in §6, each green. *0.1 = M10:* all 136 v8.0.0 pure and pseudo-pure
HEOS fluids, all 19 pairs, transport and surface tension where models exist, melting lines, reference states, batch with
`rayon`, the compat API, the C ABI and the WASM browser build. After 0.1: M11 SIMD lanes, M12 mixtures, M13 ice Ih +
IF97, M14 cubic + PC-SAFT, M15 Python, then INCOMP and humid air. Plotting, isolines and cycle helpers (CoolPropPlot) are
deferred (digest coverage note). TTSE/BICUBIC are dropped (map 08).

**D16 Naming.** The placeholder prefix is `cprs-`; the final name is the user's call. Constraints *(inference)*: available
on crates.io, npm and PyPI; does not imply CoolProp endorsement; short enough for C symbol prefixes (`cprs_`).

**D17 Edition, MSRV, tooling, CI, lints, unsafe.** Edition 2024, resolver 3. MSRV 1.85 for core, data, compat and verify;
1.89 for the future SIMD crate; facades follow their binding crate (R3). CI: test on x86_64 Linux and Windows MSVC and on
`wasm32-wasip2` under wasmtime (the whole corpus); build `wasm32-unknown-unknown` baseline and `+simd128`; aarch64 test;
MSRV `check --lib`; clippy `-D warnings`; rustfmt; cargo-deny; zero-dependency guard; executed-test-count guard; weekly
latest-dependencies job; cargo-semver-checks before release. Lints: workspace `unsafe_code = "forbid"`, `missing_docs`,
`unwrap_used`/`expect_used` denied, `dbg_macro`, and `disallowed-types`/`disallowed-methods` (D8, D12). Unsafe:
`cprs-capi` overrides to `deny` plus a per-item allow with a SAFETY comment. `cprs-simd` starts with
`forbid(unsafe_code)` on fearless_simd and allows raw `std::arch` paths only by exception (T1b).

## 4. Walkthroughs

**(a) One PT -> h call for Water** (compiled as a `no_run` doctest in `cprs-core/src/lib.rs`):
```rust
let water = Registry::embedded().get("Water")?;
let state = water.flash(Input::PT { p: Pressure::new(101_325.0)?, t: Temperature::new(300.0)? }, &FlashOptions::default())?;
let h = state.enthalpy(Basis::Mass);   let path = state.path();
```
1. `Registry::embedded()` builds the static index once and decodes nothing. `get` case-folds and resolves "Water" to a
   `FluidId`. The first call decodes Water's blob only (~22 KiB, map 09), applies `CORRECTIONS` (DIV-0002 melting p0) and
   compiles into `Arc<FluidData>`. Later calls are an atomic load. *Seam:* L0 data tests, patch proofs.
2. `Pressure::new` / `Temperature::new` reject non-finite or non-positive values. `flash` checks `capabilities()` for PT
   (or returns `Unsupported`) and normalises to molar.
3. Domain: T within [Tmin, Tmax], p <= pmax, melting line. *Seam:* `DomainError` tests, e.g. Water at 250 K must fail
   where CoolProp returns p = -5.928 Pa (map 12 §6.3).
4. Phase determination: the superancillary (inner `OnceLock`, decoded on first saturation use) gives psat(300 K); p > psat,
   so liquid. *Seam:* `SatPoint.source`, sa-coeff and sat-mp fixtures.
5. Density: `newton_bracketed` on rho(T, p), bracketed by rho'(T), using residual `Order::Two` delta-derivatives. It returns
   `Root{status: Converged}`, and the acceptance gate checks p reproduces, dp/drho > 0 and cv > 0. *Seam:* `Root`, flash
   round-trip fixtures.
6. `State::single_phase` evaluates the ideal and residual `Derivs` once into a `Bundle`. `enthalpy` is
   `relations::enthalpy + gauge.dh`, divided by M. `path()` = `DensityNewton`, n iterations. *Seams:* K1 (IAPWS-95 alpha table at
   500 K and 838.025 kg/m3; oracle agreement <= 2.9e-9, map 13 §3), K2 rows, identities, gauge invariance.

**(b) 1,000,000 PH flashes over 3 fluids from 16 threads at once.** Each of the 16 request threads holds a `Fluid` (an
`Arc` clone made once per handle) and calls `evaluate_batch(&fluid, InputKind::HP, &h, &p, &[Prop::T, Prop::D], out,
status, &opts)` with caller-owned slices. Concurrent first users of one fluid block once on its `OnceLock`; the other two
fluids initialise independently. After that no request touches shared mutable memory: models are immutable, `State` and
solver scratch live on the stack, nothing allocates per point, and there are no locks to contend. Memory is 3 x (model
<= 25 KiB + superancillary <= 25 KiB), shared by all threads, with no per-thread copies (target, research §3.4). PH stays
scalar per point because it is branchy (K14). With `rayon`, a single big request can use
`ExecPolicy::Parallel{chunk: 1024}`; fixed chunks make results bitwise identical for any thread count (K6), which the M9
test asserts. Throughput *(target, research §3.4)*: PH <= 15 us single-phase per state, so 1e6 x 15 us / (16 x 0.9) is
about 1.0 s. CoolProp's oracle does 5.2k-22k PH states/s per thread (map 10 §8.2). Failing points get a per-cell
`Status` and NaN, never stale values (map 11 F15).

**(c) Adding PC-SAFT, later ice Ih.**
- *PC-SAFT (new crate, no core edits):* write the residual once over `Real` and differentiate it with `Jet4`/`HyperDual`.
  CoolProp hand-codes PC-SAFT four times in about 1.45k lines (map 06). Borrow the ideal-gas part and gauge from the
  canonical fluid's `IdealTerms` (map 06, map 15 X6). Implement `HelmholtzModel` and `PureFluidModel` with
  `saturation() = None`, so the flash uses the generic VLE strategy. Declare capabilities: start from PT/DT and grow as the
  matrix passes.
- *PC-SAFT verification:* dev-depend on `cprs-verify` and run `fd_first_order`, `gauge_invariance`, `capability_matrix`
  and `policy_equivalence` unchanged. Arbiters are paper tables. CoolProp 8.0.0 PC-SAFT is not an oracle: it lacks h/s/cp/w,
  mis-computes Q (map 06) and its test values lack provenance (map 10 U14). Independent codes are labelled
  `OtherImplementation`, never arbiters.
- *Ice Ih:* the first Gibbs model (materials R13). Additive changes: a `GibbsModel` trait, `relations::gibbs` (g, g_T, g_p,
  g_TT, g_Tp, g_pp), `Phase::Solid` (`#[non_exhaustive]`, so not breaking) and a private `Body::Gibbs` variant in `State`
  (not breaking). `Substance` arrives when humid air needs ice and liquid together (materials §3.2).
- *Ice Ih verification:* IAPWS R10-06 check tables as `Iapws` provenance; the cross-family test g_ice = g_liquid on IAPWS
  R14-08 melting points, valid because R10-06's s0 is anchored to the IAPWS-95 zero (map 15 §2.1); the g00 revision
  question becomes a register entry (map 07 C1).

**(d) A CoolProp bug where the literature overrides CoolProp** (R1234ze(E), map 13 §3 item 3).
1. At M5 the K2 fixture fails: Rust on `DataSet::Parity` reproduces CoolProp, but both are +1.0e-6 to +1.4e-6 high in p
   against Thol 2016 Table 3, where `from_printed` allows about 2e-7.
2. Arbitration without the oracle: decode the record, set `eos.gas_constant = 8.3144621` (the paper's stated R, recorded
   in `ARBITERS.stated_r`), compile, and re-run. Every row now lands within its printed digits (<= 1.9e-7). The table is
   self-consistent with its own constants, so it is a valid arbiter.
3. Record it as data, not as a widened tolerance:
   `Divergence{id: "DIV-0001", policy: UsePaper, arbiter: Thol 2016 Table 3, evidence: "map 13 §3"}` plus
   `Patch{divergence: "DIV-0001", edit: GasConstant(8.3144621)}`. The EOS hash changes, so the v8.0.0 superancillary is
   marked stale and saturation uses VLE until a refit (D7).
4. Proof test (in the corpus): (i) `Corrected` matches Table 3 within printed digits; (ii) the oracle fixture differs from
   Table 3 beyond them, otherwise the entry is stale; (iii) `Parity` matches the oracle within `prop`. Oracle rows for
   R1234ze(E) that depend on R are reported as "covered by DIV-0001", counted and not asserted.
5. When the oracle pin moves (for example to a release with this fixed), (ii) fails and forces
   `Status::ResolvedUpstream{commit}`, which keeps the audit trail (map 10 §8.5).

## 5. CoolProp rot designed out

| Rot (evidence) | How this design prevents it |
|---|---|
| Fused mutable AbstractState; friend classes write ~170 fields (map 12 R1) | Immutable `Arc<FluidData>`; `State` created only by a flash; no `update()` |
| Sticky phase imposition, poisoned states (map 01 R12, map 03 §6) | `PhaseHint` is a per-call field of `FlashOptions`; `State` has no setters |
| Global config/env, 141 reads, RAII guards in tests (map 12 R4, map 01 R15/R16) | Options by value; clippy bans `std::env::var`; multi-threaded `cargo test` |
| `catch(...)` cascades, `_HUGE` sentinels (map 12 R3) | `Result` everywhere; `Root.status`; `Option` for missing data; `SolvePath` |
| Solver returns x(n+1) with state at x(n); unchecked max_iter (map 03 §6) | `Root{x, f at x, iterations, status}`; acceptance gate |
| Hand-coded 20 derivatives/term + test-only multicomplex copy (map 12 R5, map 10 R7) | `value::<R>` once; jets checked against AD of the same production code |
| Fail-open gates, hidden sweeps (map 10 R1/R2, map 12 R17) | Executed-test-count gate; nightly `--ignored` with a report |
| Weak assertions, tolerances tuned to CoolProp (map 10 R3-R5) | Provenance-derived tolerances; round trips assert values; never widen |
| Self-referential reference values (map 10 R6) | `Provenance::SelfReferential` / `OtherImplementation` are not arbiters |
| Thin term coverage, unknown blocks skipped (map 10 R8, map 09 R7) | All term kinds x ~300 points + AD; closed enums, `LoadError` |
| Stale derived h/s in JSON (map 10 R11, map 12 R10) | Never stored; computed from the model; hash freshness gate |
| Critical point changes with a flag (map 10 R12, map 03 §6) | `published_critical()` and `eos_critical()` are separate methods |
| Disabled tests lose visibility (map 10 R14) | `#[ignore = "DIV-/issue id"]` required and reported |
| Unvalidated inputs: NaN Q, Q=5, T<Tmin (map 12 R9, §6.3) | Validating newtypes; `DomainError`; `DomainPolicy` per call |
| Transport and cp on two-phase states (map 10 R18, map 02 §6) | `PropError::TwoPhaseUndefined`; DIV-0004 |
| Gas-constant scatter, stored R != paper R (map 12 R7, map 13 R1/R3) | R is per-model data; `ARBITERS.stated_r` audit at datagen |
| Reference state as global mutation; mixed gauges (map 15 X1-X3, X7, X12) | `Gauge` value applied at the boundary; invariance properties |
| Capability discovered by exception (map 01 R21) | Declared `Capabilities`; full matrix test |
| Enum integers as ABI, renumbered (map 01 R1) | String keys in compat and capi; discriminants never exported |
| Eager whole-library load ~2 s (map 01 R10, map 09 R1) | Per-fluid `OnceLock`; lazy superancillary and transport |
| Deep copies per state, 78-126 KiB (map 09 R5, map 11 F11) | `Copy` `State` <= 256 B asserted; shared `Arc` model |
| Hot-path SA mutex (map 03 §6, map 11 F7/F8) | Offline-precomputed inverses; `OnceLock`; clippy-banned locks |
| T+X non-uniqueness answered silently (map 03 §6) | `RootPolicy::Strict` returns `Ambiguous{roots}` |
| Bit-exact asserts across code paths (map 10 R16) | Bitwise only for the same path; bound classes otherwise |
| C ABI hazards: handle escape, `long`, missing lengths, throw across `extern "C"` (map 11 F1-F4, F16) | capi rules in D11 |
| Batch swallows per-point errors (map 01 R22, map 11 F15) | Per-cell `Status`; NaN only with non-Ok status |
| TTSE/BICUBIC crashes and garbage (map 08) | Not ported; any future surrogate is a model checked by the same kit |
| Open/Closed violations, 4 edits per transport form (map 12 R13) | Families via traits; closed term set validated at load |
| Hidden ECS coupling (map 12 R12) | `FluidId` references; acyclic graph check; impact tests on change |

## 6. First 10 TDD milestones (each ends green on Linux, Windows MSVC and wasip2)

Capabilities grow per milestone, so the capability matrix is green at each step. Rust 1.99 is already installed (step 0).

| # | Deliverable | Green gate | Oracle / check values |
|---|---|---|---|
| M1 | Workspace, CI matrix, lints, `cprs-verify` kit (comparators, `from_printed`, fixture reader, register types), oracle generator + `oracle.lock` | 4-target builds; zero-dep guard; test-count guard; fixture round trip bit-exact | Self-tests; "21.17909" gives 5e-6 and flags R1224YDZ (map 10 §8.4) |
| M2 | Data L0: datagen for all 136 JSON, blobs, decode, index, `CORRECTIONS`, `DataSet` | Record round trip exact; Parity vs Corrected diff equals the patch set | 130 FNV-1a hashes + self-test `8e75626511d00b5c`; 556 keys, 0 collisions (map 09, map 10 §8.5 L0) |
| M3 | Separable residual kinds + `Jet4`; `PureEos::residual` | Jets vs AD at ~300 (tau, delta) per block | Oracle block isolation, all kinds incl. DoubleExponential and GaoB (map 10 §8.2, U3), class `term` |
| M4 | NonAnalytic (explicit at delta=1) + all 10 ideal-gas kinds; full alpha | FD + AD for every kind; core-10 fluids load | IAPWS-95 K1: 11 alpha values at 500 K, 838.025 kg/m3 (map 13 §3) |
| M5 | Relations, DT pair, exact virials, gauge shifts | Identities (map 01 §8); gauge invariance; capability DT | IAPWS-95 K2; Lemmon 2016 Table 7, 4 fluids (<= 4.3e-7); in-test paper rows (13 fluids, 18 states, map 10 §8.1); Thol 2016 = DIV-0001 proof; eos fixtures `prop` |
| M6 | Superancillary, pure VLE, critical points; QT, PQ | SA vs VLE differential; stale-SA fallback for patched fluids | 390 mp check points (`sat-mp`); CoolProp SA (`sa-coeff` 1e-14); crit fixtures; IAPWS-95 saturation rows |
| M7 | PT/DT phase determination, P+X, D+X, T+X, HS, remaining Q-pairs | Capability matrix 19 x 2 bases; acceptance gate; `Ambiguous` cases | 40x40 (log p, T) + 20x20 (T, Q) round trips per core fluid; ~100 issue-linked must-not-fail states (map 10 §8.5 L4) |
| M8 | Surface tension, transport (staged, IAPWS water, ECS), melting lines | Stage-by-stage `Breakdown`; two-phase transport refuses (DIV-0004) | sigma for 108 fluids; IAPWS water eta/lambda (oracle 3.3e-8/2.7e-9); 318 re-provenanced rows; R14-08 melting (DIV-0002) |
| M9 | `evaluate_batch`, `rayon`, determinism, benchmarks | Batch == scalar bitwise; 1/2/N threads bitwise; 16-thread same/different-fluid stress; remainders | Canonical-grid hash equal on Linux/Windows/wasip2 (`libm`); criterion/gungraun baselines vs a C++ CoolProp baseline |
| M10 | Reference states, compat `props_si`, C ABI, WASM browser build -> release 0.1 | Name table round trip; C smoke test; wasm build both variants; semver/deny/REUSE gates | IIR/ASHRAE/NBP for n-Propane, R134a, R124 at 1e-8 (map 01 §8); map 15 §8 gauge fixtures |

## 7. Risks and deliberate deferrals

| Risk | Mitigation |
|---|---|
| Verification overhead slows feature delivery | Kit written once (M1) and reused by every later family; corpus tests are table-driven |
| Paywalled papers: 74 fluids have only `exp.` tables, 101 papers are paywalled (map 13 §1, Q1) | `ArbiterStatus::Expected`; oracle fixtures stay provisional; user to supply core-10 papers |
| Fixture size and churn (~3.5 GB full set) | Committed core subset + manifest; nightly regeneration |
| Patched EOS constants invalidate the shipped superancillary | Hash gate marks it stale; VLE fallback; refit in datagen when tooling allows |
| Cross-platform FP drift with std libm | `libm` feature for hash parity; tolerance classes otherwise; fixtures from one platform |
| Literature itself is wrong (Helium IR 8474; post-v8 transport equation errata, map 12 R11) | Self-consistency check before a table becomes an arbiter; `Investigate` policy |
| Jet and AD types to maintain (estimated 500-800 lines, research §2.4) | Small, in one module; tested by identities |
| Performance targets unmeasured (K19) | Benchmarks at M9 before any gate; C++ baseline |

**Deferred deliberately:** mixtures (M12), SIMD lanes (M11), Python (M15), cubic/PC-SAFT, IF97, INCOMP, humid air, ice
Ih (M13), tabular surrogates (TTSE/BICUBIC dropped; SBTL only as a checked model later), CoolPropPlot plotting, isolines
and cycle helpers, REFPROP, GPU, WASM threads, plug-in loading, non-SI units in core.

**User decisions needed (no default is silently chosen):**
- Default for literature-vs-oracle conflicts: `Corrected` is proposed.
- Committing printed check values and fastchebpure data.
- Helium R (map 13 Q2).
- Extrapolation below Ttriple (map 03).
- Whether a drop-in `CoolProp` Python namespace ships (map 14).

## 8. Self-assessment against the rubric

| Criterion (weight) | Self-score /5 | Why |
|---|---|---|
| Modularity, extensibility (3) | 4 | Two object-safe traits, closed term set, reducing-invariant bundle, inherited conformance kit; walkthrough (c) needs no core edit. Gibbs seam deferred until ice. |
| Simplicity (3) | 3.5 | One kernel crate, two traits, a few value types. The verification vocabulary (provenance, classes, register, arbiters, overlay) adds concepts, though it is confined to `cprs-verify`. |
| Concurrency, memory (3) | 4.5 | Per-fluid `OnceLock`, `Arc` handles, `Copy` State <= 256 B, banned locks, compile-time Send + Sync, lazy SA/transport. |
| Performance (3) | 3 | **Weakest.** A clean path (jets, SoA, `ExecPolicy`, lane-ready `Real`), but SIMD is post-0.1 and no number is measured yet; the design spends early milestones on evidence, not speed. |
| Correctness, TDD (3) | 5 | Seams in every layer, provenance-derived tolerances, typed register with proof tests, capability-gated always-green milestones, parity/corrected switch. |
| Idiomatic Rust, DRY/SOLID, deps, types (3) | 4.5 | std-only core, newtypes, `non_exhaustive`, one formula per term, clippy-enforced policies; 15 sketch tests pass. |
| Rot elimination (2) | 4.5 | 29 rot items mapped to mechanisms (§5), most enforced by types or CI. |
| Cross-platform (2) | 4.5 | 4 targets checked in the sketch; corpus runs on wasip2; determinism hash; two wasm builds. |
| Migration (1) | 3.5 | compat `Session`/`props_si`, string-keyed C ABI, optional CoolPropLib.h shim; Python after 0.1. |
