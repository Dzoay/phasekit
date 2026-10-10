# User decisions on the architecture's open questions

First pass, 2026-10-05, answering [ARCHITECTURE.md §14](../ARCHITECTURE.md#14-risks-and-open-questions-for-the-user).
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
| 10 | Cubic borrowing a HEOS ideal gas: which R | **Split by dataset** | `Parity`: the cubic's R (CoolProp 8.0.0 behaviour). `Corrected`: the source model's R, so the ideal-gas part equals its paper. Needs a register entry when the cubic family lands (M11). |
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
| PS1 | How the pseudo-pure PQ sides are checked | **At the oracle's side temperature: PQ's T within `Prop`; at Q = 0 and Q = 1 the side's density, h and s within `Prop` from phasekit's EOS at the oracle's T and the given p; at Q = 0.5 T alone** | Like with like, no tolerance widened and no register entry. Rejected: a divergence entry exempting the side cells. |
| PS2 | The rows near the top where the definition breaks down | **Refuse and count: phasekit refuses (D6, a fit is never evaluated outside its range) with a `DomainError`, the test counts those rows, and the rows where the oracle itself errs are counted, not compared** | As M6.8 counts the rows outside the model's limits. Rejected: a divergence entry; a grid that stops at Θ = 1e-2. |

## The pseudo-pure dome against the nightly grid (2026-10-09)

Asked at M6.9, when the nightly DT grid (40 × 40 PT and 20 × 20 `dome` truths per fluid) first reached the six
pseudo-pure fluids below their critical temperature. Each blend's EOS critical point lies below its published one
(Air 131.87 K against 132.53 K, R407C 356.60 K against 359.35 K), so between them no VLE splits, where CoolProp labels
states by its ancillary density bands; and at SES36's lowest temperatures CoolProp's in-dome p changes with Q at a fixed
T, 1e-8 from phasekit's converged VLE.

| # | Question | Decision | Notes |
|---|---|---|---|
| PS3 | Which rule a pseudo-pure fluid's DT follows below its published Tc | **CoolProp's: gas below 0.95 of the dew density ancillary, liquid above 1.05 of the bubble one, liquid in the 0.9975 strip where the ancillary quality is below 0.01, p is above 1.05 of the bubble pressure ancillary and the state is stable; otherwise the EOS's VLE from the ancillary densities** | For the six pseudo-pure fluids only: their saturation is CoolProp's construct, and decision 4 asks for its rules from oracle fixtures; pure fluids keep the band-free rule (ROT-088). Rejected: the EOS's VLE then the ancillary densities as boundaries; the EOS alone with a divergence entry. |
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
