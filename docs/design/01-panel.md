# Architecture panel: outcome

Workflow run `wf_d479b609-c5b`, 2026-10-04. The result is [../ARCHITECTURE.md](../ARCHITECTURE.md).

## Proposals and scores

| | Proposal | Emphasis |
|---|---|---|
| A | [Lean](proposal-a-lean.md) | Fewest concepts: one computing crate, one trait, zero public generics |
| B | [Extensible](proposal-b-extensible.md) | Open/closed: four traits, one crate per family, Gibbs solid out of tree |
| C | [Kernel](proposal-c-kernel.md) | Performance: lane executors, SIMD crate, lock-free borrowed handles |
| D | [Verification-first](proposal-d-verification-first.md) | TDD: typed divergence register, Parity/Corrected data, capability-gated milestones |

Full scores, rationales, fatal flaws and grafts are in [01-judgments.md](01-judgments.md) ([01-judgments.json](01-judgments.json)).

**Ranking** (weighted rubric, mean over judges, 1-10): Verification-first 7.55 (rust 7.52, thermo 7.57, systems 7.57) >
Lean 7.45 (7.22, 7.70, 7.43) > Extensible 7.22 (7.04, 7.26, 7.35) > Kernel 6.87 (6.61, 6.87, 7.13). Two judges called
the top two within judgement noise.

**Judges' winners**
- *Rust (idiomatic Rust, API, types):* **Verification-first.** It best fits "TDD with CoolProp as verification, source
  material when we find bugs", and it is the most type-strong design while staying simple: one kernel crate, two traits,
  validating newtypes, lint-enforced bans.
- *Thermo (thermodynamics, numerics):* **Lean.** It has the simplest kernel that still gets the thermodynamics right: an
  order-4, reducing-invariant (T, ρ) contract, and both open seams proved out of tree. The judge recommended grafting
  Verification-first's verification design onto it in full.
- *Systems (concurrency, memory, SIMD, WASM):* **Verification-first.** Its register and capability gates serve the
  user's TDD contract, and its `Real`/`ExecPolicy`/`policy_equivalence` harness is the right base for Kernel's lane
  executors.

## Fatal flaws raised against the winner, and the fix

| Flaw (judge) | Fix in the architecture |
|---|---|
| `Registry` is concrete over `Arc<FluidData>`, and the compat `Session` is hard-wired to the embedded registry, so a third-party family cannot reach the string, C or WASM facades (systems) | The registry stores `Fluid` = `Arc<dyn ThermoModel>` + `Gauge`. `Registry::with_model` returns a new value. `cprs_compat::props_si_in(&Registry, ..)` works over any registry, and the C ABI takes registry handles. `tests/new_family.rs` proves a van der Waals family defined out of tree through the registry, flash, gauge, batch, compat and 16 threads |
| The lazy transport `OnceLock` is missing from the sketch, and ECS `FluidId` references have no resolution path (systems) | `PureFluid` holds a lazily materialised `TransportSet`. Revised after the critiques (S-05, E5): references are declared in the data index, resolved to strong slot handles and checked for cycles when a registry layer is built, and decoded on the first transport call; the `Weak` `Resolver` is gone. `fluid-<x>` data features enable their references |
| Gibbs solids need core edits (`GibbsModel`, `relations::gibbs`, `Body::Gibbs`) because `State` is flash-only (thermo) | `State::from_total`, `bundle_from_gibbs` (exact Legendre transform) and `Phase::Solid` exist now, and `ThermoModel` is the registry currency. Revised (E1, E2): `State::from_split` lets any family build two-phase states, and `ThermoModel::derivs` gives order ≥ 3. `tests/gibbs_seam.rs` proves an out-of-tree Gibbs solid, and ice + vapour, through the registry, a reference state, batch and compat |
| The "no core edit" claim for new families is never demonstrated (thermo) | `tests/new_family.rs`; milestone gates require a zero-line `cprs-core` diff for the cubic (M11) and for ice/IF97 (M14) |

The fatal flaws raised against grafted designs are avoided too:
- Lean's stale superancillary after corrections is handled by the EOS-hash gate (revised, E14: the hash covers the whole canonical EOS section; R and ρ_r corrections rescale the curve exactly; other changes → `SatAccuracy::Guess` + VLE polish).
- Lean's eager ECS decode is replaced by lazy resolution.
- Extensible's order-2-only currency is avoided: `Derivs` goes to order 4, and `ThermoModel::derivs` exposes it for every family (E1).
- Kernel's (τ, δ) + `reducing()` contract, its concrete registry and its v0.1 SIMD are not adopted.

## Grafts

| Graft (source) | Verdict | Reason |
|---|---|---|
| Open registry over a dyn package + `with_model` + `props_si_in(&Registry)` + `new_family.rs` gate (Lean) | Applied | Fixes the winner's main fatal flaw. Adapted: the package trait is `ThermoModel`, not `Box<dyn Residual>`, so solids fit too |
| `State::from_total` + Gibbs Legendre test (Lean) | Applied | States-of-matter seam at near-zero cost. Takes a `Bundle` and a `ModelKey` |
| `Derivs::IDEAL_DELTA` (Lean) | Applied | Density solves never evaluate α⁰ |
| Chunked `eval_batch`, terms outer, states inner, bitwise (Lean) | Reverted (S-01) | No v0.1 path called it; returns with the SIMD milestone if a measured win justifies it |
| Precomputed τ-side B-factors, per-state `Vars` δ^k table, const `ORD` (Kernel) | Applied | Extended: δ^d also comes from the table, so the 57 d = 0 MBWR terms stay finite at δ = 0; δ-side factors are now exact polynomials, cancellation-free as δ → 0 (E4) |
| Generic hot kernel `accumulate<R: Real, const ORD>` (Kernel) | Applied | Crate-private (S-03); lanes would run the fast path, not only the AD form; tested bitwise on a test-only hyper-dual |
| `DataSource` + `Blob::{Static, Shared}` + counting-source test (Kernel) | Applied | `tests/lazy_load.rs` (16 racing threads, one read); sources also declare references (E5) |
| Lazy superancillary and transport; ECS through the same registry (Kernel) | Applied | Via std `LazyLock` and strong slot handles (S-05, S-08) |
| `fluid-<x>` features that pull in ECS references; WASM memory policy (Kernel) | Applied | `cprs-data` features; `fluids-core` embed with features forwarded through every facade (E7) + packs added with `Registry::with_source` (E6) |
| Borrowed per-request lookup (Kernel) | Applied, adapted | `Registry::get` returns `&Fluid`: zero refcount traffic without `FluidRef<'r>` lifetimes in the API |
| `ModelKey` content hash in `State`, refusing foreign states (Kernel) | Applied | `Error::ForeignState`; also the future surrogate cache key; hashes the whole model (E14) |
| clippy bans on `f64::powi`/`powf` (Kernel) | Applied | Plus every `f64` transcendental (E16), `mul_add`, env, locks; probes verified that they fire |
| Perf-gate table, zero-allocation test, 1..N thread-scaling harness (Kernel) | Applied | Recorded as each path lands from M2, enforced at M9 (E9) |
| LaneKernel/`run_lanes`/`cprs-simd` as the post-0.1 SIMD plan behind the ≥ 2.5× gate (Kernel) | Applied as a documented plan, revised (S-02) | Built only if the gate fires; a `simd` feature of core on fearless_simd with a core-local `Lanes<W>`, not a `cprs-simd` crate |
| Own msun `exp`/`ln` as the determinism choke point (Kernel, via thermo) | Partially | The choke point is `math`; the `libm` feature is the candidate for bit-identity (R14), unverified until the M9 cross-target hash (E16). An own vector `exp`/`ln` waits for lanes (K10), so libm is not duplicated before then |
| `cfg`-gate thread tests off wasm (Kernel/systems) | Applied | Both thread tests in the sketch are gated |
| Zero-line core diff gate for the second family (Extensible) | Applied | M11 cubic; also M14 ice/IF97 |
| `rho_max(T)`, open saturation with `SatAccuracy`, `CriticalPoint` origin (Extensible) | Applied | `SaturationCurve` trait replaces Lean's closed enum |
| One gauge wrapper for every model, tested on a solid; `Phase::Solid` (Extensible) | Applied, adapted | The `Fluid` handle *is* the gauged view, so there is no separate `Gauged` type |
| Family conformance suite (Extensible) | Applied | `fd_first_order`, `gauge_invariance` and `policy_equivalence` now; part additivity and thread invariance at M5-M9 |
| Flashable non-Helmholtz models behind the same registry and batch (Extensible) | Applied now | Deferring it would make the first solid a core edit |
| Jets and value forms generic over `Real` with no public generics (Extensible/Verification-first) | Applied, adapted | `Real` is public and sealed (S-02); the value forms are test-only (S-07); `Derivs<R = f64>` is the only public generic type |
| Parity/Corrected, typed patches, register cross-check, three-part proofs, EOS-hash gate; `FluidRecord` seam, `from_printed`, arbiters, test-count gate (Verification-first) | Kept | The winner's backbone. Patches move from core into the data blobs (rust-judge note) |
| Extensible's `Catalog`, `FamilyId`, `LibraryBuilder` | Rejected | `with_model` does the job with no extra concept |
| Extensible's `&Composition` on every call | Rejected | Pure fluids pay for mixtures; composition is not always mole fractions (materials §4) |
| Kernel's `FluidRef<'r>`, 10 crates, v0.1 SIMD and lockstep PH pipeline | Rejected | API friction and over-engineering (Kernel fatal flaw) |
| Verification-first's 10-method `PureFluidModel`, `Session`, four error types, column-major batch | Replaced | `ThermoModel` (3 required methods) + concrete `PureFluid`; `&Registry` arguments; one `Error`; point-major buffers (contiguous chunks) |

## Sketches

The four proposal type sketches lived in a session scratchpad that a reboot wiped. Only the proposal documents
survive, and judges' notes that cite sketch paths or `file:line` refer to that lost code. The final sketch was rebuilt
from the proposals' §2 excerpts and adapted to the final design. It is the only sketch kept, at
[sketch/](sketch): 8 crates, edition 2024, zero third-party dependencies. On 2026-10-04 it passed all seven gates with
0 warnings: `cargo check` for x86_64-linux (all targets), wasm32-unknown-unknown, wasm32-wasip2 and
x86_64-pc-windows-msvc; `clippy -D warnings`; `fmt --check`; and `cargo test` on the host (29 passed, 0 failed). The wasm targets were checked
(libraries and binaries) but not tested: no wasm runtime is installed.

## Revision after the critiques (2026-10-05)

Two adversarial critiques ([02-critique-simplicity.md](02-critique-simplicity.md), [02-critique-evidence.md](02-critique-evidence.md))
raised 32 issues; [03-decision-log.md](03-decision-log.md) records the disposition of each (all fixed, several in an
adapted form noted there). The rows above marked "revised" or "reverted" changed. The sketch now has 5 crates
(core, data, compat, verify, xtask; capi and wasm join at M10, py after 0.1) and 45 tests, and passed all seven gates
again on 2026-10-05 (results in the decision log).
