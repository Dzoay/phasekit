//! `gates decisions`: the user's decisions, one file per question under `docs/decisions/` (PLAN.md M25.3; user decision
//! LM4). Each file is named `<date>-<slug>.md` for the date its heading ends with; no id (the first cell of a row of a
//! `| # |` table) is in two rows; `docs/DECISIONS.md` holds none; and every id cited as "user decision(s) …" in the
//! living docs, the code, the scripts and the configuration is one.

use std::collections::{BTreeMap, BTreeSet};

use super::{Verdict, no_args};
use crate::repo::Repo;

const DIR: &str = "docs/decisions";
const PREAMBLE: &str = "docs/DECISIONS.md";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let files = repo.files(DIR, ".md", false).map_err(|e| vec![e])?;
    let ids = check_files(&files)?;
    let mut errors = check_preamble(&repo.read(PREAMBLE).map_err(|e| vec![e])?);
    let tracked = repo.git(&["ls-files"]).map_err(|e| vec![e])?;
    let sources = tracked
        .lines()
        .filter(|path| cited_in(path))
        .map(|path| repo.read(path).map(|text| (path.to_string(), text)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| vec![e])?;
    errors.extend(check_citations(&ids, &sources));
    let cited: usize = sources.iter().map(|(_, text)| citations(text).len()).sum();
    if errors.is_empty() {
        Ok(format!(
            "{} files, {} ids; {cited} citations in {} files each name one",
            files.len(),
            ids.len(),
            sources.len()
        ))
    } else {
        Err(errors)
    }
}

/// The decision files' ids, or what is wrong with the files: a name or heading out of form, an id in two rows, or no
/// file at all.
fn check_files(files: &[(String, String)]) -> Result<BTreeSet<String>, Vec<String>> {
    let mut errors = Vec::new();
    if files.is_empty() {
        errors.push(format!("{DIR}: no decision files"));
    }
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
    for (path, text) in files {
        let name = path.rsplit('/').next().unwrap_or_default();
        if let Some(e) = misnamed(name, text) {
            errors.push(format!("{path}: {e}"));
        }
        for id in ids(text) {
            if let Some(other) = seen.insert(id, path) {
                errors.push(format!("{path}: {id} is also in {other}"));
            }
        }
    }
    if errors.is_empty() { Ok(seen.into_keys().map(str::to_string).collect()) } else { Err(errors) }
}

/// What is wrong with a decision file's name or first line, if anything: the line is `# <title> (<date>)`, and the
/// name `<date>-<slug>.md`, the slug lower-case letters, digits and hyphens.
fn misnamed(name: &str, text: &str) -> Option<String> {
    let heading = text.lines().next().unwrap_or_default();
    let date =
        heading.strip_prefix("# ").and_then(|h| h.strip_suffix(')')).and_then(|h| h.rsplit_once(" (")).map(|(_, d)| d);
    let Some(date) = date.filter(|date| is_date(date)) else {
        return Some(format!("its first line is not `# <title> (<yyyy-mm-dd>)`: `{heading}`"));
    };
    let slug =
        name.strip_prefix(date).and_then(|rest| rest.strip_prefix('-')).and_then(|rest| rest.strip_suffix(".md"));
    match slug {
        Some(slug)
            if !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') =>
        {
            None
        }
        _ => Some(format!("not named `{date}-<slug>.md` for its heading's date")),
    }
}

/// Whether `text` is a date, `yyyy-mm-dd`.
fn is_date(text: &str) -> bool {
    text.len() == 10
        && text.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
}

/// The ids of a decision file: the first cell of each row of a table whose header starts `| # |`, an id-less row's
/// `—` aside.
fn ids(text: &str) -> Vec<&str> {
    let (mut ids, mut decisions) = (Vec::new(), false);
    for line in text.lines() {
        if line.starts_with("| # |") {
            decisions = true;
        } else if !line.starts_with('|') {
            decisions = false;
        } else if decisions && !line.starts_with("|---") {
            ids.extend(line.strip_prefix("| ").and_then(|row| row.split(" | ").next()).filter(|id| *id != "—"));
        }
    }
    ids
}

/// `docs/DECISIONS.md` says where decisions live and holds none: each `##` section or decision table there is a
/// violation.
fn check_preamble(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.starts_with("## ") || line.starts_with("| # |"))
        .map(|(i, line)| format!("{PREAMBLE}:{}: `{line}`: a decision goes in a file of its own under {DIR}/", i + 1))
        .collect()
}

/// Each citation of an id that no decision file has.
fn check_citations(ids: &BTreeSet<String>, sources: &[(String, String)]) -> Vec<String> {
    sources
        .iter()
        .flat_map(|(path, text)| {
            citations(text)
                .into_iter()
                .filter(|id| !ids.contains(id))
                .map(move |id| format!("{path}: user decision {id} is in no file under {DIR}/"))
        })
        .collect()
}

/// The ids `text` cites as "user decision(s) …", with the text's lines joined and their comment markers dropped: an
/// id, then more after `, `, ` and `, `, and ` or ` ` or `, and a range after `-` or `–` ([`range`]).
fn citations(text: &str) -> Vec<String> {
    let flat: Vec<&str> =
        text.lines().map(|line| line.trim_start().trim_start_matches(['/', '!', '#', '*', '>']).trim()).collect();
    let flat = flat.join(" ").replace("User decision", "user decision");
    let mut cited = Vec::new();
    for after in flat.split("user decision").skip(1) {
        let Some(mut rest) = after.strip_prefix("s ").or_else(|| after.strip_prefix(' ')) else { continue };
        let mut from: Option<&str> = None;
        while let Some((id, after)) = id_at(rest) {
            match from.take() {
                Some(from) => cited.extend(range(from, id)),
                None => cited.push(id.to_string()),
            }
            let next = [", and ", ", ", " and ", " or ", "-", "–"]
                .into_iter()
                .find_map(|sep| after.strip_prefix(sep).filter(|next| id_at(next).is_some()).map(|next| (sep, next)));
            let Some((sep, next)) = next else { break };
            if sep == "-" || sep == "–" {
                cited.pop();
                from = Some(id);
            }
            rest = next;
        }
    }
    cited
}

/// The decision id `text` starts with, and the text after it: up to three capitals, one or two digits and perhaps a
/// letter (`MX10`, `3a`, `12`), not followed by a word character, an apostrophe, or `.` and a digit (`D8's`, `M6.3`,
/// `2026`).
fn id_at(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let letters = bytes.iter().take_while(|b| b.is_ascii_uppercase()).count();
    let digits = bytes.iter().skip(letters).take_while(|b| b.is_ascii_digit()).count();
    if letters > 3 || !(1..=2).contains(&digits) {
        return None;
    }
    let end = letters + digits + usize::from(bytes.get(letters + digits).is_some_and(u8::is_ascii_lowercase));
    let (id, rest) = text.split_at_checked(end)?;
    let mut next = rest.chars();
    match next.next() {
        Some(c) if c.is_alphanumeric() || c == '_' || c == '\'' || c == '’' => None,
        Some('.') if next.next().is_some_and(|c| c.is_ascii_digit()) => None,
        _ => Some((id, rest)),
    }
}

/// The ids a range cites: every number between its ends where both have the same capitals and a number alone
/// (`MX1-MX7`), its two ends otherwise.
fn range(from: &str, to: &str) -> Vec<String> {
    let split = |id: &str| {
        let at = id.find(|c: char| c.is_ascii_digit())?;
        let (prefix, number) = id.split_at(at);
        Some((prefix.to_string(), number.parse::<u32>().ok()?))
    };
    match (split(from), split(to)) {
        (Some((p, a)), Some((q, b))) if p == q => (a.min(b)..=a.max(b)).map(|n| format!("{p}{n}")).collect(),
        _ => vec![from.to_string(), to.to_string()],
    }
}

/// Whether a tracked file's citations are checked: the living docs, the code, the scripts and the configuration, not
/// the frozen evidence and history (AGENTS.md), whose ids may predate the decisions.
fn cited_in(path: &str) -> bool {
    const FROZEN: [&str; 3] = ["docs/history/", "docs/coolprop-map/", "docs/research/"];
    const TEXT: [&str; 9] = [".md", ".rs", ".toml", ".txt", ".sh", ".py", ".yml", ".ts", ".svelte"];
    !FROZEN.iter().any(|dir| path.starts_with(dir)) && TEXT.iter().any(|ext| path.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> (String, String) {
        (format!("{DIR}/{path}"), text.to_string())
    }

    const TABLE: &str = "| # | Question | Decision | Notes |\n|---|---|---|---|\n";

    /// The phrase, for the examples that cite an id no file has: the gate reads this file too, and a literal citation
    /// of such an id here would fail it.
    const CITE: &str = "user decision";

    /// A decision file with one table of the given ids.
    fn decision(date: &str, ids: &[&str]) -> String {
        let rows: String = ids.iter().map(|id| format!("| {id} | A question? | **An answer** | Why |\n")).collect();
        format!("# A question ({date})\n\nAsked when.\n\n{TABLE}{rows}")
    }

    /// An id is one decision's (user decision LM4): the same id in two files, or twice in one, fails and names where;
    /// an id-less row (`—`) and the rows of other tables are not ids.
    #[test]
    fn an_id_in_two_files_is_rejected() {
        let one = file("2026-10-18-panics.md", &decision("2026-10-18", &["PH1", "—"]));
        let other = file("2026-10-18-lists.md", &decision("2026-10-18", &["LM1", "LM2", "—"]));
        let want: BTreeSet<String> = ["LM1", "LM2", "PH1"].map(String::from).into();
        assert_eq!(check_files(&[one.clone(), other.clone()]), Ok(want));
        let again = file("2026-10-19-again.md", &decision("2026-10-19", &["PH1"]));
        assert_eq!(
            check_files(&[one.clone(), again.clone()]),
            Err(vec![format!("{DIR}/2026-10-19-again.md: PH1 is also in {DIR}/2026-10-18-panics.md")])
        );
        let twice = file("2026-10-18-twice.md", &decision("2026-10-18", &["SP1", "SP1"]));
        assert_eq!(check_files(&[twice]).unwrap_err().len(), 1);
        let measured =
            format!("{}\n| Mixture | α^r |\n|---|---|\n| PH1 | 4.4 µs |\n", decision("2026-10-18", &["MX1"]));
        assert!(check_files(&[one, file("2026-10-18-measured.md", &measured)]).is_ok(), "a table of measurements");
        assert!(check_files(&[]).is_err(), "no decision files at all");
    }

    /// A file is named for the day its heading gives: `<date>-<slug>.md`, the slug lower-case letters, digits and
    /// hyphens; its first line is `# <title> (<date>)`.
    #[test]
    fn a_file_named_for_another_date_is_rejected() {
        let text = decision("2026-10-18", &["LM1"]);
        assert!(check_files(&[file("2026-10-18-lists-that-merge-2.md", &text)]).is_ok());
        let named = |name: &str| check_files(&[file(name, &text)]);
        assert!(named("2026-10-17-lists.md").is_err(), "another date");
        assert!(named("2026-10-18-.md").is_err(), "no slug");
        assert!(named("2026-10-18-Lists.md").is_err(), "a capital");
        assert!(named("2026-10-18-lists_that.md").is_err(), "an underscore");
        assert!(named("2026-10-18-lists.txt").is_err(), "not markdown");
        assert!(named("2026-10-18lists.md").is_err(), "no hyphen after the date");
        let headed = |heading: &str| {
            check_files(&[file("2026-10-18-lists.md", &text.replace("# A question (2026-10-18)", heading))])
        };
        assert!(headed("## A question (2026-10-18)").is_err(), "not a title");
        assert!(headed("# A question").is_err(), "no date");
        assert!(headed("# A question (soon)").is_err(), "not a date");
        assert!(headed("# A question (2026/10/18)").is_err(), "not a date");
        assert!(headed("# A question (2026-10-1)").is_err(), "not a date");
        assert!(headed("# A question (2026-10-18").is_err(), "unclosed");
        assert!(headed("# A question (20261-0-18)").is_err(), "hyphens out of place");
        // Not a date though the name agrees with the heading.
        for date in ["2026x10x18", "2026-10-1", "2026-10-180"] {
            let text = text.replace("2026-10-18", date);
            assert!(check_files(&[file(&format!("{date}-lists.md"), &text)]).is_err(), "{date}");
        }
    }

    /// `docs/DECISIONS.md` says where decisions live and holds none: a section or a decision table there fails.
    #[test]
    fn decisions_md_holds_no_decision() {
        let preamble = "# User decisions\n\nThey live in [decisions/](decisions/).\n";
        assert_eq!(check_preamble(preamble), Vec::<String>::new());
        let section = format!("{preamble}\n## Panics at the facades (2026-10-18)\n");
        assert_eq!(check_preamble(&section).len(), 1);
        assert!(check_preamble(&section)[0].starts_with(&format!("{PREAMBLE}:5: ")));
        assert_eq!(check_preamble(&format!("{preamble}\n{TABLE}| PH1 | When? | Now | |\n")).len(), 1);
    }

    /// The ids a citation names: one or a list (`, `, `and`, `or`), a range of one prefix expanded, wrapped across
    /// comment lines; a step, a date or a possessive after the list is not an id.
    #[test]
    fn citations_name_ids() {
        let cited = |text: &str| citations(text);
        assert_eq!(cited("(user decision PH1)."), ["PH1"]);
        assert_eq!(cited("User decision CB1 replaced decision 10"), ["CB1"]);
        assert_eq!(
            cited("user decisions TQ1, TQ2 and TQ3; user decisions 7, 11, and 3a or 12"),
            ["TQ1", "TQ2", "TQ3", "7", "11", "3a", "12"]
        );
        assert_eq!(cited("user decisions MX1-MX4"), ["MX1", "MX2", "MX3", "MX4"]);
        assert_eq!(cited("user decisions LM3–LM1"), ["LM1", "LM2", "LM3"]);
        assert_eq!(cited("user decisions MX9-SP2"), ["MX9", "SP2"], "a range across prefixes: its ends");
        assert_eq!(cited(&format!("{CITE} ABC1; {CITE} MX10")), ["ABC1", "MX10"]);
        assert_eq!(cited("// Exempt (user\n    // decision PS1, M6.3)."), ["PS1"]);
        assert_eq!(cited("# a comment (user\n# decisions CI5,\n# CI6)"), ["CI5", "CI6"]);
        assert_eq!(cited("user decision 1, 2026-10-05"), ["1"]);
        assert_eq!(cited("user decision MX9, D8's exception"), ["MX9"]);
        assert_eq!(cited("user decision DP1, M6.3"), ["DP1"]);
        assert_eq!(cited("as user decision PH1. Then"), ["PH1"], "a full stop, then no digit");
        let none: [&str; 7] = [
            "user decision ABCD1",
            "user decision 123",
            "user decision M6.3",
            "user decision D8's",
            "user decision X1_y",
            "user decision on it",
            "user decisionsPH1",
        ];
        for text in none {
            assert_eq!(cited(text), Vec::<String>::new(), "{text}");
        }
    }

    /// A citation of an id no file has fails and names the file; the frozen evidence and history are not read.
    #[test]
    fn a_citation_of_an_unknown_id_is_rejected() {
        let ids: BTreeSet<String> = ["LM1", "PH1"].map(String::from).into();
        let sources = [
            ("docs/PLAN.md".to_string(), "user decisions LM1, PH1".to_string()),
            ("crates/x/src/lib.rs".to_string(), format!("/// {CITE} XX9")),
        ];
        assert_eq!(
            check_citations(&ids, &sources),
            [format!("crates/x/src/lib.rs: {CITE} XX9 is in no file under docs/decisions/")]
        );
        for path in [
            "docs/PLAN.md",
            "AGENTS.md",
            "crates/phasekit-core/src/lib.rs",
            ".cargo/mutants.toml",
            "ci/test-counts.txt",
        ] {
            assert!(cited_in(path), "{path}");
        }
        for path in ["docs/history/README.md", "docs/coolprop-map/04.md", "docs/research/r.md", "data/fluids.bin"] {
            assert!(!cited_in(path), "{path}");
        }
    }
}
