//! `cargo xtask gates abi` (PLAN.md M10.4; VERIFICATION.md §11.2; ROT-149, ROT-159): the C ABI's committed header is the
//! pinned cbindgen CLI's output for `phasekit-capi`, it compiles as C99 with `-pedantic-errors -Wall -Werror`, and the
//! built library exports `pk_*` symbols and nothing else; the Tier A shim's build (`coolproplib-shim`, D11) exports
//! exactly CoolProp's Tier A names; and every `extern "C"` function runs its body under a catch, so that a panic never
//! unwinds into C (ROT-150; PLAN.md M13.14). MSVC's compile is `c_smoke` on the Windows runner.

use syn::visit::{self, Visit};
use syn::{Block, Expr, ExprCall, Item, ItemFn, Stmt};

use super::{Verdict, no_args};
use crate::repo::Repo;

const HEADER: &str = "crates/phasekit-capi/include/phasekit.h";
const CONFIG: &str = "crates/phasekit-capi/cbindgen.toml";

/// The sources of the C ABI's `extern "C"` functions: the `pk_*` exports and the Tier A shim's.
const EXPORTS: [&str; 2] = ["crates/phasekit-capi/src/lib.rs", "crates/phasekit-capi/src/shim/mod.rs"];

/// The exports that may run without a catch, each with its reason.
const UNCAUGHT: [(&str, &str); 4] = [
    (
        "pk_last_error",
        "copies the thread's last error into the caller's buffer, and nothing in it panics short of an allocation \
         failure, which aborts",
    ),
    ("pk_last_error_code", "copies the thread's last code into the caller's buffer, as pk_last_error does"),
    ("pk_last_error_rule", "copies the thread's last rule into the caller's buffer, as pk_last_error does"),
    ("pk_last_error_remedies", "copies the thread's last remedies into the caller's buffer, as pk_last_error does"),
];

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
    let mut caught = 0;
    for path in EXPORTS {
        caught += check_caught(path, &repo.read(path).map_err(|e| vec![e])?)?;
    }
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
    let exports =
        check_exports(&repo.dynamic_symbols(&target.join("debug/libphasekit_capi.so")).map_err(|e| vec![e])?)?;
    // Its own target directory: a cdylib's file name carries no feature hash, so the shim's would replace the default's.
    let shim = target.join("shim").to_string_lossy().into_owned();
    let build = ["build", "-p", "phasekit-capi", "--features", "coolproplib-shim", "--target-dir", &shim];
    if !repo.cargo(&build).map_err(|e| vec![e])?.0 {
        return Err(vec!["cargo build -p phasekit-capi --features coolproplib-shim failed".into()]);
    }
    check_shim_exports(&repo.dynamic_symbols(&target.join("shim/debug/libphasekit_capi.so")).map_err(|e| vec![e])?)?;
    Ok(format!(
        "{HEADER} is cbindgen's and strict C99; the library exports {exports} pk_* symbols and nothing else, the shim's \
         build the {} Tier A names; {caught} extern \"C\" functions run under a catch, besides {}",
        TIER_A.len(),
        UNCAUGHT.map(|(name, _)| name).join(", ")
    ))
}

/// ROT-150: every `extern "C"` function of the source at `path` is one call to a catching function, so that a panic in
/// its body never unwinds into C: a function of the source that calls `catch_unwind` (`guard`, `run`), or one whose body
/// is in turn one call to such a function (the shim's `set_config`). [`UNCAUGHT`] names the exceptions. How many are
/// caught, or each that is not.
fn check_caught(path: &str, source: &str) -> Result<usize, Vec<String>> {
    let file = syn::parse_file(source).map_err(|e| vec![format!("{path} does not parse: {e}")])?;
    let fns: Vec<&ItemFn> =
        file.items.iter().filter_map(|item| if let Item::Fn(f) = item { Some(f) } else { None }).collect();
    let mut catching: Vec<String> =
        fns.iter().filter(|f| calls_catch_unwind(f)).map(|f| f.sig.ident.to_string()).collect();
    // A chain of helpers is at most as long as there are helpers, so as many passes reach its end.
    let helpers: Vec<&ItemFn> = fns.iter().copied().filter(|f| f.sig.abi.is_none()).collect();
    for _ in 0..helpers.len() {
        let more: Vec<String> = helpers
            .iter()
            .filter(|f| one_call(&f.block).is_some_and(|c| catching.contains(&c)))
            .map(|f| f.sig.ident.to_string())
            .filter(|name| !catching.contains(name))
            .collect();
        catching.extend(more);
    }
    let exports = fns.iter().filter(|f| f.sig.abi.is_some()).map(|f| (f.sig.ident.to_string(), one_call(&f.block)));
    let exports: Vec<(String, Option<String>)> =
        exports.filter(|(name, _)| !UNCAUGHT.iter().any(|(u, _)| u == name)).collect();
    let errors: Vec<String> = exports
        .iter()
        .filter(|(_, call)| !call.as_ref().is_some_and(|c| catching.contains(c)))
        .map(|(name, _)| format!("{path}: `{name}` does not run its body under a catch (ROT-150)"))
        .collect();
    if errors.is_empty() { Ok(exports.len()) } else { Err(errors) }
}

/// Whether `f`'s body calls `catch_unwind`.
fn calls_catch_unwind(f: &ItemFn) -> bool {
    struct Finds(bool);
    impl<'ast> Visit<'ast> for Finds {
        fn visit_expr_call(&mut self, call: &'ast ExprCall) {
            if let Expr::Path(path) = &*call.func {
                self.0 |= path.path.segments.last().is_some_and(|s| s.ident == "catch_unwind");
            }
            visit::visit_expr_call(self, call);
        }
    }
    let mut finds = Finds(false);
    finds.visit_block(&f.block);
    finds.0
}

/// The function a block's one statement calls, `f(...)` with or without a semicolon, or inside `unsafe { }`.
fn one_call(block: &Block) -> Option<String> {
    let [Stmt::Expr(expr, _)] = &block.stmts[..] else { return None };
    match expr {
        Expr::Call(call) => match &*call.func {
            Expr::Path(path) => path.path.get_ident().map(ToString::to_string),
            _ => None,
        },
        Expr::Unsafe(inner) => one_call(&inner.block),
        _ => None,
    }
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

    /// ROT-150: an export is one call to a function that calls `catch_unwind`, or to a helper that is one such call in
    /// turn, inside `unsafe { }` or not; one that runs its body otherwise is named, unless [`UNCAUGHT`] excuses it, and a
    /// function named `guard` that catches nothing does not count.
    #[test]
    fn every_export_runs_under_a_catch() {
        let catching = "fn guard(b: impl FnOnce()) -> i32 { match std::panic::catch_unwind(b) { _ => 0 } }\n";
        let helper = "fn set(k: i32) { guard(|| ()); }\n";
        let ok = "pub extern \"C\" fn a() -> i32 { guard(|| ()) }\n\
                  pub unsafe extern \"C\" fn b() { unsafe { set(1) }; }\n\
                  pub extern \"C\" fn pk_last_error() -> usize { let n = 1; n }\n";
        assert_eq!(check_caught("x.rs", &format!("{catching}{helper}{ok}")), Ok(2));
        let bare = "pub extern \"C\" fn c() -> i32 { let x = 1; guard(|| ()) }\npub extern \"C\" fn d() -> i32 { 0 }\n";
        let errors = check_caught("x.rs", &format!("{catching}{bare}")).unwrap_err();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].contains("`c`") && errors[1].contains("`d`"), "{errors:?}");
        let fake = "fn guard(b: impl FnOnce()) -> i32 { b(); 0 }\npub extern \"C\" fn e() -> i32 { guard(|| ()) }\n";
        assert!(check_caught("x.rs", fake).unwrap_err()[0].contains("`e`"));
        assert!(check_caught("x.rs", "fn (").is_err());
    }

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
