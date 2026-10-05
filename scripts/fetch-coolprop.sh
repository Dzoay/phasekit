#!/usr/bin/env bash
# Fetch the pinned, read-only CoolProp checkout into reference/CoolProp.
# The pin MUST match the CoolProp PyPI wheel used to generate test fixtures
# (the wheel reports this exact SHA as CoolProp.__gitrevision__).
set -euo pipefail

COOLPROP_TAG="v8.0.0"
COOLPROP_SHA="ae81610e7d23efc57f9d051c8e70a4d66e87537f"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/reference/CoolProp"

if [ ! -d "$DEST/.git" ]; then
  git clone https://github.com/CoolProp/CoolProp "$DEST"
fi
git -C "$DEST" fetch --tags origin
git -C "$DEST" checkout --quiet --detach "$COOLPROP_SHA"

# Fail closed: never leave a checkout that silently differs from the pin.
if [ "$(git -C "$DEST" rev-parse HEAD)" != "$COOLPROP_SHA" ]; then
  echo "error: reference/CoolProp is not at $COOLPROP_SHA" >&2
  exit 1
fi
echo "CoolProp $COOLPROP_TAG ($COOLPROP_SHA) at $DEST"
