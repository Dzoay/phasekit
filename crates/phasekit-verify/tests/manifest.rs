//! The committed fixtures and their manifest (VERIFICATION.md §3.4; map 10 R15). This file lists every committed
//! fixture; the tests that read them live elsewhere, which `cargo xtask gates fixtures` checks.

use phasekit_verify::{fixture, sha256_hex};

/// Every committed fixture. A new fixture is added here, to MANIFEST.sha256 (`cargo xtask oracle --write-manifest`)
/// and to the test that reads it.
const COMMITTED: &[(&str, &str)] = &[
    fixture!("coolprop-8.0.0/facts/register.csv"),
    fixture!("coolprop-8.0.0/facts/smoke.csv"),
    fixture!("mp/check-points.csv"),
    fixture!("paper/Helium/OrtizVega-JPCRD-2019.3.check.csv"),
    fixture!("paper/Helium/OrtizVega-JPCRD-2019.3.csv"),
    fixture!("paper/Helium/OrtizVega-JPCRD-2019.4.check.csv"),
    fixture!("paper/Helium/OrtizVega-JPCRD-2019.4.csv"),
    fixture!("paper/PropyleneGlycol/Eisenbach-JPCRD-2021.8.check.csv"),
    fixture!("paper/PropyleneGlycol/Eisenbach-JPCRD-2021.8.csv"),
    fixture!("paper/R1123/Akasaka-IJR-2020-R1123.8.check.csv"),
    fixture!("paper/R1123/Akasaka-IJR-2020-R1123.8.csv"),
    fixture!("paper/R1130(E)/Huber-IJT-2025-R1130E.4.check.csv"),
    fixture!("paper/R1130(E)/Huber-IJT-2025-R1130E.4.csv"),
    fixture!("paper/R1132(E)/Akasaka-IJT-2024-R1132E.6.check.csv"),
    fixture!("paper/R1132(E)/Akasaka-IJT-2024-R1132E.6.csv"),
    fixture!("paper/R115/Lemmon-JCED-2016-365227.7.check.csv"),
    fixture!("paper/R115/Lemmon-JCED-2016-365227.7.csv"),
    fixture!("paper/R1224YDZ/Akasaka-IJT-2023-R1224ydZ.7.check.csv"),
    fixture!("paper/R1224YDZ/Akasaka-IJT-2023-R1224ydZ.7.csv"),
    fixture!("paper/R1233zd(E)/Akasaka-JPCRD-2022-R1233zdE.IX.check.csv"),
    fixture!("paper/R1233zd(E)/Akasaka-JPCRD-2022-R1233zdE.IX.csv"),
    fixture!("paper/R1234yf/Lemmon-IJT-2022.7.check.csv"),
    fixture!("paper/R1234yf/Lemmon-IJT-2022.7.csv"),
    fixture!("paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.check.csv"),
    fixture!("paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.csv"),
    fixture!("paper/R1243zf/Akasaka-IJT-2025-R1243zf.6.check.csv"),
    fixture!("paper/R1243zf/Akasaka-IJT-2025-R1243zf.6.csv"),
    fixture!("paper/R13I1/Lemmon-JCED-2016-365227.7.check.csv"),
    fixture!("paper/R13I1/Lemmon-JCED-2016-365227.7.csv"),
    fixture!("paper/R227EA/Lemmon-JCED-2016-365227.7.check.csv"),
    fixture!("paper/R227EA/Lemmon-JCED-2016-365227.7.csv"),
    fixture!("paper/R365MFC/Lemmon-JCED-2016-365227.7.check.csv"),
    fixture!("paper/R365MFC/Lemmon-JCED-2016-365227.7.csv"),
    fixture!("paper/Tetrahydrofuran/Fiedler-IJT-2023-THF.11.check.csv"),
    fixture!("paper/Tetrahydrofuran/Fiedler-IJT-2023-THF.11.csv"),
    fixture!("paper/VinylChloride/Thol-IJT-2022-VinylChloride.5.check.csv"),
    fixture!("paper/VinylChloride/Thol-IJT-2022-VinylChloride.5.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.6.check.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.6.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.7.check.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.7.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.8.check.csv"),
    fixture!("paper/Water/IAPWS-R6-95-2018.8.csv"),
    fixture!("paper/n-Perfluorobutane/Gao-2022-CxFy.14.check.csv"),
    fixture!("paper/n-Perfluorobutane/Gao-2022-CxFy.14.csv"),
    fixture!("paper/n-Perfluorohexane/Gao-2022-CxFy.14.check.csv"),
    fixture!("paper/n-Perfluorohexane/Gao-2022-CxFy.14.csv"),
    fixture!("paper/n-Perfluoropentane/Gao-2022-CxFy.14.check.csv"),
    fixture!("paper/n-Perfluoropentane/Gao-2022-CxFy.14.csv"),
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
