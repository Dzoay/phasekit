//! `gates perf` (VERIFICATION.md §11.2, §12; PLAN.md M9.6): the performance gate. With no arguments, the instruction
//! gate: it runs `benches/instructions.rs` under gungraun, a static musl build with `--features phasekit-core/libm`,
//! reads each benchmark's Callgrind instruction count from its `summary.json`, and fails on a count more than 5 % above
//! `crates/phasekit-verify/benches/baseline/instructions.csv`, on a baselined benchmark that did not run, or on one
//! with no baseline row. Every run writes the counts as a proposed baseline to `<target>/instructions-baseline.csv`,
//! which `--update` writes over the committed one. `--targets <results.csv>` instead checks a criterion results file
//! (`cargo xtask bench --record` on the reference machine) against ARCHITECTURE.md §7's targets, each miss failing
//! unless `ci/perf-exemptions.txt` names it with its re-plan issue.

use super::Verdict;
use crate::repo::Repo;

/// The committed instruction baseline, relative to the repository root.
pub const BASELINE: &str = "crates/phasekit-verify/benches/baseline/instructions.csv";

/// The misses of ARCHITECTURE.md §7's targets that have a re-plan issue (PLAN.md M9.6).
const EXEMPTIONS: &str = "ci/perf-exemptions.txt";

/// The allowed increase over the baseline, in percent: 5 *(decision: noise margin, VERIFICATION.md §12)*.
const LIMIT_PERCENT: u128 = 5;

/// ARCHITECTURE.md §7's per-core targets, as criterion bench ids of `benches/` (the `bench` column of a results file),
/// the fluids they apply to (`None`: every fluid the bench records), the target in ns, and the §7 row it comes from.
/// `targets_match_the_architecture_table` keeps them equal to the table.
pub const TARGETS: [(&str, Option<&str>, f64, &str); 7] = [
    ("alphar_order2/residual", Some("n-Propane"), 300.0, "α^r bundle, order 2, 16-20 terms | ≤ 0.3 µs"),
    ("properties_at_t_rho/dt_h_cp", None, 500.0, "Properties at (T, ρ) | ≤ 0.5 µs"),
    ("qt_superancillary/flash", None, 100.0, "QT / PQ via superancillary | ≤ 0.1 µs"),
    ("pq_superancillary/flash", None, 100.0, "QT / PQ via superancillary | ≤ 0.1 µs"),
    ("pt_flash/pt_h", None, 3_000.0, "PT / PH single phase | ≤ 3 µs / ≤ 15 µs"),
    ("ph_flash/ph_t", None, 15_000.0, "PT / PH single phase | ≤ 3 µs / ≤ 15 µs"),
    ("lookup_by_name/name", None, 50.0, "Hot lookup by name | ≤ 50 ns"),
];

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    match args {
        [] => instructions(repo, false),
        [flag] if flag == "--update" => instructions(repo, true),
        [flag, results] if flag == "--targets" => {
            let results = repo.read(results).map_err(|e| vec![e])?;
            let exemptions = repo.read(EXEMPTIONS).map_err(|e| vec![e])?;
            check_targets(&results, &exemptions)
        }
        _ => Err(vec!["usage: gates perf [--update | --targets <results.csv>]".into()]),
    }
}

/// The instruction gate: run, propose, then check against the committed baseline (or replace it).
fn instructions(repo: &Repo, update: bool) -> Verdict {
    let counts = repo
        .instruction_summaries()
        .map_err(|e| vec![e])?
        .iter()
        .map(|json| summary_count(json))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| vec![e])?;
    let proposal = baseline_file(&counts);
    let proposed = repo.write_target("instructions-baseline.csv", proposal.as_bytes()).map_err(|e| vec![e])?;
    if update {
        repo.write_bytes(BASELINE, proposal.as_bytes()).map_err(|e| vec![e])?;
        return Ok(format!("{} benchmarks written to {BASELINE}", counts.len()));
    }
    let baseline = repo.read(BASELINE).map_err(|e| vec![format!("{e}; the run's counts are in {proposed}")])?;
    check(&baseline, &counts).map_err(|mut errors| {
        errors.push(format!("the run's counts, as a baseline: {proposed}"));
        errors
    })
}

/// A benchmark's id (`module path.id`, e.g. `instructions::kernel::pt_flash.water`) and its Callgrind instruction
/// count (`Ir`, the total over its parts) from a gungraun `summary.json` (format 7).
pub fn summary_count(json: &str) -> Result<(String, u64), String> {
    let summary: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("summary.json: {e}"))?;
    let path = summary.get("module_path").and_then(serde_json::Value::as_str).ok_or("summary.json: no module_path")?;
    let id = match summary.get("id").and_then(serde_json::Value::as_str) {
        Some(id) => format!("{path}.{id}"),
        None => path.to_string(),
    };
    let profiles = summary.get("profiles").and_then(serde_json::Value::as_array).ok_or(format!("{id}: no profiles"))?;
    let callgrind = profiles.iter().find(|p| p.get("tool").and_then(serde_json::Value::as_str) == Some("Callgrind"));
    let ir = callgrind
        .and_then(|p| p.pointer("/data/total/metrics/Ir/values/new"))
        .and_then(serde_json::Value::as_u64)
        .filter(|ir| *ir > 0)
        .ok_or(format!("{id}: no Callgrind instruction count"))?;
    Ok((id, ir))
}

/// The baseline file of `counts`, sorted by id.
pub fn baseline_file(counts: &[(String, u64)]) -> String {
    let mut rows: Vec<String> = counts.iter().map(|(id, ir)| format!("{id},{ir}")).collect();
    rows.sort();
    let header = "# fixture: phasekit-instructions/v1 (VERIFICATION.md §12; written by \
         `cargo xtask gates perf --update`)\n\
         # Callgrind instruction counts of benches/instructions.rs, built with --features phasekit-core/libm\n\
         # columns: bench,instructions\n";
    header.to_string() + &rows.iter().map(|r| format!("{r}\n")).collect::<String>()
}

/// Checks `counts` against `baseline`: each within 5 % above its row, every row run, every count baselined.
pub fn check(baseline: &str, counts: &[(String, u64)]) -> Verdict {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for line in baseline.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        match line.split_once(',').map(|(id, ir)| (id, ir.trim().parse::<u64>())) {
            Some((id, Ok(ir))) if ir > 0 => rows.push((id, ir)),
            _ => errors.push(format!("{BASELINE}: not a `bench,instructions` row: {line}")),
        }
    }
    if rows.is_empty() && errors.is_empty() {
        errors.push(format!("{BASELINE} has no rows"));
    }
    let mut largest = (f64::NEG_INFINITY, String::new());
    let mut faster = Vec::new();
    for (id, ir) in counts {
        let Some(&(_, base)) = rows.iter().find(|(row, _)| row == id) else {
            errors.push(format!("{id}: no baseline row ({ir} instructions)"));
            continue;
        };
        let change = *ir as f64 / base as f64 - 1.0;
        if u128::from(*ir) * 100 > u128::from(base) * (100 + LIMIT_PERCENT) {
            errors.push(format!(
                "{id}: {ir} instructions, {:+.1} % over the baseline's {base} (limit +5 %)",
                100.0 * change
            ));
        } else if u128::from(*ir) * 100 < u128::from(base) * (100 - LIMIT_PERCENT) {
            faster.push(format!("{id} {:+.1} %", 100.0 * change));
        }
        if change > largest.0 {
            largest = (change, id.clone());
        }
    }
    for (id, _) in rows.iter().filter(|(row, _)| !counts.iter().any(|(id, _)| id == row)) {
        errors.push(format!("{id}: in the baseline but not run"));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let note = if faster.is_empty() { String::new() } else { format!("; more than 5 % fewer: {}", faster.join(", ")) };
    Ok(format!(
        "{} benchmarks within +5 % of the baseline, the largest change {:+.1} % ({}){note}",
        counts.len(),
        100.0 * largest.0,
        largest.1
    ))
}

/// Checks a criterion results file (`date,commit,step,bench,fluid,median,unit,...`) against [`TARGETS`]: each matching
/// row's median at or under its target, or exempted by an `exemptions` line `<bench> <fluid|*> issue #N: <reason>`. An
/// exemption that matches no miss fails too, so the list cannot go stale.
pub fn check_targets(results: &str, exemptions: &str) -> Verdict {
    let mut errors = Vec::new();
    let mut exempt = Vec::new();
    for line in exemptions.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let mut fields = line.splitn(3, ' ');
        match (fields.next(), fields.next(), fields.next()) {
            (Some(bench), Some(fluid), Some(why)) if why.starts_with("issue #") && why.contains(": ") => {
                exempt.push((bench, fluid, false));
            }
            _ => errors.push(format!("{EXEMPTIONS}: not `<bench> <fluid|*> issue #N: <reason>`: {line}")),
        }
    }
    let (mut met, mut missed) = (0, 0);
    for line in results.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let cells: Vec<&str> = line.split(',').collect();
        let (Some(bench), Some(fluid), Some(median), Some(unit)) =
            (cells.get(3), cells.get(4), cells.get(5), cells.get(6))
        else {
            errors.push(format!("not a results row: {line}"));
            continue;
        };
        let Some(&(_, _, target, label)) =
            TARGETS.iter().find(|(b, f, ..)| b == bench && f.is_none_or(|f| f == *fluid))
        else {
            continue;
        };
        let Ok(ns) = median.parse::<f64>() else {
            errors.push(format!("{bench} {fluid}: median {median} is not a number"));
            continue;
        };
        if *unit != "ns" {
            errors.push(format!("{bench} {fluid}: unit {unit}, not ns"));
        } else if ns <= target {
            met += 1;
        } else if let Some(e) = exempt.iter_mut().find(|(b, f, _)| b == bench && (*f == "*" || f == fluid)) {
            e.2 = true;
            missed += 1;
        } else {
            errors.push(format!("{bench} {fluid}: {ns:.1} ns over §7's {label} ({target} ns) and not exempted"));
        }
    }
    for (bench, fluid, _) in exempt.iter().filter(|(.., used)| !used) {
        errors.push(format!("{EXEMPTIONS}: {bench} {fluid} matches no miss: remove it"));
    }
    if met + missed == 0 && errors.is_empty() {
        errors.push("no results row has a §7 target".into());
    }
    if errors.is_empty() {
        Ok(format!("{met} medians meet ARCHITECTURE.md §7, {missed} misses re-planned"))
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(path: &str, id: Option<&str>, ir: u64) -> String {
        let total = serde_json::json!({"regressions": [], "metrics": {"Ir": {"change": null, "values": {"new": ir}}}});
        let profile = serde_json::json!({"tool": "Callgrind", "data": {"parts": [], "total": total}});
        let mut summary = serde_json::json!({"module_path": path, "profiles": [profile], "version": "7"});
        if let Some(id) = id {
            summary["id"] = id.into();
        }
        summary.to_string()
    }

    /// A summary's id is its module path and bench id; its count the Callgrind total `Ir`. No Callgrind profile, a
    /// zero count or no module path is an error.
    #[test]
    fn instruction_counts_come_from_gungraun_summaries() {
        let water = summary("instructions::kernel::pt_flash", Some("water"), 123_456);
        assert_eq!(summary_count(&water), Ok(("instructions::kernel::pt_flash.water".into(), 123_456)));
        assert_eq!(summary_count(&summary("i::k::lookup", None, 7)), Ok(("i::k::lookup".into(), 7)));
        assert!(summary_count(&summary("i::k::f", None, 0)).unwrap_err().contains("no Callgrind"));
        assert!(summary_count(&water.replace("Callgrind", "DHAT")).unwrap_err().contains("no Callgrind"));
        assert!(summary_count(r#"{"profiles":[]}"#).unwrap_err().contains("module_path"));
        assert!(summary_count("{").is_err());
    }

    /// The gate: within +5 % passes (faster runs noted), above fails, and a missing row or an unrun benchmark fails
    /// closed; the proposed baseline reads back as a passing one.
    #[test]
    fn counts_more_than_five_percent_over_the_baseline_fail() {
        let counts = vec![("k::a.water".to_string(), 1_050), ("k::b".to_string(), 900)];
        let baseline = baseline_file(&[("k::b".into(), 1_000), ("k::a.water".into(), 1_000)]);
        assert!(baseline.lines().filter(|l| !l.starts_with('#')).eq(["k::a.water,1000", "k::b,1000"]));
        let ok = check(&baseline, &counts).unwrap();
        assert!(
            ok.contains("2 benchmarks") && ok.contains("+5.0 % (k::a.water)") && ok.contains("k::b -10.0 %"),
            "{ok}"
        );
        let over = check(&baseline, &[("k::a.water".into(), 1_051), ("k::b".into(), 1_000)]).unwrap_err();
        assert!(over.len() == 1 && over[0].contains("+5.1 % over"), "{over:?}");
        let unbaselined = check(&baseline, &[("k::a.water".into(), 1_000), ("k::b".into(), 1_000), ("k::c".into(), 1)]);
        assert!(unbaselined.unwrap_err()[0].contains("k::c: no baseline row"));
        assert!(
            check(&baseline, &[("k::b".into(), 1_000)]).unwrap_err()[0]
                .contains("k::a.water: in the baseline but not run")
        );
        assert!(check("# only a header\n", &counts).is_err());
        assert!(check("k::a.water,x\n", &counts).unwrap_err()[0].contains("not a `bench,instructions` row"));
        assert!(check(&baseline_file(&counts), &counts).is_ok());
        // Exactly 5 % fewer is not noted; only the faster benchmark is; the first of equal changes is the largest.
        let note = check(&baseline, &[("k::a.water".into(), 950), ("k::b".into(), 900)]).unwrap();
        assert!(note.ends_with("; more than 5 % fewer: k::b -10.0 %"), "{note}");
        let tie = check(&baseline, &[("k::a.water".into(), 1_000), ("k::b".into(), 1_000)]).unwrap();
        assert!(tie.ends_with("+0.0 % (k::a.water)"), "{tie}");
        assert!(check("k::a.water,0\n", &counts).unwrap_err()[0].contains("not a `bench,instructions` row"));
    }

    /// §7 targets on a results file: a median at or under its target passes, a miss fails unless exempted with an
    /// issue, and an exemption that matches no miss fails. The α^r target applies to the 16-20-term fluid only: Water's
    /// α^r row is not counted.
    #[test]
    fn medians_over_their_section_7_target_need_a_re_plan_issue() {
        let row = |bench: &str, fluid: &str, ns: f64| {
            format!("2026-10-13,abc,M9.6,{bench},{fluid},{ns},ns,cpu,os,rustc,gov\n")
        };
        let results = [
            row("alphar_order2/residual", "n-Propane", 290.0),
            row("alphar_order2/residual", "Water", 1_000.0),
            row("pt_flash/pt_h", "Water", 7_000.0),
            row("pt_flash/pt_h", "R134a", 2_000.0),
            row("dt_flash/single", "Water", 9_999.0),
        ]
        .concat();
        let exemptions = "# misses with an issue\npt_flash/pt_h Water issue #140: seven Newton steps\n";
        assert_eq!(
            check_targets(&results, exemptions),
            Ok("2 medians meet ARCHITECTURE.md §7, 1 misses re-planned".into())
        );
        let unexempted = check_targets(&results, "").unwrap_err();
        assert!(
            unexempted.len() == 1 && unexempted[0].contains("pt_flash/pt_h Water: 7000.0 ns over"),
            "{unexempted:?}"
        );
        let stale = check_targets(&results, &format!("{exemptions}qt_superancillary/flash * issue #9: gone\n"));
        assert!(stale.unwrap_err()[0].contains("matches no miss"));
        for malformed in ["pt_flash/pt_h Water slow", "pt_flash/pt_h Water issue #140", "pt_flash/pt_h Water slow: why"]
        {
            assert!(check_targets(&results, &format!("{malformed}\n")).unwrap_err()[0].contains("not `<bench>"));
        }
        // An exemption for another bench or another fluid does not cover a miss (and is stale itself).
        for other in ["ph_flash/ph_t Water issue #1: x", "pt_flash/pt_h Methane issue #1: x"] {
            let errors = check_targets(&results, &format!("{other}\n")).unwrap_err();
            assert!(errors.len() == 2 && errors[0].contains("not exempted"), "{other}: {errors:?}");
        }
        // Comment lines are not rows; a file whose only target rows are exempted misses passes.
        let only_a_miss = format!("# fixture: results\n{}", row("pt_flash/pt_h", "Water", 7_000.0));
        assert_eq!(
            check_targets(&only_a_miss, exemptions),
            Ok("0 medians meet ARCHITECTURE.md §7, 1 misses re-planned".into())
        );
        assert!(check_targets("", "").is_err());
    }

    /// Every target is the value ARCHITECTURE.md §7's table states on that row, so the table stays the one home.
    #[test]
    fn targets_match_the_architecture_table() {
        let architecture = include_str!("../../../../docs/ARCHITECTURE.md");
        for (bench, _, ns, label) in TARGETS {
            let (row, target) = label.split_once(" | ").unwrap();
            let line = architecture.lines().find(|l| l.starts_with(&format!("| {row} |"))).unwrap_or_default();
            assert!(line.contains(&format!("| {target}")), "{bench}: §7 has no row `{row}` with `{target}`: {line}");
            let micro = |s: &str| s.trim_start_matches("≤ ").trim_end_matches(" µs").parse::<f64>().map(|v| v * 1e3);
            let stated: Vec<f64> = target
                .split(" / ")
                .filter_map(|t| {
                    micro(t).ok().or_else(|| t.trim_start_matches("≤ ").trim_end_matches(" ns").parse().ok())
                })
                .collect();
            assert!(stated.contains(&ns), "{bench}: {ns} ns is not among §7's {target}");
        }
    }
}
