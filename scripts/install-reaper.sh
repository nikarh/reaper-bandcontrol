#!/usr/bin/env bash
# Downloads REAPER for Linux into a directory (for CI, where it runs unlicensed) and prints the executable's path.
#
#   scripts/install-reaper.sh DIR [VERSION]   # default: the tested version from Cargo.toml
set -euo pipefail

dir=${1:?usage: install-reaper.sh DIR [VERSION]}
version=${2:-$("$(dirname "$0")/reaper-version.sh" tested)}
arch=$(uname -m)
major=${version%%.*}
mkdir -p "$dir"
curl -sSfL "https://www.reaper.fm/files/$major.x/reaper${version//./}_linux_$arch.tar.xz" | tar xJ -C "$dir"
# REAPER 5 calls its executable reaper5.
for exe in reaper reaper5; do
  if [[ -x $dir/reaper_linux_$arch/REAPER/$exe ]]; then
    echo "$dir/reaper_linux_$arch/REAPER/$exe"
    exit 0
  fi
done
echo "no REAPER executable in $dir" >&2
exit 1
