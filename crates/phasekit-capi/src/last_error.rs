//! The per-thread last error of `pk_*` (map 11 F5; ROT-148): every call sets its thread's message, code and rule, empty
//! on success, and `pk_last_error`, `pk_last_error_code`, `pk_last_error_rule` and `pk_last_error_remedies` copy them out (user decisions
//! EM12, EM15). A thread never sees another's (the Tier A shim's process-wide `errstring` is a
//! separate slot, D11). One of the two modules allowed per-thread or shared state (D8, E13).

#![allow(clippy::disallowed_types, reason = "the C ABI's per-thread last error, its only per-thread state (D11, E13)")]

use std::cell::RefCell;

use phasekit_core::Remedy;

/// A call's error: its message, its code, an invalid input's rule and its remedies, each empty where it has none.
struct Last {
    message: String,
    code: &'static str,
    rule: &'static str,
    remedies: &'static [Remedy],
}

thread_local! {
    static LAST: RefCell<Last> =
        const { RefCell::new(Last { message: String::new(), code: "", rule: "", remedies: &[] }) };
}

/// Sets this thread's last error.
pub(crate) fn set(message: &str, code: &'static str, rule: &'static str, remedies: &'static [Remedy]) {
    LAST.with(|last| {
        let mut last = last.borrow_mut();
        last.message.clear();
        last.message.push_str(message);
        (last.code, last.rule, last.remedies) = (code, rule, remedies);
    });
}

/// This thread's last error's remedies' codes, comma-separated (PLAN.md M29.6).
pub(crate) fn remedies() -> String {
    LAST.with(|last| last.borrow().remedies.iter().map(|r| r.code()).collect::<Vec<_>>().join(","))
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
