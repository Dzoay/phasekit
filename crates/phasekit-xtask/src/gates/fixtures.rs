//! `gates fixtures` (VERIFICATION.md §3.4, §3.6; map 10 R15; ROT-134): every committed fixture has its MANIFEST.sha256
//! line and matches it, the manifest lists nothing else, every committed fixture is read by a test (beyond the list in
//! tests/manifest.rs), every `fixture!` path exists, and the committed fixtures stay within 16 MiB.

use super::{Verdict, no_args};
use crate::repo::Repo;

const FIXTURES: &str = "crates/phasekit-verify/fixtures";
/// The test that lists every fixture to check the manifest; a reference there does not count as reading one.
const LISTING: &str = "crates/phasekit-verify/tests/manifest.rs";
/// Committed files under fixtures/ that are not fixtures.
const NOT_FIXTURES: [&str; 3] = ["oracle.lock", "MANIFEST.sha256", "mp/fastchebpure.lock"];
/// The committed budget (VERIFICATION.md §3.6).
const BUDGET: usize = 16 * 1024 * 1024;

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let prefix = format!("{FIXTURES}/");
    let files: Vec<(String, String)> = repo
        .files(FIXTURES, "", true)
        .map_err(|e| vec![e])?
        .into_iter()
        .filter_map(|(path, text)| path.strip_prefix(&prefix).map(|rel| (rel.to_string(), text)))
        .collect();
    let manifest = repo.read(&format!("{FIXTURES}/MANIFEST.sha256")).map_err(|e| vec![e])?;
    let sources = repo.files("crates/phasekit-verify", ".rs", true).map_err(|e| vec![e])?;
    check(&manifest, &files, &sources)
}

/// Checks the manifest against the files under fixtures/ (path relative to it, text) and the crate's sources.
fn check(manifest: &str, files: &[(String, String)], sources: &[(String, String)]) -> Verdict {
    let mut errors = Vec::new();
    let mut listed: Vec<Vec<&str>> = Vec::new();
    for line in manifest.lines().filter(|line| !line.is_empty() && !line.starts_with('#')) {
        match line.split(' ').collect::<Vec<_>>() {
            fields if fields.len() == 4 => listed.push(fields),
            _ => errors.push(format!("MANIFEST.sha256: `{line}` is not `<sha256> <bytes> <rows> <path>`")),
        }
    }
    let fixtures: Vec<&(String, String)> =
        files.iter().filter(|(path, _)| !NOT_FIXTURES.contains(&path.as_str())).collect();
    if fixtures.is_empty() {
        errors.push("no committed fixtures".to_string());
    }
    for (path, text) in &fixtures {
        let Some(fields) = listed.iter().find(|fields| fields[3] == path) else {
            errors.push(format!("{path}: not in MANIFEST.sha256 (`cargo xtask oracle --write-manifest`)"));
            continue;
        };
        let rows = text.lines().filter(|line| !line.starts_with('#')).count();
        let file = [phasekit_verify::sha256_hex(text.as_bytes()), text.len().to_string(), rows.to_string()];
        if fields[..3] != file {
            errors.push(format!(
                "{path}: the file has sha256, bytes, rows `{}`; MANIFEST.sha256 says `{}`",
                file.join(" "),
                fields[..3].join(" ")
            ));
        }
    }
    for fields in &listed {
        if !fixtures.iter().any(|(path, _)| path == fields[3]) {
            errors.push(format!("{}: in MANIFEST.sha256 but not committed", fields[3]));
        }
    }
    let references: Vec<(&str, &str)> = sources
        .iter()
        .flat_map(|(file, text)| {
            text.split("fixture!(\"")
                .skip(1)
                .filter_map(|rest| rest.split('"').next())
                .map(move |path| (file.as_str(), path))
        })
        .collect();
    for (file, path) in &references {
        if !fixtures.iter().any(|(p, _)| p == path) {
            errors.push(format!("{file}: fixture!(\"{path}\") does not exist"));
        }
    }
    for (path, _) in &fixtures {
        if !references.iter().any(|(file, p)| p == path && *file != LISTING) {
            errors.push(format!("{path}: no test reads it besides the list in {LISTING} (map 10 R15)"));
        }
    }
    let total: usize = files.iter().map(|(_, text)| text.len()).sum();
    if over_budget(total) {
        errors
            .push(format!("the committed fixtures take {total} bytes, over the 16 MiB budget (VERIFICATION.md §3.6)"));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let bytes: usize = fixtures.iter().map(|(_, text)| text.len()).sum();
    Ok(format!("{} fixtures, {bytes} bytes, all read by tests", fixtures.len()))
}

/// Whether `total` committed bytes exceed the budget.
fn over_budget(total: usize) -> bool {
    total > BUDGET
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMOKE: &str = "# fixture: facts/v1\nr134a,PropsSI(H),ok,1.5\nwater,Props1SI(Tcrit),ok,647.0\n";

    fn line(path: &str, text: &str) -> String {
        let rows = text.lines().filter(|l| !l.starts_with('#')).count();
        format!("{} {} {rows} {path}\n", phasekit_verify::sha256_hex(text.as_bytes()), text.len())
    }

    fn files(extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let base = [
            ("oracle.lock", "version 8.0.0\n"),
            ("MANIFEST.sha256", "# manifest\n"),
            ("coolprop-8.0.0/facts/smoke.csv", SMOKE),
        ];
        base.iter().chain(extra).map(|(p, t)| (p.to_string(), t.to_string())).collect()
    }

    fn sources(reader: &str) -> Vec<(String, String)> {
        let listing = r#"const COMMITTED: &[(&str, &str)] = &[fixture!("coolprop-8.0.0/facts/smoke.csv")];"#;
        vec![
            (LISTING.to_string(), listing.to_string()),
            ("crates/phasekit-verify/tests/fixtures.rs".to_string(), reader.to_string()),
        ]
    }

    const READER: &str = r#"let (path, text) = fixture!("coolprop-8.0.0/facts/smoke.csv");"#;

    /// Map 10 R15: dangling fixtures. ROT-134.
    #[test]
    fn committed_fixtures_match_the_manifest_and_are_read() {
        let manifest = format!("# manifest\n{}", line("coolprop-8.0.0/facts/smoke.csv", SMOKE));
        assert_eq!(
            check(&manifest, &files(&[]), &sources(READER)),
            Ok(format!("1 fixtures, {} bytes, all read by tests", SMOKE.len()))
        );
        // A changed byte, a wrong row count or size, a file the manifest misses, a line without a file.
        let edited = files(&[]).into_iter().map(|(p, t)| (p, t.replace("1.5", "1.6"))).collect::<Vec<_>>();
        assert!(check(&manifest, &edited, &sources(READER)).unwrap_err()[0].contains("sha256"));
        let rows = manifest.replace(&format!(" {} 2 ", SMOKE.len()), &format!(" {} 3 ", SMOKE.len()));
        assert!(check(&rows, &files(&[]), &sources(READER)).is_err());
        let extra = files(&[("coolprop-8.0.0/facts/register.csv", SMOKE)]);
        let errors = check(&manifest, &extra, &sources(READER)).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("register.csv") && e.contains("not in MANIFEST")), "{errors:?}");
        let ghost = format!("{manifest}{}", line("coolprop-8.0.0/facts/ghost.csv", SMOKE));
        assert!(check(&ghost, &files(&[]), &sources(READER)).unwrap_err().iter().any(|e| e.contains("ghost.csv")));
        assert!(check("# manifest\nnot a line\n", &files(&[]), &sources(READER)).is_err());
    }

    #[test]
    fn every_fixture_is_read_and_every_reference_exists() {
        let manifest = format!("# manifest\n{}", line("coolprop-8.0.0/facts/smoke.csv", SMOKE));
        // Listed in tests/manifest.rs only: nothing reads it.
        let errors = check(&manifest, &files(&[]), &sources("")).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("smoke.csv") && e.contains("no test reads")), "{errors:?}");
        let dangling = format!(r#"{READER} fixture!("coolprop-8.0.0/eos/Water.csv");"#);
        let errors = check(&manifest, &files(&[]), &sources(&dangling)).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Water.csv") && e.contains("does not exist")), "{errors:?}");
        // The 16 MiB budget counts every committed byte.
        let big = "x".repeat(BUDGET);
        let errors = check(&manifest, &files(&[("hash/big.txt", &big)]), &sources(READER)).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("16 MiB")), "{errors:?}");
        assert!(check("# manifest\n", &files(&[])[..2], &sources(READER)).is_err(), "no fixtures is a failure");
        // VERIFICATION.md §3.6: 16 MiB in total, the limit itself allowed.
        assert!(!over_budget(16_777_216) && over_budget(16_777_217));
    }
}
