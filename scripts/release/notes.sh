#!/usr/bin/env bash
# notes.sh <version>: the GitHub release's notes (PLAN.md §5.4 item 8): the version's CHANGELOG.md section, then where
# the user decisions in force and the known divergences are, at the tag. Run from the repository root.
set -euo pipefail
version=${1:?version}
section=$(awk -v head="## $version " 'index($0, head) == 1 { on = 1; next } on && /^## / { exit } on' CHANGELOG.md)
[ -n "$section" ] || { echo "notes.sh: CHANGELOG.md has no section for $version" >&2; exit 1; }
base=https://github.com/Dzoay/phasekit/blob/v$version
cat <<EOF
phasekit $version.

- The user decisions in force: [docs/DECISIONS.md]($base/docs/DECISIONS.md).
- The known divergences from CoolProp 8.0.0, each with its evidence and test: the divergence register,
  [crates/phasekit-verify/src/register.rs]($base/crates/phasekit-verify/src/register.rs).
- Migrating from CoolProp, the C shim included: [docs/MIGRATION.md]($base/docs/MIGRATION.md).
$section
EOF
