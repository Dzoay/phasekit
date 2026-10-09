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
