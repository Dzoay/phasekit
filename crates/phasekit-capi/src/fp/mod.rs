//! The FP guard (PLAN.md M10.7; ROT-151; map 11 F17; user decision 9b). Every C export runs with every FP trap masked
//! and round-to-nearest, whatever its host set: Delphi and some Fortran builds unmask traps, and a NaN would then fault
//! them mid-call. On return a `pk_*` export gives the caller back its own environment, the control word and the sticky
//! flags as they were. The shim's exports clear the sticky flags instead, as v8.0.0 does for hosts that poll them
//! (Excel/VBA; `CoolProp-Tests-FPUGuard.cpp:27-82`).
//!
//! glibc's `fe*` functions and MSVC's `_controlfp_s` are declared here directly. Elsewhere (wasm, other platforms) the
//! guard does nothing: they do not trap by default, and wasm has no FP environment.

/// The caller's environment, restored when the guard drops.
pub(crate) struct Guard {
    saved: sys::Env,
    clear: bool,
}

impl Guard {
    /// Masks every trap and sets round-to-nearest; dropping the guard restores the caller's environment, or with
    /// `clear` its control word with the sticky flags cleared (the shim's, v8.0.0's).
    pub(crate) fn new(clear: bool) -> Guard {
        Guard { saved: sys::hold(), clear }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        sys::restore(&self.saved, self.clear);
    }
}

#[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]
// The C runtime's functions and types, not the ABI's: cbindgen skips them.
/// cbindgen:ignore
pub(crate) mod linux;
#[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]
pub(crate) use linux as sys;
#[cfg(all(target_os = "windows", target_env = "msvc"))]
// The C runtime's functions and types, not the ABI's: cbindgen skips them.
/// cbindgen:ignore
pub(crate) mod windows;
#[cfg(all(target_os = "windows", target_env = "msvc"))]
pub(crate) use windows as sys;
#[cfg(not(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "windows", target_env = "msvc")
)))]
// The C runtime's functions and types, not the ABI's: cbindgen skips them.
/// cbindgen:ignore
mod none;
#[cfg(not(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "windows", target_env = "msvc")
)))]
use none as sys;
