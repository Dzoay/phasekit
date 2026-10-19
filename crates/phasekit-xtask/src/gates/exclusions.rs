//! The mutants cargo-mutants leaves out (PLAN.md M25.4; user decision LM2). An equivalent mutant, one no test can
//! catch, is excluded where it is: a line comment `// equivalent mutant: <mutation>` ends the comment that says why,
//! and names a mutation as cargo-mutants describes it up to " in <function>" (`replace + with *`) on the next line
//! that is not a comment; `#k` after the mutation picks the k-th such mutant on that line by column. Each marker
//! resolves, from cargo-mutants' own list, to the full names of the mutants it means, which join
//! `.cargo/mutants.toml`'s `exclude_re` in the configuration `gates mutants` and the weekly run pass with `--config`.
//! An edit elsewhere moves no marker. A marker with no reason above it or matching no mutant fails, and so does a
//! pattern the toml keeps that matches no mutant.

/// The words a marker starts with.
const MARKER: &str = "// equivalent mutant: ";

pub const TOML: &str = ".cargo/mutants.toml";

/// A mutant as `cargo mutants --list --json` lists it: its full name (`<file>:<line>:<col>: <description>`), file,
/// line, column and mutation (the description without " in <function>").
#[derive(Debug, PartialEq)]
pub struct Listed {
    pub name: String,
    file: String,
    line: usize,
    column: usize,
    mutation: String,
}

/// A marker: the file and line it is on, the line it marks, the mutation it names and which of that line's mutants,
/// if not all of them.
#[derive(Debug, PartialEq)]
pub struct Marker<'a> {
    file: &'a str,
    line: usize,
    target: usize,
    mutation: &'a str,
    nth: Option<usize>,
}

/// The configuration cargo-mutants runs with, and how many markers and mutants it excludes by them: the toml and
/// each marker's mutants, after checking every marker in `sources` and every pattern the toml keeps against
/// cargo-mutants' list of the tree's mutants, which `list` gives for its extra arguments (`--re <pattern>`).
pub fn generate(
    toml: &str,
    sources: &[(String, String)],
    list: impl Fn(&[&str]) -> Result<String, String>,
) -> Result<(String, usize, usize), Vec<String>> {
    let all = listed(&list(&[]).map_err(|e| vec![e])?).map_err(|e| vec![e])?;
    let (mut found, mut errors) = (Vec::new(), Vec::new());
    for (path, text) in sources {
        match markers(path, text) {
            Ok(marked) => found.extend(marked),
            Err(e) => errors.extend(e),
        }
    }
    let names = resolve(&found, &all).unwrap_or_else(|e| {
        errors.extend(e);
        Vec::new()
    });
    for pattern in kept(toml).map_err(|e| vec![e])? {
        let matched = listed(&list(&["--re", &pattern]).map_err(|e| vec![e])?).map_err(|e| vec![e])?;
        if matched.is_empty() {
            errors.push(format!("{TOML}: the exclude_re pattern `{pattern}` matches no mutant"));
        }
    }
    if errors.is_empty() { Ok((config(toml, &names), found.len(), names.len())) } else { Err(errors) }
}

/// The mutants of `cargo mutants --list --json`'s output.
pub fn listed(json: &str) -> Result<Vec<Listed>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("cargo mutants --list --json: {e}"))?;
    let items = value.as_array().ok_or("cargo mutants --list --json: not an array")?;
    items.iter().map(mutant).collect()
}

/// One mutant of the JSON listing.
fn mutant(item: &serde_json::Value) -> Result<Listed, String> {
    let text = |key: &str| item.get(key).and_then(serde_json::Value::as_str);
    let start = item.get("span").and_then(|span| span.get("start"));
    let at = |key: &str| start.and_then(|s| s.get(key)).and_then(serde_json::Value::as_u64)?.try_into().ok();
    let (Some(name), Some(file), Some(line), Some(column)) = (text("name"), text("file"), at("line"), at("column"))
    else {
        return Err(format!("cargo mutants --list --json: a mutant without a name, file or span: {item}"));
    };
    let description = name
        .strip_prefix(&format!("{file}:{line}:{column}: "))
        .ok_or(format!("cargo mutants --list --json: `{name}` is not named for {file}:{line}:{column}"))?;
    let function = item.get("function").and_then(|f| f.get("function_name")).and_then(serde_json::Value::as_str);
    let mutation = function.and_then(|f| description.strip_suffix(&format!(" in {f}"))).unwrap_or(description);
    Ok(Listed { name: name.to_string(), file: file.to_string(), line, column, mutation: mutation.to_string() })
}

/// The markers of one source file, or why one is not a marker: no comment above it, no mutation, a `#k` that is not
/// a count from 1, or no line after it.
pub fn markers<'a>(file: &'a str, text: &'a str) -> Result<Vec<Marker<'a>>, Vec<String>> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let is_marker = |line: &&str| line.starts_with(MARKER.trim_end());
    let (mut found, mut errors) = (Vec::new(), Vec::new());
    for (i, rest) in lines.iter().enumerate().filter_map(|(i, line)| Some((i, line.strip_prefix(MARKER.trim_end())?))) {
        let line = i + 1;
        let reason = lines.iter().take(i).rev().find(|above| !is_marker(above));
        let target = lines.iter().enumerate().skip(line).find(|(_, below)| !below.starts_with("//"));
        let (mutation, nth) = match rest.trim().rsplit_once(" #") {
            Some((mutation, k)) => (mutation, Some(k.parse::<usize>().ok().filter(|k| *k > 0))),
            None => (rest.trim(), None),
        };
        let problem = if !reason.is_some_and(|above| above.starts_with("//")) {
            Some("a marker with no comment above it to say why")
        } else if mutation.is_empty() {
            Some("a marker that names no mutation")
        } else if nth == Some(None) {
            Some("a marker's `#k` counts the line's mutants from 1")
        } else {
            None
        };
        match (problem, target) {
            (Some(problem), _) => errors.push(format!("{file}:{line}: {problem}")),
            (None, Some((j, below))) if !below.is_empty() => {
                found.push(Marker { file, line, target: j + 1, mutation, nth: nth.flatten() });
            }
            (None, _) => errors.push(format!("{file}:{line}: a marker with no line of code after it")),
        }
    }
    if errors.is_empty() { Ok(found) } else { Err(errors) }
}

/// The full names of the mutants the markers mean, or each marker that means none.
pub fn resolve(markers: &[Marker<'_>], listed: &[Listed]) -> Result<Vec<String>, Vec<String>> {
    let (mut names, mut errors) = (Vec::new(), Vec::new());
    for marker in markers {
        let mut hits: Vec<&Listed> = listed
            .iter()
            .filter(|m| m.file == marker.file && m.line == marker.target && m.mutation == marker.mutation)
            .collect();
        hits.sort_by_key(|m| m.column);
        if let Some(k) = marker.nth {
            hits = k.checked_sub(1).and_then(|i| hits.get(i).copied()).into_iter().collect();
        }
        if hits.is_empty() {
            let which = marker.nth.map(|k| format!(" #{k}")).unwrap_or_default();
            errors.push(format!(
                "{}:{}: `{}{which}` matches no mutant on line {}",
                marker.file, marker.line, marker.mutation, marker.target
            ));
        }
        names.extend(hits.into_iter().map(|m| m.name.clone()));
    }
    if errors.is_empty() { Ok(names) } else { Err(errors) }
}

/// The patterns of the toml's `exclude_re`, each unescaped from its TOML string, one a line.
pub fn kept(toml: &str) -> Result<Vec<String>, String> {
    let mut lines = toml.lines().map(str::trim).skip_while(|line| *line != "exclude_re = [");
    if lines.next().is_none() {
        return Ok(Vec::new());
    }
    let mut patterns = Vec::new();
    for line in lines {
        if line == "]" {
            return Ok(patterns);
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let quoted = line.strip_suffix(',').unwrap_or(line);
        let inner = quoted.strip_prefix('"').and_then(|q| q.strip_suffix('"'));
        patterns.push(
            inner
                .and_then(unescape)
                .ok_or(format!("{TOML}: `{line}`: each exclude_re entry is one basic string on a line of its own"))?,
        );
    }
    Err(format!("{TOML}: exclude_re has no closing `]`"))
}

/// A TOML basic string's text, with `\\` and `\"` its only escapes.
fn unescape(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next().filter(|next| matches!(next, '\\' | '"'))?),
            '"' => return None,
            c => out.push(c),
        }
    }
    Some(out)
}

/// The toml with one `exclude_re` pattern per name, matching that name alone.
pub fn config(toml: &str, names: &[String]) -> String {
    let entries: String = names.iter().map(|name| format!("    \"{}\",\n", exact(name))).collect();
    const HEAD: &str = "exclude_re = [\n";
    match toml.find(HEAD).and_then(|at| toml.split_at_checked(at + HEAD.len())) {
        Some((before, after)) => format!("{before}{entries}{after}"),
        None => format!("{toml}\n{HEAD}{entries}]\n"),
    }
}

/// A regex matching `name` alone, as a TOML basic string's text: the regex crate's metacharacters escaped (its
/// `regex::escape`), then TOML's backslashes and quotes.
fn exact(name: &str) -> String {
    let mut pattern = String::from("^");
    for c in name.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('$');
    pattern.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two functions: `f` with a marked line, `g` with the same mutation unmarked.
    fn source(marker: &str) -> String {
        format!(
            "fn f(a: f64, b: f64) -> bool {{\n    // `<=` differs only at a tie.\n    {marker}\n    a < b && b < 2.0\n}}\n\nfn g(a: f64, b: f64) -> bool {{\n    a < b\n}}\n"
        )
    }

    /// One listed mutant of `f.rs`.
    fn mutant(line: usize, column: usize, mutation: &str, function: &str) -> Listed {
        let name = format!("f.rs:{line}:{column}: {mutation} in {function}");
        Listed { name, file: "f.rs".to_string(), line, column, mutation: mutation.to_string() }
    }

    /// The listing with `f`'s marked line at `line` and `g`'s four lines below it.
    fn listing(line: usize) -> Vec<Listed> {
        vec![
            mutant(line, 7, "replace < with <=", "f"),
            mutant(line, 11, "replace && with ||", "f"),
            mutant(line, 16, "replace < with <=", "f"),
            mutant(line + 4, 7, "replace < with <=", "g"),
        ]
    }

    fn names(listed: &[Listed], which: &[usize]) -> Vec<String> {
        which.iter().map(|&i| listed[i].name.clone()).collect()
    }

    /// Rot: ROT-294's exclusions, kept by meaning (user decision LM2). A marker excludes its line's mutants of the
    /// mutation it names, or the k-th of them, and not the same mutation on another line.
    #[test]
    fn a_marker_excludes_its_lines_mutant_only() {
        let text = source(&format!("{MARKER}replace < with <="));
        let found = markers("f.rs", &text).unwrap();
        let want = Marker { file: "f.rs", line: 3, target: 4, mutation: "replace < with <=", nth: None };
        assert_eq!(found, [want]);
        assert_eq!(resolve(&found, &listing(4)), Ok(names(&listing(4), &[0, 2])));
        let second = source(&format!("{MARKER}replace < with <= #2"));
        assert_eq!(resolve(&markers("f.rs", &second).unwrap(), &listing(4)), Ok(names(&listing(4), &[2])));
        // Markers stack under one reason, and a comment between a marker and its line is skipped.
        let stacked = source(&format!("{MARKER}replace < with <= #1\n    {MARKER}replace && with ||\n    // and more"));
        assert_eq!(resolve(&markers("f.rs", &stacked).unwrap(), &listing(6)), Ok(names(&listing(6), &[0, 1])));
        assert_eq!(markers("f.rs", &source("// no marker")), Ok(vec![]));
    }

    /// A marker that matches nothing, or the k-th of fewer, or has no reason above it or no line after it, fails and
    /// says where.
    #[test]
    fn a_marker_that_matches_no_mutant_fails() {
        let stale = source(&format!("{MARKER}replace > with >="));
        let errors = resolve(&markers("f.rs", &stale).unwrap(), &listing(4)).unwrap_err();
        assert_eq!(errors, ["f.rs:3: `replace > with >=` matches no mutant on line 4"]);
        let third = source(&format!("{MARKER}replace < with <= #3"));
        assert_eq!(resolve(&markers("f.rs", &third).unwrap(), &listing(4)).unwrap_err().len(), 1);
        let marked = source(&format!("{MARKER}replace < with <="));
        let other_file = markers("g.rs", &marked).unwrap();
        assert!(resolve(&other_file, &listing(4)).is_err(), "the same line of another file");
        for bad in ["#0", "#x", "#"] {
            let text = source(&format!("{MARKER}replace < with <= {bad}"));
            assert!(markers("f.rs", &text).is_err(), "{bad}");
        }
        let unreasoned = format!("fn f(a: f64, b: f64) -> bool {{\n    {MARKER}replace < with <=\n    a < b\n}}\n");
        assert_eq!(
            markers("f.rs", &unreasoned),
            Err(vec!["f.rs:2: a marker with no comment above it to say why".to_string()])
        );
        let last = format!("// why\n{MARKER}replace < with <=\n// nothing after\n");
        assert_eq!(markers("f.rs", &last).unwrap_err().len(), 1, "no line to mark");
        let empty = format!("// why\n{MARKER}\nfn f() {{}}\n");
        assert_eq!(markers("f.rs", &empty).unwrap_err().len(), 1, "no mutation");
        let blank = format!("// why\n{MARKER}replace < with <=\n\nfn f() {{}}\n");
        assert_eq!(markers("f.rs", &blank).unwrap_err().len(), 1, "a blank line is no line of code");
    }

    /// The whole configuration from the toml, the sources and cargo-mutants' listing: each marker's mutants added as
    /// exact patterns and counted; a stale marker or a pattern matching no mutant fails, each named.
    #[test]
    fn the_configuration_comes_from_the_markers_and_the_toml() {
        let json = r#"[{"file": "f.rs", "function": {"function_name": "f"}, "name": "f.rs:4:7: replace < with <= in f",
            "span": {"start": {"line": 4, "column": 7}}},
            {"file": "f.rs", "function": {"function_name": "f"}, "name": "f.rs:4:16: replace < with <= in f",
            "span": {"start": {"line": 4, "column": 16}}}]"#;
        let list = |args: &[&str]| Ok(if args == ["--re", "in run$"] { "[]".to_string() } else { json.to_string() });
        let toml = "exclude_re = [\n    \"in f$\",\n]\n";
        let sources = [("f.rs".to_string(), source(&format!("{MARKER}replace < with <=")))];
        let want = "exclude_re = [\n    \"^f\\\\.rs:4:7: replace < with <= in f$\",\n    \"^f\\\\.rs:4:16: replace < with <= in f$\",\n    \"in f$\",\n]\n";
        assert_eq!(generate(toml, &sources, list), Ok((want.to_string(), 1, 2)));
        let stale = [("f.rs".to_string(), source(&format!("{MARKER}replace > with >=")))];
        assert_eq!(generate(toml, &stale, list).unwrap_err().len(), 1);
        let unmatched = "exclude_re = [\n    \"in run$\",\n]\n";
        let errors = generate(unmatched, &sources, list).unwrap_err();
        assert_eq!(errors, [format!("{TOML}: the exclude_re pattern `in run$` matches no mutant")]);
        let failing = |_: &[&str]| Err("cargo mutants is not installed".to_string());
        assert!(generate(toml, &sources, failing).is_err());
    }

    /// cargo-mutants' JSON listing: a function's mutation is its description less " in <function>"; a mutant of a
    /// whole function's value, whose description names the function first, keeps its description.
    #[test]
    fn cargo_mutants_listing_is_read() {
        let json = r#"[
            {"file": "f.rs", "function": {"function_name": "f"}, "name": "f.rs:4:7: replace < with <= in f",
             "span": {"start": {"line": 4, "column": 7}, "end": {"line": 4, "column": 8}}},
            {"file": "f.rs", "function": {"function_name": "is_empty"}, "name": "f.rs:9:5: replace is_empty -> bool with false",
             "span": {"start": {"line": 9, "column": 5}, "end": {"line": 9, "column": 9}}}
        ]"#;
        let want = vec![
            mutant(4, 7, "replace < with <=", "f"),
            Listed {
                name: "f.rs:9:5: replace is_empty -> bool with false".to_string(),
                file: "f.rs".to_string(),
                line: 9,
                column: 5,
                mutation: "replace is_empty -> bool with false".to_string(),
            },
        ];
        assert_eq!(listed(json), Ok(want));
        assert!(listed("[{\"file\": \"f.rs\"}]").is_err(), "no name");
        assert!(listed("not json").is_err());
    }

    /// The configuration is the toml with one exact pattern per name: regex metacharacters escaped, then TOML's
    /// backslashes and quotes; the toml's own patterns are read back unescaped.
    #[test]
    fn the_config_adds_one_exact_pattern_per_name() {
        let toml =
            "gitignore = true\n\nexclude_re = [\n    # Why.\n    \"in run$\",\n    \"sha256\\\\.rs:1:1: x\",\n]\n";
        assert_eq!(kept(toml), Ok(vec!["in run$".to_string(), "sha256\\.rs:1:1: x".to_string()]));
        let name = "a/b.rs:3:9: replace * with + in Search<'_>::roots (x|y) [z] {w} ^$?.\"".to_string();
        let want = "exclude_re = [\n    \"^a/b\\\\.rs:3:9: replace \\\\* with \\\\+ in Search<'_>::roots \\\\(x\\\\|y\\\\) \\\\[z\\\\] \\\\{w\\\\} \\\\^\\\\$\\\\?\\\\.\\\"$\",\n";
        let generated = config(toml, &[name]);
        assert!(generated.starts_with("gitignore = true\n\n") && generated.contains(want), "{generated}");
        assert!(generated.ends_with("    \"in run$\",\n    \"sha256\\\\.rs:1:1: x\",\n]\n"), "{generated}");
        let bare = config("gitignore = true\n", &["f.rs:1:1: x".to_string()]);
        assert_eq!(bare, "gitignore = true\n\nexclude_re = [\n    \"^f\\\\.rs:1:1: x$\",\n]\n");
        assert_eq!(kept("gitignore = true\n"), Ok(vec![]));
        assert!(kept("exclude_re = [\n    'literal',\n]\n").is_err(), "only basic strings");
        assert!(kept("exclude_re = [\n    \"unclosed,\n]\n").is_err());
    }
}
