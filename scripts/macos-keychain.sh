#!/usr/bin/env bash
# CI only: imports the Developer ID certificates into a temporary keychain for scripts/package-macos.sh.
#
#   MACOS_CERTIFICATES_P12_BASE64  both certificates with their keys ("Developer ID Application" and
#                                  "Developer ID Installer"), exported from Keychain Access as one .p12, base64
#   MACOS_CERTIFICATES_PASSWORD    the .p12's password
set -euo pipefail

: "${MACOS_CERTIFICATES_P12_BASE64:?}"
keychain=${RUNNER_TEMP:-/tmp}/signing.keychain-db
password=$(openssl rand -hex 16)
p12=${RUNNER_TEMP:-/tmp}/signing.p12
printf '%s' "$MACOS_CERTIFICATES_P12_BASE64" | base64 --decode > "$p12"
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security import "$p12" -k "$keychain" -P "${MACOS_CERTIFICATES_PASSWORD:-}" -T /usr/bin/codesign -T /usr/bin/productbuild
security set-key-partition-list -S apple-tool:,apple: -s -k "$password" "$keychain" > /dev/null
# Keep the user's keychains in the search list (bash 3.2: no mapfile).
keychains=()
while IFS= read -r line; do keychains+=("$line"); done < <(security list-keychains -d user | tr -d '" ')
security list-keychains -d user -s "$keychain" ${keychains[@]+"${keychains[@]}"}
rm -f "$p12"
security find-identity -v "$keychain"
