#!/usr/bin/env bash
# gates.sh <worktree>: all local gates (PLAN.md §2.4 G1-G8, plus deny, shear, reuse); one line per gate, logs in $LOG.
# Copy before running: never edit a running script (bash reads it as it goes).
ROOT=${PHASEKIT_ROOT:-$HOME/Projects}
export CARGO_TARGET_DIR=${TARGET:?set TARGET}
export PHASEKIT_REFERENCE_DIR=$ROOT/phasekit/reference
export TMPDIR=$HOME/.cache/phasekit-tmp
cd "${1:?worktree}" || exit 1
LOG=${LOG:?set LOG}
mkdir -p $LOG $TMPDIR; fail=0
run() { local name=$1; shift; if "$@" >"$LOG/$name.txt" 2>&1; then echo "ok   $name"; else echo "FAIL $name ($LOG/$name.txt)"; fail=1; fi; }
g3() { cargo nextest run --workspace --profile ci && cargo test --doc --workspace; }
g4() { cargo nextest run --workspace --exclude phasekit-xtask --target wasm32-wasip2 --profile ci && cargo test --doc --workspace --exclude phasekit-xtask --target wasm32-wasip2; }
g7() { local compat=(); [ -d crates/phasekit-compat ] && compat=(-p phasekit-compat); cargo clippy -p phasekit-core "${compat[@]}" --no-default-features --all-targets -- -D warnings; }
run G1 cargo fmt --all --check
run G2 cargo clippy --workspace --all-targets -- -D warnings
run G3 g3
run G4 g4
run G5 cargo check --workspace --target wasm32-unknown-unknown
run G6 cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
run G7 g7
if [ -n "${G8_GATES:-}" ]; then for g in $G8_GATES; do run G8-$g cargo xtask gates $g; done; else run G8 cargo xtask gates all; fi
run deny cargo deny check
run shear cargo shear
run reuse uvx reuse lint
for g in G3 G4; do echo "$g: $(grep -h -E '^ *Summary' $LOG/$g.txt | sed 's/^ *//') + $(grep -h 'test result' $LOG/$g.txt | awk '{s+=$4} END {print s}') doctests"; done
exit $fail
