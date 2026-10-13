//! `phasekit-capi`: the `pk_*` C ABI (PLAN.md M10.4; D11, user decisions 1, 9a), over `phasekit-compat`, so its keys,
//! fluid strings and errors are the strings' (ROT-169). Its header, `include/phasekit.h`, is cbindgen's output,
//! committed. The Tier A CoolPropLib.h shim, a separate build of this crate, is [`shim`] (M10.6).
//!
//! The conventions, every export alike (map 11 F1-F5, F16):
//! - **Status returns.** Each function returns an `int32_t` status, `PK_OK` (0) or one of the `PK_*` codes, and sets
//!   its thread's last error, empty on success, which [`pk_last_error`] copies out; one thread never sees another's.
//! - **Handles** are generational `uint64_t` ids ([`pk_registry`], [`pk_fluid`]), never pointers: a freed or never
//!   issued id is `PK_STALE_HANDLE`, and 0 is never one.
//! - **Strings in** are UTF-8 bytes with a length, no terminator needed; **strings out** are written with a terminating
//!   NUL into a buffer of a given length, and the length needed, NUL included, is reported, so a call with too small a
//!   buffer is `PK_BUFFER_TOO_SMALL` and writes nothing past its length.
//! - Only `int32_t`, `uint64_t`, `size_t`, `double` and `char` cross the boundary (map 11 F4), and no panic does: each
//!   export catches it as `PK_PANIC`.

// clippy's `disallowed_macros` honours an allow only at the crate root, so this one is here; `gates lints` keeps
// `thread_local` to last_error.rs, the C ABI's one piece of per-thread state (D8, D11, E13).
#![allow(clippy::disallowed_macros, reason = "the per-thread last error, in last_error.rs alone (gates lints)")]
// The shim's build (`coolproplib-shim`) exports CoolProp's Tier A names and none of `pk_*`, whose code it then leaves
// unused (D11).
#![cfg_attr(
    feature = "coolproplib-shim",
    allow(dead_code, unused_imports, reason = "the shim's build exports no pk_* function")
)]

use std::ffi::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use phasekit_compat::{CompatError, batch_si, fluid_in, parse_reference_state, phase_si_in, props_si_in, props1_si_in};
use phasekit_core::batch::Status;
use phasekit_core::{DataSet, Error, Fluid, Registry};

mod fp;
mod handles;
mod last_error;
pub mod shim;

#[cfg(test)]
mod tests;

use handles::{fluids, lock, registries};

/// A registry: the embedded fluids under one data set, perhaps re-gauged ([`pk_registry_with_reference`]).
#[allow(non_camel_case_types, reason = "a C type name")]
pub type pk_registry = u64;

/// One fluid of a registry ([`pk_fluid_new`]).
#[allow(non_camel_case_types, reason = "a C type name")]
pub type pk_fluid = u64;

/// Success.
pub const PK_OK: i32 = 0;
/// A batch cell computed outside the validated domain, flagged, not a failure.
pub const PK_EXTRAPOLATED: i32 = 1;
/// A quantity failed validation: NaN, infinite, non-positive T, p or density, Q outside [0, 1].
pub const PK_INVALID_INPUT: i32 = 2;
/// The model does not declare the input pair.
pub const PK_UNSUPPORTED: i32 = 3;
/// Outside the model's validity domain.
pub const PK_DOMAIN: i32 = 4;
/// A solver did not converge.
pub const PK_NO_CONVERGENCE: i32 = 5;
/// Several physical solutions.
pub const PK_AMBIGUOUS: i32 = 6;
/// The property is undefined in this phase.
pub const PK_UNDEFINED: i32 = 7;
/// The model has no correlation for this property.
pub const PK_NO_MODEL: i32 = 8;
/// A fluid could not be loaded, or no fluid has the name.
pub const PK_LOAD: i32 = 9;
/// Another kernel failure.
pub const PK_OTHER: i32 = 10;
/// A key or a fluid string outside the grammar: an unknown output or input key, two keys that form no pair, a fluid
/// string the grammar refuses, a state-dependent output asked of `pk_props1_si`, `Phase` asked of `pk_batch`, an unknown
/// reference state.
pub const PK_UNKNOWN_KEY: i32 = 11;
/// A required pointer is null, or not aligned for its type.
pub const PK_NULL_POINTER: i32 = 12;
/// A length is larger than any buffer can be (over `PTRDIFF_MAX` bytes), or a batch's buffers do not hold its cells.
pub const PK_BAD_LENGTH: i32 = 13;
/// A string is not UTF-8.
pub const PK_BAD_UTF8: i32 = 14;
/// The handle was freed, or never issued.
pub const PK_STALE_HANDLE: i32 = 15;
/// The output buffer is shorter than the needed length, which is reported; nothing is written past its length.
pub const PK_BUFFER_TOO_SMALL: i32 = 16;
/// A panic was caught at the boundary (a bug: please report it); the call did nothing.
pub const PK_PANIC: i32 = 17;

/// `pk_registry_new`'s data set: CoolProp's data with the published corrections (user decision 2).
pub const PK_DATASET_CORRECTED: i32 = 0;
/// `pk_registry_new`'s data set: CoolProp 8.0.0's data exactly (oracle comparison).
pub const PK_DATASET_PARITY: i32 = 1;

/// A failed call: its status and its message.
struct Fail {
    status: i32,
    message: String,
}

impl Fail {
    fn new(status: i32, message: impl Into<String>) -> Self {
        Fail { status, message: message.into() }
    }
}

/// A kernel error's code: its batch `Status`'s (one exhaustive `match` in core, ROT-023).
fn kernel_status(e: &Error) -> i32 {
    status_code(Status::from(e))
}

/// A batch `Status`'s code.
fn status_code(status: Status) -> i32 {
    match status {
        Status::Ok => PK_OK,
        Status::Extrapolated => PK_EXTRAPOLATED,
        Status::InvalidInput => PK_INVALID_INPUT,
        Status::Unsupported => PK_UNSUPPORTED,
        Status::Domain => PK_DOMAIN,
        Status::NoConvergence => PK_NO_CONVERGENCE,
        Status::Ambiguous => PK_AMBIGUOUS,
        Status::Undefined => PK_UNDEFINED,
        Status::NoModel => PK_NO_MODEL,
        Status::Load => PK_LOAD,
        _ => PK_OTHER,
    }
}

impl From<CompatError> for Fail {
    fn from(e: CompatError) -> Self {
        let status = match &e {
            CompatError::Core(e) => kernel_status(e),
            _ => PK_UNKNOWN_KEY,
        };
        Fail::new(status, e.to_string())
    }
}

impl From<Error> for Fail {
    fn from(e: Error) -> Self {
        Fail::new(kernel_status(&e), e.to_string())
    }
}

/// Runs an export's body: its status, its thread's last error set (empty on success), and a panic caught as
/// `PK_PANIC` (map 11 F16; ROT-150); under the FP guard, which gives the caller its environment back ([`fp`]; ROT-151).
fn guard(body: impl FnOnce() -> Result<(), Fail>) -> i32 {
    let _fp = fp::Guard::new(false);
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => {
            last_error::set("");
            PK_OK
        }
        Ok(Err(fail)) => {
            last_error::set(&fail.message);
            fail.status
        }
        Err(_) => {
            last_error::set("a panic was caught at the C boundary; please report it");
            PK_PANIC
        }
    }
}

/// The UTF-8 string of `len` bytes at `ptr`.
///
/// # Safety
/// `ptr` is null or points to `len` readable bytes that stay unchanged during the call.
#[allow(unsafe_code)]
unsafe fn text<'a>(ptr: *const c_char, len: usize, what: &str) -> Result<&'a str, Fail> {
    if ptr.is_null() {
        return Err(Fail::new(PK_NULL_POINTER, format!("{what} is null")));
    }
    if isize::try_from(len).is_err() {
        return Err(Fail::new(PK_BAD_LENGTH, format!("{what}: length {len} is over PTRDIFF_MAX")));
    }
    // SAFETY: non-null, `len` ≤ isize::MAX, readable for `len` bytes by the caller's contract.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    std::str::from_utf8(bytes).map_err(|_| Fail::new(PK_BAD_UTF8, format!("{what} is not UTF-8")))
}

/// The `len` values at `ptr`; none if `len` is 0, when `ptr` may be null.
///
/// # Safety
/// `ptr` is null or points to `len` readable `T`s that stay unchanged during the call.
#[allow(unsafe_code)]
unsafe fn values<'a, T>(ptr: *const T, len: usize, what: &str) -> Result<&'a [T], Fail> {
    if len == 0 {
        return Ok(&[]);
    }
    buffer(ptr, len, what)?;
    // SAFETY: non-null, aligned, at most isize::MAX bytes, readable for `len` `T`s by the caller's contract.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// The `len` writable values at `ptr`; none if `len` is 0, when `ptr` may be null.
///
/// # Safety
/// `ptr` is null or points to `len` writable `T`s that nothing else reads or writes during the call.
#[allow(unsafe_code)]
unsafe fn values_mut<'a, T>(ptr: *mut T, len: usize, what: &str) -> Result<&'a mut [T], Fail> {
    if len == 0 {
        return Ok(&mut []);
    }
    buffer(ptr.cast_const(), len, what)?;
    // SAFETY: non-null, aligned, at most isize::MAX bytes, writable for `len` `T`s and unaliased by the caller's
    // contract.
    Ok(unsafe { std::slice::from_raw_parts_mut(ptr, len) })
}

/// Whether a buffer of `len` `T`s at `ptr` can be a slice: not null, aligned, at most `PTRDIFF_MAX` bytes.
fn buffer<T>(ptr: *const T, len: usize, what: &str) -> Result<(), Fail> {
    if ptr.is_null() || !ptr.is_aligned() {
        return Err(Fail::new(PK_NULL_POINTER, format!("{what} is null or not aligned")));
    }
    if len.checked_mul(size_of::<T>()).is_none_or(|bytes| isize::try_from(bytes).is_err()) {
        return Err(Fail::new(PK_BAD_LENGTH, format!("{what}: {len} values are over PTRDIFF_MAX bytes")));
    }
    Ok(())
}

/// The `m` output keys at `keys`, key `k` of `lens[k]` bytes.
///
/// # Safety
/// `keys` and `lens` are null or point to `m` readable entries; each key points to its length's readable bytes.
#[allow(unsafe_code)]
unsafe fn keys<'a>(keys: *const *const c_char, lens: *const usize, m: usize) -> Result<Vec<&'a str>, Fail> {
    // SAFETY: the caller's contract.
    let (keys, lens) = unsafe { (values(keys, m, "outputs")?, values(lens, m, "output_lens")?) };
    // SAFETY: the caller's contract, for each key.
    keys.iter().zip(lens).map(|(&key, &len)| unsafe { text(key, len, "an output key") }).collect()
}

/// Writes `value` to `out`.
///
/// # Safety
/// `out` is null or points to writable memory for one `T`.
#[allow(unsafe_code)]
unsafe fn put<T>(out: *mut T, value: T, what: &str) -> Result<(), Fail> {
    if out.is_null() {
        return Err(Fail::new(PK_NULL_POINTER, format!("{what} is null")));
    }
    // SAFETY: non-null and writable for one `T` by the caller's contract.
    unsafe { out.write(value) };
    Ok(())
}

/// Writes `s` and a NUL into the buffer of `len` bytes at `buf`, and its needed length, NUL included, to `needed` if that
/// is not null. A buffer shorter than that is `PK_BUFFER_TOO_SMALL`, given only a NUL at its start if it has one byte.
///
/// # Safety
/// `buf` is null or points to `len` writable bytes; `needed` is null or points to writable memory for one `size_t`.
#[allow(unsafe_code)]
unsafe fn put_text(s: &str, (buf, len): (*mut c_char, usize), needed: *mut usize) -> Result<(), Fail> {
    let n = s.len() + 1;
    if !needed.is_null() {
        // SAFETY: non-null and writable for one `size_t` by the caller's contract.
        unsafe { needed.write(n) };
    }
    if len > 0 && buf.is_null() {
        return Err(Fail::new(PK_NULL_POINTER, "the output buffer is null"));
    }
    if isize::try_from(len).is_err() {
        return Err(Fail::new(PK_BAD_LENGTH, format!("the output buffer's length {len} is over PTRDIFF_MAX")));
    }
    if len < n {
        if len > 0 {
            // SAFETY: `buf` is non-null and writable for `len` ≥ 1 bytes.
            unsafe { buf.write(0) };
        }
        return Err(Fail::new(PK_BUFFER_TOO_SMALL, format!("the output needs {n} bytes, the buffer has {len}")));
    }
    // SAFETY: `buf` is non-null and writable for `len` ≥ n bytes; `s` is `n - 1` readable bytes that do not overlap it.
    unsafe {
        std::ptr::copy_nonoverlapping(s.as_ptr(), buf.cast::<u8>(), n - 1);
        buf.add(n - 1).write(0);
    }
    Ok(())
}

/// The registry a handle names.
fn registry(h: pk_registry) -> Result<Arc<Registry>, Fail> {
    lock(registries()).get(h).ok_or_else(|| Fail::new(PK_STALE_HANDLE, format!("registry handle {h:#x} is stale")))
}

/// The fluid a handle names.
fn fluid(h: pk_fluid) -> Result<Arc<Fluid>, Fail> {
    lock(fluids()).get(h).ok_or_else(|| Fail::new(PK_STALE_HANDLE, format!("fluid handle {h:#x} is stale")))
}

/// A new handle for `registry`.
fn new_registry(registry: Registry) -> Result<pk_registry, Fail> {
    lock(registries()).insert(registry).ok_or_else(|| Fail::new(PK_OTHER, "every registry handle is in use"))
}

/// Copies this thread's last error, the message of its last `pk_*` call (empty if that succeeded), with a NUL into the
/// buffer of `len` bytes at `buf`, if it fits; a buffer too short gets only a NUL at its start. Returns the length
/// needed, NUL included: call it with `len` 0 to learn the size.
///
/// # Safety
/// `buf` is null or points to `len` writable bytes.
#[allow(unsafe_code)]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pk_last_error(buf: *mut c_char, len: usize) -> usize {
    let _fp = fp::Guard::new(false);
    let message = last_error::get();
    // SAFETY: the caller's contract; a null `needed` is allowed.
    let _ = unsafe { put_text(&message, (buf, len), std::ptr::null_mut()) };
    message.len() + 1
}

/// A new registry of the embedded fluids under `data_set` (`PK_DATASET_CORRECTED` or `PK_DATASET_PARITY`), into
/// `*out`. Free it with [`pk_registry_free`].
///
/// # Safety
/// `out` is null or points to writable memory for one `pk_registry`.
#[allow(unsafe_code)]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pk_registry_new(data_set: i32, out: *mut pk_registry) -> i32 {
    guard(|| {
        let set = match data_set {
            PK_DATASET_CORRECTED => DataSet::Corrected,
            PK_DATASET_PARITY => DataSet::Parity,
            other => return Err(Fail::new(PK_INVALID_INPUT, format!("data set {other} is not PK_DATASET_*"))),
        };
        let handle = new_registry(Registry::from_embedded(set)?)?;
        // SAFETY: the caller's contract.
        unsafe { put(out, handle, "out") }
    })
}

/// Frees a registry. Fluids made from it stay valid (each holds what it needs); a call already using it finishes.
#[allow(unsafe_code)]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub extern "C" fn pk_registry_free(registry: pk_registry) -> i32 {
    guard(|| {
        let freed = lock(registries()).remove(registry);
        freed.map(drop).ok_or_else(|| Fail::new(PK_STALE_HANDLE, format!("registry handle {registry:#x} is stale")))
    })
}

/// A new registry, into `*out`, in which the fluid named `fluid` (and every alias of it) reports in the reference state
/// named `reference`: CoolProp's `IIR`, `ASHRAE`, `NBP`, or `DEF` (and `RESET`) for the model's own (E10). `registry`
/// itself is unchanged.
///
/// # Safety
/// `fluid` and `reference` point to `fluid_len` and `reference_len` readable bytes; `out` is null or points to writable
/// memory for one `pk_registry`.
#[allow(unsafe_code)]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pk_registry_with_reference(
    registry: pk_registry,
    fluid: *const c_char,
    fluid_len: usize,
    reference: *const c_char,
    reference_len: usize,
    out: *mut pk_registry,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's contract.
        let (name, reference) =
            unsafe { (text(fluid, fluid_len, "fluid")?, text(reference, reference_len, "reference")?) };
        let state = parse_reference_state(reference)
            .ok_or_else(|| Fail::new(PK_UNKNOWN_KEY, format!("`{reference}` is not IIR, ASHRAE, NBP, DEF or RESET")))?;
        let name = fluid_in(&*self::registry(registry)?, name)?.info().name().to_string();
        let handle = new_registry(self::registry(registry)?.with_reference(&name, state)?)?;
        // SAFETY: the caller's contract.
        unsafe { put(out, handle, "out") }
    })
}

/// A new handle, into `*out`, for the fluid the fluid string `name` names in `registry` (a name, alias or CAS number,
/// `HEOS::` optional; the compat grammar). Free it with [`pk_fluid_free`].
///
/// # Safety
/// `name` points to `name_len` readable bytes; `out` is null or points to writable memory for one `pk_fluid`.
#[allow(unsafe_code)]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pk_fluid_new(
    registry: pk_registry,
    name: *const c_char,
    name_len: usize,
    out: *mut pk_fluid,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's contract.
        let name = unsafe { text(name, name_len, "name")? };
        let found = fluid_in(&*self::registry(registry)?, name)?.clone();
        let handle = lock(fluids()).insert(found).ok_or_else(|| Fail::new(PK_OTHER, "every fluid handle is in use"))?;
        // SAFETY: the caller's contract.
        unsafe { put(out, handle, "out") }
    })
}

/// Frees a fluid handle; a call already using it finishes.
#[allow(unsafe_code)]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub extern "C" fn pk_fluid_free(fluid: pk_fluid) -> i32 {
    guard(|| {
        let freed = lock(fluids()).remove(fluid);
        freed.map(drop).ok_or_else(|| Fail::new(PK_STALE_HANDLE, format!("fluid handle {fluid:#x} is stale")))
    })
}

/// The fluid's canonical name, with a NUL, into the buffer of `len` bytes at `buf`; its needed length to `*needed` if
/// `needed` is not null.
///
/// # Safety
/// `buf` is null or points to `len` writable bytes; `needed` is null or points to writable memory for one `size_t`.
#[allow(unsafe_code)]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pk_fluid_name(fluid: pk_fluid, buf: *mut c_char, len: usize, needed: *mut usize) -> i32 {
    guard(|| {
        let name = self::fluid(fluid)?.info().name().to_string();
        // SAFETY: the caller's contract.
        unsafe { put_text(&name, (buf, len), needed) }
    })
}

/// CoolProp's `PropsSI(output, name1, value1, name2, value2, fluid)` over `registry`, into `*out`: the compat strings'
/// keys, grammar and checks (map 01 section 4a; no echo, map 01 R7).
///
/// # Safety
/// Each string pointer points to its length's readable bytes; `out` is null or points to writable memory for one
/// `double`.
#[allow(unsafe_code, clippy::too_many_arguments, reason = "PropsSI's six arguments, with the lengths and the output")]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pk_props_si(
    registry: pk_registry,
    output: *const c_char,
    output_len: usize,
    name1: *const c_char,
    name1_len: usize,
    value1: f64,
    name2: *const c_char,
    name2_len: usize,
    value2: f64,
    fluid: *const c_char,
    fluid_len: usize,
    out: *mut f64,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's contract.
        let (output, name1, name2, fluid) = unsafe {
            (
                text(output, output_len, "output")?,
                text(name1, name1_len, "name1")?,
                text(name2, name2_len, "name2")?,
                text(fluid, fluid_len, "fluid")?,
            )
        };
        let value = props_si_in(&*self::registry(registry)?, output, name1, value1, name2, value2, fluid)?;
        // SAFETY: the caller's contract.
        unsafe { put(out, value, "out") }
    })
}

/// CoolProp's `Props1SI(output, fluid)` over `registry`, into `*out`: a state-independent output.
///
/// # Safety
/// `output` and `fluid` point to their lengths' readable bytes; `out` is null or points to writable memory for one
/// `double`.
#[allow(unsafe_code)]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pk_props1_si(
    registry: pk_registry,
    output: *const c_char,
    output_len: usize,
    fluid: *const c_char,
    fluid_len: usize,
    out: *mut f64,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's contract.
        let (output, fluid) = unsafe { (text(output, output_len, "output")?, text(fluid, fluid_len, "fluid")?) };
        let value = props1_si_in(&*self::registry(registry)?, output, fluid)?;
        // SAFETY: the caller's contract.
        unsafe { put(out, value, "out") }
    })
}

/// CoolProp's `PhaseSI(name1, value1, name2, value2, fluid)` over `registry`: the phase's name, with a NUL, into the
/// buffer of `len` bytes at `buf`; its needed length to `*needed` if `needed` is not null.
///
/// # Safety
/// Each string pointer points to its length's readable bytes; `buf` is null or points to `len` writable bytes; `needed`
/// is null or points to writable memory for one `size_t`.
#[allow(unsafe_code, clippy::too_many_arguments, reason = "PhaseSI's five arguments, with the lengths and the output")]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pk_phase_si(
    registry: pk_registry,
    name1: *const c_char,
    name1_len: usize,
    value1: f64,
    name2: *const c_char,
    name2_len: usize,
    value2: f64,
    fluid: *const c_char,
    fluid_len: usize,
    buf: *mut c_char,
    len: usize,
    needed: *mut usize,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's contract.
        let (name1, name2, fluid) = unsafe {
            (text(name1, name1_len, "name1")?, text(name2, name2_len, "name2")?, text(fluid, fluid_len, "fluid")?)
        };
        let phase = phase_si_in(&*self::registry(registry)?, name1, value1, name2, value2, fluid)?;
        // SAFETY: the caller's contract.
        unsafe { put_text(phase, (buf, len), needed) }
    })
}

/// `n` points of the input pair that the keys `name1` and `name2` name, in either order as `PropsSI`'s, at `values1[i]`
/// and `values2[i]`, on `fluid`: output `k` of point `i`, of the output key `outputs[k]` (`output_lens[k]` bytes), into
/// `out[i * m + k]` and its status into `status[i * m + k]` (point-major, map 11 F15, F23; user decision 11). Each cell
/// is `pk_props_si`'s value bit for bit, or NaN with its failure's code. `out` and `status` hold `cells` = n x m each.
///
/// The call's own status is `PK_OK` when its keys, handle and buffers are good, whatever its cells'. An unknown key, an
/// output that is not a property (`Phase`), or `cells` other than n x m refuses the whole call, with no cell written.
///
/// # Safety
/// `outputs` and `output_lens` point to `m` readable entries, each key to its length's readable bytes; `name1` and
/// `name2` to their lengths' readable bytes; `values1` and `values2` to `n` readable doubles; `out` and `status` to
/// `cells` writable doubles and `int32_t`s that overlap nothing else. Any pointer may be null when its length is 0.
#[allow(unsafe_code, clippy::too_many_arguments, reason = "each buffer with its length, as every pk_* call")]
#[cfg(not(feature = "coolproplib-shim"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pk_batch(
    fluid: pk_fluid,
    outputs: *const *const c_char,
    output_lens: *const usize,
    m: usize,
    name1: *const c_char,
    name1_len: usize,
    values1: *const f64,
    name2: *const c_char,
    name2_len: usize,
    values2: *const f64,
    n: usize,
    out: *mut f64,
    status: *mut i32,
    cells: usize,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's contract.
        let (keys, input1, input2) = unsafe {
            (
                keys(outputs, output_lens, m)?,
                (text(name1, name1_len, "name1")?, values(values1, n, "values1")?),
                (text(name2, name2_len, "name2")?, values(values2, n, "values2")?),
            )
        };
        if n.checked_mul(m) != Some(cells) {
            return Err(Fail::new(PK_BAD_LENGTH, format!("{n} points of {m} outputs are not {cells} cells")));
        }
        // SAFETY: the caller's contract.
        let (out, status) = unsafe { (values_mut(out, cells, "out")?, values_mut(status, cells, "status")?) };
        let mut cell = vec![Status::Ok; cells];
        batch_si(&*self::fluid(fluid)?, &keys, input1, input2, out, &mut cell)?;
        for (code, cell) in status.iter_mut().zip(cell) {
            *code = status_code(cell);
        }
        Ok(())
    })
}
