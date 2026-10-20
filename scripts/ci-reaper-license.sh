#!/usr/bin/env bash
# CI: the REAPER license for the e2e tests, from the base64 encoded secret in $LICENSE. Without it REAPER runs
# unlicensed (evaluation), as in pull requests from forks and from Dependabot, which get no secrets.
#
#   scripts/ci-reaper-license.sh install   # LICENSE=... ; sets REAPER_LICENSE for the following steps
#   scripts/ci-reaper-license.sh remove    # always, before the job ends and the Rust cache is saved
#
# The license is never printed: it only goes into a file that only the runner user can read, and `remove` deletes it
# together with the test directories, which hold copies of it.
set -euo pipefail
file=$RUNNER_TEMP/reaper-license.rk
case ${1:?usage: ci-reaper-license.sh install|remove} in
  install)
    if [[ -n ${LICENSE:-} ]]; then
      (umask 077 && printf '%s' "$LICENSE" | base64 -d > "$file")
      echo "REAPER_LICENSE=$file" >> "$GITHUB_ENV"
      echo "REAPER: licensed" >> "$GITHUB_STEP_SUMMARY"
    else
      echo "REAPER_LICENSE=" >> "$GITHUB_ENV"
      echo "REAPER: unlicensed (evaluation)" >> "$GITHUB_STEP_SUMMARY"
    fi
    ;;
  remove)
    rm -rf "$file" target/reaper-e2e
    ;;
  *)
    echo "unknown command $1" >&2
    exit 1
    ;;
esac
