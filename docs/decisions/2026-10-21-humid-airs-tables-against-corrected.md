# Humid air's tables against `Corrected` (2026-10-21)

Asked in issue #275, when M18.3's red gate failed in a way the plan did not foresee (PLAN.md §0.4). Under `Corrected`,
every v of the authors' Tables 1, 2 and 5-14 (Herrmann, Kretzschmar and Gatley 2009) held within its printed digits,
but three groups of values did not:
- **h from 35 °C up.** Table 2's h_s missed at 9 of 19 rows and Tables 5-14's h at 14 of 106. One constant per
  kilogram of water fits all 125 h values from 0 to 320 °C and 0.1 to 10 MPa: ours is 2.29 to 2.47 J/kg of water above
  the authors'.
- **Table 8's s (200 °C, 5 MPa).** It sits above the printed values on all 7 rows, by a mean of 0.56 of a printed unit
  and at most 0.98. Tables 7, 9 and 13 show nothing like it.
- **One Table 12 s (320 °C, 2 MPa, W = 0.05).** It is 0.501 of a printed unit off.

The issue ruled out:
- IAPWS-95's ideal gas in place of IF97's, which misses by more;
- a virial term;
- dry air;
- a mix of gas constants;
- IF97's own zero.

RP-1485's report, which might name the cause, is not in hand (PLAN.md §6).

| # | Question | Decision | Notes |
|---|---|---|---|
| HA8 | How should M18.3 check the authors' values that miss their printed digits under `Corrected`? | **Option 1: keep `Corrected` as RP-1485 describes it. Check those values within a register entry's measured bounds (`Investigate`), and leave the cause open until the report is in hand; every other value of Tables 1, 2 and 5-14 stays at its printed digits** | The bounds, each the measured residual: Table 2's and Tables 5-14's h within the printed half unit plus W × 2.47 J/kg; Table 8's s within one printed unit, 0.1 J/(kg K); Table 12's s within the half unit plus 1e-4 J/(kg K). *Rejected:* subtracting a fitted 2.26 J/kg from water vapour's h under `Corrected` (the constant has no source); holding M18.3 until the report is in hand. |
