# Error messages in the caller's units (2026-10-21)

Asked when the user took up issue #191: core's error messages name SI values without their units, and every facade
passes them on verbatim (`invalid temperature: 0`, `outside the validity domain: AboveMaxTemperature { t: 700.0, t_max:
647.0959999999873 }`). The user wanted messages in a viewer's display units, not just SI, while the site offers two
unit systems today and a caller may pick any. Core's API is SI only (D5) and the kernel has no global mutable state, so
core cannot know a viewer's units. `InvalidInput` names its quantity by a string, about 37 different ones across the
places that build it: `"T"` beside `"temperature"`, `"p"` beside `"pressure"`, and many that are not physical
quantities (`"number of mole fractions"`, `"plane"`). The core is frozen from `m14-start` to M18's close (user decision
HA6), and M18 had not started. The proposal was core's wording rendered through units the caller supplies.

| # | Question | Decision | Notes |
|---|---|---|---|
| EM1 | How do error messages reach a caller's display units? | **Core keeps the wording and renders it through units the caller supplies, keyed by dimension; `Display` is SI** | Any number of unit systems, and per-quantity choices, work without core listing them; the browser package takes the site's units as a table, compat and the C ABI write SI. Each number in `Error` is tagged with its quantity, so a facade can also map the value, not parse the text (issue #191's option 2). *Rejected:* SI units in the messages alone (the issue's option 1); leaving it to each facade (option 3); core knowing the unit systems; the site writing its own messages from the fields (the wording in two places); a units argument on every package call. |
| EM2 | Which message style? | **Both, chosen by the caller: a long form in words and a short form that states the relation the value breaks** | "can we support long form language based message style and short form range / maths based style? I would like the option as in some places in UI's we won't have space for long form." `{}` is the long form in SI, `{:#}` the short; the browser package takes the style beside the units. The drafts shown were `temperature 200 K is below the minimum, 273.16 K` and `T = 200 K < T_min = 273.16 K`; M28.1's PR shows every message in both styles for review. |
| EM3 | Does `InvalidInput` split? | **Yes: physical values, tagged with their quantity, apart from structural arguments, which carry no unit** | "Split invalid input as you say". Counts, names, planes and grids are arguments, not quantities. |
| EM4 | How precise are the numbers in a message? | **Six significant figures; the exact value stays in the error's fields** | "precision as you say". `647.0959999999873` is written 647.096. |
| EM5 | When does the core change land? | **Before M18.1, as one core step (M28.1); then the core is frozen from a new tag, `m18-start`, at its merge, to M18's close** | `m14-start` stays where it is; M18's freeze moves to `m18-start` (this replaces HA6's for M18 only). Humid air's errors are built on the new quantities from the start, so core's dimensions cover what M18 needs. *Rejected:* after M18 closes. |
| EM6 | Unicode or ASCII in the short form? | **Unicode** | `ρ = 0 kg/m³ ≤ 0 kg/m³`, `Q = 1.5 ∉ [0, 1]`. Every facade passes UTF-8, and the units already use °C and µPa·s. *Rejected:* ASCII only (`rho`, `<=`, `not in`). |
