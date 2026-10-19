#!/usr/bin/env bash
# Instrumented end-to-end tests on the connected phone against `bandctl sim`, reached over TCP through `adb reverse`.
set -euo pipefail
cd "$(dirname "$0")/.."

TCP_PORT=47021
CONTROL_PORT=47022
SECRET=AAAAAAAAAAAAAAAAAAAAAAAAAA
APK=android/app/build/outputs/apk

cargo build -q -p bandctl
(cd android && ./gradlew -q --console=plain :app:assembleDebug :app:assembleDebugAndroidTest)

./target/debug/bandctl sim --name Simulator --secret "$SECRET" \
  --tcp "127.0.0.1:$TCP_PORT" --control "127.0.0.1:$CONTROL_PORT" > target/android-e2e-sim.log 2>&1 &
SIM=$!
cleanup() {
  kill "$SIM" 2> /dev/null || true
  adb reverse --remove "tcp:$TCP_PORT" 2> /dev/null || true
  adb reverse --remove "tcp:$CONTROL_PORT" 2> /dev/null || true
}
trap cleanup EXIT

adb reverse "tcp:$TCP_PORT" "tcp:$TCP_PORT" > /dev/null
adb reverse "tcp:$CONTROL_PORT" "tcp:$CONTROL_PORT" > /dev/null
adb install -r -g "$APK/debug/app-debug.apk" > /dev/null
adb install -r -g "$APK/androidTest/debug/app-debug-androidTest.apk" > /dev/null
scripts/android-wake.sh

adb shell am instrument -w \
  -e simTcp "127.0.0.1:$TCP_PORT" -e simControl "127.0.0.1:$CONTROL_PORT" -e secret "$SECRET" \
  dev.nikarh.bandcontrol.test/androidx.test.runner.AndroidJUnitRunner | tee target/android-e2e.txt
grep -q '^OK (' target/android-e2e.txt
