#!/usr/bin/env bash
# Downloads REAPER for Linux into a directory (for CI, where it runs unlicensed) and prints the executable's path.
#
#   scripts/install-reaper.sh DIR [VERSION]
set -euo pipefail

dir=${1:?usage: install-reaper.sh DIR [VERSION]}
version=${2:-7.82}
arch=$(uname -m)
major=${version%%.*}
mkdir -p "$dir"
curl -sSfL "https://www.reaper.fm/files/$major.x/reaper${version//./}_linux_$arch.tar.xz" | tar xJ -C "$dir"
echo "$dir/reaper_linux_$arch/REAPER/reaper"
