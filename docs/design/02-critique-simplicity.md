# 02 Critique: simplicity, idiomatic Rust, YAGNI

**Target:** [../ARCHITECTURE.md](../ARCHITECTURE.md) (2026-10-04) and its sketch [sketch/](sketch).
**Lens:** over-engineering, leaky or confusing abstractions, non-idiomatic APIs, needless dependencies, unsafe without
need, doc vs sketch drift. **Date:** 2026-10-04. Line numbers are per file. "ARCH l. N" = line N of ARCHITECTURE.md.

## 1. Gates re-run (observed, not copied)

Run from `docs/design/sketch` with `CARGO_TARGET_DIR=…/scratchpad/target-critic-simplicity`, Rust 1.99.0 stable.

| # | Command | Result |
|---|---|---|
| 1 | `cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu` | pass (exit 0, 0 warnings) |
| 2 | `cargo check --workspace --target wasm32-unknown-unknown` | pass |
| 3 | `cargo check --workspace --target wasm32-wasip2` | pass |
| 4 | `cargo check --workspace --target x86_64-pc-windows-msvc` | pass |
| 5 | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| 6 | `cargo fmt --all --check` | pass |
| 7 | `cargo test --workspace` | pass: 29 passed, 0 failed (core 15, verify unit 1, `gibbs_seam` 2, `lazy_load` 3, `new_family` 5, `register` 2, doctest 1). The doctest is `no_run`, so it only compiles: 28 tests execute |

The MSRV (`rust-version = "1.85"`) was not checked: only the stable toolchain is installed. A grep finds no let-chains.

The probes below ran in a scratch crate outside the repo, with a path dependency on the sketch:

| Probe | Observed |
|---|---|
| `size_of` State / Error / `Result<State, Error>` / Fluid / Input | 192 / 48 / 192 / 32 / 40 B: matches ARCH §3.6 and §9 |
| `clippy.toml` bans (`Mutex`, `Cell`, `env::var`, `f64::{exp, ln, powi, powf, mul_add}`) | all 8 fire |
| `Registry::from_pack(..)` | **panics** (`todo!`) |
| `Registry::from_embedded(..).resolver().get("Water")` after the registry drops | `Err(Load(Detached))` |
| Out-of-tree vdW (the `new_family.rs` model), DT at T = 0.8·Tc / 2·Tc | `Err(Load(MissingPart("saturation")))` / `Ok(SupercriticalGas)` |
| Public surface of `cprs-core` (grep) | 17 `pub mod`s, **68** public types, traits and consts, **143** `pub fn`s |
| Call sites of `Real::{Mask, lt, select}` and `Vars::{tau, delta}` | **0** |
| Call sites of `residual_batch` / `accumulate_many` outside their own test | **0** |

## 2. Issues

There are no blockers. No requirement is violated and nothing is unsound: there is no `unsafe`, there are no third-party
dependencies, and all four targets build. The five majors are all **deletions**: each one shrinks the TDD plan.

| ID | Sev | Area | Issue (one line) | Fix (one line) |
|---|---|---|---|---|
| S-01 | major | D9, §7 | The v0.1 "how" hooks are dead code: no production path calls `residual_batch`/`accumulate_many`, `flash_many` is never overridden, and `Auto` == `Reference` | Delete them; re-add them as defaulted methods when the SIMD gate fires (a minor semver change) |
| S-02 | major | D2, D9, D17 | The SIMD plan is cyclic without the rejected `cprs` root crate, and it already forces a public, SIMD-shaped `Real` | Lanes become a `simd` feature inside core; seal `Real`; drop `Mask/lt/select`; one MSRV |
| S-03 | major | §3, D1 | The public API is ~3× the documented "whole surface", including generic kernels and record structs that churn every milestone | Private modules; `lib.rs` re-exports the API; `#[doc(hidden)]` internals for verify and xtask; `#[non_exhaustive]` records |
| S-04 | major | D5, `input.rs` | Two input representations, five 19-arm tables, and a typed→raw→typed round trip with double validation per call | Models receive one normalised `(pair, x, y)` value; the typed variants are only sugar at the edge |
| S-05 | major | D7, D8, §6 | `Resolver` holds a `Weak`, so a `Fluid`'s transport depends on its registry still being alive (`Detached`) | Hold a strong `Arc` to the reference fluid's slot (the DAG is acyclic); delete `Resolver` and `Detached` from the API |
| S-06 | minor | §9 | 5 reachable `todo!`s, none behind a capability, and `clippy::todo` is not denied | Deny `todo`/`unimplemented`/`panic` in lib code; return typed errors |
| S-07 | minor | D2 | Three AD implementations (HyperDual, Jet4, num-dual) plus production `value::<R>` code that only tests use | One in-house `Jet4`; num-dual as the dev oracle; `value` under `cfg(test)` |
| S-08 | minor | `fluid.rs` | A hand-rolled `Lazy<T>` duplicates `std::sync::LazyLock`, has an unreachable error path and keeps its closure alive | `LazyLock<Result<T, LoadError>, Box<dyn FnOnce…>>`; `Box`, not `Arc`, for the curve |
| S-09 | minor | registry, builder | A one-field `RegistryOptions`, a `from_pack` that duplicates `from_source`, and a builder that checks its only required field at run time | Pass `DataSet`; add a `Pack: DataSource`; use `builder(info, eos, limits)` |
| S-10 | minor | flash, state | Silent fallbacks, a NaN sentinel and misclassified errors (`Load` for a missing VLE) in a design that bans them | Use `Point`-typed `two_phase` and `Result` plumbing; return `Unsupported` until M6; add a subcritical gate point |
| S-11 | minor | D1, D15 M0 | Three empty facade crates are built, linted and tested from M0 (`cprs-py` is post-0.1); the capi lints are hand-copied without `dbg_macro` | Add each crate in its milestone; fix the capi lint list |
| S-12 | minor | D1, D7 | The `json` feature puts serde into core at M2, but M2's only consumer is xtask | Keep the serde mirror in xtask until a runtime-JSON consumer is scheduled |
| S-13 | minor | thesis, §1, §6 | The doc overstates simplicity: it says "two traits" (there are 4 open traits + `Real`), "OnceLock only" and "29 tests", and its "deps ever" list omits SIMD | Correct the text and state the real concept count |

### S-01 (major): the v0.1 batch hooks are speculative and unreachable
- **Evidence.**
  - ARCH D9 (l. 529-531) and §7 (l. 698-700) promise v0.1 "a chunked bitwise-equal `residual_batch`" plus the
    `flash_many` hook.
  - The batch path never reaches it: `batch::run_chunk` calls `flash_many` (batch.rs:152), whose default calls `flash`
    per point (model.rs:197-201), which calls `residual` per point. `residual_batch` (helmholtz/mod.rs:37,
    helmholtz/eos.rs:95) has one caller, its own test (eos.rs:143).
  - No v0.1 model overrides `flash_many`. `ExecPolicy::Auto` is documented as `= Reference` in v0.1 (l. 700).
  - The cost grows with every term kind. `eval_many` dispatches each block kind to its own `accumulate_many`
    (eos.rs:59-65, power.rs:145), so M3/M4 add six or seven more batch loops, each with a bitwise test, that nothing
    calls.
  - Pre-adding hooks buys no "zero core edit". The hooks are core executor plumbing. A new public item or a
    `#[non_exhaustive]` variant is a minor change (materials-extensibility l. 18, citing the Cargo SemVer guide). By
    inference, a defaulted trait method is too (the guide calls it "possibly-breaking" only through name ambiguity).
    Research R12 asks only for sequential batches plus a `rayon` `par_*` path.
- **Fix.**
  - Delete `HelmholtzModel::residual_batch`, `MultiParameterEos::{residual_batch, eval_many}`, `CHUNK`,
    `PowerBlock::accumulate_many`, `ThermoModel::flash_many` and `ExecPolicy::Auto`.
  - Keep `#[non_exhaustive] ExecPolicy { Reference, Parallel { chunk } }`; `batch::evaluate` calls `Fluid::flash` per
    point.
  - Re-add the hooks in the post-0.1 SIMD milestone, together with the ≥ 2.5× gate.
  - Alternative: if R13 step 1 (SoA auto-vectorisation) is wanted in v0.1, **wire it** into DT batches at M9 behind a
    measured win. Do not ship it unreachable.

### S-02 (major): the SIMD crate topology contradicts itself and freezes a public SIMD API now
- **Evidence.**
  - ARCH l. 702-706 requires three things at once: a crate-private `exec` module in core runs `run_lanes<W>`;
    `cprs-simd` holds the `#[target_feature]` entry points; and core's `Auto` "picks a lane executor".
  - In the Kernel proposal this worked because `cprs-simd` depended on the kernel and a separate `cprs` composition root
    wired them together (proposal-c l. 46, 57). D1 (l. 426) rejected that root, so one of two things now breaks:
    - If `cprs-simd` depends on core, core's `Auto` cannot reach it, and `exec::run_lanes` must be public.
    - If core depends on `cprs-simd` (research T1b: "optional dependency of core via a `simd` feature"), `cprs-simd`
      cannot implement `cprs_core::Real` or call `PowerBlock::accumulate`. "The same generic code on lanes" then fails.
  - The plan already shapes today's API:
    - `Real` is public and unsealed (num.rs:10-40, ARCH l. 430-431), with `type Mask`, `lt` and `select` (num.rs:25,
      37, 39). These have zero call sites.
    - Research K8 and K10 specify a **crate-internal** `Real` and `vmath`.
    - Once a public, unsealed trait is published, every method added later breaks any external implementor.
  - The plan also needs two MSRVs (1.85 for core, 1.89 for `cprs-simd`, l. 604-605). Research dependencies Q1 (l. 546)
    calls a single value "simpler".
- **Fix.**
  - Drop `cprs-simd` from the plan. If the gate fires, add a `simd` feature to `cprs-core` with an optional
    `fearless_simd` dependency (0 dependencies, runtime dispatch, safe loads and stores; dependencies R13, T1b) wrapped
    in a **core-local** `Lanes<W>` newtype that implements `Real`. The orphan rule is satisfied (kernel-performance
    l. 285), there is no cycle, and `Auto` can see the lane executor.
  - Keep `forbid(unsafe_code)`, unless raw `std::arch` proves necessary. Then add one `#[allow]` module.
  - Today: seal `Real` (private supertrait), so methods can be added later without a break, and delete
    `Mask/lt/select`.
  - Pick one workspace MSRV. Update l. 37 ("the only optional deps ever").

### S-03 (major): the public surface is about three times the documented one, and record churn breaks semver
- **Evidence.**
  - ARCH l. 100-103 lists about 20 concepts, then says "That is the whole surface. No public signature has a generic
    parameter except the defaulted `Derivs<R = f64>`."
  - The sketch exports far more. `lib.rs:24-40` declares 17 `pub mod`s, which expose 68 types, traits and consts plus
    143 functions. These include:
    - generic kernel internals: `Vars<R>` (power.rs:20), `b_factors<R, ORD>` (power.rs:69) and
      `PowerBlock::{value, accumulate, accumulate_many}<R, …>` (power.rs:114-145);
    - `MAX_POW`, `PowerTerm` (pub fields), `ResidualBlock`, `IdealGas`, `IdealTerm` and `MultiParameterEos::new`;
    - `relations::*`, `flash::flash`, and the milestone-progress const `fluid::IMPLEMENTED` (fluid.rs:104);
    - `transport::TransportSet`, with pub fields and `ViscosityModel::Ecs { reference: Fluid }`.
  - `FluidRecord` and `EosRecord` (data.rs:85, 108) have **all-pub fields and no `#[non_exhaustive]`**. They gain
    sections at M4, M6 and M8 (ancillaries, superancillary, transport, melting), so each of those milestones is a
    breaking change. Materials R10 asks for `#[non_exhaustive]` and private fields on growing types.
  - cargo-semver-checks (D17) guards whatever is public, including all of this by accident.
- **Fix.**
  - Make the modules private. `lib.rs` re-exports the user and family-author API, exactly the list at l. 100-103.
  - Put the multiparameter internals, the records, relations and `flash::flash` behind `pub(crate)`. Expose what
    `cprs-verify` and xtask need through one `#[doc(hidden)] pub mod internal` (semver-exempt by convention,
    *inference*).
  - Add `#[non_exhaustive]` to `FluidRecord`, `EosRecord`, `PowerTerm` and `TransportSet`.
  - Make the l. 103 sentence true, or delete it.

### S-04 (major): two input representations and a round trip on every call
- **Evidence.**
  - `input.rs` keeps five parallel 19-arm tables: `Pair::ALL` (53), `Pair::vars` (76), `Input` (124), `Input::pair`
    (189), `Input::molar_values` (214) and `Input::from_raw` (239). It also has a private `Molar` trait with 7 impls
    (147-185). A new pair or quantity touches all of them, contrary to the DRY claim at l. 36.
  - Scalar path:
    - `Fluid::native_input` (fluid.rs:273-285) breaks the typed `Input` into raw molar values, gauge-shifts them, then
      **rebuilds and re-validates** it through `from_raw`.
    - `flash::flash` (flash.rs:92) then matches the variant again and calls `to_molar` on a value that is already
      molar.
  - Batch path: `run_chunk` validates through `from_raw` (batch.rs:143), and the default `flash_many` validates again
    (model.rs:199). Points that fail are turned into NaN and "refused again" (batch.rs:148).
  - Capability is checked three times: fluid.rs:275, batch.rs:151 and each model's own `match`.
- **Fix.**
  - Minimum: `ThermoModel::flash` takes a normalised `Native { pair, x, y }` value (validated, molar, native gauge,
    private fields). `Fluid` and `batch` build it exactly once and models `match native.pair()`. This removes the
    rebuild, the second validation and the in-model `to_molar` calls.
  - Full: make `Input` a struct (pair, two raw values, their bases) built by typed constructors (`Input::dt(Density,
    Temperature)`). These keep the compile-time safety of the variants and are macro-generated from the `Pair::vars`
    table. Add per-`Var` `validate` and `to_molar` (7 arms each). The tables drop from 6 × 19 to 2 × 19 plus one macro,
    and `Molar`, `InputKind`, `molar_values` and the `Input::pair` match all go.

### S-05 (major): `Resolver` leaks registry lifetime into `Fluid`
- **Evidence.**
  - `Resolver { slots: Weak<Slots> }` (registry.rs:117-128). ECS transport resolves its reference fluid through it on
    first transport use (ARCH l. 666-670).
  - Probe: once the registry is gone, resolution returns `Err(Load(Detached))`.
  - The natural idiom `let f = Registry::from_embedded(DataSet::Parity…).get("R143a")?.clone();` (temporary registry)
    therefore yields a `Fluid` whose first viscosity call fails (*inference* from the design; the sketch's ECS closure
    is still the unused `_resolver`, registry.rs:91).
  - Both non-static cases are routine: Parity fixture registries and browser `from_pack` registries.
  - ARCH §14 (l. 922) mitigates this by "documented ownership" only. Map 05 §9 U8 recommends strong references
    ("`Arc<FluidData>` plus a pinned `ModelId`").
  - The ECS graph is declared and acyclic: Propane has 10 dependents, R134a 8 and Nitrogen 1 (map 05 §5), and datagen
    proves the graph acyclic (ARCH §8 step 6).
- **Fix.**
  - Make each slot `Arc<OnceLock<Result<Fluid, LoadError>>>` and the source an `Arc<dyn DataSource>`.
  - The lazy transport closure then captures a **strong** `Arc` to its reference fluid's slot, plus the source and the
    `DataSet`. Because the DAG is acyclic, no `Arc` cycle can form.
  - Delete `LoadError::Detached` and make `Resolver` `pub(crate)` (or delete it).
  - Out-of-tree families that borrow another fluid (a cubic borrowing an ideal gas, humid air borrowing water and air)
    take a `Fluid`, already an `Arc`, at construction. `Fluid` becomes self-contained.

### S-06 to S-13 (minor): evidence and fixes
- **S-06.** There are `todo!`s at registry.rs:183, fluid.rs:223, fluid.rs:267, helmholtz/ideal.rs:77 and flash.rs:119.
  `from_pack` panics in the probe, and `with_reference(Iir|Ashrae|Nbp)` would too.
  - ARCH §9 (l. 777-778) allows `todo!` "only behind an undeclared capability", but none of the five is.
  - The workspace lints (sketch `Cargo.toml`) do not deny `clippy::todo`, `unimplemented` or `panic`.
  - Fix: deny those three in lib code, map each branch to `Unsupported`/`NoModel`/`LoadError::Format`, and delete the
    exception from §9. A green milestone then cannot hide a panic.
- **S-07.** D2 (l. 430-431) plans `f64`, `HyperDual` and `Jet4`; l. 444 adds num-dual as the dev oracle.
  - HyperDual (num.rs) has no production role: Jet4 lands at M3 and the non-separable NonAnalytic terms at M4.
    Order-2 mixed derivatives are a slice of the bivariate order-4 jet, and one in-house `Jet<N>` covers both
    (dependencies l. 214, R5).
  - `PowerBlock::value::<R>` (power.rs:114) is production code that only tests use for separable kinds.
  - Fix: one in-house AD type (`Jet4`), cross-checked against num-dual (dev-dependency, R5c). Delete HyperDual at M3
    and put `value` for separable kinds under `#[cfg(test)]`.
- **S-08.** `Lazy<T>` (fluid.rs:78-100) duplicates the `LazyLock` that the sketch already uses (registry.rs:160, so it
  is within the MSRV per clippy's `incompatible_msrv`).
  - `init: Option<_>` and the `missing: fn() -> LoadError` parameter are unreachable, because `ready()` pre-fills the
    cell.
  - The init closure, which holds the blob and the resolver, outlives initialisation.
  - `Lazy<Option<Arc<dyn SaturationCurve>>>` (fluid.rs:114) shares nothing; a `Box` suffices.
  - Fix: `LazyLock<Result<T, LoadError>, Box<dyn FnOnce() -> Result<T, LoadError> + Send>>`, which drops `F` after the
    first use. Under the no-panic rule, its panic poisoning is moot (*inference*).
- **S-09.** `RegistryOptions` (registry.rs:18) wraps one `DataSet`. `from_pack` (registry.rs:182) is `from_source`
  over a pack source.
  - `PureFluidBuilder` stores its only required field as `Option` and reports a missing value as
    `LoadError::MissingPart("limits")` (fluid.rs:163): a programming error disguised as a data error.
  - Fix: `from_embedded(DataSet)` / `from_source(.., DataSet)`; a public `Pack::new(Arc<[u8]>) -> Result<Pack,
    LoadError>` that implements `DataSource`; `PureFluid::builder(info, eos, limits)`.
- **S-10.** Small cases of the sentinel and silent-fallback rot the design removes (map 12 R3):
  - `State::two_phase` returns `liquid` unchanged when the bodies do not match (state.rs:123-129).
  - `flash::total` substitutes a NaN bundle when a model returns order < 2 (flash.rs:132-143).
  - `State::single` reports non-finite input and a `TwoPhase` label as `MechanicallyUnstable` (state.rs:113-115).
  - A `TwoPhase` phase hint is silently ignored (flash.rs:109).
  - A subcritical DT flash of a family with no saturation curve returns a **load** error (flash.rs:116; probe above),
    although `PureFluid::saturation`'s doc says `None` means "use the generic VLE". `new_family.rs` tests only
    T ≥ Tc (lines 97-164), so the gate does not show this.
  - Fix: `two_phase(liquid: Point, vapour: Point, ..)`; `total` returns `Result`; refuse or honour `with_phase(TwoPhase)`;
    return `Unsupported` until M6 brings VLE; add one subcritical point to the M5 gate.
- **S-11.** `cprs-py` (5 lines), `cprs-wasm` (10) and `cprs-capi` (12) contain only doc comments, yet all seven gates
  build, lint and test them from M0 (D15 M0: "Workspace from this sketch").
  - `cprs-capi/Cargo.toml` copies the lints by hand, which it must because the workspace sets `forbid`, but it omits
    `dbg_macro`.
  - Fix: M0 = core, data, verify, xtask. Add compat when the M5 gates need it, capi and wasm at M10, and py after 0.1.
    Add `dbg_macro = "deny"` to capi.
- **S-12.** Core gets the `json` feature, and with it serde_json, at M2 (ARCH l. 85, 762-763). The only M2 consumer is
  xtask datagen, a tooling tier where serde is allowed anyway (dependencies l. 441). Runtime CoolProp-JSON loading has
  no v0.1 user story in BRIEF §1.
  - Fix: keep the single serde mirror in xtask; move it behind a core feature when a runtime-JSON consumer is
    scheduled. Core stays at zero dependencies until `rayon`/`libm` at M9.
- **S-13.** Doc inaccuracies:
  - The thesis (l. 21-22) says "Two small object-safe traits". There are four open object-safe traits (`ThermoModel`,
    `HelmholtzModel`, `SaturationCurve`, `DataSource`) plus `Real`, and D3 (l. 465) rejected Extensible for its "four
    traits".
  - l. 41 and l. 672 say "OnceLock is the only cell / clippy bans the rest". The sketch also uses `LazyLock` and its
    own `Lazy`, and `clippy.toml` does not ban atomics or `OnceCell`.
  - l. 12 says "29 passing tests"; one of them only compiles.
  - l. 37 ("the only optional deps ever") omits the SIMD dependency.
  - Fix: correct the text. An honest concept count is still small.

## 3. What held up under attack
- The crate boundaries are real (std-only core, `no_std` data, compat outside the kernel). There are zero third-party
  dependencies and no `unsafe` anywhere. All 7 gates are green, and I observed them myself.
- `State` (192 B, `Copy`, no `Arc`) and `Error` (48 B, `Clone`) match their claims. The `Send + Sync + 'static`
  compile-time asserts and the clippy bans work.
- The two seams that matter, `ThermoModel` (package) and `HelmholtzModel` in (T, ρ) with reducing-invariant `Derivs`,
  are small and proven out of tree. The `from_total` + `bundle_from_gibbs` states-of-matter seam costs about 40 lines.
- One `OnceLock` per registry slot, failure caching and allocation-free lookups are simple and correct.

## 4. Net effect if the fixes land
- **Removed concepts:**
  - `Resolver`, `LoadError::Detached`, `ExecPolicy::Auto`, `Real::{Mask, lt, select}`;
  - `residual_batch`, `flash_many` and the per-kind `accumulate_many` loops;
  - `HyperDual`, the `Molar` trait, the typed→raw→typed rebuild, `RegistryOptions`, the custom `Lazy`;
  - the planned `cprs-simd` crate and its second MSRV;
  - three crates that would otherwise sit empty from M0.
- **Public API = the doc's own list.** Internals stay changeable through M2-M8 without semver breaks.
- **No capability is lost.** Every deleted hook can be re-added as a defaulted method or a `#[non_exhaustive]` variant
  when its trigger fires.

## 5. Verdict

**Approve with changes.** The crate layout, the two core contracts and the concurrency model are lean and idiomatic,
and the sketch is honest about what it compiles. The over-engineering is concentrated in speculative performance seams
grafted from the Kernel proposal (batch hooks, public generic kernels, a SIMD-shaped public `Real`, `cprs-simd`):
v0.1 never calls them, and they no longer fit together once the `cprs` root crate was rejected. A second pocket is the
doubled input representation. Fix S-01 to S-05 before `docs/PLAN.md` is written. All five are deletions or narrowings,
each removes test obligations from M3-M9, and none weakens the extensibility gates.
