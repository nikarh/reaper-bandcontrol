#!/usr/bin/env bash
# Sourced by the scripts that talk to a phone: picks the device and exports ANDROID_SERIAL, which adb uses.
#
#   ANDROID_SERIAL=... set      that device
#   one device connected        that one
#   several, in a terminal      asks which one
#   several, no terminal        fails with the list

pick_android_device() {
  if [[ -n ${ANDROID_SERIAL:-} ]]; then
    echo "Using ${ANDROID_SERIAL}" >&2
    return
  fi
  local serials=() names=() serial state rest model
  while read -r serial state rest; do
    [[ $state == device ]] || continue
    model=$(sed -n 's/.*model:\([^ ]*\).*/\1/p' <<< "$rest")
    serials+=("$serial")
    names+=("${model//_/ } ($serial)")
  done < <(adb devices -l | tail -n +2)

  case ${#serials[@]} in
    0)
      echo "No device: connect a phone with USB debugging, or start one with scripts/android-emulator.sh." >&2
      return 1
      ;;
    1) export ANDROID_SERIAL=${serials[0]} ;;
    *)
      if [[ ! -t 2 ]] || ! { : < /dev/tty; } 2> /dev/null; then
        echo "Several devices are connected, choose one with ANDROID_SERIAL=<serial>:" >&2
        printf '  %s\n' "${names[@]}" >&2
        return 1
      fi
      echo "Several devices are connected:" >&2
      local i choice
      for i in "${!names[@]}"; do
        printf '  %d) %s\n' $((i + 1)) "${names[i]}" >&2
      done
      while true; do
        read -r -p "Device [1-${#serials[@]}]: " choice < /dev/tty
        if [[ $choice =~ ^[0-9]+$ ]] && ((choice >= 1 && choice <= ${#serials[@]})); then
          export ANDROID_SERIAL=${serials[choice - 1]}
          break
        fi
      done
      ;;
  esac
  echo "Using ${ANDROID_SERIAL}" >&2
}
