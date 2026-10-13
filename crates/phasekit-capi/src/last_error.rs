//! The per-thread last error of `pk_*` (map 11 F5; ROT-148): every call sets its thread's message, empty on success,
//! and `pk_last_error` copies it out. A thread never sees another's (the Tier A shim's process-wide `errstring` is a
//! separate slot, D11). One of the two modules allowed per-thread or shared state (D8, E13).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the C ABI's per-thread last error, its only per-thread state (D11, E13)"
)]

use std::cell::RefCell;

thread_local! {
    static LAST: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Sets this thread's last error.
pub(crate) fn set(message: &str) {
    LAST.with(|last| {
        let mut last = last.borrow_mut();
        last.clear();
        last.push_str(message);
    });
}

/// This thread's last error.
pub(crate) fn get() -> String {
    LAST.with(|last| last.borrow().clone())
}
