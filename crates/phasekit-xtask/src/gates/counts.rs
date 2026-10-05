//! `gates counts`: the executed-test count (map 10 R1; ROT-127). Runs the workspace tests, sums `test result: ok. N
//! passed` per test binary (doctests included) and compares it with `ci/test-counts.txt`. A listed binary that is
//! missing, runs 0 tests or runs fewer than its minimum fails; so does a binary that runs tests but is not listed.

use super::{Verdict, package_name};
use crate::repo::Repo;

const MANIFEST: &str = "ci/test-counts.txt";

const HEADER: &str = "\
# Executed-test minimums per test binary (VERIFICATION.md §11.2 `counts`): `<target> <package>::<binary> <min>`.
# `cargo xtask gates counts` runs the workspace tests and fails if a listed binary is missing, runs 0 tests or fewer
# than its minimum, or if a binary that runs tests is not listed. `--update` raises minimums and adds binaries; it
# never lowers one. Lowering a minimum by hand needs a `# lowered: <reason>` line above it. `# pending M<n>.<k>:
# <test>` lines name tests held back until that step; they fail the gate once milestone n has closed.
";

/// A test binary's owner: a package name and its integration-test files relative to the package (`tests/eos.rs`).
type Owner = (String, Vec<String>);

/// One manifest line.
#[derive(Debug, PartialEq)]
struct Entry {
    target: String,
    binary: String,
    min: usize,
}

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    let update_manifest = match args {
        [] => false,
        [flag] if flag == "--update" => true,
        _ => return Err(vec![format!("unexpected arguments: {}", args.join(" "))]),
    };
    let host = repo.host().map_err(|e| vec![e])?;
    let text = match repo.read(MANIFEST) {
        Ok(text) => text,
        Err(_) if update_manifest => HEADER.to_string(),
        Err(e) => return Err(vec![e]),
    };
    let manifest = parse_manifest(&text).map_err(|e| vec![e])?;
    // wasm32-wasip2 gets its runner at M0.5; until then only the host's counts can be measured here.
    if let Some(other) = manifest.iter().find(|entry| entry.target != host) {
        return Err(vec![format!("{MANIFEST}: no test runner for target {} in gates counts", other.target)]);
    }
    let owners = owners(repo).map_err(|e| vec![e])?;
    let (ok, output) = repo.cargo_merged("test", &["test", "--workspace"]).map_err(|e| vec![e])?;
    if !ok {
        let tail: Vec<String> = output.lines().rev().take(20).map(str::to_string).collect();
        return Err(std::iter::once("cargo test --workspace failed:".to_string())
            .chain(tail.into_iter().rev())
            .collect());
    }
    let executed = executed(&output, &owners).map_err(|e| vec![e])?;
    if update_manifest {
        repo.write(MANIFEST, &update(&text, &host, &executed)).map_err(|e| vec![e])?;
        return Ok(format!("{MANIFEST} updated for {host}"));
    }
    let mut errors = check(&manifest, &host, &executed);
    errors.extend(pending_due(&text, phasekit_verify::MILESTONE));
    let total: usize = executed.iter().map(|(_, n)| n).sum();
    if errors.is_empty() { Ok(format!("{total} tests in {} binaries on {host}", manifest.len())) } else { Err(errors) }
}

/// Each member's package name and integration-test files.
fn owners(repo: &Repo) -> Result<Vec<Owner>, String> {
    let mut owners = Vec::new();
    for member in repo.members()? {
        let name = package_name(&member.manifest).ok_or(format!("{}/Cargo.toml: no package name", member.dir))?;
        let prefix = format!("{}/", member.dir);
        let tests = repo.files(&format!("{}/tests", member.dir), ".rs", true)?;
        let tests = tests.into_iter().filter_map(|(path, _)| path.strip_prefix(&prefix).map(str::to_string)).collect();
        owners.push((name.to_string(), tests));
    }
    Ok(owners)
}

/// Executed tests per binary, `<package>::<binary>`, in the order of `cargo test` output (stdout and stderr merged).
fn executed(output: &str, owners: &[Owner]) -> Result<Vec<(String, usize)>, String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut current: Option<String> = None;
    for line in output.lines().map(str::trim) {
        let next = if let Some(running) = line.strip_prefix("Running ") {
            Some(binary(running, owners)?)
        } else if let Some(krate) = line.strip_prefix("Doc-tests ") {
            Some(format!("{}::doc", package_of(krate, owners)?))
        } else {
            None
        };
        if let Some(next) = next {
            if let Some(unfinished) = current.replace(next) {
                return Err(format!("{unfinished}: no `test result` line"));
            }
        } else if let Some(result) = line.strip_prefix("test result: ") {
            let binary = current.take().ok_or(format!("`{line}` before any test binary"))?;
            let passed = result
                .strip_prefix("ok. ")
                .and_then(|counts| counts.split_once(" passed"))
                .and_then(|(n, _)| n.parse::<usize>().ok())
                .ok_or(format!("{binary}: {line}"))?;
            match counts.iter_mut().find(|(b, _)| *b == binary) {
                Some((_, n)) => *n += passed,
                None => counts.push((binary, passed)),
            }
        }
    }
    match current {
        Some(unfinished) => Err(format!("{unfinished}: no `test result` line")),
        None => Ok(counts),
    }
}

/// `<package>::<binary>` for a `Running <src> (<executable>)` line: `lib`, `main` or `bin/<name>` for unit tests
/// (the executable is named after the crate), the file stem for an integration test (found among the owners' files).
fn binary(running: &str, owners: &[Owner]) -> Result<String, String> {
    let running = running.strip_prefix("unittests ").unwrap_or(running);
    let (src, executable) = running.split_once(" (").ok_or(format!("unreadable `Running {running}`"))?;
    let stem = src.strip_suffix(".rs").unwrap_or(src);
    if let Some(target) = stem.strip_prefix("src/") {
        let file = executable.trim_end_matches(')').rsplit('/').next().unwrap_or_default();
        let krate = file.rsplit_once('-').map_or(file, |(krate, _hash)| krate);
        return Ok(format!("{}::{target}", package_of(krate, owners)?));
    }
    let mut found = owners.iter().filter(|(_, tests)| tests.iter().any(|test| test == src));
    match (found.next(), found.next()) {
        (Some((package, _)), None) => {
            let test = stem.strip_prefix("tests/").unwrap_or(stem);
            Ok(format!("{package}::{}", test.strip_suffix("/main").unwrap_or(test)))
        }
        (None, _) => Err(format!("no workspace member has {src}")),
        _ => Err(format!("more than one workspace member has {src}")),
    }
}

/// The package that builds `krate` (`phasekit_core` → `phasekit-core`).
fn package_of<'a>(krate: &str, owners: &'a [Owner]) -> Result<&'a str, String> {
    owners
        .iter()
        .map(|(package, _)| package.as_str())
        .find(|package| package.replace('-', "_") == krate)
        .ok_or(format!("no workspace member builds crate {krate}"))
}

fn parse_manifest(text: &str) -> Result<Vec<Entry>, String> {
    let mut entries: Vec<Entry> = Vec::new();
    for (i, line) in text.lines().enumerate().map(|(i, line)| (i + 1, line.trim())) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || format!("{MANIFEST}:{i}: expected `<target> <package>::<binary> <min>`, got `{line}`");
        let fields: Vec<&str> = line.split_whitespace().collect();
        let &[target, binary, min] = fields.as_slice() else { return Err(bad()) };
        let min: usize = min.parse().map_err(|_| bad())?;
        if !binary.contains("::") {
            return Err(bad());
        }
        if min == 0 {
            return Err(format!("{MANIFEST}:{i}: a minimum of 0 is not a minimum; remove the line"));
        }
        if entries.iter().any(|entry| entry.target == target && entry.binary == binary) {
            return Err(format!("{MANIFEST}:{i}: {target} {binary} is listed twice"));
        }
        entries.push(Entry { target: target.to_string(), binary: binary.to_string(), min });
    }
    Ok(entries)
}

/// The violations of `executed` (one run on `target`) against the manifest.
fn check(manifest: &[Entry], target: &str, executed: &[(String, usize)]) -> Vec<String> {
    let mut errors = Vec::new();
    if executed.iter().all(|(_, n)| *n == 0) {
        errors.push(format!("no tests executed on {target} (map 10 R1)"));
    }
    for entry in manifest.iter().filter(|entry| entry.target == target) {
        match executed.iter().find(|(binary, _)| *binary == entry.binary) {
            None => errors.push(format!("{}: listed in {MANIFEST} but did not run", entry.binary)),
            Some((_, 0)) => errors.push(format!("{}: ran 0 tests (minimum {})", entry.binary, entry.min)),
            Some((_, n)) if *n < entry.min => {
                errors.push(format!("{}: ran {n} tests, minimum {}", entry.binary, entry.min))
            }
            Some(_) => {}
        }
    }
    for (binary, n) in executed {
        if *n > 0 && !manifest.iter().any(|entry| entry.target == target && entry.binary == *binary) {
            errors.push(format!(
                "{binary}: ran {n} tests but is not in {MANIFEST}; run `cargo xtask gates counts --update`"
            ));
        }
    }
    errors
}

/// The manifest with every minimum raised to what ran and every new binary that ran tests appended; comments and
/// order are kept, and no minimum is lowered.
fn update(text: &str, target: &str, executed: &[(String, usize)]) -> String {
    let mut out = String::new();
    let mut listed = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if let (false, &[t, binary, min]) = (line.trim_start().starts_with('#'), fields.as_slice()) {
            if t == target {
                listed.push(binary);
                let ran = executed.iter().find(|(b, _)| b == binary).map(|(_, n)| *n);
                if let (Some(n), Ok(min)) = (ran, min.parse::<usize>()) {
                    if n > min {
                        out.push_str(&format!("{t} {binary} {n}\n"));
                        continue;
                    }
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    for (binary, n) in executed {
        if *n > 0 && !listed.contains(&binary.as_str()) {
            out.push_str(&format!("{target} {binary} {n}\n"));
        }
    }
    out
}

/// `# pending M<n>.<k>:` lines whose milestone n has closed.
fn pending_due(text: &str, milestone: u8) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|line| {
            let n: u8 = line.strip_prefix("# pending M")?.split(['.', ':']).next()?.parse().ok()?;
            (n < milestone).then(|| format!("{MANIFEST}: `{line}`: M{n} has closed, so this test should be back"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "x86_64-unknown-linux-gnu";

    /// `cargo test --workspace` output, stdout and stderr merged (trimmed from a real run).
    const OUTPUT: &str = "   Compiling phasekit-core v0.0.0 (/w/crates/phasekit-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.00s
     Running unittests src/lib.rs (/t/debug/deps/phasekit_core-a5df2cc20558210d)

running 24 tests
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (/t/debug/deps/phasekit_data-ea5cd5166d2a4d9b)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/lazy_load.rs (/t/debug/deps/lazy_load-4a2343a918e5373e)

running 9 tests
test result: ok. 8 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/main.rs (/t/debug/deps/phasekit_xtask-1af9db83006fce99)

running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

   Doc-tests phasekit_core

running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
";

    fn owners() -> Vec<Owner> {
        let owner = |name: &str, tests: &[&str]| (name.to_string(), tests.iter().map(|t| t.to_string()).collect());
        vec![
            owner("phasekit-core", &[]),
            owner("phasekit-data", &[]),
            owner("phasekit-verify", &["tests/lazy_load.rs"]),
            owner("phasekit-xtask", &[]),
        ]
    }

    fn ran(counts: &[(&str, usize)]) -> Vec<(String, usize)> {
        counts.iter().map(|&(binary, n)| (binary.to_string(), n)).collect()
    }

    #[test]
    fn count_executed_parses_libtest_summary() {
        let want = ran(&[
            ("phasekit-core::lib", 24),
            ("phasekit-data::lib", 0),
            ("phasekit-verify::lazy_load", 8),
            ("phasekit-xtask::main", 1),
            ("phasekit-core::doc", 1),
        ]);
        assert_eq!(executed(OUTPUT, &owners()), Ok(want));
        // A binary without a result line, a failed binary or an owner nobody declares is an error, never a count.
        assert!(executed("     Running tests/lazy_load.rs (/t/lazy_load-0)\n", &owners()).is_err());
        assert!(executed(&OUTPUT.replace("ok. 8 passed; 0 failed", "FAILED. 7 passed; 1 failed"), &owners()).is_err());
        assert!(executed(&OUTPUT.replace("tests/lazy_load.rs", "tests/eos.rs"), &owners()).is_err());
    }

    /// Rot: ROT-127. Map 10 R1: a gate that passes with zero tests executed is fail-open.
    #[test]
    fn zero_executed_is_a_failure() {
        let manifest = parse_manifest(&format!("{HEADER}{HOST} phasekit-core::lib 24\n")).unwrap();
        assert_eq!(check(&manifest, HOST, &ran(&[("phasekit-core::lib", 24)])), Vec::<String>::new());
        let zero = check(&manifest, HOST, &ran(&[("phasekit-core::lib", 0)]));
        assert!(zero.iter().any(|e| e.contains("ran 0 tests")), "{zero:?}");
        assert!(!check(&manifest, HOST, &ran(&[("phasekit-core::lib", 23)])).is_empty(), "fewer than the minimum");
        assert!(!check(&manifest, HOST, &[]).is_empty(), "a listed binary that did not run");
        // A binary that runs tests must be listed; one that runs none (an empty doctest harness) need not be.
        let unlisted = ran(&[("phasekit-core::lib", 24), ("phasekit-core::doc", 1)]);
        assert!(!check(&manifest, HOST, &unlisted).is_empty());
        let empty = ran(&[("phasekit-core::lib", 24), ("phasekit-data::lib", 0)]);
        assert_eq!(check(&manifest, HOST, &empty), Vec::<String>::new());
        // Nothing executed at all fails even against an empty manifest, and a minimum of 0 is not a minimum.
        assert!(!check(&[], HOST, &ran(&[("phasekit-data::lib", 0)])).is_empty());
        assert!(parse_manifest(&format!("{HOST} phasekit-core::lib 0\n")).is_err());
        assert!(parse_manifest(&format!("{HOST} phasekit-core 24\n")).is_err());
    }

    #[test]
    fn update_raises_and_adds_but_never_lowers() {
        let text =
            format!("# header\n{HOST} phasekit-core::lib 24\n# pending M5.9: x\n{HOST} phasekit-verify::lazy_load 8\n");
        let now = ran(&[
            ("phasekit-core::lib", 30),
            ("phasekit-verify::lazy_load", 7),
            ("phasekit-xtask::main", 2),
            ("phasekit-data::lib", 0),
        ]);
        let want = format!(
            "# header\n{HOST} phasekit-core::lib 30\n# pending M5.9: x\n{HOST} phasekit-verify::lazy_load 8\n{HOST} phasekit-xtask::main 2\n"
        );
        assert_eq!(update(&text, HOST, &now), want);
    }

    #[test]
    fn a_pending_line_past_its_milestone_is_rejected() {
        let text = "# pending M5.9: new_family.rs::compat_strings_reach_the_new_family\n";
        assert_eq!(pending_due(text, 5), Vec::<String>::new());
        assert_eq!(pending_due(text, 6).len(), 1);
    }
}
