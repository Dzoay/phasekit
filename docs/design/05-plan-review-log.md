# Plan review log (fix pass)

Fix pass over [PLAN.md](../PLAN.md), [VERIFICATION.md](../VERIFICATION.md) and [ROT-REGISTER.md](../ROT-REGISTER.md),
2026-10-05, answering [05-review-executability.md](05-review-executability.md) (X-##) and
[05-review-consistency.md](05-review-consistency.md) (C##). ARCHITECTURE.md was edited only to replace "forthcoming"
mentions with links and to fix three real inconsistencies (§10 fluid count, §10 gungraun start, D12 `libm` timing; §14
now lists the new shim `set_config_*` question). 04-user-decisions.md: two follow-ups marked done. The sketch was not
touched.

**Normative homes chosen (X-01, C01, C02).** PLAN.md §2.4 defines G1-G8 (PLAN's numbering kept, with G4 now
`--exclude phasekit-xtask`); PLAN.md §2.3 is the shared names-and-paths table, each row naming its defining section;
VERIFICATION.md defines fixtures, kinds, the committed fluid set, tolerance classes, the register schema, the xtask
gate rules (§11.2), the hash grid (§9.4) and the bench harness (§12); PLAN.md M9.5 defines the `libm` rule;
ROT-REGISTER.md defines rot rows and proof-test names.

## Issues

| ID | Sev | Doc | Disposition | What changed, or why rejected |
|---|---|---|---|---|
| X-01 | blocker | PLAN vs VERIF | fixed | One home per artefact (above). VERIF §11.1 now points to PLAN §2.4; xtask subcommands unified as `deps`, `lints`, `counts`, `ignores`, `doc-excerpts`, `rot`, `fixtures`, `register`, `datagen`, `features`, `perf`, `abi`, `core-frozen` (VERIF §11.2 table, PLAN G8); count file `ci/test-counts.txt` in VERIF's format; lock at `fixtures/oracle.lock`; one `MANIFEST.sha256` over committed files; double entry = committed `.check.csv` + test; `facts` and `checkpoints` kinds added to the gen.py CLI; one bench harness, fluid list and result path (VERIF §12). Every PLAN done-when re-derived from these. |
| X-02 | major | PLAN | fixed | M0.2 holds back all 4 compat-calling tests (they stay in the sketch), drops the compat dev-dependency, its workspace entry, the unused imports and the `gibbs_seam.rs::qt` helper. Simulated on a scratch copy: clippy clean, 43 tests executed. Seed = 43 (40 on wasip2); M5.9 restores the 4; VERIF §14 M0 fixed. |
| X-03 | major | PLAN | fixed | M2.4 adds the v1 decoder beside the toy path; M2.6 re-points `lazy_load.rs` to v1 blobs from `internal::FluidRecord::synthetic` (the renamed `toy`, kept in the doc-hidden `internal` module) and only then deletes `PKIT\0toy:`. |
| X-04 | major | PLAN vs VERIF | fixed | One list, VERIF §3.6: the 14 fluids (Methanol, n-Heptane, R410A included, so M3.3, M6.9 and CP0AlyLee are covered). PLAN §2.3 and ARCH §10 link to it. Budget re-checked: core ~8 MiB + all-fluid tier ~2.5 MiB + paper/mp < 1 MiB ≤ 16 MiB. |
| X-05 | major | PLAN | fixed | New committed **all-fluid tier** (`coolprop-8.0.0/all/<kind>.csv`, a few rows per fluid per kind, ~2.5 MiB) runs every all-fluid gate per PR on all platforms; the dense grids are labelled *(nightly)*, and the §0.2 DoD says nightly items close on the first green nightly that includes them. Each M3-M8 exit gate states which applies. |
| X-06 | major | PLAN / VERIF | fixed | Every first-use step's *Do* adds its kind to gen.py and commits the files with their manifest lines (M1.3 facts, M1.17 checkpoints, M2.3 crit, M3.1/M3.6/M4.3 term, M5.1 eos, M5.2 sat `sa`, M5.3/M7.1 flash, M6.8-M6.9 sat, M8.4 sigma, M8.5 transport, M8.11 melt, M10.1 refstate, M10.2 props, M10.6 codes). `sa` (SuperAncillary.eval_sat) is a `sat` input mode. Manifest: committed files only; the full set has **no** manifest (stronger than the suggested split): the nightly regenerates the committed files twice to detect drift and non-determinism. gen.py's sha moved from the lock to fixture headers, so a new kind never edits the lock; "own PR" kept only for pin moves. |
| X-07 | major | PLAN / VERIF | fixed | G4 = VERIF's command (`--exclude phasekit-xtask`). Rule (PLAN §2.5): tests that spawn processes or read `reference/` live in xtask, a Linux/dev-box tool; windows and aarch64 run G3 `--exclude phasekit-xtask`. The linux job caches `reference/CoolProp` by pinned commit and runs `scripts/fetch-coolprop.sh` on a miss (M0.6, VERIF §11.3). |
| X-08 | major | PLAN / VERIF | fixed | M1.15 builds from a throwaway `git clone --shared` in the scratch dir; names the CMake options (`-DCMAKE_BUILD_TYPE=Release -DCOOLPROP_STATIC_LIBRARY=ON`, `CPM_SOURCE_CACHE`) and the link line (`add_subdirectory` + `target_link_libraries(... ${COOLPROP_LIBRARY_NAME} ${CMAKE_DL_LIBS})`, PUBLIC includes at `CMakeLists.txt:830`); `reference_checkout_is_untouched` compares `status --porcelain --ignored` with a recorded baseline (today: the two `__pycache__` dirs). |
| X-09 | major | PLAN | fixed | M1.17 (gen.py `checkpoints` kind → `mp/check-points.csv`, test `check_points_are_390_and_well_formed`) and M2.2 `check_points_match_the_json`; M6.1 `cargo xtask fetch-fastchebpure` (anonymous zip download of the pin in `fluid_properties.Superancillary.py:15-18`, `mp/fastchebpure.lock` with zip and per-file sha256, conversion to `mp/v1`, REUSE annotation, test `fastchebpure_files_match_the_v8_eos`). |
| X-10 | major | PLAN vs VERIF | fixed | New M1.2 SplitMix64 golden vector (before the generator), M1.6 register schema (`fix`, `exempt`, `tolerance`, `proof: &[u8]`), `tests/divergences.rs` dispatcher and `MILESTONE` with its bump rule (also in §0.2: each closing step sets `MILESTONE = n + 1`), M1.7 `ARBITERS` + `tests/arbiters.rs`. The three VERIF arbiters are now in PLAN: Lemmon & Akasaka 2022 Table 7 (M1.12, M5.7), IAPWS-95 Table 8 and IR 8474 Table 4 (M1.8, M1.11, M6.10). |
| X-11 | major | PLAN vs VERIF | fixed | Hash grid defined once in VERIF §9.4 (all 136 fluids; 64 (T, ρ) + 16 QT + 16 PQ + 16 PT + 16 PH; p, h, s, c_v, c_p, w, bundle, status), run from M9.4. Cost rule defined once in PLAN M9.5 (PLAN's threshold kept); VERIF §9.4 quotes and cites it. The VERIF-only std hash from M5 was dropped. |
| X-12 | major | ROT vs PLAN | fixed | Every PLAN milestone ends with a **ROT rows** line mapping each row to a step (163 v0.1 rows, script-checked against the register: 0 mismatches). ROT's Proof names are normative; 42 Proof cells were harmonised with PLAN's test names or made concrete (e.g. ROT-072 → `a_guess_is_only_a_seed`, ROT-150 → `stale_handle_is_an_error`). The step loop writes its rows' proof tests. "Tick" defined (PLAN §2.1, ROT §1): drop "new", append `[Mx.y]`; `gates rot` enforces it when `MILESTONE` passes. All 14 GAPs got steps and flipped. |
| X-13 | major | PLAN | fixed | M0.6 is a user checkpoint; until done, steps run G1-G8 locally (G6 = Windows check) and record "CI owed"; M1 may proceed, but no milestone after M0 closes while CI is owed; the first green CI run replays the backlog. §6 decision 2 now says it blocks M0.6 and every later milestone close. |
| X-14 | major | PLAN / VERIF | fixed | M1.8 adds the fetch list (DOIs or URLs, access, step) and the double-entry orchestration (fresh session, separate worktree, only the citation); one source per step M1.8-M1.12. M8 split into M8.1 decode/enums, M8.2 transcription, M8.3 relabelling of the 318 rows. M8.9 names the map 05 U9/U10 models and its tests. VERIF §4.2 matches. |
| X-15 | major | PLAN | fixed | M3.2 and the M3 exit gate assert class `Term` (scale Σ abs(φ_k)); no literal. |
| X-16 | minor | PLAN | fixed | M0.1 installs with `cargo install --locked --root ~/.local ...` (`~/.local/bin` is on PATH) and the script prints the PATH fix. |
| X-17 | minor | PLAN | fixed | M0.5 red reason is "no runner"; the gating was dropped from *Do*. |
| X-18 | minor | PLAN | fixed | M10.6 (and VERIF `codes` kind): parameter codes from `get_parameter_index`, phase codes from `get_phase_index`, pair values from `CoolProp.constants`, pair names from `DataStructures.cpp:513-572` with `coolprop-source:` provenance. |
| X-19 | minor | PLAN | fixed | M10.4 names the driver (`cc` dev-dependency of capi, tier T3, marked inference; `Command` fallback) and the flags: GCC/Clang `-std=c99 -pedantic-errors -Wall -Werror`, MSVC `/std:c11 /W4 /WX`; VERIF `abi` matches. |
| X-20 | minor | PLAN | fixed | §2.2: CI names the stable version in the workflow; a stable that turns a gate red is fixed in a dedicated `chore: rust 1.NN` PR, never a blanket `allow`. |
| X-21 | minor | VERIF | fixed | Weekly `cargo-mutants` dropped from VERIF §8.6 and §11.3 (no requirement; principle 5 is the map 10 R3 remedy); noted as a possible later report-only job. |
| X-22 | minor | PLAN | fixed | M9.7 `state_fits_256_bytes`: asserts ≤ 256, records the actual size (208 B in the sketch). |
| X-23 | minor | PLAN | fixed | M9.2 records, never asserts; N = physical cores (dev box i7-8700K: 6 cores, 12 threads, checked with `lscpu`); a miss opens a re-plan issue. VERIF §12 table matches. |
| X-24 | minor | PLAN | fixed | M2.4: v1 reserves a section id for every planned section; sections stay empty until filled (M5.2, M8.1); filling changes bytes, not the version; only a layout change to a filled section bumps it, and other versions are refused. |
| X-25 | minor | PLAN | fixed | Count file created in M0.4; the generator step (M1.3) checks a temp output twice, the committed fixtures follow in M1.4; M3.4 and the stale-polish step (now M6.6) have done-whens; the SA-vs-VLE step (now M6.4) has a *Do*. |
| X-26 | minor | PLAN / ROT | fixed | Settled in PLAN §6 without a new user decision (it follows 9b: quirks only in the shim): `pk_*` restores the caller's sticky flags, the shim clears them as v8.0.0 does; M10.7 has one test per facade; ROT-151 and open detail 6 updated. |
| C01 | blocker | PLAN, VERIF | fixed | As X-01: one gate table (PLAN §2.4), VERIF §11 references it and §14 uses G1-G8; one subcommand set, one count file and format, one ignore grammar (`DIV-NNNN: `, `issue #N: `, `nightly: `). |
| C02 | blocker | PLAN, VERIF | fixed | VERIF §3.4 is the single layout; PLAN §2.3 links each path to it. `smoke` became the `facts` kind's `smoke` set and `facts` was added to the CLI and kinds table; `.check.csv` is committed (VERIF §4.2 changed); M1.3, M1.4, M1.8-M1.13 use these names. |
| C03 | major | PLAN, VERIF, ARCH | fixed | One set (VERIF §3.6), linked from PLAN §2.3 and ARCH §10. The suggested union of 19 was not adopted: the five PLAN-only fluids (R227EA, R365MFC, R115, R13I1, HeavyWater) are arbitrated by paper rows and get their oracle rows from the new all-fluid tier, so dense core fixtures would add size without coverage. |
| C04 | major | PLAN, VERIF | fixed | As X-05: committed all-fluid tier with its own budget line, plus *(nightly)* labels and the DoD rule. |
| C05 | major | PLAN, VERIF | fixed | PLAN §0.3 step 5 now uses VERIF's `Investigate` semantics (oracle cells asserted, literature rows at `Measured`, proof pins the disagreement); M5.7 asserts R1224YDZ p at `Measured` 8e-7 as `div_0014`. |
| C06 | major | PLAN, VERIF | fixed | As X-11 for the rule. The std-hash report was dropped from VERIF §14 M5. Timing: deviation recorded (PLAN M0 and §6): the opt-in feature lands at M9.3, before the M9.5 decision, because core is std-only until M9 (ARCH §2 lists `libm` under M9); ARCH D12 wording aligned. |
| C07 | major | PLAN, VERIF | fixed | New register field `fix: Data | Code(path) | None`; the cubic-R entry is `UsePaper` with `fix: Code`; §7.2 requires a patch only for `fix: Data`. VERIF's ratio corrected to 1.128e-6 (54 of 97 fluids, map 06 C6) and 1.10e-5 (Water, map 15 §3.1). |
| C08 | major | PLAN, ROT | fixed | As X-26. |
| C09 | major | PLAN, ROT | fixed | PLAN §6 decision 6 (user), default = ROT open detail 1's proposal; M10.6 implements whatever it settles (test `set_config_follows_decision_6`); ROT-015/-019 cite it; ARCH §14 lists it as still open. |
| C10 | major | ROT, PLAN | fixed | Every GAP has a step: ROT-024, -153, -031, -268 lints in M0.3; ROT-139 `doc-excerpts` in M0.4; ROT-143/-144 in M2.10; ROT-057 in M2.3 + M10.2; ROT-067 in M6.2; ROT-114 in M8.4; ROT-110 in M8.1 + M8.8; ROT-111/-112/-113 in M8.9; ROT-048 in M8.11 (datagen check and OrthoHydrogen); ROT-157/-158 in M10.8. Open details 2-6 settled in PLAN §6. Steps were inserted by renumbering, not letters, because no step has run yet; all cross-references updated. Rows flipped; ROT summary recounted (GAP 0). |
| C11 | major | PLAN, ROT | fixed | As X-12. The 9 milestone conflicts were reconciled by generating PLAN's lists from the register and moving four rows where PLAN's ordering is right: ROT-074/-075/-076 M5 → M6 (root toolbox; ROT-074's PT test at M7), ROT-087 M6 → M5 (Clenshaw at M5.2), ROT-164 M7 → M10 (compat tables), ROT-010 → "M5, M8", ROT-057 → "M2, M10". The milestone DoD now requires every ROT row ticked. |
| C12 | major | PLAN | fixed | New PLAN §1.1: one row per BRIEF §1 phrase with building steps and keeping gate ("connected" = registries as values, C ABI, browser, wasip2, plus an on-demand network facade row in §4). `gates lints` checks `unsafe_code = "forbid"` outside capi. M12 trigger is now "v0.1 released: a time-boxed SIMD spike". |
| C13 | major | PLAN | fixed | As X-07. |
| C14 | major | PLAN, VERIF | fixed | Jets → `Term`; Δg/RT → `Identity`; reference anchors → new class `RefAnchor` (VERIF §5); M10.8 dh/dT = cp → `Identity`; M8.7 D₂O numbers reported, not asserted; State ≤ 256. B virial: class `Measured` with DIV-0011's tolerance 1e-10 instead of the suggested `Prop`, because `Prop` (1e-12) equals the measured agreement (map 12 §6.3) and would flake; DIV-0011's exemption narrowed to C and dC/dT. |
| C15 | major | VERIF, PLAN, ROT | fixed | `proof: &'static [u8]`; VERIF §6.6 lists parts per entry; added `two_phase_cp_cv_w_are_undefined` (M5.3, DIV-0004 c_p/c_v), `div_0005` Table 4 part (M6.10), DIV-0003 M2 part and DIV-0006..0008 pins (M2.7); register facts moved after the table transcriptions (M1.13). ROT-010 → "M5, M8". |
| C16 | major | PLAN, ROT | fixed | q_D: `SkipOracle`, `fix: Code` entry (M8.8), replacing check = the record with CoolProp's default q_D reproduces the oracle; exempt λ on **all** rows of the 5 fluids rather than a near-Tc band (the critical term is non-zero everywhere, so a band would leave failing cells); `UsePaper` after Huber 2003. Ammonia: `SkipOracle` with a `TBand` exemption (M8.9). VERIF §6.6 planned entries and ROT-110/-111 match. |
| C17 | major | PLAN, VERIF, ARCH | fixed | One harness spec (VERIF §12: 7 workloads × Water, Methane, R134a, n-Propane, a 12-term fluid; `scripts/baseline/`; `benches/results/<milestone>-<machine>.csv`); gungraun a `cfg(target_os = "linux")` dev-dependency; ARCH §10 fixed (recorded from M2, enforced from M9). The suggested separate bench crate was not created: `phasekit-verify` stays unpublished at v0.1 (PLAN §5.4), which already satisfies dependencies §2.12's "unpublished workspace member" without a sixth crate (ARCH §2 keeps benches in verify). |
| C18 | minor | ROT | fixed | Added ROT-292 (COO-26) and ROT-293 (COO-27), Deferred: M13; ROT-210 gains "(also map 12 COO-15)"; counts now 293 rows, 455 items. |
| C19 | minor | VERIF, PLAN | fixed | 12,355 → map 12 §6.2; 22 KiB → map 09 §4 (22,348 B median; D3 kept for the format); one-row fluids → `CoolProp-Tests.cpp:4798-4815`; 67/68 attributed to all 18 states; M0.5 premise (X-17). |
| C20 | minor | PLAN | fixed | As X-02. |
| C21 | minor | ROT | fixed | Family rows use M15 (PC-SAFT), M16 (Python), M17 (INCOMP), M18 (humid air); 41 rows renamed; §1 and §6 trigger table updated; "post-0.1" kept only for work without a milestone number. |
| C22 | minor | PLAN | fixed | "user decision D14" throughout. |
| C23 | minor | PLAN | fixed | M0.7 bans `once_cell` and `lazy_static` as direct dependencies, adds `cargo shear`, commits `Cargo.lock`; M0.1 installs cargo-shear; VERIF §11.2 lists them. |
| C24 | minor | PLAN, VERIF | fixed | CI fetch-and-cache of `reference/CoolProp` (M0.6, VERIF §11.3); `cargo xtask fetch-fastchebpure` with sha256 lines in `mp/fastchebpure.lock` (M6.1; kept out of `oracle.lock` so the oracle pin is not moved). |

Rejected outright: none. Where the suggested fix was replaced by a different one, the row says why (C03, C14 B
virial, C16 band, C17 bench crate, X-06 manifest split).

## Gate results (sketch untouched)

Run once from `docs/design/sketch` with `CARGO_TARGET_DIR=.../scratchpad/target-final` (fmt without it):

| Gate | Result |
|---|---|
| `cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu` | exit 0 |
| `cargo check --workspace --target wasm32-unknown-unknown` | exit 0 |
| `cargo check --workspace --target wasm32-wasip2` | exit 0 |
| `cargo check --workspace --target x86_64-pc-windows-msvc` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all --check` | exit 0 |
| `cargo test --workspace` | exit 0: 47 passed, 0 failed, 0 ignored (core 24, verify lib 1, gibbs_seam 3, lazy_load 8, new_family 7, register 3, doctest 1) |

Extra check for X-02 (scratch copy of the sketch without compat, the 4 tests and their unused imports and helper):
clippy `-D warnings` clean; `cargo test --workspace` executed 43 tests; wasip2 check clean.

## Final line counts

| Doc | Lines |
|---|---|
| PLAN.md | 1247 |
| VERIFICATION.md | 851 |
| ROT-REGISTER.md | 549 |
