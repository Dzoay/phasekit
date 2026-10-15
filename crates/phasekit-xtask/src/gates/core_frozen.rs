//! `gates core-frozen`: a family milestone's zero-line core diff (VERIFICATION.md §10; D3; PLAN.md M11.1). Nothing under
//! `crates/phasekit-core/` may differ from the milestone's start tag: no committed, staged, unstaged or untracked change.
//! The tag is `--since <tag>`'s, or else the one `ci/core-frozen.txt` names; between family milestones the file names
//! none and the gate has nothing to hold.

use crate::repo::Repo;

use super::Verdict;

/// Names the open family milestone's start tag, or none.
const FROZEN: &str = "ci/core-frozen.txt";

/// The tree the gate holds.
const CORE: &str = "crates/phasekit-core/";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    let tag = match args {
        [] => tag(&repo.read(FROZEN).map_err(|e| vec![e])?)?,
        [flag, tag] if flag == "--since" => Some(tag.clone()),
        _ => return Err(vec![format!("unexpected arguments: {} (core-frozen takes --since <tag>)", args.join(" "))]),
    };
    let Some(tag) = tag else {
        return Ok(format!("no family milestone holds the core ({FROZEN} names no tag)"));
    };
    let commit = format!("{tag}^{{commit}}");
    repo.git(&["rev-parse", "--verify", "--quiet", &commit])
        .map_err(|_| vec![format!("the tag {tag} does not exist here (fetch tags, or fix {FROZEN})")])?;
    let mut changed = repo.git(&["diff", "--name-only", &tag, "--", CORE]).map_err(|e| vec![e])?;
    changed.push_str(&repo.git(&["ls-files", "--others", "--exclude-standard", "--", CORE]).map_err(|e| vec![e])?);
    check(&tag, &changed)
}

/// The tag [`FROZEN`] names: its one line that is not blank or a `#` comment, or `None` when it names none.
fn tag(text: &str) -> Result<Option<String>, Vec<String>> {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#'));
    match (lines.next(), lines.next()) {
        (None, _) => Ok(None),
        (Some(tag), None) if !tag.contains(char::is_whitespace) => Ok(Some(tag.to_string())),
        _ => Err(vec![format!("{FROZEN}: one tag or none expected, beside # comments")]),
    }
}

/// Passes when `changed`, the paths `git diff --name-only` and `git ls-files --others` list, names nothing under
/// [`CORE`]; otherwise each such path is a violation.
fn check(tag: &str, changed: &str) -> Verdict {
    let core: Vec<String> = changed
        .lines()
        .map(str::trim)
        .filter(|path| path.starts_with(CORE))
        .map(|path| format!("{path} differs from {tag} (a zero-line core diff, D3)"))
        .collect();
    if core.is_empty() { Ok(format!("{CORE} is unchanged since {tag}")) } else { Err(core) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D3, PLAN.md M11.1: a change under `crates/phasekit-core/`, committed or not, fails the gate by file; changes
    /// elsewhere pass, and so does no change at all.
    #[test]
    fn a_core_change_fails_by_file() {
        assert_eq!(check("m11-start", ""), Ok("crates/phasekit-core/ is unchanged since m11-start".to_string()));
        let elsewhere = "crates/phasekit-cubic/src/lib.rs\ndocs/PLAN.md\n";
        assert!(check("m11-start", elsewhere).is_ok());
        let core =
            "crates/phasekit-cubic/src/lib.rs\ncrates/phasekit-core/src/fluid.rs\ncrates/phasekit-core/src/new.rs\n";
        assert_eq!(
            check("m11-start", core),
            Err(vec![
                "crates/phasekit-core/src/fluid.rs differs from m11-start (a zero-line core diff, D3)".to_string(),
                "crates/phasekit-core/src/new.rs differs from m11-start (a zero-line core diff, D3)".to_string(),
            ])
        );
    }

    /// `ci/core-frozen.txt` names one tag beside `#` comments, or none between family milestones; two tags, or a tag
    /// with spaces, fail closed.
    #[test]
    fn the_file_names_one_tag_or_none() {
        assert_eq!(tag("# a reason\n\nm11-start\n"), Ok(Some("m11-start".to_string())));
        assert_eq!(tag("# between family milestones\n"), Ok(None));
        assert_eq!(tag(""), Ok(None));
        assert!(tag("m11-start\nm14-start\n").is_err());
        assert!(tag("m11 start\n").is_err());
    }
}
