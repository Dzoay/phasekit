# QT and PQ against CoolProp (2026-10-13)

Issue #142, re-planned at M9.6: §7's ≤ 0.1 µs for QT and PQ timed the flash alone, against CoolProp's update, which
evaluates only the superancillary and leaves the saturated phases' EOS to the first property read. A `State` holds
both phases' bundles (D8: `Copy`, no interior mutability), so it pays that work in the flash.

| # | Question | Decision | Notes |
|---|---|---|---|
| PF1 | Defer the phases' bundles (a lazy `State`), restate the target, or build cheaper bundles? | **Restate the row as QT / PQ + h, the property a caller reads, against CoolProp's update + hmolar on the same machine; and build the saturated phases at order 1 strictly inside the dome** | CoolProp's two-phase hmolar evaluates each phase's EOS, so both sides do the same work. The C++ baseline adds `qt_h` and `pq_h` (format v2), which `gates perf --targets` reads for the machine. Strictly inside the dome a state reads only ρ, h, s and u (SQ1-SQ3), so its phases' second order is not computed; at Q = 0 and 1 it is. *Rejected:* a lazy `State`, which breaks D8 and would evaluate again at every read. |
