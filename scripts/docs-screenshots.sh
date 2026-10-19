#!/usr/bin/env bash
# Regenerates docs/screenshots from the example project (tests/fixtures/Band Project.RPP).
#
#   scripts/docs-screenshots.sh reaper   # headless REAPER (cage locally, Xvfb on CI)
#   scripts/docs-screenshots.sh app      # the app on the connected phone or emulator, against the simulator
#   scripts/docs-screenshots.sh          # both
#
# App screenshots come from ScreenshotTest; their names are referenced by docs/features.toml.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=docs/screenshots
WIDTH=540
mkdir -p "$OUT"
IM=$(command -v magick || command -v convert)

reaper() {
  cargo build -q -p reaper-harness -p reaper-bandcontrol
  local tmp
  tmp=$(mktemp -d)
  # Whole project in view, small tracks, no mixer.
  # Then the pairing window, alone where windows can be told apart (Xvfb).
  printf '%s\n' "40295;40110;40078" "sleep 2000" "screenshot $tmp/reaper-project.png" \
    _BANDCONTROL_SHOW_PAIRING "sleep 1500" "screenshot-window $tmp/reaper-pairing.png ^BandControl pairing$" q |
    BANDCONTROL_BLE=0 BANDCONTROL_TCP=127.0.0.1:0 BANDCONTROL_NAME="Rehearsal laptop" BANDCONTROL_SECRET=MZXW6YTBOI2DGNBVGY3TQOJQGE \
      ./target/debug/reaper-harness --dir target/reaper-harness-docs > target/docs-reaper.log 2>&1
  for f in "$tmp"/*.png; do
    "$IM" "$f" -strip "$OUT/$(basename "$f")"
  done
  rm -rf "$tmp"
}

app() {
  # shellcheck source=scripts/android-device.sh
  source scripts/android-device.sh
  pick_android_device
  local tcp=47021 control=47022 secret=AAAAAAAAAAAAAAAAAAAAAAAAAA apk=android/app/build/outputs/apk
  local files=/sdcard/Android/data/dev.nikarh.bandcontrol/files/screenshots
  cargo build -q -p bandctl
  (cd android && ./gradlew -q --console=plain :app:assembleDebug :app:assembleDebugAndroidTest)
  ./target/debug/bandctl sim --name "Band PC" --secret "$secret" --project band \
    --tcp "127.0.0.1:$tcp" --control "127.0.0.1:$control" > target/docs-sim.log 2>&1 &
  local sim=$!
  trap 'kill $sim 2>/dev/null || true; adb reverse --remove-all >/dev/null 2>&1 || true' RETURN
  adb reverse "tcp:$tcp" "tcp:$tcp" > /dev/null
  adb reverse "tcp:$control" "tcp:$control" > /dev/null
  adb install -r -g "$apk/debug/app-debug.apk" > /dev/null
  adb install -r -g "$apk/androidTest/debug/app-debug-androidTest.apk" > /dev/null
  adb shell rm -rf "$files"
  scripts/android-wake.sh || true
  adb shell am instrument -w -e class dev.nikarh.bandcontrol.app.ScreenshotTest \
    -e simTcp "127.0.0.1:$tcp" -e simControl "127.0.0.1:$control" -e secret "$secret" \
    dev.nikarh.bandcontrol.test/androidx.test.runner.AndroidJUnitRunner | tee target/docs-app.txt
  grep -q '^OK (' target/docs-app.txt
  local tmp
  tmp=$(mktemp -d)
  adb pull "$files/." "$tmp" > /dev/null
  for f in "$tmp"/*.png; do
    "$IM" "$f" -strip -resize "${WIDTH}x" "$OUT/$(basename "$f")"
  done
  rm -rf "$tmp"
}

case "${1:-all}" in
  reaper) reaper ;;
  app) app ;;
  all) reaper && app ;;
  *) echo "usage: $0 [reaper|app|all]" >&2 && exit 1 ;;
esac
