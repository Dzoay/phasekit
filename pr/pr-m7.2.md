## What and why

M7.2 completes **the PT and DT phase rule**: hint → critical point → curve → the generic VLE (PLAN.md M7.2; map 03 §6 "PT near saturation", "Inconsistent range policy"; ROT-073, ROT-080; ARCHITECTURE.md D6).

- **PT at the saturation pressure has two roots** (ROT-080).
  - CoolProp's PT of Water at psat answers liquid at 305 K, from its p-based rule below 0.9 T_triple + 0.1 Tc, and throws at 315 K.
  - Here one rule holds at every T. At p = psat, `Strict` returns `Ambiguous` with both densities in ascending order (ρ″, ρ′). `Stable` does the same, since neither is more stable there.
  - `Nearest(ρ)` picks the root nearer its density, and an imposed phase picks its own.
  - One float above or below psat, the stable root is the only answer.
- **The saturated end of each PT bracket reaches 1e-9 past ρ′ or ρ″.** A curve's densities and pressure agree with the EOS's VLE only to their fit, so for p a hair from psat the root lies a hair past them. The spinodal is far further away.
- **The generic VLE** (`vle::from_eos`).
  - Below Tc where no saturation curve's fitted range covers T, DT and PT take the saturation from the EOS alone. Before, they answered `Unsupported`; a fit is still never evaluated outside its range (D6).
  - The isotherm's spinodals either side of the critical density bound its branches (TOMS 748 on (∂p/∂ρ)_T).
  - A Maxwell construction in ln p between the spinodal pressures follows, each side's density bracketed on its branch. It works because d(g′ − g″)/dp = v′ − v″ < 0.
  - The result seeds the VLE's Newton.
  - The out-of-tree van der Waals family now has a dome without shipping a curve, and below its curve's range too.
- **The PT bench** (`pt_flash`, the C++ baseline's `pt` workload): recorded in `benches/results/M7-intel-core-i7-8700k.csv`: n-Heptane 1.60 µs, R134a 2.28, n-Propane 3.02, Methane 4.51, Water 38.6 µs; CoolProp's C++ on the same machine 3.4, 5.3, 4.8, 9.7 and 25.1 µs. Each of Water's Newton steps is one α^r (5.3 µs, its non-analytic terms, M9.2a), and from the ideal-gas seed these dense supercritical states take about seven; a better seed and NA1 are M9's.

## Tests

| Test | Truth | Result |
|---|---|---|
| `pt_at_saturation_is_ambiguous_under_strict` | Water's superancillary at 305 K and 315 K (ROT-080) | `Ambiguous` [ρ″, ρ′] within 1e-9 of the curve's under `Strict` and `Stable`; `Nearest` and hints pick one; one float off psat, liquid above, gas below |
| `imposed_two_phase_dt_matches_qt` | QT at the state's quality (ROT-073) | Water at 1000 mol/m³ and 400 K: p, h, s, ρ within 1e-12; the same state as without the hint |
| `water_dg_dt_at_constant_p_equals_minus_s` | the identity; CoolProp's −393.0620684404547 J/kg/K (map 01 §8) | within 1e-14 of −s, 1e-12 of the oracle |
| `pt_range_is_one_rule` | the domain (map 03 §6) | Water at 3000 K and 1e5 K, and at 5 GPa, refused under `Enforce` with DT's errors; flagged under `Extrapolate` |
| `subcritical_without_a_curve_uses_the_eos_vle`, `new_family_subcritical_dt_goes_through_the_core_phase_rule` | the test's own Maxwell construction, bisected to adjacent floats | van der Waals at 0.8 Tc and 0.4 Tc: p_sat within 1e-12, the lever rule's quality, PT's two roots at psat within 1e-10 |
| `pt_flash` (bench) | the C++ baseline's grid | recorded |

## Red evidence

The six tests were written first. On the parent, three of them fail: DT and PT below every curve's fitted range answer `Unsupported`, and PT at Water's psat does not converge (the liquid bracket's end sat on the root):

```
FAIL [   0.004s] (1/6) phasekit-verify::new_family subcritical_without_a_curve_uses_the_eos_vle
thread 'subcritical_without_a_curve_uses_the_eos_vle' (2164518) panicked at crates/phasekit-verify/tests/new_family.rs:128:45:
called `Result::unwrap()` on an `Err` value: Unsupported { pair: DT }
FAIL [   0.004s] (2/6) phasekit-verify::new_family new_family_subcritical_dt_goes_through_the_core_phase_rule
thread 'new_family_subcritical_dt_goes_through_the_core_phase_rule' (2164519) panicked at crates/phasekit-verify/tests/new_family.rs:265:50:
called `Result::unwrap()` on an `Err` value: Unsupported { pair: DT }
FAIL [   0.009s] (6/6) phasekit-core flash::tests::pt_at_saturation_is_ambiguous_under_strict
thread 'flash::tests::pt_at_saturation_is_ambiguous_under_strict' (2164521) panicked at crates/phasekit-core/src/flash.rs:814:17:
305 K: Err(NoConvergence { strategy: DensityNewton, iterations: 1 })
```

The other three (`imposed_two_phase_dt_matches_qt`, `water_dg_dt_at_constant_p_equals_minus_s`, `pt_range_is_one_rule`) already passed on the parent. They stay as the checks PLAN.md M7.2 names.

## Gates

GATES

Plan-Step: M7.2

🤖 Generated with [Claude Code](https://claude.com/claude-code)
