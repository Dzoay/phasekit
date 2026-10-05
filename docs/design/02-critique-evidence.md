# Critique: evidence, correctness and requirements coverage

Adversarial review of [../ARCHITECTURE.md](../ARCHITECTURE.md) and [sketch/](sketch), 2026-10-05. Lens: does the design
meet every verbatim requirement (BRIEF §1) and decision (D1-D17), are the fatal flaws of
[01-judgments.md](01-judgments.md) really fixed, and do its claims about CoolProp hold against the maps?
Citations: "map NN §X" = `docs/coolprop-map/NN-*.md`; sketch paths are relative to `docs/design/sketch/crates/`.

**Reproduced first.** The sketch was copied to the scratchpad, and cargo ran there with its own `CARGO_TARGET_DIR`. The
panel's gates hold: 29/29 tests pass; `check --all-targets` passes for wasm32-unknown-unknown, wasm32-wasip2 and
x86_64-pc-windows-msvc; `clippy -D warnings` and `fmt --check` are clean; zero third-party dependencies. `State` is 192 B,
`Error` 48 B, `Result<State, Error>` 192 B, as claimed. A probe crate shows that `#[wasm_bindgen]` exports compile under
`forbid(unsafe_code)`, so the D17 unsafe policy is feasible for `cprs-wasm`.

## Verdict

The concurrency core is sound. Shared types are Send + Sync by compile-time assertion, nothing on the hot path takes a
lock, and lookups return `&Fluid`. Most rot items are designed out by types rather than renamed. **It is not yet a safe
basis for a long TDD plan.**
- **Two blockers.** Both sit in the public contract that the M11/M14 zero-line-diff gates freeze: CoolProp's
  derivative and residual outputs have no API (E1), and no family outside the core can build a two-phase `State` (E2).
  Both are cheap to fix now and expensive after M5.
- **Seven majors.** Each is a local fix: pseudo-pure saturation types (E3), cancellation-free δ-factors for exact virials
  (E4), registry-construction checks for ECS references and cycles (E5), layered sources for browser loading (E6), facade
  feature forwarding (E7), a licence decision on the superancillary data (E8), and benchmarks from M3 (E9).
- **Ten minors.** Fix them in the sketch before `docs/PLAN.md` is written.

## Issues (ranked)

| ID | Sev | Section | Issue | Evidence | Fix |
|---|---|---|---|---|---|
| E1 | blocker | D5, D2, D15, §3.4 | **No API for CoolProp's derivative and part-wise outputs.** The design has no first- or second-partial-derivative engine, no order-≥3 path, and no ideal/residual split. D5 says "Order ≥ 3 and residual-only outputs ... come from the model", but no method delivers them. For families that only implement `ThermoModel` (Gibbs solids, IF97), order 3 cannot be reached without a core edit. Extensible's order-2 fatal flaw therefore comes back one level up | Map 01 §4a: 85 outputs, 5 of them derivative-based (fundamental derivative, κ, β, ...), 8 Helmholtz-term outputs, H/S/G residual, Cp0, Z, virials, and the strings `d(X)/d(Y)\|Z` and `d(d(X)/d(Y)\|Z)/d(W)\|V`. Map 01 §9 rates U8 (first partials) P0-core and U9/U11 P1-early. Digest 01 says "port it early". The sketch has no such path: `ThermoModel` (cprs-core/src/model.rs:185-229) has no derivative hook; `Prop` (prop.rs:9) has 20 variants; `Fluid::prop` (fluid.rs:305) is a closed match; `State` keeps only the order-2 *total* bundle (state.rs:61-74). M5-M10 never schedule U8/U9/U11 | Add `ThermoModel::derivs(&self, &State, Order) -> Result<PointDerivs, Error>`, returning ideal and residual separately. Its default re-evaluates through `helmholtz()`; Gibbs families override it. Add a `Prop::Deriv1 { of, wrt, at }` / `Deriv2` key and the Jacobian-ratio engine in `relations` (order 2 for first partials, order 3 for second). Schedule first partials, Cp0, Z and residual parts in M5; second partials and the fundamental derivative in M7; the derivative-string grammar plus an 85-output oracle gate in M10 |
| E2 | blocker | D4, D10, §11, M14 gate | **No family outside the core can build a two-phase `State`.** The thermo judge's fatal flaw ("State is flash-only") is fixed only for single-phase states. A mixture `MixtureState`, IF97 QT/PQ, and ice + water coexistence cannot pass through the registry/batch/compat currency, so "0 expected" core edits (§11) and the M14 zero-line gate fail by construction | `from_total` refuses `Phase::TwoPhase` (state.rs:114). `two_phase` is `pub(crate)` (state.rs:123). `ThermoModel::flash -> Result<State, Error>` (model.rs:193) is the only path into `batch::evaluate` and `props_si_in`. Map 01 §4b: IF97 supports QT and PQ natively, and HP/PS/HS cross the dome. D4 plans a separate `MixtureState`, which `flash` cannot return | Add a public `State::from_split(liquid: State, vapour: State, q: Quality, p) -> Result<State, Error>` that validates key, finiteness and p. Let the vapour and liquid points carry their own T (needed for E3) and remain composition-agnostic (q is molar). Prove it in `gibbs_seam.rs`: an out-of-tree model returns a QT two-phase state through the registry, batch and compat |
| E3 | major | D4, D6, §3.6 `SaturationCurve` | **The 6 pseudo-pure v0.1 fluids cannot be represented.** Their bubble and dew points differ in p at fixed T, and in T at fixed p, but `SatPoint` holds one T and one p, and the two-phase body assumes a single T. The semantics question (map 03 Q7, map 04 Q10) never reached §14 | Oracle, CoolProp 8.0.0, R410A: QT at 280 K gives p = 990480.5 Pa (Q = 0) and 987288.1 Pa (Q = 1), and refuses Q = 0.5. PQ at 1 MPa gives T = 280.3166 / 280.3700 / 280.4235 K for Q = 0 / 0.5 / 1. DT at (280 K, 100 kg/m³) gives p = 988112.05 Pa and Q = 0.3591. Map 03 §3.1 (QT/PQ rows) and map 04 §1 table. Sketch: saturation.rs:23-44; flash.rs:108-129; state.rs:123 | Return `SatPair { bubble: SatSide { t, p, rho }, dew: SatSide { .. } }` from `at_t`/`at_p` (pure fluids: equal p and T). Specify the pseudo-pure rules from the oracle (Q-pairs at interior Q refused or interpolated; PQ T-interpolation; the DT in-dome rule), with fixtures. Add the question to §14 |
| E4 | major | D2, M5 ("exact virials") | **The exact-virials promise is unsupported, and the B-factor recurrence cancels as δ → 0.** Every scaled A₀ⱼ vanishes at δ = 0, so B and C need A₀₁/δ and A₀₂/δ² at small δ. That is CoolProp's δ = 1e-12 approach, which map 12 lists as a defect | Map 12 §6.3: C-virial relative error is about 7e-5 from evaluating at δ = 1e-12, and the exact path is unmerged. Digest 02 asks for exact δ → 0 Taylor coefficients. The sketch computes b₂ = (p₂ + p₁²) − p₁ (helmholtz/power.rs:75-76). For d = 1, l = 1 its relative error is 3.6e-13 at δ = 1e-4, 4.5e-9 at 1e-8 and **2.2e-5 at 1e-12** (Python replica, appendix). 135 of 136 default EOSs have terms with d ∈ {1, 2} and l ≥ 1 (scan of `dev/fluids`). `HelmholtzModel` (helmholtz/mod.rs:19) has no zero-density method | Precompute the δ-side factors per term as polynomials in x = cδˡ with exact coefficients, for example B₂ = d(d−1) − (l² + 2dl − l)x + l²x². This form has no cancellation and is cheaper, and it parallels the precomputed τ-side `bt`. Add a defaulted `HelmholtzModel::zero_density(t) -> Option<Virials>`. Gate M5 against analytic virial derivations, not the oracle |
| E5 | major | D7, D8, §6 cross-fluid | **The ECS resolution path is unproven, still fails late, and deadlocks on cycles.** References still resolve by name at the first transport call, which is CoolProp's own pattern. Acyclicity is proved only for datagen output, but runtime packs and JSON bypass datagen. The `Resolver` also cannot reach provided models, which contradicts §6's claim about humid air's ice | In `Slots::load` the resolver is created and dropped (`let _resolver = ...; // captured by the lazy transport (M8)`, registry.rs:91). `compile` takes no resolver (data.rs:175). No test covers "transport loads B once, thermo never". `Resolver::get(&self, name: &str)` (registry.rs:123). Map 05 R7: "Reference built by name on first evaluation ... only fails at runtime"; its remedy is "Resolve references at load". Probe: two `OnceLock`s whose inits call each other hang (`timeout 5` exits 124), and std documents re-entrant init as unspecified (currently a deadlock). `Resolver { slots: Weak<Slots> }` covers only data-backed slots (registry.rs:118) | Have `DataSource` (or the pack index) declare references as `FluidId`s. `Slots::build` then checks that every reference exists, is available and that the graph is acyclic, returning a `LoadError` at construction and decoding lazily as planned. Resolve by id, not by name. Add a counting test: an A→B edge reads B 0 times on a thermo path, 1 time on the first viscosity call, and 1 time under 16 threads; a cyclic pack is refused. Humid air gets injected handles (map 07 H3) |
| E6 | major | D7, §8 WASM loading | **Browser on-demand loading cannot work.** A failure is cached forever, and registries cannot be merged. A request that arrives before its blob is fetched poisons that fluid for the registry's lifetime, and a newly needed fluid forces a new registry that decodes everything again. This conflicts with "only loading what is needed for the requests coming in" | The registry caches every load failure in `OnceLock<Result<Fluid, LoadError>>` (registry.rs:47; D8). `DataSource::names()` is read once (registry.rs:54) and `blob()` is synchronous, while fetch is async. `with_model` adds models, not sources (registry.rs:188), and `Fluid::model()` returns `&dyn`, so a fluid cannot move between registries | Add `Registry::with_source(Box<dyn DataSource>) -> Result<Registry, Error>`. It shares the existing slots and adds new ones, refusing collisions, and JS calls it as `registry.withPack(bytes)`. Optionally add an uncached `LoadError::Unavailable`: check `cell.get()` first and load outside the cell |
| E7 | major | D1, §6 WASM memory | **Cargo feature unification defeats the browser `fluids-core` policy.** Every facade embeds all 136 fluids and cannot opt out, because Cargo features are additive | The workspace dependency `cprs-core = { path = "crates/cprs-core" }` keeps default features (sketch Cargo.toml:16). `cprs-compat` forwards no features. `cargo tree -e features -p cprs-compat` shows `cprs-core feature "default" → "fluids-all" → cprs-data "all"`. `cprs-wasm`, `cprs-capi` and `cprs-py` all sit on compat (§2) | Set `default-features = false` on the workspace dependency. Give compat/capi/wasm/py forwarding `fluids-all` (default) and `fluids-core` features; `cprs-wasm` selects `fluids-core`. Add a CI gate on `cargo tree -e features -p cprs-wasm --target wasm32-unknown-unknown` plus a .wasm size budget |
| E8 | major | D14 vs D7/§7 | **The superancillary licence status contradicts the design.** D14 keeps "fastchebpure outputs" out of default features, yet default saturation (`Exact`), the M6 gate and the QT/PQ ≤ 0.1 µs targets are built on exactly those blocks. §14 Q3 asks only about fixtures | Map 09 §1: "The SA blocks come from an external fitter, fastchebpure", and "the fastchebpure SA outputs" are still listed under provenance to clear. The superancillary is 89.8 % of the data (map 09). Map 10 §8.1 separates the dense outputcheck files ("license unverified") | Separate (a) the SA coefficient blocks inside CoolProp's MIT JSON from (b) the dense outputcheck fixtures. Put (a) to the user as a §14 question that must be answered before M2. Document the fallback now: a datagen refit (also wanted for E14), or `Guess`-only saturation with adjusted perf gates |
| E9 | major | §7 perf gates, D15 | **Performance is first measured at M9**, after the flash (M5-M7) and transport (M8) depend on `dyn` + jets + eager bundles. §14's own risk ("dyn dispatch and jets slower than CoolProp's `all()`") is mitigated only at M9. Speed is weight 3 and was named in the user's request | §7: "K19 estimates until the M9 baseline". D15 M9: "perf baselines". §14 risk row 2 | Add criterion and counting-allocator benches per milestone: the α^r bundle at M3, DT properties at M5, PT/PH at M7. Record them against the gate table (non-blocking until M9). Build the C++ CoolProp baseline at M1 |
| E10 | minor | D11, §12 migration | **String, C and WASM callers cannot select a reference state.** CoolProp users rely on `set_reference_stateS` (IIR/ASHRAE/NBP) | `with_model` always wraps in a native-gauge `Fluid::new(model)` (registry.rs:195). `props_si_in` resolves by name only (cprs-compat/src/lib.rs:124). Map 15 §2.2, map 01 U14 | Add `Registry::with_reference(name, ReferenceState) -> Result<Registry, Error>`, a new value with a gauged handle, plus `cp_registry_with_reference` and a JS equivalent |
| E11 | minor | §11 row "new input variable" | **§11 claims `Pair` is `#[non_exhaustive]`; it is not.** `Var` is not either, `Pair::ALL` has a fixed array length that is public API, and `Capabilities(u32)` caps at 32 pairs. Adding a materials input after 0.1 is therefore a breaking change | input.rs:9 (`Var`), :29 (`Pair`), :53 (`ALL: [Pair; 19]`), :273 (`Capabilities(u32)`) | Mark `Pair`, `Var` and `Basis` `#[non_exhaustive]`; make `ALL` a `&'static [Pair]`; make the bitset private and wider |
| E12 | minor | D12 "no panics on input" | **The no-panic policy is not enforced: a panic is reachable today.** Several public entry points also end in `todo!`, contrary to §9 ("todo! only behind an undeclared capability"). On wasm32-unknown-unknown a panic aborts the instance, and the page loses its registry | Probe: `batch::evaluate` with `outputs: &[]` panics "chunk size must be non-zero" at batch.rs:117 (`chunks_mut(chunk * m)`, m = 0). `todo!` is reachable at flash.rs:119 (any `Guess` curve, third-party included), fluid.rs:267 (IIR/ASHRAE/NBP), fluid.rs:223 and registry.rs:183 (`from_pack`) | At M0, replace the `todo!`s with `Unsupported`/`NoModel` and return early for m = 0. Add a proptest/fuzz target over `BatchRequest` shapes and `Input::from_raw`, a CI grep gate for `todo!` and `unimplemented!` outside tests, and `clippy::indexing_slicing` in batch and flash |
| E13 | minor | D17 lints vs D11 | **The workspace lint bans contradict the specified C ABI.** The thread-local last error, the generational handle table and the shim's global errstring all trip clippy. The compat model-cache policy (map 01 Q8) is undecided, and the ban blocks the obvious implementation | Probe: adding `thread_local! { RefCell<String> }` and `static Mutex<..>` to cprs-capi gives `error: use of a disallowed type std::cell::RefCell` / `std::sync::Mutex` under `clippy -D warnings`. The bans live in the workspace clippy.toml:5-11 | Exempt `cprs-capi` only, with `#![allow(clippy::disallowed_types)]` and a reason. Decide map 01 Q8 in §14: no compat cache, callers hold mixture handles |
| E14 | minor | D7 hash gate, D5 `ModelKey` | **The freshness hash and `ModelKey` cover only part of the model.** Corrections to Gaussian, NonAnalytic, GaoB or Lemmon2005 terms would not flip the gate (Lean's fatal flaw would return at M3/M4). `ModelKey` omits α⁰ and M, so the `ForeignState` guard and the future surrogate key accept states from a model that differs only in α⁰. Both register EOS corrections are exact rescalings, so they need no VLE fallback | `eos_hash` covers only R, T_r, ρ_r and power terms (data.rs:154). Key = eos_hash + name (data.rs:182). The rescaling is *inference*: from p = ρRT(1+δα^r_δ) and phase equilibrium being invariant in (τ, δ), an R edit scales p_sat by R′/R, and a ρ_r edit scales ρ′, ρ″ and p_sat by ρ_r′/ρ_r (DIV-0001, DIV-0003) | Hash the canonical bytes of the whole EOS section, with one function shared with datagen and a property test that any byte change flips it. Make `ModelKey` hash the whole compiled model. Make `Edit::GasConstant` and `ReducingDensity` rescale the SA exactly (staying `Exact`); use `Guess` only for other edits |
| E15 | minor | D13 register, `Edit` | **The divergence register is under-seeded and `Edit` is too narrow** | Map 12 §6.3 and the map 10 §8.5 seed list: four reducing-density fluids (N₂, Ethylene **+M**, OrthoHydrogen **+M**, n-Undecane), R1233zd(E) viscosity, PR/SRK entropy, the C virial, below-Tmin. Only DIV-0001..0005 exist (cprs-verify/src/register.rs). `Edit` (data.rs:59) has no `MolarMass` or transport edits. Map 13 R1 lists 15 R-audit candidates | Seed the register from map 12 §6.3 and map 10 §8.5 at M1, with `Investigate` stubs for unverified candidates. Add `Edit::MolarMass` and transport-coefficient edits |
| E16 | minor | D12 determinism, D2 `Real` | **The transcendental choke point is incomplete, and the cross-target bit-identity is asserted as fact.** sinh, cosh, expm1, atan and sqrt bypass the bans and the `Real` trait. Because `Real` is public and unsealed, adding required methods later breaks external implementations | The clippy.toml:12-20 bans cover exp, ln, powf, powi and mul_add only. Map 02 §3.2 uses sinh/cosh (AlyLee, GERG2004Sinh/Cosh) and recommends `ln(-expm1(-x))` (map 02 §9). Map 05 §3 needs sqrt and atan (Olchowy-Sengers). Research dependencies §4 Warnings (line 519) says libm's cross-target identity is "unverified; prove it with a cross-target test", but §9 and 01-panel state it as fact | Ban every `f64` transcendental outside `num::math`. Add expm1, ln_1p, sinh, cosh, atan, sqrt and abs to `Real` now (defaulted where possible), and seal the trait or default every method. Reword the claim as "proved by the M9 cross-target hash" |
| E17 | minor | D5, D12 critical point | **At the Water and CO₂ critical points the whole flash fails, with the wrong error.** A divergent A₂₀ (cv) makes `State::single` reject the state as `MechanicallyUnstable`, which loses p, h and s. `relations::cp` returns `Ok(inf)` at 2A₀₁ + A₀₂ = 0 | state.rs:114 refuses any non-finite bundle entry; relations.rs:48. Map 02 §2 and §3.1 list the non-analytic terms (Water, CO₂). Digest 02 open question: "δ = 1 4th derivative infinite, cp divergent at reducing point" | Decide now: keep p, h, s, and have cp, cv, w return `Undefined { prop, Phase::CriticalPoint }`. Add critical-point fixtures for Water and CO₂ |
| E18 | minor | D15 always-green | **The M5 DT gate depends on M6.** The DT phase rule needs the saturation curve for every T < Tc, and per-pair `Capabilities` cannot declare "DT above Tc only" | flash.rs:116 (`fluid.saturation()?.ok_or(MissingPart)`). M5 = DT; M6 = lazy superancillary | Move superancillary *evaluation* (Clenshaw only, no VLE) into M5, or list M5's DT fixture subset explicitly in PLAN.md |
| E19 | minor | D11 "WASM (browser + WASI)" | **"WASI" means only that the core compiles for wasip2.** A non-Rust WASI host has nothing to load | cprs-wasm/src/lib.rs:9 ("WASI needs no facade ... a WIT component waits for a consumer and wasip3") | Ship a minimal WIT world (props-si plus batch over a pack) at M10 behind a feature, or record it as an explicit user decision in §14 |

## Requirements coverage (BRIEF §1, verbatim fragments)

| Requirement | Status | Why |
|---|---|---|
| "a plan to migrate coolprop to rust" | Partial | About 65 of 85 outputs and the derivative API are missing (E1); pseudo-pure semantics are undefined (E3) |
| "modular extensible" / "modularity is again KEY" | Partial | `new_family.rs` and `gibbs_seam.rs` pass (reproduced). External two-phase states are impossible (E2); input enums are not open (E11) |
| "memory safe" | Met | Core is `forbid(unsafe_code)`; the wasm-bindgen probe compiles under forbid |
| "cross platform ... windows, linux, and wasm" | Partial | All 4 targets check (reproduced). Browser memory policy (E7), on-demand loading (E6), panics abort wasm (E12), WASI is Rust-only (E19) |
| "connected" | Met | Registries are values; facades sit over any registry |
| "rot in there we can eliminate" | Mostly | See the rot check below: ECS late failure is renamed rather than removed (E5); virials (E4); partial hash (E14) |
| "DRY, SOLID ... idiomatic modern Rust" | Mostly | One `Prop` table, one input gate, small traits. The lint policy is self-contradictory (E13); `Real` surface (E16) |
| "minimal dependencies" | Met | `cargo tree`: zero third-party crates |
| "heavy OSS clear type constraints" | Partial | Superancillary licence unresolved (E8) |
| "kernal ... speed and scalability ... parallel requests on the same and different fluids" | Met by construction, unmeasured | Send + Sync asserted (lib.rs `const _`); no locks after first touch. Performance first measured at M9 (E9) |
| "Rusts immutability should help us here" | Met | Models immutable; `OnceLock` is the only cell |
| "coolprop as varification but with source material when we find bugs" | Met | Parity/Corrected, three-part proofs, typed register; register under-seeded (E15) |
| "We will TDD this step by step" | Partial | M5 depends on M6 (E18); the no-panic gate is missing (E12) |
| "*material* properties library" / "all states of matter" | Partial | Single-phase Gibbs works out of tree; coexistence (E2) and order-3 Gibbs outputs (E1) need core edits |
| "coolprop fluids are first" | Partial | The 6 pseudo-pure fluids (E3); default saturation data licence (E8) |
| "only loading what is needed for the requests coming in" | Partial | Native: met for thermo. ECS is unproven (E5); browser (E6, E7) |
| "highfrequency and batch requests" | Met | Zero-refcount lookups, caller-owned buffers, per-cell status; a batch panic exists (E12) |
| "Layers ... must not over engineer" | Met | 8 crates, 2 traits; SIMD, mixtures and materials stay as seams |
| Follow-up: "parallel computation ... side by side ... SIMD" | Met as seams | `ExecPolicy`, `Real::Mask`, `residual_batch`, `policy_equivalence`; `Real` surface (E16) |

## Decisions D1-D17

| D | Status | Notes |
|---|---|---|
| D1 crates/features | Partial | E7 |
| D2 numerics/derivatives | Partial | Order-4 jets correct (hand-checked: IDEAL_DELTA, τ-side invariance, Legendre transform algebra); E4, E16 |
| D3 model representation | Met | Out-of-tree vdW reaches registry, flash, batch, compat and 16 threads |
| D4 pure vs mixture | Partial | Composition per instance is right; two-phase mixture results cannot flow (E2) |
| D5 State/properties | Partial | E1, E17 |
| D6 flash | Partial | Typed roots, per-call phase, acceptance gate; E3, E18 |
| D7 data pipeline | Partial | E5, E6, E8, E14 |
| D8 concurrency | Met | Deadlock only on cyclic references (E5) |
| D9 execution strategies | Met | |
| D10 materials seams | Partial | E1, E2 |
| D11 facades | Partial | E10, E13, E19 |
| D12 errors/NaN/determinism | Partial | E12, E16, E17 |
| D13 verification | Partial | E4, E15 |
| D14 licensing | Partial | E8 |
| D15 milestones | Partial | E1 unscheduled; E9, E18 |
| D16 naming | Met | |
| D17 tooling/lints | Partial | E13 |

## Fatal flaws from 01-judgments: are they fixed?

| Fatal flaw (judge) | Status | Evidence |
|---|---|---|
| Verification-first: concrete `Registry`, compat `Session` hard-wired (systems) | Fixed | `Arc<dyn ThermoModel>`, `with_model`, `props_si_in(&Registry)`; `compat_strings_reach_the_new_family` passes |
| Verification-first: lazy transport `OnceLock` missing; ECS has no resolution path (systems) | Partial | `Lazy<TransportSet>` exists (fluid.rs:78, :115), but the resolver is dropped in `load` (registry.rs:91); no test; cycles unchecked (E5) |
| Verification-first / Kernel: Gibbs solids need core edits (thermo) | Partial | Single-phase: fixed (`gibbs_seam.rs` passes). Two-phase or coexistence: not fixed (E2). Order-3 outputs: not fixed (E1) |
| Verification-first: no-core-edit never demonstrated (thermo) | Fixed | `tests/new_family.rs` |
| Lean: stale superancillary after corrections (thermo) | Partial | The hash gate exists but hashes only power terms (E14) |
| Lean: eager ECS decode (systems) | Fixed in design, unproven | E5 |
| Extensible: order-2-only currency (thermo) | Fixed for `HelmholtzModel`; reappears at `State`/`ThermoModel` | E1 |
| Kernel: (τ, δ) + `reducing()` contract (thermo) | Fixed | `HelmholtzModel` is (T, ρ) and returns reducing-invariant A_ij; invariance holds because τ^i∂^i_τ is a polynomial in τ∂_τ = −T∂_T (checked by hand) |
| Kernel: SIMD in v0.1, transport deferred (rust, thermo) | Fixed | SIMD comes after a measured gate; transport is in M8 |
| Kernel: concrete registry blocks new families (systems) | Fixed | As for the first row |

## Rot check: designed out, or renamed?

| Rot (map) | Verdict | Evidence |
|---|---|---|
| ECS reference built by name at first use, late failure (map 05 R7, map 12 R12) | **Renamed** | Still by name and still at the first transport call (E5); only the "declared in data" part is new |
| Approximate virials at δ = 1e-12 (map 12 §6.3) | **Not yet designed out** | E4 |
| Pseudo-pure "not EOS-consistent" semantics hidden (map 04 U4) | **Not designed** | E3 |
| Stale derived data / superancillary freshness (map 12 R10, map 09) | Partial | E14 |
| Global reference state (map 15) | Designed out for Rust; unreachable from strings and C | E10 |
| Sticky phase, failed update leaves state (map 01 R12/R13) | Designed out | `FlashOptions` by value (flash.rs:41-89); `Result` |
| Solver returns x(n+1) with the state at x(n); unchecked `max_iter` (map 03 §6) | Designed out | `Root { x, f, iterations, stop }` (num.rs `roots`) |
| Q = −1 sentinel, NaN Q, Q = 5 (map 01 §4c, map 12 R9) | Designed out | `Option` quality (state.rs:158); validating `Quality::new` |
| Env-var numerics, global locks (map 12 R4, map 11 F7) | Designed out | clippy bans fire (verified by the E13 probe) |
| Enum discriminants as ABI (map 01 R1) | Designed out | String keys; capi maps names |

## Reproduction (scratchpad only; nothing written to the repo)

Scratchpad: `<session scratchpad>`.

```text
cp -r docs/design/sketch $SP/sketch-critic; CARGO_TARGET_DIR=$SP/target-critic-evidence
cargo test --workspace                                  -> 29 passed (15+1+2+3+5+2 + 1 doctest)
cargo check --workspace --all-targets --target {wasm32-unknown-unknown,wasm32-wasip2,x86_64-pc-windows-msvc} -> ok
cargo clippy --workspace --all-targets -- -D warnings; cargo fmt --check -> clean
cargo tree -e features -p cprs-compat                   -> cprs-core "default" -> "fluids-all" -> cprs-data "all"   (E7)
probe test: batch::evaluate(.., outputs: &[], ..)       -> panicked at batch.rs:117 "chunk size must be non-zero"  (E12)
probe: thread_local RefCell + static Mutex in cprs-capi -> clippy: disallowed type RefCell / Mutex                (E13)
probe: two OnceLocks whose inits call each other        -> hangs; timeout 5 exit 124                             (E5)
probe: #[wasm_bindgen] under forbid(unsafe_code)        -> compiles (wasm32-unknown-unknown)
python replica of power.rs:75-76, d=1 l=1               -> b2 rel. error 3.6e-13 / 4.5e-9 / 2.2e-5 at δ = 1e-4 / 1e-8 / 1e-12  (E4)
scan dev/fluids EOS[0]: d,l integral; max d 15, max l 6; 54 power d=0 terms in 9 fluids; 135/136 have d∈{1,2}, l≥1
oracle CoolProp 8.0.0, R410A: P(T=280,Q=0)=990480.517  P(T=280,Q=1)=987288.072  Q=0.5 refused;
  T(P=1e6,Q=0/0.5/1)=280.31657/280.37003/280.42348;  DT(280 K,100 kg/m3): P=988112.05 Q=0.35909          (E3)
```
