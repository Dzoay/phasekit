//! `cargo xtask baseline [--check]` (VERIFICATION.md §12, PLAN.md M1.15, M1.15a). Without arguments it builds CoolProp
//! v8.0.0 and the harness of scripts/baseline/ in a scratch directory and runs it; `--check` checks the committed result
//! files in crates/phasekit-verify/benches/baseline/: at least one machine, each with its timing, memory and scaling
//! files and every row of each.

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
/// The thread counts of the scaling file: 1, 2, 4, the reference machine's 6 physical cores and its 12 hardware threads.
pub const THREADS: [usize; 5] = [1, 2, 4, 6, 12];
/// The memory file's rows per fluid, after its library row and before its all-fluids row.
const MEMORY_PER_FLUID: [&str; 2] = ["state", "state_after_qt"];
/// The header keys of a timing or scaling file, in order.
const HEADER: [&str; 9] = ["fixture", "coolprop", "cpu", "os", "governor", "date", "compiler", "method", "columns"];
/// The header keys of a memory file (no governor: it does not move a byte count).
const MEMORY_HEADER: [&str; 8] = ["fixture", "coolprop", "cpu", "os", "date", "compiler", "method", "columns"];

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

/// A result file's header against its keys and kind; returns its CPU model, or the errors.
fn check_header(name: &str, text: &str, keys: &[&str], kind: &str) -> (String, Vec<String>) {
    let mut errors = Vec::new();
    let header: Vec<(&str, &str)> = text
        .lines()
        .take_while(|line| line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("# ")?.split_once(": "))
        .collect();
    let found: Vec<&str> = header.iter().map(|(key, _)| *key).collect();
    if found != keys {
        errors.push(format!("{name}: the header keys are {found:?}, not {keys:?}"));
    }
    let get = |key: &str| header.iter().find(|(k, _)| *k == key).map_or("", |(_, v)| *v);
    if get("fixture") != kind || !get("coolprop").starts_with("8.0.0 git=ae81610e") {
        errors.push(format!("{name}: not a CoolProp 8.0.0 (ae81610e) `{kind}` file"));
    }
    if get("cpu").trim().is_empty() {
        errors.push(format!("{name}: no CPU model"));
    }
    (get("cpu").to_string(), errors)
}

/// The data rows of a result file, split into at most `fields` comma-separated fields, with their line numbers.
fn rows(text: &str, fields: usize) -> impl Iterator<Item = (usize, Vec<&str>)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with('#'))
        .map(move |(n, line)| (n + 1, line.splitn(fields, ',').collect()))
}

/// A finite positive number.
fn positive(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0)
}

/// Checks one timing file; returns its CPU model.
pub fn check(name: &str, text: &str) -> Result<String, Vec<String>> {
    let (cpu, mut errors) = check_header(name, text, &HEADER, "coolprop-baseline/v1");
    let mut seen = Vec::new();
    for (n, fields) in rows(text, 7) {
        let &[workload, fluid, states, median, min, max, grid] = fields.as_slice() else {
            errors.push(format!("{name}:{n}: not workload,fluid,states,median,min,max,grid"));
            continue;
        };
        let timed = matches!(
            (states.parse::<usize>(), positive(median), positive(min), positive(max)),
            (Ok(states), Some(median), Some(min), Some(max)) if states > 0 && min <= median && median <= max
        );
        if !timed || !grid.contains("seed=1") {
            errors.push(format!("{name}:{n}: needs states > 0, 0 < min <= median <= max and a seeded grid"));
        }
        seen.push((workload, fluid));
    }
    let want: Vec<(&str, &str)> = FLUIDS.iter().flat_map(|f| WORKLOADS.iter().map(move |w| (*w, *f))).collect();
    if seen != want {
        errors.push(format!("{name}: rows {seen:?}, want every workload of every fluid in order"));
    }
    if errors.is_empty() { Ok(cpu) } else { Err(errors) }
}

/// Checks one memory file: the library row, `state` and `state_after_qt` for every bench fluid (heap > 0, the state
/// growing with its update) and one all-fluids row naming the fluid count; byte counts are integers.
pub fn check_memory(name: &str, text: &str) -> Result<String, Vec<String>> {
    let (cpu, mut errors) = check_header(name, text, &MEMORY_HEADER, "coolprop-baseline-memory/v1");
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut heap = Vec::new();
    for (n, fields) in rows(text, 4) {
        let &[measure, fluid, heap_bytes, rss_bytes] = fields.as_slice() else {
            errors.push(format!("{name}:{n}: not measure,fluid,heap_bytes,rss_bytes"));
            continue;
        };
        match (heap_bytes.parse::<i64>(), rss_bytes.parse::<i64>()) {
            (Ok(h), Ok(_)) if h > 0 => heap.push(h),
            _ => errors.push(format!("{name}:{n}: heap bytes > 0 and integer rss bytes")),
        }
        seen.push((measure.to_string(), fluid.to_string()));
    }
    let mut want = vec![("library_first_use".to_string(), "Water".to_string())];
    for fluid in FLUIDS {
        want.extend(MEMORY_PER_FLUID.map(|m| (m.to_string(), fluid.to_string())));
    }
    let all = seen
        .last()
        .filter(|(m, count)| m == "all_fluids_one_state_each" && count.parse::<usize>().is_ok_and(|c| c > 0));
    want.push(all.cloned().unwrap_or(("all_fluids_one_state_each".into(), "<fluid count>".into())));
    if seen != want {
        errors.push(format!("{name}: rows {seen:?}, want {want:?}"));
    } else if heap
        .get(1..=2 * FLUIDS.len())
        .is_some_and(|states| states.chunks(2).any(|pair| pair.first() > pair.get(1)))
    {
        errors.push(format!("{name}: a state shrank after its QT update"));
    }
    if errors.is_empty() { Ok(cpu) } else { Err(errors) }
}

/// Checks one scaling file: every mode (each bench fluid, then `mixed`) at every thread count of `THREADS`, ns per
/// state positive, speedup exactly 1 at one thread and positive elsewhere.
pub fn check_scaling(name: &str, text: &str) -> Result<String, Vec<String>> {
    let (cpu, mut errors) = check_header(name, text, &HEADER, "coolprop-baseline-scaling/v1");
    let mut seen: Vec<(String, usize)> = Vec::new();
    for (n, fields) in rows(text, 5) {
        let &[mode, threads, states, ns, speedup] = fields.as_slice() else {
            errors.push(format!("{name}:{n}: not mode,threads,states_per_thread,median_ns_per_state,speedup"));
            continue;
        };
        let threads = threads.parse::<usize>().unwrap_or(0);
        let one = threads != 1 || speedup == "1";
        if states.parse::<usize>().map_or(true, |s| s == 0)
            || positive(ns).is_none()
            || positive(speedup).is_none()
            || !one
        {
            errors.push(format!("{name}:{n}: needs states > 0, ns > 0, speedup > 0 and exactly 1 at one thread"));
        }
        seen.push((mode.to_string(), threads));
    }
    let modes = FLUIDS.iter().copied().chain(["mixed"]);
    let want: Vec<(String, usize)> = modes.flat_map(|m| THREADS.map(|t| (m.to_string(), t))).collect();
    if seen != want {
        errors.push(format!("{name}: rows {seen:?}, want every mode at {THREADS:?} threads in order"));
    }
    if errors.is_empty() { Ok(cpu) } else { Err(errors) }
}

/// Every result file in `files` (name, text): at least one machine, each with `coolprop-8.0.0-<machine>.csv`,
/// `.memory.csv` and `.scaling.csv`, all well formed and naming one CPU.
pub fn check_all(files: &[(String, String)]) -> Result<Vec<String>, Vec<String>> {
    let file = |name: &str| name.rsplit('/').next().unwrap_or(name).to_string();
    let csv: Vec<&(String, String)> = files.iter().filter(|(name, _)| name.ends_with(".csv")).collect();
    // Every file present must find its trio below, so any file at all means at least one timing file.
    if csv.is_empty() {
        return Err(vec![format!("{DIR}: no coolprop-8.0.0-<machine>.csv (cargo xtask baseline)")]);
    }
    let (mut machines, mut errors) = (Vec::new(), Vec::new());
    for (name, text) in &csv {
        let base = file(name);
        let kind = [".memory.csv", ".scaling.csv"].into_iter().find(|suffix| base.ends_with(suffix)).unwrap_or(".csv");
        let stem = base.strip_suffix(kind).unwrap_or(&base);
        if !stem.starts_with("coolprop-8.0.0-") {
            errors.push(format!("{name}: a baseline file is named coolprop-8.0.0-<machine>{kind}"));
        }
        let names = |suffix: &str| csv.iter().any(|(other, _)| file(other) == format!("{stem}{suffix}"));
        if !names(".csv") || !names(".memory.csv") || !names(".scaling.csv") {
            errors.push(format!("{name}: {stem} needs its .csv, .memory.csv and .scaling.csv"));
        }
        let checked = match kind {
            ".memory.csv" => check_memory(name, text),
            ".scaling.csv" => check_scaling(name, text),
            _ => check(name, text),
        };
        match checked {
            Ok(cpu) if kind == ".csv" => machines.push(cpu),
            Ok(_) => {}
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

    const CPU: &str = "# cpu: Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz\n# os: Linux 7.2\n";

    /// A well-formed memory file.
    fn memory() -> String {
        let mut text = format!(
            "# fixture: coolprop-baseline-memory/v1\n# coolprop: 8.0.0 git=ae81610e\n{CPU}# date: 2026-10-05\n\
             # compiler: 16\n# method: mallinfo2\n# columns: measure,fluid,heap_bytes,rss_bytes\nlibrary_first_use,Water,9,9\n"
        );
        for fluid in FLUIDS {
            text.push_str(&format!("state,{fluid},500,0\nstate_after_qt,{fluid},500,4\n"));
        }
        text + "all_fluids_one_state_each,136,1000,2000\n"
    }

    /// A well-formed scaling file.
    fn scaling() -> String {
        let mut text = format!(
            "# fixture: coolprop-baseline-scaling/v1\n# coolprop: 8.0.0 git=ae81610e\n{CPU}# governor: powersave\n\
             # date: 2026-10-05\n# compiler: 16\n# method: median of 5\n\
             # columns: mode,threads,states_per_thread,median_ns_per_state,speedup\n"
        );
        for mode in FLUIDS.iter().copied().chain(["mixed"]) {
            for threads in THREADS {
                let speedup = if threads == 1 { "1".to_string() } else { format!("{}", threads as f64 * 0.8) };
                text.push_str(&format!("{mode},{threads},2000,1000,{speedup}\n"));
            }
        }
        text
    }

    /// VERIFICATION.md §12 (M1.15a): the memory file holds the library row, both rows of every bench fluid (heap > 0, not
    /// shrinking with the QT update) and the all-fluids row; the scaling file every mode at every thread count, speedup
    /// exactly 1 at one thread; a machine needs all three files.
    #[test]
    fn memory_and_scaling_files_are_complete() {
        let (good_memory, good_scaling) = (memory(), scaling());
        assert!(check_memory("m.csv", &good_memory).is_ok() && check_scaling("s.csv", &good_scaling).is_ok());
        let broken_memory = [
            good_memory.replace("state,Water,500,0", "state,Water,0,0"),
            good_memory.replace("state,Water,500,0", "state,Water,600,0"),
            good_memory.replace("state_after_qt,Water,500,4", "state_after_qt,Water,500,x"),
            good_memory.replace("all_fluids_one_state_each,136", "all_fluids_one_state_each,many"),
            good_memory.replace("all_fluids_one_state_each,136", "all_fluids_one_state_each,0"),
            good_memory.replace("state_after_qt,n-Heptane,500,4\n", ""),
            good_memory.replace("library_first_use", "library"),
            good_memory.replace("state,R134a,500,0", "state,R134a,500"),
            good_memory.replace("# date: 2026-10-05\n", "# governor: powersave\n# date: 2026-10-05\n"),
            good_memory.replace("coolprop-baseline-memory/v1", "coolprop-baseline/v1"),
        ];
        for (i, text) in broken_memory.iter().enumerate() {
            assert!(check_memory("m.csv", text).is_err(), "memory case {i} was accepted");
        }
        let edge = good_memory.replace("state_after_qt,Water,500,4", "state_after_qt,Water,500,-4");
        assert!(check_memory("m.csv", &edge).is_ok(), "an rss delta may be negative or zero; heap may stay equal");
        let broken_scaling = [
            good_scaling.replace("Water,1,2000,1000,1\n", "Water,1,2000,1000,1.0\n"),
            good_scaling.replace("Water,2,2000,1000,1.6", "Water,2,2000,1000,0"),
            good_scaling.replace("Water,2,2000,1000,1.6", "Water,2,2000,0,1.6"),
            good_scaling.replace("Water,2,2000,1000,1.6", "Water,2,0,1000,1.6"),
            good_scaling.replace("mixed,12,2000,1000,9.600000000000001\n", ""),
            good_scaling.replace("Methane,4", "Methane,3"),
            good_scaling.replace("Water,2,2000,1000,1.6", "Water,2,2000,1000"),
            good_scaling.replace("# governor: powersave\n", ""),
        ];
        for (i, text) in broken_scaling.iter().enumerate() {
            assert!(check_scaling("s.csv", text).is_err(), "scaling case {i} was accepted");
        }
        let machine = |m: &str, s: &str| {
            vec![
                (format!("{DIR}/coolprop-8.0.0-i7.csv"), baseline()),
                (format!("{DIR}/coolprop-8.0.0-i7.memory.csv"), m.to_string()),
                (format!("{DIR}/coolprop-8.0.0-i7.scaling.csv"), s.to_string()),
            ]
        };
        assert_eq!(check_all(&machine(&good_memory, &good_scaling)).map(|m| m.len()), Ok(1));
        assert!(
            check_all(&machine(&broken_memory[0], &good_scaling)).is_err(),
            "a broken memory file fails the machine"
        );
        assert!(
            check_all(&machine(&good_memory, &broken_scaling[0])).is_err(),
            "a broken scaling file fails the machine"
        );
        for missing in 0..3 {
            let mut files = machine(&good_memory, &good_scaling);
            files.remove(missing);
            assert!(check_all(&files).is_err(), "a machine without file {missing} of its three");
        }
        let mut stray = machine(&good_memory, &good_scaling);
        stray.push((format!("{DIR}/other.memory.csv"), good_memory.clone()));
        assert!(check_all(&stray).is_err(), "a misnamed memory file");
    }

    /// VERIFICATION.md §12: a result file names the pin and the CPU and holds the 7 workloads × 5 fluids, each timed
    /// (0 < min ≤ median ≤ max) on a seeded grid; anything else is reported with the file.
    #[test]
    fn baseline_files_hold_every_workload_and_fluid() {
        let good = baseline();
        assert_eq!(check("x.csv", &good), Ok("Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz".into()));
        let named = |text: &str| {
            vec![
                (format!("{DIR}/coolprop-8.0.0-i7.csv"), text.to_string()),
                (format!("{DIR}/coolprop-8.0.0-i7.memory.csv"), memory()),
                (format!("{DIR}/coolprop-8.0.0-i7.scaling.csv"), scaling()),
            ]
        };
        assert_eq!(check_all(&named(&good)).map(|m| m.len()), Ok(1));
        assert!(check_all(&[]).is_err(), "no baseline");
        let misnamed = [
            (format!("{DIR}/other.csv"), good.clone()),
            (format!("{DIR}/other.memory.csv"), memory()),
            (format!("{DIR}/other.scaling.csv"), scaling()),
        ];
        assert!(check_all(&misnamed).is_err(), "misnamed");
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
        let first_row = check("x.csv", broken.get(5).map_or("", String::as_str)).unwrap_err();
        assert!(first_row.iter().any(|e| e.starts_with("x.csv:10:")), "errors name the file's line: {first_row:?}");
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
