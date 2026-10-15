//! `gates features`: the fluid set is forwarded, never implied (E7; VERIFICATION.md §11.2). `phasekit-compat` built
//! with `--no-default-features --features fluids-core` resolves to the three workspace crates, compat and core with
//! `embedded` and `fluids-core` only (no `rayon`, `libm` or `simd`, PLAN.md M12.4), and `phasekit-data` with `core` and
//! exactly the fluids its `core` list names.
//! The browser package's build resolves the same (beside wasm-bindgen's own crates), and its release `.wasm`, baseline
//! and `simd128`, stays within `crates/phasekit-wasm/size-budget.txt` (PLAN.md M10.8).

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

/// `cargo tree` over the browser package's build, in [`TREE`]'s format.
const WASM_TREE: [&str; 11] = [
    "tree",
    "-p",
    "phasekit-wasm",
    "--target",
    "wasm32-unknown-unknown",
    "-e",
    "normal",
    "-f",
    "{p} [{f}]",
    "--prefix",
    "none",
];

/// The browser module's size budget in bytes: its first green release size + 10 % *(inference: no evidence for an
/// absolute number)*, raised only with a reason written there.
const BUDGET: &str = "crates/phasekit-wasm/size-budget.txt";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let core = core_list(&repo.read("crates/phasekit-data/Cargo.toml").map_err(|e| vec![e])?)?;
    for (tree, root) in [(&TREE[..], "phasekit-compat"), (&WASM_TREE[..], "phasekit-wasm")] {
        let (ok, listing) = repo.cargo(tree).map_err(|e| vec![e])?;
        if !ok {
            return Err(vec![format!("cargo {} failed", tree.join(" "))]);
        }
        check(&listing, &core, root)?;
    }
    let budget = budget(&repo.read(BUDGET).map_err(|e| vec![e])?)?;
    let mut sizes = Vec::new();
    for (build, simd) in [("baseline", false), ("simd128", true)] {
        let dir = repo.target_dir().join(if simd { "simd" } else { "" });
        let wasm = dir.join("wasm32-unknown-unknown/release/phasekit_wasm.wasm");
        sizes.push((build, repo.build_size(&wasm_build(&dir.to_string_lossy(), simd), &wasm).map_err(|e| vec![e])?));
    }
    let over: Vec<String> = sizes
        .iter()
        .filter(|(_, size)| *size > budget)
        .map(|(build, size)| format!("the {build} .wasm is {size} bytes, over {BUDGET}'s {budget}"))
        .collect();
    if !over.is_empty() {
        return Err(over);
    }
    Ok(format!(
        "phasekit-compat --features fluids-core and phasekit-wasm embed {} fluids: {}; the .wasm is {} bytes, {} with \
         simd128, within {budget}",
        core.len(),
        core.join(", "),
        sizes[0].1,
        sizes[1].1
    ))
}

/// The budget: the one line of [`BUDGET`] that is not a `#` comment, a byte count.
fn budget(text: &str) -> Result<u64, Vec<String>> {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#'));
    match (lines.next().map(str::parse::<u64>), lines.next()) {
        (Some(Ok(bytes)), None) => Ok(bytes),
        _ => Err(vec![format!("{BUDGET}: one byte count expected, beside # comments")]),
    }
}

/// The cargo arguments of the browser package's release build into `dir`; `simd128` in its own directory, as the flag
/// would rebuild the baseline's.
fn wasm_build(dir: &str, simd: bool) -> Vec<String> {
    let mut build = vec!["build", "-p", "phasekit-wasm", "--release", "--target", "wasm32-unknown-unknown"];
    build.extend(["--target-dir", dir]);
    if simd {
        build.extend(["--config", "build.rustflags=[\"-C\", \"target-feature=+simd128\"]"]);
    }
    build.into_iter().map(String::from).collect()
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

/// Checks `cargo tree` output in the [`TREE`] format, of the build of `root`, against the `core` list. The browser
/// package's build may pull in third-party crates (wasm-bindgen's); a repeated crate is marked `(*)`.
fn check(tree: &str, core: &[String], root: &str) -> Result<(), Vec<String>> {
    let (mut errors, mut seen) = (Vec::new(), Vec::new());
    for line in tree.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let name = line.split_whitespace().next().unwrap_or_default();
        let features = line.split_once(" [").and_then(|(_, rest)| rest.split_once(']')).map(|(features, _)| features);
        let mut features: Vec<&str> = features.unwrap_or_default().split(',').filter(|f| !f.is_empty()).collect();
        features.sort_unstable();
        seen.push(name);
        let want: Vec<&str> = match name {
            "phasekit-wasm" if root == "phasekit-wasm" => Vec::new(),
            _ if root == "phasekit-wasm" && !name.starts_with("phasekit-") => continue,
            "phasekit-compat" | "phasekit-core" => vec!["embedded", "fluids-core"],
            "phasekit-data" => {
                let mut want: Vec<&str> = core.iter().map(String::as_str).chain(["core"]).collect();
                want.sort_unstable();
                want
            }
            _ => {
                errors.push(format!("the fluids-core build of {root} pulls in {line}"));
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
    let crates = ["phasekit-compat", "phasekit-core", "phasekit-data"];
    for name in crates.into_iter().chain(Some(root).filter(|root| !crates.contains(root))) {
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
        assert_eq!(check(&tree("core,fluid-nitrogen,fluid-water"), &core, "phasekit-compat"), Ok(()));
        let extra = check(&tree("core,fluid-air,fluid-nitrogen,fluid-water"), &core, "phasekit-compat").unwrap_err();
        assert_eq!(extra, ["phasekit-data enables fluid-air beyond the fluids-core set (E7)"]);
        let all = check(&tree("all,core,fluid-nitrogen,fluid-water"), &core, "phasekit-compat").unwrap_err();
        assert_eq!(all, ["phasekit-data enables all beyond the fluids-core set (E7)"]);
        let absent = check(&tree("core,fluid-water"), &core, "phasekit-compat").unwrap_err();
        assert_eq!(absent, ["phasekit-data lacks fluid-nitrogen of the fluids-core set"]);
        let forwarded = tree("core,fluid-nitrogen,fluid-water").replacen(
            "[embedded,fluids-core]",
            "[default,embedded,fluids-all]",
            1,
        );
        let errors = check(&forwarded, &core, "phasekit-compat").unwrap_err();
        assert_eq!(
            errors,
            [
                "phasekit-compat enables default, fluids-all beyond the fluids-core set (E7)",
                "phasekit-compat lacks fluids-core of the fluids-core set"
            ]
        );
        let third = format!("{}libm v0.2.15\n", tree("core,fluid-nitrogen,fluid-water"));
        assert_eq!(
            check(&third, &core, "phasekit-compat").unwrap_err(),
            ["the fluids-core build of phasekit-compat pulls in libm v0.2.15"]
        );
        let short = tree("core").lines().take(2).collect::<Vec<_>>().join("\n");
        assert!(
            check(&short, &core, "phasekit-compat").unwrap_err()[0]
                .starts_with("cargo tree does not list phasekit-data")
        );
        assert_eq!(check("", &core, "phasekit-compat").unwrap_err().len(), 3, "an empty tree fails closed");
    }

    /// PLAN.md M12.4: core's `simd` feature, or `fearless_simd` itself, in a fluids-core build fails the gate by name.
    #[test]
    fn the_simd_feature_stays_out_of_the_fluids_core_build() {
        let core = core_list(DATA).unwrap();
        let simd = tree("core,fluid-nitrogen,fluid-water").replacen(
            "(/w/crates/phasekit-core) [embedded,fluids-core]",
            "(/w/crates/phasekit-core) [embedded,fluids-core,libm,simd]",
            1,
        );
        assert_eq!(
            check(&simd, &core, "phasekit-compat").unwrap_err(),
            ["phasekit-core enables libm, simd beyond the fluids-core set (E7)"]
        );
        let dependency = format!("{}fearless_simd v1.1.0 [std]\n", tree("core,fluid-nitrogen,fluid-water"));
        assert_eq!(
            check(&dependency, &core, "phasekit-compat").unwrap_err(),
            ["the fluids-core build of phasekit-compat pulls in fearless_simd v1.1.0 [std]"]
        );
    }

    /// PLAN.md M10.8: the browser package's build embeds the same set; wasm-bindgen's crates and a repeated crate's
    /// `(*)` line pass, another workspace crate or an extra fluid does not; the budget is one byte count.
    #[test]
    fn wasm_feature_tree_has_no_extra_fluids() {
        let core = core_list(DATA).unwrap();
        let wasm = format!(
            "phasekit-wasm v0.0.0 (/w/crates/phasekit-wasm) []\n{}phasekit-core v0.0.0 (/w/crates/phasekit-core) \
             [embedded,fluids-core] (*)\nwasm-bindgen v0.2.129 [default,std]\n",
            tree("core,fluid-nitrogen,fluid-water")
        );
        assert_eq!(check(&wasm, &core, "phasekit-wasm"), Ok(()));
        assert!(check(&wasm, &core, "phasekit-compat").unwrap_err()[0].contains("pulls in phasekit-wasm"));
        let extra = wasm.replace("core,fluid-nitrogen,fluid-water", "core,fluid-air,fluid-nitrogen,fluid-water");
        assert_eq!(
            check(&extra, &core, "phasekit-wasm").unwrap_err(),
            ["phasekit-data enables fluid-air beyond the fluids-core set (E7)"]
        );
        let verify = format!("{wasm}phasekit-verify v0.0.0 (/w/crates/phasekit-verify) []\n");
        assert!(check(&verify, &core, "phasekit-wasm").unwrap_err()[0].contains("pulls in phasekit-verify"));
        assert_eq!(budget("# a reason\n1184227\n"), Ok(1_184_227));
        let (base, simd) = (wasm_build("/t", false).join(" "), wasm_build("/t/simd", true).join(" "));
        assert_eq!(base, "build -p phasekit-wasm --release --target wasm32-unknown-unknown --target-dir /t");
        assert!(simd.ends_with("--target-dir /t/simd --config build.rustflags=[\"-C\", \"target-feature=+simd128\"]"));
        assert!(budget("1\n2\n").is_err() && budget("# none\n").is_err() && budget("many\n").is_err());
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
