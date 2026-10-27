#!/usr/bin/env bash
# End-to-end check of BandControl.app on macOS: assembles the app, starts it through Launch Services like the REAPER
# extension does, talks to it like the extension (bandctl app-check) and checks that it quits after the "extension"
# left. Without Bluetooth (CI) the app reports why Bluetooth doesn't work, which shows that it answers.
#
#   scripts/macos-app-e2e.sh [BANDCONTROL BANDCTL]   # the two binaries; built here when not given
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ $# -ge 2 ]]; then
  app_bin=$1
  bandctl=$2
else
  cargo build --locked -p bandcontrol-app -p bandctl
  app_bin=target/debug/BandControl
  bandctl=target/debug/bandctl
fi

work=$(mktemp -d)
exe="$work/BandControl.app/Contents/MacOS/BandControl"
trap 'pkill -f "$exe" || true; rm -rf "$work"' EXIT
scripts/macos-app.sh "$app_bin" "$work" > /dev/null
socket="$work/helper.sock"
open -n -g --env BANDCONTROL_HELPER_SOCKET="$socket" "$work/BandControl.app"
if ! "$bandctl" app-check --socket "$socket"; then
  tail -20 ~/Library/Logs/BandControl.log >&2 || true
  exit 1
fi
for _ in $(seq 30); do
  if ! pgrep -f "$exe" > /dev/null; then
    echo "BandControl.app answered and quit after the extension left"
    exit 0
  fi
  sleep 1
done
echo "BandControl.app is still running 30 s after the extension left" >&2
tail -20 ~/Library/Logs/BandControl.log >&2 || true
exit 1
