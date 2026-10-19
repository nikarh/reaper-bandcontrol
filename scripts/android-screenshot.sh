#!/usr/bin/env bash
# Saves a screenshot of the connected phone: scripts/android-screenshot.sh [file] (default target/screenshot.png).
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/android-device.sh
source scripts/android-device.sh
pick_android_device

out=${1:-target/screenshot.png}
mkdir -p "$(dirname "$out")"
adb exec-out screencap -p >| "$out"
echo "$out"
