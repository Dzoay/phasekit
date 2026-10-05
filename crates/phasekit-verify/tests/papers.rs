//! Printed check tables (VERIFICATION.md §4.2): each transcribed twice by sessions that could not see each other's
//! output, parsed with its printed precision, and cited down to the page and the file read.

use phasekit_verify::{ColumnRole, Fixture, ToleranceClass, fixture, from_printed};

/// A committed fixture: (path, text).
type Text = (&'static str, &'static str);

/// Every transcribed table and its independent second transcription.
const PAPERS: &[(Text, Text)] = &[
    (
        fixture!("paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.csv"),
        fixture!("paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.check.csv"),
    ),
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
                if *role == ColumnRole::Output(ToleranceClass::Paper) {
                    assert!(from_printed(printed).is_some(), "{path} {column} row {row}: `{printed}`");
                }
            }
        }
    }
}

/// VERIFICATION.md §4.2 steps 1, 4 and §3.7: the header cites the table, the page (or, for a PMC author manuscript
/// read from its JATS XML, the PMC id and version), the erratum check and the sha256 of the file read, names both
/// transcribers, and the file has the paper annotation in REUSE.toml.
#[test]
fn every_paper_file_has_a_citation_and_reuse_annotation() {
    assert!(REUSE.contains(r#"path = "crates/phasekit-verify/fixtures/paper/**""#));
    for (path, text) in files() {
        let table = Fixture::parse(path, text).unwrap_or_else(|e| panic!("{e}"));
        let citation = table.header("citation").unwrap_or_default();
        let field = |key: &str| citation.split(' ').find_map(|f| f.strip_prefix(key));
        assert!(field("doi=").or(field("report=")).is_some_and(|s| !s.is_empty()), "{path}: doi= or report=");
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let (located, sha) = match field("manuscript=") {
            Some(id) => {
                let pmc = id.strip_prefix("PMC").and_then(|v| v.split_once('.'));
                (pmc.is_some_and(|(n, version)| digits(n) && digits(version)), field("xml_sha256="))
            }
            None => (field("page=").is_some_and(digits), field("pdf_sha256=")),
        };
        assert!(field("table=").is_some() && located, "{path}: table= and page= or manuscript=PMC<n>.<version>");
        let date = field("erratum-checked=").unwrap_or_default();
        assert!(
            date.len() == 10 && date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-',
            "{path}: erratum-checked"
        );
        let sha = sha.unwrap_or_default();
        assert!(sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()), "{path}: sha256 of the file read");
        assert!(table.header("transcribed").is_some_and(|t| t.contains("; checked: ")), "{path}: both transcribers");
    }
}
