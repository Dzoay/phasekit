//! `gates datagen` (VERIFICATION.md §11.2; PLAN.md M2.5): the committed `phasekit-data` files (blobs, index,
//! features) and `phasekit-mix`'s mixture tables (M13.2) are exactly what `cargo xtask datagen` generates from the
//! pinned CoolProp files, byte for byte, with no stale blob left over. The check compares in memory; it never writes
//! the tree.

use super::{Verdict, no_args};
use crate::datagen;
use crate::repo::Repo;

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let (sources, entries) = datagen::generate(repo)?;
    let mut outputs = datagen::index::outputs(&entries);
    outputs.extend(datagen::mixtures::outputs(&datagen::mixtures::load(repo, &sources)?));
    let found = datagen::differences(repo, &outputs).map_err(|e| vec![e])?;
    if found.is_empty() {
        Ok(format!("{} generated files match a fresh generation", outputs.len()))
    } else {
        Err(found.into_iter().chain(["run `cargo xtask datagen` and commit the result".to_string()]).collect())
    }
}
