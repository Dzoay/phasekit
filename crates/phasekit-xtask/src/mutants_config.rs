//! `cargo xtask mutants-config <path>`: writes the configuration `gates mutants` runs cargo-mutants with
//! (`gates::exclusions`; PLAN.md M25.4, user decision LM2) to `<path>`, for the weekly full run, which calls
//! cargo-mutants itself. A stale marker or pattern fails it, as it fails the gate.

use std::path::Path;
use std::process::ExitCode;

use crate::gates::exclusions;
use crate::repo::Repo;

pub fn main(args: &[String]) -> ExitCode {
    let [path] = args else {
        eprintln!("usage: cargo xtask mutants-config <path>");
        return ExitCode::FAILURE;
    };
    let repo = Repo::locate();
    let inputs = repo.read(exclusions::TOML).and_then(|toml| Ok((toml, repo.files("crates", ".rs", true)?)));
    let written = inputs
        .map_err(|e| vec![e])
        .and_then(|(toml, sources)| exclusions::generate(&toml, &sources, |args| repo.mutants_list(args)))
        .and_then(|(config, marked, excluded)| {
            repo.write_outside(Path::new(path), &config).map_err(|e| vec![e])?;
            Ok(format!("{path}: {marked} markers exclude {excluded} mutants"))
        });
    match written {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in errors {
                eprintln!("mutants-config: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
