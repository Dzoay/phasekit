//! `cargo xtask datagen` (ARCHITECTURE.md §8; PLAN.md M2): the pinned v8.0.0 fluid JSON → phasekit's data. This
//! step reads: the files are checked against `data/fluids.lock` first, then each is parsed with the
//! literal-kind-preserving reader ([`json`]) and the closed serde mirror ([`mirror`]), and every stored superancillary
//! stamp must recompute ([`fnv`], M2.2); each default EOS maps into core's record, every JSON quirk resolved and every
//! value validated ([`record`], M2.3). Later steps add the blobs (M2.4) and the index and features (M2.5).

pub mod caloric;
pub mod citations;
pub mod corrections;
pub mod fnv;
pub mod index;
pub mod json;
pub mod mirror;
pub mod record;
pub mod superanc;
pub mod transport;

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
        if let Some(first) = polyt.next()
            && polyt.any(|other| other != first)
        {
            return Err(format!("{file}: EOS[{i}]: c_p0 blocks with different Tc or T0 (CoolProp would join them)"));
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
    let mut records = sources.iter().map(record::to_record).collect::<Result<Vec<_>, _>>().map_err(|e| vec![e])?;
    let rows = corrections::parse(&repo.read(corrections::CORRECTIONS).map_err(|e| vec![e])?).map_err(|e| vec![e])?;
    corrections::attach(&rows, &mut records)?;
    let bib = citations::parse_bib(&repo.read(citations::BIB).map_err(|e| vec![e])?);
    let reviewed = citations::parse(&repo.read(citations::CITATIONS).map_err(|e| vec![e])?).map_err(|e| vec![e])?;
    citations::attach(&sources, &mut records, &bib, &reviewed)?;
    let mut fluids = Vec::with_capacity(sources.len());
    for (source, record) in sources.iter().zip(&records) {
        fluids.push((source, blob(record).map_err(|e| vec![e])?));
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
        let skipped = record::skipped(source).into_iter().chain(transport::skipped(source));
        skipped.for_each(|line| println!("datagen: skipped: {line}"));
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
    println!("datagen: {keys} index keys; blob bytes per fluid: min {min}, median {median}, max {max}");
    println!("datagen: wrote {} files under {}", outputs.len(), index::DATA_CRATE);
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests;
