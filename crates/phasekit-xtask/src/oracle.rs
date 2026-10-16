//! `cargo xtask oracle <gen.py arguments>`: runs scripts/oracle/gen.py exactly as VERIFICATION.md §3.1 shows, in an
//! environment rebuilt from nothing (`env -i`), with the uv command taken from `oracle.lock` so the pin lives once.
//! `cargo xtask teqp <teqp.py arguments>` runs the GERG-2008 generator, scripts/oracle/teqp.py, the same way under
//! `teqp.lock` (VERIFICATION.md §3.8, PLAN.md M13.5).

use std::process::ExitCode;

use crate::repo::Repo;

/// The lock, relative to the repository root.
pub const LOCK: &str = "crates/phasekit-verify/fixtures/oracle.lock";

/// A generator script and the lock that pins its environment, both relative to the repository root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Generator {
    pub script: &'static str,
    pub lock: &'static str,
}

/// gen.py, the CoolProp oracle's generator.
pub const ORACLE: Generator = Generator { script: "scripts/oracle/gen.py", lock: LOCK };

/// teqp.py, the GERG-2008 generator (teqp's vectors and AGA8's test data).
pub const TEQP: Generator =
    Generator { script: "scripts/oracle/teqp.py", lock: "crates/phasekit-verify/fixtures/teqp.lock" };

/// A process to run: program, arguments and the whole environment.
#[derive(Debug, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The generator run: the lock's `uv_command`, then `python <script> --lock <lock>` and the caller's arguments; the
/// environment is PATH, HOME, the fixed locale and hashing settings, and UV_CACHE_DIR when set.
pub fn invocation(
    generator: Generator,
    lock: &str,
    gen_args: &[String],
    path: &str,
    home: &str,
    uv_cache_dir: Option<&str>,
) -> Result<Invocation, String> {
    let name = generator.lock;
    let lock = phasekit_verify::OracleLock::parse(lock)?;
    let uv = lock.get("uv_command").ok_or(format!("{name}: no uv_command"))?;
    let mut words = uv.split_whitespace().map(str::to_string);
    let program = words.next().ok_or(format!("{name}: uv_command is empty"))?;
    let tail = ["python", generator.script, "--lock", generator.lock].map(str::to_string);
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

pub fn main(generator: Generator, args: &[String]) -> ExitCode {
    let repo = Repo::locate();
    let invocation = repo.read(generator.lock).and_then(|lock| {
        let (path, home, uv_cache) = repo.oracle_environment();
        invocation(generator, &lock, args, &path, &home, uv_cache.as_deref())
    });
    match invocation.and_then(|invocation| repo.run(&invocation)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("xtask {}: {e}", generator.script);
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
        let run = invocation(ORACLE, UV, &words(&["--kind", "facts"]), "/usr/bin", "/home/u", None).unwrap();
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
        let cached = invocation(ORACLE, UV, &[], "/usr/bin", "/home/u", Some("/cache")).unwrap();
        assert_eq!(cached.env.last(), Some(&("UV_CACHE_DIR".to_string(), "/cache".to_string())));
        let no_uv = invocation(ORACLE, "version 8.0.0\n", &[], "/usr/bin", "/home/u", None);
        assert!(no_uv.is_err(), "no uv_command in the lock");
    }

    /// VERIFICATION.md §3.8: teqp.py runs the same way, under teqp.lock's uv command and with that lock.
    #[test]
    fn the_gerg_generator_runs_under_its_own_lock() {
        let lock = "uv_command uv run --no-project --python 3.12 --with teqp==0.23.2 --with numpy==2.5.4\n";
        let run = invocation(TEQP, lock, &words(&["--kind", "gerg"]), "/usr/bin", "/home/u", None).unwrap();
        let args = ["run", "--no-project", "--python", "3.12", "--with", "teqp==0.23.2", "--with", "numpy==2.5.4"];
        let script = ["python", "scripts/oracle/teqp.py", "--lock", TEQP.lock, "--kind", "gerg"];
        assert_eq!(run.args, [&args[..], &script[..]].concat());
        assert_eq!(run.env, invocation(ORACLE, UV, &[], "/usr/bin", "/home/u", None).unwrap().env);
    }
}
