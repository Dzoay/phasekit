//! `cargo xtask baseline [--check]` (VERIFICATION.md §12, PLAN.md M1.15). Without arguments it builds CoolProp v8.0.0
//! and the harness of scripts/baseline/ in a scratch directory and runs it on one core; `--check` checks the committed
//! result files in crates/phasekit-verify/benches/baseline/: at least one, each with every workload × fluid row.

use std::process::ExitCode;

use crate::oracle::Invocation;
use crate::repo::Repo;

/// The result files, one per machine.
pub const DIR: &str = "crates/phasekit-verify/benches/baseline";
/// `git -C reference/CoolProp status --porcelain --ignored`, recorded before the first build.
pub const REFERENCE_STATUS: &str = "scripts/baseline/reference-status.txt";
/// The 7 workloads of VERIFICATION.md §12, as the harness names them.
pub const WORKLOADS: [&str; 7] = ["alphar2", "dt_h_cp", "qt", "pq", "pt", "ph", "propssi"];
/// The bench fluids of VERIFICATION.md §12; n-Heptane is the 12-term fluid chosen at M1.15.
pub const FLUIDS: [&str; 5] = ["Water", "Methane", "R134a", "n-Propane", "n-Heptane"];
/// The header keys of a result file, in order.
const HEADER: [&str; 9] = ["fixture", "coolprop", "cpu", "os", "governor", "date", "compiler", "method", "columns"];

/// The build and run: scripts/baseline/build.sh with PATH, HOME and SCRATCH only.
pub fn invocation(scratch: &str, path: &str, home: &str) -> Invocation {
    let env = [("PATH", path), ("HOME", home), ("SCRATCH", scratch)];
    Invocation {
        program: "bash".into(),
        args: vec!["scripts/baseline/build.sh".into()],
        env: env.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    }
}

/// The reference checkout's status (`None` when it is absent) lists nothing beyond what was recorded before the first
/// build: a build that writes into the checkout adds lines, while a recorded entry (the bytecode caches of local oracle
/// runs) may be missing, as on a fresh CI checkout.
pub fn reference_untouched(status: Option<Result<String, String>>, recorded: &str) -> Result<(), String> {
    match status {
        None => Ok(()),
        Some(Ok(status)) => {
            let added: Vec<&str> = status.lines().filter(|line| !recorded.lines().any(|r| r == *line)).collect();
            if added.is_empty() { Ok(()) } else { Err(format!("reference/CoolProp changed: {added:?}")) }
        }
        Some(Err(e)) => Err(e),
    }
}

/// Checks one result file; returns its CPU model.
pub fn check(name: &str, text: &str) -> Result<String, Vec<String>> {
    let mut errors = Vec::new();
    let header: Vec<(&str, &str)> = text
        .lines()
        .take_while(|line| line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("# ")?.split_once(": "))
        .collect();
    let keys: Vec<&str> = header.iter().map(|(key, _)| *key).collect();
    if keys != HEADER {
        errors.push(format!("{name}: the header keys are {keys:?}, not {HEADER:?}"));
    }
    let get = |key: &str| header.iter().find(|(k, _)| *k == key).map_or("", |(_, v)| *v);
    if get("fixture") != "coolprop-baseline/v1" || !get("coolprop").starts_with("8.0.0 git=ae81610e") {
        errors.push(format!("{name}: not a CoolProp 8.0.0 (ae81610e) baseline"));
    }
    if get("cpu").trim().is_empty() {
        errors.push(format!("{name}: no CPU model"));
    }
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate().filter(|(_, line)| !line.starts_with('#')) {
        let fields: Vec<&str> = line.splitn(7, ',').collect();
        let &[workload, fluid, states, median, min, max, grid] = fields.as_slice() else {
            errors.push(format!("{name}:{}: `{line}` is not workload,fluid,states,median,min,max,grid", n + 1));
            continue;
        };
        let ns = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0);
        let timed = matches!(
            (states.parse::<usize>(), ns(median), ns(min), ns(max)),
            (Ok(states), Some(median), Some(min), Some(max)) if states > 0 && min <= median && median <= max
        );
        if !timed || !grid.contains("seed=1") {
            errors.push(format!("{name}:{}: needs states > 0, 0 < min <= median <= max and a seeded grid", n + 1));
        }
        rows.push((workload, fluid));
    }
    let want: Vec<(&str, &str)> = FLUIDS.iter().flat_map(|f| WORKLOADS.iter().map(move |w| (*w, *f))).collect();
    if rows != want {
        errors.push(format!("{name}: rows {rows:?}, want every workload of every fluid in order"));
    }
    if errors.is_empty() { Ok(get("cpu").to_string()) } else { Err(errors) }
}

/// Every result file in `files` (name, text); at least one.
pub fn check_all(files: &[(String, String)]) -> Result<Vec<String>, Vec<String>> {
    let baselines: Vec<&(String, String)> = files.iter().filter(|(name, _)| name.ends_with(".csv")).collect();
    if baselines.is_empty() {
        return Err(vec![format!("{DIR}: no coolprop-8.0.0-<machine>.csv (cargo xtask baseline)")]);
    }
    let (mut machines, mut errors) = (Vec::new(), Vec::new());
    for (name, text) in baselines {
        if !name.rsplit('/').next().is_some_and(|file| file.starts_with("coolprop-8.0.0-")) {
            errors.push(format!("{name}: a baseline is named coolprop-8.0.0-<machine>.csv"));
        }
        match check(name, text) {
            Ok(cpu) => machines.push(cpu),
            Err(e) => errors.extend(e),
        }
    }
    if errors.is_empty() { Ok(machines) } else { Err(errors) }
}

pub fn main(args: &[String]) -> ExitCode {
    let repo = Repo::locate();
    let result = match args {
        [flag] if flag == "--check" => repo
            .files(DIR, "csv", false)
            .map_err(|e| vec![e])
            .and_then(|files| check_all(&files))
            .map(|machines| format!("{} baseline(s): {}", machines.len(), machines.join("; "))),
        [] => {
            let scratch =
                repo.var("SCRATCH").or_else(|| repo.var("CARGO_TARGET_DIR").map(|t| format!("{t}/coolprop-baseline")));
            let (path, home, _) = repo.oracle_environment();
            match scratch {
                None => Err(vec!["set SCRATCH (or CARGO_TARGET_DIR) to a directory outside the repository".into()]),
                Some(scratch) => match repo.run(&invocation(&scratch, &path, &home)) {
                    Ok(true) => repo
                        .read(REFERENCE_STATUS)
                        .and_then(|recorded| reference_untouched(repo.reference_status(), &recorded))
                        .map(|()| "built and ran the baseline".into())
                        .map_err(|e| vec![e]),
                    Ok(false) => Err(vec!["scripts/baseline/build.sh failed".into()]),
                    Err(e) => Err(vec![e]),
                },
            }
        }
        _ => Err(vec!["usage: cargo xtask baseline [--check]".into()]),
    };
    match result {
        Ok(summary) => {
            println!("xtask baseline: {summary}");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            errors.iter().for_each(|e| eprintln!("xtask baseline: {e}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A well-formed result file: every workload of every fluid.
    fn baseline() -> String {
        let mut text = String::from(
            "# fixture: coolprop-baseline/v1\n# coolprop: 8.0.0 git=ae81610e7d23efc57f9d051c8e70a4d66e87537f\n\
             # cpu: Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz\n# os: Linux 7.2\n# governor: powersave\n\
             # date: 2026-10-05\n# compiler: 16.2.1\n# method: median of 7\n\
             # columns: workload,fluid,states,median_ns,min_ns,max_ns,grid\n",
        );
        for fluid in FLUIDS {
            for workload in WORKLOADS {
                text.push_str(&format!("{workload},{fluid},100,2.5,2,3,T=U[1;2] rho=U[3;4] seed=1\n"));
            }
        }
        text
    }

    /// VERIFICATION.md §12: a result file names the pin and the CPU and holds the 7 workloads × 5 fluids, each timed
    /// (0 < min ≤ median ≤ max) on a seeded grid; anything else is reported with the file.
    #[test]
    fn baseline_files_hold_every_workload_and_fluid() {
        let good = baseline();
        assert_eq!(check("x.csv", &good), Ok("Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz".into()));
        let named = |text: &str| vec![(format!("{DIR}/coolprop-8.0.0-i7.csv"), text.to_string())];
        assert_eq!(check_all(&named(&good)).map(|m| m.len()), Ok(1));
        assert!(check_all(&[]).is_err(), "no baseline");
        assert!(check_all(&[(format!("{DIR}/other.csv"), good.clone())]).is_err(), "misnamed");
        let broken = [
            good.replace("# cpu: Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz", "# cpu: "),
            good.replace("git=ae81610e", "git=0000000e"),
            good.replace("coolprop-baseline/v1", "coolprop-baseline/v2"),
            good.replace("# governor: powersave\n", ""),
            good.replace("propssi,n-Heptane,100,2.5,2,3,T=U[1;2] rho=U[3;4] seed=1\n", ""),
            good.replacen("alphar2,Water,100,2.5,2,3", "alphar2,Water,100,3.5,2,3", 1),
            good.replacen("alphar2,Water,100,2.5,2,3", "alphar2,Water,100,1.5,2,3", 1),
            good.replacen("alphar2,Water,100,2.5,2,3", "alphar2,Water,0,2.5,2,3", 1),
            good.replacen("alphar2,Water,100,2.5,2,3", "alphar2,Water,100,2.5,0,3", 1),
            good.replacen("alphar2,Water,100,2.5,2,3", "alphar2,Water,100,inf,2,3", 1),
            good.replacen("seed=1", "seed=2", 1),
            good.replacen("alphar2,Water", "qt,Water", 1),
            good.replacen("alphar2,Water,100", "alphar2,Water", 1),
        ];
        for (i, text) in broken.iter().enumerate() {
            assert!(check("x.csv", text).is_err(), "case {i} was accepted");
            assert!(check_all(&named(text)).is_err(), "case {i} was accepted by check_all");
        }
        let edge = good.replacen("alphar2,Water,100,2.5,2,3", "alphar2,Water,1,2,2,2", 1);
        assert!(check("x.csv", &edge).is_ok(), "min = median = max and one state are a timing");
    }

    /// The build runs scripts/baseline/build.sh with only PATH, HOME and SCRATCH.
    #[test]
    fn build_invocation_passes_only_the_scratch_directory() {
        let run = invocation("/tmp/s", "/bin", "/home/u");
        assert_eq!(
            (run.program.as_str(), run.args.as_slice()),
            ("bash", &["scripts/baseline/build.sh".to_string()][..])
        );
        let env: Vec<(&str, &str)> = run.env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(env, [("PATH", "/bin"), ("HOME", "/home/u"), ("SCRATCH", "/tmp/s")]);
    }

    /// PLAN.md M1.15: building the baseline never writes reference/CoolProp: its status, ignored files included, is the
    /// one recorded before the first build (scripts/baseline/reference-status.txt).
    #[test]
    fn reference_checkout_is_untouched() {
        let repo = Repo::locate();
        let recorded = repo.read(REFERENCE_STATUS).unwrap_or_else(|e| panic!("{e}"));
        assert!(recorded.lines().all(|line| line.starts_with("!! ")), "only ignored files: {recorded:?}");
        assert_eq!(reference_untouched(repo.reference_status(), &recorded), Ok(()));
        // The comparison itself: absent, as recorded, clean (a fresh CI checkout), changed, unreadable.
        let changed = format!("{recorded} M src/CoolProp.cpp\n");
        assert_eq!(reference_untouched(None, &recorded), Ok(()));
        assert_eq!(reference_untouched(Some(Ok(recorded.clone())), &recorded), Ok(()));
        assert_eq!(reference_untouched(Some(Ok(String::new())), &recorded), Ok(()));
        assert!(reference_untouched(Some(Ok(changed)), &recorded).is_err_and(|e| e.contains("src/CoolProp.cpp")));
        assert!(reference_untouched(Some(Ok("?? include/x.h\n".into())), "").is_err());
        assert!(reference_untouched(Some(Err("git failed".into())), &recorded).is_err());
    }
}
