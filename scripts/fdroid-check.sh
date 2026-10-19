#!/usr/bin/env bash
# Checks that F-Droid can build the app and publish our signed releases (reproducible builds), the way it would:
#
# 1. builds the release APK of HEAD (unsigned) with this machine's toolchain, like CI;
# 2. builds HEAD in F-Droid's build server container from the draft recipe (packaging/fdroid), with its scanner;
# 3. compares the two APKs byte for byte;
# 4. signs ours with apksigner from build-tools 34 and a throwaway key (as the Publish workflow signs releases) and
#    lets fdroidserver verify that signature on its own build, which is what `fdroid publish` does with `Binaries`.
#
#   scripts/fdroid-check.sh        # needs podman (or CONTAINER=docker), the network and ~2 GB for the image
#
# HEAD is built, so commit first. Everything goes to target/fdroid-check.
set -euo pipefail
cd "$(dirname "$0")/.."

container=${CONTAINER:-podman}
image=registry.gitlab.com/fdroid/fdroidserver:buildserver
app=dev.nikarh.bandcontrol
commit=$(git rev-parse HEAD)
work=$PWD/target/fdroid-check
sdk=${ANDROID_HOME:?ANDROID_HOME is not set (mise sets it)}
rm -rf "$work/src" "$work/fdroiddata"
mkdir -p "$work"

echo "== our build of $commit"
git clone -q . "$work/src"
git -C "$work/src" checkout -q "$commit"
(cd "$work/src/android" && ./gradlew -q --console=plain --no-build-cache --no-configuration-cache \
  :app:assembleRelease -Punsigned)
ours=$work/src/android/app/build/outputs/apk/release/app-release-unsigned.apk

echo "== F-Droid's build"
if [[ ! -d $work/fdroidserver ]]; then
  git clone -q --depth=1 https://gitlab.com/fdroid/fdroidserver "$work/fdroidserver"
fi
mkdir -p "$work/fdroiddata/metadata" "$work/fdroiddata/config"
# shellcheck disable=SC2016 # fdroidserver expands $ANDROID_HOME itself
printf 'sdk_path: $ANDROID_HOME\n' > "$work/fdroiddata/config.yml"
chmod 600 "$work/fdroiddata/config.yml"
# The recipe's categories, so lint can check them without fdroiddata's category icons.
sed -n '/^Categories:/,/^[A-Z]/s/^  - \(.*\)/\1:\n  name: \1/p' packaging/fdroid/$app.yml > "$work/fdroiddata/config/categories.yml"
# The recipe, building this commit from the local clone instead of the release tag on GitHub, without the download
# of the published APK (there is none for this commit).
sed -e "s|^Repo: .*|Repo: /src|" -e "s|^    commit: .*|    commit: $commit|" -e '/^Binaries:/d' \
  packaging/fdroid/$app.yml > "$work/fdroiddata/metadata/$app.yml"
cat > "$work/fdroiddata/run.sh" << 'EOF'
#!/bin/bash
set -uo pipefail
. /etc/profile
export PATH="/home/vagrant/fdroidserver:$PATH" PYTHONPATH="/home/vagrant/fdroidserver"
git config --global --add safe.directory "*"
cd /build
fdroid readmeta || exit 1
echo "-- fdroid lint (a missing AllowedAPKSigningKeys is expected until the release key exists):"
fdroid lint dev.nikarh.bandcontrol
fdroid build -l dev.nikarh.bandcontrol || exit 1
EOF
chmod -R a+rwX "$work/fdroiddata" "$work/fdroidserver"
"$container" run --rm -u vagrant --entrypoint /bin/bash -v "$work/fdroiddata:/build:z" \
  -v "$work/fdroidserver:/home/vagrant/fdroidserver:Z" -v "$work/src:/src:ro,z" "$image" /build/run.sh
theirs=$(find "$work/fdroiddata/build/$app/android/app/build/outputs/apk/release" -name '*.apk' | head -1)

echo "== comparison"
sha256sum "$ours" "$theirs"
if ! cmp -s "$ours" "$theirs"; then
  echo "NOT reproducible: the APKs differ (try diffoscope on them)" >&2
  exit 1
fi
echo "identical"

echo "== signature copied onto F-Droid's build"
apksigner=$sdk/build-tools/34.0.0/apksigner
if [[ ! -x $apksigner ]]; then
  yes | sdkmanager --sdk_root="$sdk" "build-tools;34.0.0" > /dev/null
fi
rm -f "$work/test.jks"
keytool -genkeypair -keystore "$work/test.jks" -alias test -keyalg RSA -keysize 2048 -validity 1 -dname CN=test \
  -storepass fdroidcheck -keypass fdroidcheck > /dev/null 2>&1
cp "$ours" "$work/fdroiddata/signed.apk"
cp "$theirs" "$work/fdroiddata/unsigned.apk"
"$apksigner" sign --ks "$work/test.jks" --ks-key-alias test --ks-pass pass:fdroidcheck --key-pass pass:fdroidcheck \
  "$work/fdroiddata/signed.apk"
"$container" run --rm --entrypoint /bin/bash -v "$work/fdroiddata:/build:z" \
  -v "$work/fdroidserver:/home/vagrant/fdroidserver:Z" "$image" -c '. /etc/profile; cd /build && mkdir -p /tmp/v &&
  PYTHONPATH=/home/vagrant/fdroidserver python3 -c "
import sys
from fdroidserver import common
common.config = common.read_config()
problem = common.verify_apks(\"signed.apk\", \"unsigned.apk\", \"/tmp/v\")
print(\"verified: the signature is valid on F-Droid build\" if problem is None else \"FAILED: \" + problem)
sys.exit(problem is not None)
"'
