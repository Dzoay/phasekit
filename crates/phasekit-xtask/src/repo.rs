//! The repository as the tasks see it: its files and `cargo`. This is the only module that touches the filesystem
//! or spawns processes, so the library's bans (clippy.toml) still hold for the rest of xtask.
#![allow(clippy::disallowed_methods, reason = "xtask is a dev tool: it reads and writes the repository and runs cargo")]

use std::path::{Path, PathBuf};
use std::process::Command;

/// A workspace member: its directory relative to the root (`crates/phasekit-core`) and its manifest text.
pub struct Member {
    pub dir: String,
    pub manifest: String,
}

/// The checkout this xtask binary was built from.
pub struct Repo {
    root: PathBuf,
}

impl Repo {
    /// The repository root: two levels above this crate's manifest.
    pub fn locate() -> Repo {
        Repo { root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
    }

    pub fn read(&self, rel: &str) -> Result<String, String> {
        std::fs::read_to_string(self.root.join(rel)).map_err(|e| format!("cannot read {rel}: {e}"))
    }

    pub fn write(&self, rel: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.root.join(rel), text).map_err(|e| format!("cannot write {rel}: {e}"))
    }

    /// Every file under `dir` (relative to the root) whose name ends in `ext`, as (relative path, text), sorted by
    /// path. `recursive` descends into subdirectories, skipping `target` and hidden ones. A missing `dir` has none.
    pub fn files(&self, dir: &str, ext: &str, recursive: bool) -> Result<Vec<(String, String)>, String> {
        let mut found = Vec::new();
        if !self.root.join(dir).is_dir() {
            return Ok(found);
        }
        let mut pending = vec![dir.to_string()];
        while let Some(rel) = pending.pop() {
            let entries = std::fs::read_dir(self.root.join(&rel)).map_err(|e| format!("cannot list {rel}: {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("cannot list {rel}: {e}"))?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = format!("{rel}/{name}");
                let kind = entry.file_type().map_err(|e| format!("cannot stat {path}: {e}"))?;
                if kind.is_dir() {
                    if recursive && name != "target" && !name.starts_with('.') {
                        pending.push(path);
                    }
                } else if name.ends_with(ext) {
                    let text = self.read(&path)?;
                    found.push((path, text));
                }
            }
        }
        found.sort();
        Ok(found)
    }

    /// The workspace members, `crates/*` with a `Cargo.toml` (root `members = ["crates/*"]`), sorted by directory.
    pub fn members(&self) -> Result<Vec<Member>, String> {
        let entries = std::fs::read_dir(self.root.join("crates")).map_err(|e| format!("cannot list crates: {e}"))?;
        let mut members = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot list crates: {e}"))?;
            let dir = format!("crates/{}", entry.file_name().to_string_lossy());
            if self.root.join(&dir).join("Cargo.toml").is_file() {
                let manifest = self.read(&format!("{dir}/Cargo.toml"))?;
                members.push(Member { dir, manifest });
            }
        }
        members.sort_by(|a, b| a.dir.cmp(&b.dir));
        Ok(members)
    }

    /// Runs `cargo <args>` in the root with the toolchain that built xtask; returns (success, stdout).
    pub fn cargo(&self, args: &[&str]) -> Result<(bool, String), String> {
        let output = Command::new(env!("CARGO"))
            .args(args)
            .current_dir(&self.root)
            .output()
            .map_err(|e| format!("cannot run cargo {}: {e}", args.join(" ")))?;
        Ok((output.status.success(), String::from_utf8_lossy(&output.stdout).into_owned()))
    }

    /// Runs `cargo <args>` with stdout and stderr in one stream, in the order they were written (cargo's
    /// `Running ...` lines go to stderr, libtest's results to stdout); returns (success, output).
    pub fn cargo_merged(&self, label: &str, args: &[&str]) -> Result<(bool, String), String> {
        let log = std::env::temp_dir().join(format!("phasekit-xtask-{}-{label}.log", std::process::id()));
        let file = std::fs::File::create(&log).map_err(|e| format!("cannot create {}: {e}", log.display()))?;
        let stderr = file.try_clone().map_err(|e| format!("cannot share {}: {e}", log.display()))?;
        let status = Command::new(env!("CARGO"))
            .args(args)
            .current_dir(&self.root)
            .stdout(file)
            .stderr(stderr)
            .status()
            .map_err(|e| format!("cannot run cargo {}: {e}", args.join(" ")));
        let text = std::fs::read_to_string(&log).map_err(|e| format!("cannot read {}: {e}", log.display()));
        let _ = std::fs::remove_file(&log);
        Ok((status?.success(), text?))
    }

    /// The host target triple, from `rustc -vV`.
    pub fn host(&self) -> Result<String, String> {
        let output = Command::new("rustc").arg("-vV").output().map_err(|e| format!("cannot run rustc -vV: {e}"))?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .map(str::to_string)
            .ok_or_else(|| "rustc -vV printed no host".to_string())
    }
}
