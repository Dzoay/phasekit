#!/usr/bin/env bash
# build.sh: the site (PLAN.md M21.1) into site/dist. The browser package's two release modules of this checkout go to
# site/src/pk (scripts/release/wasm-modules.sh, so the page runs what the npm package ships), one pack per fluid and
# their index.json to site/public/packs (`cargo xtask packs`, PLAN.md M21.4), the licences and notices to
# site/public/notices, then `npm ci` and `vite build`. With PK_PREBUILT=1 the modules and THIRD-PARTY-CRATES.txt are
# already in site/src/pk and the packs in site/public/packs (CI's `site` job takes them from the `wasm-browser` job).
# PHASEKIT_COMMIT names the commit the footer links (default: HEAD). Runs from anywhere.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
if [ "${PK_PREBUILT:-}" != 1 ]; then
  scripts/release/wasm-modules.sh site/src/pk
  rm -rf site/public/packs
  cargo xtask packs site/public/packs
fi
notices=site/public/notices
rm -rf "$notices"
mkdir -p "$notices"
cp LICENSE-MIT LICENSE-APACHE LICENSE-THIRD-PARTY site/src/pk/THIRD-PARTY-CRATES.txt "$notices/"
{
  echo "Inter, the typeface of this page: Copyright (c) 2016 The Inter Project Authors (https://github.com/rsms/inter)"
  echo
  cat LICENSES/OFL-1.1.txt
} >"$notices/Inter-OFL-1.1.txt"
(cd site && npm ci --no-audit --no-fund)
node scripts/site/notices.mjs site deny.toml LICENSES "$notices/THIRD-PARTY-NPM.txt"
PHASEKIT_COMMIT=${PHASEKIT_COMMIT:-$(git rev-parse HEAD)} npm --prefix site run build
