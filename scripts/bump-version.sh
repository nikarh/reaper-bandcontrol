#!/usr/bin/env bash
# Bumps the version everything uses (the workspace version in Cargo.toml; the Android app reads it from there) and
# the Android versionCode next to it, points the draft F-Droid recipe at the new release, and prints the new version.
#
#   scripts/bump-version.sh patch|minor|major|X.Y.Z
set -euo pipefail
cd "$(dirname "$0")/.."

current=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
IFS=. read -r major minor patch <<< "$current"
case "${1:?usage: bump-version.sh patch|minor|major|X.Y.Z}" in
  patch) next="$major.$minor.$((patch + 1))" ;;
  minor) next="$major.$((minor + 1)).0" ;;
  major) next="$((major + 1)).0.0" ;;
  *.*.*) next=$1 ;;
  *) echo "unknown bump $1" >&2 && exit 1 ;;
esac

sed -i "/^\[workspace.package\]/,/^\[/s/^version\( *\)= *\"$current\"/version\1= \"$next\"/" Cargo.toml
IFS=. read -r major minor patch <<< "$next"
code=$((major * 10000 + minor * 100 + patch))
sed -i "s/^version-code\( *\)= *[0-9]*/version-code\1= $code/" Cargo.toml
sed -i -e "s/^  - versionName: .*/  - versionName: $next/" -e "s/^    versionCode: .*/    versionCode: $code/" \
  -e "s/^    commit: .*/    commit: v$next/" -e "s/^CurrentVersion: .*/CurrentVersion: $next/" \
  -e "s/^CurrentVersionCode: .*/CurrentVersionCode: $code/" packaging/fdroid/dev.nikarh.bandcontrol.yml
cargo update -q --workspace --offline 2> /dev/null || cargo update -q --workspace
echo "$next"
