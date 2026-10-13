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
- a performance target is missed by more than 2×, or a seam gate (zero-line core diff, `new_family.rs`) breaks.

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
| SIMD lanes | Investigated in a time-boxed spike after v0.1, shipped only if the measured gate passes (D9; Kernel's fatal flaw) | M12 |
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

**Goal.** Thread-parallel batches bitwise equal to the scalar reference; the `libm` default decided on evidence;
performance and memory gates enforced (D8, D9, D12). **Prerequisites.** M8; `rayon` and `libm` as optional core
dependencies (the first third-party dependencies of core; R2, R12); `valgrind` on the Linux CI runner for gungraun;
the reference machine (user decisions P3, RM1).

| Step | Landed |
|---|---|
| M9.1 | `ExecPolicy::Parallel` on rayon |
| M9.2 | Thread scaling recorded |
| M9.2a | Non-analytic terms at the requested order |
| M9.3 | `libm` backend |
| M9.4 | Cross-target hash, the `libm` hash committed |

- **M9.5 `libm` decision.** Apply this rule (user decision 12; threshold set here; VERIFICATION.md §9.4 cites it).
  `libm` becomes a default feature of `phasekit-core`, forwarded by compat (and capi, wasm at M10), if and only if
  **both** hold:
  1. *Bit-identity:* with `libm`, the M9.4 hash is identical on all four targets.
  2. *Cost:* on the reference machine (user decisions P3, RM1; measured on the laptop, user decision RM2), the geometric mean of the libm/std median-time ratios over the six
     scalar gate benches (α^r order 2, properties at (T, ρ), QT, PQ, PT, PH; each on the VERIFICATION.md §12 bench
     fluids, its ratio the geometric mean over those fluids; criterion, ≥ 30 samples each, same build) is **≤ 1.05**,
     and no single bench ratio exceeds **1.10**.

  *Justification:* exp throughput is the kernel's cost centre (kernel-performance §2.4), so these benches measure
  what `libm` changes. 5 % sits far inside every §7 target's margin over CoolProp (PT ≤ 3 µs vs 19-27 µs, PH ≤ 15 µs
  vs 119-376 µs), so the migration benefit is unchanged. In return, bit-identity makes cross-platform results
  `Exact`-class, which removes the arm64/FMA class of brittle tests (map 12 R17) and is what K17 asks for.
  wasm32-unknown-unknown already uses the `libm` port (kernel-performance §2.4.2; prior-art verification log item
  12), so browsers pay nothing. The 10 % cap stops one regressed path (e.g. `powf`-heavy NonAnalytic Water) hiding
  behind the mean. *(The 5 % and 10 % values are judgement, not measurement.)* *Done when:* the measured numbers
  and the outcome are written into ARCHITECTURE.md D12 and the CHANGELOG; if either condition fails, `libm` stays
  opt-in and VERIFICATION.md §5 keeps its cross-target tolerances.
- **M9.6 Performance gates enforced.** *Failing command:* `cargo xtask gates perf` (no thresholds yet). *Do:* gungraun
  instruction-count benches on Linux CI failing on a > 5 % increase against the baseline committed in this step
  *(inference: noise margin)*; criterion medians on the reference machine against the ARCHITECTURE.md §7 table (one command the user runs on the
  i7-8700K, user decision RM2). *Carried
  from M6.11, M7.2 and M7.3:* QT and PQ ran 7 to 19 times their 0.1 µs target (#88: the two-phase `State` builds both
  phases' EOS bundles eagerly, where CoolProp evaluates only the superancillary; a state that defers its bundles is one
  way); Water's PT took 38.6 µs (#92: about seven Newton steps from the ideal-gas seed, each one α^r; a better seed and
  M9.2a); PH takes about ten PT solves (#94). *Done when:* every §7 target is met, or each miss has a re-plan issue
  with the measurement (targets are estimates, K19).
- **M9.7 Memory budgets.** *Failing tests:* `eos_and_superancillary_bytes_per_fluid` (≤ 25 KiB each),
  `all_136_loaded_rss` (≤ 8 MiB, Linux; ARCHITECTURE.md §6 *(inference)*), `state_fits_256_bytes` (asserts
  `size_of::<State>() <= 256` as ARCHITECTURE.md §3.8 does and records the actual size). *Carried from M6.11:* 97 of the 130 decoded superancillaries exceed 25 KiB (median
  27 472 B, PropyleneGlycol's 39 576 B; #88). Close M9:
  set `MILESTONE = 10`. *Done when:* green or re-planned.

**Exit gate.** G1-G8 incl. `perf`; `policy_equivalence` bitwise for 1..N threads; scaling recorded (0.9·N target);
cross-target hashes recorded; the `libm` rule applied and documented; §7 targets met or re-planned; memory budgets.
**ROT rows.** M9.1: ROT-135. M9.2: ROT-031. M9.6: ROT-064.
**User decisions implemented.** 12 (`libm` decided on evidence), 11 (point-major chunks), NA1 (non-analytic terms
at the requested order).

### M10 Facades and v0.1

**Goal.** Reference states, the full PropsSI grammar, the C ABI (`pk_*` and the Tier A shim) and the browser package;
release v0.1 (D11, ARCHITECTURE.md §12). **Prerequisites.** M9; the shim library file name and the shim's
`set_config_*` behaviour (user decisions P1 and P6); tools: cbindgen CLI, `wasm-bindgen-cli` matching the crate
version, Node.js LTS (wasm-bindgen-test), cargo-about, cargo-semver-checks.

- **M10.1 IIR, ASHRAE, NBP.** *Failing tests:* `reference_states_hit_their_check_values` (n-Propane, R134a, R124;
  class `RefAnchor`, 1e-8 absolute; map 01 §8, `CoolProp-Tests.cpp:2263-2436`), `iir_requires_tc_above_273_15_k`
  (typed precondition, not a QT failure; map 15 X10), `registry_with_reference_reaches_strings`,
  `with_reference_refuses_unknown_names` (map 15 X13). *Do:* add the `refstate` kind; `ReferenceState::{Iir, Ashrae,
  Nbp, Native, Custom}` resolved by flashing anchors; `Registry::with_reference` (E10). *Done when:* green.
- **M10.2 Full PropsSI tables.** *Failing tests:* `all_85_outputs_match_oracle_or_a_typed_error` (map 01 §4a: each
  output equals the oracle within class, or both error, or a DIV entry applies), `strict_grammar_rejects_lax_keys`
  (`x(P)/y(T)|Dmolar` rejected; map 01 R20), `parsing_is_locale_free` (map 01 R17/R18),
  `all_19_pairs_by_name_in_both_bases` (map 01 R2/R3), `pair_names_round_trip_in_both_bases`,
  `fluid_string_grammar_is_strict`, `ampersand_output_is_an_unknown_output` (map 14 R9),
  `backend_options_suffix_is_refused` (map 01 R18), `compat_metadata_has_no_sentinels` (absent ODP, GWP100, ASHRAE 34 give
  `NoModel`; map 09 R17), `fill_policies_match_the_python_contract` (map 14 U1). *Do:* add the `props` kind; compat key
  tables, `HEOS::` prefix, Props1SI constants, PhaseSI strings, `FillPolicy`. *Done when:* 85/85.
- **M10.3 Derivative grammar and α-term outputs.** *Failing tests:* `first_and_second_derivative_strings_match_oracle`
  (`d(X)/d(Y)|Z`, `d(d(X)/d(Y)|Z)/d(W)|V`; `CoolProp-Tests.cpp:1744, 1829`), `alpha_term_outputs_match_oracle` (the 8
  α-term outputs, τ/δ-scaled using the published reducing constants as metadata; ARCHITECTURE.md §12). *Done when:*
  green.
- **M10.4 `pk_*` C ABI.** *Failing tests:* `c_smoke` (compiles `crates/phasekit-capi/tests/c/smoke.c` against the
  cdylib and the committed header with the platform C compiler that the `cc` crate finds (a dev-dependency of
  `phasekit-capi`, tier T3 *(inference: `cc` is not in the dependencies research; `std::process::Command` with an
  explicit compiler is the fallback)*): GCC or Clang with `-std=c99 -pedantic-errors -Wall -Werror`, MSVC `cl` with
  `/std:c11 /W4 /WX` (cl has no C99 mode *(inference)*); runs on Linux and Windows CI), `stale_handle_is_an_error`
  (generational u64), `free_during_use_gives_stale_handle_status`, `handle_generation_wraps_without_aliasing`,
  `too_small_buffer_returns_the_needed_length` (map 11 F2), `null_and_bad_lengths_return_status`,
  `panics_never_cross_the_boundary`, `pk_error_is_thread_local`, `default_library_exports_only_pk_symbols`. *Do:*
  `phasekit-capi` (`unsafe_code = "deny"` with per-item `allow` and SAFETY comments; the thread-local error and handle
  table modules allow `disallowed_types`, atomics and `thread_local!`, E13): registry, fluid, properties, batch,
  `pk_registry_with_reference`, `pk_last_error(buf, len) -> needed`; exact names fixed in this step and frozen in the
  header; `cargo xtask gates abi`. *Done when:* green; header committed; `abi` in G8.
- **M10.5 C batch.** *Failing test:* `pk_batch_equals_rust_batch_bitwise` (point-major `out[i*M + k]`, per-cell
  status). *Done when:* green.
- **M10.6 Tier A CoolPropLib.h shim.** *Failing tests:* `parameter_and_pair_codes_match_v8_0_0` (map 01 R1, R2: every
  parameter code equals the oracle's `get_parameter_index` and every phase code its `get_phase_index`; input-pair
  values equal `CoolProp.constants`; the wheel exposes no `get_input_pair_index`, so pair *names*, collisions included
  ("QS_INPUTS", "HQ_INPUTS"), come from `src/DataStructures.cpp:513-572` with `coolprop-source:` provenance;
  VERIFICATION.md §3.3), `errstring_is_readable_from_another_thread` (set on thread A, read on thread B, last writer
  wins; #3211, map 11 §8), `hapropssi_is_a_documented_stub` (returns `_HUGE` (+inf) and sets "HAPropsSI is not
  available in this version of phasekit"), `propssi_fills_inf_on_error`, `shim_exports_exactly_tier_a` (`PropsSI`,
  `Props1SI`, `PhaseSI`, `HAPropsSI`, `get_global_param_string`, `get_fluid_param_string`, `get_param_index`,
  `get_input_pair_index`, `set_config_*`), `fluids_list_ignores_unrelated_config`, `set_config_follows_decision_6`.
  *Do:* add the `codes` kind; feature `coolproplib-shim`, a separate build with CoolProp's names and signatures (D11);
  the process-wide `errstring` slot; `set_config_*` per user decision P6: a shim-side settings store (process-wide, shim only) whose keys with a phasekit equivalent become per-call options (`FlashOptions`, `DomainPolicy`, `FillPolicy`) on every shim call, display keys such as `LIST_STRING_DELIMITER` read by shim functions, every other v8.0.0 key accepted, ignored and listed in MIGRATION.md; the key-by-key mapping table is written in this step; no global state reaches the kernel. *Done when:* green on Linux and
  Windows.
- **M10.7 FP guard.** *Failing tests:* `pk_exports_restore_the_callers_fp_environment` (traps masked and
  round-to-nearest inside; the caller's control word **and sticky flags** restored on return) and
  `shim_fp_guard_matches_v8_0_0` (the three cases of `CoolProp-Tests-FPUGuard.cpp:27-82`: traps masked inside, an
  already-masked environment unchanged, flags cleared; map 11 F17: clearing is intended for polling hosts such as
  Excel/VBA). *Do:* save, set, restore in every `pk_*` export; the shim's guard additionally clears the sticky flags
  as v8.0.0 does (user decision 9b); `_controlfp_s` on Windows, `fe*` on glibc, declared directly; no-op on
  wasm. *Done when:* green on Linux and Windows.
- **M10.8 Browser WASM.** *Failing tests:* wasm-bindgen-test under Node: `water_nbp_in_range` ([373.124, 373.125] K),
  `dh_dt_at_constant_p_equals_cp` (class `Identity`), `dp_dt_sat_is_positive` (the pure-fluid parts of
  `test_wasm.mjs`; map 11 §8), `with_pack_adds_a_fluid_and_misses_are_not_cached` (E6), `batch_round_trips_float64array`
  (point-major `Float64Array` (N × M) and a `Uint8Array` status; map 11 F23), `handles_do_not_leak_over_1e5_calls`
  (create and free a fluid handle 10⁵ times; wasm memory stops growing after the first 10³ *(inference)*; map 11 §5g).
  *Do:* `phasekit-wasm` on compat with `default-features = false, features = ["fluids-core"]`: `PropsSI`, typed flash,
  one batch `evaluate`, `withPack`, `withReference`, explicit `free()` on the only handle objects; baseline and
  `+simd128` builds; `gates features` extended to the wasm tree (no `phasekit-data` features beyond `core` and its 5
  fluids) and the `.wasm` size budget (first green size + 10 %, committed in `crates/phasekit-wasm/size-budget.txt`
  *(inference: no evidence for an absolute number)*). *Done when:* both builds pass in CI.
- **M10.9 Release v0.1.** Run the release checklist (section 5.4). Close M10: set `MILESTONE = 11`. *Done when:* tag
  `v0.1.0` on a green `main`.

**Exit gate.** G1-G8 incl. `abi` and the wasm `features` check; reference states within `RefAnchor`; 85-output gate; C
smoke on Linux and Windows; shim codes match v8.0.0 and the cross-thread `errstring` handoff works; `HAPropsSI` stub
documented; both FP-guard semantics; both wasm builds; full nightly sweep green *(nightly)*; the section 5.4 checklist
complete.
**ROT rows.** M10.1: ROT-097, ROT-099, ROT-100, ROT-101. M10.2: ROT-008, ROT-057, ROT-163, ROT-164, ROT-166, ROT-167,
ROT-168, ROT-170, ROT-172. M10.4: ROT-023, ROT-148, ROT-149, ROT-150, ROT-159, ROT-169. M10.6: ROT-019, ROT-146,
ROT-147. M10.7: ROT-151. M10.8: ROT-154, ROT-157, ROT-158. M10.9: ROT-155.
**User decisions implemented.** 9a (`pk_*` + Tier A, `HAPropsSI` stub), 9b (v8.0.0 codes, process-wide `errstring`,
v8.0.0 FP-flag clearing in the shim only), 8 (wasip2 Rust only, no WIT), 1 (`pk_`), 11 (C and JS point-major), 12
(forwarded `libm` if default).

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
| M12 | core `simd` feature | v0.1 released: a time-boxed SIMD spike (the user's follow-up asks to investigate SIMD), about ten steps *(judgement)* | `fearless_simd` optional dependency, core-local `Lanes<W>` implementing sealed `Real`, crate-private `exec`, batch hooks and an `ExecPolicy` variant; wasm `simd128` build; in-house vector exp/ln (D9, ARCHITECTURE.md §7) | ≥ 2.5× α^r batch throughput on AVX2 over the M9 baseline, then the feature ships; otherwise the spike's measurements are recorded and the branch closed (D9); `policy_equivalence` bitwise; arbiter rows pass on every dispatch level; the workspace MSRV is already 1.89 (user decision MS1) |
| M13 | `phasekit-mix` | v0.1 + demand for mixtures | `Mixture: HelmholtzModel` at fixed z, own `ThermoModel` (VLE, stability, `from_split`), `MixtureModel` with AD composition derivatives, GERG-2008 datasets, compat `A[x]&B[y]`, true-mixture variants (`R410A.mix`) registered beside the pseudo-pure names (D4, user decision 4) | x = [1, 0] equals the pure fluid under any gauge (map 04 §8, map 15 §8); GERG teqp vectors (map 06 U7); Bell 2023 Table XIII; DIV-0013 resolved or kept; core diff zero expected |
| M14 | `phasekit-iapws` | v0.1 + a water/steam or solid use case | IF97 regions 1-5 with backward equations, ice Ih (R10-06) and sublimation (R14-08) on `bundle_from_gibbs`, `from_split` for ice + vapour (map 07 §9) | **Zero-line core diff**; IAPWS tables first, oracle second; `gibbs_seam` conformance on real models |
| M15 | `phasekit-pcsaft` | M11 done + demand | Hard chain, dispersion, association, polar terms on `Jet4` (map 06 U9-U11) | Zero-line core diff; teqp/FeOs cross-checks; oracle only where map 06 §8 says it is usable |
| M16 | `phasekit-py` | v0.1 + demand | pyo3 abi3 wheels, import name `phasekit`, `phasekit.compat.PropsSI(...)`, GIL released around kernel calls, NumPy `(N, M)` point-major (user decisions 7, 11; R16) | map 14 U1 Python-contract fixtures for the compat surface; installs beside `CoolProp` |
| M17 | `phasekit-incomp` | v0.1 + demand | INCOMP fits as a `ThermoModel`, placeholders rejected at datagen, `Example*` fluids excluded, restricted data off by default (map 07 §9; D14) | Source tables; known-bad register (map 07 I1-I14) |
| M18 | `phasekit-humidair` | M14 done (ice) | RP-1485 humid air on injected Water, Air and ice `Fluid`s (map 07 H3); HA strings in compat; the shim's real `HAPropsSI` replaces the stub; `pk_*` gains humid-air symbols (D11) | RP-1485 tables (map 10 §8.1, ~120 values); `HAPropsSI("H","T",300,"P",1e5,"R",0) = 27013.112479771713` (map 11 §8) |
| M19 | plotting crate | M13 done (user decision 13) | Isolines and diagrams over saturation, `flash` and batch; a separate crate | Visual regression on documented diagrams *(inference)* |
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

## 5. Cross-cutting tracks

### 5.1 Performance recording

Each path is benched as it lands (criterion; gungraun from M9.6) against the targets of ARCHITECTURE.md §7;
workloads, bench fluids, the harness and the result files are VERIFICATION.md §12. Non-blocking until M9; from M9.6
`gates perf` enforces gungraun regressions on Linux CI and the reference machine holds the absolute targets.

### 5.2 Documentation

- Rustdoc on every public item (`missing_docs`, enforced by G2); a `cargo doc --no-deps` check that denies broken intra-doc
  links is planned, but no workflow runs it yet; the crate-level doctest of `phasekit-core` executes (the seed's does).
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

1. All M0-M10 exit gates green on `main`; the nightly sweep green for the last 3 nights.
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
8. Tag `v0.1.0`; release notes list the user decisions in force and the known divergences.

## 6. Resolved follow-ups and remaining user decisions

Every decision, and the steps it applies to, is in [DECISIONS.md](DECISIONS.md). This section lists only what is
still open; an item leaves it when a step or a decision takes it.

**Open user decisions.** P4: the paywalled EOS papers (Ammonia, CO₂, R125, Air, Nitrogen, HFE143m). Until a paper
arrives its fluid stays oracle-only, labelled provisional; nothing waits on it. The thread-scaling target's wording
(issue #135): ARCHITECTURE.md §7's ≥ 0.9·N is above the laptop's own ceiling, and met on the aarch64 runner.

**Waiting for a paper.** Each item moves when a copy reaches `reference/papers/`.

| Item | Paper | Then |
|---|---|---|
| Methane's and n-Pentane's friction-theory rows (user decision FT1) | Quiñones-Cisneros and Deiters, J. Phys. Chem. B 110, 12820 (2006) | transcribe its check values or coefficients, check CoolProp's two REFPROP-substituted n-Pentane coefficients, add the rows to `viscosity_paper_rows` |
| q_D of R13, R14, R142b, R218, RC318 (DIV-0026) | Huber et al., IECR 2003 | `SkipOracle` becomes `UsePaper` |
| R123's c_p⁰ block `Tc` (DIV-0015) | Younglove and McLinden 1994 (P4) | the entry decides Corrected |
| Stored constants of map 13 R1's 15 candidates | each EOS paper | an `Investigate` entry once a paper value is in hand |
| The order-3 identities of Thorade and Saadat 2013 (user decision OD5) | not open | M7.10 checks the identities VERIFICATION.md §8.1 lists |
| The CoolProp rows of R1132(E) and VinylChloride | CC BY, but Springer refuses scripted downloads | re-check them against their papers |

**Not yet planned.**

- Cv, Cp, w, τ and δ as derivative variables (user decision OD1): no oracle (CoolProp's second derivatives refuse Cv,
  Cp and w), and τ and δ need the model trait to expose its reducing constants. A step of its own, before M10.2 and
  M10.3 need them.
- The conformance kit's `identities` and `part_additivity` (VERIFICATION.md §10): `tests/eos.rs` checks the identities
  per fluid, but an out-of-tree family has neither.
- A `cargo doc --no-deps` check that denies broken intra-doc links (section 5.2): no workflow runs it.
- The nightly report VERIFICATION.md once specified (per fluid and kind: cells asserted, cells exempt per DIV id, `nan`
  cells, oracle-gap rows, headroom, each fluid's arbiter status): the job summary lists rows and failed calls only.
