//! `cargo xtask datagen` (ARCHITECTURE.md §8; PLAN.md M2): the pinned v8.0.0 fluid JSON → phasekit's data. This
//! step reads: the files are checked against `data/fluids.lock` first, then each is parsed with the
//! literal-kind-preserving reader ([`json`]) and the closed serde mirror ([`mirror`]), and every stored superancillary
//! stamp must recompute ([`fnv`], M2.2); each default EOS maps into core's record, every JSON quirk resolved and every
//! value validated ([`record`], M2.3). Later steps add the blobs (M2.4) and the index and features (M2.5).

pub mod fnv;
pub mod index;
pub mod json;
pub mod mirror;
pub mod record;

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

/// The v1 blob of a record, checked to decode back to the shipped record bit for bit (restricted metadata is not
/// written; D14).
pub fn blob(record: &phasekit_core::internal::FluidRecord) -> Result<Vec<u8>, String> {
    let bytes = record.encode();
    let mut shipped = record.clone();
    shipped.environmental = None;
    match phasekit_core::internal::FluidRecord::decode(&bytes) {
        Ok(back) if back == shipped => Ok(bytes),
        Ok(_) => Err(format!("{}: the blob decodes to a different record", record.name)),
        Err(e) => Err(format!("{}: the blob does not decode: {e}", record.name)),
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

/// Everything datagen derives from the pinned files: the parsed sources and the index entries, blobs included.
pub fn generate(repo: &Repo) -> Result<(Vec<Source>, Vec<index::Entry>), Vec<String>> {
    let sources = load(repo)?;
    let mut fluids = Vec::with_capacity(sources.len());
    for source in &sources {
        let bytes = record::to_record(source).and_then(|r| blob(&r)).map_err(|e| vec![e])?;
        fluids.push((source, bytes));
    }
    let entries = index::index(&fluids).map_err(|e| vec![e])?;
    Ok((sources, entries))
}

/// How the files on disk differ from `outputs`: changed or missing outputs, and blobs nothing generates.
pub fn differences(repo: &Repo, outputs: &[(String, Vec<u8>)]) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for (path, bytes) in outputs {
        match repo.read_bytes(path) {
            Ok(on_disk) if on_disk == *bytes => {}
            Ok(_) => found.push(format!("{path} differs from a fresh generation")),
            Err(_) => found.push(format!("{path} is missing")),
        }
    }
    let blobs = format!("{}/blobs", index::DATA_CRATE);
    for name in repo.file_names(&blobs)? {
        let path = format!("{blobs}/{name}");
        if !outputs.iter().any(|(p, _)| *p == path) {
            found.push(format!("{path} is not generated by datagen"));
        }
    }
    Ok(found)
}

pub fn main(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask datagen");
        return ExitCode::FAILURE;
    }
    let repo = Repo::locate();
    let (sources, entries) = match generate(&repo) {
        Ok(generated) => generated,
        Err(errors) => {
            errors.iter().for_each(|e| eprintln!("datagen: {e}"));
            return ExitCode::FAILURE;
        }
    };
    for source in &sources {
        source.waivers.iter().for_each(|w| println!("datagen: waiver: {w}"));
        record::skipped(source).iter().for_each(|line| println!("datagen: skipped: {line}"));
        let name = &source.fluid.info.name;
        if source.file.trim_end_matches(".json") != name {
            println!("datagen: {} defines {name} (names come from INFO.NAME; map 09 §4.3)", source.file);
        }
    }
    let outputs = index::outputs(&entries);
    let written = outputs.iter().try_for_each(|(path, bytes)| repo.write_bytes(path, bytes));
    let stale = differences(&repo, &outputs).and_then(|found| {
        let blobs = format!("{}/blobs/", index::DATA_CRATE);
        let stale = found.iter().filter_map(|d| d.strip_suffix(" is not generated by datagen"));
        stale.filter(|p| p.starts_with(&blobs)).try_for_each(|p| repo.remove(p))
    });
    if let Err(e) = written.and(stale) {
        eprintln!("datagen: {e}");
        return ExitCode::FAILURE;
    }
    let mut sizes: Vec<usize> = entries.iter().map(|e| e.blob.len()).collect();
    sizes.sort_unstable();
    let (min, median, max) = (sizes[0], sizes[sizes.len() / 2], sizes[sizes.len() - 1]);
    let keys: usize = entries.iter().map(|e| e.keys().len()).sum();
    let stamps = sources.iter().filter(|s| matches!(check_stamp(s), Ok(Some(_)))).count();
    let eos: usize = sources.iter().map(|s| s.fluid.eos.len()).sum();
    println!("datagen: {} fluids, {eos} EOS entries, {stamps} source_eos_hash stamps recomputed", sources.len());
    println!("datagen: {keys} index keys; blob v1 bytes per fluid: min {min}, median {median}, max {max}");
    println!("datagen: wrote {} files under {}", outputs.len(), index::DATA_CRATE);
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirror::{IdealBlock, Num, ResidualBlock};
    use phasekit_core::internal::{Environmental, EosRecord, FluidRecord, IdealTerm};

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

    /// An edit of a parsed fluid file.
    type Edit = fn(&mut serde_json::Value);

    /// A pinned file with one edit applied to its parsed JSON, written back as JSON (numbers keep their literal kind).
    fn edited(file: &str, edit: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut value: serde_json::Value = serde_json::from_str(&text(file)).unwrap();
        edit(&mut value);
        serde_json::to_string(&value).unwrap()
    }

    /// Every fluid's record, by name.
    fn records() -> Vec<(Source, FluidRecord)> {
        sources()
            .into_iter()
            .map(|s| {
                let r = record::to_record(&s).unwrap();
                (s, r)
            })
            .collect()
    }

    fn record_of(file: &str, text: &str) -> Result<FluidRecord, String> {
        record::to_record(&parse(file, text)?)
    }

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
        let gao: Vec<_> =
            records.iter().flat_map(|(_, r)| r.eos.gao_b.iter().map(|t| (r.name.as_str(), t.eta))).collect();
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
                IdealTerm::Cp0Power { tc, .. } if tc != r.eos.t_reducing => {
                    Some((r.name.as_str(), tc, r.eos.t_reducing))
                }
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
        for (bad, why) in
            [(17.0, "exponent 17 is not an integer in 0..=16"), (-1.0, "exponent -1"), (1.5, "exponent 1.5")]
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

    /// The record as its blob ships it: restricted metadata is not written (D14).
    fn shipped(record: &FluidRecord) -> FluidRecord {
        let mut shipped = record.clone();
        shipped.environmental = None;
        shipped
    }

    /// PLAN.md M2.4: every fluid's record round-trips through blob v1 bitwise (decode(encode(r)) == r, and the bytes
    /// re-encode identically), restricted metadata aside.
    #[test]
    fn every_fluid_round_trips_bitwise() {
        let records = records();
        for (_, record) in &records {
            let blob = record.encode();
            let back = FluidRecord::decode(&blob).unwrap_or_else(|e| panic!("{}: {e}", record.name));
            assert_eq!(back, shipped(record), "{}", record.name);
            assert_eq!(back.encode(), blob, "{}", record.name);
            assert_eq!(super::blob(record).as_ref(), Ok(&blob), "{}", record.name);
        }
        assert_eq!(records.len(), 136);
        // Datagen's own check refuses a record its blob does not reproduce (`applied` is runtime state).
        let mut applied = records[0].1.clone();
        applied.applied = vec!["DIV-0001".into()];
        assert_eq!(super::blob(&applied), Err("1-Butene: the blob decodes to a different record".to_string()));
    }

    /// Every value of an EOS: its constants and every term field, as mutable places (u8 exponents apart).
    fn every_value(e: &mut EosRecord) -> (Vec<&mut f64>, Vec<&mut u8>) {
        let (mut f, mut u): (Vec<&mut f64>, Vec<&mut u8>) = (Vec::new(), Vec::new());
        f.extend([&mut e.gas_constant, &mut e.t_reducing, &mut e.rho_reducing, &mut e.rho_max]);
        for p in &mut e.power {
            f.extend([&mut p.n, &mut p.t, &mut p.c]);
            u.extend([&mut p.d, &mut p.l]);
        }
        for p in &mut e.lemmon2005 {
            f.extend([&mut p.n, &mut p.t, &mut p.m]);
            u.extend([&mut p.d, &mut p.l]);
        }
        for p in &mut e.double_exponential {
            f.extend([&mut p.n, &mut p.t, &mut p.gd, &mut p.gt, &mut p.lt]);
            u.extend([&mut p.d, &mut p.ld]);
        }
        for p in &mut e.gaussian {
            f.extend([&mut p.n, &mut p.t, &mut p.eta, &mut p.epsilon, &mut p.beta, &mut p.gamma]);
            u.push(&mut p.d);
        }
        for p in &mut e.gao_b {
            f.extend([&mut p.n, &mut p.t, &mut p.eta, &mut p.epsilon, &mut p.beta, &mut p.gamma, &mut p.b]);
            u.push(&mut p.d);
        }
        for p in &mut e.non_analytic {
            f.extend([
                &mut p.n,
                &mut p.a,
                &mut p.b,
                &mut p.beta,
                &mut p.big_a,
                &mut p.big_b,
                &mut p.big_c,
                &mut p.big_d,
            ]);
        }
        for term in &mut e.ideal {
            match term {
                IdealTerm::Lead { a1, a2 } | IdealTerm::Offset { a1, a2, .. } => f.extend([a1, a2]),
                IdealTerm::LogTau { a } => f.push(a),
                IdealTerm::Power { n, t } => f.extend([n, t]),
                IdealTerm::PlanckEinstein { n, theta } => f.extend([n, theta]),
                IdealTerm::PlanckEinsteinGeneralized { n, theta, c, d } => f.extend([n, theta, c, d]),
                IdealTerm::Cp0Power { c, t, tc, t0 } => f.extend([c, t, tc, t0]),
                _ => panic!("an ideal-gas kind this test does not know"),
            }
        }
        (f, u)
    }

    /// E14 on real records (the seed's `every_eos_field_is_hashed`, PLAN.md M2.4): moving any value of any of the 136
    /// EOS by one ulp (an exponent by one) changes the EOS hash, and the shape hash too unless the value is R, ρ_r or
    /// ρ_max.
    #[test]
    fn every_eos_field_is_hashed_on_real_records() {
        let mut checked = 0;
        for (_, record) in records() {
            let (base, shape) = (record.eos.eos_hash(), record.eos.shape_hash());
            let mut probe = record.eos.clone();
            let (floats, ints) = every_value(&mut probe);
            let (nf, ni) = (floats.len(), ints.len());
            for k in 0..nf + ni {
                let mut eos = record.eos.clone();
                let (mut floats, mut ints) = every_value(&mut eos);
                if k < nf {
                    *floats[k] = f64::from_bits(floats[k].to_bits() + 1);
                } else {
                    *ints[k - nf] ^= 1;
                }
                let scale = matches!(k, 0 | 2 | 3); // R, rho_r, rho_max
                assert_ne!(eos.eos_hash(), base, "{} value {k}", record.name);
                assert_eq!(eos.shape_hash() != shape, !scale, "{} value {k}", record.name);
                checked += 1;
            }
        }
        assert!(checked > 10_000, "{checked}");
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
