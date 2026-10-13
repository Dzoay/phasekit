//! glibc's and musl's FP environment, x86_64 and aarch64 (the `fe*` functions of `<fenv.h>`).

#![allow(unsafe_code, reason = "the C runtime's FP environment functions")]

use std::ffi::c_int;

/// glibc's and musl's `fenv_t`, opaque: 32 bytes on x86_64, 8 on aarch64.
#[repr(C, align(16))]
pub(crate) struct Env([u8; 64]);

unsafe extern "C" {
    fn feholdexcept(env: *mut Env) -> c_int;
    fn fesetenv(env: *const Env) -> c_int;
    fn fesetround(round: c_int) -> c_int;
    fn feclearexcept(excepts: c_int) -> c_int;
}

/// `FE_ALL_EXCEPT`, every flag `<fenv.h>` names.
#[cfg(target_arch = "x86_64")]
pub(crate) const ALL: c_int = 0x3d;
#[cfg(target_arch = "aarch64")]
pub(crate) const ALL: c_int = 0x1f;
/// `FE_TONEAREST`.
pub(crate) const NEAREST: c_int = 0;

/// The caller's environment; then its flags cleared, every trap masked (`feholdexcept`) and round-to-nearest.
pub(super) fn hold() -> Env {
    let mut env = Env([0; 64]);
    // SAFETY: `env` is writable and larger than `fenv_t`; the calls only change the thread's FP environment.
    unsafe {
        feholdexcept(&mut env);
        fesetround(NEAREST);
    }
    env
}

/// Restores `env`, control word and flags, then with `clear` clears the flags.
pub(super) fn restore(env: &Env, clear: bool) {
    // SAFETY: `env` holds what `feholdexcept` saved.
    unsafe {
        fesetenv(env);
        if clear {
            feclearexcept(ALL);
        }
    }
}

/// Test support: the caller's side of the environment, through glibc's `fe*` functions (the trap ones are GNU's).
#[cfg(all(test, target_env = "gnu"))]
pub(crate) mod probe {
    use std::ffi::c_int;

    unsafe extern "C" {
        fn fegetexcept() -> c_int;
        fn feenableexcept(excepts: c_int) -> c_int;
        fn fedisableexcept(excepts: c_int) -> c_int;
        fn fetestexcept(excepts: c_int) -> c_int;
        fn feclearexcept(excepts: c_int) -> c_int;
        fn fesetexceptflag(flags: *const u64, excepts: c_int) -> c_int;
        fn fegetround() -> c_int;
        fn fesetround(round: c_int) -> c_int;
    }

    /// `FE_INVALID` and `FE_OVERFLOW`.
    pub(crate) const INVALID: c_int = 0x01;
    #[cfg(target_arch = "x86_64")]
    pub(crate) const OVERFLOW: c_int = 0x08;
    #[cfg(target_arch = "aarch64")]
    pub(crate) const OVERFLOW: c_int = 0x04;
    /// `FE_TOWARDZERO`.
    #[cfg(target_arch = "x86_64")]
    pub(crate) const TOWARD_ZERO: c_int = 0xc00;
    #[cfg(target_arch = "aarch64")]
    pub(crate) const TOWARD_ZERO: c_int = 0xc0_0000;

    /// The enabled traps.
    pub(crate) fn traps() -> c_int {
        // SAFETY: reads the thread's FP environment.
        unsafe { fegetexcept() }
    }
    /// Enables exactly the traps `excepts` (the hardware may refuse: aarch64 cores need not trap).
    pub(crate) fn set_traps(excepts: c_int) {
        // SAFETY: changes the thread's FP environment.
        unsafe {
            fedisableexcept(super::ALL);
            feenableexcept(excepts);
        }
    }
    /// The raised sticky flags.
    pub(crate) fn flags() -> c_int {
        // SAFETY: reads the thread's FP environment.
        unsafe { fetestexcept(super::ALL) }
    }
    /// Sets exactly the sticky flags `excepts`, raising no trap.
    pub(crate) fn set_flags(excepts: c_int) {
        // glibc's `fexcept_t` is at most 8 bytes; `fesetexceptflag` reads the bits `excepts` names.
        let bits = u64::try_from(excepts).unwrap_or(0);
        // SAFETY: `bits` is readable for one `fexcept_t`.
        unsafe {
            feclearexcept(super::ALL);
            fesetexceptflag(&bits, excepts);
        }
    }
    /// The rounding mode.
    pub(crate) fn rounding() -> c_int {
        // SAFETY: reads the thread's FP environment.
        unsafe { fegetround() }
    }
    /// Sets the rounding mode.
    pub(crate) fn set_rounding(mode: c_int) {
        // SAFETY: changes the thread's FP environment.
        unsafe { fesetround(mode) };
    }
}
