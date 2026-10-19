#!/usr/bin/env bash
# Builds the macOS installer (one .pkg for Apple silicon and Intel), signed and notarized when credentials are set.
#
#   scripts/package-macos.sh [DYLIB] [OUTDIR]
#
# Without DYLIB the universal library is built here. OUTDIR defaults to dist; writes OUTDIR/bandcontrol-macos.pkg.
# Needs macOS (pkgbuild, productbuild).
#
# Signing, optional; the certificates must be in a keychain (CI imports them with scripts/macos-keychain.sh):
#   MACOS_SIGN_IDENTITY        "Developer ID Application: …", signs the library
#   MACOS_INSTALLER_IDENTITY   "Developer ID Installer: …", signs the package
# Notarization, optional, needs both identities (so Gatekeeper opens the package without a warning):
#   MACOS_NOTARY_APPLE_ID, MACOS_NOTARY_TEAM_ID, MACOS_NOTARY_PASSWORD (an app-specific password)
set -euo pipefail
cd "$(dirname "$0")/.."

dylib=${1:-}
out=${2:-dist}
if [[ -z $dylib ]]; then
  libs=()
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked -p reaper-bandcontrol --lib --target "$target"
    libs+=("target/$target/release/libreaper_bandcontrol.dylib")
  done
  dylib=target/reaper_bandcontrol-macos-universal.dylib
  lipo -create -output "$dylib" "${libs[@]}"
fi
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
plugins="$work/root/Library/Application Support/REAPER/UserPlugins"
mkdir -p "$plugins"
cp "$dylib" "$plugins/reaper_bandcontrol.dylib"
if [[ -n ${MACOS_SIGN_IDENTITY:-} ]]; then
  codesign --force --timestamp --options runtime --sign "$MACOS_SIGN_IDENTITY" "$plugins/reaper_bandcontrol.dylib"
else
  # Apple silicon only loads signed code: an ad-hoc signature at least.
  codesign --force --sign - "$plugins/reaper_bandcontrol.dylib"
  echo "MACOS_SIGN_IDENTITY not set: the library is only ad-hoc signed" >&2
fi

# Paths are relative to the user's home, see the domains in distribution.xml.
pkgbuild --root "$work/root" --install-location / --identifier dev.nikarh.bandcontrol.extension \
  --version "$version" --ownership preserve "$work/extension.pkg"
sed "s/VERSION/$version/" packaging/macos/distribution.xml > "$work/distribution.xml"
mkdir -p "$out"
pkg="$(cd "$out" && pwd)/bandcontrol-macos.pkg"
sign=()
if [[ -n ${MACOS_INSTALLER_IDENTITY:-} ]]; then
  sign=(--sign "$MACOS_INSTALLER_IDENTITY" --timestamp)
else
  echo "MACOS_INSTALLER_IDENTITY not set: the package is not signed" >&2
fi
productbuild --distribution "$work/distribution.xml" --resources packaging/macos --package-path "$work" \
  "${sign[@]}" "$pkg"

if [[ -n ${MACOS_NOTARY_APPLE_ID:-} && ${#sign[@]} -gt 0 ]]; then
  xcrun notarytool submit "$pkg" --apple-id "$MACOS_NOTARY_APPLE_ID" --team-id "$MACOS_NOTARY_TEAM_ID" \
    --password "$MACOS_NOTARY_PASSWORD" --wait
  xcrun stapler staple "$pkg"
fi
echo "$pkg"
