//! The committed fixtures and their manifest (VERIFICATION.md §3.4; map 10 R15). This file lists every committed
//! fixture; the tests that read them live elsewhere, which `cargo xtask gates fixtures` checks.

use phasekit_verify::{fixture, sha256_hex};

/// Every committed fixture. A new fixture is added here, to MANIFEST.sha256 (`cargo xtask oracle --write-manifest`)
/// and to the test that reads it.
const COMMITTED: &[(&str, &str)] = &[
    fixture!("coolprop-8.0.0/facts/smoke.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.6.check.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.6.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.7.check.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.7.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.8.check.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.8.csv"),
];

const MANIFEST: &str = include_str!("../fixtures/MANIFEST.sha256");
const REUSE: &str = include_str!("../../../REUSE.toml");

/// REUSE `path` patterns: `**` matches anything, `*` anything but `/`.
fn matches(pattern: &str, path: &str) -> bool {
    match (pattern.split_once("**"), pattern.split_once('*')) {
        (Some((head, tail)), _) => {
            path.starts_with(head) && (0..=path.len()).any(|i| path.get(i..).is_some_and(|rest| matches(tail, rest)))
        }
        (None, Some((head, tail))) => {
            path.starts_with(head)
                && (head.len()..=path.len()).any(|i| {
                    !path[head.len()..i].contains('/') && path.get(i..).is_some_and(|rest| matches(tail, rest))
                })
        }
        (None, None) => pattern == path,
    }
}

/// The REUSE.toml `path` patterns other than the catch-all `**`, as paths from the repository root.
fn annotations() -> Vec<&'static str> {
    REUSE
        .lines()
        .filter_map(|line| line.trim().trim_start_matches("path = ").strip_prefix('"'))
        .filter_map(|rest| rest.split('"').next())
        .filter(|pattern| *pattern != "**")
        .collect()
}

/// Map 10 R15: every committed fixture is in the manifest with its sha256, size and row count, and has a REUSE
/// annotation of its own (provenance and licence, VERIFICATION.md §3.7); the manifest lists nothing else.
#[test]
fn committed_fixtures_match_manifest() {
    let lines: Vec<Vec<&str>> =
        MANIFEST.lines().filter(|l| !l.starts_with('#')).map(|l| l.split(' ').collect()).collect();
    assert!(lines.iter().all(|fields| fields.len() == 4), "a manifest line is `<sha256> <bytes> <rows> <path>`");
    let listed: Vec<&str> = lines.iter().map(|fields| fields[3]).collect();
    let committed: Vec<&str> = COMMITTED.iter().map(|(path, _)| *path).collect();
    assert_eq!(listed, committed, "MANIFEST.sha256 and this file list the same fixtures, sorted");
    for ((path, text), fields) in COMMITTED.iter().zip(&lines) {
        let rows = text.lines().filter(|line| !line.starts_with('#')).count();
        let want = [sha256_hex(text.as_bytes()), text.len().to_string(), rows.to_string()];
        assert_eq!(fields[..3], want, "{path}");
        let repo_path = format!("crates/phasekit-verify/fixtures/{path}");
        assert!(annotations().iter().any(|p| matches(p, &repo_path)), "{path} has no REUSE annotation of its own");
    }
}

#[test]
fn reuse_patterns_match_like_reuse() {
    assert!(matches("crates/a/**", "crates/a/b/c.csv") && matches("**", "x"));
    assert!(matches("crates/*/c.csv", "crates/b/c.csv") && !matches("crates/*/c.csv", "crates/b/d/c.csv"));
    assert!(matches("a.csv", "a.csv") && !matches("a.csv", "b.csv") && !matches("crates/a/**", "crates/b/x"));
    assert!(annotations().contains(&"crates/phasekit-verify/fixtures/coolprop-8.0.0/**"));
}
