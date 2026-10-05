# Concurrent, memory-frugal kernel and SIMD / parallel execution

Research input for design decision **D9** (execution strategies). It also touches D2 (derivatives), D3 (model representation), D5 (state/properties) and D7/D8 (lazy loading, derived data), numbered as in `materials-extensibility.md`.
Date: 2026-10-04. Stable Rust is 1.99.0, released 2026-10-01 (the std docs report `1.99.0 (b940084d7 2026-09-28)`). Crate versions and project status are as of that date.
Local CoolProp citations are paths under `reference/CoolProp` at v8.0.0 (`ae81610e`). `coolprop-map/NN` means `docs/coolprop-map/NN-*.md`. "Oracle" timings are the CoolProp 8.0.0 Python wheel measured for those docs; they include about 0.3 µs of wrapper overhead per call.

**Bottom line:**

- **Concurrency comes from immutability, not from locks.** The model is shared read-only. The evaluation path never writes shared memory: no `Arc` clone per call, no lock, no global counter, no lazily built cache behind a mutex. Every one of those exists on CoolProp's hot path today.
- **The math is written once.** It is generic over a numeric trait (`f64`, SIMD lane types, AD jets). Executors (scalar reference, SIMD lanes, rayon chunks) hold no math.
- **Do SIMD across states first, across terms later.** When each lane is one state, every lane runs exactly the scalar sequence of operations. SIMD results are then **bit-identical** to the scalar reference, so testing is exact and SIMD can be on by default.
- **Own `exp` and `ln`.** Write one generic implementation (a port of the FreeBSD msun algorithms that Rust's `libm` uses, error under 1 ulp) and use it for every lane width and every platform. `std`'s `f64::exp` is documented as non-deterministic across platforms and versions.
- **Stable Rust only.** `std::simd`, `sleef-rs` and WASM threads still need nightly. Use `core::arch` with `#[target_feature]` (safe since 1.86/1.87) and a hand-rolled runtime dispatch done once per batch. WASM ships two builds (baseline and simd128) and is single-threaded. No GPU work.

---

## 1. Summary of recommendations

| # | Decision | Recommendation | Confidence |
|---|---|---|---|
| K1 | Sharing models across requests | `Fluid` is immutable, `Send + Sync`, with no interior mutability except `OnceLock`. Kernels take `&Fluid`. `State` is plain data (T, ρ, phase, derivative bundle) and holds **no `Arc`**. | High |
| K2 | Lazy per-fluid initialization | A static `[OnceLock<Arc<Fluid>>; N]` indexed by `FluidId`, plus inner `OnceLock`s for the superancillary and transport. The name→id index is immutable. No `RwLock<HashMap>`, no `Weak` caches. Add a `preload(&[FluidId])` API for servers. | High |
| K3 | Hot-path hygiene | No shared writes during evaluation. Enforce it with static `Send + Sync` assertions and clippy `disallowed-types` (`Mutex`, `RwLock`, `RefCell`) in kernel modules. | High |
| K4 | Scratch space | Per-state scratch lives on the stack (about 100–200 B). Batch output goes into caller-provided buffers. Later mixture workspaces use rayon `for_each_init`. No `thread_local!` in kernels. | High |
| K5 | Batch API shape | Inputs as two slices. Outputs as one column per property. One `u8` status per point. Outputs are compiled into a plan once. Points are partitioned by phase region before evaluation. No allocation. A row-major adapter gives CoolProp `fast_evaluate` parity. | Medium-High |
| K6 | Thread parallelism | rayon behind an optional feature. Chunk sizes are fixed, a multiple of 16 points, and independent of the thread count. The library never spawns threads itself, and small batches run sequentially. | High |
| K7 | Warm-starting flashes | Opt-in and only within a chunk. The default is a cold start, so scalar and batch results are bitwise equal. | Medium |
| K8 | Write the math once | A crate-internal `Real` trait with a `Mask` type and `select`. Term code is branch-free and generic. Per-term integer data stays scalar. Executors contain no math. | High |
| K9 | SIMD axis | First **across states** (lanes = states, coefficients broadcast), which is bit-identical to scalar. **Across terms** (single-call latency) is a later, benchmark-gated experiment checked with a tolerance. | High (order) / Medium (payoff) |
| K10 | Transcendental functions | A crate-internal generic `vmath::{exp, ln, ln_1p, exp_m1}` ported from msun (via Rust `libm`), used by every path. No `std` transcendentals inside kernels. Non-integer `pow` is allowed only in scalar code (NonAnalytic terms). | Medium-High |
| K11 | SIMD implementation | Stable only. Own small lane types: portable `[f64; N]` first, then explicit `core::arch` for AVX2. Dispatch is detected once and kept in a `OnceLock` table. NEON is the aarch64 baseline. WASM simd128 is chosen at compile time. `fearless_simd` 1.0 is the fallback backend if maintaining our own lanes grows costly. Avoid `std::simd`, `sleef`, `wide`, `simdeez`. | Medium |
| K12 | FMA | Deterministic kernels never call `mul_add`. Rust never contracts on its own. FMA belongs to a possible later "Fast" policy only. | High |
| K13 | Cargo features vs runtime dispatch | Features only add capabilities (`std`, `rayon`, `simd`). Numerics are chosen at runtime (`ExecPolicy`). Feature unification therefore cannot change anyone's numbers. | Medium-High |
| K14 | Algorithm classes | Data-parallel kernels (term sums, relations, superancillary Clenshaw, ancillaries, transport, tables) get lane versions. The ρ(T,p) solve becomes a lockstep solve with masks and a scalar retry. Phase logic, flash cascades, VLE and mixtures stay scalar and run in parallel across requests. | High |
| K15 | WASM | v1 is single-threaded with two builds (baseline, `+simd128`) and no relaxed-simd. Web parallelism means one worker per module instance. A nightly threads build is documented as experimental. | High |
| K16 | GPU | None now. Keep SoA data and the `Real` generics so a backend stays possible. f64 is unavailable on WebGPU and Metal. | High |
| K17 | Testing accelerated paths | Lanes and thread counts against scalar: bitwise. Across-terms and Fast against reference: a summation-bound tolerance. CI forces every dispatch level and compares a cross-platform result hash. | High |
| K18 | Tolerances against the oracle | Use the classes in `coolprop-map/10` plus a condition-aware bound for α^r sums. Generate fixtures on x86-64 Linux only and never assert bitwise against CoolProp. | Medium-High |
| K19 | Performance targets | §3.4. Measure a C++-level CoolProp baseline before committing to the numbers. | Medium (targets are estimates) |

---

## 2. Findings by subtopic

### 2.1 Concurrency for many simultaneous requests

#### 2.1.1 Why CoolProp's `AbstractState` design is hostile to this

| Property of v8.0.0 | Evidence | Consequence |
|---|---|---|
| One object is model, state, memo cache and solver workspace | `coolprop-map/01` §1; master [`dev/agent-notes.md`](https://github.com/CoolProp/CoolProp/blob/master/dev/agent-notes.md): "Thread-safety contract is one AbstractState per thread … Sharing one backend across threads would genuinely race." | Parallelism means N copies. A copy cannot be pooled safely (next row). |
| Cross-call mutable flags | DQ/HQ/QS set the imposed phase to two-phase and never clear it. On a reused Water state, PH at (30 MPa, 700 K) then returns T = 270.79 K (`coolprop-map/03` §6). | Results depend on what the instance did before. |
| Deep copies per instance | 3 copies of `CoolPropFluid` (self, SatL, SatV) plus the retained superancillary JSON string, about 70 KB (`coolprop-map/03` §5). 65–164 KiB per instance; construction 39–59 µs; `PropsSI` 81 µs against 20 µs for a reused instance (`coolprop-map/01` §5). | Memory grows as threads × fluids. About 75 % of a `PropsSI` call is construction. |
| Shared writes on hot paths | `deriv_counter.fetch_add` on every α^r bundle (`src/Backends/Helmholtz/HelmholtzEOSMixtureBackend.cpp:47, :3554`). Made atomic in 8.0.0 for correctness (#2855, `Web/coolprop/changelog.rst:14`), so now correct but contended. | Every core evaluating any fluid writes one shared cache line. |
| A lock on hot paths | `get_T_from_p` → `get_invlnp()` takes a `std::lock_guard` on **every** call (`include/CoolProp/superancillary/superancillary.h:1078-1086, :1272-1274`). That serializes PQ, PH, PS, PU and p-based phase determination per fluid. | Same-fluid requests serialize. |
| Per-thread copies to work around it | HumidAir keeps `thread_local` backends that are never evicted (`src/HumidAirProp.cpp:44-53`). In the SVDSBTL parallel build "each thread holds its own `AbstractState` (~few MB) plus a thread-local SuperAncillary lazy-build buffer" (`Web/coolprop/SVDSBTL.rst:455-458`), and 4 threads give only about 2.25× on the table build (`:440`). | Memory × threads, with poor scaling. |
| First-use builds | Caloric superancillary 34–51 ms per fluid, `invlnp` 1.3–1.7 ms (`coolprop-map/03` §5); "~10–50 ms" (`Web/coolprop/LowLevelAPI.rst:270`). | Tail-latency spike on the first request per fluid. |
| REFPROP backend | "not thread-safe — REFPROP itself is not reentrant" (`Web/coolprop/changelog.rst:14`). | Out of scope, but rules out REFPROP as a parallel oracle. |

#### 2.1.2 Rust building blocks

| Primitive | Status and semantics | Hot-path cost | Use here |
|---|---|---|---|
| `OnceLock<T>` | Stable since 1.70. `get()` "never blocks". `get_or_init` runs exactly one initializer. Reentrant initialization is unspecified ("Current implementation deadlocks, but this may be changed to a panic"). Never poisoned ([docs](https://doc.rust-lang.org/std/sync/struct.OnceLock.html)). | One acquire load after initialization. | Per-fluid cell and per-part cells (superancillary, transport). |
| `LazyLock<T>` | Stable since 1.80 ([1.80 notes](https://blog.rust-lang.org/2024/07/25/Rust-1.80.0/)). | Same. | Registry index, dispatch table. |
| `[const { OnceLock::new() }; N]` | Inline `const` blocks in repeat expressions are stable since 1.79 ([1.79 notes](https://blog.rust-lang.org/2024/06/13/Rust-1.79.0/)). | None. | Static table indexed by `FluidId`. |
| `RwLock<HashMap>` | Every `read()` does a `compare_exchange` on the shared state word ([Bos, ch. 9](https://mara.nl/atomics/building-locks.html); std's futex `RwLock::read` does `compare_exchange_weak` too, [`rwlock/futex.rs`](https://github.com/rust-lang/rust/blob/main/library/std/src/sys/sync/rwlock/futex.rs)). | A shared write per lookup. | Never on a hot path. |
| `Arc::clone` / drop | `strong.fetch_add(1, Relaxed)` ([std `arc.rs`](https://github.com/rust-lang/rust/blob/main/library/alloc/src/rcs/arc.rs)). | A shared write. | Only when a handle is created. |
| `Weak::upgrade` | A CAS loop on the strong count (same file). | A shared write. | Not on a hot path. |
| `arc-swap` 1.9.2 | Lock-free swappable `Arc` ([crates.io](https://crates.io/crates/arc-swap)). | Low. | Only if a hot-swappable global registry is ever needed. Not in v1. |
| `CachePadded` (crossbeam-utils 0.8.23) | Pads to 128 B on x86-64, aarch64, arm64ec and powerpc64 ([source](https://github.com/crossbeam-rs/crossbeam/blob/master/crossbeam-utils/src/cache_padded.rs)). The docs' rationale for x86-64 is that on modern Intel the "spatial prefetcher is pulling pairs of 64-byte cache lines" ([docs](https://docs.rs/crossbeam-utils/latest/crossbeam_utils/struct.CachePadded.html)). | None. | Per-thread counters, if any exist. Hand-written `#[repr(align(128))]` avoids the dependency. |

#### 2.1.3 Contention analysis

Background: under MESI, a cache line that nobody writes stays Shared in every core with no coherence traffic. Any store or read-modify-write, **including a failed compare-exchange**, needs exclusive ownership and slows the other cores. In Bos's experiment a spin loop went from about 300 ms to about 3000 ms. Unrelated variables in one line contend in the same way (false sharing), and alignment padding fixes it ([Bos, ch. 7](https://mara.nl/atomics/hardware.html)).

| Shared item in coolprop-rs | Access during evaluation | Verdict |
|---|---|---|
| Residual coefficients: about 0.2 MB for all 2532 terms, about 1.5 KiB per fluid on average (`coolprop-map/02` §5) | Read only | Scales with cores, whether requests hit the same fluid or different ones. Fits in L1/L2. |
| Superancillary pieces: 53–91 pieces × 13 coefficients × 3 curves, about 20 KB per fluid (`coolprop-map/03` §3.3) | Read only | Same. |
| `OnceLock::get` | Acquire load | No contention after initialization. |
| A `State` that owns `Arc<Fluid>` | `fetch_add` and `fetch_sub` per state | **Same-fluid contention on one line.** At µs-scale flashes on many cores this approaches the throughput limit of a single contended line (inference). Avoid: `State` holds no fluid pointer, and `StateRef<'a>` borrows. |
| Global statistics or debug counters | Read-modify-write | Do not have any. If needed, make them per-thread and padded. |
| Batch output slices | Writes to disjoint chunks | False sharing only at chunk edges. Choose chunk lengths that are multiples of 16 f64 (128 B) and align the buffers. |
| Lazy initialization of a fluid | One initializer; other threads block in `get_or_init` | A one-time cost per fluid, bounded by decoding. Precompute derived superancillary data offline so it never costs tens of ms (§2.1.1). |

#### 2.1.4 Per-thread scratch and memory reclamation

- **Scratch.** A pure-fluid state needs no heap scratch. The per-state variables (τ, δ, ln τ, ln δ, δ¹…δ⁶) and a derivative bundle of at most 15 f64 live on the stack. Power-term exponents l take at most 7 distinct values per fluid (median 3), computed from `dev/fluids/*.json`, so δ^l comes from that small table. For future mixtures, use explicit workspaces created per rayon job (`for_each_init`/`map_init`, [rayon docs](https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html)). Do not hide them in `thread_local!`, which is hidden state and complicates WASM and async callers.
- **Reclamation.** Built-in fluids are never freed. That costs about 22 KiB per fluid and about 3 MiB for all of them (`coolprop-map/09` D3), plus about 20 KB per fluid of superancillary data (about 2.5 MB for all, `coolprop-map/03` §3.3). For comparison, CoolProp's eager superancillary build costs about 1.8 s and about 11–12 MB at import (`coolprop-map/02` §5, `coolprop-map/09`).
  - **No `Weak` caches.** Upgrading is a shared write, and drop/re-create churn would repeat decoding.
  - User-built models are `Arc`s that drop naturally.
  - An interning cache is allowed only at mixture *construction*, never during evaluation.

### 2.2 Batch evaluation

#### 2.2.1 API shapes

CoolProp's `fast_evaluate` (`include/CoolProp/AbstractState.h:889-924`):

- Takes caller buffers and writes the row-major layout `out[k*N_outputs + o]`.
- Fills a row with NaN and an integer status on per-point failure.
- Does no allocation and bypasses the cache.
- Is implemented only by the IF97 and tabular backends; HEOS does not have it.

SVDSBTL `fast_evaluate` takes about 300–330 ns per point for 4 outputs on Apple Silicon. That is a setup intercept of about 170 ns, shared across outputs, plus about 35 ns per output (`Web/coolprop/SVDSBTL.rst:259-266`). **Lesson:** compile the output request once and share per-point setup (region, τ, δ, derivative bundle) across all outputs.

| Layout | Advantages | Costs | Use |
|---|---|---|---|
| SoA inputs (`x: &[f64]`, `y: &[f64]`) | Contiguous lane loads | — | Always |
| SoA outputs (one column per property) | Contiguous lane stores, no transposition, any subset of properties | Longer signature | Core API |
| AoS rows (CoolProp `fast_evaluate`) | One state's properties sit together | Strided stores, or a transposition per chunk | C ABI adapter |
| Model coefficients as SoA per term family | Vector loads (across terms) or broadcast loads (across states) | Padding | Always. Datagen does the padding and ordering offline. |

#### 2.2.2 Partition, then evaluate

1. Classify every point once (phase region from the superancillary).
2. Make a stable partition by region key.
3. Run each homogeneous group through lane kernels.
4. Scatter results back to the caller's order.

Sorting is O(n log n) against µs-scale flashes per point, so it is negligible. It also groups similar states, which helps lockstep solvers and warm starts.

Status is one `u8` per point (SIMD-friendly). Detailed errors go into a sparse side list `(index, FlashError)`.

#### 2.2.3 Rayon chunking

- rayon 1.12.0 (crates.io 2026-04-14; `RELEASES.md` says 2026-04-13). MIT OR Apache-2.0. MSRV 1.80. It depends on `either` and `rayon-core`, which in turn pulls in `crossbeam-deque` and `crossbeam-utils` ([crates.io](https://crates.io/crates/rayon)).
- `sum` and `reduce` have unspecified order, so floating-point reductions "are not fully deterministic" ([docs](https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html)). The library must **never** reduce across points in parallel.
- Use `par_chunks_mut` with fixed-size chunks. `with_min_len`: "Rayon will not split any smaller than this length" ([docs](https://docs.rs/rayon/latest/rayon/iter/trait.IndexedParallelIterator.html)).
- rayon has had "a fallback when threading is unsupported" since 1.7.0, and 1.11.0 fixed `in_place_scope` "on WebAssembly without threading support" ([RELEASES.md](https://github.com/rayon-rs/rayon/blob/main/RELEASES.md)). The same API therefore runs single-threaded on WASM.
- Chunk length should be a multiple of the lane width and of 16 points, for example 256–1024 points. That is large enough to amortize job overhead (unverified; measure).

#### 2.2.4 Warm-starting and determinism

- CoolProp already supports external guesses (`update_with_guesses`, `include/CoolProp/AbstractState.h`) and warm inner Newton steps (FlashRoutines.cpp:2896, `coolprop-map/03` §3.1).
- Warm-starting from the previous point of a sorted batch cuts iterations. The converged bits, however, depend on the starting guess, which depends on the order and on chunk boundaries.
- A reproducible recipe:
  - chunk boundaries depend on size only, never on thread count;
  - the first point of each chunk starts cold;
  - each later point is seeded from its predecessor;
  - a final polish step runs to machine precision.
- Results are then independent of the thread count but differ from cold-start results at solver tolerance. Test them with the `flash` class, not bitwise. The default is off.

### 2.3 SIMD in Rust, October 2026

#### 2.3.1 Language and standard library

| Mechanism | Status (source) | Consequence |
|---|---|---|
| `std::simd` / `core::simd` | Nightly only: "🔬 This is a nightly-only experimental API (`portable_simd` #86656)" on the 1.99.0 docs ([std::simd](https://doc.rust-lang.org/std/simd/index.html); [#86656](https://github.com/rust-lang/rust/issues/86656) open). `StdFloat` "may … canonicalize to calling an operating system's `math.h`" ([docs](https://doc.rust-lang.org/std/simd/trait.StdFloat.html)). A 2026 survey calls its float math "scalar implementations in a SIMD guise" ([Davidoff 2026](https://shnatsel.github.io/state-of-simd-rust-2026/)). | Not usable. Its `exp` would not be vectorized anyway. |
| `core::arch` x86-64 (SSE2 … AVX2) | Stable. AVX-512 target features and intrinsics have been stable since 1.89 ([1.89 notes](https://blog.rust-lang.org/2025/08/07/Rust-1.89.0/)). | AVX2 and AVX-512 paths work on stable. |
| target_feature 1.1 | 1.86: safe `#[target_feature]` functions "can only be safely called from other functions marked with the target feature attribute". Elsewhere they need `unsafe` plus a feature check. Coercion to a function pointer happens only inside such functions ([1.86 notes](https://blog.rust-lang.org/2025/04/03/Rust-1.86.0/)). | One `unsafe` per dispatch entry point. |
| Safe intrinsics | 1.87: "Most `std::arch` intrinsics that are unsafe only due to requiring target features to be enabled are now callable in safe code that has those features enabled" ([1.87 notes](https://blog.rust-lang.org/2025/05/15/Rust-1.87.0/)). | Lane types are mostly safe code. |
| Inlining | Target-feature functions "are not inlined into a context that does not support the given features". `#[inline(always)]` cannot be combined with `#[target_feature]`. Only closures inherit features, not callees ([Reference](https://doc.rust-lang.org/reference/attributes/codegen.html)). | Generic kernel bodies must be `#[inline(always)]` so they are inlined *into* the feature-enabled entry point. Otherwise they silently compile for the baseline. |
| Runtime detection | `is_x86_feature_detected!` (std, since 1.27; [docs](https://doc.rust-lang.org/std/macro.is_x86_feature_detected.html)) and `is_aarch64_feature_detected!` (since 1.60; [docs](https://doc.rust-lang.org/std/arch/macro.is_aarch64_feature_detected.html)). Results are cached in static atomics ([`std_detect` cache.rs](https://github.com/rust-lang/rust/blob/master/library/std_detect/src/detect/cache.rs)). For `no_std`, `cpufeatures` 0.3.1 ([crates.io](https://crates.io/crates/cpufeatures)). | Detect once and store a function table. |
| Baseline and `target-cpu` | Each target has a default base CPU. `native` exists. `-C target-feature` "is unsafe and might result in undefined runtime behavior" ([codegen options](https://doc.rust-lang.org/rustc/codegen-options/index.html)). | Generic x86-64 binaries (wheels) need runtime dispatch. `-C target-cpu=native` users get the static path. |
| aarch64 NEON | Stable since 1.59 (`#[stable(feature = "neon_intrinsics", since = "1.59.0")]` in [stdarch `aarch64/neon`](https://github.com/rust-lang/stdarch/blob/master/crates/core_arch/src/aarch64/neon/mod.rs); [Arm blog](https://developer.arm.com/community/arm-community-blogs/b/architectures-and-processors-blog/posts/rust-neon-intrinsics)). f64x2 with FMA. SVE intrinsics are not stable. The [2025H2 goal](https://goals.rust-lang.org/2025h2/scalable-vectors.html) targeted a nightly experiment, and the [2026 goal](https://goals.rust-lang.org/2026/scalable-vectors.html) still aims only to "continue nightly support for scalable vectors". | 2 lanes and no dispatch. SVE is out. |
| wasm32 simd128 | Stable since 1.54 ([stdarch simd128.rs](https://github.com/rust-lang/stdarch/blob/master/crates/core_arch/src/wasm32/simd128.rs)). Chosen at compile time only: "your binary will either have SIMD and can only run on engines which support SIMD, or it will not have SIMD at all" ([core::arch::wasm32](https://doc.rust-lang.org/core/arch/wasm32/index.html)). | Ship two `.wasm` builds and pick one from JS. |
| wasm relaxed-simd | Intrinsics stable since 1.82 ([relaxed_simd.rs](https://github.com/rust-lang/stdarch/blob/master/crates/core_arch/src/wasm32/relaxed_simd.rs)). `f64x2_relaxed_madd` "computes `a * b + c` with either one rounding or two roundings". | Engine-dependent results. Never use in deterministic kernels. |
| Algebraic float methods | Stable since 1.98: optimizations "similar to … `-ffast-math`", and "these methods are non-deterministic" ([1.98 notes](https://blog.rust-lang.org/2026/08/20/Rust-1.98.0/)). | Not for reference kernels. Our own lanes make them unnecessary. |
| Auto-vectorization | "this method is not very reliable. The larger and more complex your function is, the greater is the chance that the compiler will not be able to vectorize it" ([Davidoff 2026](https://shnatsel.github.io/state-of-simd-rust-2026/)). LLVM leaves math-function calls scalar unless a vector math library is mapped ([LLVM RFC, 2016](https://discourse.llvm.org/t/rfc-a-proposal-for-vectorizing-loops-with-calls-to-math-functions-using-svml/40565): "The loops are vectorized, but the calls remain scalar"). Whether rustc maps a vector library by default today is unverified. | Check the assembly of each hot kernel. An inline `vmath::exp` is required. |

#### 2.3.2 Portable SIMD crates

| Crate (version, date) | Dependencies, licence | Dispatch | `exp`/`ln` | Verdict |
|---|---|---|---|---|
| `fearless_simd` 1.0.0 (2026-09-21), MSRV 1.89 | No required deps (`libm` optional); Apache-2.0 OR MIT ([crates.io](https://crates.io/crates/fearless_simd)) | Built in (`Level`, `dispatch!`): SSE2, SSE4.2, AVX2, AVX-512 (Ice Lake), NEON, WASM simd128 and relaxed ([docs](https://docs.rs/fearless_simd/latest/fearless_simd/)) | None. `SimdFloat` has sqrt, mul_add, floor and the like, but no exp, ln or pow ([docs](https://docs.rs/fearless_simd/latest/fearless_simd/trait.SimdFloat.html)). The survey: "There just isn't a port of anything like SLEEF to fearless_simd machinery yet". | **Fallback backend** for our lane trait. The survey calls it "an all-in-one solution where all the parts work together". Its author became a fearless_simd maintainer and discloses this, so weigh its verdicts with that in mind. |
| `wide` 1.7.1 (2026-09-14), MSRV 1.89 | `bytemuck`, `safe_arch`; Zlib OR Apache-2.0 OR MIT ([crates.io](https://crates.io/crates/wide)) | "fundamentally incompatible with multiversioning" (survey) | `exp`, `ln`, `powf_simd`, documented "The precision of this function is non-deterministic" ([docs](https://docs.rs/wide/latest/wide/struct.f64x4.html)) | No: compile-time-only selection and unspecified precision. |
| `pulp` 0.22.3 (2026-06-20) | 8 deps, including `raw-cpuid`, `num-complex`, `libm`, `reborrow`; MIT ([crates.io](https://crates.io/crates/pulp)) | Runtime (`Arch::new().dispatch`); "the most verbose I've ever seen" (survey) | None listed ([docs](https://docs.rs/pulp/latest/pulp/)) | No: too many deps. |
| `macerator` 0.5.1 (2026-09-28), MSRV 1.94 | 6 deps; MIT OR Apache-2.0 ([crates.io](https://crates.io/crates/macerator)) | Integrated. Built for burn's CPU backend; the survey says it "isn't used by anything on crates.io other than burn". | No | No. |
| `simdeez` 3.0.1 (2026-03-25) | `cfg-if`, `paste` (`libm` optional); Apache-2.0/MIT ([crates.io](https://crates.io/crates/simdeez)) | Yes | — | No. The survey excludes it as "primarily AI-driven" and "disconcertingly buggy". |
| `multiversion` 0.9.0 (2026-09-06), MSRV 1.86 | proc-macro, `target-features` optional ([crates.io](https://crates.io/crates/multiversion)) | `#[multiversion(targets = "simd")]`, runtime with `std`, compile-time without ([docs](https://docs.rs/multiversion/latest/multiversion/)). Has call overhead on tiny functions (survey). | — | Optional. Hand-rolled dispatch is about 30 lines and needs no dependency. |
| `sleef` 0.3.3 (2026-02-21) | `doubled`; MIT OR Apache-2.0 ([crates.io](https://crates.io/crates/sleef)) | — | Full SLEEF port | No: "Requires nightly feature `portable_simd`" ([README](https://github.com/burrbull/sleef-rs)). |
| `libm` 0.2.16 (2026-01-24), MSRV 1.63 | No deps; MIT ([crates.io](https://crates.io/crates/libm)); now lives in `rust-lang/compiler-builtins` ([README](https://github.com/rust-lang/compiler-builtins/blob/main/libm/README.md)) | — | Scalar ports via musl; `exp.rs` and `log.rs` carry "origin: FreeBSD /usr/src/lib/msun". `exp` uses an arch-specific path only on x86 without SSE2. `exp` and `log`: "the error is always less than 1 ulp" ([exp.rs](https://github.com/rust-lang/compiler-builtins/blob/main/libm/src/math/exp.rs)). | Source algorithm for `vmath`. A test oracle. |
| `simba` 0.10.2 (2026-08-07) | `approx`, `num-complex`, `num-traits`; `wide` optional; Apache-2.0 ([docs](https://docs.rs/simba/latest/simba/simd/index.html)) | — | — | A precedent: nalgebra is generic over scalars and SIMD lanes through `SimdRealField`. |

WASM engine support, from the official feature table ([webassembly.org/features](https://webassembly.org/features/), data file `features.json`):

| Feature | Chrome | Firefox | Safari | Node.js | Wasmtime |
|---|---|---|---|---|---|
| Fixed-width SIMD | 91 | 89 | 16.4 | 16.4 | 0.33 |
| Relaxed SIMD | 114 | 145 | flag only | 21.0 | 15 |
| Threads | 74 | 79 | 14.1 | 16.4 | 15 |

### 2.4 Vectorized `exp` / `ln` / `pow`: the cost centre

#### 2.4.1 What actually costs

- **Term counts.** Computed from `dev/fluids/*.json`, EOS[0]:
  - residual terms per fluid: min 12, median 16, mean 18.6, p90 31, max 56 (Water); 2532 in total;
  - by family: Power 1966, Gaussian 433, Exponential 100, Lemmon2005 18, DoubleExponential 8, NonAnalytic 5, GaoB 2;
  - α⁰ blocks: 2–14 terms per fluid (median 6).
- **CoolProp's GenExp trick.** It evaluates **one exp per term**, `n·exp(t·lnτ + d·lnδ + u)`, with ln τ and ln δ computed once per state. All 15 derivatives cost "about 1 exp + 1 powInt + ~60 flops per term" (`coolprop-map/02` §3.3; `src/Helmholtz.cpp:152-273`).
- **teqp's experience.** In 2021 teqp replaced the Eigen array expression `(n * exp(t * log(tau) + d * log(delta) - c * powIVi(delta, l_i))).sum()` with a scalar loop. The loop hoists `log(tau)` and `log(delta)` and specializes small integer powers. The commit message reads "matches REFPROP speed now for Krypton" ([usnistgov/teqp@1b32291d](https://github.com/usnistgov/teqp/commit/1b32291d4271a79810b0276a8bb6cc0ac8d533fe)). Current teqp keeps per-family loops and a δ = 0 branch that uses `powi` ([multifluid_eosterms.hpp](https://github.com/usnistgov/teqp/blob/master/include/teqp/models/multifluid_eosterms.hpp)). In other words, naive array-over-terms vectorization was not faster than a tight scalar loop.
- **Flashes multiply the kernel.** CoolProp estimates "~25 EOS evals per solve" for a PT flash (`src/Tests/CoolProp-Tests-TermCacheProfile.cpp:330-334`). The HS cascade averages about 16 EOS evaluations and stays under 40 in the worst case (`Web/coolprop/HSFlash.ipynb`).
- **Conclusion.** Throughput of exp per term dominates. The two logs per state are minor. Integer powers come from a per-state δ^k table.

#### 2.4.2 Implementation options

| Approach | Accuracy | SIMD fit | Determinism |
|---|---|---|---|
| `std` `f64::exp`/`ln`, which call the platform libm | glibc "does not aim for correctly rounded results", "within a few ulp" ([glibc manual](https://sourceware.org/glibc/manual/latest/html_node/Errors-in-Math-Functions.html)) | Scalar calls that LLVM will not vectorize (§2.3.1) | "non-deterministic. This means it varies by platform, Rust version, and can even differ within the same execution" ([f64 docs](https://doc.rust-lang.org/std/primitive.f64.html)) |
| Rust `libm` (FreeBSD msun) | exp and log < 1 ulp; pow "nearly rounded" (source comments) | Branches map onto selects; one division (rational approximation) | Pure basic operations, so identical on every target (inference; verify in CI) |
| SLEEF-style: Cody–Waite reduction plus a minimax polynomial, no tables | 1-ulp and faster few-ulp variants. sleef.org says its *scalar* functions "return exactly the same value for the same argument even on different architectures" ([sleef.org](https://sleef.org/)). | Designed for SIMD: "only uses a small number of conditional branches, and all the computation paths are vectorized" ([arXiv:2001.09258](https://arxiv.org/abs/2001.09258)). Gather-free for exp/log (unverified). | Yes for SLEEF's scalar functions; for its vector functions, unverified. An own port of the algorithm is deterministic by construction. |
| Table-driven (2^(j/N) tables, as in modern glibc) | About 0.5 ulp | Needs gathers, so poor in SIMD | Depends on the implementation (unverified) |

**Choice:** port msun `exp`, `log`, `log1p` and `expm1` once into generic `vmath` functions over `R: Real`.

- For `R = f64` the port must be bit-identical to the `libm` crate. That is testable over billions of random inputs plus edge cases.
- For lane types it vectorizes with masks.
- If benchmarks show that the division hurts, switch to a SLEEF-u10-style polynomial. It remains one generic implementation for all R, so scalar and SIMD stay identical; only parity with `libm` is lost.
- `ln(1 − e^(−x))` (Planck–Einstein terms) should use `ln(-expm1(-x))`, as `coolprop-map/02` §3.7 already notes.
- `core` has no `exp` or `ln` even on nightly. `core_float_math` covers only floor, ceil, round, trunc, fract, `mul_add`, div/rem_euclid, powi, sqrt and cbrt, because "the blocker for others is the quality of our `libm` implementations" ([#137578](https://github.com/rust-lang/rust/issues/137578)). Owning `vmath` therefore also keeps `no_std` possible.
- On `wasm32-unknown-unknown`, `std` has no system libm and uses the Rust `libm` port ([archived libm README](https://github.com/rust-lang/libm)). Results are therefore the same in every browser for a given binary, but they differ from native glibc.

#### 2.4.3 ULP budget (analysis)

Notation: u = 2⁻⁵³, the unit roundoff; one ulp of a double near 1 is 2u ≈ 2.2·10⁻¹⁶. φ_k is the k-th term.

- **Per-term relative error** is about (|arg_k| + c)·u.
  - With the folded form, rounding of the *argument* is amplified: a relative error of exp(x) equals the absolute error of x.
  - At δ = 10⁻³ and d = 10, |d·lnδ| ≈ 69, which gives about 10⁻¹⁴ per term.
  - CoolProp has the same floor. That is consistent with the ≤ 6.4·10⁻¹² α^r agreement at δ = 0.05 reported in `coolprop-map/02` §3.7, which also involves cancellation.
- **Summation.** Ordered summation adds at most (n − 1)·u·Σ|φ_k|. At low δ, α^r of the MBWR-converted fluids is a difference of O(10) terms (`coolprop-map/02` §3.1), so relative-to-result tolerances blow up.
  - Use the bound |Δ| ≤ K·u·Σ_k (|arg_k| + n)·|φ_k| with K ≈ 4.
  - A test-only kernel output can return that condition sum.
- **Lanes across states.** Same operations, so Δ = 0.
- **Lanes across terms.** Only the order changes, so |Δ| ≤ n·u·Σ|φ_k|.

### 2.5 Where SIMD fits a Helmholtz-EOS library

#### 2.5.1 Across terms against across states

Lane utilization is computed from the term counts above. "Per family" pads each term family to a multiple of W. "Single block" merges all separable families into one padded block, which carries unused exponent parts per term.

| Aspect | Across terms (one state) | Across states (same fluid, batch) |
|---|---|---|
| Parallel width | 12–56 terms (median 16) | Batch size |
| Lane utilization, W = 4 | 0.889 per family / 0.941 single block | 1.0 (tail runs scalar) |
| Lane utilization, W = 8 | 0.746 per family / 0.860 single block | 1.0 |
| Coefficient access | Vector loads | Broadcast scalar loads (L1-resident) |
| Integer exponents l, d | Vary by lane: gathers, or sort terms by l | Uniform per term: scalar control flow |
| ln τ, ln δ | 2 scalar per state | 2 vector per W states |
| Reductions | One horizontal sum per derivative: 6 (order 2) to 15 (order 4) per state | None |
| Result against scalar | Reordered sum, a few ulp | **Bit-identical** |
| Special states (δ = 0, near (1,1) for NonAnalytic) | Branch per state | Partitioned out |
| Helps | Single-call latency | Throughput: batches and lockstep density solves |
| Expected gain on the α^r kernel (estimate, unverified) | About 1.3–2× with AVX2 for a 16-term fluid | About 3–3.5× AVX2, 5–7× AVX-512, 1.6–1.8× NEON or simd128 (2 lanes) |

A third, smaller axis is SIMD across derivative orders: the τ- and δ-jets of a separable term combine as an outer product. It is a micro-optimization inside the scalar kernel and comes last.

#### 2.5.2 Classification of the algorithms

| Algorithm | Class | SIMD axis | Thread level | Notes (evidence) |
|---|---|---|---|---|
| α^r and α⁰ term sums and derivative bundles | Data-parallel | States (then terms) | Requests and chunks | 99.8 % of terms are separable n·F(τ)·G(δ) (`coolprop-map/02` §7) |
| Properties from the bundle (p, h, s, c_v, c_p, w, Jacobian-ratio partials) | Data-parallel, branch-free | States | — | `coolprop-map/01` §7 |
| Superancillary Chebyshev evaluation (degree 12, 53–91 pieces) | Data-parallel after piece lookup | States: scalar or branchless search, gather 13 coefficients, vector Clenshaw; sort by T | Chunks | About 41 ns/eval batched in the oracle (`coolprop-map/03` §3.3) |
| Ancillaries, melting curves, surface tension | Data-parallel (Horner, exp) | States | — | |
| Transport correlations (dilute: exp of a polynomial in ln T*; residual: term sums in τ, δ) | Data-parallel | States | — | Same kernel shape as the EOS |
| Tabular interpolation (TTSE, bicubic, SBTL) | Gather plus a small polynomial; memory-bound | States (modest) | Chunks | SVDSBTL about 300 ns per point (§2.10) |
| ρ(T,p) single-phase solve | Iterative, bracketed | **Lockstep with masks**: fixed-budget Newton/Halley inside superancillary brackets; lanes that fail retry in scalar | Requests | `coolprop-map/03` §7 |
| QT, PQ (superancillary plus the T(ln p) inverse), DP | Mostly evaluation, or a short lockstep | States | Requests | |
| Phase determination | Comparisons | Used to *partition*, not to compute | — | §2.2.2 |
| P+X, T+X, D+X cascades; HS homotopy (16–40 EOS evaluations, adaptive subdivision); superancillary inversion (TOMS748); VLE (Maxwell, Akasaka); critical points | Branchy and sequential | Only inside the EOS kernel calls | Requests | `coolprop-map/03` §7 |
| Mixtures (stability, Rachford–Rice, SS + Newton, dense LA with N ≤ 20) | Branchy | None | Requests | `coolprop-map/04` |
| NonAnalytic terms (5 terms, Water and CO₂) | Fractional pow, singular at (1, 1) | Scalar | — | `coolprop-map/02` §3.7 |

### 2.6 Side-by-side implementations without duplicating the math

**Precedents:**

- **teqp (C++).** Term `alphar` is templated on `TauType` and `DeltaType`, and derivatives come from autodiff or the complex step. The paper reports "minimal computational overhead and negligible loss in numerical precision" against analytic derivatives ([Bell, Deiters, Leal 2022, doi:10.1021/acs.iecr.2c00237](https://www.nist.gov/node/1716461)).
- **FeOs / num-dual (Rust).**
  - `Residual<N, D: DualNum<f64> + Copy>`: the same functions are evaluated with `f64`, `Dual64`, `HyperDual64` and so on ([feos-core docs](https://docs.rs/feos-core/latest/feos_core/trait.Residual.html)).
  - Monomorphization means "no checks at runtime are necessary" ([Rehner & Bauer 2021](https://www.frontiersin.org/journals/chemical-engineering/articles/10.3389/fceng.2021.758090/full)).
  - num-dual's base type is limited to f32 and f64 (`DualNumFloat`), so it cannot carry SIMD lanes ([docs](https://docs.rs/num-dual/latest/num_dual/)). Versions: num-dual 0.15.0, feos 0.10.1 ([crates.io](https://crates.io/crates/num-dual)).
- **simba / nalgebra.** One generic code base runs on scalars and on `WideF64x4` lanes ([docs](https://docs.rs/simba/latest/simba/simd/index.html)).
- **CoolProp.** No precedent. It has 992 lines of hand-written per-term derivatives plus 634 lines of plumbing. Its value-only `one_mcx` test hook is the seed of the generic design (`coolprop-map/02` §3.3).

**Design rules:**

1. **One numeric trait.** `Real` has `Mask` and `select`, so term code needs no `if` on values. Per-term integers (d, l, counts) are scalar arguments, so lanes never diverge in control flow.
2. **Two derivative routes over the same `R`.** The fast route uses B-factor or jet recurrences. The AD route (own jets, or `num-dual` as a **dev-dependency** through a local `impl Real for Dual64`, which the orphan rule allows) verifies them. D2 decides which route is primary.
3. **Executors own loops, threads and dispatch.** Model code never loops over states. Remainders run through the same generic code with `R = f64`.
4. **Dispatch happens once per batch.**
   - First `cfg!(target_feature = …)`, which serves static builds.
   - Then runtime detection, cached in `OnceLock<Kernels>`.
   - Then an `unsafe` call into a `#[target_feature(enable = "avx2,fma")]` entry point whose generic body is `#[inline(always)]`.
   - Enabling `fma` changes no results: Rust never contracts, and the kernels never call `mul_add`.
5. **Cargo features are additive and unified across the dependency graph.**
   - Features may add capability (`rayon`, `simd`, `std`) but must not change numerics.
   - The executor is chosen at runtime: `ExecPolicy::{Reference, Auto, Parallel{..}}`.
   - Because lanes across states are bit-identical, `simd` can be a default feature without changing anybody's results.
6. **The scalar path is the source of truth.** `Reference` is the `R = f64` instantiation. Every accelerated path is tested against it (§3.3).

### 2.7 WASM threads in 2026

| Route | Requirements and status | Usable in v1? |
|---|---|---|
| `wasm32-unknown-unknown` + atomics + `wasm-bindgen-rayon` 1.3.0 (released 2024-12-21) | "still only available in nightly". The README was last updated 2025-11-21 and tested with `nightly-2025-11-15`. Needs `-C target-feature=+atomics,+bulk-memory`, `build-std = ["panic_abort", "std"]` and `--target web`. `initThreadPool` must run on the main thread ([README](https://github.com/RReverser/wasm-bindgen-rayon); [crates.io](https://crates.io/crates/wasm-bindgen-rayon)). | No (nightly). |
| `-Z build-std` | The 2026 project goal is to "complete the remaining design work for #3874 and #3875 and start on implementation" ([goal](https://goals.rust-lang.org/2026/build-std.html)). | Not stable in 2026. |
| Cross-origin isolation | `SharedArrayBuffer` and shared `WebAssembly.Memory` need COOP/COEP and a `crossOriginIsolated` check ([MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer)). | A deployment burden for embedders. |
| `wasm32-wasip1-threads` (wasi-threads) | Tier 2 with a prebuilt std, but "not a stable target". Engines implement it "likely behind a flag" (Wasmtime `--wasi threads`, WAMR) ([rustc docs](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasip1-threads.html)). | Server-side experiments only. |
| shared-everything-threads | Phase 1; core threads proposal at Phase 4 ([proposals](https://github.com/WebAssembly/proposals)). | No. |
| Share-nothing workers | Each Web Worker instantiates its own module and the batch is split in JS. Works on stable; memory is per worker. | **Yes.** |

### 2.8 GPU as a far-future backend

| Fact | Source |
|---|---|
| WGSL defines only `f32`, and `f16` behind an extension. There is no `f64` (Candidate Recommendation Draft, 21 Sept 2026). | [W3C WGSL](https://www.w3.org/TR/WGSL/) |
| "Metal does not support the double, long long, … data types." An Apple engineer confirmed that Metal-based WebGPU implementations would not offer an f64 extension. | [Metal Shading Language Specification 4.1, §2.1](https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf); [public-gpu, 2022-04](https://lists.w3.org/Archives/Public/public-gpu/2022Apr/0006.html) |
| wgpu 30.0.1 `SHADER_F64` is native-only (Vulkan). "64-bit floating point operations are frequently between 16 and 64 times slower than equivalent operations on 32-bit floats." | [wgpu Features](https://docs.rs/wgpu/latest/wgpu/struct.Features.html); [crates.io](https://crates.io/crates/wgpu) |
| CubeCL (0.10.0 stable, 0.11.0-pre.4) targets CUDA, HIP, Metal, SPIR-V, WGSL and CPU. The v0.10.0 README says it "is currently in **alpha**" and that unsupported instructions give "a compilation error at runtime". The main-branch README (rewritten 2026-05) no longer states a maturity level, and there is no 1.0. | [v0.10.0 README](https://github.com/tracel-ai/cubecl/blob/v0.10.0/README.md); [README](https://github.com/tracel-ai/cubecl); [crates.io](https://crates.io/crates/cubecl) |
| rust-gpu (spirv-builder 0.10.0, 2026-10-01) is "at an early stage … not yet production-ready" and is built from the latest `main`. | [README](https://github.com/Rust-GPU/rust-gpu); [crates.io](https://crates.io/crates/spirv-builder) |
| Double-single emulation gives about 48 bits (about 14 digits) and is not IEEE binary64. | [luma.gl](https://luma.gl/docs/api-guide/shaders/gpu-floating-point-precision) |

**Implications:**

- f32 cannot carry α^r cancellation (MBWR pairs, low δ) or near-critical derivatives.
- f64 on a GPU means CUDA or Vulkan on hardware with a good f64 rate.
- A realistic GPU use is f32 surrogates (SBTL-like tables) for CFD, not the EOS.
- Do nothing now. Flat SoA coefficient arrays and the `Real` generics keep the option open.

### 2.9 Floating-point determinism across Linux, Windows and WASM

- **Rust basic operations are deterministic.** +, −, ×, ÷, %, sqrt and `mul_add` "exactly match IEEE 754-2008 (with roundTiesToEven …)". FMA contraction is precluded. Only NaN bit patterns are non-deterministic. On 32-bit x86, x87 return values alter NaN payloads. On i586 (no SSE2) results can differ from IEEE "even outside of NaNs", so i586 is outside the bit-identity claim. Inline assembly must leave the FP control bits unchanged ([RFC 3514](https://rust-lang.github.io/rfcs/3514-float-semantics.html)).
- **Transcendentals and algebraic operations are not deterministic.** `std` transcendentals are non-deterministic (§2.4.2), and so are the algebraic operations (§2.3.1).
- **`mul_add` without hardware FMA calls libm.** It "_may_ be more performant … if the target architecture has a dedicated `fma`" ([f64 docs](https://doc.rust-lang.org/std/primitive.f64.html)). Without one it is a libm call: 211 ns/iter against 1 ns/iter in a 2015 benchmark ([users.rust-lang.org](https://users.rust-lang.org/t/why-does-the-mul-add-method-produce-a-more-accurate-result-with-better-performance/1626)). The x86-64 baseline has no FMA.
- **The oracle itself is platform-dependent.**
  - GCC defaults to `-ffp-contract=fast` outside strict ISO modes ([GCC](https://gcc.gnu.org/onlinedocs/gcc/Optimize-Options.html)). Clang defaults to `on`, which contracts within statements ([Clang](https://clang.llvm.org/docs/UsersManual.html)).
  - CoolProp's CMake sets no `-ffp-contract` or `-march` (grep of `CMakeLists.txt` and `cmake/`). The x86-64 baseline wheels therefore have no FMA to fuse, while aarch64 builds can fuse (inference).
  - Upstream 5b9c32ac saw 1–3 ulp differences from FMA contraction (`coolprop-map/10` §6).
- **WASM.** Wasm 3.0 (2025-09-17) added relaxed SIMD and a *deterministic profile* "for every instruction with otherwise non-deterministic results". Platforms may adopt it but are not required to ([announcement](https://webassembly.org/news/2025-09-17-wasm-3.0/)).

| Comparison | Why it can differ | Tolerance |
|---|---|---|
| SIMD across states against scalar | It cannot (same operations, same `vmath`) | **Bitwise** |
| Any thread count against sequential | It cannot (per-point independence, no reductions) | **Bitwise** |
| Own `vmath` (f64) against the `libm` crate | Faithful port | Bitwise; any difference is a porting bug |
| Rust on Linux, Windows, macOS and wasm32 | `vmath`, no FMA | Bitwise. Verify with a result hash in CI. |
| Across-terms SIMD against scalar | Summation order | \|Δ\| ≤ n·u·Σ\|φ_k\| |
| Rust reference against the CoolProp oracle (x86-64 Linux wheel) | glibc exp, summation order, folded exp argument | `coolprop-map/10` classes (`term` 1e-13, `prop` 1e-12, or 1e-8 near critical; `flash` 1e-9; absolute floors), plus the α^r bound K·u·Σ(\|arg_k\| + n)\|φ_k\| where sums cancel |

### 2.10 Performance baselines

| Operation | Measured | Conditions | Source |
|---|---|---|---|
| QT via superancillary | 0.45 µs (0.5 µs, against 17–69 µs without the superancillary) | Oracle, Water | `coolprop-map/03` §3.1; `coolprop-map/10` §4 |
| PQ | 0.64 µs; first call +1.3–1.7 ms | Oracle | `coolprop-map/03` §3.1 |
| Superancillary batch evaluation | About 41 ns/eval | `eval_sat_many` (C++ loop) | `coolprop-map/03` §3.3 |
| DT (two-phase) / PT / DP | 6.6 / 19–27 / 11–18 µs | Oracle | `coolprop-map/03` §3.1 |
| HT, ST, TU / D+X / HS | 23–59 / 31–41 / 43–82 µs | Oracle | `coolprop-map/03` §3.1 |
| PH, PS, PU | 7 µs two-phase; 119–376 µs single-phase | Oracle | `coolprop-map/03` §3.1 |
| `update(D,T)` + h + c_p | 1.5–10.7 µs | Oracle | `coolprop-map/02` §5 |
| 3-component mixture PT | 20–33 ms | Oracle | `coolprop-map/03` §3.1 |
| HS for Air (no superancillary) | About 160 ms → about 40 µs | v8.0.0 change | `Web/coolprop/changelog.rst:17` |
| SVDSBTL native `update` + ρ + T | Under 200 ns single-phase; 1–1.5 µs two-phase | C++, Apple Silicon | `Web/coolprop/SVDSBTL.rst:207-211` |
| SVDSBTL `fast_evaluate` | About 300–330 ns per point (4 outputs) | Supercritical, 10k batch | `Web/coolprop/SVDSBTL.rst:259-266` |
| IF97 (p, h) inversion | About 2 µs per probe | | `Web/fluid_properties/IF97.rst:314` |
| BICUBIC&HEOS HmassP via the C API | 0.8 µs per call | | `Web/coolprop/snippets/HighLevelLowLevel.cxx.output` |
| TTSE and bicubic density | About 1 µs; more than 120× faster than the EOS | 2014 paper | [Bell et al. 2014 (PMC)](https://pmc.ncbi.nlm.nih.gov/articles/PMC3944605) |
| teqp PR cubic `Ar01` / `Ar04n` | 567 ns / 1.11 µs including Python ("calling overhead is usually on the order of 1 microsecond") | teqp 0.22.0 docs (latest PyPI release is 0.23.2) | [teqp docs PDF](https://pages.nist.gov/teqp-docs/en/main/_downloads/teqp.pdf) |
| Superancillary against full VLE | "approximately 400 times faster" | Multiparameter EOS | [Bell 2021, IJT, doi:10.1007/s10765-021-02824-x](https://www.nist.gov/node/1716426) |
| Superancillary against REFPROP iteration | "hundreds to thousands of times faster" | 147 REFPROP fluids | [Bell 2024, JPCRD 53, doi:10.1063/5.0191228](https://www.nist.gov/node/1861001) |
| REFPROP IAPWS-95 against SBTL (computing-time ratio) | p(v,u) 243 (liquid) / 434 (gas); T(p,h) ≈15 000 / 6 760. CFD runs 6–10× faster than direct IF97. | 2015, i7-4500U, Intel compiler, phase known | [IAPWS G13-15](https://iapws.org/documents/release/SBTL) §7.3, §8 |
| `SpeedTest.cpp` | No numbers. It is only a `clock()` loop comparing HEOS and REFPROP `update` | | `src/SpeedTest.cpp` |

No published C++-level multiparameter α^r timing was found (teqp's matched-REFPROP commit gives no figure), so it is **unverified**. All oracle figures above include Python overhead.

---

## 3. Recommendations for coolprop-rs

### 3.1 Kernel architecture: what is computed and how it is executed

Module names are placeholders; D1 decides the crate layout. Start as modules of one crate and split them only when a second consumer appears.

```rust
// ── num: numeric abstraction; knows nothing about fluids ─────────────────────
pub trait Real: Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self>
              + Div<Output = Self> + Neg<Output = Self> {
    type Mask: Copy;                    // bool for f64; lane mask for SIMD
    const LANES: usize;                 // 1 for f64 and AD jets
    fn splat(x: f64) -> Self;
    fn exp(self) -> Self;               // = vmath::exp::<Self>: ONE algorithm for every impl
    fn ln(self) -> Self;                // = vmath::ln::<Self>
    fn powi(self, n: i32) -> Self;      // n is per-term data (scalar), never per-lane
    fn lt(self, rhs: Self) -> Self::Mask;
    fn select(m: Self::Mask, a: Self, b: Self) -> Self;
}
// impls: f64 (reference) | F64x2 / F64x4 / F64x8 (lanes = states, per-ISA cfg) | Jet<f64, N> (AD, tests)
pub mod vmath { /* exp, ln, ln_1p, exp_m1 generic over Real; msun port; no std calls */ }

// ── model: WHAT is computed (generic, branch-free, no loops over states) ─────
pub struct Vars<R> { tau: R, delta: R, ln_tau: R, ln_delta: R, delta_pow: [R; 7] } // per state
pub struct PowerBlock { n: Box<[f64]>, t: Box<[f64]>, d: Box<[f64]>, c: Box<[f64]>, l: Box<[i32]> } // SoA, padded in datagen
impl PowerBlock {
    #[inline(always)]
    pub fn accumulate<R: Real, const ORD: usize>(&self, v: &Vars<R>, acc: &mut Derivs<R, ORD>) {
        // per term: e = n·exp(t·lnτ + d·lnδ − c·δ^l); B-factor recurrence → acc   (no branches on values)
    }
}
pub fn residual<R: Real, const ORD: usize>(eos: &ResidualEos, tau: R, delta: R) -> Derivs<R, ORD>;
pub mod relations { /* pressure, enthalpy, cp, w … : fn(&Derivs<R, _>, t: R, rho: R, r: f64) -> R */ }

// ── exec: HOW it is executed (loops, lanes, threads, dispatch) ───────────────
#[non_exhaustive]
pub enum ExecPolicy { Reference, Auto, Parallel { chunk: usize } }   // public knob; Reference = R = f64
trait Backend { fn residual_batch<const ORD: usize>(&self, eos: &ResidualEos,
                tau: &[f64], delta: &[f64], out: &mut DerivColumns<'_, ORD>); }
struct Scalar;                         // for each state: residual::<f64, ORD>
struct Lanes<L: Real>;                 // full chunks of L::LANES states; tail via Scalar → bit-identical
#[cfg(feature = "rayon")]
struct Parallel<B: Backend> { inner: B, chunk: usize }   // fixed-size chunks, independent of thread count

static KERNELS: OnceLock<Kernels> = OnceLock::new();     // detected once; fn-pointer table
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]                   // fma enables nothing unless mul_add is called
fn residual2_avx2(eos: &ResidualEos, tau: &[f64], delta: &[f64], out: &mut DerivColumns<'_, 2>) {
    Lanes::<F64x4>.residual_batch(eos, tau, delta, out)  // #[inline(always)] generic body inherits AVX2
}
// selection: if cfg!(target_feature = "avx2") || is_x86_feature_detected!("avx2") { unsafe { residual2_avx2(..) } }

// ── flash: branchy algorithms; scalar-first, lockstep where it pays ──────────
pub fn rho_tp<R: Real>(eos: &ResidualEos, t: R, p: R, bracket: (R, R), o: &SolveOpts) -> (R, R::Mask /*converged*/);
// scalar flash = rho_tp::<f64>; batch = rho_tp::<F64x4> per partition, then scalar retry of !converged lanes
```

The registry and handles follow `coolprop-map/09` D4: `static FLUIDS: [OnceLock<Arc<Fluid>>; N] = [const { OnceLock::new() }; N];`.

- Public handles are `Arc`s, cloned once per handle, never per evaluation.
- `State` is a plain `Copy` value. `StateRef<'a>` borrows the fluid.
- Derived data (superancillary inverse T(ln p), caloric curves, monotonic intervals) is precomputed offline by datagen. When it has to be built at runtime, it goes in an inner `OnceLock`, never behind a mutex.

### 3.2 Phasing (keeps the TDD rhythm; no speculative layers)

| Phase | Build | Gate to the next phase |
|---|---|---|
| 0 (with the first EOS port) | `Real` for `f64`; `vmath` scalar (bit-tested against `libm`); generic, branch-free term code; immutable registry; `Scalar` executor; batch API shape; `rayon` feature | Oracle fixtures pass. Thread-invariance test passes. |
| 1 (after the PT/DT flashes) | Benchmarks; `Lanes` over `[f64; N]` plus an AVX2 `core::arch` implementation; dispatch table; across-states α^r, properties and superancillary batch evaluation | `Lanes` = `Scalar` bitwise. Gain ≥ 2.5× on AVX2 for α^r, otherwise stop and re-plan. |
| 2 | Partitioned batch flashes; lockstep ρ(T,p) with scalar retry; NEON and simd128 builds; WASM two-build packaging | Bitwise against scalar flashes with cold start. |
| 3 (only if profiling asks) | Across-terms SIMD for single-call latency; AVX-512 level; an optional `Fast` policy (FMA) | Tolerance tests (§2.9). Measured benefit on the target hardware. |
| Far | GPU; WASM threads | Stable `build-std`; a real f64 GPU use case |

### 3.3 Test and benchmark policy

- **L6 kernel tests (bitwise)**, extending `coolprop-map/10` §8.5:
  - `Lanes` against `Scalar`;
  - `Parallel` with 1, 2 and N threads against sequential;
  - batch against single calls;
  - remainder handling (n mod W ≠ 0).
- **Forced dispatch levels.** A test-only override selects each level (scalar, SSE2, AVX2, AVX-512 when present), so one CI machine exercises all of them.
- **CI targets.** Run on x86-64 Linux and Windows, aarch64 (Linux or macOS), and `wasm32-wasip1` with `+simd128` under Wasmtime. Compare an FNV-1a hash of a canonical output grid; it must be identical everywhere.
- **`vmath`.** Bitwise against `libm` 0.2.16 (pinned) over random and edge inputs: subnormal results, overflow thresholds near ±709.78, NaN and ±∞.
- **Accelerated paths that reorder sums.** Use the bound-based tolerance (§2.4.3) and never a bare relative epsilon.
- **Benchmarks** (dev-dependencies only: `criterion` 0.8.2 (2026-02-04), [crates.io](https://crates.io/crates/criterion), is preferred; `divan` 0.1.21 is an alternative, but it has had no release since 2025-04-10 even though its repository is still active, [crates.io](https://crates.io/crates/divan)):
  - ns per state for α^r orders 1, 2 and 4 on Water (56 terms), Methane (40), R134a (21), Propane (18) and a 12-term fluid;
  - µs per flash for each input pair;
  - thread scaling from 1 to N cores, same fluid against different fluids;
  - RSS after loading every fluid; first-use latency per fluid.
- **C++ baseline.** Build CoolProp's Catch2 benchmark cases (for example `[cubic][!benchmark]` in `src/Tests/CoolProp-Tests-CubicU.cpp` and the hidden `[HSU_D_bench]` case in `-HSU_D.cpp`) or a small C++ harness in the scratch area. Python-level oracle numbers cannot be compared one-to-one with native Rust.

### 3.4 Initial performance targets

These are estimates from the analysis above. Confirm them with the Phase 1 benchmarks before treating them as gates.

| Operation (single core, x86-64) | Target | Basis |
|---|---|---|
| α^r bundle to order 2, 16–20 terms, scalar | ≤ 0.3 µs | About one exp plus O(30) flops per term (§2.4.1) |
| Same, AVX2 across states | ≤ 0.1 µs per state amortized | 4 lanes at about 75–85 % efficiency (estimate) |
| Properties at (T, ρ) (p, h, s, c_p, w) | ≤ 0.5 µs | Bundle plus relations |
| QT or PQ via superancillary | ≤ 0.1 µs | Oracle 0.45 µs including about 0.3 µs of wrapper |
| PT flash, single phase | ≤ 3 µs | About 25 α^r evaluations in CoolProp's estimate; a superancillary bracket allows fewer |
| PH flash, single phase | ≤ 15 µs | Oracle 119–376 µs |
| Thread scaling, independent requests | ≥ 0.9·N up to physical cores, same or different fluid | No shared writes (§2.1.3) |
| Memory | Per-fluid model ≤ 25 KiB plus superancillary ≤ 25 KiB; no per-thread model copies; `State` ≤ 256 B | `coolprop-map/09`, `coolprop-map/03` |

---

## 4. Warnings

- **Nightly-only routes:** `std::simd` (#86656), `sleef` (needs `portable_simd`), WASM threads (`build-std`, atomics) and SVE intrinsics. None may sit on the critical path.
- **`f64::exp`, `ln` and `powf` are documented as non-deterministic** across platforms, Rust versions and even calls within one execution. Bit-exact fixtures or cross-platform hashes are impossible if kernels call them.
- **Without hardware FMA, `mul_add` is a slow libm call**, and x86-64 baseline builds have no FMA. Keep `mul_add` out of portable kernels.
- **Target features do not propagate into non-inlined callees.** A generic body that is not `#[inline(always)]` silently compiles for the baseline. Check the assembly of each hot kernel and keep a benchmark regression guard.
- **WASM has no runtime feature detection.** A simd128 binary fails to load on engines without SIMD, so ship two builds. Relaxed SIMD is behind a flag in Safari, and `relaxed_madd` rounds differently per engine.
- **Avoid these crates for this purpose:** `wide` (compile-time selection only, unspecified precision), `simdeez` (reported buggy) and `sleef-rs` (nightly). `fearless_simd` only reached 1.0 on 2026-09-21; pin it if it is ever adopted.
- **Do not put an `Arc` in `State`, and do not look fluids up through `RwLock<HashMap>` per call.** Both are shared writes that destroy same-fluid scaling, even though they look harmless.
- **δ = 0 in the folded exp form.** With d = 0 terms, `0·ln 0 = 0·(−∞)` gives NaN; CoolProp has the same issue (`coolprop-map/02` §3.7). Partition δ = 0 (ideal-gas limit, virials) to a dedicated path, as teqp does with `powi`. Pad remainder lanes by duplicating a valid lane, not with zeros.
- **Masked or padded lanes may compute inf or NaN.** Rust assumes the default FP environment (RFC 3514). Hosts that unmask FP exceptions (Delphi, polled by Excel/VBA, per `include/CoolProp/FPUGuard.h`) would trap, so a future C ABI needs a CoolProp-style FPU guard at the boundary.
- **The oracle's last bits depend on platform and compiler** (GCC and Clang contraction defaults). Generate fixtures on one pinned platform (x86-64 Linux) and record it in the fixture metadata.
- **Warm starts make batch results depend on order and chunk size.** Keep them opt-in and keep the default bitwise-equal to scalar.
- **AVX-512 gains vary by microarchitecture.** Feature detection checks whether AVX-512 "is present, not whether it's actually fast" (Davidoff 2026, about `multiversion`), so benchmark before enabling it by default. fearless_simd's AVX-512 level targets Ice Lake to avoid early slow implementations.
- **GPU f64 is absent** on WebGPU and Metal, Vulkan-only in wgpu, and 16–64× slower than f32. CubeCL is alpha and rust-gpu is early-stage.
- **Runtime-built derived data is a tail-latency trap** (CoolProp: 34–51 ms per fluid for caloric superancillaries). Precompute it in datagen.

## 5. Open questions

1. `vmath` origin: an exact msun port (bit-parity with the `libm` crate, one division per exp) or a SLEEF-u10-style polynomial (faster, no `libm` parity)? Decide from the Phase 1 benchmark. Both keep scalar = SIMD.
2. Is a `Fast` policy with FMA and reassociation (aarch64, x86-64-v3) worth losing cross-platform bit-identity? It would need its own tolerance class.
3. Is across-terms SIMD worth its complexity for the median 16-term fluid, given that teqp found a tight scalar loop matched REFPROP? Needs a single-call latency benchmark.
4. Default batch output layout for the C ABI: columns (SIMD-natural) or rows (CoolProp `fast_evaluate` parity)?
5. Web: is share-nothing workers plus a single-threaded module enough, or is a nightly threaded build needed for some user?
6. Should `no_std` be supported? It is cheap with an own `vmath`, but runtime detection then needs `cpufeatures` or static features only.
7. Should the derivative bundle in `State` be eager (order 2 at flash end) or lazy? Recommended: eager order 2, with higher orders recomputed by a pure function (`coolprop-map/01` open question 2). Needs the flash-cost benchmark.
8. Can a C++-level CoolProp benchmark build be done in the scratch area (never in `reference/`) to replace the Python-level baselines in §2.10?
9. Which CI runners cover aarch64 and Windows, so that the cross-platform bit-identity claim is actually verified?

## Verification log

Date: 2026-10-04. Adversarial fact-check against primary sources: crates.io API (versions, dates, licences, MSRV, dependency lists), docs.rs, GitHub sources and READMEs, the Rust blog and std/Reference docs for 1.99.0, rust-lang issues, webassembly.org `features.json`, W3C, Apple's MSL spec, IAPWS G13-15 (PDF), NIST publication pages, the teqp docs PDF, and CoolProp v8.0.0 sources at `reference/CoolProp`. Term statistics and lane-utilization figures were recomputed from `dev/fluids/*.json`.

**Claims checked:** about 110.

**Confirmed (selection).**
- **Crate versions, dates and MSRVs:**
  - rayon 1.12.0 (MSRV 1.80), arc-swap 1.9.2, crossbeam-utils 0.8.23, fearless_simd 1.0.0 (2026-09-21, MSRV 1.89, only `libm` optional), wide 1.7.1 (MSRV 1.89);
  - pulp 0.22.3 (8 required deps), macerator 0.5.1 (6 deps, MSRV 1.94), simdeez 3.0.1, multiversion 0.9.0 (MSRV 1.86), sleef 0.3.3, libm 0.2.16 (MSRV 1.63, MIT);
  - num-dual 0.15.0, feos 0.10.1, wasm-bindgen-rayon 1.3.0, wgpu 30.0.1, cubecl 0.10.0 / 0.11.0-pre.4, spirv-builder 0.10.0 (2026-10-01), criterion 0.8.2, divan 0.1.21, cpufeatures 0.3.1.
- **Stabilizations:** 1.79 inline `const` in repeat expressions; 1.80 `LazyLock`; 1.70 `OnceLock`; 1.86 target_feature 1.1; 1.87 safe intrinsics; 1.89 AVX-512; 1.98 algebraic float methods (2026-08-20); NEON 1.59; simd128 1.54; relaxed-simd 1.82; `is_x86_feature_detected!` 1.27; `is_aarch64_feature_detected!` 1.60.
- **Nightly-only status:** `portable_simd` is still nightly on the 1.99.0 docs, and #86656 is open.
- **Rust docs and RFCs:** all quotes from the Reference, std, rustc and RFC 3514 match the sources.
- **WASM:** every cell of the engine support table matches `features.json`; threads is at Phase 4 and shared-everything threads at Phase 1. The Wasm 3.0 deterministic profile is confirmed. The wasm-bindgen-rayon README matches (nightly-2025-11-15, updated 2025-11-21), as do the build-std 2026 goal and the wasip1-threads page.
- **rayon:** the release notes for 1.7.0 and 1.11.0, and the doc quotes, match.
- **Concurrency sources:** Bos chapters 7 and 9 (about 300 ms → about 3 s), the `Arc` sources (`library/alloc/src/rcs/arc.rs`) and the `std_detect` cache path match.
- **Libraries and papers:** quotes from glibc, `libm` (`exp`/`log` "less than 1 ulp", `pow` "nearly rounded"), wide's "non-deterministic" precision, and the GCC/Clang `-ffp-contract` defaults match. The teqp commit 1b32291d (2021-09-12), the Bell 2014/2021/2022/2024 quotes, the IAPWS G13-15 Table 25, the CFD 6–10× figure and the i7-4500U setup, and the teqp 567 ns / 1.11 µs timing all match.
- **CoolProp citations:** every cited CoolProp line was checked (`deriv_counter`, the `get_invlnp` lock, the HumidAir `thread_local`, SVDSBTL, the changelog, LowLevelAPI, IF97, `fast_evaluate`, TermCacheProfile, HSFlash). Term counts (2532; min 12, median 16, mean 18.6, p90 31, max 56) and lane utilizations (0.889/0.941 and 0.746/0.860) reproduce exactly.

**Corrections made.**
1. Rust 1.99.0 was released 2026-10-01. 2026-09-28 is the commit date in the rustdoc version string, not a docs build date.
2. Re-entrant `OnceLock` initialization is *unspecified* (it currently deadlocks and may become a panic). It is not guaranteed to deadlock.
3. `CachePadded`: 128 B also applies to arm64ec and powerpc64. The "spatial prefetcher" rationale is the docs' reason for Intel only.
4. The SVDSBTL figure of 2.25× at 4 threads is for the table *build*, not for evaluation.
5. Survey quotes:
   - fearless_simd: the "most complete all-in-one solution" wording was not in the source. Replaced with the verbatim quote.
   - macerator: "used only by burn" was a paraphrase. Replaced with the verbatim quote.
   - simdeez: added that the survey excludes it as "primarily AI-driven".
   - The AVX-512 "doesn't verify performance" wording was not verbatim. Replaced with the verbatim quote and its context (`multiversion`).
   - Added a note that the survey's author is a fearless_simd maintainer, which he discloses.
6. SLEEF: its determinism quote is about the *scalar* functions. Determinism of the vector functions and "gather-free" are now marked (unverified).
7. `core_float_math` covers more than "floor through sqrt and powi" (it also has `mul_add`, euclid, cbrt and others). Its exp/ln conclusion is unchanged.
8. Metal f64: replaced the mailing-list question with the MSL 4.1 spec ("Metal does not support the double … data types").
9. CubeCL: the "alpha" and "compilation error at runtime" quotes come from the v0.10.0 README. The current README no longer states a maturity level.
10. RFC 3514: added that i586 (x87, no SSE2) can differ even outside NaNs, so it is outside the bit-identity claim.
11. The LLVM SVML RFC is from 2016. Whether rustc maps a vector library by default today is marked (unverified).
12. HSU_D benchmarks are tagged `[HSU_D_bench][.]`, not `[!benchmark]`.
13. Added missing facts:
    - licences for simdeez, simba and sleef;
    - the simba release date;
    - rayon's release-date discrepancy (crates.io 2026-04-14, RELEASES.md 2026-04-13);
    - the wasm-bindgen-rayon 1.3.0 release date (2024-12-21);
    - teqp's latest release (0.23.2);
    - a 2026 SVE goal citation;
    - primary sources for NEON (stdarch) and for std `RwLock::read`.

**Recommendation changes.**
- Benchmarks (§3.3): criterion is now *preferred* over divan, because divan has had no release since 2025-04-10.
- No other recommendation changed. Nothing corrected above affects K1–K19:
  - fearless_simd stays a fallback only;
  - `std::simd`, `sleef` and WASM threads stay nightly-only;
  - all stabilization versions the design relies on are confirmed.

**Unverified.**
- The expected SIMD gains and performance targets (§2.5.1, §3.4) are estimates.
- Chunk-size amortization (256–1024 points).
- Whether SLEEF's vector functions are deterministic and gather-free.
- That the msun port is bit-identical on every target (inference; to be checked in CI).
- That std on `wasm32-unknown-unknown` uses compiler-builtins' `libm` (supported only by the libm README's stated goal).
- Whether rustc enables a vector math library by default.
- The luma.gl page: the direct fetch returned 404, and the "about 48 bits / about 14 digits" figure is confirmed only via search snippets of luma.gl's fp64 docs.
- The Arm NEON blog (HTTP 403; NEON stability is confirmed from stdarch instead).
- Internal `coolprop-map/NN` figures, which were not re-derived here except the term statistics.
