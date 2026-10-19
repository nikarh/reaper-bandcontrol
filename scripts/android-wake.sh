#!/usr/bin/env bash
# Keeps the connected phone awake and unlocked (no secure lock screen assumed) while plugged in.
set -euo pipefail

adb shell svc power stayon usb
adb shell input keyevent KEYCODE_WAKEUP
adb shell wm dismiss-keyguard
