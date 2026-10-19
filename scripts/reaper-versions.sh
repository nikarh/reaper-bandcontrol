#!/usr/bin/env bash
# Runs the REAPER end-to-end tests against several REAPER releases for Linux, to check the supported range.
#
#   scripts/reaper-versions.sh [VERSION...]   # default: the supported range from Cargo.toml, some in between
#
# Releases are downloaded once into target/reaper-versions/. Prints one line per version.
set -euo pipefail
cd "$(dirname "$0")/.."

versions=("$@")
if [[ ${#versions[@]} -eq 0 ]]; then
  versions=("$(scripts/reaper-version.sh min)" 5.99 6.0 6.29 6.83 7.0 "$(scripts/reaper-version.sh tested)")
fi
cargo build -q -p reaper-bandcontrol -p reaper-harness
failed=0
for version in "${versions[@]}"; do
  dir=target/reaper-versions/$version
  bin=$(scripts/install-reaper.sh "$dir" "$version")
  log=$dir/e2e.log
  if REAPER_BIN=$PWD/$bin cargo test -q -p reaper-harness --test e2e -- --ignored --test-threads=1 >| "$log" 2>&1; then
    echo "REAPER $version: ok"
  else
    echo "REAPER $version: FAILED, see $log"
    failed=1
  fi
done
exit $failed
