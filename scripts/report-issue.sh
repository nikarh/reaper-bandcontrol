#!/usr/bin/env bash
# CI: opens an issue with TITLE and BODY, or comments on the open one with the same title (no duplicates week after
# week). Needs GH_TOKEN with issues: write.
#
#   scripts/report-issue.sh TITLE BODY
set -euo pipefail

title=${1:?usage: report-issue.sh TITLE BODY}
body=${2:?usage: report-issue.sh TITLE BODY}
existing=$(gh issue list --state open --search "\"$title\" in:title" --json number,title \
  --jq "map(select(.title == \"$title\"))[0].number // empty")
if [[ -n $existing ]]; then
  gh issue comment "$existing" --body "$body"
else
  gh issue create --title "$title" --body "$body"
fi
