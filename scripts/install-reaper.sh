#!/usr/bin/env bash
# Downloads REAPER for Linux into a directory (for CI, where it runs unlicensed) and prints the executable's path.
#
#   scripts/install-reaper.sh DIR [VERSION]   # default: the tested version from Cargo.toml
#
# With REAPER_DOWNLOADS set, the archive is kept in that directory and reused (CI caches it).
set -euo pipefail

dir=${1:?usage: install-reaper.sh DIR [VERSION]}
version=${2:-$("$(dirname "$0")/reaper-version.sh" tested)}
arch=$(uname -m)
major=${version%%.*}
mkdir -p "$dir"
archive=reaper${version//./}_linux_$arch.tar.xz
url=https://www.reaper.fm/files/$major.x/$archive
if [[ -n ${REAPER_DOWNLOADS:-} ]]; then
  mkdir -p "$REAPER_DOWNLOADS"
  if [[ ! -s $REAPER_DOWNLOADS/$archive ]]; then
    curl -sSfL --retry 3 -o "$REAPER_DOWNLOADS/$archive.part" "$url"
    mv "$REAPER_DOWNLOADS/$archive.part" "$REAPER_DOWNLOADS/$archive"
  fi
  tar xJf "$REAPER_DOWNLOADS/$archive" -C "$dir"
else
  curl -sSfL --retry 3 "$url" | tar xJ -C "$dir"
fi
# REAPER 5 calls its executable reaper5.
for exe in reaper reaper5; do
  if [[ -x $dir/reaper_linux_$arch/REAPER/$exe ]]; then
    echo "$dir/reaper_linux_$arch/REAPER/$exe"
    exit 0
  fi
done
echo "no REAPER executable in $dir" >&2
exit 1
