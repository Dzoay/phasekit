//! `gates assertions`: every test can fail (ROT-294; user decision TQ1). Each `#[test]` function contains an
//! `assert!`, `assert_eq!` or `assert_ne!`, or is `#[should_panic]` (map 10 R3: tests that assert nothing), and no
//! `assert_eq!`/`assert_ne!` compares an expression with itself, the form clippy's `eq_op` lets through when the
//! sides are calls (`assert_eq!(f(), f())`; map 07 I11). Comments and string literals are not code: an assertion
//! there does not count, and a test written inside a string is not a test.

use super::{Verdict, no_args};
use crate::repo::Repo;

/// The lint probes break rules on purpose (`assert_eq!(x, x)` is one of them).
const PROBES: &str = "crates/phasekit-xtask/probes/";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let files: Vec<(String, String)> = repo
        .files("crates", ".rs", true)
        .map_err(|e| vec![e])?
        .into_iter()
        .filter(|(path, _)| !path.starts_with(PROBES))
        .collect();
    let tests = check(&files)?;
    Ok(format!("{tests} tests in {} files assert something", files.len()))
}

/// Checks the (path, text) files; returns how many tests they hold.
fn check(files: &[(String, String)]) -> Result<usize, Vec<String>> {
    let (mut tests, mut errors) = (0, Vec::new());
    for (path, text) in files {
        let code = code_only(text);
        let line = |offset: usize| text[..offset].matches('\n').count() + 1;
        for (at, _) in code.match_indices("#[test]") {
            tests += 1;
            let Some((name, fn_at, body)) = test_fn(&code, at) else {
                errors.push(format!("{path}:{}: a #[test] without a function body", line(at)));
                continue;
            };
            // Attributes before and after `#[test]`, back to the end of the previous item.
            let item = code[..at].rfind(['}', ';', '{']).unwrap_or(0);
            let should_panic = code[item..fn_at].contains("#[should_panic");
            if !should_panic && !["assert!(", "assert_eq!(", "assert_ne!("].iter().any(|m| body.contains(m)) {
                errors.push(format!("{path}:{}: `{name}` asserts nothing (map 10 R3)", line(fn_at)));
            }
        }
        for mac in ["assert_eq!(", "assert_ne!("] {
            for (at, _) in code.match_indices(mac) {
                let sides = arguments(&code, at + mac.len());
                if let [a, b, ..] = sides.as_slice() {
                    let (a, b) = (squash(&text[a.0..a.1]), squash(&text[b.0..b.1]));
                    if !a.is_empty() && a == b {
                        errors.push(format!("{path}:{}: `{mac}` compares `{a}` with itself (map 07 I11)", line(at)));
                    }
                }
            }
        }
    }
    if tests == 0 {
        errors.push("no tests found".to_string());
    }
    if errors.is_empty() { Ok(tests) } else { Err(errors) }
}

/// The test function after the `#[test]` at `at`: its name, where `fn` starts and its body.
fn test_fn(code: &str, at: usize) -> Option<(&str, usize, &str)> {
    let fn_at = at + code[at..].find("fn ")?;
    let name = code[fn_at + 3..].split(|c: char| !(c.is_alphanumeric() || c == '_')).next()?;
    let open = fn_at + code[fn_at..].find('{')?;
    let mut depth = 0;
    for (i, c) in code[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 1 => return Some((name, fn_at, &code[open..open + i])),
            '}' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The byte ranges of a macro's comma-separated arguments, starting just after its `(`.
fn arguments(code: &str, start: usize) -> Vec<(usize, usize)> {
    let (mut depth, mut from, mut ranges) = (0, start, Vec::new());
    for (i, c) in code[start..].char_indices() {
        let i = start + i;
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth > 0 => depth -= 1,
            ')' => {
                ranges.push((from, i));
                return ranges;
            }
            ',' if depth == 0 => {
                ranges.push((from, i));
                from = i + 1;
            }
            _ => {}
        }
    }
    Vec::new()
}

fn squash(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `text` with comments and the contents of string and character literals blanked to spaces, byte for byte (so
/// offsets and line numbers still match `text`); the quotes stay.
fn code_only(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let mut blank = |from: usize, to: usize| {
        for byte in &mut out[from..to.min(bytes.len())] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    };
    let mut i = 0;
    while i < bytes.len() {
        let next = bytes.get(i + 1).copied();
        match bytes[i] {
            b'/' if next == Some(b'/') => {
                let end = text[i..].find('\n').map_or(bytes.len(), |n| i + n);
                blank(i, end);
                i = end;
            }
            b'/' if next == Some(b'*') => {
                let (mut depth, mut j) = (0, i);
                while j < bytes.len() {
                    match (bytes[j], bytes.get(j + 1)) {
                        (b'/', Some(b'*')) => (depth, j) = (depth + 1, j + 2),
                        (b'*', Some(b'/')) if depth == 1 => break,
                        (b'*', Some(b'/')) => (depth, j) = (depth - 1, j + 2),
                        _ => j += 1,
                    }
                }
                blank(i, j + 2);
                i = j + 2;
            }
            b'"' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'"' {
                    j += if bytes[j] == b'\\' { 2 } else { 1 };
                }
                blank(i + 1, j);
                i = j + 1;
            }
            // r"...", r#"..."#, br"...", cr"...": in valid Rust an `r` before `#*"` always opens a raw string.
            b'r' => {
                let hashes = bytes[i + 1..].iter().take_while(|&&b| b == b'#').count();
                if bytes.get(i + 1 + hashes) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let close = format!("\"{}", "#".repeat(hashes));
                let start = i + 2 + hashes;
                let end = text[start..].find(&close).map_or(bytes.len(), |n| start + n);
                blank(start, end);
                i = end + close.len();
            }
            // A char literal ('x', '\n', '\u{1F600}', 'é'); anything else after `'` is a lifetime or a label.
            b'\'' => {
                let end = if next == Some(b'\\') {
                    text[i + 2..].find('\'').map(|n| i + 2 + n.max(1))
                } else {
                    text[i + 1..].chars().next().map(|c| i + 1 + c.len_utf8()).filter(|&j| bytes.get(j) == Some(&b'\''))
                };
                match end {
                    Some(end) => {
                        blank(i + 1, end);
                        i = end + 1;
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> Vec<(String, String)> {
        vec![("crates/phasekit-core/src/x.rs".to_string(), text.to_string())]
    }

    /// Rot: ROT-294. Map 10 R3: a test that asserts nothing passes whatever the code does.
    #[test]
    fn an_assertion_free_test_is_rejected() {
        assert_eq!(check(&file("#[test]\nfn adds() {\n    assert_eq!(1 + 1, 2);\n}\n")), Ok(1));
        let free =
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn runs() {\n        let _ = compute().unwrap();\n    }\n}\n";
        let errors = check(&file(free)).unwrap_err();
        assert!(errors.len() == 1 && errors[0].starts_with("crates/phasekit-core/src/x.rs:4: `runs`"), "{errors:?}");
        // An assertion in a comment or a string does not count; `#[should_panic]` does.
        let hidden = "#[test]\nfn hidden() {\n    // assert!(done);\n    let s = \"assert!(x)\";\n}\n";
        assert!(check(&file(hidden)).is_err());
        assert_eq!(check(&file("#[test]\n#[should_panic]\nfn refuses() {\n    parse(\"\");\n}\n")), Ok(1));
        // The body ends at its own closing brace, after any nested block.
        let nested = "#[test]\nfn nested() {\n    if ready() {\n        go();\n    }\n    assert!(done());\n}\n";
        assert_eq!(check(&file(nested)), Ok(1));
        let after =
            "#[test]\nfn early() {\n    if ready() {\n        go();\n    }\n}\nfn later() {\n    assert!(done());\n}\n";
        assert!(check(&file(after)).is_err(), "an assertion after the test's body is not the test's");
        // A test inside a string literal is not a test, and no tests at all is a failure.
        assert!(check(&file("const SRC: &str = \"#[test]\\nfn t() {}\";\n")).is_err());
    }

    /// Comments and literal contents become spaces, byte for byte; quotes, lifetimes and a division stay.
    #[test]
    fn code_only_blanks_comments_and_literals() {
        let source = r##"let a = "x\"y"; // c
/* b /* n */ c */ let b = r#"a "b" c"#;
let c = br"\"; let d = 'e'; let e = '\''; let f = '\u{1F600}'; let g = 'é';
fn h<'a>(x: &'a str) -> u8 { 4 / 2 }
let t = ('a','b'); let q = '"'; let k = cr"\";
/* " */ let z = 1;
"##;
        let blanked = r##"let a = "    ";     
                  let b = r#"       "#;
let c = br" "; let d = ' '; let e = '  '; let f = '         '; let g = '  ';
fn h<'a>(x: &'a str) -> u8 { 4 / 2 }
let t = (' ',' '); let q = ' '; let k = cr" ";
        let z = 1;
"##;
        assert_eq!(code_only(source), blanked);
        // Unterminated at the end of the file: blanked to the end, never a panic.
        assert_eq!(code_only("x /* y"), "x     ");
        assert_eq!(code_only("x \"ab"), "x \"  ");
    }

    /// Rot: ROT-294. Map 07 I11: expected and actual computed by the same call.
    #[test]
    fn an_assert_with_identical_sides_is_rejected() {
        let same_call = "#[test]\nfn stable() {\n    assert_eq!(model.p(t, rho), model.p( t, rho ));\n}\n";
        let errors = check(&file(same_call)).unwrap_err();
        assert!(errors.len() == 1 && errors[0].starts_with("crates/phasekit-core/src/x.rs:3:"), "{errors:?}");
        assert!(check(&file("#[test]\nfn f() {\n    assert_ne!(a.b(), a.b(), \"msg\");\n}\n")).is_err());
        assert!(check(&file("#[test]\nfn f() {\n    assert_eq!(\"ab\", \"ab\");\n}\n")).is_err());
        // Different sides pass, also with commas inside calls and a message after them.
        let fine =
            "#[test]\nfn f() {\n    assert_eq!(g(1, 2), h(1, 2), \"{}\", 3);\n    assert_eq!(\"ab\", \"cd\");\n}\n";
        assert_eq!(check(&file(fine)), Ok(1));
    }
}
