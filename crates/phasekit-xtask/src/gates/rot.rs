//! `gates rot`: the rot register is ticked as milestones close (PLAN.md §2.1). A row whose milestone has closed (is
//! below `phasekit_verify::MILESTONE`) may not be GAP, and no part of a Proof cell may still say "new" once it is due:
//! at its "(Mk)" if it has one, otherwise at the row's first milestone.

use super::{Verdict, no_args};
use crate::repo::Repo;

const REGISTER: &str = "docs/ROT-REGISTER.md";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let register = repo.read(REGISTER).map_err(|e| vec![e])?;
    let milestone = phasekit_verify::MILESTONE;
    let rows = check(&register, milestone)?;
    Ok(format!("{rows} rows; nothing due before M{milestone} is open"))
}

/// Checks the register text against the first open milestone; returns how many register rows it read.
fn check(register: &str, milestone: u8) -> Result<usize, Vec<String>> {
    let (mut rows, mut errors) = (0, Vec::new());
    for (i, line) in register.lines().enumerate().filter(|(_, line)| line.starts_with("| ROT-")) {
        let cells: Vec<&str> = match line.trim_end().strip_prefix("| ").and_then(|l| l.strip_suffix(" |")) {
            Some(inner) => inner.split(" | ").map(str::trim).collect(),
            None => Vec::new(),
        };
        match cells.len() {
            8 => rows += 1,
            6 => continue, // section 3 keeps a record of the former gaps; section 4 is the register
            n => {
                errors.push(format!("{REGISTER}:{}: a ROT row with {n} cells, expected 8", i + 1));
                continue;
            }
        }
        let (id, proof, status) = (cells[0], cells[5], cells[7]);
        let first = match first_milestone(cells[6]) {
            Ok(first) => first,
            Err(e) => {
                errors.push(format!("{id}: {e}"));
                continue;
            }
        };
        if let Some(m) = first.filter(|&m| m < milestone && status.starts_with("GAP")) {
            errors.push(format!("{id}: still GAP, but M{m} has closed"));
        }
        for part in proof.split(';').filter(|part| has_word(part, "new")) {
            if let Some(due) = due(part).or(first).filter(|&due| due < milestone) {
                errors
                    .push(format!("{id}: proof \"{}\" is still marked new, due at M{due} (PLAN.md §2.1)", part.trim()));
            }
        }
    }
    if rows == 0 {
        errors.push(format!("{REGISTER}: no register rows"));
    }
    if errors.is_empty() { Ok(rows) } else { Err(errors) }
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

    const HEAD: &str = "| ID | Source | Code | Problem | Mechanism | Proof | Milestone | Status |\n|---|---|---|---|---|---|---|---|\n";

    fn register(proof: &str, milestone: &str, status: &str) -> String {
        format!(
            "{HEAD}| ROT-900 | map 01 R1 | a.cpp:1 | a problem | a mechanism | {proof} | {milestone} | {status} |\n"
        )
    }

    /// PLAN.md §2.1 "Ticking"; the fail-closed half of the milestone definition of done (§0.2).
    #[test]
    fn an_unticked_due_rot_row_is_rejected() {
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
        assert_eq!(check(&register("new `a`", "post-0.1", "Deferred: M19"), 99), Ok(1));
        // Fail closed: a malformed row, an unreadable milestone or an empty register.
        assert!(check(&format!("{HEAD}| ROT-901 | too | few |\n"), 0).is_err());
        assert!(check(&register("`a`", "soon", "Test-guarded"), 0).is_err());
        assert!(check(HEAD, 0).is_err());
    }
}
