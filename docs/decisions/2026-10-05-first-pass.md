# First pass (2026-10-05)

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
