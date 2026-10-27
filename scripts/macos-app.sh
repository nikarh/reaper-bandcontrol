#!/usr/bin/env bash
# Assembles BandControl.app, the macOS app that gives the REAPER extension Bluetooth, from its binary.
#
#   scripts/macos-app.sh BINARY OUTDIR
#
# Writes OUTDIR/BandControl.app. Signed with MACOS_SIGN_IDENTITY ("Developer ID Application: …") when set, otherwise
# with MACOS_SELF_SIGNED_IDENTITY (the project's self-signed certificate, see scripts/macos-keychain.sh), otherwise ad
# hoc (Apple silicon only runs signed code). macOS ties the Bluetooth permission to the signature: an ad-hoc one
# changes with every build, so macOS asks again after each update. Needs macOS (sips, iconutil, codesign).
set -euo pipefail
cd "$(dirname "$0")/.."

binary=$1
out=$2
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
macos_min=$(sed -n 's/^MACOSX_DEPLOYMENT_TARGET *= *"\(.*\)"/\1/p' .cargo/config.toml)

# The binary must run on every macOS the Info.plist promises, and every slice of a universal binary alike.
minos=$(otool -l "$binary" | awk '/LC_BUILD_VERSION/ { b = 1 } b && /minos/ { print $2; b = 0 }' | sort -u)
if [[ $minos != "$macos_min" ]]; then
  echo "$binary is built for macOS ${minos//$'\n'/, }, the app promises $macos_min (.cargo/config.toml)" >&2
  exit 1
fi

app="$out/BandControl.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/BandControl"
chmod 755 "$app/Contents/MacOS/BandControl"
sed -e "s/MACOS_MIN/$macos_min/" -e "s/VERSION/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"

# The app icon, from the Android app's.
icons=$(mktemp -d)
trap 'rm -rf "$icons"' EXIT
iconset="$icons/AppIcon.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z $size $size fastlane/metadata/android/en-US/images/icon.png --out "$iconset/icon_${size}x${size}.png" > /dev/null
  double=$((size * 2))
  if [[ $double -le 512 ]]; then
    sips -z $double $double fastlane/metadata/android/en-US/images/icon.png \
      --out "$iconset/icon_${size}x${size}@2x.png" > /dev/null
  fi
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"

if [[ -n ${MACOS_SIGN_IDENTITY:-} ]]; then
  codesign --force --timestamp --options runtime --sign "$MACOS_SIGN_IDENTITY" "$app"
elif [[ -n ${MACOS_SELF_SIGNED_IDENTITY:-} ]]; then
  codesign --force --options runtime --sign "$MACOS_SELF_SIGNED_IDENTITY" "$app"
  echo "BandControl.app is signed with the self-signed certificate" >&2
else
  codesign --force --sign - "$app"
  echo "MACOS_SIGN_IDENTITY not set: BandControl.app is only ad-hoc signed" >&2
fi
codesign --verify --strict "$app"
echo "$app"
