#!/usr/bin/env bash
# wasm-modules.sh <dir>: the browser package's two release modules, as the npm package ships them (PLAN.md §5.4 item 7)
# and the site runs them (M21.1): `phasekit-wasm` for wasm32-unknown-unknown, the baseline and the `+simd128` build with
# `--features simd` (M12.10), each through wasm-bindgen's ES-module output with weak references (ROT-158) into
# <dir>/baseline and <dir>/simd, and cargo-about's notices of the simd build's tree, which holds the baseline's, as
# <dir>/THIRD-PARTY-CRATES.txt. Run from the repository root; needs wasm-bindgen-cli at the locked version and
# cargo-about. The builds go to CARGO_TARGET_DIR (default target/), the simd one to its simd/ subdirectory.
set -euo pipefail
dir=${1:?dir}
target=${CARGO_TARGET_DIR:-target}
cargo build --release --locked -p phasekit-wasm --target wasm32-unknown-unknown
RUSTFLAGS="-C target-feature=+simd128" cargo build --release --locked -p phasekit-wasm --features simd \
  --target wasm32-unknown-unknown --target-dir "$target/simd"
rm -rf "$dir/baseline" "$dir/simd"
mkdir -p "$dir"
for build in baseline simd; do
  from=$target
  [ "$build" = simd ] && from=$target/simd
  wasm-bindgen --target web --weak-refs --out-name phasekit --out-dir "$dir/$build" \
    "$from/wasm32-unknown-unknown/release/phasekit_wasm.wasm"
done
# cargo-about reads `cargo metadata` of the whole workspace, which needs every locked crate on disk, dev-dependencies
# included; the builds above fetched only theirs.
cargo fetch --locked
cargo about generate --offline --fail --manifest-path crates/phasekit-wasm/Cargo.toml --target wasm32-unknown-unknown \
  --features simd -o "$dir/THIRD-PARTY-CRATES.txt" about.hbs
