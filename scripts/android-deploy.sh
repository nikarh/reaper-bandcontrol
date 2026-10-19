#!/usr/bin/env bash
# Builds and installs the app on the connected phone, then starts it.
#
#   scripts/android-deploy.sh                 # keep the current pairing
#   scripts/android-deploy.sh <pairing url>   # pair via deep link, e.g. bandcontrol://pair?s=...&n=...[&t=127.0.0.1:47011]
#   scripts/android-deploy.sh -s SERIAL ...   # this device (or ANDROID_SERIAL); asks when several are connected
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ ${1:-} == -s ]]; then
  export ANDROID_SERIAL=${2:?-s needs a device serial (see adb devices)}
  shift 2
fi
# shellcheck source=scripts/android-device.sh
source scripts/android-device.sh
pick_android_device

(cd android && ./gradlew -q --console=plain :app:assembleDebug)
adb install -r -g android/app/build/outputs/apk/debug/app-debug.apk > /dev/null
scripts/android-wake.sh
if [[ $# -gt 0 ]]; then
  # A pairing URL may point to TCP on this machine: forward the port.
  if [[ $1 =~ [\&?]t=127\.0\.0\.1:([0-9]+) ]]; then
    adb reverse "tcp:${BASH_REMATCH[1]}" "tcp:${BASH_REMATCH[1]}" > /dev/null
  fi
  adb shell am start -a android.intent.action.VIEW -d "'$1'" dev.nikarh.bandcontrol > /dev/null
else
  adb shell am start -n dev.nikarh.bandcontrol/.app.MainActivity > /dev/null
fi
