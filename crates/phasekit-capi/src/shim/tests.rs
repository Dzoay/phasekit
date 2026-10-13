//! The shim's own tests (PLAN.md M10.6), calling its functions as a C caller does. The `errstring` and the settings are
//! process-wide, so each test holds one lock and starts from CoolProp's defaults.

#![allow(unsafe_code, reason = "the tests call the unsafe shim functions, each with valid strings and buffers")]
#![allow(clippy::disallowed_types, reason = "one lock serialises the tests of the process-wide state")]

use std::ffi::CString;
use std::sync::{Mutex, MutexGuard};

use super::*;
use crate::handles::lock;

/// The lock every test holds; the settings are CoolProp's defaults and the `errstring` is empty when it is taken.
fn serial() -> MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    let guard = lock(&SERIAL);
    state::reset();
    state::take_error();
    guard
}

/// A C string.
fn c(text: &str) -> CString {
    CString::new(text).unwrap_or_default()
}

/// `PropsSI` with Rust strings.
fn props(output: &str, (n1, v1): (&str, f64), (n2, v2): (&str, f64), fluid: &str) -> f64 {
    let (o, a, b, f) = (c(output), c(n1), c(n2), c(fluid));
    // SAFETY: every string is NUL-terminated.
    unsafe { PropsSI(o.as_ptr(), a.as_ptr(), v1, b.as_ptr(), v2, f.as_ptr()) }
}

/// `get_global_param_string` into a buffer of `n` bytes: its return and the buffer's text.
fn global(param: &str, n: usize) -> (c_long, String) {
    let (p, mut buf) = (c(param), vec![b'#' as c_char; n]);
    // SAFETY: `p` is NUL-terminated; `buf` holds `n` bytes.
    let ok = unsafe { get_global_param_string(p.as_ptr(), buf.as_mut_ptr(), c_int::try_from(n).unwrap_or(0)) };
    (ok, text(&buf))
}

/// A buffer's text up to its NUL (or all of it).
fn text(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf.iter().map(|&b| b as u8).take_while(|&b| b != 0).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The `errstring`, read and cleared.
fn errstring() -> String {
    global("errstring", 1024).1
}

/// `set_config_bool`, `set_config_double` and `set_config_string` with Rust strings.
fn set_bool(key: &str, val: bool) {
    // SAFETY: `key` is NUL-terminated.
    unsafe { set_config_bool(c(key).as_ptr(), val) };
}
fn set_double(key: &str, val: f64) {
    // SAFETY: `key` is NUL-terminated.
    unsafe { set_config_double(c(key).as_ptr(), val) };
}
fn set_string(key: &str, val: &str) {
    // SAFETY: both are NUL-terminated.
    unsafe { set_config_string(c(key).as_ptr(), c(val).as_ptr()) };
}

/// PLAN.md M10.6 (#3211; map 11 §8). The `errstring` is one slot for the process: a failure on thread A is read on
/// thread B, the last writer wins, and reading it clears it, on any thread.
#[cfg(not(target_family = "wasm"))]
#[test]
fn errstring_is_readable_from_another_thread() {
    let _serial = serial();
    std::thread::spawn(|| {
        assert_eq!(props("X", ("T", 300.0), ("P", 1e5), "Water"), f64::INFINITY);
        assert!(props("T", ("P", 1e5), ("Q", 0.0), "NoSuchFluid").is_infinite());
    })
    .join()
    .unwrap_or_default();
    let read = std::thread::spawn(errstring).join().unwrap_or_default();
    assert!(read.contains("NoSuchFluid") && !read.contains("`X`"), "the last writer wins: {read}");
    assert_eq!(errstring(), "", "reading cleared it");
    let registry = Registry::embedded().unwrap_or_else(|_| unreachable!());
    let want = phasekit_compat::props_si_in(registry, "T", "P", 101_325.0, "Q", 0.0, "Water").unwrap_or(f64::NAN);
    assert_eq!(props("T", ("P", 101_325.0), ("Q", 0.0), "Water").to_bits(), want.to_bits());
    assert_eq!(errstring(), "", "a success leaves the errstring alone");
}

/// PLAN.md M10.6 (D11). `HAPropsSI` is a stub until the humid-air milestone: `_HUGE` and the documented message.
#[test]
fn hapropssi_is_a_documented_stub() {
    let _serial = serial();
    let (w, t, p, r) = (c("W"), c("T"), c("P"), c("R"));
    let value = HAPropsSI(w.as_ptr(), t.as_ptr(), 300.0, p.as_ptr(), 101_325.0, r.as_ptr(), 0.5);
    assert_eq!((value, errstring()), (f64::INFINITY, HAPROPSSI_MESSAGE.to_string()));
}

/// PLAN.md M10.6 (map 14: PropsSI fills +inf). A failed `PropsSI` or `Props1SI` is `_HUGE` with the `errstring` set; a
/// good one is compat's value, bit for bit. `Props1SI` takes its two arguments in either order, and refuses two fluids
/// or none.
#[test]
fn propssi_fills_inf_on_error() {
    let _serial = serial();
    let registry = Registry::embedded().unwrap_or_else(|_| unreachable!());
    let want = phasekit_compat::props_si_in(registry, "Hmass", "T", 300.0, "P", 1e5, "Water").unwrap_or(f64::NAN);
    assert_eq!(props("Hmass", ("T", 300.0), ("P", 1e5), "Water").to_bits(), want.to_bits());
    for (output, t, fluid) in [("X", 300.0, "Water"), ("Hmass", -1.0, "Water"), ("Hmass", 300.0, "Water&Ethanol")] {
        assert_eq!(props(output, ("T", t), ("P", 1e5), fluid), f64::INFINITY, "{output} {t} {fluid}");
        assert!(!errstring().is_empty());
    }
    let one = |a: &str, b: &str| {
        let (a, b) = (c(a), c(b));
        // SAFETY: both are NUL-terminated.
        unsafe { Props1SI(a.as_ptr(), b.as_ptr()) }
    };
    let tcrit = phasekit_compat::props1_si_in(registry, "Tcrit", "Water").unwrap_or(f64::NAN);
    assert_eq!((one("Water", "Tcrit").to_bits(), one("Tcrit", "Water").to_bits()), (tcrit.to_bits(), tcrit.to_bits()));
    assert_eq!((one("Water", "R134a"), one("Tcrit", "Tmax")), (f64::INFINITY, f64::INFINITY));
    assert!(errstring().starts_with("neither input"));
    assert_eq!(one("Water", "T"), f64::INFINITY, "a state-dependent output");
    // SAFETY: a null string is refused before it is read.
    assert_eq!(
        unsafe { PropsSI(std::ptr::null(), c("T").as_ptr(), 1.0, c("P").as_ptr(), 1.0, c("Water").as_ptr()) },
        f64::INFINITY
    );
    assert_eq!(errstring(), "Output is null");
    let bad = [0xff_u8 as c_char, 0];
    // SAFETY: `bad` is NUL-terminated.
    assert_eq!(unsafe { Props1SI(bad.as_ptr(), c("Tcrit").as_ptr()) }, f64::INFINITY);
    assert_eq!(errstring(), "FluidName is not UTF-8");
}

/// User decision P6. `DONT_CHECK_PROPERTY_LIMITS` is the flash options' `DomainPolicy::Extrapolate` on every later call;
/// `LIST_STRING_DELIMITER` is the lists' delimiter; every other v8.0.0 key, set with its own type, is accepted and changes
/// nothing; an unknown key or another type is refused with the `errstring` set, as v8.0.0 refuses it.
#[test]
fn set_config_follows_decision_6() {
    let _serial = serial();
    let supercooled = || props("Dmass", ("T", 260.0), ("P", 1e5), "Water");
    assert_eq!(supercooled(), f64::INFINITY, "below the melting line, the domain is enforced");
    assert!(!errstring().is_empty());
    set_bool("DONT_CHECK_PROPERTY_LIMITS", true);
    assert!(supercooled().is_finite() && errstring().is_empty(), "extrapolated");
    set_bool("DONT_CHECK_PROPERTY_LIMITS", false);
    assert_eq!(supercooled(), f64::INFINITY);
    assert!(!errstring().is_empty());
    let reference = props("Hmass", ("T", 300.0), ("P", 1e5), "Water").to_bits();
    let (fluids, aliases) = (global("FluidsList", 1 << 14).1, fluid_param_text("Water", "aliases"));
    for (key, kind) in
        config_keys().filter(|(key, _)| !["DONT_CHECK_PROPERTY_LIMITS", "LIST_STRING_DELIMITER"].contains(key))
    {
        match kind {
            Kind::Bool => set_bool(key, true),
            Kind::Double => set_double(key, 2.5),
            Kind::Text => set_string(key, ";"),
            Kind::Int => set_double(key, 1.0),
        }
        let refused = errstring();
        assert_eq!(refused.contains("takes a Int"), kind == Kind::Int, "{key}: {refused}");
        assert_eq!(props("Hmass", ("T", 300.0), ("P", 1e5), "Water").to_bits(), reference, "{key} changed PropsSI");
    }
    assert_eq!(
        (global("FluidsList", 1 << 14).1, fluid_param_text("Water", "aliases")),
        (fluids.clone(), aliases.clone())
    );
    set_string("LIST_STRING_DELIMITER", ";");
    assert_eq!(global("FluidsList", 1 << 14).1, fluids.replace(',', ";"));
    assert_eq!(fluid_param_text("Water", "aliases"), aliases.replace(',', ";"));
    set_bool("NO_SUCH_KEY", true);
    assert_eq!(errstring(), "`NO_SUCH_KEY` is not a CoolProp 8.0.0 configuration key");
    set_double("DONT_CHECK_PROPERTY_LIMITS", 1.0);
    assert_eq!(errstring(), "`DONT_CHECK_PROPERTY_LIMITS` takes a Bool value, not a Double one");
    assert_eq!(supercooled(), f64::INFINITY, "a refused value changes nothing");
    // SAFETY: a null key is refused before it is read.
    unsafe { set_config_bool(std::ptr::null(), true) };
    assert_eq!(errstring(), "key is null");
    assert_eq!(config_keys().count(), 38);
}

/// `get_fluid_param_string` into a 4 KiB buffer, its text if it succeeds.
fn fluid_param_text(fluid: &str, param: &str) -> String {
    let (f, p, mut buf) = (c(fluid), c(param), vec![b'#' as c_char; 4096]);
    // SAFETY: both strings are NUL-terminated; `buf` holds 4096 bytes.
    let ok = unsafe { get_fluid_param_string(f.as_ptr(), p.as_ptr(), buf.as_mut_ptr(), 4096) };
    if ok == 1 { text(&buf) } else { format!("refused: {}", errstring()) }
}

/// ROT-019 (map 14 R15: the `LIST_STRING_DELIMITER` config breaks `FluidsList()`). The fluids list is the registry's
/// names, each once, joined by the delimiter alone: no other key changes it, and the kernel keeps no configuration.
#[test]
fn fluids_list_ignores_unrelated_config() {
    let _serial = serial();
    let (ok, list) = global("FluidsList", 1 << 14);
    let names: Vec<&str> = list.split(',').collect();
    assert_eq!(ok, 1);
    assert_eq!(names, Registry::embedded().map(|r| r.names().collect::<Vec<_>>()).unwrap_or_default());
    for alias in ["fluids_list", "fluidslist"] {
        assert_eq!(global(alias, 1 << 14).1, list);
    }
    set_string("FLOAT_PUNCTUATION", ",");
    set_bool("NORMALIZE_GAS_CONSTANTS", false);
    set_double("R_U_CODATA", 8.0);
    assert_eq!(global("FluidsList", 1 << 14).1, list);
}

/// ROT-057 (map 09 R17: ASHRAE 34 "UNKNOWN" or "?" for fluids without a class). No fluid has an ASHRAE 34 class in
/// phasekit's data, so the parameter is refused for every one and the buffer is left alone; the other parameters read
/// the fluid's record.
#[test]
fn ashrae34_class_is_absent_for_every_fluid() {
    let _serial = serial();
    let registry = Registry::embedded().unwrap_or_else(|_| unreachable!());
    for name in registry.names() {
        let refused = fluid_param_text(name, "ASHRAE34");
        assert_eq!(refused, format!("refused: {name} has no ASHRAE 34 safety class in phasekit's data"));
    }
    assert_eq!(fluid_param_text("R718", "name"), "Water");
    let fluid = registry.get("Water").unwrap_or_else(|_| unreachable!());
    assert_eq!(fluid_param_text("Water", "BibTeX-EOS"), *fluid.info().source().bibkey);
    assert_eq!(fluid_param_text("Water", "aliases"), fluid.info().aliases().collect::<Vec<_>>().join(","));
    assert_eq!(fluid_param_text("Water", "formula"), "refused: `formula` is not a fluid parameter phasekit has");
    assert!(fluid_param_text("NoSuchFluid", "name").starts_with("refused: "));
}

/// CoolProp's buffer and string conventions (`str2buf`; map 11 F2): a buffer of `n` bytes takes a string shorter than
/// `n`, or nothing is written and the call fails; `PhaseSI` writes `unknown: <message>` for a state that fails; the
/// codes are -1 for an unknown name.
#[test]
fn shim_strings_and_buffers_follow_str2buf() {
    let _serial = serial();
    let phase = |t: f64, n: usize| {
        let (a, b, f, mut buf) = (c("T"), c("P"), c("Water"), vec![b'#' as c_char; n]);
        // SAFETY: every string is NUL-terminated; `buf` holds `n` bytes.
        let ok = unsafe {
            PhaseSI(a.as_ptr(), t, b.as_ptr(), 1e5, f.as_ptr(), buf.as_mut_ptr(), c_int::try_from(n).unwrap_or(0))
        };
        (ok, text(&buf))
    };
    assert_eq!(phase(300.0, 7), (1, "liquid".to_string()));
    assert_eq!(phase(300.0, 6), (0, "######".to_string()));
    assert_eq!(errstring(), "the output needs 7 bytes, the buffer has 6");
    let (ok, unknown) = phase(-1.0, 256);
    assert!(ok == 1 && unknown.starts_with("unknown: ") && unknown.contains("temperature"), "{unknown}");
    // SAFETY: a null buffer is refused before it is written.
    assert_eq!(unsafe { get_global_param_string(c("version").as_ptr(), std::ptr::null_mut(), 8) }, 0);
    assert_eq!(errstring(), "the output buffer is null");
    assert_eq!(global("version", 64), (1, env!("CARGO_PKG_VERSION").to_string()));
    assert_eq!(global("version", 0).0, 0);
    assert_eq!(global("HOME", 64), (0, "#".repeat(64)));
    assert_eq!(errstring(), "`HOME` is not a global parameter phasekit has");
    assert_eq!((global("warnstring", 8), global("predefined_mixtures", 8)), ((1, String::new()), (1, String::new())));
    let parameters = global("parameter_list", 4096).1;
    assert_eq!(parameters, output_names().collect::<Vec<_>>().join(","));
    let index = |f: unsafe extern "C" fn(*const c_char) -> c_long, name: &str| {
        // SAFETY: `name` is NUL-terminated.
        unsafe { f(c(name).as_ptr()) }
    };
    assert_eq!(
        (index(get_param_index, "T"), index(get_param_index, "DMASS"), index(get_param_index, "dmass")),
        (19, 40, -1)
    );
    assert_eq!(errstring(), "`dmass` is not a parameter name");
    assert_eq!((index(get_input_pair_index, "PT_INPUTS"), index(get_input_pair_index, "QS_INPUTS")), (17, 5));
    assert_eq!(index(get_input_pair_index, "QSmolar_INPUTS"), -1);
    assert_eq!(errstring(), "`QSmolar_INPUTS` is not an input pair name");
}

/// ROT-150 (map 11 F16). A panic in a shim function's body is its failure value with the `errstring` set. Where a panic
/// aborts (wasm32-wasip2), there is no unwind to catch.
#[cfg(panic = "unwind")]
#[test]
fn shim_panics_are_caught() {
    let _serial = serial();
    assert_eq!(run(7, || -> Result<i32, String> { std::panic::panic_any("a bug") }), 7);
    assert!(errstring().contains("panic"));
}

/// User decision P6: docs/MIGRATION.md's `set_config_*` table lists each of v8.0.0's keys once, with its type and what
/// the shim does with it.
#[test]
fn migration_lists_every_config_key() {
    let doc = include_str!("../../../../docs/MIGRATION.md");
    let rows: Vec<Vec<&str>> = doc
        .lines()
        .filter_map(|line| line.strip_prefix("| `")?.strip_suffix(" |"))
        .map(|row| row.split(" | ").collect())
        .collect();
    let want: Vec<Vec<String>> = config_keys()
        .map(|(key, kind)| {
            let (kind, treatment) = match (key, kind) {
                ("DONT_CHECK_PROPERTY_LIMITS", _) => ("bool", "`DomainPolicy::Extrapolate` while true"),
                ("LIST_STRING_DELIMITER", _) => ("string", "the delimiter of `FluidsList` and `aliases`"),
                (_, Kind::Bool) => ("bool", "ignored"),
                (_, Kind::Double) => ("double", "ignored"),
                (_, Kind::Text) => ("string", "ignored"),
                (_, Kind::Int) => ("integer", "refused (no Tier A setter)"),
            };
            vec![format!("{key}`"), kind.to_string(), treatment.to_string()]
        })
        .collect();
    let mut got: Vec<Vec<String>> = rows.iter().map(|row| row.iter().map(|cell| cell.to_string()).collect()).collect();
    got.sort();
    let mut want = want;
    want.sort();
    assert_eq!(got, want);
}

/// Rot: ROT-151 (user decision 9b; `CoolProp-Tests-FPUGuard.cpp:27-82`). The shim's guard is v8.0.0's: a trap the host
/// enabled is masked inside, so a 0/0 there is a quiet NaN, and enabled again after; an environment with every trap
/// masked stays so; and the sticky flags are cleared on return, the host's and those raised inside, for hosts that
/// poll them. Inside, rounding is to nearest; after, the host's again.
#[cfg(any(
    all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "windows", target_env = "msvc")
))]
#[test]
fn shim_fp_guard_matches_v8_0_0() {
    use crate::fp::sys::probe::{
        INVALID, OVERFLOW, TOWARD_ZERO, flags, rounding, set_flags, set_rounding, set_traps, traps,
    };
    let _serial = serial();
    let nearest = rounding();
    let inside = || {
        run((0, 0, false), || {
            let nan = std::hint::black_box(0.0_f64) / std::hint::black_box(0.0);
            Ok((traps(), rounding(), nan.is_nan()))
        })
    };
    set_rounding(TOWARD_ZERO);
    set_flags(OVERFLOW);
    set_traps(INVALID);
    let trapping = traps() == INVALID;
    let (unmasked, masked_inside) = (inside(), (traps(), flags(), rounding()));
    set_traps(0);
    let (masked, still_masked) = (inside(), (traps(), flags(), rounding()));
    set_flags(OVERFLOW);
    let value = props("Hmass", ("T", 300.0), ("P", 1e5), "Water");
    let polled = flags();
    set_rounding(nearest);
    assert_eq!(unmasked, (0, nearest, true), "masked, to nearest and a quiet NaN inside");
    assert_eq!(masked_inside, (if trapping { INVALID } else { 0 }, 0, TOWARD_ZERO), "the trap back, the flags cleared");
    assert!(trapping || cfg!(target_arch = "aarch64"), "the trap was not enabled");
    assert_eq!((masked, still_masked), ((0, nearest, true), (0, 0, TOWARD_ZERO)), "a masked host stays masked");
    assert!(value.is_finite() && polled == 0, "PropsSI leaves a clean status word: {polled:#x}");
}
