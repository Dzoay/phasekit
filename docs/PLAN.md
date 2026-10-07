# phasekit implementation plan

**Status:** the step-by-step TDD plan for implementing [ARCHITECTURE.md](ARCHITECTURE.md), written 2026-10-05 and
revised the same day after two adversarial reviews ([design/05-plan-review-log.md](design/05-plan-review-log.md)). The
contract is the user's requirements, verbatim in [BRIEF.md](BRIEF.md) §1. The user's answers in
[design/04-user-decisions.md](design/04-user-decisions.md) (first pass 2026-10-05) override any older text. Written
alongside [VERIFICATION.md](VERIFICATION.md) (how truth is established) and [ROT-REGISTER.md](ROT-REGISTER.md)
(CoolProp rot → mechanism → test).
**Readers:** the user, and AI coding agents who execute one step at a time. Every step names its failing test, the
truth it uses, the code it touches and the check that ends it.
**Citations:** "map NN §X", "map NN R#" (or another item id such as F#, U#, X#, C#) = `docs/coolprop-map/NN-*.md`. K#,
R#, P#, T# = recommendation tables of `docs/research/{kernel-performance,dependencies,prior-art}.md`. D1-D17 =
ARCHITECTURE.md §4. "User decision N" = row N of 04-user-decisions.md (the licence row is "D14"). ROT-NNN = rows of
ROT-REGISTER.md. S-##/E## = critic issues in [design/03-decision-log.md](design/03-decision-log.md). Anything else is
marked *(inference)*.
**Names:** project `phasekit`; crates `phasekit-core`, `phasekit-data`, `phasekit-compat`, `phasekit-verify`,
`phasekit-xtask`, later `phasekit-capi`, `phasekit-wasm`, `phasekit-py`; Rust paths `phasekit_core` etc.; C prefix
`pk_`. The historical design records in `docs/design` use the old placeholders `cprs-*`/`cp_*` and are never edited.
**One home per artefact.** Every path, file name and gate is listed once in §2.3 and §2.4 with the section that
defines it. This plan is normative for steps, gates G1-G8 and the `libm` threshold; VERIFICATION.md is normative for
fixtures, kinds, tolerance classes, the register schema, xtask gate rules, the cross-target hash and the bench
harness; ROT-REGISTER.md is normative for rot rows and their proof-test names.

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
7. **Commit and merge** (section 2.2). `main` is green after every merge.

Scaffolding steps with no runtime behaviour (toolchain, CI, licence files) use a failing *command* in place of a
failing test; the step names that command.

Code from the type sketch (`docs/design/sketch`, compiled and tested) may be copied when a step says so, but its tests
are copied **first** and seen red against a stub (or, for the one-off seed import at M0.2, seen green as the
baseline). The sketch is never edited.

### 0.2 Definition of done

| Level | Done means |
|---|---|
| Step | The named failing tests existed and were red first (evidence in the PR). They are now green. G1-G8 (section 2.4) pass locally, and in CI once M0.6 exists. No new `#[ignore]` outside the grammar of section 2.1. `missing_docs` is clean. The step's ROT rows are ticked. The commit carries the step ID in its `Plan-Step:` footer. |
| Milestone | Every step is done. Every exit-gate item passes with the stated numbers: items marked *(nightly)* on the first green nightly run that includes them (Linux x86-64 only); all other items per PR on Linux x86-64, Windows MSVC and wasm32-wasip2 (D15), except items checked by `phasekit-xtask` (datagen, `gates`), which is a Linux tool (section 2.5). The closing step sets `phasekit_verify::MILESTONE` to n + 1, which arms two fail-closed checks: every divergence proof due by Mn exists (`tests/divergences.rs`, VERIFICATION.md §6.3) and every ROT row of Mn is ticked (`cargo xtask gates rot`). Benches for paths that landed are recorded (section 5.1). CI is green on `main` (from M1 on, a milestone cannot close while CI is owed, section 3 M0.6). The VERIFICATION.md §14 row is satisfied. A tag `m<n>` is set on the merge commit. |
| v0.1 | M10 is done and the release checklist (section 5.4) is complete. |

### 0.3 When CoolProp and the literature disagree

CoolProp 8.0.0 is a provisional oracle; printed check tables arbitrate (ARCHITECTURE.md thesis, D13). When a Rust
value and the oracle differ beyond the tolerance class (VERIFICATION.md §5), follow VERIFICATION.md §6.4:

1. **Suspect your own code first.** Re-run the L1 and L4 checks (AD vs fast path, identities, FD). Most disagreements
   are ours.
2. **Check the stored constants** (R, M, T_r, ρ_r, the per-block `Tc`) against the paper before blaming the evaluator
   (map 13 §3 "Lesson"; R1234ze(E), Helium and R123 were constant problems).
3. **Find an arbiter** (paper, IAPWS, multiprecision point; map 13 §8). A table is an arbiter only if it reproduces
   with its own constants (map 13 R3; `ARBITERS` status `SelfConsistent`, VERIFICATION.md §4.3).
4. **If Rust matches the arbiter and the oracle does not:** allocate the next DIV id, add the entry to
   `crates/phasekit-verify/src/register.rs` with its policy (`UsePaper`, `SkipOracle`, `KeepOracle`, `Investigate`),
   its `fix` (`Data` with a row in `data/corrections.csv`, or `Code`) and its `proof` milestones, and write the proof
   (three parts for `UsePaper`: Corrected matches the arbiter, the oracle still differs, Parity matches the oracle).
   That is its own commit inside the step.
5. **No arbiter:** break the tie with an independent evaluation (AD of the paper formula, or an mpmath evaluation of
   the JSON coefficients; map 10 §8.5). If it is still open, the entry is `Investigate`: the oracle cells on Parity
   stay asserted, literature rows are asserted at `Measured` (the residual recorded in the entry), and the proof pins
   the measured disagreement and names the action that resolves it (VERIFICATION.md §6.2-§6.3).
6. **Never** widen a tolerance to make CoolProp pass (map 10 R4/R5), never "fix" a printed value without a published
   erratum (map 10 §8.5 rule 3), never skip a failing oracle row without a register entry.

### 0.4 When a step fails its gate: stop and re-plan

Stop when any of these is true, rather than pushing on:
- the step has grown past about twice its planned size, or needs work that belongs to a later step;
- the fix would change a public type or trait in ARCHITECTURE.md §3, a decision D1-D17, or a user decision;
- a gate could pass only by weakening it (a tolerance, a skipped test, a lint `allow`);
- a new third-party dependency outside the tiers of dependencies §3.1 seems needed;
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
| TTSE/BICUBIC, REFPROP backend | Not ported: crashes and garbage reproduced on the oracle (map 08 R1-R9); proprietary and process-global (map 07) | ROT-REGISTER.md section 5 |
| Runtime CoolProp JSON (`json` feature) | No consumer yet (S-12) | When a consumer is scheduled |
| Materials abstractions (`Substance`, min-Gibbs selection) | "One implementation is a guess" (materials R1); seams exist now (D10) | On first coexistence need |

### 1.1 Requirements traced to steps

One row per phrase of BRIEF §1 (mechanisms: ARCHITECTURE.md §1); each row names the steps that build it and the gate
that keeps it.

| BRIEF §1 phrase | Built by | Kept by |
|---|---|---|
| "a plan to migrate coolprop to rust" | this plan, M0-M10 | §0.2 definition of done |
| "modular extensible" ... "modularity is again KEY" | M0.2 crates; M5.10 out-of-tree family and Gibbs solid; M11, M14 families | `new_family.rs`, `gibbs_seam.rs` (G3, G4); `gates core-frozen` (M11, M14) |
| "memory safe" | M0.2 `unsafe_code = "forbid"` in the workspace lints; M10.4 capi `deny` with per-item `allow` | `gates lints` (every member inherits the workspace lints; only capi relaxes `unsafe_code`), M0.4 |
| "cross platform" ... "windows, linux, and wasm" | M0.5 wasip2 runner; M0.6 CI matrix; M10.8 browser package | G4, G5, G6; CI windows, aarch64, wasm-browser jobs |
| "connected" (embeddable in services, simulators, browsers) | M2.6 registries as values with layers; M9.1 shared registry under threads; M10.4 C ABI; M10.8 browser package with `withPack`; Rust on wasip2 (M0.5) | `policy_equivalence` (M9); C smoke (M10.4); JS smoke (M10.8). A network server is an on-demand facade (section 4) |
| "rot in there we can eliminate" | each milestone's **ROT rows** | `gates rot` (M0.4) |
| "DRY, SOLID code in idiomatic modern rust" | M0.2 lints; one pair table, one input gate, one `Prop` table (seed); one EOS encoder (M2.4) | G2, G7 |
| "minimal dependencies" | M0.4 zero-dependency guard; M0.7 `deny.toml` | `gates deps`; `cargo deny check`; `cargo shear` |
| "heavy OSS clear type constraints" | M0.7 licences, REUSE; newtypes and typed errors (seed, M1.14 proptest) | `reuse lint`, `cargo deny check`; proptest |
| "a kernal we can optimize for speed and scalability" | benches from M1.15 (baseline) and M2.9 on; M9.1 rayon | `gates perf` (M9.6) |
| "parallel requests on the same and different fluids" | seed `Send + Sync` asserts; M9.1-M9.2 | `one_to_n_threads_bitwise`, thread-scaling record (M9.2) |
| "Rusts immutability should help us here" | seed: immutable `Arc` models, `Copy` `State`; M0.3 bans on locks, cells, atomics, `thread_local!` | M0.3 `clippy_bans_fire` |
| "coolprop as varification but with source material when we find bugs" | M1 kit, oracle and paper corpus; M2.7 Parity/Corrected; divergence register | `tests/divergences.rs`; `gates register` |
| "We will TDD this step by step" | §0.1 step loop; M0.4a test-quality gates (user decisions TQ1, TQ2) | red evidence in every PR; `gates counts`; `gates assertions`; `gates mutants` |
| "extend it to be a *material* properties library" / "all states of matter" | M5.10 Gibbs seam; M14 ice and IF97 | `gibbs_seam.rs`; `gates core-frozen` (M14) |
| "coolprop fluids are first" | M2-M8 for all 136 fluids | exit gates M3-M8 |
| "only loading what is needed" | M2.6 lazy slots; M8.8 ECS references on first need | `lazy_load.rs`; `thermo_only_workloads_never_load_references` |
| "highfrequency and batch requests" | M2.9 lookup; M5.8 batch; M9.1 parallel batch | `batch_point_allocates_nothing`; perf targets (M9.6) |
| "Layers will be important but we must not over engineer this" | one kernel crate with module layers; crates only with their milestone (S-11) | §0.4 stop rules |
| Follow-up: "parallel computation ... side by side implementations ... SIMD to investigate" | M9.1 `ExecPolicy::Parallel`; M12 spike | `policy_equivalence`; M12 gate (≥ 2.5× or stop) |

## 2. Conventions

### 2.1 IDs and references

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
- **Divergences** are `DIV-0001`... (ARCHITECTURE.md §10; 14 seeded). New ones take the next free number when they are
  added to `register.rs` (section 0.3); the PR adds the same row to VERIFICATION.md §6.6.
- **Tests** cite their truth in a doc comment: `/// Arbiter: IAPWS R6-95(2018) Table 6`, `/// Oracle: CoolProp
  8.0.0, fixtures/coolprop-8.0.0/eos/Water.csv` or `/// Rot: ROT-069`.
- **Ignores** follow VERIFICATION.md §11.2 `ignores`: the reason starts with `DIV-NNNN: `, `issue #N: ` or `nightly: `
  (`nightly:` only in `tests/sweeps.rs`); anything else fails G8 (map 10 R14).

### 2.2 Branches, commits, PRs, versions, toolchain

CONTRIBUTING.md is the one definition of these rules; this is the summary agents need per step.

- One step per branch and PR: branch `m3.2-power-oracle`.
- **Conventional Commits 1.0.** The PR title is the squash-commit subject: `<type>(<scope>): <imperative summary>`,
  types `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`, `chore`; scope = the crate without its
  prefix (`core`, `data`, `compat`, `verify`, `xtask`, `capi`, `wasm`, `py`) or `plan`, `deps`, `repo`. A breaking
  change adds `!` and a `BREAKING CHANGE:` footer. Footers: `Plan-Step: M3.2` (required on plan steps), optional
  `Rot: ROT-074`, `Div: DIV-0012`. Example: `feat(core): power-term jets match the oracle` + `Plan-Step: M3.2`.
- **Squash merge is the only merge method** (user decision P7), so `main` is linear, has one commit per PR and stays
  green; the PR body keeps the red evidence. The `main` ruleset requires a PR, linear history and (from M0.6) green
  CI, and forbids force-push and deletion.
- **Commit identity:** GitHub identity only. The maintainer commits as `Dzoay <3277116+Dzoay@users.noreply.github.com>`
  (repo-local `git config`); contributors use their own GitHub no-reply address. Never a personal email (user rule).
- **SemVer 2.0.** All published crates share `[workspace.package] version`. Before 1.0, a breaking change bumps the
  minor version (0.1 → 0.2) and everything else bumps the patch; after 1.0, standard SemVer. `cargo semver-checks`
  gates every release (section 5.4). Releases are tags `vX.Y.Z` on `main`; `CHANGELOG.md` is generated from the
  Conventional Commit subjects since the last tag. The pinned v8.0.0 shim codes and the `phasekit-data` `DATASET` id
  are versioned on their own, not by the crate version.
- Generated files (`phasekit-data` blobs and index, fixtures and their manifest lines, cbindgen header) are committed in
  the step that changes their generator; CI regenerates and diffs them.
- No `rust-toolchain.toml` (D17). Develop on the latest stable (1.99.0 today); the workspace MSRV is 1.85. CI installs
  the stable version named once, as the default of `.github/actions/rust/action.yml`. A new stable that turns any gate
  red (for example a new clippy lint under `-D warnings`) is handled in a dedicated `chore: rust 1.NN` PR that fixes the
  code and bumps that version; a blanket `allow` is never the fix (section 0.4). Agents use the version CI names.
- `Cargo.lock` is committed (dependencies §3.4 item 8).

### 2.3 Names and paths (shared table)

Every artefact has one path and one defining section. VERIFICATION.md uses the same names.

| What | Path | Defined in |
|---|---|---|
| Workspace manifest, lints, `clippy.toml`, `rustfmt.toml`, `deny.toml`, `REUSE.toml`, `Cargo.lock` | repo root | D17; M0.2, M0.7 |
| Cargo alias `cargo xtask`, wasip2 runner | `.cargo/config.toml` | M0.2, M0.5 |
| Crates | `crates/phasekit-*` (`members = ["crates/*"]`) | ARCHITECTURE.md §2 |
| Unit tests | `#[cfg(test)] mod tests` in the module they test | — |
| Corpus (integration) tests | `crates/phasekit-verify/tests/<area>.rs` (`divergences.rs`, `arbiters.rs`, `eos.rs`, `sat.rs`, `flash.rs`, `transport.rs`, `new_family.rs`, `gibbs_seam.rs`, `lazy_load.rs`, `sweeps.rs`, ...) | VERIFICATION.md §2 |
| Oracle lock | `crates/phasekit-verify/fixtures/oracle.lock` | VERIFICATION.md §3.2 |
| Fixture manifest (committed fixtures only) | `crates/phasekit-verify/fixtures/MANIFEST.sha256` | VERIFICATION.md §3.4 |
| Oracle fixtures, core subset (14 fluids) | `crates/phasekit-verify/fixtures/coolprop-8.0.0/<kind>/<Fluid>.csv` (+ `<Fluid>.edge.csv`) | VERIFICATION.md §3.4-§3.6 |
| Oracle fixtures, all-fluid tier | `crates/phasekit-verify/fixtures/coolprop-8.0.0/all/<kind>.csv` | VERIFICATION.md §3.6 |
| Oracle facts | `crates/phasekit-verify/fixtures/coolprop-8.0.0/facts/<set>.csv` (`smoke`, `register`) | VERIFICATION.md §3.5 |
| Printed check tables (double entry) | `crates/phasekit-verify/fixtures/paper/<Fluid>/<bibkey>.<table>.csv` and `<bibkey>.<table>.check.csv` | VERIFICATION.md §4.2 |
| Multiprecision points | `crates/phasekit-verify/fixtures/mp/check-points.csv`; `crates/phasekit-verify/fixtures/mp/fastchebpure-2026.06.02-v2/<Fluid>.csv` with `fixtures/mp/fastchebpure.lock` | VERIFICATION.md §3.4 |
| Cross-target hashes | `crates/phasekit-verify/fixtures/hash/` | VERIFICATION.md §9.4 |
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
| Executed-test minimums | `ci/test-counts.txt` (`<target> <package>::<binary> <min>`; `# lowered: <reason>` and `# pending M5.9: <test>` comments) | VERIFICATION.md §11.2 `counts` |
| CI workflows | `.github/workflows/{ci,nightly,weekly}.yml`, toolchain action `.github/actions/rust`; `pr-title.yml` (exists since 2026-10-05, CONTRIBUTING.md) | VERIFICATION.md §11.3; M0.6 |
| C header (M10) | `crates/phasekit-capi/include/phasekit.h` (cbindgen CLI output, committed; R15) | M10.4 |

Committed fixtures are read with `include_str!`, so the corpus runs on wasip2 without filesystem access. Their sizes:
the core subset about 8 MiB, the all-fluid tier about 2.5 MiB *(inference)*, within the 16 MiB budget that `gates
fixtures` enforces (VERIFICATION.md §3.6). The full set (~3.5 GB) is regenerated nightly, never committed (user
decision 3b).

### 2.4 Standard gates

This table is the one definition of G1-G8. Agents set `CARGO_TARGET_DIR` to their own scratch directory; humans may
use the gitignored `target/`.

| Gate | Command | Where it runs | From |
|---|---|---|---|
| G1 | `cargo fmt --all --check` | local; CI linux | M0.2 |
| G2 | `cargo clippy --workspace --all-targets -- -D warnings` | local; CI linux | M0.2 |
| G3 | `cargo nextest run --workspace --profile ci` and `cargo test --doc --workspace` (nextest runs every test in its own process, all binaries in one parallel pool, and runs no doctests; `.config/nextest.toml`) | local (every crate); CI linux (every crate), windows and aarch64 (`--exclude phasekit-xtask`) | M0.2; nextest since 2026-10-07 |
| G4 | `cargo nextest run --workspace --exclude phasekit-xtask --target wasm32-wasip2 --profile ci` and `cargo test --doc` with the same arguments (runner `wasmtime`) | local; CI wasip2 | M0.5; nextest since 2026-10-07 |
| G5 | `cargo check --workspace --target wasm32-unknown-unknown` | local; CI wasm-browser | M0.2 |
| G6 | `cargo check --workspace --all-targets --target x86_64-pc-windows-msvc` | local (check only: no MSVC linker); CI windows runs G3 | M0.2 |
| G7 | `cargo clippy -p phasekit-core --no-default-features --all-targets -- -D warnings` (+ `-p phasekit-compat` from M5.9) | local; CI linux | M0.2 |
| G8 | `cargo xtask gates all`: `deps`, `lints`, `counts`, `ignores`, `doc-excerpts`, `rot` (M0.4); `assertions`, `mutants` (M0.4a); `fixtures`, `register` (M1); `datagen` (M2); `features` (M5); `perf` (M9); `abi` (M10); `core-frozen` (M11, M14). Rules: VERIFICATION.md §11.2 | local; CI linux (needs `reference/CoolProp`, fetched and cached by the job) | M0.4 |

CI-only work (M0.6): Windows MSVC and aarch64 test runs, MSRV (`cargo +1.85 check --lib` for core, data and compat),
the nightly oracle sweep, the weekly latest-dependency test, gungraun benches (from M9). `cargo deny check`, `reuse
lint` and `cargo shear` run locally and in CI.

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
- Every new dependency is named in its step, respects the tiers of dependencies §3.1 (core: std only until M9), and
  passes `cargo deny check`. Dev-dependencies that do not build for wasm (criterion, gungraun) go under
  `[target.'cfg(not(target_family = "wasm"))'.dev-dependencies]`; gungraun under `cfg(target_os = "linux")`, its bench
  file compiling to an empty `main` elsewhere so G6 `--all-targets` still checks *(inference)*.
- **Personal data:** no request to any external service (oracle download, paper or fastchebpure download, registry,
  CI) ever carries the user's email address or another personal identifier (user rule; AGENTS.md).

## 3. Milestones to v0.1

| M | Deliverable (D15, refined) | Steps | Key exit criterion |
|---|---|---|---|
| M0 | Toolchain, workspace seeded from the sketch, lints, xtask gates, test-quality gates, CI (user checkpoint), licences | 8 | G1-G8 green; 43 seed tests; zero third-party deps |
| M1 | Verification kit, oracle generator and lock, register and arbiter machinery, paper corpus, proptest, C++ baseline (time, memory, threads), mp check points | 18 | Fixture round trip bit-exact; `from_printed("21.17909")` rejects the oracle; 14/14 register facts |
| M2 | Datagen, blob v1, index, features, Parity/Corrected, hash gate, readable fluid dump, citations on real data, caloric-curve contract | 12 | 130 FNV stamps; 556 keys, 0 collisions; Parity vs Corrected = 3 patches |
| M3 | Separable residual kinds on real data, `Jet4` with num-dual oracle | 8 | Jets = AD (class `Term`); oracle block isolation; α^r totals of 134 fluids |
| M4 | NonAnalytic, all ideal kinds, all 136 compile | 7 | IAPWS-95 Table 6; Water/CO₂ critical `Undefined` |
| M5 | Relations, SA evaluation, caloric curves, DT, gauge, partials, virials, batch, compat, seam gates | 12 | Lemmon 2016 Table 7; DIV-0001 proof; analytic virials; seams green |
| M6 | fastchebpure fetch, roots, pure VLE, critical points, QT/PQ, Guess polish, pseudo-pure rules, saturation arbiters | 11 | 390 mp points; exact rescaling; R410A rows |
| M7 | All 19 pairs, phase rule, second partials, fundamental derivative | 10 | 19 × 2 capability matrix; `Ambiguous` cases |
| M8 | Transport decode and arbiters, σ, transport (staged, IAPWS, ECS), melting | 12 | Paper rows stage by stage; DIV-0002, -0004, -0009 |
| M9 | rayon, `libm` decision, perf and memory gates enforced | 7 | 1/N threads bitwise; cross-target hash; libm rule applied |
| M10 | Reference states, full compat, `pk_*`, Tier A shim, browser WASM → v0.1 | 9 | 85-output gate; C smoke; shim codes + errstring; both wasm builds |

### M0 Toolchain, workspace and CI

**Goal.** A workspace at the repo root with the four M0 crates seeded from the sketch, every lint and gate armed, CI
green on all targets. **Prerequisites.** Rust 1.99.0 with the targets x86_64-unknown-linux-gnu, wasm32-unknown-unknown,
wasm32-wasip2, x86_64-pc-windows-msvc, clippy and rustfmt (installed); `uv`; `reference/CoolProp` fetched by
`scripts/fetch-coolprop.sh` (anonymous `git clone` of CoolProp, checkout ae81610e; written in M0.2).

- **M0.1 Toolchain check and wasmtime.** *Failing command:* `scripts/check-toolchain.sh`, which asserts `rustc` ≥ 1.85
  (and reports the version), the 4 targets in `rustup target list --installed`, `wasmtime --version`, `cargo deny
  --version`, `cargo shear --version`, `uv --version`, and warns (not fails) on missing `cmake`/`g++` (needed at
  M1.15). It fails today: no wasmtime, no cargo-deny (checked 2026-10-05). *Do:* write the script; install with
  `cargo install --locked --root ~/.local wasmtime-cli cargo-deny cargo-shear`, so the binaries land in `~/.local/bin`,
  which is on this box's `PATH` (`~/.cargo/bin` is not; rustup is system-wide); the script prints the PATH fix when a
  tool is installed but not found. Never install into the repo. *Done when:* the script exits 0; versions noted in the
  PR.
- **M0.2 Workspace seeded from the sketch.** *Failing command:* `cargo test --workspace` at the repo root (no
  manifest). *Do:* copy from `docs/design/sketch` the root `Cargo.toml` (`resolver = "3"`, edition 2024,
  `rust-version = "1.85"`, `license = "MIT OR Apache-2.0"`, `publish = false`, workspace lints, internal deps with
  `default-features = false`), `clippy.toml`, `rustfmt.toml`, and the crates `phasekit-core`, `phasekit-data`,
  `phasekit-verify`, `phasekit-xtask` with their tests. **Not** `phasekit-compat` (S-11: it joins at M5.9). Four seed
  tests call `phasekit_compat` and stay in the sketch until M5.9: `new_family.rs::compat_strings_reach_the_new_family`,
  `new_family.rs::reference_states_work_for_any_family`,
  `gibbs_seam.rs::solid_reaches_registry_batch_compat_and_reference_states` and
  `gibbs_seam.rs::two_phase_states_from_outside_the_core`. Drop the compat dev-dependency, its `[workspace.dependencies]` entry, and the
  imports and the helper (`gibbs_seam.rs::qt`) that only those tests use; change nothing else. Add `.cargo/config.toml` with
  `xtask = "run --quiet -p phasekit-xtask --"`, `scripts/fetch-coolprop.sh` and `.gitignore` entries (`/target`,
  `/reference/`, `crates/phasekit-verify/fixtures-full/`). *Done when:* G1-G3 and G5-G7 pass and 43 tests execute
  (the 47 sketch tests minus the 4 held back; measured on a scratch copy 2026-10-05, clippy clean).
- **M0.3 Lint probes.** *Failing test:* `xtask` test `clippy_bans_fire` runs clippy on a scratch copy of a probe
  crate (`crates/phasekit-xtask/probes/bans/`, excluded from the workspace) that calls each of the 27 banned `f64`
  methods (incl. `mul_add`), `std::env::{var, var_os, home_dir}`, the `std::fs` entry points, uses the 5 banned types,
  `std::sync::atomic` types, `thread_local!`, `println!`/`eprintln!`, `unwrap`, `expect`, `todo!`, `panic!`, `dbg!`,
  and asserts every one is reported (E16 made permanent; the sketch probe covered 15). *Do:* add to the workspace
  lints `print_stdout = "deny"` and `print_stderr = "deny"` (ROT-024; xtask allows them); add to `clippy.toml`
  `disallowed-methods` for `std::fs::*` entry points and `std::env::home_dir` (ROT-153), `disallowed-types` for
  `std::sync::atomic::*` (ROT-031) and `disallowed-macros` for `std::thread_local` (ROT-268) *(inference: clippy's
  `disallowed_macros` covers std macros by path)*; the capi crate (M10.4) is the only place allowed atomics and
  `thread_local!`, per module. *Done when:* all entries fire, or an entry that cannot fire is removed from
  `clippy.toml` with a comment saying why (never kept silently).
- **M0.4 xtask gates v0.** *Failing tests:* `count_executed_parses_libtest_summary` (sums `N passed` lines per test
  binary), `zero_executed_is_a_failure` (map 10 R1), `ignore_without_a_valid_reason_is_rejected` (map 10 R14; the
  grammar of section 2.1), `zero_deps_rejects_a_third_party_crate` (parses `cargo tree -e normal,build --depth 1
  --prefix none`; dependencies §3.4 item 1), `a_crate_without_workspace_lints_is_rejected` and
  `unsafe_code_is_forbidden_outside_capi`, `a_doc_excerpt_not_in_the_sources_is_rejected` (ROT-139),
  `an_unticked_due_rot_row_is_rejected`. *Do:* `cargo xtask gates` with the M0 subcommands of VERIFICATION.md §11.2
  (`deps`, `lints`, `counts`, `ignores`, `doc-excerpts`, `rot`) and `gates all`; create `ci/test-counts.txt` from the
  seed run (43 on Linux) with `# pending M5.9:` lines naming the four held-back tests; add `pub const MILESTONE: u8 =
  0;` to `phasekit-verify` (section 0.2). xtask stays std-only until M2.1. *Done when:* G8 passes and fails on each
  injected fault (shown in the PR).
- **M0.4a Test-quality gates** (user decisions TQ1, TQ2, 2026-10-05; ROT-294). Tests that cannot fail are caught
  before the test-driven steps begin. *Failing tests:* `an_assertion_free_test_is_rejected` and
  `an_assert_with_identical_sides_is_rejected` (`gates assertions`), `a_surviving_mutant_fails_the_gate` and
  `untracked_files_join_the_diff` (`gates mutants`), and `clippy_bans_fire` requiring probes for the tautology lints
  `clippy::assertions_on_constants`, `clippy::eq_op` and `clippy::bool_assert_comparison`; `scripts/check-toolchain.sh`
  fails without `cargo-mutants`. *Do:* `cargo xtask gates assertions` (std-only; comments and string literals are
  not code): every `#[test]` function contains an `assert!`, `assert_eq!` or `assert_ne!` or is `#[should_panic]`
  (map 10 R3), and no `assert_eq!`/`assert_ne!` has identical sides, the call form `eq_op` lets through (map 07 I11).
  `cargo xtask gates mutants`: `cargo mutants --in-diff` over the Rust changes since the merge base with
  `origin/main`, untracked files included; a missed mutant fails the gate unless `.cargo/mutants.toml` excludes it with
  a reason (process glue in `repo.rs`, `main.rs` and each gate's `run` is excluded: G8 exercises it), and a timeout
  (the tests hang, so they did not pass) counts as caught and is listed.
  Install `cargo-mutants` (`cargo install --locked --root ~/.local cargo-mutants`; a T4 tool) and check it in
  `scripts/check-toolchain.sh`. Both gates join `gates all`. M0.6 adds the weekly full run as a report and gives the
  linux job `origin/main` and cargo-mutants. *Done when:* G8 passes, including `gates mutants` over this step's own
  code, and each gate fails on an injected fault.
- **M0.5 wasip2 tests under wasmtime.** *Failing command:* G4 (no runner for wasm32-wasip2; the seed's three thread
  tests are already gated off `target_family = "wasm"`). *Do:* `[target.wasm32-wasip2] runner = "wasmtime"` in
  `.cargo/config.toml`. *Done when:* G4 passes and its executed count (40 expected: 43 minus the three thread tests)
  is in `ci/test-counts.txt`.
- **M0.6 CI.** The remote exists (`Dzoay/phasekit`, created 2026-10-05; section 6, P2), with the `pr-title` check and signed-commit rule already required on `main`. *Failing command:* a probe PR with a mis-formatted file fails the `fmt` job, and one with a banned call fails
  `clippy`. *Do:* `.github/workflows/ci.yml` (GitHub Actions on the public repo `Dzoay/phasekit`, section 6 P2; then add the green jobs as required checks to the `main` ruleset) with the jobs of VERIFICATION.md §11.3: linux
  (G1, G2, G3, G7, G8, deny, reuse, shear, MSRV `cargo +1.85 check --lib -p phasekit-core -p phasekit-data`;
  dev-dependencies such as num-dual need newer toolchains, so tests are not run on 1.85, dependencies §4; caches
  `reference/CoolProp` keyed by its pinned commit and runs `scripts/fetch-coolprop.sh` on a miss), windows (G3
  without xtask), aarch64 (`ubuntu-24.04-arm` *(inference: hosted arm64 runner)*; G3 without xtask), wasip2 (G4,
  installs wasmtime), wasm-browser (G5 and the browser builds); `nightly.yml` (filled at M1.16) and `weekly.yml`
  (`cargo update` + G3; dependencies §3.4 item 4). *Done when:* both probes fail as expected and `main` is green on
  every job. **Until this step is done**, steps run G1-G8 locally (G6 is the Windows check), record "CI owed" in the PR
  description, and M1 may proceed; no milestone after M0 closes while CI is owed. The first green CI run on `main`
  tests the cumulative tree, so it replays the backlog.
- **M0.7 Licences, REUSE, supply chain.** *Failing command:* `uvx reuse lint`, `cargo deny check` (no config) and
  `cargo shear`. *Do:* `LICENSE-MIT`, `LICENSE-APACHE`, `LICENSE-THIRD-PARTY` (CoolProp's MIT notice, "Copyright (c)
  2012-2018 Ian H. Bell and other CoolProp developers", map 09 §9), `LICENSES/`, `REUSE.toml` (annotations for
  fixtures and blobs), `deny.toml` (licence allow-list of dependencies §3.1; ban `bincode`, `serde_cbor`, and
  `once_cell` and `lazy_static` as direct dependencies (dependencies §3.1 "Banned"); `wildcards = "deny"`; unknown
  registries and git sources denied; R7, R19); `Cargo.lock` is committed since M0.2; add `reuse`, `deny` and `shear` to the linux CI
  job. Close M0: set `MILESTONE = 1`. *Done when:* the three commands pass locally (and in CI once M0.6 is done); `gates
  rot` passes with every M0 row ticked.

**Exit gate.** G1-G8 green locally; all CI jobs green, or CI recorded as owed (M0.6); 43 tests on Linux and 40 on
wasip2 executed; `cargo xtask gates deps` shows only workspace crates; every `clippy.toml` entry fires or is removed
with a reason.
**ROT rows.** M0.2: ROT-032, ROT-037, ROT-136. M0.3: ROT-015, ROT-016, ROT-018, ROT-024, ROT-153. M0.4: ROT-127,
ROT-139, ROT-156. M0.4a: ROT-294. M0.6: ROT-138, ROT-152. M0.7: ROT-141.
**User decisions implemented.** 1 (name, `pk_`), D14 (licence). Decision 12: the `libm` feature lands opt-in at M9.3,
just before the M9.5 decision; until then core has no `libm` feature (core is std-only until M9, ARCHITECTURE.md §2) and
every transcendental already goes through the one `math` choke point (recorded deviation, section 6).

### M1 Verification kit and oracle

**Goal.** Before any physics: fixtures that round-trip bit-exactly, an oracle whose environment is pinned, the
register and arbiter machinery, the paper corpus, the register's v8.0.0 facts reproduced, the multiprecision check
points, and a C++-level CoolProp baseline (VERIFICATION.md §3-§6). **Prerequisites.** M0; network access for `uv` to
fetch the `CoolProp==8.0.0` wheel and for the open papers of M1.8 (anonymous; no personal data sent); `cmake` and a
C++ compiler for M1.15 (present on the dev box, 2026-10-05).

- **M1.1 Fixture reader.** *Failing tests:* `fixture_round_trip_is_bit_exact` (Python-repr strings for subnormals,
  ±0.0, 1e-300, 1.7976931348623157e308, `nan` parse to the same bits; map 10 §8.3), `header_fields_are_required`
  (missing `# oracle:` line is an error), `mismatch_reports_row_column_class_and_provenance` (map 10 U1). *Do:*
  `phasekit_verify::fixture::{Fixture, Row}` for the `<kind>/v1` format of VERIFICATION.md §3.3: `#`-header CSV,
  status column, `bits:` check, zero dependencies. *Done when:* green on G3 and G4.
- **M1.2 SplitMix64 sampler.** *Failing test:* `splitmix64_seed_1_golden_vector` (the first 4 outputs for seed 1, the
  vector gen.py asserts at start-up; VERIFICATION.md §3.1 assertion 5). *Do:* `phasekit_verify::sample::SplitMix64` and
  the grid helpers (uniform, log-uniform; dependencies §2.12). *Done when:* green on G3 and G4.
- **M1.3 Oracle generator and lock.** *Failing test:* `oracle_lock_records_the_wheel` (reads
  `crates/phasekit-verify/fixtures/oracle.lock`: version 8.0.0, git `ae81610e7d23efc57f9d051c8e70a4d66e87537f`, wheel
  tag, `CoolProp.abi3.so` size and sha256, Python 3.12, `fluids_sha256`, the 38 config keys; VERIFICATION.md §3.2).
  *Do:* `scripts/oracle/gen.py` with the CLI, start-up assertions and per-case rules of VERIFICATION.md §3.1, run by
  `cargo xtask oracle` exactly as that section shows; the first kind is `facts` (VERIFICATION.md §3.5). *Done when:*
  `cargo xtask oracle --kind facts --set smoke --out <tmp>` run twice gives byte-identical files, and every start-up
  assertion fails when its input is perturbed (shown in the PR).
- **M1.4 First fixtures and the manifest.** *Failing tests:* `oracle_smoke_round_trip` reads
  `coolprop-8.0.0/facts/smoke.csv` and asserts `PropsSI("H","T",300,"Q",1,"R134a") = 413265.6843372975` bit-exactly
  (map 11 §8) plus `Props1SI("Water","Tcrit") = 647.0959999999873` and `T_reducing = 647.096` (map 01 §8);
  `committed_fixtures_match_manifest` (sha256 of every committed fixture equals its `MANIFEST.sha256` line; REUSE
  annotation present; map 10 R15). *Do:* commit `facts/smoke.csv` and `MANIFEST.sha256` (written by `cargo xtask
  oracle --write-manifest`); `cargo xtask gates fixtures`. *Done when:* green on Linux, Windows and wasip2 (ARCH M1
  gate: fixture round trip bit-exact); `cargo xtask oracle --check` regenerates the committed fixtures byte-identical.
- **M1.5 Tolerance classes and printed digits.** *Failing tests:* `r1224ydz_printed_p_rejects_the_oracle`
  (`from_printed("21.17909")` gives a half-unit of 5e-6 MPa, and the oracle's 21.1790735 MPa lies 3.3 half-units
  away; map 10 §8.4; DIV-0014), `self_referential_rows_are_not_arbiters` (`Provenance::is_arbiter` is false for
  R-self and R-other; map 10 R6), `tolerance_classes_match_verification_md` (the class table in
  `phasekit_verify::tolerance` equals VERIFICATION.md §5 row for row, read with `include_str!`, so changing a class
  means editing both, with a derivation). *Do:* extend the seed's `tolerance.rs` with `SaFit`, `Measured`, `Identity`,
  `Fd`, `RefAnchor`. *Done when:* green.
- **M1.6 Register schema and proof dispatcher.** *Failing tests:* `every_due_proof_exists` (iterates `DIVERGENCES`;
  fails if an entry has a `proof` milestone below `MILESTONE` and no `div_NNNN` function), `every_proof_names_a_registered_id`,
  `every_entry_cites_a_map_id` (`evidence` cites a map item), `use_paper_data_fixes_are_cited_by_a_patch`. *Do:* the
  `Divergence` fields of VERIFICATION.md §6.1 (`fix`, `exempt`, `tolerance`, `proof: &'static [u8]`) on the 14 seeds;
  `tests/divergences.rs` with one `fn div_NNNN()` per entry whose proof milestone is reached and the dispatcher;
  `cargo xtask gates register` (VERIFICATION.md §7.2); `gates ignores` also checks that cited DIV ids exist; drop the
  `<!-- excerpt: illustrative -->` marker on the VERIFICATION.md §6.1 schema, which `gates doc-excerpts` then checks
  against `register.rs`. *Done when:* green; G8 includes `register`.
- **M1.7 Arbiter records.** *Failing tests:* `arbiter_statuses_are_asserted` (`tests/arbiters.rs`: each `ARBITERS`
  record's status equals the recorded one; at M1 all are `Expected`, `Transcribed`, `None` or `Unpublished`),
  `every_arbiter_cites_a_doi_or_report`. *Do:* `phasekit_verify::arbiters::ARBITERS` with the statuses of
  VERIFICATION.md §4.3, one record per (fluid, part) of the §4.4 core set; the evaluation half of the self-consistency
  procedure (compile with the paper's constants) is wired when the kinds exist (M4.5, M5.7, M6.10). *Done when:* green.
- **M1.8 Paper corpus: format, fetch list, double entry, IAPWS-95.** *Failing tests:* `paper_tables_double_entry_agree`
  (every `<bibkey>.<table>.csv` equals its `.check.csv` string for string), `printed_strings_parse` (every value keeps
  its printed string; map 13 §3 tolerance rule), `every_paper_file_has_a_citation_and_reuse_annotation`. *Do:* the
  `paper/v1` header of VERIFICATION.md §4.2; the fetch list below, PDFs downloaded anonymously into the gitignored
  `reference/papers/` (sha256 in each header); transcribe IAPWS R6-95(2018) Tables 6, 7 and 8 (Water; free from
  iapws.org; map 13 §3 notes the table numbers were from memory, so confirm them on the rendered page). **Double entry,
  every transcription step:** session A writes `<file>.csv` on the step branch; session B starts fresh in a separate
  worktree at the parent commit with only the citation (DOI, table, page) and the header template, and writes
  `<file>.check.csv`; neither sees the other's file; a mismatch is settled by a third read of the rendered page and
  noted in the PR. *Done when:* green; `REUSE.toml` covers every file.

  | Source | Tables | DOI or URL | Access | Step |
  |---|---|---|---|---|
  | IAPWS R6-95(2018) (Water) | 6 (α at 500 K, 838.025 kg/m³), 7 (T, ρ), 8 (saturation) | iapws.org release R6-95(2018) | free | M1.8 |
  | Lemmon et al., JCED 60:3745 (2015; the key's 2016 is stale, map 13 R2) (R227EA, R365MFC, R115, R13I1) | 7 (12 states) | 10.1021/acs.jced.5b00684 (map 13 §4) | green OA: PMC13576076 author manuscript (XML, VERIFICATION.md §4.2) | M1.9 |
  | Thol et al., IJT 37:28 (2016) (R1234ze(E)) | 3 (6 rows) | 10.1007/s10765-016-2040-6 | green OA: PMC13576062 author manuscript (XML, VERIFICATION.md §4.2) | M1.10 |
  | NIST IR 8474 (Helium; key OrtizVega-JPCRD-2019, map 13 §4) | 3, 4 | 10.6028/NIST.IR.8474 | OA | M1.11 |
  | CoolProp test rows: Lemmon & Akasaka IJT 2022 Table 7 (R1234yf, 6 states) and 12 one-row fluids (R1130(E) from Huber et al. IJT 2025 Table 4 among them) | 18 states, 68 values | `CoolProp-Tests.cpp:4399-4474, 4798-4815` (map 10 §8.1); 10.1007/s10765-022-03015-y (paywalled); 10.1007/s10765-025-03535-3 (OA) | in repo; open papers re-checked: THF and R1130(E) (PMC OA subset); R1132(E) and VinylChloride are CC BY but Springer refuses scripted downloads, so not re-checked yet | M1.12 |
  | Transport, σ and melting releases | see M8.2 | | | M8.2 |

- **M1.9 Lemmon et al. 2016 Table 7.** *Failing test:* `paper_tables_double_entry_agree` and `printed_strings_parse`
  over the new file. *Do:* transcribe with double entry; record the paper's R, M, T_r, ρ_r in the header and in
  `ARBITERS` (status `Transcribed`). *Done when:* green.
- **M1.10 Thol et al. 2016 Table 3.** As M1.9 for R1234ze(E) (the DIV-0001 arbiter; one row at ρ = 0).
- **M1.11 NIST IR 8474 Tables 3 and 4.** As M1.9 for Helium (DIV-0005; Table 1 R recorded beside Table 2's
  coefficients, map 13 §3 item 4).
- **M1.12 CoolProp test rows.** As M1.9 for the 18 states of `CoolProp-Tests.cpp` (P-paper, map 10 §8.1), each row
  re-checked against its open paper where one exists; R1224YDZ p carries `DIV-0014` in the row comment. *Done when:*
  green; ARBITERS lists the 13 fluids.
- **M1.13 Register facts reproduced.** *Failing test:* `register_cites_reproducible_oracle_facts` reads
  `facts/register.csv` and checks each seed entry's cited v8.0.0 value: N₂ `rhomolar_reducing` 11183.901464580624
  (DIV-0003), Water DT(55018.5 mol/m³, 250 K) p = −5.9277123935677105 Pa without error (DIV-0012), C virial errors
  −6.7e-5/−7.1e-5/+1.9e-5 for Propane 300 K/N₂ 300 K/Water 600 K (DIV-0011), two-phase Water η(500 K, Q=0.5) =
  1.6048e-5 (DIV-0004), R1233zd(E) viscosity raising (DIV-0009), R1224YDZ p (DIV-0014), and the oracle side of the
  DIV-0001 and DIV-0005 table comparisons (map 12 §6.3, map 10 §8.4, map 13 §3). *Do:* the `register` set of the
  `facts` kind. *Done when:* all 14 entries pass; a fact that does not reproduce is fixed in the register (with the
  oracle output) before merge.
- **M1.14 Property tests.** *Failing tests:* `input_new_never_panics` (any f64 bit pattern × every `Pair` × both
  bases; 4096 cases) and `batch_request_shapes_never_panic` (any n, m including n·m overflow and zero; S-06, E12),
  plus the ROT rows' `failed_flash_leaves_the_previous_state_untouched` and `nan_input_is_invalid_input_status`. *Do:*
  add `proptest` (dev-dependency, `default-features = false, features = ["std"]`; dependencies §2.12), fixed CI seed,
  committed `proptest-regressions/`. *Done when:* green on G3 and G4.
- **M1.15 C++ CoolProp baseline.** *Failing command:* `cargo xtask baseline --check` (no
  `benches/baseline/coolprop-8.0.0-<machine>.csv`). *Do:* the harness of VERIFICATION.md §12: `scripts/baseline/build.sh`
  makes a throwaway source copy (`git clone --shared --no-checkout reference/CoolProp $SCRATCH/coolprop-src`, then
  `git -C $SCRATCH/coolprop-src checkout ae81610e`), because CoolProp's configure runs `dev/generate_headers.py`, which
  writes `include/*.h`, `dev/hashes.json`, `.version` and `dev/all_fluids.json` into its source tree
  (`CMakeLists.txt:557-559`); configures it with `cmake -S scripts/baseline -B $SCRATCH/coolprop-build
  -DCMAKE_BUILD_TYPE=Release -DCOOLPROP_SRC=$SCRATCH/coolprop-src -DCOOLPROP_STATIC_LIBRARY=ON`
  (`CPM_SOURCE_CACHE=$SCRATCH/cpm`; CoolProp fetches packages at configure time, map 12 R16); the harness
  `CMakeLists.txt` does `add_subdirectory(${COOLPROP_SRC})` and `target_link_libraries(coolprop_baseline PRIVATE
  ${COOLPROP_LIBRARY_NAME} ${CMAKE_DL_LIBS})` (the static library target, `CMakeLists.txt:607-656`), which also carries
  CoolProp's PUBLIC include directories (`:830`).
  Test `reference_checkout_is_untouched` (xtask) asserts `git -C reference/CoolProp status --porcelain --ignored`
  lists nothing beyond `scripts/baseline/reference-status.txt` (recorded before the first build; on 2026-10-05 it lists only
  `dev/__pycache__/` and `dev/scripts/__pycache__/`). *Done when:* the CSV holds the 7 workloads × 5 fluids of
  VERIFICATION.md §12 with the CPU model, and the reference test passes after a build. If the build cannot be made to
  work in one step, commit the wheel's Python-level timings (map 10 §8.2) marked provisional and open a re-plan
  issue: perf gates stay non-blocking until M9.
- **M1.15a C++ baseline: memory and thread scaling** (user decision BG1, 2026-10-05). *Failing command:* `cargo xtask
  baseline --check` (a machine without `coolprop-8.0.0-<machine>.memory.csv` and `.scaling.csv`). *Do:* the harness
  also writes the memory file (heap via glibc `mallinfo2` and resident bytes: the library's first use, bytes per
  `AbstractState` of each bench fluid after construction and after a QT update, one state of every fluid) and the scaling
  file (DT + h + c_p at 1, 2, 4, 6 and 12 threads, one state per thread and fluid, each bench fluid and a mixed mode; speedup
  against one thread); the single-thread timing pins itself to one CPU and unpins before the threads start. *Done when:*
  `--check` passes with all three files of the reference machine.
- **M1.16 Nightly sweep.** *Failing command:* `nightly.yml` absent. *Do:* the nightly job of VERIFICATION.md §11.3:
  in the pinned runner image, regenerate the committed fixtures twice (`cargo xtask oracle --check`: environment
  drift or non-determinism fails), then generate the full set into `fixtures-full/` and run `cargo test -p
  phasekit-verify --release -- --ignored`; publish the report. *Done when:* the nightly job has run green once (needs
  M0.6).
- **M1.17 Superancillary check points.** *Failing test:* `check_points_are_390_and_well_formed` (130 fluids × Θ =
  0.5, 0.3, 0.1; columns T, p, ρ′, ρ″; source `mp:coolprop-json`; map 09 §8, map 10 §8.1). *Do:* gen.py kind
  `checkpoints` copies `EOS[0].SUPERANCILLARY.check_points` of every fluid JSON into `mp/check-points.csv` (97 kB);
  the cross-check against the parsed JSON follows at M2.2 (`check_points_match_the_json`). Close M1: set
  `MILESTONE = 2`. *Done when:* green; manifest line committed.

**Exit gate.** G1-G8 incl. `fixtures` and `register`; `cargo test -p phasekit-verify` green on Linux, Windows and
wasip2; `cargo xtask oracle --check` byte-identical; 14/14 register facts reproduced; `from_printed("21.17909")`
rejects 21.1790735 MPa; proptest 4096 cases × 2 properties without a panic; IAPWS-95, Lemmon 2016, Thol 2016, NIST IR
8474 and the 18 CoolProp rows double-entered; baseline timing, memory and scaling files committed; nightly green once
*(nightly)*.
**ROT rows.** M1.4: ROT-134. M1.5: ROT-130, ROT-131. M1.6: ROT-133. M1.14: ROT-006, ROT-012, ROT-022. M1.16:
ROT-128.
**User decisions implemented.** 3b (fixtures committed with provenance), 5 (DIV-0005 `KeepOracle`, seeded; facts
reproduced).

### M2 Data pipeline

**Goal.** `xtask datagen` turns the pinned v8.0.0 JSON into committed little-endian blobs, the name/alias/CAS index
and per-fluid features; the seed's toy decoder is gone; Parity/Corrected and the superancillary hash gate run on real
data (D7, ARCHITECTURE.md §8). **Prerequisites.** M1; `reference/CoolProp` at ae81610e.

- **M2.1 Serde mirror.** *Failing tests (xtask):* `all_136_fluids_parse`, `chlorine_duplicate_key_is_a_logged_waiver`
  (map 09 R19), `unknown_block_type_is_an_error` (map 10 R8, map 09 R7), `integral_float_d_and_l_are_accepted` (277
  `d` and 161 `l` stored as floats; map 02 §3.1). *Do:* add `serde` + `serde_json` to xtask only (tier T4; S-12); a
  literal-kind-preserving parse; `data/fluids.lock` with per-file sha256 checked first. *Done when:* 136/136 parse.
- **M2.2 FNV-1a stamps.** *Failing tests:* `fnv_self_test_is_8e75626511d00b5c`, `all_130_stamps_recompute` (map 09 §8,
  map 10 §8.5 L0) and `check_points_match_the_json` (`mp/check-points.csv` of M1.17 equals the parsed JSON). *Do:* the
  hash over the parsed EOS, int vs float literal kind preserved. *Done when:* 130/130.
- **M2.3 Closed enums and validation.** *Failing tests:* one per quirk of map 02 §9: `zero_means_absent` (l = 0, m =
  0), `gaob_eta_sign_is_flipped_to_the_paper`, `planck_einstein_theta_sign_normalised`, `cp0_block_tc_differs_from_tr`
  (R123 456.82 vs 456.831 recorded, map 02 §6), `exponents_within_max_pow` (d ≤ 15, l ≤ 6), `unequal_lengths_are_errors`,
  `t_min_and_t_triple_kept_apart` (16 fluids differ, map 09 R8), `alternate_eos_entries_are_skipped_explicitly` (23,
  map 09 R10); plus `published_constants_match_the_oracle_crit_rows` (class `Exact` against the new `crit` kind, core
  subset and all-fluid tier). *Do:* xtask mapping into `phasekit_core::internal::{FluidRecord, EosRecord, PowerTerm,
  IdealTerm, ...}` (`#[non_exhaustive]`, grown as kinds land); metadata sentinels (ODP −1, GWP100 −1, ASHRAE 34
  "UNKNOWN"/"?", `REFPROP_NAME` "N/A"; map 09 R17) become `None` with their source (ROT-057); add the `crit` kind to
  gen.py (VERIFICATION.md §3.5) and commit its files. *Done when:* all 136 map without error.
- **M2.4 Blob format v1.** *Failing tests:* `every_fluid_round_trips_bitwise` (decode(encode(r)) == r for 136),
  `truncated_or_corrupt_blob_is_a_load_error` (checksum, section table, version mismatch),
  `record_name_must_match_the_index`, and the seed's `every_eos_field_is_hashed` on a real record. *Do:* versioned
  header, checksum, section table, 8-byte-aligned LE sections (map 09 D3). v1 reserves a section id for every planned
  section (EOS, α⁰, superancillary with its precomputed extrema and inverse, caloric curves, ancillaries, transport, σ,
  melting, metadata, corrections); a section is empty until the milestone that fills it (M5.2 SA, M5.2a caloric curves,
  M8.1 transport), and filling one changes blob bytes, not the version. Only a layout change to a filled section bumps
  the version; the decoder refuses other versions (packs are regenerated, never migrated). `EosRecord::encode` is the
  one encoder (E14). The v1 decoder is added beside the seed's `PKIT\0toy:` path, which stays until M2.6. *Done when:*
  round trip 136/136; bytes per fluid recorded (map 09 §4: median 22,348 B raw f64).
- **M2.5 Index, features, references.** *Failing tests:* `index_has_556_keys_and_no_collisions` (map 09),
  `case_variants_collapse` ("water"/"WATER"), `feature_names_are_unique`, `ecs_reference_graph_is_acyclic` (map 05 R7),
  `fluid_feature_enables_its_references`. *Do:* generated `phasekit-data` (`#![no_std]`): one blob per fluid via
  `include_bytes!`, sorted index with `requires`, `DATASET`; features `all` (default), `core` = Water, Nitrogen,
  CarbonDioxide, R134a, n-Propane (the map 09 §9 TDD set), `fluid-<name>` each enabling its ECS references;
  `cargo xtask gates datagen` (regenerate, `git diff --exit-code`). *Done when:* `gates datagen` is clean and part of G8.
- **M2.6 Real data in the registry; toy path removed.** *Failing tests:* `embedded_layer_indexes_all_136_without_decoding`
  (counting source: 0 blob reads), `unimplemented_kinds_are_cached_typed_load_errors` (a fluid whose kinds have not
  landed gives `LoadError::Format("... lands at M<n>.<k>")`, read once), `pack_from_generated_bytes_layers_over_embedded`,
  `not_embedded_is_uncached` (`LoadError::NotEmbedded { feature }`), and `compilable_fluid_count_never_drops` (the
  test's own `MIN_COMPILABLE` constant, raised as kinds land; 136 at M4.4). *Do:* point `Registry::embedded`,
  `Pack::new` and the lazy slots at v1 blobs; re-point `lazy_load.rs` to v1 blobs built from
  `phasekit_core::internal::FluidRecord::synthetic(name).encode()` (the seed's `FluidRecord::toy`, renamed and kept in
  the doc-hidden `internal` module because integration tests cannot see core's `cfg(test)` items); only then delete the
  `PKIT\0toy:` decode path. Every seed `lazy_load.rs` test stays green. *Done when:* green; the compilable count
  recorded.
- **M2.7 Parity and Corrected.** *Failing tests:* `corrections_check_the_value_they_replace` (datagen refuses a row
  whose v8.0.0 value differs from the JSON), `parity_and_corrected_differ_by_exactly_the_patches` (real data),
  `check_register_accepts_the_shipped_patches`, `helium_ships_no_patch` (DIV-0005), `no_gas_constant_literal_in_core`
  (no `8.314` float literal under `crates/phasekit-core/src`; ROT-042), and the proofs due at M2: `div_0003`'s M2 part
  (Corrected ρ_r = 11183.9 as Span 2000 prints it, the oracle's 11183.901464580624 differs, Parity = oracle) and
  `div_0006`..`div_0008` (the `Investigate` pins: Parity's ρ_r and M equal the `facts/register.csv` values).
  *Do:* `data/corrections.csv` with DIV-0001 (R1234ze(E) R 8.314472 → 8.3144621; map 13 R1), DIV-0002 (Water ice VI
  melting p0; map 10 R10), DIV-0003 (N₂ ρ_r 11183.901464580624 → 11183.9; map 12 §6.3); corrections section in the
  blob; `DataSet::Corrected` default. *Done when:* exactly these 3 patches differ.
- **M2.8 Superancillary freshness.** *Failing tests:* `all_130_superancillaries_are_fresh_under_parity`,
  `rho_r_and_r_corrections_rescale_exactly` (DIV-0001, DIV-0003 → `Rescaled { p, rho }`),
  `any_other_eos_edit_marks_the_curve_stale`. *Do:* `FluidRecord::superancillary_freshness()` over the canonical EOS
  bytes, α⁰ included; datagen precomputes the SA stamp (shape hash, R, ρ_r). *Done when:* 130 fresh, 2 rescaled.
- **M2.9 Lookup cost.** *Failing test:* `hot_get_allocates_nothing` (counting `#[global_allocator]`). *Do:* criterion
  bench `lookup_by_name` (target ≤ 50 ns, ARCHITECTURE.md §7), `cargo xtask bench --record`; criterion added as a
  non-wasm dev-dependency of `phasekit-verify` (section 2.5). *Done when:* recorded, non-blocking.
- **M2.9a Readable fluid data** (user decision FD1). Humans and agents review exactly what ships: `cargo xtask fluid
  list|show|diff` prints each decoded record (from the blobs, under Parity or Corrected) as JSON, deterministic and at
  full float precision (shortest round-trip form). xtask already depends on core and may use serde_json (tier T4), so
  this adds no crate, no dependency and nothing published; TOML or YAML would need a dependency decision. *Failing tests
  (xtask):* `dump_round_trips_bitwise` (parsing the dump and re-encoding it reproduces the blob byte for byte, for all
  136 fluids: the readable form is complete, and it is the obvious input format for authoring a fluid later),
  `parity_and_corrected_diff_is_exactly_the_three_patches` (M2.7's criterion, through `fluid diff`), and
  `every_blob_section_is_dumped` (fails when a step fills a blob section without extending the dump; section 2.5). *Do:*
  the dump form and its parser in xtask; `diff` compares two datasets or the blobs of two git revisions. Also, if it
  needs no new permission: on PRs that change `data/` or `crates/phasekit-data/`, the `linux` CI job writes `cargo xtask
  fluid diff origin/main` to its job summary, so data changes are reviewable on GitHub without committing text dumps
  (about 10 MB). *Done when:* green.
- **M2.10 Citations.** *Failing tests (xtask):* `every_default_source_has_an_identifier` and `every_bibkey_resolves`
  (every default model's `bibkey` resolves in CoolProp's bundled `CoolPropBibTeXLibrary.bib` and has a DOI or report
  id, else a waiver with a reason; the 19 EOS keys without a DOI, map 13 R2, are waived with the OpenAlex DOI map 13
  lists), `composite_citations_are_split_by_role` (map 13 R5: coefficients, erratum, check table), and
  `unpublished_models_have_no_paper_arbiter` (Propylene, SES36, Neon; map 13 R7). *Do:* datagen citation lint; the
  record's `Source` holds a list of role-tagged citations; reviewed corrections to citations live in
  `data/citations.csv`. *Done when:* green.
- **M2.11 Caloric-curve contract** (user decision CC1). The curves h′, h″, s′, s″, u′, u″ along both saturation
  branches, which CoolProp builds lazily at first use (45-63 ms per fluid behind a mutex; map 03 §6), are precomputed by
  datagen (ARCHITECTURE.md §8 step 5); this step fixes their form, and M5.2a computes them. *Failing tests:*
  `caloric_section_round_trips` (a synthetic curve set through blob v1, bitwise),
  `caloric_curves_share_the_superancillary_pieces` (one breakpoint array for the SA and all six curves, 13 coefficients
  per piece, as CoolProp builds them, SA.h:1156-1174; a mismatch is a `LoadError`),
  `caloric_stamp_binds_the_eos_and_the_gauge` (the stamp records the SA stamp and the α⁰ offset (a1, a2) the curves were
  sampled in; an R-only correction rescales h, s, u by R′/R exactly, a ρ_r-only correction leaves them unchanged, any
  other EOS edit marks them stale; a different offset shifts them by Δh = Δu = R·T_r·Δa2, Δs = −R·Δa1, map 03 §6), and
  `caloric_section_is_empty_until_m5_2a` (every shipped blob carries the section empty; asking for a curve gives the
  typed "not yet" error). *Do:* the `Caloric` section of blob v1 and its layout, `FluidRecord::caloric` (`Option`), its
  freshness check beside `superancillary_freshness()`, and the datagen hook that writes the section when curves exist.
  Close M2: set `MILESTONE = 3`. *Done when:* green.

**Exit gate.** G1-G8 incl. `datagen`; 136 parse; 130 FNV stamps; 390 check points match the JSON; 556 keys, 0
collisions; 136 blobs round-trip bitwise, and so does their readable dump; Parity vs Corrected = 3 patches; 130 fresh /
2 rescaled; the caloric section round-trips and its stamp classifies edits; DIV-0003 (part 1) and DIV-0006..0008 proofs;
core and data still zero third-party dependencies.
**ROT rows.** M2.1: ROT-038, ROT-058. M2.2: ROT-053. M2.3: ROT-041, ROT-046, ROT-052, ROT-068. M2.4: ROT-026,
ROT-040, ROT-055. M2.5: ROT-045, ROT-054. M2.6: ROT-025, ROT-027, ROT-033, ROT-034, ROT-039. M2.7: ROT-042. M2.8:
ROT-056. M2.10: ROT-142, ROT-143, ROT-144. (ROT-057's datagen half lands in M2.3; the row closes at M10.2.)
**User decisions implemented.** 2 (Corrected default), 3a (superancillaries shipped; NOTICE in `phasekit-data`), 5.

### M3 Separable residual kinds and `Jet4`

**Goal.** Every separable residual kind evaluated to order 4 on real data, each checked against AD of the paper
formula and against the oracle block by block (L1, L3); α^r totals for every fluid without NonAnalytic terms.
**Prerequisites.** M2; `num-dual` as a dev-dependency (MSRV 1.89, so MSRV CI runs `check --lib` only).

- **M3.1 Power blocks against the oracle.** *Failing test:* `power_blocks_match_oracle_term_fixtures` (class `Term`,
  all 15 A_ij up to order 4, the `term` grid of VERIFICATION.md §3.5 with δ log-spaced down to 1e-8, core subset;
  map 10 §8.5 L1). *Do:* add the `term` kind to gen.py (per-block isolation through `add_fluids_as_JSON`, map 10 §8.2)
  and commit its core-subset files; fix whatever the seed's `PowerBlock` gets wrong on real data. *Done when:* green.
- **M3.2 `Jet4` with a num-dual oracle.** *Failing tests:* `jet4_matches_num_dual_on_every_real_method` (exp, expm1,
  ln, ln_1p, powi, powf, sqrt, sinh, cosh, atan) and `jets_match_num_dual_ad` (replacing the seed's test-only
  HyperDual, S-07), both at class `Term` (VERIFICATION.md §5, §9.3: scale Σ_k |φ_k|, so entries near zero cannot flake).
  *Do:* the in-house bivariate `Jet4` implementing the sealed `Real` (D2). *Done when:* green on SplitMix64 inputs;
  HyperDual deleted.
- **M3.3 Exponential, Lemmon2005, DoubleExponential.** *Failing tests:* `<kind>_matches_ad_of_the_paper_formula` and
  `<kind>_matches_oracle_term_fixtures` (R1130(E) Exponential with g ≠ 1, R125 Lemmon2005, Methanol
  DoubleExponential, all in the core subset; map 02 §3.1, map 13 §8). *Do:* SoA blocks in `helmholtz` with τ- and
  δ-side factor jets combined by `add_outer`; exact δ-polynomials wherever the δ-side is `e^(−cδ^l)`. *Done when:* both
  classes green.
- **M3.4 Gaussian.** *Failing tests:* the same two for Gaussian (433 default terms, 78 fluids; R1234yf and Water's
  Gaussian blocks). *Do:* Gaussian block; δ-side jet of `e^(−η(δ−ε)²)` *(inference: closed form or `Jet4`, whichever
  passes the class)*. *Done when:* both classes green.
- **M3.5 GaoB.** *Failing tests:* the same two on Ammonia (the only GaoB fluid, 2 terms). *Do:* GaoB block with the
  paper's η sign (map 02 §3.1). *Done when:* green.
- **M3.6 α^r totals.** *Failing test:* `alphar_totals_match_oracle_for_134_fluids` (every fluid except Water and CO₂;
  `alphar` and its 14 derivatives with imposed phase; class `Term`): per PR on the committed all-fluid tier (4 (τ, δ)
  per fluid); the 64-point grid per fluid runs in the nightly sweep. *Do:* `MultiParameterEos::residual` over all
  blocks, `match` once per block (D3); add the `term` totals rows (`block_idx = all`) to the all-fluid tier. *Done
  when:* 134/134.
- **M3.7 Zero density on real data.** *Failing tests:* `finite_at_zero_density_for_every_fluid` (incl. the 57 MBWR
  d = 0 terms of CycloPropane, Propyne, R114, R123, R13, R14, R152A, R21, RC318 and Methane's 3 Gaussians; map 02
  §3.1) and `delta_factors_are_cancellation_free_near_zero_density` at δ = 1e-12 for every (d, l) in the data. *Do:*
  fixes only. *Done when:* green.
- **M3.8 α^r bench.** *Failing test:* none (measurement step). *Do:* criterion `alphar_order2` on the VERIFICATION.md
  §12 bench fluids vs the M1.15 baseline; record EOS bytes per fluid (target ≤ 25 KiB) and the `dyn`/`match` dispatch
  share (target ≤ 2 %, D3 *(inference)*). Close M3: set `MILESTONE = 4`. *Done when:* rows recorded.

**Exit gate.** G1-G8; jets vs num-dual within `Term`; every residual block of the core subset within `Term` against
the oracle; α^r totals 134/134 (all-fluid tier) and on the 64-point grid *(nightly)*; bench recorded (target ≤ 0.3 µs,
non-blocking).
**ROT rows.** M3.2: ROT-137, ROT-215. M3.6: ROT-030.
**User decisions implemented.** None new (D2, D3).

### M4 NonAnalytic, ideal kinds, all 136 fluids

**Goal.** Full α = α⁰ + α^r for all 136 fluids; the registry compiles every embedded fluid; IAPWS-95 Table 6.
**Prerequisites.** M3; M1.8 Table 6 transcription.

- **M4.1 NonAnalytic.** *Failing tests:* `nonanalytic_matches_ad_of_the_paper_formula` (Wagner & Pruß 2002 form, map
  02 §3.1), `nonanalytic_matches_oracle_away_from_the_critical_point` (Water 2 terms, CO₂ 3 terms; class `Term`).
  *Do:* a scalar block on `Jet4`, written with sign(δ−1)|δ−1|^p and no 0/0 forms or ε nudge (map 02 §6). *Done
  when:* green.
- **M4.2 Ideal kinds.** *Failing tests:* `<kind>_matches_ad` to order 4 for PlanckEinsteinFunctionT,
  PlanckEinsteinGeneralized, EnthalpyEntropyOffset, CP0PolyT, CP0Constant and CP0AlyLee (converted at datagen;
  map 02 §3.2), plus `cp0polyt_t_minus_one_fourth_derivative_is_minus_2c` (−2c/(τ³T_c), not CoolProp's −3c; map 02
  §6) and `air_generalized_term_refuses_overflow` (below 16.3 K; map 02 §3.2). *Do:* the `IdealTerm` variants; the
  τ-based CP0 forms take T_r from the model. *Done when:* green.
- **M4.3 α⁰ against the oracle.** *Failing test:* `alpha0_matches_oracle_for_136_fluids` (orders 0-3, the oracle's
  limit; class `Term`; per PR on the all-fluid tier's `block_idx = ideal` rows, nightly on the full grid). *Do:* add the
  α⁰ rows to the `term` kind. The R123 block-`Tc` mismatch (c_p⁰ −1.3e-5, map 02 §6, map 13 A4) gets a new
  `Investigate` entry: Parity keeps the JSON value, Corrected = Parity until its paper is checked (ROT-REGISTER.md
  open detail 5). *Done when:* 136/136 or registered.
- **M4.4 All fluids compile.** *Failing tests:* `all_136_fluids_compile_under_both_datasets`,
  `embedded_water_initialises_once_under_16_threads` (real data). *Do:* `FluidRecord::compile` complete;
  `MIN_COMPILABLE` goes to 136. *Done when:* green; `Registry::embedded()?.get("Water")` succeeds.
- **M4.5 IAPWS-95 Table 6.** *Failing test:* `iapws95_table6_within_printed_digits` (α⁰, α^r and derivatives at
  500 K, 838.025 kg/m³; class `Paper`); the oracle's values are also within `Paper` (measured ≤ 2.9e-9, map 13 §3).
  *Do:* the evaluation half of the `ARBITERS` procedure (VERIFICATION.md §4.3) runs for the first time; Water α →
  `SelfConsistent`. *Done when:* every printed value passes.
- **M4.6 Critical point.** *Failing test:* `water_and_co2_critical_bundles_keep_first_order` (at (T_c, ρ_c):
  first-order A_ij finite, the divergent second-order entries ±∞ never NaN; `State::from_total` then reports p, h, s
  and `Undefined { prop, phase }` for cv, cp, w; E17). *Do:* fixes in NonAnalytic and `State::single`. *Done when:*
  green for Water and CO₂.
- **M4.7 Constants audit.** *Failing test:* `stored_constants_match_their_arbiter_records` (R, M, T_r, ρ_r of every
  fluid with a transcribed arbiter; a mismatch must cite a DIV id; map 13 A3, R1). *Do:* datagen check; the 15 R1
  candidates of map 13 R1 listed as `Investigate` only when a paper value is in hand. Close M4: set `MILESTONE = 5`.
  *Done when:* green.

**Exit gate.** G1-G8; 136/136 compile (Parity and Corrected); α⁰ 136/136 (order ≤ 3; all-fluid tier, full grid
*(nightly)*) and orders 4 vs AD; IAPWS-95 Table 6 within printed digits; Water/CO₂ critical `Undefined`; every
constant mismatch registered.
**ROT rows.** M4.2: ROT-060, ROT-061, ROT-194. M4.3: ROT-050. M4.6: ROT-065.
**User decisions implemented.** None new.

### M5 Properties, DT, superancillary, gauge, batch, compat, seams

**Goal.** DT works for every fluid (subcritical through superancillaries), with properties, first partials, Cp0,
residual parts and exact virials; batch and compat run on it; the new-family and Gibbs seams are green with a
subcritical point (D5, D6, E18). **Prerequisites.** M4; M1.9-M1.12 transcriptions.

- **M5.1 Properties at (T, ρ).** *Failing tests:* `eos_fixtures_match_oracle` (p, h, s, u, cv, cp, w, Z on the
  (T, ρ) grid with region labels and imposed phase; class `Prop`; core subset and all-fluid tier per PR, the 10,000-row
  grid of all 136 fluids *(nightly)*; map 10 §8.5 L2) and `identities_hold` (h = u + p/ρ, g = h − Ts, cp − cv =
  Tβ²/(ρκ_T), w² = (∂p/∂ρ)_s, μ = g for pure fluids; class `Identity`; map 01 §8, map 02 §8). *Do:* add the `eos` kind
  to gen.py and commit its files; fixes to `relations` and `State`. *Done when:* green.
- **M5.2 Superancillary evaluation.** *Failing tests:* `superancillary_matches_oracle_eval_sat` (p, ρ′, ρ″ against
  `CP.SuperAncillary(json).eval_sat`, class `SaCoeff`; map 03 §8: the `sat` kind's `input = sa` rows, 8 T per fluid in
  the all-fluid tier per PR, 200 T *(nightly)*), `superancillary_matches_the_check_points` (class `SaFit`, the 390
  points of M1.17), `curve_refuses_outside_its_fitted_range` (map 03 §6), `degree_zero_clenshaw_is_c0`,
  `short_expansions_do_not_crash` and `chebyshev_derivative_keeps_every_coefficient` (map 03 §6 SA latent defects),
  `rescaled_curves_apply_exact_factors`. *Do:* add the `sat` kind with its `sa` rows to gen.py; the SA section decoder
  and a Clenshaw evaluator (no FMA, fixed order) written from the superancillary paper (NOTICE credits
  NIST/fastchebpure; if any CoolProp SA code is translated, the NIST disclaimer goes into NOTICE; user decision 3a);
  datagen fills the reserved SA section, with extrema and the T(ln p) inverse precomputed (map 03 §9). *Done when:*
  130 fluids green.
- **M5.2a Caloric curves** (user decisions CC1, CC2). The curves are starting points: M7.7 and M7.8 polish every
  answer with the EOS at (T, ρ_SA(T)), so their fit class is the measured `CaloricFit`, not `SaCoeff` (CC2: a
  degree-12 fit on the SA pieces misses the EOS by up to 1.1e-6 next to Tc, where h, s, u go like (Tc − T)^β; measured
  at M5.2a). *Failing tests:* `caloric_curves_match_the_eos_between_nodes` (h, s, u of each curve against the EOS at
  (T, ρ_SA(T)) at piece midpoints, where a fit has no node, for 130 fluids; class `CaloricFit`),
  `caloric_fit_error_is_checked` (datagen refuses a piece whose midpoint error exceeds the class; CoolProp has no
  fit-error check, map 03 §6), `caloric_curves_match_oracle_sat_rows` (hL, hV, sL, sV of the `sat` kind's QT rows on
  the all-fluid tier: the EOS at the SA densities, the values the polish converges to, at class `Prop`, and the curves at
  `CaloricFit`), `gauge_shift_is_exact` (another α⁰ offset, as CoolProp's IIR and NBP reference states write one, shifts
  the curves by the M2.11 formula, against the EOS), `corrected_curves_rescale_or_go_stale` (DIV-0001's R correction
  rescales them by R′/R, checked against the Corrected EOS; a synthetic shape edit reports them stale and the curve
  refuses to answer, so M7.7 falls back to the EOS), and `first_caloric_query_builds_nothing` (counting allocator: 0
  allocations; ROT-027). *Do:* datagen samples h, s, u from the compiled Parity record at the 13 Chebyshev-Lobatto
  nodes of every SA piece, at the M5.2 SA densities, in the native gauge, fits degree 12 (the Lobatto L matrix), and
  fills the M2.11 section; the `sat` kind's QT rows on the all-fluid tier; bytes per fluid recorded. Consumed by the
  SA-based Q pairs and HS (M7.7, M7.8). *Done when:* 130 fluids green.
- **M5.3 DT below the critical temperature.** *Failing tests:* `dt_two_phase_matches_oracle` (lever rule through
  `State::from_split`; `flash` kind DT rows), `two_phase_cp_cv_w_are_undefined` (`Undefined { prop, TwoPhase }`; the
  DIV-0004 c_p/c_v proof part due at M5), `dt_below_the_triple_point_is_refused` (Water DT(55018.5 mol/m³, 250 K) →
  `DomainError`; oracle gives −5.928 Pa: DIV-0012 `SkipOracle`), `extrapolation_is_opt_in_flagged_and_never_extends_a_fit`
  (real Water, supercooled liquid), `extrapolate_is_per_call_never_sticky`, `pseudo_pure_subcritical_dt_is_unsupported_until_m6`.
  *Do:* add the `flash` kind's DT rows to gen.py; the phase rule (hint → critical point → `SaturationCurve` →
  `Unsupported`), `Limits` with the bound max(t_min, t_triple) (map 09 R8), acceptance gate. Melting-line checks join
  at M8.11. *Done when:* green for 130 SA fluids.
- **M5.4 Gauge and custom reference states.** *Failing tests:* `gauge_invariance_on_real_fluids` (any two gauges give
  identical p, ρ, cp, w; h, s, u, g, a shift by exactly Δh, Δs, Δh, Δh − TΔs, Δh − TΔs; map 15 §8) and
  `custom_reference_state_hits_its_anchor` (class `RefAnchor`). *Do:* `Gauge` at the boundary,
  `ReferenceState::Custom`, `Fluid::with_reference`; IIR/ASHRAE/NBP need QT/PQ and land at M10.1. *Done when:* green.
- **M5.5 First partials, Cp0, residual parts.** *Failing tests:* `first_partials_match_oracle` (all 12 `DerivVar`s
  against `first_partial_deriv`; class `Prop`), `fd_first_order` conformance on every core fluid,
  `cp0_and_residual_parts_match_oracle` (`cp0molar`, `*_residual`). *Do:* `Prop::Partial`, `Prop::Z`,
  `Prop::Cp0molar/Cp0mass`, residual h/s/g via `ThermoModel::derivs`; the extra oracle columns in the `eos` kind. *Done
  when:* green.
- **M5.6 Exact virials.** *Failing tests:* `virials_equal_the_delta_series_for_every_fluid` (B, C, dB/dT, dC/dT from
  `zero_density` vs an independent δ → 0 extrapolation; class `Identity`), `bvirial_matches_oracle_within_the_registered_bound`
  (B and dB/dT at class `Measured`, the DIV-0011 tolerance: the oracle evaluates at δ = 1e-12 and divides by δ, so
  `Prop` would leave no margin; map 12 §6.3; measured at M5.6 as 9e-8, Methanol's dB/dT), and `div_0011`
  (`SkipOracle`: our C is exact; the oracle's C still differs beyond `Prop`, e.g. −7.1e-5 for N₂ at 300 K). *Do:*
  `MultiParameterEos::zero_density` on real data; `Prop` virial outputs. *Done when:* green.
- **M5.7 Paper arbiters.** *Failing tests:* `lemmon2016_table7_within_printed_digits` (R227EA, R365MFC, R115, R13I1;
  12 states; the oracle is within 4.3e-7, map 13 §3), `iapws95_table7_within_printed_digits`, `div_0001` three-part
  proof (Corrected within Thol 2016 Table 3 digits; oracle still +1.0e-6..+1.4e-6 high; Parity = oracle), `div_0005`
  Table 3 part (`KeepOracle`: no patch, `ModelKey` equal, IR 8474 Table 3 at `Measured` 5e-7),
  `coolprop_test_rows_within_printed_digits` (the 18 states of M1.12 at `Paper`, except R1224YDZ p) and `div_0014`
  (`Investigate`: R1224YDZ p asserted at `Measured` 8e-7, the oracle value pinned; resolution action: re-check
  Akasaka & Lemmon 2023 Table 7). *Do:* the ARBITERS self-consistency runs for these tables (statuses become
  `SelfConsistent`, or `Inconsistent` for IR 8474 Table 3); fixes only. *Done when:* all green.
- **M5.8 Batch on real fluids.** *Failing tests:* `batch_equals_scalar_bitwise` (DT, 10⁴ points, 3 fluids),
  `batch_point_allocates_nothing` and `dt_flash_allocates_nothing` (counting allocator; ARCHITECTURE.md §7),
  `out_is_point_major`. *Do:* `batch::evaluate` over `Fluid::flash`, per-cell `Status` incl. `Extrapolated`. *Done
  when:* green.
- **M5.9 `phasekit-compat` joins.** *Failing tests:* the four held-back seed tests, copied unchanged from the sketch
  with their imports (`compat_strings_reach_the_new_family`, `reference_states_work_for_any_family`,
  `solid_reaches_registry_batch_compat_and_reference_states`, `two_phase_states_from_outside_the_core`);
  `props_si_dt_matches_oracle`; `output_equal_to_an_input_is_not_echoed` (`PropsSI("T","T",-5,"P",101325,"Water")` is
  an error; map 01 R7); `fluids_core_feature_tree_has_no_extra_fluids` (E7). *Do:* add the crate (features
  `fluids-all` default, `fluids-core`, `embedded`), `props_si_in`, `FillPolicy`, the DT keys; G7 extended; `cargo xtask
  gates features` (compat `fluids-core` tree). *Done when:* `ci/test-counts.txt` has no `# pending` lines.
- **M5.10 Seam gates with a subcritical point.** *Failing tests:*
  `new_family_subcritical_dt_goes_through_the_core_phase_rule` (the out-of-tree van der Waals family supplies its own
  `SaturationCurve`, computed in the test by a Maxwell construction *(inference)*), plus all of `new_family.rs`,
  `gibbs_seam.rs` and `lazy_load.rs`. *Do:* fixes only; no core edit for the family. *Done when:* green on Linux,
  Windows and wasip2 (threads gated).
- **M5.11 DT bench.** *Failing test:* none (measurement step). *Do:* criterion `properties_at_t_rho` (target ≤ 0.5 µs;
  CoolProp 1.5-10.7 µs) and `dt_flash` recorded. Close M5: set `MILESTONE = 6`. *Done when:* rows recorded.

**Exit gate.** G1-G8 (G7 with compat; `features`); eos fixtures at `Prop` for the core subset and all-fluid tier, full
grid *(nightly)*; SA within `SaCoeff` for 130 fluids (all-fluid tier; 200 T *(nightly)*) and within `SaFit` at the 390
check points; caloric curves within `CaloricFit` of the EOS for 130 fluids (CC2); Lemmon 2016 Table 7 (12 states), IAPWS-95 Table
7 and the CoolProp paper rows within printed digits; proofs due at M5: DIV-0001 (three parts), DIV-0004 (c_p, c_v),
DIV-0005 (Table 3), DIV-0011, DIV-0012, DIV-0014; 0 allocations per DT flash and batch point; seam tests green.
**ROT rows.** M5.1: ROT-062. M5.2: ROT-035, ROT-087, ROT-093. M5.2a: ROT-027 (caloric part). M5.3: ROT-010 (c_p, c_v, w
part), ROT-013, ROT-020, ROT-078, ROT-083, ROT-092. M5.4: ROT-098, ROT-102. M5.6: ROT-063. M5.7: ROT-043, ROT-044,
ROT-059, ROT-132. M5.8: ROT-011, ROT-014, ROT-171. M5.9: ROT-028, ROT-165. M5.10: ROT-001, ROT-002, ROT-003, ROT-005,
ROT-212.
**User decisions implemented.** 2, 6 (refuse below the triple point; `Extrapolate` opt-in), 11 (point-major), 3a
(exact saturation from superancillaries).

### M6 Saturation, VLE and critical points

**Goal.** QT and PQ for every fluid; a pure EOS VLE that validates superancillaries and polishes stale ones; published
and numerical critical points; CoolProp's pseudo-pure rules; the saturation arbiters (D4, D6). **Prerequisites.** M5.

- **M6.1 fastchebpure outputcheck files.** *Failing test:* `fastchebpure_files_match_the_v8_eos` (each committed
  file's sha256 equals its `fixtures/mp/fastchebpure.lock` line; the EOS hash a file records equals the fluid's
  `source_eos_hash` (map 10 §8.1: the release must match the v8.0.0 EOS hashes); its values at the check points' Θ
  agree with `mp/check-points.csv` *(inference: the file layout is confirmed when the release is first fetched; if a
  file records no hash, the check-point agreement alone decides)*). *Do:* `cargo xtask fetch-fastchebpure`: an anonymous download of
  `https://github.com/CoolProp/fastchebpure/archive/refs/tags/2026.06.02-v2.zip` (the pin of
  `Web/scripts/fluid_properties.Superancillary.py:15-18`; no personal data in the URL or headers), the zip's sha256
  and a per-file sha256 written to `fastchebpure.lock`, conversion of `outputcheck/<Fluid>_check.json` to the
  `mp/v1` CSV for the core subset (the nightly converts all 130 into `fixtures-full/`), REUSE annotation
  `LicenseRef-fastchebpure` (VERIFICATION.md §3.7; user decision 3b). *Done when:* green; files and lock committed.
- **M6.2 Root toolbox and small solves.** *Failing tests:* `root_reports_the_point_it_evaluated` (map 03 §6
  solver/state mismatch), `exhausted_iterations_are_an_error` (map 03 §6 silent non-convergence),
  `newton_converges_on_residual_and_step`, `a_nan_residual_is_an_error` (map 03 §3.4), `log_axis_tolerance_is_relative`
  (map 03 §6: 6.1 % ST error at ρ = 1e-8 from an absolute tolerance), `singular_2x2_is_an_error` and
  `near_singular_3x3_is_scale_invariant` (map 03 §3.4). *Do:* crate-private `roots`: native TOMS748, bracketed
  Newton/Halley, `Root { x, f, iterations, stop }`, typed `Tol` (map 03 §9); `num::solve_small` (N ≤ 4, `Result`,
  pivot test relative to the row scale; dependencies R9), the only matrix solve in core. *Done when:* green.
- **M6.3 Pure VLE.** *Failing tests:* `vle_matches_390_multiprecision_points` (3 per fluid × 130 at Θ = 0.5/0.3/0.1;
  class `SatMp`; map 10 §8.1, map 09 §8), `vle_equilibrium_self_check` (p′ = p″, g′ = g″; class `Identity`; map 03
  §8), `vle_jacobian_matches_ad`, `vle_nan_or_stall_is_no_convergence`. *Do:* Newton in (τ, ln δ′, ln δ″) with a
  `Jet4` Jacobian solved by `num::solve_small`, SA or ancillary seeds, bracketed fallback, residual gate (map 04 U3).
  *Done when:* 390/390.
- **M6.4 Superancillary vs VLE.** *Failing test:* `superancillary_matches_vle_within_sa_fit` (class `SaFit`: 4 × the
  fastchebpure ratio; the M6.1 dense files incl. near-critical points; map 10 §8.1, §8.3). *Do:* the comparison
  harness over `mp/fastchebpure-*/` (core subset per PR; all 130 *(nightly)*); fixes in VLE or the evaluator only.
  *Done when:* green for the core subset; all 130 *(nightly)*.
- **M6.5 Exact rescaling proved.** *Failing test:* `rescaled_superancillary_equals_vle` for Nitrogen (DIV-0003) and
  R1234ze(E) (DIV-0001), class `SatMp` (ARCHITECTURE.md §8 inference, now proved). *Do:* the DIV-0001 and DIV-0003
  proof parts due at M6. *Done when:* green; the ARCHITECTURE.md §8 sentence loses its *(inference)* tag in the same PR.
- **M6.6 Stale curve polish.** *Failing test:* `stale_curve_is_a_guess_polished_by_vle` (an `Edit` other than R/ρ_r on
  a test record → `SatAccuracy::Guess`; QT equals VLE within `SatMp`). *Do:* the `Guess` path in the phase rule. *Done
  when:* green.
- **M6.7 Critical points.** *Failing tests:* `published_critical_point_is_exact` (Water 647.096 K vs the oracle's
  computed 647.0959999999873; map 01 §8), `numerical_critical_point_satisfies_the_criticality_conditions`
  (∂p/∂ρ = ∂²p/∂ρ² = 0, class `Flash`), `both_critical_points_are_exposed_distinctly` (R13: 303.05 K vs 301.88 K;
  map 02 §6). *Do:* `CriticalPoint { origin }` published and numerical (map 03 §6 config-dependent critical point);
  the numerical columns of the `crit` kind. *Done when:* green.
- **M6.8 QT and PQ.** *Failing tests:* `sat_fixtures_match_oracle` (QT, PQ, both Q; class `SaCoeff` where the oracle
  path is `superanc`, `Prop` for `ancillary`; core subset and all-fluid tier), `q_pairs_report_their_saturation_source`
  (`State::path()`), `r134a_qt_smoke` (h = 413265.6843372975 J/kg at 300 K, Q = 1; map 11 §8),
  `water_normal_boiling_point` (PQ at 101325 Pa in [373.124, 373.125] K; map 11 §8). *Do:* the `sat` kind's PQ rows
  and core files (the all-fluid tier's QT rows land at M5.2a); QT/PQ strategies via `SaturationCurve` then VLE. *Done when:* green for 136 fluids.
- **M6.9 Pseudo-pure rules.** *Failing test:* `r410a_pseudo_pure_rows` (oracle, 03-decision-log: p(280 K, Q=0) =
  990480.516605891 Pa, Q=1: 987288.0717853763 Pa; QT at Q = 0.5 refused; T(1 MPa, Q = 0/0.5/1) = 280.31657 /
  280.37003 / 280.42348 K), plus `pseudo_pure_in_dome_dt_follows_the_oracle_rule` for all 6 blends. *Do:*
  `Definition` curves with distinct bubble/dew `SatSide`s; QT only at Q ∈ {0, 1}; PQ with T linear in Q; the in-dome DT
  rule (map 04 §1); the pseudo-pure `sat` rows; the M5.3 `Unsupported` test is replaced. *Done when:* 6 blends green.
- **M6.10 Saturation arbiters.** *Failing tests:* `iapws95_saturation_table_within_printed_digits` (Table 8, class
  `Paper`) and the `div_0005` Table 4 part (NIST IR 8474 Table 4 at `Paper`, or at `Measured` if the ARBITERS
  procedure finds Table 4 inconsistent too; VERIFICATION.md §6.6). *Done when:* green; ARBITERS statuses updated.
- **M6.11 Saturation benches.** *Failing test:* none (measurement step). *Do:* criterion `qt_superancillary`,
  `pq_superancillary` (target ≤ 0.1 µs; CoolProp 0.45 / 0.64 µs); record SA bytes per fluid (≤ 25 KiB). Close M6: set
  `MILESTONE = 7`. *Done when:* rows recorded.

**Exit gate.** G1-G8; 390/390 mp points; SA vs VLE within `SaFit` (core subset; all 130 *(nightly)*); stale polish;
exact rescaling for DIV-0001 and DIV-0003; R410A rows; QT/PQ for 136 fluids (all-fluid tier); IAPWS-95 Table 8 and IR
8474 Table 4; benches recorded.
**ROT rows.** M6.2: ROT-067, ROT-074, ROT-075, ROT-076. M6.3: ROT-066, ROT-082, ROT-089, ROT-090. M6.4: ROT-094.
M6.5: ROT-047. M6.6: ROT-088, ROT-096. M6.7: ROT-084. M6.8: ROT-004, ROT-085. M6.9: ROT-091, ROT-095.
**User decisions implemented.** 4 (pseudo-pure rules), 3a, 3b (fastchebpure files committed).

### M7 All input pairs

**Goal.** All 19 pairs with one phase rule, typed ambiguity and an acceptance gate; second-order outputs (D6, §3.7).
**Prerequisites.** M6. Truth states are built in (p, T) for single phase and (T, Q) for two phase, never in density
bands (map 03 §8; map 10 §8.5 L4).

- **M7.1 Density solver ρ(T, p).** *Failing tests:* `pt_round_trips_on_a_40x40_log_p_t_grid` (core subset; class
  `Flash`), `pt_reproduces_its_input_pressure` (oracle Nitrogen 10 MPa/200 K is off 3.3e-9; map 03 §6),
  `a_guess_is_only_a_seed` (Water 10 MPa/300 K with a 0.9·ρ guess must not return dp/dρ < 0 as the oracle does; map 03
  §6), `a_guess_changes_no_output` (every `Prop`, with and without `with_guess`; map 01 R6). *Do:* add the `flash`
  kind's PT rows to gen.py; Newton on `residual + IDEAL_DELTA`, bracketed by ρ′/ρ″ and `rho_max(T)`, acceptance (inputs
  reproduced, in domain, dp/dρ > 0, cv > 0 unless a phase is imposed); datagen's provisional `rho_max` (the saturated
  liquid density at T_min, M2.3) becomes ρ(T_min, p_max) from this solver. *Done when:* green.
- **M7.2 PT and DT phase rule complete.** *Failing tests:* `pt_at_saturation_is_ambiguous_under_strict`
  (`Ambiguous { roots: [ρ′, ρ″] }` at Water 305 K and 315 K at p_sat; `Nearest` and `with_phase` pick one; map 03 §6
  "PT near saturation"), `imposed_two_phase_dt_matches_qt`, `water_dg_dt_at_constant_p_equals_minus_s` (300 K, 1 atm:
  −393.0620684404547 J/kg/K, map 01 §8), `pt_range_is_one_rule` (Water 3000 K refused under `Enforce`; map 03 §6).
  *Do:* hint → critical point → curve (`Exact`/`Definition`, `Guess` polished) → VLE. *Done when:* green; PT bench
  (target ≤ 3 µs) recorded.
- **M7.3 PH, PS, PU.** *Failing tests:* `p_x_round_trips` (from (p, T) truth), `nitrogen_supercritical_cold_ps`
  (86.35 K/4.754 MPa and 90.20 K/7.768 MPa; map 03 §8 PXFlash), `pxcdj_named_regressions` (Water, CO₂, R134a,
  Propane, N₂, MM; map 03 §8), `errors_name_the_failing_strategy`. *Do:* outer T TOMS748 with a warm-started inner
  density, brackets from SA and the domain bound (map 03 §9 `flash::px`). *Done when:* green; PH bench (target
  ≤ 15 µs) recorded.
- **M7.4 DH, DS, DU.** *Failing test:* `d_x_round_trips` incl. the Air near-critical cases (map 03 §8 AirCritical).
  *Do:* the interval method (map 03 §9 `flash::dx`). *Done when:* green.
- **M7.5 HT, ST, TU.** *Failing tests:* `compressed_liquid_ht_is_ambiguous` (Water 300 K at 1/10/50 MPa: the oracle
  silently returns two-phase at 3536.8 Pa with Q = 0.00038/0.0038/0.0185; ST and UT at 275 K; map 03 §6),
  `low_density_round_trips` (Water 1.3 T_c, ρ = 1e-8…1e-6, ST and HT; the oracle's ST is 6.1 % off at 1e-8). *Do:*
  log-ρ axis, detect (∂X/∂ρ)_T > 0 on the liquid branch, `Ambiguous { roots }` (map 03 §9 `flash::tx`). *Done when:*
  green.
- **M7.6 DP.** *Failing test:* `dp_honours_the_phase_hint` (oracle ignores an imposed gas at a two-phase (d, p);
  map 03 §6). *Do:* `flash::dp`. *Done when:* green.
- **M7.7 QS, HQ, DQ.** *Prerequisite:* the caloric curves (M5.2a). *Failing tests:* `hq_water_400k_lists_both_roots`
  (400 K and 587.912 K; map 03 §6), `qs_r1234ze_e_vapour_has_three_roots` (267.3, 275.6, 363.7 K), `dq_works_near_tc`
  (oracle fails at T_c − 0.05 K), `q_pairs_are_symmetric_in_q`. *Do:* SA-based Q-pairs bracketed to T_c,num, roots found
  on the precomputed caloric curves (M5.2a) and polished on the EOS, roots as data (map 03 §9 `flash::qx`). *Done when:*
  green.
- **M7.8 HS and SU.** *Failing tests:* `hs_round_trips` (two-phase 25 T × 5 Q and single-phase 20 × 20 grids; map 03
  §8 HS tests), `hsu_d_issue_cases` (#2486, #2157, #1698, #1054, #2154, #2173, #1965, #2022, #2685, #2426),
  `su_round_trips` (no CoolProp arm, map 01 R4: truth from (p, T) only). *Do:* two-phase screen, legs and corrector;
  acceptance needs dp/dρ > 0 and cv > 0 (map 03 §8 agent notes). *Done when:* green.
- **M7.9 Capability matrix.** *Failing tests:* `capability_matrix_19x2` (each pair × {single, two-phase} declared and
  round-tripped; conformance kit `capability_matrix`), `phase_hints_never_leak` (QS then PH(30 MPa, h(700 K)) gives
  ≈ 700 K, where CoolProp's reused state gives 270.79 K; map 03 §6), `phase_hint_is_honoured_by_every_pair`,
  `domain_errors_are_uniform_across_pairs` (T > Tmax, p > pmax for all 19 pairs), `dt_mass_basis_reproduces_input`,
  `robustness_corpus_never_panics` (~100 issue-linked states, map 10 §8.1: a state that passes acceptance or a typed
  error). *Done when:* green for the core subset; all 136 *(nightly)*.
- **M7.10 Order-3 outputs.** *Failing tests:* `second_partials_match_oracle` (`second_partial_deriv`), Thorade &
  Saadat identities (class `Identity`), `fundamental_derivative_matches_oracle`, `kappa_and_beta_match_oracle`. *Do:*
  additive `Prop` variants via `ThermoModel::derivs` (ARCHITECTURE.md §3.7). Close M7: set `MILESTONE = 8`. *Done
  when:* green.

**Exit gate.** G1-G8; 19 × 2 capability matrix for the core subset (136 *(nightly)*); round trips in class `Flash`;
every map 03 §6 oracle-defect case returns a value passing acceptance or a typed error; ~100 corpus states without a
panic; PT and PH benches recorded.
**ROT rows.** M7.1: ROT-017, ROT-072 (and ROT-074's PT test). M7.2: ROT-073, ROT-080. M7.3: ROT-021. M7.5: ROT-071,
ROT-077. M7.9: ROT-007, ROT-009, ROT-069, ROT-070, ROT-079, ROT-081, ROT-086, ROT-129.
**User decisions implemented.** 6 (domain applied to every pair).

### M8 Transport, surface tension, melting

**Goal.** σ, viscosity and conductivity where v8.0.0 has models, verified stage by stage against papers; melting lines
with per-segment domains; two-phase transport refused (D7 transport section, map 05 §9). **Prerequisites.** M7.

- **M8.1 Transport decode and closed enums.** *Failing tests:* `all_transport_blocks_decode` (70 fluids without
  viscosity and 73 without conductivity give `NoModel`; map 10 §8.2), `transport_lengths_are_checked` (map 05 R11),
  `every_transport_key_is_consumed_or_waived` (map 05 R2: `q_D` is a consumed key), `an_empty_transport_list_is_a_load_error`
  and `transport_source_is_the_active_entry` (map 05 R5). *Do:* datagen fills the reserved transport section; closed
  enums `ViscosityModel`, `ConductivityModel` (map 05 U2); `Edit` gains transport-coefficient edits (E15). *Done
  when:* green.
- **M8.2 Transport, σ and melting arbiters transcribed.** *Failing tests:* `paper_tables_double_entry_agree` and
  `printed_strings_parse` over the new files. *Do:* the M1.8 double-entry procedure for IAPWS R12-08 (water η),
  R15-11 (water λ), R1-76(2014) (water σ), R14-08 (melting), the IAPWS D₂O transport release and Herrig 2018 (D₂O
  melting) (map 05 §8, map 13 §4; IAPWS releases free at iapws.org, Herrig JPCRD 10.1063/1.5053993); ARBITERS records
  `Transcribed`. *Done when:* green.
- **M8.3 CoolProp transport rows relabelled.** *Failing test:* `transport_rows_carry_their_provenance` (each of the
  318 active rows of `CoolProp-Tests.cpp:44-301, 347-581` has a provenance; R-other, R-self and unsourced rows are not
  arbiters; map 10 §8.1, R6). *Do:* transcribe the rows into `paper/` files with per-row provenance tags, keeping only
  P and P-IAPWS rows as arbiters. *Done when:* green; the arbiter-row counts per fluid recorded.
- **M8.4 Surface tension.** *Failing tests:* `sigma_matches_the_mulero_closed_form` (Mulero's own T_c; map 13 §3),
  `sigma_matches_oracle` (108 fluids; all-fluid tier), `water_sigma_is_labelled_coolprop_model` (vs IAPWS R1-76:
  +0.12 % at 300 K, −0.90 % at 600 K; map 10 §8.4), `surface_tension_between_tc_sigma_and_tc_eos` (σ for
  T_c,σ < T ≤ T_c,EOS is a `Domain` error, one case per affected fluid; map 05 R13: the oracle throws there; the same
  rule as "fits are never evaluated outside their range", user decision 6). *Do:* add the `sigma` kind; pure
  `fn sigma(&self, T) -> Result`. *Done when:* green.
- **M8.5 Staged viscosity.** *Failing tests:* `viscosity_stages_match_contributions` (dilute, initial-density,
  residual vs `viscosity_contributions` for staged fluids), `viscosity_paper_rows` (the arbiter rows of M8.3),
  friction-theory paper rows (Methane, n-Pentane, SF₆, H₂S). *Do:* add the `transport` kind; `transport` forms over
  `TransportInputs` (map 05 U1, U3). *Done when:* green.
- **M8.6 Staged conductivity and Olchowy-Sengers.** *Failing tests:* `conductivity_stages_match`, the arbiter rows
  among the 141 active conductivity rows, `methane_lambda_is_independent_of_the_saturation_source`. *Do:* η passed
  explicitly to the critical term (map 05 R6), joint {η, λ} evaluation (map 05 U6). *Done when:* green.
- **M8.7 IAPWS water and heavy water.** *Failing tests:* `iapws_r12_08_table`, `iapws_r15_11_table` and the D₂O
  release tables, class `Paper` (the oracle's measured agreement, 3.3e-8 and 2.7e-9 for water and ≤ 5.3e-11 for D₂O,
  map 10 §8.1, is reported, not asserted). *Do:* one shared ξ/Y module (map 05 U7). *Done when:* green.
- **M8.8 ECS.** *Failing tests:* `ecs_matches_oracle_where_the_oracle_converges` (class `TransportEcs`; oracle
  failures, 7.3 % of 19 ECS fluids, stay `status=err` rows; map 10 §8.4), `q_d_is_read` (R13, R14, R142b, R218,
  RC318; map 05 R2) with its new `SkipOracle` entry (below),
  `a_reference_is_read_once_on_first_need_and_outlives_its_registry` (real data),
  `thermo_only_workloads_never_load_references` (counting). *Do:* conformal solver, references through the slot
  handles resolved at layer build (map 05 U8). **New entry (q_D):** fluids R13, R14, R142b, R218, RC318; part
  `Transport`; `SkipOracle`, `fix: Code` (CoolProp never reads `q_D`, `FluidLibrary.h:668-680`); exempt: `transport`
  `lambda`, all rows of these fluids (honouring q_D moves λ by −16 % to +24 % near T_c, map 05 R2, and the critical
  term is non-zero everywhere); replacing check: the same record with `q_D` edited to CoolProp's default 2e9
  (`CoolPropFluid.h:188`) reproduces the oracle's λ at `TransportEcs` on every row, and at least one exempt cell
  still differs; becomes `UsePaper` once Huber et al. IECR 2003 is transcribed. *Done when:* green.
- **M8.9 rhosr-CS, Chung, other hard-coded models.** *Failing tests:* `hardcoded_models_match_their_paper_tables`
  then `hardcoded_models_match_oracle`, one case per model of map 05 U9 (rho*sr-CS; the Chung LJ estimate) and U10
  (xylenes and cyclohexane, Shan R23, Friend Ω22, Huber 2016 CO₂, the R123 critical term; Helium oracle-only, map 05
  says no paper can arbitrate it; Ammonia defect-flagged); `ammonia_lambda_is_finite_near_405_4_k`
  (map 05 R3: the oracle gives 6.26 W/m/K at 405.3999 K, 50 kg/m³ and NaN at 405.4 K);
  `rhosr_critical_matches_the_eos` (map 05 R4: R1234yf's stored −54468.48 vs −54095.50 from the current EOS);
  `ecs_lj_parameters_come_from_the_ecs_record` (map 05 R17). *Do:* `x_crossover` read or refused; the Chung LJ fill
  becomes a typed, named choice in the record (`LjSource::{Model, Chung}`), never silent. **New entries:** Ammonia λ,
  `SkipOracle`, exempt `transport` `lambda` rows with |T − 405.4 K| < 1 K *(inference: band width set from the
  measured divergence when the step runs)*, replacing check: finite and continuous across 405.4 K; the Corrected model
  (Monogenidou 2018 or a bounded term) waits for the paper check (map 05 Q3). R1234yf `rhosr_critical`,
  `Investigate`, Parity keeps the stored value. R124, R22, R245fa, R32 ECS λ with Chung LJ, `Investigate` pending
  Huber 2003 and McLinden 2000. *Done when:* green or registered.
- **M8.10 Two-phase and missing models.** *Failing tests:* `two_phase_transport_is_undefined` (the DIV-0004 η, λ part:
  oracle Water η(500 K, Q = 0.5) = 1.6048e-5 is not asserted), `r1233zd_e_viscosity_is_no_model` (DIV-0009 stays
  `Investigate`; v8.0.0 data has no model). *Done when:* green.
- **M8.11 Melting lines.** *Failing tests:* `div_0002` three-part proof (IAPWS R14-08, 4 points; Corrected ice VI p0),
  `heavy_water_melting_herrig` (4 points, class `Paper`), `simon_segments_respect_t_min` (Hydrogen T(0.5·p_min) =
  9.52 K below T_min 13.957 K in the oracle; map 02 §6), `melting_meets_the_triple_pressure` (datagen: every melting
  curve gives p_melt(T_triple) within its fit's stated tolerance of p_triple, Helium exempt, waivers listed; map 02
  §6), `enforce_refuses_below_the_melting_line`, `melting_caloric_data_lives_in_the_model`. *Do:* add the `melt` kind;
  `MeltingSegment` with [T_min, T_max] per segment; `DomainError::BelowMeltingLine` in `Enforce` for every pair. **New
  entry:** Hydrogen and OrthoHydrogen melting (p_melt(13.957 K) = 23.6 MPa vs p_triple 7.36 kPa), `Investigate`
  against Datchi 2000. *Done when:* green.
- **M8.12 Transport bench.** *Failing test:* none (measurement step). *Do:* criterion `pt_viscosity_conductivity`
  recorded (no §7 target; CoolProp 34 µs for Water, map 10 §8.2). Close M8: set `MILESTONE = 9`. *Done when:* row
  recorded.

**Exit gate.** G1-G8; every transport arbiter row within printed digits, stage by stage where staged; oracle classes
`TransportDirect` and `TransportEcs` where it converges, outside registered exemptions; DIV-0002 three-part proof;
DIV-0004 (η, λ) refusal; DIV-0009 recorded; the new q_D, Ammonia, R1234yf, ECS-LJ and Hydrogen entries proved;
thermo-only workloads read 0 transport sections.
**ROT rows.** M8.1: ROT-109, ROT-110, ROT-116. M8.3: ROT-121, ROT-124. M8.4: ROT-114, ROT-123. M8.5: ROT-115,
ROT-118, ROT-119. M8.6: ROT-108, ROT-117. M8.8: ROT-029, ROT-122. M8.9: ROT-111, ROT-112, ROT-113. M8.10: ROT-010
(transport part), ROT-120. M8.11: ROT-036, ROT-048, ROT-049, ROT-051.
**User decisions implemented.** 2 (DIV-0002 applied by default), 6 (σ and melting fits not extrapolated).

### M9 Parallel batches, `libm` and performance gates

**Goal.** Thread-parallel batches bitwise equal to the scalar reference; the `libm` default decided on evidence;
performance and memory gates enforced (D8, D9, D12). **Prerequisites.** M8; `rayon` and `libm` as optional core
dependencies (the first third-party dependencies of core; R2, R12); `valgrind` on the Linux CI runner for gungraun;
the reference machine (section 6, P3).

- **M9.1 `ExecPolicy::Parallel`.** *Failing tests:* `policy_equivalence_parallel_is_bitwise` (every pair, 10⁵ points,
  chunk sizes 16, 64, 1024), `one_to_n_threads_bitwise` (1..N rayon threads),
  `parallel_without_the_feature_is_sequential_and_identical`. *Do:* the `rayon` feature, fixed chunks (multiple of 16,
  independent of thread count; K6); `gates deps` accepts exactly `rayon` and `libm` under `--features rayon,libm`.
  *Done when:* green.
- **M9.2 Thread scaling.** *Failing command:* `cargo xtask bench --record scaling` (no harness). *Do:* the
  `scaling_same_and_different_fluids` bench at 1, 2, 4 and N threads, N = the reference machine's physical cores (the
  dev box's i7-8700K: 6 cores, 12 hardware threads); it records throughput relative to one thread, it asserts nothing
  (K19 targets are estimates). *Done when:* recorded on the reference machine and CI runners; a result below 0.9·N at N
  threads opens a re-plan issue with the measurement.
- **M9.3 `libm` backend.** *Failing tests:* `libm_backend_matches_the_libm_crate_bitwise` (every `math` function on
  10⁶ random and edge inputs: subnormals, ±709.78, NaN, ±∞), `std_vs_libm_ulp_report` (recorded per target, not
  asserted across targets). *Do:* the opt-in `libm` feature routing `math` (R14); the feature is the documented
  exception to "features never change numerics" (dependencies §4). *Done when:* green.
- **M9.4 Cross-target hash.** *Failing test:* `canonical_grid_hash` (`tests/cross_target.rs`) over the grid VERIFICATION.md
  §9.4 defines (the one definition); CI job `hash-compare` collects it from x86_64-linux, x86_64-windows-msvc,
  aarch64-linux and wasm32-wasip2, with and without `libm`. *Done when:* hashes recorded for both backends on all four
  targets; if the `libm` hashes agree, that hash is committed as `fixtures/hash/libm.txt` and asserted from then on.
- **M9.5 `libm` decision.** Apply this rule (user decision 12; threshold set here; VERIFICATION.md §9.4 cites it).
  `libm` becomes a default feature of `phasekit-core`, forwarded by compat (and capi, wasm at M10), if and only if
  **both** hold:
  1. *Bit-identity:* with `libm`, the M9.4 hash is identical on all four targets.
  2. *Cost:* on the reference machine (section 6), the geometric mean of the libm/std median-time ratios over the six
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
  *(inference: noise margin)*; criterion medians on the reference machine against the ARCHITECTURE.md §7 table. *Done
  when:* every §7 target is met, or each miss has a re-plan issue with the measurement (targets are estimates, K19).
- **M9.7 Memory budgets.** *Failing tests:* `eos_and_superancillary_bytes_per_fluid` (≤ 25 KiB each),
  `all_136_loaded_rss` (≤ 8 MiB, Linux; ARCHITECTURE.md §6 *(inference)*), `state_fits_256_bytes` (asserts
  `size_of::<State>() <= 256` as ARCHITECTURE.md §3.8 does and records the actual size, 208 B in the sketch). Close M9:
  set `MILESTONE = 10`. *Done when:* green or re-planned.

**Exit gate.** G1-G8 incl. `perf`; `policy_equivalence` bitwise for 1..N threads; scaling recorded (0.9·N target);
cross-target hashes recorded; the `libm` rule applied and documented; §7 targets met or re-planned; memory budgets.
**ROT rows.** M9.1: ROT-135. M9.2: ROT-031. M9.6: ROT-064.
**User decisions implemented.** 12 (`libm` decided on evidence), 11 (point-major chunks).

### M10 Facades and v0.1

**Goal.** Reference states, the full PropsSI grammar, the C ABI (`pk_*` and the Tier A shim) and the browser package;
release v0.1 (D11, ARCHITECTURE.md §12). **Prerequisites.** M9; the shim library file name and the shim's
`set_config_*` behaviour (section 6, P1 and P6); tools: cbindgen CLI, `wasm-bindgen-cli` matching the crate
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
  `backend_options_suffix_is_refused` (map 01 R18), `no_metadata_sentinels` (absent ODP, GWP100, ASHRAE 34 give
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
  the process-wide `errstring` slot; `set_config_*` per section 6 P6: a shim-side settings store (process-wide, shim only) whose keys with a phasekit equivalent become per-call options (`FlashOptions`, `DomainPolicy`, `FillPolicy`) on every shim call, display keys such as `LIST_STRING_DELIMITER` read by shim functions, every other v8.0.0 key accepted, ignored and listed in MIGRATION.md; the key-by-key mapping table is written in this step; no global state reaches the kernel. *Done when:* green on Linux and
  Windows.
- **M10.7 FP guard.** *Failing tests:* `pk_exports_restore_the_callers_fp_environment` (traps masked and
  round-to-nearest inside; the caller's control word **and sticky flags** restored on return) and
  `shim_fp_guard_matches_v8_0_0` (the three cases of `CoolProp-Tests-FPUGuard.cpp:27-82`: traps masked inside, an
  already-masked environment unchanged, flags cleared; map 11 F17: clearing is intended for polling hosts such as
  Excel/VBA). *Do:* save, set, restore in every `pk_*` export; the shim's guard additionally clears the sticky flags
  as v8.0.0 does (user decision 9b; section 6); `_controlfp_s` on Windows, `fe*` on glibc, declared directly; no-op on
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

| M | Crate | Entry trigger | Scope | Exit gate |
|---|---|---|---|---|
| M11 | `phasekit-cubic` | v0.1 released | vdW/SRK/PR via Δ1/Δ2, Soave/MC/Twu alpha, parameters from the canonical fluid records, borrowed HEOS ideal gas, cubic superancillary (map 06 U1-U5) | Zero-line core diff; `new_family` conformance; cubic-R register entry (below); DIV-0010 proof by the entropy identity (oracle PR propane 400 K, 1 bar: T(∂s/∂T)_p = 91.35 vs cp = 93.89 J/mol/K; map 12 §6.3); Z_c = 1/3 (SRK), 0.30740 (PR) (map 06 §8) |
| M12 | core `simd` feature | v0.1 released: a time-boxed SIMD spike (the user's follow-up asks to investigate SIMD), about ten steps *(judgement)* | `fearless_simd` optional dependency, core-local `Lanes<W>` implementing sealed `Real`, crate-private `exec`, batch hooks and an `ExecPolicy` variant; wasm `simd128` build; in-house vector exp/ln (D9, ARCHITECTURE.md §7) | ≥ 2.5× α^r batch throughput on AVX2 over the M9 baseline, then the feature ships; otherwise the spike's measurements are recorded and the branch closed (D9); `policy_equivalence` bitwise; arbiter rows pass on every dispatch level; MSRV raised to 1.89 workspace-wide |
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

**M11 cubic-R register entry** (user decision 10; ARCHITECTURE.md §10, §11). Added in the first M11 step, before any
cubic code; VERIFICATION.md §6.6 carries the same row.

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

| From | Bench (criterion; gungraun from M9.6) | Target (ARCHITECTURE.md §7) |
|---|---|---|
| M1.15 | C++ CoolProp baseline, 7 workloads × 5 fluids | reference only |
| M1.15a | C++ CoolProp memory (first use, per state, all fluids) and thread scaling (1-12 threads) | reference only |
| M2.9 | hot lookup by name | ≤ 50 ns *(inference)* |
| M3.8 | α^r bundle order 2; EOS bytes; dispatch share | ≤ 0.3 µs; ≤ 25 KiB; ≤ 2 % |
| M5.8, M5.11 | properties at (T, ρ); allocations per flash and batch point | ≤ 0.5 µs; 0 |
| M6.11 | QT, PQ via superancillary; SA bytes | ≤ 0.1 µs; ≤ 25 KiB |
| M7.2, M7.3 | PT, PH single phase | ≤ 3 µs, ≤ 15 µs |
| M8.12 | PT + η + λ | recorded |
| M9 | thread scaling; RSS for 136 fluids | 0.9·N recorded; ≤ 8 MiB |

Workloads, bench fluids, harness and result files: VERIFICATION.md §12 (the one definition). Non-blocking until M9;
from M9.6 `gates perf` enforces gungraun regressions on Linux CI and the reference machine holds the absolute targets.

### 5.2 Documentation

- Rustdoc on every public item (`missing_docs`, enforced by G2); `cargo doc --no-deps` with broken intra-doc links
  denied from M5; the crate-level doctest of `phasekit-core` executes (the seed's does).
- `docs/`: this plan, ARCHITECTURE.md, VERIFICATION.md and ROT-REGISTER.md stay current. A step that changes a
  decision's evidence edits the document in the same PR (for example M6.5 removing an *(inference)* tag, M9.5 recording
  the `libm` outcome). Rust code blocks in `docs/*.md` are either verbatim excerpts of the workspace or the sketch, or
  marked `<!-- excerpt: illustrative -->` (`gates doc-excerpts`, ROT-139).
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
  tier (dependencies §3.1) in its step. `cargo about generate` for the capi and wasm bundles from M10.

### 5.4 v0.1 release checklist (M10.9)

1. All M0-M10 exit gates green on `main`; the nightly sweep green for the last 3 nights.
2. Register: every entry has its proof test; no `Investigate` entry hides a failing assertion; VERIFICATION.md §6.6
   and `register.rs` agree.
3. ROT-REGISTER.md: `gates rot` passes with `MILESTONE = 11`; section 3 lists no GAP.
4. `cargo semver-checks` baseline recorded (first release: establishes it).
5. `reuse lint`, `cargo deny check`, `cargo about generate` for the C library and wasm package; licences in each
   artefact; `cargo package --list -p phasekit-data` shows only blobs, the index and the notice (ROT-155).
6. Versions 0.1.0 across the workspace; `phasekit-data` carries its `DATASET` id; CHANGELOG and MIGRATION.md final.
7. Artefacts: crates core, data and compat (`publish = true` for these three only; `phasekit-verify` stays unpublished
   because it holds the benches, dependencies §2.12, and family authors use it as a git dev-dependency
   *(inference)*), the C library and header (`pk_*` and the separate shim build under the agreed file name), the npm
   package. Published under the `Dzoay` accounts (section 6, P5): crates.io Trusted Publishing after the first manual
   release (R19), npm trusted publishing from the release workflow; no long-lived tokens in the repository.
8. Tag `v0.1.0`; release notes list the user decisions in force and the known divergences.

## 6. Resolved follow-ups and remaining user decisions

**Follow-ups from 04-user-decisions.md**

| Follow-up | Resolution |
|---|---|
| Rename `cprs-*` → `phasekit-*`, `cp_*` → `pk_*` | Done for ARCHITECTURE.md and the sketch (2026-10-05). This plan uses only the new names; the historical design records keep the old ones and are never edited. |
| Where `HAPropsSI` sits (9a) | Done: the v0.1 Tier A shim exports a stub returning `_HUGE` with the documented `errstring` (M10.6). The real function arrives at **M18** (humid air), when `pk_*` also gains humid-air symbols. |
| `libm` default threshold (12) | Set in M9.5: bit-identical hash on 4 targets **and** geometric-mean slowdown ≤ 5 % with no bench > 10 %. |
| DIV-0005 accepted divergence; cubic-R entry (10) | DIV-0005 done (`KeepOracle`), proved at M5.7 (Table 3) and M6.10 (Table 4). The cubic-R entry is specified in section 4 and added in the first M11 step. |
| ARCHITECTURE.md §14 | Done; its remaining "for PLAN.md" items are answered here (M9 threshold, M11 entry, humid air = M18). |

**Settled in this plan without a new user decision** (ROT-REGISTER.md section 3 open details)

| Item | Settlement |
|---|---|
| Open details 2-4: atomics, `thread_local!`, `std::fs` not linted | Linted from M0.3; only `phasekit-capi`'s FFI-state modules may use atomics and `thread_local!` (M10.4). |
| Open detail 5: R123 / R11 cp⁰ `Tc` | Parity keeps the JSON value; an `Investigate` entry decides Corrected after the paper check (M4.3). |
| Open detail 6: shim FP flags | Follows user decision 9b ("quirks live only in the shim"): the shim clears the caller's sticky flags as v8.0.0 does; `pk_*` restores them (M10.7, two tests). |
| Decision 12 timing | Recorded deviation: the `libm` feature lands opt-in at M9.3, two steps before the M9.5 decision, rather than existing from M0; core stays std-only until M9 (ARCHITECTURE.md §2) and nothing before M9 can observe the feature. |

**Where each user decision is implemented**

| Decision | Milestone(s) |
|---|---|
| 1 name `phasekit`, `pk_` | M0.2, M10.4 |
| 2 Corrected default | M2.7 (M5, M8 use it) |
| 3a superancillaries under CoolProp's MIT notice | M2.5, M5.2 |
| 3b fixtures committed with provenance | M1.3-M1.17, M6.1, M8.2-M8.3 |
| 4 pseudo-pure rules now, true-mixture variant later | M6.9; M13 |
| 5 Helium keeps CoolProp's R | M1.11, M1.13, M2.7, M5.7, M6.10 |
| 6 refuse below the triple point, `Extrapolate` opt-in | M5.3, M7.2, M8.4, M8.11 |
| 7 Python own namespace + compat | M16 |
| 8 wasip2 Rust only, WIT on demand | M0.5, M10; on demand |
| 9a `pk_*` + Tier A shim, `HAPropsSI` stub | M10.4-M10.7; M18 |
| 9b shim matches v8.0.0 | M10.6, M10.7 |
| 10 cubic R split by dataset | M11 |
| 11 point-major batches | M5.8, M9.1, M10.5, M10.8, M16 |
| 12 `libm` decided at M9 | M9.3-M9.5 |
| 13 plotting after mixtures | M19 |
| D14 MIT OR Apache-2.0 | M0.7 |

**Decisions taken on 2026-10-05** (docs/design/04-user-decisions.md, rows P1-P7)

| # | Question | Decision | Where |
|---|---|---|---|
| P1 | Shim library file name | phasekit name only (`phasekit_coolproplib.dll`, `libphasekit_coolproplib.so`); MIGRATION.md shows hosts how to point at it | M10.6 |
| P2 | CI host and remote | GitHub + Actions, public repo `Dzoay/phasekit`; commits use the GitHub no-reply identity only | M0.6 |
| P3 | Performance reference machine | The dev box (i7-8700K, 6 cores / 12 threads); CPU model recorded with each result | M9.2-M9.5 |
| P4 | Paywalled EOS papers (Ammonia, CO₂, R125, Air, Nitrogen, HFE143m) | Open: the user will look into cost and access. Until a paper arrives its fluid stays oracle-only, labelled provisional; nothing waits on it | M5.7, then whenever a paper arrives |
| P5 | Publishing v0.1 | crates.io (core, data, compat) and npm (browser package) under the `Dzoay` accounts via trusted publishing from GitHub Actions; C binaries as release assets; PyPI at M16 | M10.9 |
| P6 | Shim `set_config_*` | The shim keeps its own process-wide settings and translates every key that has a phasekit equivalent into per-call options on each shim call (e.g. `DONT_CHECK_PROPERTY_LIMITS` → `DomainPolicy::Extrapolate`); other keys are accepted, ignored and listed in MIGRATION.md; nothing reaches the kernel | M10.6 |
| P7 | How PRs land | Squash merge only; Conventional Commit PR titles; linear `main` (section 2.2) | M0.6 and every PR |

Only P4 remains open, and no milestone waits on it.
