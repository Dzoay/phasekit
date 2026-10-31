#!/usr/bin/env bash
# Prints this checkout's cargo target directory, one of a few that every session on the machine shares (user decision
# SP4): ${XDG_CACHE_HOME:-~/.cache}/phasekit-target-1 … -N, N = $PHASEKIT_TARGET_SLOTS (default 3). Every shell command
# that runs cargo starts with
#
#   export CARGO_TARGET_DIR=$(scripts/target-dir.sh)
#
# and never writes the path down. A directory belongs to one checkout at a time (its `.owner` file): cargo judges a build
# fresh by file times, so in a directory another checkout built in, this checkout's crates could pass for built when
# they hold the other checkout's code. On taking a directory over, the script deletes the workspace crates'
# fingerprints, so they rebuild and the dependencies do not. It takes the directory this checkout owns, else one whose
# checkout is gone, else one not yet made, else the least recently used if no command has used it for an hour; failing
# that it exits 1, the variable is empty and cargo refuses to run. Linux: flock and GNU stat.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cache=${XDG_CACHE_HOME:-$HOME/.cache}
mkdir -p "$cache"
exec 9>"$cache/phasekit-target.lock"
flock 9

dirs=()
for i in $(seq "${PHASEKIT_TARGET_SLOTS:-3}"); do dirs+=("$cache/phasekit-target-$i"); done
owner() { cat "$1/.owner" 2>/dev/null || true; }

take() {
  # The fingerprints sit in `.fingerprint/` under each profile, target and nested target directory (the gates' `simd`
  # and `shim`). A directory without one, cargo's layout or not, starts empty.
  if [ -n "$(find "$1" -mindepth 2 -maxdepth 4 -type d -name .fingerprint -print -quit 2>/dev/null)" ]; then
    find "$1" -mindepth 3 -maxdepth 5 -type d -path '*/.fingerprint/phasekit-*' -prune -exec rm -rf {} +
  else
    rm -rf "$1"
  fi
  mkdir -p "$1"
  printf '%s\n' "$root" > "$1/.owner"
  echo "target-dir.sh: $1 now serves $root; its workspace crates rebuild" >&2
  echo "$1"
  exit 0
}

for d in "${dirs[@]}"; do
  if [ "$(owner "$d")" = "$root" ]; then
    touch "$d/.owner"
    echo "$d"
    exit 0
  fi
done
for d in "${dirs[@]}"; do
  if [ -d "$d" ] && [ ! -d "$(owner "$d")" ]; then take "$d"; fi
done
for d in "${dirs[@]}"; do
  if [ ! -e "$d" ]; then take "$d"; fi
done
now=$(date +%s) oldest="" idle=0
for d in "${dirs[@]}"; do
  age=$((now - $(stat -c %Y "$d/.owner")))
  if [ "$age" -gt "$idle" ]; then oldest=$d idle=$age; fi
done
if [ "$idle" -ge 3600 ]; then take "$oldest"; fi
{
  echo "target-dir.sh: all ${#dirs[@]} shared target directories were used within the hour:"
  for d in "${dirs[@]}"; do echo "  $d: $(owner "$d")"; done
  echo "Remove a finished checkout (git worktree remove), wait, or raise PHASEKIT_TARGET_SLOTS."
} >&2
exit 1
