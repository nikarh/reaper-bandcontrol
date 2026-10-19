#!/usr/bin/env bash
# Starts a headless Android emulator (API 35, no Google services) for screenshots and tests without a phone.
# Prints the serial to use as ANDROID_SERIAL. Needs `mise run android:emulator-sdk` once.
#
#   ANDROID_SERIAL=$(scripts/android-emulator.sh) scripts/docs-screenshots.sh app
set -euo pipefail
cd "$(dirname "$0")/.."

AVD=bandcontrol
PORT=5580
export ANDROID_AVD_HOME="$PWD/.android-sdk/avd"
mkdir -p "$ANDROID_AVD_HOME"

# Written directly: avdmanager can't find an SDK outside its own installation.
if [[ ! -f "$ANDROID_AVD_HOME/$AVD.ini" ]]; then
  mkdir -p "$ANDROID_AVD_HOME/$AVD.avd"
  printf '%s\n' "avd.ini.encoding=UTF-8" "path=$ANDROID_AVD_HOME/$AVD.avd" "target=android-35" > "$ANDROID_AVD_HOME/$AVD.ini"
  printf '%s\n' \
    "image.sysdir.1=system-images/android-35/default/x86_64/" "abi.type=x86_64" "hw.cpu.arch=x86_64" \
    "tag.id=default" "PlayStore.enabled=false" "hw.lcd.width=1080" "hw.lcd.height=2400" "hw.lcd.density=420" \
    "hw.ramSize=3072" "disk.dataPartition.size=4G" "hw.keyboard=yes" "hw.gpu.enabled=yes" \
    > "$ANDROID_AVD_HOME/$AVD.avd/config.ini"
fi
serial="emulator-$PORT"
if ! adb devices | grep -q "^$serial"; then
  "$ANDROID_HOME/emulator/emulator" -avd "$AVD" -port "$PORT" -no-window -no-audio -no-boot-anim \
    -gpu swiftshader_indirect -no-snapshot-save > target/emulator.log 2>&1 &
fi
adb -s "$serial" wait-for-device
until [[ $(adb -s "$serial" shell getprop sys.boot_completed 2> /dev/null | tr -d '\r') == 1 ]]; do sleep 2; done
adb -s "$serial" shell settings put global window_animation_scale 0 > /dev/null
adb -s "$serial" shell settings put global transition_animation_scale 0 > /dev/null
adb -s "$serial" shell settings put global animator_duration_scale 0 > /dev/null
echo "$serial"
