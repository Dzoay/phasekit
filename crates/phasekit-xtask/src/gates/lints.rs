//! `gates lints`: every workspace member inherits the workspace lints, and `unsafe_code` is forbidden everywhere
//! except `phasekit-capi`, which may relax it to `deny` (BRIEF "memory safe"; D17).

use super::{Verdict, no_args, package_name, table, value};
use crate::repo::Repo;

/// The only crate allowed to relax `unsafe_code`, and only to `deny` (per-item `allow` with a SAFETY comment; M10.4).
const CAPI: &str = "phasekit-capi";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let workspace = repo.read("Cargo.toml").map_err(|e| vec![e])?;
    let members: Vec<(String, String)> = repo
        .members()
        .map_err(|e| vec![e])?
        .into_iter()
        .map(|member| (format!("{}/Cargo.toml", member.dir), member.manifest))
        .collect();
    let errors = check(&workspace, &members);
    if errors.is_empty() { Ok(format!("{} members inherit the workspace lints", members.len())) } else { Err(errors) }
}

/// Checks the root manifest and each member's (path, manifest text).
fn check(workspace: &str, members: &[(String, String)]) -> Vec<String> {
    let mut errors = Vec::new();
    if table(workspace, "workspace.lints.rust").and_then(|lints| value(&lints, "unsafe_code")) != Some("forbid") {
        errors.push("Cargo.toml: `[workspace.lints.rust]` must set `unsafe_code = \"forbid\"`".to_string());
    }
    if members.is_empty() {
        errors.push("no workspace members to check".to_string());
    }
    for (path, manifest) in members {
        let inherits = table(manifest, "lints").and_then(|lints| value(&lints, "workspace")) == Some("true");
        if inherits {
            continue;
        }
        if package_name(manifest) == Some(CAPI) {
            if table(manifest, "lints.rust").and_then(|lints| value(&lints, "unsafe_code")) != Some("deny") {
                errors.push(format!("{path}: {CAPI} may relax `unsafe_code` to \"deny\" and to nothing else"));
            }
        } else {
            errors.push(format!("{path}: no `[lints] workspace = true`; every member inherits the workspace lints"));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACE: &str = "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\nmissing_docs = \"warn\"\n";
    const INHERITS: &str = "[lints]\nworkspace = true\n";

    fn member(name: &str, lints: &str) -> (String, String) {
        (format!("crates/{name}/Cargo.toml"), format!("[package]\nname = \"{name}\"\n\n{lints}"))
    }

    #[test]
    fn a_crate_without_workspace_lints_is_rejected() {
        assert_eq!(check(WORKSPACE, &[member("phasekit-core", INHERITS)]), Vec::<String>::new());
        let errors = check(WORKSPACE, &[member("phasekit-core", INHERITS), member("phasekit-data", "[features]\n")]);
        assert!(errors.len() == 1 && errors[0].starts_with("crates/phasekit-data/Cargo.toml"), "{errors:?}");
        assert!(!check(WORKSPACE, &[member("phasekit-data", "[lints]\nworkspace = false\n")]).is_empty());
        assert!(!check(WORKSPACE, &[]).is_empty(), "no members is a failure, not a pass");
    }

    /// BRIEF "memory safe": `forbid` at the workspace; only phasekit-capi may relax `unsafe_code`, to `deny`.
    #[test]
    fn unsafe_code_is_forbidden_outside_capi() {
        let relaxed = "[lints.rust]\nunsafe_code = \"deny\"\n";
        assert!(!check(WORKSPACE, &[member("phasekit-core", relaxed)]).is_empty());
        assert_eq!(check(WORKSPACE, &[member(CAPI, relaxed)]), Vec::<String>::new());
        assert!(!check(WORKSPACE, &[member(CAPI, "[lints.rust]\nunsafe_code = \"allow\"\n")]).is_empty());
        let lax = WORKSPACE.replace("\"forbid\"", "\"deny\"");
        assert!(!check(&lax, &[member("phasekit-core", INHERITS)]).is_empty());
    }
}
