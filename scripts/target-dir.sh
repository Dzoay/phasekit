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
# that it exits 1, the variable is empty and cargo refuses to run.
#
# It also cleans up, so that no machine needs a timer for it. Once a day it removes every `phasekit-target-*` directory
# with nothing in it changed for 7 days, and empties the directory it hands out if that has grown past
# $PHASEKIT_TARGET_MAX_GB (default 20; `du`) and no command has used it for an hour, as cargo never removes what an old
# toolchain or dependency built.
# Linux: flock, GNU find and stat.
set -euo pipefail
shopt -s nullglob

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cache=${XDG_CACHE_HOME:-$HOME/.cache}
mkdir -p "$cache"
exec 9>"$cache/phasekit-target.lock"
flock 9

# `$1` was touched within the day.
recent() { [ -n "$(find "$1" -maxdepth 0 -mtime -1 2>/dev/null)" ]; }

if ! recent "$cache/phasekit-target.swept"; then
  for d in "$cache"/phasekit-target-*/; do
    d=${d%/}
    if [ ! -L "$d" ] && [ -z "$(find "$d" -newermt '7 days ago' -print -quit 2>/dev/null)" ]; then
      rm -rf "$d"
      echo "target-dir.sh: removed $d, unused for 7 days" >&2
    fi
  done
  touch "$cache/phasekit-target.swept"
fi

dirs=()
for i in $(seq "${PHASEKIT_TARGET_SLOTS:-3}"); do dirs+=("$cache/phasekit-target-$i"); done
owner() { cat "$1/.owner" 2>/dev/null || true; }

serve() {
  # Only a directory no command has used for an hour, which no other command in this checkout can still be building
  # in; a busy one is checked on a later call.
  if ! recent "$1/.sized" && [ $(($(date +%s) - $(stat -c %Y "$1/.owner"))) -ge 3600 ]; then
    if [ "$(du -sk "$1" | cut -f1)" -gt $((${PHASEKIT_TARGET_MAX_GB:-20} * 1024 * 1024)) ]; then
      find "$1" -mindepth 1 -maxdepth 1 ! -name .owner -exec rm -rf {} +
      echo "target-dir.sh: emptied $1, past ${PHASEKIT_TARGET_MAX_GB:-20} GB; everything rebuilds" >&2
    fi
    touch "$1/.sized"
  fi
  touch "$1/.owner"
  echo "$1"
  exit 0
}

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
  serve "$1"
}

for d in "${dirs[@]}"; do
  if [ "$(owner "$d")" = "$root" ]; then serve "$d"; fi
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
