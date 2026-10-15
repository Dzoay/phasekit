#!/usr/bin/env bash
# npm-package.sh <version>: the browser package's npm tarball (PLAN.md §5.4 item 7) from the release builds of
# `phasekit-wasm` in target/ (baseline) and target/simd/ (`+simd128`, `--features simd`): wasm-bindgen's ES-module output of each, with
# weak references (ROT-158), `phasekit` and `phasekit/simd`, the licences and cargo-about's notices, as
# dist/phasekit-<version>.tgz. Run from the repository root; needs wasm-bindgen-cli at the locked version and npm.
set -euo pipefail
version=${1:?version}
out=dist/npm
rm -rf "$out" && mkdir -p "$out"
for build in baseline simd; do
  dir=target
  [ "$build" = simd ] && dir=target/simd
  wasm-bindgen --target web --weak-refs --out-name phasekit --out-dir "$out/$build" \
    "$dir/wasm32-unknown-unknown/release/phasekit_wasm.wasm"
done
cp LICENSE-MIT LICENSE-APACHE LICENSE-THIRD-PARTY "$out/"
# The simd build's tree (`--features simd`, PLAN.md M12.10) holds the baseline's and adds fearless_simd and libm.
cargo about generate --offline --fail --manifest-path crates/phasekit-wasm/Cargo.toml --target wasm32-unknown-unknown \
  --features simd -o "$out/THIRD-PARTY-CRATES.txt" about.hbs
cat > "$out/README.md" <<EOF
# phasekit

Thermophysical properties of fluids in the browser: phasekit's WebAssembly package, with CoolProp 8.0.0's five core
fluids embedded and more from packs a page fetches (\`withPack\`). \`phasekit\` is the baseline build, \`phasekit/simd\`
the \`simd128\` one, whose batch (\`Fluid.evaluate\`) runs eight states at a time in WebAssembly SIMD lanes, with the
same results.

\`\`\`js
import init, { Registry } from "phasekit";
await init();
const registry = new Registry();
const t = registry.PropsSI("T", "P", 101325, "Q", 0, "Water");
registry.free();
\`\`\`

Free the handle objects (\`Registry\`, \`Fluid\`) with \`free()\`. Documentation: https://github.com/Dzoay/phasekit.
EOF
cat > "$out/package.json" <<EOF
{
  "name": "phasekit",
  "version": "$version",
  "description": "Thermophysical properties of fluids: phasekit's browser package (WebAssembly)",
  "license": "MIT OR Apache-2.0",
  "repository": { "type": "git", "url": "git+https://github.com/Dzoay/phasekit.git" },
  "type": "module",
  "exports": {
    ".": { "types": "./baseline/phasekit.d.ts", "default": "./baseline/phasekit.js" },
    "./simd": { "types": "./simd/phasekit.d.ts", "default": "./simd/phasekit.js" }
  },
  "files": ["baseline", "simd", "README.md", "LICENSE-MIT", "LICENSE-APACHE", "LICENSE-THIRD-PARTY", "THIRD-PARTY-CRATES.txt"],
  "sideEffects": false
}
EOF
node scripts/release/npm-smoke.mjs "$PWD/$out"
(cd dist && npm pack --silent "./npm" >/dev/null)
echo "dist/phasekit-$version.tgz"
