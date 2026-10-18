# Helium's critical conductivity term (2026-10-13)

Asked after M8 (DIV-0035): Hands and Arp 1981's critical term, as REFPROP codes it and CoolProp after it, is NaN in two
lobes either side of the critical isochore from 5.20 to 5.99 K, where phasekit refused λ. No free copy of the paper was
found; the scaled equation it reads is printed in Levelt Sengers, Greer and Sengers 1976 (eq. 3.1, Table 17: Kierstead's
He⁴ data) and Hanley, McCarty and Haynes 1974 (eqs. 21-23), and the code differs from both in three places.

| # | Question | Decision | Notes |
|---|---|---|---|
| HE1 | Keep refusing helium's λ in the lobes, or fix the term from the printed sources? | **Fix it from the printed sources** | The NBS equation as printed: inner exponent 2β (the code has 2/β), x = ΔT*/\|Δρ*\|^{1/β} (the code (ΔT*/Δρ*)^{1/β}), ρ_c 69.58 kg/m³ (the code 69.158, which also sets the damping and the blend's bounds). REFPROP's blend, its bounds, the damping and its two factors stay, as no printed source gives Hands and Arp's. A code fix in the core cannot select by `DataSet` (VERIFICATION.md §6.4), so both data sets carry it and DIV-0035 stays `SkipOracle`: its oracle λ cells of 3.5-12 K are exempt, replaced by NIST TN 1334's 54 rows (within 1.07 %; CoolProp's code 8.1 % off at one, NaN at another). |
