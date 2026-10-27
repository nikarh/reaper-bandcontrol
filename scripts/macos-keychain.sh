#!/usr/bin/env bash
# CI only: imports the signing certificates into a temporary keychain for scripts/package-macos.sh. Either or both:
#
#   MACOS_CERTIFICATES_P12_BASE64  both Developer ID certificates with their keys ("Developer ID Application" and
#                                  "Developer ID Installer"), exported from Keychain Access as one .p12, base64
#   MACOS_CERTIFICATES_PASSWORD    the .p12's password
#   MACOS_SELF_SIGNED_P12_BASE64   the project's self-signed code signing certificate with its key, .p12, base64:
#                                  without Developer ID it signs BandControl.app, so the app's signature stays the
#                                  same across releases and macOS keeps its Bluetooth permission after updates
#   MACOS_SELF_SIGNED_PASSWORD     its password
#
# Writes MACOS_SELF_SIGNED_IDENTITY (the certificate's SHA-1, which codesign takes for a certificate macOS doesn't
# trust) to $GITHUB_ENV for the next steps.
set -euo pipefail

keychain=${RUNNER_TEMP:-/tmp}/signing.keychain-db
password=$(openssl rand -hex 16)
p12=${RUNNER_TEMP:-/tmp}/signing.p12
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$password" "$keychain"

import() {
  printf '%s' "$1" | base64 --decode > "$p12"
  security import "$p12" -k "$keychain" -P "$2" -T /usr/bin/codesign -T /usr/bin/productbuild
  rm -f "$p12"
}
if [[ -n ${MACOS_CERTIFICATES_P12_BASE64:-} ]]; then
  import "$MACOS_CERTIFICATES_P12_BASE64" "${MACOS_CERTIFICATES_PASSWORD:-}"
fi
if [[ -n ${MACOS_SELF_SIGNED_P12_BASE64:-} ]]; then
  import "$MACOS_SELF_SIGNED_P12_BASE64" "${MACOS_SELF_SIGNED_PASSWORD:-}"
  identity=$(security find-certificate -c "BandControl (self-signed)" -Z "$keychain" | awk '/^SHA-1/ { print $3 }')
  echo "self-signed certificate $identity"
  if [[ -n ${GITHUB_ENV:-} ]]; then
    echo "MACOS_SELF_SIGNED_IDENTITY=$identity" >> "$GITHUB_ENV"
  fi
fi
security set-key-partition-list -S apple-tool:,apple: -s -k "$password" "$keychain" > /dev/null
# Keep the user's keychains in the search list (bash 3.2: no mapfile).
keychains=()
while IFS= read -r line; do keychains+=("$line"); done < <(security list-keychains -d user | tr -d '" ')
security list-keychains -d user -s "$keychain" ${keychains[@]+"${keychains[@]}"}
security find-identity -v "$keychain"
