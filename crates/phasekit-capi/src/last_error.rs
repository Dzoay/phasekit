//! The per-thread last error of `pk_*` (map 11 F5; ROT-148): every call sets its thread's message, code and rule, empty
//! on success, and `pk_last_error`, `pk_last_error_code` and `pk_last_error_rule` copy them out (user decision EM12). A thread never sees another's (the Tier A shim's process-wide `errstring` is a
//! separate slot, D11). One of the two modules allowed per-thread or shared state (D8, E13).

#![allow(clippy::disallowed_types, reason = "the C ABI's per-thread last error, its only per-thread state (D11, E13)")]

use std::cell::RefCell;

/// A call's error: its message, its code and an invalid input's rule, each empty where it has none.
struct Last {
    message: String,
    code: &'static str,
    rule: &'static str,
}

thread_local! {
    static LAST: RefCell<Last> = const { RefCell::new(Last { message: String::new(), code: "", rule: "" }) };
}

/// Sets this thread's last error.
pub(crate) fn set(message: &str, code: &'static str, rule: &'static str) {
    LAST.with(|last| {
        let mut last = last.borrow_mut();
        last.message.clear();
        last.message.push_str(message);
        (last.code, last.rule) = (code, rule);
    });
}

/// This thread's last error.
pub(crate) fn get() -> String {
    LAST.with(|last| last.borrow().message.clone())
}

/// This thread's last error's code.
pub(crate) fn code() -> &'static str {
    LAST.with(|last| last.borrow().code)
}

/// This thread's last error's rule.
pub(crate) fn rule() -> &'static str {
    LAST.with(|last| last.borrow().rule)
}
