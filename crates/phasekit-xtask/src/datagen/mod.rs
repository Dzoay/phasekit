//! `cargo xtask datagen` (ARCHITECTURE.md §8; PLAN.md M2): the pinned v8.0.0 fluid JSON → phasekit's data. This
//! step reads: the files are checked against `data/fluids.lock` first, then each is parsed with the
//! literal-kind-preserving reader ([`json`]) and the closed serde mirror ([`mirror`]), and every stored superancillary
//! stamp must recompute ([`fnv`], M2.2). Later steps add the mapping into core records (M2.3), the blobs (M2.4) and the
//! index and features (M2.5).

pub mod fnv;
pub mod json;
pub mod mirror;

use std::process::ExitCode;

use json::Json;

use crate::repo::Repo;

/// The pinned fluid files (gitignored; `scripts/fetch-coolprop.sh`).
pub const FLUIDS_DIR: &str = "reference/CoolProp/dev/fluids";

/// Their sha256 lines.
pub const FLUIDS_LOCK: &str = "data/fluids.lock";

/// One fluid file, parsed.
#[derive(Debug)]
pub struct Source {
    /// The file name (`R1224yd(Z).json`: names come from `INFO.NAME`, not file names; map 09 §4.3).
    pub file: String,
    /// The literal-kind tree, each duplicate key reduced to its last value.
    pub tree: Json,
    /// The typed mirror of the same tree.
    pub fluid: mirror::Fluid,
    /// Logged waivers (map 09 R19).
    pub waivers: Vec<String>,
}

/// Duplicate keys parsed under a logged waiver, each only while every occurrence holds the same value: (file, path).
const DUPLICATE_KEY_WAIVERS: [(&str, &str); 1] = [("Chlorine.json", "EOS[0].SUPERANCILLARY.source_eos_hash")];

/// Parses one fluid file: literal-kind tree, duplicate-key check, typed mirror, cross-block checks.
pub fn parse(file: &str, text: &str) -> Result<Source, String> {
    let tree = Json::parse(text).map_err(|e| format!("{file}: {e}"))?;
    let mut waivers = Vec::new();
    for path in tree.duplicate_keys() {
        let values = tree.values_at(&path);
        if !(DUPLICATE_KEY_WAIVERS.contains(&(file, path.as_str())) && values.windows(2).all(|w| w[0] == w[1])) {
            return Err(format!("{file}: duplicate key {path} ({} values)", values.len()));
        }
        waivers.push(format!("{file}: duplicate key {path} with equal values (map 09 R19)"));
    }
    let tree = tree.deduplicated();
    let fluid: mirror::Fluid = serde_json::from_value(tree.to_value()).map_err(|e| format!("{file}: {e}"))?;
    for (i, eos) in fluid.eos.iter().enumerate() {
        let mut polyt = eos.alpha0.iter().filter_map(mirror::IdealBlock::polyt_constants);
        if let Some(first) = polyt.next() {
            if polyt.any(|other| other != first) {
                return Err(format!(
                    "{file}: EOS[{i}]: c_p0 blocks with different Tc or T0 (CoolProp would join them)"
                ));
            }
        }
    }
    Ok(Source { file: file.to_string(), tree, fluid, waivers })
}

/// The FNV-1a gate (map 09 §8): a stored `source_eos_hash` must recompute from the parsed `EOS[0]`, so the shipped
/// superancillary was fitted to exactly this EOS. Returns the stamp, or `None` for a fluid without one.
pub fn check_stamp(source: &Source) -> Result<Option<String>, String> {
    let file = &source.file;
    let Some(stored) = source.fluid.eos.first().and_then(|e| e.superancillary.as_ref()?.source_eos_hash.as_ref())
    else {
        return Ok(None);
    };
    let computed = fnv::eos_stamp(&source.tree).ok_or_else(|| format!("{file}: no EOS[0] object to stamp"))?;
    if computed == *stored {
        Ok(Some(computed))
    } else {
        Err(format!("{file}: source_eos_hash {stored}, recomputed {computed}"))
    }
}

/// Checks the files against the lock: same names, same sha256, and the lock's lines hash to `fluids_sha256`.
pub fn check_lock(lock: &str, fluids_sha256: &str, files: &[(String, String)]) -> Result<(), Vec<String>> {
    let lines: Vec<&str> = lock.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).collect();
    let listing: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let mut errors = Vec::new();
    if phasekit_verify::sha256_hex(listing.as_bytes()) != fluids_sha256 {
        errors.push(format!("{FLUIDS_LOCK} does not hash to oracle.lock's fluids_sha256 {fluids_sha256}"));
    }
    let locked: Vec<(&str, &str)> = lines.iter().filter_map(|l| l.split_once(' ')).collect();
    for (name, text) in files {
        match locked.iter().find(|(f, _)| f == name) {
            None => errors.push(format!("{name}: not in the lock")),
            Some((_, sha)) if *sha != phasekit_verify::sha256_hex(text.as_bytes()) => {
                errors.push(format!("{name}: sha256 differs from the lock ({sha})"));
            }
            Some(_) => {}
        }
    }
    for (name, _) in locked.iter().filter(|(f, _)| !files.iter().any(|(n, _)| n == f)) {
        errors.push(format!("{name}: in the lock, missing from {FLUIDS_DIR}"));
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

/// Every fluid of the pinned checkout, sorted by file name, after the lock check.
pub fn load(repo: &Repo) -> Result<Vec<Source>, Vec<String>> {
    let fail = |e: String| vec![e];
    let oracle = repo.read(crate::oracle::LOCK).map_err(fail)?;
    let oracle = phasekit_verify::OracleLock::parse(&oracle).map_err(fail)?;
    let fluids_sha256 = oracle.get("fluids_sha256").ok_or_else(|| fail("oracle.lock has no fluids_sha256".into()))?;
    let files: Vec<(String, String)> = repo
        .files(FLUIDS_DIR, ".json", false)
        .map_err(fail)?
        .into_iter()
        .map(|(path, text)| (path.rsplit('/').next().unwrap_or_default().to_string(), text))
        .collect();
    if files.is_empty() {
        return Err(fail(format!("no fluid files in {FLUIDS_DIR} (scripts/fetch-coolprop.sh)")));
    }
    check_lock(&repo.read(FLUIDS_LOCK).map_err(fail)?, fluids_sha256, &files)?;
    let checked = |(file, text): &(String, String)| parse(file, text).and_then(|s| check_stamp(&s).map(|_| s));
    let (sources, errors): (Vec<_>, Vec<_>) = files.iter().map(checked).partition(Result::is_ok);
    if errors.is_empty() {
        Ok(sources.into_iter().flatten().collect())
    } else {
        Err(errors.into_iter().filter_map(Result::err).collect())
    }
}

pub fn main(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask datagen");
        return ExitCode::FAILURE;
    }
    match load(&Repo::locate()) {
        Ok(sources) => {
            for source in &sources {
                source.waivers.iter().for_each(|w| println!("datagen: waiver: {w}"));
                let name = &source.fluid.info.name;
                if source.file.trim_end_matches(".json") != name {
                    println!("datagen: {} defines {name} (names come from INFO.NAME; map 09 §4.3)", source.file);
                }
            }
            let entries: usize = sources.iter().map(|s| s.fluid.eos.len()).sum();
            let stamps = sources.iter().filter(|s| matches!(check_stamp(s), Ok(Some(_)))).count();
            println!(
                "datagen: {} fluids parsed, {entries} EOS entries, {stamps} source_eos_hash stamps recomputed",
                sources.len()
            );
            ExitCode::SUCCESS
        }
        Err(errors) => {
            errors.iter().for_each(|e| eprintln!("datagen: {e}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirror::{IdealBlock, Num, ResidualBlock};

    fn sources() -> Vec<Source> {
        load(&Repo::locate()).unwrap()
    }

    /// The text of one pinned fluid file.
    fn text(file: &str) -> String {
        Repo::locate().read(&format!("{FLUIDS_DIR}/{file}")).unwrap()
    }

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
        let others: Vec<&String> =
            sources.iter().filter(|s| s.file != "Chlorine.json").flat_map(|s| &s.waivers).collect();
        assert!(others.is_empty(), "{others:?}");

        let unequal = text("Chlorine.json").replacen("\"3144edae6b63f0b5\"", "\"3144edae6b63f0b6\"", 1);
        let err = parse("Chlorine.json", &unequal).unwrap_err();
        assert!(err.contains("duplicate key EOS[0].SUPERANCILLARY.source_eos_hash"), "{err}");
        let nitrogen = text("Nitrogen.json").replacen(
            "\"source_eos_hash\":",
            "\"source_eos_hash\": \"x\", \"source_eos_hash\":",
            1,
        );
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
        let listing_sha =
            phasekit_verify::sha256_hex(lock.lines().nth(1).map(|l| format!("{l}\n")).unwrap().as_bytes());
        let files = |name: &str, text: &str| vec![(name.to_string(), text.to_string())];
        assert_eq!(check_lock(lock, &listing_sha, &files("a.json", "foo")), Ok(()));
        let err = |r: Result<(), Vec<String>>| r.unwrap_err().join("\n");
        assert!(err(check_lock(lock, &listing_sha, &files("a.json", "fop"))).contains("a.json: sha256"));
        assert!(err(check_lock(lock, &listing_sha, &files("b.json", "foo"))).contains("a.json: in the lock"));
        assert!(err(check_lock(lock, &listing_sha, &files("b.json", "foo"))).contains("b.json: not in the lock"));
        assert!(err(check_lock(lock, &"0".repeat(64), &files("a.json", "foo"))).contains("fluids_sha256"));
    }
}
