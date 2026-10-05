//! Printed check tables (VERIFICATION.md §4.2): each transcribed twice by sessions that could not see each other's
//! output, parsed with its printed precision, and cited down to the page and the file read.

use phasekit_verify::{
    Cell, ColumnRole, DIVERGENCES, Fixture, ORACLE_LOCK, OracleLock, ToleranceClass, fixture, from_printed,
};

/// A committed fixture: (path, text).
type Text = (&'static str, &'static str);

/// Every transcribed table and its independent second transcription.
const PAPERS: &[(Text, Text)] = &[
    (
        fixture!("paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.csv"),
        fixture!("paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.check.csv"),
    ),
    (fixture!("paper/Helium/OrtizVega-JPCRD-2019.3.csv"), fixture!("paper/Helium/OrtizVega-JPCRD-2019.3.check.csv")),
    (fixture!("paper/Helium/OrtizVega-JPCRD-2019.4.csv"), fixture!("paper/Helium/OrtizVega-JPCRD-2019.4.check.csv")),
    (fixture!("paper/Water/IAPWS-R6-95-2018.6.csv"), fixture!("paper/Water/IAPWS-R6-95-2018.6.check.csv")),
    (fixture!("paper/Water/IAPWS-R6-95-2018.7.csv"), fixture!("paper/Water/IAPWS-R6-95-2018.7.check.csv")),
    (fixture!("paper/Water/IAPWS-R6-95-2018.8.csv"), fixture!("paper/Water/IAPWS-R6-95-2018.8.check.csv")),
    (
        fixture!("paper/R227EA/Lemmon-JCED-2016-365227.7.csv"),
        fixture!("paper/R227EA/Lemmon-JCED-2016-365227.7.check.csv"),
    ),
    (
        fixture!("paper/R365MFC/Lemmon-JCED-2016-365227.7.csv"),
        fixture!("paper/R365MFC/Lemmon-JCED-2016-365227.7.check.csv"),
    ),
    (fixture!("paper/R115/Lemmon-JCED-2016-365227.7.csv"), fixture!("paper/R115/Lemmon-JCED-2016-365227.7.check.csv")),
    (
        fixture!("paper/R13I1/Lemmon-JCED-2016-365227.7.csv"),
        fixture!("paper/R13I1/Lemmon-JCED-2016-365227.7.check.csv"),
    ),
    (fixture!("paper/R1234yf/Lemmon-IJT-2022.7.csv"), fixture!("paper/R1234yf/Lemmon-IJT-2022.7.check.csv")),
    (
        fixture!("paper/R1224YDZ/Akasaka-IJT-2023-R1224ydZ.7.csv"),
        fixture!("paper/R1224YDZ/Akasaka-IJT-2023-R1224ydZ.7.check.csv"),
    ),
    (
        fixture!("paper/R1132(E)/Akasaka-IJT-2024-R1132E.6.csv"),
        fixture!("paper/R1132(E)/Akasaka-IJT-2024-R1132E.6.check.csv"),
    ),
    (
        fixture!("paper/Tetrahydrofuran/Fiedler-IJT-2023-THF.11.csv"),
        fixture!("paper/Tetrahydrofuran/Fiedler-IJT-2023-THF.11.check.csv"),
    ),
    (
        fixture!("paper/PropyleneGlycol/Eisenbach-JPCRD-2021.8.csv"),
        fixture!("paper/PropyleneGlycol/Eisenbach-JPCRD-2021.8.check.csv"),
    ),
    (
        fixture!("paper/VinylChloride/Thol-IJT-2022-VinylChloride.5.csv"),
        fixture!("paper/VinylChloride/Thol-IJT-2022-VinylChloride.5.check.csv"),
    ),
    (fixture!("paper/R1123/Akasaka-IJR-2020-R1123.8.csv"), fixture!("paper/R1123/Akasaka-IJR-2020-R1123.8.check.csv")),
    (
        fixture!("paper/n-Perfluorobutane/Gao-2022-CxFy.14.csv"),
        fixture!("paper/n-Perfluorobutane/Gao-2022-CxFy.14.check.csv"),
    ),
    (
        fixture!("paper/n-Perfluoropentane/Gao-2022-CxFy.14.csv"),
        fixture!("paper/n-Perfluoropentane/Gao-2022-CxFy.14.check.csv"),
    ),
    (
        fixture!("paper/n-Perfluorohexane/Gao-2022-CxFy.14.csv"),
        fixture!("paper/n-Perfluorohexane/Gao-2022-CxFy.14.check.csv"),
    ),
    (
        fixture!("paper/R1233zd(E)/Akasaka-JPCRD-2022-R1233zdE.IX.csv"),
        fixture!("paper/R1233zd(E)/Akasaka-JPCRD-2022-R1233zdE.IX.check.csv"),
    ),
    (
        fixture!("paper/R1130(E)/Huber-IJT-2025-R1130E.4.csv"),
        fixture!("paper/R1130(E)/Huber-IJT-2025-R1130E.4.check.csv"),
    ),
    (
        fixture!("paper/R1243zf/Akasaka-IJT-2025-R1243zf.6.csv"),
        fixture!("paper/R1243zf/Akasaka-IJT-2025-R1243zf.6.check.csv"),
    ),
];

const REUSE: &str = include_str!("../../../REUSE.toml");

fn files() -> impl Iterator<Item = (&'static str, &'static str)> {
    PAPERS.iter().flat_map(|(first, second)| [*first, *second])
}

/// VERIFICATION.md §4.2 step 4: the two transcriptions agree string for string, every line.
#[test]
fn paper_tables_double_entry_agree() {
    for ((path, first), (check_path, second)) in PAPERS {
        assert_eq!(*check_path, path.replace(".csv", ".check.csv"));
        let differ: Vec<String> = first
            .lines()
            .zip(second.lines())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| format!("line {}: `{a}` vs `{b}`", i + 1))
            .collect();
        assert!(differ.is_empty() && first.lines().count() == second.lines().count(), "{path}: {differ:#?}");
    }
}

/// Map 13 §3: every printed value parses and keeps the precision it was printed with, which is its tolerance.
#[test]
fn printed_strings_parse() {
    for (path, text) in files() {
        let table = Fixture::parse(path, text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(table.kind(), "paper");
        for (column, role) in table.columns().iter().zip(table.roles()) {
            for row in 0..table.rows().len() {
                let printed = table.printed(row, column).unwrap_or_default();
                assert_eq!(table.value(row, column), printed.parse().ok(), "{path} {column} row {row}");
                // A blank is a value the source does not give (VERIFICATION.md §4.2).
                if *role == ColumnRole::Output(ToleranceClass::Paper) && !printed.is_empty() {
                    assert!(from_printed(printed).is_some(), "{path} {column} row {row}: `{printed}`");
                }
            }
        }
    }
}

/// VERIFICATION.md §4.2 steps 1, 3, 4 and §3.7: the header cites the table, the source read and its sha256 (a PDF
/// page; a PMC author manuscript's JATS XML; or CoolProp v8.0.0's test source, which a re-check on the paper's page
/// adds a page and PDF to), the erratum check, names both transcribers, and the file has the paper annotation in
/// REUSE.toml.
#[test]
fn every_paper_file_has_a_citation_and_reuse_annotation() {
    assert!(REUSE.contains(r#"path = "crates/phasekit-verify/fixtures/paper/**""#));
    let lock = OracleLock::parse(ORACLE_LOCK).unwrap_or_else(|e| panic!("{e}"));
    let oracle_git = lock.get("git").unwrap_or_default();
    assert_eq!(oracle_git.len(), 40, "oracle.lock pins CoolProp's commit");
    for (path, text) in files() {
        let table = Fixture::parse(path, text).unwrap_or_else(|e| panic!("{e}"));
        let citation = table.header("citation").unwrap_or_default();
        let field = |key: &str| citation.split(' ').find_map(|f| f.strip_prefix(key));
        assert!(field("doi=").or(field("report=")).is_some_and(|s| !s.is_empty()), "{path}: doi= or report=");
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let hex = |key: &str| field(key).is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
        let on_page = field("page=").is_some_and(digits) && hex("pdf_sha256=");
        let located = match (field("manuscript="), field("tests=")) {
            (Some(id), _) => {
                let pmc = id.strip_prefix("PMC").and_then(|v| v.split_once('.'));
                pmc.is_some_and(|(n, version)| digits(n) && digits(version)) && hex("xml_sha256=")
            }
            (None, Some(tests)) => {
                let at = tests.strip_prefix("src/Tests/CoolProp-Tests.cpp:").and_then(|t| t.split_once('@'));
                let pinned = at.is_some_and(|(lines, commit)| {
                    lines.split('-').all(digits) && commit.len() == 8 && oracle_git.starts_with(commit)
                });
                pinned && hex("cpp_sha256=") && (field("page=").is_none() || on_page)
            }
            (None, None) => on_page,
        };
        let sources = "page= and pdf_sha256=, manuscript= and xml_sha256=, or tests= and cpp_sha256=";
        assert!(field("table=").is_some() && located, "{path}: table= and the source read: {sources}");
        let date = field("erratum-checked=").unwrap_or_default();
        assert!(
            date.len() == 10 && date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-',
            "{path}: erratum-checked"
        );
        assert!(table.header("transcribed").is_some_and(|t| t.contains("; checked: ")), "{path}: both transcribers");
    }
}

/// PLAN.md M1.12: a row's note may cite a divergence (R1224YDZ p carries DIV-0014); every id cited is registered for
/// the file's fluid.
#[test]
fn row_notes_cite_registered_divergences() {
    let mut cited = Vec::new();
    for (path, text) in files() {
        let table = Fixture::parse(path, text).unwrap_or_else(|e| panic!("{e}"));
        let fluid = path.split('/').nth(1).unwrap_or_default();
        for row in table.rows() {
            let notes = row.cells.iter().filter_map(|cell| match cell {
                Cell::Text(text) => Some(*text),
                _ => None,
            });
            for id in notes.flat_map(|note| note.split(' ')).filter(|word| word.starts_with("DIV-")) {
                let entry = DIVERGENCES.iter().find(|d| d.id == id);
                assert!(
                    entry.is_some_and(|d| d.fluids.contains(&fluid)),
                    "{path}:{}: {id} not registered for {fluid}",
                    row.line
                );
                cited.push((fluid, id));
            }
        }
    }
    assert!(cited.contains(&("R1224YDZ", "DIV-0014")), "{cited:?}");
}
