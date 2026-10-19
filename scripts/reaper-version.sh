#!/usr/bin/env bash
# Prints a REAPER version from [workspace.metadata.reaper] in Cargo.toml.
#
#   scripts/reaper-version.sh min      # the oldest supported REAPER
#   scripts/reaper-version.sh tested   # the newest REAPER the tests run against
#   scripts/reaper-version.sh latest   # the newest REAPER release on reaper.fm
set -euo pipefail
cd "$(dirname "$0")/.."

case ${1:?usage: reaper-version.sh min|tested|latest} in
  min | tested)
    sed -n '/^\[workspace.metadata.reaper\]/,/^\[/s/^'"$1"' *= *"\(.*\)"/\1/p' Cargo.toml
    ;;
  latest)
    # The download page links the current release, e.g. reaper782_linux_x86_64.tar.xz for 7.82.
    digits=$(curl -sSfL https://www.reaper.fm/download.php | grep -oE 'reaper[0-9]+_linux_x86_64\.tar\.xz' | head -1 |
      grep -oE '[0-9]+' | head -1)
    [[ -n $digits ]] || { echo "no REAPER release found on reaper.fm" >&2 && exit 1; }
    echo "${digits:0:1}.${digits:1}"
    ;;
  *) echo "unknown: $1 (min, tested or latest)" >&2 && exit 1 ;;
esac
