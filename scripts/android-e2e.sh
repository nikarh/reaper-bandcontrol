#!/usr/bin/env bash
# Instrumented end-to-end tests on the connected phone against `bandctl sim`.
#
#   scripts/android-e2e.sh        # phone -> TCP (adb reverse) -> simulator
#   scripts/android-e2e.sh ble    # phone -> Bluetooth LE -> simulator
#
# The simulator's control port is always reached through `adb reverse`.
set -euo pipefail
cd "$(dirname "$0")/.."

TCP_PORT=47021
CONTROL_PORT=47022
SECRET=AAAAAAAAAAAAAAAAAAAAAAAAAA
# A second computer for the several-computers test, always over TCP.
STAGE_TCP_PORT=47023
STAGE_CONTROL_PORT=47024
STAGE_SECRET=CCCCCCCCCCCCCCCCCCCCCCCCCA
APK=android/app/build/outputs/apk
MODE=${1:-tcp}
if [[ $MODE == ble ]]; then
  SERVE=(--ble)
else
  SERVE=(--tcp "127.0.0.1:$TCP_PORT")
fi

cargo build -q -p bandctl
(cd android && ./gradlew -q --console=plain :app:assembleDebug :app:assembleDebugAndroidTest)

./target/debug/bandctl sim --name Simulator --secret "$SECRET" --project band \
  "${SERVE[@]}" --control "127.0.0.1:$CONTROL_PORT" > target/android-e2e-sim.log 2>&1 &
SIM=$!
./target/debug/bandctl sim --name "Stage PC" --secret "$STAGE_SECRET" --project band \
  --tcp "127.0.0.1:$STAGE_TCP_PORT" --control "127.0.0.1:$STAGE_CONTROL_PORT" > target/android-e2e-stage.log 2>&1 &
STAGE=$!
PORTS=("$TCP_PORT" "$CONTROL_PORT" "$STAGE_TCP_PORT" "$STAGE_CONTROL_PORT")
cleanup() {
  kill "$SIM" "$STAGE" 2> /dev/null || true
  for port in "${PORTS[@]}"; do adb reverse --remove "tcp:$port" 2> /dev/null || true; done
}
trap cleanup EXIT

for port in "${PORTS[@]}"; do adb reverse "tcp:$port" "tcp:$port" > /dev/null; done
adb install -r -g "$APK/debug/app-debug.apk" > /dev/null
adb install -r -g "$APK/androidTest/debug/app-debug-androidTest.apk" > /dev/null
scripts/android-wake.sh

adb shell am instrument -w -e class dev.nikarh.bandcontrol.app.SimulatorE2ETest \
  -e ble "$([[ $MODE == ble ]] && echo true || echo false)" -e simTcp "127.0.0.1:$TCP_PORT" -e simControl "127.0.0.1:$CONTROL_PORT" -e secret "$SECRET" \
  -e stageTcp "127.0.0.1:$STAGE_TCP_PORT" -e stageControl "127.0.0.1:$STAGE_CONTROL_PORT" -e stageSecret "$STAGE_SECRET" \
  dev.nikarh.bandcontrol.test/androidx.test.runner.AndroidJUnitRunner | tee target/android-e2e.txt
grep -q '^OK (' target/android-e2e.txt
