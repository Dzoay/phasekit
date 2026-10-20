//! `cargo xtask nightly-report [--tallies <dir>] <dir>...`: the nightly `oracle` job's summary in Markdown on stdout
//! (PLAN.md M20.4, M20.4a; user decision PL3). Each `<dir>` is a repo-relative oracle fixture set, a tree of
//! `<kind>/<Fluid>.csv` and `all/<kind>.csv`. The summary lists, per fluid and kind of each set, the rows, the oracle's
//! failed calls (`err:` rows) and the `nan` cells of its other rows. From the `*.tsv` files in the tallies `<dir>`,
//! which the nightly sweeps write (`phasekit_verify::Tallies`), it lists what the oracle checks asserted of the cells,
//! per fluid and kind and per kind and column: the cells asserted and failed, the exempt ones by DIV id, the oracle-gap
//! rows and the headroom. It gives each fluid's arbiter status (`phasekit_verify::ARBITERS`), oracle only where no table
//! arbitrates, so such fluids stay visible, and the ignored tests (`gates ignores`). A file that does not parse is
//! listed, and the report is still written: the job runs it after a failed step too.

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

use phasekit_verify::tallies::Column;
use phasekit_verify::{Arbiter, ArbiterStatus, Cell, ColumnRole, Fixture, Tallies};

use crate::gates::ignores;
use crate::repo::Repo;

pub fn main(args: &[String]) -> ExitCode {
    let (tallies, dirs) = match args {
        [flag, dir, rest @ ..] if flag == "--tallies" => (Some(dir.as_str()), rest),
        _ => (None, args),
    };
    if dirs.is_empty() {
        eprintln!("usage: cargo xtask nightly-report [--tallies <dir>] <fixture dir>...");
        return ExitCode::FAILURE;
    }
    let repo = Repo::locate();
    let sets: Result<Vec<Files<'_>>, String> =
        dirs.iter().map(|dir| Ok((dir.as_str(), repo.files(dir, ".csv", true)?))).collect();
    let sources = repo.files("crates", ".rs", true);
    let tallies = tallies.map_or(Ok(Vec::new()), |dir| repo.files(dir, ".tsv", true));
    match (sets, sources, tallies) {
        (Ok(sets), Ok(sources), Ok(tallies)) => {
            let ignored = ignores::check(&sources, phasekit_verify::DIVERGENCES).unwrap_or_else(|errors| errors);
            println!("{}", report(&sets, &tallies, &ignored, phasekit_verify::ARBITERS));
            ExitCode::SUCCESS
        }
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
            eprintln!("nightly-report: {e}");
            ExitCode::FAILURE
        }
    }
}

/// A fixture set: its name and its (path, text) files.
type Files<'a> = (&'a str, Vec<(String, String)>);

/// The summary of `sets` and the sweeps' `tallies` files, with the `ignored` tests and the `arbiters`.
pub(crate) fn report(
    sets: &[Files<'_>],
    tallies: &[(String, String)],
    ignored: &[String],
    arbiters: &[Arbiter],
) -> String {
    let read: Vec<(&str, Set)> = sets.iter().map(|(name, files)| (*name, Set::of(files))).collect();
    let mut out = String::new();
    for (name, set) in &read {
        out.push_str(&set.table(name));
    }
    let fluids: BTreeSet<&str> = read.iter().flat_map(|(_, set)| set.tallies.keys().map(|(f, _)| f.as_str())).collect();
    out.push_str(&asserted_tables(tallies));
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

    /// `fixture`'s rows, each under its fluid (`Fixture::fluid`), else the file's `stem`.
    fn add(&mut self, fixture: &Fixture<'_>, stem: &str) {
        for (i, row) in fixture.rows().iter().enumerate() {
            let fluid = fixture.fluid(i).unwrap_or(stem);
            let tally = self.tallies.entry((fluid.to_string(), fixture.kind().to_string())).or_default();
            tally.rows += 1;
            if fixture.printed(i, "status").is_some_and(|s| s.starts_with("err:")) {
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

/// What the sweeps' checks asserted, from their tallies `files` (PLAN.md M20.4a): [`by_fluid_and_kind`] and
/// [`by_kind_and_column`], then the files that are not tallies.
fn asserted_tables(files: &[(String, String)]) -> String {
    let mut tallies = Tallies::default();
    let unreadable: Vec<String> =
        files.iter().filter_map(|(path, text)| tallies.read(text).err().map(|e| format!("{path}: {e}"))).collect();
    let mut out = by_fluid_and_kind(&tallies);
    out.push_str(&by_kind_and_column(&tallies));
    for line in &unreadable {
        out.push_str(&format!("\nUnreadable tallies: {line}\n"));
    }
    out.push('\n');
    out
}

/// Per fluid and kind, the cells asserted and failed, the exempt ones by DIV id, the oracle-gap rows and the headroom
/// of the column with the most.
fn by_fluid_and_kind(tallies: &Tallies) -> String {
    let mut out = String::from("### What the nightly sweeps assert, by fluid and kind\n\n");
    out.push_str("| Fluid | Kind | Cells asserted | Failed | Exempt | Oracle-gap rows | Headroom |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for (fluid, kind, group) in tallies.groups() {
        let mut all = Asserted::default();
        for (name, column) in &group.columns {
            all.add(name, column);
        }
        let headroom = all.headroom.map_or("—".to_string(), |(h, column)| format!("{h:.3} ({column})"));
        let (asserted, failed, exempt, gaps) = (all.asserted, all.failed, all.exempt(), group.gaps);
        out.push_str(&format!("| {fluid} | {kind} | {asserted} | {failed} | {exempt} | {gaps} | {headroom} |\n"));
    }
    out
}

/// Per kind and column over every fluid, the cells asserted and failed, the exempt ones by DIV id, and the most
/// headroom and its fluid.
fn by_kind_and_column(tallies: &Tallies) -> String {
    let mut by_column: BTreeMap<(&str, &str), Asserted<'_>> = BTreeMap::new();
    for (fluid, kind, group) in tallies.groups() {
        for (name, column) in &group.columns {
            by_column.entry((kind, name)).or_default().add(fluid, column);
        }
    }
    let mut out = String::from("\n### Headroom by kind and column\n\n");
    out.push_str("| Kind | Column | Cells asserted | Failed | Exempt | Headroom | Fluid |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for ((kind, name), all) in &by_column {
        let (headroom, fluid) =
            all.headroom.map_or(("—".to_string(), "—"), |(h, fluid)| (format!("{h:.3}"), fluid));
        let (asserted, failed, exempt) = (all.asserted, all.failed, all.exempt());
        out.push_str(&format!("| {kind} | {name} | {asserted} | {failed} | {exempt} | {headroom} | {fluid} |\n"));
    }
    out
}

/// Columns' tallies added up: the cells asserted and failed, the exempt ones by DIV id, and the largest headroom of a
/// column with a cell that held, with the name it came under (a column's, or a fluid's).
#[derive(Debug, Default)]
struct Asserted<'a> {
    asserted: usize,
    failed: usize,
    exempt: BTreeMap<&'a str, usize>,
    headroom: Option<(f64, &'a str)>,
}

impl<'a> Asserted<'a> {
    /// `column`'s tallies, under `name`.
    fn add(&mut self, name: &'a str, column: &'a Column) {
        (self.asserted, self.failed) = (self.asserted + column.asserted, self.failed + column.failed);
        for (id, n) in &column.exempt {
            *self.exempt.entry(id).or_default() += n;
        }
        let held = column.asserted > column.failed;
        if held && self.headroom.is_none_or(|(h, _)| column.headroom > h) {
            self.headroom = Some((column.headroom, name));
        }
    }

    /// The exempt cells as `DIV-0019: 40; DIV-0020: 6400`, or `—`.
    fn exempt(&self) -> String {
        let ids: Vec<String> = self.exempt.iter().map(|(id, n)| format!("{id}: {n}")).collect();
        if ids.is_empty() { "—".to_string() } else { ids.join("; ") }
    }
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
        let report = report(&[("fixtures-full", full)], &[], &ignored, phasekit_verify::ARBITERS);
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

    /// PLAN.md M20.4a: what the sweeps' checks asserted, from the tallies files they write: per fluid and kind, the
    /// cells asserted and failed, the exempt ones by DIV id, the oracle-gap rows and the headroom of the column with the
    /// most (the first of a tie); per kind and column over every fluid, the same and the fluid with the most headroom; a
    /// column only exempt has none; two files add up; a file that is not tallies is named, and the report still written.
    #[test]
    fn the_report_adds_what_the_checks_assert() {
        let (water, ses36, r134a) = (("Water", "eos"), ("SES36", "flash"), ("R134a", "props"));
        let mut eos = Tallies::default();
        eos.cell(water, "p", Some(0.25));
        eos.cell(water, "p", None);
        eos.cell(water, "cp", Some(0.5));
        eos.cell(("Air", "eos"), "cp", Some(0.75));
        eos.cell(("Air", "eos"), "cv", Some(0.75));
        let mut flash = Tallies::default();
        flash.cell(ses36, "h", Some(0.125));
        flash.cell(ses36, "h", Some(0.0));
        flash.exempt(ses36, "p", "DIV-0019");
        flash.exempt(ses36, "p", "DIV-0019");
        flash.exempt(ses36, "h", "DIV-0020");
        let mut props = Tallies::default();
        props.cell(r134a, "viscosity", Some(0.0));
        props.gap(r134a);
        props.gap(r134a);
        props.gap(r134a);
        let tallies = [
            ("fixtures-full/tallies/eos.tsv".to_string(), eos.to_text()),
            ("fixtures-full/tallies/flash.tsv".to_string(), flash.to_text()),
            ("fixtures-full/tallies/props.tsv".to_string(), props.to_text()),
            ("fixtures-full/tallies/again.tsv".to_string(), props.to_text()),
            ("fixtures-full/tallies/broken.tsv".to_string(), "cell\tWater\n".to_string()),
        ];
        let report = report(&[], &tallies, &[], phasekit_verify::ARBITERS);
        let rows = [
            "### What the nightly sweeps assert, by fluid and kind",
            "| Fluid | Kind | Cells asserted | Failed | Exempt | Oracle-gap rows | Headroom |",
            "| Air | eos | 2 | 0 | — | 0 | 0.750 (cp) |",
            "| R134a | props | 2 | 0 | — | 6 | 0.000 (viscosity) |",
            "| SES36 | flash | 2 | 0 | DIV-0019: 2; DIV-0020: 1 | 0 | 0.125 (h) |",
            "| Water | eos | 3 | 1 | — | 0 | 0.500 (cp) |",
            "### Headroom by kind and column",
            "| Kind | Column | Cells asserted | Failed | Exempt | Headroom | Fluid |",
            "| eos | cp | 2 | 0 | — | 0.750 | Air |",
            "| eos | cv | 1 | 0 | — | 0.750 | Air |",
            "| eos | p | 2 | 1 | — | 0.250 | Water |",
            "| flash | h | 2 | 0 | DIV-0020: 1 | 0.125 | SES36 |",
            "| flash | p | 0 | 0 | DIV-0019: 2 | — | — |",
            "| props | viscosity | 2 | 0 | — | 0.000 | R134a |",
            "Unreadable tallies: fixtures-full/tallies/broken.tsv: line 1: not `# tallies/v1`",
        ];
        for row in rows {
            assert!(report.contains(row), "{row} in\n{report}");
        }
    }
}
