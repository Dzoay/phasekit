//! `cargo xtask gates abi` (PLAN.md M10.4; VERIFICATION.md §11.2; ROT-149, ROT-159): the C ABI's committed header is the
//! pinned cbindgen CLI's output for `phasekit-capi`, it compiles as C99 with `-pedantic-errors -Wall -Werror`, and the
//! built library exports `pk_*` symbols and nothing else. MSVC's compile is `c_smoke` on the Windows runner.

use super::{Verdict, no_args};
use crate::repo::Repo;

const HEADER: &str = "crates/phasekit-capi/include/phasekit.h";
const CONFIG: &str = "crates/phasekit-capi/cbindgen.toml";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let target = repo.target_dir();
    let generated = target.join("phasekit-abi.h");
    let out = generated.to_string_lossy().into_owned();
    let cbindgen =
        ["--config", CONFIG, "--crate", "phasekit-capi", "--output", &out, "--quiet", "crates/phasekit-capi"];
    let (ok, _, err) = repo.tool("cbindgen", &cbindgen).map_err(|e| vec![e])?;
    if !ok {
        return Err(vec![format!("cbindgen failed: {}", err.trim())]);
    }
    let committed = repo.read(HEADER).map_err(|e| vec![e])?;
    check_header(&committed, &repo.read_outside(&generated).map_err(|e| vec![e])?)?;
    let probe = target.join("phasekit-abi.c");
    repo.write_outside(&probe, "#include \"phasekit.h\"\nint main(void) { return PK_OK; }\n").map_err(|e| vec![e])?;
    let include = repo.root().join("crates/phasekit-capi/include").to_string_lossy().into_owned();
    let (probe, object) = (probe.to_string_lossy().into_owned(), target.join("phasekit-abi.o"));
    let object = object.to_string_lossy().into_owned();
    let strict = ["-std=c99", "-pedantic-errors", "-Wall", "-Werror", "-I", &include, "-c", &probe, "-o", &object];
    let (ok, _, err) = repo.tool("cc", &strict).map_err(|e| vec![e])?;
    if !ok {
        return Err(vec![format!("{HEADER} does not compile as strict C99: {}", err.trim())]);
    }
    let (ok, _) = repo.cargo(&["build", "-p", "phasekit-capi"]).map_err(|e| vec![e])?;
    if !ok {
        return Err(vec!["cargo build -p phasekit-capi failed".into()]);
    }
    let library = target.join("debug/libphasekit_capi.so").to_string_lossy().into_owned();
    let (ok, listing, err) = repo.tool("nm", &["-D", "--defined-only", &library]).map_err(|e| vec![e])?;
    if !ok {
        return Err(vec![format!("nm {library} failed: {}", err.trim())]);
    }
    let exports = check_exports(&listing)?;
    Ok(format!("{HEADER} is cbindgen's and strict C99; the library exports {exports} pk_* symbols and nothing else"))
}

/// The committed header equals cbindgen's: the first differing line otherwise.
fn check_header(committed: &str, generated: &str) -> Result<(), Vec<String>> {
    if committed == generated {
        return Ok(());
    }
    let line = committed.lines().zip(generated.lines()).position(|(a, b)| a != b);
    let line = line.unwrap_or_else(|| committed.lines().count().min(generated.lines().count()));
    Err(vec![format!(
        "{HEADER} differs from cbindgen's output from line {}: regenerate it (cbindgen --config {CONFIG})",
        line + 1
    )])
}

/// The defined dynamic symbols of an `nm -D --defined-only` listing ("address type name") all start with `pk_`, and
/// there is one at least: how many.
fn check_exports(listing: &str) -> Result<usize, Vec<String>> {
    let names: Vec<&str> = listing.lines().filter_map(|line| line.split_whitespace().nth(2)).collect();
    let others: Vec<String> = names.iter().filter(|n| !n.starts_with("pk_")).map(|n| format!("exports {n}")).collect();
    match (names.is_empty(), others.is_empty()) {
        (true, _) => Err(vec!["the library exports no symbol".into()]),
        (false, true) => Ok(names.len()),
        (false, false) => Err(others),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_is_cbindgens() {
        assert_eq!(check_header("a\nb\n", "a\nb\n"), Ok(()));
        let differs = check_header("a\nb\nc\n", "a\nx\nc\n").unwrap_err();
        assert!(differs[0].contains("from line 2"), "{differs:?}");
        let shorter = check_header("a\nb\n", "a\nb\nc\n").unwrap_err();
        assert!(shorter[0].contains("from line 3"), "{shorter:?}");
    }

    #[test]
    fn the_library_exports_pk_symbols_only() {
        let listing = "0000000000012340 T pk_props_si\n0000000000012350 T pk_last_error\n";
        assert_eq!(check_exports(listing), Ok(2));
        let leaked = check_exports(&format!("{listing}0000000000012360 T PropsSI\n")).unwrap_err();
        assert_eq!(leaked, vec!["exports PropsSI".to_string()]);
        assert_eq!(check_exports("").unwrap_err(), vec!["the library exports no symbol".to_string()]);
        assert_eq!(
            check_exports("                 w __cxa_finalize\n"),
            Err(vec!["the library exports no symbol".into()])
        );
    }
}
