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

/// One `cargo mutants` run: its exit status, the contents of `caught.txt`, `missed.txt`, `timeout.txt` and
/// `unviable.txt`, and where its output stays when it failed.
pub struct MutantsRun {
    pub success: bool,
    pub outcomes: [String; 4],
    pub output: String,
}

/// Where `reference/` is when it is not under the root: cargo-mutants tests a copy of the tree without gitignored
/// files (`.cargo/mutants.toml`), so `gates mutants` names the checkout's own `reference/` here, and the tests that
/// read the pinned CoolProp data (datagen, M2) still find it.
const REFERENCE_DIR: &str = "PHASEKIT_REFERENCE_DIR";

/// The checkout this xtask binary was built from.
pub struct Repo {
    root: PathBuf,
    /// The gitignored `reference/` directory (the CoolProp checkout, local papers).
    reference: PathBuf,
}

impl Repo {
    /// The repository root: two levels above this crate's manifest.
    pub fn locate() -> Repo {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let reference = std::env::var_os(REFERENCE_DIR).map_or_else(|| root.join("reference"), PathBuf::from);
        Repo { root, reference }
    }

    /// The path of `rel` (relative to the root); `reference/...` resolves into the reference directory.
    fn path(&self, rel: &str) -> PathBuf {
        match rel.strip_prefix("reference/") {
            Some(rest) => self.reference.join(rest),
            None => self.root.join(rel),
        }
    }

    pub fn read(&self, rel: &str) -> Result<String, String> {
        std::fs::read_to_string(self.path(rel)).map_err(|e| format!("cannot read {rel}: {e}"))
    }

    pub fn read_bytes(&self, rel: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.path(rel)).map_err(|e| format!("cannot read {rel}: {e}"))
    }

    /// Writes `bytes` to `rel`, creating its directory.
    pub fn write_bytes(&self, rel: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.path(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("cannot create the directory of {rel}: {e}"))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("cannot write {rel}: {e}"))
    }

    /// The names of the files directly in `dir` (relative to the root), sorted; none when it does not exist.
    pub fn file_names(&self, dir: &str) -> Result<Vec<String>, String> {
        let Ok(entries) = std::fs::read_dir(self.path(dir)) else { return Ok(Vec::new()) };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot list {dir}: {e}"))?;
            if entry.file_type().map_err(|e| format!("cannot stat in {dir}: {e}"))?.is_file() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn remove(&self, rel: &str) -> Result<(), String> {
        std::fs::remove_file(self.path(rel)).map_err(|e| format!("cannot remove {rel}: {e}"))
    }

    pub fn write(&self, rel: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.root.join(rel), text).map_err(|e| format!("cannot write {rel}: {e}"))
    }

    /// Every file under `dir` (relative to the root) whose name ends in `ext`, as (relative path, text), sorted by
    /// path. `recursive` descends into subdirectories, skipping `target` and hidden ones. A missing `dir` has none.
    pub fn files(&self, dir: &str, ext: &str, recursive: bool) -> Result<Vec<(String, String)>, String> {
        let mut found = Vec::new();
        if !self.path(dir).is_dir() {
            return Ok(found);
        }
        let mut pending = vec![dir.to_string()];
        while let Some(rel) = pending.pop() {
            let entries = std::fs::read_dir(self.path(&rel)).map_err(|e| format!("cannot list {rel}: {e}"))?;
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
        let output =
            self.cargo_command(args).output().map_err(|e| format!("cannot run cargo {}: {e}", args.join(" ")))?;
        Ok((output.status.success(), String::from_utf8_lossy(&output.stdout).into_owned()))
    }

    /// Runs `cargo <args>` with stdout and stderr in one stream, in the order they were written (cargo's
    /// `Running ...` lines go to stderr, libtest's results to stdout); returns (success, output).
    pub fn cargo_merged(&self, label: &str, args: &[&str]) -> Result<(bool, String), String> {
        let log = std::env::temp_dir().join(format!("phasekit-xtask-{}-{label}.log", std::process::id()));
        let file = std::fs::File::create(&log).map_err(|e| format!("cannot create {}: {e}", log.display()))?;
        let stderr = file.try_clone().map_err(|e| format!("cannot share {}: {e}", log.display()))?;
        let status = self
            .cargo_command(args)
            .stdout(file)
            .stderr(stderr)
            .status()
            .map_err(|e| format!("cannot run cargo {}: {e}", args.join(" ")));
        let text = std::fs::read_to_string(&log).map_err(|e| format!("cannot read {}: {e}", log.display()));
        let _ = std::fs::remove_file(&log);
        Ok((status?.success(), text?))
    }

    /// `cargo <args>` in the root with the toolchain that built xtask, uncoloured: its output is parsed, and an
    /// inherited `CARGO_TERM_COLOR=always` (as in CI) would put escape codes in front of every line.
    fn cargo_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO"));
        command.args(args).current_dir(&self.root).env("CARGO_TERM_COLOR", "never");
        command
    }

    /// Runs `git <args>` in the root; returns stdout.
    pub fn git(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .map_err(|e| format!("cannot run git {}: {e}", args.join(" ")))?;
        if !output.status.success() {
            return Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Runs `cargo mutants --in-diff` on `diff` (configured by `.cargo/mutants.toml`), its progress on the terminal.
    /// Builds happen in cargo-mutants' own copy of the tree outside the repository, so an inherited
    /// `CARGO_TARGET_DIR` is dropped and `reference/` is named through `PHASEKIT_REFERENCE_DIR`. The output is removed after a successful run and kept after a failed one.
    pub fn cargo_mutants(&self, diff: &str) -> Result<MutantsRun, String> {
        let dir = std::env::temp_dir().join(format!("phasekit-mutants-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let diff_file = dir.join("changes.diff");
        std::fs::write(&diff_file, diff).map_err(|e| format!("cannot write {}: {e}", diff_file.display()))?;
        // A quarter of the hardware threads, 1 to 4: each job runs its own parallel build and test binaries.
        let jobs = std::thread::available_parallelism().map_or(1, |n| (n.get() / 4).clamp(1, 4));
        let status = Command::new(env!("CARGO"))
            .args(["mutants", "--jobs", &jobs.to_string(), "--in-diff"])
            .arg(&diff_file)
            .arg("--output")
            .arg(&dir)
            .current_dir(&self.root)
            .env_remove("CARGO_TARGET_DIR")
            .env(REFERENCE_DIR, &self.reference)
            .status()
            .map_err(|e| format!("cannot run cargo mutants (scripts/check-toolchain.sh): {e}"))?;
        let out = dir.join("mutants.out");
        let outcomes = ["caught.txt", "missed.txt", "timeout.txt", "unviable.txt"]
            .map(|name| std::fs::read_to_string(out.join(name)).unwrap_or_default());
        if status.success() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        Ok(MutantsRun { success: status.success(), outcomes, output: out.display().to_string() })
    }

    /// Runs `cargo bench -p phasekit-verify` (criterion), its output on the terminal.
    pub fn bench(&self) -> Result<(), String> {
        let status = Command::new(env!("CARGO"))
            .args(["bench", "-p", "phasekit-verify"])
            .current_dir(&self.root)
            .status()
            .map_err(|e| format!("cannot run cargo bench: {e}"))?;
        if status.success() { Ok(()) } else { Err("cargo bench failed".into()) }
    }

    /// Every criterion result under the target directory: (`benchmark.json`, `estimates.json`) of each `new/` run.
    pub fn criterion_results(&self) -> Result<Vec<(String, String)>, String> {
        let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| self.root.join("target"), PathBuf::from);
        let mut pending = vec![target.join("criterion")];
        let mut found = Vec::new();
        while let Some(dir) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|n| n == "new") {
                        let read = |f: &str| {
                            std::fs::read_to_string(path.join(f)).map_err(|e| format!("{}: {e}", path.display()))
                        };
                        found.push((read("benchmark.json")?, read("estimates.json")?));
                    } else {
                        pending.push(path);
                    }
                }
            }
        }
        if found.is_empty() {
            return Err(format!("no criterion results under {}", target.display()));
        }
        found.sort();
        Ok(found)
    }

    /// The date, commit, CPU model, OS, rustc and CPU frequency governor a bench result is recorded with.
    pub fn machine(&self) -> Result<crate::bench::Machine, String> {
        let run = |program: &str, args: &[&str]| -> String {
            let output = Command::new(program).args(args).current_dir(&self.root).output();
            output.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
        };
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let cpu = cpuinfo.lines().find_map(|l| l.strip_prefix("model name")).and_then(|l| l.split_once(':'));
        let governor = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor");
        Ok(crate::bench::Machine {
            date: run("date", &["-u", "+%Y-%m-%d"]),
            commit: run("git", &["rev-parse", "--short", "HEAD"]),
            cpu: cpu.map(|(_, m)| m.trim().to_string()).ok_or("no CPU model in /proc/cpuinfo")?,
            os: run("uname", &["-sr"]),
            rustc: run("rustc", &["-V"]),
            governor: governor.map(|g| g.trim().to_string()).unwrap_or_else(|_| "unknown".into()),
        })
    }

    /// An environment variable, if set.
    pub fn var(&self, name: &str) -> Option<String> {
        std::env::var_os(name).map(|v| v.to_string_lossy().into_owned())
    }

    /// `git -C reference/CoolProp status --porcelain --ignored`, or `None` when the checkout is absent (as on CI).
    pub fn reference_status(&self) -> Option<Result<String, String>> {
        self.root
            .join("reference/CoolProp")
            .is_dir()
            .then(|| self.git(&["-C", "reference/CoolProp", "status", "--porcelain", "--ignored"]))
    }

    /// What the oracle generator inherits from this environment: PATH, HOME and UV_CACHE_DIR if set.
    pub fn oracle_environment(&self) -> (String, String, Option<String>) {
        let var = |name: &str| std::env::var_os(name).map(|v| v.to_string_lossy().into_owned());
        (var("PATH").unwrap_or_default(), var("HOME").unwrap_or_default(), var("UV_CACHE_DIR"))
    }

    /// Runs `invocation` in the root with exactly its environment (nothing inherited); returns whether it succeeded.
    pub fn run(&self, invocation: &crate::oracle::Invocation) -> Result<bool, String> {
        Command::new(&invocation.program)
            .args(&invocation.args)
            .env_clear()
            .envs(invocation.env.iter().map(|(k, v)| (k, v)))
            .current_dir(&self.root)
            .status()
            .map(|status| status.success())
            .map_err(|e| format!("cannot run {}: {e}", invocation.program))
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
