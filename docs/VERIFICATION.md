# phasekit verification

**Status:** the full verification specification (D13) that
[ARCHITECTURE.md §10](ARCHITECTURE.md#10-verification-architecture-summary) summarises; written 2026-10-05 alongside
[PLAN.md](PLAN.md) and [ROT-REGISTER.md](ROT-REGISTER.md). It refines ARCHITECTURE.md and reverses none of its
decisions; [04-user-decisions.md](design/04-user-decisions.md) overrides anything older. It specifies the
`phasekit-verify` kit and corpus, `scripts/oracle/gen.py`, `cargo xtask {oracle,gates}`, CI and the benches;
*(decision)* marks what it settles. Revised 2026-10-05 after two reviews
([design/05-plan-review-log.md](design/05-plan-review-log.md)).
**One home per artefact.** This document is normative for fixtures, fixture kinds, the committed fluid set, tolerance
classes, the register schema, `cargo xtask gates` rules, the cross-target hash grid and the bench harness. PLAN.md is
normative for steps, gates G1-G8 ([§2.4](PLAN.md#24-standard-gates)) and the `libm` threshold (M9.5); its §2.3 is the
shared names-and-paths table. ROT-REGISTER.md is normative for rot rows and their proof-test names.
**Citations:** "map NN §X / R#" = `docs/coolprop-map/NN-*.md`; K#, R#, T# = the recommendation tables of
`docs/research/{kernel-performance,dependencies}.md`; D#, E#, S# = ARCHITECTURE.md decisions and decision-log issues;
"user decision N" = row N of 04-user-decisions.md (the licence row is "D14"). *(inference)* marks claims grounded in none of these. "Measured
2026-10-05" marks oracle runs made for this document (CoolProp 8.0.0 wheel, `.so` sha256 `05d85591…089be5`).

## 1. Principles

1. **The scalar `f64` reference path is phasekit's specification.** Batch, threads and later SIMD lanes prove bitwise
   equality with it (section 9). Bitwise asserts only within one code path (map 10 R16).
2. **CoolProp 8.0.0 is a provisional oracle.** Its outputs depend on 38 global config keys and hidden environment
   switches (map 10 §4), and it is wrong in known places (map 10 §8.4, map 12 §6.3). It catches our mistakes; it
   cannot certify itself. Its fixtures come from x86-64 Linux only and are never asserted bitwise (K18).
3. **Printed literature arbitrates** (computer-verification tables, IAPWS releases; map 13 §1); multiprecision values
   arbitrate saturation of a given EOS (map 09 §8). A table arbitrates only after it reproduces with its own stated
   constants (section 4.3); NIST IR 8474 fails that test (map 13 §3).
4. **Tolerances derive from provenance** (printed digits, a solver's stopping rule, measured conditioning) and live in
   one class table (section 5). They are never widened to make CoolProp pass (map 10 R4, R5): a disagreement becomes a
   cited register entry with proofs (section 6), never a looser bound or an edited fixture. Every expected value carries
   its provenance (4.1); self-referential values and other implementations never arbitrate (map 10 R6).
5. **Fail closed, assert strongly.** Zero tests run, a missing or unparsable fixture, an unknown header key, a manifest
   mismatch, an unknown DIV id, a bare `#[ignore]`, a register entry without its proof: each fails, none skips (map 10
   R1, R2, R14, R15; map 12 R17). A round trip asserts recovered values against a class, never only "no error" or
   "finite" (map 10 R3); `Smoke` is only for the robustness corpus.
6. **Tests share nothing mutable.** `cargo test` runs in parallel threads; models are immutable (map 10 §7). The only
   global state is CoolProp's, inside the generator, which builds a fresh state per case.
7. **TDD by capability.** Failing tests first, then code; unreached capabilities return typed errors, so every
   milestone ends green (D15). `phasekit-verify` depends on core, never the reverse (D1).

## 2. Layers of truth (L0-L6)

Numbering is ARCHITECTURE.md §10's; map 10 §8.5's L1-L5 (term, EOS, saturation, flash, transport) all sit in L1-L4 here.

| Layer | Unit under test | Seam | Truth (priority order) | Classes | First | Where |
|---|---|---|---|---|---|---|
| L0 unit values | newtypes, `math`, `powi` chain, `Jet4`, δ-factor polynomials, FNV-1a, fixture reader | private functions | hand derivations; literal vectors (FNV self-test `8e75626511d00b5c`, map 10 §3) | `Exact`, `Term` | M0 | `#[cfg(test)]` per crate |
| L1 two derivative mechanisms | every term kind, value and all A_ij to order 4 | `accumulate` vs `Jet4` vs num-dual AD of the paper formula | AD; oracle block isolation (L3) second | `Term` | M3 | core unit tests; `tests/terms.rs` |
| L2 printed tables | compiled record: EOS, α⁰, transport, σ, melting | `FluidRecord` → `compile` → rows | self-consistent paper, IAPWS, P-mp rows (section 4) | `Paper`, `SatMp`, `SaFit`, `Measured` | M4 | `tests/arbiters.rs` |
| L3 oracle fixtures | everything CoolProp computes, on `DataSet::Parity` | fixture kinds (3.5) | CoolProp 8.0.0 (provisional) | per column | M2 | `tests/oracle_<kind>.rs` |
| L4 identities, round trips | relations, partials, gauge, flash, saturation equilibrium, virials, capability matrix | `State`, `relations`, `Fluid::with_gauge` | identities; no data | `Identity`, `Fd`, `Flash`, `Exact` | M5 | `tests/{identities,round_trip,capability}.rs`; conformance kit |
| L5 nightly sweeps | L3 + L4 over 136 fluids, all pairs, full grids | `--ignored` | as L3, L4 | as L3, L4 | M1 (drift check), M3 on (sweeps) | `tests/sweeps.rs` |
| L6 execution | every `ExecPolicy`, 1..N threads, batch vs scalar, later lanes | `batch::evaluate`, `policy_equivalence` | the `Reference` scalar path | `Exact` | M1; rayon M9 | `tests/policy.rs` |

**When sources disagree:**
1. An L0, L1 or L4 failure is a phasekit bug (no external data involved): fix the code, never a class.
2. A self-consistent L2 arbiter beats the L3 oracle: open or extend a register entry (6.4).
3. P-mp beats the oracle for saturation of the same EOS (map 09 §8, map 10 §1); it says nothing about coefficients.
4. Independent evaluations localise faults but never arbitrate: mpmath on the JSON coefficients (map 02 §8: ≤ 5.8e-14
   values, ≤ 3.4e-13 derivatives; map 10 §8.5 step 2), teqp or Clapeyron (`OtherImplementation`), REFPROP out of
   process (map 07; never in CI).
5. Self-referential values (CoolProp-computed "reference" rows, map 10 R6; Air and Neon melting, map 10 §8.1) are
   not used.

## 3. Oracle: CoolProp 8.0.0 fixtures

### 3.1 Generator and invocation

`scripts/oracle/gen.py`: one file, stdlib + `CoolProp` only (no numpy), so the uv environment is exactly the pinned
wheel. `cargo xtask oracle` drives it (alias `xtask = "run --quiet -p phasekit-xtask --"` in `.cargo/config.toml`). xtask builds
the environment from nothing (passing `UV_CACHE_DIR` through if set) and runs exactly:

```sh
env -i PATH="$PATH" HOME="$HOME" LC_ALL=C TZ=UTC PYTHONHASHSEED=0 PYTHONDONTWRITEBYTECODE=1 \
  uv run --no-project --python 3.12 --with CoolProp==8.0.0 \
  python scripts/oracle/gen.py --lock crates/phasekit-verify/fixtures/oracle.lock \
    --kind eos --tier core --out crates/phasekit-verify/fixtures/coolprop-8.0.0 --jobs 0
```

CLI: `--kind {facts,checkpoints,term,eos,crit,sat,flash,transport,sigma,melt,refstate,props,codes}`, `--set S` (the
`facts` set: `smoke`, `register`), `--tier {core|all|full}` (the committed core subset, the committed all-fluid tier,
or the nightly full set; 3.6), `--fluids <a>,<b>` (a subset of the tier), `--rows N` (override of the tier's grid,
3.5), `--seed S` (default 1), `--jobs J` (0 = one per core), `--out DIR`, `--check` (generate to a temp dir, compare
bytes with `DIR`, exit 1 on any difference), `--write-manifest` (rewrite the `MANIFEST.sha256` lines of the files
written). A kind is added to gen.py in the PLAN.md step that first uses it, and its committed files and manifest lines
land in that step's PR. Files are written to a temp name and renamed; nothing is written under `reference/`.

**Start-up assertions** (any failure exits non-zero before a row is computed):
1. Before `import CoolProp`: no environment variable starts with `COOLPROP_` or `PXFLASH_`, and `LC_ALL == "C"`. This
   covers the load-time switches the config system cannot reset (`COOLPROP_DISABLE_SUPERANCILLARIES_ENTIRELY`,
   `…_SUPERANC_HSU_D`, `…_MELTING_CALORIC_HS`, `…_SUPERANC_HS`, `PXFLASH_DIRECT_EOS`, `PXFLASH_INNER_NEWTON`;
   map 10 §4).
2. After import: `__version__ == "8.0.0"`; `__gitrevision__` and the sha256 of `CoolProp.abi3.so` equal the lock;
   `len(FluidsList()) == 136` (map 10 U2).
3. All 38 config keys are set explicitly to the lock's values (`set_config_*`, map 10 U2); `get_config_as_json_string()`
   must then parse equal to the lock's `config_json`.
4. Per fluid, `json.loads(get_fluid_param_string(f, "JSON"))[0]` equals the parsed
   `reference/CoolProp/dev/fluids/<file>.json` (the oracle's data is the repo data, map 10 §4); its sha256 goes in the
   header.
5. The SplitMix64 golden vector (first 4 outputs, seed 1) matches `phasekit_verify::sample`; both sides test it.

**Per case:** a fresh `AbstractState("HEOS", fluid)` per row (map 03 §8; `specify_phase` is sticky, map 01 R12; about
49 µs each, map 10 §5). Reference state `DEF`; only the `refstate` kind calls `set_reference_state` (global, mutating;
map 10 §4, map 15 §8), one child process per reference state. `term` isolates blocks with `add_fluids_as_JSON` on a
renamed one-block clone (map 10 §8.2), only in generator children. A `fork` process pool (children inherit the loaded
library; 5.9× on 6 cores, map 10 §7) computes rows by index and writes them sorted, so bytes do not depend on `--jobs`.
An exception becomes `status = err:<class>`, and so does a call that reports failure only through CoolProp's process-wide `errstring` (`Props1SI` returns `inf` and sets it; gen.py clears it before each call and reads it after, M1.3); the class comes from a fixed message table: `notimpl` (not available or not
implemented), `solver` (iterations, no solution, solver failed), `domain` (out of range, "must be", outside), `other`.
Messages are not stored *(decision)*: they differ between scalar and vector paths (map 14 §10) and contain commas. NaN
is canonicalised and written `nan`; ±∞ as `inf`, `-inf`.

### 3.2 `oracle.lock`

Line-oriented `key value` text *(decision)*, so gen.py, xtask and the kit (via `include_str!`, to check fixture
headers) read it without a parser dependency:

```text
# oracle.lock v1. Editing any line is an oracle-pin move (section 6.5).
package          CoolProp
version          8.0.0
git              ae81610e7d23efc57f9d051c8e70a4d66e87537f
wheel_tag        cp312-abi3-manylinux_2_17_x86_64
so_name          CoolProp.abi3.so
so_size          9050856
so_sha256        05d85591871524e83bb23170e22ee149e1167fb3ef5deaf81b9d248283089be5
python           3.12
platform         x86_64-linux-gnu
runner_image     <OCI digest of the pinned nightly runner image>
uv_command       uv run --no-project --python 3.12 --with CoolProp==8.0.0
fluids_sha256    <sha256 of the sorted "<file name> <sha256>\n" lines of reference/CoolProp/dev/fluids/*.json>
sampler          splitmix64-v1
env_scrubbed     COOLPROP_* PXFLASH_*
locale           LC_ALL=C
config_json      {"ENABLE_SUPERANCILLARIES":true,"NORMALIZE_GAS_CONSTANTS":true, ... all 38 keys ...}
```

`git`, the `so_*` values, 38 keys and 136 fluids were re-measured 2026-10-05 (uv chose Python 3.12.14); the committed lock (M1.3) records them. Since M1.16 `runner_image` is astral's uv 0.12.23 image with Python 3.12 on Debian 13, pinned by digest; `nightly.yml` reads it from the lock and runs the generator in it, and a run dispatched with `regenerate` writes the committed fixtures there and uploads them for commit (K18: fixtures come from this image only; a local `cargo xtask oracle` run is for development and differs in its `# generator:` line). gen.py checks `fluids_sha256` against the fluid directory as part of assertion 4. The lock pins
the oracle's environment only *(decision)*: gen.py's sha256 goes into each fixture header (`# generator:`), so adding
or changing a kind regenerates the affected fixtures but never edits the lock, and the fastchebpure pin (the release
`2026.06.02-v2` that CoolProp's v8.0.0 docs pin, map 10 §8.1) lives with its per-file sha256 lines in
`mp/fastchebpure.lock` (PLAN.md M6.1). Python's patch version and the host libc go into each fixture header, not the
lock: the `.so` calls the host libm, so another glibc may move last bits
*(inference)*, which is why nightly regeneration runs in the pinned `runner_image`.

### 3.3 Fixture file format (`<kind>/v1`)

```text
# fixture: eos/v1
# oracle: CoolProp 8.0.0 git=ae81610e7d23efc57f9d051c8e70a4d66e87537f so_sha256=05d85591...
# generator: gen.py sha256=... python=3.12.14 libc=glibc-2.xx
# config: {... the 38 keys as set ...}
# env: scrubbed COOLPROP_* PXFLASH_*; LC_ALL=C
# fluid: Water json_sha256=... source_eos_hash=... R=8.314371357587 refstate=DEF
# grid: T~U[Tlow,Tmax] rho~logU[1e-6*rhoL(Tlow),rhoL(Tlow)] n=500 seed=1 phase=imposed:gas
# source: coolprop
# columns: T,rhomolar,region,status,p,hmolar,smolar,umolar,cvmolar,cpmolar,speed_sound,Z,...
# units: K,mol/m3,-,-,Pa,J/mol,J/mol/K,J/mol,J/mol/K,J/mol/K,m/s,-,...
# tol: in,in,label,label,prop,prop,prop,prop,prop,prop,prop,prop,...
# bits: fnv1a64=0123456789abcdef
300,55000.12,stable,ok,...
```

Rules, all enforced by `phasekit_verify::fixture` (a violation is an error):
- UTF-8, LF, no quoting, no commas inside cells. Header lines are `# key: value` in the order above. `fixture`,
  `source`, `columns`, `tol` are always required; `oracle`, `generator`, `config`, `env`, `fluid`, `bits` too when
  `source: coolprop`. Unknown or duplicate keys fail. No separate CSV header row; every row has exactly `columns` cells.
- Floats are Python `repr` (shortest round trip); Rust `str::parse::<f64>` is correctly rounded, so every finite value
  is bit-exact (map 10 §8.3, measured for all finite values). Tokens: `nan`, `inf`, `-inf`.
- `bits:` is FNV-1a 64 (offset `0xcbf29ce484222325`, prime `0x100000001b3`, map 10 §3) over the little-endian IEEE
  bits of every float cell in row order, `nan` as `0x7ff8000000000000`. gen.py computes it with `struct.pack('<d')`;
  the kit recomputes it after parsing. This is the M1 "fixture round trip bit-exact" gate.
- `status` is per row: `ok` or `err:<class>`. A `nan` output in an `ok` row means "no number from the oracle" (e.g. w
  at an unstable state; 4.3 % of a Water grid, map 10 §5).
- `tol` names a section 5 class per column (`in` = input, `label` = compared as text: `region` ∈ {stable, metastable,
  unstable}, `phase` in CoolProp's names, `path` ∈ {superanc, vle, ancillary, crit_spline}; map 10 §8.3). Tests take
  tolerances only from here or from a register entry; no tolerance literal appears in a test *(decision)*.
- `.gitattributes` marks `crates/phasekit-verify/fixtures/** -text`, so Windows checkouts keep the hashed bytes
  *(inference: autocrlf would rewrite them)*.
- `source:` → `Provenance`: `coolprop` → `Oracle`; `paper:<bibkey>/<table>` → `Paper`; `iapws:<release>/<table>` →
  `Iapws`; `mp:coolprop-json`, `mp:fastchebpure@<tag>` → `MultiPrecision`; `coolprop-source:<path>@<git>` (names
  and codes read from the reference sources, e.g. the pair-name table of `src/DataStructures.cpp`) → `Oracle`.

### 3.4 Layout and naming

```text
crates/phasekit-verify/
  fixtures/                                   committed; read with include_str!, so the corpus runs on wasip2
    oracle.lock
    MANIFEST.sha256                           "<sha256> <bytes> <rows> <path>" for every COMMITTED fixture
    coolprop-8.0.0/<kind>/<Fluid>.csv         core subset (14 fluids, 3.6)
    coolprop-8.0.0/<kind>/<Fluid>.edge.csv    domain-edge rows (below Tmin, near Tc) where a kind has them
    coolprop-8.0.0/all/<kind>.csv             all-fluid tier: a leading `fluid` column, a few rows per fluid (3.6)
    coolprop-8.0.0/facts/<set>.csv            named oracle facts: `smoke`, `register` (3.5)
    paper/<Fluid>/<bibkey>.<table>.csv        transcribed check tables (4.2)
    paper/<Fluid>/<bibkey>.<table>.check.csv  the independent second transcription (4.2)
    mp/check-points.csv                       the 390 superancillary check points (3 x 130 fluids, 62 kB)
    mp/fastchebpure.lock                      release tag, zip sha256, per-file sha256 (PLAN.md M6.1)
    mp/fastchebpure-2026.06.02-v2/<Fluid>.csv dense outputcheck data, core fluids
    hash/                                     cross-target hashes (9.4)
  fixtures-full/                              gitignored: the nightly full set, same tree under coolprop-8.0.0/
```

`<Fluid>` is CoolProp's canonical name (`R1234ze(E)`); characters outside `[A-Za-z0-9._()-]` are percent-encoded
*(decision)*. `MANIFEST.sha256` lists the committed fixtures only; `cargo xtask oracle --write-manifest` rewrites the
lines of the files it writes, in the PR that changes them, and `gates fixtures` checks every line per PR. The full set
has no manifest *(decision)*: the nightly regenerates it, uses it and discards it, and detects oracle drift through
the committed files instead (3.6). Tests reach the full set through `concat!(env!("CARGO_MANIFEST_DIR"),
"/fixtures-full")` (compile-time; clippy bans run-time env reads, D17); a missing full set fails the nightly tests,
never skips them.

### 3.5 Kinds, columns and grids

| Kind | Columns after the inputs | Grid: core / all-fluid tier / nightly | First (PLAN.md step) | Compared with |
|---|---|---|---|---|
| `facts` | `name, call, status, value`: one named oracle fact per row, `call` written as `PropsSI(H;T;300;Q;1;R134a)` because cells hold no commas; a call is `PropsSI`, `Props1SI` or `AbstractState(backend;fluids;[pair;v1;v2;]method;args…)`, one method on a state (`melting_line`, `d2alphar_dDelta2`) (`set` = `smoke`: the R134a QT enthalpy, Water `Tcrit` and `T_reducing`; `set` = `register`: every value a register entry cites) | one file per set | M1.3, M1.13 | `Exact` (bit-exact parse of what the oracle printed) |
| `checkpoints` | `fluid, Tc, T, p, rhoL, rhoV, p_sa_mp, rhoL_sa_mp, rhoV_sa_mp` from `EOS[0].SUPERANCILLARY` (`check_points`, and `meta` `Tcrittrue / K`, the T_c of Θ = (T_c − T)/T_c), source `mp:coolprop-json` (written to `mp/check-points.csv`); the SA/mp ratios set each point's `SaFit` bound. Points sit near Θ = 0.5, 0.3, 0.1 (fastchebpure's grid); one below the triple point moves up to it | 390 rows (all SA fluids) | M1.17 | `SaFit` (SA), `SatMp` (VLE) |
| `term` | `block_idx` (a block index in `EOS[0].alphar`, `all` for the α^r total, `ideal` for the α⁰ total), `block_type` (the JSON type without `ResidualHelmholtz`), `terms` (the block's term count: datagen merges Power and Exponential blocks into one power list in file order, so a test finds a block's terms by it), the inputs `T, rhomolar` and the `tau, delta` CoolProp then holds, `status`, `alphar` + 14 α^r derivatives to order 4, unscaled, in `AbstractState`'s method order (α⁰ to order 3: the oracle exposes no 4th, map 10 §8.2) | δ ~ logU[1e-8, ρ_max/ρ_r] (ρ_max: saturated liquid at the minimum temperature, the record's density bound), τ ~ U[T_r/Tmax, T_r/Tmin], one set of points per fluid shared by all its blocks, each row evaluated at T = T_r/τ, ρ = δρ_r with the phase imposed; NonAnalytic blocks add 20 points with \|τ − 1\|, \|δ − 1\| ~ logU[1e-6, 1e-2] (map 10 §8.5); total rows (`block_idx = block_type = all`, `terms` the fluid's residual term count) evaluate the fluid itself at the first points of the same grid, each with an α⁰ row at the same point (`block_idx = block_type = ideal`, `terms` its α⁰ block count, the order-4 columns `nan`; M4.3): 100 per block / 4 total rows per fluid (`all/term.csv`, a leading `fluid` column) / 300 per block + 64 total rows per fluid (`term/<Fluid>.csv` of the full set, read by `tests/sweeps.rs`) | M3.1 (blocks), M3.6 (totals), M4.3 (α⁰) | `Term` (section 5) on A_ij = τ^i δ^j ∂^(i+j)α, never on the unscaled derivative relative to itself: CoolProp divides by δ^j, so at δ = 1.8e-8 its ∂⁴α^r/∂δ⁴ for Air's Power block is 7.8e6 where the exact value is 5.18, a difference of 1e-15 of that entry's `Term` scale (measured at M3.1) |
| `eos` | `region, status, p, h, s, u, cv, cp, w, Z, dpdrho_T, dpdT_rho, Bvirial, Cvirial, dBvirial_dT, dCvirial_dT` and the partial, `cp0molar` and residual columns (molar SI) | T ~ U[T_low, Tmax], ρ ~ logU[1e-6·ρL(T_low), ρL(T_low)], phase imposed, T_low = max(Tmin, Ttriple) (map 09 R8): 500 / 8 / 10,000 | M5.1, M5.5 | `Prop` |
| `crit` | `Tc_pub, pc_pub, rhoc_pub` (SA off), `Tc_num, pc_num, rhoc_num` (SA on), `Ttriple, ptriple, Tmin, Tmax, pmax, M, R`; `Ttriple` and `ptriple` are CoolProp's, i.e. the saturation minimum (map 09 R8), not the record's triple point; the all-fluid tier adds a leading `fluid` column | 1 / 1 / 1 | M2.3 published, M6.7 numerical | `Exact` (data constants); `Flash` (numerical point) |
| `sat` | `input, Q, T, p, rhoL, rhoV, hL, hV, sL, sV, path`; `input` ∈ {T, p, `sa`}: `sa` rows come straight from `CP.SuperAncillary(json).eval_sat` (p, ρ′, ρ″; map 03 §8) | Θ = 1 − T/Tc log-spaced 1e-7 … 1 − Tt/Tc, Q ∈ {0, 1}, T and p inputs: 100 / 8 `sa` + 8 QT/PQ / 2,000 (`sa`: 200 T). Pseudo-pure: QT at Q ∈ {0, 1}, PQ at Q ∈ {0, 0.5, 1}, in-dome DT | M5.2 (`sa`), M6.8, M6.9 | `SaCoeff` (`sa` and `superanc` rows), `Prop` (`ancillary` rows) |
| `flash` | `pair, x1, x2, truth, status, T, rho, p, h, s, u, Q, phase` (truth PT or QT) | truth states on (log p, T) and (T, Q) grids, all pairs read off each, never density bands (map 12 §6.4): 6×6 + 4×4 / none / 40×40 + 20×20 (map 10 §2.3) | M5.3 (DT), M7.1 on | `Flash` |
| `transport` | `T, rho, phase, status, eta, lambda, model_kind` (direct, ecs, chung, rhosr, hardcoded) | stable single phase only (map 10 §8.5): 200 / 8 / 5,000 | M8.5 | `TransportDirect` / `TransportEcs` by `model_kind` |
| `sigma` | `T, status, sigma` | 50 / 4 / 500 over the fitted range | M8.4 | `Prop` |
| `melt` | `input, T, p, status` | 50 / 4 / 500 per segment | M8.11 | `Prop` |
| `refstate` | `refstate, T, Q, h_mass, s_mass` (IIR, ASHRAE, NBP) | anchors + 20 / none / same | M10.1 | `Prop`; anchors `RefAnchor` (1e-8 absolute, map 01 §8) |
| `props` | `pair, x1, x2, output, status, value`: the 85 outputs of map 01 §4a via `PropsSI`, long format | 10 / none / 200 states per fluid | M10.2 | per output; trivial constants `Prop`, not `Exact` (map 01 §8: `Tcrit` 647.0959999999873) |
| `codes` | `table, name, index`: parameter codes from `get_parameter_index`, phase codes from `get_phase_index`, input-pair values from `CoolProp.constants`; pair names (collisions included) from `src/DataStructures.cpp:513-572` with `coolprop-source:` provenance (the wheel exposes no `get_input_pair_index`; map 01 R2) | all / none / all | M10.6 | `Exact` (the shim's v8.0.0 codes, D11) |

SA-off saturation is never a fixture *(decision)*: near Tc it uses critical splines with errors up to 3.5 % (map 03
§8). EOS VLE is verified against P-mp instead (4.4, 7.4).

### 3.6 Committed core subset, all-fluid tier and nightly full set

This section is the one definition of the committed fluid set; PLAN.md §2.3 and ARCHITECTURE.md §10 link here. 14
fluids are committed at full core-grid density *(decision)*: map 13 §8's 10 (its answer to map 10 Q6) plus 4 for
proofs and block coverage. Every other fluid is reached per PR through the all-fluid tier below, so a fluid needs a
core-subset place only for dense or multi-block coverage, a divergence proof or a pseudo-pure rule. Budget: core subset
about 8 MiB, all-fluid tier about 2.5 MiB *(inference: about 22 B per cell)*, paper and mp files under 1 MiB; 16 MiB
in total, enforced by `gates fixtures`.

| Fluid | Why (map 13 §8 unless stated) |
|---|---|
| Water | NonAnalytic, Gaussian, PlanckEinstein; every IAPWS arbiter; DIV-0002, DIV-0012; critical `Undefined` (M4) |
| CarbonDioxide | second NonAnalytic set, EnthalpyEntropyOffset; η, λ paper rows; critical `Undefined` |
| R1234yf | Gaussian + PlanckEinstein; Lemmon & Akasaka 2022 Table 7 (6 states); mixtures at M13 |
| R1130(E) | Exponential with g ≠ 1; Huber et al. 2025 Table 4 (OA) |
| Helium | α⁰ only Lead + LogTau + EnthalpyEntropyOffset; quantum fluid; K3 rows; DIV-0005 |
| Ammonia | GaoB (only fluid) |
| R125 | Lemmon2005 (only fluid); α⁰ Power |
| Air | pseudo-pure; PlanckEinsteinGeneralized; η, λ rows |
| Nitrogen | PlanckEinsteinFunctionT; DIV-0003 |
| HFE143m | CP0Constant + CP0PolyT |
| R1234ze(E) | DIV-0001 exemplar (Thol 2016 Table 3, OA) |
| Methanol | DoubleExponential, oracle-only (map 13 §8 gap) |
| n-Heptane | CP0AlyLee, arbitrated by the closed-form c_p⁰ (map 13 §8 gap) |
| R410A | pseudo-pure with distinct bubble and dew points (E3; user decision 4) |

The subset covers all 17 block types in use (map 10 U3). The fluids of the Lemmon 2016 table (R227EA, R365MFC, R115,
R13I1) and HeavyWater need no core-subset place: their arbiters are paper rows (4.4), and the all-fluid tier carries
their oracle rows.

The **all-fluid tier** (`coolprop-8.0.0/all/<kind>.csv`, committed, run per PR on every platform) holds, for every
fluid that has the model: 4 `term` total rows each for α^r and α⁰, 1 `crit` row, 8 `eos` rows, 8 `sa` and 8 QT/PQ
`sat` rows, 8 `transport`, 4 `sigma` and 4 `melt` rows (grid column of 3.5). It catches per-fluid data and decoding
errors, which any state reveals; the dense grids stay nightly. Unpublished fluids (Propylene, SES36, Neon) appear only
as oracle rows, labelled (D14, map 13 R7).

The **nightly full set** (136 fluids × every kind × the nightly grid, about 3.5 GB raw, map 10 §8.3) is generated by
`cargo xtask oracle --tier full` in the pinned runner, used by the `--ignored` sweeps, and discarded. Before it, the
nightly regenerates the committed files twice: the first run must equal the repository bytes (else the oracle
environment drifted) and the second the first (else gen.py is non-deterministic); either failure stops the job before
any result is believed. Per PR, CI only re-hashes the committed files (no Python).

### 3.7 Licensing and provenance annotations

All three fixture kinds may be committed (user decision 3b; D14), each with provenance and a citation through REUSE 3.3
`REUSE.toml` annotations (dependencies §2.13). `reuse lint` is a gate (section 11); the full set is never committed.

| Path | SPDX-License-Identifier | SPDX-FileCopyrightText | Citation carried by |
|---|---|---|---|
| `coolprop-8.0.0/**`, `mp/check-points.csv`, `oracle.lock` | `MIT` | CoolProp's copyright line, verbatim from its v8.0.0 LICENSE | header `oracle:` line |
| `paper/**` | `MIT OR Apache-2.0` (the transcription) | phasekit contributors | header `citation:` (DOI or report, table, page); the cells are numerical facts *(inference: facts are not copyrightable; not legal advice)* |
| `mp/fastchebpure-*/**`, `mp/fastchebpure.lock` | `LicenseRef-fastchebpure`; its `LICENSES/` text records the user-decision-3a fact (no upstream licence file; NIST work generally not under US copyright, *inference*) and the NIST disclaimer | NIST (fastchebpure authors) | `source: mp:fastchebpure@2026.06.02-v2` + the superancillary paper in NOTICE |

## 4. Literature arbiters

### 4.1 What arbitrates

Strongest first (map 13 §1): (1) computer-verification tables and IAPWS releases: K1 α⁰/α^r and derivatives at one
(τ, δ), K2 (T, ρ) → p, c_v, c_p, w, K3 saturation rows; (2) printed correlation equations evaluated independently (c_p⁰,
σ with Mulero's own Tc, melting; map 13 §3); (3) printed property tables (lower precision); (4) the oracle only. P-mp
values (fastchebpure, the 390 check points) are exact for the EOS: they arbitrate saturation, not coefficients
(map 10 §1). `Provenance::is_arbiter()` is true only for `Paper`, `Iapws`, `MultiPrecision` (sketch `tolerance.rs`).

### 4.2 Transcribing a printed table

1. Download the paper anonymously into the gitignored `reference/papers/` (the fetch list is PLAN.md M1.8 and M8.2).
   Read the rendered page (PDF view or print), never PDF text extraction alone (map 12 §6.4). Confirm table numbers:
   map 13 §3 notes the IAPWS-95 table numbers 6 and 7 were from memory. A green-OA copy that is a PMC author
   manuscript is read from its JATS XML, fetched anonymously from the PMC Article Datasets
   (`https://pmc-oa-opendata.s3.amazonaws.com/<pmcid>.<ver>/<pmcid>.<ver>.xml`; the PMC PDF sits behind a browser
   proof-of-work; the Cloud Service is one of the routes PMC allows for automated retrieval). Each `<td>` holds one
   printed string, so this is no PDF text extraction; the XML has no pages, so the citation gives
   `manuscript=<pmcid>.<ver>` and `xml_sha256=` in place of `page=` and `pdf_sha256=` (PLAN.md M1.9). Such a file
   states "available for text mining … fair use" (PMC code `TDM`), not an open licence: it stays in
   `reference/papers/`, and committing its numbers rests on 3.7, not on that statement (user decision PS1).
2. Write `fixtures/paper/<Fluid>/<bibkey>.<table>.csv`:

```text
# fixture: paper/v1
# source: paper:Thol-IJT-2016-R1234zeE/Table3
# citation: doi=<doi> table=3 page=<page> erratum-checked=<date> pdf_sha256=<sha256 of the file read>
#   (a PMC author manuscript: doi=<doi> table=<n> manuscript=<pmcid>.<ver> erratum-checked=<date> xml_sha256=<sha256>)
# kind: K2
# constants: R=8.3144621 M=<as printed> Tc=<...> rhoc=<...> Tr=<...> rhor=<...>   (as printed, with units)
# transcribed: <who>; checked: <who>
# columns: T,rho,p,cv,cp,w
# units: K,mol/dm3,MPa,J/(mol K),J/(mol K),m/s
# tol: in,in,paper,paper,paper,paper
```

3. Cells keep the printed strings exactly, trailing zeros included: they set the tolerance. Digit-group spaces are
   dropped and `× 10ⁿ` is written `en`, so `0.996 556 0 × 10³` is `0.9965560e3` (PLAN.md M1.8); a release without a DOI
   cites `report=<id>` in place of `doi=`; `page=` is the printed page number, which can differ from the PDF's (NIST IR
   8474 page 16 is PDF page 22). The kit runs `from_printed` on the string (half a unit in the last printed digit, map
   13 §3), then converts value and bound to SI by `units`. An output cell left empty is a value the source does not give
   (`Cell::Blank`: R1234yf's ρ = 0 row has no p in CoolProp's comment, three papers print no c_v); an input cell is
   never empty. A `label` column `note` may follow the values; a note that names a divergence (`row 4 DIV-0014`) must
   name one registered for the file's fluid (`row_notes_cite_registered_divergences`).

   Rows a paper reaches us through CoolProp's own tests (map 10 §8.1, PLAN.md M1.12) are transcribed from
   `CoolProp-Tests.cpp` at v8.0.0: the citation gives `tests=src/Tests/CoolProp-Tests.cpp:<lines>@ae81610e` and
   `cpp_sha256=` in place of `page=` and `pdf_sha256=`, and the cells keep CoolProp's strings (SI literals such as
   `21.17909e6` Pa keep the paper's digits, so `from_printed` gives the paper's half unit). Where the paper is open, the
   row is re-checked on its rendered page and the citation adds that `page=` and `pdf_sha256=`, the header its
   constants.
4. Double entry *(decision)*: the second, independent transcription is committed beside the first as
   `<bibkey>.<table>.check.csv`, and the test `paper_tables_double_entry_agree` requires the two to match string for
   string, so the evidence stays checkable. The two are written by two agent sessions that cannot see each other's
   output (the second starts fresh in a separate worktree at the parent commit, with only the citation and the header
   template; PLAN.md M1.8), or by a person and an agent; a mismatch is settled by a third read of the rendered page.
   `checked:` names the second transcriber.
5. Record the paper's stated constants (R, M, T_r, ρ_r, the Tc of every block that has its own; map 13 R1 remedy) in
   the header and in `ARBITERS` (4.3).
6. Erratum check via the Crossref `updated-by` relation of the DOI (map 10 §8.4, map 12 R11), queried anonymously: no
   `mailto` polite-pool parameter and no personal identifier in any URL, header or payload. Never "fix" a paper value
   without a published erratum (map 10 §8.5 rule 3).

### 4.3 Self-consistency before a table arbitrates

`ARBITERS` (`phasekit-verify`, map 13 A1) holds one record per (fluid, part ∈ {α^r, α⁰, η, λ, σ, melting,
saturation}): citation {key, DOI or report, role}, table {file, kind, rows}, printed constants, and a status.

| Status | Meaning | Arbitrates? |
|---|---|---|
| `Expected` | a table is expected, not yet obtained (74 fluids, map 13 §1) | no |
| `Transcribed` | double-entered, not yet evaluated | no |
| `SelfConsistent` | every row within `Paper` with the paper's own constants | yes |
| `Inconsistent { residual }` | no constant set stated in the paper reproduces it; residual recorded | no: `Investigate` or `KeepOracle` |
| `None`, `Unpublished` | stated absent, or no paper (Propylene, SES36, Neon) | no; oracle-only fixtures |

Procedure (`tests/arbiters.rs`, on the mutable `FluidRecord` seam, ARCHITECTURE.md §3.8):
1. Decode under `Parity`; diff R, M, T_r, ρ_r and per-block Tc against the printed constants (map 13 §3 lesson: check
   these before blaming the evaluator). `stored_constants_match_their_arbiter_records` (M4.7) audits R, M, T_r and ρ_r
   of every record with transcribed constants (9 fluids) within half a unit of the last printed digit: a mismatch
   must be a registered `GasConstant` or `Reducing` divergence of that fluid, and the two found are pinned
   (R1234ze(E)'s R, DIV-0001; Helium's, DIV-0005). Map 13 R1's other 14 R candidates enter the register as
   `Investigate` only once a paper's printed R is transcribed; none is yet.
2. Set the paper's constants, `compile`, evaluate every row. All within `Paper` → `SelfConsistent`; otherwise
   `Inconsistent { residual }`. NIST IR 8474 Table 3 is 1.0e-6 to 1.4e-6 off with its own Table 1 R, so it cannot
   arbitrate (map 13 §3 item 4).
3. Evaluate with the v8.0.0 constants. Both pass → no divergence. Paper passes, v8.0.0 fails → `UsePaper` candidate
   (DIV-0001: ≤ 1.9e-7 with R = 8.3144621).
4. The test asserts the recorded status, so an arbiter cannot change category silently.

### 4.4 Core arbiter set

| Fluid / part | Arbiter | Kind | Status (map 13 unless stated) | Gate |
|---|---|---|---|---|
| Water α | IAPWS R6-95(2018) α table, 500 K, 838.025 kg/m³ ("Table 6") | K1 | oracle ≤ 2.9e-9; `SelfConsistent` at M4.5 (Tables 6 and 7, both constant sets) | M4 |
| Water properties | IAPWS-95 (T, ρ) table ("Table 7"), saturation table ("Table 8") | K2, K3 | ≤ 2.6e-9 (K2); Table 7 within its printed digits at M4.5 through `State::from_total` | M4.5 (Table 7), M6 (Table 8) |
| Water η, λ, melting, σ | IAPWS R12-08, R15-11, R14-08, R1-76(2014) | rows | R14-08 decides DIV-0002; CoolProp's σ is Mulero 2012, so IAPWS σ is a separate labelled comparison (map 10 §8.4) | M8 |
| R227EA, R365MFC, R115, R13I1 | Lemmon et al. 2016 Table 7 (OA), 12 states | K2 | oracle ≤ 4.3e-7, within 7 digits | M5 |
| R1234ze(E) | Thol et al. 2016 Table 3 (OA) | K2 | DIV-0001 | M5 |
| Helium | NIST IR 8474 Tables 3, 4 (OA) | K2, K3 | `Inconsistent`; DIV-0005 at 5e-7 | M5, M6 |
| R1234yf | Lemmon & Akasaka 2022 Table 7 (6 states, in CoolProp's tests) | K2 | part of the 18 CoolProp test states, of whose 68 values 67 are within half a digit (map 10 §8.1) | M5 |
| 12 one-row fluids (R1130(E) among them) | `CoolProp-Tests.cpp:4798-4815` rows (map 10 §8.1), re-checked against the papers where obtainable | K2 | R1224YDZ p fails (DIV-0014) | M5 |
| HeavyWater | IAPWS R16-17; IAPWS D2O transport; Herrig 2018 melting | K1/K2, rows | expected | M5, M8 |
| Ammonia, CO₂, R125, Air, Nitrogen, HFE143m | Gao 2023; Span & Wagner 1996; Lemmon & Jacobsen 2005; Lemmon 2000; Span 2000; Akasaka 2012 | K2 | `Expected`: paywalled, the user supplies them (map 13 §10 Q1; PLAN.md section 6) | when obtained |
| n-Heptane, D6 α⁰ | Jaeschke & Schley 1995 closed-form c_p⁰ | equation | no table needed | M4 |
| 130 SA fluids | P-mp check points (Θ = 0.5, 0.3, 0.1), fastchebpure dense files | K3 (mp) | SA vs mp: p median 4.8e-14, max 4.7e-9 (map 10 §8.1) | M5 SA, M6 VLE |
| Transport, 41 fluids | the 318 `CoolProp-Tests.cpp` rows re-labelled: only P/P-IAPWS rows arbitrate; R-other, R-self, unsourced rows dropped (map 13 §8, map 10 R6) | rows | median 7.8e-6 vs oracle | M8 |

`phasekit_verify::arbiters::ARBITERS` (PLAN.md M1.7) holds these rows as records, all `Expected` or `Unpublished`
at M1 until their tables are transcribed; the 12 one-row fluids join at M1.12, the n-Heptane and D6 c_p⁰ equation checks
with M4, the transport rows and releases with M8.2, the mp check points with M1.17. No milestone gate depends on a
paywalled paper: every D15 gate names an open arbiter (IAPWS, Lemmon 2016, Thol 2016,
NIST IR 8474, mp points). Fluids without a usable arbiter stay oracle-verified (provisional) and the nightly report
lists them.

## 5. Tolerance classes

Relative unless marked abs: |a − b| ≤ x · scale, scale = max(|b|, floor). Floors *(decision)*: molar energies (h, u, g,
a) R·T; entropies and heat capacities R; first partials ∂X/∂Y |X|/|Y| at the state; α values Σ_k |φ_k|. For a scaled
derivative A_ij the φ_k are the summands of its expansion by the product and chain rules, every falling factorial
written out, so Σ_k |φ_k| bounds the rounding of any evaluation order and is Σ_k |φ_k| of the terms for A_00
(`phasekit_verify::majorant`; PLAN.md M3.1). The plain Σ_k |φ_k| of the terms cannot bound a derivative: the
τ⁴-derivative of a term with t = 50 carries 50·49·48·47 ≈ 5.5e6, and Water's Power block misses it by up to 1.5e5 times,
while the expansion scale leaves every Power block of the core subset at headroom ≤ 0.03. The floor applies in the units
of each entry and to α itself: when Σ_k |φ_k| of α is below 1e-300 the terms' values, and every entry built from them,
pass through subnormal numbers, so each entry's scale is raised by the same factor 1e-300 / Σ_k |φ_k|
(`majorant::floored`), and an entry is never compared below the floor in its scaled form A_ij (M4.1: far from the
critical point Water's non-analytic terms underflow, e^(−700(τ − 1)²) ≈ 1e-300). Non-separable terms take their scale
from the formula evaluated in majorant arithmetic (`majorant::Bound`: every sum and product taken on absolute values,
every composed function's derivatives at the true value). "nc" is the near-critical window |T/Tc − 1| < 1e-3 and |ρ/ρc −
1| < 0.1, published Tc, ρc (map 10 §8.3).

| Class | Bound | Applies to | Derivation |
|---|---|---|---|
| `Exact` | bitwise (`to_bits`), statuses equal | executors, threads, batch vs scalar; gauge-invariant outputs; data constants (M, R, published Tc); fixture parse (`bits:`); `libm` cross-target hash | same code path, same operations; Rust neither contracts nor reorders (K12, map 10 R16) |
| `Term` | 1e-13 · Σ_k abs(φ_k), floor 1e-300 | α terms and A_ij vs oracle block isolation; `accumulate` vs `Jet4`/num-dual | sum-of-blocks vs total ≤ 2.2e-13 near critical (map 10 §8.2); 1-3 ulp FMA/libm (map 10 R16); 1e-13 is about 450 ulp of the expansion scale above; core-subset blocks vs the oracle (M3.1-M3.5): headroom ≤ 0.04 for every kind but Gaussian, 0.36 there, from contributions near 1e-141 whose exponent (\|u\| ≈ 320) rounds to \|u\|·ulp; α^r totals of 134 fluids 0.02 (M3.6); non-analytic blocks 0.006 (M4.1) |
| `Prop` | 1e-12; nc 1e-8 | properties at (T, ρ), ancillaries, σ, melting vs oracle | cancellation near critical in c_p, w (map 10 §8.3) |
| `SaCoeff` | 1e-14 | Rust Clenshaw vs oracle SA at the same input | same Chebyshev coefficients (map 10 §8.3) |
| `SaFit` | 4 · abs(SA/mp − 1) of that point, floor 1e-14 | SA vs P-mp points | CoolProp's own acceptance rule (map 10 §3) |
| `SatMp` | p, ρ 1e-11; ρ 1e-6 if Θ < 1e-3 | Rust VLE vs P-mp; rescaled SA vs VLE | mp exact for the EOS (map 10 §8.3); the 1e-6 is *inference*, measured at M6, then tightened, never loosened |
| `Flash` | T, ρ, p, h, s, u 1e-9; Q 1e-8 abs; nc: ρ 1e-6, T 1e-8 | round trips; flash vs oracle; numerical critical point | iterative solvers whose stopping rule must be ≤ 1/10 of this bound (map 10 §8.3); nc values *inference*, measured at M7 |
| `TransportDirect` | 1e-12 | direct correlations vs oracle at the same (T, ρ) | closed forms (map 10 §8.3) |
| `TransportEcs` | 1e-8 | ECS vs oracle, SA flag pinned | inner conformal solve; Tc/ρc dependence up to 1.5e-6 if the flag moved (map 10 §4) |
| `Paper` | half a unit in the last printed digit (`from_printed`) | printed rows | what the authors guarantee (map 13 §3) |
| `Measured` | the residual recorded in a register entry, rounded up to one significant digit | literature rows under `KeepOracle`, `Investigate` | measured, not chosen (DIV-0005: 5e-7; oracle 4.8e-7) |
| `Identity` | 1e-12 of the largest term of the identity; nc 1e-8 | L4 identities; saturation equilibrium | the oracle meets them to ≤ 2e-15 (map 02 §8); margin for cancelling terms *(inference)* |
| `Fd` | 1e-7, central differences, relative step 1e-5 | conformance FD checks | truncation ~h² ≈ 1e-10, rounding ~u/h ≈ 2e-11; FD is a dimensional sanity check, AD the precise one *(inference)* |
| `RefAnchor` | 1e-8 abs, SI mass units *(inference: map 01 §8 states 1e-8 without units)* | reference-state anchors: IIR, ASHRAE, NBP and custom states hit their defining h, s | the bound CoolProp's own reference-state tests use (map 01 §8, `CoolProp-Tests.cpp:2263-2436`); the anchors are definitions, so only the anchor flash's rounding remains |
| `Smoke` | finite value or a documented error class | robustness corpus | must-not-crash inputs (map 10 §8.1) |

Rules: classes live in `phasekit_verify::tolerance` and in fixture `tol:` lines; `ToleranceClass` is
`#[non_exhaustive]` and this document adds `SaFit`, `Measured`, `Identity`, `Fd`, `RefAnchor` to the sketch's list.
`tolerance_classes_match_verification_md` (PLAN.md M1.5) keeps the code and this table equal. A class value changes
only in a PR that edits this table with a derivation; making CoolProp pass is never one. Fast policies
(across-terms SIMD, FMA) get their own class when they exist, |Δ| ≤ n·u·Σ|φ_k| (kernel-performance §2.9). A failure
prints fixture, row, column, class, provenance, got, want, error and bound (map 10 U1); every passing fixture reports
its headroom (max error / bound), because map 10 R5 found 7 of 318 transport rows within 10 % of a hand-tuned bound.

## 6. Divergence register

### 6.1 Schema

```rust
/// One register entry (VERIFICATION.md §6.1).
pub struct Divergence {
    /// "DIV-0001"; never reused or renumbered.
    pub id: &'static str,
    /// Canonical names, "A&B" for a mixture, `["*"]` for every fluid.
    pub fluids: &'static [&'static str],
    /// GasConstant | Reducing | Melting | Transport | Algorithm.
    pub part: Part,
    /// "<bibkey> <table>"; required for UsePaper.
    pub arbiter: Option<&'static str>,
    /// UsePaper | SkipOracle | KeepOracle | Investigate.
    pub policy: Policy,
    /// Data (a corrections.csv patch) | Code(module) | None.
    pub fix: Fix,
    /// Map section, measurement (value, date), upstream commit.
    pub evidence: &'static str,
    /// SkipOracle only: oracle cells not asserted on Parity.
    pub exempt: Option<Exempt>,
    /// The Measured bound: literature rows (KeepOracle, Investigate) or oracle columns that stay asserted beside an
    /// exemption.
    pub tolerance: Option<Tolerance>,
    /// Milestones whose PRs add the proof's parts, e.g. `&[5, 6]`.
    pub proof: &'static [u8],
    /// Open | ResolvedUpstream { commit }.
    pub status: DivStatus,
}

/// The oracle cells a `SkipOracle` entry does not assert on Parity; the tests count them.
pub struct Exempt {
    /// Fixture kinds.
    pub kinds: &'static [Kind],
    /// Columns (`props` outputs for the long format); `"*"` for every column.
    pub columns: &'static [&'static str],
    /// Rows.
    pub rows: Rows,
}

/// Which rows of an exempt kind are not asserted against the oracle.
#[non_exhaustive]
pub enum Rows {
    /// Every row.
    All,
    /// Two-phase rows (0 < Q < 1).
    TwoPhase,
    /// Rows outside the model's domain (the `.edge` files).
    BelowDomain,
    /// Rows with lo ≤ T ≤ hi.
    TBand {
        /// Lower temperature (K).
        lo: f64,
        /// Upper temperature (K).
        hi: f64,
    },
}

/// How the divergence is implemented.
#[non_exhaustive]
pub enum Fix {
    /// A `data/corrections.csv` patch on the Corrected dataset; every patch cites its entry.
    Data,
    /// A code path, named by its module (`phasekit_core::state`), that must exist once the proof is due.
    Code(&'static str),
    /// Nothing changes: `KeepOracle` and `Investigate`.
    None,
}
```

The sketch (`register.rs`) had a single `fluid` and none of `fix`, `exempt`, `tolerance` and `proof`; PLAN.md M1.6
added them *(decision)*, and `gates doc-excerpts` checks this block against `register.rs`. `fluids` is a list so that one entry covers one defect in several fluids (the q_D and ECS-LJ
entries, 6.6); `proof` is a list because several entries are proved in parts at different milestones. The register is
a compiled `static DIVERGENCES`, not TOML (no parser dependency; proposal D rejected TOML overrides).

### 6.2 Policies and statuses

| Policy | Arbiter | Data patch may cite it | Oracle cells on Parity | Literature rows on Corrected | Proof |
|---|---|---|---|---|---|
| `UsePaper` | required, `SelfConsistent` | yes for `fix: Data`; `fix: Code` names the code path that selects by `DataSet` | asserted (Parity is v8.0.0 data) | asserted at `Paper` | three-part (6.3) |
| `SkipOracle` | none: an identity, analytic result or typed refusal replaces the oracle | no | `exempt` cells counted, not asserted | — | replacing check passes; oracle still shows the defect |
| `KeepOracle` | the inconsistent table | no (`check_register` refuses) | asserted | asserted at `Measured` | no patch; rows at `Measured`; equal `ModelKey` |
| `Investigate` | none yet | no | asserted | at `Measured` where rows exist | pins the measured disagreement; names its resolution action |

Lifecycle: `Investigate` → `UsePaper` (arbiter transcribed and self-consistent), `KeepOracle` (the user accepts the
oracle value, as for Helium, user decision 5) or `SkipOracle`. `status` is `Open` while the pinned oracle still shows
the divergence, `ResolvedUpstream { commit }` after a pin move under which it agrees (6.5); entries are never deleted.
DIV-0005 Helium: policy `KeepOracle`, status `Open`.

### 6.3 Proofs

`tests/divergences.rs` has one `fn div_NNNN()` per entry and one test that iterates `DIVERGENCES` and dispatches. It
fails if an entry has a `proof` milestone below `phasekit_verify::MILESTONE` and no proof function, or if a proof
function's id is not in the register. `MILESTONE` is the milestone in progress: the step that closes Mn sets it to
n + 1 (PLAN.md §0.2), so every proof part due by Mn must then exist; each `div_NNNN` runs the parts whose milestones
are reached.
- `UsePaper`, three parts (ARCHITECTURE.md §10): (1) `Corrected` matches the arbiter rows within `Paper`; (2) the
  oracle differs from the arbiter beyond `Paper` on at least one row, else the entry is stale and must become
  `ResolvedUpstream { commit }` at the next pin move; (3) `Parity` matches the oracle within each column's class.
- `KeepOracle`: `check_register` refuses any patch citing it; `ModelKey(Corrected) == ModelKey(Parity)`; literature
  rows within `Measured`; Parity matches the oracle.
- `SkipOracle`: the replacing check passes and at least one exempt oracle cell still differs from phasekit (so a fixed
  oracle is noticed). `Investigate`: the recorded oracle values and residual reproduce.

### 6.4 When a new CoolProp bug is found (the TDD path)

1. **Failing test** (L3 comparison, L2 row or L4 identity). Rule out phasekit first: L1 and L4 pass for the fluid; its
   R, M, T_r, ρ_r and per-block Tc match the paper (map 13 §3); the oracle row was generated under the lock.
2. **Arbiter:** a printed table (map 13 §4 inventory), IAPWS release or P-mp points; failing those, an identity or an
   analytic result. mpmath or teqp only localise the fault (section 2).
3. **Self-consistency** of the arbiter (4.3).
4. **Register entry:** next free id; policy from the outcome; `evidence` cites the map item, the measurement (value,
   date, oracle hash) and any upstream commit; `fix`; `exempt` or `tolerance` as the policy needs; the `proof`
   milestones.
5. **Patch.** Data (`fix: Data`): a row in `data/corrections.csv` (DIV id, fluid, field, v8.0.0 value, corrected value,
   citation; ARCHITECTURE.md §8 step 4); datagen checks the v8.0.0 value against the JSON bit for bit, and the hash gate
   then classifies the superancillary (7.3). Algorithm (`fix: Code`): a code fix; Parity cannot express it, so the
   entry is `SkipOracle` with an `exempt` predicate, unless the fix is a documented choice by `DataSet` inside a family
   crate (the M11 cubic-R entry), which stays `UsePaper`.
6. **Proofs** (6.3) and `check_register` pass; ROT-REGISTER.md section 4 gets a row if the defect is systemic.

Never widen a class, hand-edit a fixture, drop failing rows from the generator, or `#[ignore]` without the DIV id.

### 6.5 Moving the oracle pin

Not planned for v0.1. If it happens, one PR edits `oracle.lock`, regenerates every fixture and `MANIFEST.sha256`,
re-runs datagen on the new JSON (dropping corrections whose v8.0.0 value no longer matches) and marks every entry whose
proof part 2 now fails `ResolvedUpstream { commit }` (map 10 §8.5 rule 4). A second oracle built from upstream master
(map 10 Q2) is not part of this plan.

### 6.6 Seed entries (DIV-0001..0014), added entries and planned entries

All seeds are status `Open` (ARCHITECTURE.md §10; sketch `register.rs`). "Proof" lists the milestones of the proof's
parts (the `proof` field).

| ID | Fluids | Part | Policy, fix | Arbiter or replacing check | Evidence | Proof | Patch or exemption |
|---|---|---|---|---|---|---|---|
| DIV-0001 | R1234ze(E) | GasConstant | `UsePaper`, `Data` | Thol et al. IJT 2016 Table 3 (OA; 6 rows, one at ρ = 0) | map 13 §3 item 3, R1: JSON R 8.314472, paper 8.3144621; oracle p +1.0e-6..+1.4e-6, ≤ 1.9e-7 with the paper's R | M5 (three parts); M6 (rescaling) | `Edit::GasConstant(8.3144621)`; SA `Rescaled { p: 8.3144621/8.314472, rho: 1 }` |
| DIV-0002 | Water | Melting | `UsePaper`, `Data` | IAPWS R14-08: ice VI p* 632.4 MPa; T(1356.76 MPa) = 320 K | map 10 R10: JSON p0 623.4 MPa; oracle T 320.965 K, p(320 K) 1337.45 MPa | M8 | `Edit::MeltingP0 { segment: <ice VI>, p0: 632.4e6 }`; SA unaffected |
| DIV-0003 | Nitrogen | Reducing | `UsePaper`, `Data` | Span et al. JPCRD 2000: ρ_c 11183.9 mol/m³ | map 12 §6.3: oracle ρ_r 11183.901464580624; upstream 2acbbc82; saturated ρ shift 1.3e-7 (map 10 §8.4) | M2 (constant); M6 (rescaled SA) | `Edit::ReducingDensity(11183.9)`; SA `Rescaled` |
| DIV-0004 | * | Transport | `SkipOracle`, `Code` | `Undefined { prop, TwoPhase }` | map 10 R18, map 12 §6.3 (Water 500 K, Q 0.5: η 1.6048e-5); upstream da48a3ee throws. Measured 2026-10-05, Water QT(0.5, 400 K): η 1.2979e-5, λ 0.02849, c_p 4056.47, c_v 2913.73 J/kg/K, w refused. ARCHITECTURE.md §13 ties two-phase c_p, c_v here | M5 (c_p, c_v: DT states in the dome); M8 (η, λ) | exempt: `props` rows with 0 < Q < 1, c_p, c_v, η, λ (both bases) |
| DIV-0005 | Helium | GasConstant | `KeepOracle`, `None` | NIST IR 8474 Table 3 (6 states: p, c_v, w) at `Measured` 5e-7; Table 4 (K3) at `Paper`, or at `Measured` if the 4.3 procedure finds it inconsistent too | map 13 §3 item 4, R3: Table 3 fails with its Table 1 R (8.314472); oracle (R 8.3144598) within 4.8e-7; user decision 5 | M5 (Table 3); M6 (Table 4) | no patch may cite it; Corrected = Parity |
| DIV-0006 | Ethylene | Reducing (+M) | `Investigate`, `None` | to transcribe: Smukala et al. JPCRD 2000 (paywalled, map 13 §4) | map 12 §6.3, map 10 §8.4: ρ_r and M corrected upstream 2acbbc82; saturated ρ shift 2.1e-5 | M2 (pins v8.0.0 ρ_r, M) | → `UsePaper` with `ReducingDensity` + `MolarMass` edits |
| DIV-0007 | OrthoHydrogen | Reducing (+M) | `Investigate`, `None` | to transcribe (map 13 §4) | as DIV-0006; shift 3.0e-5 | M2 | as DIV-0006 |
| DIV-0008 | n-Undecane | Reducing | `Investigate`, `None` | to transcribe (map 13 `?` class) | as DIV-0006; shift 1.1e-5 | M2 | → `ReducingDensity` edit |
| DIV-0009 | R1233zd(E) | Transport | `Investigate`, `None` | none yet; restored and refit upstream 14da1f0d (v7.2.0 had rhosr-CS) | map 12 §6.3: v8.0.0 raises "Viscosity model is not available" | M8: `NoModel { Viscosity }`, as the oracle | add the correlation with its paper's check values |
| DIV-0010 | * (cubic PR, SRK) | Algorithm | `SkipOracle`, `Code` | identity T(∂s/∂T)_p = c_p | map 12 §6.3, map 10 §8.4: PR propane 400 K, 1 bar: 91.35 vs c_p 93.89 J/mol/K; upstream 9b96b64b | M11 | no cubic s or s-input flash fixtures from 8.0.0 |
| DIV-0011 | * | Algorithm | `SkipOracle`, `Code` | exact Taylor coefficients (`zero_density`), hand derivations | map 12 §6.3, R8: C off −6.7e-5 / −7.1e-5 / +1.9e-5 (Propane 300 K, N2 300 K, Water 600 K); B agrees to ~1e-12, the truncation of the oracle's δ = 1e-12 evaluation | M5: the oracle's C still differs beyond `Prop`; B and dB/dT asserted at `Measured` 1e-10 (the entry's `tolerance`: about 100× the oracle's own truncation *(inference)*, since `Prop` 1e-12 would leave no margin) | exempt: `eos` `Cvirial`, `dCvirial_dT`, all rows |
| DIV-0012 | Water | Algorithm | `SkipOracle`, `Code` | `DomainError` under `Enforce`; flagged state under `Extrapolate` (user decision 6) | map 12 §6.3 (#3394): oracle accepts DT(55018.5 mol/m³, 250 K), Tmin 273.16 K; re-measured 2026-10-05 p = −5.9277123935677105 Pa | M5 | exempt: `.edge` rows of `eos`, `flash` with T < 273.16 K |
| DIV-0013 | R1234yf&R1234ze(E) | Reducing | `Investigate`, `None` | Bell JPCRD 2022 Table XI | map 10 §8.4: α^r −0.464679 vs −0.460595 (+0.89 %); τ 0.80266 vs ~0.80; sibling R1234yf&R134a matches to 6e-9 | M13 | — |
| DIV-0014 | R1224YDZ | Algorithm | `Investigate`, `None` | Akasaka & Lemmon IJT 2023 Table 7, to re-check | map 10 §8.4: p(400 K, 8000 mol/m³) 21.1790735 MPa vs printed "21.17909" (7.8e-7, 3.3× half a digit); c_v, c_p, w match | M5: p asserted at `Measured` 8e-7, oracle value pinned | decide: CoolProp test transcription vs EOS difference |
| DIV-0015 | R123 | Algorithm (α⁰ block `Tc`) | `Investigate`, `None` | the EOS paper's c_p⁰ (Younglove & McLinden JPCRD 1994, paywalled: PLAN.md §6 P4) | map 02 §6, map 13 A4: the c_p⁰ blocks use Tc 456.82 K against T_r 456.831 K; c_p⁰ −1.33e-5 at 300 K (−1.46e-5 at 200 K, −1.06e-5 at 500 K; M4.3) | M4: Parity matches the oracle's α⁰ rows; the shift reproduces; Corrected = Parity | — |

**Planned entries.** Ids are allocated in the PLAN.md step that adds each one (the next free number then); the PR
adds the row above with its id.

| PLAN.md step | Fluids | Part | Policy, fix | Arbiter or replacing check | Evidence | Exemption or tolerance |
|---|---|---|---|---|---|---|
| M8.8 | R13, R14, R142b, R218, RC318 | Transport (`q_D`) | `SkipOracle`, `Code` (q_D is read) | the record with `q_D` edited to CoolProp's default 2e9 reproduces the oracle's λ at `TransportEcs` on every row; at least one exempt cell still differs | map 05 R2: `FluidLibrary.h:668-680` never reads q_D; honouring it moves λ by −16 % to +24 % near T_c | exempt: `transport` `lambda`, all rows of these fluids; → `UsePaper` once Huber et al. IECR 2003 is transcribed |
| M8.9 | Ammonia | Transport (Tufeu 1984 critical term) | `SkipOracle`, `Code` | λ finite and continuous across 405.4 K | map 05 R3: oracle λ(405.3999 K, 50 kg/m³) = 6.26 W/m/K (~100× too high), NaN at 405.4 K | exempt: `transport` `lambda`, `Rows::TBand` around 405.4 K (width from the measured divergence *(inference)*); Corrected model (Monogenidou 2018 or a bounded term) after the paper check (map 05 Q3) |
| M8.9 | R1234yf | Transport (`rhosr_critical`) | `Investigate`, `None` | `rhosr_critical` derived from the current EOS: −54095.50 vs stored −54468.48 | map 05 R4, U9 (+0.8-1.6 % liquid η if derived) | oracle cells asserted; Parity = Corrected = stored value |
| M8.9 | R124, R22, R245fa, R32 | Transport (ECS λ LJ parameters) | `Investigate`, `None` | Huber 2003, McLinden 2000 | map 05 R17: Chung LJ used instead of the ECS model's own (e.g. R245fa σ 0.5529 vs 0.5151 nm) | oracle cells asserted; the record names `LjSource::Chung` |
| M8.11 | Hydrogen, OrthoHydrogen | Melting | `Investigate`, `None` | Datchi et al. PRB 2000 | map 02 §6: p_melt(13.957 K) = 23.6 MPa vs p_triple 7.36 kPa | oracle cells asserted; `melting_meets_the_triple_pressure` waiver cites the entry |
| M11 | every cubic fluid borrowing a HEOS ideal gas | GasConstant | `UsePaper` (user decision 10), `Code` (`phasekit-cubic` selects R by `DataSet` when it builds the model; no `corrections.csv` row) | the borrowed model's ideal-gas source (its EOS paper's c_p⁰; IAPWS-95 for Water) | map 06 C6, map 15 §3.1, X11 | proof (1) Corrected cubic α⁰ `Exact` to the borrowed model's at the same (T, ρ), c_p⁰ within `Term` of it; (2) the oracle's cubic c_p⁰ differs by the R ratio: 1.128e-6 (8.314472/8.31446262 − 1) for 54 of 97 fluids (map 06 C6), 1.10e-5 for Water (map 15 §3.1); (3) Parity cubic matches the oracle except the DIV-0010 columns. PLAN.md §4 has the full row |

## 7. Datasets: Parity and Corrected

### 7.1 Definitions and use

`DataSet::Parity` is the v8.0.0 JSON bit for bit. `DataSet::Corrected`, the default (user decision 2), is Parity plus
each blob's corrections section; every patch cites a `UsePaper` entry and `FluidRecord::applied` lists the ids applied.
L3 always uses `Registry::from_embedded(DataSet::Parity)`; L2 rows, L4, L6, the conformance kit and the benches use
the default (Corrected); DIV proofs use both.

### 7.2 Gates

- `check_register(DIVERGENCES, &patches)` over the patches of every embedded record (decoded through
  `phasekit_core::internal`) refuses duplicate ids, unknown ids, and patches citing an entry that is not `UsePaper` or
  has no arbiter (sketch). Added here: every id cited anywhere (`corrections.csv`, `#[ignore]` reasons) exists; every
  `UsePaper` entry with `fix: Data` is cited by at least one patch, and no patch cites an entry whose `fix` is not
  `Data`; a `fix: Code(path)` names a module that exists (the M11 cubic-R entry is `UsePaper` with `fix: Code`).
- **Parity diff = patches** (M2): for all 136 fluids the field-by-field difference of the Parity and Corrected records
  is exactly the shipped edits, and `ModelKey` is equal if and only if no patch applies.
- Datagen fails unless each `corrections.csv` row's "v8.0.0 value" equals the JSON value bitwise.

### 7.3 Superancillary hash gate

`FluidRecord::superancillary_freshness()` compares the corrected EOS with the stamp its SA was fitted to; the hash
covers `EosRecord::encode`, the canonical encoder datagen also uses (E14).

| Result | When | SatAccuracy | Test |
|---|---|---|---|
| `Fresh` | same shape, same R, ρ_r | `Exact` | all 130 SA fluids under Parity |
| `Rescaled { p, rho }` | same shape; R and/or ρ_r corrected | `Exact`; ρ′, ρ″ × rho, p_sat × p | DIV-0001: p = 8.3144621/8.314472, rho = 1; DIV-0003: p = rho = 11183.9/11183.901464580624 |
| `Stale` | any other EOS change | `Guess` + VLE polish | a synthetic edit of one power-term coefficient |

Also: `every_eos_field_is_hashed` flips each field once and the hash moves (the shape hash only for shape fields);
datagen recomputes all 130 CoolProp `source_eos_hash` stamps (FNV-1a, self-test `8e75626511d00b5c`; map 09 §8);
`MolarMass` and `MeltingP0` edits leave the SA `Fresh` (the SA is molar; melting is outside the EOS section).

### 7.4 Exact rescaling (M6)

Exact in exact arithmetic (saturation is invariant in (τ, δ), p = ρRT(1 + δα^r_δ); *inference* in ARCHITECTURE.md §8).
M6 proves it in floating point for the DIV-0001 and DIV-0003 fluids: the rescaled SA vs phasekit's VLE on Corrected at
the 3.5 Θ grid, and the rescaled P-mp points (same factors) vs that VLE, both `SatMp`. A stale SA (synthetic edit) must
report `Guess`, and every QT and PQ answer must equal the VLE-polished result.

## 8. Identity, consistency and property tests

### 8.1 Identities (L4, class `Identity`)

On every core fluid at eos-grid states (stable and imposed phase), on the out-of-tree vdW family and the Gibbs solid:
- g = h − Ts, a = u − Ts, h = u + p/ρ, Z = p/(ρRT); mass and molar values differ by exactly M (`Exact`) (map 01 §8).
- (∂h/∂T)_p = c_p, (∂s/∂T)_p = c_p/T, (∂g/∂T)_p = −s, c_p − c_v = T(∂p/∂T)²_ρ/(ρ²(∂p/∂ρ)_T), w² = (∂p/∂ρ)_s, Maxwell
  relations (map 01 §8, map 02 §8, ARCHITECTURE.md §3.7). μ = g (pure); ln φ = α^r + δα^r_δ − ln(1 + δα^r_δ) once
  fugacity lands (map 02 §8).
- Virials: `zero_density` B, C, dB/dT, dC/dT against hand derivations (sketch tests) and B = lim (Z − 1)/ρ at low
  density (map 02 §8); never against the oracle's C (DIV-0011).
- M7: second partials and the fundamental derivative by the Thorade & Saadat identities (ARCHITECTURE.md §3.7) and
  closed form vs the generic Jacobian path.
- Saturation equilibrium at every SA or VLE point: p(T, ρ′) = p(T, ρ″), g′ = g″ from the EOS (the oracle shows Δg/RT
  1e-15 to 1e-13, map 03 §8).
- Critical point (E17): at (Tc, ρc) of Water and CO₂, evaluated at the point itself with no nudge (ROT-065): α^r's
  α, first derivatives, A11 and A02 finite and the limits of their neighbours, A20 = −∞, orders 3-4 NaN (no limit);
  p, h, s finite; c_v, c_p, w return `Undefined { prop, CriticalPoint }`. On the critical isochore ρ = ρc every entry
  but A04 (|δ − 1|^(1/β) has no fourth derivative at δ = 1) is finite (`tests/terms.rs`, M4.6).

### 8.2 Round trips (L4, class `Flash`)

Truth states come from (p, T) for single phase and (T, Q) for two phase, never density bands (map 12 §6.4). For each
truth state and each of the 19 pairs: read (x1, x2) off the state, flash, compare T, ρ, p and Q with the truth.
Acceptance (ARCHITECTURE.md D6): inputs reproduced, inside the domain, dp/dρ > 0 and c_v > 0 unless a phase is imposed;
`SolvePath` recorded. On known multi-root inputs (T + X in the liquid; map 03 §8) `RootPolicy::Strict` returns
`Ambiguous { roots }`, each root a valid state. The robustness corpus (about 100 issue-linked states, map 10 §8.1,
map 12 §8) lives in `tests/robustness.rs` as literals with their issue numbers, class `Smoke`. Grids: committed 6×6
(log p, T) + 4×4 (T, Q) per core fluid per pair; nightly 40×40 + 20×20 for all fluids (map 10 §2.3). CoolProp's
devdocs consistency report (12,355 failures over 82 fluids at 7.2.1dev, map 12 §6.2) is the baseline to beat; the
nightly report prints phasekit's count.

### 8.3 Finite differences (class `Fd`)

`fd_first_order(model, t, rho, 1e-5, ..)` checks A10 and A01 against central differences in dimensional T and ρ, so a
family's reducing choice cannot hide an error. First partials of the 12 `DerivVar`s are checked against FD of the state
functions along the constrained path, porting CoolProp's 14-of-19-variable check (map 01 §8,
`AbstractState.cpp:1335-1460`) with the tighter class.

### 8.4 Gauge invariance (map 15 §8)

For random states and any two gauges: p, ρ, T, c_p, c_v, w and every partial not involving h, s, u, g, a are `Exact`;
h, s, u, g, a shift by exactly Δh, Δs, Δh, Δh − TΔs, Δh − TΔs (`Identity`); an h- or s-input flash under gauge G
returns the same (T, ρ) bits as the native flash at h − Δh (`Exact`). M10: IIR, ASHRAE and NBP anchors for n-Propane,
R134a and R124 at 1e-8 absolute (map 01 §8). M13: x = [1, 0] equals the pure fluid under any gauge (map 15 §8).

### 8.5 Capability matrix (19 × 2)

`capability_matrix(fluid, truth)` returns `Pass`, `Fail(Mismatch)`, `Unsupported` or `Refused(Error)` for each
`Pair::ALL` entry × {single-phase truth, two-phase truth}. Declared pairs must pass at `Flash`; undeclared pairs return
`Unsupported { pair }`; combinations that cannot define a two-phase state (PT inside the dome) return the documented
error; nothing panics. The declared set grows DT (M5) → + QT, PQ (M6) → all 19 (M7) (D6); the test holds the expected
matrix per milestone as a table.

### 8.6 Property-based and structural tests

- proptest (std only, dependencies R18), fixed CI seed, committed `proptest-regressions/`: `Input::new` over NaN, ±∞,
  ±0, subnormals, negatives, Q ∉ [0, 1] → `InvalidInput`, never a panic (map 12 R9); `BatchRequest` shapes (zero
  points, zero outputs, n·m overflow, mismatched lengths) → early return or `Shape` (E12). Since M1.14 these two are
  `crates/phasekit-core/tests/properties.rs`, 4096 cases each from `RngSeed::Fixed` in the test's config (every machine
  runs the same cases), failures replayed from `crates/phasekit-core/proptest-regressions/properties.txt`; n·m overflow
  is reachable only where `usize` is 32 bits (wasip2), elsewhere such buffers cannot exist. Registry names (case folding,
  non-ASCII, unknown) → typed errors; truncated or bit-flipped blobs → `LoadError` (checksum). Rust-side grids use
  SplitMix64 (`phasekit_verify::sample`), reproducible on every target, wasip2 included (dependencies §2.12).
- Lazy loading with a counting `DataSource` (ARCHITECTURE.md §6, E5, E6): building the index decodes nothing; a thermo
  path never reads an ECS reference; the first viscosity call reads it once, also under 16 threads; failures are
  cached, misses are not.
- allocation-counter's counting global allocator (user decision AC1) asserts 0 heap allocations per flash and per batch
  point after warm-up (M5). At compile time: Send + Sync, `size_of::<State>() <= 256`, `size_of::<Error>() <= 48`
  (ARCHITECTURE.md §3.8); PLAN.md M9.7 records the actual `State` size (208 B in the sketch) and asserts only the bound.
- Tests that cannot fail (ROT-294; user decisions TQ1, TQ2, which replace the earlier deferral of mutation testing):
  `gates assertions` requires every `#[test]` to assert (or be `#[should_panic]`) and rejects an `assert_eq!` or
  `assert_ne!` with identical sides; `clippy_bans_fire` keeps clippy's tautology lints firing; `gates mutants` runs
  `cargo mutants --in-diff` on every PR, and a mutant of changed code that no test catches fails it unless
  `.cargo/mutants.toml` excludes it with a reason. Principle 5 (round trips assert values against a class) still
  answers map 10 R3 for what a test asserts.

## 9. Differential tests for execution strategies

### 9.1 Batch vs scalar and threads (L6, class `Exact`)

- `policy_equivalence(fluid, req, candidate)`: every cell and `Status` bitwise equal to `ExecPolicy::Reference` (sketch
  `conformance.rs`). Batch vs scalar: `batch::evaluate` vs a loop of `Fluid::flash` + getters on the same inputs.
- `Parallel { chunk }` for chunk ∈ {1, 16, 1024, N} on rayon pools of 1, 2, 4 and `available_parallelism` threads,
  N mod chunk ≠ 0 included (kernel-performance §3.3); without `rayon`, `Parallel` runs sequentially with the same bits.
- Concurrency stress: N threads × M fluids, each result bitwise equal to the serial run (map 11 §8); 16 threads racing
  the first `get` of one fluid decode it once (counting source).
- Warm starts (`with_guess`) are another path: `Flash`, never `Exact` (K7). Thread tests are
  `#[cfg(not(target_family = "wasm"))]` (D17).

### 9.2 SIMD lanes (M12, only if the gate fires)

Lanes across states vs scalar: bitwise, remainder lanes padded by duplicating a valid lane (K9, K17, kernel-performance
§4). CI forces every dispatch level (scalar, AVX2, AVX-512 where present, NEON, the `simd128` build). Accelerated
kernels must also pass the K1/K2 paper rows (map 13 §7). Across-terms or FMA variants exist only as a separate `Fast`
policy with its own class (section 5).

### 9.3 Same model, two derivative mechanisms (L1)

`accumulate::<f64>`, `Jet4` and num-dual (on the paper formula) agree within `Term` at about 300 (τ, δ) per block,
including δ → 0 (1e-8, 1e-12) and τ = δ = 1 neighbourhoods. The generic fast path on `f64` is bitwise equal to `Jet4`'s
value part (`generic_fast_path_is_bitwise_scalar`, sketch).

### 9.4 Cross-target determinism hash

This is the one definition of the hash grid; PLAN.md M9.4 runs it and M9.5 applies the decision rule.
- **Grid and hash:** all 136 embedded fluids (the hash needs no fixtures, so it is not limited to the core subset);
  per fluid 64 (T, ρ) points on the `eos` grid of 3.5, 16 QT and 16 PQ points on the `sat` grid, and 16 PT and 16 PH
  points read off (p, T) truth states, all drawn with SplitMix64 seed 1; outputs p, h, s, c_v, c_p, w, the order-2 α^r
  bundle and the status. FNV-1a 64 over the fluid name bytes, then in grid order each output's
  `to_bits().to_le_bytes()` (NaN canonical) and the status byte. `tests/cross_target.rs` prints `hash=<hex>
  target=<triple> math=<std|libm>`.
- **When:** from M9.4, on x86_64 Linux, Windows MSVC, wasip2 under wasmtime and aarch64, with std math and with
  `--features libm`. std hashes are reported, never asserted (std transcendentals are documented as
  non-deterministic, kernel-performance §4). If the four `libm` hashes agree, that hash is committed as
  `fixtures/hash/libm.txt` and every target must reproduce it from then on. `libm` changes bits relative to std
  (dependencies §4), the one exception to "features never change numerics" (K13), so oracle comparisons stay
  class-based under either math.
- **Decision rule** (user decision 12): PLAN.md M9.5 is the one statement of it: `libm` becomes the default only if its
  hash is identical on all four targets **and** the geometric mean of the libm/std median-time ratios over the six
  scalar gate benches (α^r order 2, properties at (T, ρ), QT, PQ, PT, PH; 12) is ≤ 1.05 with no bench above 1.10.
  Otherwise it stays opt-in and nothing else changes.

## 10. Conformance kit for model families

Any family crate, in tree or out, dev-depends on `phasekit-verify` and runs the kit unchanged; it is identity-based,
so it needs no oracle (sketch `conformance.rs`).

| Check | Function | Asserts | Class | From |
|---|---|---|---|---|
| FD | `fd_first_order` | A10, A01 vs FD in dimensional T, ρ | `Fd` | M1 (in sketch) |
| Gauge | `gauge_invariance` | 8.4 | `Exact` / `Identity` | M1 (in sketch) |
| Executors | `policy_equivalence` | 9.1 | `Exact` | M1 (in sketch) |
| Identities | `identities(fluid, states)` | the 8.1 list | `Identity` | M5 |
| Part additivity | `part_additivity` | `ThermoModel::derivs`: total = ideal + residual | `Identity` | M5 |
| Capability | `capability_matrix` | 8.5 | `Flash` | M7 |
| Threads | `thread_invariance` | 1..N threads and racing first use, bitwise | `Exact` | M9 |

**Proof that the seams suffice** (from M5): `tests/new_family.rs`, an out-of-tree van der Waals family (registry, flash
including a subcritical point, derivative outputs, exact virials, gauge, batch, compat strings, 16 threads), and
`tests/gibbs_seam.rs`, a Gibbs solid (PT solid, QT ice + vapour through `from_split`, registry, reference state, batch,
compat, partials).

**Zero-line core diff gates (M11 cubic; M14 IAPWS ice + IF97).** The milestone starts by tagging `m11-start` (or
`m14-start`); `cargo xtask gates core-frozen --since m11-start` fails if any file under `crates/phasekit-core/` differs
from the tag (zero lines, `git diff --quiet`). A failure means the seam was insufficient: stop, record why in the
decision log, land the seam change as its own core PR, re-tag and restart the family series (Extensible graft, D3).

Family arbiters are papers. CoolProp 8.0.0 cubic and PC-SAFT values are not arbiters (map 10 U14: no provenance;
DIV-0010); teqp and Clapeyron are `OtherImplementation`, differential only (GERG vectors from teqp at M13, D4).

## 11. Gates and CI

### 11.1 The cargo gates

G1-G8 are defined once, in PLAN.md §2.4: G1 fmt, G2 clippy, G3 test, G4 wasip2 test (`--exclude phasekit-xtask`), G5
wasm32 check, G6 MSVC check, G7 clippy without default features, G8 `cargo xtask gates all`. ARCHITECTURE.md D15's
"7 cargo gates" are G1-G7; its zero-dependency guard is G8's `deps`. Locally `CARGO_TARGET_DIR` is set outside the
repository; 11.3 says where each gate runs in CI.

### 11.2 `cargo xtask gates`

Each gate is a subcommand; `gates all` runs those in force on Linux CI and the dev box (xtask needs
`reference/CoolProp` and spawns processes, so it never runs on wasip2, Windows or aarch64). This table is the one
definition of the subcommands; PLAN.md steps add them in the order of the last column.

| Gate | Rule | From (PLAN.md) |
|---|---|---|
| `deps` | zero-dependency guard: `cargo tree -p phasekit-core -e normal,build --depth 1 --prefix none` prints only `phasekit-core` and `phasekit-data`, and the same for `phasekit-data` (itself only); from M9 `--features rayon,libm` adds exactly those two (dependencies §3.4, R1) | M0.4 |
| `lints` | the workspace sets `unsafe_code = "forbid"` and every workspace member has `[lints] workspace = true`, except `phasekit-capi`, which may instead declare `unsafe_code = "deny"` in its own `[lints.rust]` (per-item `allow` with a SAFETY comment; BRIEF "memory safe"; D17) | M0.4 |
| `counts` | executed-test count: sums `test result: ok. N passed` per test binary (doctests included) per target (the host's G3 run and, from M0.5, the wasm32-wasip2 G4 run) against `ci/test-counts.txt` (`<target> <package>::<binary> <min>`; `<binary>` is `lib`, `main`, `doc` or the integration-test name); fails if nothing ran, a listed binary is missing, runs 0 tests or runs fewer than its minimum, or a binary that runs tests is not listed (map 10 R1). The PR adding tests raises minimums with `gates counts --update`, which never lowers one; lowering one by hand needs a `# lowered: <reason>` comment; `# pending M5.9: <test>` lines track the four held-back seed tests and must be gone after M5.9 | M0.4 |
| `ignores` | every `#[ignore = "..."]` reason starts with `DIV-NNNN: `, `issue #N: ` or `nightly: `; DIV ids exist and are not `ResolvedUpstream` (checked against the register from M1.6); `nightly:` only in `tests/sweeps.rs`; a bare `#[ignore]` fails. Each run publishes the ignored list (map 10 R14) | M0.4 |
| `doc-excerpts` | every fenced `rust` block in `docs/*.md` is split at blank lines, and each chunk appears verbatim in one source file under `crates/` or `docs/design/sketch/`, its lines in order (modulo indentation and `///`/`//!` lines; an excerpt may leave lines out), unless the block is preceded by `<!-- excerpt: illustrative -->` (ROT-139; map 01 R19) | M0.4 |
| `rot` | every ROT-REGISTER.md row whose milestone is below `phasekit_verify::MILESTONE` has no GAP status and no Proof still marked "new" that is due (PLAN.md §2.1 defines ticking) | M0.4 |
| `assertions` | every `#[test]` in `crates/` (lint probes aside) contains `assert!`, `assert_eq!` or `assert_ne!`, or is `#[should_panic]`; no `assert_eq!`/`assert_ne!` has identical sides; comments and string literals are not code (ROT-294; map 10 R3, map 07 I11) | M0.4a |
| `mutants` | `cargo mutants --in-diff` over the Rust changes since the merge base with `origin/main` (untracked files included): no mutant missed unless `.cargo/mutants.toml` excludes it with a reason; a timeout counts as caught and is listed (ROT-294; user decisions TQ1, TQ2) | M0.4a |
| `fixtures` | committed fixtures hash to their `MANIFEST.sha256` lines and the manifest lists nothing else; every committed fixture is read by a `fixture!` in a test besides `tests/manifest.rs` (which lists them all for `committed_fixtures_match_manifest`) and every `fixture!` path exists (map 10 R15); committed total ≤ 16 MiB (3.6) | M1.4 |
| `register` | `check_register` + the 7.2 cross-id checks (also a test) | M1.6 |
| `datagen` | `cargo xtask datagen` regenerates blobs, index and features; `git diff --exit-code` on the outputs | M2.5 |
| `features` | `cargo tree -e features`: `phasekit-compat --no-default-features --features fluids-core` enables `phasekit-data/core` only (E7, M5.9); from M10.8 also `-p phasekit-wasm --target wasm32-unknown-unknown`, and the `.wasm` size within `crates/phasekit-wasm/size-budget.txt` | M5.9, M10.8 |
| `perf` | gungraun instruction counts of the tracked benches within 5 % of the baseline committed at M9.6 (Linux) | M9.6 |
| `abi` | the cbindgen header compiles as C99 with `-pedantic-errors` (GCC/Clang) and with `/W4 /WX` (MSVC, C11 mode); exported symbols: the `pk_*` library exports no CoolProp name, the `coolproplib-shim` build exactly the Tier A names (map 11 §8, D11) | M10.4 |
| `core-frozen` | section 10 | M11, M14 |

Also per PR: `cargo deny check` (bans incl. `bincode`, `serde_cbor`, and `once_cell` and `lazy_static` as direct
dependencies; licence allow-list; dependencies §3.4), `reuse lint`, `cargo shear`, MSRV `cargo +1.85 check --lib` for
core, data and compat (D17). Before a release: `cargo semver-checks` and `cargo about generate` (D14).

### 11.3 CI matrix

| Job | Runner | Runs | Notes |
|---|---|---|---|
| fmt, clippy, linux, msrv | x86_64 Linux | `fmt`: G1. `clippy`: G2, G7. `linux`: G3, G8 (`gates all`), and from M0.7 deny, reuse, shear. `msrv`: `cargo +1.85.0 check --lib -p phasekit-core -p phasekit-data` (compat from M5.9) | `linux` fetches `origin/main`, the wasip2 target, wasmtime and cargo-mutants for `gates counts` and `gates mutants`; caches `reference/CoolProp` keyed by its pinned commit and runs `scripts/fetch-coolprop.sh` on a miss (anonymous clone); gungraun from M9 |
| windows | Windows MSVC | G3 with `--exclude phasekit-xtask` (so G6 is a real build) | MSVC CRT transcendentals differ in ulps; the classes absorb it |
| wasip2 | x86_64 Linux + wasmtime | G4: `cargo test --workspace --exclude phasekit-xtask --target wasm32-wasip2` under the `wasmtime` runner set in `.cargo/config.toml` (M0.5) | fixtures via `include_str!`, no filesystem; thread tests compiled out |
| wasm-browser | x86_64 Linux | G5; `cargo build -p phasekit-core --no-default-features --features fluids-core --target wasm32-unknown-unknown`, baseline and `RUSTFLAGS="-C target-feature=+simd128"`; from M10 also `phasekit-wasm` and the JS smoke test (section 13) | K15 |
| aarch64 | aarch64 Linux | G3 with `--exclude phasekit-xtask` | D17 |
| hash-compare | the four targets above | from M9.4: `tests/cross_target.rs` with std and with `--features libm`; collects and compares the hashes (9.4) | |
| weekly | Linux | `cargo update` then G3; a full `cargo mutants` run, published as a report (surviving mutants of code no PR has touched since) | dependencies §3.4 item 4; user decision TQ2 |
| nightly | pinned `runner_image` | until M1.16 only the L5 step, as `-- --include-ignored`; then `cargo xtask oracle --check` twice over the committed files (drift, determinism; 3.6), then `--tier full` into `fixtures-full/`, from M6 `cargo xtask fetch-fastchebpure --all`, then `cargo test -p phasekit-verify --release -- --ignored` (L5), then the report | fail closed |

The workflows live in `.github/workflows/{ci,weekly,nightly}.yml` (M0.6); every `ci.yml` job is a required check on
`main`. They pin GitHub's own actions by commit, install the toolchain through `.github/actions/rust` (which holds the
one stable version) and build pinned tools from crates.io into a cache.

Nightly report, per fluid × kind: rows, cells asserted, cells exempt per DIV id, `nan` cells, oracle-gap rows (11.4),
headroom; plus the ignored tests and each fluid's arbiter status (oracle-only fluids stay visible).

### 11.4 Outcomes of an oracle comparison

| Oracle | phasekit | Result |
|---|---|---|
| value | value | compared at the column's class, unless a `SkipOracle` entry exempts the cell |
| value | `Err` | failure, unless exempt (DIV-0004 two-phase c_p; DIV-0012 below the domain) |
| `err:<class>` | `Err` | pass; the class pair is reported |
| `err:<class>` | value | `oracle-gap`: counted; the state must pass the 8.2 acceptance check (CoolProp's ECS solver fails on 7.3 % of states, map 10 §8.4) |
| `nan` cell | anything | not asserted; counted |

## 12. Benchmarks and performance gates

This section is the one definition of the bench harness, the C++ baseline and the result files; PLAN.md §5.1 lists
the steps that record each row.
- **Where:** `crates/phasekit-verify/benches/` (`harness = false`); `phasekit-verify` stays unpublished, which is the
  "unpublished workspace member" dependencies §2.12 asks for. criterion 0.7 for wall clock on every OS (a non-wasm
  dev-dependency; 0.8 adds `alloca`, a C build script that breaks the local G6 cross-check); gungraun for instruction counts on Linux only (it cannot run on Windows): a
  `cfg(target_os = "linux")` dev-dependency whose bench file compiles to an empty `main` elsewhere, so G6
  `--all-targets` still builds *(inference)*; allocation-counter's counting global allocator for allocations (a T3
  dev-dependency, user decision AC1: implementing `GlobalAlloc` would need `unsafe`, which the workspace forbids;
  dependencies §2.12, R18). `cargo test` never builds the benches.
- **Bench fluids:** Water (56 terms), Methane (40), R134a (21), n-Propane (18) and n-Heptane (12 power terms, chosen at
  M1.15 from the JSON term counts: one of 28 twelve-term fluids, with a superancillary and in the core subset;
  kernel-performance §3.3), on SplitMix64 grids shared with the C++ baseline.
- **Workloads (7):** (1) the order-2 α^r bundle at (T, ρ); (2) properties at (T, ρ): update `DmolarT` + h + c_p; (3) QT
  and (4) PQ through the superancillary; (5) PT and (6) PH single phase; (7) a by-name `PropsSI` call (name lookup).
- **C++ baseline (M1.15):** CoolProp v8.0.0 built Release, no `-march`, like the wheels (kernel-performance §2.9), from
  a throwaway clone of `reference/CoolProp` in a scratch directory, because CoolProp's configure writes generated
  headers and data into its own source tree (`CMakeLists.txt:557-559`, `dev/generate_headers.py`). The harness
  `scripts/baseline/{build.sh,CMakeLists.txt,coolprop_baseline.cpp}` links the static library target and runs the 7
  workloads × 5 fluids, writing `crates/phasekit-verify/benches/baseline/coolprop-8.0.0-<machine>.csv` (`cargo xtask
  baseline` builds and runs it in `$SCRATCH`; `--check` checks every committed file). The file's header names the pin,
  CPU, OS, governor, compiler and date; each row is `workload,fluid,states,median_ns,min_ns,max_ns,grid`, the median,
  min and max over 7 passes of the mean ns per state, and `grid` the SplitMix64 bounds (seed 1) it drew from. Grids need
  no rejection: (T, ρ) states lie in [1.05·T_c, min(1.5·T_c, T_max)] × [0.1, 2]·ρ_c (PT, PH and by-name use their p and
  h); saturation in [T_t + 0.05 (T_c − T_t), 0.98 T_c] with Q in [0, 1]. The α^r row sets the state with
  `update_DmolarT_direct` and reads the 6 order-2 derivatives (CoolProp evaluates its whole derivative cache). The xtask
  test `reference_checkout_is_untouched` checks that `git -C reference/CoolProp status --porcelain --ignored` lists
  nothing beyond a recorded baseline (`scripts/baseline/reference-status.txt`; a fresh checkout lists less).
  Python-level oracle timings are not comparable (kernel-performance §3.3).
- **C++ memory and thread scaling (M1.15a, user decision BG1):** beside the timing file the harness writes
  `<…>.memory.csv` (`measure,fluid,heap_bytes,rss_bytes`; heap = glibc `mallinfo2` arena + mmapped bytes, rss =
  `/proc/self/statm`): the library's first use, the mean bytes per `AbstractState` over 100 states of each bench fluid
  after construction and after a QT update at 0.7 T_c, and one state of every fluid. The per-state RSS column shows page
  reuse more than the states, so heap is the per-state figure. And `<…>.scaling.csv`
  (`mode,threads,states_per_thread,median_ns_per_state,speedup`): DT + h + c_p on the (T, ρ) grid, 2000 states per
  thread, one state per thread and fluid built before the clock, threads unpinned, at 1, 2, 4, 6 (the physical cores)
  and 12 threads, for each bench fluid alone and `mixed` (every thread takes the five fluids in turn); speedup = N × the
  one-thread time over the N-thread time.
- **Recording:** `cargo xtask bench --record` writes `crates/phasekit-verify/benches/results/<milestone>-<machine>.csv`
  (date, commit, step, bench, fluid, median, unit, CPU, OS, rustc, governor). At each milestone close the results are
  compared with the ARCHITECTURE.md §7 table.

| Bench (one x86-64 core) | Target (ARCHITECTURE.md §7) | Recorded | Enforced | CoolProp 8.0.0 |
|---|---|---|---|---|
| Hot lookup by name | ≤ 50 ns *(inference)* | M2 | M9 | `PropsSI` rebuilds a backend: 76.5 µs; C++ 62-88 µs (M1.15a run) |
| α^r bundle, order 2, 16-20 terms | ≤ 0.3 µs | M3: 0.22 µs (n-Heptane, 12 terms), 0.35-0.39 µs (R134a 21, n-Propane 18), 0.69 µs (Methane 40); 31 ns of it fixed per call (M3.8, `alphar_order2`) | M9 | C++ (M1.15a run): 0.37 µs (12 terms), 0.55-0.60 µs (18-21), 1.2 µs (40), 2.7 µs (56) |
| Properties at (T, ρ) | ≤ 0.5 µs | M5 | M9 | update(D,T) + h + c_p: 1.5-10.7 µs; C++ 0.76-3.2 µs (M1.15a run) |
| Heap allocations per flash or batch point | 0 | M5 | M5 (test) | n/a |
| QT / PQ via superancillary | ≤ 0.1 µs | M6 | M9 | 0.45 / 0.64 µs; C++ 0.21 / 0.22-0.23 µs (M1.15a run) |
| PT / PH single phase | ≤ 3 / ≤ 15 µs | M7 | M9 | 19-27 / 119-376 µs; C++ 3.4-25 / 37-255 µs (M1.15a run) |
| Thread scaling, same and different fluids (N = physical cores) | ≥ 0.9·N | M9 | recorded, a miss is re-planned | GIL-bound 0.97× on 4 threads; C++ (M1.15a) 3.0-3.8× at 4, 3.1-5.4× at 6 (mixed 4.2×), 3.4-6.5× at 12 threads |
| Memory: EOS ≤ 25 KiB + SA ≤ 25 KiB per fluid; all 136 ≤ 8 MiB RSS | as stated | M3: compiled residual part ≤ 13.4 KiB (Methanol, 44 terms; `compiled_residual_parts_fit_25_kib`, M3.8) | M9 | 100-300 KiB per state; +67 MiB on first use; C++ (M1.15a): 0.49-0.56 MB heap per state, first use 33 MB heap / 58 MB RSS, one state of each of 136 fluids 100 MB heap / 105 MB RSS |
| α^r on AVX2 lanes | ≤ 0.1 µs/state and ≥ 2.5×, or stop | M12 | M12 | n/a |

**Enforcement from M9 (E9):** (1) on Linux PRs, gungraun instruction counts of the tracked benches may not regress by
more than 5 % against the baseline committed at M9 *(decision)*; (2) allocation counts stay 0 (a test, from M5); (3) the
§7 targets are checked on the reference machine at each milestone close; a miss fails the milestone and is resolved by
optimisation or a recorded re-plan with measurements, never by quietly editing the target. Wall-clock numbers from
shared CI runners are informational (noise) *(inference)*. The `libm` cost rule (PLAN.md M9.5) and the SIMD gate
(≥ 2.5× α^r batch throughput on AVX2 over the M9 baseline, or stop and re-plan; ARCHITECTURE.md §7) use the same
harness.

## 13. Platform verification (Linux, Windows, WASM)

| Platform | Verified by | Specifics |
|---|---|---|
| x86_64 Linux | everything; the only fixture-generation platform (K18) | nightly sweeps, gungraun, cross-target hash reference |
| Windows MSVC | full `cargo test`, corpus included | MSVC CRT transcendentals differ in ulps from glibc *(inference)*, covered by the classes; fixtures checked out without CRLF conversion (3.3). M10: C smoke program; the shim's cross-thread `errstring` handoff (#3211: thread B reads thread A's error, map 11 §8); FP guard for hosts that unmask FP exceptions (map 12 R18): `pk_*` exports mask traps inside and restore the caller's control word and sticky flags; the shim reproduces v8.0.0's three cases (traps masked inside, an already-masked environment unchanged, flags cleared; map 11 F17, user decision 9b) |
| wasm32-wasip2 | full corpus under wasmtime | no filesystem (embedded fixtures), no threads, rayon off; no-panic tests matter most, as a panic aborts the instance (E12); no WIT component in v0.1 (user decision 8) |
| wasm32-unknown-unknown | builds (baseline, `+simd128`) from M0 | M10: `phasekit-wasm` builds, `features` and size gates, and a JS smoke test ported from CoolProp's `test_wasm.mjs` (Water normal boiling point in [373.124, 373.125] K, dH/dT at constant P = c_p to 1e-6, dp/dT along saturation > 0; map 11 §8) via wasm-bindgen-test under Node, CI only *(inference)*. Browser and wasip2 share wasm codegen, so wasip2 results stand for the browser's numerics *(inference)* |
| aarch64 | `cargo test` | Rust never contracts to FMA (K12), so CoolProp's arm64 tolerance failures (map 12 R17) do not apply; one of the four `libm` hash targets |
| C ABI (M10) | `abi` gate; C smoke on Linux and Windows | `catch_unwind` at every export (a panicking-stub test); the `HAPropsSI` stub returns +inf and sets the documented `errstring` (D11); shim codes match the `codes` fixture; Tier B absent |

## 14. Per-milestone verification matrix

Gate names are those of PLAN.md §2.4 and 11.2; fixture names those of 3.4-3.6.

| M | New verification | Fixtures added | Arbiters | Gates turned on |
|---|---|---|---|---|
| M0 | the sketch's 47 tests minus the 4 that call `phasekit_compat` (43 executed on Linux, 40 on wasip2; the 4 return at M5.9); L0 unit tests; compile-time Send/Sync and size asserts; lint probes | — | hand derivations | G1-G8 with `deps`, `lints`, `counts`, `ignores`, `doc-excerpts`, `rot`, `assertions`, `mutants`; deny, reuse, shear, MSRV; CI matrix (or CI owed until the user's remote exists) |
| M1 | kit: `Provenance`, `ToleranceClass` (+ `SaFit`, `Measured`, `Identity`, `Fd`, `RefAnchor`), `from_printed`, fixture reader + `bits`, comparators, `sample::SplitMix64` (golden vector), report; register schema (6.1), `check_register`, proof dispatcher and `MILESTONE`; `ARBITERS`; proptest over `Input::new` and batch shapes; conformance kit (FD, gauge, sequential policy); C++ baseline | `oracle.lock`, `MANIFEST.sha256`, `facts/smoke.csv`, `facts/register.csv`, `mp/check-points.csv` | `from_printed("21.17909")` = 5e-6 (map 10 §8.4); double entry of IAPWS-95 Tables 6-8, Lemmon 2016 Table 7, Thol 2016 Table 3, NIST IR 8474 Tables 3-4 and the 18 CoolProp test states | fixture round trip bit-exact; `fixtures`, `register`; nightly drift check |
| M2 | datagen: 136 fluids parse; 130 `source_eos_hash` stamps recompute + self-test; 390 check points match the JSON; 556 keys, 0 collisions; JSON → record bit for bit (map 09 §8); Parity diff = patches; hash gate (7.3); caloric section round trip and stamp (M2.11); readable fluid dump round-trips every blob (M2.9a); counting-source lazy tests; blob fuzz; citation lint | `crit` (published, `Exact`; core and all-fluid tier) | — | `datagen`; DIV-0003 (constant) and DIV-0006..0008 proofs; lookup bench |
| M3 | L1 for separable kinds: jets vs num-dual at ~300 points per block incl. δ → 0; oracle block isolation | `term` (separable residual kinds, core subset; α^r total rows in the all-fluid tier) | AD (num-dual) | α^r bench; memory budget recorded |
| M4 | NonAnalytic + 10 ideal kinds; full α^r, α⁰; critical `Undefined` (Water, CO₂) | `term` (NonAnalytic, every α⁰ kind; α⁰ total rows in the all-fluid tier) | IAPWS-95 α table ("Table 6") → `SelfConsistent`; Jaeschke & Schley c_p⁰ | R123 `Investigate` entry |
| M5 | relations, DT, SA evaluation, caloric curves vs the EOS (`SaCoeff`, M5.2a), gauge, `from_total`/`from_split`, first partials, Z, Cp0, residual parts, exact virials, compat; L4 identities, FD on partials, gauge invariance; `new_family.rs`, `gibbs_seam.rs` (with the 4 restored tests); zero allocations | `eos`; `sat` `sa` rows (DT needs them, E18); `flash` DT rows | IAPWS-95 (T, ρ) table; Lemmon 2016 Table 7; Thol 2016 Table 3; Lemmon & Akasaka 2022 Table 7; NIST IR 8474 Table 3; one-row EOS states; P-mp points vs SA (`SaFit`) | `features`; DIV-0001 (three parts), -0004 (c_p, c_v), -0005 (Table 3), -0011, -0012, -0014 proofs; DT-properties bench; allocation test |
| M6 | pure VLE, critical points, QT, PQ, `Guess` polish, pseudo-pure rules; saturation equilibrium; SA vs VLE; stale-SA polish; exact rescaling; small dense solves | `sat` QT/PQ and pseudo-pure rows; numerical `crit`; fastchebpure dense (core) + `mp/fastchebpure.lock` | 390 P-mp points (`SatMp` for VLE); fastchebpure dense near Tc; IAPWS-95 saturation table; NIST IR 8474 Table 4 | DIV-0001/-0003 rescaling, DIV-0005 (Table 4); R410A oracle rows; QT/PQ bench |
| M7 | all 19 pairs, phase rule, round trips, 19 × 2 capability matrix, `Ambiguous`, robustness corpus, second partials, fundamental derivative | `flash` | Thorade & Saadat identities | L5 nightly sweeps (flash, eos, sat); PT/PH bench |
| M8 | σ; staged, IAPWS and ECS transport; melting; transport edits; stage-by-stage paper rows; thermo never loads ECS references | `transport`, `sigma`, `melt` | IAPWS R12-08, R15-11, R14-08, R1-76(2014), D2O releases; Herrig 2018; re-provenanced transport rows (P only); Mulero σ coefficients | DIV-0002, -0004 (η, λ), -0009 proofs; the planned M8 entries (6.6) |
| M9 | rayon: `policy_equivalence` for `Parallel`, 1..N threads, racing first use; `thread_invariance` in the kit; cross-target hash (9.4) | `hash/libm.txt` if the `libm` hashes agree | — | `perf` (gungraun ≤ 5 %, §7 targets); `libm` decision (PLAN.md M9.5) |
| M10 | reference states; compat tables + derivative grammar; C ABI + shim; WASM | `refstate`, `props` (85 outputs), `codes` | IIR, ASHRAE, NBP definitions (`RefAnchor`) | 85-output oracle gate; `abi`, wasm `features` and size budget; C smoke; shim codes, #3211 handoff, `HAPropsSI` stub, FP-guard semantics; JS smoke; release: semver-checks, cargo-about → **v0.1** |
| M11 | cubic family through the kit; cubic-R register entry (6.6) | none from 8.0.0 for s or s-input flashes (DIV-0010) | cubic papers; source model c_p⁰ | `core-frozen`; DIV-0010 proof |
| M12 | lanes vs scalar bitwise; forced dispatch; paper rows on lanes | — | K1, K2 rows | SIMD gate ≥ 2.5× or stop |
| M13 | mixtures: x = [1, 0] invariance; equilibrium residual max abs(ln(f_V/f_L)) < 1e-6 (map 10 §2.2); composition derivatives, AD vs FD | mixture fixtures with `NORMALIZE_GAS_CONSTANTS` pinned per model (map 10 §8.4) | Bell 2022/2023 tables; Tkaczuk Table 8; teqp GERG vectors (differential) | DIV-0013 resolved |
| M14 | IAPWS-06 ice Ih, IF97 through the kit; g_ice = g_liquid on R14-08 melting points | IF97 oracle rows | IAPWS R10-06 tables; IF97 release tables | `core-frozen` |
