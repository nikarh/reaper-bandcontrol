#!/usr/bin/env bash
# Builds the Windows installer for one architecture, signed when a code signing certificate is configured.
#
#   scripts/package-windows.sh ARCH [DLL] [OUTDIR]
#
# ARCH is x64 or arm64 (REAPER for Windows on ARM is an ARM64EC build). Without DLL the library is built here, which
# needs Windows; on other systems pass a DLL built elsewhere (e.g. the CI artifact). OUTDIR defaults to dist.
# Writes OUTDIR/bandcontrol-windows-ARCH-setup.exe. Needs makensis (NSIS 3.08 or newer).
#
# Signing, optional (see scripts/sign-windows.sh): WINDOWS_SIGN_PFX, WINDOWS_SIGN_PASSWORD, WINDOWS_SIGN_TIMESTAMP.
set -euo pipefail
cd "$(dirname "$0")/.."

arch=${1:?usage: package-windows.sh x64|arm64 [DLL] [OUTDIR]}
dll=${2:-}
out=${3:-dist}
case $arch in
  x64) target=x86_64-pc-windows-msvc ;;
  arm64) target=arm64ec-pc-windows-msvc ;;
  *) echo "unknown architecture $arch (x64 or arm64)" >&2 && exit 1 ;;
esac
if [[ -z $dll ]]; then
  cargo build --release --locked -p reaper-bandcontrol --lib --target "$target"
  dll=target/$target/release/reaper_bandcontrol.dll
fi
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)${BANDCONTROL_VERSION_SUFFIX:-}

# makensis on Windows wants Windows paths.
native() { if command -v cygpath > /dev/null; then cygpath -w "$1"; else echo "$1"; fi; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$dll" "$work/reaper_bandcontrol.dll"
mkdir -p "$out"
setup="$(cd "$out" && pwd)/bandcontrol-windows-$arch-setup.exe"
sign=()
if [[ -n ${WINDOWS_SIGN_PFX:-} ]]; then
  scripts/sign-windows.sh "$work/reaper_bandcontrol.dll"
  sign=("-DSIGN=bash \"$(native "$PWD/scripts/sign-windows.sh")\"")
else
  echo "WINDOWS_SIGN_PFX not set: the installer is not signed" >&2
fi
makensis -V2 -DVERSION="$version" -DARCH="$arch" -DDLL="$(native "$work/reaper_bandcontrol.dll")" \
  -DOUT="$(native "$setup")" "${sign[@]}" "$(native packaging/windows/bandcontrol.nsi)"
echo "$setup"
