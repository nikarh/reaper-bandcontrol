#!/usr/bin/env bash
# Signs a Windows executable or library in place (Authenticode, with osslsigncode).
#
#   scripts/sign-windows.sh FILE
#
#   WINDOWS_SIGN_PFX        the code signing certificate with its key (.pfx / .p12)
#   WINDOWS_SIGN_PASSWORD   its password
#   WINDOWS_SIGN_TIMESTAMP  timestamp server (default http://timestamp.digicert.com, empty for none), so the signature
#                           stays valid after the certificate expires
set -euo pipefail

file=${1:?usage: sign-windows.sh FILE}
: "${WINDOWS_SIGN_PFX:?WINDOWS_SIGN_PFX is not set}"
timestamp=${WINDOWS_SIGN_TIMESTAMP-http://timestamp.digicert.com}
args=(-pkcs12 "$WINDOWS_SIGN_PFX" -h sha256 -n "BandControl for REAPER" -i https://github.com/nikarh/reaper-bandcontrol)
if [[ -n $timestamp ]]; then
  args+=(-ts "$timestamp")
fi
# The password through a file descriptor, not the command line.
if ! log=$(osslsigncode sign "${args[@]}" -readpass <(printf '%s' "${WINDOWS_SIGN_PASSWORD:-}") \
  -in "$file" -out "$file.signed" 2>&1); then
  echo "$log" >&2
  echo "signing $file failed" >&2
  exit 1
fi
mv "$file.signed" "$file"
osslsigncode verify -in "$file" > /dev/null 2>&1 || echo "note: $file is signed but not trusted here (e.g. a test certificate)" >&2
echo "signed $file" >&2
