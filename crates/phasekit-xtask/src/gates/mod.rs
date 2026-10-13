//! `cargo xtask gates <gate>` and `cargo xtask gates all` (gate G8; the rules are VERIFICATION.md §11.2). Each gate
//! is a pure check over text, tested in its module, plus a `run` that gathers that text from the repository. Every
//! gate fails closed: no input to check is a failure, never a pass (ROT-127).

mod assertions;
mod complexity;
mod counts;
mod datagen;
mod deps;
mod doc_excerpts;
mod features;
mod fixtures;
mod ignores;
mod lints;
mod mutants;
mod perf;
mod register;
mod rot;

use std::process::ExitCode;

use crate::repo::Repo;

/// A gate's verdict: a one-line summary when it passes, the violations when it fails.
type Verdict = Result<String, Vec<String>>;

/// A gate: checks the repository, given its own command-line arguments.
type Gate = fn(&Repo, &[String]) -> Verdict;

/// The gates in force, in the order `gates all` runs them (VERIFICATION.md §11.2).
const GATES: [(&str, Gate); 14] = [
    ("deps", deps::run),
    ("lints", lints::run),
    ("counts", counts::run),
    ("ignores", ignores::run),
    ("doc-excerpts", doc_excerpts::run),
    ("rot", rot::run),
    ("fixtures", fixtures::run),
    ("register", register::run),
    ("datagen", datagen::run),
    ("features", features::run),
    ("assertions", assertions::run),
    ("complexity", complexity::run),
    ("perf", perf::run),
    ("mutants", mutants::run),
];

pub fn main(args: &[String]) -> ExitCode {
    let repo = Repo::locate();
    let Some(selected) = select(args) else { return usage() };
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

/// The gates `args` name, each with its own arguments: `all`, every gate in force; `all --except <gate>[,<gate>…]`,
/// every gate but those (CI runs `gates mutants` and `gates perf` as jobs of their own, user decision CI2, PLAN.md
/// M9.6); or one gate and its arguments. `None` for anything else, an unknown gate in the list included.
fn select(args: &[String]) -> Option<Vec<(&'static str, Gate, &[String])>> {
    let all = |except: &[&str]| {
        GATES.iter().filter(|(name, _)| !except.contains(name)).map(|&(name, gate)| (name, gate, &[][..])).collect()
    };
    match args.split_first()? {
        (name, []) if name == "all" => Some(all(&[])),
        (name, [flag, except]) if name == "all" && flag == "--except" => {
            let except: Vec<&str> = except.split(',').collect();
            except.iter().all(|e| GATES.iter().any(|(gate, _)| gate == e)).then(|| all(&except))
        }
        (name, rest) => GATES.iter().find(|(gate, _)| gate == name).map(|&(name, gate)| vec![(name, gate, rest)]),
    }
}

fn usage() -> ExitCode {
    let names: Vec<&str> = GATES.iter().map(|(name, _)| *name).collect();
    eprintln!(
        "usage: cargo xtask gates <all [--except <gate>[,<gate>…]]|{}> (counts also takes --update, mutants --shard \
         <k>/<n>, perf --update or --targets <results.csv>)",
        names.join("|")
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The gates a command line selects, by name.
    fn names(args: &[&str]) -> Option<Vec<&'static str>> {
        let args: Vec<String> = args.iter().map(ToString::to_string).collect();
        select(&args).map(|gates| gates.iter().map(|(name, ..)| *name).collect())
    }

    /// `all` is every gate in force, in order; `all --except <gate>[,<gate>…]` all but those (CI runs `gates mutants`
    /// and `gates perf` as jobs of their own); one gate takes its own arguments; anything else is refused.
    #[test]
    fn gates_are_selected_by_name() {
        let every: Vec<&str> = GATES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names(&["all"]), Some(every.clone()));
        let but_mutants: Vec<&str> = every.iter().copied().filter(|name| *name != "mutants").collect();
        assert_eq!(names(&["all", "--except", "mutants"]), Some(but_mutants.clone()));
        let neither: Vec<&str> = but_mutants.into_iter().filter(|name| *name != "perf").collect();
        assert_eq!(names(&["all", "--except", "mutants,perf"]), Some(neither));
        assert_eq!(names(&["counts", "--update"]), Some(vec!["counts"]));
        let args = ["counts".to_string(), "--update".to_string()];
        assert_eq!(select(&args).map(|gates| gates.first().map(|(.., rest)| rest.len())), Some(Some(1)));
        let refused: [&[&str]; 6] = [
            &[],
            &["all", "--except", "nothing"],
            &["all", "--except", "mutants,nothing"],
            &["all", "mutants"],
            &["all", "--skip", "rot"],
            &["no"],
        ];
        for args in refused {
            assert_eq!(names(args), None, "{args:?}");
        }
    }
}
