#!/usr/bin/env bash
# CI: installs the Ubuntu packages that the runner doesn't have yet, without the usual minute of overhead: the package
# lists are only refreshed when an install from the image's lists fails, and man-db doesn't rebuild its index.
#
#   scripts/ci-apt.sh PACKAGE...
set -euo pipefail

missing=()
for package in "$@"; do
  if [[ $(dpkg-query -W -f='${Status}' "$package" 2> /dev/null) != "install ok installed" ]]; then
    missing+=("$package")
  fi
done
[[ ${#missing[@]} -gt 0 ]] || exit 0
echo "Installing ${missing[*]}"
sudo rm -f /var/lib/man-db/auto-update
if ! sudo apt-get install -yq "${missing[@]}"; then
  sudo apt-get update -q
  sudo apt-get install -yq "${missing[@]}"
fi
