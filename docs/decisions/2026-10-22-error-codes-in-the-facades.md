# Error codes in the facades, and the m18-start tag (2026-10-22)

Asked once core had each error's code and rule (user decision EM11) and the browser package read them: the C ABI
(`pk_*`) and the CoolProp-compatible API (`phasekit-compat`) still gave only a status number and the message, so a C
caller or an agent behind either would parse text for what core already states. Also asked for the approval EM5 needs
before `m18-start` is tagged.

| # | Question | Decision | Notes |
|---|---|---|---|
| EM12 | Do the C ABI and the CoolProp-compatible API expose the codes? | **Yes, in M28.2, beside the browser package's: `CompatError::code()` and `CompatError::rule()`, core's for a core error and compat's own for the rest; `pk_last_error_code` and `pk_last_error_rule` copy out the code and rule of the thread's last `pk_*` call, as `pk_last_error` does its message** | "C and coolprop compatible should expose the new error codes". M28.2 touches no core, so it keeps its place beside M18.11 (EM5). The browser package reads compat's codes, so the three facades share one vocabulary. The Tier A shim keeps CoolProp's API, which has no codes (D11). |
| EM13 | May `m18-start` be tagged on M28.1's merge commit? | **Yes** | "OK to tag m18-start on M28.1's merge commit". The approval EM5 asked for. |
