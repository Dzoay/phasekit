//! `gates deps`: the zero-dependency guard (dependencies §3.4 item 1; ROT-156). `phasekit-core` depends on
//! `phasekit-data` only and `phasekit-data` on nothing, until M9 adds the optional `rayon` and `libm`.

use super::{Verdict, no_args};
use crate::repo::Repo;

/// Each guarded crate and the crates its normal and build dependency tree may name (itself included).
const ALLOWED: [(&str, &[&str]); 2] =
    [("phasekit-core", &["phasekit-core", "phasekit-data"]), ("phasekit-data", &["phasekit-data"])];

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let mut summary = Vec::new();
    for (package, _) in ALLOWED {
        let (ok, tree) = repo
            .cargo(&["tree", "-p", package, "-e", "normal,build", "--depth", "1", "--prefix", "none"])
            .map_err(|e| vec![e])?;
        if !ok {
            return Err(vec![format!("cargo tree -p {package} failed")]);
        }
        let deps = check(package, &tree)?;
        summary.push(format!("{package} -> {}", if deps.is_empty() { "nothing".into() } else { deps.join(", ") }));
    }
    Ok(summary.join("; "))
}

/// Checks `cargo tree -p <package> -e normal,build --depth 1 --prefix none` output; returns the direct dependencies.
fn check(package: &str, tree: &str) -> Result<Vec<String>, Vec<String>> {
    let Some(&(_, allowed)) = ALLOWED.iter().find(|(name, _)| *name == package) else {
        return Err(vec![format!("{package}: not a crate the zero-dependency guard covers")]);
    };
    let name = |line: &str| line.split_whitespace().next().unwrap_or_default().to_string();
    let mut lines = tree.lines().map(str::trim).filter(|line| !line.is_empty());
    if lines.next().map(name).as_deref() != Some(package) {
        return Err(vec![format!("cargo tree -p {package} does not start with {package}: {tree:?}")]);
    }
    let (mut deps, mut errors) = (Vec::new(), Vec::new());
    for line in lines {
        let dep = name(line);
        if !allowed.contains(&dep.as_str()) {
            errors.push(format!("{package} may not depend on {line} (dependencies §3.4 item 1)"));
        } else if dep != package {
            deps.push(dep);
        }
    }
    if errors.is_empty() { Ok(deps) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str =
        "phasekit-core v0.0.0 (/w/crates/phasekit-core)\nphasekit-data v0.0.0 (/w/crates/phasekit-data)\n";

    /// Rot: ROT-156. Dependencies §3.4 item 1: the core's tree names only workspace crates.
    #[test]
    fn zero_deps_rejects_a_third_party_crate() {
        assert_eq!(check("phasekit-core", TREE), Ok(vec!["phasekit-data".to_string()]));
        let errors = check("phasekit-core", &format!("{TREE}libm v0.2.15\n")).unwrap_err();
        assert!(errors.len() == 1 && errors[0].contains("libm v0.2.15"), "{errors:?}");
        // phasekit-data depends on nothing, not even another workspace crate.
        assert_eq!(check("phasekit-data", "phasekit-data v0.0.0 (/w/crates/phasekit-data)\n"), Ok(vec![]));
        assert!(check("phasekit-data", "phasekit-data v0.0.0 (/w)\nphasekit-core v0.0.0 (/w)\n").is_err());
        // Fail closed: no output, a tree of another crate, or a crate the guard does not know.
        assert!(check("phasekit-core", "").is_err());
        assert!(check("phasekit-core", "phasekit-data v0.0.0 (/w)\n").is_err());
        assert!(check("phasekit-verify", "phasekit-verify v0.0.0 (/w)\n").is_err());
    }
}
