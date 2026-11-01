# The rest of IF97 (2026-11-01)

Asked when the user, shown what v8.0.0's `IF97::Water` computes beyond phasekit's IF97 water, said: "I want phasekit
to match coolprops capability so we need to add the plan scope and then start the rest of the IF97 work". M14 had
built what humid air takes from IF97: regions 1, 2 and 4 below 623.15 K, named in no compat string (user decisions
HA1, HA4, HA5). M27, the rest, waited for a steam use case (HA1), and its scope named no surface tension. The oracle
that day: v8.0.0's `IF97::Water` gives region 2 above 623.15 K, region 3 (ρ = 488.875 kg/m³ at 650 K and 25 MPa),
region 5 (h = 5167.24 kJ/kg at 1500 K and 30 MPa; 60 MPa refused), the HP, PS and HS flashes, two-phase states
included, μ and λ (8.537e-4 Pa s and 0.6095 W/(m K) at 300 K and 1 bar), σ (0.07169 N/m at 300 K) and the Prandtl
number, under the names `IF97::Water` and `IF97::H2O`. It refuses the pairs with a density (TD, DH), generic partial
derivatives, and every state below 273.15 K or 611.213 Pa. The free arbiters, fetched from iapws.org that day:
R7-97(2012), SR2-01(2014), SR3-03(2014), SR4-04(2014) and SR5-05(2016); R12-08, R15-11 and R1-76(2014) were in hand.

| # | Question | Decision | Notes |
|---|---|---|---|
| SW1 | When does M27 start? | **Now: the user's word replaces HA1's trigger, a steam use case** | HA1's order for M14 and M18 stands; both are closed. |
| SW2 | What is M27's scope? | **Everything v8.0.0's `IF97::Water` computes, as the floor: its five regions, its backward equations, the PT, QT, PQ, HP, PS and HS pairs, μ, λ, σ and Pr, and the names `IF97::Water` and `IF97::H2O` in compat, C and the browser package** | Surface tension (R1-76(2014)) joins M27's scope, which lacked it. HA5's condition for the names, all five regions, is then met. What v8.0.0 refuses (the pairs with a density, generic partial derivatives) is not required, and phasekit keeps its wider domain below 611.213 Pa (ROT-242). |
