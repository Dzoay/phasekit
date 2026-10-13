//! Every other target: no FP environment to keep (wasm has none; the others do not trap by default).

/// No FP environment to keep.
pub(crate) struct Env;
pub(super) fn hold() -> Env {
    Env
}
pub(super) fn restore(_env: &Env, _clear: bool) {}
