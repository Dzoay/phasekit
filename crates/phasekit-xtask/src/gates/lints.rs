//! `gates lints`: every workspace member inherits the workspace lints, and `unsafe_code` is forbidden everywhere
//! except `phasekit-capi`, which may relax it to `deny` (BRIEF "memory safe"; D17). Its `thread_local!` stays in one
//! file, its last error (D8, E13).

use super::{Verdict, no_args, package_name, table, value};
use crate::repo::Repo;

/// The only crate allowed to relax `unsafe_code`, and only to `deny` (per-item `allow` with a SAFETY comment; M10.4).
const CAPI: &str = "phasekit-capi";

/// The one file that may use `thread_local!`. clippy's `disallowed_macros` honours an allow only at a crate's root, so
/// phasekit-capi allows it there and this gate keeps it to this file (D8, E13; M10.4).
const PER_THREAD: &str = "crates/phasekit-capi/src/last_error.rs";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let workspace = repo.read("Cargo.toml").map_err(|e| vec![e])?;
    let members: Vec<(String, String)> = repo
        .members()
        .map_err(|e| vec![e])?
        .into_iter()
        .map(|member| (format!("{}/Cargo.toml", member.dir), member.manifest))
        .collect();
    let sources = repo.files("crates/phasekit-capi/src", ".rs", true).map_err(|e| vec![e])?;
    let mut errors = check(&workspace, &members);
    errors.extend(per_thread(&sources));
    if errors.is_empty() {
        Ok(format!("{} members inherit the workspace lints; `thread_local!` only in {PER_THREAD}", members.len()))
    } else {
        Err(errors)
    }
}

/// Each of phasekit-capi's (path, text) sources but [`PER_THREAD`] that uses `thread_local!`.
fn per_thread(sources: &[(String, String)]) -> Vec<String> {
    sources
        .iter()
        .filter(|(path, text)| path != PER_THREAD && text.contains("thread_local!"))
        .map(|(path, _)| format!("{path}: `thread_local!` outside {PER_THREAD}, the C ABI's one per-thread state"))
        .collect()
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

    /// D8, E13: phasekit-capi's crate-level allow of `thread_local!` reaches only its last error.
    #[test]
    fn thread_local_is_the_last_errors_alone() {
        let source = |path: &str, text: &str| (path.to_string(), text.to_string());
        let used = "thread_local! {\n    static LAST: u8 = const { 0 };\n}\n";
        assert_eq!(
            per_thread(&[source(PER_THREAD, used), source("crates/phasekit-capi/src/lib.rs", "")]),
            Vec::<String>::new()
        );
        let errors = per_thread(&[source(PER_THREAD, used), source("crates/phasekit-capi/src/handles.rs", used)]);
        assert!(errors.len() == 1 && errors[0].starts_with("crates/phasekit-capi/src/handles.rs"), "{errors:?}");
    }
}
