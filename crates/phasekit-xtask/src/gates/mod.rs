//! `cargo xtask gates <gate>` and `cargo xtask gates all` (gate G8; the rules are VERIFICATION.md §11.2). Each gate
//! is a pure check over text, tested in its module, plus a `run` that gathers that text from the repository. Every
//! gate fails closed: no input to check is a failure, never a pass (ROT-127).

mod assertions;
mod counts;
mod deps;
mod doc_excerpts;
mod fixtures;
mod ignores;
mod lints;
mod mutants;
mod rot;

use std::process::ExitCode;

use crate::repo::Repo;

/// A gate's verdict: a one-line summary when it passes, the violations when it fails.
type Verdict = Result<String, Vec<String>>;

/// A gate: checks the repository, given its own command-line arguments.
type Gate = fn(&Repo, &[String]) -> Verdict;

/// The gates in force, in the order `gates all` runs them (VERIFICATION.md §11.2).
const GATES: [(&str, Gate); 9] = [
    ("deps", deps::run),
    ("lints", lints::run),
    ("counts", counts::run),
    ("ignores", ignores::run),
    ("doc-excerpts", doc_excerpts::run),
    ("rot", rot::run),
    ("fixtures", fixtures::run),
    ("assertions", assertions::run),
    ("mutants", mutants::run),
];

pub fn main(args: &[String]) -> ExitCode {
    let repo = Repo::locate();
    let selected: Vec<_> = match args.split_first() {
        Some((name, [])) if name == "all" => GATES.iter().map(|&(name, gate)| (name, gate, &[][..])).collect(),
        Some((name, rest)) => match GATES.iter().find(|(gate, _)| gate == name) {
            Some(&(name, gate)) => vec![(name, gate, rest)],
            None => return usage(),
        },
        None => return usage(),
    };
    let mut failed = false;
    for (name, gate, rest) in selected {
        match gate(&repo, rest) {
            Ok(summary) => {
                let mut lines = summary.lines();
                println!("gates {name}: ok ({})", lines.next().unwrap_or_default());
                lines.for_each(|line| println!("{line}"));
            }
            Err(violations) => {
                failed = true;
                eprintln!("gates {name}: FAILED");
                for violation in violations {
                    eprintln!("  {violation}");
                }
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

fn usage() -> ExitCode {
    let names: Vec<&str> = GATES.iter().map(|(name, _)| *name).collect();
    eprintln!("usage: cargo xtask gates <all|{}> (counts also takes --update)", names.join("|"));
    ExitCode::FAILURE
}

/// Refuses arguments a gate does not take.
fn no_args(args: &[String]) -> Result<(), Vec<String>> {
    if args.is_empty() { Ok(()) } else { Err(vec![format!("unexpected arguments: {}", args.join(" "))]) }
}

/// The lines of a TOML table (`[name]`) up to the next table header, or `None` without one. Enough for the
/// workspace's own hand-written manifests; xtask stays std-only until M2.1.
fn table<'a>(manifest: &'a str, name: &str) -> Option<Vec<&'a str>> {
    let header = format!("[{name}]");
    let mut lines = manifest.lines().skip_while(|line| line.trim() != header);
    lines.next()?;
    Some(lines.take_while(|line| !line.trim_start().starts_with('[')).collect())
}

/// The string value of `key = "value"` among a table's lines.
fn value<'a>(lines: &[&'a str], key: &str) -> Option<&'a str> {
    lines.iter().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        let v = v.split('#').next()?.trim();
        (k.trim() == key).then(|| v.trim_matches('"'))
    })
}

/// A manifest's `[package] name`.
fn package_name(manifest: &str) -> Option<&str> {
    value(&table(manifest, "package")?, "name")
}
