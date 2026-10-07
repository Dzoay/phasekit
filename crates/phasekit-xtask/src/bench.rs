//! `cargo xtask bench --record [--step M<n>.<k>]` (VERIFICATION.md §12; PLAN.md §5.1): runs the criterion benches of
//! `phasekit-verify` and records each benchmark's median in `crates/phasekit-verify/benches/results/M<n>-<machine>.csv`,
//! one row per (bench, fluid), replacing that pair's earlier row. Ids are `<group>/<fluid>/<case>`; the step defaults
//! to the branch name's `m<n>.<k>` prefix.

use std::process::ExitCode;

use crate::repo::Repo;

/// The results directory, relative to the repository root.
pub const RESULTS: &str = "crates/phasekit-verify/benches/results";

const COLUMNS: &str = "date,commit,step,bench,fluid,median,unit,cpu,os,rustc,governor";

/// Where and when a result was measured.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Machine {
    pub date: String,
    pub commit: String,
    pub cpu: String,
    pub os: String,
    pub rustc: String,
    pub governor: String,
}

/// The machine name of a CPU model: `Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz` → `intel-core-i7-8700k`.
pub fn slug(cpu: &str) -> String {
    let model = cpu.split('@').next().unwrap_or_default().replace("(R)", "").replace("(TM)", "").replace(" CPU", "");
    let mut out = String::new();
    for c in model.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// One result row from criterion's `benchmark.json` and `estimates.json`: (bench, fluid, median in ns).
pub fn criterion_row(benchmark: &str, estimates: &str) -> Result<(String, String, f64), String> {
    let benchmark: serde_json::Value = serde_json::from_str(benchmark).map_err(|e| format!("benchmark.json: {e}"))?;
    let estimates: serde_json::Value = serde_json::from_str(estimates).map_err(|e| format!("estimates.json: {e}"))?;
    let text = |key: &str| benchmark.get(key).and_then(|v| v.as_str()).ok_or(format!("benchmark.json: no {key}"));
    let (group, fluid, case) = (text("group_id")?, text("function_id")?, text("value_str")?);
    let median = estimates.pointer("/median/point_estimate").and_then(serde_json::Value::as_f64);
    let median = median.filter(|m| m.is_finite() && *m > 0.0).ok_or("estimates.json: no positive median")?;
    Ok((format!("{group}/{case}"), fluid.to_string(), median))
}

/// The results file with `rows` (bench, fluid, median ns) recorded at `step` on `machine`: each pair's earlier row is
/// replaced, rows are sorted by (bench, fluid).
pub fn merge(existing: &str, machine: &Machine, step: &str, rows: &[(String, String, f64)]) -> String {
    let mut kept: Vec<String> =
        existing.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).map(String::from).collect();
    kept.retain(|line| {
        let cells: Vec<&str> = line.split(',').collect();
        !rows
            .iter()
            .any(|(bench, fluid, _)| cells.get(3) == Some(&bench.as_str()) && cells.get(4) == Some(&fluid.as_str()))
    });
    let m = machine;
    for (bench, fluid, median) in rows {
        kept.push(format!(
            "{},{},{step},{bench},{fluid},{median:.3},ns,{},{},{},{}",
            m.date, m.commit, m.cpu, m.os, m.rustc, m.governor
        ));
    }
    kept.sort_by(|a, b| {
        let key = |l: &str| l.split(',').skip(3).take(2).collect::<Vec<_>>().join(",");
        key(a).cmp(&key(b))
    });
    let header = format!(
        "# fixture: phasekit-bench/v1 (VERIFICATION.md §12; written by `cargo xtask bench --record`)\n# machine: {}\n# columns: {COLUMNS}\n",
        slug(&m.cpu)
    );
    header + &kept.iter().map(|l| format!("{l}\n")).collect::<String>()
}

/// `m2.9-lookup-cost` → `M2.9`.
pub fn step_of_branch(branch: &str) -> Option<String> {
    let prefix = branch.split('-').next()?.strip_prefix('m')?;
    let (major, minor) = prefix.split_once('.')?;
    (major.chars().all(|c| c.is_ascii_digit()) && minor.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| format!("M{major}.{minor}"))
}

/// The results file's milestone: `M5.11` → `M5`, so the step that closes a milestone (and raises `MILESTONE`) still
/// records into that milestone's file.
pub fn milestone_of_step(step: &str) -> Option<&str> {
    let (milestone, minor) = step.split_once('.')?;
    let number = milestone.strip_prefix('M')?;
    let valid = !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) && !minor.is_empty();
    valid.then_some(milestone)
}

pub fn main(args: &[String]) -> ExitCode {
    let repo = Repo::locate();
    let step = match args {
        [record] if record == "--record" => {
            repo.git(&["branch", "--show-current"]).ok().and_then(|b| step_of_branch(b.trim()))
        }
        [record, flag, step] if record == "--record" && flag == "--step" => Some(step.clone()),
        _ => None,
    };
    let Some((step, milestone)) = step.as_deref().and_then(|s| Some((s, milestone_of_step(s)?))) else {
        eprintln!("usage: cargo xtask bench --record [--step M<n>.<k>] (the step comes from an m<n>.<k>-... branch)");
        return ExitCode::FAILURE;
    };
    let run = || -> Result<String, String> {
        repo.bench()?;
        let rows = repo.criterion_results()?.iter().map(|(b, e)| criterion_row(b, e)).collect::<Result<Vec<_>, _>>()?;
        let machine = repo.machine()?;
        let file = format!("{RESULTS}/{milestone}-{}.csv", slug(&machine.cpu));
        let existing = repo.read(&file).unwrap_or_default();
        repo.write_bytes(&file, merge(&existing, &machine, step, &rows).as_bytes())?;
        Ok(format!("bench: recorded {} rows in {file}", rows.len()))
    };
    match run() {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("bench: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Machine names match the C++ baseline's (`coolprop-8.0.0-intel-core-i7-8700k.csv`).
    #[test]
    fn machine_names_come_from_the_cpu_model() {
        assert_eq!(slug("Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz"), "intel-core-i7-8700k");
        assert_eq!(slug("AMD Ryzen 9 7950X 16-Core Processor"), "amd-ryzen-9-7950x-16-core-processor");
        assert_eq!(slug("  (TM) Apple  M3 "), "apple-m3", "no leading or doubled separator");
        assert_eq!(step_of_branch("m2.9-lookup-cost"), Some("M2.9".into()));
        assert_eq!(step_of_branch("m12.3a-x"), Some("M12.3a".into()));
        assert_eq!(step_of_branch("main"), None);
        assert_eq!(step_of_branch("mx.1-y"), None);
        assert_eq!(milestone_of_step("M5.11"), Some("M5"));
        assert_eq!(milestone_of_step("M12.3a"), Some("M12"));
        for refused in ["M5", "M5.", "M.1", "Mx.1", "5.11", "m5.11"] {
            assert_eq!(milestone_of_step(refused), None, "{refused}");
        }
    }

    /// A criterion result becomes (group/case, fluid, median ns); a missing or non-positive median is an error.
    #[test]
    fn criterion_results_are_read() {
        let benchmark = r#"{"group_id":"lookup_by_name","function_id":"R134a","value_str":"alias"}"#;
        let estimates = r#"{"median":{"point_estimate":46.7,"standard_error":0.1}}"#;
        assert_eq!(criterion_row(benchmark, estimates), Ok(("lookup_by_name/alias".into(), "R134a".into(), 46.7)));
        assert!(criterion_row(benchmark, r#"{"median":{"point_estimate":0.0}}"#).is_err());
        assert!(criterion_row(r#"{"group_id":"g"}"#, estimates).unwrap_err().contains("no function_id"));
    }

    /// Recording replaces a (bench, fluid) pair's earlier row and keeps every other, sorted.
    #[test]
    fn recording_replaces_only_its_own_rows() {
        let m = Machine {
            date: "2026-10-06".into(),
            commit: "abc".into(),
            cpu: "Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz".into(),
            os: "Linux 7.2".into(),
            rustc: "rustc 1.99.0".into(),
            governor: "powersave".into(),
        };
        let first = merge(
            "",
            &m,
            "M2.9",
            &[("lookup/name".into(), "R134a".into(), 50.0), ("alpha/x".into(), "Water".into(), 9.0)],
        );
        assert!(first.starts_with("# fixture: phasekit-bench/v1"));
        assert!(first.contains("# machine: intel-core-i7-8700k\n"));
        let water = merge(&first, &m, "M2.9", &[("lookup/name".into(), "Water".into(), 60.0)]);
        let again = merge(&water, &m, "M3.8", &[("lookup/name".into(), "R134a".into(), 40.0)]);
        assert_eq!(again.matches("# fixture:").count(), 1, "comments are not rows");
        assert!(again.contains(",M2.9,lookup/name,Water,60.000,ns,"), "the same bench for another fluid stays");
        let rows: Vec<&str> =
            again.lines().filter(|l| !l.starts_with('#') && l.contains(",R134a,") || l.contains(",alpha/")).collect();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].starts_with("2026-10-06,abc,M2.9,alpha/x,Water,9.000,ns,"));
        assert!(
            rows[1].starts_with(
                "2026-10-06,abc,M3.8,lookup/name,R134a,40.000,ns,Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz,"
            )
        );
    }
}
