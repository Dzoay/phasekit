//! `gates features`: the fluid set is forwarded, never implied (E7; VERIFICATION.md §11.2). `phasekit-compat` built
//! with `--no-default-features --features fluids-core` resolves to the three workspace crates, compat and core with
//! `embedded` and `fluids-core` only (no `rayon`, `libm` or `simd`, PLAN.md M12.4), and `phasekit-data` with `core` and
//! exactly the fluids its `core` list names.
//! The browser package's build resolves the same (beside wasm-bindgen's own crates, and phasekit-mix with no feature for
//! its mixture names, M13.14), except that its `simd128` build, with the package's `simd` feature, enables core's `simd`
//! (M12.10, user decision SL8), and its release `.wasm`,
//! baseline and `simd128`, stays within its budget in `crates/phasekit-wasm/size-budget.txt` (PLAN.md M10.8).

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

/// The browser module's size budgets in bytes, one per build: its measured release size + 10 % *(inference: no evidence
/// for an absolute number)*, raised only with a reason written there.
const BUDGET: &str = "crates/phasekit-wasm/size-budget.txt";

/// The browser package's builds, in [`budget`]'s order: the baseline and `simd128`'s.
const BUILDS: [&str; 2] = ["baseline", "simd128"];

/// The cargo arguments of the browser package's `simd128` build: the package's `simd` feature, for WASM's `simd128`.
const SIMD128: [&str; 4] = ["--features", "simd", "--config", "build.rustflags=[\"-C\", \"target-feature=+simd128\"]"];

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let core = core_list(&repo.read("crates/phasekit-data/Cargo.toml").map_err(|e| vec![e])?)?;
    let compat: Vec<String> = TREE.into_iter().map(String::from).collect();
    for (tree, root, simd) in [
        (compat, "phasekit-compat", false),
        (wasm_tree(false), "phasekit-wasm", false),
        (wasm_tree(true), "phasekit-wasm", true),
    ] {
        let args: Vec<&str> = tree.iter().map(String::as_str).collect();
        let (ok, listing) = repo.cargo(&args).map_err(|e| vec![e])?;
        if !ok {
            return Err(vec![format!("cargo {} failed", tree.join(" "))]);
        }
        check(&listing, &core, root, simd)?;
    }
    let budgets = budget(&repo.read(BUDGET).map_err(|e| vec![e])?)?;
    let mut sizes = Vec::new();
    for (build, simd) in BUILDS.into_iter().zip([false, true]) {
        let dir = repo.target_dir().join(if simd { "simd" } else { "" });
        let wasm = dir.join("wasm32-unknown-unknown/release/phasekit_wasm.wasm");
        sizes.push((build, repo.build_size(&wasm_build(&dir.to_string_lossy(), simd), &wasm).map_err(|e| vec![e])?));
    }
    let over: Vec<String> = sizes
        .iter()
        .zip(budgets)
        .filter(|((_, size), budget)| size > budget)
        .map(|((build, size), budget)| format!("the {build} .wasm is {size} bytes, over {BUDGET}'s {budget}"))
        .collect();
    if !over.is_empty() {
        return Err(over);
    }
    Ok(format!(
        "phasekit-compat --features fluids-core and phasekit-wasm embed {} fluids: {}; the .wasm is {} bytes within {}, \
         and {} with simd128 and simd within {}",
        core.len(),
        core.join(", "),
        sizes[0].1,
        budgets[0],
        sizes[1].1,
        budgets[1]
    ))
}

/// The features [`check`] expects of the workspace crate `name` in `root`'s build (with `simd`, the browser package's
/// `simd128` build), sorted, or `None` for a crate that build must not pull in.
fn wanted<'a>(name: &str, core: &'a [String], root: &str, simd: bool) -> Option<Vec<&'a str>> {
    let want = match name {
        "phasekit-wasm" if root == "phasekit-wasm" => simd.then_some("simd").into_iter().collect(),
        "phasekit-mix" if root == "phasekit-wasm" => Vec::new(),
        "phasekit-core" if simd => vec!["embedded", "fluids-core", "libm", "simd"],
        "phasekit-compat" | "phasekit-core" => vec!["embedded", "fluids-core"],
        "phasekit-data" => {
            let mut want: Vec<&str> = core.iter().map(String::as_str).chain(["core"]).collect();
            want.sort_unstable();
            want
        }
        _ => return None,
    };
    Some(want)
}

/// The budgets, in [`BUILDS`]' order: the lines of [`BUDGET`] that are not `#` comments, each a build's name and a
/// byte count, every build once.
fn budget(text: &str) -> Result<[u64; 2], Vec<String>> {
    let refused = || vec![format!("{BUDGET}: `baseline <bytes>` and `simd128 <bytes>` expected, beside # comments")];
    let mut budgets = [None; 2];
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let mut words = line.split_whitespace();
        let (Some(build), Some(bytes), None) = (words.next(), words.next(), words.next()) else {
            return Err(refused());
        };
        let slot = BUILDS.iter().position(|b| *b == build).and_then(|i| budgets.get_mut(i)).ok_or_else(refused)?;
        if slot.replace(bytes.parse::<u64>().map_err(|_| refused())?).is_some() {
            return Err(refused());
        }
    }
    match budgets {
        [Some(baseline), Some(simd128)] => Ok([baseline, simd128]),
        _ => Err(refused()),
    }
}

/// [`WASM_TREE`], for the `simd128` build when `simd`.
fn wasm_tree(simd: bool) -> Vec<String> {
    let simd = if simd { &SIMD128[..] } else { &[] };
    WASM_TREE.iter().chain(simd).map(|arg| (*arg).to_string()).collect()
}

/// The cargo arguments of the browser package's release build into `dir`; `simd128` in its own directory, as the flag
/// would rebuild the baseline's.
fn wasm_build(dir: &str, simd: bool) -> Vec<String> {
    let mut build = vec!["build", "-p", "phasekit-wasm", "--release", "--target", "wasm32-unknown-unknown"];
    build.extend(["--target-dir", dir]);
    if simd {
        build.extend(SIMD128);
    }
    build.into_iter().map(String::from).collect()
}

/// The `core = [...]` list of phasekit-data's `[features]`, sorted.
pub(crate) fn core_list(manifest: &str) -> Result<Vec<String>, Vec<String>> {
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

/// Checks `cargo tree` output in the [`TREE`] format, of the build of `root`, against the `core` list; with `simd`, the
/// browser package's `simd128` build, the package enables `simd` and core `libm` and `simd`. The browser package's build may pull in
/// third-party crates (wasm-bindgen's, fearless_simd); a repeated crate is marked `(*)`.
fn check(tree: &str, core: &[String], root: &str, simd: bool) -> Result<(), Vec<String>> {
    if simd && root != "phasekit-wasm" {
        return Err(vec![format!("{root} has no simd128 build")]);
    }
    let lines = tree.lines().map(str::trim).filter(|line| !line.is_empty());
    let seen: Vec<&str> = lines.clone().map(|line| line.split_whitespace().next().unwrap_or_default()).collect();
    let mut errors: Vec<String> = lines.flat_map(|line| check_line(line, core, root, simd)).collect();
    let crates = ["phasekit-compat", "phasekit-core", "phasekit-data"];
    for name in crates.into_iter().chain(Some(root).filter(|root| !crates.contains(root))) {
        if !seen.contains(&name) {
            errors.push(format!("cargo tree does not list {name}: {tree:?}"));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

/// [`check`]'s errors for one line of the tree: a crate the build must not pull in, or a workspace crate whose features
/// go beyond or fall short of [`wanted`]'s. The browser package's third-party crates pass.
fn check_line(line: &str, core: &[String], root: &str, simd: bool) -> Vec<String> {
    let name = line.split_whitespace().next().unwrap_or_default();
    if root == "phasekit-wasm" && !name.starts_with("phasekit-") {
        return Vec::new();
    }
    let Some(want) = wanted(name, core, root, simd) else {
        return vec![format!("the fluids-core build of {root} pulls in {line}")];
    };
    let features = line.split_once(" [").and_then(|(_, rest)| rest.split_once(']')).map(|(features, _)| features);
    let mut features: Vec<&str> = features.unwrap_or_default().split(',').filter(|f| !f.is_empty()).collect();
    features.sort_unstable();
    let set = if simd { "the simd128 build's set (SL8)" } else { "the fluids-core set" };
    let extra: Vec<&str> = features.iter().filter(|f| !want.contains(f)).copied().collect();
    let absent: Vec<&str> = want.iter().filter(|f| !features.contains(f)).copied().collect();
    let mut errors = Vec::new();
    if !extra.is_empty() {
        errors.push(format!("{name} enables {} beyond {set} (E7)", extra.join(", ")));
    }
    if !absent.is_empty() {
        errors.push(format!("{name} lacks {} of {set}", absent.join(", ")));
    }
    errors
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
        assert_eq!(check(&tree("core,fluid-nitrogen,fluid-water"), &core, "phasekit-compat", false), Ok(()));
        let extra =
            check(&tree("core,fluid-air,fluid-nitrogen,fluid-water"), &core, "phasekit-compat", false).unwrap_err();
        assert_eq!(extra, ["phasekit-data enables fluid-air beyond the fluids-core set (E7)"]);
        let all = check(&tree("all,core,fluid-nitrogen,fluid-water"), &core, "phasekit-compat", false).unwrap_err();
        assert_eq!(all, ["phasekit-data enables all beyond the fluids-core set (E7)"]);
        let absent = check(&tree("core,fluid-water"), &core, "phasekit-compat", false).unwrap_err();
        assert_eq!(absent, ["phasekit-data lacks fluid-nitrogen of the fluids-core set"]);
        let forwarded = tree("core,fluid-nitrogen,fluid-water").replacen(
            "[embedded,fluids-core]",
            "[default,embedded,fluids-all]",
            1,
        );
        let errors = check(&forwarded, &core, "phasekit-compat", false).unwrap_err();
        assert_eq!(
            errors,
            [
                "phasekit-compat enables default, fluids-all beyond the fluids-core set (E7)",
                "phasekit-compat lacks fluids-core of the fluids-core set"
            ]
        );
        let third = format!("{}libm v0.2.15\n", tree("core,fluid-nitrogen,fluid-water"));
        assert_eq!(
            check(&third, &core, "phasekit-compat", false).unwrap_err(),
            ["the fluids-core build of phasekit-compat pulls in libm v0.2.15"]
        );
        let short = tree("core").lines().take(2).collect::<Vec<_>>().join("\n");
        assert!(
            check(&short, &core, "phasekit-compat", false).unwrap_err()[0]
                .starts_with("cargo tree does not list phasekit-data")
        );
        assert_eq!(check("", &core, "phasekit-compat", false).unwrap_err().len(), 3, "an empty tree fails closed");
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
            check(&simd, &core, "phasekit-compat", false).unwrap_err(),
            ["phasekit-core enables libm, simd beyond the fluids-core set (E7)"]
        );
        let dependency = format!("{}fearless_simd v1.1.0 [std]\n", tree("core,fluid-nitrogen,fluid-water"));
        assert_eq!(
            check(&dependency, &core, "phasekit-compat", false).unwrap_err(),
            ["the fluids-core build of phasekit-compat pulls in fearless_simd v1.1.0 [std]"]
        );
    }

    /// PLAN.md M10.8: the browser package's build embeds the same set; wasm-bindgen's crates, phasekit-mix without a
    /// feature (M13.14) and a repeated crate's `(*)` line pass, another workspace crate or an extra fluid does not, nor
    /// phasekit-mix in compat's build; the budget is one byte count.
    #[test]
    fn wasm_feature_tree_has_no_extra_fluids() {
        let core = core_list(DATA).unwrap();
        let wasm = format!(
            "phasekit-wasm v0.0.0 (/w/crates/phasekit-wasm) []\n{}phasekit-core v0.0.0 (/w/crates/phasekit-core) \
             [embedded,fluids-core] (*)\nphasekit-mix v0.0.0 (/w/crates/phasekit-mix) []\nwasm-bindgen v0.2.129 \
             [default,std]\n",
            tree("core,fluid-nitrogen,fluid-water")
        );
        assert_eq!(check(&wasm, &core, "phasekit-wasm", false), Ok(()));
        assert!(check(&wasm, &core, "phasekit-compat", false).unwrap_err()[0].contains("pulls in phasekit-wasm"));
        let mixed =
            format!("{}phasekit-mix v0.0.0 (/w/crates/phasekit-mix) []\n", tree("core,fluid-nitrogen,fluid-water"));
        assert!(check(&mixed, &core, "phasekit-compat", false).unwrap_err()[0].contains("pulls in phasekit-mix"));
        let extra = wasm.replace("core,fluid-nitrogen,fluid-water", "core,fluid-air,fluid-nitrogen,fluid-water");
        assert_eq!(
            check(&extra, &core, "phasekit-wasm", false).unwrap_err(),
            ["phasekit-data enables fluid-air beyond the fluids-core set (E7)"]
        );
        let verify = format!("{wasm}phasekit-verify v0.0.0 (/w/crates/phasekit-verify) []\n");
        assert!(check(&verify, &core, "phasekit-wasm", false).unwrap_err()[0].contains("pulls in phasekit-verify"));
        let (base, simd) = (wasm_build("/t", false).join(" "), wasm_build("/t/simd", true).join(" "));
        assert_eq!(base, "build -p phasekit-wasm --release --target wasm32-unknown-unknown --target-dir /t");
        assert!(simd.ends_with(
            "--target-dir /t/simd --features simd --config build.rustflags=[\"-C\", \"target-feature=+simd128\"]"
        ));
    }

    /// M12.10 (user decision SL8): the browser package's simd128 build enables core's `simd`, and so `libm`, and
    /// fearless_simd comes with it; the baseline build does not. `simd` in the baseline's tree, or a simd128 tree
    /// without it, fails the gate by name.
    #[test]
    fn only_the_simd128_browser_build_enables_simd() {
        let core = core_list(DATA).unwrap();
        let wasm = |features: &str| {
            let tree = tree("core,fluid-nitrogen,fluid-water").replacen(
                "(/w/crates/phasekit-core) [embedded,fluids-core]",
                &format!("(/w/crates/phasekit-core) [{features}]"),
                1,
            );
            format!("phasekit-wasm v0.0.0 (/w/crates/phasekit-wasm) [simd]\n{tree}fearless_simd v1.1.0 [std]\n")
        };
        let simd = wasm("embedded,fluids-core,libm,simd");
        assert_eq!(check(&simd, &core, "phasekit-wasm", true), Ok(()));
        assert_eq!(
            check(&simd, &core, "phasekit-wasm", false).unwrap_err(),
            [
                "phasekit-wasm enables simd beyond the fluids-core set (E7)",
                "phasekit-core enables libm, simd beyond the fluids-core set (E7)"
            ]
        );
        let unasked = simd.replacen("[simd]", "[]", 1);
        assert_eq!(
            check(&unasked, &core, "phasekit-wasm", true).unwrap_err(),
            ["phasekit-wasm lacks simd of the simd128 build's set (SL8)"]
        );
        assert_eq!(
            check(&wasm("embedded,fluids-core"), &core, "phasekit-wasm", true).unwrap_err(),
            ["phasekit-core lacks libm, simd of the simd128 build's set (SL8)"]
        );
        assert_eq!(
            check(&wasm("embedded,fluids-core,simd"), &core, "phasekit-wasm", true).unwrap_err(),
            ["phasekit-core lacks libm of the simd128 build's set (SL8)"]
        );
        assert!(check(&simd, &core, "phasekit-compat", true).is_err(), "only the browser package has a simd128 build");
        assert_eq!(wasm_tree(false), WASM_TREE);
        let tree = wasm_tree(true).join(" ");
        assert!(tree.starts_with(&WASM_TREE.join(" ")), "{tree}");
        assert!(
            tree.ends_with("--features simd --config build.rustflags=[\"-C\", \"target-feature=+simd128\"]"),
            "{tree}"
        );
    }

    /// SL8: one budget per build, `baseline <bytes>` and `simd128 <bytes>` in either order, beside `#` comments;
    /// anything else, a build missing or named twice, fails closed.
    #[test]
    fn the_budget_is_a_byte_count_per_build() {
        assert_eq!(budget("# a reason\nbaseline 1307788\nsimd128 1458107\n"), Ok([1_307_788, 1_458_107]));
        assert_eq!(budget("simd128 2\n\n  baseline 1  \n"), Ok([1, 2]));
        for bad in ["1184227\n", "baseline 1\n", "baseline 1\nsimd128 2\nsimd128 3\n", "baseline x\nsimd128 2\n"] {
            assert!(budget(bad).is_err(), "{bad:?}");
        }
        for bad in ["baseline 1\nsimd128 2\nnode 3\n", "baseline 1 2\nsimd128 2\n", "# none\n", ""] {
            assert!(budget(bad).is_err(), "{bad:?}");
        }
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
