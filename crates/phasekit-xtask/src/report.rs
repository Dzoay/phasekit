//! `cargo xtask nightly-report <dir>...`: the nightly `oracle` job's summary in Markdown on stdout (PLAN.md M20.4,
//! user decision PL3). Each `<dir>` is a repo-relative oracle fixture set, a tree of `<kind>/<Fluid>.csv` and
//! `all/<kind>.csv`. The summary lists, per fluid and kind of each set, the rows, the oracle's failed calls (`err:`
//! rows) and the `nan` cells of its other rows. It gives each fluid's arbiter status (`phasekit_verify::ARBITERS`),
//! oracle only where no table arbitrates, so such fluids stay visible, and the ignored tests (`gates ignores`). What the
//! checks assert of the cells (asserted, exempt per DIV id, oracle-gap rows, headroom) is M20.4a's. A file that does
//! not parse is listed, and the report is still written: the job runs it after a failed step too.

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

use phasekit_verify::{Arbiter, ArbiterStatus, Cell, ColumnRole, Fixture};

use crate::gates::ignores;
use crate::repo::Repo;

pub fn main(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: cargo xtask nightly-report <fixture dir>...");
        return ExitCode::FAILURE;
    }
    let repo = Repo::locate();
    let sets: Result<Vec<Files<'_>>, String> =
        args.iter().map(|dir| Ok((dir.as_str(), repo.files(dir, ".csv", true)?))).collect();
    let sources = repo.files("crates", ".rs", true);
    match (sets, sources) {
        (Ok(sets), Ok(sources)) => {
            let ignored = ignores::check(&sources, phasekit_verify::DIVERGENCES).unwrap_or_else(|errors| errors);
            println!("{}", report(&sets, &ignored, phasekit_verify::ARBITERS));
            ExitCode::SUCCESS
        }
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("nightly-report: {e}");
            ExitCode::FAILURE
        }
    }
}

/// A fixture set: its name and its (path, text) files.
type Files<'a> = (&'a str, Vec<(String, String)>);

/// The summary of `sets` with the `ignored` tests and the `arbiters`.
pub(crate) fn report(sets: &[Files<'_>], ignored: &[String], arbiters: &[Arbiter]) -> String {
    let read: Vec<(&str, Set)> = sets.iter().map(|(name, files)| (*name, Set::of(files))).collect();
    let mut out = String::new();
    for (name, set) in &read {
        out.push_str(&set.table(name));
    }
    let fluids: BTreeSet<&str> = read.iter().flat_map(|(_, set)| set.tallies.keys().map(|(f, _)| f.as_str())).collect();
    out.push_str(&arbiter_table(&fluids, arbiters));
    out.push_str(&format!("\n### Ignored tests ({})\n\n", ignored.len()));
    for line in ignored {
        out.push_str(&format!("- {line}\n"));
    }
    out
}

/// One fluid's rows of one kind in one set: how many, how many the oracle failed (an `err:` status), and the `nan`
/// output cells of the others.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Tally {
    rows: usize,
    failed: usize,
    nan: usize,
}

/// One set's tallies by (fluid, kind), and the files that did not parse, with why.
#[derive(Debug, Default)]
struct Set {
    tallies: BTreeMap<(String, String), Tally>,
    unreadable: Vec<String>,
}

impl Set {
    /// Every one of `files`, read as a fixture.
    fn of(files: &[(String, String)]) -> Set {
        let mut set = Set::default();
        for (path, text) in files {
            match Fixture::parse(path, text) {
                Ok(fixture) => set.add(&fixture, stem(path)),
                Err(e) => set.unreadable.push(format!("{path}:{}: {}", e.line, e.message)),
            }
        }
        set
    }

    /// This set's table, under its `name`, and the files that did not parse.
    fn table(&self, name: &str) -> String {
        let mut out = format!("### Oracle fixtures in `{name}`, by fluid and kind\n\n");
        out.push_str("| Fluid | Kind | Rows | Failed calls | `nan` cells |\n|---|---|---|---|---|\n");
        for ((fluid, kind), t) in &self.tallies {
            out.push_str(&format!("| {fluid} | {kind} | {} | {} | {} |\n", t.rows, t.failed, t.nan));
        }
        for line in &self.unreadable {
            out.push_str(&format!("\nUnreadable: {line}\n"));
        }
        out.push('\n');
        out
    }

    /// `fixture`'s rows, each under its fluid: its `fluid` or `mixture` column's (a long-format file), else the first
    /// word of the header's `fluid`, else the file's `stem`.
    fn add(&mut self, fixture: &Fixture<'_>, stem: &str) {
        let column = |name: &str| fixture.columns().iter().position(|c| *c == name);
        let (fluid, status) = (column("fluid").or_else(|| column("mixture")), column("status"));
        let named = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap_or(stem);
        for row in fixture.rows() {
            let label = |i: Option<usize>| i.and_then(|i| row.printed.get(i)).copied();
            let tally = self.tallies.entry((label(fluid).unwrap_or(named).to_string(), fixture.kind().to_string()));
            let tally = tally.or_default();
            tally.rows += 1;
            if label(status).is_some_and(|s| s.starts_with("err:")) {
                tally.failed += 1;
            } else {
                tally.nan += nan_outputs(fixture.roles(), &row.cells);
            }
        }
    }
}

/// The output cells of a row that are `nan`.
fn nan_outputs(roles: &[ColumnRole], cells: &[Cell<'_>]) -> usize {
    let nan = |(role, cell): &(&ColumnRole, &Cell<'_>)| {
        matches!(role, ColumnRole::Output(_)) && matches!(cell, Cell::Num(v) if v.is_nan())
    };
    roles.iter().zip(cells).filter(nan).count()
}

/// The file name of `path` without its `.csv`.
fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".csv").unwrap_or(name)
}

/// Each of `fluids` with arbiter records: their parts and statuses, and whether one arbitrates (a self-consistent
/// table, VERIFICATION.md §4.3), else oracle only; then the fluids with no record, which only the oracle checks.
fn arbiter_table(fluids: &BTreeSet<&str>, arbiters: &[Arbiter]) -> String {
    let mut out = String::from("### Arbiters\n\n| Fluid | Arbiters | Status |\n|---|---|---|\n");
    let mut unrecorded = Vec::new();
    for &fluid in fluids {
        let own: Vec<&Arbiter> = arbiters.iter().filter(|a| a.fluid == fluid).collect();
        if own.is_empty() {
            unrecorded.push(fluid);
            continue;
        }
        let records: Vec<String> = own.iter().map(|a| format!("{:?} {:?}", a.part, a.status)).collect();
        let arbitrated = own.iter().any(|a| a.status == ArbiterStatus::SelfConsistent);
        let status = if arbitrated { "arbitrated" } else { "oracle only" };
        out.push_str(&format!("| {fluid} | {} | {status} |\n", records.join("; ")));
    }
    out.push_str(&format!("\nNo arbiter record, oracle only ({}): {}\n", unrecorded.len(), unrecorded.join(", ")));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixture of `kind` from paper `test/1` (no oracle header needed) with `columns`, its roles and `rows`.
    fn fixture(kind: &str, fluid: Option<&str>, columns: &str, tol: &str, rows: &[&str]) -> String {
        let fluid = fluid.map(|f| format!("# fluid: {f}\n")).unwrap_or_default();
        let body = rows.join("\n");
        format!("# fixture: {kind}/v1\n{fluid}# source: paper:test/1\n# columns: {columns}\n# tol: {tol}\n{body}\n")
    }

    /// PLAN.md M20.4: per fluid and kind of each set, its rows, failed calls and the `nan` cells of the rows the
    /// oracle answered (a long-format file split by its `fluid` column, a file that names no fluid under its own
    /// name); each fluid's arbiters, and whether one
    /// arbitrates (Water's IAPWS-95 tables do, Neon's EOS paper prints none, n-Propane has no record); the ignored tests;
    /// and a file that does not parse, by name.
    #[test]
    fn the_report_lists_rows_failed_calls_nan_cells_arbiters_and_ignores() {
        let air = fixture(
            "eos",
            Some("Air"),
            "T,status,p,h",
            "in,label,prop,prop",
            &["300.0,ok,1.0,nan", "310.0,err:solver,nan,nan", "320.0,ok,nan,nan"],
        );
        let all = fixture(
            "eos",
            None,
            "fluid,T,status,p",
            "label,in,label,prop",
            &["Water,300.0,ok,1.0", "Neon,40.0,ok,nan", "n-Propane,300.0,err:solver,nan", "Water,310.0,ok,2.0"],
        );
        let neon = fixture("sat", None, "T,status,p", "in,label,prop", &["40.0,ok,1.0"]);
        let full = vec![
            ("fixtures-full/eos/Air.csv".to_string(), air),
            ("fixtures-full/sat/Neon.csv".to_string(), neon),
            ("fixtures-full/all/eos.csv".to_string(), all),
            ("fixtures-full/all/broken.csv".to_string(), "not a fixture\n".to_string()),
        ];
        let ignored = ["crates/phasekit-verify/tests/sweeps.rs:40: nightly: reads the full term set".to_string()];
        let report = report(&[("fixtures-full", full)], &ignored, phasekit_verify::ARBITERS);
        let rows = [
            "### Oracle fixtures in `fixtures-full`, by fluid and kind",
            "| Air | eos | 3 | 1 | 3 |",
            "| Neon | eos | 1 | 0 | 1 |",
            "| Neon | sat | 1 | 0 | 0 |",
            "| Water | eos | 2 | 0 | 0 |",
            "| n-Propane | eos | 1 | 1 | 0 |",
        ];
        for row in rows {
            assert!(report.contains(row), "{row} in\n{report}");
        }
        assert!(report.contains("| Water | AlphaR SelfConsistent; "), "{report}");
        assert!(report.contains("| Neon | AlphaR Unpublished | oracle only |"), "{report}");
        assert!(report.contains("No arbiter record, oracle only (1): n-Propane"), "{report}");
        assert!(report.contains("- crates/phasekit-verify/tests/sweeps.rs:40: nightly: reads the full term set"));
        assert!(report.contains("fixtures-full/all/broken.csv"), "{report}");
    }
}
