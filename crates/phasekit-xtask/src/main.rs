//! Developer tasks (D7, D13, D17). Never published. Subcommands of the real tool:
//!
//! - `datagen`: pinned v8.0.0 JSON → the one serde mirror (here, not in core: S-12) with a
//!   literal-kind-preserving parse (logged Chlorine duplicate-key waiver) → FNV-1a `source_eos_hash`
//!   recomputation gate → closed enums → `data/corrections.csv` validated against the JSON values it
//!   replaces → precomputed superancillary extrema and inverses → ECS reference graph proved acyclic → one LE
//!   blob per fluid (EOS section written by `phasekit_core::internal::EosRecord::encode`, the same encoder the
//!   runtime hash gate uses, E14), the name/alias/CAS index with declared references, and the per-fluid
//!   features (with ECS implications) in `phasekit-data`.
//! - `oracle`: drives `uv run --no-project --python 3.12 --with CoolProp==8.0.0` with a fresh state per case,
//!   scrubbed `COOLPROP_*`/`PXFLASH_*`, all 38 config keys recorded and `oracle.lock` checked.
//! - `gates`: zero-dependency guard, executed-test-count ≥ manifest, `#[ignore]` ids, fixture manifest drift,
//!   the facade feature-tree check and .wasm size budget (E7), perf-table recording from M3 (E9).
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool; the library denies printing (ROT-024)"
)]

fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    let known = ["datagen", "oracle", "gates"];
    if known.contains(&task.as_str()) {
        println!("phasekit-xtask {task}: lands at M1/M2 (core {})", core::any::type_name::<phasekit_core::Registry>());
    } else {
        println!("usage: cargo xtask <{}>", known.join("|"));
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "xtask is a dev tool: this test writes a scratch workspace and runs clippy"
)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// The probe crate, relative to the repository root. It is excluded from the workspace.
    const PROBE: &str = "crates/phasekit-xtask/probes/bans";

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// The `[workspace.lints.*]` tables of a workspace manifest, verbatim.
    fn lint_tables(manifest: &str) -> String {
        let mut keep = false;
        let mut tables = String::new();
        for line in manifest.lines() {
            if line.starts_with('[') {
                keep = line.starts_with("[workspace.lints");
            }
            if keep {
                tables.push_str(line);
                tables.push('\n');
            }
        }
        tables
    }

    /// Every `path = "..."` entry of a `clippy.toml` (`disallowed-methods`, `-types` and `-macros`).
    fn banned_paths(clippy_toml: &str) -> Vec<&str> {
        clippy_toml.split("path = \"").skip(1).filter_map(|rest| rest.split('"').next()).collect()
    }

    /// Every clippy lint the `[workspace.lints.clippy]` table denies.
    fn denied_clippy_lints(manifest: &str) -> Vec<&str> {
        let mut in_clippy = false;
        let mut denied = Vec::new();
        for line in manifest.lines() {
            if line.starts_with('[') {
                in_clippy = line.trim() == "[workspace.lints.clippy]";
            } else if let (true, Some((name, level))) = (in_clippy, line.split_once('=')) {
                if level.trim() == "\"deny\"" {
                    denied.push(name.trim());
                }
            }
        }
        denied
    }

    /// The probe's marked lines: (line number, rule) for every `<code> // fires: <rule>`.
    fn markers(probe: &str) -> Vec<(usize, &str)> {
        probe
            .lines()
            .enumerate()
            .filter_map(|(i, line)| line.split_once("// fires: ").map(|(code, rule)| (i + 1, code, rule.trim())))
            .filter(|(_, code, _)| !code.trim().is_empty() && !code.trim_start().starts_with("//"))
            .map(|(n, _, rule)| (n, rule))
            .collect()
    }

    /// Rot: ROT-015, ROT-016, ROT-018, ROT-024, ROT-153 (PLAN.md M0.3; E16 made permanent). Every `clippy.toml` entry
    /// and every lint the workspace denies has a probe line, and clippy run as in G2 reports an error on every probe
    /// line (naming the banned path, for a `clippy.toml` entry) and nothing anywhere else.
    #[test]
    fn clippy_bans_fire() {
        let root = repo_root();
        let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        let clippy_toml = std::fs::read_to_string(root.join("clippy.toml")).unwrap();
        let probe = std::fs::read_to_string(root.join(PROBE).join("src/lib.rs")).unwrap();
        let markers = markers(&probe);

        let paths = banned_paths(&clippy_toml);
        let lints: Vec<String> = denied_clippy_lints(&manifest).iter().map(|lint| format!("clippy::{lint}")).collect();
        let unprobed: Vec<&str> = paths
            .iter()
            .copied()
            .chain(lints.iter().map(String::as_str))
            .filter(|rule| !markers.iter().any(|(_, r)| r == rule))
            .collect();
        assert!(unprobed.is_empty(), "rules without a `// fires:` line in {PROBE}/src/lib.rs: {unprobed:?}");

        // A scratch workspace whose lints and clippy.toml are the repository's own.
        let scratch = std::env::temp_dir().join(format!("phasekit-clippy-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("bans/src")).unwrap();
        let workspace = format!("[workspace]\nresolver = \"3\"\nmembers = [\"bans\"]\n\n{}", lint_tables(&manifest));
        std::fs::write(scratch.join("Cargo.toml"), workspace).unwrap();
        std::fs::write(scratch.join("clippy.toml"), &clippy_toml).unwrap();
        std::fs::copy(root.join(PROBE).join("Cargo.toml"), scratch.join("bans/Cargo.toml")).unwrap();
        std::fs::write(scratch.join("bans/src/lib.rs"), &probe).unwrap();

        let output = Command::new(env!("CARGO"))
            .args(["clippy", "--offline", "--quiet", "--message-format=short", "--manifest-path"])
            .arg(scratch.join("Cargo.toml"))
            .args(["--", "-D", "warnings"])
            .env("CARGO_TARGET_DIR", scratch.join("target"))
            .env_remove("CLIPPY_CONF_DIR")
            .output()
            .unwrap();
        std::fs::remove_dir_all(&scratch).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Short-format diagnostics: `bans/src/lib.rs:<line>:<col>: <level>: <message>`.
        let diagnostics: Vec<(usize, &str)> = stderr
            .lines()
            .filter_map(|l| l.strip_prefix("bans/src/lib.rs:"))
            .filter_map(|rest| rest.split_once(':').and_then(|(n, msg)| Some((n.parse().ok()?, msg))))
            .collect();
        // A clippy.toml entry must be named in the message; a lint is identified by its line alone.
        let fires = |rule: &str, msg: &str| {
            msg.contains(": error: ") && (!paths.contains(&rule) || msg.contains(&format!("`{rule}`")))
        };
        let silent: Vec<String> = markers
            .iter()
            .filter(|(line, rule)| !diagnostics.iter().any(|(n, msg)| n == line && fires(rule, msg)))
            .map(|(line, rule)| format!("line {line}: {rule}"))
            .collect();
        let stray: Vec<String> = diagnostics
            .iter()
            .filter(|(n, _)| !markers.iter().any(|(line, _)| line == n))
            .map(|(n, msg)| format!("line {n}:{msg}"))
            .collect();
        assert!(
            silent.is_empty() && stray.is_empty(),
            "rules that did not fire: {silent:#?}\nstray diagnostics: {stray:#?}\n\nclippy output:\n{stderr}"
        );
    }
}
