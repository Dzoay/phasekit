//! `gates features`: the fluid set is forwarded, never implied (E7; VERIFICATION.md §11.2). `phasekit-compat` built
//! with `--no-default-features --features fluids-core` resolves to the three workspace crates, compat and core with
//! `embedded` and `fluids-core` only, and `phasekit-data` with `core` and exactly the fluids its `core` list names.

use super::{Verdict, no_args, table};
use crate::repo::Repo;

/// `cargo tree` over compat's `fluids-core` build, one `<package> <version> (<path>) [<features>]` line per crate.
const TREE: [&str; 12] = [
    "tree",
    "-p",
    "phasekit-compat",
    "--no-default-features",
    "--features",
    "fluids-core",
    "-e",
    "normal",
    "-f",
    "{p} [{f}]",
    "--prefix",
    "none",
];

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let core = core_list(&repo.read("crates/phasekit-data/Cargo.toml").map_err(|e| vec![e])?)?;
    let (ok, tree) = repo.cargo(&TREE).map_err(|e| vec![e])?;
    if !ok {
        return Err(vec!["cargo tree -p phasekit-compat --no-default-features --features fluids-core failed".into()]);
    }
    check(&tree, &core)?;
    Ok(format!("phasekit-compat --features fluids-core embeds {} fluids: {}", core.len(), core.join(", ")))
}

/// The `core = [...]` list of phasekit-data's `[features]`, sorted.
fn core_list(manifest: &str) -> Result<Vec<String>, Vec<String>> {
    let missing = || vec!["phasekit-data's [features] has no `core = [...]` list".to_string()];
    let lines = table(manifest, "features").ok_or_else(missing)?;
    let start = lines
        .iter()
        .position(|line| line.split_once('=').is_some_and(|(key, _)| key.trim() == "core"))
        .ok_or_else(missing)?;
    let text = lines[start..].join("\n");
    let list = text.split_once('[').and_then(|(_, rest)| rest.split_once(']')).ok_or_else(missing)?.0;
    let mut fluids: Vec<String> =
        list.split(',').map(|item| item.trim().trim_matches('"').to_string()).filter(|item| !item.is_empty()).collect();
    fluids.sort();
    if fluids.is_empty() { Err(missing()) } else { Ok(fluids) }
}

/// Checks `cargo tree` output in the [`TREE`] format against the `core` list.
fn check(tree: &str, core: &[String]) -> Result<(), Vec<String>> {
    let (mut errors, mut seen) = (Vec::new(), Vec::new());
    for line in tree.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let name = line.split_whitespace().next().unwrap_or_default();
        let features = line.rsplit_once('[').and_then(|(_, rest)| rest.strip_suffix(']'));
        let mut features: Vec<&str> = features.unwrap_or_default().split(',').filter(|f| !f.is_empty()).collect();
        features.sort_unstable();
        seen.push(name);
        let want: Vec<&str> = match name {
            "phasekit-compat" | "phasekit-core" => vec!["embedded", "fluids-core"],
            "phasekit-data" => {
                let mut want: Vec<&str> = core.iter().map(String::as_str).chain(["core"]).collect();
                want.sort_unstable();
                want
            }
            _ => {
                errors.push(format!("the fluids-core build of phasekit-compat pulls in {line}"));
                continue;
            }
        };
        let extra: Vec<&str> = features.iter().filter(|f| !want.contains(f)).copied().collect();
        let absent: Vec<&str> = want.iter().filter(|f| !features.contains(f)).copied().collect();
        if !extra.is_empty() {
            errors.push(format!("{name} enables {} beyond the fluids-core set (E7)", extra.join(", ")));
        }
        if !absent.is_empty() {
            errors.push(format!("{name} lacks {} of the fluids-core set", absent.join(", ")));
        }
    }
    for name in ["phasekit-compat", "phasekit-core", "phasekit-data"] {
        if !seen.contains(&name) {
            errors.push(format!("cargo tree does not list {name}: {tree:?}"));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &str = "[package]\nname = \"phasekit-data\"\n\n[features]\ndefault = [\"all\"]\nall = [\n    \"fluid-air\",\n    \"fluid-water\",\n]\ncore = [\n    \"fluid-water\",\n    \"fluid-nitrogen\",\n]\nfluid-air = []\n";

    fn tree(data: &str) -> String {
        format!(
            "phasekit-compat v0.0.0 (/w/crates/phasekit-compat) [embedded,fluids-core]\nphasekit-core v0.0.0 \
             (/w/crates/phasekit-core) [embedded,fluids-core]\nphasekit-data v0.0.0 (/w/crates/phasekit-data) [{data}]\n"
        )
    }

    /// E7: compat's fluids-core build embeds the `core` list and nothing more. An extra fluid, the `all` set, a
    /// third-party crate or a missing workspace crate fails the gate, by name.
    #[test]
    fn fluids_core_feature_tree_has_no_extra_fluids() {
        let core = core_list(DATA).unwrap();
        assert_eq!(core, ["fluid-nitrogen", "fluid-water"]);
        assert_eq!(check(&tree("core,fluid-nitrogen,fluid-water"), &core), Ok(()));
        let extra = check(&tree("core,fluid-air,fluid-nitrogen,fluid-water"), &core).unwrap_err();
        assert_eq!(extra, ["phasekit-data enables fluid-air beyond the fluids-core set (E7)"]);
        let all = check(&tree("all,core,fluid-nitrogen,fluid-water"), &core).unwrap_err();
        assert_eq!(all, ["phasekit-data enables all beyond the fluids-core set (E7)"]);
        let absent = check(&tree("core,fluid-water"), &core).unwrap_err();
        assert_eq!(absent, ["phasekit-data lacks fluid-nitrogen of the fluids-core set"]);
        let forwarded = tree("core,fluid-nitrogen,fluid-water").replacen(
            "[embedded,fluids-core]",
            "[default,embedded,fluids-all]",
            1,
        );
        let errors = check(&forwarded, &core).unwrap_err();
        assert_eq!(
            errors,
            [
                "phasekit-compat enables default, fluids-all beyond the fluids-core set (E7)",
                "phasekit-compat lacks fluids-core of the fluids-core set"
            ]
        );
        let third = format!("{}libm v0.2.15\n", tree("core,fluid-nitrogen,fluid-water"));
        assert_eq!(
            check(&third, &core).unwrap_err(),
            ["the fluids-core build of phasekit-compat pulls in libm v0.2.15"]
        );
        let short = tree("core").lines().take(2).collect::<Vec<_>>().join("\n");
        assert!(check(&short, &core).unwrap_err()[0].starts_with("cargo tree does not list phasekit-data"));
        assert_eq!(check("", &core).unwrap_err().len(), 3, "an empty tree fails closed");
    }

    /// A manifest without a `core` list, or with an empty one, is refused: the gate never passes on nothing.
    #[test]
    fn core_list_fails_closed() {
        let refused = Err(vec!["phasekit-data's [features] has no `core = [...]` list".to_string()]);
        assert_eq!(core_list("[package]\nname = \"x\"\n"), refused);
        assert_eq!(core_list("[features]\nall = [\"fluid-air\"]\n"), refused);
        assert_eq!(core_list("[features]\ncore = []\n"), refused);
        assert_eq!(core_list("[features]\nscore = [\"a\"]\ncore = [\"fluid-water\"]\n").unwrap(), ["fluid-water"]);
    }
}
