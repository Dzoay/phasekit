//! Each default EOS mapped into core's record (PLAN.md M2.3): every JSON quirk resolved, every value validated, the
//! alternates and the unported kinds skipped explicitly, and the published constants the oracle's.

use super::*;

/// PLAN.md M2.3: all 136 default EOS map into core records without error, the names kept.
#[test]
fn all_136_fluids_map_to_records() {
    let records = records();
    assert_eq!(records.len(), 136);
    assert!(records.iter().all(|(s, r)| r.name == s.fluid.info.name));
    let terms: usize = records.iter().map(|(_, r)| r.eos.power.len() + r.eos.gaussian.len()).sum();
    assert!(terms > 2000, "{terms}");
}

/// Map 02 §3.1 "0 means absent": a Power term has an exponential exactly when `l > 0` (`c` 1 or 0); an
/// Exponential term keeps `c = g`, and its 17 `l = 0` terms all have `g = 0` (a nonzero `g` there is refused:
/// CoolProp would drop e^(−g)); R125's Lemmon2005 terms keep their 15 `m = 0` and 5 `l = 0`.
#[test]
fn zero_means_absent() {
    let records = records();
    let power: Vec<_> = records.iter().flat_map(|(_, r)| &r.eos.power).collect();
    assert!(power.iter().all(|p| (p.l == 0) == (p.c == 0.0)));
    let r125 = &records.iter().find(|(_, r)| r.name == "R125").unwrap().1.eos.lemmon2005;
    let zeros = (r125.iter().filter(|t| t.m == 0.0).count(), r125.iter().filter(|t| t.l == 0).count());
    assert_eq!((r125.len(), zeros), (18, (15, 5)));
    let exponential: Vec<(u8, f64)> = sources()
        .iter()
        .flat_map(|s| &s.fluid.eos[0].alphar)
        .filter_map(|b| match b {
            ResidualBlock::Exponential { l, g, .. } => Some(l.iter().zip(g).map(|(l, g)| (l.value() as u8, *g))),
            _ => None,
        })
        .flatten()
        .filter(|(l, _)| *l == 0)
        .collect();
    assert_eq!((exponential.len(), exponential.iter().all(|(_, g)| *g == 0.0)), (17, true));
    let r13 = edited("R13.json", |v| v["EOS"][0]["alphar"][1]["l"][0] = serde_json::json!(0));
    let err = record_of("R13.json", &r13).unwrap_err();
    assert!(err.contains("EOS[0].alphar[1]: term 0: l = 0 with g = 0.98230055"), "{err}");
}

/// Map 02 §3.1: Ammonia's two GaoB terms carry the paper's η (+2.8452, +2.8342); the JSON stores −η.
#[test]
fn gaob_eta_sign_is_flipped_to_the_paper() {
    let records = records();
    let gao: Vec<_> = records.iter().flat_map(|(_, r)| r.eos.gao_b.iter().map(|t| (r.name.as_str(), t.eta))).collect();
    assert_eq!(gao, [("Ammonia", 2.8452), ("Ammonia", 2.8342)]);
}

/// Map 02 §3.2: every Planck-Einstein θ is positive, as papers print it, including the PlanckEinsteinFunctionT
/// terms (θ = v/T_crit; Nitrogen 3364.011/126.192); Air's generalized term keeps CoolProp's θ = +87.31279 with
/// c = 2/3, d = 1. n-Heptane's two Aly-Lee blocks convert as CoolProp converts them, skipping zero constants: the
/// first (A, B, D nonzero) gives a c_p⁰ term and two generalized terms, the second (only B) one generalized term.
#[test]
fn planck_einstein_theta_sign_normalised() {
    let records = records();
    let ideal = |name: &str| records.iter().find(|(_, r)| r.name == name).unwrap().1.eos.ideal.clone();
    let thetas: Vec<f64> = records
        .iter()
        .flat_map(|(_, r)| &r.eos.ideal)
        .filter_map(|t| match t {
            IdealTerm::PlanckEinstein { theta, .. } => Some(*theta),
            _ => None,
        })
        .collect();
    assert!(thetas.len() > 370 && thetas.iter().all(|&t| t > 0.0), "{}", thetas.len());
    assert!(ideal("Nitrogen").contains(&IdealTerm::PlanckEinstein { n: 1.012941, theta: 3364.011 / 126.192 }));
    let air = IdealTerm::PlanckEinsteinGeneralized { n: -0.197938904, theta: 87.31279, c: 2.0 / 3.0, d: 1.0 };
    assert!(ideal("Air").contains(&air));
    let heptane = ideal("n-Heptane");
    let generalized = heptane.iter().filter(|t| matches!(t, IdealTerm::PlanckEinsteinGeneralized { .. })).count();
    let cp0 = heptane.iter().filter(|t| matches!(t, IdealTerm::Cp0Power { t: 0.0, .. })).count();
    assert_eq!((generalized, cp0), (3, 1));
    assert!(heptane.contains(&IdealTerm::Cp0Power { c: 4.0, t: 0.0, tc: 540.13, t0: 371.533277446 }));
    let sinh = IdealTerm::PlanckEinsteinGeneralized { n: 43.5561, theta: -2.0 * 1760.46 / 540.13, c: 1.0, d: -1.0 };
    let cosh = IdealTerm::PlanckEinsteinGeneralized { n: -30.4707, theta: -2.0 * 836.195 / 540.13, c: 1.0, d: 1.0 };
    assert!(heptane.contains(&sinh) && heptane.contains(&cosh));
}

/// Map 02 §6: R123's c_p⁰ blocks (CP0Constant and a three-term CP0PolyT) are written with Tc = 456.82 K while its
/// T_r is 456.831 K; the record keeps both, as CoolProp evaluates them (Parity). Every other fluid's c_p⁰ terms use
/// its T_r.
#[test]
fn cp0_block_tc_differs_from_tr() {
    let records = records();
    let apart: Vec<(&str, f64, f64)> = records
        .iter()
        .flat_map(|(_, r)| r.eos.ideal.iter().map(move |t| (r, t)))
        .filter_map(|(r, t)| match *t {
            IdealTerm::Cp0Power { tc, .. } if tc != r.eos.t_reducing => Some((r.name.as_str(), tc, r.eos.t_reducing)),
            _ => None,
        })
        .collect();
    assert_eq!(apart, [("R123", 456.82, 456.831); 4]);
}

/// Map 02 §3.1: every `d` and `l` is an integer within `MAX_POW` (the data's largest are 15 and 6); an exponent
/// beyond it, a negative one or a fractional one is refused.
#[test]
fn exponents_within_max_pow() {
    let records = records();
    let power = records.iter().flat_map(|(_, r)| &r.eos.power);
    let (d, l) = power.fold((0, 0), |(d, l), p| (p.d.max(d), p.l.max(l)));
    assert_eq!((d, l), (15, 6));
    for (bad, why) in [(17.0, "exponent 17 is not an integer in 0..=16"), (-1.0, "exponent -1"), (1.5, "exponent 1.5")]
    {
        let nitrogen = edited("Nitrogen.json", |v| v["EOS"][0]["alphar"][0]["d"][0] = serde_json::json!(bad));
        let err = record_of("Nitrogen.json", &nitrogen).unwrap_err();
        assert!(err.contains("EOS[0].alphar[0]: ") && err.contains(why), "{bad}: {err}");
    }
}

/// Map 10 R8: a block whose lists differ in length is an error (CoolProp only `assert`s it, compiled out).
#[test]
fn unequal_lengths_are_errors() {
    let push = |v: &mut serde_json::Value, path: [&str; 2], x: f64| {
        let list = &mut v["EOS"][0]["alphar"][path[0].parse::<usize>().unwrap_or_default()][path[1]];
        list.as_array_mut().unwrap().push(serde_json::json!(x));
    };
    let nitrogen = edited("Nitrogen.json", |v| push(v, ["0", "n"], 1.0));
    let err = record_of("Nitrogen.json", &nitrogen).unwrap_err();
    assert!(err.contains("EOS[0].alphar[0]: t has 32 entries, n has 33"), "{err}");
    let methanol = edited("Methanol.json", |v| {
        let i = v["EOS"][0]["alphar"]
            .as_array()
            .unwrap()
            .iter()
            .position(|b| b["type"] == "ResidualHelmholtzDoubleExponential");
        push(v, [&i.unwrap().to_string(), "gt"], -1.0);
    });
    let err = record_of("Methanol.json", &methanol).unwrap_err();
    assert!(err.contains("gt has 9 entries, n has 8"), "{err}");
}

/// Rot: ROT-046. CoolProp's T_min (the saturation minimum) and the triple point (`EOS.Ttriple`, never read by
/// CoolProp) stay apart: they differ beyond 1e-9 in 16 fluids (map 09 R8), e.g. R114 273.15 vs 180.63 K and
/// CycloPropane 273 vs 145.7 K.
#[test]
fn t_min_and_t_triple_kept_apart() {
    let records = records();
    let mut differ = Vec::new();
    for (source, record) in &records {
        let eos = &source.fluid.eos[0];
        assert_eq!(record.limits.t_triple(), Some(eos.t_triple), "{}", record.name);
        assert_eq!(record.limits.t_min(), eos.states.sat_min_liquid.t, "{}", record.name);
        if (eos.t_triple / eos.states.sat_min_liquid.t - 1.0).abs() > 1e-9 {
            differ.push(record.name.as_str());
        }
    }
    let want = [
        "Ammonia",
        "CycloPropane",
        "DiethylEther",
        "Ethanol",
        "MD4M",
        "MethylLinoleate",
        "MethylLinolenate",
        "Neon",
        "Propyne",
        "R114",
        "R124",
        "R13",
        "R14",
        "R21",
        "R236EA",
        "R40",
    ];
    assert_eq!(differ, want);
    let r114 = &records.iter().find(|(_, r)| r.name == "R114").unwrap().1.limits;
    assert_eq!((r114.t_min(), r114.t_triple()), (273.15000000000003, Some(180.63)));
}

/// Rot: ROT-041. Only the default EOS is mapped; the 23 alternates are listed, one line each, never compiled.
#[test]
fn alternate_eos_entries_are_skipped_explicitly() {
    let sources = sources();
    let skipped: Vec<String> = sources.iter().flat_map(record::skipped).collect();
    assert_eq!(skipped.len(), 23);
    assert!(skipped.contains(&"R1234yf.json: EOS[1] (Richter-JCED-2011) is an alternate, not mapped".to_string()));
    let methanol = sources.iter().find(|s| s.file == "Methanol.json").unwrap();
    let mapped = record::to_record(methanol).unwrap();
    assert_eq!(mapped.source.bibkey.as_ref(), methanol.fluid.eos[0].bibtex_eos);
    assert_ne!(methanol.fluid.eos[1].bibtex_eos, methanol.fluid.eos[0].bibtex_eos);
}

/// Rot: ROT-068. The association term is not ported: Methanol's alternate carries the only one (skipped with
/// its alternate); in a default EOS it is refused.
#[test]
fn associating_blocks_are_skipped_explicitly() {
    let methanol = text("Methanol.json");
    let source = parse("Methanol.json", &methanol).unwrap();
    let associating = |eos: &mirror::Eos| eos.alphar.iter().any(|b| matches!(b, ResidualBlock::Associating { .. }));
    assert_eq!(source.fluid.eos.iter().map(associating).collect::<Vec<_>>(), [false, true]);
    assert!(record::to_record(&source).is_ok());
    let edited = methanol.replacen("\"ResidualHelmholtzDoubleExponential\"", "\"ResidualHelmholtzAssociating\"", 1);
    let block = parse("Methanol.json", &edited);
    assert!(block.is_err(), "the DoubleExponential fields do not fit an associating block");
    let assoc = r#"{"a": 1, "epsilonbar": 12.0, "kappabar": 0.001, "m": 1.0, "type": "ResidualHelmholtzAssociating", "vbarn": 0.2}"#;
    let first = methanol.find("\"alphar\": [").unwrap() + 11;
    let edited = format!("{}{assoc}, {}", &methanol[..first], &methanol[first..]);
    let err = record_of("Methanol.json", &edited).unwrap_err();
    assert!(err.contains("EOS[0].alphar[0]: ResidualHelmholtzAssociating is not ported (ROT-068"), "{err}");
}

/// Rot: ROT-057 (datagen half). No record carries a metadata sentinel: REFPROP_NAME "N/A" (9 fluids), ODP and GWP
/// −1 or ±10^n, NFPA ratings outside 0-4, ASHRAE 34 "UNKNOWN" or "?" are all `None`; the environmental block is
/// restricted data.
#[test]
fn no_metadata_sentinels() {
    let records = records();
    assert_eq!(records.iter().filter(|(_, r)| r.refprop_name.is_none()).count(), 9);
    assert!(records.iter().all(|(_, r)| r.refprop_name.as_deref() != Some("N/A")));
    let envs: Vec<_> = records.iter().filter_map(|(_, r)| r.environmental.as_ref()).collect();
    assert_eq!(envs.len(), 125);
    for env in &envs {
        assert_eq!(env.source.terms, phasekit_core::DataTerms::Restricted);
        assert!([env.gwp20, env.gwp100, env.gwp500, env.odp].iter().flatten().all(|x| *x >= 0.0));
        assert!([env.health, env.flammability, env.physical].iter().flatten().all(|x| *x <= 4));
        assert!(env.ashrae34.as_deref().is_none_or(|c| c != "UNKNOWN" && c != "?"));
    }
    let counted = |f: fn(&Environmental) -> bool| envs.iter().filter(|e| f(e)).count();
    assert_eq!(counted(|e| e.odp.is_none()), 100);
    assert_eq!(counted(|e| e.gwp100.is_none()), 59);
    assert_eq!(counted(|e| e.ashrae34.is_none()), 80);
    assert_eq!(counted(|e| e.physical.is_none()), 33);
    let r134a = &records.iter().find(|(_, r)| r.name == "R134a").unwrap().1;
    assert_eq!(r134a.refprop_name.as_deref(), Some("R134A"));
    assert_eq!(r134a.environmental.as_ref().and_then(|e| e.ashrae34.as_deref()), Some("A1"));
}

/// Rot: ROT-052. Derived states are never mapped: no `hmolar` or `smolar` of any stored state (fossil in 57
/// fluids, map 10 R11) appears in a record's canonical EOS bytes, the bytes the blob's EOS section holds (M2.4).
#[test]
fn no_derived_hs_in_blobs() {
    for (source, record) in records() {
        let mut bytes = Vec::new();
        record.eos.encode(&mut bytes);
        let eos = &source.fluid.eos[0];
        let s = &eos.states;
        let points = [&s.reducing, &s.sat_min_liquid, &s.sat_min_vapor, &source.fluid.states.critical];
        for value in points.iter().flat_map(|p| [p.hmolar, p.smolar]).flatten().filter(|v| *v != 0.0) {
            let pattern = value.to_le_bytes();
            assert!(!bytes.windows(8).any(|w| w == pattern), "{}: {value} in the EOS bytes", record.name);
        }
    }
}

/// Map 09 §9 D2: every `*_units` field is checked, then dropped: an EOS constant's, a stored state's and a
/// PlanckEinsteinFunctionT block's.
#[test]
fn units_are_checked_then_dropped() {
    let cases: [(&str, Edit, &str); 3] = [
        ("Nitrogen.json", |v| v["EOS"][0]["molar_mass_units"] = "g/mol".into(), "molar_mass_units is \"g/mol\""),
        ("Nitrogen.json", |v| v["STATES"]["critical"]["p_units"] = "kPa".into(), "p_units is \"kPa\", not \"Pa\""),
        (
            "Methanol.json",
            |v| {
                let alpha0 = v["EOS"][0]["alpha0"].as_array_mut().unwrap();
                let block =
                    alpha0.iter_mut().find(|b| b["type"] == "IdealGasHelmholtzPlanckEinsteinFunctionT").unwrap();
                block["Tcrit_units"] = "C".into();
            },
            "Tcrit_units is \"C\"",
        ),
    ];
    for (file, edit, why) in cases {
        assert!(record_of(file, &text(file)).is_ok());
        let err = record_of(file, &edited(file, edit)).unwrap_err();
        assert!(err.contains(why), "{err}");
    }
}

/// The constants are finite and positive, and the conversions divide only by a positive T_crit or Tc.
#[test]
fn constants_and_divisors_must_be_positive() {
    let cases: [(&str, Edit, &str); 4] = [
        ("Nitrogen.json", |v| v["EOS"][0]["molar_mass"] = 0.0.into(), "molar_mass = 0 is not finite and positive"),
        ("Nitrogen.json", |v| v["EOS"][0]["gas_constant"] = (-8.3).into(), "gas_constant = -8.3 is not"),
        ("Nitrogen.json", |v| v["EOS"][0]["alpha0"][3]["Tcrit"] = 0.0.into(), "Tcrit = 0 is not positive"),
        ("n-Heptane.json", |v| (2..4).for_each(|i| v["EOS"][0]["alpha0"][i]["Tc"] = 0.0.into()), "Tc = 0 is not"),
    ];
    for (file, edit, why) in cases {
        let err = record_of(file, &edited(file, edit)).unwrap_err();
        assert!(err.contains(why), "{err}");
    }
}

/// `FluidLibrary.h:288-310`: an Aly-Lee constant at or below 1e-14 in magnitude adds no term; above it, its term.
#[test]
fn aly_lee_skips_constants_at_or_below_1e_14() {
    let ideal = |constants: [f64; 5]| {
        let file = edited("n-Heptane.json", |v| {
            v["EOS"][0]["alpha0"][2]["c"] = serde_json::json!(constants);
            v["EOS"][0]["alpha0"].as_array_mut().unwrap().truncate(3);
        });
        record_of("n-Heptane.json", &file).unwrap().eos.ideal
    };
    let lead = ideal([0.0; 5]).len();
    assert_eq!(ideal([1e-14, -1e-14, 1.0, 1e-14, 1.0]).len(), lead);
    let added = ideal([2e-14, -2e-14, 1.0, 2e-14, 1.0]);
    let (tc, t0) = (540.13, 371.533277446);
    assert_eq!(
        added[lead..],
        [
            IdealTerm::Cp0Power { c: 2e-14, t: 0.0, tc, t0 },
            IdealTerm::PlanckEinsteinGeneralized { n: -2e-14, theta: -2.0 / tc, c: 1.0, d: -1.0 },
            IdealTerm::PlanckEinsteinGeneralized { n: -2e-14, theta: -2.0 / tc, c: 1.0, d: 1.0 },
        ]
    );
}

/// An ASHRAE 34 class is a letter A or B and a digit (with an optional L), kept as written; anything else that is
/// not a sentinel is refused.
#[test]
fn ashrae_classes_are_checked() {
    let class = |c: &'static str| {
        let file = edited("R134a.json", |v| v["INFO"]["ENVIRONMENTAL"]["ASHRAE34"] = c.into());
        record_of("R134a.json", &file).map(|r| r.environmental.and_then(|e| e.ashrae34))
    };
    assert_eq!(class("A2L"), Ok(Some("A2L".into())));
    assert_eq!(class("?"), Ok(None));
    for bad in ["Z1", "A2LX", "A", "B9"] {
        assert!(class(bad).unwrap_err().contains(&format!("unknown class {bad:?}")), "{bad}");
    }
}

/// A committed oracle fixture's text.
fn oracle_fixture(rel: &str) -> String {
    Repo::locate().read(&format!("crates/phasekit-verify/fixtures/coolprop-8.0.0/{rel}")).unwrap()
}

/// The published constants a record holds, in the `crit` kind's column order (VERIFICATION.md §3.5) with
/// CoolProp's `Ttriple` being its T_min (map 09 R8): `(column, value)`.
fn published(record: &FluidRecord) -> Vec<(&'static str, f64)> {
    let c = record.critical.unwrap();
    let l = &record.limits;
    vec![
        ("Tc_pub", c.t),
        ("pc_pub", c.p),
        ("rhoc_pub", c.rho),
        ("Ttriple", l.t_min()),
        ("Tmin", l.t_min()),
        ("Tmax", l.t_max()),
        ("pmax", l.p_max()),
        ("M", record.molar_mass),
        ("R", record.eos.gas_constant),
    ]
}

/// PLAN.md M2.3, class `Exact`: every record's published constants equal the oracle's `crit` row bit for bit, on
/// the all-fluid tier (136 rows) and the core subset (14 files, whose rows equal their all-tier rows). The
/// numerical critical point and `ptriple` are EOS solutions, checked from M6 on.
#[test]
fn published_constants_match_the_oracle_crit_rows() {
    let records = records();
    let all = oracle_fixture("all/crit.csv");
    let all = phasekit_verify::Fixture::parse("all/crit.csv", &all).unwrap();
    assert_eq!((all.kind(), all.rows().len()), ("crit", 136));
    let row_of = |name: &str| (0..136).find(|&i| all.printed(i, "fluid") == Some(name));
    for (_, record) in &records {
        let row = row_of(&record.name).unwrap_or_else(|| panic!("{}: no crit row", record.name));
        for (column, value) in published(record) {
            assert!(
                all.check(row, column, value).is_ok(),
                "{} {column}: {value} vs {:?}",
                record.name,
                all.printed(row, column)
            );
        }
    }
    let core: Vec<&str> = ["Air", "Ammonia", "CarbonDioxide", "HFE143m", "Helium", "Methanol", "Nitrogen"]
        .into_iter()
        .chain(["R1130(E)", "R1234yf", "R1234ze(E)", "R125", "R410A", "Water", "n-Heptane"])
        .collect();
    for name in core {
        let text = oracle_fixture(&format!("crit/{name}.csv"));
        let file = phasekit_verify::Fixture::parse(name, &text).unwrap();
        let row = row_of(name).unwrap();
        for column in file.columns() {
            assert_eq!(file.printed(0, column), all.printed(row, column), "{name} {column}");
        }
    }
}
