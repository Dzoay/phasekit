//! `gates deps`: the zero-dependency guard (dependencies §3.4 item 1; ROT-156). `phasekit-core` depends on
//! `phasekit-data` only and `phasekit-data` on nothing; core's optional features (`rayon`, PLAN.md M9.1; `libm`,
//! M9.3) each add their own crate and nothing else, and `simd` (M12.4) adds `fearless_simd` and the `libm` it enables.
//! A family crate depends on `phasekit-core` alone (`phasekit-cubic`, PLAN.md M11.1; dependencies, the family row).

use super::{Verdict, no_args};
use crate::repo::Repo;

/// Each guarded crate, the features it is built with, and exactly the crates its normal and build dependency tree
/// names at depth 1 then (itself included).
const GUARDED: [(&str, &str, &[&str]); 8] = [
    ("phasekit-core", "", &["phasekit-core", "phasekit-data"]),
    ("phasekit-core", "rayon", &["phasekit-core", "phasekit-data", "rayon"]),
    ("phasekit-core", "libm", &["phasekit-core", "phasekit-data", "libm"]),
    ("phasekit-core", "rayon,libm", &["phasekit-core", "phasekit-data", "rayon", "libm"]),
    ("phasekit-core", "simd", &["phasekit-core", "phasekit-data", "libm", "fearless_simd"]),
    ("phasekit-core", "rayon,simd", &["phasekit-core", "phasekit-data", "rayon", "libm", "fearless_simd"]),
    ("phasekit-data", "", &["phasekit-data"]),
    ("phasekit-cubic", "", &["phasekit-cubic", "phasekit-core"]),
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
        assert!(check("phasekit-core", "rayon,libm,simd", TREE).is_err());
    }

    /// PLAN.md M9.3: `--features libm` adds exactly `libm`, and `--features rayon,libm` exactly the two.
    #[test]
    fn the_libm_feature_adds_exactly_libm() {
        let with_libm = format!("{TREE}libm v0.2.16\n");
        assert_eq!(check("phasekit-core", "libm", &with_libm), Ok(vec!["phasekit-data".into(), "libm".into()]));
        let both = format!("{with_libm}rayon v1.12.0\n");
        assert_eq!(check("phasekit-core", "rayon,libm", &both).map(|d| d.len()), Ok(3));
        assert!(check("phasekit-core", "libm", &both).unwrap_err()[0].contains("rayon v1.12.0"));
        assert!(
            check("phasekit-core", "rayon,libm", &with_libm).unwrap_err()[0].contains("no longer depends on rayon")
        );
    }

    /// PLAN.md M11.1 (dependencies, the family-crate row): `phasekit-cubic` depends on `phasekit-core` alone; a
    /// third-party crate or another workspace crate fails the guard by name.
    #[test]
    fn the_cubic_family_depends_on_core_alone() {
        let tree = "phasekit-cubic v0.0.0 (/w/crates/phasekit-cubic)\nphasekit-core v0.0.0 (/w/crates/phasekit-core)\n";
        assert_eq!(check("phasekit-cubic", "", tree), Ok(vec!["phasekit-core".to_string()]));
        let errors = check("phasekit-cubic", "", &format!("{tree}libm v0.2.16\n")).unwrap_err();
        assert!(errors.len() == 1 && errors[0].contains("libm v0.2.16"), "{errors:?}");
        let compat = format!("{tree}phasekit-compat v0.0.0 (/w/crates/phasekit-compat)\n");
        assert!(check("phasekit-cubic", "", &compat).unwrap_err()[0].contains("phasekit-compat"));
    }

    /// PLAN.md M12.4 (dependencies T1): `--features simd` adds exactly `fearless_simd` and the `libm` it enables (user
    /// decision SL1), and `--features rayon,simd` `rayon` too; the default and `libm` builds have no `fearless_simd`.
    #[test]
    fn the_simd_feature_adds_fearless_simd_and_libm() {
        let with_simd = format!("{TREE}fearless_simd v1.1.0\nlibm v0.2.16\n");
        assert_eq!(
            check("phasekit-core", "simd", &with_simd),
            Ok(vec!["phasekit-data".into(), "fearless_simd".into(), "libm".into()])
        );
        assert_eq!(
            check("phasekit-core", "rayon,simd", &format!("{with_simd}rayon v1.12.0\n")).map(|d| d.len()),
            Ok(4)
        );
        let errors = check("phasekit-core", "simd", &format!("{TREE}fearless_simd v1.1.0\n")).unwrap_err();
        assert!(errors.len() == 1 && errors[0].contains("no longer depends on libm"), "{errors:?}");
        assert!(check("phasekit-core", "", &with_simd).is_err());
        assert!(check("phasekit-core", "libm", &with_simd).unwrap_err()[0].contains("fearless_simd v1.1.0"));
    }
}
