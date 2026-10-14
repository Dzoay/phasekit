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
6. **Run the gates**: G1-G8 (section 2.4) plus the step's own checks. In the same PR: raise the minimums in
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
| Step | The named failing tests existed and were red first (evidence in the PR). They are now green. G1-G8 (section 2.4) pass locally and in CI. No new `#[ignore]` outside the grammar of section 2.1. `missing_docs` is clean. The step's ROT rows are ticked. The commit carries the step ID in its `Plan-Step:` footer. |
| Milestone | Every step is done. Every exit-gate item passes with the stated numbers: items marked *(nightly)* on the first green nightly run that includes them (Linux x86-64 only); all other items per PR on Linux x86-64, Windows MSVC and wasm32-wasip2 (D15), except items checked by `phasekit-xtask` (datagen, `gates`), which is a Linux tool (section 2.5). The closing step sets `phasekit_verify::MILESTONE` to n + 1, which arms two fail-closed checks: every divergence proof due by Mn exists (`tests/divergences.rs`, VERIFICATION.md §6.3) and every ROT row of Mn is ticked (`cargo xtask gates rot`). Benches for paths that landed are recorded (section 5.1). CI is green on `main` (a milestone cannot close while CI is owed). A tag `m<n>` is set on the merge commit. |
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
| Plotting | After the mixtures milestone (user decision 13) | M19 |
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
| Steps, exit gates, G1-G8, what is still open | this plan (§3, §2.4, §6) |
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

This table is the one definition of G1-G8.

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
      ├─ M13 mixtures ────── M19 plotting
      ├─ M14 IAPWS, ice ──── M18 humid air
      └─ M12 SIMD spike, M16 Python, M17 INCOMP; on demand: Tier B shim, WIT, network service, materials
```

| M | Crate | Entry trigger | Scope | Exit gate |
|---|---|---|---|---|
| M11 | `phasekit-cubic` | v0.1 released | vdW/SRK/PR via Δ1/Δ2, Soave/MC/Twu alpha, parameters from the canonical fluid records, borrowed HEOS ideal gas, cubic superancillary (map 06 U1-U5) | Zero-line core diff; `new_family` conformance; cubic-R register entry (below); DIV-0010 proof by the entropy identity (oracle PR propane 400 K, 1 bar: T(∂s/∂T)_p = 91.35 vs cp = 93.89 J/mol/K; map 12 §6.3); Z_c = 1/3 (SRK), 0.30740 (PR) (map 06 §8) |
| M12 | core `simd` feature | Built from 2026-10-13 in a worktree; merged when green, before the `v0.1.0` tag if it is ready first (user decision RL4) | A time-boxed SIMD spike (the user's follow-up asks to investigate SIMD), ten steps: [M12 SIMD lanes](#m12-simd-lanes) | ≥ 2.5× α^r batch throughput on AVX2 over the scalar path is the target (user decisions SL3, TG1); the feature ships if it is faster than the scalar path, otherwise the spike's measurements are recorded and the stack closed (D9) |
| M13 | `phasekit-mix` | v0.1 + demand for mixtures | `Mixture: HelmholtzModel` at fixed z, own `ThermoModel` (VLE, stability, `from_split`), `MixtureModel` with AD composition derivatives, GERG-2008 datasets, compat `A[x]&B[y]`, true-mixture variants (`R410A.mix`) registered beside the pseudo-pure names (D4, user decision 4) | x = [1, 0] equals the pure fluid under any gauge (map 04 §8, map 15 §8); GERG teqp vectors (map 06 U7); Bell 2023 Table XIII; DIV-0013 resolved or kept; core diff zero expected |
| M14 | `phasekit-iapws` | v0.1 + a water/steam or solid use case | IF97 regions 1-5 with backward equations, ice Ih (R10-06) and sublimation (R14-08) on `bundle_from_gibbs`, `from_split` for ice + vapour (map 07 §9) | **Zero-line core diff**; IAPWS tables first, oracle second; `gibbs_seam` conformance on real models |
| M15 | `phasekit-pcsaft` | M11 done + demand | Hard chain, dispersion, association, polar terms on `Jet4` (map 06 U9-U11) | Zero-line core diff; teqp/FeOs cross-checks; oracle only where map 06 §8 says it is usable |
| M16 | `phasekit-py` | v0.1 + demand | pyo3 abi3 wheels, import name `phasekit`, `phasekit.compat.PropsSI(...)`, GIL released around kernel calls, NumPy `(N, M)` point-major (user decisions 7, 11; R16) | map 14 U1 Python-contract fixtures for the compat surface; installs beside `CoolProp` |
| M17 | `phasekit-incomp` | v0.1 + demand | INCOMP fits as a `ThermoModel`, placeholders rejected at datagen, `Example*` fluids excluded, restricted data off by default (map 07 §9; D14) | Source tables; known-bad register (map 07 I1-I14) |
| M18 | `phasekit-humidair` | M14 done (ice) | RP-1485 humid air on injected Water, Air and ice `Fluid`s (map 07 H3); HA strings in compat; the shim's real `HAPropsSI` replaces the stub; `pk_*` gains humid-air symbols (D11) | RP-1485 tables (map 10 §8.1, ~120 values); `HAPropsSI("H","T",300,"P",1e5,"R",0) = 27013.112479771713` (map 11 §8) |
| M19 | plotting crate | M13 done (user decision 13) | Isolines and diagrams over saturation, `flash` and batch; a separate crate | Visual regression on documented diagrams *(inference)* |
| M20 | core, compat, verify, CI | M12 landed (user decision PL1) | Four follow-ups: Cv, Cp, w, τ and δ as derivative variables; the conformance kit's `identities` and `part_additivity`; the intra-doc link check; the nightly report: [M20 Follow-ups](#m20-follow-ups) | Each step's checks; G1-G8 |
| — | Tier B shim | A host needs `AbstractState_*` | ~25 exports, generational handles (map 11 U11) | v8.0.0 signatures; fuzzed |
| — | WIT component | A concrete non-Rust WASI host (user decision 8) | `wit-bindgen` component over compat (R17) | Host smoke test |
| — | Network service facade | A consumer needs phasekit over the network (BRIEF "connected") | A thin HTTP or RPC service over compat in its own crate; never core (ARCHITECTURE.md §1) | Service smoke test; core diff zero |
| — | Materials | First phase-coexistence need (ice + water) | `Substance` + min-Gibbs selector in a new crate (D10; materials R1) | Zero-line core diff |
| — | Near-critical VLE in extended precision | A use needs the VLE itself at multiprecision accuracy near Tc (idea recorded at the user's request, 2026-10-09) | Near Tc the double-precision VLE carries its rounding amplified as Θ^−1.5 (user decision NC1, measured at M6.4). Double-double (or similar) arithmetic in the near-critical solve, as fastchebpure computes its multiprecision values. A scope change: the core is std-only until M9 and has performance targets | The dense fastchebpure rows within `SatMp`'s far-field 1e-11 closer to Tc than today; no cost to the double-precision path (the M9 benches) |

**M11 cubic-R register entry** (user decision 10; ARCHITECTURE.md §10, §11). Added in the first M11 step, before any
cubic code.

| Field | Value |
|---|---|
| id | next free DIV id |
| fluids / part | every cubic fluid that borrows a HEOS ideal gas / `Part::GasConstant` |
| policy, fix | `UsePaper`, `fix: Code` (`phasekit-cubic` selects the R of the ideal part by `DataSet` when it builds the model; no `corrections.csv` row, so `gates register` accepts the entry without a data patch, VERIFICATION.md §7.2) |
| arbiter | the borrowed HEOS model's own α⁰ (its EOS paper's ideal-gas equation; IAPWS-95 for Water) |
| Parity | the ideal part is evaluated with the cubic's own R, CODATA 2018 8.31446261815324 as in v8.0.0 (map 06 C6, map 15 §2.1) |
| Corrected | the ideal part uses the source model's R, so α⁰ equals its paper |
| proof | (1) Corrected: cubic α⁰ equals the borrowed model's at the same (T, ρ) (`Exact`) and cp⁰(PR::X) equals cp⁰(HEOS::X) (`Term`); (2) the oracle's cubic cp⁰ differs by the R ratio (1.128e-6 = 8.314472/8.31446262 − 1 for 54 of 97 fluids, map 06 C6; 1.10e-5 for Water, map 15 §3.1); (3) Parity reproduces the oracle except the DIV-0010 columns |
| scope note | part (3) holds only where the v8.0.0 cubic α⁰ snapshot matches the HEOS α⁰ apart from R; the drifted fluids (map 06 C5: cp⁰ > 0.1 % off for 11 of 116; map 15 X6 offsets) get a separate `SkipOracle` entry for α⁰-dependent cubic outputs |

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

- **M12.4 The `simd` feature.** `simd = ["libm", "dep:fearless_simd"]` (fearless_simd 1.x, default features off but
  `std`): `Level::new()` once per batch, `dispatch!` into `run_lanes` with W = 2 (SSE2, SSE4.2, NEON, simd128), 4
  (AVX2) or 8 (AVX-512), the kernel path `#[inline(always)]` down to the lane operations; transcendentals still per
  lane. `gates deps` and `features` learn the feature; DEPENDENCIES.md T1 lists it as present; CI runs the lane tests
  with `--features simd` at every level each runner has (forced down from `Level::new()`), on aarch64, and on wasip2
  with `+simd128`. If the inlining moves the scalar instruction counts, `gates perf --update` commits them with the
  counts in the PR. *Red:* `policy_equivalence` at each forced level, against a dispatch stub that refuses the batch.
  *Done when:* green on every level of the i7-8700K (SSE2, SSE4.2, AVX2) and of the CI runners, and the lanes' time
  with per-lane `libm` is recorded (the ceiling user decision SL1 estimated).
- **M12.5 Vector `exp`.** `Lanes::exp` ports the locked `libm` crate's `exp` operation for operation, its special
  cases as lane selects inside `Lanes`. *Red:* `lane_exp_is_libm_bitwise` (VERIFICATION.md §9.2) at every level: random
  inputs over α^r's argument range and over all of `f64`, and the edges ±0, |x| < 2⁻²⁸, the reduction boundaries at
  0.5 ln 2 and 1.5 ln 2, 708.39…, 709.78…, −745.13…, subnormal results, NaN and ±∞.
- **M12.6 Vector `ln`.** The same for `log`. *Red:* `lane_ln_is_libm_bitwise`: random inputs and the edges ±0,
  negative inputs, subnormals, 1, the √2/2 reduction boundary, +∞ and NaN.
- **M12.7 The gate.** Bench `alphar_batch/simd` on the i7-8700K (AVX2). It ships at ≥ 2.5× over M12.1's
  `alphar_batch/scalar` of the default build for n-Propane, the §7 row's 16-20-term fluid (user decision SL3); the
  other bench fluids are recorded. `gates perf --targets` gains the ≤ 0.1 µs/state row, and a miss names its re-plan
  issue in `ci/perf-exemptions.txt` (SL3). *Red:* `targets_match_the_architecture_table` with the row added to
  `TARGETS` first. *Done when:* the record and the targets check pass, or the gate fails and the stack closes (Entry).
- **M12.8 Properties at (T, ρ) in lanes.** `Pair::DT` batches under the lanes variant: α⁰ and the relations from the
  bundle in lanes. Points inside the dome (T < T_c, ρ between the saturated densities), and every point a lane kernel
  refuses, take the scalar path in place. α⁰'s other transcendentals run per lane through `math` unless a measurement
  justifies a port like M12.5's. *Red:* `policy_equivalence` over DT batches of every embedded fluid on the `eos` and
  `flash` grids, in-dome points included. Bench `properties_at_t_rho_batch/{scalar,simd}`.
- **M12.9 QT and PQ in lanes.** `Pair::QT` and `Pair::PQ` batches: the superancillary's Clenshaw sums (and T from p
  for PQ) in lanes, then both phases' bundles through M12.8. *Red:* `policy_equivalence` over QT and PQ batches on
  the `sat` grid. Benches `{qt,pq}_superancillary_batch/{scalar,simd}`.
- **M12.10 WASM, and M12 closed.** `phasekit-wasm`'s `simd128` build enables `simd` (one compile-time level); its
  size budget and Node tests on both builds. The i7-8700K's M12 record; ARCHITECTURE.md §7 and VERIFICATION.md §12
  say what landed. Tag `m12` with the user's approval.

**Exit gate.** G1-G8 with and without `--features simd`; `policy_equivalence` bitwise at every dispatch level of the
i7-8700K, the CI runners (x86-64, aarch64) and wasip2 with `simd128`; the lane `exp`/`ln` bitwise equal to `libm`'s;
the `simd` build reproduces `fixtures/hash/libm.txt`; the K1/K2 paper rows pass at every level (VERIFICATION.md §9.2);
≥ 2.5× (SL3). **ROT rows.** None. **User decisions implemented.** SL1, SL2, SL3.

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
  asserts VERIFICATION.md §8.1's list in class `Identity`, moved into the kit from `tests/eos.rs`, which then calls it
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
| The CoolProp rows of R1132(E) and VinylChloride | CC BY, but Springer refuses scripted downloads | re-check them against their papers |
