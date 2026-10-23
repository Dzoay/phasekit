#!/usr/bin/env bash
# npm-package.sh <version>: the browser package's npm tarball (PLAN.md §5.4 item 7): the two release modules of
# wasm-modules.sh (baseline and `+simd128`) as `phasekit` and `phasekit/simd`, the licences and cargo-about's notices, as
# dist/phasekit-<version>.tgz. Run from the repository root; needs wasm-bindgen-cli at the locked version, cargo-about
# and npm.
set -euo pipefail
version=${1:?version}
out=dist/npm
rm -rf "$out" && mkdir -p "$out"
"$(dirname "$0")/wasm-modules.sh" "$out"
cp LICENSE-MIT LICENSE-APACHE LICENSE-THIRD-PARTY "$out/"
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

A refused call throws an \`Error\` whose message is in SI, \`temperature 0 K is not above absolute zero\`. It also
carries \`code\` (\`invalid_input\`, \`below_min_temperature\`, …), an invalid input's \`rule\` (\`not_positive\`,
\`out_of_range\`, …), \`quantity\`, \`dimension\` and \`values\` (SI): read those, not the message, to act on a refusal.
\`refusal.explain(units, style)\` writes the message in a page's units, in words (\`"long"\`) or as the relation the
value breaks (\`"short"\`): \`units\` maps a dimension (\`temperature\`, \`pressure\`, \`specificEnergy\`, …) to
\`{ label, scale, offset }\`, SI = value · scale + offset, so \`{ temperature: { label: "°C", scale: 1, offset: 273.15 } }\`
gives \`T = −273.15 °C ≤ −273.15 °C\`; a dimension it lacks is written in SI, and a third argument, a fluid's name, writes
a domain limit as its (\`above Water's maximum\`). \`remedies\` says what the caller can do (\`extrapolate\`,
\`choose_root\`, \`saturated_phases\`, \`other_pair\`, \`nearest_name\`, …), \`limit\` (\`"max"\` or \`"min"\`) which end of a
range the value lies beyond, \`refusal.last\` where a stalled solve stopped; \`registry.nearest(name, k)\` offers the
fluid names nearest an unknown one, and \`fluid.evaluateDetailed(...)\` is \`evaluate\` with each point's refusal as
\`codes\`, \`rules\`, \`limits\` and \`numbers\`.

Free the handle objects (\`Registry\`, \`Fluid\`) with \`free()\`. A panic, a bug, ends the module: its call throws an
\`Error\` naming it, \`panicked()\` then returns its message, and every later call refuses; load the module anew.
Documentation: https://github.com/Dzoay/phasekit.
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
