# User decisions

The user's decisions, dated, second only to [BRIEF.md](BRIEF.md) §1 ([AGENTS.md](../AGENTS.md)); a later decision
overrides older text anywhere. Each has a unique id: other docs cite the id and do not restate the decision.

First pass, 2026-10-05, answering [ARCHITECTURE.md §14](ARCHITECTURE.md#14-risks-and-open-questions-for-the-user).
"First pass" means the user may revisit an answer after reading the reference material; a later change is recorded
here with its date.

| # | Question (§14) | Decision | Notes |
|---|---|---|---|
| 3a | Superancillary curves in CoolProp's fluid JSON as default `Exact` saturation | **Ship under CoolProp's MIT notice** | Credit NIST / fastchebpure and cite the SA paper in NOTICE; keep the NIST disclaimer if SA evaluation code is ported. Fact found 2026-10-05: `usnistgov/fastchebpure` has no licence file or statement (GitHub API: `license: null`; README silent); NIST employee works are generally not under US copyright *(inference, not legal advice)*. |
| 3b | Fixtures that may be committed | **All three:** CoolProp-derived outputs; paper check-value tables; fastchebpure dense outputcheck files | Each with provenance and citation (REUSE annotations). Full ~3.5 GB oracle set still regenerated, not committed. |
| 2 | Default dataset | **`DataSet::Corrected`** | `Parity` stays available for oracle comparison. |
| 5 | Helium R (`DIV-0005`) | **Keep CoolProp's R = 8.3144598** | Corrected = Parity for helium. The register records the NIST IR 8474 inconsistency (Table 1 R vs Table 3); the Table 3 fixture tolerance is 5e-7. Entry moves from `Investigate` to a documented, accepted divergence. |
| 4 | Pseudo-pure blends (Air, R404A, R407C, R410A, R507A, SES36) | **CoolProp's ancillary-defined rules now; true-mixture variant offered later, default unchanged** | v0.1 specifies QT (Q ∈ {0, 1}), PQ (T linear in Q) and the in-dome DT rule from oracle fixtures, typed `SatAccuracy::Definition`. At the mixture milestone a true-mixture model (e.g. `R410A.mix`) is registered alongside; the pseudo-pure names keep their behaviour. |
| 6 | Below the triple point | **Refuse by default; opt-in `DomainPolicy::Extrapolate` per call** | Extrapolate permits EOS evaluation of metastable single-phase states (e.g. supercooled water), flagged as extrapolated in the `State`. Superancillary / ancillary fits are never evaluated outside their fitted range under any policy. |
| 10 | Cubic borrowing a HEOS ideal gas: which R | **Split by dataset** (replaced by CB1, 2026-10-15) | `Parity`: the cubic's R (CoolProp 8.0.0 behaviour). `Corrected`: the source model's R, so the ideal-gas part equals its paper. Needs a register entry when the cubic family lands (M11). |
| 12 | `libm` by default | **Decide at M9 on evidence** | Opt-in feature until M9. Default on only if the cross-target hash proves bit-identity *and* the measured speed cost is under a threshold that PLAN.md sets. |
| 9a | C ABI scope at v0.1 | **New `cp_*` ABI + Tier A `CoolPropLib.h` shim** | Tier A (map 11 U11): `PropsSI`, `Props1SI`, `PhaseSI`, `HAPropsSI`, `get_global_param_string`, `get_fluid_param_string`, `get_param_index`, `get_input_pair_index`, `set_config_*`. Tier B (`AbstractState_*`) deferred. Follow-up for PLAN.md: humid air is not in the v0.1 scope, so `HAPropsSI` either returns a documented "not available" error until the humid-air milestone or moves to that milestone. |
| 9b | Shim fidelity | **Match v8.0.0 exactly** | v8.0.0 integer parameter codes and the process-wide, last-writer-wins `errstring` (cross-thread readable for Excel/COM and Mathcad pools, map 11 F5 / #3211). Quirks live only in the shim; `cp_*` keeps per-thread errors and string keys. |
| 11 | Batch layout | **Point-major everywhere** | `out[i*M + k]` for Rust, C and NumPy `(N, M)`; a future SIMD path transposes chunks internally. |
| 8 | WASI component (WIT) | **Later, on demand** | v0.1: Rust on `wasm32-wasip2` tested and supported, no WIT component; add one when a concrete non-Rust WASI host needs it. |
| 7 | Python binding (after v0.1) | **Own namespace + `compat` submodule** | Native API under the project's own import name, plus `<name>.compat.PropsSI(...)` for migration. Never claims the `CoolProp` import name, so it installs alongside real CoolProp. |
| 13 | Plotting (CoolPropPlot) | **Deferred until after the mixtures milestone** | Out of v0.1. The core already exposes saturation, flashes and batch, which is what diagrams need. |
| 1 | Name (D16) | **`phasekit`** (C prefix `pk_`) | Checked 2026-10-05: free on crates.io, PyPI and npm (`thermoprop` is taken on PyPI by an active thermophysical-property library; `matprop` by an unrelated project). 5 GitHub repos already use the name; none checked further. Replaces the `cprs-` / `cp_` placeholders from the plan-doc stage on. |
| D14 | Project code licence | **MIT OR Apache-2.0** | Confirms the D14 assumption. CoolProp's MIT notice ships in `LICENSE-THIRD-PARTY` and the data crate. |

## Follow-ups for the plan-doc stage

- Rename `cprs-*` → `phasekit-*` and `cp_*` → `pk_*` in ARCHITECTURE.md, the sketch and every new doc.
  **Done 2026-10-05 for ARCHITECTURE.md and the sketch:** crate directories, manifests, Rust paths, the C prefix and
  the sketch's blob magic (`PKIT`); neither contains `cprs` or `cp_` any more. New docs must use the new names.
- Decide where `HAPropsSI` sits (9a): a documented "not available" error in the v0.1 shim, or the shim export lands with humid air.
  **Done 2026-10-05:** the v0.1 Tier A shim exports `HAPropsSI` as a stub that returns `_HUGE` and sets a documented
  "not available in this version" `errstring`, so hosts that bind every symbol at load time still load; the real
  function arrives with the humid-air milestone (ARCHITECTURE.md D11, §12).
- Set the `libm` default threshold (12) in PLAN.md's M9 gate.
  **Done 2026-10-05:** PLAN.md M9.5 (bit-identical hash on 4 targets and a geometric-mean slowdown ≤ 5 % over six
  benches, none > 10 %); VERIFICATION.md §9.4 cites it.
- Record DIV-0005 (Helium) as an accepted divergence and add the cubic-R register entry (10) to the M11 plan.
  **DIV-0005 done 2026-10-05:** a new register policy `KeepOracle` (accepted, documented divergence: the oracle value
  is kept, no patch may cite the entry, literature rows asserted at the measured tolerance, 5e-7 for Table 3); used
  in the sketch register, its test and ARCHITECTURE.md §10. **Cubic-R entry done 2026-10-05:** PLAN.md §4 (M11 table)
  and VERIFICATION.md §6.6 (planned entries): `UsePaper`, `fix: Code`, proved in three parts.
- Update ARCHITECTURE.md §14 to point here and mark the questions answered.
  **Done 2026-10-05:** §14 keeps the risk table, lists the decisions taken with a link here, and names what is still
  open (the shim's library file name, needed by M10).

## PLAN.md section 6 decisions (2026-10-05)

| # | Question (PLAN.md §6) | Decision | Notes |
|---|---|---|---|
| P2 | CI host and remote | **GitHub + GitHub Actions**, repo `Dzoay/phasekit` | The agent may create the repo (`gh` authenticated, SSH set up). Commits use the GitHub identity only: `Dzoay <3277116+Dzoay@users.noreply.github.com>`, repo-local config; never a personal email. Rules for README / AGENTS.md: linear history through fast-forward PRs, Conventional Commits, SemVer. |
| P2b | Visibility | **Public from the start** | Licence and NOTICE files must be correct from the first push. |
| P3 | Performance reference machine | **The dev box** (i7-8700K, 6 cores / 12 threads, AVX2) | CPU model recorded with every result; CI tracks relative regressions only. |
| P4 | Paywalled EOS papers (Ammonia, CO₂, R125, Air, Nitrogen, HFE143m) | **Undecided: the user will look into cost and access later** | Until then these six stay oracle-only, labelled provisional; each upgrades to paper-verified when its paper arrives. No milestone waits on them. PDFs never enter the repo. |
| P1 | Shim library file name | **phasekit name only** | e.g. `phasekit_coolproplib.dll` / `libphasekit_coolproplib.so`; MIGRATION.md shows hosts how to point at it (or rename locally). Nothing is distributed under CoolProp's file names. |
| P6 | Shim `set_config_*` | **Translate where an equivalent exists** | The shim keeps its own process-wide settings (quirks live only in the shim) and turns keys with a phasekit equivalent into per-call options on every shim call (e.g. `DONT_CHECK_PROPERTY_LIMITS` → `DomainPolicy::Extrapolate`); other keys are accepted, ignored and listed in MIGRATION.md. No global state reaches the kernel. |
| P5 | Publishing v0.1 | **crates.io + npm at v0.1** | Under the `Dzoay` accounts via trusted publishing from GitHub Actions (no stored tokens); C library binaries as GitHub release assets; PyPI at M16. |
| P7 | How PRs land on `main` | **Squash merge** | One Conventional Commit per PR (the PR title), linear history; squash is the only enabled merge method. |

## Test quality (2026-10-05)

Asked during M0.4: does anything check for useless tests, such as tautological tests that test nothing, before the
test-driven steps begin? Findings: `gates counts` catches tests that stop running and clippy catches `assert!(true)`
and `assert_eq!(x, x)`, but nothing caught a test without an assertion or `assert_eq!(f(), f())`, and VERIFICATION.md
§8.6 had deferred mutation testing past v0.1.

| # | Question | Decision | Notes |
|---|---|---|---|
| TQ1 | Checks for tests that cannot fail | **Both: a static test-shape gate and mutation testing** | `cargo xtask gates assertions`: every `#[test]` asserts (or is `#[should_panic]`) and no `assert_eq!`/`assert_ne!` compares an expression with itself; clippy's tautology lints are probed. `cargo-mutants` is a T4 tool (dependencies §3.1), not a crate dependency. Reverses VERIFICATION.md §8.6. PLAN.md M0.4a; ROT-294. |
| TQ2 | How strict mutation testing is | **Gate PRs on changed code** | `cargo xtask gates mutants` runs `cargo mutants --in-diff` over each PR's Rust changes: every mutant is caught, or excluded in `.cargo/mutants.toml` with a written reason. A full run weekly, report only (M0.6). |

## Paper sources (2026-10-05)

Asked during M1.10: the green-OA copies of Lemmon et al. 2015 and Thol and Lemmon 2016 are NIH author manuscripts that
PMC serves to scripts only as JATS XML, under the statement "This file is available for text mining. It may also be
used consistent with the principles of fair use under the copyright law." (PMC's metadata code `TDM`; not an open
licence, copyright stays with the publishers). Can they be used here?

| # | Question | Decision | Notes |
|---|---|---|---|
| PS1 | PMC author manuscripts as the source of transcribed check values | **Yes, as is** | Fetched anonymously from the PMC Cloud Service (`pmc-oa-opendata`), one of the routes PMC allows for automated retrieval; kept in the gitignored `reference/papers/`, never committed. Only the table's numbers and the citation are committed, on the basis of VERIFICATION.md §3.7 (numerical facts; user decision 3b), not on the text-mining licence. The citation gives `manuscript=` and `xml_sha256=` (VERIFICATION.md §4.2). Publisher PDFs the user supplies remain an alternative. |

## Baseline gaps (2026-10-05)

Asked whether the plan compares speed, memory use and parallel execution: it does, but the CoolProp side of memory and
parallelism came only from Python-level figures (the wheel's per-state bytes, 0.97× on 4 threads under the GIL), and
the M1.15 C++ baseline timed one thread only.

| # | Question | Decision | Notes |
|---|---|---|---|
| BG1 | Close the gaps in the CoolProp baseline now? | **Yes: "close what gaps you can now"** | PLAN.md M1.15a: the C++ harness also records native memory (library first use, bytes per state, every fluid loaded) and thread scaling (1, 2, 4, 6, 12 threads; same fluid and mixed), beside the timing file; VERIFICATION.md §12. |

## Caloric curves (2026-10-06)

Asked at the start of M2 (M2.3 under way): ARCHITECTURE.md §8 step 5 and ROT-027 say datagen precomputes the caloric
curves (h, s, u along both saturation branches on the superancillary's pieces; CoolProp builds them lazily at first use,
45-63 ms per fluid behind a mutex, map 03 §6), but no PLAN.md step built them. The user asked to fill the gap as part of
M2. Computing the curves needs every residual and ideal-gas kind (M4.4) and the superancillary evaluator (M5.2), so the
numbers cannot exist in M2.

| # | Question | Decision | Notes |
|---|---|---|---|
| CC1 | Where the caloric-curve work goes | **Contract in M2, filled at M5.2a** | PLAN.md M2.11 (closes M2): the blob v1 `Caloric` section and layout, `FluidRecord::caloric`, the stamp that binds the curves to the EOS and the α⁰ gauge (exact R rescale and gauge shift, stale otherwise) and the datagen hook, tested on synthetic curves. PLAN.md M5.2a: datagen computes the curves for 130 fluids from the compiled EOS at the SA densities and fills the section; tested against the EOS between nodes, the oracle's `sat` rows and the gauge shift. M7.7 and M7.8 consume them. |

## Readable fluid data (2026-10-06)

Asked during M2 (M2.4 under way): the blobs are binary, so neither a human nor an agent can read or check the data
that ships. The user asked for a readable form in M2.

| # | Question | Decision | Notes |
|---|---|---|---|
| FD1 | A readable, checkable form of the fluid data | **Yes: PLAN.md M2.9a, `cargo xtask fluid list\|show\|diff`** | JSON from the decoded blobs, deterministic, full float precision; in xtask (serde_json is already allowed there, tier T4), so no new crate or dependency. Its key test: the dump parses and re-encodes to the blob byte for byte for all 136 fluids, so the readable form is complete (and the natural input for authoring a fluid later). `every_blob_section_is_dumped` plus a PLAN.md §2.5 rule make every later section-filling step extend the dump. Optional, if it needs no new permission: the CI `linux` job writes `fluid diff origin/main` to its job summary on PRs that change data. |

## Allocation counting (2026-10-06)

Asked at M2.9: the plan's allocation tests (M2.9 `hot_get_allocates_nothing`, later M5.2a and M5.8) need a counting
`#[global_allocator]`, and implementing `GlobalAlloc` needs `unsafe impl`, which `unsafe_code = "forbid"` rules out in
every crate but `phasekit-capi`; no crate of the T3 list provides one.

| # | Question | Decision | Notes |
|---|---|---|---|
| AC1 | How allocations are counted | **allocation-counter as a dev-dependency of `phasekit-verify`** | allocation-counter 0.8 (MIT OR Apache-2.0), tier T3: dev-only, never shipped. It installs its own counting global allocator in the test binaries that use it, so the workspace stays free of `unsafe`. |

## Property tolerance where a relation cancels (2026-10-07)

Asked at M5.1 (PLAN.md §0.4 stop: a gate could pass only by changing a tolerance class). Comparing properties at (T, ρ)
with the oracle, 7 of 80,880 per-PR entries (c_p, w, (∂p/∂ρ)_T next to a spinodal or just outside the near-critical
window) and 584 of 13.5 M nightly entries (p and Z at Z ≈ 1e-4; h, u, s, c_v, c_p, w in deep metastable liquid, R22 at
130 K and −138 MPa) missed `Prop` (1e-12) by up to 127×. Both codes agree there only as far as their α derivatives
do, which `Term` already verifies. The user first approved a floor for the three outputs that inherit (∂p/∂ρ)_T's
error; when the nightly grid showed cancellation inside α^r itself, they approved the general rule below instead.

| # | Question | Decision | Notes |
|---|---|---|---|
| TC1 | How `Prop` treats outputs whose relation cancels | **`Prop` bound = max(1e-12 · max(\|v\|, floor) (nc 1e-8), `Term` carried through the relation)** | The carried scale is Σ_ij \|∂X/∂A_ij\|·M_ij with M_ij the `Term` scales of the order-2 bundle (`phasekit_verify::eos::carried`); no new constant. Measured at M5.1: 0 of 13.6 M entries fail, headroom 0.036; the strict bound is the larger for 89 % of entries and 99 % lie within 15× of it. VERIFICATION.md §5 states the rule; principle 4 ("measured conditioning") and `Identity`'s largest-term scale are its precedents. |

## Caloric curves are starting points (2026-10-07)

Asked at M5.2a (PLAN.md §0.4 stop: the step's tests could pass only with another tolerance class). Degree-12 fits of
h, s, u along both saturation branches on the superancillary's pieces miss the EOS between nodes by up to 1.1e-6 in
the last piece below Tc (h, s, u go like (Tc − T)^β there) and by up to 1e-11 elsewhere (wide low-T pieces, where the
EOS's own rounding is that size); all 130 fluids exceed `SaCoeff` (1e-14) and 129 exceed 1e-12 (measured). The curves'
only consumers are the superancillary-based HQ, SQ, UQ and HS flashes (M7.7, M7.8).

| # | Question | Decision | Notes |
|---|---|---|---|
| CC2 | What accuracy the caloric curves carry | **Starting points on the shared pieces: class `CaloricFit` (2e-6), every answer polished with the EOS** | CC1 and the M2.11 contract stand (one set of pieces with the superancillary). Datagen refuses a fit that misses the EOS at a midpoint by more than `CaloricFit` (the measured 1.1e-6, rounded up); M7.7 and M7.8 polish with the EOS at (T, ρ_SA(T)), so their answers stay EOS-exact. A QT state's h′, s′, u′ come from the EOS at the SA densities, as the oracle computes them (class `Prop`). Rejected: own dyadic pieces to an exact class (amends M2.11, more bytes, deep splits near Tc); no fitted curves (undoes CC1). |

## Build profile and the non-analytic cost (2026-10-07)

Asked at M5.11, after the DT bench. Water's α^r (order 2) took 5.65 µs against C++ CoolProp's 2.7 µs; its 2
non-analytic terms took 4.65 µs of it (the other 54 terms 1.02 µs), because `NonAnalyticBlock` evaluates them on `Jet4`
at order 4 whatever order is asked (order 2 and order 4 cost the same). The workspace had no `[profile]`: opt-level 3,
16 codegen units, no LTO. Measured on the i7-8700K: fat LTO with one codegen unit takes the 54 ordinary terms from
1.02 to 0.71 µs and leaves the non-analytic ones at 4.6 µs; `target-cpu=native` adds about 10 % on those.

| # | Question | Decision | Notes |
|---|---|---|---|
| BP1 | The workspace's build profile | **`[profile.release]` and `[profile.bench]`: `lto = "fat"`, `codegen-units = 1`; no `target-cpu`** | Applies to what the workspace builds itself (benches, and the C library, wheel and wasm module from M10); dependents use their own profile. Bench results from M5.11 on are measured with it; the M2 and M3 rows were not. |
| NA1 | When to make the non-analytic block order-aware | **M9 (step M9.2a), not M5** | Before the `libm` cost rule (M9.5) and the performance gates (M9.6) measure the kernel. |

## Every test run through cargo-nextest (2026-10-07)

Asked during the M5 landing, after PR #62's CI spent 4 hours in `gates mutants` (353 mutants, one job on a 4-vCPU
runner). Every mutant ran the whole workspace through `cargo test`, which runs the test binaries one after another: a
mutant caught by a late binary, or missed, paid for every earlier one, xtask's 51 s datagen tests included. Measured
here (i7-8700K, quiet, pre-built): the host suite 24.3 s with `cargo test`, 17.2 s with nextest and `cargo test --doc`;
wasip2 22.9 s and 10.9 s; M5.11's 7 mutants 8 min 17 s and 2 min 21 s (16-21 s of tests per mutant instead of
166-168 s), the same 7 caught.

| # | Question | Decision | Notes |
|---|---|---|---|
| TQ3 | Which test runner the gates use | **cargo-nextest for everything: G3, G4, `gates counts`, `gates mutants` (fail-fast), CI and the nightly sweep; doctests with `cargo test --doc`** | `.config/nextest.toml`: profile `default` stops at the first failure (the mutants gate), `ci` runs everything (G3, G4, CI), `counts` adds the JUnit report `gates counts` reads. The tool is pinned in CI (0.9.146) and checked by `scripts/check-toolchain.sh`. The mutants gate ends every running test at the first failure, and its test timeout is never shorter than a full workspace run (cargo-mutants timed its baseline on the mutated packages only, so uncaught mutants ran out of time and counted as caught). |

## The multiprecision check points' pressure (2026-10-08)

Asked at M6.3, when 50 of the 390 VLE answers missed class `SatMp` (1e-11) in p while their ρ′ and ρ″ matched the
check points to about 1e-14. The check points' p (CoolProp's `check_points`, and fastchebpure's `p(mp)` in the M6.1
files) is not multiprecision: at each point's own T and ρ″ it differs from the vapour side's pressure (p ≈ ρ″RT at low
pressure, rounding ε-relative) by up to 0.76·ε·ρ′RT·(2A01′ + A02′), the rounding of a liquid-side pressure evaluated in
double precision. In 49 rows that is more than 1e-11 of p, 3.7e-9 for MethylLinolenate at Θ = 0.5; at its triple point
(260 K, fastchebpure) the gap is 2 %. The densities are multiprecision, and ρ″ is well conditioned (the liquid side's
rounding enters it as ε·(2A01′ + A02′)), so the true saturation pressure is computable in double precision.

| # | Question | Decision | Notes |
|---|---|---|---|
| DP1 | How the VLE's p is compared with the multiprecision check points | **Derive p: the reference p is the vapour side's pressure at the check point's own (T, ρ″); `SatMp` stays 1e-11 for T, p, ρ′ and ρ″** | The VLE at a given p solves at that derived p. `check_point_pressures_carry_liquid_side_rounding` pins the evidence (every row within 1·ε·ρ′RT·max(1, \|2A01′ + A02′\|) of the derived p; 49 rows beyond 1e-11), so a release that fixes the column changes the count and reopens this. M6.4's fastchebpure comparisons use the same rule (830 of their 18 875 rows are beyond 1e-11). Rejected: a carried bound on the arbiter's p, max(1e-11, ε·ρ′RT·K′/p), which keeps p independent of the EOS but reaches 4e-9 on heavy fluids at Θ = 0.5. |

## The superancillary against the VLE, and the VLE near Tc (2026-10-09)

Asked at M6.4, on fastchebpure's dense files for the core subset's 12 fluids (18 875 rows down to Θ = 0). The
superancillary evaluated here equals the files' own, misfit for misfit. The VLE matches the multiprecision densities to
about 1e-13 far from Tc (2.5e-13 at Θ ~ 0.1), which is above the 1e-14 floor of `SaFit`, so the planned direct check of
the superancillary against the VLE within `SaFit` fails through no fault of the superancillary. Near Tc, g′ = g″ is
solved on an almost flat Gibbs surface and the double-precision VLE's densities carry its rounding amplified as
Θ^−1.5: within 2.2·ε·Θ^−1.5 of the multiprecision values (p within 1.5·ε/Θ), inside `SatMp`'s 1e-11 down to Θ ≈ 1e-3
and inside the inferred near-critical 1e-6 down to Θ ≈ 3e-7, but 7.8e-5 at Θ ≈ 1e-7 and 2e-3 at Θ ≈ 1e-8, where the
superancillary itself misfits by up to 1e-4.

| # | Question | Decision | Notes |
|---|---|---|---|
| SV1 | How M6.4 compares the superancillary and the VLE | **Each against the multiprecision values: the superancillary within `SaFit` on every dense row, the VLE within `SatMp` (ρ′, ρ″, and p derived as DP1 decides)** | Their agreement follows within `SaFit` + `SatMp`; every class keeps its definition. Rejected: the direct comparison bounded by `SaFit` + `SatMp` (one check mixing two classes); `SaFit` with a 1e-12 floor for this comparison only. |
| NC1 | How `SatMp` treats near-critical rows | **A conditioning bound: ρ max(1e-11, 4·ε·Θ^−1.5), p and T max(1e-11, 4·ε/Θ), Θ = (Tc − T)/Tc, on every row; the measured constants (2.2, 1.5) pinned by a test** | Replaces the inferred 1e-6 for Θ < 1e-3 (VERIFICATION.md §5 said it would be measured at M6): tighter down to Θ ≈ 3e-7, looser closer in, like TC1 carries `Term` through cancelling relations. Below Θ = 1e-8 the VLE is not compared: the bound there exceeds 1e-3 of ρ, and over all 130 fluids it refuses some rows below Θ = 6e-9. Rejected: keeping 1e-6 and comparing only Θ ≥ 1e-6. Reaching multiprecision accuracy near Tc would take extended precision in the near-critical solve; PLAN.md §4 records it as an idea, at the user's request. |

## The near-critical bound across all 130 fluids, and an arbiter divergence (2026-10-09)

Asked at M6.4, when the nightly sweep over all 130 fluids (204 050 rows) ran locally. NC1's constant had been measured
on the 12 core fluids (2.2); 129 fluids stay within 3.5, but R22 needs 22.5 in ρ and 23.5 in p (7e-8 at Θ = 6e-6):
its EOS carries far more rounding in g near Tc. And PropyleneGlycol's dense file disagrees with the v8.0.0 EOS below
227.6 K, the first interval of fastchebpure's fit: ρ″ 0.7 % off at 213 K. phasekit's VLE and CoolProp 8.0.0's own (with
the superancillaries off) agree with each other there to 6e-15.

| # | Question | Decision | Notes |
|---|---|---|---|
| NC2 | How NC1's bound covers fluids whose EOS rounds more | **Carry the `Term` scale: ρ max(1e-11, 4·ε·Θ^−1.5·μ), p and T max(1e-11, 4·ε·μ/Θ), μ = max(1, (M00′ + M01′)/(1 + \|g′/RT\|)), the liquid's `Term` majorant of g relative to g** | As TC1 carries `Term` for properties. μ is 404 for R22 near Tc, 233 for Methanol, 2 to 3 for simple fluids; measured, every fluid is within 0.39·ε·Θ^−1.5·μ in ρ and 0.38·ε·μ/Θ in p. Rejected: a constant of 32 for every fluid (looser for the well-behaved ones); an R22 exception. |
| — | PropyleneGlycol's dense rows below 227.6 K | **A divergence entry: DIV-0016 (`SkipOracle`)** | The check skips the p and ρ″ cells of those rows by citing it; its proof pins CoolProp's VLE values and the remaining gap. Rejected: skipping the rows without an entry. |

## The computed critical point (2026-10-09)

Asked at M6.7. The model's own critical point is solved from the EOS (Newton on K1 = 2A01 + A02 = 0 and
K2 = 2A01 + 4A02 + A03 = 0) and compared with CoolProp's computed columns (fastchebpure's multiprecision metadata).
T agrees to 2e-14 for every fluid but Chlorine and ρc within `Flash`'s near-critical 1e-6 for all but two:
DimethylCarbonate, 2.0e-6 off on a very flat critical isotherm (∂K2/∂ln ρ = 3.7e-5; phasekit's point satisfies K1 and
K2 to 1e-15, while at CoolProp's ρc phasekit's EOS gives K2 = 7.5e-11), and Chlorine, whose critical region is
degenerate (K1 and K2 within 1e-12 of 0 from 7950 to 8153 mol/m³ within 5e-5 K; the two points 2.6 % apart in ρ).

| # | Question | Decision | Notes |
|---|---|---|---|
| CR1 | How the critical point is checked | **The criticality conditions first: every solved model critical point satisfies K1 = K2 = 0 within `Flash`; the oracle's computed columns within `Flash`'s near-critical bounds, but for a divergence entry, DIV-0017 (`SkipOracle`), on Chlorine's and DimethylCarbonate's columns** | DIV-0017's proof pins the evidence (K2 at the oracle's points). Rejected: raising `Flash`'s near-critical ρ bound to 1e-5 with an entry for Chlorine alone. |

## PQ against the oracle (2026-10-09)

Asked at M6.8. CoolProp 8.0.0's PQ takes T straight from its T(ln p) inverse, with no polish on the forward p(T)
(FlashRoutines.cpp:1169, superancillary.h:1272-1274; map 03 §3.3), so its PQ states sit off its own superancillary:
over the 130 pure fluids (3 248 rows, p from its own QT), up to 2.0e-11 in T, 1.1e-6 in ρ and 1.3e-7 in h at
Θ ~ 1e-7 (R245fa), and still 4.5e-14 in T, 6.3e-13 in ρ and 7.4e-12 in h at Θ ~ 0.1. That is beyond `SaCoeff`'s 1e-14.
phasekit's PQ solves the curve's own p(T) = p to rounding (TOMS 748 within the inverse's 1e-9 bracket).

| # | Question | Decision | Notes |
|---|---|---|---|
| PQ1 | How M6.8 checks PQ | **Polish, and a divergence entry: DIV-0018 (`SkipOracle`) on the oracle's PQ cells (T, ρ′, ρ″, h′, h″, s′, s″); each PQ row is checked against the oracle's QT row at the temperature that generated its p, T within `SaCoeff`, and its state equals phasekit's QT at its T** | DIV-0018's proof pins CoolProp's PQ off its own curve. Rejected: copying CoolProp's unpolished inverse (phasekit's inverse is built independently to the same 1e-12 target, so PQ would still need a class of about 2e-11 in T and 1e-6 in ρ near Tc, a tolerance widened to fit CoolProp); keeping the polish and comparing at a new class sized to CoolProp's inverse error (also widened). |

## The pseudo-pure fluids against the oracle (2026-10-09)

Asked at M6.9, on the six pseudo-pure fluids' QT and PQ rows (path `ancillary`). CoolProp inverts a pressure ancillary
by Brent, which stops at about 1e-10 K (`Ancillaries.cpp:82-113`), so near the top of the range its PQ sides sit at
that T: 63 of 282 PQ rows miss `Prop` in density, by up to 4.1e-11 within Θ ≈ 2e-3 of the top, while its PQ T matches
phasekit's (an exact inversion) to 7.6e-13. Within Θ ≈ 1e-3 of the top the bubble-pressure ancillaries of Air and
R407C turn over, so some QT pressures have no root in the ancillary's fitted range (CoolProp then extrapolates by a
secant beyond it), and SES36's own QT fails there.

| # | Question | Decision | Notes |
|---|---|---|---|
| PP1 | How the pseudo-pure PQ sides are checked | **At the oracle's side temperature: PQ's T within `Prop`; at Q = 0 and Q = 1 the side's density, h and s within `Prop` from phasekit's EOS at the oracle's T and the given p; at Q = 0.5 T alone** | Like with like, no tolerance widened and no register entry. Rejected: a divergence entry exempting the side cells. |
| PP2 | The rows near the top where the definition breaks down | **Refuse and count: phasekit refuses (D6, a fit is never evaluated outside its range) with a `DomainError`, the test counts those rows, and the rows where the oracle itself errs are counted, not compared** | As M6.8 counts the rows outside the model's limits. Rejected: a divergence entry; a grid that stops at Θ = 1e-2. |

## The pseudo-pure dome against the nightly grid (2026-10-09)

Asked at M6.9, when the nightly DT grid (40 × 40 PT and 20 × 20 `dome` truths per fluid) first reached the six
pseudo-pure fluids below their critical temperature. Each blend's EOS critical point lies below its published one
(Air 131.87 K against 132.53 K, R407C 356.60 K against 359.35 K), so between them no VLE splits, where CoolProp labels
states by its ancillary density bands; and at SES36's lowest temperatures CoolProp's in-dome p changes with Q at a fixed
T, 1e-8 from phasekit's converged VLE.

| # | Question | Decision | Notes |
|---|---|---|---|
| PP3 | Which rule a pseudo-pure fluid's DT follows below its published Tc | **CoolProp's: gas below 0.95 of the dew density ancillary, liquid above 1.05 of the bubble one, liquid in the 0.9975 strip where the ancillary quality is below 0.01, p is above 1.05 of the bubble pressure ancillary and the state is stable; otherwise the EOS's VLE from the ancillary densities** | For the six pseudo-pure fluids only: their saturation is CoolProp's construct, and decision 4 asks for its rules from oracle fixtures; pure fluids keep the band-free rule (ROT-088). Rejected: the EOS's VLE then the ancillary densities as boundaries; the EOS alone with a divergence entry. |
| — | SES36's in-dome p at its lowest temperatures | **A divergence entry: DIV-0019 (`SkipOracle`) on those p cells** | Its proof pins CoolProp's variation with Q and phasekit's single VLE p. Rejected: an `Investigate` entry with the measured residual as the tolerance. |

## NIST IR 8474 Table 4 (2026-10-09)

Asked at M6.10. Helium's saturation table, NIST IR 8474 Table 4, printed to five digits, does not reproduce with its own
Table 1 constants: 12 cells miss their printed digits, 11 h″ by up to 2.7 half units and p_σ at 5.1 K by 1.5 half units
(3.5e-6; the question put the residual at h″'s 1.7e-6, the p_σ cell having been overlooked). With the shipped constants
7 h″ cells miss, by up to 1.05e-6. DIV-0005's registered 5e-7 is Table 3's.

| # | Question | Decision | Notes |
|---|---|---|---|
| H4 | How Table 4 holds the shipped model | **Its own `Measured` 2e-6 in DIV-0005 (1.05e-6 rounded up to one digit), beside Table 3's 5e-7: each cell within its printed digits or within 2e-6 of the printed value; Helium's saturation arbiter `Inconsistent { residual: 3.5e-6 }`, the worst cell beyond its digits with the paper's constants** | Register entries gain per-table tolerances for this. Rejected: one tolerance of 2e-6 for both tables (Table 3 held looser than its measured 5e-7). |

## The MSRV (2026-10-10)

Asked during M7.1, when a test could not use `f64::next_up` (stable since 1.86) under the workspace MSRV 1.85. The MSRV
never limited tooling: development, lints and CI already use the latest stable (1.99.0); it limits only the language
and library features the source may use. 1.85 was chosen as the edition-2024 floor, Debian 13's packaged rustc and the
Linux kernel's minimum (dependencies R3, open question 1).

| # | Question | Decision | Notes |
|---|---|---|---|
| MS1 | Raise the workspace MSRV? | **1.89, workspace-wide, now** | Concrete features (D17): let-chains (1.88), `f64::next_up`/`next_down` (1.86), and every test builds and runs on it, num-dual's (1.89) included, so the `msrv` CI job runs G3 and the doctests instead of `check --lib`; M12's SIMD needed 1.89 anyway. Users building with Debian 13's packaged 1.85 need rustup or trixie-backports: "users using rustup is fine, it is a common practice in rust development". Rejected: keeping 1.85, a rolling "stable minus N" policy, tracking the latest stable. Its own PR, after the step PRs in flight. |

## The performance reference machine after the move (2026-10-10)

Asked when work moved from the dev box to another, less powerful machine. P3 named the dev box (i7-8700K) as the
machine on which ARCHITECTURE.md §7's absolute targets are checked from M9; the C++ baseline and the recorded benches
are both keyed by CPU model, so the two machines' files never mix.

| # | Question | Decision | Notes |
|---|---|---|---|
| RM1 | Does the new machine replace the dev box as the performance reference? | **No: the i7-8700K dev box stays the reference machine, for now** | "this box can remain the reference for now". The new machine records its own C++ baseline (`cargo xtask baseline`, `coolprop-8.0.0-<machine>.csv`) in its own PR before it records any bench; its benches go to `results/<milestone>-<machine>.csv`, and a PR compares them with that machine's baseline only. The §7 targets stay absolute and are checked on the dev box at each milestone close from M9. To be revisited if the dev box goes. |

## The datagen tests' build (2026-10-11)

Asked after a review of the M7 loop's speed on the laptop: the 26 datagen tests in phasekit-xtask regenerate all 136
fluids each in an unoptimised build: 831 of the native suite's 1 018 test CPU-seconds (134 s wall on the i7-1165G7,
M7.5 stack).

| # | Question | Decision | Notes |
|---|---|---|---|
| SP1 | Build phasekit-xtask and the dependencies optimised in the dev profile (and so `test` and `mutants`)? | **Yes, opt-level 2, phasekit-core too, as a standalone PR before the next stacked step's gates** | Approved as small and low-risk for xtask and the dependencies; measured, that gave 134 s → 117 s, as the datagen tests spend their time in phasekit-core, so the user added phasekit-core: 44 s (335 test CPU-seconds). Floating-point results do not depend on opt-level; datagen's byte-for-byte tests and `gates datagen` check the generated files; debug assertions and overflow checks stay on. A core edit now rebuilds core optimised (a 20 s build against 11 s). The other workspace crates stay at opt-level 0. |

## The CI mutants jobs (2026-10-11)

Asked after a review of the M7 loop: the weekly full mutants run (user decision TQ2) had never run. At about 6,700
mutants and 3.8 s each it would take about 7 h, longer than a GitHub job may run (6 h), and its report steps ran only
after the run, so a killed job would report nothing; it also lacked cargo-nextest, which `.cargo/mutants.toml` names, and
the CoolProp checkout. The PR mutants gate runs last in the linux job, after G3 and the other gates.

| # | Question | Decision | Notes |
|---|---|---|---|
| CI1 | Fix the weekly run? | **Yes: 8 shards, each a job with a timeout, reporting also after a timeout** | Before its first scheduled run (Monday 04:17 UTC). |
| CI2 | Run the PR mutants gate as its own job? | **Yes, in parallel with the linux job** | About 4 min off every PR's wall time, the same coverage; its own PR. |

## A duplication refactor during M7 (2026-10-11)

Asked after a duplication audit of the workspace at `08a7226` (#90; jscpd 5.4.1 and similarity-rs 0.5.0, generated.rs
excluded): 1.3 % of lines are exact copies and 5.1 % of code differs only in names or literals, the hotspot being
phasekit-verify's tests (8.3 %). Six areas: the verify integration tests share no helper module (`label()` three times,
the pinned-generator-environment test four times, the paper-formula AD tests); the check routines of verify's `eos.rs`
and `term.rs`; the blob and pack framing in core's `blob.rs`; the per-family EOS term scaffolding in core, xtask
datagen and verify; parallel mapping tables (compat's CoolProp names, the Prop-to-state mapping); and two pairs of
xtask functions.

| # | Question | Decision | Notes |
|---|---|---|---|
| DR1 | Where does the refactor go? | **M7.9a, after the M7.2-M7.9 stack lands and before M7.10** | Mid-stack it would conflict with every stacked step; M7.10 adds verify check routines and test files, which would otherwise copy the duplicated preambles again. A refactor has no failing test: its gate is behaviour unchanged (PLAN.md M7.9a). |
| DR2 | Is the EOS term scaffolding (area 4) part of it? | **No: its own step, M7.9b, right after M7.9a** | The largest area, across three crates; a PR of its own keeps each reviewable. |
| DR3 | Split phasekit-core's `flash.rs` tests module, where every stacked M7 step appended its tests and rebases conflicted? | **Yes, in M7.9a** | One file per input-pair family under `flash/tests/` and the shared fixtures in `flash/tests.rs`; the tests unchanged. The paper-formula AD tests' scaffolding in verify's `tests/terms.rs` is area 4's, so M7.9b's (DR2). |

## A complexity refactor during M7 (2026-10-11)

Asked after a complexity audit of the workspace at `c094914` (#104; rust-code-analysis 0.0.25 for cognitive complexity
as Sonar specifies it, tree-sitter-rust 0.24.2 for nesting, generated.rs excluded): of 1 739 functions, 33 have a
cognitive complexity over 15 (Sonar's S3776), 33 nest control flow deeper than 3 (S134) and 5 run over 100 lines; 54
cross at least one bar. Tests nest deepest (22 of the 33: loops over fluids, points and pairs around a `match` that
collects failures, six deep in core's `input.rs`); core's `flash.rs` holds the library's five worst functions, which
hand-roll the same three search scaffolds. SonarQube itself is not adopted: for Rust it needs a server or an account,
runs only in CI, and its rules are not pinned with the repository. Clippy's lints of the same family gate instead.

| # | Question | Decision | Notes |
|---|---|---|---|
| CX1 | Where do the steps go? | **M7.9c-M7.9f, after M7.9b and before M7.10** | The gate first, so M7.10's new check routines and tests are held to the bar; then the fixes, each removing its exemptions. |
| CX2 | What bar? | **Sonar's: cognitive complexity 15, control-flow nesting 3, 100 lines per function** | 54 functions at `c094914`; outliers only (25 and 4) would have caught about 20. M7.9c calibrates clippy's thresholds to it; a permanent exemption (an algorithm as printed, a data table) states its reason. |
| CX3 | Production code and tools in one step or two? | **Two: M7.9e (core's flash scaffolds), M7.9f (verify's library and xtask)** | Core's diff needs bit-identical states as its proof; the others do not. |
| CX4 | How does M7.9c gate the bar? (asked after M7.9c's calibration run) | **An xtask gate on `syn` (`gates complexity`) for cognitive complexity and nesting as Sonar specifies them; clippy's `too_many_lines` at 100 for length** | Supersedes CX2's note on calibrating clippy. At `c094914` clippy's `too_many_lines` matches the length bar (4 functions over 100 code lines); `excessive_nesting` counts every block, not control flow (61 functions at 4, 32 of them not over S134's 3); the nursery `cognitive_complexity` is another measure (20 of the 33 functions over 15 score 10 or less, `sx` among them; a tolerance test Sonar scores 2 gets 32). `syn` 3 is already in Cargo.lock through serde's derive; xtask is tier T4. |

## Order-3 outputs (2026-10-12)

Asked before M7.10, after a read of ARCHITECTURE.md §3.7, the oracle and the bench records.

| # | Question | Decision | Notes |
|---|---|---|---|
| OD1 | Which order-3 outputs does M7.10 implement? | **PLAN.md's: second partials over the 12 first-order variables, the fundamental derivative, κ_T and β; Cv, Cp, w, τ and δ as derivative variables deferred** | ARCHITECTURE.md §3.7's M7 row also listed those five; CoolProp's second derivatives refuse Cv, Cp and w (no oracle), and τ and δ need the model trait to expose its reducing constants, an API change of its own. PLAN.md §6 holds the follow-up, and ARCHITECTURE.md's row follows this decision. |
| OD2 | Their tolerances? | **Class `Prop`, with the order-3 bundle's `Term` scales carried through each relation; floors \|X\|/(\|Y\|·\|W\|), 1, 1/\|p\| and 1/T; the identities in class `Identity`** | No new class: the carried bound is absolute, from summed magnitudes, so a second partial that crosses zero needs no special case; the near-critical window as defined. A disagreement beyond it goes through the divergence register; no widening. |
| OD3 | Regenerate the committed fixtures for the new `eos` columns? | **Yes, through the nightly's `regenerate` dispatch (K18)** | The change to gen.py re-hashes every committed fixture, which only the pinned image may write. |
| OD4 | M7's exit gate asks for PT and PH benches: is this machine's record enough? | **Yes** | `results/M7-11th-gen-intel-core-i7-1165g7.csv` holds PT and PH (M7.3); the i7-8700K, the reference machine (RM1), recorded PT only (M7.2). The §7 targets are checked on the dev box from M9. |
| OD5 | Thorade & Saadat (2013), which VERIFICATION.md §8.1 named for M7's identities? | **Fetch it if it is open; otherwise the identities §8.1 itself lists** | It is not: Springer's copy is paywalled and the one public repository copy refuses an anonymous fetch. M7.10 checks the mixed second partial, Maxwell's relation differentiated once more, (∂c_v/∂ρ)_T, (∂c_p/∂p)_T and three κ_T and β relations. |

## Mutants speed-ups (2026-10-12)

Asked after a measurement of the M7 loop (by another session, at the user's request): the earlier speed-ups held (prep
7-8 min to about 3, the CI linux job 10-25 min to 5-7), and what stayed slow was mutation testing, run locally and
again in CI. M7.9f's CI mutants job took 165 min: eight mutants of the `gates assertions` tokenizer looped until the
600 s timeout, one after another (80 min), and each xtask mutant took about 50 s to be caught, against 1-5 s in
phasekit-core, because it waited for the whole workspace's tests. Each change is its own PR, measured before and after,
and merged once its checks are green.

| # | Question | Decision | Notes |
|---|---|---|---|
| CI3 | Make the assertions tokenizer always advance? | **Yes** | Its loops step through ranges and iterators, so a mutant that stalls a token fails a test in seconds instead of running to the timeout. |
| CI4 | Lower CI's mutants timeout floor (`minimum_test_timeout`, 600 s) towards the local pre-check's 240 s? | **Yes, after measuring the workspace suite on the CI runner, with a safe margin** | The weekly shards read the same setting. |
| CI5 | Make xtask's mutants cheaper? | **Yes, by whichever option keeps every mutant caught that is caught now** | The options: test them with xtask's tests only, order nextest's tests so the ones that catch them run first, or make the datagen tests' setup (about 3 s each) cheaper. |

## The friction-theory papers for M8.5 (2026-10-12)

Asked during M8.5: its failing tests name friction-theory paper rows for Methane, n-Pentane, SF₆ and H₂S. SF₆'s and
H₂S's papers print check values, and the step checks them; Methane's and n-Pentane's correlation is
Quiñones-Cisneros and Deiters, J. Phys. Chem. B 110, 12820 (2006), which is paywalled, and CoolProp's tests have no
rows for them.

| # | Question | Decision | Notes |
|---|---|---|---|
| FT1 | Hold M8.5 for that paper, or land it without Methane's and n-Pentane's paper rows? | **Land M8.5; the two fluids' rows wait for the paper** | Both fluids still match the oracle stage by stage. The paper is also the arbiter of n-Pentane's viscosity itself: CoolProp replaced two of its coefficients (`Aa[1]`, `Ai[0]`) with REFPROP 9.1's, and at Vassiliou et al. 2015's check point its η is 27.15 µPa s where that correlation used 49.465 (M8.6, DIV-0025). A copy that reaches `reference/papers/` adds the rows. |

## The docs' layout and upkeep (2026-10-12)

Asked after a review of the docs (five reviewers, 2026-10-12): about 130 findings, nearly all one fact kept by hand in
several places (the current milestone, the divergence entries, gates, rules, measurements), with the copies drifted
apart; and frozen records beside living ones with nothing to tell them apart. Two decisions shared the id PS1.

| # | Question | Decision | Notes |
|---|---|---|---|
| DL1 | Where do the frozen design records go? | **`docs/history/`, the type sketch included** | Never edited; [history/README.md](history/README.md) says so and maps the old paths. |
| DL2 | Where do the user's decisions live? | **`docs/DECISIONS.md`** (was `docs/design/04-user-decisions.md`) | Rank 2 of AGENTS.md's sources of truth, unchanged. |
| DL3 | The two decisions named PS1 | **The pseudo-pure series becomes PP1-PP3**; PS1 stays the paper-sources decision | Every citation renamed with it. |
| DL4 | How a step's PR keeps the docs | **Each fact has one home; a finished step's PLAN.md text becomes one ledger row; its measurements go to the PR body (the squash commit's body) and to the test that pins them** | The homes: PLAN.md §2.1. One PR for the whole clean-up, a commit per stage, at the user's request. |
| DL5 | Does a ledger row carry its PR and commit? (asked after #125: a step's PR cannot name itself before it is opened, nor its squash commit before it merges) | **No: the row is the step's ID and what landed; `git log --grep '^Plan-Step: <id>$'` finds the commit and its PR** | Writing the PR number in would cost every step an extra push and CI run. |

## Sharded CI mutants (2026-10-12)

Asked during M8.9, whose diff has 1 717 mutants against 200-500 for the other M8 steps: after CI3-CI5 a core mutant
costs an incremental build and the tests up to their first failure, about 10-17 a minute here and half that on CI's
runner, so M8.9's CI mutants job would take about four hours.

| # | Question | Decision | Notes |
|---|---|---|---|
| CI6 | Shard the PR mutants job? | **Yes, four shards** | `gates mutants --shard k/4` runs one slice of both passes; a `mutants` job, the required check, passes only when every shard did. The same CI minutes in about a quarter of the wall time. |

## Ammonia's critical conductivity term (2026-10-12)

Asked during M8.9: Tufeu et al. 1984's critical term, as CoolProp codes it, diverges at its own T_c, 405.4 K, at every
density (CoolProp gives 6.26 W/(m K) at 405.3999 K and 50 kg/m³, NaN at 405.4 K). PLAN.md asked only for λ finite and
continuous there; the step chose how.

| # | Question | Decision | Notes |
|---|---|---|---|
| AM1 | How is Ammonia's λ kept finite at 405.4 K? | **Keep the step's floor: the term's reduced temperature is floored at the gap to the EOS's T_c, (405.56 − 405.4)/405.4** | CoolProp's value outside 405.24-405.56 K, finite and continuous inside (DIV-0030, ROT-111). The Corrected model (Monogenidou et al. 2018) remains the alternative after the paper check (map 05 Q3). |

## c_p, c_v and w of a saturated phase (2026-10-13)

Asked after M8.10, which reads η and λ at Q = 0 and 1 from the saturated phase, while c_p, c_v and w stayed refused for
every two-phase state (M5.3). CoolProp gives the saturated phase's: its w reads that phase at Q = 0 and 1, and its c_p
and c_v evaluate the EOS at the mean density, which there is that phase's (Water, CO₂ and Nitrogen identical to the
single phase at the saturated density).

| # | Question | Decision | Notes |
|---|---|---|---|
| SQ1 | Do c_p, c_v and w at Q = 0 and 1 come from the saturated phase? | **Yes, to match CoolProp** | The liquid's at Q = 0, the vapour's at Q = 1, at its own T (E3). Strictly inside the dome they stay `Undefined`, where CoolProp's c_p and c_v are the EOS at the mean density (Water at QT(0.5, 500 K): 35 464 J/(kg K)) (ROT-010, DIV-0004). Asked for these three only: the partial derivatives and the order-3 outputs stay refused at Q = 0 and 1. |
| SQ2 | Do the partial derivatives and the order-3 outputs at Q = 0 and 1 follow c_p, c_v and w? (asked after SQ1 landed) | **Yes, the saturated phase's** | The first and second partials, κ_T, β and Γ read the liquid at Q = 0 and the vapour at Q = 1, the order-3 bundle at that phase's T and ρ. CoolProp's are the single phase's there, bit for bit (Water at 500 K, CO₂ at 280 K, Air at 101 325 Pa). Strictly inside the dome they stay `Undefined`, where CoolProp evaluates the EOS at the mean density (CO₂ at QT(0.5, 280 K): κ_T −1.71e-7 1/Pa) and refuses Γ (ROT-010, DIV-0004). Supersedes SQ1's note that they stay refused. |
| SQ3 | Do c_p⁰ and the residual h, s and g at Q = 0 and 1 follow too? (asked on SQ2's pull request) | **Yes, the same way** | The residual h, s and g read the saturated phase at Q = 0 and 1, CoolProp's there bit for bit, and stay `Undefined` strictly inside the dome, where CoolProp's are the EOS at the mean density (Water at QT(0.5, 500 K): h^r −6 256 J/mol, against its phases' −35 254 and −2 340) (ROT-010, DIV-0004). c_p⁰, a function of T alone, is given in the dome too, at its T, as CoolProp's. |

## Helium's critical conductivity term (2026-10-13)

Asked after M8 (DIV-0035): Hands and Arp 1981's critical term, as REFPROP codes it and CoolProp after it, is NaN in two
lobes either side of the critical isochore from 5.20 to 5.99 K, where phasekit refused λ. No free copy of the paper was
found; the scaled equation it reads is printed in Levelt Sengers, Greer and Sengers 1976 (eq. 3.1, Table 17: Kierstead's
He⁴ data) and Hanley, McCarty and Haynes 1974 (eqs. 21-23), and the code differs from both in three places.

| # | Question | Decision | Notes |
|---|---|---|---|
| HE1 | Keep refusing helium's λ in the lobes, or fix the term from the printed sources? | **Fix it from the printed sources** | The NBS equation as printed: inner exponent 2β (the code has 2/β), x = ΔT*/\|Δρ*\|^{1/β} (the code (ΔT*/Δρ*)^{1/β}), ρ_c 69.58 kg/m³ (the code 69.158, which also sets the damping and the blend's bounds). REFPROP's blend, its bounds, the damping and its two factors stay, as no printed source gives Hands and Arp's. A code fix in the core cannot select by `DataSet` (VERIFICATION.md §6.4), so both data sets carry it and DIV-0035 stays `SkipOracle`: its oracle λ cells of 3.5-12 K are exempt, replaced by NIST TN 1334's 54 rows (within 1.07 %; CoolProp's code 8.1 % off at one, NaN at another). |

## M9's measurements without the reference machine (2026-10-13)

Asked at the start of M9: M9.2 (thread scaling), M9.5 (the `libm` cost rule) and M9.6 (the §7 targets) measure on the
reference machine, the i7-8700K (P3, RM1), which cannot be reached from the laptop (i7-1165G7) where M9 is worked.

| # | Question | Decision | Notes |
|---|---|---|---|
| RM2 | Where are M9's measurements taken? | **Ratios on the laptop, the §7 targets on the i7-8700K** | M9.2's scaling and M9.5's libm/std ratios are relative measurements within one build on one machine, so the laptop records them (N = its 4 physical cores). M9.6's absolute check against ARCHITECTURE.md §7 waits for one command the user runs on the i7-8700K. RM1 otherwise stands. |

## SIMD and the transcendentals (2026-10-13)

Asked after a review of M12 against the code. M12 promised lanes bitwise equal to the scalar path, an in-house vector
exp/ln, and features that never change numerics (K13). The scalar path's exp and ln are std's, though (D12: `libm`
stays opt-in, M9.5), and a vector exp cannot reproduce the platform's bits portably. Measured on the i7-8700K with a
model of n-Propane's 18 power terms: 1 ln and 18 exp take about 61 ns of α^r's 316 ns. With std called per lane, the
speedup is at most 2.5× even if everything else vectorises perfectly 4 wide (about 2.2-2.4× realistically). With a
4-wide exp at 1-1.5 ns per lane it is about 3.2-3.5×.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL1 | How do SIMD lanes stay bitwise equal to the scalar path when std's exp and ln cannot be vectorised bit for bit? | **The `simd` feature requires `libm`** | `simd` enables `libm`. A SIMD build's scalar path therefore runs the `libm` crate, bit-identical across targets (M9.4, `fixtures/hash/libm.txt`), and its lanes are bitwise equal to that path. The lane exp and ln are ports of the locked `libm` crate's, operation for operation. Any other transcendental a laned kernel needs is ported the same way or called per lane through `math`. So `simd` changes numerics only as `libm` does, and `libm` stays the one exception to K13. The cost: the paths a SIMD build does not lane (the flashes) run at libm's 1.35-1.53× of std (D12). The gate (ARCHITECTURE.md §7) is unchanged: it still compares with the M9 record, std's scalar path. *Rejected:* an in-house exp/ln as everyone's reference (reopens M9.5); lanes as a tolerance-checked `Fast` policy (gives up K17's bitwise check); std called per lane (puts the gate out of reach). |

## The v0.1.0 release (2026-10-13)

Asked when M10.9's pull request was ready: PLAN.md §5.4 wants the nightly green for the 3 nights before the tag, and each
registry's first release is manual (dependencies R19).

| # | Question | Decision | Notes |
|---|---|---|---|
| RL1 | When is v0.1.0 tagged? | **Merge M10.9 when green; tag `v0.1.0` and `m10` after three nightly runs on the final main pass** | The nightly had been green every night, but not on the final main. The tags are signed; `v0.1.0` starts the release workflow, which attaches the C library and the npm package to the GitHub release. |
| RL2 | Who does the first crates.io and npm publishes? | **The maintainer** | Given the commands and the trusted-publishing settings; no credential enters an agent's session. The workflow publishes later releases once the repository variable `TRUSTED_PUBLISHING` is `true`. |
| RL3 | Does a pull request merged after M10.9 (#161, issue #143) wait for the tags? | **No: merge it when green; `v0.1.0` and `m10` still go on #159's release commit, 448f9a6** | The nightly runs after that commit count toward RL1's three, with later merges in them. |

## QT and PQ against CoolProp (2026-10-13)

Issue #142, re-planned at M9.6: §7's ≤ 0.1 µs for QT and PQ timed the flash alone, against CoolProp's update, which
evaluates only the superancillary and leaves the saturated phases' EOS to the first property read. A `State` holds
both phases' bundles (D8: `Copy`, no interior mutability), so it pays that work in the flash.

| # | Question | Decision | Notes |
|---|---|---|---|
| PF1 | Defer the phases' bundles (a lazy `State`), restate the target, or build cheaper bundles? | **Restate the row as QT / PQ + h, the property a caller reads, against CoolProp's update + hmolar on the same machine; and build the saturated phases at order 1 strictly inside the dome** | CoolProp's two-phase hmolar evaluates each phase's EOS, so both sides do the same work. The C++ baseline adds `qt_h` and `pq_h` (format v2), which `gates perf --targets` reads for the machine. Strictly inside the dome a state reads only ρ, h, s and u (SQ1-SQ3), so its phases' second order is not computed; at Q = 0 and 1 it is. *Rejected:* a lazy `State`, which breaks D8 and would evaluate again at every read. |

## Starting M12 before v0.1 (2026-10-13)

Asked when the user wanted the SIMD build started. M12's entry trigger is "v0.1 released", D9 rejected SIMD in v0.1,
and only M10.9 (the release) was left. ARCHITECTURE.md §7 also stated the SIMD gate twice: "≥ 2.5×" in its list, and
"≤ 0.1 µs/state and ≥ 2.5×" in its table. From n-Propane's 315.7 ns on the i7-8700K, the second needs about 3.2×.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL2 | When is M12 built? | **Now, landed after v0.1** | M12 is worked on the i7-8700K in its own worktree, its steps stacked. Its PRs merge to `main` only after `v0.1.0` is tagged, so the release carries no SIMD code and D9's rejection of SIMD in v0.1 stands. |
| SL3 | Which gate decides whether `simd` ships? | **≥ 2.5× decides; ≤ 0.1 µs/state is a §7 target** | ≥ 2.5× α^r batch throughput over the scalar path, on AVX2 on the reference machine, is the ship-or-stop gate (D9). ≤ 0.1 µs/state is checked by `gates perf --targets` like the other §7 targets: a miss names its re-plan issue in `ci/perf-exemptions.txt`, as #142-#144 do. |

## M12's exp probe and the policy's name (2026-10-13)

Asked after M12.1. A throwaway 4-lane port of the `libm` crate's `exp` (scratch code, not committed) matched `libm::exp`
bit for bit on 20,000,080 inputs, edge cases included, and took 3.80 ns per lane under AVX2 on the i7-8700K. Scalar
`libm` took 8.1-8.7 ns and glibc 2.98 ns. For n-Propane's 19 transcendentals per state, that estimates the α^r lanes at
about 2.0-2.3× over the scalar path, if everything else vectorises 3-4×: below SL3's 2.5×.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL4 | The probe estimates the gate below 2.5×: prototype it first, build the steps, or close the spike? | **Build the steps as planned** | M12.2-M12.6 are built as production steps, and M12.7 measures the gate, as PLAN.md M12 says. |
| SL5 | The lanes `ExecPolicy` variant's name (M12.3) | **`ExecPolicy::Simd`** | It runs the scalar path where no lane kernel exists or the `simd` feature is off. |

## M12's lane widths (2026-10-14)

Asked at M12.7. M12.4's plan ran the lanes one vector wide (8 on AVX-512, 4 on AVX2, 2 below). With that width,
n-Propane's α^r took 215 ns per state on the i7-8700K, 1.46× over M12.1's 312.9 ns, against SL3's 2.5×. α^r's work in
a lane is chains of dependent multiply-adds, and one vector's latency leaves the pipelines idle. A width sweep at one,
two, four and eight vectors gave 213, 120, 109 and 125 ns on AVX2, and 356, 252, 184 and 192 ns on SSE4.2.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL6 | Run the lanes four vectors wide, a deviation from M12.4's table, or record the gate as failed? | **Four vectors to a lane operation** | The lanes are 32 wide on AVX-512 (unmeasured), 16 on AVX2, and 8 on SSE2, SSE4.2, NEON and simd128. A batch's last partial group runs one vector wide. Each lane is still its state's scalar evaluation, so no bits change. n-Propane's α^r takes 109.2 ns, 2.87× over M12.1, so SL3's gate passes; it misses ≤ 0.1 µs/state by 9 % (issue #171). *Rejected:* M12.4's one vector, which fails the gate. |

## The lanes' expm1 (2026-10-14)

Asked at M12.8, which runs α⁰'s transcendentals other than `exp` and `ln` per lane through `math` unless a measurement
justifies a port. On the i7-8700K with `libm`, `expm1` takes 11.1 ns a call, and n-Propane's four Planck-Einstein terms
call it twice each: about 89 ns of its 129 ns of α⁰ in lanes. 114 of the 136 fluids have such terms.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL7 | Port `expm1` to lanes in M12.8? | **Yes** | The `libm` crate's `expm1` ported like M12.5's `exp`, operation for operation, bitwise. `pow` stays per lane: libm's is far larger to port. |

## Targets as goals, and v0.1.0 on the user's word (2026-10-14)

Asked after M12.7 and the i7-8700K's M10 record, about what PLAN.md §6 listed as open: the §7 misses (issues #143,
#144), the thread-scaling wording (#135), the memory budgets (#146), M12.7's ≤ 0.1 µs/state (#171), the `libm`
backend's failures in phasekit-verify (#170), and when v0.1.0 is tagged (RL1). The user: phasekit is experimental, and
a good improvement is not held back for a target.

| # | Question | Decision | Notes |
|---|---|---|---|
| TG1 | Are ARCHITECTURE.md §7's performance targets, §6's memory budgets and M12's ≥ 2.5× hard gates? | **Desirable targets, not gates, while phasekit is experimental** | A miss is logged with its measurement in an issue labelled `future-work` and named in `ci/perf-exemptions.txt` (or pinned in `tests/allocations.rs`); it does not stop a step or hold back a change that improves on what is there (PLAN.md §0.4). SL3's ≥ 2.5× becomes M12's target: `simd` ships if it is faster than the scalar path. G1-G8, the instruction gate's 5 % regression limit (VERIFICATION.md §12) and the bitwise checks stay gates. |
| TS1 | Issue #135: the thread-scaling target's wording | **Keep ≥ 0.9·N as written** | Met on the i7-8700K, 0.94-0.98·N (`results/M9-intel-core-i7-8700k.scaling.csv`), and on the aarch64 runner, 0.98-1.00·N. The laptop's own ceiling, its control row at 0.82-0.84·N, is below the target, so the laptop does not judge it. |
| PF2 | Issue #143: PT + h over ≤ 3 µs for Water and Methane | **PT + h ≤ ½ of CoolProp's update(PT) on the same machine; ≤ 3 µs stays the goal, logged in #143** | The C++ baseline's `pt`, read by `gates perf --targets` as for PF1. On the i7-8700K (the M10 record against baseline v2) phasekit's PT + h is 2.30× (n-Propane) to 3.86× (Water) faster than CoolProp's update alone. PH keeps ≤ 15 µs, met since #161. |
| PF3 | Issue #144: α^r at order 2 (n-Propane) and DT + h + c_p over ≤ 0.3 µs and ≤ 0.5 µs | **≤ CoolProp's on the same machine (the baseline's `alphar2` and `dt_h_cp`); the absolute targets stay the goals, logged in #144** | Relaxed for now, as PF2 is, in PF1's form. On the i7-8700K, n-Propane's α^r is 1.56× faster than CoolProp's, and DT + h + c_p 1.46× (n-Propane) to 2.48× (R134a). |
| MB1 | Issue #146: the superancillary over 25 KiB for 97 of 130 fluids, and 15.3 MiB RSS with all 136 loaded | **Keep the stored inverse; the budgets become ≤ 40 KiB per superancillary and ≤ 16 MiB RSS; 25 KiB and 8 MiB stay the goals, logged in #146** | The values `tests/allocations.rs` pinned at M9.7: median 27 472 B, PropyleneGlycol's 39 576 B the most; RSS 15.3 MiB, its heap 6.1 MiB within 8 MiB. |
| LB1 | Issue #170: three phasekit-verify tests fail on the `libm` backend | **Log them as known differences of the `libm` backend; investigate later (#170)** | `phasekit_verify::backend::LIBM_KNOWN` lists them: 16 cells of Ammonia's η and its initial-density term and one of Methanol's η outside `transport_direct` (the oracle computed with glibc's maths), and Water's HP round trip at its lowest temperature. On `libm` the tests count those cells instead of failing and assert the counts; on std they are asserted as before. |
| RL4 | When is v0.1.0 tagged? | **When the user says; there is no deadline, and M12 may land first** | Replaces RL1's three nightly runs, RL3's commit and SL2's order: M12's steps merge to `main` when green, so v0.1.0 may carry the opt-in `simd` feature. `m10` still marks M10.9's merge commit, 448f9a6; `v0.1.0` goes on the commit the user names, with the CHANGELOG brought up to date first. |
| PL1 | PLAN.md §6's four unplanned follow-ups | **Plan them** | Milestone M20 (PLAN.md §4), worked after M12: Cv, Cp, w, τ and δ as derivative variables (OD1), the conformance kit's `identities` and `part_additivity`, a `cargo doc` check that denies broken intra-doc links, and the nightly report. |

## The browser module's size and its lanes (2026-10-15)

Asked at M12.9. The browser `.wasm` had one budget for both builds, 1 184 227 B (M10.8's first release size + 10 %).
M12.9's QT and PQ lanes, which `ExecPolicy::Simd` reaches on every build, take the baseline to 1 188 898 B. M12.10
enables `simd` in the `simd128` build, whose size grows with the lane width. Under Node 26 on the i7-8700K, n-Propane's
DT + h + c_p, QT + h and PQ + h batches against the same build's scalar path: 2 lanes 1 221 205 B, 1.30×, 1.22× and
1.17×; 4 lanes, padded, 1 288 342 B, 1.37×, 1.37× and 1.33×; 8 lanes, padded, 1 325 552 B, 1.50×, 1.53× and 1.45×;
SL6's 8 lanes with a one-vector remainder 1 402 513 B, 1.25×, 1.62× and 1.45×. Without `simd` the `simd128` build is
1 165 725 B.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL8 | The browser `.wasm`'s budget, and the lane width of its `simd128` build | **Eight lanes with the last group padded; a budget per build, its measured size + 10 %** | The `simd128` build enables `simd` and runs 8 lanes, a short last group padded as K17 pads it, with no one-vector remainder: on wasm a deviation from SL6, smaller and faster. The budgets: baseline 1 307 788 B (M12.9's 1 188 898 + 10 %), `simd128` 1 458 107 B (1 325 552 + 10 %). The browser package's batch runs `ExecPolicy::Simd` on both builds, with the same bits. *Rejected:* 2 or 4 lanes (smaller, slower); no `simd` in the browser. |

## The cubic family's gas constant, and M11's start (2026-10-15)

Asked when M12 landed. Decision 10 split the R of a cubic's borrowed ideal-gas part by dataset: `Parity` the cubic's
own R, as CoolProp 8.0.0 does; `Corrected` the source model's R, so that α⁰ equals its paper. CoolProp's cubics use
`R_U_CODATA` = 8.31446261815324 J/(mol·K) for everything, the borrowed α⁰ included (map 06 C6, map 15 §2.1). The
borrowed models' own R values differ from it by up to 1.96e-5 (CycloPropane and Propyne, R = 8.3143), 1.10e-5 for
Water (IAPWS-95), and 1.128e-6 for the 58 cubic fluids with R = 8.314472.

| # | Question | Decision | Notes |
|---|---|---|---|
| CB1 | Which R does the cubic family use? | **CoolProp's: 8.31446261815324 (CODATA 2018, exact since the 2019 SI redefinition), everywhere and in both datasets** | Replaces decision 10. A cubic model's one `gas_constant()` is this R under `Parity` and `Corrected`; its α^r and the borrowed HEOS α⁰ are both scaled by it. The borrowed α⁰ is the source model's dimensionless function, unchanged, so a cubic fluid's ideal-gas c_p⁰, h⁰ and s⁰ are its source model's times R_CODATA/R_source. The register records that as an accepted divergence from the source papers, with the oracle kept (`KeepOracle`, like DIV-0005). |
| CB2 | When is M11 built? | **Now, on the i7-8700K, before `v0.1.0`** | The user's word, replacing PLAN.md §4's trigger "v0.1 released". M11's steps are planned first, then built and merged in order when green, as M12's were (RL4). |

## Mixtures: M13's start, seams, gas constant, scope and reach (2026-10-15)

Asked when M11 closed, for M13's plan (`phasekit-mix`). Facts behind the questions: map 04 §10 left the R policy, the
first mixture milestone's scope and the second oracle open; a zero-line core diff would leave a mixture split's mass
quality wrong (`State::from_split` requires one molar mass for both phases, and `Prop::Qmass` returns the molar q),
the density solve crate-private and composed names (`R32[0.7]&R125[0.3]`) unreachable, since compat refuses `&`, `[`
and `]` and a registry layer always decodes to a `PureFluid`. CoolProp v8.0.0 gives every mixture of two or more
components `R_U_CODATA` (`NORMALIZE_GAS_CONSTANTS`, on by default) and scales each component's ideal part by R_i/R;
Tkaczuk et al. (2020) print their Table 8 pressures with Σx_i R_i. GERG-2008 is not in v8.0.0, and Kunz and Wagner
(2012) is paywalled; NIST's AGA8 repository (public domain) carries its coefficients and a check state.

| # | Question | Decision | Notes |
|---|---|---|---|
| MX1 | When is M13 built? | **Now, on the i7-8700K, before `v0.1.0`** | The user's word ("can you do M13 now - mixtures"), replacing PLAN.md §4's trigger "v0.1 + demand for mixtures". M13's steps are planned first, then built and merged in order when green, as M11's and M12's were. |
| MX2 | A zero-line core diff, or core seams first? | **One core seam step before `m13-start`; then the core is frozen to M13's close** | M13.1 adds three seams: each phase of a two-phase state carries its own molar mass (a mixture split's mass quality and mass outputs are right, and a mass quality input reaches the model as one); a public density solve at (T, p) with core's root policy and acceptance gate; a registry layer that resolves names no layer holds (composed mixture names) on first use. `m13-start` is tagged on its merge commit and `gates core-frozen` holds the core from there, as at M11. *Rejected:* a strict zero-line diff (mixtures only by pre-registered names, a second density solver in the family, a wrong mass quality). |
| MX3 | Which R does a mixture use? | **CoolProp's: `R_CODATA` = 8.31446261815324 for every mixture of two or more components, in both datasets** | As CB1. Each component's ideal part keeps its own R (scaled by R_i/R, as v8.0.0), so it equals the pure fluid's. A composition that trims to one component is that pure fluid, with its own R; near it the mixture differs by the R ratio (about 1e-6 in p). Tkaczuk et al. (2020)'s Σx_i R_i pressures are checked at their own convention, the difference recorded as an accepted divergence (`KeepOracle`, like DIV-0005). GERG-2008 keeps its own R = 8.314472, part of the model's definition. *Rejected:* a split by dataset; Σx_i R_i everywhere. |
| MX4 | How much phase equilibrium is in M13? | **Vapour-liquid equilibrium, critical points and phase envelopes** | As the rot register assigns them to M13: the stability test, bubble and dew points and every input pair the model can define, two-phase states with mass balance; Heidemann-Khalil critical points computed once per mixture (ROT-187); a phase-envelope tracer with validity bounds (ROT-188). Liquid-liquid and three-phase states are refused with a typed error. *Rejected:* envelopes at M19; VLE only. |
| MX5 | Do mixtures reach C and the browser in M13? | **Yes: compat, the C ABI and its Tier A shim, and the browser package** | `pk_*` and the shim's `PropsSI` take `HEOS::A[x]&B[y]` and `.mix` names as v8.0.0 does, and `get_global_param_string("predefined_mixtures")` lists them; the browser package serves mixtures, its `.wasm` budgets re-measured at size + 10 % (as SL8). *Rejected:* Rust and compat only. |
| MX6 | GERG-2008's registry prefix | **`GERG2008::`** | `GERG2008::Methane[0.9]&Ethane[0.1]`, and `GERG2008::Methane` for a GERG pure fluid; teqp's spelling (`GERG2008resid`). Components resolve through the registry's usual names and aliases. *Rejected:* `GERG-2008::`. |
| MX7 | Where GERG-2008's coefficients come from | **NIST's AGA8 code (`usnistgov/AGA8`, public domain), pinned at commit 3bdb9ab8** | Transcribed into `phasekit-mix`, credited in NOTICE, and checked number for number by an xtask test against a gitignored fetched copy; cross-checked against the GERG TM15 monograph (the 18 GERG-2004 components) and teqp 0.23.2. *Rejected:* teqp's `GERG.hpp`. |

## Mixtures: the fixture budget (2026-10-16)

Asked at M13.4, whose mixture fixtures took the committed set to 17.17 MB, over `gates fixtures`' 16 MiB (16.78 MB;
M13.3 was at 16.60 MB). M13.5's GERG-2008 fixtures (AGA8's check state and binary set, teqp's vectors, GERG TM15's
tables) add about 1.5 MB, and M13.6-M13.12's kinds (fugacity columns, `mix_flash`, `mix_sat`) perhaps 2 MB more. PLAN.md
§0.4 counts a looser gate as a re-plan.

| # | Question | Decision | Notes |
|---|---|---|---|
| MX8 | The committed fixtures' budget | **24 MiB** | VERIFICATION.md §3.6 and `gates fixtures`; about 3 MB of headroom after M13's close (about 21 MB). *Rejected:* 16 MiB with shrunken grids (fewer states per pair, or a 400-row core `eos` grid: weaker coverage, with more cuts to come); a separate 8 MiB mixture budget beside the 16 MiB one. |

## Mixtures: a name used again (2026-10-17)

Asked at M13.13, which measured `PropsSI` at (T, p) on `R410A.mix` at 73 ms a call against v8.0.0's 4.4 ms (161 ms
against 8.4 on `R407C.mix`). Each call resolves the name to a new mixture, and the label of its single phase needs the
mixture's critical points (M13.8, ROT-009; v8.0.0 compares ρ with ρ_r(z) instead), which M13.7's search finds in 65 ms;
a fluid held across calls flashes the state in 0.78 ms. D8 had kept the library's interior mutability to one-shot
`OnceLock` and `LazyLock`.

| # | Question | Decision | Notes |
|---|---|---|---|
| MX9 | A mixture name used again | **A bounded name→fluid cache** ("the compatibility api containing a bounded name-fluid cache would be fine") | Kept by phasekit-mix's mixture-name resolver (`with_mixtures`), which compat hands every name the registry does not hold, so `PropsSI`, the C ABI, its shim and the browser package all use it (PLAN.md M13.14): the models of the last 64 names, the least recently used dropped first, a refusal never kept. Its one lock is the exception to D8 this decision grants; a model is immutable, so a cached one gives a fresh one's values bit for bit. *Not chosen now:* a cheaper search for the critical point the label needs. |

## Mixtures: phasekit-mix's licence (2026-10-18)

Asked at M13.14 and again at M13's close: phasekit-mix transcribes GERG-2008's coefficients from NIST's AGA8 code
(MX7), whose notice `LICENSE-THIRD-PARTY` and `LICENSES/LicenseRef-NIST-GERG.txt` carry, while the crate's Cargo
`license` was the workspace's alone.

| # | Question | Decision | Notes |
|---|---|---|---|
| MX10 | phasekit-mix's Cargo `license` | **`(MIT OR Apache-2.0) AND LicenseRef-NIST-GERG`, before it is ever published** ("yes, phasekit-mix's Cargo license field should carry LicenseRef-NIST-GERG before it's ever published") | The crate ships the notice as `crates/phasekit-mix/LICENSE-NIST-GERG`, `LICENSES/LicenseRef-NIST-GERG.txt` byte for byte; `deny.toml` allows the `LicenseRef` for phasekit-mix alone and `about.toml` accepts it there. crates.io parses `license` with unknown identifiers refused, and the SPDX parser it uses admits `LicenseRef-` under that mode, so the expression should publish; the first publish confirms it. Publishing stays the user's call. |

## Mixtures: the dev profile (2026-10-18)

Asked at M13's close: phasekit-mix built at opt-level 0 in the dev profile (SP1 had optimised phasekit-core, xtask and
the dependencies), and its debug mixture tests took 50-106 s each (`tests/mixture_family.rs`, M13.15).

| # | Question | Decision | Notes |
|---|---|---|---|
| SP2 | Build phasekit-mix optimised in the dev profile, as SP1 does phasekit-core? | **Yes, opt-level 2** ("don't bother with before and after timings for the build profile. Just look at the difference in CI after the fact. We can always revert.") | `Cargo.toml`'s `[profile.dev.package.phasekit-mix]`, and so `test` and `mutants`, which inherit it. Floating-point results do not depend on opt-level (SP1); overflow checks and debug assertions stay on. Judged by CI's job times after the merge; reverted if they are worse. |

## Panics at the facades (2026-10-18)

Asked at M13's close: issue #219, raised at M13.14, had left two pieces of panic hardening unscheduled, fault injection
at every C export and the browser package's panics, which wasm cannot catch.

| # | Question | Decision | Notes |
|---|---|---|---|
| PH1 | When to do issue #219 | **Before `v0.1.0`, now** ("do the fault injection and harden the C export / wasm code"), and the lints on phasekit-mix and phasekit-core too ("Yes, mix then core") | PLAN.md M24: fault injection along the C exports' paths, the browser package's panic hook, then clippy's `indexing_slicing` and `arithmetic_side_effects` denied in compat, capi and wasm, then phasekit-mix, then phasekit-core, which M13's close unfroze. |

## The site (2026-10-16)

Asked while M13 was being planned: "I would like to have you build a webpage that visualises phasekits capabilities
dynamically [...] Should this even be in the same repo. I would like nice things like themeing on the page - it should
probably be a spa", then "it would be great if this was a github page on the repo that we build and deploy". Facts: the
repository is public; the user turned its Pages site on the same day (source GitHub Actions; the `github-pages`
environment deploys from `main` only). The browser package on `main` (M10.8, M12.10) serves `PropsSI`, the
typed flash, the batch over typed arrays and `withReference` in a baseline and a `simd128` build: enough for a
calculator, saturation domes, isolines sampled by batch, the state under a cursor, property sweeps and an in-browser
benchmark. It embeds five fluids (CO₂, n-Propane, Nitrogen, R134a, Water); nothing builds the packs `withPack` takes
(`phasekit_core::internal::pack` serves tests only), and the registry's names are not exported to JS. PLAN.md §4 had
M19 as a plotting crate checked by visual regression on documented diagrams *(inference)*.

| # | Question | Decision | Notes |
|---|---|---|---|
| WS1 | Where does the site live, and when is it deployed? | **In this repository, under `site/`, as its GitHub Pages site, built and deployed on every merge to `main` whose CI is green** | `https://dzoay.github.io/phasekit/`, built from the commit it shows, so it waits for no npm publish (P5 and RL2 unchanged). The site ships in no crate or package. Pages is on, its source GitHub Actions (2026-10-16). *Rejected:* a separate repository over the published npm package; deploys on milestone or release tags only. |
| WS2 | The site's stack | **A single-page app in Svelte 5, Vite and TypeScript; d3's modules for the diagrams; the browser package in a Web Worker; themes as CSS custom properties** | Hash routes (Pages has no rewrites) with the view's state in the URL. The diagrams draw their lines on a canvas and their axes and labels in SVG. Themes: light, dark and the system's, with accent themes; the diagrams read the same tokens, so they re-theme live. The worker loads the `simd128` build where the browser validates SIMD, else the baseline. *Rejected:* React (a heavier runtime); TypeScript with web components and no framework (the fewest packages, more hand-written state and routing). |
| WS3 | How does the site fit the plan? | **Milestone M21, six steps, one PR each with its `Plan-Step:` footer; it starts now, beside M13, and does not move `MILESTONE`** | M21.4 builds the per-fluid packs; M21.5 waits for M13 (MX5 brings mixtures to the browser package), M21.6 for M19. M21.1 adds the site's gate, G9. *Rejected:* the site inside M19 (it would wait for M13); a lane outside the plan, whose progress git would not record. |
| WS4 | What does M19 build? | **Diagram geometry, drawn by the site** | M19's crate computes saturation domes, isolines and mixture envelopes as polylines with labels and ranges, split at the dome and at failed cells, on core's public API with no new dependency (T1f). The browser package serves them to the site (M21.6), and M16's Python can draw them with matplotlib. Its checks are numerical, and the site's screenshots are its visual check. Replaces PLAN.md §4's visual regression on documented diagrams. Decision 13 still holds: M19 starts after M13. *Rejected:* M19 as a plotting crate that draws, checked by visual regression. |
| WS5 | Mutation testing for the site | **StrykerJS 10 with its Vitest runner: a mutant that survives in the site files a PR changes fails G9; a weekly full run over `site/src`** | Asked after the maintainer's visual test of M21.1 found the calculator's controls snapping back to the URL's view, a wiring bug in a component only Playwright exercised. Stryker judges mutants through unit-test runners (Playwright only through its command runner, which rebuilds and runs the browser suite per mutant), so the calculator's state moves into a module that Vitest tests, its reactive wiring included. A seventh step, M21.7, worked next, before M21.2. T5 gains `@stryker-mutator/core` and `@stryker-mutator/vitest-runner` (Apache-2.0). *Rejected:* a weekly report only; no mutation testing for now. |

## Mixtures in lanes (2026-10-16)

Asked during M13 ("what part of the mixture calculation pipeline takes advantage of simd work?", then "lets plan this
as a follow-up"). Facts: no part does. Core's batch takes its lane path only for a `PureFluid` (`batch.rs`), and
`phasekit-mix` overrides neither `residual_batch` nor `ideal_batch`, so a mixture's batch runs point by point. Its
numerics are scalar: the multi-fluid residual (each component through its own `HelmholtzModel::residual` at the
mixture's τ and δ, then the departures), the composition derivatives on multivariate Taylor numbers, and the flashes'
density solves and Newton iterations. `Lanes<W>` and the lane `exp`, `ln` and `expm1` are crate-private in core
(ARCHITECTURE.md §7); `Real`, which `Lanes<W>` implements, is public and sealed and has no comparisons (§3.1).
Measured in release on the i7-8700K at a load average of about 28, so as proportions only (M22.1 records the baseline
on the quiet machine):

| Mixture | α^r, order 2 | its components' own α^r | composition derivatives, order 2 / 3 | PT flash |
|---|---|---|---|---|
| R32/R125 | 4.4 µs | 1.5 µs | 24 / 45 µs | 2.5 ms |
| Methane/Ethane | 7.3 µs | 3.4 µs | 16 / 34 µs | 19 ms |
| a five-component natural gas | 20.8 µs | 6.5 µs | 115 / 387 µs | 59 ms |

A flash's time goes to the composition derivatives, which its stability test and Newton iterations evaluate many times,
not to α^r alone; and of α^r much is the reducing functions and the departures, which a batch at fixed z computes once
and lanes respectively *(inference)*.

| # | Question | Decision | Notes |
|---|---|---|---|
| ML1 | Which parts of the mixture pipeline go into lanes? | **Batches in lanes, then a time-boxed spike on a flash's inner kernels** | A mixture's α^r and α⁰ batches at fixed z through the components' own lane kernels (M12), the reducing state once per batch and the departures in lanes; DT batches under a phase hint (`Strategy::Direct`); the `simd128` browser build. Then the spike: a flash's two phases, and its stability test's trial phases, evaluated in lanes in the composition derivatives, each kept only if faster (ML4). *Rejected:* batches only (flashes stay scalar); a scalar profiling step first, lanes decided from its baseline. |
| ML2 | When? | **After M13 closes (the `m13` tag)** | Core is frozen through M13 (MX2), and two steps change it: a runner for a family's kernels over `Real` (ML3) and a group hook on `ThermoModel` (ML5). Planned now as milestone M22, which does not move `MILESTONE`. *Rejected:* after `v0.1.0`; starting the parts that touch only `phasekit-mix` beside M13. |
| ML3 | How does a model crate reach core's lane math? | **Through core: the family writes its kernel over the public, sealed `Real`, and core runs it in its lanes, which stay crate-private** | Revised 2026-10-17, before this plan merged ("I want to avoid making lane public"). Beside `ExecPolicy` in `batch`, core adds one public trait whose one method is generic over `R: Real`, a pure function from a few numbers to a few, and one function that runs it over columns of `f64` under an `ExecPolicy`, at `Lanes<W>` inside core (D9: the family's math is the "what", core's executors the "how"). `Real` has no comparisons, so such a kernel cannot branch on a value, and each of its operations on lanes is `f64`'s bit for bit (K17, SL1): a kernel is bitwise its scalar evaluation by construction. The semver promise is that trait and that function, a minor version (D9); `Lanes<W>`, its widths, the dispatch and the lane `exp`, `ln` and `expm1` stay private, and `Real` stays sealed, so methods can still be added. The cubic family's α^r, already over `Real`, can use it (PLAN.md §4). *Rejected:* a public, semver-stable `phasekit_core::lanes` (the first answer, 2026-10-16); under `phasekit_core::internal`, doc-hidden with no semver promise (a family crate uses core's public API only, and a third-party family could not); kept private, the departures staying scalar. |
| ML4 | What decides whether a kernel ships? | **It ships if it is faster (D9)** | Measured on the i7-8700K against M22.1's scalar baseline, the same build; a kernel that is not faster is taken out and its measurements recorded. Targets are recorded, not enforced. Lanes stay bitwise equal to the scalar path (K17), so no result changes either way. *Rejected:* a required speedup that the gate enforces, as SL3 did for M12. |
| ML5 | How does core's batch reach a family's lanes? | **A group hook on `ThermoModel`: the flash of a group of points that core's batch has validated, by default each point's `flash`; core's batch calls it for every model under `ExecPolicy::Simd`** | Decided 2026-10-17 after a review of M22.4; replaces ML2's first answer, a batch lane path for any Helmholtz model under a phase hint. Core cannot build a mixture's state: a hinted DT on a `Mixture` builds it in `phasekit-mix` (its own limits, `State::from_total`, an external solve path), a `PureFluid`'s in core (a pressure check, the melting line), and `ThermoModel` has no method that takes a bundle. The hook keeps what a state is in the family (D3) and replaces the batch's `PureFluid` downcast; `PureFluid`'s hook is its M12 lane path, so its batches do not change. A provided method, so a minor version. *Rejected:* a narrower method, a single-phase state from a total bundle, that core's lane path and the family's own hinted DT both call. |

## Notices for the lane ports (2026-10-17)

Found while planning in-house maths: M12's lane `exp`, `ln` and `expm1` (`crates/phasekit-core/src/num/lanes/`) port
the `libm` crate 0.2.16's operation for operation, constants included. That crate is MIT, and its sources are FreeBSD's
msun under Sun Microsystems' notices ("provided that this notice is preserved"). Neither notice was carried:
`REUSE.toml` annotated the three files as the project's own, and `LICENSE-THIRD-PARTY` did not name them.

| # | Question | Decision | Notes |
|---|---|---|---|
| LC1 | Credit the `libm` crate and Sun for the lane ports? | **Yes, now, in a pull request of its own** | `REUSE.toml` annotates the three files `(MIT OR Apache-2.0) AND MIT AND SunPro` (`LICENSES/SunPro.txt`), and `LICENSE-THIRD-PARTY`, which the C library, the npm package and the site ship, reproduces the crate's MIT notice and the three msun notices. The core is frozen until the `m13` tag (MX2), so the files' own heads (each its msun file's notice) and a `crates/phasekit-core/NOTICE` in the crate's package come after it, before any crates.io publish. |

## In-house maths (2026-10-17)

Asked after a review of the bitwise rule, an FMA probe and a profile, all on the i7-8700K. A `simd` build's single
queries run the `libm` crate's maths (SL1): its PH takes 1.39-1.75× the default build's time (Water 18.4 µs against
10.5 µs; quiet machine, medians). Per Water PH query the flash calls `exp` 371 times, `expm1` 143, `pow` 118 and `ln`
115 (callgrind), and a call of the `libm` crate takes 4.3× std's (glibc's) time for `pow`, 2.4× for `exp`, 1.5× for
`ln` and 1.4× for `expm1` (interleaved runs on a loaded machine, so ratios only): `pow` is about 55-60 % of Water's
difference and `exp` about 30 % (estimates); Methane, n-Propane and n-Heptane call no `pow` there. The probe fused the
α^r term sums, the power terms' exponent and the lane `exp` and `ln` polynomials. With hardware FMA, α^r at order 2 ran
1.05-1.09× faster, PH 1.00-1.03× and the α^r lanes 0.99-1.06×, and results moved by up to 1.9e-13 relative (α^r's
derivatives) and 4.2e-15 (PH's T). With the `libm` crate's software `fma`, as a target without FMA runs it, the bits
were the same, and the lanes took 2-6× as long, α^r 1.5-2× and PH 1.2-1.4×.

| # | Question | Decision | Notes |
|---|---|---|---|
| IM1 | Which builds run in-house `exp`, `ln`, `pow` and `expm1`? | **Every build, if they pass M9.5's rule** | The rule is D12's (user decision 12): the §9.4 hash one on x86_64 Linux, Windows MSVC, aarch64 Linux and wasm32-wasip2, and the geometric mean of the in-house/std median-time ratios over the six scalar benches ≤ 1.05, none above 1.10, measured on the i7-8700K. If they pass, they replace std's and the `libm` crate's in every build, so the default build's results move by up to an ulp a call, once, and `simd` no longer enables `libm`: its lanes then run the scalar path's own functions, which was SL1's reason for `libm` (SL1 is replaced for these four). If they fail, they replace only the `libm` crate's, in the `libm` and `simd` builds, and std stays the default. Either way the lanes stay bitwise equal to the scalar path (K17). Reopens SL1's rejection of "an in-house exp/ln as everyone's reference". *Rejected:* the `libm` and `simd` builds only; an opt-in fast build only. |
| IM2 | Which functions? | **`exp`, `ln`, `pow` and `expm1`** | `pow` is most of Water's and R134a's difference. `expm1` because the lanes compute their own, so `simd` can leave `libm` only if its scalar path is in-house too. The others (`ln_1p`, `sinh`, `cosh`, `atan`, `acos`, `sin`, `tan`, `cos`) stay std's, or the `libm` crate's with that feature, and lanes call them per lane; `sqrt` is correctly rounded on every target. The §9.4 grid reaches only `exp`, `expm1`, `ln`, `powf` and `sqrt` (core's EOS path, read from the code; M23's hash checks it), so IM1's hash needs no other function in-house. *Rejected:* `exp` and `ln` only (Water keeps most of its difference, and `simd` keeps `libm` for `pow` and `expm1`); `exp`, `ln` and `expm1`. |
| IM3 | What accuracy? | **Under 1 ulp** | Faithfully rounded, as glibc's and musl's are: by their own bounds Arm's `exp`, `log` and `pow` (IM5) err by at most 0.511, 0.532 and 0.54 ulp without FMA. Checked against correctly rounded reference values, and each function's worst observed error recorded. *Rejected:* a few ulps, for speed (unmeasured); correctly rounded (slower than glibc, against IM1's rule). |
| IM4 | FMA? | **An opt-in `fast` feature with FMA on every target** | The user: "The policy should be use it everywhere, and it should be opt in on a "fast" build." Planned as M23's last step. Everywhere means on every target: a·b + c is fused through `f64::mul_add`, in hardware or in software, so a `fast` build's results are one set of bits across targets, but not the other builds'. In code it covers the in-house functions (Arm's FMA forms), the α^r and α⁰ term sums and the polynomial evaluations, the paths the probe measured; the solvers stay as they are. `fast` joins `libm` as K13's exceptions, the lanes stay bitwise equal to its scalar path, and outside it clippy still bans `mul_add`. The gain measured on hardware FMA is small (above), and on a target without it `fast` is slower; the browser builds do not enable it. Replaces ARCHITECTURE.md §7's future `Fast` policy as the place for FMA. *Rejected:* recorded now, planned later; FMA only where the hardware has it (results would differ by target). |
| IM5 | Where does the code come from, under which licence? | **Ported from Arm's optimized-routines, under Arm's own licence** | `exp`, `log` and `pow` from its `math/exp.c`, `log.c` and `pow.c` with their 128-entry tables, the algorithms glibc and musl use. Their licence is `MIT OR Apache-2.0 WITH LLVM-exception` (the repository's LICENSE, and each file's SPDX line, "Copyright (c) 2018-2025, Arm Limited"), which keeps both of phasekit's options. Each ported file keeps that expression and Arm's copyright in `REUSE.toml`; `LICENSES/LLVM-exception.txt` comes with the first, and `LICENSE-THIRD-PARTY` and core's NOTICE (LC1) carry Arm's notice. Ported from Arm's repository (or musl's copy) only, never from glibc, which is LGPL (PLAN.md §5.3). Arm's only scalar double `expm1` is a 2.5-ulp one (`math/aarch64/experimental/expm1_2u5.c`, 2.17 ulp observed), so `expm1` starts from M12.8's port of the `libm` crate's (LC1). *Rejected:* written from the papers (Tang's table method, own coefficients): no notice to carry, but slower to build, with the error bounds to show from scratch. |

## Lists that merge (2026-10-18)

Asked during M24, through its session: should the repository's lists live in an SQLite table, so that sessions working
at once stop clashing? Facts: of `main`'s 127 commits since 2026-10-10, `docs/PLAN.md` changed in 106,
`ci/test-counts.txt` in 80, `docs/VERIFICATION.md` in 71, `docs/ROT-REGISTER.md` in 56, `.cargo/mutants.toml` in 42 and
`docs/DECISIONS.md` in 28. Of `.cargo/mutants.toml`'s 129 `exclude_re` entries, 51 name a line and a column, so an edit
above one moves it: M24.4 and M24.5 remap about 50 by script. SP2 and PH1 were both inserted before the same section of
this file, and the second rebase conflicted. Decisions are cited by id (253 citations outside `docs/history/`), never
by a heading's anchor. A new ROT row takes the next free number, and `gates rot` does not check that ids are unique;
the divergence register's `check_register` refuses a duplicate (`RegisterError::DuplicateId`). cargo-mutants 27.1's
`#[mutants::skip]` skips everything inside the function, statement or block it marks, not one operator.

| # | Question | Decision | Notes |
|---|---|---|---|
| LM1 | Should the lists live in SQLite? | **No: text in git stays the only store, with no database derived from it either** ("No SQLite") | Two squash-merged branches that both change a binary file cannot be merged: one side's version wins whole, and a PR's diff of it cannot be read. `rusqlite` links C, a tier decision of its own (DEPENDENCIES.md). The lists hold at most about 300 rows, which grep serves. *Rejected:* a gitignored database built from the text files by `sqlite3`'s command line, for queries only. |
| LM2 | Where do mutant exclusions live? | **In the code, beside what they exclude** ("Mutant exclusions in code") | A marker comment names the mutation under the reason no test can catch it; `gates mutants` resolves each marker to its file, line and column from cargo-mutants' own list, and fails on a marker that matches no mutant. An edit elsewhere never moves one. `.cargo/mutants.toml` keeps the settings, `exclude_globs` and the patterns that name no file. *Rejected:* `#[mutants::skip]`, which skips a whole function or statement and needs the `mutants` crate; entries keyed by function and line text in the toml, still one shared file. After `m24`, whose steps rewrite these entries. |
| LM3 | How does `ci/test-counts.txt` merge? | **By itself: a git merge driver keeps the larger minimum per test binary** ("test-counts merges itself") | Three-way per binary; what it cannot decide (a minimum lowered on one side) it leaves to git as a conflict. `.gitattributes` names the driver and the repo-local git config defines it, set up as signing is (CONTRIBUTING.md); without that config git merges the file as text, as before. |
| LM4 | How are decisions kept? | **One file per section** ("One file per decision") | `docs/decisions/<date>-<slug>.md`, each section moved unchanged; `docs/DECISIONS.md` keeps its preamble and says where decisions live, so every link to it still resolves, and no index lists the files, so none clashes. A gate checks that no id is in two files and that every citation names an id. |
| LM5 | How are ROT and DIV ids kept apart? | **`gates rot` refuses a ROT id on two rows, and `docs/ROT-REGISTER.md` merges by union** ("ROT/DIV ID-clash check") | `merge=union` keeps both sides' lines where both changed one place: two new rows both stay, and a row both sides edited appears twice, which the check refuses. Union also keeps both versions of prose both sides changed at one place, which no gate sees. DIV ids need nothing new: `check_register` refuses a duplicate. |
| LM6 | Where does the work go in the plan? | **Milestone M25, one step and PR per change** ("New milestone M25") | M25 does not move `MILESTONE`. Its first three steps start beside M24; the mutants step waits for `m24`. |
| LM7 | How do M25's steps land? | **M25.1-M25.3 in one PR, a commit each** ("I think that we should do all these updates in one PR but with each having a commit, to save on all the CI") | To save CI runs. The squash merge makes one commit on `main`, and its body carries each step's `Plan-Step:` footer, so `git log --grep '^Plan-Step:'` still finds every step; the step commits stay in the PR. An exception to one plan step per PR (AGENTS.md) for these three steps alone. M25.4 waits for `m24` (LM6), so it lands in a later PR. |
