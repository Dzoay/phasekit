//! `gates counts`: the executed-test count (map 10 R1; ROT-127). Runs the workspace tests with cargo-nextest (profile
//! `counts`, `.config/nextest.toml`) and the doctests with `cargo test --doc`, counts the passed tests per test binary
//! from nextest's JUnit report and libtest's `test result: ok. N passed` lines, and compares them with
//! `ci/test-counts.txt`. A listed binary that is missing, runs 0 tests or runs fewer than its minimum fails; so does a
//! binary that runs tests but is not listed.

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

/// The targets measured besides the host (G4). xtask spawns processes and reads the repository, so it is a host-only
/// tool and their runs leave it out (PLAN.md §2.5).
const CROSS: [&str; 1] = ["wasm32-wasip2"];

/// The nextest profile the gate runs: G3's `ci` plus the JUnit report it reads.
const PROFILE: &str = "counts";

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
    if let Some(other) = manifest.iter().find(|entry| test_command(&entry.target, &host).is_none()) {
        return Err(vec![format!("{MANIFEST}: no test runner for target {} on {host}", other.target)]);
    }
    let members = repo.members().map_err(|e| vec![e])?;
    let package =
        |dir: &str, manifest: &str| package_name(manifest).map(str::to_string).ok_or(format!("{dir}: no package name"));
    let packages =
        members.iter().map(|m| package(&m.dir, &m.manifest)).collect::<Result<Vec<_>, _>>().map_err(|e| vec![e])?;
    let (mut updated, mut errors, mut summary) = (text.clone(), Vec::new(), Vec::new());
    for target in std::iter::once(host.as_str()).chain(CROSS) {
        let (nextest, doc) = test_command(target, &host).unwrap_or_default();
        let nextest: Vec<&str> = nextest.iter().map(String::as_str).collect();
        let (ok, output, junit) = repo.nextest(&format!("nextest-{target}"), &nextest, PROFILE).map_err(|e| vec![e])?;
        failed(ok, &nextest, &output)?;
        let mut executed = junit_counts(&junit.map_err(|e| vec![e])?).map_err(|e| vec![e])?;
        let doc: Vec<&str> = doc.iter().map(String::as_str).collect();
        let (ok, output) = repo.cargo_merged(&format!("doc-{target}"), &doc).map_err(|e| vec![e])?;
        failed(ok, &doc, &output)?;
        executed.extend(doc_counts(&output, &packages).map_err(|e| vec![e])?);
        updated = update(&updated, target, &executed);
        errors.extend(check(&manifest, target, &executed).into_iter().map(|e| format!("{target}: {e}")));
        let total: usize = executed.iter().map(|(_, n)| n).sum();
        summary.push(format!("{total} tests on {target}"));
    }
    if update_manifest {
        repo.write(MANIFEST, &updated).map_err(|e| vec![e])?;
        return Ok(format!("{MANIFEST} updated: {}", summary.join(", ")));
    }
    errors.extend(pending_due(&text, phasekit_verify::MILESTONE));
    if errors.is_empty() { Ok(format!("{} binaries; {}", manifest.len(), summary.join(", "))) } else { Err(errors) }
}

/// A failed run's last 20 lines, as the gate's violations.
fn failed(ok: bool, args: &[&str], output: &str) -> Result<(), Vec<String>> {
    if ok {
        return Ok(());
    }
    let tail: Vec<String> = output.lines().rev().take(20).map(str::to_string).collect();
    Err(std::iter::once(format!("cargo {} failed:", args.join(" "))).chain(tail.into_iter().rev()).collect())
}

/// The commands that measure `target`, as (`cargo nextest` arguments, `cargo test --doc` arguments): everything on the
/// host, everything but xtask on a cross target (run by the runner in `.cargo/config.toml`), and `None` for a target
/// this machine cannot run. The nextest profile is added by [`Repo::nextest`].
fn test_command(target: &str, host: &str) -> Option<(Vec<String>, Vec<String>)> {
    let (nextest, doc): (&[&str], &[&str]) = if target == host {
        (&["nextest", "run", "--workspace"], &["test", "--doc", "--workspace"])
    } else if CROSS.contains(&target) {
        (
            &["nextest", "run", "--workspace", "--exclude", "phasekit-xtask", "--target", target],
            &["test", "--doc", "--workspace", "--exclude", "phasekit-xtask", "--target", target],
        )
    } else {
        return None;
    };
    let words = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect();
    Some((words(nextest), words(doc)))
}

/// Passed tests per binary from a nextest JUnit report, in report order. A test case is one `<testcase .../>`
/// element; one with a `<failure>`, `<error>` or `<skipped>` child did not pass. The `classname` is nextest's binary
/// id, named as the manifest names binaries ([`binary_of`]).
fn junit_counts(xml: &str) -> Result<Vec<(String, usize)>, String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for case in xml.split("<testcase ").skip(1) {
        let (head, rest) = case.split_once('>').ok_or("junit: an unterminated <testcase>")?;
        let id = head
            .split_once("classname=\"")
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(id, _)| id)
            .ok_or("junit: a <testcase> without a classname")?;
        let body = if head.ends_with('/') { "" } else { rest.split("</testcase>").next().unwrap_or_default() };
        let passed = !["<failure", "<error", "<skipped"].iter().any(|tag| body.contains(tag));
        let binary = binary_of(id);
        match counts.iter_mut().find(|(b, _)| *b == binary) {
            Some((_, n)) => *n += usize::from(passed),
            None => counts.push((binary, usize::from(passed))),
        }
    }
    Ok(counts)
}

/// The manifest's `<package>::<binary>` for a nextest binary id: `phasekit-core` (unit tests of a library) is
/// `phasekit-core::lib`, `phasekit-xtask::bin/phasekit-xtask` (the package's main binary) is `phasekit-xtask::main`,
/// and an integration test keeps its name (`phasekit-verify::eos`).
fn binary_of(id: &str) -> String {
    match id.split_once("::") {
        None => format!("{id}::lib"),
        Some((package, binary)) if binary.strip_prefix("bin/") == Some(package) => format!("{package}::main"),
        Some(_) => id.to_string(),
    }
}

/// Executed doctests per crate, `<package>::doc`, from `cargo test --doc` output (stdout and stderr merged).
fn doc_counts(output: &str, packages: &[String]) -> Result<Vec<(String, usize)>, String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut current: Option<String> = None;
    for line in output.lines().map(str::trim) {
        let next = match line.strip_prefix("Doc-tests ") {
            Some(krate) => Some(format!("{}::doc", package_of(krate, packages)?)),
            None => None,
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

/// The package that builds `krate` (`phasekit_core` → `phasekit-core`).
fn package_of<'a>(krate: &str, packages: &'a [String]) -> Result<&'a str, String> {
    packages
        .iter()
        .map(String::as_str)
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
        if !line.trim_start().starts_with('#')
            && let &[t, binary, min] = fields.as_slice()
            && t == target
        {
            listed.push(binary);
            let ran = executed.iter().find(|(b, _)| b == binary).map(|(_, n)| *n);
            if let (Some(n), Ok(min)) = (ran, min.parse::<usize>())
                && n > min
            {
                out.push_str(&format!("{t} {binary} {n}\n"));
                continue;
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

    /// A nextest JUnit report (profile `counts`; trimmed from a real run): a library's unit tests, an integration test
    /// with one failure, and the xtask binary's unit tests. Ignored tests are not in the report.
    const JUNIT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="5" skipped="0" failures="1" errors="0">
    <testsuite name="phasekit-core" tests="2" skipped="0" errors="0" failures="0">
        <testcase name="input::tests::typed" classname="phasekit-core" time="0.002"/>
        <testcase name="flash::tests::dt" classname="phasekit-core" time="0.004"/>
    </testsuite>
    <testsuite name="phasekit-verify::lazy_load" tests="2" skipped="0" errors="0" failures="1">
        <testcase name="first_touch" classname="phasekit-verify::lazy_load" time="0.011"/>
        <testcase name="released" classname="phasekit-verify::lazy_load" time="0.020">
            <failure type="test failure">thread panicked</failure>
        </testcase>
    </testsuite>
    <testsuite name="phasekit-xtask::bin/phasekit-xtask" tests="1" skipped="0" errors="0" failures="0">
        <testcase name="gates::counts::tests::x" classname="phasekit-xtask::bin/phasekit-xtask" time="0.1"/>
    </testsuite>
</testsuites>
"#;

    /// `cargo test --doc --workspace` output, stdout and stderr merged (trimmed from a real run).
    const DOC: &str = "   Compiling phasekit-core v0.0.0 (/w/crates/phasekit-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.00s
   Doc-tests phasekit_core

running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests phasekit_data

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
";

    fn packages() -> Vec<String> {
        ["phasekit-core", "phasekit-data", "phasekit-verify", "phasekit-xtask"].map(String::from).to_vec()
    }

    fn ran(counts: &[(&str, usize)]) -> Vec<(String, usize)> {
        counts.iter().map(|&(binary, n)| (binary.to_string(), n)).collect()
    }

    /// Passed tests per binary from nextest's JUnit report, named as the manifest names them; a failed test case
    /// does not count, and a case without a binary id is an error, never a count.
    #[test]
    fn junit_report_counts_passed_tests_per_binary() {
        let want = ran(&[("phasekit-core::lib", 2), ("phasekit-verify::lazy_load", 1), ("phasekit-xtask::main", 1)]);
        assert_eq!(junit_counts(JUNIT), Ok(want));
        let errored = JUNIT.replace("<failure type=\"test failure\">thread panicked</failure>", "<error/>");
        assert_eq!(junit_counts(&errored).unwrap()[1], ("phasekit-verify::lazy_load".to_string(), 1));
        let skipped = JUNIT.replace(r#"time="0.004"/>"#, r#"time="0.004"><skipped/></testcase>"#);
        assert_eq!(junit_counts(&skipped).unwrap()[0], ("phasekit-core::lib".to_string(), 1));
        assert_eq!(junit_counts("<testsuites/>"), Ok(Vec::new()));
        assert!(junit_counts(r#"<testcase name="x" time="0.1"/>"#).is_err(), "no classname");
        assert!(junit_counts("<testcase name=\"x\"").is_err(), "unterminated");
        assert_eq!(
            binary_of("phasekit-xtask::bin/other"),
            "phasekit-xtask::bin/other",
            "a second binary keeps its name"
        );
    }

    /// Doctests per crate from libtest's summary lines; a crate with no result line, a failed run or a crate no
    /// member builds is an error, never a count.
    #[test]
    fn doctests_are_counted_from_libtest_summaries() {
        assert_eq!(doc_counts(DOC, &packages()), Ok(ran(&[("phasekit-core::doc", 2), ("phasekit-data::doc", 0)])));
        assert!(doc_counts("   Doc-tests phasekit_core\n", &packages()).is_err());
        assert!(doc_counts(&DOC.replace("ok. 2 passed; 0 failed", "FAILED. 1 passed; 1 failed"), &packages()).is_err());
        assert!(doc_counts(&DOC.replace("phasekit_data", "phasekit_mix"), &packages()).is_err());
        assert!(doc_counts("test result: ok. 1 passed;\n", &packages()).is_err(), "a result before any crate");
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

    /// A failed run is the gate's verdict, its command and the last 20 lines of its output; a passed one is nothing.
    #[test]
    fn a_failed_run_reports_its_command_and_tail() {
        let output: String = (1..=25).map(|i| format!("line {i}\n")).collect();
        assert_eq!(failed(true, &["nextest", "run"], &output), Ok(()));
        let errors = failed(false, &["nextest", "run"], &output).unwrap_err();
        assert_eq!(errors.len(), 21);
        let ends = (errors[0].as_str(), errors[1].as_str(), errors[20].as_str());
        assert_eq!(ends, ("cargo nextest run failed:", "line 6", "line 25"));
    }

    /// G3 and G4 (PLAN.md §2.4): the host runs every crate; wasip2 runs all but the host-only xtask under wasmtime;
    /// nextest runs the tests and `cargo test --doc` the doctests.
    #[test]
    fn each_target_is_measured_by_its_own_test_command() {
        let words = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        let host = (words(&["nextest", "run", "--workspace"]), words(&["test", "--doc", "--workspace"]));
        assert_eq!(test_command(HOST, HOST), Some(host));
        let cross = ["--workspace", "--exclude", "phasekit-xtask", "--target", "wasm32-wasip2"];
        let wasip2 =
            (words(&[&["nextest", "run"][..], &cross].concat()), words(&[&["test", "--doc"][..], &cross].concat()));
        assert_eq!(test_command("wasm32-wasip2", HOST), Some(wasip2));
        assert_eq!(test_command("x86_64-pc-windows-msvc", HOST), None, "no runner for Windows on this host");
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
