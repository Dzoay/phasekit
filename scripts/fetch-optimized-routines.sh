#!/usr/bin/env bash
# Fetch Arm's optimized-routines (MIT OR Apache-2.0 WITH LLVM-exception) into reference/optimized-routines at the
# pinned release, v26.07 (user decision IM5): core's in-house exp, ln and pow port its math/exp.c, log.c and pow.c
# (PLAN.md M23.2-M23.4), and scripts/math/reference.py copies its directed test cases into fixtures/math (M23.1).
# Read-only, like reference/CoolProp. Never glibc's copy, which is LGPL (PLAN.md §5.3).
set -euo pipefail

ARM_SHA="4be260a5117480382690c6d8c300bc784e927d76"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/reference/optimized-routines"

if [ ! -d "$DEST/.git" ]; then
  git clone https://github.com/ARM-software/optimized-routines "$DEST"
fi
git -C "$DEST" fetch origin
git -C "$DEST" checkout --quiet --detach "$ARM_SHA"

# Fail closed: never leave a checkout that silently differs from the pin.
if [ "$(git -C "$DEST" rev-parse HEAD)" != "$ARM_SHA" ]; then
  echo "error: reference/optimized-routines is not at $ARM_SHA" >&2
  exit 1
fi
echo "optimized-routines v26.07 ($ARM_SHA) at $DEST"
