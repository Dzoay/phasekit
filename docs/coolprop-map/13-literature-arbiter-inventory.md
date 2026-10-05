# 13 Per-fluid literature-arbiter inventory - CoolProp v8.0.0 map

> Scope: `dev/fluids/*.json` (136 fluids: 130 pure, 6 pseudo-pure; keys `EOS[0].BibTeX_EOS`, `BibTeX_CP0`, `TRANSPORT.{viscosity,conductivity}.BibTeX`, `ANCILLARIES.{surface_tension,melting_line}.BibTeX`), `CoolPropBibTeXLibrary.bib` (3,622 lines), `src/Tests/CoolProp-Tests.cpp:44-581,4165-4193,4394-4500,4781-4840`, `Web/scripts/fluid_properties.PurePseudoPure.py` (96 lines). Loader and accessor code is about 40 lines. Part of the coolprop-rs port plan; cites the v8.0.0 source.
>
> Method (2026-10-04): keys come from the oracle `get_BibTeXKey` for all 136 fluids, cross-checked against the JSON. DOIs come from the `.bib`; 19 EOS keys have no DOI there, so their DOIs were looked up in OpenAlex and are labelled as such. Open-access status is OpenAlex `oa_status`, measured on the same date. Check tables were confirmed in three ways: (a) rows already transcribed in `CoolProp-Tests.cpp`, (b) reading the open-access full text (NIST IR 8474; PMC author manuscripts of Thol 2016 and Lemmon 2016), (c) reproducing the IAPWS-95 values with the oracle. **Every other "has a check table" entry is an expectation based on the authors' usual practice, not a verified fact.** The papers are paywalled and were not read.

## 1. Purpose and concepts

- **Arbiter.** The printed artefact that settles a disagreement between Rust, the oracle and the paper for one fluid and one model part. The hierarchy, strongest first:
  1. **Computer-verification table.** Either α⁰/αʳ and their derivatives at one (τ, δ), or (T, ρ) → p, c_v, c_p, w. These are exact to the printed digits.
  2. **Printed correlation equation and coefficients.** For c_p⁰, σ and melting lines, the closed form can be evaluated independently even without a check table.
  3. **Printed property tables** (saturation tables, etc.), at lower precision.
  4. **Oracle only.**
- **Classes used in the `check table` column of §4:**

| Code | Meaning | Fluids |
|---|---|---|
| `Tn (tests)` | Table n is transcribed in `CoolProp-Tests.cpp`. Provenance P-paper (doc 10 §8.1). | 13 |
| `Tn (verified)` | The table was read in open-access full text but is **not** transcribed | 6 (Helium, R1234ze(E), R115, R13I1, R227EA, R365MFC) |
| `IAPWS …` | IAPWS release, free at iapws.org | 2 (Water: values reproduced; HeavyWater: expected) |
| `exp.` | A check table is **expected** (NIST/Bochum/Akasaka/Zhou/Thol EOS papers since ~1996 routinely print one). Table number unverified; paper paywalled. | 74 |
| `?` | Older, conference or Energy & Fuels/FPE papers where the convention is uncertain | 24 |
| `none …` | Stated absent (R1336mzz(Z), `CoolProp-Tests.cpp:4484-4490`), or not expected: book tabulations, MBWR originals | 14 |
| `UNPUBLISHED` | No paper exists, or none could be found | 3 (Propylene, SES36, Neon) |

- **Concepts reused from earlier docs, not repeated here:** provenance classes P-paper, P-IAPWS, R-other, R-self and P-mp, plus the fixture format (doc 10 §8.1, §8.3). The term catalogue (doc 02 §3.1-3.2). Transport model inventory (doc 05). Missing `.bib` keys (doc 09 §4.4).

## 2. Structure (key types/functions -> path:line)

| Item | Location | Notes |
|---|---|---|
| EOS/CP0 key load | `src/Backends/Helmholtz/Fluids/FluidLibrary.h:389-390` | Plain strings. An empty `BibTeX_CP0` (111 of 136) means "same paper as the EOS" by convention (inference). Only n-Heptane cites a separate c_p⁰ paper (Jaeschke-IJT-1995). |
| Viscosity key | `FluidLibrary.h:714-718,721` | **If `viscosity` is an array, only `front()` is parsed** (code comment: "If an array, use the first one, and then stop"). In 7 fluids (R1234yf, R1234ze(E), R124, R152A, R22, R245fa, R32) entry 0 is `Bell-PURDUE-2016-ETA` rhosr-CS. The second entry (ECS: BELL-PERSONAL-2015, Huber-IECR-2003, Klein-IJR-1997; or untyped Krauss-IJT-1996 for R152A) is never loaded but still carries a citation. On master, R245fa and R32 have grown 3-entry arrays with a new front entry. |
| Conductivity, melting, σ keys | `FluidLibrary.h:944,1017`; `FluidLibraryFactories.h:34` | |
| Accessor | `HelmholtzEOSMixtureBackend.cpp:263-289` (BibTeX branch of `HelmholtzEOSMixtureBackend::fluid_param_string`); the Python `get_BibTeXKey` is a thin wrapper that forwards `"BibTeX-"+key` (`src/nanobind_interface.cxx:1167`, `wrappers/Python/CoolProp/CoolProp.pyx:645`). `CoolProp.cpp:1147-1152` is only a Catch test listing the accepted `BibTeX-*` strings | `ECS_*` keys throw `NotImplementedError` (`:278-283`) |
| Docs table generator | `Web/scripts/fluid_properties.PurePseudoPure.py:49-58,85-89` | Prints "problem" and then **blanks** any key that is missing from the `.bib` (`:54-56`) |
| Paper EOS rows | `CoolProp-Tests.cpp:4394-4474` (R1234yf, 6 states, Table 7), `:4787-4840` (12 fluids × 1 row), `:4484-4500` (R1336mzz(Z), constants only) | |
| Paper transport rows | `CoolProp-Tests.cpp:44-301` (η), `:347-581` (λ) | Provenance per fluid in the §4 `tests` column |
| Melting check values | `CoolProp-Tests.cpp:4165-4193` (D2O, Herrig Sec. 3.4); `Ancillaries.cpp:282-312` (IAPWS R14-08) | |

## 3. Algorithms and formulas (cite the papers CoolProp cites)

- **Kinds of check table:**
  - **K1, α derivatives at one state.** IAPWS-95 (Wagner & Pruß 2002; IAPWS R6-95(2018) Table 6, at 500 K and 838.025 kg/m³) and IAPWS R16-17 (D2O). These isolate EOS-evaluator bugs from property-formula bugs, so they are ideal for the first TDD step.
  - **K2, (T, ρ) → p, c_v, c_p, w (sometimes h, s).** Every NIST-style table (for example Lemmon & Akasaka 2022 Table 7). The ρ = 0 row tests α⁰ alone.
  - **K3, saturation rows** (NIST IR 8474 Table 4; IAPWS-95 Table 8). These test VLE and superancillaries (SA).
- **Tolerance rule.** Half a unit in the last printed digit, relative: tol = 0.5·10^(e−k+1)/|x| for k significant digits. Doc 10 R4 flags the ad-hoc tolerances in use today. Where K2 tables print c_p near T_c (for example R1234yf c_p = 48981.3), the tolerance is still the printed digits, not a hand-set `5e-3` (`CoolProp-Tests.cpp:4471`).
- **Arbitration protocol** (measured examples):
  1. **Water.** Oracle vs IAPWS-95 K1 (11 α values) agree to ≤ 2.9e-9 relative, and vs K2 (3 rows) to ≤ 2.6e-9. Both are within the 9 printed digits, so the oracle and the paper agree. *(The table numbers 6 and 7 are from memory; the values reproduced.)*
  2. **Lemmon et al. 2016 Table 7** (R227EA, R365MFC, R115, R13I1; 12 states). Oracle ≤ 4.3e-7, which is within the 7 printed digits.
  3. **R1234ze(E), Thol et al. 2016 Table 3.** Oracle p is **+1.0e-6 to +1.4e-6** high at all 5 states with ρ > 0 (the 6th row is ρ = 0, p = 0); the printed digits allow about 2e-7. The ρ = 0 row's c_p⁰ is also +1.3e-6 off (allowance 6e-7). The cause: the JSON stores R = 8.314472 (`dev/fluids/R1234ze(E).json:4179`, again at `:4373`), while the paper states "the value of the gas constant (and that used in this work) is 8.3144621". With R = 8.3144621 swapped in through `add_fluids_as_JSON`, the p error falls to ≤ 1.9e-7 and every c_v, c_p and w value falls within half a unit of its last printed digit (re-measured 2026-10-04). **The paper wins; this is an oracle divergence.** Still the same on master.
  4. **Helium, NIST IR 8474 Table 3.** The oracle is systematically −0.9e-7 to −4.8e-7 off in p, c_v and w (re-measured). c_v is printed to 7 significant digits (±6e-8), so the error is about 5-6× the last digit. **The residual coefficients are not the cause.** IR Table 2 matches `dev/fluids/Helium.json` EOS[0] term for term (23/23 terms: n, t, d, l, η, β, γ, ε; verified 2026-10-04), and so do T_c = 5.1953 K, ρ_c = 17.3837 mol/dm³ and M = 4.002602 g/mol. That supports upstream 520b8809. **The difference is in R.** The JSON stores R = 8.3144598 (`Helium.json:3639`, CODATA 2014). IR Table 1 gives R = 8.314472 and says "the value in Table 1 should be used in implementing the equation of state". Yet using that R makes Table 3 worse (+1.0e-6 to +1.4e-6), and R = 8.3144621 makes it better (max 2.7e-7). *Inference:* Table 3 was not generated with the R printed in Table 1, so the arbiter is internally inconsistent. The 1-3e-7 that remains at the best-fitting R is unexplained. Table 4 (K3 saturation, 5 printed digits) agrees within its digits. **Divergence of unresolved cause: neither the oracle nor the IR as printed can be taken as the arbiter.**
  - **Lesson:** before blaming the evaluator, check the stored **R, M, T_r, ρ_r and the per-block `Tc`** against the paper. Then check that the paper's own check table reproduces with its own stated constants. The known cases: R1234ze(E) R (above); Helium R vs IR Table 1 vs Table 3 (above); the 4 reducing densities fixed in 2acbbc82; R123 block `Tc` (doc 02 §6).
- **Arbitration without a check table.** The ideal-gas part can always be checked against the printed c_p⁰ equation, which is closed form, for example the Aly-Lee form from Jaeschke & Schley 1995 for n-Heptane. σ can be checked against Mulero's coefficients, which use Mulero's own T_c, not the EOS T_c. Melting can be checked against the printed Simon or polynomial form. So "no check table" removes the arbiter only for αʳ.

## 4. Data and configuration inputs

**Per-fluid inventory (136 rows).** Legend:
- `pp` = pseudo-pure. CP0 is blank when the c_p⁰ source is the EOS paper; `=EOS` means the key is given explicitly and equals the EOS key. M12/M14 = Mulero-JPCRD-2012/2014 (10.1063/1.4768782, 10.1063/1.4878755).
- **tests:** E = EOS paper row. η/λ rows: P = paper/IAPWS, R = REFPROP 9.1/10 (R-other), P* = experimental data, ? = "Some of these don't work", unsourced (`:408`), off = transcribed but commented out. m = melting (P = paper, S = self-generated R-self).
- **OA:** pay = closed; green = repository or PMC copy; OA = gold/hybrid; free = bronze; n/a = no paper.
- DOIs marked with a year in brackets, `(OpenAlex)`, or the 2023/IECR/NIST-IR ones were **not** in the `.bib`; they were found by OpenAlex search. See R2.
- The EthylBenzene η row (`CoolProp-Tests.cpp:285-286`, 1 row, tol 1e-2) is attributed to "Mylona, JPCRD, 2014", a thermal-conductivity paper, while the v8.0.0 model is the ECS `Huber-RP912`. So it is marked `?`, not P. On master the default η is now `Meng-JPCRD-2017-ethylbenzene`.

| Fluid | EOS key | EOS DOI | CP0 | η key | λ key | σ | Melting | Check table | Tests | OA |
|---|---|---|---|---|---|---|---|---|---|---|
| 1-Butene | Lemmon-FPE-2005 | 10.1016/j.fluid.2004.09.004 |  | - | - | M12 | - | exp. |  | pay |
| Acetone | Lemmon-JCED-2006 | 10.1021/je050186n |  | - | - | M12 | - | exp. |  | pay |
| Air (pp) | Lemmon-JPCRD-2000 | 10.1063/1.1285884 |  | Lemmon-IJT-2004 | Lemmon-IJT-2004 | - | Lemmon-JPCRD-2000 | exp. | ηP λP mS | pay |
| Ammonia | Gao-JPCRD-2020 | 10.1063/5.0128269 (JPCRD 2023) |  | Fenghour-JPCRD-1995 | Tufeu-BBPC-1984 | M12 | - | exp. | ηP λR | pay |
| Argon | Tegeler-JPCRD-1999 | 10.1063/1.556037 |  | Lemmon-IJT-2004 | Lemmon-IJT-2004 | M12 | Tegeler-JPCRD-1999 | exp. | ηP λP | pay |
| Benzene | Thol-HTHP-2012 | - |  | Avgeri-JPCRD-2014-Benzene | Assael-JPCRD-2012-Benzene | M12 | - | ? | ηP λoff | pay |
| CarbonDioxide | Span-JPCRD-1996 | 10.1063/1.555991 |  | Laesecke-JPCRD-2017-CO2 | Huber-JPCRD-2016-CO2 | M12 | Span-JPCRD-1996 | exp. | ηP λP | pay |
| CarbonMonoxide | Lemmon-JCED-2006 | 10.1021/je050186n |  | - | - | M12 | Barreiros-JCT-1982 | exp. |  | pay |
| CarbonylSulfide | Lemmon-JCED-2006 | 10.1021/je050186n |  | - | - | M12 | - | exp. |  | pay |
| Chlorine | Thol-AICHE-2021 | 10.1002/aic.17326 | =EOS | - | - | - | - | exp. |  | OA |
| cis-2-Butene | Lemmon-FPE-2005 | 10.1016/j.fluid.2004.09.004 |  | - | - | M14 | - | exp. |  | pay |
| CycloHexane | Zhou-JPCRD-2014 | 10.1063/1.4900538 |  | Tariq-JPCRD-2014-Cyclohexane | - | M12 | Penoncello-IJT-1995 | exp. | ηP | pay |
| Cyclopentane | Gedanitz-JCED-2015 | 10.1021/je5010164 |  | Chung-IECR-1988 | Vassiliou-JPCRD-2015-pentanes | M14 | - | exp. | λP | pay |
| CycloPropane | Polt-CT-1992 | - | =EOS | - | - | M14 | - | none exp. (book) |  | unknown |
| D4 | Thol-THESIS-2015 | RUB repository (thesis) |  | - | - | M14 | - | ? |  | green |
| D5 | Thol-FPE-2019-siloxanes | 10.1021/acs.iecr.9b00608 (IECR) |  | - | - | M14 | - | exp. |  | pay |
| D6 | Colonna-FPE-2008 | 10.1016/j.fluid.2007.10.001 |  | - | - | M14 | - | ? |  | pay |
| Deuterium | Richardson-JPCRD-2013 | 10.1063/1.4864752 |  | - | - | M12 | Diatschenko-PRB-1985 | exp. |  | pay |
| Dichloroethane | Thol-THESIS-2015 | RUB repository (thesis) |  | - | - | - | - | ? |  | green |
| DiethylEther | Thol-IJT-2014 | 10.1007/s10765-014-1633-1 |  | - | - | M14 | - | exp. |  | pay |
| DimethylCarbonate | Zhou-JPCRD-2011 | 10.1063/1.3664084 |  | - | - | M14 | - | exp. |  | pay |
| DimethylEther | Wu-JPCRD-2011 | 10.1063/1.3582533 |  | Meng-JCED-2012 | - | M12 | - | exp. | ηP* | pay |
| Ethane | Buecker-JPCRD-2006 | 10.1063/1.1859286 |  | Friend-JPCRD-1991 | Friend-JPCRD-1991 | M12 | Buecker-JPCRD-2006 | exp. | ηP λP | pay |
| Ethanol | Schroeder-JPCRD-2014 | 10.1063/1.4895394 |  | Kiselev-IECR-2005 | Assael-JPCRD-2013-Ethanol | M12 | Sun-BBPC-1988 | exp. | ηR λP | pay |
| EthylBenzene | Zhou-JPCRD-2012 | 10.1063/1.3703506 |  | Huber-RP912 | Mylona-JPCRD-2014-xylenes | M14 | - | exp. | η? λR+P | pay |
| Ethylene | Smukala-JPCRD-2000 | 10.1063/1.1329318 |  | - | - | M12 | Smukala-JPCRD-2000 | exp. |  | pay |
| EthyleneOxide | Thol-CES-2015,Thol-CES-2015-CORR,Thol-THESIS-2015 | 10.1016/j.ces.2014.07.051 |  | - | - | - | - | exp. |  | pay |
| Fluorine | deReuck-BOOK-1990 | - | =EOS | - | - | M12 | deReuck-BOOK-1990 | none exp. (book) |  | book |
| HeavyWater | Herrig-JPCRD-2019 | 10.1063/1.5053993 (2018) |  | IAPWS-D2O-2007-Transport | IAPWS-D2O-2007-Transport | IAPWS-SurfaceTension-1994 | Herrig-JPCRD-2019 | IAPWS R16-17 (exp.) | ηP λP mP | IAPWS free |
| Helium | OrtizVega-JPCRD-2019 | 10.6028/NIST.IR.8474 (2023) |  | Arp-NIST-1998 | Hands-CRYO-1981 | M12 | Datchi-PRB-2000 | T3+T4 (verified) | ηP λP | OA (NIST IR) |
| HFE143m | Akasaka-IJR-2012 | 10.1016/j.ijrefrig.2012.01.003 |  | - | - | - | - | exp. |  | pay |
| Hydrogen | Leachman-JPCRD-2009 | 10.1063/1.3160306 |  | Muzny-JCED-2013 | Assael-JPCRD-2011-Hydrogen | M12 | Datchi-PRB-2000 | exp. | ηP λP | pay |
| HydrogenChloride | Thol-JCED-2018-HCl | 10.1021/acs.jced.7b01031 |  | - | - | - | - | exp. |  | pay |
| HydrogenSulfide | Lemmon-JCED-2006 | 10.1021/je050186n |  | QuinonesCisneros-JCED-2012 | - | M12 | - | exp. | ηP | pay |
| IsoButane | Buecker-JPCRD-2006B | 10.1063/1.1901687 |  | Vogel-IJT-2000 | Perkins-JCED-2002-Isobutane | M12 | Buecker-JPCRD-2006B | exp. | ηR λ? | pay |
| IsoButene | Lemmon-FPE-2005 | 10.1016/j.fluid.2004.09.004 |  | - | - | M12 | - | exp. |  | pay |
| Isohexane | Lemmon-JCED-2006 | 10.1021/je050186n |  | - | - | M12 | - | exp. |  | pay |
| Isopentane | Lemmon-JCED-2006 | 10.1021/je050186n |  | Chung-IECR-1988 | Vassiliou-JPCRD-2015-pentanes | M12 | Reeves-JCP-1964 | exp. | λP | pay |
| Krypton | Lemmon-JCED-2006 | 10.1021/je050186n |  | - | - | M12 | Michels-PHYSICA-1962 | exp. |  | pay |
| m-Xylene | Zhou-JPCRD-2012 | 10.1063/1.3703506 |  | Cao-JPCRD-2016-mxylene | Mylona-JPCRD-2014-xylenes | M14 | - | exp. | ηP λR+P | pay |
| MD2M | Thol-JCED-2017-siloxanes | 10.1021/acs.jced.7b00092 |  | - | - | M14 | - | exp. |  | pay |
| MD3M | Thol-FPE-2019-siloxanes | 10.1021/acs.iecr.9b00608 (IECR) |  | - | - | M14 | - | exp. |  | pay |
| MD4M | Thol-FPE-2019-siloxanes | 10.1021/acs.iecr.9b00608 (IECR) |  | - | - | M14 | - | exp. |  | pay |
| MDM | Thol-JCED-2017-siloxanes | 10.1021/acs.jced.7b00092 |  | - | - | M14 | - | exp. |  | pay |
| Methane | Setzmann-JPCRD-1991 | 10.1063/1.555898 |  | QuinonesCisneros-JPCB-2006 | Friend-JPCRD-1989 | M12 | Abramson-HPR-2011 | ? | λP | pay |
| Methanol | deReuck-BOOK-1993 | - |  | Xiang-JPCRD-2006 | Sykioti-JPCRD-2013-Methanol | M12 | deReuck-BOOK-1993 | none exp. (book) | ηP λP | book |
| MethylLinoleate | Huber-EF-2009 | 10.1021/ef900159g |  | - | - | M14 | - | ? |  | pay |
| MethylLinolenate | Huber-EF-2009 | 10.1021/ef900159g |  | - | - | - | - | ? |  | pay |
| MethylOleate | Huber-EF-2009 | 10.1021/ef900159g |  | - | - | M14 | - | ? |  | pay |
| MethylPalmitate | Huber-EF-2009 | 10.1021/ef900159g |  | - | - | M14 | - | ? |  | pay |
| MethylStearate | Huber-EF-2009 | 10.1021/ef900159g |  | - | - | M14 | - | ? |  | pay |
| MM | Thol-FPE-2016-MM,Thol-THESIS-2015 | 10.1016/j.fluid.2015.09.047 |  | - | - | M14 | - | exp. |  | pay |
| n-Butane | Buecker-JPCRD-2006B | 10.1063/1.1901687 |  | Vogel-HTHP-1999 | Perkins-JCED-2002-nButane | M12 | Buecker-JPCRD-2006B | exp. | ηR λ? | pay |
| n-Decane | Lemmon-JCED-2006 | 10.1021/je050186n |  | Huber-FPE-2004 | Huber-FPE-2005 | M12 | - | exp. | ηP λoff | pay |
| n-Dodecane | Lemmon-EF-2004 | 10.1021/ef0341062 |  | Huber-EF-2004 | Huber-EF-2004 | M12 | - | exp. | ηP λP | pay |
| n-Heptane | Span-IJT-2003B | 10.1023/A:1022310214958 | Jaeschke-IJT-1995 | Michailidou-JPCRD-2014-Heptane | Assael-JPCRD-2013-Heptane | M12 | - | ? | ηP λP | pay |
| n-Hexane | Thol-FPE-2019-alkanes-hexane | - |  | Michailidou-JPCRD-2013-Hexane | Assael-JPCRD-2013-Hexane | M12 | - | ? | ηP λP | unknown |
| n-Nonane | Lemmon-JCED-2006 | 10.1021/je050186n |  | Huber-FPE-2004 | Huber-FPE-2005 | M12 | - | exp. | ηP λP | pay |
| n-Octane | Beckmueller-IJT-2019-octane | JPCRD 51:043103 (2022) |  | Huber-FPE-2004 | Huber-FPE-2005 | M12 | - | exp. | ηP λP | unknown |
| n-Pentane | Thol-FPE-2019-alkanes-pentane | - |  | QuinonesCisneros-JPCB-2006 | Vassiliou-JPCRD-2015-pentanes | M12 | Reeves-JCP-1964 | ? | λP | unknown |
| n-Perfluorobutane | Gao-2022-CxFy | 10.1021/acs.iecr.1c02969 | =EOS | - | - | - | - | T14 (tests) | E | pay |
| n-Perfluorohexane | Gao-2022-CxFy | 10.1021/acs.iecr.1c02969 | =EOS | - | - | - | - | T14 (tests) | E | pay |
| n-Perfluoropentane | Gao-2022-CxFy | 10.1021/acs.iecr.1c02969 | =EOS | - | - | - | - | T14 (tests) | E | pay |
| n-Propane | Lemmon-JCED-2009 | 10.1021/je900217v |  | Vogel-JPCRD-1998 | Marsh-JCED-2002 | M12 | Reeves-JCP-1964 | exp. | ηP λR | pay |
| n-Undecane | Aleksandrov-TE-2011 | 10.1134/S0040601511080027 |  | - | - | M14 | - | ? |  | pay |
| Neon | Thol-JPCRD-2019-Neon | - |  | - | - | M12 | Bell-NeonMelting-2026 | UNPUBLISHED? | mS | n/a |
| Neopentane | Lemmon-JCED-2006 | 10.1021/je050186n |  | - | - | M14 | - | exp. |  | pay |
| Nitrogen | Span-JPCRD-2000 | 10.1063/1.1349047 |  | Lemmon-IJT-2004 | Lemmon-IJT-2004 | M12 | Span-JPCRD-2000 | exp. | ηP λP | pay |
| NitrousOxide | Lemmon-JCED-2006 | 10.1021/je050186n |  | - | - | M12 | - | exp. |  | pay |
| Novec649 | McLinden-JCED-2015-Novec649 | 10.1021/acs.jced.5b00623 |  | - | - | - | - | exp. |  | pay |
| o-Xylene | Zhou-JPCRD-2012 | 10.1063/1.3703506 |  | Cao-JPCRD-2016-oxylene | Mylona-JPCRD-2014-xylenes | M14 | - | exp. | ηP λR+P | pay |
| OrthoDeuterium | Richardson-JPCRD-2013 | 10.1063/1.4864752 |  | - | - | - | Diatschenko-PRB-1985 | exp. |  | pay |
| OrthoHydrogen | Leachman-JPCRD-2009 | 10.1063/1.3160306 |  | - | - | - | Datchi-PRB-2000 | exp. |  | pay |
| Oxygen | Schmidt-FPE-1985,Stewart-JPCRD-1991 | 10.1016/0378-3812(85)87016-3 |  | Lemmon-IJT-2004 | Lemmon-IJT-2004 | M12 | Younglove-NIST-1982 | ? | ηP λP | pay |
| p-Xylene | Zhou-JPCRD-2012 | 10.1063/1.3703506 |  | Balogun-JPCRD-2015-pxylene | Mylona-JPCRD-2014-xylenes | M14 | - | exp. | ηP λR+P | pay |
| ParaDeuterium | Richardson-JPCRD-2013 | 10.1063/1.4864752 |  | - | - | - | Diatschenko-PRB-1985 | exp. |  | pay |
| ParaHydrogen | Leachman-JPCRD-2009 | 10.1063/1.3160306 |  | Muzny-JCED-2013 | Assael-JPCRD-2011-Hydrogen | M12 | Younglove-NIST-1982 | exp. | λoff | pay |
| Propylene | Lemmon-PROPYLENE-2013 | - |  | Huber-IECR-2003 | Huber-IECR-2003 | M12 | Reeves-JCP-1964 | UNPUBLISHED |  | n/a |
| PropyleneGlycol | Eisenbach-JPCRD-2021 | 10.1063/5.0050021 | =EOS | - | - | - | - | T8 (tests) | E | pay |
| Propyne | Polt-CT-1992 | - | =EOS | - | - | M12 | - | none exp. (book) |  | unknown |
| R11 | Jacobsen-FPE-1992 | 10.1016/0378-3812(92)87054-Q | =EOS | Klein-IJR-1997 | McLinden-IJR-2000 | M12 | - | ? |  | pay |
| R1123 | Akasaka-IJR-2020-R1123 | 10.1016/j.ijrefrig.2020.07.011 | =EOS | - | - | - | - | T8 (tests) | E | free |
| R113 | Marx-BOOK-1992 | - |  | - | - | M12 | - | none exp. (book) |  | book |
| R1130(E) | Huber-IJT-2025-R1130E | 10.1007/s10765-025-03535-3 | =EOS | - | - | - | - | T4 (tests) | E | OA |
| R1132(E) | Akasaka-IJT-2024-R1132E | 10.1007/s10765-024-03447-8 | =EOS | - | - | - | - | T6 (tests) | E | OA |
| R114 | Platzer-BOOK-1990 | 10.1007/978-3-662-02608-3 | =EOS | - | - | M12 | - | none exp. (book) |  | book |
| R115 | Lemmon-JCED-2016-365227 | 10.1021/acs.jced.5b00684 |  | - | - | - | - | T7 (verified) |  | green |
| R116 | Lemmon-JCED-2006 | 10.1021/je050186n |  | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | exp. |  | pay |
| R12 | Marx-BOOK-1992 | - |  | Klein-IJR-1997 | McLinden-IJR-2000 | M12 | - | none exp. (book) |  | book |
| R1224YDZ | Akasaka-IJT-2023-R1224ydZ | 10.1007/s10765-023-03266-3 | =EOS | - | - | - | - | T7 (tests) | E | pay |
| R123 | Younglove-JPCRD-1994 | 10.1063/1.555950 | =EOS | Tanaka-IJT-1996 | Laesecke-IJR-1996 | M12 | - | none exp. (MBWR) | ηP λP | pay |
| R1233zd(E) | Akasaka-JPCRD-2022-R1233zdE | 10.1063/5.0083026 | =EOS | - | - | - | - | T IX (tests) | E | pay |
| R1234yf | Lemmon-IJT-2022 | 10.1007/s10765-022-03015-y |  | Bell-PURDUE-2016-ETA | Perkins-JCED-2011 | M12 | - | T7 (tests, 6 states) | E λoff | pay |
| R1234ze(E) | Thol-IJT-2016-R1234zeE | 10.1007/s10765-016-2040-6 |  | Bell-PURDUE-2016-ETA | Perkins-JCED-2011 | M14 | - | T3 (verified) | λoff | green |
| R1234ze(Z) | Akasaka-JCED-2019 | 10.1021/acs.jced.9b00007 (OpenAlex) |  | - | - | Kondou-IJR-2015 | - | exp. |  | pay |
| R124 | deVries-ICR-1995 | - |  | Bell-PURDUE-2016-ETA | Huber-IECR-2003 | M12 | - | ? |  | unknown |
| R1243zf | Akasaka-IJT-2025-R1243zf | 10.1007/s10765-024-03481-6 | =EOS | - | - | - | - | T6 (tests) | E | pay |
| R125 | Lemmon-JPCRD-2005 | 10.1063/1.1797813 |  | Huber-IECR-2006 | Perkins-JCED-2006 | M12 | - | exp. | ηP λ? | pay |
| R13 | Platzer-BOOK-1990 | 10.1007/978-3-662-02608-3 | =EOS | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | none exp. (book) |  | book |
| R1336mzz(E) | Akasaka-IJT-2023 | 10.1007/s10765-022-03143-5 |  | - | - | - | - | exp. |  | pay |
| R1336mzz(Z) | McLinden-JCED-2020-R1336mzzZ | 10.1021/acs.jced.9b01198 | =EOS | - | - | - | - | none (tests:4487) +mixIR |  | OA |
| R134a | TillnerRoth-JPCRD-1994 | 10.1063/1.555958 |  | Huber-IECR-2003 | McLinden-IJR-2000 | M12 | - | exp. | ηR λR | pay |
| R13I1 | Lemmon-JCED-2016-365227 | 10.1021/acs.jced.5b00684 |  | - | - | - | - | T7 (verified) |  | green |
| R14 | Platzer-BOOK-1990 | 10.1007/978-3-662-02608-3 | =EOS | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | none exp. (book) |  | book |
| R141b | Lemmon-JCED-2006 | 10.1021/je050186n |  | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | exp. |  | pay |
| R142b | Lemmon-JCED-2006 | 10.1021/je050186n |  | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | exp. |  | pay |
| R143a | LemmonJacobsen-JPCRD-2000 | 10.1063/1.1318909 |  | Klein-IJR-1997 | McLinden-IJR-2000 | M14 | - | exp. |  | pay |
| R152A | Outcalt-JPCRD-1996-R152A | 10.1063/1.555979 |  | Bell-PURDUE-2016-ETA | Krauss-IJT-1996 | M12 | - | none exp. (MBWR) |  | pay |
| R161 | Wu-IJT-2012 | 10.1007/s10765-011-1151-3 |  | - | - | M12 | - | exp. |  | pay |
| R21 | Platzer-BOOK-1990 | 10.1007/978-3-662-02608-3 | =EOS | - | - | M12 | - | none exp. (book) |  | book |
| R218 | Lemmon-JCED-2006 | 10.1021/je050186n |  | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | exp. |  | pay |
| R22 | Kamei-IJT-1995 | 10.1007/BF02081283 |  | Bell-PURDUE-2016-ETA | McLinden-IJR-2000 | M12 | - | ? |  | pay |
| R227EA | Lemmon-JCED-2016-365227 | 10.1021/acs.jced.5b00684 |  | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | T7 (verified) |  | green |
| R23 | Penoncello-JPCRD-2003 | 10.1063/1.1559671 |  | Shan-ASHRAE-2000 | Shan-ASHRAE-2000 | M12 | - | exp. | ηP λP | pay |
| R236EA | Rui-FPE-2013 | 10.1016/j.fluid.2012.12.026 |  | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | ? |  | pay |
| R236FA | Pan-FPE-2012 | 10.1016/j.fluid.2012.02.012 |  | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | ? |  | pay |
| R245ca | Zhou-IJT-2016-R245ca | 10.1007/s10765-016-2039-z |  | - | - | M12 | - | exp. |  | pay |
| R245fa | Akasaka-JPCRD-2015-R245fa | 10.1063/1.4913493 |  | Bell-PURDUE-2016-ETA | Huber-IECR-2003 | M12 | - | exp. |  | pay |
| R32 | TillnerRoth-JPCRD-1997 | 10.1063/1.556002 |  | Bell-PURDUE-2016-ETA | Huber-IECR-2003 | M12 | - | exp. |  | pay |
| R365MFC | Lemmon-JCED-2016-365227 | 10.1021/acs.jced.5b00684 |  | - | - | M12 | - | T7 (verified) |  | green |
| R40 | Thol-IJT-2014 | 10.1007/s10765-014-1633-1 |  | - | - | M14 | - | exp. |  | pay |
| R404A (pp) | Lemmon-IJT-2003 | 10.1023/A:1025048800563 |  | Geller-PURDUE-2000 | Geller-IJT-2001 | Okada-IJT-1999 | - | ? | ηoff λoff | pay |
| R407C (pp) | Lemmon-IJT-2003 | 10.1023/A:1025048800563 |  | Geller-PURDUE-2000 | Geller-IJT-2001 | Okada-IJT-1999 | - | ? | ηoff λoff | pay |
| R41 | Lemmon-JCED-2006 | 10.1021/je050186n |  | - | - | M12 | - | exp. |  | pay |
| R410A (pp) | Lemmon-IJT-2003 | 10.1023/A:1025048800563 |  | Geller-PURDUE-2000 | Geller-IJT-2001 | Okada-IJT-1999 | - | ? | ηoff λoff | pay |
| R507A (pp) | Lemmon-IJT-2003 | 10.1023/A:1025048800563 |  | Geller-PURDUE-2000 | Geller-IJT-2001 | Okada-IJT-1999 | - | ? | ηoff λoff | pay |
| RC318 | Platzer-BOOK-1990 | 10.1007/978-3-662-02608-3 | =EOS | Huber-IECR-2003 | Huber-IECR-2003 | M12 | - | none exp. (book) |  | book |
| SES36 (pp) | Thol-2012 | - |  | - | - | - | - | UNPUBLISHED |  | n/a |
| SulfurDioxide | Gao-JCED-2016 | 10.1021/acs.jced.6b00195 (OpenAlex) |  | - | - | M12 | - | exp. |  | pay |
| SulfurHexafluoride | Guder-JPCRD-2009 | 10.1063/1.3037344 |  | QuinonesCisneros-JPCRD-2012 | Assael-JPCRD-2012-SF6 | M12 | - | exp. | ηP λP | pay |
| Tetrahydrofuran | Fiedler-IJT-2023-THF | 10.1007/s10765-023-03258-3 | =EOS | - | - | - | - | T11 (tests) | E | OA |
| Toluene | Lemmon-JCED-2006 | 10.1021/je050186n |  | Avgeri-JPCRD-2015-Toluene | Assael-JPCRD-2012-Toluene | M12 | - | exp. | ηP λoff | pay |
| trans-2-Butene | Lemmon-FPE-2005 | 10.1016/j.fluid.2004.09.004 |  | - | - | M14 | - | exp. |  | pay |
| VinylChloride | Thol-IJT-2022-VinylChloride | 10.1007/s10765-021-02961-3 | =EOS | - | - | - | - | T5 (tests) | E | OA |
| Water | Wagner-JPCRD-2002 | 10.1063/1.1461829 |  | Huber-JPCRD-2009 | Huber-JPCRD-2012 | M12 | IAPWS-Melting-2011 | IAPWS R6-95 T6+T7 (repro) | ηP λP mP | IAPWS free |
| Xenon | Lemmon-JCED-2006 | 10.1021/je050186n |  | - | - | M12 | - | exp. |  | pay |

**Citations other than the EOS** (η/λ/σ/m/cp0; 78 keys; DOI from the `.bib` unless marked; OA from OpenAlex):

| Key (use) | DOI | OA | Key (use) | DOI | OA |
|---|---|---|---|---|---|
| Abramson-HPR-2011 (m) | 10.1080/08957959.2011.629617 | pay | Arp-NIST-1998 (η) | - | - |
| Assael-JPCRD-2011-Hydrogen (λ) | 10.1063/1.3606499 | pay | Assael-JPCRD-2012-Benzene (λ) | 10.1063/1.4755781 | pay |
| Assael-JPCRD-2012-SF6 (λ) | 10.1063/1.4708620 | pay | Assael-JPCRD-2012-Toluene (λ) | 10.1063/1.3700155 | pay |
| Assael-JPCRD-2013-Ethanol (λ) | 10.1063/1.4797368 | pay | Assael-JPCRD-2013-Heptane (λ) | 10.1063/1.4794091 | pay |
| Assael-JPCRD-2013-Hexane (λ) | 10.1063/1.4793335 | pay | Avgeri-JPCRD-2014-Benzene (η) | 10.1063/1.4892935 | pay |
| Avgeri-JPCRD-2015-Toluene (η) | 10.1063/1.4926955 | pay | Balogun-JPCRD-2015-pxylene (η) | 10.1063/1.4908048 | pay |
| Barreiros-JCT-1982 (m) | 10.1016/0021-9614(82)90044-1 | pay | Bell-NeonMelting-2026 (m) | - | - |
| Bell-PURDUE-2016-ETA (η) | - | Purdue e-Pubs? | Cao-JPCRD-2016-mxylene (η) | 10.1063/1.4941241 | pay |
| Cao-JPCRD-2016-oxylene (η) | 10.1063/1.4945663 | pay | Chung-IECR-1988 (η) | 10.1021/ie00076a024 | pay |
| Datchi-PRB-2000 (m) | 10.1103/PhysRevB.61.6535 | free | Diatschenko-PRB-1985 (m) | 10.1103/PhysRevB.32.381 | pay |
| Fenghour-JPCRD-1995 (η) | 10.1063/1.555961 | pay | Friend-JPCRD-1989 (λ) | 10.1063/1.555828 | pay |
| Friend-JPCRD-1991 (η/λ) | 10.1063/1.555881 | pay | Geller-IJT-2001 (λ) | 10.1023/A:1010691504352 | pay |
| Geller-PURDUE-2000 (η) | - | Purdue e-Pubs? | Hands-CRYO-1981 (λ) | 10.1016/0011-2275(81)90211-3 | pay |
| Huber-EF-2004 (η/λ) | 10.1021/ef034109e | pay | Huber-FPE-2004 (η) | 10.1016/j.fluid.2004.07.012 | pay |
| Huber-FPE-2005 (λ) | 10.1016/j.fluid.2004.10.031 | pay | Huber-IECR-2003 (η/λ) | 10.1021/ie0300880 | pay |
| Huber-IECR-2006 (η) | 10.1021/ie051367l | pay | Huber-JPCRD-2009 (η) | 10.1063/1.3088050 | pay; = IAPWS R12-08, free |
| Huber-JPCRD-2012 (λ) | 10.1063/1.4738955 | pay; = IAPWS R15-11, free | Huber-JPCRD-2016-CO2 (λ) | 10.1063/1.4940892 | green |
| Huber-RP912 (η) | **not in .bib** | - | IAPWS-D2O-2007-Transport (η/λ) | - | IAPWS free |
| IAPWS-Melting-2011 (m) | - | IAPWS free | IAPWS-SurfaceTension-1994 (σ) | - | IAPWS free |
| Jaeschke-IJT-1995 (cp0) | 10.1007/BF02083547 | pay | Kiselev-IECR-2005 (η) | 10.1021/ie050010e | pay |
| Klein-IJR-1997 (η) | 10.1016/S0140-7007(96)00073-4 | pay | Kondou-IJR-2015 (σ) | 10.1016/j.ijrefrig.2015.01.005 | green |
| Krauss-IJT-1996 (λ) | 10.1007/BF01439187 | pay | Laesecke-IJR-1996 (λ) | 10.1016/0140-7007(96)00019-9 | pay |
| Laesecke-JPCRD-2017-CO2 (η) | 10.1063/1.4977429 | green | Lemmon-IJT-2004 (η/λ) | 10.1023/B:IJOT.0000022327.04529.f3 | pay |
| Marsh-JCED-2002 (λ) | 10.1021/je010001m | pay | McLinden-IJR-2000 (λ) | 10.1016/S0140-7007(99)00024-9 | pay |
| Meng-JCED-2012 (η) | 10.1021/je201297j | pay | Michailidou-JPCRD-2013-Hexane (η) | 10.1063/1.4818980 | pay |
| Michailidou-JPCRD-2014-Heptane (η) | 10.1063/1.4875930 | pay | Michels-PHYSICA-1962 (m) | 10.1016/0031-8914(62)90096-4 | pay |
| Mulero-JPCRD-2012 (σ) | 10.1063/1.4768782 | pay | Mulero-JPCRD-2014 (σ) | 10.1063/1.4878755 | pay |
| Muzny-JCED-2013 (η) | 10.1021/je301273j | pay | Mylona-JPCRD-2014-xylenes (λ) | 10.1063/1.4901166 | pay |
| Okada-IJT-1999 (σ) | 10.1023/A:1021482231102 | pay | Penoncello-IJT-1995 (m) | 10.1007/BF01441918 | pay |
| Perkins-JCED-2002-Isobutane (λ) | 10.1021/je010121u | pay | Perkins-JCED-2002-nButane (λ) | 10.1021/je0101202 | pay |
| Perkins-JCED-2006 (λ) | 10.1021/je050372t | pay | Perkins-JCED-2011 (λ) | 10.1021/je200811n | pay |
| QuinonesCisneros-JCED-2012 (η) | 10.1021/je300601h | pay | QuinonesCisneros-JPCB-2006 (η) | 10.1021/jp0618577 | pay |
| QuinonesCisneros-JPCRD-2012 (η) | 10.1063/1.3702441 | pay | Reeves-JCP-1964 (m) | 10.1063/1.1725068 | pay |
| Shan-ASHRAE-2000 (η/λ) | - | - | Sun-BBPC-1988 (m) | 10.1002/bbpc.198800153 | pay |
| Sykioti-JPCRD-2013-Methanol (λ) | 10.1063/1.4829449 | pay | Tanaka-IJT-1996 (η) | 10.1007/BF01443394 | pay |
| Tariq-JPCRD-2014-Cyclohexane (η) | 10.1063/1.4891103 | pay | Tufeu-BBPC-1984 (λ) | 10.1002/bbpc.19840880421 | pay |
| Vassiliou-JPCRD-2015-pentanes (λ) | 10.1063/1.4927095 (OpenAlex) | pay | Vogel-HTHP-1999 (η) | 10.1068/htrt154 | ? |
| Vogel-IJT-2000 (η) | 10.1023/A:1006623310780 | pay | Vogel-JPCRD-1998 (η) | 10.1063/1.556025 | pay |
| Xiang-JPCRD-2006 (η) | 10.1063/1.2360605 | pay | Younglove-NIST-1982 (m) | - | - |

**Statistics** (measured): 87 distinct EOS keys after the 3 composite strings are split (85 distinct strings); 19 have no DOI in the `.bib`. EOS OA status: 101 fluids paywalled, 9 book, 7 green, 6 gold/hybrid, 1 bronze, 2 IAPWS, 1 NIST IR, 6 unknown, 3 no paper. Transport: 66 fluids have η and 63 have λ (oracle key non-empty), with 36 and 31 distinct keys. σ: 108 fluids, 5 keys, 102 of them Mulero (75 × 2012, 27 × 2014). Melting: 30 fluids, 21 keys.

**Fluids with no αʳ arbiter (oracle only), 17 firm:**
- **Book tabulations** (pre-1993 Bender/Schmidt-Wagner forms; no computer-check convention): CycloPropane and Propyne (Polt 1992); R113 and R12 (Marx 1992); R114, R13, R14, R21 and RC318 (Platzer 1990); Fluorine (de Reuck 1990); Methanol (de Reuck 1993; its only cross-check is REFPROP-gated, `CoolProp-Tests.cpp:3361-3375`).
- **MBWR originals** (the published form is MBWR; the JSON is a 40-term Helmholtz transform with d = 0 pairs, doc 02 §3.1): R123 and R152A. The paper's property tables are a weak arbiter.
- **Unpublished:**
  - Propylene: "Personal communication", 2010 (`.bib:1599`).
  - SES36: "Unpublished", 2012 (`.bib:2948`).
  - Neon: "2019, Submitted" (`.bib:3549`); no publication found by OpenAlex search.
- **Stated no table:** R1336mzz(Z) (`CoolProp-Tests.cpp:4484-4490`). It is covered only indirectly by the NIST IR 8570 mixture αʳ rows.
- **Pending the 24 `?` entries**, which may extend this list: D6, the 5 FAMEs (Huber EF 2009), R22, R124, R236EA, R236FA, n-Undecane, Oxygen, Methane, R11, n-Heptane, the 4 Lemmon-2003 blends, n-Hexane, n-Pentane, D4, Dichloroethane and Benzene.

## 5. State, caching, globals, thread-safety, memory

- Citation strings live in each `CoolPropFluid` inside the library singleton. They are loaded eagerly with the fluid (doc 09 §2, doc 11) and immutable afterwards. `get_BibTeXKey` returns a copy. There is no hazard beyond the library-init issues already in doc 11.
- They do not affect any computed number. That makes them metadata: in Rust they belong in datagen/registry data (`&'static str` or a separate `citations` table), not in the hot model struct. The size is negligible (about 165 keys).
- The `.bib` ships in the wheel (doc 09 §4.4) but nothing reads it at runtime. Only the docs script reads it.

## 6. Rot and bugs (table: issue | evidence | impact | Rust remedy)

| # | Issue | Evidence | Impact | Rust remedy |
|---|---|---|---|---|
| R1 | Stored gas constant ≠ paper's | R1234ze(E): JSON R = 8.314472 (`dev/fluids/R1234ze(E).json:4179`); Thol 2016 states 8.3144621. Oracle p is +1.0..1.4e-6 vs the paper's Table 3 at the 5 states with ρ > 0, and ≤ 1.9e-7 after the swap (§3, re-measured). Unchanged on master. **15 audit candidates** (paper ≥ 2012 but R ≤ CODATA-2006): Benzene, Cyclopentane, DiethylEther, Ethanol, EthylBenzene, HFE143m, R1234ze(E), R161, R236EA, R236FA, R40, SES36 and the three xylenes. Only R1234ze(E) is confirmed; the rest are unverified. (Propylene also matches the year heuristic by its key, `-2013`, but the work is the 2010 personal communication.) The heuristic misses the reverse case: Helium stores 8.3144598, while NIST IR 8474 Table 1 states 8.314472 (R3). | About 1e-6 systematic error in every property; fixtures inherit it | Store R, M, T_r and ρ_r with a `source: {doc, table/eq}` field; datagen asserts them against the arbiter record |
| R2 | Stale or placeholder `.bib` entries; 19 EOS keys without a DOI | `Gao-JPCRD-2020` (`.bib:857`): working title, missing author; really JPCRD 52:013102 (2023). `OrtizVega-JPCRD-2019` "Unpublished" (`:2181`); really NIST IR 8474 (2023). These two, plus n-Octane, D4 and Dichloroethane, were fixed upstream in 520b8809. **Still stale on master:** `Thol-FPE-2019-siloxanes` (`:3556`, really IECR 2019, 10.1021/acs.iecr.9b00608); `Akasaka-JCED-2019` "2019, submitted" (`:30`, really 10.1021/acs.jced.9b00007); `Lemmon-JCED-2016-365227` "2016, submitted"; `Herrig-JPCRD-2019` with no DOI (really 2018, 10.1063/1.5053993; CoolProp's own test cites 2018 at `CoolProp-Tests.cpp:4168`); `Thol-JPCRD-2019-Neon` "Submitted". Per 520b8809, D4 and Dichloroethane have published papers (JCED 61:2580, 2016; Mol. Phys. 115:1166, 2017), so their `?` check-table class and their thesis-only "green" OA status in §4 describe the v8.0.0 citation, not the best available arbiter. Key years disagree with the entries (Richardson-…-2013 is year 2014; Thol-FPE-2016-MM is 2015; Gao-2022-CxFy is 2021). | The arbiter cannot be located from the citation | Citation records keyed by DOI (or report number/ISBN), with a CI link check; the key is an opaque id |
| R3 | Helium: stored R ≠ the report's stated R, and the report's check table does not reproduce with its own R | Oracle vs NIST IR 8474 Table 3: −0.9e-7..−4.8e-7 in p, c_v and w (6 states), about 5-6× the last printed c_v digit. Residual coefficients, T_c, ρ_c and M match IR Tables 1-2 term for term (23/23; re-verified, consistent with upstream 520b8809). JSON R = 8.3144598 (`dev/fluids/Helium.json:3639`); IR Table 1 R = 8.314472 "should be used", but with it Table 3 is off by +1.0..1.4e-6. A fit with R = 8.3144621 leaves ≤ 2.7e-7 (§3, measured). | The arbiter is internally inconsistent; the oracle is close to it but not within its printed digits | Record both R values and the measured residual in the divergence registry. The fixture tolerance comes from the measured residual, not the printed digits, until NIST confirms which R generated Table 3 (Q2) |
| R4 | Viscosity arrays: only `front()` is used, yet every entry carries a citation | `FluidLibrary.h:714-718` (front-only is deliberate per the code comment). 7 fluids keep a dead ECS or Krauss entry. Upstream 767b1b3b removed 2 (R1234yf, R1234ze(E), dead `BELL-PERSONAL-2015`); R124, R152A, R22, R245fa and R32 remain. | Low. `get_BibTeXKey` and the docs table report the active front entry correctly. Only readers of the raw JSON can mistake the dead entry for the model in use. On master the arrays are growing (R245fa and R32 now have 3 entries). | One model per property slot; alternates go in a separate, explicitly selected list |
| R5 | Composite, untyped citation strings | `"Thol-CES-2015,Thol-CES-2015-CORR,Thol-THESIS-2015"` (EthyleneOxide); `"Schmidt-FPE-1985,Stewart-JPCRD-1991"` (Oxygen); `"Thol-FPE-2016-MM,Thol-THESIS-2015"` (MM) | Which paper holds the coefficients and which holds the check table is unknown | `Vec<Citation{role: Coefficients\|Erratum\|CheckTable\|Ancillary}>` |
| R6 | Docs generator hides missing keys | `Web/scripts/fluid_properties.PurePseudoPure.py:54-56` prints "problem" and writes `''` | The published table loses citations; the only signal is a stdout line. At v8.0.0 this affects exactly one active citation, EthylBenzene η `Huber-RP912`, since only `Huber-RP912` and the never-loaded `BELL-PERSONAL-2015` are missing from the `.bib` | Datagen fails on an unresolved citation |
| R7 | Defaults that are unpublished or pre-publication EOS | Propylene (2010 personal communication), SES36 (2012 unpublished), Neon (submitted) | No arbiter possible | Flag `provenance: Unpublished` in the registry; oracle-only fixtures with an explicit label |
| R8 | Free arbiters left unused | Water has no EOS check-value test, although IAPWS R6-95 is free and reproduces to 3e-9 (§3). The OA tables for Helium, R1234ze(E) and Lemmon 2016 (4 fluids) are not transcribed. Only 13/136 fluids have paper EOS rows. | Errata like R1 and R3 went unseen | See §8 core set; transcribe every OA table first |

Cross-references, not repeated here: transport R-other rows presented as paper rows, commented-out rows (doc 10 R14), Water σ Mulero vs IAPWS (doc 10 R4), R123 block `Tc` (doc 02 §6), reducing-density errata 2acbbc82 (doc 10 R11), missing `.bib` keys (doc 09 §4.4).

## 7. Parallelism fit

- Arbiter checks are tiny: about 1-20 rows per fluid. They are embarrassingly parallel across fluids and need no concurrency design.
- **Use them as the conformance gate for side-by-side kernels.** K1 rows (α derivatives at one τ, δ) pin scalar, SIMD and future GPU αʳ evaluators to the same printed digits. K2 rows pin the property layer. A SIMD kernel must pass the same K1/K2 rows as the scalar reference before it is accepted.

## 8. Verification assets

**Recommended core fixture set (answers doc 10 §10 Q6).** Ten fluids together cover every residual and ideal-gas term type used by a default EOS, except DoubleExponential (Methanol only, no arbiter) and association (not used by any default EOS). Measured coverage: the union over the 10 is Power, Gaussian, NonAnalytic, GaoB, Exponential and Lemmon2005, plus all 10 α⁰ types except CP0AlyLee.

| # | Fluid | Unique coverage | Arbiter | Status | OA |
|---|---|---|---|---|---|
| 1 | Water | NonAnalytic, Gaussian, PE; also IAPWS η, λ, melting | IAPWS R6-95(2018) K1 + K2 (+ R12-08, R15-11, R14-08) | Verified (values reproduced, ≤ 3e-9) | Free |
| 2 | R1234yf | Gaussian + PE; mixtures (Bell 2022) | Lemmon & Akasaka 2022, Table 7 (6 states) | In tests (`:4399-4474`) | Pay |
| 3 | R1130(E) | Exponential with g ≠ 1 | Huber et al. 2025, Table 4 | In tests (`:4812`) | OA |
| 4 | Helium | α⁰ is Lead + LogTau + EnthalpyEntropyOffset only; quantum fluid; K3 saturation | NIST IR 8474, Tables 3-4 | Verified; **known divergence of unresolved cause (R3)**. Coefficients match the IR; R and the IR's own Table 3 do not | OA |
| 5 | Ammonia | **GaoB** (only fluid), Gaussian | Gao et al., JPCRD 2023 | Expected; obtain the paper | Pay |
| 6 | CarbonDioxide | 2nd NonAnalytic set, EnthalpyEntropyOffset; λ/η paper rows | Span & Wagner 1996 | Expected | Pay |
| 7 | R125 | **Lemmon2005** (only fluid), α⁰ Power | Lemmon & Jacobsen 2005 | Expected | Pay |
| 8 | Air (pp) | **PlanckEinsteinGeneralized**, pseudo-pure path; η, λ rows | Lemmon et al. 2000 | Expected | Pay |
| 9 | Nitrogen | **PlanckEinsteinFunctionT**, α⁰ Power; the 2acbbc82 ρ_r erratum | Span et al. 2000 | Expected | Pay |
| 10 | HFE143m | **CP0Constant + CP0PolyT** | Akasaka et al. 2012 | Expected; alternate R143a (CP0PolyT, Lemmon & Jacobsen 2000) | Pay |

- **Supplement (cheap, verified OA, add early):**
  - R227EA, R365MFC, R115, R13I1: Lemmon 2016 Table 7, 12 states; oracle ≤ 4.3e-7.
  - R1234ze(E): Thol 2016 Table 3; the R1 erratum, a registry exemplar.
  - HeavyWater: IAPWS R16-17 + IAPWS transport + Herrig melting check values in tests.
- **Coverage gaps, flagged:**
  - CP0AlyLee: n-Heptane, D6. Both have `?` check tables. Arbitrate α⁰ from the Jaeschke & Schley 1995 closed-form c_p⁰.
  - DoubleExponential: Methanol, oracle-only.
  - Association: Methanol EOS[1] (Piazza 2013) only. Defer.
- **P0 action:** obtain the paywalled papers for items 5-10 and confirm the table numbers. Transcribe with double entry, record the printed digits, and record the paper's R, M, T_r and ρ_r. If one has no table, swap in its listed alternate.
- **Existing in-repo assets to reuse:**
  - The 13 EOS paper rows (doc 10 §8.1; 67/68 values within half a unit).
  - 318 transport rows. Re-label them by the §4 `tests` column; the R/P*/? rows are not arbiters.
  - Commented-out paper rows that can be revived after a check: Toluene λ (`:369-375`), Benzene λ (`:386-390`), R1234yf/ze(E) λ (`:431-438`), Geller blends η (`:183-198`).
  - D2O melting check values (`:4165-4193`).
- **Oracle hooks:** per-block isolation through `add_fluids_as_JSON` (doc 10 §8.2) lets a gas-constant or coefficient hypothesis be tested in seconds, as in §3.

## 9. Port recommendation (units with priority P0-core / P1-early / P2-later / defer / drop)

| Unit | Priority | Content |
|---|---|---|
| A1 Arbiter registry schema | P0-core | One record per (fluid, part ∈ {αʳ, α⁰, η, λ, σ, melting}). Fields: citation {DOI/report/ISBN, role}, check table {id, kind K1/K2/K3, rows, printed digits}, printed constants {R, M, T_r, ρ_r}, `status` ∈ {transcribed, verified, expected, none, unpublished}, OA link. Seed it from §4; it lives beside the divergence registry (doc 10 §8.3-8.4). |
| A2 Core-10 transcription | P0-core | §8 table. Water (IAPWS) and the two in-test fluids first; this unblocks the EOS-evaluator TDD step (K1 rows). |
| A3 Constants audit | P0-core | Datagen check of R/M/T_r/ρ_r against A1; start with the R1 candidate list |
| A4 Divergence entries | P1-early | R1234ze(E) R; Helium R / IR Table 1 vs Table 3 inconsistency; R123 c_p⁰; 2acbbc82 densities. Each carries the paper value, the oracle value and the decision. |
| A5 OA supplement + 13 existing rows | P1-early | Lemmon 2016 (4 fluids), Thol 2016, the 12 batch rows |
| A6 Remaining `exp.` papers | P2-later | 74 fluids, in user-demand order; mark tables confirmed or absent |
| A7 Transport/σ/melting arbiters | P2-later | Re-provenance the 318 rows; closed-form σ (Mulero T_c) and melting checks |
| A8 Association arbiter | defer | Only when a non-default association EOS is ported |
| A9 Composite citation strings, front-only arrays, docs-blanking script | drop | Replaced by typed `Vec<Citation>` and explicit alternates |

## 10. Open questions

1. **Access.** Can the user (or NIST contacts) supply the 101 paywalled EOS papers, at least core items 5-10? Who does the double-entry transcription?
2. **Helium.** The term-by-term comparison is now done: the coefficients are identical. The open question is narrower. Which R should the Rust default use: the IR Table 1 value (8.314472, which the IR says to use but which fits its own Table 3 worse), or CoolProp's 8.3144598? And which tolerance should the Table 3 fixture carry? Asking the authors (NIST) which R generated Table 3 would settle it.
3. **Unpublished defaults** (Propylene, SES36, Neon). Keep them as explicitly labelled oracle-only fluids, or replace them with published EOS where one exists?
4. **R audit.** Are any of the 14 other R1 candidates also wrong? Confirming needs each paper's stated R. Also audit the reverse case, where the stored R is newer than the paper's (Helium).
5. **Indirect arbiters.** Do mixture αʳ tables (Bell 2022/2023, NIST IR 8570) count as pure-fluid arbiters (R1336mzz(Z))? *Inference:* only weakly, because a mixture value cannot isolate a pure-fluid term error.
6. **Licensing.** May printed check values be committed as fixtures? This is doc 10 Q1(c); numerical facts are probably fine, but confirm.

## Verification log

**2026-10-04, adversarial verification.** About 95 claims were checked against the v8.0.0 checkout (ae81610e), the oracle (CoolProp==8.0.0), OpenAlex, and the OA full texts (NIST IR 8474 PDF; PMC manuscripts of Thol 2016 and Lemmon 2016).

**Confirmed:**
- **Counts.**
  - 136 fluids, of which 6 are pseudo-pure.
  - `.bib` has 3,622 lines; the docs script has 96.
  - 111 empty `BibTeX_CP0`; n-Heptane is the only one with a separate c_p⁰ key.
  - 7 front-only viscosity arrays.
  - 19 EOS keys with no DOI.
  - η: 66 fluids / 36 keys. λ: 63 fluids / 31 keys. σ: 108 fluids / 5 keys. Melting: 30 fluids / 21 keys.
  - 318 active transport rows (177 η + 141 λ).
  - 13 fluids with paper EOS rows (R1234yf with 6 states, plus 12 fluids with 1 row each).
  - Check-class counts 13/6/2/74/24/14/3 and the OA-status counts.
- **All 136 rows of §4.** The EOS, CP0, η, λ, σ and melting keys were compared field by field against the JSON: 0 mismatches. The oracle `get_BibTeXKey` also agrees with the JSON for all 136 fluids × 6 keys.
- **Code locations.** All cited loader lines and test ranges were opened and confirmed: `FluidLibrary.h:389-390,715,721,944,1017`; `FluidLibraryFactories.h:34`; the docs script `:49-58,54-56,85-89`; and in `CoolProp-Tests.cpp` `:44-301`, `:347-581`, `:183-198`, `:369-375`, `:386-390`, `:408`, `:431-438`, `:3361-3375`, `:4165-4193`, `:4399-4474`, `:4471`, `:4484-4500`, `:4787-4840`, `:4812`. Also `Ancillaries.cpp:282-312`.
- **`.bib` lines.** Lines 30, 857, 1599, 2181, 2948, 3549 and 3556 are as described.
- **Upstream commits.** 520b8809, 767b1b3b and 2acbbc82 are all after v8.0.0. The entries R2 calls stale are still stale on master.
- **Arbiter measurements.**
  - Water: IAPWS-95 Table 6 reproduced (≤ 2.9e-9).
  - Lemmon 2016 Table 7: 12 states reproduced, ≤ 4.3e-7.
  - R1234ze(E): reproduced, including the R quote from the paper and R = 8.314472 at `:4179`, still on master.
- **Core-10 coverage.** Recomputed. It covers every default αʳ type except DoubleExponential (Methanol) and every α⁰ type except CP0AlyLee (n-Heptane, D6). Association appears only in Methanol EOS[1].
- **R1 candidate list.** Reproduced: 15, plus Propylene by key year.

**Corrections made:**
1. **Helium (§3.4, R3, §8 #4, A4, Q2): the inference was refuted.** NIST IR 8474 Table 2 matches the JSON term for term (23/23), and T_c, ρ_c and M match too. So "the residual coefficients, not R, differ" is false, and upstream 520b8809 is right. The divergence comes from R: the JSON stores 8.3144598, while IR Table 1 gives 8.314472. The IR's own Table 3 also does not reproduce with its Table 1 R. R3 was rewritten as a divergence of unresolved cause.
2. **Accessor location.** It is `HelmholtzEOSMixtureBackend.cpp:263-289`, inside `fluid_param_string`, not a function named `get_BibTeXKey`. The Python `get_BibTeXKey` lives in nanobind/pyx. `CoolProp.cpp:1147-1152` is a test listing the accepted strings.
3. **σ Mulero count.** 102 (75 + 27), not 97.
4. **Distinct EOS keys.** 87 is the count after splitting composite strings; there are 85 distinct strings.
5. **R1234ze(E).** The p error holds at the 5 states with ρ > 0, not "every/all 6" (the 6th row has p = 0).
6. **§4 tests column.**
   - R1132(E): E was missing; it is in the batch test at `:4799`.
   - R404A, R407C, R410A, R507A: added ηoff λoff.
   - n-Decane: added λoff.
   - EthylBenzene: η changed from P to ?, because the row cites Mylona 2014, a λ paper, against the ECS `Huber-RP912` model.
7. **DOI provenance.** R1234ze(Z) and SulfurDioxide DOIs are not in the `.bib`; they are now marked (OpenAlex).
8. **Helium α⁰.** It also has EnthalpyEntropyOffset, not "Lead + LogTau only".

**Rot items after the refutation attempt:**
- **R1 survives.** Added the reverse case (Helium).
- **R2 survives.** Added a note that 520b8809 found published papers for D4 and Dichloroethane.
- **R3 reframed** as above.
- **R4 kept, impact lowered.** Front-only loading is deliberate (code comment), and the API and docs report the active entry correctly.
- **R5 survives.**
- **R6 kept, impact made precise.** The script does print "problem", and at v8.0.0 exactly one active key (`Huber-RP912`) is blanked.
- **R7 survives.**
- **R8 survives.** A repo-wide grep found no IAPWS-95, Helium or Lemmon-2016 check values.
