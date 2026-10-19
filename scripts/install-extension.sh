#!/usr/bin/env bash
# Builds the extension in release mode and installs it into REAPER's UserPlugins.
# REAPER_RESOURCE overrides the resource directory. Restart REAPER afterwards.
set -euo pipefail
cd "$(dirname "$0")/.."

case "$(uname -s)" in
  Linux)
    dir=${REAPER_RESOURCE:-$HOME/.config/REAPER}
    built=libreaper_bandcontrol.so
    ext=so
    ;;
  Darwin)
    dir=${REAPER_RESOURCE:-$HOME/Library/Application Support/REAPER}
    built=libreaper_bandcontrol.dylib
    ext=dylib
    ;;
  *)
    echo "Build with 'cargo build --release -p reaper-bandcontrol' and copy target/release/reaper_bandcontrol.dll" \
      "into REAPER's UserPlugins directory." >&2
    exit 1
    ;;
esac

cargo build --release -p reaper-bandcontrol --lib
mkdir -p "$dir/UserPlugins"
# A running REAPER may have the old library mapped: replace the file instead of overwriting it.
tmp=$(mktemp "$dir/UserPlugins/.reaper_bandcontrol.XXXXXX")
cp "target/release/$built" "$tmp"
chmod 755 "$tmp"
mv -f "$tmp" "$dir/UserPlugins/reaper_bandcontrol.$ext"
echo "Installed $dir/UserPlugins/reaper_bandcontrol.$ext, restart REAPER to load it."
