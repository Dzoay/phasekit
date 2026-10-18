//! `gates rot`: the rot register is ticked as milestones close (PLAN.md §2.1). A row whose milestone has closed (is
//! below `phasekit_verify::MILESTONE`) may not be GAP, and no part of a Proof cell may still say "new" once it is due:
//! at its "(Mk)" if it has one, otherwise at the row's first milestone. Every test a Proof cell names is a function
//! under `crates/` unless it is marked new, and one marked new is not yet one (the step that writes it ticks it).

use std::collections::{BTreeMap, BTreeSet};

use super::{Verdict, no_args};
use crate::repo::Repo;

const REGISTER: &str = "docs/ROT-REGISTER.md";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let register = repo.read(REGISTER).map_err(|e| vec![e])?;
    let sources = repo.files("crates", ".rs", true).map_err(|e| vec![e])?;
    let fns = function_names(sources.iter().map(|(_, text)| text.as_str()));
    let milestone = phasekit_verify::MILESTONE;
    let rows = check(&register, milestone, &fns)?;
    Ok(format!("{rows} rows; nothing due before M{milestone} is open; every named proof exists"))
}

/// Checks the register text against the first open milestone and the workspace's function names; returns how many
/// register rows it read. A ROT id is one row's (user decision LM5).
fn check(register: &str, milestone: u8, fns: &BTreeSet<&str>) -> Result<usize, Vec<String>> {
    let (mut rows, mut errors) = (0, Vec::new());
    let mut lines_of: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, line) in register.lines().enumerate().filter(|(_, line)| line.starts_with("| ROT-")) {
        let cells: Vec<&str> = match line.trim_end().strip_prefix("| ").and_then(|l| l.strip_suffix(" |")) {
            Some(inner) => inner.split(" | ").map(str::trim).collect(),
            None => Vec::new(),
        };
        if cells.len() != 7 {
            errors.push(format!("{REGISTER}:{}: a ROT row with {} cells, expected 7", i + 1, cells.len()));
            continue;
        }
        if let Some(first) = lines_of.insert(cells[0], i + 1) {
            errors.push(format!("{REGISTER}:{}: {} is also on line {first}", i + 1, cells[0]));
        }
        rows += 1;
        check_row(&cells, milestone, &mut errors);
        check_names(cells[0], cells[4], fns, &mut errors);
    }
    if rows == 0 {
        errors.push(format!("{REGISTER}: no register rows"));
    }
    if errors.is_empty() { Ok(rows) } else { Err(errors) }
}

/// Checks one register row's seven `cells`: not GAP once its first milestone has closed, and no part of its Proof
/// cell marked new once that part is due.
fn check_row(cells: &[&str], milestone: u8, errors: &mut Vec<String>) {
    let (id, proof, status) = (cells[0], cells[4], cells[6]);
    let first = match first_milestone(cells[5]) {
        Ok(first) => first,
        Err(e) => {
            errors.push(format!("{id}: {e}"));
            return;
        }
    };
    if let Some(m) = first.filter(|&m| m < milestone && status.starts_with("GAP")) {
        errors.push(format!("{id}: still GAP, but M{m} has closed"));
    }
    for part in proof.split(';').filter(|part| has_word(part, "new")) {
        if let Some(due) = due(part).or(first).filter(|&due| due < milestone) {
            errors.push(format!("{id}: proof \"{}\" is still marked new, due at M{due} (PLAN.md §2.1)", part.trim()));
        }
    }
}

/// Checks that each test the `proof` cell of row `id` names is in `fns` exactly when it is not marked new.
fn check_names(id: &str, proof: &str, fns: &BTreeSet<&str>, errors: &mut Vec<String>) {
    for (name, new) in named_tests(proof) {
        match (new, fns.contains(name)) {
            (false, false) => errors.push(format!("{id}: proof `{name}` names no function under crates/")),
            (true, true) => errors.push(format!("{id}: proof `{name}` exists, so tick it (PLAN.md §2.1)")),
            _ => {}
        }
    }
}

/// The tests a Proof cell names, each with whether it is marked new: the code spans that name a workspace item
/// (`phasekit_…::name` or `<file>.rs::name`, `{a, b}` groups included) whose last segment is a snake_case name with an
/// underscore, as test names are. "new" marks every span after it in its `;`-separated part, as for the due check.
fn named_tests(proof: &str) -> Vec<(&str, bool)> {
    let mut named = Vec::new();
    for (span, new) in proof.split(';').flat_map(spans) {
        let last = span.rsplit_once("::").map_or(span, |(_, last)| last);
        let names = last.trim_start_matches('{').trim_end_matches('}').split(',').map(str::trim);
        named.extend(names.filter(|name| is_test_name(name)).map(|name| (name, new)));
    }
    named
}

/// The code spans of one Proof part that name a workspace item by path, each with whether "new" precedes it.
fn spans(part: &str) -> Vec<(&str, bool)> {
    let pieces: Vec<&str> = part.split('`').collect();
    let mut new = false;
    let mut spans = Vec::new();
    for pair in pieces.chunks(2) {
        new |= has_word(pair[0], "new");
        spans.extend(pair.get(1).copied().filter(|span| names_workspace_item(span)).map(|span| (span, new)));
    }
    spans
}

/// Whether a code span is a path into the workspace: `phasekit_…::…` or `<file>.rs::…`.
fn names_workspace_item(span: &str) -> bool {
    span.starts_with("phasekit_") && span.contains("::") || span.contains(".rs::")
}

/// Whether `name` looks like a test function's name: lowercase snake_case with at least one underscore.
fn is_test_name(name: &str) -> bool {
    name.contains('_') && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Every name that follows `fn ` in `sources`.
fn function_names<'a>(sources: impl Iterator<Item = &'a str>) -> BTreeSet<&'a str> {
    let after_fn = |text: &'a str| text.split("fn ").skip(1).collect::<Vec<_>>();
    sources
        .flat_map(after_fn)
        .filter_map(|rest| rest.split(|c: char| !(c.is_alphanumeric() || c == '_')).next())
        .filter(|name| !name.is_empty())
        .collect()
}

/// The row's first milestone (`M6, M7` → 6), or `None` for `post-0.1` and `n/a`, which never come due.
fn first_milestone(cell: &str) -> Result<Option<u8>, String> {
    if cell == "n/a" || cell.starts_with("post-") {
        return Ok(None);
    }
    number(cell.strip_prefix('M').unwrap_or_default()).map(Some).ok_or(format!("unreadable milestone `{cell}`"))
}

/// The milestone of a part's "(Mk ...)" marker.
fn due(part: &str) -> Option<u8> {
    number(part.split_once("(M")?.1)
}

/// The leading decimal number of `text`.
fn number(text: &str) -> Option<u8> {
    text.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// Whether `word` occurs in the prose of `text` as a whole word (`new`, not `new_family`, and not inside a code span
/// such as `` `Input::new` ``).
fn has_word(text: &str, word: &str) -> bool {
    let prose = text.split('`').step_by(2);
    prose.flat_map(|part| part.split(|c: char| !(c.is_alphanumeric() || c == '_'))).any(|w| w == word)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str =
        "| ID | Source | Problem | Mechanism | Proof | Milestone | Status |\n|---|---|---|---|---|---|---|\n";

    fn register(proof: &str, milestone: &str, status: &str) -> String {
        format!("{HEAD}| ROT-900 | map 01 R1 | a problem | a mechanism | {proof} | {milestone} | {status} |\n")
    }

    /// PLAN.md §2.1 "Ticking"; the fail-closed half of the milestone definition of done (§0.2).
    #[test]
    fn an_unticked_due_rot_row_is_rejected() {
        let none = BTreeSet::new();
        let check = |register: &str, milestone| check(register, milestone, &none);
        let unticked = register("new `phasekit_xtask::tests::t`", "M0", "Lint-guarded");
        assert_eq!(check(&unticked, 0), Ok(1), "not due while M0 is open");
        let errors = check(&unticked, 1).unwrap_err();
        assert!(errors.len() == 1 && errors[0].starts_with("ROT-900"), "{errors:?}");
        assert_eq!(check(&register("`phasekit_xtask::tests::t` [M0.4]", "M0", "Lint-guarded"), 1), Ok(1));
        // A part marked (Mk) is due at Mk, not at the row's first milestone; "(new)" counts as new.
        let later = register("`a` [M6.2]; new `b` (M7)", "M6, M7", "Test-guarded");
        assert_eq!(check(&later, 7), Ok(1));
        assert!(check(&later, 8).is_err());
        assert!(check(&register("exported-symbol list test (new)", "M10", "Test-guarded"), 11).is_err());
        // A code span is not prose: `Input::new` names a function, not a planned proof.
        assert_eq!(check(&register("M1 proptest over `Input::new` [M1.14]", "M1", "Test-guarded"), 2), Ok(1));
        assert!(check(&register("new proptest over `Input::new`", "M1", "Test-guarded"), 2).is_err());
        // GAP fails once its milestone closes; a row without a milestone number never comes due.
        assert!(check(&register("`a`", "M2", "GAP"), 3).is_err());
        assert_eq!(check(&register("`a`", "M2", "GAP"), 2), Ok(1), "M2 is still open");
        assert_eq!(check(&register("new `a`", "post-0.1", "Deferred: M19"), 99), Ok(1));
        // Fail closed: a malformed row, an unreadable milestone or an empty register.
        assert!(check(&format!("{HEAD}| ROT-901 | too | few |\n"), 0).is_err());
        assert!(check(&register("`a`", "soon", "Test-guarded"), 0).is_err());
        assert!(check(HEAD, 0).is_err());
    }

    /// A ROT id is one row's (PLAN.md M25.2; user decision LM5): two sessions that both took the next free number, or a
    /// row both sides of a union merge edited, show as one id on two rows, and the gate names both lines.
    #[test]
    fn a_rot_id_on_two_rows_is_rejected() {
        let none = BTreeSet::new();
        let row = register("`a`", "M2", "Test-guarded");
        assert_eq!(check(&row, 2, &none), Ok(1));
        let twice = format!("{row}{}", row.trim_start_matches(HEAD).replace("a problem", "another problem"));
        let errors = check(&twice, 2, &none).unwrap_err();
        assert_eq!(errors, [format!("{REGISTER}:4: ROT-900 is also on line 3")]);
        let other = format!("{row}{}", row.trim_start_matches(HEAD).replace("ROT-900", "ROT-901"));
        assert_eq!(check(&other, 2, &none), Ok(2));
    }

    /// The code is the truth (user decision DL4): a Proof cell may not name a test that does not exist, and a test it
    /// still marks new may not exist yet. ROT-002 named a test renamed at M6, ROT-010 and ROT-020 unwritten ones.
    #[test]
    fn a_proof_naming_a_missing_test_is_rejected() {
        let fns = function_names(["#[test]\nfn rows_round_trip() {}\npub(crate) fn fd_first_order() {}\n"].into_iter());
        let check = |proof: &str| check(&register(proof, "M5", "Test-guarded"), 5, &fns);
        assert_eq!(check("`phasekit_verify::tests::rows_round_trip` [M5.1]"), Ok(1));
        assert_eq!(check("`phasekit_verify::conformance::{fd_first_order, rows_round_trip}`"), Ok(1));
        assert_eq!(check("`new_family.rs::rows_round_trip`; new `phasekit_core::tests::not_written_yet` (M8)"), Ok(1));
        let errors = check("`a`; `phasekit_core::tests::renamed_long_ago` [M5.3]").unwrap_err();
        assert!(errors.len() == 1 && errors[0].contains("renamed_long_ago"), "{errors:?}");
        assert!(check("`flash.rs::renamed_long_ago`").is_err(), "a file-qualified name too");
        let errors = check("new `phasekit_verify::tests::rows_round_trip` (M8)").unwrap_err();
        assert!(errors.len() == 1 && errors[0].contains("tick it"), "{errors:?}");
        // "new" covers the rest of its part, a list included, and no earlier name in it.
        assert_eq!(check("new `phasekit_capi::tests::one_test`, `phasekit_capi::tests::another_test`; `a`"), Ok(1));
        assert_eq!(
            check("`phasekit_verify::tests::rows_round_trip` [M5.1], new `phasekit_capi::tests::one_test`"),
            Ok(1)
        );
        // Spans that name no workspace test are not checked: std paths, types, modules, crates, lints.
        assert_eq!(
            check("`std::env::var_os`, `phasekit_core::Error`, `phasekit_core::state`, clippy `unwrap_used`"),
            Ok(1)
        );
        assert_eq!(check("`phasekit_core` and `phasekit_py` builds"), Ok(1));
    }
}
