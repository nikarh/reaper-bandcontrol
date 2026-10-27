#!/usr/bin/env bash
# Builds the macOS installer (one .pkg for Apple silicon and Intel) with the REAPER extension and BandControl.app, and
# the app alone as a zip for manual installs. Signed and notarized when credentials are set.
#
#   scripts/package-macos.sh [DYLIB APP] [OUTDIR]
#
# DYLIB is the universal extension, APP the universal BandControl binary (crate bandcontrol-app); without them both
# are built here. OUTDIR defaults to dist; writes OUTDIR/bandcontrol-macos.pkg and OUTDIR/BandControl-macos.zip.
# Needs macOS (pkgbuild, productbuild, codesign, sips, iconutil).
#
# Signing, optional; the certificates must be in a keychain (CI imports them with scripts/macos-keychain.sh):
#   MACOS_SIGN_IDENTITY        "Developer ID Application: …", signs the library and the app
#   MACOS_INSTALLER_IDENTITY   "Developer ID Installer: …", signs the package
# Notarization, optional, needs both identities (so Gatekeeper opens the package and the app without a warning):
#   MACOS_NOTARY_APPLE_ID, MACOS_NOTARY_TEAM_ID, MACOS_NOTARY_PASSWORD (an app-specific password)
#
# Runs with macOS's own bash 3.2 too: no mapfile, and empty arrays only as ${array[@]+"${array[@]}"} under set -u.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ $# -ge 2 ]]; then
  dylib=$1
  app_bin=$2
  out=${3:-dist}
else
  out=${1:-dist}
  libs=()
  bins=()
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked -p reaper-bandcontrol --lib -p bandcontrol-app --target "$target"
    libs+=("target/$target/release/libreaper_bandcontrol.dylib")
    bins+=("target/$target/release/BandControl")
  done
  dylib=target/reaper_bandcontrol-macos-universal.dylib
  app_bin=target/BandControl-macos-universal
  lipo -create -output "$dylib" "${libs[@]}"
  lipo -create -output "$app_bin" "${bins[@]}"
fi
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
mkdir -p "$out"
out=$(cd "$out" && pwd)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

notarize() {
  xcrun notarytool submit "$1" --apple-id "$MACOS_NOTARY_APPLE_ID" --team-id "$MACOS_NOTARY_TEAM_ID" \
    --password "$MACOS_NOTARY_PASSWORD" --wait
}
notarizing=
if [[ -n ${MACOS_NOTARY_APPLE_ID:-} && -n ${MACOS_SIGN_IDENTITY:-} && -n ${MACOS_INSTALLER_IDENTITY:-} ]]; then
  notarizing=1
fi

# The extension, into the current user's REAPER folder.
plugins="$work/extension/Library/Application Support/REAPER/UserPlugins"
mkdir -p "$plugins"
cp "$dylib" "$plugins/reaper_bandcontrol.dylib"
if [[ -n ${MACOS_SIGN_IDENTITY:-} ]]; then
  codesign --force --timestamp --options runtime --sign "$MACOS_SIGN_IDENTITY" "$plugins/reaper_bandcontrol.dylib"
else
  # Apple silicon only loads signed code: an ad-hoc signature at least.
  codesign --force --sign - "$plugins/reaper_bandcontrol.dylib"
  echo "MACOS_SIGN_IDENTITY not set: the library is only ad-hoc signed" >&2
fi

# The app, into the current user's Applications folder (~/Applications), where the extension looks for it.
mkdir -p "$work/app/Applications"
scripts/macos-app.sh "$app_bin" "$work/app/Applications" > /dev/null
app="$work/app/Applications/BandControl.app"
zip="$out/BandControl-macos.zip"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
if [[ -n $notarizing ]]; then
  notarize "$zip"
  xcrun stapler staple "$app"
  rm -f "$zip"
  ditto -c -k --keepParent "$app" "$zip"
fi

# Paths are relative to the user's home, see the domains in distribution.xml.
pkgbuild --root "$work/extension" --install-location / --identifier dev.nikarh.bandcontrol.extension \
  --version "$version" --ownership preserve "$work/extension.pkg"
# Installer would otherwise "upgrade" a copy of the app found elsewhere instead of installing into ~/Applications.
pkgbuild --analyze --root "$work/app" "$work/app.plist"
plutil -replace 0.BundleIsRelocatable -bool NO "$work/app.plist"
pkgbuild --root "$work/app" --component-plist "$work/app.plist" --install-location / \
  --identifier dev.nikarh.bandcontrol.app --version "$version" --ownership preserve "$work/app.pkg"

sed "s/VERSION/$version/g" packaging/macos/distribution.xml > "$work/distribution.xml"
pkg="$out/bandcontrol-macos.pkg"
sign=()
if [[ -n ${MACOS_INSTALLER_IDENTITY:-} ]]; then
  sign=(--sign "$MACOS_INSTALLER_IDENTITY" --timestamp)
else
  echo "MACOS_INSTALLER_IDENTITY not set: the package is not signed" >&2
fi
productbuild --distribution "$work/distribution.xml" --resources packaging/macos --package-path "$work" \
  ${sign[@]+"${sign[@]}"} "$pkg"

if [[ -n $notarizing ]]; then
  notarize "$pkg"
  xcrun stapler staple "$pkg"
fi
echo "$pkg"
echo "$zip"
