#!/usr/bin/env bash
# Taps the app element with the given test tag on the connected phone, e.g. `scripts/android-tap.sh tab-mix`.
set -euo pipefail
tag=${1:?usage: android-tap.sh <test tag>}
# shellcheck source=scripts/android-device.sh
source "$(dirname "$0")/android-device.sh"
pick_android_device

adb shell uiautomator dump /sdcard/ui.xml > /dev/null
point=$(adb shell cat /sdcard/ui.xml | python3 -c '
import re, sys
tag = sys.argv[1]
for node in re.findall(r"<node [^>]*>", sys.stdin.read()):
    if f"resource-id=\"{tag}\"" in node:
        x1, y1, x2, y2 = map(int, re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
        print((x1 + x2) // 2, (y1 + y2) // 2)
        break
' "$tag")
if [[ -z $point ]]; then
  echo "no element with tag $tag" >&2
  exit 1
fi
# shellcheck disable=SC2086
adb shell input tap $point
