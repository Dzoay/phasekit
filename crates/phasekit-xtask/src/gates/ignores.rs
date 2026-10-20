//! `gates ignores`: every ignored test says why, in the grammar of PLAN.md §2.1 (map 10 R14: disabled instead of
//! fixed). The reason starts with `DIV-NNNN: `, `issue #N: ` or `nightly: ` (the last only in `tests/sweeps.rs`), and
//! a bare `#[ignore]` fails. Each run lists the ignored tests. From M1.6 the DIV ids are checked against the register.

use phasekit_verify::{DivStatus, Divergence};

use super::{Verdict, no_args};
use crate::repo::Repo;

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let files = repo.files("crates", ".rs", true).map_err(|e| vec![e])?;
    let ignored = check(&files, phasekit_verify::DIVERGENCES)?;
    let mut summary = format!("{} files, {} ignored tests", files.len(), ignored.len());
    for line in ignored {
        summary.push_str("\n  ignored: ");
        summary.push_str(&line);
    }
    Ok(summary)
}

/// Checks every `#[ignore]` in the (path, text) files; returns the ignored tests as `path:line: reason`.
pub(crate) fn check(files: &[(String, String)], register: &[Divergence]) -> Result<Vec<String>, Vec<String>> {
    let (mut ignored, mut errors) = (Vec::new(), Vec::new());
    for (path, text) in files {
        for (i, line) in text.lines().enumerate() {
            let Some((line, attribute)) = ignore(line) else { continue };
            let at = format!("{path}:{}", i + 1);
            match reason(attribute).map(|reason| (reason, valid(reason, path, register))) {
                Some((reason, Ok(()))) => ignored.push(format!("{at}: {reason}")),
                Some((_, Err(why))) => errors.push(format!("{at}: {why}")),
                None => errors.push(format!("{at}: an ignored test needs a reason (PLAN.md §2.1): `{line}`")),
            }
        }
    }
    if errors.is_empty() { Ok(ignored) } else { Err(errors) }
}

/// The trimmed `line` and its `ignore...` attribute, `#[ignore...]` or a `#[cfg_attr(..., ignore...)]`, if it has one.
fn ignore(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    let attribute = match line.strip_prefix("#[") {
        Some(attr) if attr.starts_with("ignore") => attr,
        Some(attr) if attr.starts_with("cfg_attr(") => attr.split_once(", ignore")?.1.trim_start_matches(", "),
        _ => return None,
    };
    Some((line, attribute))
}

/// The reason in `ignore = "<reason>"...`, or `None` for a bare `ignore`.
fn reason(attribute: &str) -> Option<&str> {
    let quoted = attribute.strip_prefix("ignore")?.trim_start().strip_prefix('=')?.trim_start().strip_prefix('"')?;
    quoted.split_once('"').map(|(reason, _)| reason)
}

/// PLAN.md §2.1: `DIV-NNNN: `, `issue #N: ` or (in `tests/sweeps.rs` only) `nightly: `, then some words.
fn valid(reason: &str, path: &str, register: &[Divergence]) -> Result<(), String> {
    let words = if let Some((id, words)) = reason.strip_prefix("DIV-").and_then(|r| r.split_once(": ")) {
        divergence(reason, id, register)?;
        words
    } else if let Some((n, words)) = reason.strip_prefix("issue #").and_then(|r| r.split_once(": ")) {
        if !digits(n) {
            return Err(format!("`{reason}`: an issue reference is `issue #<number>: `"));
        }
        words
    } else if let Some(words) = reason.strip_prefix("nightly: ") {
        if !path.ends_with("tests/sweeps.rs") {
            return Err(format!("`{reason}`: `nightly:` ignores live only in tests/sweeps.rs"));
        }
        words
    } else {
        return Err(format!("`{reason}`: the reason starts with `DIV-NNNN: `, `issue #N: ` or `nightly: `"));
    };
    if words.trim().is_empty() { Err(format!("`{reason}`: say why after the id")) } else { Ok(()) }
}

/// The id of a `DIV-NNNN: ` reason: four digits, a divergence of `register` still open.
fn divergence(reason: &str, id: &str, register: &[Divergence]) -> Result<(), String> {
    if id.len() != 4 || !digits(id) {
        return Err(format!("`{reason}`: a divergence id is DIV- and four digits"));
    }
    match register.iter().find(|d| d.id.strip_prefix("DIV-") == Some(id)) {
        None => Err(format!("`{reason}`: DIV-{id} is not in the register")),
        Some(d) if d.status != DivStatus::Open => Err(format!("`{reason}`: DIV-{id} is resolved upstream")),
        Some(_) => Ok(()),
    }
}

/// Whether `s` is one or more ASCII digits.
fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, attribute: &str) -> Vec<(String, String)> {
        vec![(path.to_string(), format!("#[test]\n{attribute}\nfn slow() {{}}\n"))]
    }

    fn accepted(path: &str, attribute: &str) -> bool {
        check(&file(path, attribute), phasekit_verify::DIVERGENCES).is_ok()
    }

    /// Map 10 R14. PLAN.md §2.1 "Ignores".
    #[test]
    fn ignore_without_a_valid_reason_is_rejected() {
        let eos = "crates/phasekit-verify/tests/eos.rs";
        assert!(accepted(eos, r#"#[ignore = "DIV-0007: R1234ze(E) gas constant"]"#));
        assert!(accepted(eos, r#"#[ignore = "issue #12: flaky under wasmtime"]"#));
        assert!(accepted("crates/phasekit-verify/tests/sweeps.rs", r#"#[ignore = "nightly: full oracle sweep"]"#));
        assert!(!accepted(eos, "#[ignore]"));
        assert!(!accepted(eos, r#"#[ignore = "slow"]"#));
        assert!(!accepted(eos, r#"#[ignore = "DIV-7: short id"]"#));
        let short = check(&file(eos, r#"#[ignore = "DIV-7: short id"]"#), phasekit_verify::DIVERGENCES);
        assert!(short.unwrap_err()[0].contains("four digits"), "the id's form, before the register");
        assert!(!accepted(eos, r#"#[ignore = "issue #: no number"]"#));
        assert!(!accepted(eos, r#"#[ignore = "DIV-0007: "]"#), "a reason needs words after its id");
        assert!(!accepted(eos, r#"#[ignore = "nightly: only tests/sweeps.rs may say this"]"#));
        assert!(!accepted(eos, r#"#[cfg_attr(target_family = "wasm", ignore)]"#));
        assert!(accepted(eos, "#[allow(dead_code, ignored_unit_patterns)]"), "only `cfg_attr` carries an ignore");
        // From M1.6 a cited divergence must exist in the register and still be open (VERIFICATION.md §11.2).
        assert!(!accepted(eos, r#"#[ignore = "DIV-9999: no such entry"]"#));
        let resolved =
            [Divergence { status: DivStatus::ResolvedUpstream { commit: "abc" }, ..phasekit_verify::DIVERGENCES[6] }];
        let errors = check(&file(eos, r#"#[ignore = "DIV-0007: fixed upstream"]"#), &resolved).unwrap_err();
        assert!(errors[0].contains("resolved upstream"), "{errors:?}");
        let listed = check(&file(eos, r#"#[ignore = "issue #12: flaky"]"#), phasekit_verify::DIVERGENCES);
        assert_eq!(listed, Ok(vec![format!("{eos}:2: issue #12: flaky")]));
    }
}
