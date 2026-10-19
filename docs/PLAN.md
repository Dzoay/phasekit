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
| IF97, ice Ih, solids | Gibbs families arrive through the seam proved at M5 (D10) | M14 |
| INCOMP, humid air (real `HAPropsSI`) | Own `ThermoModel`s; humid air needs water, air and ice models (map 07 §9) | M17, M18 |
| SIMD lanes | Investigated in a time-boxed spike, shipped if it is faster than the scalar path (D9; Kernel's fatal flaw; user decision TG1) | M12, which may land before the `v0.1.0` tag as an opt-in feature (user decision RL4) |
| Python | After v0.1, own namespace + `compat` (user decision 7) | M16 |
| Plotting | After the mixtures milestone (user decision 13); diagram geometry, drawn by the site (user decision WS4) | M19 |
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
      ├─ M13 mixtures ────── M19 diagram geometry
      ├─ M14 IAPWS, ice ──── M18 humid air
      ├─ M21 site: M21.1, M21.7, M21.2, M21.8, M21.9, M21.3, M21.4, M21.10 landed, M21.5 after M13, M21.6 after M19
      └─ M12 SIMD spike, M16 Python, M17 INCOMP; on demand: Tier B shim, WIT, network service, materials
```

| M | Crate | Entry trigger | Scope | Exit gate |
|---|---|---|---|---|
| M11 | `phasekit-cubic` | The user's word, before `v0.1.0` (user decision CB2) | vdW/SRK/PR via Δ1/Δ2, Soave/MC/Twu alpha, parameters from the canonical fluid records, borrowed HEOS ideal gas, cubic superancillary (map 06 U1-U5), eight steps: [M11 Cubic family](#m11-cubic-family) | Zero-line core diff; `new_family` conformance; cubic-R register entry (below); DIV-0010 proof by the entropy identity (oracle PR propane 400 K, 1 bar: T(∂s/∂T)_p = 91.35 vs cp = 93.89 J/mol/K; map 12 §6.3); Z_c = 1/3 (SRK), 0.30740 (PR) (map 06 §8) |
| M12 | core `simd` feature | Built from 2026-10-13 in a worktree; merged when green, before the `v0.1.0` tag if it is ready first (user decision RL4) | A time-boxed SIMD spike (the user's follow-up asks to investigate SIMD), ten steps: [M12 SIMD lanes](#m12-simd-lanes) | ≥ 2.5× α^r batch throughput on AVX2 over the scalar path is the target (user decisions SL3, TG1); the feature ships if it is faster than the scalar path, otherwise the spike's measurements are recorded and the stack closed (D9) |
| M13 | `phasekit-mix` | The user's word, before `v0.1.0` (user decision MX1) | `Mixture: HelmholtzModel` at fixed z, own `ThermoModel` (stability, VLE, critical points, phase envelopes, `from_split`), `MixtureModel` with AD composition derivatives, GERG-2008 (`GERG2008::`, from NIST's AGA8 code), compat `A[x]&B[y]` in Rust, C and the browser, true-mixture variants (`R410A.mix`) registered beside the pseudo-pure names (D4, user decisions 4, MX2-MX7), sixteen steps: [M13 Mixtures](#m13-mixtures) | One core seam step (M13.1, MX2), then the core frozen from `m13-start`; x = [1, 0] is the pure fluid under any gauge (map 04 §8, map 15 §8); GERG teqp vectors and the AGA8 check state (map 06 U7); Bell 2023 Table XIII; DIV-0013 resolved |
| M14 | `phasekit-iapws` | v0.1 + a water/steam or solid use case | IF97 regions 1-5 with backward equations, ice Ih (R10-06) and sublimation (R14-08) on `bundle_from_gibbs`, `from_split` for ice + vapour (map 07 §9) | **Zero-line core diff**; IAPWS tables first, oracle second; `gibbs_seam` conformance on real models |
| M15 | `phasekit-pcsaft` | M11 done + demand | Hard chain, dispersion, association, polar terms on `Jet4` (map 06 U9-U11) | Zero-line core diff; teqp/FeOs cross-checks; oracle only where map 06 §8 says it is usable |
| M16 | `phasekit-py` | v0.1 + demand | pyo3 abi3 wheels, import name `phasekit`, `phasekit.compat.PropsSI(...)`, GIL released around kernel calls, NumPy `(N, M)` point-major (user decisions 7, 11; R16) | map 14 U1 Python-contract fixtures for the compat surface; installs beside `CoolProp` |
| M17 | `phasekit-incomp` | v0.1 + demand | INCOMP fits as a `ThermoModel`, placeholders rejected at datagen, `Example*` fluids excluded, restricted data off by default (map 07 §9; D14) | Source tables; known-bad register (map 07 I1-I14) |
| M18 | `phasekit-humidair` | M14 done (ice) | RP-1485 humid air on injected Water, Air and ice `Fluid`s (map 07 H3); HA strings in compat; the shim's real `HAPropsSI` replaces the stub; `pk_*` gains humid-air symbols (D11) | RP-1485 tables (map 10 §8.1, ~120 values); `HAPropsSI("H","T",300,"P",1e5,"R",0) = 27013.112479771713` (map 11 §8) |
| M19 | a diagram-geometry crate (T1f), named at its plan | M13 done (user decision 13) | Geometry, not drawing (user decision WS4): saturation domes, isolines of T, p, h, s, ρ and quality on the p–h, T–s, p–T, T–ρ and h–s planes, split at the dome and at failed cells, sampled more densely where a line bends, with labels and ranges; M13's phase envelopes; over saturation, `flash` and batch; the browser package serves it to the site (M21.6) | Every vertex reproduces its line's value within `Flash`; a line's dome crossings are the saturation states at its value (`Exact` against `QT` or `PQ`); M21.2's checks pass on its geometry; the site's screenshots re-approved at M21.6 |
| M20 | core, compat, verify, CI | M12 landed (user decision PL1) | Four follow-ups: Cv, Cp, w, τ and δ as derivative variables; the conformance kit's `identities` and `part_additivity`; the intra-doc link check; the nightly report: [M20 Follow-ups](#m20-follow-ups) | Each step's checks; G1-G8 |
| M21 | `site/`, the GitHub Pages site | The user's word, 2026-10-16, beside M13 (user decision WS3) | A single-page site over the browser package: a calculator, five diagrams with the state under the cursor, sweeps, an in-browser benchmark of both builds, all 136 fluids from per-fluid packs, mixtures after M13, M19's geometry after M19, mutation testing of the site's code, picking states on the diagrams, phase regions, how a property is calculated; ten steps: [M21 The site](#m21-the-site) | G9 at every step; the page deployed from `main`; every value on the page equals the package's (`Exact`) |
| M22 | `phasekit-mix`, core `batch` and `ThermoModel` | After M13 closes (user decision ML2) | Mixtures in lanes: α^r and α⁰ batches at fixed z through the components' lane kernels with the departures in lanes, hinted DT batches through a group hook on `ThermoModel`, the `simd128` build, then a time-boxed spike putting a flash's two phases and its stability test's trial phases in lanes; core runs a family's kernels over `Real` in its private lanes (user decisions ML1, ML3, ML5), eight steps: [M22 Mixture lanes](#m22-mixture-lanes) | Each kernel ships if faster than the scalar path (ML4); `policy_equivalence` bitwise at every dispatch level; G1-G9, and G1-G8 with `--features simd` |
| M23 | core `math` | After M13 closes (the core is frozen until `m13`, MX2); independent of M22 | In-house `exp`, `ln`, `pow` and `expm1` under 1 ulp, ported from Arm's optimized-routines, one code for the scalar path and the lanes; every build's maths if they pass M9.5's rule, so `simd` leaves `libm`; then an opt-in `fast` feature with FMA on every target (user decisions IM1-IM5), eight steps: [M23 In-house maths](#m23-in-house-maths) | Under 1 ulp on the reference values (IM3); lanes bitwise at every dispatch level; each committed hash one across the four targets; M9.5's rule applied and recorded (IM1); G1-G9, and G1-G8 with `--features simd` and with `--features fast` |
| M24 | compat, capi, wasm, mix, core | The user's word, 2026-10-18, before `v0.1.0` (user decision PH1) | Panics at the facades (issue #219): fault injection along every C export's path, the browser package's panic hook, and clippy's `indexing_slicing` and `arithmetic_side_effects` denied in the crates it reaches: [M24 Panics at the facades](#m24-panics-at-the-facades) | The fault-injection tests; clippy clean with both lints; `gates perf` |
| M25 | xtask, `.gitattributes`, docs | The user's word, 2026-10-18, beside M24; its mutants step after `m24` (user decision LM6) | Lists that merge, kept as text (LM1): `ci/test-counts.txt` by a merge driver, ROT ids unique and the register merged by union, one file per decision section, mutant exclusions as markers in the code: [M25 Lists that merge](#m25-lists-that-merge) | Each step's red tests; G1-G8; `actionlint` clean |
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

### M20 Follow-ups

**Goal.** The four follow-ups PLAN.md §6 had left unplanned (user decision PL1). **Entry.** After M12 lands; the steps
are independent and may be reordered. M20 does not move `MILESTONE`.

- **M20.1 Cv, Cp, w, τ and δ as derivative variables.** M10.3's strict grammar `d(X)/d(Y)|Z` and
  `d(d(X)/d(Y)|Z)/d(W)|V` differentiates over the 12 first-order variables; Cv, Cp and w join them as X, Y, Z, W or V
  through the order-3 bundle (`ThermoModel::derivs`, M7.10), and τ and δ through the published reducing state
  (`FluidInfo::constants`, M10.2) (user decision OD1). There is no oracle: CoolProp's second derivatives refuse Cv, Cp
  and w. *Red:* `derivative_variables_match_central_differences`: each new first derivative against central
  differences of its property along the held variable on the `eos` grid (class `Fd`), and the τ and δ derivatives
  against the T and ρ ones scaled by the reducing state (`Identity`), against a grammar that refuses the new names.
- **M20.2 The conformance kit's `identities` and `part_additivity`.** `phasekit_verify::identities(fluid, states)`
  asserts VERIFICATION.md §8.1's list in class `Identity` (M11.7's `entropy_identity` beside it), moved into the kit from `tests/eos.rs`, which then calls it
  per embedded fluid; `part_additivity` asserts that `ThermoModel::derivs`'s total is its ideal part plus its residual
  part. VERIFICATION.md §10's two rows say M20.2. *Red:* `tests/new_family.rs` runs both on its van der Waals family,
  against kit functions that report a mismatch.
- **M20.3 The intra-doc link check.** `cargo doc --workspace --no-deps` with `-D rustdoc::broken_intra_doc_links` and
  `-D rustdoc::private_intra_doc_links` in CI's linux job, and section 5.2 says it runs. *Red:* the command, failing on
  a branch whose doc comment links a missing item; then fix what it reports on `main`. *Done when:* CI runs it and
  `actionlint` is clean.
- **M20.4 The nightly report.** The nightly `oracle` job's summary gains, per fluid and kind, the rows, the cells
  asserted, the cells exempt per DIV id, the `nan` cells, the oracle-gap rows and the headroom (each column's largest
  error over its class bound), plus the ignored tests and each fluid's arbiter status, so oracle-only fluids stay
  visible. VERIFICATION.md §11.3's `nightly` row says what it lists. *Red:* an xtask unit test of the report over a
  small fixture set, against a report that lists rows and failed calls only.

**Exit gate.** G1-G8; each step's checks. **ROT rows.** None. **User decisions implemented.** PL1, OD1.

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
| M21.7 | `site/src/lib/calculator.svelte.ts`: the calculator's view and wiring, which `Calculator.svelte` binds to and Vitest drives in a client-transformed environment; StrykerJS 10 with its Vitest runner (Vitest held at 4.1): `scripts/site/mutants.sh` in G9 on the site TypeScript a PR changes, static mutants skipped, and the weekly `site mutants` job over all of it; equivalent mutants excluded with their reasons; the baseline's 38.45 % raised to 100 % |
| M21.8 | Picking a state on a diagram (the user's request, 2026-10-16): the cursor snaps to a grid in the shown units at five densities (`site/src/diagram/grid.ts`: d3's nice steps on a linear axis; decades, 1-2-5, 1 to 9, halves and tenths per decade on a log one, each an integer over a power of ten), its lines drawn while snapping; a click pins the readout, the arrow keys step it along the grid, Escape lets it go; Open in calculator carries its two inputs, so the calculator shows the same state. Snapping and the density are in the URL. Before it, the readout keeps one flash in flight |
| M21.9 | Phase regions on the diagrams (the user's request, 2026-10-16), behind a toggle in the URL (`regions=1`): each region a polygon edged by the package's lines (`site/src/sample/regions.ts`): the dome, the critical isotherm and isobar, the isotherm and isobars that bound the range (T to `T_max`; p from half the saturation pressure at the lowest T to `P_max`), and on the cold side each isobar's lowest temperature with a state, found by bisection, which follows the melting line where the package has one; sampled once per fluid by the batch with every diagram's axes among its outputs (34 to 46 calls, 20 to 180 ms in Node); each region's phase the package's at a state inside it, so the page names none. The tints are drawn softened under the lines, each line on a halo of the surface, the critical isotherm and isobar sharp over them; each region is named at the deepest point of its part in the plot where the name fits (`site/src/diagram/geometry.ts`), zoom frames still at Chromium's 16.7 ms. Re-planned before it started: classifying the plot's cells by the readout pair took 15 s on T–s and 106 s on h–s |
| M21.10 | How a property is calculated (the user's request, 2026-10-17): `#/how-it-works`, the property pipeline of `phasekit-core` in four static figures traced from `flash.rs`, `flash/phase.rs`, `fluid.rs`, `state.rs` and `batch.rs`: one PT call from the registry to the outputs, the 19 pairs over the three primitives their iterates come down to, the phase rule, and a batch under `Reference` and `Simd`; a table of what loads when. The figures are inline SVG drawn from data (`site/src/pages/HowItWorks.svelte`), themed by the page's tokens with the Helmholtz model in the accent, and scroll in their own frames on a phone, where the header's links now take their own row. The page computes nothing, so its arbiter is the source it was traced from. Loaded on first visit; the budget 96 289 B + 10 % |

- **M21.5 Mixtures.** After M13, on what MX5 brings to the browser package: a mixture picker (components from the
  index, mole fractions that sum to one, the predefined mixtures); the phase envelope on p–T and T–ρ with its critical
  point; bubble and dew points; two-phase states with each phase's composition in the readout; a liquid-liquid or
  three-phase refusal shown as the package's typed error. The step's detail is fixed when M13 closes. *Red:* Vitest:
  every envelope vertex is a bubble or dew state of the package (`Exact` against its flash at Q = 0 or 1) and the
  critical point lies on the envelope. Playwright: R410A.mix's envelope and a two-phase readout.
- **M21.6 The diagrams from M19.** The diagrams take their domes, isolines and envelopes from M19's geometry through
  the browser package, and `site/src/sample/` is deleted. *Red:* M21.2's checks on M19's geometry, run first against
  the package without its export. *Done when:* G1-G9; the screenshots re-approved in the PR.

**Exit gate.** G1-G9 at every step; the page deployed from `main` at each merge; every value on the page equals the
package's (`Exact`); M21.1's budget and SL8's wasm budgets hold; `reuse lint` and `actionlint` clean. **ROT rows.**
None. **User decisions implemented.** WS1, WS2, WS3, WS4, WS5.

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

- **M22.1 The mixture batch and its scalar baseline.** `Mixture` overrides `residual_batch` and `ideal_batch`: the
  reducing state once per batch (z is fixed), each component's own `residual_batch` over its scaled (T_i, ρ_i) and its
  own `ideal_batch` at the mixture's (T, ρ), as `residual_at` and `ideal_at` evaluate them, so a built-in component
  runs M12's lanes under `ExecPolicy::Simd`, and the departures point by point; the parts summed in the scalar order.
  GERG-2008's components, the crate's own, run point by point until M22.3. Benches
  `mixture_alphar_batch/{scalar,simd}` for R32/R125, Methane/Ethane, a five-component natural gas and AGA8's
  21-component check gas (GERG-2008), recorded on the i7-8700K (§5.1) with the breakdown into components, reducing
  functions and departures. *Red:* `mixture_batch_is_residual_per_point`, bitwise against per-point `residual` and
  `ideal` for the bench mixtures at every order under `Reference`, `Parallel` and `Simd`, against a stub returning
  `Error::Unsupported`. *Done when:* green, and the record holds both benches.
- **M22.2 Core runs a family's kernels in lanes (ML3).** In `batch`, beside `ExecPolicy`: a public trait whose one
  method is generic over `R: Real`, a pure function from inputs to outputs whose counts each kernel value gives (M22.5's
  Taylor arithmetic has one variable per component, known at run time), and a function that runs it over columns of
  `f64` under an `ExecPolicy`, at `Lanes<W>` inside core under `Simd`; unequal columns are `Error::Shape`. The runner
  allocates its working columns once per call, not per group of lanes: counts known at run time must not put an
  allocation inside the loop, where ML4 would measure it as the kernel's cost. `Lanes<W>`, the executors and the lane
  `exp`, `ln` and `expm1` stay crate-private. The runner's docs say that a kernel is `#[inline(always)]` down to its
  `Real` operations, as core's are, so that `run_simd`'s target features reach it; M22.3's and M22.5's kernels are.
  ARCHITECTURE.md §3 (the seam table, the public generic types, the count of traits), §3.1 and §7 say what is public; if
  a release tag exists, `cargo semver-checks` against it is clean for a minor version. *Red:*
  `real_kernels_are_scalar_bitwise` in `phasekit-verify` (outside core, through the public path only): a test kernel
  that uses every `Real` method, its counts set at run time, over batches of 1 to 65 points (a short last group at every
  width, SL6), bitwise equal under `Reference`, `Parallel` and `Simd` to the kernel on `f64` point by point, against a
  stub returning `Error::Unsupported`.
- **M22.3 Departures in lanes.** `phasekit-mix`'s departure kernel moves from its own `Scalar` trait (M13.7) onto
  `Real`: `Series` over `R: Real`, its test for the variable in `compose` a flag rather than a comparison (`Real` has
  none); `MultiFluid`'s batch runs the departures through M22.2's runner, and GERG-2008's components (`gerg::Pure`),
  whose residual is the same kernel, override `residual_batch` through it (their α⁰, on `Jet4`, stays point by point).
  *Red:* `departure_lanes_are_scalar_bitwise`: every library and GERG-2008 departure function, and each GERG-2008
  component's `residual_batch`, at every order through the runner under `Simd`, over batches of 1 to 65 points, equal
  to the scalar kernel point by point, δ → 0 included. Bench `mixture_alphar_batch/simd` again; kept if faster than
  M22.1's (ML4).
- **M22.4 Hinted DT batches (ML5).** `ThermoModel` gains a provided method, named at this step: the flash of a group of
  points that core's batch has validated (validation, basis, gauge), by default each point's `flash`. Core gives the
  hook only the points its gate passed, in order, and writes each result back to its point's row, so a refused point
  keeps its refusal and no family fills a lane for it (core's lane path copies the group's first passing point into it
  today). The hook's docs say that an override returns each point's `flash` bit for bit (K17), which the conformance
  kit's `policy_equivalence` checks for any family. Under `ExecPolicy::Simd` core's batch calls it for every model, in
  place of its `PureFluid` downcast (`batch.rs`); `PureFluid`'s is its M12 lane path for DT, QT and PQ. `Mixture`'s
  takes a DT group under a phase hint (`Strategy::Direct`): the bundles through its `residual_batch` and `ideal_batch`,
  then each point's state from its bundle by the function its scalar hinted DT calls. Unhinted DT keeps the stability
  test (M13.10), and it and the other pairs run point by point. ARCHITECTURE.md §3 (the seam table) and §7 say what is
  public. *Red:* `policy_equivalence` over hinted DT batches of the bench mixtures, points the gate refuses among them
  (bitwise against per-point `Fluid::flash`), against a `Mixture` hook stubbed to return `Error::Unsupported`. Bench
  `mixture_dt_batch/{scalar,simd}`.
- **M22.5 The spike: a flash's two phases in lanes.** Where a flash evaluates its liquid and its vapour at one T (the
  split's Newton, the bubble and dew Newton, the isochoric search), the two phases' composition derivatives as a batch
  of two: the components' jets through `residual_batch` at both points, the departures and the Taylor arithmetic as
  one kernel over `Real` through M22.2's runner. *Red:* `two_phase_lanes_are_scalar_bitwise`: the two phases'
  composition derivatives in lanes equal the scalar ones bitwise, and `mix_flash`'s and `mix_sat`'s results are bit
  for bit those of the scalar path. *Done when:* green, and benches `mixture_flash/{pt,qt,pq}` against M22.1's record
  decide (ML4).
- **M22.6 The spike: the stability test's trial phases in lanes.** Each round of the N + 2 trial phases' substitution
  and Newton stages runs the trials still iterating as one batch through M22.2's runner; the convergence tests and a
  trial's stop stay scalar in `phasekit-mix`, so no lane select is needed. *Red:* the stability verdicts and
  minima of `tests/pt_flash.rs`'s corpus bitwise the scalar ones. *Done when:* green, and `mixture_flash/pt` decides
  (ML4).
- **M22.7 The spike's record.** Each spike kernel kept or taken out by its measurement; the i7-8700K's M22 record; the
  re-plan, if any, for the flash's remaining scalar time. *Red:* `gates perf` with the mixture benches' instruction
  counts in its baseline.
- **M22.8 The browser build, and M22 closed.** `phasekit-wasm`'s `simd128` build runs the mixture batches in lanes (the
  browser package serves mixtures since M13.14), its size budget and Node tests on both builds; ARCHITECTURE.md §7 and
  VERIFICATION.md §12 say what landed. Tag `m22` with the user's approval.

**Exit gate.** G1-G9, and G1-G8 with `--features simd`; `policy_equivalence` bitwise for the mixture batches at every
dispatch level of the i7-8700K, the CI runners (x86-64, aarch64) and wasip2 with `simd128`; the flash fixtures'
results unchanged by the spike; each kernel faster than the scalar path or taken out (ML4). **ROT rows.** None. **User
decisions implemented.** ML1-ML5.

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
without `libm`, `math` and `Lanes` keep std's, lane by lane, as now. Each step re-commits `fixtures/hash/libm.txt` once
the four CI targets agree on it.

| Step | Landed |
|---|---|
| M23.1 | `scripts/math/reference.py` (mpmath 1.3.0 at 256 bits) writes `fixtures/math/{exp,ln,pow,expm1}.csv`, 3940 to 4131 values each in three sets: `edge` (VERIFICATION.md §9.2's edges and the thresholds of the `libm` crate's and Arm's code paths, each ±1 ulp), `eos` (1950: exp's −60 to 5; ln's τ in [0.0026, 5.27] and δ in [1e-12, 3.83], the 136 fluids' ranges; pow's bases τ, δ, θ, Δ and (δ − 1)² with the fluids' non-integer exponents; expm1's −θ_k τ of the Planck-Einstein terms) and `binade` (1950, uniform over every exponent of the domain); and `fixtures/math/arm/` at the inputs of Arm's directed cases (optimized-routines v26.07, pinned by `scripts/fetch-optimized-routines.sh`; `log.tst` for ln; a case with a signalling NaN operand is left out, as the format writes every NaN `nan`), each value checked against Arm's result. A value is its correctly rounded double and its residual in ulps to four digits; 979 kB together. Class `Faithful` (VERIFICATION.md §5): under 1 ulp in Arm's convention, `phasekit_verify::ulp`. `tests/math.rs` reports the backend's worst error per set (`--no-capture`; CI's linux job prints both backends'): glibc 0.72 ulp (expm1) and the `libm` crate 0.76 (exp) on x86-64 Linux. Core's lane `exp`, `ln` and `expm1` carry their msun notices at their heads and `crates/phasekit-core/NOTICE` both notices (LC1, `tests/notices.rs`); `LICENSES/LLVM-exception.txt` came with `fixtures/math/arm/`. `benches/math.rs`: `math_per_call/{exp,ln,pow,expm1}`, `throughput` and `chain`; it and the six scalar benches of D12's rule are named `<case>_libm` on the `libm` crate's maths (`backend::case`), and both builds are recorded on the i7-8700K as M23's baseline (`benches/results/M23-intel-core-i7-8700k.csv`) |

- **M23.2 `exp`.** Arm's `math/exp.c` without FMA (128-entry table, degree-5 polynomial; k rounded by the shift, never
  by an `as` conversion), its table as `u64` words, under IM5's notice (`REUSE.toml`, `LICENSE-THIRD-PARTY`, core's
  NOTICE; `LICENSES/LLVM-exception.txt` came with M23.1). Under `libm`, `math::exp` and `Lanes::exp` run it; M12.5's
  port of the `libm` crate's `exp` goes. *Red:* `exp_is_faithful` in `phasekit-verify` (through
  `phasekit_core::internal`, so in every build): under 1 ulp on M23.1's values (class `Faithful`), Arm's cases
  (`fixtures/math/arm/`) bit for bit (NaN canonical), against a stub returning NaN; with core's
  `lane_exp_is_scalar_bitwise` (M12.5's test, now against the in-house scalar function, in every build and at every
  dispatch level). *Done when:* green, the worst error recorded, the `libm` hash re-committed, and `math_per_call/exp`
  and the α^r benches (the `libm` scalar path and the lanes) recorded against M23.1's.
- **M23.3 `ln`.** Arm's `math/log.c` without FMA: the 128-entry table with the second table that stands in for FMA,
  and the degree-12 polynomial near 1. As M23.2: M12.6's port goes; *Red:* `ln_is_faithful` and
  `lane_ln_is_scalar_bitwise`.
- **M23.4 `pow`.** Arm's `math/pow.c` without FMA: its `log` to about 68 bits (its own 128-entry table) and its `exp`
  with the tail, and every special case of C99 Annex F (zero, infinite and NaN arguments, a negative base with an
  integer exponent, subnormal bases, overflow and underflow). Under `libm`, `Lanes::powf` runs it across lanes, where
  M12.8 called `math::powf` lane by lane, a group of positive normal bases with a finite exponent taking the main path.
  *Red:* `pow_is_faithful` on M23.1's pairs and Annex F's cases, and `lane_pow_is_scalar_bitwise`. Benches: the DT and
  PH of Water and R134a (their non-analytic and α⁰ Power terms) under `libm` and `simd`.
- **M23.5 `expm1`.** M12.8's port of the `libm` crate's `expm1` (under 1 ulp, already without branches) becomes the
  scalar path's function too, in M23.2's module. A table form built on M23.2's replaces it only if it stays under 1 ulp
  on M23.1's values and is faster on `math_per_call/expm1`; the record says which. *Red:* `expm1_is_faithful` and
  `lane_expm1_is_scalar_bitwise`.
- **M23.6 M9.5's rule, and the default.** The six scalar benches of D12's rule over VERIFICATION.md §12's bench fluids,
  in three interleaved rounds on the quiet i7-8700K: the `libm` build (the four in-house functions; the benches reach no
  other, IM2) against the default build (std). If the geometric mean of the median-time ratios is ≤ 1.05 and none is
  above 1.10, every build runs the four in-house functions and:
  - `simd` no longer enables `libm`, and `gates features`' `simd128` tree loses `libm`;
  - `tests/cross_target.rs` asserts one hash for the std and `libm` builds, `fixtures/hash/libm.txt` renamed for it;
  - `math::BACKEND` names the backend of the remaining functions;
  - LB1's known differences (`backend::LIBM_KNOWN`) are re-measured for the default build, any new cell taken by LB1's
    route and never by a wider tolerance;
  - `gates perf`'s instruction baseline is updated, with the counts in the PR;
  - D12, ARCHITECTURE.md §7 and §9, VERIFICATION.md §9.2 and §9.4, and DEPENDENCIES.md (the `libm` crate's role) say
    what holds.

  If the rule fails, the measurements go into D12, the default keeps std, and `simd` keeps enabling `libm`. *Red (if it
  passes):* `cross_target`'s std build asserting the committed hash, which std's maths fails.
- **M23.7 The `fast` feature (IM4).** `Real` gains a·b + c as a method (`Real` is sealed, so not a break): two roundings
  by default, `f64::mul_add` under `fast` on every target, through one helper exempt from clippy's ban. Under `fast` the
  in-house functions take Arm's FMA forms (`log`'s and `pow`'s r = fma(z, invc, −1) without the second table, `exp`'s
  polynomial), and the α^r and α⁰ term sums and the polynomial and Clenshaw evaluations take the method. CI runs G1-G8
  with `--features fast`. `fixtures/hash/fast.txt` is committed once the four targets agree (wasm32-wasip2 fuses in
  software); `policy_equivalence` is bitwise under `--features fast,simd` at every dispatch level; if M22.2 has landed,
  the kernel of its `real_kernels_are_scalar_bitwise` uses the method, and the test runs under `--features fast,simd`
  too; M23.1's values stay under 1 ulp; the K1/K2 paper rows and the oracle classes pass unchanged. Benches: the six
  scalar benches and the α^r lanes, `fast` against M23.6's default, at `-C target-cpu=x86-64-v3` and at the x86-64
  baseline, recorded; the crate docs say that `fast` pays only on a target with FMA. The browser builds do not enable
  it. *Red:* `fast_fuses`: under `fast`, the method gives the correctly rounded a·b + c where one rounding and two
  differ, against the unfused method.
- **M23.8 M23 closed.** The browser package's two builds re-measured under Node (both run the in-house functions if
  M23.6 passed), each `.wasm` within its budget or re-budgeted at its size + 10 % (SL8); ARCHITECTURE.md §7 and
  VERIFICATION.md §12 say what landed; the i7-8700K's M23 record. Tag `m23` with the user's approval.

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

## 5. Cross-cutting tracks

### 5.1 Performance recording

Each path is benched as it lands (criterion; gungraun from M9.6) against the targets of ARCHITECTURE.md §7;
workloads, bench fluids, the harness and the result files are VERIFICATION.md §12. Non-blocking until M9; from M9.6
`gates perf` enforces gungraun regressions on Linux CI, and the reference machine's record is checked against the
targets, a miss logged rather than blocking (user decision TG1).

### 5.2 Documentation

- Rustdoc on every public item (`missing_docs`, enforced by G2); a `cargo doc --no-deps` check that denies broken intra-doc
  links is planned (M20.3), but no workflow runs it yet; the crate-level doctest of `phasekit-core` executes (the seed's does).
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
