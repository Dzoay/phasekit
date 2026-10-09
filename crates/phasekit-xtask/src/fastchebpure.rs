//! `cargo xtask fetch-fastchebpure [--zip <file>] [--all <dir>]` (PLAN.md M6.1; VERIFICATION.md §3.2, §3.7; user
//! decisions 3a, 3b): NIST's fastchebpure release that CoolProp v8.0.0 pins, the multiprecision saturation of its EOS.
//! The zip is downloaded anonymously (no personal data in the URL or headers) or read from `--zip`, checked against the
//! sha256 in `mp/fastchebpure.lock` once the lock exists, and each fluid's `outputcheck/<Fluid>_check.json` becomes a
//! CSV in the layout of `mp/check-points.csv`, source `mp:fastchebpure@<tag>`: the core subset's into the committed
//! fixtures (with the lock and MANIFEST.sha256 lines), and with `--all` every fluid's with a v8.0.0 superancillary
//! into `<dir>/mp/fastchebpure-<tag>/` (the nightly, into `fixtures-full/`). A fluid is converted only if the EOS
//! fastchebpure fitted, the `source_eos_hash` of its `output/<Fluid>_exps.json`, is the fluid's v8.0.0 one; the check
//! files record no hash themselves.

use std::process::ExitCode;

use serde_json::Value;

use crate::repo::Repo;

/// The release CoolProp v8.0.0 pins (`Web/scripts/fluid_properties.Superancillary.py:15-18`).
pub const TAG: &str = "2026.06.02-v2";

/// Its GitHub archive.
pub const URL: &str = "https://github.com/CoolProp/fastchebpure/archive/refs/tags/2026.06.02-v2.zip";

/// The committed fixtures directory.
const FIXTURES: &str = "crates/phasekit-verify/fixtures";

/// The pin, relative to [`FIXTURES`].
pub const LOCK: &str = "mp/fastchebpure.lock";

/// The converted files' directory, relative to a fixtures directory.
pub const DIR: &str = "mp/fastchebpure-2026.06.02-v2";

/// The columns, as `mp/check-points.csv` has them.
const COLUMNS: &str = "fluid,Tc,T,p,rhoL,rhoV,p_sa_mp,rhoL_sa_mp,rhoV_sa_mp";

/// One fluid's check file converted: the CSV text, or why it is refused. `check` is `outputcheck/<Fluid>_check.json`,
/// `exps` `output/<Fluid>_exps.json`, and `v8_hash` the fluid's v8.0.0 `source_eos_hash`. T_c is the meta `Tcrittrue`
/// (the T_c of Θ, as `mp/check-points.csv` has it); p_sa_mp is p(SA)/p(mp), which the check files leave out.
pub fn convert(name: &str, check: &str, exps: &str, v8_hash: &str) -> Result<String, String> {
    let check: Value = serde_json::from_str(check).map_err(|e| format!("{name}_check.json: {e}"))?;
    let exps: Value = serde_json::from_str(exps).map_err(|e| format!("{name}_exps.json: {e}"))?;
    let hash = exps["source_eos_hash"].as_str().ok_or(format!("{name}_exps.json: no source_eos_hash"))?;
    if hash != v8_hash {
        return Err(format!("{name}: fastchebpure fitted the EOS {hash}, v8.0.0 has {v8_hash}"));
    }
    let tc = exps["meta"]["Tcrittrue / K"].as_f64().ok_or(format!("{name}_exps.json: no meta Tcrittrue / K"))?;
    let rows = check["data"].as_array().filter(|rows| !rows.is_empty()).ok_or(format!("{name}_check.json: no data"))?;
    let mut text = format!(
        "# fixture: checkpoints/v1\n# fluid: {name} source_eos_hash={hash}\n# source: mp:fastchebpure@{TAG}\n\
         # columns: {COLUMNS}\n# units: -,K,K,Pa,mol/m3,mol/m3,-,-,-\n# tol: label,in,in,sa_fit,sa_fit,sa_fit,in,in,in\n"
    );
    for (i, row) in rows.iter().enumerate() {
        let get = |key: &str| row[key].as_f64().ok_or(format!("{name}_check.json: row {i} has no `{key}`"));
        let (p_mp, p_sa) = (get("p(mp) / Pa")?, get("p(SA) / Pa")?);
        let cells = [
            tc,
            get("T / K")?,
            p_mp,
            get("rho'(mp) / mol/m^3")?,
            get("rho''(mp) / mol/m^3")?,
            p_sa / p_mp,
            get("rho'(SA)/rho'(mp)")?,
            get("rho''(SA)/rho''(mp)")?,
        ];
        text.push_str(name);
        for cell in cells {
            text.push_str(&format!(",{cell:?}"));
        }
        text.push('\n');
    }
    Ok(text)
}

/// The release's file for `name` among `files`, `<name><suffix>` ignoring case: fastchebpure names R1234yf's fit
/// `R1234YF_exps.json` and its check file `R1234yf_check.json`. An error if none matches or more than one does.
pub fn file_for(name: &str, suffix: &str, files: &[String]) -> Result<String, String> {
    let want = format!("{name}{suffix}").to_lowercase();
    let found: Vec<&String> = files.iter().filter(|f| f.to_lowercase() == want).collect();
    match found.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(format!("the release has no {name}{suffix}")),
        _ => Err(format!("the release has {} files named {name}{suffix} but for case", found.len())),
    }
}

/// The name fastchebpure gives a fluid's files: CoolProp's file name without `.json` (`R1224yd(Z)` for
/// `R1224yd(Z).json`, whose `INFO.NAME` is `R1224YDZ`), not the fluid's name.
pub fn release_name(file: &str) -> &str {
    file.strip_suffix(".json").unwrap_or(file)
}

/// The lock: the tag, the URL, the zip's sha256 and one `file <name> <sha256>` line per converted file, by name.
pub fn lock_text(zip_sha256: &str, files: &[(String, String)]) -> String {
    let mut text = format!(
        "# fastchebpure.lock v1 (docs/VERIFICATION.md section 3.2; `cargo xtask fetch-fastchebpure`). Editing a line is a\n\
         # pin move: a new release is a new tag, zip and files.\ntag {TAG}\nurl {URL}\nzip_sha256 {zip_sha256}\n"
    );
    let mut files: Vec<&(String, String)> = files.iter().collect();
    files.sort();
    for (name, sha) in files {
        text.push_str(&format!("file {name} {sha}\n"));
    }
    text
}

/// The zip sha256 a lock records.
pub fn locked_zip(lock: &str) -> Option<&str> {
    lock.lines().find_map(|line| line.strip_prefix("zip_sha256 "))
}

/// `manifest` with a `<sha256> <bytes> <rows> <path>` line for each of `files` (path relative to the fixtures
/// directory, text), replacing any earlier line for the path: comments first, then every entry sorted by path, as
/// `gen.py --write-manifest` writes it. Rows are the lines that are not comments.
pub fn manifest_with(manifest: &str, files: &[(String, String)]) -> String {
    let mut comments = Vec::new();
    let mut entries: Vec<(String, String)> = Vec::new();
    for line in manifest.lines() {
        if line.starts_with('#') {
            comments.push(line.to_string());
        } else if let Some((_, path)) = line.rsplit_once(' ') {
            entries.push((path.to_string(), line.to_string()));
        }
    }
    for (path, text) in files {
        let rows = text.lines().filter(|line| !line.starts_with('#')).count();
        let line = format!("{} {} {rows} {path}", phasekit_verify::sha256_hex(text.as_bytes()), text.len());
        entries.retain(|(p, _)| p != path);
        entries.push((path.clone(), line));
    }
    entries.sort();
    comments.into_iter().chain(entries.into_iter().map(|(_, line)| line)).map(|line| line + "\n").collect()
}

pub fn main(args: &[String]) -> ExitCode {
    match run(&Repo::locate(), args) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fetch-fastchebpure: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(repo: &Repo, args: &[String]) -> Result<String, String> {
    let (mut zip, mut all) = (None, None);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--zip" => zip = Some(args.next().ok_or("--zip needs a file")?.clone()),
            "--all" => all = Some(args.next().ok_or("--all needs a directory")?.clone()),
            _ => return Err("usage: cargo xtask fetch-fastchebpure [--zip <file>] [--all <dir>]".into()),
        }
    }
    let scratch = std::env::temp_dir().join(format!("phasekit-fastchebpure-{}", std::process::id()));
    let zip = match zip {
        Some(zip) => std::path::PathBuf::from(zip),
        None => repo.download(URL, &scratch.join(format!("fastchebpure-{TAG}.zip")))?,
    };
    let bytes = repo.read_outside_bytes(&zip)?;
    let zip_sha256 = phasekit_verify::sha256_hex(&bytes);
    let lock = repo.read(&format!("{FIXTURES}/{LOCK}")).ok();
    if let Some(locked) = lock.as_deref().and_then(locked_zip).filter(|locked| *locked != zip_sha256) {
        return Err(format!("the zip has sha256 {zip_sha256}; {LOCK} pins {locked}"));
    }
    let top = format!("fastchebpure-{TAG}");
    let tree = repo.unzip(&zip, &[&format!("{top}/outputcheck/*"), &format!("{top}/output/*")], &scratch)?;
    let sources = crate::datagen::load(repo).map_err(|e| e.join("; "))?;
    let hash_of = |name: &str| {
        let source = sources.iter().find(|s| s.fluid.info.name == name)?;
        source.fluid.eos.first()?.superancillary.as_ref()?.source_eos_hash.clone()
    };
    let core: Vec<&str> = phasekit_verify::eos::CORE_FILES
        .iter()
        .filter_map(|(path, _)| path.rsplit('/').next()?.strip_suffix(".csv"))
        .collect();
    let stem_of =
        |name: &str| sources.iter().find(|s| s.fluid.info.name == name).map(|s| release_name(&s.file).to_string());
    let names: Vec<String> = match &all {
        Some(_) => sources.iter().map(|s| s.fluid.info.name.clone()).filter(|n| hash_of(n).is_some()).collect(),
        None => core.iter().filter(|n| hash_of(n).is_some()).map(|n| n.to_string()).collect(),
    };
    let mut written = Vec::new();
    for name in &names {
        let stem = stem_of(name).unwrap_or_else(|| name.clone());
        let read = |dir: &str, suffix: &str| {
            let dir = tree.join(&top).join(dir);
            repo.read_outside(&dir.join(file_for(&stem, suffix, &repo.names_outside(&dir)?)?))
        };
        let (check, exps) = (read("outputcheck", "_check.json")?, read("output", "_exps.json")?);
        let v8 = hash_of(name).unwrap_or_default();
        written.push((format!("{DIR}/{name}.csv"), convert(name, &check, &exps, &v8)?));
    }
    repo.remove_scratch(&scratch);
    match all {
        Some(out) => {
            for (rel, text) in &written {
                repo.write_bytes(&format!("{out}/{rel}"), text.as_bytes())?;
            }
            Ok(format!("fetch-fastchebpure: {} files into {out}/{DIR}", written.len()))
        }
        None => {
            for (rel, text) in &written {
                repo.write_bytes(&format!("{FIXTURES}/{rel}"), text.as_bytes())?;
            }
            let shas: Vec<(String, String)> = written
                .iter()
                .map(|(rel, text)| {
                    (
                        rel.rsplit('/').next().unwrap_or_default().to_string(),
                        phasekit_verify::sha256_hex(text.as_bytes()),
                    )
                })
                .collect();
            repo.write_bytes(&format!("{FIXTURES}/{LOCK}"), lock_text(&zip_sha256, &shas).as_bytes())?;
            let manifest = repo.read(&format!("{FIXTURES}/MANIFEST.sha256"))?;
            repo.write_bytes(&format!("{FIXTURES}/MANIFEST.sha256"), manifest_with(&manifest, &written).as_bytes())?;
            Ok(format!("fetch-fastchebpure: {} core files, {LOCK} and MANIFEST.sha256 written", written.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPS: &str = r#"{"source_eos_hash": "b8bfb6326273c018", "meta": {"Tcrittrue / K": 647.0959999999873}}"#;

    fn check(rows: &str) -> String {
        format!(r#"{{"data": [{rows}], "meta": {{"Tcrit / K": 647.096}}}}"#)
    }

    const ROW: &str = r#"{"T / K": 273.16, "p(SA) / Pa": 611.654771069956, "p(mp) / Pa": 611.6547709684291,
        "rho'(mp) / mol/m^3": 55496.95514003009, "rho'(SA)/rho'(mp)": 0.9999999999999996,
        "rho''(mp) / mol/m^3": 0.26947008086577456, "rho''(SA)/rho''(mp)": 0.9999999999999993}"#;

    /// A check file becomes the layout of `mp/check-points.csv`: T_c from the fit's meta, the multiprecision values,
    /// p(SA)/p(mp) computed and the density ratios as given; every float round-trips.
    #[test]
    fn convert_writes_the_check_points_layout() {
        let text = convert("Water", &check(ROW), EXPS, "b8bfb6326273c018").unwrap();
        let fixture = phasekit_verify::Fixture::parse("Water.csv", &text).unwrap();
        assert_eq!(fixture.header("fluid"), Some("Water source_eos_hash=b8bfb6326273c018"));
        assert_eq!(fixture.header("source"), Some("mp:fastchebpure@2026.06.02-v2"));
        assert_eq!(fixture.columns().join(","), COLUMNS);
        let value = |column| fixture.value(0, column).unwrap();
        assert_eq!(value("Tc"), 647.0959999999873);
        assert_eq!((value("T"), value("p"), value("rhoV")), (273.16, 611.6547709684291, 0.26947008086577456));
        assert_eq!(value("p_sa_mp"), 611.654771069956 / 611.6547709684291);
        assert_eq!((value("rhoL_sa_mp"), value("rhoV_sa_mp")), (0.9999999999999996, 0.9999999999999993));
        assert_eq!(fixture.rows().len(), 1);
    }

    /// A fit of another EOS, or a file without its hash, T_c, data or a column, is refused by name.
    #[test]
    fn convert_refuses_another_eos_and_missing_values() {
        let err = convert("Water", &check(ROW), EXPS, "0000000000000000").unwrap_err();
        assert_eq!(err, "Water: fastchebpure fitted the EOS b8bfb6326273c018, v8.0.0 has 0000000000000000");
        assert!(convert("Water", &check(ROW), r#"{"meta": {}}"#, "x").unwrap_err().contains("no source_eos_hash"));
        let no_tc = r#"{"source_eos_hash": "h", "meta": {}}"#;
        assert!(convert("Water", &check(ROW), no_tc, "h").unwrap_err().contains("no meta Tcrittrue"));
        assert!(convert("Water", &check(""), EXPS, "b8bfb6326273c018").unwrap_err().contains("no data"));
        let short = check(&ROW.replace(r#""p(SA) / Pa": 611.654771069956, "#, ""));
        assert!(convert("Water", &short, EXPS, "b8bfb6326273c018").unwrap_err().contains("`p(SA) / Pa`"));
        assert!(convert("Water", "{", EXPS, "b8bfb6326273c018").unwrap_err().starts_with("Water_check.json: "));
    }

    /// The lock names the release and each file by sha256, in name order; its zip line reads back.
    #[test]
    fn lock_records_the_release_and_each_file() {
        let files = [("b.csv".to_string(), "22".to_string()), ("a.csv".to_string(), "11".to_string())];
        let text = lock_text("abc", &files);
        assert!(text.contains("\ntag 2026.06.02-v2\nurl https://github.com/"), "{text}");
        assert!(text.ends_with("zip_sha256 abc\nfile a.csv 11\nfile b.csv 22\n"), "{text}");
        assert_eq!(locked_zip(&text), Some("abc"));
        assert_eq!(locked_zip("tag x\n"), None);
    }

    /// Files are found ignoring case, and a missing or ambiguous one is an error.
    #[test]
    fn release_files_are_found_ignoring_case() {
        let files = ["R1234YF_exps.json", "R1234yf_check.json", "Water_exps.json"].map(String::from).to_vec();
        assert_eq!(file_for("R1234yf", "_exps.json", &files), Ok("R1234YF_exps.json".to_string()));
        assert_eq!(file_for("Water", "_exps.json", &files), Ok("Water_exps.json".to_string()));
        assert_eq!(file_for("Argon", "_exps.json", &files), Err("the release has no Argon_exps.json".to_string()));
        assert_eq!([release_name("R1224yd(Z).json"), release_name("Water.json")], ["R1224yd(Z)", "Water"]);
        let twice = ["a_x", "A_x"].map(String::from).to_vec();
        assert_eq!(file_for("a", "_x", &twice), Err("the release has 2 files named a_x but for case".to_string()));
    }

    /// PLAN.md M6.1 (map 10 §8.1: the release must match the v8.0.0 EOS hashes): each committed file names the EOS
    /// fastchebpure fitted, and that is its fluid's v8.0.0 `source_eos_hash`, which datagen recomputes from `EOS[0]`.
    #[test]
    fn committed_files_name_the_v8_eos() {
        let repo = Repo::locate();
        let sources = crate::datagen::load(&repo).unwrap();
        let files = repo.files(&format!("{FIXTURES}/{DIR}"), ".csv", false).unwrap();
        assert_eq!(files.len(), 12);
        for (path, text) in files {
            let header = text.lines().find_map(|l| l.strip_prefix("# fluid: ")).unwrap_or_default();
            let (name, hash) = header.split_once(" source_eos_hash=").unwrap_or_default();
            let source = sources.iter().find(|s| s.fluid.info.name == name).unwrap_or_else(|| panic!("{path}: {name}"));
            let v8 = source.fluid.eos[0].superancillary.as_ref().and_then(|sa| sa.source_eos_hash.as_deref());
            assert_eq!(Some(hash), v8, "{path}");
        }
    }

    /// Manifest lines as `gen.py --write-manifest` writes them: replaced or added, comments first, sorted by path.
    #[test]
    fn manifest_lines_are_replaced_and_sorted() {
        let manifest = "# header\nold 1 1 mp/z.csv\nstale 9 9 mp/a.csv\n";
        let text = "# fixture: x/v1\nrow\nrow\n";
        let got = manifest_with(manifest, &[("mp/a.csv".to_string(), text.to_string())]);
        let sha = phasekit_verify::sha256_hex(text.as_bytes());
        assert_eq!(got, format!("# header\n{sha} {} 2 mp/a.csv\nold 1 1 mp/z.csv\n", text.len()));
    }
}
