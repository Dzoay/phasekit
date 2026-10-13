//! MSVC's FP environment: `_controlfp_s` for the masks and the rounding mode, the UCRT's C99 `fe*` functions for the
//! flags, one family for reading, clearing and setting them.

#![allow(unsafe_code, reason = "the C runtime's FP environment functions")]

use std::ffi::{c_int, c_uint, c_ulong};

/// The caller's control word and sticky flags.
pub(crate) struct Env {
    control: c_uint,
    flags: c_ulong,
}

unsafe extern "C" {
    fn _controlfp_s(current: *mut c_uint, new: c_uint, mask: c_uint) -> c_int;
    fn feclearexcept(excepts: c_int) -> c_int;
    fn fegetexceptflag(flags: *mut c_ulong, excepts: c_int) -> c_int;
    fn fesetexceptflag(flags: *const c_ulong, excepts: c_int) -> c_int;
}

/// `_MCW_EM | _MCW_RC`: the exception masks and the rounding mode.
const CONTROL: c_uint = 0x0008_001f | 0x0000_0300;
/// `_MCW_EM | _RC_NEAR`: every exception masked, round-to-nearest.
const MASKED_NEAREST: c_uint = 0x0008_001f;
/// The UCRT's `FE_ALL_EXCEPT`.
pub(crate) const ALL: c_int = 0x1f;

/// The caller's environment; then its flags cleared, every trap masked and round-to-nearest.
pub(super) fn hold() -> Env {
    let (mut control, mut flags, mut unused) = (0, 0, 0);
    // SAFETY: each pointer is writable; the calls only change the thread's FP environment.
    unsafe {
        _controlfp_s(&mut control, 0, 0);
        fegetexceptflag(&mut flags, ALL);
        feclearexcept(ALL);
        _controlfp_s(&mut unused, MASKED_NEAREST, CONTROL);
    }
    Env { control, flags }
}

/// Restores the control word of `env` and, unless `clear`, its flags; with `clear` the flags are left cleared.
pub(super) fn restore(env: &Env, clear: bool) {
    let mut unused = 0;
    // SAFETY: as in `hold`.
    unsafe {
        feclearexcept(ALL);
        _controlfp_s(&mut unused, env.control, CONTROL);
        if !clear {
            fesetexceptflag(&env.flags, ALL);
        }
    }
}

/// Test support: the caller's side of the environment: `_controlfp_s` for the traps and the rounding mode, the UCRT's
/// `fe*` functions for the flags, as the guard reads them.
#[cfg(test)]
pub(crate) mod probe {
    use std::ffi::{c_int, c_uint};

    unsafe extern "C" {
        fn _controlfp_s(current: *mut c_uint, new: c_uint, mask: c_uint) -> c_int;
        fn fetestexcept(excepts: c_int) -> c_int;
        fn feclearexcept(excepts: c_int) -> c_int;
    }

    /// The UCRT's `FE_INVALID`, as `_controlfp_s`'s `_EM_INVALID`; `FE_OVERFLOW`.
    pub(crate) const INVALID: c_int = 0x10;
    pub(crate) const OVERFLOW: c_int = 0x04;
    /// `_RC_CHOP`.
    pub(crate) const TOWARD_ZERO: c_int = 0x300;

    fn control() -> c_uint {
        let mut control = 0;
        // SAFETY: `control` is writable.
        unsafe { _controlfp_s(&mut control, 0, 0) };
        control
    }
    /// The enabled (unmasked) traps among `_EM_INVALID`, `_EM_ZERODIVIDE`, `_EM_OVERFLOW`, `_EM_UNDERFLOW`,
    /// `_EM_INEXACT`.
    pub(crate) fn traps() -> c_int {
        c_int::try_from(!control() & 0x1f).unwrap_or(0)
    }
    /// Enables exactly the traps `excepts`.
    pub(crate) fn set_traps(excepts: c_int) {
        let (mut unused, masks) = (0, 0x1f & !c_uint::try_from(excepts).unwrap_or(0));
        // SAFETY: changes the thread's FP environment.
        unsafe { _controlfp_s(&mut unused, masks, 0x1f) };
    }
    /// The raised sticky flags.
    pub(crate) fn flags() -> c_int {
        // SAFETY: reads the thread's FP environment.
        unsafe { fetestexcept(super::ALL) }
    }
    /// Raises the overflow flag (and the inexact one) as a host's own arithmetic does.
    pub(crate) fn raise_overflow() {
        std::hint::black_box(std::hint::black_box(f64::MAX) * std::hint::black_box(2.0));
    }
    /// Clears every sticky flag.
    pub(crate) fn clear_flags() {
        // SAFETY: changes the thread's FP environment.
        unsafe { feclearexcept(super::ALL) };
    }
    /// The rounding mode.
    pub(crate) fn rounding() -> c_int {
        c_int::try_from(control() & 0x300).unwrap_or(0)
    }
    /// Sets the rounding mode.
    pub(crate) fn set_rounding(mode: c_int) {
        let mut unused = 0;
        // SAFETY: changes the thread's FP environment.
        unsafe { _controlfp_s(&mut unused, c_uint::try_from(mode).unwrap_or(0), 0x300) };
    }
}
