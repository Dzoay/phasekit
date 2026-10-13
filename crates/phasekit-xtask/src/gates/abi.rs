//! `cargo xtask gates abi` (PLAN.md M10.4; VERIFICATION.md §11.2; ROT-149, ROT-159): the C ABI's committed header is the
//! pinned cbindgen CLI's output for `phasekit-capi`, it compiles as C99 with `-pedantic-errors -Wall -Werror`, and the
//! built library exports `pk_*` symbols and nothing else; the Tier A shim's build (`coolproplib-shim`, D11) exports
//! exactly CoolProp's Tier A names. MSVC's compile is `c_smoke` on the Windows runner.

use super::{Verdict, no_args};
use crate::repo::Repo;

const HEADER: &str = "crates/phasekit-capi/include/phasekit.h";
const CONFIG: &str = "crates/phasekit-capi/cbindgen.toml";

/// CoolProp's Tier A names, exactly what the shim's build exports (D11; map 11 U11; PLAN.md M10.6).
const TIER_A: [&str; 11] = [
    "HAPropsSI",
    "PhaseSI",
    "Props1SI",
    "PropsSI",
    "get_fluid_param_string",
    "get_global_param_string",
    "get_input_pair_index",
    "get_param_index",
    "set_config_bool",
    "set_config_double",
    "set_config_string",
];

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
    let exports = check_exports(&exported(repo, &target.join("debug/libphasekit_capi.so"))?)?;
    // Its own target directory: a cdylib's file name carries no feature hash, so the shim's would replace the default's.
    let shim = target.join("shim").to_string_lossy().into_owned();
    let build = ["build", "-p", "phasekit-capi", "--features", "coolproplib-shim", "--target-dir", &shim];
    if !repo.cargo(&build).map_err(|e| vec![e])?.0 {
        return Err(vec!["cargo build -p phasekit-capi --features coolproplib-shim failed".into()]);
    }
    check_shim_exports(&exported(repo, &target.join("shim/debug/libphasekit_capi.so"))?)?;
    Ok(format!(
        "{HEADER} is cbindgen's and strict C99; the library exports {exports} pk_* symbols and nothing else, the shim's \
         build the {} Tier A names",
        TIER_A.len()
    ))
}

/// The `nm -D --defined-only` listing of a built library.
fn exported(repo: &Repo, library: &std::path::Path) -> Result<String, Vec<String>> {
    let library = library.to_string_lossy().into_owned();
    let (ok, listing, err) = repo.tool("nm", &["-D", "--defined-only", &library]).map_err(|e| vec![e])?;
    if ok { Ok(listing) } else { Err(vec![format!("nm {library} failed: {}", err.trim())]) }
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

/// The defined dynamic symbols of the shim's listing are exactly [`TIER_A`]: each one missing or extra otherwise.
fn check_shim_exports(listing: &str) -> Result<(), Vec<String>> {
    let names: Vec<&str> = listing.lines().filter_map(|line| line.split_whitespace().nth(2)).collect();
    let missing = TIER_A.iter().filter(|name| !names.contains(name)).map(|name| format!("the shim lacks {name}"));
    let extra = names.iter().filter(|name| !TIER_A.contains(name)).map(|name| format!("the shim exports {name}"));
    let errors: Vec<String> = missing.chain(extra).collect();
    if errors.is_empty() { Ok(()) } else { Err(errors) }
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

    /// PLAN.md M10.6 (D11): the shim's build exports CoolProp's Tier A names, each of them and nothing else.
    #[test]
    fn shim_exports_exactly_tier_a() {
        let listing: String = TIER_A.iter().map(|name| format!("0000000000012340 T {name}\n")).collect();
        assert_eq!(check_shim_exports(&format!("{listing}                 w __cxa_finalize\n")), Ok(()));
        let short = listing.replace("0000000000012340 T HAPropsSI\n", "");
        assert_eq!(check_shim_exports(&short), Err(vec!["the shim lacks HAPropsSI".to_string()]));
        let leaked = format!("{listing}0000000000012360 T pk_props_si\n0000000000012370 T AbstractState_free\n");
        let errors =
            vec!["the shim exports pk_props_si".to_string(), "the shim exports AbstractState_free".to_string()];
        assert_eq!(check_shim_exports(&leaked), Err(errors));
    }
}
