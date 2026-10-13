//! Windows x64's FP environment: MXCSR, the SSE control and status register. On x64 `_controlfp_s` sets only MXCSR,
//! and MXCSR holds the flags Rust's arithmetic raises, which the UCRT's C99 flag functions do not see (the Windows
//! runner read a flag raised by an SSE multiply as 0 through `fetestexcept`, and one set by `fesetexceptflag` as 0
//! through `_statusfp`). So the guard reads and writes MXCSR itself and restores it whole: masks, rounding, flags.

#![allow(unsafe_code, reason = "MXCSR is read and written by stmxcsr and ldmxcsr")]

use core::arch::asm;

/// MXCSR's sticky flags (bits 0-5), its exception masks (bits 7-12) and its rounding mode (bits 13-14).
const FLAGS: u32 = 0x3f;
const MASKS: u32 = 0x1f80;
const ROUNDING: u32 = 0x6000;

/// The caller's MXCSR.
pub(crate) struct Env(u32);

/// MXCSR.
fn get() -> u32 {
    let mut csr = 0_u32;
    // SAFETY: stmxcsr stores the 32-bit MXCSR into `csr`, which is writable.
    unsafe { asm!("stmxcsr [{}]", in(reg) &raw mut csr, options(nostack, preserves_flags)) };
    csr
}

/// Sets MXCSR (its reserved bits stay 0: every value here is one read from it, with defined bits changed).
fn set(csr: u32) {
    // SAFETY: ldmxcsr loads a 32-bit value with no reserved bit set; it changes only the thread's FP environment.
    unsafe { asm!("ldmxcsr [{}]", in(reg) &raw const csr, options(nostack, readonly, preserves_flags)) };
}

/// The caller's environment; then its flags cleared, every trap masked and round-to-nearest.
pub(super) fn hold() -> Env {
    let csr = get();
    set((csr & !(FLAGS | ROUNDING)) | MASKS);
    Env(csr)
}

/// Restores `env` whole or, with `clear`, with its flags cleared.
pub(super) fn restore(env: &Env, clear: bool) {
    set(if clear { env.0 & !FLAGS } else { env.0 });
}

/// Test support: the caller's side of the environment, in MXCSR's terms.
#[cfg(test)]
pub(crate) mod probe {
    use std::ffi::c_int;

    use super::{FLAGS, MASKS, ROUNDING, get, set};

    /// The invalid and overflow flags (IE, OE); a trap's mask is its flag's bit shifted by 7.
    pub(crate) const INVALID: c_int = 0x01;
    pub(crate) const OVERFLOW: c_int = 0x08;
    /// Round toward zero (RC = 11).
    pub(crate) const TOWARD_ZERO: c_int = 0x6000;

    fn bits(value: c_int) -> u32 {
        u32::try_from(value).unwrap_or(0)
    }
    fn int(value: u32) -> c_int {
        c_int::try_from(value).unwrap_or(-1)
    }

    /// The enabled (unmasked) traps, as their flags' bits.
    pub(crate) fn traps() -> c_int {
        int((!get() & MASKS) >> 7)
    }
    /// Enables exactly the traps `excepts`.
    pub(crate) fn set_traps(excepts: c_int) {
        set((get() | MASKS) & !(bits(excepts) << 7));
    }
    /// The raised sticky flags.
    pub(crate) fn flags() -> c_int {
        int(get() & FLAGS)
    }
    /// Raises the overflow flag (and the inexact one) as a host's own arithmetic does.
    pub(crate) fn raise_overflow() {
        std::hint::black_box(std::hint::black_box(f64::MAX) * std::hint::black_box(2.0));
    }
    /// Clears every sticky flag.
    pub(crate) fn clear_flags() {
        set(get() & !FLAGS);
    }
    /// The rounding mode.
    pub(crate) fn rounding() -> c_int {
        int(get() & ROUNDING)
    }
    /// Sets the rounding mode.
    pub(crate) fn set_rounding(mode: c_int) {
        set((get() & !ROUNDING) | bits(mode));
    }
}
