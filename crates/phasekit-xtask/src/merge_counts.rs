//! `cargo xtask merge-counts <base> <ours> <theirs>`: git's merge driver for `ci/test-counts.txt` (PLAN.md M25.1; user
//! decision LM3), which `scripts/merge-counts.sh` runs. It writes the merge [`counts::merge`] makes to `<ours>` and
//! exits 0, or leaves `<ours>` as it is and exits 1 where that merge does not decide, and the script then has git merge
//! the text. It reads and writes only the paths git passes, so the root this binary was built in does not matter.

use std::path::Path;
use std::process::ExitCode;

use crate::gates::counts;
use crate::repo::Repo;

pub fn main(args: &[String]) -> ExitCode {
    let [base, ours, theirs] = args else {
        eprintln!("usage: cargo xtask merge-counts <base> <ours> <theirs> (git's %O %A %B)");
        return ExitCode::FAILURE;
    };
    let repo = Repo::locate();
    let read = |path: &String| repo.read_outside(Path::new(path));
    let merged = match (read(base), read(ours), read(theirs)) {
        (Ok(base), Ok(mine), Ok(theirs)) => counts::merge(&base, &mine, &theirs),
        (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => {
            eprintln!("merge-counts: {e}");
            return ExitCode::FAILURE;
        }
    };
    match merged.map(|text| repo.write_outside(Path::new(ours), &text)) {
        Some(Ok(())) => ExitCode::SUCCESS,
        Some(Err(e)) => {
            eprintln!("merge-counts: {e}");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("merge-counts: a minimum lowered or a line removed on one side and changed on the other");
            ExitCode::FAILURE
        }
    }
}
