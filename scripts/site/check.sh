#!/usr/bin/env bash
# check.sh: G9, the site's gate (PLAN.md §2.4, M21.1): the build (build.sh), then prettier, svelte-check, Vitest, the
# page's budget (site/size-budget.txt) and Playwright on Chromium (`npx playwright install chromium` once). Runs from
# anywhere; PK_PREBUILT=1 as for build.sh.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
"$root/scripts/site/build.sh"
cd "$root/site"
npx prettier --check .
npx svelte-check --fail-on-warnings
npx vitest run
node "$root/scripts/site/budget.mjs" dist size-budget.txt
npx playwright test
