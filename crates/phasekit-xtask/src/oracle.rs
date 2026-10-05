//! `cargo xtask oracle <gen.py arguments>`: runs scripts/oracle/gen.py exactly as VERIFICATION.md §3.1 shows, in an
//! environment rebuilt from nothing (`env -i`), with the uv command taken from `oracle.lock` so the pin lives once.

use std::process::ExitCode;

use crate::repo::Repo;

/// The lock, relative to the repository root.
pub const LOCK: &str = "crates/phasekit-verify/fixtures/oracle.lock";

/// A process to run: program, arguments and the whole environment.
#[derive(Debug, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The generator run: the lock's `uv_command`, then `python scripts/oracle/gen.py --lock <LOCK>` and the caller's
/// arguments; the environment is PATH, HOME, the fixed locale and hashing settings, and UV_CACHE_DIR when set.
pub fn invocation(
    lock: &str,
    gen_args: &[String],
    path: &str,
    home: &str,
    uv_cache_dir: Option<&str>,
) -> Result<Invocation, String> {
    let lock = phasekit_verify::OracleLock::parse(lock)?;
    let uv = lock.get("uv_command").ok_or(format!("{LOCK}: no uv_command"))?;
    let mut words = uv.split_whitespace().map(str::to_string);
    let program = words.next().ok_or(format!("{LOCK}: uv_command is empty"))?;
    let tail = ["python", "scripts/oracle/gen.py", "--lock", LOCK].map(str::to_string);
    let args = words.chain(tail).chain(gen_args.iter().cloned()).collect();
    let fixed = [
        ("PATH", path),
        ("HOME", home),
        ("LC_ALL", "C"),
        ("TZ", "UTC"),
        ("PYTHONHASHSEED", "0"),
        ("PYTHONDONTWRITEBYTECODE", "1"),
    ];
    let cache = uv_cache_dir.map(|dir| ("UV_CACHE_DIR", dir));
    let env = fixed.into_iter().chain(cache).map(|(k, v)| (k.to_string(), v.to_string())).collect();
    Ok(Invocation { program, args, env })
}

pub fn main(args: &[String]) -> ExitCode {
    let repo = Repo::locate();
    let invocation = repo.read(LOCK).and_then(|lock| {
        let (path, home, uv_cache) = repo.oracle_environment();
        invocation(&lock, args, &path, &home, uv_cache.as_deref())
    });
    match invocation.and_then(|invocation| repo.run(&invocation)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("xtask oracle: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UV: &str = "uv_command       uv run --no-project --python 3.12 --with CoolProp==8.0.0\n";

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    /// VERIFICATION.md §3.1: `env -i PATH HOME LC_ALL=C TZ=UTC PYTHONHASHSEED=0 PYTHONDONTWRITEBYTECODE=1 uv run ...`.
    #[test]
    fn the_oracle_runs_in_a_scrubbed_pinned_environment() {
        let run = invocation(UV, &words(&["--kind", "facts"]), "/usr/bin", "/home/u", None).unwrap();
        assert_eq!(run.program, "uv");
        let args =
            ["run", "--no-project", "--python", "3.12", "--with", "CoolProp==8.0.0", "python", "scripts/oracle/gen.py"];
        assert_eq!(run.args, [&args[..], &["--lock", LOCK, "--kind", "facts"]].concat());
        let env = [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/u"),
            ("LC_ALL", "C"),
            ("TZ", "UTC"),
            ("PYTHONHASHSEED", "0"),
            ("PYTHONDONTWRITEBYTECODE", "1"),
        ];
        assert_eq!(run.env, env.map(|(k, v)| (k.to_string(), v.to_string())));
        let cached = invocation(UV, &[], "/usr/bin", "/home/u", Some("/cache")).unwrap();
        assert_eq!(cached.env.last(), Some(&("UV_CACHE_DIR".to_string(), "/cache".to_string())));
        assert!(invocation("version 8.0.0\n", &[], "/usr/bin", "/home/u", None).is_err(), "no uv_command in the lock");
    }
}
