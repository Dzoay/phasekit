//! `gates deps`: the zero-dependency guard (dependencies §3.4 item 1; ROT-156). `phasekit-core` depends on
//! `phasekit-data` only and `phasekit-data` on nothing; core's optional executors (PLAN.md M9.1) add their own crate
//! and nothing else.

use super::{Verdict, no_args};
use crate::repo::Repo;

/// Each guarded crate, the features it is built with, and exactly the crates its normal and build dependency tree
/// names at depth 1 then (itself included).
const GUARDED: [(&str, &str, &[&str]); 3] = [
    ("phasekit-core", "", &["phasekit-core", "phasekit-data"]),
    ("phasekit-core", "rayon", &["phasekit-core", "phasekit-data", "rayon"]),
    ("phasekit-data", "", &["phasekit-data"]),
];

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let mut summary = Vec::new();
    for (package, features, _) in GUARDED {
        let mut cargo = vec!["tree", "-p", package, "-e", "normal,build", "--depth", "1", "--prefix", "none"];
        if !features.is_empty() {
            cargo.extend(["--features", features]);
        }
        let (ok, tree) = repo.cargo(&cargo).map_err(|e| vec![e])?;
        if !ok {
            return Err(vec![format!("cargo tree -p {package} --features '{features}' failed")]);
        }
        let deps = check(package, features, &tree)?;
        let with = if features.is_empty() { String::new() } else { format!(" (--features {features})") };
        summary
            .push(format!("{package}{with} -> {}", if deps.is_empty() { "nothing".into() } else { deps.join(", ") }));
    }
    Ok(summary.join("; "))
}

/// Checks `cargo tree -p <package> -e normal,build --depth 1 --prefix none [--features <features>]` output against
/// [`GUARDED`]: every crate it names is listed and every listed crate is named. Returns the direct dependencies.
fn check(package: &str, features: &str, tree: &str) -> Result<Vec<String>, Vec<String>> {
    let Some(&(_, _, allowed)) = GUARDED.iter().find(|(name, with, _)| *name == package && *with == features) else {
        return Err(vec![format!("{package} --features '{features}': not a build the zero-dependency guard covers")]);
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
    for missing in allowed.iter().filter(|crate_| **crate_ != package && !deps.iter().any(|dep| dep == *crate_)) {
        errors.push(format!("{package} --features '{features}' no longer depends on {missing}: update the guard"));
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
        assert_eq!(check("phasekit-core", "", TREE), Ok(vec!["phasekit-data".to_string()]));
        let errors = check("phasekit-core", "", &format!("{TREE}libm v0.2.15\n")).unwrap_err();
        assert!(errors.len() == 1 && errors[0].contains("libm v0.2.15"), "{errors:?}");
        // phasekit-data depends on nothing, not even another workspace crate.
        assert_eq!(check("phasekit-data", "", "phasekit-data v0.0.0 (/w/crates/phasekit-data)\n"), Ok(vec![]));
        assert!(check("phasekit-data", "", "phasekit-data v0.0.0 (/w)\nphasekit-core v0.0.0 (/w)\n").is_err());
        // Fail closed: no output, a tree of another crate, or a crate the guard does not know.
        assert!(check("phasekit-core", "", "").is_err());
        assert!(check("phasekit-core", "", "phasekit-data v0.0.0 (/w)\n").is_err());
        assert!(check("phasekit-verify", "", "phasekit-verify v0.0.0 (/w)\n").is_err());
    }

    /// PLAN.md M9.1 (dependencies T1): `--features rayon` adds exactly `rayon` to core's direct dependencies, and the
    /// default build still has none of it; a build the guard does not list fails closed.
    #[test]
    fn the_rayon_feature_adds_exactly_rayon() {
        let with_rayon = format!("{TREE}rayon v1.12.0\n");
        assert_eq!(check("phasekit-core", "rayon", &with_rayon), Ok(vec!["phasekit-data".into(), "rayon".into()]));
        let errors = check("phasekit-core", "rayon", &format!("{with_rayon}libm v0.2.16\n")).unwrap_err();
        assert!(errors.len() == 1 && errors[0].contains("libm v0.2.16"), "{errors:?}");
        let errors = check("phasekit-core", "rayon", TREE).unwrap_err();
        assert!(errors.len() == 1 && errors[0].contains("no longer depends on rayon"), "{errors:?}");
        assert!(check("phasekit-core", "", &with_rayon).is_err());
        assert!(check("phasekit-core", "simd", TREE).is_err());
    }
}
