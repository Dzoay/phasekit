//! Developer tasks (D7, D13, D17). Never published. Subcommands of the real tool:
//!
//! - `datagen`: pinned v8.0.0 JSON → the one serde mirror (here, not in core: S-12) with a
//!   literal-kind-preserving parse (logged Chlorine duplicate-key waiver) → FNV-1a `source_eos_hash`
//!   recomputation gate → closed enums → `data/corrections.csv` validated against the JSON values it
//!   replaces → precomputed superancillary extrema and inverses → ECS reference graph proved acyclic → one LE
//!   blob per fluid (EOS section written by `phasekit_core::internal::EosRecord::encode`, the same encoder the
//!   runtime hash gate uses, E14), the name/alias/CAS index with declared references, and the per-fluid
//!   features (with ECS implications) in `phasekit-data`.
//! - `baseline`: builds and runs the C++ CoolProp v8.0.0 baseline of scripts/baseline/ in a scratch directory; with
//!   `--check`, checks the committed result files (M1.15).
//! - `oracle`: runs scripts/oracle/gen.py through `uv run --no-project --python 3.12 --with CoolProp==8.0.0` (M1.3),
//!   scrubbed `COOLPROP_*`/`PXFLASH_*`, all 38 config keys recorded and `oracle.lock` checked.
//! - `gates`: gate G8 (VERIFICATION.md §11.2). From M0.4 the zero-dependency guard, workspace lints, executed-test
//!   counts, `#[ignore]` reasons, doc excerpts and rot-register ticks; later fixture manifest drift, the facade
//!   feature-tree check and .wasm size budget (E7), perf-table recording (E9).
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool; the library denies printing (ROT-024)"
)]

mod baseline;
mod datagen;
mod gates;
mod oracle;
mod repo;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((task, rest)) if task == "gates" => gates::main(rest),
        Some((task, rest)) if task == "oracle" => oracle::main(rest),
        Some((task, rest)) if task == "baseline" => baseline::main(rest),
        Some((task, rest)) if task == "datagen" => datagen::main(rest),
        _ => {
            eprintln!("usage: cargo xtask <baseline|datagen|oracle|gates>");
            ExitCode::FAILURE
        }
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

    /// Lints of `clippy::all` that catch tautological assertions (ROT-294). They are not in the workspace table, so
    /// they are probed by name: `-D warnings` makes them errors only while they stay in `all`.
    const TAUTOLOGY_LINTS: [&str; 3] =
        ["clippy::assertions_on_constants", "clippy::eq_op", "clippy::bool_assert_comparison"];

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

    /// Lines (1-based) of `source` outside its test module whose code (comments aside) holds a float literal starting
    /// `8.31`: a hard-coded gas constant.
    fn gas_constant_literals(source: &str) -> Vec<usize> {
        let mut found = Vec::new();
        for (i, line) in source.lines().enumerate() {
            if line.starts_with("#[cfg(test)]") {
                break;
            }
            let code = line.split("//").next().unwrap_or_default();
            let bytes = code.as_bytes();
            let hit = code.match_indices("8.31").any(|(at, _)| {
                !at.checked_sub(1)
                    .and_then(|j| bytes.get(j))
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'.')
            });
            if hit {
                found.push(i + 1);
            }
        }
        found
    }

    /// Rot: ROT-042. R is per-model data (`HelmholtzModel::gas_constant`), never a literal in the kernel: no float
    /// literal `8.31…` appears in `crates/phasekit-core/src` outside test modules (CoolProp hard-codes 8.3144598 in five
    /// places beside 10 distinct EOS values; map 12 R7, map 02 §6). Comments and tests may name values.
    #[test]
    fn no_gas_constant_literal_in_core() {
        assert_eq!(
            gas_constant_literals("let r = 8.314_462_618;\nlet x = 18.31; // 8.31\n#[cfg(test)]\nlet r = 8.31;"),
            [1]
        );
        assert_eq!(gas_constant_literals("let r = R * 8.3144598;"), [1]);
        let files = crate::repo::Repo::locate().files("crates/phasekit-core/src", ".rs", true).unwrap();
        assert!(files.len() > 20);
        let hits: Vec<(String, Vec<usize>)> = files
            .iter()
            .map(|(path, text)| (path.clone(), gas_constant_literals(text)))
            .filter(|(_, lines)| !lines.is_empty())
            .collect();
        assert!(hits.is_empty(), "{hits:?}");
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

    /// Rot: ROT-015, ROT-016, ROT-018, ROT-024, ROT-153, ROT-294 (PLAN.md M0.3, M0.4a; E16 made permanent). Every `clippy.toml` entry
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
            .chain(TAUTOLOGY_LINTS)
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
            .env("CARGO_TERM_COLOR", "never") // CI colours cargo's output; the diagnostics are parsed below
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
