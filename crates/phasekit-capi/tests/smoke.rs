//! The C smoke test (PLAN.md M10.4; ROT-149, ROT-150): `tests/c/smoke.c` compiled against the committed header and the
//! library cargo built beside this test, with the platform's C compiler (`cc` finds GCC, Clang or MSVC), as strict C
//! (GCC and Clang: C99 with `-pedantic-errors -Wall -Werror`; MSVC: C11 with `/W4 /WX`), then run.

#![allow(clippy::unwrap_used, reason = "test-crate helpers")]
#![cfg(not(target_family = "wasm"))]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The target triple this test runs on, as `cc` names it.
fn target() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "windows" => format!("{arch}-pc-windows-msvc"),
        "macos" => format!("{arch}-apple-darwin"),
        _ => format!("{arch}-unknown-linux-gnu"),
    }
}

/// The directory of the library: the profile's `deps/` holds this test and the `cdylib`.
fn library_dir() -> PathBuf {
    std::env::current_exe().unwrap().parent().unwrap().to_path_buf()
}

#[test]
#[allow(clippy::disallowed_methods, reason = "the test builds and runs a C program in a scratch directory")]
fn c_smoke() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (lib, out) = (library_dir(), std::env::temp_dir().join(format!("phasekit-smoke-{}", std::process::id())));
    std::fs::create_dir_all(&out).unwrap();
    let compiler = cc::Build::new().target(&target()).host(&target()).opt_level(0).cargo_metadata(false).get_compiler();
    let mut command = compiler.to_command();
    let (source, include) = (crate_dir.join("tests/c/smoke.c"), crate_dir.join("include"));
    let exe = out.join(if cfg!(windows) { "smoke.exe" } else { "smoke" });
    if compiler.is_like_msvc() {
        command.args(["/nologo", "/std:c11", "/W4", "/WX"]).arg(format!("/I{}", include.display())).arg(&source);
        command.arg(format!("/Fe{}", exe.display())).arg(format!("/Fo{}\\", out.display()));
        command.arg("/link").arg(lib.join("phasekit_capi.dll.lib"));
    } else {
        command.args(["-std=c99", "-pedantic-errors", "-Wall", "-Werror"]).arg("-I").arg(&include).arg(&source);
        command.arg("-o").arg(&exe).arg(format!("-L{}", lib.display())).args(["-lphasekit_capi", "-lm"]);
        command.arg(format!("-Wl,-rpath,{}", lib.display()));
    }
    let built = command.output().unwrap();
    let report = |o: &std::process::Output| {
        format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
    };
    assert!(built.status.success(), "{command:?}: {}", report(&built));
    let mut run = Command::new(&exe);
    if cfg!(windows) {
        let path = std::env::var_os("PATH").unwrap_or_default();
        run.env(
            "PATH",
            std::env::join_paths(std::iter::once(lib.clone()).chain(std::env::split_paths(&path))).unwrap(),
        );
    }
    let ran = run.output().unwrap();
    assert!(ran.status.success() && String::from_utf8_lossy(&ran.stdout).trim() == "ok", "{}", report(&ran));
    std::fs::remove_dir_all(&out).ok();
}
