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
| The user's decisions | [DECISIONS.md](DECISIONS.md), each by its id |
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
- D1-D17 = ARCHITECTURE.md §4. "User decision X" = the row with id X in DECISIONS.md (the first pass numbered its rows
  1-13 and D14; later ids carry a prefix: TQ1, MS1, DL4). ROT-NNN = ROT-REGISTER.md rows. DIV-NNNN = `register.rs`
  entries. S-## and E## = critic issues in [history/03-decision-log.md](history/03-decision-log.md).
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
| Fixtures: the oracle lock, the manifest, oracle kinds, papers, multiprecision points, hashes | `crates/phasekit-verify/fixtures/` | VERIFICATION.md §3.4 |
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
| G9 | `scripts/site/check.sh`: the site's build (`scripts/site/build.sh`), `prettier --check`, `svelte-check`, Vitest, the page's budget (`site/size-budget.txt`) and Playwright on Chromium | local (Node, npm, cargo-about and wasm-bindgen-cli; `npx playwright install chromium` once); CI site, on every PR | M21.1 |

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
      ├─ M21 site: M21.1, M21.7, M21.2-M21.4 now, M21.5 after M13, M21.6 after M19
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
| M21 | `site/`, the GitHub Pages site | The user's word, 2026-10-16, beside M13 (user decision WS3) | A single-page site over the browser package: a calculator, five diagrams with the state under the cursor, sweeps, an in-browser benchmark of both builds, all 136 fluids from per-fluid packs, mixtures after M13, M19's geometry after M19, mutation testing of the site's code; seven steps: [M21 The site](#m21-the-site) | G9 at every step; the page deployed from `main`; every value on the page equals the package's (`Exact`) |
| — | Tier B shim | A host needs `AbstractState_*` | ~25 exports, generational handles (map 11 U11) | v8.0.0 signatures; fuzzed |
| — | WIT component | A concrete non-Rust WASI host (user decision 8) | `wit-bindgen` component over compat (R17) | Host smoke test |
| — | Network service facade | A consumer needs phasekit over the network (BRIEF "connected") | A thin HTTP or RPC service over compat in its own crate; never core (ARCHITECTURE.md §1) | Service smoke test; core diff zero |
| — | Materials | First phase-coexistence need (ice + water) | `Substance` + min-Gibbs selector in a new crate (D10; materials R1) | Zero-line core diff |
| — | Near-critical VLE in extended precision | A use needs the VLE itself at multiprecision accuracy near Tc (idea recorded at the user's request, 2026-10-09) | Near Tc the double-precision VLE carries its rounding amplified as Θ^−1.5 (user decision NC1, measured at M6.4). Double-double (or similar) arithmetic in the near-critical solve, as fastchebpure computes its multiprecision values. A scope change: the core is std-only until M9 and has performance targets | The dense fastchebpure rows within `SatMp`'s far-field 1e-11 closer to Tc than today; no cost to the double-precision path (the M9 benches) |
| — | Cubic family in lanes | A use needs cubic batches faster (idea recorded at the user's request, 2026-10-15) | M11's cubic fluids take `ExecPolicy::Simd`'s lane path but evaluate point by point: `CubicModel` and `CubicSaturation` keep the scalar defaults of `residual_batch`, `ideal_batch`, `at_t_batch` and `at_p_batch`. α⁰ needs no core change: `CubicModel::ideal_batch` can forward to the borrowed HEOS model's, already in lanes (M12.8). α^r is generic over `Real`, which core's `Lanes<W>` implements, but `Lanes` is crate-private and α^r's derivatives run on `Jet4`; lanes for them and for the superancillary's Clenshaw sums (as M12.9's) need a public lane type: a core API change, outside a family milestone's zero-line core diff | `policy_equivalence` bitwise on the cubic family at every dispatch level; faster than the cubic's scalar path on M12's benches |

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

- **M13.4 The mixture at fixed composition.**
  - `Mixture` holds a `MultiFluid` and a composition z. z is validated: finite, each fraction in [0, 1], summing to 1
    within a stated tolerance, else a typed refusal (ROT-193). Zero fractions are trimmed (ROT-192), and a composition
    that trims to one component is that pure `Fluid` itself.
  - `Mixture: HelmholtzModel`:
    - R = R_CODATA (MX3); α^r from M13.3.
    - α⁰ = Σ z_i (R_i/R)[α⁰_oi(T, ρ) + ln z_i]. Each component's ideal part is in its own variables and carries its
      native gauge, so the mixture's gauge is the z-weighted one (D4; ROT-062, ROT-103).
    - Exact virials: the components' at T_r,i/τ plus the departures' δ-series.
    - `rho_max`, and its own `Limits`: the components' intersection, not their mole-fraction averages (ROT-197).
    - `FluidInfo` with M_z and the reducing state (T_r(z), ρ_r(z)); a `ModelKey` from components, pairs and z.
  - Its `ThermoModel` flashes DT and PT with a phase hint, single-phase, through M13.1's density solve. Until M13.8 it
    returns `Unsupported` otherwise.
  - Register entries:
    - The 15 fluids whose critical and reducing states differ: v8.0.0 evaluates their α⁰ at τ_i = T_c,i τ/T_r (map 15
      X12), so their α⁰-derived mixture outputs are `SkipOracle`.
    - A composition with a unit fraction differs from v8.0.0 by its R ratio, since v8.0.0 keeps R_CODATA at x = [1, 0]
      (MX3).
    - Tkaczuk's Table 8 is checked at Σx_iR_i (`KeepOracle`, MX3).
  - Oracle kind `mix_eos`: single-phase (T, ρ) states of the binaries, a ternary and the `.mix` blends, with p, h, s,
    u, c_v, c_p, w, Z and the α⁰ and α^r parts.

  *Red:* `unit_fraction_is_the_pure_fluid` (bitwise, under any gauge; ROT-099, ROT-208);
  `near_unit_fraction_approaches_the_pure_fluid` (up to MX3's R ratio); `trace_mixture_equals_pure` over the 15
  fluids (ROT-062); `composition_is_validated` (ROT-193: (0.3, 0.3), (−0.1, 1.1), NaN);
  `zero_components_equal_the_trimmed_mixture` (ROT-192); `limits_are_the_components_intersection` (ROT-197); Lemmon
  and Jacobsen's Table 6 and Tkaczuk's Table 8, each class set by its transcription's self-consistency (VERIFICATION.md
  §4.3); `mix_eos` within `Prop`. They run first against a `Mixture` that refuses every composition.
- **M13.5 GERG-2008.**
  - The 21 components get GERG's own pure equations (12- or 24-term power and exponential sums) and ideal gas
    (Planck–Einstein in sinh and cosh, n₁ and n₂ fixed so that h = s = 0 for the ideal gas at 298.15 K and 101 325 Pa),
    each as its own `HelmholtzModel`.
  - Its 210 binaries and 15 departure functions; R = 8.314472 (MX3).
  - Transcribed from NIST's AGA8 code (MX7): `scripts/fetch-aga8.sh` fetches it into a gitignored `reference/AGA8` at
    the pinned commit, like `fetch-coolprop.sh`; xtask's `gerg_2008_is_aga8s` checks the transcription number for
    number; NOTICE credits it. Cross-checked against TM15's tables for the 18 GERG-2004 components.
  - It is a `MultiFluid` like M13.3's, so every later step serves both models.
  - `GERG2008::` names (MX6) for the pure components and for mixtures. The pure components are `PureFluid`s with GERG's
    critical parameters, core's VLE serving their saturation. GERG's 60-700 K range is their `Limits`, with `t_triple`
    the component's own (ROT-205).
  - A second generator, `scripts/oracle/teqp.py`, pins teqp 0.23.2 as gen.py pins CoolProp and runs in the same image.
    It writes `fixtures/teqp-0.23.2/gerg/*.csv`: the 21 pure components and every binary at 50/50 on a (T, ρ) grid,
    and AGA8's natural gases. VERIFICATION.md §3 gains its lock and kinds.

  *Red:*
  - AGA8's check state: Z, its p derivatives, c_v, c_p, w, JT and κ at AGA8's own 1e-8 test; u, h, s and g within the
    1e-8 that the reference-constant rounding leaves.
  - AGA8's binary test set: 3645 rows at their printed digits.
  - teqp's vectors within `Term` and `Prop` (ROT-198, ROT-203).
  - Pure GERG HP, PS and PU round trips (ROT-083); x = [1, 0] equals the GERG pure fluid, bitwise.

  They run first against a GERG model with no departures. ROT-204 and ROT-206 are designed out: the data is immutable
  and the vectors are CSV fixtures.
- **M13.6 Composition derivatives.** `MixtureModel` is public in phasekit-mix and implemented by both multi-fluid
  models.
  - At (T, ρ, n) it gives the residual Helmholtz energy's derivatives in n to order 3, by one mole-number AD path. The
    crate's own multivariate Taylor numbers compose each component's (τ, δ) jet with τ(n) and δ(n); there is no
    third-party dependency (T1f).
  - From them: ln φ_i, n ∂ln φ_i/∂n_j, (∂ln φ_i/∂T)_p,n, (∂ln φ_i/∂p)_T,n, partial molar volumes and chemical
    potentials (ROT-191, ROT-210).
  - `mix_eos` gains the oracle's `fugacity_coefficient` and `chemical_potential` columns.

  *Red:* `gibbs_duhem_holds` (Σ x_i n ∂ln φ_i/∂n_j = 0, `Identity`); `ln_phi_hessian_is_symmetric`;
  `euler_homogeneity`; `composition_derivatives_match_fd` (`Fd`) and num-dual (`Term`); Lemmon and Jacobsen's fugacity
  column; the oracle's ln φ within `Prop`. They run first against derivatives that return zero.
- **M13.7 Critical points.**
  - Heidemann and Khalil's criticality conditions (det L* = det M* = 0, the order-3 tensors from M13.6) at a fixed z.
  - Every critical point in the model's range, found once and kept in the immutable `Mixture` (ROT-187).
  - `ThermoModel::critical_point()` is the one the vapour-liquid envelope passes (`CriticalOrigin::Model`), and M13.8's
    labels use it.

  *Red:* `critical_points_satisfy_heidemann_khalil` (`Identity`); the Methane/Ethane locus against Bell and Jäger 2017
  once the paper is in hand, until then teqp's `trace_critical_arclength_binary` (differential); the oracle's
  `all_critical_points` outside its known-bad states (map 04 #14). They run first against a `critical_point` of
  `None`.
- **M13.8 Stability and the PT flash.**
  - Rachford–Rice in β with ln K inputs, bracketed and negative-flash capable; Wilson's K.
  - Michelsen's tangent-plane stability test: successive substitution, then a second-order minimisation on the
    n ∂ln φ/∂n Hessian, with near-pure trial phases. The feed density comes from the density solve's roots, never a
    forced branch (ROT-186).
  - The PT split: accelerated successive substitution, then Newton in the minority phase's variables.
  - Every density root is enumerated through M13.1's solve and its acceptance gate (ROT-181, ROT-183).
  - Phase labels from the critical point. Supercritical labels are as for a pure fluid; below the critical point a
    phase is liquid where ρ > ρ_c(z) and gas otherwise (ROT-009; v8.0.0 compares with ρ_r(z)). A liquid-liquid split is
    refused with a typed error (MX4).
  - Oracle kind `mix_flash`: PT, and later pairs, with map 04 §8's known-bad states excluded and listed.

  *Red:*
  - `every_split_has_equal_fugacities_and_mass_balance` and `a_split_lowers_the_gibbs_energy` (`Identity`).
  - The C1/C2/C3 state (0.05, 0.90, 0.05) at 2 MPa and 223.15 K is single-phase on its stable root (ROT-181).
  - ROT-183's regression corpus (bd:ft05, #3283, #3448), and v8.0.0's Michelsen scenarios
    (`CoolProp-Tests-Michelsen.cpp`, the papers' compositions), judged by the invariants.
  - `mix_flash` PT within `Flash`.

  They run first against a flash that returns the single-phase state.
- **M13.9 Bubble and dew points.**
  - QT and PQ at Q ∈ {0, 1}: Newton in (ln K, ln T or ln p) with the AD Jacobian, seeded by Wilson and successive
    substitution. It ends on a residual gate over the equal-fugacity and summation equations, never on the step size
    (ROT-185).
  - The incipient phase's composition and density are on the split (M13.1's phase M); fugacities at Q = 0 and 1
    (ROT-194).
  - The `.mix` blends' bubble and dew points are checked against the pseudo-pure names' ancillaries, which were fitted
    to them.
  - Oracle kind `mix_sat`: bubble and dew T and p sweeps for the binaries and the `.mix` blends.

  *Red:* `bubble_and_dew_residuals_meet_the_gate` (`Identity`); `fugacity_at_q_0_and_1` (ROT-194); teqp's isotherm
  traces (differential); `mix_sat` within `Flash`. They run first against QT and PQ that return `Unsupported`.
- **M13.10 Inside the envelope: interior Q and DT.**
  - QT and PQ at 0 < Q < 1 by a flash with Q imposed: Rachford–Rice at the given β, with both phases' compositions and
    the mass balance among the unknowns (ROT-182).
  - DT at (T, v): the stability test at fixed volume, then the isochoric split, and the homogeneous state where it is
    stable.
  - ROT-190's bench measures the test's cost over `Strategy::Direct`'s DT; ARCHITECTURE.md §7 sets no target.
  - v8.0.0 returns a homogeneous gas at 1.513 MPa for an R32/R125 split at 1 MPa (its TP test at the EOS pressure):
    those oracle rows are excluded under a register entry.

  *Red:* `interior_q_conserves_mass_and_fugacity` (`Identity`); `dt_in_the_dome_is_the_split` (a DT at a two-phase
  state's (T, ρ) returns its split within `Flash`); `mix_flash` QT, PQ and DT within `Flash`. They run first against a
  DT that returns the homogeneous state.
- **M13.11 Every pair.**
  - The other pairs come from an outer solve on M13.8-M13.10: HP, PS and PU along the isobar; HT, ST and TU along the
    isotherm; DP, DH, DS and DU along the isochore; HS, SU, DQ, HQ and QS along the saturation boundary.
  - Each pair enters `Capabilities` only once it passes (ROT-293: v8.0.0 lacks DP, DQ, HQ and QS for mixtures).
  - The mass quality in and out (ROT-201).

  *Red:* the conformance kit's `capability_matrix` (19 pairs × single- and two-phase truths) on R410A.mix, Air.mix and
  a GERG natural gas; the near-dew HSU_P band of map 04 #12's mixture (ROT-184); Qmass round trips; phase-label
  fixtures (ROT-009). They run first against capabilities that declare PT, DT, QT and PQ only.
- **M13.12 Phase envelopes.**
  - `Mixture::envelope(&EnvelopeOptions)`: a continuation in (ln K, ln T, ln p) with the most sensitive variable
    specified (Michelsen's method, map 04 §3.8).
  - It runs from a low-pressure dew or bubble point through the critical point to the other side, bounded by the
    model's `Limits` and a stated p_max.
  - It ends typed (closed, a bound, or no convergence) instead of running away (ROT-188).
  - Its points are named by incipient and bulk phase, in a struct of arrays. Nothing in a flash reads it (ROT-189: no
    history).

  *Red:* `every_envelope_point_is_a_bubble_or_dew_point` (M13.9's gate); M13.7's critical point lies on the envelope;
  the envelopes of CO₂/water, C₁/nC₁₀, H₂/C₁ and N₂/C₁/C₂/C₃ (map 04 #15) end typed within bounds; the oracle's
  `build_phase_envelope` for well-behaved binaries. They run first against a tracer that returns no points.
- **M13.13 Names.** `phasekit_mix::with_mixtures(&registry)`:
  - A resolver (M13.1) for CoolProp's `A[x]&B[y]` grammar. Names go through the registry's names and aliases, the
    fractions are validated as M13.4's, and `HEOS::` and `GERG2008::` prefixes are accepted. Nothing is parsed
    leniently (ROT-166).
  - CoolProp's predefined mixtures by `.mix` name, registered beside the pseudo-pure names, which keep their behaviour
    (user decision 4).
  - compat's `fluid_in` passes `&`, `[` and `]` to the registry's `resolve`; it still refuses `?` and `%`.
  - A missing binary pair is `NoModel` naming the pair (ROT-292: Ammonia&Water), never an estimate (estimators come on
    demand).
  - The predefined blends' fractions (ROT-199): `Corrected` derives the mole fractions from ASHRAE 34's mass fractions
    (M13.2's table) with the registry's molar masses, `Parity` keeps v8.0.0's (at most 8e-6 apart); a register entry
    (the next free id) records the difference and its arbiter.
  - Parameters are overridden through a builder that returns a new model and leaves the defaults untouched (ROT-196).

  *Red:* `compat_reads_coolprop_mixture_strings` (the oracle's `PropsSI` on `HEOS::R32[0.697615]&R125[0.302385]` and
  `R410A.mix`); `pseudo_pure_names_keep_their_behaviour`; `ammonia_water_is_no_model`;
  `bip_override_never_poisons_the_defaults`; `malformed_mixture_strings_are_refused`. They run first against compat's
  current refusal.
- **M13.14 C and the browser (MX5).**
  - `phasekit-capi` and `phasekit-wasm` depend on phasekit-mix and serve `with_mixtures` over their embedded
    registries.
  - `pk_*` and the shim's `PropsSI` take mixture strings. `get_global_param_string("predefined_mixtures")` and
    `("mixture_binary_pairs_list")` return v8.0.0's lists (today they return "").
  - The browser package serves the same strings. Its two `.wasm` budgets are re-measured at size + 10 % (as SL8), and
    the `cargo about` notices carry AGA8's credit.

  *Red:* the C smoke test and the shim's `PropsSI` on a mixture string, against the oracle's value; the two global
  strings against v8.0.0's (`facts`); the wasm-bindgen test under Node. They run first against facades that refuse
  the strings.
- **M13.15 Conformance.** `tests/mixture_family.rs` runs the kit on R410A.mix, Air.mix, a GERG natural gas and a
  ternary, in both datasets:
  - `capability_matrix`;
  - `gauge_invariance`, with x = [1, 0] under IIR, ASHRAE and NBP;
  - `policy_equivalence` under `Reference`, `Parallel` and `Simd`;
  - `thread_invariance`, `fd_first_order` and `entropy_identity`.

  The mixture benches (α^r, DT, PT, QT; ROT-190, ROT-195) are recorded on the i7-8700K (section 5.1). *Red:* each
  check on a mixture registered through `with_mixtures`, run first against a registry without it.
- **M13.16 M13 closed.** `MILESTONE` = 14: every M13 proof falls due (DIV-0013 and M13's new entries) and every M13
  ROT row is ticked. `gates core-frozen` passes since `m13-start`, and `ci/core-frozen.txt` then names no tag.
  ARCHITECTURE.md's crate table and §11, VERIFICATION.md §10, MIGRATION.md (mixture strings) and the README say what
  landed. Tag `m13` with the user's approval.

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
word, 2026-10-16, beside M13 (WS3): M21.1, then M21.7 (WS5), then M21.2-M21.4; M21.5 after M13, M21.6 after M19. **Prerequisites.** User
decisions WS1-WS4; DEPENDENCIES.md's tier T5; Pages on, its source GitHub Actions, the `github-pages` environment
deploying from `main` only (done 2026-10-16). **Arbiters.** The page adds no numerics, so its oracle is the browser package: a value on
the page equals the package's at the same inputs (`Exact`), and the library's own tests check the package. Unit
conversions follow the SI brochure's definitions (0 °C = 273.15 K, 1 bar = 10⁵ Pa), theme contrast WCAG 2.2's
contrast ratio. M21 does not move `MILESTONE`.

| Step | Landed |
|---|---|
| M21.1 | `site/`: Svelte 5, Vite and TypeScript (T5); the calculator over the five embedded fluids, its view in the URL; themes (light, dark, the system's; four accents), WCAG 2.2 AA contrast checked; the engine worker on the `simd128` build where validated; the browser package's `outputKeys()`; `scripts/release/wasm-modules.sh`, which the npm package and the site share; G9 and CI's `site` job; `pages.yml`; the licences page; the page's budget, 33 218 B + 10 % |

- **M21.2 The diagrams.** p–h (log p), T–s, p–T, T–ρ and h–s for any loaded fluid.
  - The saturation dome from QT batches between T_triple (or T_min) and T_c, the grid closing in on T_c; the critical
    and triple points.
  - Isotherms, isobars, isentropes, isenthalps, isochores and lines of constant quality, each from `evaluate` along
    its line, split where it meets the dome (its two-phase part from `PQ` or `QT`) and where a cell fails, which
    leaves a gap, never a join.
  - Zoom and pan; a crosshair that flashes the state under the cursor (p–h by HP, T–s by ST, p–T by PT, T–ρ by DT,
    h–s by HS) and lists its properties; PNG, SVG and CSV export; colours from the theme tokens, the isolines in a
    colour-blind-safe palette.
  - The sampling is plain TypeScript in `site/src/sample/`, which M21.6 replaces with M19's geometry.

  *Red:* Vitest under Node on the real package, for Water and n-Propane: every vertex equals the package's value at
  its inputs (`Exact`); a one-phase segment's vertices lie on one side of the dome, and a two-phase segment's ends are
  the saturation states at its value (`Exact` against `QT` or `PQ`); a failed cell leaves a gap. Playwright: a hover
  over a known point of Water's p–h diagram lists the HP flash's values; screenshots of each diagram in the light and
  dark themes (`toHaveScreenshot`, Chromium on Linux) become the visual baseline. *Done when:* G9; each diagram's
  sampling time in the worker (Chromium, the i7-8700K) is recorded in the PR.
- **M21.3 Sweeps and the benchmark.**
  - Sweeps: any output against T along isobars or against p along isotherms, several lines at once (c_p near the
    critical point, w, η, λ, Z); σ, the latent heat and both phases' properties along saturation.
  - The benchmark: N states of a chosen pair on both builds, in the viewer's browser: states per second, the cells by
    status, and whether the two builds' values are equal bit for bit, as SL8 says they are.

  *Red:* Vitest: a sweep's points equal `PropsSI` at the same inputs (`Exact`); the benchmark's comparison reports a
  planted one-ulp difference, and none between the two builds on n-Propane's DT, QT and PQ grids (Node runs
  `simd128`). Playwright: the benchmark page completes and reports the equality.
- **M21.4 All 136 fluids.**
  - `cargo xtask packs <dir>`: one pack per fluid from `phasekit-data`'s blobs (`phasekit_core::internal::pack`),
    naming the fluids it requires (ECS references), and `index.json` with each fluid's names, aliases, CAS number,
    bytes and requirements. `scripts/site/build.sh` writes them into the page; they are never committed.
  - The browser package exports what its registry names and has loaded (core's `Registry::names` and `loaded`), with
    Node tests.
  - The page: a fluid picker that searches names, aliases and CAS numbers; a fluid's packs fetched on first use, its
    requirements first; the bytes fetched and the fluids loaded shown, ARCHITECTURE.md §8's "only what is needed"
    made visible.

  *Red:* an xtask test: every pack decodes (`Pack::new`), its layer over the five embedded fluids resolves the fluid
  and each alias, and a fluid whose transport needs a reference computes its viscosity from its packs alone. A Node
  test (`tests/node.rs`): R32 from its pack gives the fixture values in their classes. Playwright: a fluid with an
  ECS reference fetches both packs, the reference first. *Done when:* G1-G9; the core unchanged (`gates core-frozen`
  holds through M13); the packs' total and largest sizes recorded in the PR.
- **M21.5 Mixtures.** After M13, on what MX5 brings to the browser package: a mixture picker (components from the
  index, mole fractions that sum to one, the predefined mixtures); the phase envelope on p–T and T–ρ with its critical
  point; bubble and dew points; two-phase states with each phase's composition in the readout; a liquid-liquid or
  three-phase refusal shown as the package's typed error. The step's detail is fixed when M13 closes. *Red:* Vitest:
  every envelope vertex is a bubble or dew state of the package (`Exact` against its flash at Q = 0 or 1) and the
  critical point lies on the envelope. Playwright: R410A.mix's envelope and a two-phase readout.
- **M21.6 The diagrams from M19.** The diagrams take their domes, isolines and envelopes from M19's geometry through
  the browser package, and `site/src/sample/` is deleted. *Red:* M21.2's checks on M19's geometry, run first against
  the package without its export. *Done when:* G1-G9; the screenshots re-approved in the PR.

- **M21.7 Mutation testing for the site (user decision WS5).** Worked next, before M21.2, so the diagrams' code is
  gated from its first PR.
  - The calculator's view (its state, the URL's load and write, the input text, a new input variable, the units, the
    presets) moves from `Calculator.svelte` into `site/src/lib/calculator.svelte.ts`, which the component binds to.
    Vitest tests it, its reactive wiring included (`$effect.root`, `flushSync`).
  - StrykerJS 10 with the Vitest runner (T5): `scripts/site/mutants.sh` mutates the site's TypeScript and Svelte
    files changed against `origin/main`, in incremental mode. A surviving mutant fails G9, as G8's `gates mutants`
    does for Rust, unless the Stryker config excludes it with its reason. G9's row in section 2.4 says so.
  - The weekly workflow gains a full Stryker run over `site/src`, its report an artifact.

  *Red:* the calculator module's tests, written first against its stub, cover each case of
  `tests/e2e/interaction.spec.ts`; Stryker over `src/lib` and `src/engine` as they stand, its survivors listed in the
  PR, each killed by a test or excluded with its reason. *Done when:* G1-G9; the weekly run green on its first run.

**Exit gate.** G1-G9 at every step; the page deployed from `main` at each merge; every value on the page equals the
package's (`Exact`); M21.1's budget and SL8's wasm budgets hold; `reuse lint` and `actionlint` clean. **ROT rows.**
None. **User decisions implemented.** WS1, WS2, WS3, WS4, WS5.

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
  credits NIST/fastchebpure and cites the superancillary paper (M2.5, M5.2; user decision 3a).
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
