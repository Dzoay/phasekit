//! `gates register` (VERIFICATION.md §7.2): the divergence register against the shipped corrections
//! (`data/corrections.csv`, `check_register`), and every `Fix::Code` module named by an entry whose proof is due exists.

use phasekit_verify::{Divergence, Fix, MILESTONE};

use super::{Verdict, no_args};
use crate::repo::Repo;

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let corrections = repo.read(crate::datagen::corrections::CORRECTIONS).map_err(|e| vec![e])?;
    let rows = crate::datagen::corrections::parse(&corrections).map_err(|e| vec![e])?;
    let patches: Vec<_> = rows.into_iter().map(|row| row.patch).collect();
    let register = phasekit_verify::DIVERGENCES;
    phasekit_verify::check_register(register, &patches).map_err(|e| vec![format!("check_register: {e:?}")])?;
    let files = repo.files("crates", ".rs", true).map_err(|e| vec![e])?;
    let manifests = repo.members().map_err(|e| vec![e])?;
    let exists = |path: &str| {
        files.iter().any(|(p, _)| p == path) || manifests.iter().any(|m| format!("{}/Cargo.toml", m.dir) == path)
    };
    let errors = missing_modules(register, MILESTONE, &exists);
    if errors.is_empty() {
        Ok(format!("{} entries, {} corrections, code paths of due entries exist", register.len(), patches.len()))
    } else {
        Err(errors)
    }
}

/// The files that would hold a module path: `phasekit_core::state` → `crates/phasekit-core/src/state.rs` or
/// `…/state/mod.rs`; a crate alone → its `Cargo.toml`.
fn module_files(module: &str) -> Vec<String> {
    let mut segments = module.split("::");
    let dir = format!("crates/{}", segments.next().unwrap_or_default().replace('_', "-"));
    let path: Vec<&str> = segments.collect();
    if path.is_empty() {
        return vec![format!("{dir}/Cargo.toml")];
    }
    let path = path.join("/");
    vec![format!("{dir}/src/{path}.rs"), format!("{dir}/src/{path}/mod.rs")]
}

/// Entries whose proof is due (a milestone below `milestone`) and whose `Fix::Code` module has no file.
fn missing_modules(register: &[Divergence], milestone: u8, exists: &dyn Fn(&str) -> bool) -> Vec<String> {
    register
        .iter()
        .filter(|d| d.proof.iter().any(|m| *m < milestone))
        .filter_map(|d| match d.fix {
            Fix::Code(module) if !module_files(module).iter().any(|file| exists(file)) => Some(format!(
                "{}: Fix::Code module `{module}` does not exist ({})",
                d.id,
                module_files(module).join(" or ")
            )),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths_name_their_files() {
        assert_eq!(
            module_files("phasekit_core::state"),
            ["crates/phasekit-core/src/state.rs", "crates/phasekit-core/src/state/mod.rs"]
        );
        assert_eq!(
            module_files("phasekit_core::helmholtz::power"),
            ["crates/phasekit-core/src/helmholtz/power.rs", "crates/phasekit-core/src/helmholtz/power/mod.rs"]
        );
        assert_eq!(module_files("phasekit_cubic"), ["crates/phasekit-cubic/Cargo.toml"]);
    }

    /// VERIFICATION.md §7.2: a `fix: Code(path)` names a module that exists, once the entry's proof is due.
    #[test]
    fn a_due_code_fix_names_an_existing_module() {
        let entry =
            |id, module, proof| Divergence { id, fix: Fix::Code(module), proof, ..phasekit_verify::DIVERGENCES[3] };
        let register = [entry("DIV-0101", "phasekit_core::state", &[5]), entry("DIV-0102", "phasekit_cubic", &[11])];
        let exists = |path: &str| path == "crates/phasekit-core/src/state.rs";
        assert_eq!(
            missing_modules(&register, 6, &exists),
            Vec::<String>::new(),
            "the cubic crate is not due before M11"
        );
        let errors = missing_modules(&register, 12, &exists);
        assert!(
            errors.len() == 1 && errors[0].starts_with("DIV-0102") && errors[0].contains("phasekit_cubic"),
            "{errors:?}"
        );
        let nothing = |_: &str| false;
        assert_eq!(missing_modules(&register, 6, &nothing).len(), 1, "the state module is due at M5");
        assert!(missing_modules(&register, 5, &nothing).is_empty(), "but not while M5 is open");
        let data = [Divergence { proof: &[2], ..phasekit_verify::DIVERGENCES[0] }];
        assert!(missing_modules(&data, 19, &nothing).is_empty(), "only code fixes name modules");
    }
}
