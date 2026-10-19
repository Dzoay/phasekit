//! `gates mutants`: every change is tested (ROT-294; user decisions TQ1, TQ2). Runs `cargo mutants --in-diff` over
//! the Rust changes since the merge base with `origin/main`, working tree included. A mutant of changed code that no
//! test catches (missed) fails the gate, unless `.cargo/mutants.toml` excludes it with a reason. A mutant that makes
//! the tests hang (timeout, typically a loop that no longer advances) did not pass them, so it counts as caught and
//! is listed. With no Rust changes there is nothing to mutate. The full run is weekly and report-only (M0.6).
//!
//! It runs in two passes (user decision CI5). No crate depends on phasekit-xtask and no other crate's tests run it,
//! so only its own tests can catch its mutants: they run those alone, not the whole workspace's, which made each one
//! wait about 50 s for tests that could not fail. Every other mutant runs the whole workspace's tests.
//!
//! `--shard k/n` runs the k-th of n slices of both passes, so CI spreads one PR's mutants over n jobs (user decision
//! CI6); together the n shards test every mutant once.

use super::{Verdict, exclusions};
use crate::repo::Repo;

/// The branch every PR is squash-merged into.
const BASE: &str = "origin/main";

/// Each pass: its name, which mutants it tests and on whose tests, and the cargo-mutants arguments that select them.
/// Together they cover every mutant once; each keeps `.cargo/mutants.toml`'s exclusions.
const PASSES: [(&str, &str, &[&str]); 2] = [
    ("workspace", "in the other crates, on the workspace's tests", &["--exclude", "crates/phasekit-xtask/**"]),
    ("xtask", "in phasekit-xtask, on its own tests", &["--package", "phasekit-xtask", "--test-workspace", "false"]),
];

/// What cargo-mutants wrote: one mutant per line in each of `caught.txt`, `missed.txt` and `timeout.txt`.
struct Outcomes<'a> {
    caught: &'a str,
    missed: &'a str,
    timeout: &'a str,
}

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    let shard = shard(args)?;
    // The markers and the toml's patterns are checked on every run, Rust changes or none (user decision LM2).
    let toml = repo.read(exclusions::TOML).map_err(|e| vec![e])?;
    let sources = repo.files("crates", ".rs", true).map_err(|e| vec![e])?;
    let (config, marked, excluded) = exclusions::generate(&toml, &sources, |args| repo.mutants_list(args))?;
    let config = repo.write_target("mutants.toml", config.as_bytes()).map_err(|e| vec![e])?;
    let base = repo.git(&["merge-base", "HEAD", BASE]).map_err(|e| vec![e])?;
    let mut diff = code_hunks(&repo.git(&["diff", "-U0", base.trim(), "--", "*.rs"]).map_err(|e| vec![e])?);
    // `git diff` leaves out files git does not track yet; a run before the first commit must mutate them too.
    for path in repo.git(&["ls-files", "--others", "--exclude-standard", "--", "*.rs"]).map_err(|e| vec![e])?.lines() {
        diff.push_str(&new_file_diff(path, &repo.read(path).map_err(|e| vec![e])?));
    }
    if diff.trim().is_empty() {
        return Ok(format!("no Rust code changes since {BASE}; {marked} markers exclude {excluded} mutants"));
    }
    let mut summary = format!("mutants of the changes since {BASE}:");
    let mut hung = Vec::new();
    for (pass, mutants, selection) in PASSES {
        let args: Vec<&str> =
            selection.iter().copied().chain(shard.iter().map(String::as_str)).chain(["--config", &config]).collect();
        let run = repo.cargo_mutants(pass, &diff, &args).map_err(|e| vec![e])?;
        let [caught, missed, timeout, unviable] = run.outcomes.each_ref().map(String::as_str);
        let (caught, timeouts) = check(run.success, &Outcomes { caught, missed, timeout }).map_err(|errors| {
            errors.into_iter().chain([format!("cargo mutants output: {}", run.output)]).collect::<Vec<_>>()
        })?;
        summary.push_str(&format!(
            " {mutants}, {caught} caught ({} by timeout), {} unviable;",
            timeouts.len(),
            lines(unviable).count()
        ));
        hung.extend(timeouts.into_iter().map(str::to_string));
    }
    summary.pop();
    summary.push_str(&format!("; {marked} markers exclude {excluded} mutants"));
    for mutant in hung {
        summary.push_str("\n  timeout: ");
        summary.push_str(&mutant);
    }
    Ok(summary)
}

/// The cargo-mutants arguments that select this run's shard: none without arguments, `--shard k/n` for one of n.
fn shard(args: &[String]) -> Result<Vec<String>, Vec<String>> {
    let [flag, slice] = args else {
        return if args.is_empty() {
            Ok(Vec::new())
        } else {
            Err(vec![format!("unexpected arguments: {}", args.join(" "))])
        };
    };
    let parsed = slice.split_once('/').and_then(|(k, n)| Some((k.parse::<u32>().ok()?, n.parse::<u32>().ok()?)));
    match parsed {
        Some((k, n)) if flag == "--shard" && k < n => Ok(vec![flag.clone(), slice.clone()]),
        _ => Err(vec![format!("expected --shard <k>/<n> with k < n, not: {flag} {slice}")]),
    }
}

/// Checks one cargo-mutants run (`success` is its exit status, which is also false after a timeout); returns how many
/// mutants were caught, timeouts included, and the timeouts.
fn check<'a>(success: bool, outcomes: &Outcomes<'a>) -> Result<(usize, Vec<&'a str>), Vec<String>> {
    let errors: Vec<String> =
        lines(outcomes.missed).map(|mutant| format!("missed (no test fails without this code): {mutant}")).collect();
    let hung: Vec<&str> = lines(outcomes.timeout).collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    if !success && hung.is_empty() {
        return Err(vec!["cargo mutants failed without naming a mutant; did the unmutated tests pass?".to_string()]);
    }
    Ok((lines(outcomes.caught).count() + hung.len(), hung))
}

/// The diff without its hunks that only add or remove comments and blank lines (`git diff -U0`'s hunks, so each is one
/// change): a comment changes no mutant, yet cargo-mutants tests every whole-function mutant of a function a changed
/// line falls in, and a marker (PLAN.md M25.4) is such a comment. A file whose hunks all leave leaves with its header.
fn code_hunks(diff: &str) -> String {
    let (mut out, mut header, mut hunk, mut code) = (String::new(), String::new(), String::new(), false);
    let mut finish = |header: &mut String, hunk: &mut String, code: bool| {
        if code && !hunk.is_empty() {
            out.push_str(&std::mem::take(header));
            out.push_str(hunk);
        }
        hunk.clear();
    };
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            finish(&mut header, &mut hunk, code);
            header = format!("{line}\n");
        } else if line.starts_with("@@") {
            finish(&mut header, &mut hunk, code);
            hunk = format!("{line}\n");
            code = false;
        } else if hunk.is_empty() {
            header.push_str(line);
            header.push('\n');
        } else {
            hunk.push_str(line);
            hunk.push('\n');
            let changed = line.strip_prefix(['+', '-']).map(str::trim);
            code |= changed.is_some_and(|text| !text.is_empty() && !text.starts_with("//"));
        }
    }
    finish(&mut header, &mut hunk, code);
    out
}

/// A unified diff that adds `path` with `text`, as `git diff` would show a new file.
fn new_file_diff(path: &str, text: &str) -> String {
    let count = text.lines().count();
    if count == 0 {
        return String::new();
    }
    let mut diff = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{count} @@\n"
    );
    for line in text.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAUGHT: &str = "crates/phasekit-xtask/src/gates/rot.rs:61:5: replace has_word -> bool with true\n";
    const MISSED: &str = "crates/phasekit-xtask/src/gates/rot.rs:70:9: replace < with <= in check\n";

    fn outcomes<'a>(missed: &'a str, timeout: &'a str) -> Outcomes<'a> {
        Outcomes { caught: CAUGHT, missed, timeout }
    }

    /// Rot: ROT-294. A change no test notices is untested, however many tests run (user decision TQ2).
    #[test]
    fn a_surviving_mutant_fails_the_gate() {
        assert_eq!(check(true, &outcomes("", "")), Ok((1, vec![])));
        let errors = check(false, &outcomes(MISSED, "")).unwrap_err();
        assert!(errors.len() == 1 && errors[0].contains("missed") && errors[0].contains("rot.rs:70:9"), "{errors:?}");
        // A mutant that hangs the tests did not pass them: caught, and listed (cargo mutants exits non-zero then).
        assert_eq!(check(false, &outcomes("", MISSED)), Ok((2, vec![MISSED.trim()])));
        // Fail closed: a failed run that names no mutant (the unmutated tests failed, say) is not a pass.
        assert!(check(false, &outcomes("", "")).is_err());
        assert!(check(true, &outcomes(MISSED, "")).is_err());
    }

    /// User decision CI6: a shard is `k/n` with k < n, passed on to cargo-mutants; anything else is refused.
    #[test]
    fn a_shard_selects_one_slice_of_the_mutants() {
        let args = |args: &[&str]| args.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(shard(&[]), Ok(vec![]));
        assert_eq!(shard(&args(&["--shard", "0/4"])), Ok(args(&["--shard", "0/4"])));
        assert_eq!(shard(&args(&["--shard", "3/4"])), Ok(args(&["--shard", "3/4"])));
        for bad in [
            &["--shard", "4/4"][..],
            &["--shard", "1/0"],
            &["--shard", "a/4"],
            &["--shard", "1"],
            &["--shard"],
            &["--jobs", "2"],
        ] {
            assert!(shard(&args(bad)).is_err(), "{bad:?}");
        }
    }

    /// A comment changes no mutant (PLAN.md M25.4): a hunk that only adds or removes comments and blank lines leaves
    /// the diff, with its file's header when it was the file's only hunk; a hunk that changes code stays whole.
    #[test]
    fn comment_only_hunks_leave_the_diff() {
        let header =
            |path: &str| format!("diff --git a/{path} b/{path}\nindex 1..2 100644\n--- a/{path}\n+++ b/{path}\n");
        let comments = "@@ -3,0 +4,2 @@ fn f() {\n+    // Why.\n+    // equivalent mutant: replace < with <=\n";
        let blank = "@@ -9,1 +11,1 @@ fn g() {\n-    // old\n+\n";
        let code = "@@ -20,1 +22,2 @@ fn h() {\n-    a < b\n+    // Why.\n+    a <= b\n";
        let diff = format!("{}{comments}{blank}{code}{}{comments}", header("a.rs"), header("b.rs"));
        assert_eq!(code_hunks(&diff), format!("{}{code}", header("a.rs")));
        assert_eq!(code_hunks(&format!("{}{comments}", header("b.rs"))), "");
        assert_eq!(code_hunks(""), "");
    }

    #[test]
    fn untracked_files_join_the_diff() {
        let want = "diff --git a/src/new.rs b/src/new.rs\nnew file mode 100644\n--- /dev/null\n+++ b/src/new.rs\n@@ -0,0 +1,2 @@\n+fn a() {}\n+fn b() {}\n";
        assert_eq!(new_file_diff("src/new.rs", "fn a() {}\nfn b() {}\n"), want);
        assert_eq!(new_file_diff("src/empty.rs", ""), "");
    }
}
