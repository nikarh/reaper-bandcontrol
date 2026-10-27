# Releasing

## How a release happens

1. Run the **Create a release PR** workflow (Actions → Create a release PR → patch / minor / major). It bumps the
   workspace version in `Cargo.toml` (the Android app reads it from there), regenerates `CHANGELOG.md` with
   git-cliff and opens a pull request titled `Release vX.Y.Z` with the release notes as its body.
2. Review and merge it. The **Release** workflow tags the merge commit and creates the GitHub release with the notes;
   the **Site** workflow redeploys the website with the new version and changelog.
3. The **Publish** workflow builds every artifact, builds the installers and signs them and the libraries inside
   (when the certificates below are set up), signs the APK with the release key, writes `SHA256SUMS` and its GPG
   signature, and uploads everything to the release.

Asset names never change, so `https://github.com/nikarh/reaper-bandcontrol/releases/latest/download/<name>` always
points at the newest file:

| Asset | Contents |
| --- | --- |
| `bandcontrol.apk` | Android app, signed with the release key |
| `reaper_bandcontrol-linux-x86_64.so`, `reaper_bandcontrol-linux-aarch64.so` | REAPER extension for Linux (built on Ubuntu 22.04) |
| `reaper_bandcontrol-macos-universal.dylib` | REAPER extension for macOS, Apple Silicon and Intel |
| `reaper_bandcontrol-windows-x86_64.dll`, `reaper_bandcontrol-windows-arm64ec.dll` | REAPER extension for Windows, x64 and ARM (REAPER's ARM64EC build) |
| `bandcontrol-windows-x64-setup.exe`, `bandcontrol-windows-arm64-setup.exe` | Windows installers (NSIS), per user |
| `bandcontrol-macos.pkg` | macOS installer, Apple Silicon and Intel: the extension into the user's REAPER folder, BandControl.app into `~/Applications` |
| `BandControl-macos.zip` | BandControl.app alone, which gives the extension Bluetooth on macOS, for installing by hand |
| `SHA256SUMS`, `SHA256SUMS.asc` | Checksums of all of the above and their detached GPG signature |

## One-time setup

### Release bot and the `release` environment

The release workflows act as a GitHub App, like the other nikarh repositories: create (or reuse) the app with
*Contents* and *Pull requests* read & write, install it on the repository, and create an environment named
`release` (Settings → Environments) limited to the `main` and `release` branches, with these secrets:

| Secret | Value |
| --- | --- |
| `APP_ID` | The app's ID |
| `APP_PRIVATE_KEY` | A private key of the app (PEM) |
| `ANDROID_KEYSTORE_BASE64` | The Android release keystore, base64 encoded |
| `ANDROID_KEYSTORE_PASSWORD` | Its store password |
| `ANDROID_KEY_ALIAS` | The key alias inside it |
| `ANDROID_KEY_PASSWORD` | The key password |
| `GPG_PRIVATE_KEY` | ASCII armored private key that signs `SHA256SUMS` |
| `GPG_PASSPHRASE` | Its passphrase |

Optional, for signed installers (without them the installers are published unsigned, and Windows SmartScreen and
macOS Gatekeeper warn before opening them):

| Secret | Value |
| --- | --- |
| `WINDOWS_SIGN_PFX_BASE64` | An Authenticode code signing certificate with its key (`.pfx`), base64 encoded |
| `WINDOWS_SIGN_PASSWORD` | Its password |
| `MACOS_CERTIFICATES_P12_BASE64` | *Developer ID Application* and *Developer ID Installer* certificates with their keys, exported together from Keychain Access as one `.p12`, base64 encoded |
| `MACOS_CERTIFICATES_PASSWORD` | Its password |
| `MACOS_SIGN_IDENTITY` | `Developer ID Application: Name (TEAMID)` |
| `MACOS_INSTALLER_IDENTITY` | `Developer ID Installer: Name (TEAMID)` |
| `MACOS_NOTARY_APPLE_ID`, `MACOS_NOTARY_TEAM_ID`, `MACOS_NOTARY_PASSWORD` | Apple ID, team ID and an [app-specific password](https://support.apple.com/102654) for notarization |
| `MACOS_SELF_SIGNED_P12_BASE64` | Without Developer ID: the project's self-signed code signing certificate with its key (`.p12`, base64 encoded), see below |
| `MACOS_SELF_SIGNED_PASSWORD` | Its password |

#### The self-signed certificate for BandControl.app

macOS ties an app's permissions to its signature. With an ad-hoc signature, which changes with every build, macOS
would ask for Bluetooth again after every update. So without a Developer ID, Publish signs BandControl.app with a
self-signed certificate ("BandControl (self-signed)", valid until 2056): Gatekeeper still warns before the first
install, but updates keep the permission. Keep a backup of the `.p12` and its password: a new certificate makes
every Mac ask once more. It was made like this (macOS's `security import` needs the old `.p12` encryption):

```sh
printf '%s\n' '[req]' 'distinguished_name = dn' 'x509_extensions = ext' 'prompt = no' \
  '[dn]' 'CN = BandControl (self-signed)' 'O = BandControl' \
  '[ext]' 'basicConstraints = critical,CA:false' 'keyUsage = critical,digitalSignature' \
  'extendedKeyUsage = critical,codeSigning' 'subjectKeyIdentifier = hash' > cert.cnf
openssl req -x509 -newkey rsa:3072 -nodes -days 10950 -keyout key.pem -out cert.pem -config cert.cnf
openssl rand -hex 24 | tr -d '\n' > password.txt
openssl pkcs12 -export -inkey key.pem -in cert.pem -name "BandControl (self-signed)" \
  -out bandcontrol-self-signed.p12 -passout file:password.txt -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1
gh secret set MACOS_SELF_SIGNED_P12_BASE64 --env release --body "$(base64 -w0 < bandcontrol-self-signed.p12)"
gh secret set MACOS_SELF_SIGNED_PASSWORD --env release --body "$(cat password.txt)"
```

### REAPER license for the e2e tests (optional)

Without a license REAPER runs as an evaluation copy, whose reminder window the test harness has to close at every
start. A repository secret (Settings → Secrets and variables → Actions, not the `release` environment) lets the
Check and Newest REAPER workflows run a licensed REAPER instead:

```sh
base64 -w0 < ~/.config/REAPER/reaper-license.rk | gh secret set REAPER_LICENSE_BASE64 --repo nikarh/reaper-bandcontrol
```

`scripts/ci-reaper-license.sh` writes it to a file only the job can read and deletes it, and the test directories
with copies of it, before the job ends; only `*.log` files are uploaded on failure. Pull requests from forks and from
Dependabot get no secrets and test unlicensed. The Docs workflow never uses it: a licensed REAPER can show its owner's
name, and its screenshots are published. Locally the harness uses the license of your own REAPER installation, if
there is one (`REAPER_LICENSE=` tests unlicensed).

### Installers and signing, locally

The same scripts run locally, through mise; every signing variable is optional and the result is unsigned without
it:

```sh
# Windows: needs makensis (NSIS 3.08+) and, for signing, osslsigncode. Without a DLL argument it builds the
# library, which needs Windows; elsewhere pass one built there or downloaded from CI.
WINDOWS_SIGN_PFX=cert.pfx WINDOWS_SIGN_PASSWORD=... mise run package:windows x64 reaper_bandcontrol.dll
# macOS: the certificates in the login keychain.
MACOS_SIGN_IDENTITY="Developer ID Application: ..." MACOS_INSTALLER_IDENTITY="Developer ID Installer: ..." \
  MACOS_NOTARY_APPLE_ID=... MACOS_NOTARY_TEAM_ID=... MACOS_NOTARY_PASSWORD=... mise run package:macos
```

Both write to `dist/`. `WINDOWS_SIGN_TIMESTAMP` overrides the timestamp server (empty: no timestamp).

### F-Droid

The app is ready for F-Droid (free dependencies, reproducible builds, listing metadata in `fastlane/`); what was
checked and how to submit is in [`FDROID.md`](FDROID.md). `mise run android:fdroid-check` verifies the build in
F-Droid's build server container.

### Actions policy

The repository allows only GitHub's own actions, actions by verified creators and the third party actions in
[`.github/allowed-actions.txt`](../.github/allowed-actions.txt), and requires every action to be pinned to a full
commit SHA. Every workflow refers to actions as `owner/action@<40 hex SHA> # vX.Y.Z`; Dependabot updates the SHA and
the version comment together. `mise run lint` (`scripts/check-actions.sh`) fails when a workflow uses an action that
isn't pinned or isn't allowed. Adding a third party action: add it to `allowed-actions.txt`, pin it, and add it to
the repository's allowed actions as well (Settings → Actions → General, or the `gh api` call in the setup notes).

### Android release key

Keep the keystore safe: every future update of the app has to be signed with the same key, otherwise Android
refuses to install it over the old version.

```sh
keytool -genkeypair -v -keystore bandcontrol-release.jks -alias bandcontrol -keyalg RSA -keysize 4096 \
  -validity 10000 -dname "CN=BandControl"
base64 -w0 bandcontrol-release.jks   # -> ANDROID_KEYSTORE_BASE64
```

Local release builds use the same variables (`ANDROID_KEYSTORE` is the path to the keystore file here) and fall back
to the debug key when they are not set.

### GPG signing key

```sh
gpg --quick-generate-key "BandControl releases <you@example.com>" ed25519 sign 3y
gpg --armor --export-secret-keys "BandControl releases"   # -> GPG_PRIVATE_KEY
gpg --armor --export "BandControl releases" > docs/signing-key.asc
```

Commit `docs/signing-key.asc`; the README and the website link to it for verifying downloads.

### Website

Settings → Pages → Build and deployment → Source: **GitHub Actions**. The **Site** workflow builds the page from
`site/`, `docs/features.toml`, `docs/screenshots` and `CHANGELOG.md` on every relevant push to `main` (a merged
release PR is one); there is no `gh-pages` branch. Only `main` may deploy to the `github-pages` environment. For a custom domain, enter it in the same settings page and add the DNS
record GitHub shows (a `CNAME` to `nikarh.github.io` for a subdomain); with Actions deployments no `CNAME` file is
needed.

### Screenshots

The **Docs** workflow regenerates `docs/screenshots` on GitHub (an Android emulator against the simulated example
project, and an unlicensed headless REAPER under Xvfb) and opens a `docs: refresh screenshots` pull request when they
changed. It runs when app, extension, fixture or feature descriptions change on `main`, or by hand.
