#!/usr/bin/env bash
# c-library.sh <version> <target>: the C library's release archive (PLAN.md §5.4 item 7) from the release builds of
# `phasekit-capi` in target/ (`pk_*`) and target/shim/ (`--features coolproplib-shim`): the header, both libraries (the
# shim under its own name, user decision P1), the licences and cargo-about's notices, as
# dist/phasekit-<version>-<target>.tar.gz (.zip on Windows). Run from the repository root.
set -euo pipefail
version=${1:?version}
target=${2:?target}
name=phasekit-$version-$target
out=dist/$name
rm -rf "$out" && mkdir -p "$out/include" "$out/lib"
cp crates/phasekit-capi/include/phasekit.h "$out/include/"
case $target in
  *windows*)
    # The shim's DLL is renamed and has no import library: hosts load it by path, as they load CoolProp's.
    cp target/release/phasekit_capi.dll target/release/phasekit_capi.dll.lib "$out/lib/"
    cp target/shim/release/phasekit_capi.dll "$out/lib/phasekit_coolproplib.dll"
    ;;
  *apple*)
    cp target/release/libphasekit_capi.dylib "$out/lib/"
    cp target/shim/release/libphasekit_capi.dylib "$out/lib/libphasekit_coolproplib.dylib"
    ;;
  *)
    cp target/release/libphasekit_capi.so "$out/lib/"
    cp target/shim/release/libphasekit_capi.so "$out/lib/libphasekit_coolproplib.so"
    ;;
esac
cp LICENSE-MIT LICENSE-APACHE LICENSE-THIRD-PARTY "$out/"
cargo about generate --offline --fail --manifest-path crates/phasekit-capi/Cargo.toml --target "$target" -o "$out/THIRD-PARTY-CRATES.txt" about.hbs
cat > "$out/README.txt" <<EOF
phasekit $version, the C library for $target.

include/phasekit.h      the pk_* ABI
lib/                    phasekit_capi (pk_*) and phasekit_coolproplib (the Tier A CoolPropLib.h shim)

The shim and how to point a host at it: docs/MIGRATION.md in https://github.com/Dzoay/phasekit at v$version.
EOF
case $target in
  *windows*) (cd dist && 7z a -tzip "$name.zip" "$name" >/dev/null) ;;
  *) tar -C dist -czf "dist/$name.tar.gz" "$name" ;;
esac
echo "dist/$name"
