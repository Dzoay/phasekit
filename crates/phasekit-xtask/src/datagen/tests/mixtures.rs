//! The mixture step (PLAN.md M13.2): v8.0.0's mixture library read through the closed mirror, checked and oriented.

use super::*;
use crate::datagen::mixtures::{self, Kind, Library, Reducing};

fn library() -> Library {
    let repo = Repo::locate();
    mixtures::load(&repo, &sources()).unwrap()
}

/// A library fluid by any of its names, case-insensitively: (CAS, molar mass in kg/mol).
fn component(sources: &[Source], name: &str) -> Option<(String, f64)> {
    let info = |s: &&Source| {
        let i = &s.fluid.info;
        i.name.eq_ignore_ascii_case(name) || i.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    };
    let s = sources.iter().find(info)?;
    Some((s.fluid.info.cas.clone(), s.fluid.eos.first()?.molar_mass))
}

/// v8.0.0 orients a pair by its CAS numbers sorted as strings and inverts β_T and β_v when that swaps the components
/// (`MixtureParameters.cpp:130-163`): every generated pair is so oriented, β inverted where the file lists the larger
/// CAS first, γ and Lemmon's symmetric ξ and ζ unchanged.
#[test]
fn pairs_are_oriented_by_cas() {
    let lib = library();
    let raw: Vec<mixtures::PairJson> = serde_json::from_str(
        &Repo::locate().read(&format!("{}/{}", mixtures::MIXTURES_DIR, mixtures::FILES[1])).unwrap(),
    )
    .unwrap();
    assert_eq!((raw.len(), lib.pairs.len()), (888, 888));
    let mut swapped = 0;
    for r in &raw {
        let key = if r.cas1 < r.cas2 { [r.cas1.clone(), r.cas2.clone()] } else { [r.cas2.clone(), r.cas1.clone()] };
        let p = lib.pairs.iter().find(|p| p.cas == key).unwrap();
        let want = match (r.beta_t, r.gamma_t, r.beta_v, r.gamma_v, r.xi, r.zeta) {
            (Some(bt), Some(gt), Some(bv), Some(gv), ..) if r.cas1 > r.cas2 => {
                Reducing::Gerg([1.0 / bt, gt, 1.0 / bv, gv])
            }
            (Some(bt), Some(gt), Some(bv), Some(gv), ..) => Reducing::Gerg([bt, gt, bv, gv]),
            (.., Some(xi), Some(zeta)) => Reducing::Lemmon([xi, zeta]),
            _ => panic!("{r:?}"),
        };
        assert_eq!(p.reducing, want, "{:?}", p.names);
        swapped += usize::from(r.cas1 > r.cas2 && matches!(want, Reducing::Gerg([bt, ..]) if bt != 1.0));
    }
    assert!(swapped > 0, "some pair with β ≠ 1 is listed with the larger CAS first");
    assert!(lib.pairs.windows(2).all(|w| w[0].cas < w[1].cas));
}

/// The 28 departure functions decode by kind (16 Exponential, 9 GERG-2008, 3 Gaussian + Exponential), and the 40
/// pairs with F ≠ 0 each name one; the Exponential functions that carry an `Npower` v8.0.0 ignores are noted.
#[test]
fn every_departure_kind_decodes() {
    let lib = library();
    let count = |kind| lib.departures.iter().filter(|d| d.kind == kind).count();
    assert_eq!([Kind::Exponential, Kind::Gerg2008, Kind::GaussianExponential].map(count), [16, 9, 3]);
    assert_eq!(lib.pairs.iter().filter(|p| p.departure.is_some()).count(), 40);
    let gerg = lib.departures.iter().find(|d| d.name == "Methane-Nitrogen").unwrap();
    assert_eq!((gerg.kind, gerg.power, gerg.terms.len()), (Kind::Gerg2008, 2, 9));
    assert_eq!(
        (gerg.terms[2].eta, gerg.terms[2].epsilon, gerg.terms[2].beta, gerg.terms[2].gamma),
        (1.0, 0.5, 1.0, 0.5)
    );
    let ignored = lib.notes.iter().filter(|n| n.contains("Npower ignored")).count();
    assert!(ignored >= 2, "{:?}", lib.notes);
}

/// ROT-199: v8.0.0 stores the predefined blends as mole fractions, which its ASHRAE injection derived from ASHRAE 34's
/// mass fractions with CoolProp's molar masses. Where the list has the blend, the generated table keeps its mass
/// fractions, matched by name or library fluid (`R290` is `PROPANE`): converted with the library's molar masses they give
/// v8.0.0's mole fractions for all 120 whose components the library has (the four with R1132a are the rest), 80 to
/// 1e-12, the older entries' rounding to at most 8e-6 (R405A).
#[test]
fn predefined_fractions_follow_ashrae_mass_fractions() {
    let (lib, sources) = (library(), sources());
    let mut worst = Vec::new();
    for m in lib.predefined.iter().filter(|m| m.mass_fractions.is_some()) {
        let masses: Option<Vec<f64>> = m.components.iter().map(|c| component(&sources, c).map(|(_, mm)| mm)).collect();
        let (Some(w), Some(masses)) = (&m.mass_fractions, masses) else { continue };
        let moles: Vec<f64> = w.iter().zip(&masses).map(|(w, mm)| w / mm).collect();
        let total: f64 = moles.iter().sum();
        let off = moles.iter().zip(&m.mole_fractions).map(|(n, x)| (n / total - x).abs()).fold(0.0, f64::max);
        worst.push((off, m.name.clone()));
    }
    worst.sort_by(|a, b| b.0.total_cmp(&a.0));
    assert_eq!(lib.predefined.iter().filter(|m| m.mass_fractions.is_some()).count(), 124);
    assert_eq!(worst.len(), 120);
    assert!(worst[0].0 < 8e-6 && worst[0].1 == "R405A", "{:?}", &worst[..3]);
    assert_eq!(worst.iter().filter(|(off, _)| *off < 1e-12).count(), 80);
}

/// ROT-200: of v8.0.0's 147 predefined mixtures, 31 cannot be built: 27 lack a binary pair (R124/R22 the commonest)
/// and 4 name R1132a, which the library does not have. The step names each.
#[test]
fn pair_coverage_names_every_unbuildable_mixture() {
    let (lib, sources) = (library(), sources());
    let (mut missing_pair, mut unknown) = (Vec::new(), Vec::new());
    for m in &lib.predefined {
        let cas: Option<Vec<String>> =
            m.components.iter().map(|c| component(&sources, c).map(|(cas, _)| cas)).collect();
        let Some(cas) = cas else {
            unknown.push(m.name.clone());
            continue;
        };
        let has = |a: &String, b: &String| {
            let key = if a < b { [a.clone(), b.clone()] } else { [b.clone(), a.clone()] };
            lib.pairs.iter().any(|p| p.cas == key)
        };
        let complete = cas.iter().enumerate().all(|(i, a)| cas[i + 1..].iter().all(|b| has(a, b)));
        if !complete {
            missing_pair.push(m.name.clone());
        }
    }
    assert_eq!((missing_pair.len(), lib.predefined.len()), (27, 147), "{missing_pair:?}");
    assert_eq!(unknown, ["R468A", "R468B", "R468C", "R473A"]);
}

/// The lock: an edited file, a file not in the lock and a locked file not read each fail by name.
#[test]
fn the_mixture_lock_refuses_an_edited_file() {
    let sha = phasekit_verify::sha256_hex(b"[]");
    let lock = format!("# comment\na.json {sha}\nb.json {sha}\n");
    assert_eq!(mixtures::check_lock(&lock, &[("a.json", "[]"), ("b.json", "[]")]), Ok(()));
    let errors = mixtures::check_lock(&lock, &[("a.json", "[ ]"), ("c.json", "[]")]).unwrap_err();
    assert!(errors[0].starts_with("a.json: sha256 differs"), "{errors:?}");
    assert!(errors[1].starts_with("c.json: not in") && errors[2].starts_with("b.json: in"), "{errors:?}");
}

/// A pair gives one reducing form and carries a departure exactly where F ≠ 0; a CAS pair twice, a predefined mixture
/// repeated differently, or an unknown departure kind is refused, and an exact repeat is dropped.
#[test]
fn malformed_library_entries_are_refused() {
    let pair = |extra: &str| {
        format!(
            r#"[{{"BibTeX": "x", "CAS1": "2-2-2", "CAS2": "1-1-1", "F": 0.0, "Name1": "B", "Name2": "A", {extra}}}]"#
        )
    };
    let gerg = r#""betaT": 2.0, "gammaT": 1.5, "betaV": 4.0, "gammaV": 1.25"#;
    let ok = |pairs: &str, departures: &str, predefined: &str| {
        mixtures::library(pairs, departures, predefined, "400\tR-12/114 (60.0/40.0)\n", &[])
    };
    let lib = ok(&pair(gerg), "[]", "[]").unwrap();
    assert_eq!(lib.pairs[0].cas, ["1-1-1".to_string(), "2-2-2".to_string()]);
    assert_eq!(lib.pairs[0].reducing, Reducing::Gerg([0.5, 1.5, 0.25, 1.25]));
    assert!(ok(&pair(&format!(r#"{gerg}, "xi": 1.0, "zeta": 0.0"#)), "[]", "[]").is_err(), "both forms");
    assert!(ok(&pair(&format!(r#"{gerg}, "function": "X""#)), "[]", "[]").is_err(), "a function with F = 0");
    let twice = format!("[{}, {}]", &pair(gerg)[1..pair(gerg).len() - 1], &pair(gerg)[1..pair(gerg).len() - 1]);
    assert!(ok(&twice, "[]", "[]").unwrap_err()[0].contains("given twice"));
    let unknown = r#"[{"Name": "X", "BibTeX": "x", "aliases": [], "type": "Cubic", "n": [], "t": [], "d": []}]"#;
    assert!(ok("[]", unknown, "[]").unwrap_err()[0].contains("unknown type Cubic"));
    let air = r#"{"fluids": ["N2", "O2"], "mole_fractions": [0.79, 0.21], "name": "Air"}"#;
    let repeated = ok("[]", "[]", &format!("[{air}, {air}]")).unwrap();
    assert_eq!(repeated.predefined.len(), 1);
    assert!(repeated.notes.iter().any(|n| n.contains("Air: an exact repeat")));
    let other = r#"{"fluids": ["N2", "O2"], "mole_fractions": [0.78, 0.22], "name": "Air"}"#;
    assert!(ok("[]", "[]", &format!("[{air}, {other}]")).unwrap_err()[0].contains("differently"));
}

/// A pair JSON record over `reducing` (and `extra` keys), CAS numbers `cas`, weight `f`.
fn pair_json(reducing: &str, cas: (&str, &str), f: f64, extra: &str) -> String {
    format!(
        r#"[{{"BibTeX": "x", "CAS1": "{}", "CAS2": "{}", "F": {f:?}, "Name1": "A", "Name2": "B", {reducing}{extra}}}]"#,
        cas.0, cas.1
    )
}

/// GERG's reducing keys with β_T = `bt` and β_v = `bv`.
fn gerg(bt: f64, bv: f64) -> String {
    format!(r#""betaT": {bt:?}, "gammaT": 1.0, "betaV": {bv:?}, "gammaV": 1.0"#)
}

/// One Exponential departure function "D".
const ONE_DEPARTURE: &str = r#"[{"Name": "D", "BibTeX": "x", "aliases": [], "type": "Exponential", "n": [1.0], "t": [1.0], "d": [1], "l": [0]}]"#;

/// The library of `pairs` beside [`ONE_DEPARTURE`], or its first error.
fn pairs_library(pairs: &str) -> Result<Library, String> {
    mixtures::library(pairs, ONE_DEPARTURE, "[]", "", &[]).map_err(|e| e[0].clone())
}

/// The pair guards: β_T and β_v positive (each alone refused at 0), two distinct components, a departure exactly where
/// F ≠ 0 and named by a function that exists; the CAS order decides the orientation.
#[test]
fn pair_guards_refuse_what_cannot_be_oriented() {
    let ab = ("1-1-1", "2-2-2");
    assert!(pairs_library(&pair_json(&gerg(1.0, 1.0), ab, 0.0, "")).is_ok());
    for (bt, bv) in [(0.0, 1.0), (1.0, 0.0), (-1.0, -1.0)] {
        assert!(pairs_library(&pair_json(&gerg(bt, bv), ab, 0.0, "")).unwrap_err().contains("β > 0"), "{bt} {bv}");
    }
    let same = pairs_library(&pair_json(&gerg(1.0, 1.0), ("1-1-1", "1-1-1"), 0.0, "")).unwrap_err();
    assert!(same.contains("two components"), "{same}");
    let named = pairs_library(&pair_json(&gerg(2.0, 4.0), ab, 0.5, r#", "function": "D""#)).unwrap();
    assert_eq!((named.pairs[0].departure, named.pairs[0].f), (Some(0), 0.5));
    let missing = pairs_library(&pair_json(&gerg(1.0, 1.0), ab, 0.5, r#", "function": "X""#)).unwrap_err();
    assert!(missing.contains("no departure X"), "{missing}");
    assert!(pairs_library(&pair_json(&gerg(1.0, 1.0), ab, 0.5, "")).unwrap_err().contains("without a departure"));
    let lemmon = pairs_library(&pair_json(r#""xi": 28.95, "zeta": -6e-6"#, ("2-2-2", "1-1-1"), 0.0, "")).unwrap();
    assert_eq!(lemmon.pairs[0].reducing, Reducing::Lemmon([28.95, -6e-6]));
    assert_eq!(named.pairs[0].reducing, Reducing::Gerg([2.0, 1.0, 4.0, 1.0]), "not swapped: CAS1 < CAS2");
}

/// A departure function with `columns` (JSON keys and values).
fn departure_json(kind: &str, columns: &str) -> String {
    format!(r#"[{{"Name": "D", "BibTeX": "x", "aliases": [], "type": "{kind}", {columns}}}]"#)
}

/// The departure guards: every column as long as n, Npower within the terms, exponents d and l whole and ≥ 0.
#[test]
fn departure_guards_refuse_malformed_terms() {
    let refused = |kind: &str, columns: &str| {
        mixtures::library("[]", &departure_json(kind, columns), "[]", "", &[]).map_err(|e| e[0].clone()).unwrap_err()
    };
    let exp = |t: &str, d: &str, l: &str| format!(r#""n": [1.0, 2.0], "t": {t}, "d": {d}, "l": {l}"#);
    assert!(
        mixtures::library("[]", &departure_json("Exponential", &exp("[1.0, 2.0]", "[1, 2]", "[0, 1]")), "[]", "", &[])
            .is_ok()
    );
    assert!(refused("Exponential", &exp("[1.0]", "[1, 2]", "[0, 1]")).contains("differ in length"));
    assert!(refused("Exponential", &exp("[1.0, 2.0]", "[1]", "[0, 1]")).contains("differ in length"));
    assert!(refused("Exponential", &exp("[1.0, 2.0]", "[1, 2]", "[0]")).contains("l has 1 entries"));
    assert!(refused("Exponential", &exp("[1.0, 2.0]", "[1.5, 2]", "[0, 1]")).contains("whole number"));
    assert!(refused("Exponential", &exp("[1.0, 2.0]", "[1, 2]", "[0, -1]")).contains("whole number"));
    let gauss = r#""n": [1.0, 2.0], "t": [1.0, 2.0], "d": [1, 2], "eta": [0.0, 1.0], "epsilon": [0.0, 1.0], "beta": [0.0, 1.0], "gamma": [0.0, 1.0]"#;
    assert!(refused("GERG-2008", &format!(r#"{gauss}, "Npower": 3"#)).contains("Npower 3 beyond 2"));
    assert!(
        mixtures::library("[]", &departure_json("GERG-2008", &format!(r#"{gauss}, "Npower": 1"#)), "[]", "", &[])
            .is_ok()
    );
}

/// ASHRAE 34's rows: mass percent normalised to fractions (60/40 is 0.6 and 0.4, 50/40 also), components prefixed
/// with R; a row whose components and percentages differ in number, or sum to 0, is unreadable.
#[test]
fn ashrae_rows_read_as_mass_fractions() {
    let rows = mixtures::parse_ashrae("# comment\n400\tR-12/114 (60.0/40.0)\n401B\tR-22/152a (50.0/40.0\n").unwrap();
    assert_eq!(rows[0], ("R400".to_string(), vec![("R12".to_string(), 0.6), ("R114".to_string(), 0.4)]));
    assert_eq!(rows[1].1, vec![("R22".to_string(), 50.0 / 90.0), ("R152a".to_string(), 40.0 / 90.0)]);
    assert!(mixtures::parse_ashrae("400\tR-12/114 (60.0)\n").is_err());
    assert!(mixtures::parse_ashrae("400\tR-12/114 (0.0/0.0)\n").is_err());
}

/// A predefined mixture's fractions are positive (0 refused), as many as its fluids, and sum to 1 within 1e-10.
#[test]
fn predefined_fractions_are_checked() {
    let one = |fluids: &str, fractions: &str| {
        format!(r#"[{{"fluids": {fluids}, "mole_fractions": {fractions}, "name": "M"}}]"#)
    };
    let read = |text: String| mixtures::library("[]", "[]", &text, "", &[]).map_err(|e| e[0].clone());
    assert!(read(one(r#"["A", "B"]"#, "[0.25, 0.75]")).is_ok());
    for (fluids, fractions) in [
        (r#"["A", "B"]"#, "[0.0, 1.0]"),
        (r#"["A", "B"]"#, "[-0.5, 1.5]"),
        (r#"["A"]"#, "[0.5, 0.5]"),
        (r#"["A", "B"]"#, "[0.5, 0.6]"),
    ] {
        assert!(read(one(fluids, fractions)).unwrap_err().contains("fractions"), "{fluids} {fractions}");
    }
}

/// The generated file is the committed one (what `gates datagen` checks), written with round-trip floats.
#[test]
fn the_generated_tables_are_the_committed_file() {
    let outputs = mixtures::outputs(&library());
    let committed = Repo::locate().read_bytes(mixtures::GENERATED).unwrap();
    assert_eq!(outputs, vec![(mixtures::GENERATED.to_string(), committed)]);
}
