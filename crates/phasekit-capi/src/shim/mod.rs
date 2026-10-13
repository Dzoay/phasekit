//! The Tier A CoolPropLib.h shim (PLAN.md M10.6; D11; user decisions 9a, 9b, P1, P6; map 11 U11): `PropsSI`,
//! `Props1SI`, `PhaseSI`, `HAPropsSI`, `get_global_param_string`, `get_fluid_param_string`, `get_param_index`,
//! `get_input_pair_index` and `set_config_*`, with CoolProp's names and signatures, over the embedded registry.
//!
//! The `coolproplib-shim` feature exports these names and no `pk_*` one; the default library exports none of them, so
//! it can share a process with a real CoolProp. The shim's build is distributed as `phasekit_coolproplib` (P1). It keeps
//! v8.0.0's quirks, here only:
//! - the v8.0.0 integer codes, a pinned table ([`parameter_code`], [`input_pair_code`]);
//! - one process-wide `errstring`, set by the last failure on any thread and cleared by reading it;
//! - `PropsSI`, `Props1SI` and `HAPropsSI` return `_HUGE` (+inf) on failure, `PhaseSI` writes `unknown: <message>`;
//! - `Props1SI` takes its two arguments in either order;
//! - `set_config_*` per user decision P6 (`config`).
//!
//! Strings are NUL-terminated UTF-8, as CoolProp's; a buffer of `n` bytes takes a string shorter than `n`, or nothing.

#![allow(non_snake_case, reason = "CoolProp's C names")]

mod codes;
mod config;
mod state;

#[cfg(test)]
mod tests;

use std::ffi::{CStr, c_char, c_int, c_long};
use std::panic::{AssertUnwindSafe, catch_unwind};

use phasekit_compat::{fluid_in, output_names, phase_si_with, props_si_with, props1_si_in};
use phasekit_core::{Fluid, Registry};

pub use codes::{input_pair_code, parameter_code, parameter_names};
use config::Value;
pub use config::{Kind, config_keys};

/// CoolProp's `_HUGE`: what `PropsSI`, `Props1SI` and `HAPropsSI` return on failure.
const HUGE: f64 = f64::INFINITY;

/// The `errstring` `HAPropsSI` sets: humid air is not in v0.1, so it is a stub until the humid-air milestone (D11).
pub const HAPROPSSI_MESSAGE: &str = "HAPropsSI is not available in this version of phasekit";

/// Runs a shim function's body: its value, or `failed` with the `errstring` set to the failure (a caught panic too).
fn run<T>(failed: T, body: impl FnOnce() -> Result<T, String>) -> T {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(value)) => value,
        Ok(Err(message)) => {
            state::set_error(&message);
            failed
        }
        Err(_) => {
            state::set_error("a panic was caught at the C boundary; please report it");
            failed
        }
    }
}

/// The NUL-terminated UTF-8 string at `ptr`.
///
/// # Safety
/// `ptr` is null or points to a NUL-terminated string that stays unchanged during the call.
#[allow(unsafe_code)]
unsafe fn c_str<'a>(ptr: *const c_char, what: &str) -> Result<&'a str, String> {
    if ptr.is_null() {
        return Err(format!("{what} is null"));
    }
    // SAFETY: non-null and NUL-terminated by the caller's contract.
    unsafe { CStr::from_ptr(ptr) }.to_str().map_err(|_| format!("{what} is not UTF-8"))
}

/// Writes `s` and a NUL into the buffer of `n` bytes at `buf` if `s` is shorter than `n` (CoolProp's `str2buf`).
///
/// # Safety
/// `buf` is null or points to `n` writable bytes.
#[allow(unsafe_code)]
unsafe fn str2buf(s: &str, buf: *mut c_char, n: c_int) -> Result<(), String> {
    if buf.is_null() {
        return Err("the output buffer is null".to_string());
    }
    if !usize::try_from(n).is_ok_and(|n| s.len() < n) {
        return Err(format!("the output needs {} bytes, the buffer has {n}", s.len() + 1));
    }
    // SAFETY: `buf` is non-null and writable for `n` > s.len() bytes; `s` does not overlap it.
    unsafe {
        std::ptr::copy_nonoverlapping(s.as_ptr(), buf.cast::<u8>(), s.len());
        buf.add(s.len()).write(0);
    }
    Ok(())
}

/// The embedded registry, the shim's only one.
fn registry() -> Result<&'static Registry, String> {
    Registry::embedded().map_err(|e| e.to_string())
}

/// CoolProp's `PropsSI(Output, Name1, Prop1, Name2, Prop2, FluidName)`: compat's strings under the settings' options.
///
/// # Safety
/// Each string is null or NUL-terminated.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn PropsSI(
    output: *const c_char,
    name1: *const c_char,
    prop1: f64,
    name2: *const c_char,
    prop2: f64,
    fluid: *const c_char,
) -> f64 {
    run(HUGE, || {
        // SAFETY: the caller's contract.
        let (output, name1, name2, fluid) = unsafe {
            (c_str(output, "Output")?, c_str(name1, "Name1")?, c_str(name2, "Name2")?, c_str(fluid, "FluidName")?)
        };
        let flash = state::settings().flash();
        props_si_with(registry()?, &flash, output, name1, prop1, name2, prop2, fluid).map_err(|e| e.to_string())
    })
}

/// CoolProp's `Props1SI(FluidName, Output)`: a state-independent output. As CoolProp, it takes the two in either order,
/// and refuses a call in which both or neither name a fluid.
///
/// # Safety
/// Each string is null or NUL-terminated.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn Props1SI(fluid: *const c_char, output: *const c_char) -> f64 {
    run(HUGE, || {
        // SAFETY: the caller's contract.
        let (first, second) = unsafe { (c_str(fluid, "FluidName")?, c_str(output, "Output")?) };
        let registry = registry()?;
        let (fluid, output) = match (fluid_in(registry, first).is_ok(), fluid_in(registry, second).is_ok()) {
            (true, false) => (first, second),
            (false, true) => (second, first),
            (true, true) => return Err(format!("both inputs to Props1SI, `{first}` and `{second}`, are fluids")),
            (false, false) => return Err(format!("neither input to Props1SI, `{first}` or `{second}`, is a fluid")),
        };
        props1_si_in(registry, output, fluid).map_err(|e| e.to_string())
    })
}

/// CoolProp's `PhaseSI(Name1, Prop1, Name2, Prop2, FluidName, phase, n)`: the phase's name into `phase`, or, for a
/// state that fails, `unknown: <message>` (CoolProp's text). Returns 1, or 0 with the `errstring` set if a string is
/// null or not UTF-8 or the name does not fit.
///
/// # Safety
/// Each string is null or NUL-terminated; `phase` is null or points to `n` writable bytes.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn PhaseSI(
    name1: *const c_char,
    prop1: f64,
    name2: *const c_char,
    prop2: f64,
    fluid: *const c_char,
    phase: *mut c_char,
    n: c_int,
) -> c_long {
    run(0, || {
        // SAFETY: the caller's contract.
        let (name1, name2, fluid) =
            unsafe { (c_str(name1, "Name1")?, c_str(name2, "Name2")?, c_str(fluid, "FluidName")?) };
        let flash = state::settings().flash();
        let name = match phase_si_with(registry()?, &flash, name1, prop1, name2, prop2, fluid) {
            Ok(name) => name.to_string(),
            Err(e) => format!("unknown: {e}"),
        };
        // SAFETY: the caller's contract.
        unsafe { str2buf(&name, phase, n)? };
        Ok(1)
    })
}

/// CoolProp's `HAPropsSI`: a stub until the humid-air milestone (D11). It returns `_HUGE` and sets the `errstring` to
/// [`HAPROPSSI_MESSAGE`]; the symbol exists so hosts that bind every symbol at load time still load.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub extern "C" fn HAPropsSI(
    _output: *const c_char,
    _name1: *const c_char,
    _prop1: f64,
    _name2: *const c_char,
    _prop2: f64,
    _name3: *const c_char,
    _prop3: f64,
) -> f64 {
    run(HUGE, || Err(HAPROPSSI_MESSAGE.to_string()))
}

/// A global parameter: `version`, `errstring` (read and cleared), `warnstring` (always empty), `FluidsList` (also
/// `fluids_list`, `fluidslist`; the registry's names, joined by `LIST_STRING_DELIMITER`), `parameter_list` (the
/// output names, joined by commas, as CoolProp's), and the four lists phasekit has no entries for, empty.
fn global_param(param: &str) -> Result<String, String> {
    Ok(match param {
        "version" => env!("CARGO_PKG_VERSION").to_string(),
        "errstring" => state::take_error(),
        "warnstring" => String::new(),
        "FluidsList" | "fluids_list" | "fluidslist" => {
            registry()?.names().collect::<Vec<_>>().join(&state::settings().delimiter)
        }
        "parameter_list" => output_names().collect::<Vec<_>>().join(","),
        "incompressible_list_pure"
        | "incompressible_list_solution"
        | "mixture_binary_pairs_list"
        | "predefined_mixtures" => String::new(),
        _ => return Err(format!("`{param}` is not a global parameter phasekit has")),
    })
}

/// CoolProp's `get_global_param_string(param, Output, n)`: the parameter into `Output` ([`global_param`]). Returns 1,
/// or 0 with the `errstring` set.
///
/// # Safety
/// `param` is null or NUL-terminated; `output` is null or points to `n` writable bytes.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn get_global_param_string(param: *const c_char, output: *mut c_char, n: c_int) -> c_long {
    run(0, || {
        // SAFETY: the caller's contract.
        let value = global_param(unsafe { c_str(param, "param")? })?;
        // SAFETY: the caller's contract.
        unsafe { str2buf(&value, output, n)? };
        Ok(1)
    })
}

/// A fluid parameter: `name`, `aliases` (joined by `LIST_STRING_DELIMITER`) and `BibTeX-EOS`. `ASHRAE34` is refused for
/// every fluid: phasekit's data carry no safety class, never CoolProp's "UNKNOWN" or "?" (map 09 R17, ROT-057).
fn fluid_param(fluid: &Fluid, param: &str) -> Result<String, String> {
    let info = fluid.info();
    Ok(match param {
        "name" => info.name().to_string(),
        "aliases" => info.aliases().collect::<Vec<_>>().join(&state::settings().delimiter),
        "BibTeX-EOS" => info.source().bibkey.to_string(),
        "ASHRAE34" => return Err(format!("{} has no ASHRAE 34 safety class in phasekit's data", info.name())),
        _ => return Err(format!("`{param}` is not a fluid parameter phasekit has")),
    })
}

/// CoolProp's `get_fluid_param_string(fluid, param, Output, n)`: the parameter into `Output` ([`fluid_param`]). Returns
/// 1, or 0 with the `errstring` set.
///
/// # Safety
/// `fluid` and `param` are null or NUL-terminated; `output` is null or points to `n` writable bytes.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn get_fluid_param_string(
    fluid: *const c_char,
    param: *const c_char,
    output: *mut c_char,
    n: c_int,
) -> c_long {
    run(0, || {
        // SAFETY: the caller's contract.
        let (fluid, param) = unsafe { (c_str(fluid, "fluid")?, c_str(param, "param")?) };
        let value = fluid_param(fluid_in(registry()?, fluid).map_err(|e| e.to_string())?, param)?;
        // SAFETY: the caller's contract.
        unsafe { str2buf(&value, output, n)? };
        Ok(1)
    })
}

/// CoolProp's `get_param_index(param)`: the parameter's v8.0.0 code ([`parameter_code`]), or -1 with the `errstring` set.
///
/// # Safety
/// `param` is null or NUL-terminated.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn get_param_index(param: *const c_char) -> c_long {
    run(-1, || {
        // SAFETY: the caller's contract.
        let param = unsafe { c_str(param, "param")? };
        parameter_code(param).map(c_long::from).ok_or_else(|| format!("`{param}` is not a parameter name"))
    })
}

/// CoolProp's `get_input_pair_index(pair)`: the pair's v8.0.0 code ([`input_pair_code`]), or -1 with the `errstring`
/// set.
///
/// # Safety
/// `pair` is null or NUL-terminated.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn get_input_pair_index(pair: *const c_char) -> c_long {
    run(-1, || {
        // SAFETY: the caller's contract.
        let pair = unsafe { c_str(pair, "pair")? };
        input_pair_code(pair).map(c_long::from).ok_or_else(|| format!("`{pair}` is not an input pair name"))
    })
}

/// Sets a configuration key ([`config`]); a refusal sets the `errstring`.
///
/// # Safety
/// `key` is null or NUL-terminated.
#[allow(unsafe_code)]
unsafe fn set_config(key: *const c_char, value: impl FnOnce() -> Result<Value<'static>, String>) {
    run((), || {
        // SAFETY: the caller's contract.
        config::set(unsafe { c_str(key, "key")? }, value()?)
    });
}

/// CoolProp's `set_config_string(key, val)` (user decision P6).
///
/// # Safety
/// `key` and `val` are null or NUL-terminated; `val` stays unchanged during the call.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn set_config_string(key: *const c_char, val: *const c_char) {
    // SAFETY: the caller's contract; the value is copied before the call returns.
    unsafe { set_config(key, || Ok(Value::Text(c_str(val, "val")?))) };
}

/// CoolProp's `set_config_double(key, val)` (user decision P6): no double key has an equivalent, so the value is checked
/// for its type and ignored.
///
/// # Safety
/// `key` is null or NUL-terminated.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn set_config_double(key: *const c_char, _val: f64) {
    // SAFETY: the caller's contract.
    unsafe { set_config(key, || Ok(Value::Double)) };
}

/// CoolProp's `set_config_bool(key, val)` (user decision P6).
///
/// # Safety
/// `key` is null or NUL-terminated.
#[allow(unsafe_code)]
#[cfg_attr(feature = "coolproplib-shim", unsafe(no_mangle))]
pub unsafe extern "C" fn set_config_bool(key: *const c_char, val: bool) {
    // SAFETY: the caller's contract.
    unsafe { set_config(key, || Ok(Value::Bool(val))) };
}
