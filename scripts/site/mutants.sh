#!/usr/bin/env bash
# mutants.sh [--all]: StrykerJS over the site's TypeScript (PLAN.md M21.7; user decision WS5), part of G9. By default it
# takes the files changed against origin/main (committed or not), as G8's `gates mutants` takes Rust's diff; with --all
# every file, as the weekly run does. A mutant that survives fails the run (site/stryker.config.json breaks below 100),
# unless a `// Stryker disable` comment beside it gives the reason. A PR's run skips static mutants (module-level data,
# such as the output and unit tables, whose own tests check them directly), each of which re-runs every test; the weekly
# --all run includes them. Not mutated: the browser-only glue (main.ts, the route's hash listener, the engine worker and
# its client, the surfaces' WebGL scene), which Playwright drives, and the type-only files. Incremental: a re-run tests
# only what changed since the last (site/reports/stryker-incremental.json).
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root/site"
skip='^src/(main\.ts|vite-env\.d\.ts|lib/route\.svelte\.ts|engine/(worker|client|pk|protocol)\.ts|surface/scene\.ts|pk/)'
static=(--ignoreStatic)
if [ "${1:-}" = --all ]; then
  static=()
  files=$(git ls-files --cached --others --exclude-standard 'src/**.ts')
else
  base=$(git merge-base origin/main HEAD)
  files=$({
    git diff --relative --name-only --diff-filter=AMR "$base" -- 'src/**.ts'
    git ls-files --others --exclude-standard 'src/**.ts'
  } | sort -u)
fi
files=$(grep -Ev "$skip" <<<"$files" || true)
if [ -z "$files" ]; then
  echo "mutants: no site TypeScript to mutate"
  exit 0
fi
echo "mutants: $(wc -l <<<"$files") files"
npx stryker run "${static[@]}" --mutate "$(paste -sd, <<<"$files")"
