//! The pinned fluid files as datagen reads them (PLAN.md M2.1, M2.2): the lock first, then each file into the closed
//! mirror, its literal kinds kept and its quirks logged or refused, and every stored superancillary stamp and check
//! point recomputed.

use super::*;
use serde_json::json;

/// PLAN.md M2.1: every v8.0.0 fluid parses into the closed mirror, after the lock check. 136 files, 136 distinct
/// `INFO.NAME`s and 159 EOS entries (113 fluids with one, 23 with an alternate; map 09 §4.1).
#[test]
fn all_136_fluids_parse() {
    let sources = sources();
    assert_eq!(sources.len(), 136);
    let mut names: Vec<&str> = sources.iter().map(|s| s.fluid.info.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 136);
    assert_eq!(sources.iter().map(|s| s.fluid.eos.len()).sum::<usize>(), 159);
    let renamed: Vec<(&str, &str)> = sources
        .iter()
        .filter(|s| s.file.trim_end_matches(".json") != s.fluid.info.name)
        .map(|s| (s.file.as_str(), s.fluid.info.name.as_str()))
        .collect();
    assert_eq!(renamed, vec![("R1224yd(Z).json", "R1224YDZ")]);
    assert_eq!(sources.iter().filter(|s| s.fluid.eos[0].superancillary.is_some()).count(), 130);
}

/// Rot: ROT-058. Chlorine's `source_eos_hash` appears twice with equal values (map 09 R19): parsed under a
/// logged waiver. The same duplicate anywhere else, or with unequal values, is an error.
#[test]
fn chlorine_duplicate_key_is_a_logged_waiver() {
    let chlorine = parse("Chlorine.json", &text("Chlorine.json")).unwrap();
    assert_eq!(
        chlorine.waivers,
        vec!["Chlorine.json: duplicate key EOS[0].SUPERANCILLARY.source_eos_hash with equal values (map 09 R19)"]
    );
    let sources = sources();
    let others: Vec<&String> = sources.iter().filter(|s| s.file != "Chlorine.json").flat_map(|s| &s.waivers).collect();
    assert!(others.is_empty(), "{others:?}");

    let unequal = text("Chlorine.json").replacen("\"3144edae6b63f0b5\"", "\"3144edae6b63f0b6\"", 1);
    let err = parse("Chlorine.json", &unequal).unwrap_err();
    assert!(err.contains("duplicate key EOS[0].SUPERANCILLARY.source_eos_hash"), "{err}");
    let nitrogen =
        text("Nitrogen.json").replacen("\"source_eos_hash\":", "\"source_eos_hash\": \"x\", \"source_eos_hash\":", 1);
    let err = parse("Nitrogen.json", &nitrogen).unwrap_err();
    assert!(err.contains("Nitrogen.json: duplicate key EOS[0].SUPERANCILLARY.source_eos_hash"), "{err}");
}

/// Rot: ROT-038. An unknown residual or ideal-gas block type, a misspelt key and a missing key are parse errors
/// (CoolProp prints and skips an unknown α⁰ type, `FluidLibrary.h:327-329`; map 10 R8, map 09 R7).
#[test]
fn unknown_block_type_is_an_error() {
    let nitrogen = text("Nitrogen.json");
    assert!(parse("Nitrogen.json", &nitrogen).is_ok());
    let cases = [
        ("\"ResidualHelmholtzGaussian\"", "\"ResidualHelmholtzGaussianX\"", "unknown variant"),
        ("\"IdealGasHelmholtzLogTau\"", "\"IdealGasHelmholtzLogTauX\"", "unknown variant"),
        ("\"epsilon\":", "\"epsilom\":", "unknown field `epsilom`"),
        ("\"gas_constant\":", "\"gas_konstant\":", "unknown field `gas_konstant`"),
    ];
    for (from, to, why) in cases {
        assert!(nitrogen.contains(from), "{from}");
        let err = parse("Nitrogen.json", &nitrogen.replacen(from, to, 1)).unwrap_err();
        assert!(err.starts_with("Nitrogen.json: ") && err.contains(why), "{from} -> {to}: {err}");
    }

    // Rot: ROT-116 (map 05 R9). The transport forms are closed too (M8.1): an unknown `type`, a hardcoded name of
    // another slot, CoolProp's dead terms (CO₂'s Vesovic dilute term, its `RESIDUAL_CO2`, which parses and then throws,
    // and Scalabrin's critical term) and a misspelt key are refused.
    let transport_cases = [
        (
            "Nitrogen.json",
            "/viscosity/dilute/type",
            json!("collision_integrals"),
            "dilute: unknown type \"collision_integrals\"",
        ),
        ("R11.json", "/viscosity/type", json!("ECSX"), "viscosity: unknown type \"ECSX\""),
        ("Water.json", "/viscosity/hardcoded", json!("Methane"), "viscosity: unknown hardcoded model \"Methane\""),
        (
            "CarbonDioxide.json",
            "/conductivity/dilute/hardcoded",
            json!("CO2"),
            "dilute: unknown hardcoded model \"CO2\"",
        ),
        (
            "CarbonDioxide.json",
            "/conductivity/residual",
            json!({"hardcoded": "CO2"}),
            "residual: unknown hardcoded model \"CO2\"",
        ),
        (
            "CarbonDioxide.json",
            "/conductivity/critical",
            json!({"hardcoded": "CarbonDioxideScalabrinJPCRD2006"}),
            "critical: unknown hardcoded model \"CarbonDioxideScalabrinJPCRD2006\"",
        ),
        (
            "Nitrogen.json",
            "/viscosity/sigma_etta",
            json!(3.656e-10),
            "TRANSPORT.viscosity: unknown field \"sigma_etta\"",
        ),
        ("R11.json", "/conductivity/psi/b", json!([1.0]), "TRANSPORT.conductivity.psi: unknown field \"b\""),
        // Documentation is a note's string, or the unit of a key read, which must be the expected one.
        ("Ammonia.json", "/viscosity/_note", json!(1), "TRANSPORT.viscosity: unknown field \"_note\""),
        ("Nitrogen.json", "/viscosity/sigma_eta_units", json!("nm"), "sigma_eta_units is \"nm\", not \"m\""),
        ("Nitrogen.json", "/viscosity/T_critical_units", json!("K"), "unknown field \"T_critical_units\""),
    ];
    for (file, key, value, why) in transport_cases {
        let mut block = serde_json::from_str::<serde_json::Value>(&text(file)).unwrap()["TRANSPORT"].take();
        assert!(transport::record(file, Some(&block)).is_ok(), "{file}");
        let (parent, name) = key.rsplit_once('/').unwrap();
        block.pointer_mut(parent).unwrap()[name] = value;
        let err = transport::record(file, Some(&block)).unwrap_err();
        assert!(err.starts_with("TRANSPORT.") && err.ends_with(why), "{file} {key}: {err}");
    }
}

/// Map 02 §3.1: over all 159 EOS entries, 277 `d` and 161 `l` exponents are written as integral floats (`1.0`).
/// The mirror accepts them with their literal kind; every one is integral.
#[test]
fn integral_float_d_and_l_are_accepted() {
    let sources = sources();
    let (mut d, mut l) = (Vec::new(), Vec::new());
    for eos in sources.iter().flat_map(|s| &s.fluid.eos) {
        for block in &eos.alphar {
            match block {
                ResidualBlock::Power { d: bd, l: bl, .. }
                | ResidualBlock::Exponential { d: bd, l: bl, .. }
                | ResidualBlock::Lemmon2005 { d: bd, l: bl, .. } => {
                    d.extend(bd);
                    l.extend(bl);
                }
                ResidualBlock::DoubleExponential { d: bd, .. }
                | ResidualBlock::Gaussian { d: bd, .. }
                | ResidualBlock::GaoB { d: bd, .. } => d.extend(bd),
                ResidualBlock::NonAnalytic { .. } | ResidualBlock::Associating { .. } => {}
            }
        }
    }
    let floats = |xs: &[&Num]| xs.iter().filter(|x| matches!(x, Num::Float(_))).count();
    assert_eq!((floats(&d), floats(&l)), (277, 161));
    assert!(d.iter().chain(&l).all(|x| x.value().fract() == 0.0));
}

/// Rot: ROT-038. CoolProp folds every CP0PolyT and CP0AlyLee block of an EOS into one container with the first
/// block's `Tc` and `T0` (`FluidLibrary.h:288-298`), so blocks that disagree would be evaluated with another
/// block's constants. Datagen refuses them. n-Heptane's two Aly-Lee blocks agree.
#[test]
fn extend_with_mismatched_tc_is_refused() {
    let heptane = text("n-Heptane.json");
    let parsed = parse("n-Heptane.json", &heptane).unwrap();
    let blocks: Vec<_> = parsed.fluid.eos[0].alpha0.iter().filter_map(IdealBlock::polyt_constants).collect();
    assert_eq!(blocks.len(), 2);
    // The last `"Tc": 540.13` of the file is the second Aly-Lee block's.
    let tc = "\"Tc\": 540.13";
    let at = heptane.rfind(tc).unwrap();
    let edited = format!("{}\"Tc\": 540.14{}", &heptane[..at], &heptane[at + tc.len()..]);
    let blocks: Vec<_> =
        parse("n-Heptane.json", &edited.replace(tc, "\"Tc\": 540.14")).unwrap().fluid.eos[0].alpha0.clone();
    assert_eq!(blocks.iter().filter_map(IdealBlock::polyt_constants).count(), 2, "both edited: they agree again");
    let err = parse("n-Heptane.json", &edited).unwrap_err();
    assert!(err.contains("n-Heptane.json: EOS[0]: c_p0 blocks with different Tc or T0"), "{err}");
}

/// Map 09 §8: all 130 stored `source_eos_hash` stamps recompute from the parsed default EOS (literal kinds kept),
/// and the six pseudo-pure fluids carry none. One edited coefficient makes the stamp stale.
#[test]
fn all_130_stamps_recompute() {
    let sources = sources();
    let stamped: Vec<&str> =
        sources.iter().filter(|s| check_stamp(s).unwrap().is_some()).map(|s| s.fluid.info.name.as_str()).collect();
    assert_eq!(stamped.len(), 130);
    let unstamped: Vec<&str> =
        sources.iter().map(|s| s.fluid.info.name.as_str()).filter(|n| !stamped.contains(n)).collect();
    assert_eq!(unstamped, ["Air", "R404A", "R407C", "R410A", "R507A", "SES36"]);
    let nitrogen = text("Nitrogen.json");
    assert!(nitrogen.contains("0.924803575275"));
    let edited = parse("Nitrogen.json", &nitrogen.replacen("0.924803575275", "0.924803575276", 1)).unwrap();
    let err = check_stamp(&edited).unwrap_err();
    assert!(err.starts_with("Nitrogen.json: source_eos_hash 66cd08903a5b654e, recomputed "), "{err}");
}

/// PLAN.md M2.2: the committed `mp/check-points.csv` (M1.17) is the JSON's `check_points`, bit for bit, with the
/// superancillary's `Tcrittrue / K` as `Tc`: 390 points of 130 fluids, sorted by name.
#[test]
fn check_points_match_the_json() {
    let text = Repo::locate().read("crates/phasekit-verify/fixtures/mp/check-points.csv").unwrap();
    let points = phasekit_verify::Fixture::parse("mp/check-points.csv", &text).unwrap();
    let mut want: Vec<(String, [f64; 8])> = Vec::new();
    for source in sources() {
        let Some(sa) = &source.fluid.eos[0].superancillary else { continue };
        let tc = sa.meta["Tcrittrue / K"].as_f64().unwrap();
        for p in sa.check_points.as_deref().unwrap_or_default() {
            let row = [tc, p.t, p.p, p.rho_l, p.rho_v, p.p_ratio, p.rho_l_ratio, p.rho_v_ratio];
            want.push((source.fluid.info.name.clone(), row));
        }
    }
    want.sort_by(|a, b| a.0.cmp(&b.0)); // stable: points keep their JSON order within a fluid
    assert_eq!(want.len(), 390);
    let columns = ["Tc", "T", "p", "rhoL", "rhoV", "p_sa_mp", "rhoL_sa_mp", "rhoV_sa_mp"];
    let got: Vec<(String, [f64; 8])> = (0..points.rows().len())
        .map(|i| {
            let fluid = points.printed(i, "fluid").unwrap_or_default().to_string();
            (fluid, columns.map(|c| points.value(i, c).unwrap_or(f64::NAN)))
        })
        .collect();
    let bits = |rows: &[(String, [f64; 8])]| -> Vec<(String, [u64; 8])> {
        rows.iter().map(|(f, r)| (f.clone(), r.map(f64::to_bits))).collect()
    };
    assert_eq!(bits(&got), bits(&want));
}

/// PLAN.md M2.1: the lock is checked before anything is parsed. A changed byte, a missing file and an extra file
/// are refused, and the lock's lines must hash to `oracle.lock`'s `fluids_sha256`.
#[test]
fn fluids_lock_is_checked_first() {
    let lock = "# comment\na.json 2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae\n";
    let listing_sha = phasekit_verify::sha256_hex(lock.lines().nth(1).map(|l| format!("{l}\n")).unwrap().as_bytes());
    let files = |name: &str, text: &str| vec![(name.to_string(), text.to_string())];
    assert_eq!(check_lock(lock, &listing_sha, &files("a.json", "foo")), Ok(()));
    let err = |r: Result<(), Vec<String>>| r.unwrap_err().join("\n");
    assert!(err(check_lock(lock, &listing_sha, &files("a.json", "fop"))).contains("a.json: sha256"));
    assert!(err(check_lock(lock, &listing_sha, &files("b.json", "foo"))).contains("a.json: in the lock"));
    assert!(err(check_lock(lock, &listing_sha, &files("b.json", "foo"))).contains("b.json: not in the lock"));
    assert!(err(check_lock(lock, &"0".repeat(64), &files("a.json", "foo"))).contains("fluids_sha256"));
}
