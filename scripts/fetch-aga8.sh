#!/usr/bin/env bash
# Fetch NIST's AGA8 code (public domain) into reference/AGA8 at the pinned commit (user decision MX7): GERG-2008's
# coefficients are transcribed from its AGA8CODE/C/GERG2008.cpp, which `cargo xtask`'s `gerg_2008_is_aga8s` checks
# number for number, and scripts/oracle/teqp.py reads its test data (PLAN.md M13.5). Read-only, like reference/CoolProp.
set -euo pipefail

AGA8_SHA="3bdb9ab8ff317c618b0b59d1b704c2c86ddc5fce"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/reference/AGA8"

if [ ! -d "$DEST/.git" ]; then
  git clone https://github.com/usnistgov/AGA8 "$DEST"
fi
git -C "$DEST" fetch origin
git -C "$DEST" checkout --quiet --detach "$AGA8_SHA"

# Fail closed: never leave a checkout that silently differs from the pin.
if [ "$(git -C "$DEST" rev-parse HEAD)" != "$AGA8_SHA" ]; then
  echo "error: reference/AGA8 is not at $AGA8_SHA" >&2
  exit 1
fi
echo "AGA8 ($AGA8_SHA) at $DEST"
