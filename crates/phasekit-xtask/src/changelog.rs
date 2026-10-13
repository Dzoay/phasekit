//! `cargo xtask changelog <version> <date> [<rev>]`: a new section of `CHANGELOG.md` from the Conventional Commit
//! subjects since the last `v*` tag up to `rev` (`HEAD`; a release PR names `origin/main`, whose commits its squash
//! merge follows), all of them before the first tag (CONTRIBUTING.md "Versions"), grouped by type, oldest first, above
//! the sections already there.

use std::process::ExitCode;

use crate::repo::Repo;

const CHANGELOG: &str = "CHANGELOG.md";

const HEADER: &str = "# Changelog\n\nEach release's Conventional Commit subjects since the one before, by type \
                      (`cargo xtask changelog`; CONTRIBUTING.md \"Versions\").\n";

/// The commit types in the order the section lists them, with their headings; a subject of no type is "Other".
const TYPES: [(&str, &str); 10] = [
    ("feat", "Features"),
    ("fix", "Fixes"),
    ("perf", "Performance"),
    ("refactor", "Refactoring"),
    ("docs", "Documentation"),
    ("test", "Tests"),
    ("build", "Build"),
    ("ci", "CI"),
    ("chore", "Chores"),
    ("revert", "Reverts"),
];

pub fn main(args: &[String]) -> ExitCode {
    let (version, date, rev) = match args {
        [version, date] => (version, date, "HEAD"),
        [version, date, rev] => (version, date, rev.as_str()),
        _ => {
            eprintln!("usage: cargo xtask changelog <version> <date> [<rev>], e.g. 0.1.0 2026-10-20 origin/main");
            return ExitCode::FAILURE;
        }
    };
    let repo = Repo::locate();
    match run(&repo, version, date, rev) {
        Ok(n) => {
            println!("{CHANGELOG}: {version} with {n} commits");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("changelog: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Writes the section of the commits up to `rev`, through `repo` (git and the file); returns how many subjects it lists.
fn run(repo: &Repo, version: &str, date: &str, rev: &str) -> Result<usize, String> {
    let last = repo.git(&["tag", "--list", "v*", "--sort=-creatordate"])?;
    let range = last.lines().next().map_or_else(|| rev.to_string(), |tag| format!("{tag}..{rev}"));
    let subjects = repo.git(&["log", "--reverse", "--format=%s", &range])?;
    let subjects: Vec<&str> = subjects.lines().filter(|line| !line.is_empty()).collect();
    let old = repo.read(CHANGELOG).unwrap_or_default();
    let previous = old.split_once("\n## ").map(|(_, rest)| format!("\n## {rest}")).unwrap_or_default();
    repo.write(CHANGELOG, &format!("{HEADER}\n{}{previous}", section(version, date, &subjects)))?;
    Ok(subjects.len())
}

/// One release's section: `## <version> (<date>)`, then each type's subjects under its heading, oldest first.
fn section(version: &str, date: &str, subjects: &[&str]) -> String {
    let mut out = format!("## {version} ({date})\n");
    let typed: Vec<(&str, String)> = subjects.iter().map(|subject| split(subject)).collect();
    let headings = TYPES.iter().map(|(kind, heading)| (Some(*kind), *heading)).chain([(None, "Other")]);
    for (kind, heading) in headings {
        let known = |k: &str| TYPES.iter().any(|(t, _)| *t == k);
        let lines: Vec<&str> = typed
            .iter()
            .filter(|(k, _)| kind.map_or(!known(k), |kind| *k == kind))
            .map(|(_, rest)| rest.as_str())
            .collect();
        if !lines.is_empty() {
            out.push_str(&format!("\n### {heading}\n\n"));
            out.extend(lines.iter().map(|line| format!("- {line}\n")));
        }
    }
    out
}

/// A subject's type and the rest: `feat(core): x` is ("feat", "core: x"), `fix!: y` is ("fix", "y (breaking)"); a
/// subject outside the grammar is ("", the subject).
fn split(subject: &str) -> (&str, String) {
    let Some((head, rest)) = subject.split_once(": ") else { return ("", subject.to_string()) };
    let (head, breaking) = head.strip_suffix('!').map_or((head, false), |head| (head, true));
    let (kind, scope) = head.split_once('(').map_or((head, None), |(kind, scope)| (kind, scope.strip_suffix(')')));
    if kind.is_empty() || !kind.bytes().all(|b| b.is_ascii_lowercase()) {
        return ("", subject.to_string());
    }
    let text = scope.map_or(rest.to_string(), |scope| format!("{scope}: {rest}"));
    (kind, if breaking { format!("{text} (breaking)") } else { text })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Subjects split by type and scope; the section lists each type's under its heading in [`TYPES`]' order, oldest
    /// first, and the rest under "Other".
    #[test]
    fn section_groups_subjects_by_type() {
        assert_eq!(split("feat(core): a"), ("feat", "core: a".to_string()));
        assert_eq!(split("fix!: b"), ("fix", "b (breaking)".to_string()));
        assert_eq!(split("Merge branch x"), ("", "Merge branch x".to_string()));
        assert_eq!(split("WIP: c"), ("", "WIP: c".to_string()));
        let section = section("0.1.0", "2026-10-20", &["fix(xtask): b", "feat: a", "feat(capi): c (#1)", "oops"]);
        let want = "## 0.1.0 (2026-10-20)\n\n### Features\n\n- a\n- capi: c (#1)\n\n### Fixes\n\n- xtask: b\n\n\
                    ### Other\n\n- oops\n";
        assert_eq!(section, want);
    }
}
