# The characteristic curves' range (2026-10-22)

Asked at the start of M26.3, which found what the plan had not foreseen: Brown's characteristic curves reach zero
density only far above the planes' plotted range, which ends at 1.6 T_c (PLAN.md M19.1). By CoolProp 8.0.0's virials,
the ideal and Boyle curves end where B = 0, at 2.34 T_c for Water, 2.38 for CarbonDioxide and 2.6 for Nitrogen and
Air; the Joule–Thomson inversion curve where T dB/dT = B, at 4.45 T_c for CarbonDioxide and 4.8-4.9 for Nitrogen and
Air, above Water's T_max (3.09 T_c); and the Joule inversion curve, where dB/dT = 0, above every fluid's T_max.
n-Propane's, R134a's and R410A's T_max (1.2-1.8 T_c) are below even their Boyle temperatures. Within 1.6 T_c the
curves cross the planes only at liquid-like densities. The proposal was to sample the curves over the equation of
state's range and give them a view of their own.

| # | Question | Decision | Notes |
|---|---|---|---|
| DG10 | Over what range are the characteristic curves sampled, and where are they drawn? | **The equation of state's range, with a view of their own** | The curves are sampled from the plotted range's lowest temperature to the equation of state's T_max, not to the planes' 1.6 T_c. The planes draw the parts inside their range; M26.6 adds a view of the whole curves, log p against log T. `the_curves_zero_density_ends_are_the_virials` holds where its virial condition has a root below T_max, and asserts that there is none where there is none (the Joule inversion curve's, for every fluid tested). *Rejected:* the planes' range alone, which would drop the zero-density test; widening every plane, which would change M19's ranges and the site's plots. |
