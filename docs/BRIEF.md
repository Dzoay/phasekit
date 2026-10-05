# coolprop-rs design brief (requirements + decisions the architecture must make)

## 1. The user's requirements, verbatim

> I want you to put in this folder a plan to migrate coolprop to rust to give me a modular extensible
> memory safe cross platform connected fluid properties library. i have been in this code before I I
> think there is rot in there we can eliminated, I would want DRY, SOLID code in idomatic modern rust
> with minimal dependencies so other projects can use this with heavy OSS clear type constraints. Also
> I would target windows, linux, and wasm platforms. We would want to have a kernal we can optimize for
> speed and scalability in context were paralell requests on the same and different fluids are going
> to come to the kernel at the same time. Rusts immutability should help us here. [...] make a step by
> step plan using coolprop as varification but with source material when we find bugs. We will TDD this
> step by step and extend it to be a *material* properties library rather than just *fluids* - that is
> an extension we will need to add in the future. Assume we will be seeking to add the maths for all
> states of matter too in the future, so modularity is again KEY here, but we need to start somewhere
> so coolprop fluids are first. You should be able to conserve memory by only loading what is needed
> for the requests coming in. Assume highfrequency and batch requests. Layers will be important but we
> must not over engineer this.

Follow-up from the user (mid-session):

> for the future is the possibility of introducing parallel computation to the algorithms too. so
> separate concerns with side by side implementations for different architectures will be needed I
> think. Some will not suite that though. Small levels of parallelism would be interesting like SIMD to
> investigate though.

Interpretations we are making (flag if you disagree):
- "connected" = embeddable in networked/concurrent systems (services, simulators, browsers) and
  composable with other projects; a network server is an optional outer facade, never core.
- "heavy OSS clear type constraints" = permissive, OSS-friendly licensing and supply-chain hygiene,
  plus strong, explicit types (invalid inputs unrepresentable where practical).
- "kernel" = the hot computational core: model evaluation + flash/saturation solvers, shared
  immutably across threads.

## 2. Fixed facts

- Reference: CoolProp v8.0.0 (ae81610e), read-only at reference/CoolProp; identical to the PyPI wheel
  CoolProp==8.0.0, which runs via `uv run --no-project --python 3.12 --with CoolProp==8.0.0 python`.
  Its low-level API exposes alphar and partial derivatives, so we can verify individual Helmholtz terms.
- CoolProp v8 scale: ~150k LOC C++ (Helmholtz backend 28k, GERG 13k incl. 10k reference values,
  tests 29k, tabular/SVDSBTL/SBTL/Region ~11k), 139 fluid JSON files (17 MB), 136 fluids in FluidsList.
- No Rust toolchain installed yet on the dev box (plan step 0 must cover it).
- Repo: this repository (git, branch main). CoolProp map: docs/coolprop-map/*.md.
  Research: docs/research/*.md.

## 3. Decisions the design must make (each needs: decision, rationale, rejected alternatives)

- D1 Workspace/crate layout and feature flags (fewest crates that keep boundaries real).
- D2 Numeric core: f64-only vs generic numeric trait (dual numbers for derivatives; SIMD lanes).
  How derivatives are obtained (hand-coded like CoolProp / own hyper-dual / num-dual).
- D3 Model representation: closed enum of term kinds (data-oriented, match dispatch) vs trait objects
  vs generics; how new model families are added without editing the core (Open/Closed).
- D4 Pure vs mixture: composition in the core contract now? pure-fluid fast path? how mixtures land.
- D5 State and properties: what a `State` holds, what is computed eagerly vs on demand, typed input
  pairs, output selection, mass/molar basis, units/newtypes.
- D6 Flash architecture: pure functions over immutable models; phase determination; solver toolbox;
  explicit options (no global config); Result-based fallbacks instead of exception cascades.
- D7 Data pipeline: CoolProp JSON -> serde mirror -> validated model; embedding (per-fluid, feature
  gated, compressed or not); runtime loading from bytes (WASM fetch); name/alias/CAS index; lazy
  per-fluid init; cross-fluid dependencies (ECS reference fluids).
- D8 Concurrency kernel: registry, Arc sharing, no locks on hot path, batch API (slices in/out),
  optional rayon, warm starts, Send + Sync by construction.
- D9 Execution strategies for intra-algorithm parallelism: separate "what is computed" from "how it
  is executed"; scalar reference implementation as source of truth; SIMD/threaded variants side by
  side with differential tests; which algorithms qualify and which never will; dispatch mechanism
  (cargo features vs runtime detection; wasm simd128 is compile-time only).
- D10 Material / states-of-matter seams: the minimum names/traits/boundaries to build now; what is
  explicitly deferred.
- D11 Facades: Rust API, PropsSI-style string API (migration aid), C ABI, WASM (browser + WASI),
  Python later; where each lives.
- D12 Errors, panics, NaN policy, FP determinism across Linux/Windows/WASM, test tolerances.
- D13 Verification architecture: oracle fixture generation, format/size, tolerance classes,
  paper-value overrides, divergence register, consistency/property tests, differential tests,
  benchmarks and perf gates.
- D14 Licensing/attribution (CoolProp MIT notices; data provenance; cargo-deny).
- D15 Milestone order and the scope of the first useful release.
- D16 Naming (placeholder; final name is the user's call).
- D17 Edition/MSRV/tooling/CI matrix/lints; unsafe policy.

## 4. Judging rubric (weights)

| Criterion | Weight |
|---|---|
| Modularity and extensibility (new model families, materials, states of matter) without core edits | 3 |
| Simplicity: no over-engineering, few concepts, few crates, readable | 3 |
| Concurrency and memory: parallel requests on same/different fluids, lazy loading | 3 |
| Performance: high-frequency and batch; clean path to SIMD/parallel side-by-side implementations | 3 |
| Correctness and TDD-ability: oracle-driven, incremental, always-green milestones, bug arbitration by literature | 3 |
| Idiomatic modern Rust, DRY/SOLID, minimal dependencies, strong types | 3 |
| Rot elimination: designs out CoolProp's systemic problems (evidence in docs/coolprop-map) | 2 |
| Cross-platform: Linux, Windows, WASM (browser + WASI) | 2 |
| Migration path for CoolProp users (PropsSI-style API, C ABI) | 1 |
