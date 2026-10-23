# phasekit implementation plan

**Status:** living. The step-by-step TDD plan for implementing [ARCHITECTURE.md](ARCHITECTURE.md). The contract is
the user's requirements, verbatim in [BRIEF.md](BRIEF.md) §1; [DECISIONS.md](DECISIONS.md) overrides any older text.
Where the work is comes from git, never from this file ([AGENTS.md](../AGENTS.md)).
**Readers:** the user, and AI coding agents who execute one step at a time. Every step names its failing test, the
truth it uses, the code it touches and the check that ends it.
**Names:** project `phasekit`; crates `phasekit-core`, `phasekit-data`, `phasekit-compat`, `phasekit-verify`,
`phasekit-xtask`, later `phasekit-capi`, `phasekit-wasm`, `phasekit-py`; Rust paths `phasekit_core` etc.; C prefix
`pk_`. Where each fact lives, and how documents cite each other: section 2.1.

## 0. How to use this plan

### 0.1 The step loop

Every step in section 3 is one focused change, roughly one PR. Execute it like this:

1. **Read.** Read the step, the map sections it cites, the VERIFICATION.md sections it relies on and the
   ROT-REGISTER.md rows assigned to it (the milestone's **ROT rows** line). Check that the previous step is merged
   (`git log --oneline --grep '^Plan-Step: M3.1$'`) and that the step's tools are installed.
2. **Branch** from a green `main`: `m<n>.<k>-<slug>` (section 2.2).
3. **Write the failing tests first**: the ones the step names plus the proof tests of its ROT rows (names from
   ROT-REGISTER.md's Proof column). Take the expected value from, in this order: a literature arbiter (VERIFICATION.md
   §4), an analytic or identity check (VERIFICATION.md §8), an oracle fixture (VERIFICATION.md §3). If the fixture
   kind or rows do not exist yet, the step adds them to `scripts/oracle/gen.py` and commits them with their
   `MANIFEST.sha256` lines (the *Do* says so). If the API does not exist yet, add the signature with a body that
   returns the typed "not yet" error (`Error::Unsupported`, `Error::NoModel`, `LoadError::Format("... lands at
   M<n>.<k>")`). Never use `todo!`: it is denied (D12, S-06). The test must compile and fail on its assertion.
4. **See it fail** for the stated reason. Paste the failing assertion into the PR description ("red evidence").
5. **Implement** the smallest change that turns it green, in the modules and types ARCHITECTURE.md §2-§3 name.
6. **Run the gates**: G1-G9 (section 2.4) plus the step's own checks. In the same PR: raise the minimums in
   `ci/test-counts.txt` (`cargo xtask gates counts --update`), update the divergence register, and tick the step's
   ROT rows (section 2.1).
7. **Collapse the step** in the same PR: its text in section 3 becomes one ledger row, its ID and what landed (user
   decisions DL4, DL5). What it measured goes into the PR description, which becomes the squash commit's body,
   and into the test that pins it; work it leaves for a later step goes into that step's text; nothing else in the
   docs restates it.
8. **Commit and merge** (section 2.2). `main` is green after every merge.

Scaffolding steps with no runtime behaviour (toolchain, CI, licence files) use a failing *command* in place of a
failing test; the step names that command.

Code from the type sketch (`docs/history/sketch`, compiled and tested) may be copied when a step says so, but its tests
are copied **first** and seen red against a stub (or, for the one-off seed import at M0.2, seen green as the
baseline). The sketch is never edited.

### 0.2 Definition of done

| Level | Done means |
|---|---|
| Step | The named failing tests existed and were red first (evidence in the PR). They are now green. G1-G9 (section 2.4) pass locally and in CI. No new `#[ignore]` outside the grammar of section 2.1. `missing_docs` is clean. The step's ROT rows are ticked. The commit carries the step ID in its `Plan-Step:` footer. |
| Milestone | Every step is done. Every exit-gate item passes with the stated numbers: items marked *(nightly)* on the first green nightly run that includes them (Linux x86-64 only); all other items per PR on Linux x86-64, Windows MSVC and wasm32-wasip2 (D15), except items checked by `phasekit-xtask` (datagen, `gates`), which is a Linux tool (section 2.5). The closing step sets `phasekit_verify::MILESTONE` to the first milestone still open (n + 1, unless a later one closed first, as M12 did before M11), which arms two fail-closed checks: every divergence proof due by Mn exists (`tests/divergences.rs`, VERIFICATION.md §6.3) and every ROT row of Mn is ticked (`cargo xtask gates rot`). Benches for paths that landed are recorded (section 5.1). CI is green on `main` (a milestone cannot close while CI is owed). A tag `m<n>` is set on the merge commit. |
| v0.1 | M10 is done and the release checklist (section 5.4) is complete. |

### 0.3 When CoolProp and the literature disagree

CoolProp 8.0.0 is a provisional oracle; printed check tables arbitrate (D13). When a value and the oracle differ
beyond the tolerance class, triage it as [VERIFICATION.md §2](VERIFICATION.md#2-layers-of-truth-l0-l6) draws and follow
[§6.4](VERIFICATION.md#64-when-a-new-coolprop-bug-is-found-the-tdd-path); the register entry and its proof are their
own commit inside the step. Never widen a tolerance to make CoolProp pass, never "fix" a printed value without a
published erratum, never skip a failing oracle row without a register entry.

### 0.4 When a step fails its gate: stop and re-plan

Stop when any of these is true, rather than pushing on:
- the step has grown past about twice its planned size, or needs work that belongs to a later step;
- the fix would change a public type or trait in ARCHITECTURE.md §3, a decision D1-D17, or a user decision;
- a gate could pass only by weakening it (a tolerance, a skipped test, a lint `allow`);
- a new third-party dependency outside the tiers of [DEPENDENCIES.md](DEPENDENCIES.md) seems needed;
- a seam gate (zero-line core diff, `new_family.rs`) breaks.

A missed performance target or memory budget (ARCHITECTURE.md §6, §7) does not stop a step: it is logged with its
measurement in an issue labelled `future-work` and named in `ci/perf-exemptions.txt` (user decision TG1).

Then: keep the branch, open an issue titled `Re-plan M<n>.<k>: <reason>` with the evidence (failing output,
measurement, the conflicting doc line), and propose the change to this plan (a new step `M<n>.<k>a`, a reorder, or a
split). Changes to ARCHITECTURE.md decisions or user decisions need the user's explicit approval; changes that only
reorder or split steps need a reviewer's. Once a step has been merged, its ID never changes.

## 1. Scope: v0.1 and beyond

**v0.1** is M0-M10, exactly ARCHITECTURE.md D15 with the user's decisions applied:

| In v0.1 | Detail |
|---|---|
| Fluids | All 136 v8.0.0 fluids: 130 pure and 6 pseudo-pure (Air, R404A, R407C, R410A, R507A, SES36; map 13 §4), Helmholtz EOS |
| Data | `DataSet::Corrected` by default (user decision 2); `Parity` for oracle comparison; superancillaries shipped under CoolProp's MIT notice with NIST/fastchebpure credit (user decision 3a) |
| Inputs | All 19 pairs of the `pairs!` table (QT, PQ, QS, HQ, DQ, PT, DT, HT, ST, TU, DP, HP, PS, PU, HS, SU, DH, DS, DU) in both bases where the model supports them; pseudo-pure blends follow CoolProp's ancillary-defined rules (user decision 4) |
| Domain | Below the triple point refused by default; `DomainPolicy::Extrapolate` per call, flagged in the `State` (user decision 6) |
| Outputs | First-order properties, first and second partial derivatives, Z, Cp0, residual parts, exact virials, fundamental derivative; transport and σ where models exist; melting lines; reference states |
| Batch | `batch::evaluate`, point-major buffers `out[i*M + k]` (user decision 11), per-cell `Status`, optional `rayon` |
| Facades | `phasekit-compat` (PropsSI strings); C ABI `pk_*` plus the Tier A CoolPropLib.h shim with v8.0.0 codes and the process-wide `errstring`, `HAPropsSI` a documented stub (user decisions 9a, 9b); browser WASM (`phasekit-wasm`); Rust on `wasm32-wasip2`, no WIT component (user decision 8) |
| Platforms | Linux x86-64 and aarch64, Windows MSVC, wasm32-wasip2 (tested under wasmtime), wasm32-unknown-unknown (built; package tested under Node with wasm-bindgen-test at M10) |
| Licence | MIT OR Apache-2.0 (user decision D14) |

**Not in v0.1, and why**

| Not in v0.1 | Why | Where it goes |
|---|---|---|
| Cubic, PC-SAFT | New families must prove the zero-core-edit seam on a stable core first (D3) | M11, M15 |
| Mixtures, GERG, true-mixture blends | Composition stays out of the core contract; mixtures are a family crate (D4) | M13 |
| IF97, ice Ih, solids | Gibbs families arrive through the seam proved at M5 (D10) | M14: ice and IF97 below 623.15 K, what humid air needs (user decisions HA1, HA4); M27: the rest of IF97 |
| INCOMP, humid air (real `HAPropsSI`) | INCOMP its own `ThermoModel`; humid air its own type over water, air, IF97-water and ice models (map 07 §9; user decision HA4) | M17, M18 |
| SIMD lanes | Investigated in a time-boxed spike, shipped if it is faster than the scalar path (D9; Kernel's fatal flaw; user decision TG1) | M12, which may land before the `v0.1.0` tag as an opt-in feature (user decision RL4) |
| Python | After v0.1, own namespace + `compat` (user decision 7) | M16 |
| Plotting | After the mixtures milestone (user decision 13); diagram geometry, drawn by the site (user decision WS4); 3-D surfaces, characteristic curves, property maps and ternary diagrams (user decision DG6) | M19, M26 |
| The site | A page over the browser package, outside the library: it ships in no crate or package (user decisions WS1-WS3) | M21, from 2026-10-16 |
| Tier B shim (`AbstractState_*`), WIT component, network service facade | Deferred until a consumer asks (user decisions 9a, 8; BRIEF "connected": a server is an outer facade, never core) | On demand |
| TTSE/BICUBIC, REFPROP backend | Not ported: crashes and garbage reproduced on the oracle (map 08 R1-R9); proprietary and process-global (map 07) | Never: ROT-REGISTER.md's `Not-ported` rows |
| Runtime CoolProp JSON (`json` feature) | No consumer yet (S-12) | When a consumer is scheduled |
| Materials abstractions (`Substance`, min-Gibbs selection) | "One implementation is a guess" (materials R1); seams exist now (D10) | On first coexistence need |
| GPU, WASM threads, a `no_std` kernel, non-SI units in the core, CoolProp defects reproduced for parity | Non-goals (K15, K16; kernel-performance §5 Q6; map 01 Q3) | Never |

### 1.1 Requirements traced to steps

Each phrase of BRIEF §1, the mechanism that satisfies it and the gate that keeps it: [ARCHITECTURE.md
§1](ARCHITECTURE.md#1-goals-and-non-goals). The steps that built each are in section 3's ledger.

## 2. Conventions

### 2.1 IDs and references

**Where each fact lives** (user decision DL4). Every other document links to the home and never restates it; a PR
that changes a fact changes its home.

| Fact | Its one home |
|---|---|
| The user's requirements | BRIEF.md §1 |
| The user's decisions | [decisions/](decisions/), each by its id ([DECISIONS.md](DECISIONS.md) says how) |
| The design: decisions D1-D17, seams, invariants, performance targets | ARCHITECTURE.md (§4, §2-§3, §7) |
| Types, signatures, what is built | the code in `crates/` (rustdoc) |
| Steps, exit gates, G1-G9, what is still open | this plan (§3, §2.4, §6) |
| What a finished step did and measured | its PR description (the squash commit's body) and the test that pins it |
| Where the work is | git: `git tag`, the `Plan-Step:` trailers, `phasekit_verify::MILESTONE` |
| Fixtures, kinds, tolerance classes, xtask gate rules, benches | VERIFICATION.md (its §5 classes locked to `phasekit_verify::tolerance` by a test) |
| Divergence entries | `crates/phasekit-verify/src/register.rs`; their policies and workflow, VERIFICATION.md §6 |
| CoolProp problems designed out, and their proofs | ROT-REGISTER.md |
| Branches, commits, PRs, identity, signing, SemVer, toolchain | [CONTRIBUTING.md](../CONTRIBUTING.md) |
| Rules for coding agents | [AGENTS.md](../AGENTS.md) |
| Lints and bans | `[workspace.lints]` in the root `Cargo.toml`; `clippy.toml` |
| Dependency tiers | [DEPENDENCIES.md](DEPENDENCIES.md); licences and bans in `deny.toml` |
| MSRV; the stable toolchain CI uses | `rust-version` in the root `Cargo.toml`; `.github/actions/rust/action.yml` |
| CI jobs | `.github/workflows/` (VERIFICATION.md §11.3 says what each is for) |
| Evidence | `docs/coolprop-map/`, `docs/research/` (frozen: cited, not followed) |
| How the design was chosen | `docs/history/` (frozen) |

**Citations.**
- "map NN §X", "map NN R#" (or another item id: F#, U#, X#, C#, Q#) = `docs/coolprop-map/NN-*.md`.
- K# = `docs/research/kernel-performance.md`; R# and T# = `dependencies.md`; P# = `prior-art.md`; "materials R#/S#" =
  `materials-extensibility.md`; "dependencies §N" and the like name the research file.
- D1-D17 = ARCHITECTURE.md §4. "User decision X" = the row with id X in a file of `decisions/` (the first pass numbered
  its rows 1-13 and D14; later ids carry a prefix: TQ1, MS1, DL4). ROT-NNN = ROT-REGISTER.md rows. DIV-NNNN =
  `register.rs` entries. S-## and E## = critic issues in [history/03-decision-log.md](history/03-decision-log.md).
- A bare "§N" or "section N" is the citing document's own, except in ROT-REGISTER.md, where it is ARCHITECTURE.md's.
- *(inference)* marks a claim grounded in none of these; *(decision)* a choice the citing document settles.

- **Steps** are `M<n>.<k>` (M3.2). A step inserted after a neighbour has been merged gets a letter (`M3.2a`); merged
  IDs never change.
- **Milestones** are M0-M10 (v0.1), M11+ after it (section 4).
- **Rot rows** are cited by ROT-REGISTER.md id (`ROT-074`). Each milestone ends with a **ROT rows** line that assigns
  every row of that milestone to the step that writes its proof. **Ticking** a row means, in the step's PR: in the
  Proof cell, drop the word "new" before each test the step delivers and append the step id in brackets (`[M6.2]`).
  `cargo xtask gates rot` fails when a row whose milestone is below `phasekit_verify::MILESTONE` still has a GAP status
  or a proof still marked "new" that is due by then (a "new" proof followed by "(Mk)" is due at Mk, otherwise at the
  row's first milestone).
- **Map items** appear as evidence ("map 03 §6", "map 11 F5"); the rot they describe is cited by its ROT id.
- **Divergences** are `DIV-0001`...; `register.rs` is the one list (VERIFICATION.md §6.6). A new one takes the next
  free number (section 0.3).
- **Tests** cite their truth in a doc comment: `/// Arbiter: IAPWS R6-95(2018) Table 6`, `/// Oracle: CoolProp
  8.0.0, fixtures/coolprop-8.0.0/eos/Water.csv` or `/// Rot: ROT-069`.
- **Ignores** follow VERIFICATION.md §11.2 `ignores`: the reason starts with `DIV-NNNN: `, `issue #N: ` or `nightly: `
  (`nightly:` only in `tests/sweeps.rs`); anything else fails G8 (map 10 R14).

### 2.2 Branches, commits, PRs, versions, toolchain

[CONTRIBUTING.md](../CONTRIBUTING.md) is the one definition: one step per branch and PR (`m3.2-power-oracle`), a
Conventional Commit title with the `Plan-Step: M3.2` footer, squash merge only, the GitHub identity and signed
commits, SemVer, the toolchain, and generated files committed in the step that changes their generator.

### 2.3 Names and paths (shared table)

Where things live, and the section that defines each. VERIFICATION.md uses the same names.

| What | Path | Defined in |
|---|---|---|
| Workspace manifest, lints, `clippy.toml`, `rustfmt.toml`, `deny.toml`, `REUSE.toml`, `Cargo.lock` | repo root | D17; M0.2, M0.7 |
| Cargo alias `cargo xtask`, wasip2 runner | `.cargo/config.toml` | M0.2, M0.5 |
| Crates | `crates/phasekit-*` (`members = ["crates/*"]`) | ARCHITECTURE.md §2 |
| Unit tests | `#[cfg(test)] mod tests` in the module they test | — |
| Corpus (integration) tests | `crates/phasekit-verify/tests/<area>.rs` (`divergences.rs`, `arbiters.rs`, `eos.rs`, `sat.rs`, `flash.rs`, `transport.rs`, `new_family.rs`, `gibbs_seam.rs`, `lazy_load.rs`, `sweeps.rs`, ...) | VERIFICATION.md §2 |
| Fixtures: the oracle lock, the manifest, oracle kinds, papers, multiprecision points, hashes, maths reference values | `crates/phasekit-verify/fixtures/` | VERIFICATION.md §3.4 |
| Nightly full set | `crates/phasekit-verify/fixtures-full/` (gitignored) | VERIFICATION.md §3.6 |
| Divergence register and proofs | `crates/phasekit-verify/src/register.rs` (`DIVERGENCES`, `check_register`); `tests/divergences.rs`; `phasekit_verify::MILESTONE` | VERIFICATION.md §6 |
| Arbiter records | `phasekit_verify::arbiters::ARBITERS`; `tests/arbiters.rs` | VERIFICATION.md §4.3 |
| Sampler | `phasekit_verify::sample::SplitMix64` | VERIFICATION.md §3.1 |
| Oracle generator | `scripts/oracle/gen.py`, driven by `cargo xtask oracle` | VERIFICATION.md §3.1 |
| Papers used for transcription | `reference/papers/` (gitignored; each fixture header records the file's sha256) | VERIFICATION.md §4.2 |
| Benches | `crates/phasekit-verify/benches/*.rs` (`phasekit-verify` stays unpublished; dependencies §2.12) | VERIFICATION.md §12 |
| Bench results | `crates/phasekit-verify/benches/results/<milestone>-<machine>.csv`, written by `cargo xtask bench --record` | VERIFICATION.md §12 |
| C++ baseline | harness `scripts/baseline/{build.sh,CMakeLists.txt,coolprop_baseline.cpp}`, built in a scratch directory from a copy of `reference/CoolProp`; output `crates/phasekit-verify/benches/baseline/coolprop-8.0.0-<machine>.csv`; `scripts/baseline/reference-status.txt` | VERIFICATION.md §12 |
| CoolProp checkout | `reference/CoolProp` (gitignored, read-only, pinned ae81610e; `scripts/fetch-coolprop.sh`, anonymous clone); per-file sha256 of `dev/fluids/*.json` in `data/fluids.lock` | M0 prerequisites, M2.1 |
| Data corrections | `data/corrections.csv` (DIV id, fluid, field, v8.0.0 value, corrected value, citation) | VERIFICATION.md §6.4 |
| Generated data | `crates/phasekit-data/blobs/*.bin`, `crates/phasekit-data/src/generated.rs`, its `Cargo.toml` features | ARCHITECTURE.md §8; M2.4-M2.5 |
| Executed-test minimums | `ci/test-counts.txt` (`<target> <package>::<binary> <min>`; `# lowered: <reason>` comments) | VERIFICATION.md §11.2 `counts` |
| CI workflows | `.github/workflows/{ci,nightly,weekly}.yml`, toolchain action `.github/actions/rust`; `pr-title.yml` (CONTRIBUTING.md) | VERIFICATION.md §11.3; M0.6 |
| C header (M10) | `crates/phasekit-capi/include/phasekit.h` (cbindgen CLI output, committed; R15) | M10.4 |

### 2.4 Standard gates

This table is the one definition of G1-G9.

| Gate | Command | Where it runs | From |
|---|---|---|---|
| G1 | `cargo fmt --all --check` | local; CI linux | M0.2 |
| G2 | `cargo clippy --workspace --all-targets -- -D warnings` | local; CI linux | M0.2 |
| G3 | `cargo nextest run --workspace --profile ci` and `cargo test --doc --workspace` (nextest runs every test in its own process, all binaries in one parallel pool, and runs no doctests; `.config/nextest.toml`) | local (every crate); CI linux (every crate), windows and aarch64 (`--exclude phasekit-xtask`) | M0.2 |
| G4 | `cargo nextest run --workspace --exclude phasekit-xtask --target wasm32-wasip2 --profile ci` and `cargo test --doc` with the same arguments (runner `wasmtime`) | local; CI wasip2 | M0.5 |
| G5 | `cargo check --workspace --target wasm32-unknown-unknown` | local; CI wasm-browser | M0.2 |
| G6 | `cargo check --workspace --all-targets --target x86_64-pc-windows-msvc` | local (check only: no MSVC linker); CI windows runs G3 | M0.2 |
| G7 | `cargo clippy -p phasekit-core --no-default-features --all-targets -- -D warnings` (+ `-p phasekit-compat` from M5.9) | local; CI linux | M0.2 |
| G8 | `cargo xtask gates all`; each subcommand, its rule and the milestone it starts at: VERIFICATION.md §11.2 | local; CI linux (needs `reference/CoolProp`, fetched and cached by the job) | M0.4 |
| G9 | `scripts/site/check.sh`: the site's build (`scripts/site/build.sh`), `prettier --check`, `svelte-check`, Vitest, StrykerJS on the site TypeScript changed against `origin/main` (`scripts/site/mutants.sh`: a surviving mutant fails it, as G8's `gates mutants` does; M21.7), the page's budget (`site/size-budget.txt`) and Playwright on Chromium, its screenshot comparisons on CI's runner only, where their baselines are made (M21.2) | local (Node, npm, cargo-about and wasm-bindgen-cli; `npx playwright install chromium` once); CI site, on every PR | M21.1 |

CI also runs what no local gate does (VERIFICATION.md §11.3): the aarch64 tests, G3 on the MSRV, the nightly oracle
sweep, the weekly latest-dependency and full mutants runs, and from M9 the gungraun benches. `cargo deny check`,
`reuse lint` and `cargo shear` run locally and in CI.

### 2.5 Naming and code rules

- Crate, module and type names are those of ARCHITECTURE.md §2-§3. A name not there is chosen in the step and
  recorded in the PR.
- Every blob section has a readable form: a step that fills or changes a section of blob v1 (M5.2 superancillary,
  M5.2a caloric curves, M8.1 transport, ...) extends `cargo xtask fluid` in the same PR, and
  `every_blob_section_is_dumped` fails until it does (user decision FD1; M2.9a).
- `pk_*` for every native C symbol. CoolProp's unprefixed names exist only behind the `coolproplib-shim` feature,
  in a separate build (D11).
- Fluid features: `fluid-<name>`, `<name>` = CoolProp canonical name lowercased, each run of non-alphanumerics replaced
  by `-`, trailing `-` trimmed (`R1234ze(E)` → `fluid-r1234ze-e`, `n-Propane` → `fluid-n-propane`) *(inference;
  M2.5 tests that it is collision-free)*.
- Tests are snake_case sentences stating the property (`dt_below_the_triple_point_is_refused`), as in the sketch.
- Thread-spawning tests are `#[cfg(not(target_family = "wasm"))]` (the seed already gates its three). No test touches
  the network. Tests that spawn processes or read `reference/` live in `phasekit-xtask`, which is a Linux and dev-box
  tool: G3 runs them on Linux, G4 and the windows and aarch64 jobs exclude the crate.
- Every new dependency is named in its step, fits the tiers of [DEPENDENCIES.md](DEPENDENCIES.md) (core: std only
  until M9), and passes `cargo deny check`. Dev-dependencies that do not build for wasm (criterion, gungraun) go under
  `[target.'cfg(not(target_family = "wasm"))'.dev-dependencies]`; gungraun under `cfg(target_os = "linux")`, its bench
  file compiling to an empty `main` elsewhere so G6 `--all-targets` still checks *(inference)*.

## 3. Milestones to v0.1

A finished step is one ledger row: its ID and what landed (section 0.1 item 7). Its squash commit, whose body is the
PR description with the step's tests, red evidence and measurements, is `git log --grep '^Plan-Step: M7.5$'`. An open
step keeps its full text.

| M | Deliverable (D15, refined) |
|---|---|
| M0 | Toolchain, workspace seeded from the sketch, lints, xtask gates, test-quality gates, CI (user checkpoint), licences |
| M1 | Verification kit, oracle generator and lock, register and arbiter machinery, paper corpus, proptest, C++ baseline (time, memory, threads), mp check points |
| M2 | Datagen, blob v1, index, features, Parity/Corrected, hash gate, readable fluid dump, citations on real data, caloric-curve contract |
| M3 | Separable residual kinds on real data, `Jet4` with num-dual oracle |
| M4 | NonAnalytic, all ideal kinds, all 136 compile |
| M5 | Relations, SA evaluation, caloric curves, DT, gauge, partials, virials, batch, compat, seam gates |
| M6 | fastchebpure fetch, roots, pure VLE, critical points, QT/PQ, Guess polish, pseudo-pure rules, saturation arbiters |
| M7 | All 19 pairs, phase rule, second partials, fundamental derivative |
| M8 | Transport decode and arbiters, σ, transport (staged, IAPWS, ECS), melting |
| M9 | rayon, `libm` decision, perf and memory gates enforced |
| M10 | Reference states, full compat, `pk_*`, Tier A shim, browser WASM → v0.1 |

### M0 Toolchain, workspace and CI

| Step | Landed |
|---|---|
| M0.1 | Toolchain check and wasmtime |
| M0.2 | Workspace seeded from the sketch |
| M0.3 | Lint probes |
| M0.4 | xtask gates v0 |
| M0.4a | Test-quality gates |
| M0.5 | wasip2 tests under wasmtime |
| M0.6 | CI |
| M0.7 | Licences, REUSE, supply chain |

### M1 Verification kit and oracle

| Step | Landed |
|---|---|
| M1.1 | Fixture reader |
| M1.2 | SplitMix64 sampler |
| M1.3 | Oracle generator and lock |
| M1.4 | First fixtures and the manifest |
| M1.5 | Tolerance classes and printed digits |
| M1.6 | Register schema and proof dispatcher |
| M1.7 | Arbiter records |
| M1.8 | Paper corpus: format, fetch list, double entry, IAPWS-95 |
| M1.9 | Lemmon et al. 2016 Table 7 |
| M1.10 | Thol et al. 2016 Table 3 |
| M1.11 | NIST IR 8474 Tables 3 and 4 |
| M1.12 | CoolProp test rows |
| M1.13 | Register facts reproduced |
| M1.14 | Property tests |
| M1.15 | C++ CoolProp baseline |
| M1.15a | C++ baseline: memory and thread scaling |
| M1.16 | Nightly sweep |
| M1.17 | Superancillary check points |

### M2 Data pipeline

| Step | Landed |
|---|---|
| M2.1 | Serde mirror |
| M2.2 | FNV-1a stamps |
| M2.3 | Closed enums and validation |
| M2.4 | Blob format v1 |
| M2.5 | Index, features, references |
| M2.6 | Real data in the registry; toy path removed |
| M2.7 | Parity and Corrected |
| M2.8 | Superancillary freshness |
| M2.9 | Lookup cost |
| M2.9a | Readable fluid data |
| M2.10 | Citations |
| M2.11 | Caloric-curve contract |

### M3 Separable residual kinds and `Jet4`

| Step | Landed |
|---|---|
| M3.1 | Power blocks against the oracle |
| M3.2 | `Jet4` with a num-dual oracle |
| M3.3 | Exponential, Lemmon2005, DoubleExponential |
| M3.4 | Gaussian |
| M3.5 | GaoB |
| M3.6 | α^r totals |
| M3.7 | Zero density on real data |
| M3.8 | α^r bench |

### M4 NonAnalytic, ideal kinds, all 136 fluids

| Step | Landed |
|---|---|
| M4.1 | NonAnalytic |
| M4.2 | Ideal kinds |
| M4.3 | α⁰ against the oracle |
| M4.4 | All fluids compile |
| M4.5 | IAPWS-95 Table 6 |
| M4.6 | Critical point |
| M4.7 | Constants audit |

### M5 Properties, DT, superancillary, gauge, batch, compat, seams

| Step | Landed |
|---|---|
| M5.1 | Properties at (T, ρ) |
| M5.2 | Superancillary evaluation |
| M5.2a | Caloric curves |
| M5.3 | DT below the critical temperature |
| M5.4 | Gauge and custom reference states |
| M5.5 | First partials, Cp0, residual parts |
| M5.6 | Exact virials |
| M5.7 | Paper arbiters |
| M5.8 | Batch on real fluids |
| M5.9 | `phasekit-compat` joins |
| M5.10 | Seam gates with a subcritical point |
| M5.11 | DT bench |

### M6 Saturation, VLE and critical points

| Step | Landed |
|---|---|
| M6.1 | fastchebpure outputcheck files |
| M6.2 | Root toolbox and small solves |
| M6.3 | Pure VLE |
| M6.4 | Superancillary vs VLE |
| M6.5 | Exact rescaling proved |
| M6.6 | Stale curve polish |
| M6.7 | Critical points |
| M6.8 | QT and PQ |
| M6.9 | Pseudo-pure rules |
| M6.10 | Saturation arbiters |
| M6.11 | Saturation benches |

### M7 All input pairs

| Step | Landed |
|---|---|
| M7.1 | Density solver ρ(T, p) |
| M7.2 | PT and DT phase rule complete |
| M7.3 | PH, PS, PU |
| M7.4 | DH, DS, DU |
| M7.5 | HT, ST, TU |
| M7.6 | DP |
| M7.7 | QS, HQ, DQ |
| M7.8 | HS and SU |
| M7.9 | Capability matrix |
| M7.9a | Duplication refactor |
| M7.9b | EOS term scaffolding |
| M7.9c | Complexity gate |
| M7.9d | Test nesting |
| M7.9e | Flash search scaffolds |
| M7.9f | Verify library and xtask |
| M7.10 | Order-3 outputs |

### M8 Transport, surface tension, melting

| Step | Landed |
|---|---|
| M8.1 | Transport decode and closed enums |
| M8.2 | Transport, σ and melting arbiters transcribed |
| M8.3 | CoolProp transport rows relabelled |
| M8.4 | Surface tension |
| M8.5 | Staged viscosity |
| M8.6 | Staged conductivity and Olchowy-Sengers |
| M8.7 | IAPWS water and heavy water |
| M8.8 | ECS |
| M8.9 | rhosr-CS, Chung, other hard-coded models |
| M8.10 | Two-phase and missing models |
| M8.11 | Melting lines |
| M8.12 | Transport bench |

### M9 Parallel batches, `libm` and performance gates

| Step | Landed |
|---|---|
| M9.1 | `ExecPolicy::Parallel` on rayon |
| M9.2 | Thread scaling recorded |
| M9.2a | Non-analytic terms at the requested order |
| M9.3 | `libm` backend |
| M9.4 | Cross-target hash, the `libm` hash committed |
| M9.5 | `libm` decision: it stays opt-in |
| M9.6 | Performance gates: the instruction gate in CI, §7 misses re-planned |
| M9.7 | Memory budgets, M9 closed |

### M10 Facades and v0.1

**Goal.** Reference states, the full PropsSI grammar, the C ABI (`pk_*` and the Tier A shim) and the browser package;
release v0.1 (D11, ARCHITECTURE.md §12). **Prerequisites.** M9; the shim library file name and the shim's
`set_config_*` behaviour (user decisions P1 and P6); tools: cbindgen CLI, `wasm-bindgen-cli` matching the crate
version, Node.js LTS (wasm-bindgen-test), cargo-about, cargo-semver-checks.

| Step | Landed |
|---|---|
| M10.1 | IIR, ASHRAE, NBP and `Native` reference states, the `refstate` kind |
| M10.2 | CoolProp's 85 outputs by name (the 8 α terms `NoModel` until M10.3), Props1SI, PhaseSI, fill policies, blob v7's published constants, the `props` kind |
| M10.3 | The strict `d(X)/d(Y)\|Z` and `d(d(X)/d(Y)\|Z)/d(W)\|V` grammar, the 8 τ- and δ-scaled α outputs, the `props` kind's 22 derivative strings |
| M10.4 | `phasekit-capi`: the `pk_*` C ABI (registries, fluids, the PropsSI strings), its committed header, `gates abi` |
| M10.5 | `pk_batch`: the point-major C batch on a `pk_fluid`, each cell's `PK_*` status; compat's `batch_si`, the batch over the strings |
| M10.6 | The Tier A CoolPropLib.h shim (`coolproplib-shim`): v8.0.0's codes as a pinned table, the process-wide `errstring`, the `HAPropsSI` stub, `set_config_*` per P6 (MIGRATION.md); the `codes` kind; `gates abi`'s Tier A exports |
| M10.7 | The FP guard of every export: traps masked and round-to-nearest inside; `pk_*` restores the caller's environment, the shim clears the sticky flags as v8.0.0 |
| M10.8 | `phasekit-wasm`: the browser package (`PropsSI`, the typed flash, one batch over typed arrays, `withPack`, `withReference`, `free()`), its Node tests on the baseline and `simd128` builds, `gates features` over its tree and its size budget |
| M10.9 | v0.1.0: the release checklist (section 5.4), the release workflow and its scripts, `cargo xtask changelog`, MIGRATION.md's examples; M10 closed |

**Exit gate.** G1-G8 incl. `abi` and the wasm `features` check; reference states within `RefAnchor`; 85-output gate; C
smoke on Linux and Windows; shim codes match v8.0.0 and the cross-thread `errstring` handoff works; `HAPropsSI` stub
documented; both FP-guard semantics; both wasm builds; full nightly sweep green *(nightly)*; the section 5.4 checklist
complete.
**ROT rows.** M10.1: ROT-097, ROT-099, ROT-100, ROT-101. M10.2: ROT-008, ROT-057, ROT-163, ROT-164, ROT-166, ROT-167,
ROT-168, ROT-170, ROT-172. M10.4: ROT-023, ROT-148, ROT-149, ROT-150, ROT-159, ROT-169. M10.6: ROT-019, ROT-146,
ROT-147. M10.7: ROT-151. M10.8: ROT-154, ROT-157, ROT-158. M10.9: ROT-155.
**User decisions implemented.** 9a (`pk_*` + Tier A, `HAPropsSI` stub), 9b (v8.0.0 codes, process-wide `errstring`,
v8.0.0 FP-flag clearing in the shim only), 8 (wasip2 Rust only, no WIT), 1 (`pk_`), 11 (C and JS point-major), 12
(`libm` stays opt-in, ARCHITECTURE.md D12).

## 4. Post-0.1 roadmap

Each milestone starts only when its trigger fires and the previous release is green. IDs fix names, not order: M15,
M16 and M17 depend on nothing but v0.1 and may be reordered by demand. Every family milestone ends with the
conformance kit (VERIFICATION.md §10) and, where marked, a **zero-line core diff**: `cargo xtask gates core-frozen`
(`git diff <start-tag>..HEAD -- crates/phasekit-core` is empty; D3).

```text
v0.1 ─┬─ M11 cubic ───────── M15 PC-SAFT
      ├─ M13 mixtures ────── M19 diagram geometry ── M26 more diagrams
      ├─ M14 ice, IF97 water ─ M18 humid air (its chart drawn at M21.11; M28 error messages before M18.11); M27 the
      │    rest of IF97 on demand
      ├─ M21 site: M21.1-M21.10 landed; M21.11 and M21.12 after M18
      └─ M12 SIMD spike, M16 Python, M17 INCOMP; on demand: Tier B shim, WIT, network service, materials
```

| M | Crate | Entry trigger | Scope | Exit gate |
|---|---|---|---|---|
| M11 | `phasekit-cubic` | The user's word, before `v0.1.0` (user decision CB2) | vdW/SRK/PR via Δ1/Δ2, Soave/MC/Twu alpha, parameters from the canonical fluid records, borrowed HEOS ideal gas, cubic superancillary (map 06 U1-U5), eight steps: [M11 Cubic family](#m11-cubic-family) | Zero-line core diff; `new_family` conformance; cubic-R register entry (below); DIV-0010 proof by the entropy identity (oracle PR propane 400 K, 1 bar: T(∂s/∂T)_p = 91.35 vs cp = 93.89 J/mol/K; map 12 §6.3); Z_c = 1/3 (SRK), 0.30740 (PR) (map 06 §8) |
| M12 | core `simd` feature | Built from 2026-10-13 in a worktree; merged when green, before the `v0.1.0` tag if it is ready first (user decision RL4) | A time-boxed SIMD spike (the user's follow-up asks to investigate SIMD), ten steps: [M12 SIMD lanes](#m12-simd-lanes) | ≥ 2.5× α^r batch throughput on AVX2 over the scalar path is the target (user decisions SL3, TG1); the feature ships if it is faster than the scalar path, otherwise the spike's measurements are recorded and the stack closed (D9) |
| M13 | `phasekit-mix` | The user's word, before `v0.1.0` (user decision MX1) | `Mixture: HelmholtzModel` at fixed z, own `ThermoModel` (stability, VLE, critical points, phase envelopes, `from_split`), `MixtureModel` with AD composition derivatives, GERG-2008 (`GERG2008::`, from NIST's AGA8 code), compat `A[x]&B[y]` in Rust, C and the browser, true-mixture variants (`R410A.mix`) registered beside the pseudo-pure names (D4, user decisions 4, MX2-MX7), sixteen steps: [M13 Mixtures](#m13-mixtures) | One core seam step (M13.1, MX2), then the core frozen from `m13-start`; x = [1, 0] is the pure fluid under any gauge (map 04 §8, map 15 §8); GERG teqp vectors and the AGA8 check state (map 06 U7); Bell 2023 Table XIII; DIV-0013 resolved |
| M14 | `phasekit-iapws` | The user's word, 2026-10-20, as humid air's prerequisite (user decisions HA1, HA4) | What humid air takes from IAPWS: ice Ih (R10-06(2009)) and its sublimation curve (R14-08(2011)) on `bundle_from_gibbs` and `from_split`, and IF97's regions 1, 2 and 4 below 623.15 K as one `Fluid`, neither named in compat's strings yet (HA5); one core seam step first, the virials' second T-derivatives (HA6); five steps: [M14 Ice and IF97 water](#m14-ice-and-if97-water) | **Zero-line core diff** from `m14-start`; IAPWS tables first, oracle second (v8.0.0's `IF97::Water`; it has no ice to compare); the conformance kit on IF97 water; `gibbs_seam`'s checks on the real ice model |
| M15 | `phasekit-pcsaft` | M11 done + demand | Hard chain, dispersion, association, polar terms on `Jet4` (map 06 U9-U11) | Zero-line core diff; teqp/FeOs cross-checks; oracle only where map 06 §8 says it is usable |
| M16 | `phasekit-py` | v0.1 + demand | pyo3 abi3 wheels, import name `phasekit`, `phasekit.compat.PropsSI(...)`, GIL released around kernel calls, NumPy `(N, M)` point-major (user decisions 7, 11; R16) | map 14 U1 Python-contract fixtures for the compat surface; installs beside `CoolProp` |
| M17 | `phasekit-incomp` | v0.1 + demand | INCOMP fits as a `ThermoModel`, placeholders rejected at datagen, `Example*` fluids excluded, restricted data off by default (map 07 §9; D14) | Source tables; known-bad register (map 07 I1-I14) |
| M18 | `phasekit-humidair` | M14 closed (user decision HA1) | RP-1485 humid air on injected Water, Air, IF97-water and ice `Fluid`s (map 07 H3) with its transport; compat's `HAPropsSI`, the `pk_*` humid-air symbols and the shim's real `HAPropsSI` (D11), the browser package, and in `phasekit-diagram` the psychrometric chart's geometry, ASHRAE's and Mollier's forms, and ice on Water's diagrams, drawn by the site at M21.11 and M21.12 (HA3, HA7); twelve steps: [M18 Humid air](#m18-humid-air) | The authors' printed tables and IAPWS G7-04, G8-10 and G11-15 under `Corrected` (HA2); the oracle under `Parity`, `HAPropsSI("H","T",300,"P",1e5,"R",0) = 27013.112479771713` among it (map 11 §8); the core unchanged since `m14-start` to M18.10 and since `m18-start`, M28.1's merge, after it (user decision EM5) |
| M19 | `phasekit-diagram` (T1f; user decision DG1) | M13 done (user decision 13); the user's word, 2026-10-20 | Geometry, not drawing (user decision WS4): saturation domes, isolines of T, p, h, s, ρ and quality on the p–h, T–s, p–T, T–ρ and h–s planes, split at the dome and at failed cells, sampled more densely where a line bends, with labels and ranges; M13's phase envelopes, passed in as data (DG3); the phase regions (DG2); over saturation, `flash` and batch; the browser package serves it to the site (M21.6); six steps: [M19 Diagram geometry](#m19-diagram-geometry) | Every vertex reproduces its line's value within `Flash`; a line's dome crossings are the saturation states at its value (`Exact` against `QT` or `PQ`, and `QS`, `HQ` or `DQ`); M21.2's checks pass on its geometry; the site's screenshots re-approved at M21.6 |
| M20 | core, compat, verify, CI | M12 landed (user decision PL1) | Four follow-ups: Cv, Cp, w, τ and δ as derivative variables; the conformance kit's `identities` and `part_additivity`; the intra-doc link check; the nightly report: [M20 Follow-ups](#m20-follow-ups) | Each step's checks; G1-G8 |
| M21 | `site/`, the GitHub Pages site | The user's word, 2026-10-16, beside M13 (user decision WS3) | A single-page site over the browser package: a calculator, five diagrams with the state under the cursor, sweeps, an in-browser benchmark of both builds, all 136 fluids from per-fluid packs, mixtures after M13, M19's geometry after M19, mutation testing of the site's code, picking states on the diagrams, phase regions, how a property is calculated, a psychrometric chart and ice on Water's diagrams after M18 (user decisions HA3, HA7); twelve steps: [M21 The site](#m21-the-site) | G9 at every step; the page deployed from `main`; every value on the page equals the package's (`Exact`) |
| M22 | `phasekit-mix`, core `batch` and `ThermoModel` | After M13 closes (user decision ML2) | Mixtures in lanes: α^r and α⁰ batches at fixed z through the components' lane kernels with the departures in lanes, hinted DT batches through a group hook on `ThermoModel`, the `simd128` build, then a time-boxed spike putting a flash's two phases and its stability test's trial phases in lanes; core runs a family's kernels over `Real` in its private lanes (user decisions ML1, ML3, ML5), eight steps: [M22 Mixture lanes](#m22-mixture-lanes) | Each kernel ships if faster than the scalar path (ML4); `policy_equivalence` bitwise at every dispatch level; G1-G9, and G1-G8 with `--features simd` |
| M23 | core `math` | After M13 closes (the core is frozen until `m13`, MX2); independent of M22 | In-house `exp`, `ln`, `pow` and `expm1` under 1 ulp, ported from Arm's optimized-routines, one code for the scalar path and the lanes; every build's maths if they pass M9.5's rule, so `simd` leaves `libm`; then an opt-in `fast` feature with FMA on every target (user decisions IM1-IM5, IM7-IM11), nine steps: [M23 In-house maths](#m23-in-house-maths) | Under 1 ulp on the reference values (IM3); lanes bitwise at every dispatch level; each committed hash one across the four targets; M9.5's rule applied and recorded (IM1); G1-G9, and G1-G8 with `--features simd` and with `--features fast` |
| M24 | compat, capi, wasm, mix, core | The user's word, 2026-10-18, before `v0.1.0` (user decision PH1) | Panics at the facades (issue #219): fault injection along every C export's path, the browser package's panic hook, and clippy's `indexing_slicing` and `arithmetic_side_effects` denied in the crates it reaches: [M24 Panics at the facades](#m24-panics-at-the-facades) | The fault-injection tests; clippy clean with both lints; `gates perf` |
| M25 | xtask, `.gitattributes`, docs | The user's word, 2026-10-18, beside M24; its mutants step after `m24` (user decision LM6) | Lists that merge, kept as text (LM1): `ci/test-counts.txt` by a merge driver, ROT ids unique and the register merged by union, one file per decision section, mutant exclusions as markers in the code: [M25 Lists that merge](#m25-lists-that-merge) | Each step's red tests; G1-G8; `actionlint` clean |
| M26 | `phasekit-diagram`, `phasekit-wasm`, `site/` | `m19` and M21.6 landed (the site draws from `phasekit-diagram`); the user's word (user decision DG6) | Three sets of diagrams beyond M19's planes, each computed by the library and drawn by the site: a fluid's p–v–T and u–s–v surfaces in three dimensions, drawn with three.js (user decisions DG7, DG8), characteristic curves and property maps on the planes, and three-component mixtures' ternary diagrams, the surfaces first (DG9): [M26 More diagrams](#m26-more-diagrams) | Every vertex a state the library computed; each set's arbiter; the browser package's and the page's budgets hold; tag `m26` |
| M27 | `phasekit-iapws` | A steam use case (user decision HA1) | The rest of IF97: region 2 above 623.15 K with the B23 boundary, region 3 (SR3-03's backward v(T, p) refined by Newton, ROT-241) and region 5, the backward equations for HP, PS and HS, IF97's transport (R12-08, R15-11), and `IF97::Water` by name in compat, C and the browser package (HA5); map 07 §9's `if97-*` units | Zero-line core diff; R7-97(2012)'s other tables and SR3-03 Table 5; the `if97` oracle kind over all five regions |
| M28 | core, every crate that builds an `Error`, `phasekit-wasm`, `phasekit-compat`, `phasekit-capi`, `site/` | The user's word, 2026-10-21; M28.1 after M18.10 lands, before M18.11 (user decision EM5) | Issue #191: error messages in the caller's units and in two styles, a long one in words and a short one stating the relation the value breaks; each number tagged with its quantity, six significant figures, `InvalidInput` split from structural arguments, properties in words beside their keys, each error's stable code in core and the facades (user decisions EM1-EM4, EM6-EM13), three steps: [M28 Error messages](#m28-error-messages) | Every message pinned in both styles; a caller's units reach the browser package's messages; `m18-start` tagged at M28.1's merge; G1-G9 |
| — | Tier B shim | A host needs `AbstractState_*` | ~25 exports, generational handles (map 11 U11) | v8.0.0 signatures; fuzzed |
| — | WIT component | A concrete non-Rust WASI host (user decision 8) | `wit-bindgen` component over compat (R17) | Host smoke test |
| — | Network service facade | A consumer needs phasekit over the network (BRIEF "connected") | A thin HTTP or RPC service over compat in its own crate; never core (ARCHITECTURE.md §1) | Service smoke test; core diff zero |
| — | Materials | First phase-coexistence need (ice + water) | `Substance` + min-Gibbs selector in a new crate (D10; materials R1) | Zero-line core diff |
| — | Near-critical VLE in extended precision | A use needs the VLE itself at multiprecision accuracy near Tc (idea recorded at the user's request, 2026-10-09) | Near Tc the double-precision VLE carries its rounding amplified as Θ^−1.5 (user decision NC1, measured at M6.4). Double-double (or similar) arithmetic in the near-critical solve, as fastchebpure computes its multiprecision values. A scope change: the core is std-only until M9 and has performance targets | The dense fastchebpure rows within `SatMp`'s far-field 1e-11 closer to Tc than today; no cost to the double-precision path (the M9 benches) |
| — | Cubic family in lanes | A use needs cubic batches faster (idea recorded at the user's request, 2026-10-15) | M11's cubic fluids take `ExecPolicy::Simd`'s lane path but evaluate point by point: `CubicModel` and `CubicSaturation` keep the scalar defaults of `residual_batch`, `ideal_batch`, `at_t_batch` and `at_p_batch`. α⁰ needs no core change: `CubicModel::ideal_batch` can forward to the borrowed HEOS model's, already in lanes (M12.8). α^r is generic over `Real`, so M22.2's runner can evaluate it in core's private lanes (user decision ML3); its derivatives run on `Jet4`, which is over `f64`, so they need a jet over `R: Real` (the family's own, as M22.3's `Series`, or core's), and the superancillary's Clenshaw sums (as M12.9's) can take each point's piece as inputs | `policy_equivalence` bitwise on the cubic family at every dispatch level; faster than the cubic's scalar path on M12's benches |

**M11 cubic-R register entry** (user decision CB1, which replaced decision 10; ARCHITECTURE.md §10, §11). Added in
M11.1, before any cubic code.

| Field | Value |
|---|---|
| id | DIV-0046, the next free id |
| fluids / part | every cubic fluid, each of which borrows a HEOS ideal gas / `Part::GasConstant` |
| policy, fix | `KeepOracle`, `fix: None`: a cubic model's one R is CoolProp's `R_U_CODATA` = 8.31446261815324 in both datasets, for its α^r and for the borrowed α⁰, as v8.0.0 does (map 06 C6, map 15 §2.1) |
| arbiter | the borrowed HEOS model's own α⁰ (its EOS paper's ideal-gas equation; IAPWS-95 for Water), with that model's R |
| measured | a cubic fluid's c_p⁰ is its source's times R_CODATA/R_source: −1.128e-6 for the 58 of v8.0.0's 116 cubic fluids with R = 8.314472, +1.10e-5 for Water, +1.96e-5 for CycloPropane and Propyne (R = 8.3143) |
| proof | (1) the cubic's ideal-part `Derivs` equal the borrowed model's at the same (T, ρ) bit for bit (`Exact`), and its c_p⁰ equals the source's times R_CODATA/R_source (`Term`), for every cubic fluid; (2) phasekit's PR and SRK c_p⁰ equal the oracle's (`Term`) outside the scope note; (3) the source's literature rows hold for the cubic's ideal part at the measured R ratio, and only there |
| scope note | part (2) holds only where v8.0.0's cubic α⁰ snapshot matches the HEOS α⁰ and its T_c and ρ_c are the HEOS reducing state; the drifted fluids (map 06 C5: c_p⁰ > 0.1 % off for 11 of 116; map 15 X6, §3.1) get a separate `SkipOracle` entry for α⁰-dependent cubic outputs (M11.4) |

### M11 Cubic family

**Goal.** The van der Waals, Soave-Redlich-Kwong and Peng-Robinson equations of state as a family crate,
`phasekit-cubic`, on core's public seams with a zero-line core diff (D3; ARCHITECTURE.md §11; VERIFICATION.md §10).
Each cubic fluid takes its T_c, p_c and ω from a canonical HEOS fluid and borrows that fluid's ideal gas. **Entry.**
The user's word, before `v0.1.0`, on the i7-8700K (user decision CB2); the steps merge in order when green.
**Prerequisites.** User decision CB1 (one R); M12 landed, so `m11-start` is taken on a core that M11 then leaves
alone. **Arbiters.** No cubic paper is in `reference/papers`, so the arbiters are analytic (each kind's critical-point
conditions fix Ω_a, Ω_b and Z_c; exact virials), the identities of VERIFICATION.md §8 and AD; the oracle checks PR and
SRK at c = 0 where map 06 §8 says it can, never entropy (DIV-0010) or QT beyond 1e-6. Mathias-Copeman and Twu have no
oracle: no v8.0.0 fluid carries their coefficients (map 06 §4). M11 closes by setting `MILESTONE` to 13: M12 closed
first without moving it, so M13 is then the first open milestone.

| Step | Landed |
|---|---|
| M11.1 | `m11-start` on the plan's merge commit; `gates core-frozen` against the tag `ci/core-frozen.txt` names (CI's linux job has it through `fetch-depth: 0`); `crates/phasekit-cubic` on `phasekit-core` alone (DEPENDENCIES.md T1f, `gates deps`) with `R_CODATA`; DIV-0046 and its oracle facts (`facts/register.csv`) |
| M11.2 | `Kind::{Vdw, Srk, Pr}`: Δ₁, Δ₂ and Ω_a, Ω_b by `DataSet`, `Corrected` the exact closed forms and `Parity` v8.0.0's literals, whose SRK values are 4e-10 off their own stated forms (DIV-0047); `Residual`, α^r in one kernel over `Real` on `Jet4` to order 4 (ROT-218), with a and b, ρ_max = 1/b and the exact virials; Z_c = 3/8, 1/3 and 0.307401308698704 with ∂p/∂ρ = ∂²p/∂ρ² = 0 (ROT-217); `Alpha::One` |
| M11.3 | `Alpha::{One, Soave(m), MathiasCopeman([c₁, c₂, c₃]), Twu([L, M, N])}` over `Real`, α(T_c) = 1 and four θ-derivatives against AD; `Kind::soave_m`, SRK's and PR's m(ω) as v8.0.0 writes them; Mathias-Copeman's three terms at every T, as v8.0.0 (map 06 §10 Q6 stays open); the residual and its virials' da/dT with each alpha |
| M11.4 | `CubicModel` (R_CODATA; α^r from `Residual`; α⁰ the source's, unchanged); `fluid(&source, kind, alpha, data)` from the source's published critical point and ω, its limits and molar mass, the cubic's own critical point (`CriticalOrigin::Model`); `with_cubics` registers `VDW::`, `SRK::` and `PR::` for every pure fluid, aliases prefixed, blends left out; oracle kind `cubic` (1856 rows: SRK and PR for v8.0.0's 116 cubic fluids), within 1e-12 on `Parity` for 83 fluids; DIV-0048 (21 fluids whose library T_c, p_c or ω drifted, ROT-216) and DIV-0049 (12 whose library α⁰ drifted) `SkipOracle`; DIV-0046's parts 1 and 2 |
| M11.5 | `CubicSaturation`: Bell and Deiters' (2021) SRK and PR superancillary, transcribed from v8.0.0's `cubicsuperancillary.h` (checked number for number by xtask's `cubic_superancillary_is_coolprops`), `Exact` over T̃ ∈ [0.1, 1]·T̃_c, `at_p` by an Illinois solve in ln p; every SRK and PR fluid carries it; vdW has none, so the core's VLE serves its DT in the dome and its quality pairs are `Unsupported`; its saturations are the cubic's phase equilibria (p to 1e-12 ρRT, Δg/RT to 1e-12, 1e-10 within 1e-3 of T_c; ROT-219); oracle kind `cubic_sat` (1160 rows) within 1e-13 in ρ′, 2e-12 in p and ρ″ |
| M11.6 | `tests/cubic_family.rs`: the conformance kit on vdW, SRK and PR fluids of n-Propane, Water and CO₂ in both datasets: the 19 × 2 capability matrix (vdW's quality pairs `Unsupported`, as the core declares them only with a saturation curve), gauge invariance, `Reference`/`Parallel`/`Simd` bitwise on DT, PT, QT and PQ batches, `fd_first_order` (ROT-213, ROT-214); PT at a subcritical isotherm's spinodal double roots finite, at the critical point a typed refusal (ROT-220); R_CODATA/R_source scales c_p⁰ and is no reference shift (ROT-107) |
| M11.7 | the kit's `entropy_identity(fluid, inputs)`: T (∂s/∂T)_p = c_p with the derivative from flashed entropies, within 1e-7; it holds on every cubic fluid, the out-of-tree vdW family and five embedded fluids, and fails a PR n-Propane whose ideal T-derivatives lack a chain rule (ROT-212); `tests/divergences.rs` proves DIV-0010 (phasekit's PR and SRK n-Propane meet the identity at 400 K and 1 bar, v8.0.0's facts give 91.35 against 93.89 J/(mol K)) and DIV-0046 to DIV-0049, so all are due at M11's close |
| M11.8 | M11 closed: `MILESTONE` = 13 (M12 closed first), every M11 proof due and present (DIV-0010, DIV-0046 to DIV-0049) and every M11 ROT row ticked (ROT-221, volume translation, moves to post-0.1); `crates/phasekit-core/` unchanged since `m11-start`, and `ci/core-frozen.txt` names no tag; ARCHITECTURE.md's crate table and §11, VERIFICATION.md §10 and the README name the crate |

**Exit gate.** G1-G8; `gates core-frozen` since `m11-start`; the conformance kit on every cubic kind; DIV-0010's and
DIV-0046's proofs; Z_c = 3/8 (vdW), 1/3 (SRK) and 0.30740 (PR). **ROT rows.** ROT-107, ROT-212 (its identity),
ROT-213 to ROT-220; ROT-003, ROT-009, ROT-034 (their M11 proofs). **User decisions implemented.** CB1, CB2.

### M12 SIMD lanes

**Goal.** α^r, then properties at (T, ρ), QT and PQ, evaluated across states in lanes, bitwise equal to the scalar
path, shipped as core's `simd` feature if it is faster than the scalar path (D9; ARCHITECTURE.md §7; user decision
TG1). **Entry.** Built from 2026-10-13 on the i7-8700K in its own worktree; the steps merge in order when green, and
may land before the `v0.1.0` tag (user decision RL4). **Prerequisites.** User decisions SL1 (`simd` enables `libm`)
and SL3 (the target); the tools of G1-G8; fearless_simd 1.1's MSRV is the workspace's 1.89 (user decision MS1). M12 does not
move `MILESTONE`: M11 stays the first open milestone.

| Step | Landed |
|---|---|
| M12.1 | `HelmholtzModel::residual_batch`, `ExecPolicy` at L0, the α^r batch's scalar baseline |
| M12.2 | `Lanes<W>` over `[f64; W]`; the residual on lanes, bitwise the scalar one lane by lane |
| M12.3 | `LaneKernel` and `run_scalar`/`run_lanes`/`run_parallel` in L0 `exec`; `ExecPolicy::Simd`, `Lanes<2>` at the baseline (1.14-1.62× α^r on SSE2) |
| M12.4 | the `simd` feature (fearless_simd 1.1): `run_simd` at 8, 4 or 2 lanes per level; n-Propane α^r 258 ns on AVX2 with per-lane `libm`, 1.21× std's scalar path |
| M12.5 | `Lanes::exp`, libm 0.2.16's `exp` ported to lanes with the `libm` backend, bitwise; n-Propane α^r 216 ns on AVX2, 1.44× std's scalar path |
| M12.6 | `Lanes::ln`, libm 0.2.16's `log` ported to lanes with the `libm` backend, bitwise; n-Propane α^r 215 ns on AVX2 (one `ln` per state) |
| M12.7 | the gate: four vectors to a lane operation (16 lanes on AVX2, user decision SL6); n-Propane α^r 109.2 ns, 2.87× over M12.1: the ≥ 2.5× gate passes (SL3); ≤ 0.1 µs/state missed, issue #171 |
| M12.8 | DT batches under `ExecPolicy::Simd`: α⁰ (`HelmholtzModel::ideal_batch`) and α^r in lanes, then each point's flash on its own bundle; `Lanes::expm1`, libm 0.2.16's ported bitwise (user decision SL7); n-Propane DT + h + c_p 394 ns per state on AVX2, 2.23× the `libm` scalar path and 1.65× std's |
| M12.9 | QT and PQ batches under `ExecPolicy::Simd`: `SaturationCurve::at_t_batch` and `at_p_batch`, the superancillary's Clenshaw sums in lanes (PQ's T from p by TOMS 748 per point, K14), then both phases' bundles in lanes at order 1 or 2; n-Propane QT + h 574 ns per state on AVX2, 2.59× the `libm` scalar path and 1.76× std's (PQ + h 742 ns, 2.37× and 1.64×) |
| M12.10 | the browser package's `simd128` build enables core's `simd` (`phasekit-wasm`'s `simd` feature), 8 lanes with a padded last group (user decision SL8); its batch runs `ExecPolicy::Simd` on both builds (`phasekit_compat::batch_si_with`); a size budget per build, 1 307 788 and 1 458 107 B; n-Propane under Node 26, DT + h + c_p 749 ns, QT + h 1268 ns and PQ + h 1485 ns per state on `simd128`, 1.50×, 1.45× and 1.45× M12.9's |

**Exit gate.** G1-G8 with and without `--features simd`; `policy_equivalence` bitwise at every dispatch level of the
i7-8700K, the CI runners (x86-64, aarch64) and wasip2 with `simd128`; the lane `exp`, `expm1` and `ln` bitwise equal to
`libm`'s; the `simd` build reproduces `fixtures/hash/libm.txt`; the K1/K2 paper rows pass at every level
(VERIFICATION.md §9.2); ≥ 2.5× (SL3). **ROT rows.** None. **User decisions implemented.** SL1, SL2, SL3, SL6, SL7, SL8.

### M13 Mixtures

**Goal.** Multi-fluid Helmholtz mixtures (Kunz and Wagner's form, the model of CoolProp's HEOS mixtures) and GERG-2008
as a family crate, `phasekit-mix` (D4; ARCHITECTURE.md §11). A mixture at fixed composition is a `HelmholtzModel` with
its own `ThermoModel`: stability, vapour-liquid equilibrium, critical points and phase envelopes, its two-phase states
built through `State::from_split` (user decision MX4). A `MixtureModel` trait gives composition derivatives by one
mole-number AD path. Mixtures reach compat strings (`HEOS::A[x]&B[y]`, `.mix` names), the C ABI and its shim, and the
browser package (MX5). **Entry.** The user's word, before `v0.1.0`, on the i7-8700K (MX1); the steps merge in order
when green. **Prerequisites.** User decisions MX1-MX7; M13.1's core seams (MX2), after which `m13-start` is tagged and
the core stays frozen to M13's close. **Arbiters**, all open and in `reference/papers`:
- Bell et al. 2023 Table XIII: α^r of five refrigerant pairs and their six pure fluids at z₁ = 0.4; v8.0.0 and teqp
  reproduce it to 1e-14.
- Bell et al. 2022 Table XI, with DIV-0013's fix.
- Tkaczuk et al. 2020 Table 8: p of three noble-gas pairs, printed at Σx_iR_i.
- Lemmon and Jacobsen 2004 Table 6: 22 rows of p, c_v, c_p, w and the first component's fugacity, for six HFC binaries
  and two ternaries.
- AGA8's GERG-2008 check state (21 components, 400 K, 50 MPa) and its 3645-row binary test set; the GERG TM15
  monograph (2007).

NIST IR 8570 prints no check values, so v8.0.0's rows from it are self-referential (map 10). Kunz and Wagner 2012,
Michelsen 1982a,b and Gernert et al. 2014 are closed. Bell and Jäger 2017 is free at the publisher, which refuses
scripted downloads (section 6). teqp 0.23.2 (NIST public domain) is `OtherImplementation`, differential only
(VERIFICATION.md §10): it gives GERG-2008 vectors, the multi-fluid model on v8.0.0's JSON, and VLE and critical-locus
traces. Its conversion of Lemmon's ξ/ζ pairs halves γ_T, so a generator converts them to β/γ before teqp sees them. The
oracle checks the multi-fluid model outside the known-bad states of map 04 §8. M13 closes by setting `MILESTONE` to 14.

| Step | Landed |
|---|---|
| M13.1 | Core's mixture seams (MX2): each phase point keeps its molar mass, so `State::from_split` takes phases of different M (one R) and weighs them by mass (`State::mass_quality`, `Prop::Qmass`, mass-basis values), a pure fluid's split bit for bit as before; `Input::with_quality_basis`, `NativeInput::quality_basis` and `BatchRequest::with_quality_basis`, compat's `Qmass` key a mass quality; the public `density` module (`solve` on a `Branch`, `roots`, `select` under a `RootPolicy`, the acceptance gate `reproduces`); `Registry::with_resolver` and `resolve` with the `Resolver` trait (a family caches what it builds); `HelmholtzModel::reducing`; `tests/mixture_seam.rs` |
| M13.2 | `m13-start` on M13.1's merge commit, named in `ci/core-frozen.txt`; `crates/phasekit-mix` on `phasekit-core` alone (T1f, `gates deps`), PR scope `mix`; `data/mixtures.lock` (v8.0.0's three mixture JSON files and ASHRAE 34's list); datagen's mixture step writes `src/generated.rs` (`gates datagen` compares it): 888 pairs keyed by their sorted CAS pair, β inverted where the file lists the larger CAS first, Lemmon's six ξ/ζ pairs kept as such; 28 departure functions by kind (16, 9, 3); 147 predefined mixtures (seven exact repeats dropped), ASHRAE 34's mass fractions kept for 124 of them, matched by name or library fluid; every fluid's CAS number. Its tests pin ROT-199 (the mass fractions give v8.0.0's mole fractions for the 120 the library can weigh, 80 to 1e-12, at most 8e-6 off, R405A) and ROT-200 (27 blends lack a pair, 4 name the unknown R1132a) |
| M13.3 | `MultiFluid::new(registry, names, data_set)`: GERG's reducing functions over the components' own reducing states (`HelmholtzModel::reducing`), Lemmon's ξ/ζ converted, cube roots as v8.0.0's `pow(v, 1/3)`; the three departure kinds in one kernel over `Real` on `Jet4` to order 4, against num-dual; each component at the mixture's τ and δ; departures only where F ≠ 0 (ROT-195); DIV-0013 `UsePaper` (γ_T 0.993309 under `Corrected`, phasekit-mix's corrections table; ROT-207) and DIV-0050 (Bell 2022 Table XI's pre-publication R1234yf, 1.48e-8, `KeepOracle` at 2e-8); Bell 2023 Table XIII and Bell 2022 Table XI double-entered (`paper/mixtures/`); oracle kind `mix_alphar` (all 840 buildable pairs at two states, A_ij to order 3; the 48 pairs naming fluids outside v8.0.0's library refused by both) and `oracle.lock`'s `mixtures_sha256` |
| M13.4 | `Mixture` and `mixture(registry, components, data_set)`: zero fractions trimmed (ROT-192), a one-component composition the pure fluid itself, bit for bit under any gauge (ROT-099, ROT-208), compositions validated, never normalised (ROT-193); R = R_CODATA (MX3); α⁰ with its δ-part ln δ exactly and each component's τ-part by z_i R_i/R in its own variables (ROT-062, ROT-103); limits the components' intersection (ROT-197); exact virials; hinted DT and PT through the public density solve; Tkaczuk 2020 Table 8 (through Σ x_i R_i, DIV-0051) and Lemmon and Jacobsen 2004 Table 6 (DIV-0054, measured 3e-6, 1e-5 near the critical point) double-entered; oracle kind `mix_eos` (10626 cells within `Prop` or carried `Term`); DIV-0052 (v8.0.0's s weights each ln δ_i by R_i/R, so it is not its p's) and DIV-0053 (24 fluids whose critical T is not their reducing T, α⁰ at the critical state in a v8.0.0 mixture; Table 6 sides with phasekit) `SkipOracle`; the committed fixtures' budget 24 MiB (user decision MX8) |
| M13.5 | GERG-2008 (Kunz and Wagner 2012) as a `MultiFluid` (`MultiFluid::gerg2008`, `gerg2008`, `with_gerg2008`; `GERG2008::` names, MX6; R = 8.314472, MX3): its 21 components' own equations (the residual on the departure kernel, the sinh/cosh ideal gas its own term with n₁ and n₂ computed for h = s = 0 at 298.15 K and 101.325 kPa) as `PureFluid`s with GERG's 60-700 K and 70 MPa and each one's own triple point (ROT-205), the registry curve of the same substance as their saturation's seed for core's VLE, HP/PS/PU round trips (ROT-083); its 210 binaries and 15 departure functions (ROT-195); transcribed from AGA8 at 3bdb9ab8 (MX7; `scripts/fetch-aga8.sh`, xtask's `gerg_2008_is_aga8s`, `LicenseRef-NIST-GERG`), cross-checked against GERG TM15 Tables A3.1-A3.8, double-entered (2135 cells, the 44 GERG-2008 changed named); a second generator, `scripts/oracle/teqp.py` and `teqp.lock` (`cargo xtask teqp`, VERIFICATION.md §3.8): AGA8's check state within `Prop`, its 3645-row binary set within its eighth digit and DIV-0055's 2e-10 (the sheet's θ_k = ϑ_k T_c), and teqp 0.23.2's vectors for the pure components, every binary and AGA8's 200 natural gases (ROT-198, ROT-203, ROT-206); a pair term of two absent components is 0, not 0/0 |
| M13.6 | `MixtureModel` (`components`, `composition`) on `MultiFluid`, the library's and GERG-2008's: Ψ = A^r/(RT) on the crate's multivariate Taylor numbers in (n, T, V) to order 3 (graded symmetric storage, a product O(m³)), each component's and departure function's (τ, δ) jet composed with τ(n) and δ(n); `Composition` gives ln φ_i, n (∂ln φ_i/∂n_j)_(T,p), (∂ln φ_i/∂T)_(p,n), (∂ln φ_i/∂p)_(T,n), partial molar volumes, Z and p, `Mixture` its chemical potentials (ROT-191, ROT-210); Gibbs–Duhem, the Hessian's symmetry and Euler homogeneity within `Identity`, central differences (`Fd`) and num-dual on GERG's formulas (1e-12, every derivative to order 3); `mix_eos` gains the oracle's φ_i and μ_i: φ_i within `Prop`, μ_i exempt under DIV-0052 (v8.0.0's Σ x_i μ_i is not its own g; phasekit's is, `Identity`); Lemmon and Jacobsen 2004 Table 6's fugacity column |
| M13.7 | Heidemann and Khalil's conditions at fixed z from M13.6's tensors: det(D Q D) and the cubic form along Q's D-normalised null vector, found by complete pivoting; `Mixture::critical_points` finds every point in the model's range once and keeps it (ROT-187): the stability limit on a grid in (ln T, ln ρ), the cubic form sampled where it crosses the grid's edges, each sign change polished by Newton's method, a cell split in four where the cubic form halves without changing sign; `ThermoModel::critical_point()` the highest-T point at positive p (`CriticalOrigin::Model`); the departure kernel separable (each term n T(τ) D(δ) on univariate series, to the order asked); both conditions within `Identity` at every point found; teqp 0.23.2's GERG-2008 methane/ethane locus (`gerg_crit`, differential) within `Flash`; the oracle's `all_critical_points` (`mix_crit`) within DIV-0052's measured 2e-5 (its L* weights no ideal term; with unit weights `Flash`), but DIV-0056's three points, where L*'s first row vanishes and det M* with it |
| M13.8 | `Mixture::equilibrium` and the hint-free PT flash: Wilson's K, Rachford–Rice bracketed and negative-flash capable, Michelsen's tangent-plane test from Wilson's two trial phases and one near-pure trial phase per component (successive substitution, then Newton in α = 2√W on the n ∂ln φ/∂n Hessian), the split by GDEM-accelerated substitution then Newton in the minority phase's mole numbers to a 1e-12 fugacity gate; every composition on its stable root, of the roots core's solve finds only those their branch reaches with dp/dρ > 0, polished to the rounding of p (ROT-181, ROT-183, ROT-186); single phases labelled from the critical point (ROT-009), a liquid-liquid split refused as `Unsupported` (MX4); equal fugacities, mass balance and a Gibbs decrease within `Identity` at every split of the corpus (bd:ft05, #3283, #3066, #3448, v8.0.0's Michelsen scenarios); oracle kind `mix_flash` within `Flash` and DIV-0059's 1e-7 (v8.0.0's split converges to 1e-9), DIV-0057's roots past a spinodal and DIV-0058's near-azeotropic rows skipped |
| M13.9 | QT and PQ at Q = 0 and 1 (`Mixture::saturation`, `Side`, `Given`, `Saturation`): Newton in (ln K, ln T or ln p) with the AD Jacobian, seeded by Wilson and eight successive substitutions, ending on a residual gate over the N + 1 equations at the point returned, never on the step (ROT-185), the trivial solution refused; where Wilson's seed falls into it near the critical point, M13.8's PT flash seeds it, from the split nearest the side's own end of Wilson's interval, else from its boundary with the feed's own phase, bisected; each phase on its own branch, its root by core's solve or, where core's Newton wanders at an R22-rich liquid's rounding, by Newton ended on core's acceptance gate (the PT flash's roots too); the incipient phase's composition and density on the split, fugacities at Q = 0 and 1 (ROT-194); a Q inside (0, 1) `Unsupported` until M13.10; teqp 0.23.2's GERG-2008 methane/ethane isotherms at 200 and 250 K (`gerg_vle`) within `Flash`; oracle kind `mix_sat` (the ten binaries, the predefined blends) within DIV-0060's measured 5e-7 (v8.0.0's Newton stops on its step); the blends within 3e-4 of the pseudo-pure names' ancillaries (Air 4e-3) |
| M13.10 | QT and PQ at any Q (`Mixture::coexisting`, `Coexisting`): M13.9's Newton in (ln K, ln X) with x = z/(1 + β(K − 1)) and y = Kx, so the mass balance holds by construction, Rachford–Rice at the given β closing it (ROT-182), seeded by Wilson at β and else by the bubble and dew points, ln X and ln K interpolated; a mass quality's molar β by the secant method; the solve's gate half the fugacity gate. DT without a hint: the homogeneous state where it is stable, mechanically and by Michelsen's test of the state at ρ at its own pressure, else the isochoric split, found in β between the bubble and dew points at T (in p where T has none); with a hint, `Strategy::Direct`'s state (ROT-190, `benches/mixture.rs`: 5 µs direct, 1.6 ms stable, 15 ms in the dome on the i7-8700K under load). `mix_flash` gains a `pair` column and QT, PQ and DT rows (720 in all), within DIV-0060's 5e-7; DIV-0061 (v8.0.0's DT tests the feed's own root at the EOS pressure and returns one phase inside the dome) and DIV-0062 (its interior QT and PQ miss the feed by up to 7.6e-3) skipped |
| M13.11 | Every other pair by an outer solve on the feed's homogeneous states (one model evaluation each, a tangent-plane test only at a candidate, never per step) and on M13.9-M13.10's coexisting phases: HP, PS and PU along the isobar (the root in T of x on the feed's stable root, else the root in β between the bubble and dew points at p); DP, DH, DS and DU along the isochore (else Newton in (ln T, β) from the isochoric split); HT, ST and TU along the isotherm, cut at its turns on core's grid in ln ρ, each root kept where it is stable and the dome's root in β where one is not, several `Ambiguous` under `Strict` and `Stable`; HS and SU along the isentrope, the root in ln p of x over PS; QS, HQ and DQ along the line of quality Q, molar or by mass, `Ambiguous` keyed by T; each answer within core's acceptance gate (D6), a single-phase hint skipping the stability test; `Capabilities` every pair (ROT-293); the 19-pair capability matrix on R410A.mix, Air.mix and a five-component GERG-2008 gas within `Flash`, phase labels included (ROT-009); bd:ft05's near-dew HP, PS and PU band (ROT-184); R32/R125's mass quality through QT, PQ, QS, HQ and DQ (ROT-201). An isotherm's two-phase root above the mixture's critical temperature (in the retrograde band) is not searched |
| M13.12 | `Mixture::envelope(&EnvelopeOptions)`: Michelsen's continuation in (ln K, ln T, ln p) at a fixed β as GERG TM15 §7.7.1 describes it (map 04 §3.8 cites the formulation), each point specifying the unknown that varies fastest, from the first-order prediction along dX/dS (J dX/dS = e), the step adapted to the Newton iterations, every point on M13.9's gate; from the bubble point at the stated pressure (1 bar), else the dew point there, else the bubble or dew point at the model's T_min; through the critical point with β fixed, the denser phase swapped where a step crosses ln K = 0 of the most volatile component with the phases within 0.5 in ln ρ (an azeotrope keeps its liquid), refined to 0.002 in ln K either side and interpolated there by cubic Hermite (TM15 Eq. 7.134); ending typed (ROT-188): closed at the starting pressure, `Bound` at the model's T range or the stated (else the model's) p_max, or `NoConvergence`; points named by side (bulk and incipient phase) in a struct of arrays, a value no flash reads (ROT-189). The ten binaries at three compositions, R407C, R410A and a GERG-2008 gas: every point a bubble or dew point by M13.9's gate; M13.7's critical point on the trace within TM15's stated 0.02 K and 2 kPa (measured 0.42 mK and 27 Pa); CO₂/water, methane/n-decane, hydrogen/methane and N₂/C1/C2/C3 end typed within 100 MPa and the T range, where v8.0.0 runs to 23 GPa. Oracle kind `mix_env` (`build_phase_envelope`, 2058 points) within DIV-0060's 5e-7, seeded point by point; DIV-0063 (v8.0.0's near-critical points, its Newton stopped on rms 1e-7, up to 1.2e-2 off in a density) skipped |
| M13.13 | `with_mixtures(&registry, data_set)`, a registry resolver for CoolProp's names: `[HEOS:: \| GERG2008::]A[x]&B[y]...`, each name through the registry's names and aliases and each x a plain decimal, nothing read leniently (ROT-166), the fractions validated and zeros trimmed as M13.4's (a name that trims to one component is that fluid), and `[HEOS::]name.mix`; the pseudo-pure names keep their behaviour (user decision 4). `predefined(registry, name, data_set)`: `Parity` v8.0.0's mole fractions, `Corrected` those ASHRAE 34's mass fractions give with the registry's molar masses (ROT-199; DIV-0064 `UsePaper`, R411B 8.7e-7 off); `mix_alphar` and `mix_eos` on the 116 predefined mixtures v8.0.0 builds, the 31 it cannot refused by both (ROT-200). compat's `fluid_in` hands a name the registry does not hold, whole, to its resolvers and returns an owned `Fluid`; it still refuses `?`, `%` and padding. A missing pair is `UnknownName("binary pair Ammonia&Water")` (ROT-292; core's `NoModel` names a property and the core is frozen, MX2). `MultiFluid::binary` and `with_binary`: an override builds a new model (ROT-196). `facts/mixtures.csv`: v8.0.0's `PropsSI` on `HEOS::R32[0.697615]&R125[0.302385]`, `R410A.mix` and `R407C.mix` |
| M13.14 | phasekit-capi and phasekit-wasm depend on phasekit-mix (MX5): `pk_registry_new`'s registries, the shim's and the browser package's read mixture names (`with_mixtures` on their data set), so `pk_*`, the shim's `PropsSI` and the package's `propsSI` and fluid handles take `HEOS::A[x]&B[y]` and `.mix` names; `get_global_param_string("predefined_mixtures")` and `("mixture_binary_pairs_list")` are v8.0.0's strings (294 names, 888 pairs; `facts/mixtures.csv`'s item counts and sha256 digests). The resolver keeps the models of the last 64 names (user decision MX9, D8's exception): a repeated `PropsSI` at (T, p) on `R410A.mix` takes 0.77 ms (v8.0.0 4.4 ms; 73 ms a call without it). The `.wasm` budgets at size + 10 % (as SL8), 1 913 287 and 2 052 608 B (phasekit-mix adds about 550 kB); the crate notices name LICENSE-THIRD-PARTY's credit to NIST's AGA8 code. ROT-150: `gates abi` holds every `extern "C"` function to running its body under a catch, and the entry-point fuzz test calls every `pk_*` and shim function, with mixture names from the grammar's edges; panic injection and the browser package's own panics are issue #219 |
| M13.15 | `tests/mixture_family.rs`: the conformance kit on R410A.mix, Air.mix, a five-component GERG-2008 natural gas and the ternary `HEOS::R32[0.4]&R125[0.2]&R134a[0.4]`, each read by name through `with_mixtures`, in both datasets: the 19 × 2 capability matrix within `Flash` (M13.11's, moved from `tests/mixture_pairs.rs`); `gauge_invariance` at both truths; each mixture at x = [1, 0, ...] its first component's pure fluid under IIR, ASHRAE and NBP bit for bit, or refusing alike where the anchor lies above the critical temperature (Nitrogen's and Methane's IIR and ASHRAE); `Reference`, `Parallel` and `Simd` bitwise on DT, PT, QT and PQ batches; the kit's new `thread_invariance` (16 threads released together into one fresh fluid's first use, so into its one critical-point search, against a lone fluid's bits), and 16 threads racing the resolver's cache; `fd_first_order`; `entropy_identity`, its dense state at 0.98 T_max where the components' range ends below 1.3 T_c (R32's 435 K). `benches/mixture.rs` gains `mixture_alphar` (R32/R125 and the gas), `mixture_pt` and `mixture_qt`, recorded on the i7-8700K (`benches/results/M13-intel-core-i7-8700k.csv`): α^r 1.56 µs on R32/R125 and 16.2 µs on the gas; on R32/R125 at 250 K, DT 1.94 µs with the phase hint, 0.60 ms without it and 6.9 ms in the dome, PT 0.66 ms in one phase and 4.2 ms in the dome, QT 0.48 ms from 230 to 270 K |
| M13.16 | M13 closed: `MILESTONE` = 14, every M13 proof due and present (DIV-0013, DIV-0050 to DIV-0064) and every M13 ROT row ticked; `crates/phasekit-core/` unchanged since `m13-start`, and `ci/core-frozen.txt` names no tag; ARCHITECTURE.md's crate table, §11 and §12, VERIFICATION.md §10, MIGRATION.md (mixture strings; the shim's two lists, v8.0.0's since M13.14) and the README name the crate and what it does; on the i7-8700K, in one boot, the C++ baseline re-run and every bench re-recorded (`benches/results/M13-intel-core-i7-8700k.csv`), and `gates perf --targets` finds 27 of §7's medians met and its one miss logged (#171) |

**Exit gate.** G1-G8; `gates core-frozen` since `m13-start`; the conformance kit on multi-fluid and GERG-2008 mixtures;
a composition that trims to one component is the pure fluid under any gauge (map 04 §8, map 15 §8); Bell et al. 2023
Table XIII; the AGA8 GERG-2008 check state and teqp's GERG vectors; DIV-0013 resolved (`UsePaper`); equal fugacities
and mass balance at every two-phase state. **ROT rows.** ROT-199, ROT-200 (M13.2); ROT-195, ROT-207 (M13.3); ROT-062,
ROT-099, ROT-103, ROT-192, ROT-193, ROT-197, ROT-208 (M13.4); ROT-083, ROT-198, ROT-203 to ROT-206 (M13.5); ROT-191,
ROT-210 (M13.6); ROT-187 (M13.7); ROT-009, ROT-181, ROT-183, ROT-186 (M13.8); ROT-185, ROT-194 (M13.9); ROT-182,
ROT-190 (M13.10); ROT-184, ROT-201, ROT-293 (M13.11); ROT-188, ROT-189 (M13.12); ROT-196, ROT-292 (M13.13). **User
decisions implemented.** MX1-MX7; 4 (the `.mix` variants).

### M14 Ice and IF97 water

**Goal.** What humid air takes from IAPWS, as a family crate, `phasekit-iapws`, on core's public seams (D3, D10;
ARCHITECTURE.md §11): ice Ih (IAPWS R10-06(2009)) with its sublimation curve (R14-08(2011)), and IF97's regions 1, 2
and 4 below 623.15 K (R7-97(2012)) as one `Fluid`, liquid, vapour and saturation (user decisions HA1, HA4). Neither is
named in compat's strings: they are Rust functions, `if97()` and `ice(water, data_set)`, reached from strings through
humid air (HA5). IF97's region 2 above 623.15 K, regions 3 and 5, its backward equations and the name `IF97::Water` are
M27's. **Entry.** The user's word, 2026-10-20, as M18's prerequisite (HA1). The steps merge in order when green, in
three PRs, a commit and a `Plan-Step:` footer each (as LM7 bundled M25's): M14.1 alone, since it changes the core;
M14.2 and M14.3; M14.4 and M14.5. **Prerequisites.** User decisions HA1, HA4-HA6; M14.1's core seam, after which
`m14-start` is tagged and the core stays frozen to M18's close (HA6). **Arbiters**, all free from IAPWS (attribution
required, so the crate's NOTICE carries it):
- R7-97(2012) Table 5 (region 1: v, h, u, s, c_p and w at three states, 18 values), Table 15 (region 2, the same at
  three states, 18), Tables 35 and 36 (p_s at 300, 500 and 600 K; T_s at 0.1, 1 and 10 MPa), nine digits.
- R10-06(2009) Table 6: 16 quantities (g and its five derivatives, h, f, u, s, c_p, ρ, α, β, κ_T, κ_s) at the triple
  point, the normal melting point and (100 K, 100 MPa), 48 values, mostly twelve digits.
- R14-08(2011) Table 3's sublimation row, p = 8.94735 × 10⁻⁶ MPa at 230 K (transcribed at M8.2).

The oracle is v8.0.0's `IF97::Water` backend, through `PropsSI`, for IF97 only: v8.0.0's ice (`src/Ice.cpp`) is reached
only through humid air, so the ice model has the tables, the identities and its consistency with IAPWS-95's vapour.
M14 closes by setting `MILESTONE` to 15.

| Step | Landed |
|---|---|
| M14.1 | Core's seam for humid air (user decision HA6): `Virials` gains `d2b_dt2` and `d2c_dt2`. Each residual block's zero-density series gives `[a_k, τa_k′, τ²a_k″]` (power: τ²f″ = t(t − 1)f; τ-exponential: f(s² − s − m²y) with s = t − my, y = aτ^m; Gaussian and GaoB through `TauSide::series`'s τ ds/dτ; non-analytic from its `Jet4` at δ = 0), and `HelmholtzEos::zero_density` takes T²d²/dT² = τ²d²/dτ² + 2τd/dτ; `phasekit-cubic`'s through a″(T) from α's `Jet4` (d²(a/T)/dT²), `phasekit-mix`'s multi-fluid and GERG-2008 pure fluids through their departure jets at order 4 and d²τ/dT² = 2T_r/T³, the out-of-tree van der Waals by hand. `tests/virials.rs`: for all 136 fluids at 0.8, 1 and 1.5 T_r both second derivatives equal the δ → 0 extrapolation of α^r's A21 and A22 (`Identity`; the extrapolation now three-point, O(δ³), since the two-point one left 3.6e-12 of Ethane's d²B/dT² at δ = 1e-7, shrinking as δ²), and against central differences of the exact dB/dT and dC/dT (`Fd`) for the 136, the vdW, SRK and PR fluids of n-Propane, Water and CO₂, and four mixtures; each block's series against its own A20 at small δ |
| M14.2 | `crates/phasekit-iapws` on `phasekit-core` alone (T1f, `gates deps`; a hardened root of `gates lints`), PR scope `iapws`, its NOTICE the IAPWS attribution the releases ask for. R7-97(2012) Tables 5, 35 and 36 transcribed and double-entered (`paper/Water/IAPWS-R7-97-2012.{5,35,36}.csv`; no errata listed), the records `IF97 water` `Gibbs` (a new `ArbiterPart`) and `Saturation`, `SelfConsistent`; each later step transcribes the tables it reads. `if97::region1(t, p)`: Eq. (7)'s γ and its five derivatives from one pass over Table 2's 34 terms (ROT-243), g = RTγ with IF97's R (Eq. (1)) and IAPWS-95's molar mass, through `bundle_from_gibbs` and `State::from_total`, liquid below p_c and supercritical liquid above, 273.15 to 623.15 K and to 100 MPa; `saturation_pressure` (Eq. (30), 273.15 K to T_c) and `saturation_temperature` (Eq. (31), from p_s(273.15 K) = 611.2127 Pa, which the release rounds to 611.213, to p_c), each inverting the other to 1e-11. Table 5's 18 values and Tables 35 and 36's six within their printed digits; at the triple point u = −1.3e-7 J/kg and h = 0.611783 J/kg as Eq. (10) prints, but s = −6.16e-5 J/(kg K), not Eq. (8)'s 0, which v8.0.0's `IF97::Water` gives too. Oracle kind `if97` (`all/if97.csv`, 122 rows): v8.0.0's `IF97::Water` at PT on 11 temperatures by 8 pressures and QT and PQ along the line; its 58 region-1 rows within `Prop` with §5's floors (h near 0 °C is −41 J/kg), its 32 saturation rows' p or T within `Prop`; it refuses QT at 273.15 K, below its line (`tests/iapws.rs`) |
| M14.3 | Region 2 (Eqs. (15)-(17), Tables 10 and 11, the exponents Table 11's text extraction drops read from the page) as `if97::region2(t, p)`, 273.15 to 1073.15 K, one pass over its 9 + 43 terms. `if97::water()`, IF97 water as a `Fluid` (HA4; named in no compat string, HA5): PT in region 1 above Eq. (30)'s saturation pressure and region 2 below, down to any p > 0 (ROT-242's region-2 half; v8.0.0 refuses below 611.213 Pa), `Ambiguous` between the two sides on the line itself unless a `RootPolicy` picks one; QT and PQ at any Q through `State::from_split` of the two regions' states; 273.15 to 623.15 K and 100 MPa, every other pair `Unsupported`. R7-97(2012) Table 15 double-entered, its 18 values within their printed digits, two of its states at 700 K through the region's function. The `if97` kind through the `Fluid`: 120 rows' T, p, ρ, h, s, u, c_p and w within `Prop` with §5's floors, the saturated sides' c_p and w the phase's; DIV-0065 (`SkipOracle`): at the saturated liquid at 623.15 K v8.0.0's w is 2.8e-12 off Eq. (7)'s, which `scripts/math/if97.py` evaluates in 50 digits (576.91493103792562 m/s), and phasekit's 5.3e-13, so those two cells are held to 5e-12. The conformance kit: the capability matrix, the first-order identities at 12 states, gauge invariance, eight-thread invariance, and PT, QT and PQ batches bitwise under `Parallel` and `Simd`. Bench `if97_pt` (ROT-243): PT + h + c_p about 690 ns a state in region 1 and 790 ns in region 2 on the i7-8700K under load, beside v8.0.0's 750 ns `fast_evaluate`; recorded at M14's close |
| M14.4 | `ice::ice(water, data_set)`, ice Ih as a `Fluid` (named in no compat string, HA5): R10-06(2009) Eq. (1) with Table 2's coefficients, its complex t₁, r₁, t₂ and r₂ in a private complex type (the principal logarithm, §3), g and its five derivatives specific then times the borrowed water's molar mass, through `bundle_from_gibbs` with the water's gas constant (shared with the vapour side of M14.5's sublimation states) and `State::from_total`, every state `Phase::Solid`; PT from 0 K, exclusive (`Limits` states `f64::MIN_POSITIVE`, needing a positive T_min), to 273.16 K and 210 MPa (§5), every other pair `Unsupported` until M14.5. g₀₀ by data set (DIV-0066, `UsePaper`; ROT-247): `Corrected` the 2009 revision's, `Parity` v8.0.0's 2006 value, 1.13611e-4 J/kg lower in g, h, u and f and in nothing else (s within an ulp, since the state's s comes from bundle terms that both carry g). R10-06(2009) Table 6 double-entered (`paper/Water/IAPWS-R10-06-2009.6.csv`, the record `Ice Ih` `Gibbs`, `SelfConsistent`): its 48 values within their printed digits from the `State` alone (g's derivatives through ρ, s, κ_T, α and c_p; β = α/κ_T; κ_s = 1/(ρw²)), f at the triple point, −0.055446875 J/kg, among them. ROT-248: the kit's first-order identities at 12 ice states from 50 K to the triple point and to 200 MPa, where v8.0.0's ice lacks g_TT and g_Tp; no oracle: v8.0.0 reaches its ice only through humid air |
| M14.5 | Ice's sublimation states: R14-08(2011) Eq. (6) (`ice::sublimation_pressure`, 50 K to the triple point) and its inverse by Newton in θ (`sublimation_temperature`); QT and PQ through `State::from_split`, the solid the denser phase, the vapour the borrowed water's at (T, p_subl) by core's `density::solve` and its model's α⁰ + α^r, on the ice model's key and gas constant (`ice::model` for a registry). Table 3's row, 8.94735e-6 MPa at 230 K, within its printed digits; at 15 temperatures from 130 K to the triple point, where ice's and IAPWS-95 vapour's Gibbs energies are equal lies within Eq. (7a)'s uncertainty of Eq. (6)'s pressure, the largest gap 0.33 of it; `tests/gibbs_seam.rs`'s registry, compat, batch (PT and QT, `Simd` bitwise) and reference-state checks on the real ice. M14 closed: `MILESTONE` = 15, DIV-0065's and DIV-0066's proofs due and present, ROT-246, -242 (region 2), -243, -247, -248, -275 (regions 1, 2, 4) and -290 ticked; the core unchanged since `m14-start`, which `ci/core-frozen.txt` keeps naming through M18 (HA6); ARCHITECTURE.md's crate table and §11 and the README name the crate; the bench recorded |

**Exit gate.** G1-G8 at every step; `gates core-frozen` since `m14-start`; the IAPWS tables within their printed
digits; the `if97` oracle within `Prop`; the conformance kit on IF97 water; `gibbs_seam`'s checks on the real ice
model. **ROT rows.** M14.2: ROT-246. M14.3: ROT-242 (region 2's half), ROT-243. M14.4: ROT-247, ROT-248. M14.5:
ROT-290, ROT-275 (its IF97 part). **User decisions implemented.** HA1, HA4, HA5, HA6.

### M18 Humid air

**Goal.** RP-1485's humid air (Herrmann, Kretzschmar and Gatley 2009; map 07 §3.3), a virial mixture of dry air and
water vapour, as `phasekit-humidair` on core's public API alone (T1f), taking its four parts as `Fluid`s at
construction (map 07 H3, ROT-263): IAPWS-95 water (its virials and ideal gas), Lemmon's dry air, M14's IF97 water (the
saturation line and the liquid above 0 °C) and M14's ice (the sublimation curve and the solid below). `HumidAir` is
its own type, not a `ThermoModel`: a state takes three inputs, one of them a humidity, which no `Pair` names, so
ARCHITECTURE.md §11's row says so. It reaches compat's `HAPropsSI`, the `pk_*` ABI and the shim's real `HAPropsSI`
(D11), the browser package, the transport outputs μ and k (user decision HA3), and in `phasekit-diagram` the
psychrometric chart's geometry, in ASHRAE's form and Mollier's, and ice on Water's diagrams (HA7), drawn by the site at
M21.11 and M21.12. **Entry.** M14 closed (HA1). The steps merge in order when green, in eight PRs, a commit and a
`Plan-Step:` footer each: M18.1 and M18.2; M18.3; M18.4; M18.5 and M18.6; M18.7; M18.8; M18.9 and M18.10; M18.11 and
M18.12. **Prerequisites.** User decisions HA1-HA7; M14's models; core frozen since `m14-start` to M18.10; M28.1, after
M18.10 lands, then `m18-start` tagged at its merge and the core frozen from it to M18.12 (user decision EM5).
**Arbiters** (HA2), free and transcribed, double-entered, under `paper/HumidAir/`:
- The authors' tables (Herrmann, Kretzschmar and Gatley, *Tables of property values for moist air*, the supplement to
  HVAC&R Research 15(5) 961-986, 2009, on their faculty's site): Tables 1 and 2, saturated moist air at 101.325 kPa
  from −60 to 90 °C (W_s, v_s, h_s, s_s); Tables 5-14, at 200 and 320 °C and 0.101325 to 10 MPa (W, t_wb, v, h, s,
  and φ at 200 °C from 2 MPa). 570 values in the SI tables, within their printed digits (`Paper`), but for DIV-0071's
  (user decision HA8).
- IAPWS G11-15 Table 7, the seven virials at 200, 300 and 400 K (21 values): B_aa, C_aaa, B_ww and C_www are the exact
  virials of Lemmon's air and IAPWS-95, which RP-1485 takes "according to Feistel et al." (ASHRAE's LibHuAirProp
  guide §1.3); IAPWS G8-10 Table 15, B_aw, C_aaw and C_aww with their first two T-derivatives (27 values).
- IAPWS G7-04 Table 6, Henry's constants of N₂, O₂ and Ar at 300 to 600 K (12 values; Ar at 600 K is outside its fit).

The full RP-1485 report (its equations and the appendix tables of virials, f, κ_T and Henry's constant), Nelson and
Sauer (2002), whose C_aaw and C_aww RP-1485 takes, and Tsilingiris (2008), the transport's mixing rule, are not in hand
(section 6). Where an equation is the report's, it is taken as v8.0.0's `HumidAirProp.cpp` writes it, and the authors'
tables arbitrate the result. ASHRAE's LibHuAirProp guide is cited as evidence of which formulation each part takes; its
licence is personal use, so none of its numbers is committed. The oracle is v8.0.0's `HAPropsSI` and `HAProps_Aux`
under `Parity`, which keeps v8.0.0's choices (user decision 2): the IAPWS-95 saturation pressure in f, IAPWS-95's κ_T
at IF97's density (map 07 H3), IF97's p_ws in Henry's constant, IAPWS-95's ideal gas with v8.0.0's offsets (map 15
§2.1), the 2006 ice g₀₀. `Corrected` follows RP-1485 as ASHRAE's guide describes it, and the authors' tables decide:
v8.0.0's h_s at 25 °C and 101.325 kPa is 76.5045 kJ/kg, Table 2's 76.503. M18 does not move `MILESTONE`: M15 stays
the first open milestone.

| Step | Landed |
|---|---|
| M18.1 | `crates/phasekit-humidair` on `phasekit-core` alone (T1f, `gates deps`; a hardened root of `gates lints`), PR scope `humidair`. `HumidAir::new(Parts { water, air, liquid, ice }, data_set)`: water and air need exact virials, the liquid and ice water's molar mass and a saturation state each (QT at 300 K and 250 K), each refusal the part's own (ROT-267); `Send + Sync`, parts by value, no thread-local copy (ROT-268). `constants`: R = 8.314472, M_a = 28.966 g/mol, M_w = 18.015268 g/mol, Lemmon's N₂, O₂ and Ar fractions, each with its source (ROT-264). `virials(t)`: the seven with two T-derivatives each, B_aa, C_aaa, B_ww and C_www the parts' exact virials (M14.1's seam), B_aw and C_aaw G8-10's forms, C_aww v8.0.0's digits under DIV-0067 (`KeepOracle`, 1.3e-4; ROT-271: RP-1485 cites Nelson and Sauer for it). IAPWS G11-15 Table 7 and G8-10 Table 15 double-entered, every value within its printed digits but C_aww's, within 1.06e-4 (C_aww) to 1.22e-4 (its second derivative) at 200 K; G8-10's C_aww rows are its own Eq. (11). Oracle kind `humidair_aux` (51 rows, `HAProps_Aux` at 200 to 600 K and three pressures): the cross virials within `Prop`, B_aa and B_ww within DIV-0011's bound, C_aaa and C_www DIV-0011's (its C_www at 200 K is −5.93e-4 m⁶/mol², G11-15's −2.64e-4) |
| M18.2 | `saturation(t, p)`: water's saturation pressure over the condensed phase, the liquid part's at T ≥ 273.16 K and ice's below (one predicate, ROT-263); the condensed phase's κ_T and molar volume from its part's state at (T, p) (ROT-261), IF97's inside its saturation band (ROT-270); Henry's constant (`henry_constant`, IAPWS G7-04 Eq. (3) over Eq. (4)); f by the secant on Eq. (3.25) as v8.0.0 writes it, to 4 ulp and at least 1, `NoConvergence` after 50 (ROT-265). `Corrected` takes RP-1485's parts (IF97's p_ws, liquid volume and κ_T, G7-04's Henry with Lemmon's fractions, R = 8.314472), `Parity` v8.0.0's (IAPWS-95's p_ws and volume in f, IAPWS-95's κ_T at IF97's density, IF97's p_ws in Henry with 0.7812, 0.2095 and 0.0093, R = 8.314371 in f). `Humidity` (W, ψ_w, φ, the dew or frost point), `mole_fraction`, `relative_humidity`, `humidity_ratio`, `saturated_humidity_ratio`. G7-04 Table 6's N₂, O₂ and Ar rows and the authors' Tables 1 and 2 double-entered: the 12 Henry's constants and W_s at all 31 temperatures from −60 to 90 °C at 101.325 kPa within their printed digits under `Corrected`. `humidair_aux` under `Parity`: p_ws, v̄_ws and β_H within `Prop`; f within DIV-0011's bound, extended to it (up to 9.8e-8 at 200 K and 5 MPa, from v8.0.0's B and C at δ = 1e-12) |
| M18.3 | `HumidAir::moist_air(t, p, psi)` → `MoistAir`, 130 to 623.15 K and 10 Pa to 10 MPa: the molar volume by Newton on the virial equation, `AboveMaxPressure` where it has no root (at 40 °C and W = 0.1 its pressure peaks at 7.86 MPa, where v8.0.0's secant returns its 100th iterate); h, s, u and v per kilogram of dry air, per kilogram of humid air or per mole (`Per`); c_p, c_v, w, Z and the isentropic exponent from B_m and C_m and their first two T-derivatives, no finite differences (ROT-269). Ideal parts (`ideal`): dry air's from the air part's α⁰ with Lemmon's R; water vapour's from IF97 region 2 at 1e-6 Pa above the triple point under `Corrected` (DIV-0070) and from IAPWS-95's α⁰ throughout under `Parity`, whose zeros give v8.0.0's legacy constants exactly. `Corrected`'s zeros: dry air's h and s at 273.15 K and 101.325 kPa, the liquid's u and s at the triple point, and s̄₀ = 0.02366427495 J/(mol K) on both parts, as the authors' tables add it. ε = M_w/M_a (DIV-0068). The authors' Tables 5-14 double-entered (446 values; the sessions agree). Under `Corrected`: every v of Tables 1, 2 and 5-14 and every s but Table 8's and one of Table 12's within their printed digits; h, Table 8's s and that row within DIV-0071's measured bounds (`Investigate`; user decision HA8, issue #275). The oracle kind `humidair` (`HAPropsSI` at (T, p, R) and (T, p, W), 356 rows): under `Parity` the 276 answered within DIV-0069's 5e-6 (v8.0.0's virials at δ = 1e-12 and its finite differences), 80 refused (R where p_ws > p); map 11 §8's H at 300 K, 1e5 Pa and R = 0, 27013.112479771713, 5.1e-11 off |
| M18.4 | `HumidAir::state(p, a, b)` with `Given` (T, a humidity, h, s and v per kilogram of dry air, T_wb), in either order: T and a humidity directly; T and h, s, v or T_wb by a bracketed solve in ψ_w from dry air to saturation; a humidity and h, s, v or T_wb by one in T; W, ψ_w or T_dp with φ (v8.0.0's #2670) but for dry air; every other pair `InvalidState`. A solve's range narrows to where the virial equation has a state and the target a value (at 10 MPa its branch ends at low T and high ψ_w). `dew_point(p, ψ_w)`: the liquid's root where one exists, else the ice's (f jumps at the triple point), bracketed from the saturation temperature at p_w (ROT-266) and closed by the Illinois method; dry air refused (ROT-272, DIV-0072). `wet_bulb(state)`: adiabatic saturation, the condensed phase's h from its part (IF97's liquid and ice under `Corrected`, IAPWS-95 at IF97's density under `Parity`), the liquid's root where one exists, else the ice's; a T_wb in the triple point's gap between them refused as an input. `HumidAir::new` keeps every part in its model's own gauge (ROT-104), which also fixes M18.3's IF97 vapour read through the caller's gauge; under `Parity`, κ_T is IF97's inside IAPWS-95's dome. Under `Corrected`, the authors' Tables 5-14's t_wb (106) and φ (22) within their printed digits; 1996 pairs round-trip within `Flash`; every output is bit for bit the same under NBP, IIR and an arbitrary gauge. The `humidair` kind gains v8.0.0's T_wb, T_dp and R: 674 cells within DIV-0069's bound, and the states solved back from H, S, V and T_wb with T or with W within that bound carried through each target's slope |
| M18.5 | `HumidAir::viscosity` and `conductivity` (`transport`): Tsilingiris's mixing as v8.0.0 writes it, Wilke's factors from the parts' own μ and molar masses; dry air's μ and k from the air part at (T, p), water vapour's from the water part as saturated vapour at the total pressure (map 07 H2), in both data sets until the paper arrives (section 6; ROT-262 stays deferred). Provisional, oracle only: the `humidair` kind gains M and K; at a given W within `TransportDirect` (9.3e-15), at a given R within DIV-0011's bound, extended to them (ψ_w carries f's, 1.5e-8); below water's triple-point pressure, where v8.0.0's PQ flash runs IAPWS-95's curve past its end and core refuses the state, phasekit refuses them (DIV-0073, `SkipOracle`) |
| M18.6 | `phasekit_compat::ha_props_si_in(air, output, name1, value1, name2, value2, name3, value3)`, `ha_props_si_vec_in` (broadcast, `FillPolicy`, `Shape`), `ha_props_si` over the embedded registry under `Corrected` (its humid air built once), `humid_air_in(registry, data_set)` (the registry's Water and Air with `phasekit-iapws`'s IF97 water and ice) and `ha_names`: v8.0.0's 52 names and aliases, byte for byte. The pressure and two of T, W, ψ_w, φ, T_dp, T_wb, h, s and v (per kilogram of dry air) as `HumidAir::state` takes them; v8.0.0's range as its `check_bounds` (T, T_dp and T_wb from 130 K to 350 °C, W to 10, ψ_w to 0.94145, also on the solved state); every output from the state, none echoed (DIV-0074, `SkipOracle`; map 07 H10); the dry-air dew point is M18.4's DIV-0072. `gates features` lets `phasekit-iapws` and `phasekit-humidair` into compat's and the browser package's trees. Every column of the `humidair` rows through the strings is the direct API's bit for bit (276 × 23); every alias gives its key's value, as an output and, where `HumidAir::state` takes it, as an input; MIGRATION.md's Humid air section |
| M18.7 | `pk_humid_air` handles: `pk_humid_air_new(registry, out)` over a registry's Water and Air under the data set the registry was made with (the registry table now keeps it), `pk_humid_air_free`, and `pk_ha_props_si(humid_air, output, name1, value1, name2, value2, name3, value3, out)`, compat's `ha_props_si_in`, under the FP guard and the panic catch as every export; the committed header regenerated (`gates abi`: 14 `pk_*` symbols). The shim's `HAPropsSI` is real, over the embedded registry under `Corrected`, its `errstring` the typed error's message; the stub's message and test are gone (D11). The C smoke test (Linux and Windows) reaches map 11's H through `pk_ha_props_si` on the Parity registry and its refusal of φ = 1.5; the shim's test holds `HAPropsSI` to compat's bit for bit; the entry points' proptest calls the three exports; MIGRATION.md's `HAPropsSI` paragraph |
| M18.8 | The browser package: `Registry.humidAir()` over the registry's Water and Air (the package does not embed Air; a page adds its pack with `withPack`, and without it the call is refused) and `phasekit-iapws`'s IF97 water and ice, under `Corrected`; `HumidAir.HAPropsSI(...)`, compat's `ha_props_si_in`; `HumidAir.state(p, name1, value1, name2, value2)` (compat's new `ha_state_in`) and `HumidAirState`'s `T`, `P`, `W` and `get(output)` for every `HAPropsSI` output (`ha_output_at`); `humidAirKeys()`. Under Node on both builds: `HAPropsSI` through the JS bindings and every state output are compat's over the same fluids bit for bit; Tables 7 and 12's t_wb and Table 7's φ by `HAPropsSI` within their printed digits, and each row's state back from its h and W through `state` within `Flash`. `gates features`: the builds are 2268213 and 2229861 bytes, 99081 and 94235 above M19.6's, within their budgets, which are unchanged (SL8) |
| M18.9 | `phasekit-diagram`'s psychrometric chart (`psychrometric(batch, range)`): dry-bulb T across, W up, over a `ChartRange` (by default 0 to 50 °C and W to 0.03); the saturation line and φ at its tenths swept in T from the range's ticks in °C, and h, v and T_wb at 1-2-5 values over what the range's corners span swept in W from dry air up, each refined as M19.3's lines, a vertex outside the range a gap whose edge is closed in on. The crate keeps to core alone (DG3's rule): it samples through `HumidAirBatch` (W at T, or T at W, holding a line's quantity, in batches), which the browser package fills from `phasekit-humidair` and the crate's tests through a dev-dependency. The browser package's `HumidAir.psychrometric(p, t_min?, t_max?, w_max?)` returns the lines as typed arrays (`Chart`). Every vertex is humid air's at its inputs bit for bit (330 on the default chart); where the saturation line passes a temperature of Tables 1 and 2 its W is the table's within its printed digits (26 on the default chart and one from −60 to 90 °C); under Node on both builds, every vertex of a chart at 15 to 25 °C and W to 0.015 is `HAPropsSI`'s bit for bit (310) |
| M18.10 | Mollier's h–x chart (`mollier(batch, range)`), M18.9's in its European form over the same `ChartRange` and through the same seam, which gains `enthalpies` (h at each (T, W)): W across and h up on skewed axes, each vertex (`MollierVertex`) carrying T, W, h and its drawn height y = h − r₀W. r₀ is water vapour's h at 0 °C as the model has it, the slope of its 0 °C isotherm from dry air to saturation (2499.88 kJ/kg at 1 atm under `Corrected`; the textbooks' ideal mixture has 2501), so that isotherm lies level, its bow 0.45 J/kg. Its lines: the saturation line and φ at its tenths swept in T; isotherms (`Held::DryBulb`) at the range's ticks in °C from dry air to saturation, where each ends on the saturation line's vertex; h and v at 1-2-5 values swept in W; each refined in the drawn plane as M18.9's. The fog region below the saturation line is left empty. The browser package's `HumidAir.mollier(p, t_min?, t_max?, w_max?)` returns a `Chart` with `H`, `Y` and `r0` besides M18.9's arrays. Every vertex is humid air's at its inputs and drawn at h − r₀W bit for bit (379 on the default chart); where M18.9's saturation line and Mollier's share a T (16), their W is the same; under Node on both builds, every vertex of a chart at 15 to 25 °C and W to 0.015 is `HAPropsSI`'s bit for bit (346) |

- **M18.11 Ice on Water's diagrams (HA7).** M14's ice and its sublimation curve on M19's planes, passed to
  `phasekit-diagram` as data, as a mixture's envelope is (DG3's rule): along the sublimation line and ice Ih's melting
  line, each point's two sides (T, p, h, s and ρ), filled by the caller from `phasekit-iapws` and Water (the browser
  package, through the dependency M18.8 gives it; the crate's tests, a dev-dependency). With them a plane's range
  reaches down to a temperature the caller sets (by default 200 K, where the sublimation pressure is 0.16 Pa) and its
  sublimation pressure; p–T draws the sublimation and melting lines meeting the saturation line at the triple point;
  the other planes draw the solid–vapour and solid–liquid two-phase areas; M19.5's regions gain the solid, edged by
  the two lines, the isobar of the ice Ih–III–liquid triple point (208.566 MPa, R14-08's end of ice Ih's melting
  curve, since no other ice is modelled) and the range's lowest temperature, and the two areas. The browser package's
  `Fluid.diagram` and `Fluid.regions` of Water carry them. *Red:* every vertex is a state of the data passed
  (`Exact`); the three lines end at the triple point, their gaps there measured and recorded in the test; inside the
  solid region, clear of its edges, ice is the phase whose Gibbs energy is lowest of ice, IAPWS-95's liquid and its
  vapour; the Node tests on both builds.
- **M18.12 M18 closed.** The core unchanged since `m18-start`, and `ci/core-frozen.txt` names no tag; every M18 proof
  due and present and every M18 ROT row ticked; ARCHITECTURE.md's crate table, §11 and §12, VERIFICATION.md §3.5 and
  §10, MIGRATION.md and the README name humid air; the M18 benches recorded (section 5.1) beside map 07's v8.0.0 costs
  (H(T, R) 7.5 µs, T_dp 132 µs, T_wb 152 µs, T(H, R) 203 µs); tag `m18` with the user's approval. *Red:* `gates rot`
  and `tests/divergences.rs` with M18's rows and entries due.

**Exit gate.** G1-G9 at every step; `gates core-frozen` since `m14-start` to M18.10, since `m18-start` after; the
authors' tables, G7-04, G8-10 and G11-15 within their printed digits under `Corrected`; the oracle under `Parity`, each
difference a register entry; `gauge_invariance` over every output; the C smoke test on Linux and Windows; the browser
package on both builds within its budgets; every chart and ice vertex a state of its model (`Exact`), Mollier's drawn as
its stated skew. **ROT rows.** M18.1: ROT-264, ROT-267, ROT-268, ROT-271. M18.2: ROT-261, ROT-263, ROT-265, ROT-270.
M18.3: ROT-269. M18.4: ROT-104, ROT-266, ROT-272. M18.5: ROT-262 (deferred to the paper). M18.6: ROT-275 (its humid-air
part). **User decisions implemented.** HA1-HA7, D11's real `HAPropsSI`.

### M19 Diagram geometry

**Goal.** The geometry of the five diagrams, computed by the library so that the site's page adds no numerics and any
other host (M16's Python with matplotlib) draws the same lines: saturation domes, isolines, a mixture's envelope and the
phase regions, as polylines and polygons whose every vertex is a state the library computed, in a new crate,
`phasekit-diagram` (user decisions WS4, DG1, DG2). It draws nothing. A line's label is its held quantity and value, a
region's its phase; where a label goes depends on the zoom and the font, so placing it stays in the site
(`site/src/diagram/model.ts`), as do the colours and the shown units. **Entry.** M13 closed (`m13`; user decision 13);
the user's word, 2026-10-20. The steps merge in order when green: M19.1 and M19.2 in one PR, M19.3 and M19.4 in another,
a commit and a `Plan-Step:` footer each, then M19.5 and M19.6 in a PR each (DG4). **Prerequisites.** User decisions WS4
and DG1-DG4; M13.12's envelope; the site's samplers, whose behaviour M19 takes over and whose tests it ports:
`site/src/sample/{diagrams,ranges,lines}.ts` (M21.2, M21.4), `envelope.ts` (M21.5) and `regions.ts` (M21.9 and #253).
M19 does not move `MILESTONE`.

The crate depends on `phasekit-core` alone and uses only its public API, never `phasekit_core::internal` (T1f; DG3): the
fluid's constants and limits (`Fluid::constant`, which `Props1SI` reads), the batch (`batch::evaluate`, under the
caller's `FlashOptions` and `ExecPolicy`) and the flash. A step that needs core to change stops and re-plans (section
0.4). Each diagram is one function of a `Fluid` and a plane, or of a fluid alone for the regions, which serve every
plane, and returns plain data: per line its held quantity and value and its segments, each one-phase or two-phase, each
vertex its x and y and the input pair and values it was computed at. **Arbiters.** M19 adds no thermodynamics, so its
arbiter is the library: a vertex is the batch's value at its inputs (`Exact`); flashed again from its plane's
coordinates, where they name one state (on p–T a two-phase state does not), it gives back its line's value within
`Flash`; where a line meets the dome, its vertex is the saturation state at the line's value, `Exact` against `QT`,
`PQ`, `QS`, `HQ` or `DQ`. The tests run on the five embedded fluids and the pseudo-pure Air and R410A (`fluids-all` in
the crate's dev-dependencies). The ranges, the isoline values and the refinement's rule are definitions, checked as they
are stated.

| Step | Landed |
|---|---|
| M19.1 | `phasekit-diagram` on core's public API alone (`gates deps`; a hardened root of `gates lints`): `Plane` (p–h, T–s, p–T, T–ρ and h–s: the axes, p and ρ logarithmic, the isoline families each draws), `Quantity`, `Limits::of` (the constants `Props1SI` reads), and `Batch`, the seam the geometry samples through, a fluid's `FluidBatch` under the caller's flash options and executor or a test's that fails cells, a cell a value where it is `Ok` or `Extrapolated`. The ranges: T from the greater of T_min and T_triple to the lesser of T_max and 1.6 T_c, p from the greater of p_triple and p_c/10⁴ to the lesser of P_max and 10 p_c; a family's values strictly inside, 1-2-5 per decade for p and ρ while that is at most 12 lines and decades past that, d3-array 3.2.4's `ticks(a, b, 8)` for the rest. The dome: 120 temperatures from the saturation T of the lowest plotted p, closing in on T_c, by the `QT` batch at Q = 0 and 1 (a pseudo-pure fluid's bubble and dew sides), and the critical point at (T_c, ρ_c), a failed cell left out; an axis of h, s or ρ spans the dome's and the range's corners' values. Every dome vertex of Water, n-Propane, CO₂, Nitrogen, R134a, Air and R410A on the five planes is the flash's at its inputs, bit for bit (`tests/dome.rs`) |
| M19.2 | The isolines: each family's lines at its values on M21.2's fixed grids, 160 points a sweep (linear in T, logarithmic in p), and the quality lines across the dome's 120 temperatures; a `Line` names its held quantity and value, its label, and runs in `Segment`s, one-phase or two-phase. An isotherm or isobar is one-phase before and after the dome and two-phase inside it, along `QT` or `PQ` at the tenths of Q, entered from the vapour (an isotherm, swept up in p) or the liquid (an isobar, swept up in T); an isentrope, isenthalp or isochore is swept by `PS`, `HT`, `DP` or `DT`, two-phase where the batch defines Q, its edges the states `QS`, `HQ` or `DQ` give with their sweep values. A failed cell ends a segment, a lone vertex draws nothing, and an edge vertex ends one segment and starts the next. Re-planned in the step: a crossing needs only its two ends, the saturation states at Q = 0 and 1 with their sweep values, and a quality failed between them is a gap inside the dome, so a pseudo-pure fluid's line, whose qualities between the library refuses (user decision 4), meets the dome at its dew and bubble states (R410A at 250 K: 354.07 and 355.31 kPa) where the site's sampler stopped a grid step short. On eight fluids and planes, every vertex is the flash's at its inputs, bit for bit, and flashed again from its plane's coordinates is that state within `Flash` in T, ρ and Q, wherever they name one state (Water's liquid near its density maximum has three at one s and T), but for a pseudo-pure fluid's two-phase states, which follow CoolProp's ancillary rules pair by pair (R410A's `DP` and `HP` states at one ρ, h and p differ by 2e-4 in ρ); p, which follows from T and ρ, is not compared (`tests/lines.rs`) |
| M19.3 | Sampled where a line bends: each piece of a line starts on 32 points of its sweep, an isotherm's or isobar's two-phase part on 32 qualities, and is refined in the plane's unit square (each axis scaled to its span, a logarithmic one in its logarithm): an interval is halved, at its midpoint or for p its geometric mean, while the midpoint's vertex lies farther than 10⁻³ from the chord, at most 6 times, one batch call a level; an interval with one failed end is halved to the same depth, so a gap's edge lies within 1/64 of a starting interval of where the library stops answering. The dome's sides are refined in the square of the spans the unrefined dome and the range's corners give, the lines in the square of the refined dome's. Re-planned in the step: the dome's sides and the quality lines start on the dome's 120 temperatures closing in on T_c, since 32 would stop 0.37 K short of Water's; and an isentrope's, isenthalp's or isochore's change of side of the dome is closed in on as a gap's edge is, since a pseudo-pure fluid's `DP` states turn one-phase short of its `DQ` dew state (user decision 4; Air at 2 kg/m³: 40.9 and 42.1 kPa) and the segments either side share no vertex (on Air's T–s they ended up to 21 64ths of a starting interval apart without it). The seven fluids' five planes take 52,033 vertices where M19.2's grids took 107,783; on eight fluids and planes every interval of a line or the dome is within the tolerance of its midpoint's flash, or at the depth, and every change of side is closed in on (`tests/refine.rs`); M19.2's tests hold, their cells failed by input value |
| M19.4 | A mixture's envelope: `Trace` (T, p, the feed's mass density and Q, 0 at a bubble point and 1 at a dew point, in the trace's order; the critical point it passed; the error that ended it) and `envelope(fluid, trace, plane)`, the trace passed as data so that the crate depends on core alone (DG3). On p–T and T–ρ the bubble side is the dome's liquid and the dew side its vapour, each in the trace's order where the trace starts on it and reversed otherwise, so that both run toward the critical point; the model's critical point, at (T_c, ρ_c) as `Fluid::constant` gives them, is drawn where the trace passed one and the model gives T_c, p_c and ρ_c finite; each axis spans the vertices, a logarithmic one widened by 2 each way, a linear one by a tenth of its span, or of its value where the span is 0. The other planes are `Unsupported` with their pair, a trace without a point is refused with the error that ended it, and columns of different lengths are a `Shape` error. On R410A.mix's trace from `phasekit-mix` (a dev-dependency only), every vertex is the trace's point bit for bit; each of its 51 vertices but the two within 1 mK of T_c is the `QT` and the `PQ` flash's state within `Flash` (issue #257; nearer T_c each side turns in T and in p, so a given T or p has two points within 1 Pa); the model's critical point lies within TM15's 0.02 K and 2 kPa of the trace's crossing (`tests/envelope.rs`); `envelope.ts`'s tests of drawn traces are the module's |
| M19.5 | The phase regions: `regions(fluid)` and `sample_regions(batch, phase, limits)`, the site's `regions.ts` in Rust (M21.9, #253), sampled once a fluid, each `Corner` carrying T, p, h, s and ρ so that every plane reads its axes from it; `FluidBatch::phase` is the library's phase (`State::phase`) at a state under the batch's flash options. The edges: the dome on 120 temperatures closing in on T_c; the critical isotherm, and the critical isobar closing in on T_c; the isotherm at T_max, the isobar at P_max and the isobar at the lesser of the range's lowest pressure and half the saturation pressure at the lowest T; and the cold edge, on each of 48 isobars either side of p_c the lowest temperature with a state, by 12 halvings, which follows the melting line where the library has one and below p_c stays under the bubble temperature (#253); every leg on M21.9's 160 points, unrefined. Each region's phase is the library's at a seed state inside it; a region whose phase the library refuses is left out, the edges join across a critical point it refuses, and nothing is drawn without a saturated liquid at the lowest T. On the five embedded fluids, inside each region and clear of its edges by 2 % of each plane's box (at the deepest point of a fine grid where a region is a sliver), the library's phase by the plane's own pair is the region's; the cold edge halves only the isobars the library refuses at the lowest T (Water's) and follows n-Propane's melting line; Air's cold edge stays on the liquid side of its dome; a leg onto the saturation curve stops a cell short of it; the critical lines run through the critical point where the library gives one, and break there where it refuses it, as it refuses n-Propane's (T_c, ρ_c) with core's `fast` maths (`MechanicallyUnstable`) (`tests/regions.rs`) |
| M19.6 | The browser package's diagrams, and M19 closed: `planes()` (each plane's id, axes' keys, which axis is logarithmic, and its families' held and swept keys, so that a page keeps no copy) and `pairKeys()` (each input pair's two keys, D, H and S per unit mass); `Fluid.diagram(plane)`, a pure fluid's dome and isolines by its batch in lanes (`ExecPolicy::Simd`), a mixture's phase envelope on `pt` and `trho` from its own trace (`Fluid.envelope()`'s) with how the trace ended, the other planes refused; and `Fluid.regions()`, a mixture's refused: as typed arrays, each `Vertices` list its `xy` and `inputs` (`Float64Array`, two a vertex) and `pairs` (`Uint8Array`, indices into `pairKeys()`), the `Lines` and the regions' `Corners` (T, P, Hmass, Smass and Dmass, five a corner) as flat arrays with `Uint32Array` offsets. `gates features` lets `phasekit-diagram` into the browser package's tree with no feature; the builds took 2,169,132 and 2,135,626 bytes, 216,686 and 198,654 above main's, and each budget is that + 10 % (SL8). Under Node on both builds, the planes are the crate's, Water's diagrams on the five planes and its regions are phasekit-diagram's bit for bit, computed in the test on the same build, every vertex of its p–h and h–s diagrams is `evaluate`'s x and y at its inputs, R410A.mix's p–T and T–ρ diagrams are its `Fluid.envelope()` bit for bit with the model's critical point, and the getters return typed arrays (`tests/node.rs`). Under Node 26 on the i7-8700K, the five embedded fluids' diagrams, summed, against main's page sampling them with its TypeScript over the batch, on the baseline build: p–h 217 → 121 ms, T–s 808 → 308, p–T 14 → 11, T–ρ 37 → 41, h–s 64 → 53, and the regions 82 → 67 (the `simd128` build within a few per cent of these). ROT-175's mechanism names the crate. Tag `m19` with the user's approval |

**Exit gate.** G1-G9 at every step; every vertex gives back its line's value within `Flash`; every dome crossing is the
saturation state at the line's value (`Exact` against `QT`, `PQ`, `QS`, `HQ` or `DQ`); a mixture's envelope `Exact`
against its trace; each region's phase the library's; `gates deps` holds the crate to core; the browser package's
budgets hold. M21.2's checks and the screenshots then run on the geometry at M21.6. **ROT rows.** None. **User decisions
implemented.** WS4, DG1-DG4.

### M20 Follow-ups

**Goal.** The four follow-ups PLAN.md §6 had left unplanned (user decision PL1). **Entry.** After M12 lands; the steps
are independent and may be reordered. M20.3 and M20.4 land in one PR, a commit and a `Plan-Step:` footer each (PL2).
M20 does not move `MILESTONE`.

| Step | Landed |
|---|---|
| M20.1 | `DerivVar` gains `Cvmolar`, `Cvmass`, `Cpmolar`, `Cpmass`, `SpeedOfSound` (`DerivVar::is_second_order`), `Tau` and `Delta` (user decision OD1). c_v's, c_p's and w's (T, ρ) derivatives come from the model's order-3 bundle (`ThermoModel::derivs`), forward through cv = −R·A20, cp = cv + R·x²/D and w² = RT·(D − x²/A20)/M, so `Fluid::prop` makes that call only for a partial that names one; τ's and δ's from the published reducing state (`FluidInfo::constants`). They are any variable of `d(X)/d(Y)\|Z`, and W or V of `d(d(X)/d(Y)\|Z)/d(W)\|V`, whose inner partial takes τ and δ but not c_v, c_p or w (differentiated twice they are order 4): compat's grammar reads them by name and alias and refuses those strings, and a `SecondPartial` with one inside is `NoModel`, as is `State::partial` of any of the seven and `State::second_partial` of τ or δ, which only the model can give. `derivative_variables_match_central_differences` (`tests/eos.rs`), at the 5,883 stable states of the core fluids' `eos` grids: each of the five differentiated in T and in ρ against its central differences in `Fd` (58,786 checks, headroom 0.93; 44 left to the identities where a central difference's own truncation, estimated from twice the step, exceeds the bound: (∂c_p/∂T)_ρ within a few kelvin of HFE143m's, Methanol's and Nitrogen's critical points), each differentiated by and held, τ and δ against the T and ρ derivatives scaled by the reducing state, and c_v, c_p and w as a second partial's outer variables, in `Identity` (176,490, headroom 0.12). No oracle: CoolProp's second derivatives refuse c_v, c_p and w |
| M20.2 | The conformance kit's `identities(fluid, states)` and `part_additivity(fluid, states)` (`conformance.rs`), each the number of checks that held or the first `Mismatch`, at its state's index, in class `Identity` (its near-critical bound in the window of the model's critical point), states with a quality skipped. `identities` is `tests/eos.rs`'s two lists moved: the 10 first-order identities (c_p's three where c_p and c_v are defined) and, where the model gives order 3, M7.10's seven (c_p − c_v and κ_T·ρ·w² where c_p, c_v and w are). `part_additivity` holds the model's ideal and residual parts against the state's own bundle (`PointDerivs::split` makes `derivs`' own total their sum, so that sum could not fail): p, (∂p/∂ρ)_T, (∂p/∂T)_ρ and c_v from the summed A_ij, h = h⁰ + h^r and s = s⁰ + s^r through `Fluid::prop`. `tests/eos.rs`'s `identities_hold` and `parts_add_up_to_the_state` run them on every core fluid's `eos` grid and every fluid's in the all-fluid tier, 8,088 states: 137,074 identities and 48,528 sums; `tests/new_family.rs` on the van der Waals family, 17 identities and 6 sums at each of three states |
| M20.3 | CI's linux job runs `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links"`; section 5.2 and VERIFICATION.md §11.3 say so, and `actionlint` is clean. On `main` it failed on: phasekit-wasm's crate docs linking its exports, which exist only on wasm32 (now code spans); phasekit-core's `density` module, `Fluid::flash_many` and the critical point's docs linking private items, and an unresolved `SurfaceTension` (now `crate::transport::SurfaceTension`); phasekit-verify's `v[i]` read as a link to `i` and two links to the private `side`; and phasekit-capi's shim linking three private functions. Rustdoc's four warnings, unclosed HTML tags in phasekit-verify and xtask, are fixed too |
| M20.4 | `cargo xtask nightly-report <dir>...` (`report.rs`), the nightly `oracle` job's summary in place of its shell table: per fluid and kind of each fixture set (the committed `coolprop-8.0.0` and `mp` sets and the full set), a table of the rows, the oracle's failed calls (`err:` rows) and the `nan` output cells of its other rows, a long-format file split by its `fluid` or `mixture` column; each fluid's arbiter records (`ARBITERS`), part and status, and whether one arbitrates, the fluids with no record named in one line as oracle only (1,144 on the committed set, most of them binary pairs); the ignored tests (`gates ignores`); a file that does not parse is listed and the report still written, since the step runs after a failed one. VERIFICATION.md §11.3's `nightly` row says what it lists. User decision PL3 split the step: what the checks assert of the cells is M20.4a's. Test `the_report_lists_rows_failed_calls_nan_cells_arbiters_and_ignores` |
| M20.4a | `phasekit_verify::Tallies` (`tallies.rs`; user decision PL3): per fluid, kind and column, the cells asserted and failed, the cells exempt by DIV id and the largest error / bound of those that held; per fluid and kind, the oracle-gap rows (VERIFICATION.md §11.4); a text form (`# tallies/v1`, a line per column, exemption and gap count) that `Tallies::read` adds up. The five oracle checks keep one: `EosCheck` and `TermCheck` through `Tally::count` (τ and δ too, the α⁰ rows' columns as `<column> (ideal)`); `FlashCheck` each row's phase and Q, a PT row's ρ, and p, h, s and u or the entry that exempts them; `SaturationCheck` the VLE's p, ρ′ and ρ″ (or DIV-0016) and the superancillary's as `superancillary <column>`; the props corpus's `PropsCheck` each output compared or refused by both, exempt, or an ECS cell beyond the oracle as an oracle-gap row. A row's fluid is `Fixture::fluid`'s, as in M20.4's tables. The term, eos, DT and PT flash, fastchebpure and props sweeps write theirs to `fixtures-full/tallies/<sweep>.tsv` before they assert, and `cargo xtask nightly-report --tallies <dir>` adds two tables: per fluid and kind, the cells asserted and failed, the exempt ones by DIV id, the oracle-gap rows and the headroom of the column with the most; per kind and column over every fluid, the same and the fluid with the most headroom. Test `the_report_adds_what_the_checks_assert` |

**Exit gate.** G1-G8; each step's checks. **ROT rows.** None. **User decisions implemented.** PL1-PL3, OD1.

### M21 The site

**Goal.** A single-page site that shows what phasekit computes, live in the visitor's browser, built and deployed from
this repository as its GitHub Pages site, `https://dzoay.github.io/phasekit/` (user decisions WS1-WS3). The site is a
consumer of the browser package: every number it shows comes from `phasekit-wasm` at the commit it was built from, and
what the page needs that the package lacks is added to the package in a step, with its tests. **Entry.** The user's
word, 2026-10-16, beside M13 (WS3): M21.1, then M21.7 (WS5), M21.2, M21.8 and M21.9 (the user's requests, 2026-10-16), then M21.3, M21.4 and M21.10 (the user's request, 2026-10-17); M21.5 after M13, M21.6 after M19. **Prerequisites.** User
decisions WS1-WS4; DEPENDENCIES.md's tier T5; Pages on, its source GitHub Actions, the `github-pages` environment
deploying from `main` only (done 2026-10-16). **Arbiters.** The page adds no numerics, so its oracle is the browser package: a value on
the page equals the package's at the same inputs (`Exact`), and the library's own tests check the package. Unit
conversions follow the SI brochure's definitions (0 °C = 273.15 K, 1 bar = 10⁵ Pa), theme contrast WCAG 2.2's
contrast ratio. M21 does not move `MILESTONE`.

| Step | Landed |
|---|---|
| M21.1 | `site/`: Svelte 5, Vite and TypeScript (T5); the calculator over the five embedded fluids, its view in the URL; themes (light, dark, the system's; four accents), WCAG 2.2 AA contrast checked; the engine worker on the `simd128` build where validated; the browser package's `outputKeys()`; `scripts/release/wasm-modules.sh`, which the npm package and the site share; G9 and CI's `site` job; `pages.yml`; the licences page; the page's budget, 33 218 B + 10 % |
| M21.2 | The diagrams page: p–h, T–s, p–T, T–ρ and h–s of the embedded fluids, sampled in `site/src/sample/` by the package's batch (every vertex `Exact` against it): the dome down to the saturation temperature of the lowest plotted pressure and the critical point; isotherms and isobars through the dome at their QT or PQ saturation states, isentropes, isenthalps and isochores with their QS, HQ or DQ edges, quality lines; a failed cell a gap. The readout flashes the state under the cursor; zoom and pan; CSV, SVG and PNG; three colour-blind-safe family colours, each with its dash; screenshots compared on CI's runner. The browser package's `Props1SI`; the page loaded on first visit; the budget 66 406 B + 10 % |
| M21.3 | Sweeps and the benchmark. `#/sweeps`: any output against T along isobars, against p along isotherms, or against T along saturation for each phase, with the latent heat as the package's h″ less its h′; up to six lines in a sequential ramp, their held values typed in the shown units, every point the package's batch value at its inputs (`site/src/sample/sweeps.ts`), read under the cursor. `#/benchmark`: N states of ρ T, p T, q T or p q on each build this browser runs, each build in its own worker (the engine's `bench` op, the batch timed as the page calls it), the cells by status, and the builds' answers compared bit for bit (`site/src/bench/`); n-Propane's DT, QT and PQ grids agree on both builds under Node |
| M21.4 | All 136 fluids. `cargo xtask packs <dir>`: one pack per fluid of `phasekit-data` (`phasekit_core::internal::pack`), naming the fluids it requires, and `index.json` with each fluid's aliases, CAS number (126 have one), requirements, file and bytes; `scripts/site/build.sh` writes them into the page, CI's `wasm-browser` job hands them to `site`, and they are never committed. The browser package's `names()` and `loaded()` (core's), its Node tests now on the five fluids it ships (`phasekit-verify` no longer turns on `fluids-all` for crates that depend on it). The page: one fluid picker on every page, searching names, aliases and CAS numbers; a fluid's packs fetched on first use, its requirements first, only where the registry lacks them, each added to every worker; where the chosen fluid's data comes from, and what the page has fetched and decoded, shown. The dome's first temperature is exactly the lowest, which rounding put below T_min for nine fluids, whose regions then failed. Re-planned in the step: every ECS reference is one of the five embedded fluids, so no fluid fetches two packs on the page; Playwright checks that R32 fetches only its own, Vitest the order where a requirement is not served |
| M21.5 | Mixtures, on what M13.14 gave the browser package (MX5). The package: `predefinedMixtures()`, `binaryPairs()` and `mixtureComponents(name)` (phasekit-mix's `components`); a mixture fluid's `components`, `composition` and `envelope()`, M13.12's trace with T, p, Q, the feed's density by mass and by mole, the incipient phase, the critical crossing and how the trace ended; a two-phase state's `phases()`, phasekit-mix's `Mixture::phases`: core's `State` keeps no compositions, so they are solved again at the state's molar quality, at its T and at its p, the solve that lands nearer kept (at T where two dew points share a T, the solve at T finds the other); the `.wasm` budgets re-measured, the baseline 45.5 kB larger. The page: the picker lists the 116 predefined mixtures the package builds (those whose components the index has, each two paired, which Vitest holds to the package's own builds), found by their name or a component's; a mixture's packs are its components', which the engine names; the mixture editor (`site/src/lib/mixture.svelte.ts`) writes `A[x]&B[y]` with each fraction as typed, each component among the fluids the library pairs with the rest, a pure fluid mixed at 0.5 each, a predefined mixture written out with the package's fractions, and the package checks the sum; the calculator shows the feed's and each phase's mole fractions; the diagrams draw a mixture's envelope on p–T and T–ρ, its only planes (`site/src/sample/envelope.ts`), the model's critical point on it, the readout's phases and how the trace ended; a liquid-liquid split is the package's `Unsupported`, explained beside it. Re-planned in the step: the vertices are `Exact` against the package's envelope, not its QT flash, a separate Newton solve, which refused R410A within 4.5 K of T_c until #257; every vertex more than 1 mK from T_c is the QT and the PQ flash's state within `Flash` (#257; 35 of R410A's 51 were answered before it), and the model's critical point lies 0.1 µK and 0.01 Pa from the trace's crossing (TM15's 0.02 K and 2 kPa). The page's budget holds (102 379 of 105 918 B) |
| M21.6 | The diagrams from M19 (user decision DG2): each diagram is the package's `Fluid.diagram(plane)` and the phase regions its `Fluid.regions()`, one engine call each (the engine's `diagram` and `regions`, their typed arrays copied out and every handle freed) and no batch, read into the page's vertices, lines and corners by `site/src/diagram/data.ts`; a mixture's envelope is its `diagram` on p–T and T–ρ, with its components, its feed and how the trace ended, and only where the package refuses another plane is the engine asked whether the fluid is a mixture, the view then switching to p–T. `site/src/sample/` is deleted: the sweeps moved to `site/src/sweeps/sample.ts` with the batch's rows, the plotted ranges to `site/src/lib/ranges.ts`, the planes' table to `site/src/diagram/planes.ts`; the engine's `envelope` op is gone. M21.2's, M21.5's and M21.9's checks run on the geometry the page draws (`tests/unit/isolines.test.ts`, `envelope.test.ts`, `regions.test.ts`); the samplers' own tests are phasekit-diagram's (M19). Red: on the samplers, the view drew Water's T–s from `components`, `constants` and 52 `batch` calls. Re-planned in the step: the page draws its tabs before the engine has loaded, so it keeps its table of planes, and Vitest holds each plane's axes and families to the package's `planes()`. The screenshots re-approved; the page 101 698 B gzipped, 2 072 B less than before (the samplers gone, the reader in), within its budget of 105 918 B |
| M21.7 | `site/src/lib/calculator.svelte.ts`: the calculator's view and wiring, which `Calculator.svelte` binds to and Vitest drives in a client-transformed environment; StrykerJS 10 with its Vitest runner (Vitest held at 4.1): `scripts/site/mutants.sh` in G9 on the site TypeScript a PR changes, static mutants skipped, and the weekly `site mutants` job over all of it; equivalent mutants excluded with their reasons; the baseline's 38.45 % raised to 100 % |
| M21.8 | Picking a state on a diagram (the user's request, 2026-10-16): the cursor snaps to a grid in the shown units at five densities (`site/src/diagram/grid.ts`: d3's nice steps on a linear axis; decades, 1-2-5, 1 to 9, halves and tenths per decade on a log one, each an integer over a power of ten), its lines drawn while snapping; a click pins the readout, the arrow keys step it along the grid, Escape lets it go; Open in calculator carries its two inputs, so the calculator shows the same state. Snapping and the density are in the URL. Before it, the readout keeps one flash in flight |
| M21.9 | Phase regions on the diagrams (the user's request, 2026-10-16), behind a toggle in the URL (`regions=1`): each region a polygon edged by the package's lines (`site/src/sample/regions.ts`): the dome, the critical isotherm and isobar, the isotherm and isobars that bound the range (T to `T_max`; p from half the saturation pressure at the lowest T to `P_max`), and on the cold side each isobar's lowest temperature with a state, found by bisection, which follows the melting line where the package has one; sampled once per fluid by the batch with every diagram's axes among its outputs (34 to 46 calls, 20 to 180 ms in Node); each region's phase the package's at a state inside it, so the page names none. The tints are drawn softened under the lines, each line on a halo of the surface, the critical isotherm and isobar sharp over them; each region is named at the deepest point of its part in the plot where the name fits (`site/src/diagram/geometry.ts`), zoom frames still at Chromium's 16.7 ms. Re-planned before it started: classifying the plot's cells by the readout pair took 15 s on T–s and 106 s on h–s |
| M21.10 | How a property is calculated (the user's request, 2026-10-17): `#/how-it-works`, the property pipeline of `phasekit-core` in four static figures traced from `flash.rs`, `flash/phase.rs`, `fluid.rs`, `state.rs` and `batch.rs`: one PT call from the registry to the outputs, the 19 pairs over the three primitives their iterates come down to, the phase rule, and a batch under `Reference` and `Simd`; a table of what loads when. The figures are inline SVG drawn from data (`site/src/pages/HowItWorks.svelte`), themed by the page's tokens with the Helmholtz model in the accent, and scroll in their own frames on a phone, where the header's links now take their own row. The page computes nothing, so its arbiter is the source it was traced from. Loaded on first visit; the budget 96 289 B + 10 % |

- **M21.11 The psychrometric chart (user decision HA3).** `#/psychrometric`: M18.9's chart at a pressure the visitor
  sets, drawn from the browser package's `psychrometric(p)`, with the moist-air state under the cursor (T, W, φ, h, v,
  T_wb and T_dp, the package's `HumidAir` at the cursor's T and W) and the cursor snapping as M21.8's; Air's pack
  fetched on first use. *Red:* every vertex drawn is the package's (`Exact`); the readout is the package's state at
  the cursor; the page's budget; Mollier's form (M18.10) behind a toggle in the URL, its readout the same states.
  *Done when:* G1-G9; its screenshots approved in the PR.
- **M21.12 Ice on Water's diagrams (user decision HA7).** Water's five diagrams draw M18.11's sublimation and melting
  lines, the triple point and the solid's areas, and the regions toggle (M21.9) its solid region, from the browser
  package's `Fluid.diagram` and `Fluid.regions`; the readout names ice. *Red:* every vertex drawn is the package's
  (`Exact`). *Done when:* G1-G9; the screenshots re-approved in the PR.

**Exit gate.** G1-G9 at every step; the page deployed from `main` at each merge; every value on the page equals the
package's (`Exact`); M21.1's budget and SL8's wasm budgets hold; `reuse lint` and `actionlint` clean. **ROT rows.**
None. **User decisions implemented.** WS1, WS2, WS3, WS4, WS5, HA3 (M21.11), HA7 (M21.11, M21.12).

### M22 Mixture lanes

**Goal.** A mixture's evaluations in lanes where that is faster: α^r and α⁰ batches at fixed composition, DT batches
under a phase hint, and, in a time-boxed spike, the kernels inside one flash. Lanes stay bitwise equal to the scalar
path (K17; user decision SL1), so no result changes (user decisions ML1, ML4). **Entry.** After M13 closes (the `m13`
tag; user decision ML2), on the i7-8700K; the steps merge in order when green. **Prerequisites.** User decisions
ML1-ML5; M12's lanes and executors; M13's `phasekit-mix`. M22 does not move `MILESTONE`.

What `decisions/2026-10-16-mixtures-in-lanes.md` measured decides the order. A batch at fixed z computes the reducing
state once and runs each component through its own lane kernel (M12), so it needs no core change; the departures need
core to run them in its lanes, as a kernel over `Real` (ML3), and core's batch reaches a mixture only through a hook on
`ThermoModel` (ML5). A flash's time goes to the composition derivatives, so the spike targets those, through the same
runner.

| Step | Landed |
|---|---|
| M22.1 | `Mixture` overrides `residual_batch` and `ideal_batch` (`phasekit-mix`'s `batch` module): the reducing state once per batch, each component's own hook over blocks of 64 points under the batch's policy (at its scaled (T_i, ρ_i) for α^r, at the mixture's (T, ρ) for α⁰), the departures point by point, and each point summed by the code that sums a single point's (`summed`, `ideal_sum`), so a batch is its points' `residual` and `ideal` bit for bit by construction (`mixture_batch_is_residual_per_point`, in `tests/batch.rs`, which CI's lane step runs at every dispatch level). GERG-2008's components take the default hook. M22's bench mixtures are `phasekit_verify::mixtures` (R32/R125 and Methane/Ethane at 50/50 and a five-component natural gas on the library, AGA8's 21-component check gas on GERG-2008). Benches `mixture_alphar_batch/{scalar,simd}` and their breakdown `mixture_alphar_parts` (VERIFICATION.md §12), recorded on the i7-8700K as M22's baseline (`benches/results/M22-intel-core-i7-8700k.csv`): under `Simd` the library mixtures' batches take 0.70-0.84× the same build's scalar time, their components 0.24-0.48×, and the departures 81-88 % of what remains; GERG-2008's gas is unchanged (ML4) |
| M22.2 | `batch::RealKernel`, whose one method `eval<R: Real>(&self, inputs: &[R], outputs: &mut [R])` is a family's pure function, its counts those of the columns it runs on, and `batch::run_kernel(kernel, inputs, outputs, exec)`: `Reference` and `Parallel` on `f64` point by point (fixed chunks, on rayon's pool), `Simd` in `Lanes<W>` a group at a time, a short last group padded with its first point; the working columns allocated once per call; unequal columns are `Error::Shape`. Core's match on the CPU's level became `exec::dispatch` over a crate-private `Widths` batch, which `run_simd`'s lane kernels and `run_kernel`'s columns share; `Lanes<W>`, the executors and the lane maths stay crate-private. ARCHITECTURE.md §3 (the seam table, the generic API, six traits), §3.1 and §7 say what is public. `real_kernels_are_scalar_bitwise` (`tests/batch.rs`, through the public path: a kernel using every `Real` method, 1 in and 1 out, 2 and 3, 3 and 15, over 1 to 65 points), `real_kernel_chunks_run_on_the_rayon_pool` and `real_kernels_run_whole_groups_then_the_rest`. No release tag exists, so `cargo semver-checks` had nothing to check |
| M22.3 | `phasekit-mix`'s departure kernel runs on `Series<R>` over any `Real` in place of its own `Scalar` trait, the variable marked by a flag in place of `compose`'s test of its coefficients; its scalar results unchanged bit for bit (every library and GERG-2008 function and GERG-2008 component at 30 states and every order). `Departures`, a `RealKernel` ((τ, δ) in, each function's A_ij out): a mixture's batch runs its departures through `batch::run_kernel` a block at a time, and GERG-2008's components override `residual_batch` through it (their α⁰ stays point by point). `departure_lanes_are_scalar_bitwise`. Kept (ML4): under `Simd` the bench mixtures' batches take 0.74-0.80× M22.2's time (two interleaved rounds on the i7-8700K), and the kernel, `#[inline(always)]` down to its operations, takes the scalar path's to 0.73-0.88× too (`benches/results/M22-intel-core-i7-8700k.csv`); the departures' `powf`, per lane through `math`, keep them 76-84 % of a library mixture's `Simd` batch |
| M22.4 | `ThermoModel::flash_many`, provided (each point's `flash` by default): under `ExecPolicy::Simd` core's batch calls it for every model with the points its gate passed, in order, and writes each result back to its point's row, so a refused point keeps its refusal and no family fills a lane for it; its docs say an override is each point's `flash` bit for bit (K17). `PureFluid`'s is its M12 lane path for DT, QT and PQ, in place of the batch's downcast. `Mixture`'s takes a DT group under a phase hint: the bundles of the points its domain admits through its `residual_batch` and `ideal_batch`, then each point's state by `state_from`, which its scalar hinted DT calls too (a refused point is refused before it is evaluated, as its flash refuses it: the first record, which evaluated them all, put R32/R125 at 1.68× the scalar path, half the grid lying above R32's T_max). ARCHITECTURE.md §3 and §7. `hinted_mixture_dt_batches_are_per_point` (`tests/batch.rs`). The driver hands the hook up to 256 points at a time (`batch.rs`'s `HOOK_GROUP`; a pure fluid's kernels keep their groups of 64): the mixture's hook pays per call too, for lane kernels whose code comes afresh each call, and at 64 points the default build's R32/R125 batch took 1.15× its scalar time, at 256 0.93× (1024 no less); the driver keeps one array of states per chunk, which the hook writes and the rows read in place, as moving it cost 2 %. `PureFluid::flash_many`'s fallback calls `self.flash`, as calling `flash::flash` from it outlined that and slowed the pure fluids' scalar DT batch by up to 1.48×. Bench `mixture_dt_batch/{scalar,lanes,simd}` (`lanes`: `Simd` in the default build, user decision ML6): under `Simd` the bench mixtures' DT batches take 0.94× (R32/R125), 0.78× (Methane/Ethane), 0.84× (the natural gas) and 0.94× (AGA8's gas) of the default build's scalar time, and 0.80×, 0.64×, 0.76× and 0.92× of the `simd` build's (`benches/results/M22-intel-core-i7-8700k.csv`); the pure fluids' lane batches take 0.95× to 0.99× main's (same-boot A/B) |
| M22.5 | A mixture's flash evaluates the composition derivatives of its liquid and its vapour at one T side by side, under the mixture's policy (`Mixture::with_exec`, `ExecPolicy::Simd` by default): `MultiFluid::compositions` takes each component's derivatives at both phases' (τ, δ) through its own `residual_batch`, then runs the rest of Ψ, the departures and the multivariate Taylor arithmetic now generic over `Real` (`Taylor<R>`, `reduce_taylor`), as one `RealKernel` (`Phases`) through `batch::run_kernel`; a phase with a fraction at 0, whose reducing functions test a pair's denominator, runs point by point. The split's substitution and Newton steps (`both_at_t_p`) and the saturation solver's Newton steps (`AtTp::both`), and through them the isochoric split, call it. `two_phase_lanes_are_scalar_bitwise` (`composition.rs`) and `two_phase_flashes_are_scalar_bitwise` (`tests/batch.rs`: every third of `mix_flash`'s and `mix_sat`'s rows, 470 flashes, `Simd` against `Reference`, every field's bits). Bench `mixture_flash/{pt,qt,pq}` (R32/R125). Kept (user decision ML6: the default build decides): R32/R125's QT and PQ flashes take 0.92× `Reference`'s time in the default build, PT 1.00×, and 1.00× to 1.01× in the `simd` build; the `Reference` path is up to 3 % slower than M22.4's on QT (user decision ML7) (`benches/results/M22-intel-core-i7-8700k.csv`) |
| M22.6 | Taken out (ML4): the stability test's N + 2 trial phases ran in lockstep, each round the trials still iterating evaluated at (T, p) side by side (each one's stable root, then their composition derivatives one batch an order through M22.5's kernel) and each took its own scalar step, bitwise the trials taken alone (`stability_minima_are_scalar_bitwise`: every trial's distance and minimiser, `tests/pt_flash.rs`'s corpus and methanol/benzene). In the default build R32/R125's PT flash under `Simd` took 1.00× both M22.5's time and `Reference`'s (`mixture_flash/pt`, two interleaved rounds on the i7-8700K), and gungraun's single-phase PT flashes 1.004× M22.5's instructions: each trial's density, solved point by point, is 83-85 % of the test's instructions (callgrind). The code is #251's first commit |
| M22.7 | The spike's record. Of its kernels, M22.5's (a flash's two phases side by side) is kept (user decision ML6) and M22.6's (the stability test's trial phases) taken out (ML4). The i7-8700K's M22 record, every row at M22.7 and both builds (`benches/results/M22-intel-core-i7-8700k.csv`): under `Simd` the bench mixtures' α^r batches take 0.86-0.89× the default build's scalar time and 0.41-0.48× the `simd` build's, their DT batches 0.79-0.94× and 0.41-0.78×, and R32/R125's QT and PQ flashes 0.93× and 0.94× in the default build and 1.01× in the `simd` build, PT 1.00× and 1.01×. The step's red, `gates perf` with the mixture benches' instruction counts in its baseline, landed with user decision PG1 (#235); its four mixture rows now hold M22's counts (CI's on 1d5024e), 19-33 % under PG1's, which the code before M22.1 exceeds by 24-48 %. No re-plan: most of a PT flash's remaining time is the stability test's density solves, each trial's on its own (M22.6), and a kernel for them lies outside the composition derivatives that ML1's spike covers |
| M22.8 | The browser build, and M22 closed. Under `ExecPolicy::Simd` core runs a batch of at most two points, as a flash's liquid and vapour, at two lanes, which the CPU's best level had padded to a group of its own (4 lanes on AVX2, 8 on simd128), and on simd128 point by point, as `MultiFluid::compositions` then runs a flash's two phases (`exec::run` and `run_kernel`; user decision ML8: under Node, M22.5's kernel had taken a CO2/n-Propane QT or PQ flash on simd128 to 1.13-1.16× the scalar path's time, and two lanes still to 1.04-1.08×). `one_or_two_points_run_two_lanes` (`exec.rs`) and `a_mixtures_batch_is_its_reference_paths` (`phasekit-wasm`'s `tests/node.rs`, both builds: CO2/n-Propane's QT, PQ, PT and DT batches against compat's `batch_si` on the mixture under `Reference`, every cell's bits). The `simd` build's R32/R125 QT and PQ flashes take 0.91× and 0.92× its scalar time (1.01× before; same-boot A/B); under Node a CO2/n-Propane QT or PQ flash takes 0.95× `Reference`'s on the baseline module and 0.99-1.01× on simd128, whose module, no longer reaching M22.5's kernel, shrinks by 75 KB. The browser's batch takes no phase hint, so a mixture's DT batch runs each point's flash rather than M22.4's lanes |

**Exit gate.** G1-G9, and G1-G8 with `--features simd`; `policy_equivalence` bitwise for the mixture batches at every
dispatch level of the i7-8700K, the CI runners (x86-64, aarch64) and wasip2 with `simd128`; the flash fixtures'
results unchanged by the spike; each kernel faster than the scalar path or taken out (ML4). **ROT rows.** None. **User
decisions implemented.** ML1-ML8.

### M23 In-house maths

**Goal.** `exp`, `ln`, `pow` and `expm1` written in core: under 1 ulp, the same bits on every target, one code for the
scalar path and the lanes, and as fast as std's. They become every build's maths if they pass M9.5's rule (D12), so a
`simd` build's single queries no longer pay the `libm` crate's cost; an opt-in `fast` feature then adds FMA on every
target (user decisions IM1-IM5). **Entry.** After M13 closes (the `m13` tag; the core is frozen until then, MX2), on
the i7-8700K. It is independent of M22, so either may start first; the steps merge in order when green.
**Prerequisites.** User decisions IM1-IM5 and LC1; M9.4's hash on the four targets; M12's lanes; mpmath for the
reference values (`uv run --no-project --python 3.12 --with mpmath==1.3.0`). M23 does not move `MILESTONE`.

Each function is one scalar function in a private module of `num` (named at M23.2), built as M12.5's ports are: a main
path without branches or divisions, a test for whether a value takes it, and selects for the special cases. `math` calls
it at a point and `Lanes` maps it across lanes, a group whose lanes all take the main path skipping the special cases,
so each lane is the scalar path's bits by construction (K17). A table is indexed by the reduced argument's low bits,
which LLVM gathers or loads lane by lane. Core denies `clippy::indexing_slicing` and `clippy::arithmetic_side_effects`
in its library code (M24.5), and the ports keep to both without an `allow`: a table is a fixed-size array of words read
by `get` at an index masked to its length, so the compiler drops the bounds check and the fallback never runs, and the
integer arithmetic on exponents, shifts and table indices is wrapping (`wrapping_add`, `wrapping_shl`), as M12.5's
reduction has been since M24.5. Only if a bench shows `get` slower than plain indexing does a lookup keep plain
indexing, under an `allow` that names the mask, as the jets' product loops do (M24.5). Until M23.6 the in-house
functions replace only the `libm` crate's, in the `libm` and `simd` builds, which get them under either outcome of IM1;
without `libm`, `math` and `Lanes` keep std's, lane by lane, as now. The browser package's two builds are both `libm`
builds (user decision IM6), so they run the in-house functions as each step lands them, with the same bits (SL8). Each step re-commits `fixtures/hash/libm.txt` once
the four CI targets agree on it.

| Step | Landed |
|---|---|
| M23.1 | `scripts/math/reference.py` (mpmath 1.3.0 at 256 bits) writes `fixtures/math/{exp,ln,pow,expm1}.csv`, 3940 to 4131 values each in three sets: `edge` (VERIFICATION.md §9.2's edges and the thresholds of the `libm` crate's and Arm's code paths, each ±1 ulp), `eos` (1950: exp's −60 to 5; ln's τ in [0.0026, 5.27] and δ in [1e-12, 3.83], the 136 fluids' ranges; pow's bases τ, δ, θ, Δ and (δ − 1)² with the fluids' non-integer exponents; expm1's −θ_k τ of the Planck-Einstein terms) and `binade` (1950, uniform over every exponent of the domain); and `fixtures/math/arm/` at the inputs of Arm's directed cases (optimized-routines v26.07, pinned by `scripts/fetch-optimized-routines.sh`; `log.tst` for ln; a case with a signalling NaN operand is left out, as the format writes every NaN `nan`), each value checked against Arm's result. A value is its correctly rounded double and its residual in ulps to four digits; 979 kB together. Class `Faithful` (VERIFICATION.md §5): under 1 ulp in Arm's convention, `phasekit_verify::ulp`. `tests/math.rs` reports the backend's worst error per set (`--no-capture`; CI's linux job prints both backends'): glibc 0.72 ulp (expm1) and the `libm` crate 0.76 (exp) on x86-64 Linux. Core's lane `exp`, `ln` and `expm1` carry their msun notices at their heads and `crates/phasekit-core/NOTICE` both notices (LC1, `tests/notices.rs`); `LICENSES/LLVM-exception.txt` came with `fixtures/math/arm/`. `benches/math.rs`: `math_per_call/{exp,ln,pow,expm1}`, `throughput` and `chain`; it and the six scalar benches of D12's rule are named `<case>_libm` on the `libm` crate's maths (`backend::case`), and both builds are recorded on the i7-8700K as M23's baseline (`benches/results/M23-intel-core-i7-8700k.csv`) |
| M23.2 | `num::elementary` (the private module) holds core's own `exp`: Arm's `math/exp.c` without FMA, its 128-entry table as `u64` words read by `get` at an index masked to its length, the degree-5 polynomial, k rounded by adding 1.5·2⁵² and read from the sum's low bits, the integer arithmetic wrapping, no `allow`. 2⁻⁵⁴ ≤ \|x\| < 512 takes the main path; Arm's `specialcase` (512 to 1024), 1 + x below 2⁻⁵⁴, 0 or ∞ from 1024 and NaN are computed beside it and selected; Arm's guard against −0 under downward rounding is left out. Under `libm` (and so `simd`), `math::exp` and `Lanes::exp` run it; `internal::exp` exposes it in every build. M12.5's lane port of the `libm` crate's `exp` is gone, the constants the lane `expm1` shares moved there, and its e_exp.c notice left `REUSE.toml`, `LICENSE-THIRD-PARTY` and core's NOTICE, which now carry Arm's (IM5). `exp_is_faithful` (class `Faithful`): worst 0.500 ulp (`edge`), 0.503 (`eos`), 0.502 (`binade`), and Arm's results bit for bit at Arm's cases; `lane_exp_is_scalar_bitwise` runs against it. `math/exp.csv` gained a `subnormal` set (512 values, x uniform from −745.13 to −708.40, `scripts/math/reference.py`), where `exp_rounds_once_in_the_subnormals` holds `exp` under 0.51 ulp (worst 0.498, correctly rounded at all 512, as glibc is): Arm's `specialcase` rounds once there, while rounding twice errs by up to 0.76 ulp, which the faithful bound lets through. Six mutants are equivalent and carry markers. The `libm` hash is `7d143b4e2fe3ff33` on the four targets. The browser package's baseline build enables `phasekit-core/libm` (user decision IM6), so its two builds keep SL8's same bits (1 724 227 B, +35 kB). The site's critical isobar closes in on T_c (`awayFrom`): on the even grid, its first leg cut across the isobar's flat bend on T–s and tinted a sliver above p_c as Nitrogen's supercritical gas, which showed once core's `exp` let the package compute Nitrogen's critical point by (T_c, ρ_c). The regions test now stubs the refused critical point it had relied on. LB1's Water HP cell left `LIBM_KNOWN` (three cells), and `tests/common/mix_sat.rs` divides the feed by its sum as the solve does: unnormalised, R418A's bubble ln φ at 258.27 K moved by about 1e-12. Recorded on the i7-8700K against M23.1's rows (`benches/results/M23-intel-core-i7-8700k.csv`, the simd build's): `math_per_call/exp_libm` 8.8 → 4.0 ns throughput and 30.5 → 14.0 ns chain, `alphar_order2/residual_libm` 0.63-0.81× of M23.1's; the lanes, which M23.1 did not record, measured against M23.1 in the same boot: `alphar_batch/simd` 0.87-0.97× |
| M23.3 | `num::elementary` holds core's own `ln`: Arm's `math/log.c` without FMA, its two 128-entry tables as `u64` words read by `get` at an index masked to their length (`TAB`, 1/c and ln c; `TAB2`, c as chi + clo, which stands in for FMA in r = (z − chi − clo)·(1/c)), the degree-6 polynomial, and near 1, from 1 − 2⁻⁴ to 1 + 0x1.09p-4, the degree-12 polynomial with r split at 2²⁷. k becomes a double through the low bits of 1.5·2⁵² + k, never by an `as` conversion. A positive normal x away from 1 takes the main path; the path near 1, a subnormal x scaled by 2⁵², −∞ at ±0, NaN below 0 and x itself at +∞ and NaN are computed beside it and selected, and the scalar function branches to the path near 1. Under `libm` (and so `simd` and both browser builds, IM6), `math::ln` and `Lanes::ln` run it; `internal::ln` exposes it in every build. M12.6's lane port of the `libm` crate's `log` is gone, with its e_log.c notice and its mutants exclusion; Arm's notice names log.c. `ln_is_faithful` (class `Faithful`): worst 0.4962 ulp (`edge`), 0.4950 (`eos`), 0.4999 (`binade`), glibc's own figures, and Arm's results bit for bit at Arm's cases; `lane_ln_is_scalar_bitwise` in every build. Both paths sum hi + lo by `fast_two_sum` (Dekker's Fast2Sum, `fast_two_sum_is_exact`) and the path near 1 splits r by `split` (`split_keeps_26_bits`), each tested exactly: the main path with w − r for w + r moves about one result in a thousand by 1 ulp, and a split elsewhere than 26 bits leaves rhi² rounded, under a tenth of an ulp, both of which the faithful bound lets through. The `libm` hash is `2b3665969b95eb03` on the four targets. `LIBM_KNOWN` keeps its three cells. Recorded on the i7-8700K against M23.2's rows (`benches/results/M23-intel-core-i7-8700k.csv`, the simd build's): `math_per_call/ln_libm` 6.9 → 5.4 ns throughput and 25.0 → 15.3 ns chain, `alphar_order2/residual_libm` and `alphar_batch/scalar_libm` 0.96-1.01×; the lanes in a same-boot A/B against `main` (two interleaved rounds, means of the medians): `alphar_batch/simd` 0.98-1.02×, but n-Heptane 1.08× (58.8 → 63.3 ns) |
| M23.4 | `num::elementary` holds core's own `pow`: Arm's `math/pow.c` without FMA. Its `log_inline` gives ln x to about 68 bits as hi + lo from its own 128-entry table, three `u64` tables (1/c, ln c and ln c's tail) read by `get` at an index masked to their length, with z split at its top 32 bits so that r = z/c − 1 is exact, and the degree-8 polynomial; y·(hi + lo) becomes ehi + elo from 27-bit halves, which M23.2's `exp` takes with its tail (Arm's `exp_inline`): `exp`'s reduction gained the tail and a sign, and its `specialcase` below 0 became `pow`'s (±1 by y's sign, a zero signed by the scale), `exp`'s own bits unchanged. A positive normal x with 2⁻⁶⁵ ≤ \|y\| < 2⁶³ whose y·ln x takes `exp`'s main path takes the main path; C99 Annex F's cases (Arm's signalling-NaN cases kept), a negative base (NaN for a non-integer y, an odd integer's sign by `checkint` as selects), a subnormal base scaled by 2⁵², \|y\| below 2⁻⁶⁵ or from 2⁶³, and `exp`'s overflow and underflow are computed beside it and selected in reverse of Arm's order. Under `libm` (and so `simd` and both browser builds, IM6), `math::powf` runs it and `Lanes::powf` maps it at one exponent (`pow_lanes`), where M12.8 called `math::powf` lane by lane; `internal::pow` exposes it in every build; `libm_backend_matches_the_libm_crate_bitwise` no longer covers `powf`. `pow_is_faithful` (class `Faithful`): worst 0.5000 ulp (`edge`), 0.5045 (`eos`, glibc's figure), 0.5014 (`binade`) and 0.5000 (Arm's cases), and Arm's results bit for bit at Arm's cases but nine near-ties, which it names: their exact values lie at a midpoint of two doubles to four digits (a tail of ±½), and Arm's own `pow` built without FMA (gcc, `-ffp-contract=off`, x86-64) rounds them as core's does. Core's `pow` is that build's bits at all 2 105 328 pairs tried (the fixtures, random bit patterns, bases near 1, negative and subnormal bases with integer exponents); Arm's file lists the correctly rounded results, which its FMA build gives. `lane_pow_is_scalar_bitwise` in every build; `checkint_reads_parity`, `signalling_nans_are_not_one`, `negative_bases_take_the_sign_of_odd_powers` and `odd_powers_of_negative_bases_mirror_positive_ones` test the selects exactly, and `ln_is_arms_bitwise` holds `log_inline`'s hi and lo to the C build's. The `libm` hash is `f1013c2bc9735a67` on the four targets. `LIBM_KNOWN` is empty: Ammonia's η and initial-density term (16 cells) and Methanol's η (one), which the `libm` crate's `pow` moved past `transport_direct`, pass on core's; `backend::known` and `known_cells` keep LB1's route for M23.6, their list helpers tested on M9.3's list. Recorded on the i7-8700K against M23.3's rows (`benches/results/M23-intel-core-i7-8700k.csv`, the simd build's), where every bench that `pow` does not reach ran 0.93-0.95× (the machine, not the code: `lookup_by_name` 0.93×): `math_per_call/pow_libm` 58.4 → 19.2 ns throughput and 78.7 → 30.3 ns chain (std's glibc `pow` 9.8 and 22.8), Water's `alphar_batch/scalar_libm` 1883 → 1119 ns and `alphar_order2/residual_libm` 1888 → 1138 ns, the other fluids 0.92-0.95×; the lanes in a same-boot A/B against `main` (three interleaved rounds, means of the medians): Water's `alphar_batch/simd` 1328 → 682 ns (1.95×), the other four fluids 0.95-0.99×. |
| M23.5 | `num::elementary` holds core's own `expm1`, on M23.2's `exp` table, which the record chose: e^x − 1 = (scale − 1) + scale·tmp from `exp`'s reduction, scale − 1 summed exactly by a new `two_sum` (Knuth's TwoSum), for 2⁻⁵ ≤ \|x\| < 512, the main path; Taylor's polynomial to x⁸ below 2⁻⁵, `exp_signed` less 1 from 512, and x itself below 2⁻⁵⁴ and at NaN, each computed beside it and selected. It replaced M12.8's port of the `libm` crate's `expm1`, which moved to `num::elementary` first, as the step planned: faithful (0.7217 ulp at worst) but, as the scalar path's function, every path and a division at every call, 17.1 ns a call against the table form's 4.4 (`math_per_call/expm1_libm` throughput in the simd build, a same-boot A/B; the table form without its scalar branch 6.3, the `libm` crate's own branching `expm1` 8.3 at M23.4's record). `expm1_is_faithful` (class `Faithful`): worst 0.4896 ulp (`edge`), 0.5374 (`eos`) and 0.4967 (`binade`), and Arm's nine cases bit for bit. Under `libm` (and so `simd` and both browser builds) `math::expm1` runs it and `Lanes::expm1` maps it (`expm1_lanes`, the main path where every lane takes it); `internal::expm1` exposes it in every build; `libm_backend_matches_the_libm_crate_bitwise` no longer covers `expm1`. `lane_expm1_is_scalar_bitwise` runs in every build (M12.8's `lane_expm1_is_libm_bitwise` ran under `libm` only), with `two_sum_is_exact` and `the_paths_agree_where_they_meet` (Taylor's and the table's values within 1 ulp about \|x\| = 2⁻⁵). The port's notices (user decision LC1) went with it: the `libm` crate's and Sun's msun section of LICENSE-THIRD-PARTY and core's NOTICE, its REUSE annotation and `LICENSES/SunPro.txt`; cargo-about's THIRD-PARTY-CRATES.txt still credits the `libm` crate wherever it is linked. The `libm` hash is `6135a396534252b1` on the four targets; `LIBM_KNOWN` stays empty. Recorded on the i7-8700K against M23.4's rows (`benches/results/M23-intel-core-i7-8700k.csv`, the simd build's; `lookup_by_name` 1.00×): `math_per_call/expm1_libm` 8.3 → 4.9 ns throughput and 28.1 → 14.4 ns chain (std's glibc `expm1` 6.8 and 19.7); the fluids whose α⁰ calls `expm1` (all but R134a, whose α⁰ has Power terms only, 0.99-1.01×): `properties_at_t_rho_batch/scalar_libm` 0.88-0.91×, `properties_at_t_rho/dt_h_cp_libm` 0.87-0.92×, `ph_flash/ph_t_libm` 0.80-0.86× and `pt_flash/pt_h_libm` 0.95-0.98×; the lanes in a same-boot A/B against `main` (three interleaved rounds, means of the medians): `properties_at_t_rho_batch/simd` 0.88-0.96× (n-Heptane 447 → 393 ns), R134a 1.00×. |
| M23.6 | D12's rule applied to the `libm` build with core's own `exp`, `ln`, `pow` and `expm1` (user decision IM1) against the default build (std, glibc): the six scalar benches over the five bench fluids, prebuilt binaries in three interleaved rounds on the quiet i7-8700K (2026-10-20), means of the criterion medians. The geometric mean of the 30 libm/std ratios is 1.056, over 1.05: per bench α^r order 2 1.091, PT 1.084, QT 1.047, PQ 1.042, properties at (T, ρ) 1.039 and PH 1.034; per bench and fluid 0.966 (n-Propane's properties) to 1.181 (Water's α^r), seven above 1.10 (Water's α^r, PT, QT, properties and PQ; R134a's PH 1.111 and QT 1.108). The rule fails, so, as the step planned, the measurements went into D12 (ARCHITECTURE.md), the default keeps std, and `simd` keeps enabling `libm`; nothing else changed. Per call (`math_per_call`, M23.5's record) core's `pow` takes 19.1 ns to glibc's 9.8, and Water's α^r is the term sum that leans on it most. |
| M23.7a | Why fused α^r term sums stopped R411A's bubble point at 260.48 K converging under M23.7's `fast` (issue #244, user decision IM9): not `fast`. Traced in every build, Newton's iterates from Wilson's seed fall into a two-cycle by step 4, p alternating between 303500.127077285 and .127077452 Pa with the worst residual at 5.3e-13, above the solve's own gate (`GATE`, half of `FUGACITY_GATE`'s 1e-12), for all 60 steps. std and `libm` passed only because the split's seed landed at 3.93e-13; under `fast` all three seeds cycle. The floor is the feed liquid's: Z ≈ 0.009 at 3.035 bar, so its p is the 1 % of 1 + δα^r_δ that the α^r_δ sum leaves, and over ρ's next eight ulps the evaluated p moves by up to 6e-11 relative against 8e-14 for its true slope (dln p/dln ρ = 670). Fixed by user decision IM11 in every build: `saturation::Floor` counts the iterates that meet `FUGACITY_GATE` but not `GATE`, and after `STALLED` (4) of them `newton` returns the best, the gate ROT-185 and the tests judge by; every solve that does not stall still ends at `GATE`. `residual` is `meets_gate`'s measure. *Red:* `a_solve_stalled_at_its_floor_returns_its_best` (NoConvergence after 60 steps from Wilson's seed); with `the_residual_is_the_gates_measure` and `a_floor_returns_its_best_once_stalled`. The workspace's tests pass unchanged in the default and `libm` builds. |
| M23.7 | The `fast` feature (user decisions IM4, IM7-IM10). `Real::mad`, a·b + c, on `f64`, `Lanes` and the jets, through `math::mul_add`, the one call of `f64::mul_add` that clippy's ban lets through: two roundings by default, so every default bit is unchanged (core's tests, the `libm` hash), and one under `fast`, which enables `libm` (IM7) and names `math::BACKEND` `"fast"`. Under `fast` the in-house functions take Arm's FMA forms (`num/fused.rs`): `ln`'s and `pow`'s r = fma(z, 1/c, −1) without `log`'s second table or `pow`'s split of z, `pow`'s `lo3` and `elo`, and `exp`'s polynomial fused in its own order, each Arm's C built with FMA (gcc `-mfma -ffp-contract=off`, the polynomial written with explicit `fma`) bit for bit: `ln` at 999 897 of 10⁶ inputs (the rest signalling NaNs, which C quiets), `pow` at all 2 105 328 pairs, `exp` at all 10⁶. The α^r term accumulators (`Derivs::add_outer`, `add_separable`), Horner and α⁰'s lead term take `mad`, a jet's fused onto c's coefficients; the superancillary's Clenshaw sums keep CoolProp's order (IM8). *Red:* `fast_fuses`, (1 + 2⁻³⁰)(1 − 2⁻³⁰) − 1 = −2⁻⁶⁰ under `fast` on `f64`, lanes and a jet's value, 0 without, red with the helper unfused. M23.1's values stay under 1 ulp (Arm's `pow` near-ties five, not nine; `ln_is_arms_bitwise` the FMA build's lo at the last subnormal); `fixtures/hash/fast.txt` is `8fda6d9eb0e69f5b` on x86-64 Linux and wasm32-wasip2 (software `fma`), and CI's windows and aarch64 jobs assert it; the lane tests, `policy_equivalence_at_every_level` and `real_kernels_are_scalar_bitwise` (its kernel now takes `mad`) pass under `fast,simd`; the oracle classes and K1/K2 rows pass unchanged (the workspace but xtask, whose datagen regenerates std's files). CI: clippy on `fast` and `fast,simd`, G3 on `fast` with its lanes on linux and at NEON, and the hash loops and the maths report with `fast`. `gates mutants` leaves `num/fused.rs` out, which compiles only with `fast`; by hand under `fast`, 46 mutants, 46 caught. Same-boot A/B on the i7-8700K (two interleaved rounds of D12's six scalar benches, three of the α^r lanes, means of the medians), `fast` against the default build compiled alike: at `-C target-cpu=x86-64-v3` 0.951 in geometric mean (0.90-1.01 per bench and fluid), the lanes 0.929 (0.87-0.97); at the x86-64 baseline 2.157 (1.54-3.13), the lanes 1.042, each a·b + c a call to compiler-builtins' `fma`, which reaches the CPU's FMA behind it. The v3 default itself takes 1.078 times the baseline default's time (1.01-1.18), so `fast` at v3 takes 1.026 times the baseline default's (0.93-1.11): on this CPU the plain default build stays the fastest. The browser builds do not enable `fast`. |
| M23.8 | M23 closed. The browser package's two builds under Node 26 on the i7-8700K, both running the in-house functions (IM6): n-Propane's DT + h + c_p, QT + h and PQ + h batches, 4096 points on 64 × 64 grids as one `Fluid.evaluate` call, the median of 25, take 1072, 1025 and 1300 ns per state on the baseline build and 956, 829 and 1105 ns on `simd128`, 0.58-0.66 and 0.71-0.76 times main's before M23 (c04cd5c; same boot, three alternating rounds); each `.wasm` within its budget (SL8). ARCHITECTURE.md §7 item 5 and VERIFICATION.md §12 say what landed (the `_fast` and `simd_fast` cases). M23's record on the i7-8700K (`benches/results/M23-intel-core-i7-8700k.csv`, 317 rows, step M23.8): the `simd`, `fast` and `fast,simd` builds' benches, then `cargo xtask bench --record`. Its 176 re-recorded rows run a median 1.000 of their last (geomean 0.997), and 141 rows are new, the `fast` build's at the x86-64 baseline among them (each a·b + c a call to `fma` there: `math_per_call/exp_fast` 15.2 ns a call against `exp_libm`'s 3.8). One row is an outlier: `math_per_call/expm1_libm`'s throughput, 8.5 ns against M23.5's 4.9, where a same-boot A/B of the two commits' `math::expm1`, the same 180 instructions, gives 4.95 and 4.96 ns. Exit gate: each function under 1 ulp on M23.1's values (IM3), under `fast` too; the lanes bitwise at every dispatch level (the i7-8700K, CI's x86-64 and aarch64 runners, wasip2's `simd128`); the `libm` and `fast` hashes one on the four targets (`6135a396534252b1`, `8fda6d9eb0e69f5b`); M9.5's rule applied and recorded (M23.6, IM1); G1-G9, and G1-G8 with `simd` and with `fast`, in CI. The `m23` tag waits for the user's approval. |

**Exit gate.** G1-G9, and G1-G8 with `--features simd` and with `--features fast`; each function under 1 ulp on
M23.1's values (IM3); the lanes bitwise equal to the scalar path at every dispatch level of the i7-8700K, the CI
runners (x86-64, aarch64) and wasip2 with `simd128`; each committed hash reproduced on the four targets (the `libm`
build's, every build's if M23.6 passed, and `fast`'s); M9.5's rule applied and recorded (IM1). **ROT rows.** None.
**User decisions implemented.** IM1-IM5, and LC1's notices in core.

### M24 Panics at the facades

**Goal.** No panic reaches a caller unannounced (issue #219; ROT-150). At the C boundary every panic on a call's path is
caught, and that is shown along the real path, not only for a panic handed to the catch. The browser package cannot
catch one, so the code it reaches is held to lints that refuse implicit panics, and a panic that happens anyway reaches
JavaScript as an error naming it, the module then refusing every later call. **Entry.** The user's word, 2026-10-18,
before `v0.1.0` (user decision PH1), after M13's close, so phasekit-core is no longer frozen; the steps merge in order
when green. M24 does not move `MILESTONE`.

| Step | Landed |
|---|---|
| M24.1 | `phasekit_compat::fault`, behind the test-only `fault-injection` feature that no shipped build turns on: `with_faults`, a resolver whose four names panic on purpose where a call reaches the registry or a model: `fault::resolve` in the resolver, `fault::info`, `fault::flash` and `fault::drop` in a model's `info`, flash and drop (the registry's Water otherwise). phasekit-capi's tests turn it on (a dev-dependency), and both its registries, the handles' and the shim's, read the names there (`named`, under `#[cfg(test)]`). Every export a fluid name or handle reaches returns its failure value with its error set: `pk_props_si`, `pk_props1_si`, `pk_phase_si`, `pk_fluid_new`, `pk_registry_with_reference`, `pk_fluid_name`, `pk_batch` and `pk_fluid_free` return `PK_PANIC` with the last error naming the catch; the shim's `PropsSI` and `Props1SI` return `_HUGE` and `PhaseSI` and `get_fluid_param_string` 0, with the `errstring`; the next call succeeds. The other exports reach no model a test can fault (`pk_registry_new`, `pk_registry_free`, the shim's seven others); they run under the same catch, which `gates abi` checks, and `pk_last_error` only copies a string |
| M24.2 | phasekit-wasm's panic hook, installed by the first call that computes (`alive`): it runs the hook before it, keeps the first panic's message in a `OnceLock` (D8's one-shot form) and throws it to JavaScript as an `Error` (`phasekit panicked: <message>. This module cannot be used again: load it anew`) in place of `RuntimeError: unreachable`; `panicked()` returns the message (`undefined` before), and every call that computes (the `Registry` constructor, `withPack`, `withReference`, `PropsSI`, `Props1SI` and `fluid`, `Fluid.flash` and `evaluate`, `State.get`) then refuses with it, while `names`, `loaded` and the getters, which only read, stay as they were. The `fault-injection` feature turns on compat's and gives `new Registry()` its names; `tests/panics.rs`, a test binary of its own, runs on both builds in CI's `wasm-browser` job; the npm package's README says what a panic does |
| M24.3 | phasekit-compat, phasekit-capi and phasekit-wasm deny `clippy::indexing_slicing` and `clippy::arithmetic_side_effects` in their library code, their tests exempt (`#![cfg_attr(not(test), deny(...))]` at each root, which `gates lints` requires of the crates it lists). Six sites, all in capi: a string's length plus its NUL by `saturating_add` (a length is at most `isize::MAX`), `put_text`'s copy by the string's own length, a handle's index + 1 by `saturating_add` (`next_index` never issues `u32::MAX`). compat and wasm had none |
| M24.4 | phasekit-mix denies both lints in its library code (`gates lints` lists it); its 315 sites: index arithmetic saturating, so that an index past its vector stays past it; entries read and written through `slot::{at, at2, set}`, NaN outside a vector (which the solvers' finiteness checks refuse) and a write there skipped; loops over parallel vectors as iterators with every floating-point operation in its old order, so the mixture fixtures' bits are unchanged; `solve` refusing a matrix of another size before its row swaps; the departures' index loops, kept for an unoptimised test build (SP1), as iterators now that SP2 optimises the crate, except `Series::product`'s, which index up to the last slot under an allow: as an iterator chain it cost the GERG gas's α^r a fifth more instructions, and the mixture benches 8 % on the reference machine. clippy.toml passes the operators of core's `Derivs`; the series kernels (`&Taylor`, `Series`, any `Real`) allow the lint where they compute, each naming why |
| M24.5 | phasekit-core denies both lints in its library code, so `gates lints` now lists every crate the facades reach; its 410 sites, the `simd` lanes' included: the term blocks' per-term arrays walked as zipped iterators; δ powers, polynomial and Bell-sum entries read by `get` or from slices; fixed slots named by constants (`A10`, `TAU`) and `idx` saturating, which folds to constants in the unrolled kernels; `push_term` by one slice pattern per tag, a wrong arity a `LoadError`; the blob reader's offsets by checked or saturating steps and its sections a fixed-size array; `solve_small` and `build_slots` each split for the complexity bars. The jets' product loops keep plain indexing under an allow naming the compile-time check, added here, that every `PRODUCT` triple lies below the jet's slot count; `PRODUCT` and the libm reduction's bit arithmetic are compile-time and wrapping. clippy.toml passes the operators of `Derivs`, the jets and the lanes (a type of the linted crate is named by its path inside it); generic code over `Real` allows the lint where it computes. `gates perf` within +2.6 %: `slice.iter().take(n).rev()` had cost α^r 45 % (it walks back through `nth_back`), and a zipped reverse 5 % in the jets, so both read slices instead |
| M24.6 | M24 closed: ROT-150's proof cites M24.1 to M24.5, the lints by `gates lints` (xtask `hardened_roots_deny_implicit_panics`); ARCHITECTURE.md (D17), VERIFICATION.md §11's `lints` row and §13's C ABI and wasm32-unknown-unknown rows say what holds; issue #219 closed. On the i7-8700K, in M13's boot, every bench re-recorded after M24.5, the `simd` and `libm` batch builds' included (`benches/results/M24-intel-core-i7-8700k.csv`, 119 rows): the median row 1.2 % faster than M13's (−16 % to +6 %), the mixtures' 1.9 %, and `gates perf --targets` finds 27 of §7's medians met and its one miss logged (#171). A first record found M24.4's mixture rows 8 % slower, which `gates perf` could not see (gungraun benches only pure fluids); #229 fixed that before it merged |

**Exit gate.** G1-G8; the fault-injection tests (the C ABI, the shim, and the browser package on both builds); clippy
clean with both lints denied in compat, capi, wasm, mix and core; `gates perf` within +5 %. **ROT rows.** ROT-150 (its
proof widened). **User decisions implemented.** PH1.

### M25 Lists that merge

**Goal.** Sessions working at once stop clashing over the repository's lists (user decisions LM1-LM7). Every list stays
text in git (LM1), in a form that merges: a tool's list merges itself, a register refuses an id on two rows, a decision
takes a file of its own, and a mutant exclusion sits in the code it excludes, where an edit elsewhere never moves it.
**Entry.** The user's word, 2026-10-18 (LM6): M25.1-M25.3 beside M24, M25.4 after `m24` is tagged, because M24.4 and
M24.5 rewrite the exclusions it moves. M25.1-M25.3 land in one PR, a commit and a `Plan-Step:` footer each
(LM7); M25.4 in a PR of its own. M25 does not move `MILESTONE`.

| Step | Landed |
|---|---|
| M25.1 | `cargo xtask merge-counts <base> <ours> <theirs>` (`gates::counts::merge`), git's merge driver for `ci/test-counts.txt` through `scripts/merge-counts.sh`, which `.gitattributes` names (`merge=phasekit-counts`) and CONTRIBUTING.md "Merging the test counts" defines in the repo-local config (LM3). It matches an entry by target and binary and any other line by its text: a minimum both sides kept takes the larger, unless one side lowered it, which stands where the other left it as the base had it; a line one side added joins after its predecessor on that side; a line one side removed leaves where the other left it as it was. Anything else, a version that does not parse, or an xtask that does not build mid-rebase falls back to `git merge-file`, with conflict markers. It reads and writes only the paths git passes. Tests `merge_counts_keeps_both_raises`, `merge_counts_leaves_what_it_cannot_decide_to_git` |
| M25.2 | `gates rot` refuses a ROT id on two rows, naming both lines (`a_rot_id_on_two_rows_is_rejected`); `.gitattributes` merges `docs/ROT-REGISTER.md` by union, and its §1 "Merging" says what that keeps: two rows added at one place both stay, one id on two rows (the same next id taken twice, or a row both sides edited) is the gate's to refuse, and prose both sides changed at one place keeps both versions, which only the diff shows (LM5). VERIFICATION.md §6.1 says that `check_register` already refuses a duplicate DIV id |
| M25.3 | `docs/DECISIONS.md`'s first pass and 56 sections in 56 files, `docs/decisions/<date>-<slug>.md`, named for the date their heading ends with (the first pass and its follow-ups in `2026-10-05-first-pass.md`), each heading promoted to `#` and each relative link one directory up, and otherwise unchanged: the old file rebuilds from the new ones byte for byte. `docs/DECISIONS.md` keeps its preamble and says how decisions are filed (LM4); AGENTS.md, section 2.1 and M22's text point at the folder. `gates decisions` checks the names and headings, that no id is in two rows, that `docs/DECISIONS.md` holds no decision, and that each id cited as "user decision(s) …" in a tracked file outside the frozen records is one (545 citations in 473 files) |
| M25.4 | Mutant exclusions in the code (LM2): a marker `// equivalent mutant: <mutation>` (`#k` for the k-th on its line by column) ends the comment that says why, and `gates::exclusions` resolves it from `cargo mutants --list --json` to its mutants' full names, which join `.cargo/mutants.toml`'s `exclude_re` as exact patterns in the configuration `gates mutants` passes with `--config` and `cargo xtask mutants-config <path>` writes for the weekly run. A marker with no reason above it, or a marker or a pattern of the toml that matches no mutant, fails both. The toml's 110 entries that named a file became 122 markers on 114 lines in 35 files, excluding the 131 mutants they did and the two of `departure.rs` whose pins had drifted; four entries that matched nothing were dropped, two whose code is gone (`Search::roots`' `+`, `run_wide`'s operators) and two for boundary mutants cargo-mutants 27 no longer makes (`<=` to `<` in `entropy_identity`, `>=` to `>` in `friend_chi`). The toml keeps its settings, its globs and the four patterns of xtask's `run`, `main` and `usage`. `gates mutants` diffs with `-U0` and drops the hunks that change only comments and blank lines, which change no mutant (this step's would have selected 185 whole-function mutants). Tests `a_marker_excludes_its_lines_mutant_only`, `a_marker_that_matches_no_mutant_fails`, `cargo_mutants_listing_is_read`, `the_config_adds_one_exact_pattern_per_name`, `the_configuration_comes_from_the_markers_and_the_toml`, `comment_only_hunks_leave_the_diff`. M25 closes with it; `m25` is tagged with the user's approval |

**Exit gate.** G1-G8; each step's red tests; `actionlint` clean after M25.4. **ROT rows.** None. **User decisions
implemented.** LM1-LM7.

### M26 More diagrams

**Goal.** Three sets of diagrams beyond M19's five planes (user decision DG6), computed by the library as M19's are, so
that every host draws the same geometry, and drawn on the site: a fluid's thermodynamic surface in three dimensions; a
three-component mixture's ternary diagram at fixed T and p; and characteristic curves and property maps on the planes.
Other diagrams join the milestones that need them (DG5). **Entry.** `m19`, and M21.6 landed, so that the site already
draws from `phasekit-diagram` *(decision)*; the user's word, given 2026-10-21. The surfaces come first, then the curves
and the maps, then the ternaries (DG9). **Prerequisites.** User decisions WS4, DG1-DG3 and DG5-DG11; M19's crate and
exports; M13's mixtures; M20.1's derivative variables; three.js in DEPENDENCIES.md's tier T5 (DG7). M26 does not move
`MILESTONE`.

M19's rules hold. Geometry, not drawing (WS4). `phasekit-diagram` depends on `phasekit-core` alone (T1f, DG3): what
needs another crate reaches it as data, or through a seam its caller fills, as the `Batch` seam and M19.4's `Trace` do.
Every vertex is a state the library computed, carrying its inputs. A step that needs core to change stops and re-plans
(section 0.4). **Arbiters.** As M19's: a vertex is the batch's value at its inputs (`Exact`). Each set adds its own:
the saturation states for the surface, the second virial coefficient for the curves' zero-density ends, and
`phasekit-mix`'s PT equilibrium for the ternaries. The tests run on the five embedded fluids, a pseudo-pure one, and
R407C's components.

| Step | Landed |
|---|---|
| M26.1 | `surface(fluid)` and `sample_surface(batch, phase, limits)` (`surface.rs`): a `Surface` of `SurfaceVertex`es (T, ρ, p, u, s, Q where the state is two-phase, the library's phase, the inputs), triangles counter-clockwise in (ln ρ, T), and a `SurfaceDome` of indices (the saturated liquid and vapour, the critical point, and the triple line where the plotted range starts at the triple point); `Quantity::Umass`. One `QT` call takes the dome at 40 temperatures closing in on T_c (M19's `dome_temperatures`), one `DT` call the grid: those rows, T_c's with (T_c, ρ_c), and 24 rows above T_c to the plotted range's top, each at 48 densities evenly in ln ρ from the plotted range's least (its hot low-pressure corner) to the greatest of its corners and the saturated liquid. Each row joins the next by a ladder either side of the dome and one inside it, so that the dome's edges are the mesh's. A vertex's phase is the flash's at its inputs, as M19.5's regions take it: core's batch has no phase output. A failed state is left out and its row joined across the gap; a temperature below T_c without both saturated states is left out, row and all; a grid vertex sampled at a dome vertex's density gives way to it (the span's top is n-Propane's saturated liquid at its triple point). A range that starts at T_c has no dome (`a_range_from_t_c_up_has_no_dome`). Re-planned in the step: the dome's vertices are `QT` states, so `every_vertex_is_the_dt_state` holds each vertex to the flash at its own inputs, `DT` or `QT`; a pseudo-pure fluid's two-phase `DT` states are its EOS's own VLE, not its bubble and dew curves, so `the_two_phase_region_is_the_saturation_pressure` and `a_two_phase_vertex_lies_between_its_saturated_states` run on the five pure fluids, the rest of `tests/surface.rs` on them, Air and R410A: `the_dome_is_an_edge_of_the_mesh` (with no triangle across the dome, each counter-clockwise, every vertex a triangle's), `the_mesh_spans_the_plotted_range` and `a_failed_state_is_left_out_and_the_mesh_joined_across` |
| M26.2 | The browser package's `Fluid.surface()` (`crates/phasekit-wasm/src/surface.rs`): phasekit-diagram's mesh by the batch in lanes as typed arrays, each vertex's `T`, `rho`, `p`, `u`, `s` and `Q` (NaN where it is not two-phase), its phase as `phaseNames[phases[i]]`, its `inputs` and `pairs`, the `triangles` (three indices each) and the dome's `liquid`, `vapour`, `critical` and `triple` indices; a mixture's refused. `a_fluids_surface_is_the_crates_bit_for_bit` and `the_surface_is_typed_arrays` on both builds. The site's `#/surfaces` page (header link "Surfaces"): the engine's `surface` op; `src/surface/mesh.ts` places the mesh in a cube, each axis's span scaled to [−1, 1], a logarithmic one in its logarithm: p–v–T stands p up over v and T, p and v logarithmic; u–s–v stands u up over s and v, v logarithmic (its 5.4 decades for Water would leave the liquid a sliver otherwise); the deep axis runs away from the viewer, so the cold side, with the dome, is in front. `src/surface/scene.ts` draws it with three.js 0.186.1 (DG7; `OrbitControls` to turn and zoom it, a reset button), lit, each triangle one phase's colour (`trianglePhases`: the phase most of its vertices off the dome have, so that no phase bleeds into the next across a triangle; a boundary that is no mesh edge, at p_c or T_c, follows the grid), with new `--surface-<phase>` tokens (the region tints' hues at mid lightness: the tints are too pale to show a lit surface's folds), the dome as a line through the critical point, which is a marker, the cube's edges, and three of them as axes, ticked at their decades (10ᵏ) or round values and titled, the labels HTML in the scene (`CSS2DRenderer`); under the cursor, the point on the surface is marked and read as the package's state at its (T, ρ), T linear and ρ in its logarithm across its triangle (`stateAt`), in a readout card of T, p, ρ, u, h, s, Q and the phase, as the diagrams' readout reads the state at the cursor; it is browser-only glue, which Playwright drives and `scripts/site/mutants.sh` leaves out as it does the engine worker. Beside the plot, each axis's quantity, scale and span, and the phases present. The view (`src/surface/view.svelte.ts`) holds the fluid and the surface in the URL (`fluid=Water&kind=usv`) and asks for a fluid's mesh once. The page's budget rose to 271 520 B (246 836 measured, + 10 %), three.js about 137 kB of the growth, all in the page's own chunk. n-Propane's two-phase tongue, out to 10⁻⁸ kg/m³, is drawn whole. Tests: `tests/unit/surface.test.ts`, `surface-view.svelte.test.ts` and the engine's; `tests/e2e/surfaces.spec.ts` (red against the deployed site, which had no such page): the surface drawn over a tenth of the canvas, both surfaces, a drag turns it and the reset brings it back, the header link, the axes' labels, the readout's values the package's at its (T, ρ) bit for bit, and a mixture refused in the package's words |
| M26.3 | `curves(fluid)` and `sample_curves(batch, limits)` (`curves.rs`): `Curves` holds the four `Curve`s in `Characteristic::ALL`'s order (the ideal curve, Z = 1, and Brown's Boyle, Joule–Thomson inversion and Joule inversion curves), each its lines of `CurveVertex`es (T, ρ, p, Z and the `DT` inputs), from their lower-density ends, and `zero_density`, the temperature where its virial condition is zero; and the compressibility chart, Z against p on `Isotherm`s. Re-planned in the step (user decision DG10): the curves reach zero density only at 2.3-4.9 T_c, above the planes' 1.6 T_c, so they are sampled from the plotted range's lowest temperature to the equation of state's highest, on a `DT` grid of 64 temperatures and 64 densities, each evenly in its logarithm, the densities from the least of the plotted pressures' corners at those two temperatures to the greatest of them and the saturated liquid's. A curve's condition is dimensionless and of its derivative's sign: Z − 1, (ρ/p)(∂p/∂ρ)_T − 1, (p/T)(∂T/∂p)_h and (T/p)(∂p/∂T)_ρ − 1, from new `Quantity` outputs (`Z`, `DpDrhoAtT`, `JouleThomson`, `DpDTAtRho`, and `Bvirial`, `DBvirialDT` for the zero-density ends; `Quantity::var` is `None` for them), and `None` at a two-phase state. `contour.rs`, which M26.4 reuses, finds where a field crosses zero: marching squares over the grid's cells (a saddle split by its centre), each crossing bisected 40 times along its edge, an isotherm or an isochore, at the geometric mean, one batch call a level, its vertex the end nearer zero; an edge with one end in a gap is first halved toward it 6 times, M19.3's depth, so that a line runs to within 2⁻⁶ of a grid step of a gap (R410A's ideal curve runs along its 50 MPa p_max, which split it into four pieces before). A curve's zero-density end bisects its virial condition (B; T dB/dT − B; dB/dT) on the grid's temperatures at its least density, the lowest root. The chart's isotherms are at the plotted temperatures' nice values (M19's), across the plotted pressures from 32 points, refined as M19.3 refines a line in (ln p, Z), and split below T_c at the saturated vapour and liquid (`QT` states), between which nothing is drawn. `tests/curves.rs`, on the five pure fluids, Air and R410A: `each_vertex_meets_its_curves_condition` (each vertex the flash's at its inputs bit for bit, its condition within 10⁻¹⁰ by the library's own derivative there, measured 1.1e-12; the ideal, Boyle and Joule–Thomson inversion curves cross every fluid's range); `the_curves_zero_density_ends_are_the_virials` (the condition changes sign within 10⁻¹⁰ of the end, where CoolProp 8.0.0's virials put one below T_max: the ideal and Boyle curves' for Water, CarbonDioxide, Nitrogen and Air, the Joule–Thomson inversion curve's for the last three, the Joule inversion curve's for none, whose condition keeps its sign there; the lowest-density vertex within ρ/ρ_c of the end, relatively, the virial series' first-order term, whose coefficient the fluids hold below 0.6); `z_is_p_over_rho_r_t` (each isotherm's vertex the flash's bit for bit, Z within 10⁻¹³ of p/(ρRT), in order of pressure, below T_c the saturated vapour and then the liquid); `a_line_ends_where_the_batch_fails`. 49-115 ms a fluid in release |
| M26.4 | `map(fluid, plane, quantity)` and `sample_map(batch, plane, quantity, limits)` (`maps.rs`): a `Map` of any batch output over one of M19's planes (`Quantity::Output(Prop)` names any the other variants do not), its `Contour`s at the quantity's nice values strictly inside its span over the grid (M19's `values`: 1-2-5 per decade for p and ρ, d3's ticks for the rest), each its lines of M19 `Vertex`es, and its `Band`s, one more than the levels, each every grid cell's piece of it as a polygon whose corners are the cell's corners inside the band and its bounds' crossings between, in order around the cell (user decision DG11: the site fills them and computes no geometry); `values`, the quantity's span over the grid, and the plane's `x` and `y`, its diagram's (`axes_spans`, factored out of `sample`). The grid is 64 values of each axis across its diagram's span, evenly in its scale, by one call of the plane's input pair; `contour.rs` takes any pair (`Axes`, each input halved at the geometric mean on a logarithmic axis and the arithmetic one on a linear axis) and gains `Crossings`, shared by a contour's lines and its bands' polygons, and `band`. Each crossing is bisected until it is within an eighth of `LEVEL` (10⁻⁶) of the quantity's span from its level, at most 40 times, and one farther than `LEVEL`, on a jump (h across the saturation line on p–T), is left out, so that the line ends there; a gap's edge is closed in on once for every level (`less`). A cell with a corner without a value, or whose piece needs a crossing that was left out, has no piece; at a saddle the piece is one polygon around the centre. `tests/maps.rs` on Water's T on p–h, R410A's s on p–h, CarbonDioxide's h on p–T and n-Propane's p on T–ρ: `each_contour_vertex_is_its_level` (each contour and polygon vertex the flash's at its inputs bit for bit, a contour's within an eighth of `LEVEL` of its level, where its bisection stops, measured 1.2e-7 of the span, a polygon's within its band); `a_contour_closes_or_ends_at_the_range_or_a_gap` (n-Propane's p over T–ρ ends on the range; with its states from 300 K to 320 K failed, each line that crossed them split, ending within a grid step of them). Measured in release: n-Propane's p on T–ρ 51 ms, CarbonDioxide's h on p–T 114 ms, Water's T on p–h 283 ms, its ρ on T–s 1.6 s and its speed of sound on h–s 11.6 s, 8.2 s of it the grid's 4,096 `HS` flashes, 58 % of them refused at about 3 ms each |
| M26.5 | `ternary(batch)` (`ternary.rs`): a three-component mixture's `Ternary` at one T and p, in mole fractions: the `binodal`'s lines of `BinodalVertex`es, its `tie_lines` (`TieLine`, a feed and its `Split`: β, the liquid's and the vapour's fractions) and, added in the step so that the site fills the region and computes no geometry as DG11's bands have it, the two-phase `region`, each lattice triangle's piece as a polygon of the corners that split and the binodal's crossings between, counter-clockwise. The equilibria reach the crate through `TernaryBatch` (its `t`, `p` and `phases` of a batch of feeds, each `Phases::Single` or `Phases::Split`, `None` where it fails), which the browser package fills from `phasekit-mix` (DG3's rule); the tests fill it through `phasekit-mix`'s `Mixture::equilibrium`, the tangent-plane stability test and the split, a dev-dependency only, a feed with a zero fraction as its binary mixture or its pure component. The feeds lie on a lattice of 24 steps a side, corners and binary sides included (325 feeds, 576 triangles), classified in one call; each lattice edge whose ends' verdicts differ is halved 6 times, M19.3's depth, one call a level, and the binodal crosses it at its bracket's split end, within `RESOLUTION` (1/1536 in each mole fraction, to rounding) of the feed outside, which the vertex carries; marching triangles join the crossings into lines with M26.3's `chains`, now generic over the edge. A feed whose equilibrium fails leaves its triangles out and ends the lines there. The tie lines are the lattice's every third feed along each of its first two fractions that splits. `tests/ternary.rs` on R32, R125 and R134a at 273.15 K and 5 bar, where R407C splits (β 0.63): `each_tie_line_is_the_flash_of_a_feed_on_it` (each tie line's and binodal vertex's split the flash's bit for bit, the feed on it by the lever rule within `Flash`); `the_binodal_is_where_the_stability_test_changes` (two lines, the bubble and dew sides, each from the R32–R134a side to the R125–R134a side, each vertex's outside feed one phase within `RESOLUTION`); `r407c_sits_where_its_flash_says` (inside a piece of the region at 5 bar, and at 4 bar, where its flash leaves it one phase, in none). A diagram takes 1.4-2.3 s in release (the i7-8700K, beside another session's gates): a split feed's equilibrium about 5 ms, a single one's 1.4 ms |

- **M26.6 Curves, maps and ternaries on the site, and M26 closed.** The browser package exports M26.3-M26.5 as typed
  arrays. The site draws the curves as an overlay on the p–T and T–ρ planes, the parts inside their range, and whole on
  a view of their own, log p against log T to the equation of state's highest temperature (user decision DG10), with
  the compressibility chart; the maps as their bands' polygons, filled, under their contour lines (DG11); and a ternary
  view for a three-component mixture. The size budgets and the page's are re-measured (SL8). *Red:* the package's Node
  tests through the exports on both builds, and the site's tests, each run first against the page without them. Tag
  `m26` with the user's approval.

**Exit gate.** G1-G9 at every step; every vertex a state the library computed at its inputs (`Exact`), and each set's
arbiter as its step states it; `gates deps` holds `phasekit-diagram` to core; the browser package's budgets and the
page's hold. **ROT rows.** None. **User decisions implemented.** DG5-DG11.

### M28 Error messages

**Goal.** Issue #191: core's error messages name SI values without their units, and every facade passes them on
verbatim, so a reader who entered °C and bar reads `invalid temperature: 0`. Core keeps the wording, in two styles, and
writes each number in the units its caller supplies (user decisions EM1, EM2): the long form in words, for a
sentence's room, the short form stating the relation the value breaks, for a UI's narrow places, both in Unicode
(EM6). Core cannot know a viewer's units (its API is SI, D5, and it keeps no global state), so a caller passes them;
`Display` is SI. **Entry.** The user's word, 2026-10-21. M28.1 alone, since it changes the core, after M18.10 lands and
before M18.11 (EM5): it converts humid air's refusals too, and `m18-start` is tagged at its merge, with the user's
approval (EM13), and the core frozen from it to M18's close. M28.2 and M28.3 in one PR, a commit and a `Plan-Step:` footer
each (LM7), beside M18.11 and M18.12: they touch no core. **Prerequisites.** User decisions
EM1-EM13. M28 does not move `MILESTONE`.

The wording, which M28.1's PR shows in full for the user's review (EM2, EM7-EM10):

| Error | Long | Short |
|---|---|---|
| T = 0 | `temperature 0 K is not above absolute zero` | `T = 0 K ≤ 0 K` |
| p = −5 Pa | `pressure −5 Pa is not positive` | `p = −5 Pa ≤ 0 Pa` |
| Q = 1.5 | `quality 1.5 is outside [0, 1]` | `Q = 1.5 ∉ [0, 1]` |
| T = NaN | `temperature is not a number` | `T = NaN` |
| below T_min | `temperature 200 K is below the minimum, 273.16 K` | `T = 200 K < T_min = 273.16 K` |
| above p_max | `pressure 2e9 Pa is above the maximum, 1e9 Pa` | `p = 2e9 Pa > p_max = 1e9 Pa` |
| below the melting line | `temperature 250 K is below the melting temperature at this pressure, 251.2 K` | `T = 250 K < T_melt = 251.2 K` |
| ambiguous | `several states fit these inputs: T = 300 K or 350 K` | `T ∈ {300 K, 350 K}` |
| undefined | `the isobaric heat capacity (Cpmass) is undefined for a two-phase state` | `Cpmass: undefined, two-phase` |
| no convergence | `the density Newton iteration (DensityNewton) did not converge in 50 iterations` | `DensityNewton: no convergence in 50 iterations` |

In °C a temperature's zero limit is written in °C (`T = −273.15 °C ≤ −273.15 °C`), so the long form says "absolute
zero", not "positive".

| Step | Landed |
|---|---|
| M28.1 | `phasekit_core::{Quantity, Dimension, Unit, Units, Si, Style, Message}` (`quantity.rs`, `error/message.rs`): a `Quantity` is a name, a symbol and a `Dimension`, held by an error as `&'static Quantity` so that `Error` keeps its 48 bytes; core's `TEMPERATURE`, `PRESSURE`, `QUALITY`, `MOLE_FRACTION`, `MOLAR_MASS` and, by basis, `density`, `enthalpy`, `entropy` and `internal_energy`, `Var::quantity(basis)`; the dimensions humid air refuses (a fraction in [0, 1], a mass ratio, a molar mass, a specific volume). `Error::InvalidInput { quantity, value }` and `Error::InvalidArgument { argument, value: Option<usize> }` for counts, indices, planes and mixtures (EM3), `Status::InvalidInput` for both; `Roots::new(quantity, roots)`, NaN the unused slots. `Error::display(&units, style)`: each number in the caller's unit of its dimension, six significant figures (EM4), a decimal from 1e-4 to below 1e6 and otherwise `2e9`, U+2212 for minus, NaN and ∞ by name; `{}` the long form in SI, `{:#}` the short (EM2, EM6). An `InvalidInput` names the first rule its value breaks (not a number, not finite, not above absolute zero, not positive, outside [0, 1]), else "out of range", so that a message never names a rule the value keeps (EM8). A property, a constant or a strategy in the long form in words beside its key, `the isobaric heat capacity (Cpmass)`, a derivative after "the derivative", in the short form by its key (EM7; `error/words.rs`). `Error::code()` and `DomainError::code()`, a stable snake_case code for each variant (`invalid_input`, `below_min_temperature`, …, the browser package's), and `Error::rule()`, the `Rule` an `InvalidInput`'s value breaks, with `Rule::code()` (`not_positive`, `out_of_range`, …), so that a facade, an agent or a solver reads them instead of the message (EM11; `error/code.rs`; the other findings of that review are issue #290). An unsupported pair's long form stays `input pair X is not supported by this model`, which the site's `liquidLiquid()` reads (MX4). `phasekit_mix::quantities` and `phasekit_humidair::quantities` hold those crates' own (humid air's h, s and v per kilogram of dry air, the dry air in the name, not a basis, and the wet-bulb temperature among them, for M18.1-M18.10's refusals); every crate that builds an error converted; D12 says how an error is written; `ci/core-frozen.txt` names no tag; the site's two pins of `invalid temperature: 0` pin `temperature 0 K is not above absolute zero`. *Red:* `every_message_in_both_styles` (21 errors, both forms, pinned), `messages_take_the_callers_units` (°C, bar, kJ/kg: `T = −273.15 °C ≤ −273.15 °C`, `p = 20000 bar > p_max = 10000 bar`), `the_alternate_flag_is_the_short_form`, `structural_arguments_carry_no_unit`, `derivatives_and_constants_by_name`, `roots_carry_their_quantity`; `a_rule_the_value_keeps_is_never_named`, `every_error_has_its_code`, `the_rule_a_value_breaks`; `six_significant_figures` and its neighbours in `quantity.rs` |
| M28.2 | A refused call of the browser package throws a JS `Error` (`refusal.rs`) whose message is core's long form in SI and which carries `code` (core's `Error::code()`, `invalid_input`, `below_min_temperature`, …, and compat's own `unknown_output` and its kin), `rule` (an invalid input's, core's `Rule::code()`, `not_positive`, …; EM11), `quantity`, `dimension` (the site's kind names, `temperature`, `specificEnergy`, and `fraction`, `massRatio`, `specificVolume`), `values` (a `Float64Array`, SI) and `refusal`, a `Refusal` handle whose `explain(units, style)` writes core's wording in `units`, `{key: {label, scale, offset}}`, a key it lacks in SI, `style` `"long"` or `"short"`, anything else refused by name; the same from a state's `get`, from humid air's calls (`humidAir`, `HAPropsSI`, its charts and states) and from a fluid's `surface`, which M18 and M26 added. No js-sys: its declarations of every JS built-in added 0.44 MB to the module `gates features` measures, so `refusal.rs` declares `new Error`, `Reflect.get` and `Reflect.set` itself; the module grew 18.1 KB over M28.1's (2427322 bytes baseline, 2382600 simd128, within #291's budgets). The npm README says how, and to act on the fields, not the message. The C ABI and compat expose the same codes (EM12): `CompatError::code()`, core's for a kernel error and compat's own for the rest, and `CompatError::rule()`, which the browser package reads instead of its own copy; `pk_last_error_code` and `pk_last_error_rule` copy out the thread's last code and rule as `pk_last_error` does its message, the ABI's own refusals by their status (`null_pointer`, `stale_handle`, …), both empty after a success, beside `pk_last_error` in `gates abi`'s uncaught exports. *Red:* `every_refusal_has_its_code` (compat), `the_last_error_has_its_code_and_rule` (capi) and `smoke.c`'s code and rule checks, which did not compile before; `tests/refusals.rs` under Node on both builds, `a_refusal_is_an_error_with_its_fields`, `explain_writes_the_callers_units` (`T = 426.85 °C > T_max = 373.946 °C`), `an_absent_unit_is_si`, `explain_refuses_what_it_cannot_read`, all four red against M28.1's package |
| M28.3 | The site's engine worker explains each refusal once per unit system the page offers, in both styles (`site/src/lib/refusal.ts`, `refusalOf`), and frees the package's handle; a refusal crosses to the page as `{message, explained}` (`Refusal` in `protocol.ts`), an `EngineError` carrying it, so a change of units asks the worker nothing. The calculator writes its refusal in words in the shown units (`refusalText`), each refused output short (`values.errors`), and the diagrams' readout under the cursor short; the surfaces page (M26.2), which is in SI, its refusal in words and its readout's short; #190's restatement of the inputs in SI is dropped, since the message names them in the shown units. A mixture's refused phases keep the message. Issue #191 closed. *Red:* `tests/unit/refusal.test.ts` (the package's own refusal in both systems and styles, the handle freed even when `explain` throws, a kept refusal, a bare one), the calculator's `a refusal is the package's, in words and in the shown units` and `a refused output is the package's short form in the shown units`, and the end-to-end `a refusal is the package's, written in the viewer's units` (`temperature −273.15 °C is not above absolute zero`), red against the page as it was |

**Exit gate.** G1-G9 at every step; every message pinned in both styles; `gates core-frozen` since `m18-start` from
M28.2 on while M18 is open; `ci/core-frozen.txt` names no tag once M18.12 closes it. **ROT rows.** None. **User
decisions implemented.** EM1-EM13.

## 5. Cross-cutting tracks

### 5.1 Performance recording

Each path is benched as it lands (criterion; gungraun from M9.6) against the targets of ARCHITECTURE.md §7;
workloads, bench fluids, the harness and the result files are VERIFICATION.md §12. Non-blocking until M9; from M9.6
`gates perf` enforces gungraun regressions on Linux CI, and the reference machine's record is checked against the
targets, a miss logged rather than blocking (user decision TG1).

### 5.2 Documentation

- Rustdoc on every public item (`missing_docs`, enforced by G2); CI's linux job runs `cargo doc --workspace --no-deps`
  with `-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links` (M20.3); the crate-level doctest of
  `phasekit-core` executes (the seed's does).
- `docs/`: each fact stays in its one home (section 2.1); a step that changes one edits that home in the same PR (M9.5,
  for example, records the `libm` outcome in D12), and collapses its own step text (section 0.1 item 7). Rust code blocks in `docs/*.md` are either verbatim excerpts of the workspace or marked
  `<!-- excerpt: illustrative -->` (`gates doc-excerpts`, ROT-139); prefer a link to the file.
- M10 adds `docs/MIGRATION.md`: the CoolProp → phasekit table of ARCHITECTURE.md §12 with runnable examples, the
  shim's documented differences (`HAPropsSI` stub, `set_config_*` behaviour, FP flags) and the divergence list.
- `CHANGELOG.md` from M0, generated at each release from the Conventional Commit subjects since the last tag
  (section 2.2).

### 5.3 Licensing, REUSE, supply chain

- Code MIT OR Apache-2.0 (M0.7). `phasekit-data` and `LICENSE-THIRD-PARTY` carry CoolProp's MIT notice; NOTICE
  credits NIST/fastchebpure and cites the superancillary paper (M2.5, M5.2; user decision 3a). `phasekit-mix` is
  `(MIT OR Apache-2.0) AND LicenseRef-NIST-GERG` and ships NIST's notice as its `LICENSE-NIST-GERG` (user decision MX10).
- Core's lane `exp`, `ln` and `expm1` port the `libm` crate's (MIT) from FreeBSD's msun (Sun's notices):
  `LICENSE-THIRD-PARTY` and `REUSE.toml` carry both. After the `m13` tag, before any crates.io publish, the three files
  take their msun notices at their heads and `crates/phasekit-core/NOTICE` both notices (user decision LC1).
- Every fixture and blob has a REUSE annotation with its provenance: CoolProp-derived, paper (citation), or
  fastchebpure (user decision 3b). `reuse lint` runs in CI from M0.7.
- `Unpublished` data (Propylene, SES36, Neon; map 13 R7) get oracle-only fixtures, labelled (M2.10). Restricted data
  (Ethanol-Water "from REFPROP with permission", the DTU table, INCOMP sheets; map 09 §9) never enters default
  features. No GPL/LGPL/MPL code is copied (D14).
- `cargo deny check` on every PR and daily for advisories (`bincode`, `serde_cbor` banned; `once_cell`, `lazy_static`
  banned as direct dependencies); `cargo shear` on every PR; `Cargo.lock` committed. Each new dependency names its
  tier (DEPENDENCIES.md) in its step. `cargo about generate` for the capi and wasm bundles from M10.

### 5.4 v0.1 release checklist (M10.9)

1. All M0-M10 exit gates green on `main`; the nightly sweep green on `main`.
2. Register: every entry has its proof test; no `Investigate` entry hides a failing assertion.
3. ROT-REGISTER.md: `gates rot` passes with `MILESTONE = 11`; no row is GAP.
4. `cargo semver-checks` baseline recorded (first release: establishes it).
5. `reuse lint`, `cargo deny check`, `cargo about generate` for the C library and wasm package; licences in each
   artefact; `cargo package --list -p phasekit-data` shows only blobs, the index and the notice (ROT-155).
6. Versions 0.1.0 across the workspace; `phasekit-data` carries its `DATASET` id; CHANGELOG and MIGRATION.md final.
7. Artefacts: crates core, data and compat (`publish = true` for these three only; `phasekit-verify` stays unpublished
   because it holds the benches, dependencies §2.12, and family authors use it as a git dev-dependency
   *(inference)*), the C library and header (`pk_*` and the separate shim build under the agreed file name), the npm
   package. Published under the `Dzoay` accounts (user decision P5): crates.io Trusted Publishing after the first manual
   release (R19), npm trusted publishing from the release workflow; no long-lived tokens in the repository.
8. Tag `v0.1.0` when the user says, on the commit they name, with the CHANGELOG brought up to date first (user
   decision RL4); release notes list the user decisions in force and the known divergences.

## 6. Resolved follow-ups and remaining user decisions

Every decision, and the steps it applies to, is in [DECISIONS.md](DECISIONS.md). This section lists only what is
still open; an item leaves it when a step or a decision takes it.

**Open user decisions.** P4: the paywalled EOS papers (Ammonia, CO₂, R125, Air, Nitrogen, HFE143m). Until a paper
arrives its fluid stays oracle-only, labelled provisional; nothing waits on it.

**Logged future work** (user decision TG1). The goals relaxed for now, each in an issue labelled `future-work` with its
measurement: #143, PT + h ≤ 3 µs (the target is PF2's); #144, α^r ≤ 0.3 µs and properties at (T, ρ) ≤ 0.5 µs (PF3);
#146, the superancillary ≤ 25 KiB and all 136 loaded ≤ 8 MiB RSS (MB1); #170, the `libm` backend's known differences
(LB1); #171, α^r ≤ 0.1 µs/state on AVX2 (M12.7).

**Waiting for a paper.** Each item moves when a copy reaches `reference/papers/`.

| Item | Paper | Then |
|---|---|---|
| Methane's and n-Pentane's friction-theory rows (user decision FT1) | Quiñones-Cisneros and Deiters, J. Phys. Chem. B 110, 12820 (2006) | transcribe its check values or coefficients, check CoolProp's two REFPROP-substituted n-Pentane coefficients, add the rows to `viscosity_paper_rows` |
| q_D of R13, R14, R142b, R218, RC318 (DIV-0026) | Huber et al., IECR 2003 | `SkipOracle` becomes `UsePaper` |
| R123's c_p⁰ block `Tc` (DIV-0015) | Younglove and McLinden 1994 (P4) | the entry decides Corrected |
| Stored constants of map 13 R1's 15 candidates | each EOS paper | an `Investigate` entry once a paper value is in hand |
| The order-3 identities of Thorade and Saadat 2013 (user decision OD5) | not open | M7.10 checks the identities VERIFICATION.md §8.1 lists |
| The Methane/Ethane critical locus (M13.7) | Bell and Jäger, Fluid Phase Equilib. 433 (2017) 159: free to read at the publisher, which refuses scripted downloads | the locus arbitrates M13.7's critical points; until then teqp's trace and the oracle check them |
| The CoolProp rows of R1132(E) and VinylChloride | CC BY, but Springer refuses scripted downloads | re-check them against their papers |
| RP-1485's C_aaw and C_aww (DIV for ROT-271, M18.1) | Nelson and Sauer (2002), which RP-1485 cites for both, or the RP-1485 report (ASHRAE) | the `KeepOracle` entry decides `Corrected` |
| RP-1485's appendix tables: virials, f, κ_T and Henry's constant from −60 to 350 °C (user decision HA2) | the RP-1485 report (ASHRAE; the free copy is its first seven pages) | transcribe them; they then arbitrate M18.1 and M18.2 beside the IAPWS releases |
| The authors' h, 2.29 to 2.47 J per kg of water below `Corrected`'s, and Table 8's s (DIV-0071, user decision HA8, M18.3) | the RP-1485 report (ASHRAE) | its cause decides the `Investigate` entry: a fix under `Corrected` (`UsePaper`), or the bounds stay |
| Humid air's transport (ROT-262, M18.5) | Tsilingiris, Energy Convers. Manag. 49 (2008) (map 07 §3.3; pages unverified) | check v8.0.0's water-vapour state (map 07 H2) and the mixing rule; `Corrected` then follows the paper |
