#!/usr/bin/env bash
# Check that the tools the standard gates need are installed (docs/PLAN.md M0.1, gates G1-G8 in §2.4).
# Read-only and offline: it only asks each tool for its version. Exits 1 if a required tool is missing.
set -euo pipefail

# Keep in step with the workspace `rust-version` (docs/PLAN.md §2.2).
MIN_RUST_MAJOR=1
MIN_RUST_MINOR=85
TARGETS=(x86_64-unknown-linux-gnu wasm32-unknown-unknown wasm32-wasip2 x86_64-pc-windows-msvc)
INSTALL_CMD="cargo install --locked --root ~/.local wasmtime-cli cargo-deny cargo-shear cargo-mutants cargo-nextest"
# Where `cargo install` may have put a binary that is not on PATH.
BIN_DIRS=("$HOME/.local/bin" "${CARGO_HOME:-$HOME/.cargo}/bin")

failures=0
path_fix=()

ok() { printf 'ok    %s\n' "$*"; }
warn() { printf 'warn  %s\n' "$*"; }
fail() {
  printf 'FAIL  %s\n' "$*"
  failures=$((failures + 1))
}

# Report a binary that is missing from PATH, and remember a PATH fix when it is installed elsewhere.
missing() {
  local bin=$1 hint=$2 dir
  for dir in "${BIN_DIRS[@]}"; do
    if [ -x "$dir/$bin" ]; then
      fail "$bin: installed in $dir but not on PATH"
      path_fix+=("$dir")
      return
    fi
  done
  fail "$bin: not found ($hint)"
}

# check_tool <binary> <hint> <version command...>: the binary is on PATH and reports its version.
check_tool() {
  local bin=$1 hint=$2 version
  shift 2
  if ! command -v "$bin" >/dev/null 2>&1; then
    missing "$bin" "$hint"
  elif version=$("$@" 2>&1 | head -n 1) && [ -n "$version" ]; then
    # Some tools print only the number (cargo-shear: "Version: 1.14.0"); name them.
    [[ $version == *"${bin#cargo-}"* ]] || version="$bin: $version"
    ok "$version"
  else
    fail "$bin: '$*' failed"
  fi
}

# rustc >= the MSRV.
if command -v rustc >/dev/null 2>&1; then
  rustc_version=$(rustc --version)
  if [[ $rustc_version =~ ^rustc\ ([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
    major=${BASH_REMATCH[1]}
    minor=${BASH_REMATCH[2]}
    if ((major > MIN_RUST_MAJOR || (major == MIN_RUST_MAJOR && minor >= MIN_RUST_MINOR))); then
      ok "$rustc_version (>= $MIN_RUST_MAJOR.$MIN_RUST_MINOR)"
    else
      fail "$rustc_version is older than $MIN_RUST_MAJOR.$MIN_RUST_MINOR"
    fi
  else
    fail "rustc: cannot parse '$rustc_version'"
  fi
else
  fail "rustc: not found (install Rust with rustup)"
fi

check_tool cargo "install Rust with rustup" cargo --version
check_tool cargo-clippy "rustup component add clippy" cargo clippy --version
check_tool cargo-fmt "rustup component add rustfmt" cargo fmt --version

# The four targets of the gates: native, browser wasm, wasip2 and the Windows check.
if command -v rustup >/dev/null 2>&1; then
  installed=$(rustup target list --installed)
  for target in "${TARGETS[@]}"; do
    if grep -qx "$target" <<<"$installed"; then
      ok "target $target"
    else
      fail "target $target: not installed (rustup target add $target)"
    fi
  done
else
  fail "rustup: not found, cannot list the installed targets"
fi

check_tool wasmtime "$INSTALL_CMD" wasmtime --version
check_tool cargo-deny "$INSTALL_CMD" cargo deny --version
check_tool cargo-shear "$INSTALL_CMD" cargo shear --version
check_tool cargo-mutants "$INSTALL_CMD" cargo mutants --version
check_tool cargo-nextest "$INSTALL_CMD" cargo nextest --version
check_tool uv "https://docs.astral.sh/uv/" uv --version

# Not needed until the C++ baseline (M1.15): warn only.
for bin in cmake g++; do
  if command -v "$bin" >/dev/null 2>&1; then
    ok "$("$bin" --version | head -n 1)"
  else
    warn "$bin: not found (needed from M1.15 for the C++ baseline)"
  fi
done

if ((${#path_fix[@]} > 0)); then
  printf '\nSome tools are installed but not on PATH. Add this to your shell profile:\n'
  printf "  export PATH=\"%s:\$PATH\"\n" "$(printf '%s\n' "${path_fix[@]}" | sort -u | paste -sd:)"
fi

if ((failures > 0)); then
  printf '\n%d required check(s) failed.\n' "$failures"
  exit 1
fi
printf '\nAll required tools are present.\n'
