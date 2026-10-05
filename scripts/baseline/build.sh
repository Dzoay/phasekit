#!/usr/bin/env bash
# The C++ CoolProp baseline (VERIFICATION.md §12, PLAN.md M1.15): builds CoolProp v8.0.0 Release (no -march, like the
# wheels) and the harness from a throwaway clone of reference/CoolProp, then runs the 7 workloads x 5 fluids on one
# core and writes crates/phasekit-verify/benches/baseline/coolprop-8.0.0-<machine>.csv.
#
# Usage: SCRATCH=<directory outside the repository> scripts/baseline/build.sh   (cargo xtask baseline sets SCRATCH)
#
# reference/CoolProp is never configured in place: configure runs dev/generate_headers.py, which writes include/*.h,
# dev/hashes.json, .version and dev/all_fluids.json into its source tree (CMakeLists.txt:557-559). CoolProp fetches its
# packages at configure time (map 12 R16); CPM_SOURCE_CACHE keeps them in SCRATCH.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
SCRATCH=${SCRATCH:?set SCRATCH to a scratch directory outside the repository}
case "$(cd "$SCRATCH" 2>/dev/null && pwd || echo "$SCRATCH")/" in
"$ROOT"/*) echo "build.sh: SCRATCH must be outside the repository" >&2; exit 2 ;;
esac
COOLPROP_SHA=ae81610e7d23efc57f9d051c8e70a4d66e87537f
REF="$ROOT/reference/CoolProp"
if [ "$(git -C "$REF" rev-parse HEAD 2>/dev/null)" != "$COOLPROP_SHA" ]; then
  echo "build.sh: $REF is not at $COOLPROP_SHA (run scripts/fetch-coolprop.sh)" >&2
  exit 2
fi

SRC="$SCRATCH/coolprop-src"
BUILD="$SCRATCH/coolprop-build"
mkdir -p "$SCRATCH"
[ -d "$SRC/.git" ] || git clone --quiet --shared --no-checkout "$REF" "$SRC"
git -C "$SRC" checkout --quiet --detach "$COOLPROP_SHA"
CPM_SOURCE_CACHE="$SCRATCH/cpm" cmake -S "$ROOT/scripts/baseline" -B "$BUILD" -DCMAKE_BUILD_TYPE=Release \
  -DCOOLPROP_SRC="$SRC" -DCOOLPROP_STATIC_LIBRARY=ON
cmake --build "$BUILD" --target coolprop_baseline -j "$(nproc)"

cpu=$(grep -m1 '^model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')
machine=$(printf '%s' "$cpu" | sed -E 's/\((R|TM)\)//g; s/ CPU.*//; s/ +/-/g' | tr '[:upper:]' '[:lower:]')
governor=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)
out="$ROOT/crates/phasekit-verify/benches/baseline/coolprop-8.0.0-$machine.csv"
mkdir -p "$(dirname "$out")"
pin=()
command -v taskset >/dev/null && pin=(taskset -c 2)
"${pin[@]}" "$BUILD/coolprop_baseline" "$out" "$cpu" "$(uname -sr)" "$governor" "$(date -u +%Y-%m-%d)"
echo "build.sh: wrote $out"
