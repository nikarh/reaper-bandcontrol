#!/usr/bin/env bash
# Instrumented end-to-end tests on the connected phone against a real, headless REAPER running the extension.
#
#   scripts/android-reaper-e2e.sh        # phone -> Bluetooth LE -> REAPER
#   scripts/android-reaper-e2e.sh tcp    # phone -> TCP (adb reverse) -> REAPER
#
# The tests observe and drive REAPER through its web interface, reached through `adb reverse`.
set -euo pipefail
cd "$(dirname "$0")/.."

WEB_PORT=47031
TCP_PORT=47033
SECRET=AAAAAAAAAAAAAAAAAAAAAAAAAA
APK=android/app/build/outputs/apk
LOG=target/android-reaper-e2e-harness.log
MODE=${1:-ble}
if [[ $MODE == tcp ]]; then
  export BANDCONTROL_TCP=127.0.0.1:$TCP_PORT BANDCONTROL_BLE=0
  TRANSPORT=(-e tcp "$BANDCONTROL_TCP")
else
  TRANSPORT=()
fi

cargo build -q -p reaper-harness -p reaper-bandcontrol
(cd android && ./gradlew -q --console=plain :app:assembleDebug :app:assembleDebugAndroidTest)

# The harness kills REAPER when its stdin closes.
exec 3> >(BANDCONTROL_SECRET=$SECRET BANDCONTROL_NAME="REAPER e2e" \
  ./target/debug/reaper-harness --web-port "$WEB_PORT" >| "$LOG" 2>&1)
HARNESS=$!
cleanup() {
  exec 3>&-
  wait "$HARNESS" 2> /dev/null || true
  adb reverse --remove "tcp:$WEB_PORT" 2> /dev/null || true
  adb reverse --remove "tcp:$TCP_PORT" 2> /dev/null || true
}
trap cleanup EXIT

for _ in $(seq 120); do
  grep -q '^ready' "$LOG" 2> /dev/null && break
  sleep 0.5
done
grep -q '^ready' "$LOG" || {
  cat "$LOG"
  exit 1
}

adb reverse "tcp:$WEB_PORT" "tcp:$WEB_PORT" > /dev/null
adb reverse "tcp:$TCP_PORT" "tcp:$TCP_PORT" > /dev/null
adb install -r -g "$APK/debug/app-debug.apk" > /dev/null
adb install -r -g "$APK/androidTest/debug/app-debug-androidTest.apk" > /dev/null
scripts/android-wake.sh

adb shell am instrument -w -e class dev.nikarh.bandcontrol.app.ReaperE2ETest \
  -e reaperWeb "127.0.0.1:$WEB_PORT" "${TRANSPORT[@]}" -e secret "$SECRET" \
  dev.nikarh.bandcontrol.test/androidx.test.runner.AndroidJUnitRunner | tee target/android-reaper-e2e.txt
grep -q '^OK (' target/android-reaper-e2e.txt
