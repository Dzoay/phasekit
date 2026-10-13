//! The shim's process-wide state (D11; user decisions 9b, P6), its only shared state with the handle tables (D8, E13):
//! CoolProp's `errstring`, one slot for the process that the last writer sets and any thread reads and clears (#3211:
//! Excel/COM and Mathcad thread pools read it on another thread; map 11 §5c, F5), and the `set_config_*` settings,
//! which reach the kernel only as per-call options.

#![allow(clippy::disallowed_types, reason = "the shim's process-wide errstring and settings (D11, E13)")]

use std::borrow::Cow;
use std::sync::Mutex;

use phasekit_core::{DomainPolicy, FlashOptions};

use crate::handles::lock;

/// The settings with a phasekit equivalent (user decision P6).
#[derive(Clone, Debug)]
pub(super) struct Settings {
    /// `DONT_CHECK_PROPERTY_LIMITS`: `DomainPolicy::Extrapolate` on every call.
    pub(super) extrapolate: bool,
    /// `LIST_STRING_DELIMITER`: the delimiter of the shim's lists.
    pub(super) delimiter: Cow<'static, str>,
}

impl Settings {
    /// CoolProp's defaults.
    const DEFAULT: Settings = Settings { extrapolate: false, delimiter: Cow::Borrowed(",") };

    /// The flash options of a call under these settings.
    pub(super) fn flash(&self) -> FlashOptions {
        let domain = if self.extrapolate { DomainPolicy::Extrapolate } else { DomainPolicy::Enforce };
        FlashOptions::new().with_domain(domain)
    }
}

fn errstring() -> &'static Mutex<String> {
    static ERRSTRING: Mutex<String> = Mutex::new(String::new());
    &ERRSTRING
}

fn settings_slot() -> &'static Mutex<Settings> {
    static SETTINGS: Mutex<Settings> = Mutex::new(Settings::DEFAULT);
    &SETTINGS
}

/// Sets the `errstring`, replacing what it held.
pub(super) fn set_error(message: &str) {
    let mut errstring = lock(errstring());
    errstring.clear();
    errstring.push_str(message);
}

/// The `errstring`, which reading clears.
pub(super) fn take_error() -> String {
    std::mem::take(&mut *lock(errstring()))
}

/// The current settings.
pub(super) fn settings() -> Settings {
    lock(settings_slot()).clone()
}

/// Changes the settings.
pub(super) fn update(change: impl FnOnce(&mut Settings)) {
    change(&mut lock(settings_slot()));
}

/// Test support: CoolProp's default settings again.
#[cfg(test)]
pub(super) fn reset() {
    *lock(settings_slot()) = Settings::DEFAULT;
}
