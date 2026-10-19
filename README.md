# BandControl for REAPER

[![Check](https://github.com/nikarh/reaper-bandcontrol/actions/workflows/check.yml/badge.svg)](https://github.com/nikarh/reaper-bandcontrol/actions/workflows/check.yml)
[![Release](https://img.shields.io/github/v/release/nikarh/reaper-bandcontrol)](https://github.com/nikarh/reaper-bandcontrol/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/nikarh/reaper-bandcontrol/total)](https://github.com/nikarh/reaper-bandcontrol/releases)
[![Stars](https://img.shields.io/github/stars/nikarh/reaper-bandcontrol?style=flat)](https://github.com/nikarh/reaper-bandcontrol/stargazers)
[![License](https://img.shields.io/github/license/nikarh/reaper-bandcontrol)](LICENSE)
[![REAPER 5.93+](https://img.shields.io/badge/REAPER-5.93%2B-1a6fb5)](#requirements)
[![Platforms](https://img.shields.io/badge/extension-Windows%20%7C%20macOS%20%7C%20Linux-555)](#reaper-extension)
[![Android 9+](https://img.shields.io/badge/Android-9%2B-3ddc84?logo=android&logoColor=white)](#android-app)
[![Website](https://img.shields.io/badge/website-nikarh.github.io-informational)](https://nikarh.github.io/reaper-bandcontrol/)

Control REAPER from the band's phones over Bluetooth: playback and setlist, input and front of house levels, and a personal
in-ear mix for every band member.

BandControl is two parts: a **REAPER extension** (Windows, macOS, Linux) that turns the computer into a Bluetooth
Low Energy device, and an **Android app** that every band member installs.

> **Status:** the Android app is the only client for now. An **iOS app is planned** but doesn't exist yet. The app
> is not on **Google Play** or **F-Droid** yet (both planned); install the APK from the
> [releases](https://github.com/nikarh/reaper-bandcontrol/releases/latest). The Linux Bluetooth backend is tested
> daily; the macOS and Windows backends are implemented but have seen little real hardware so far.

**Works with REAPER 5.93 and newer**, on Windows, macOS and Linux; see [requirements](#requirements).

## Why this exists

BandControl supersedes [reaper-remote-bandui](https://github.com/nikarh/reaper-remote-bandui), a web page for
REAPER's built-in web interface that bands use for the same job. The web page works well on a good Wi-Fi network.
Most rehearsal rooms and venues don't have one, so in practice one phone shares its hotspot, the computer joins it,
and every musician types the computer's IP address and port into a browser, and again after the hotspot drops.

BandControl removes the network entirely:

- **No network setup.** The computer is a Bluetooth device; phones connect to it directly, wherever you are.
- **Pair once.** Scan a QR code from REAPER, or from a bandmate's phone. After that the app finds the computer by
  itself, every time, and reconnects when the connection drops.
- **Secure by default.** Only paired phones can find the computer, and every session is encrypted. A stranger in
  the audience can't change your monitor mix.
- **Built for the stage.** Faders that don't jump when touched or scrolled over, meters with clip indicators for
  inputs and front of house (FOH) outputs, a setlist editor, plain-language connection status, and the screen stays on while
  connected.
- **Compatible.** The same project conventions as reaper-remote-bandui; the song order it stores is shared.

## Features

<!-- features:start -->
<!-- Generated from docs/features.toml by scripts/build-docs.py, edit there. -->

### Playback and setlist

<img src="docs/screenshots/playback.png" alt="Playback and setlist" width="220" align="right">

Play, pause, stop, record and repeat. The songs (regions) are listed in the band's order with their progress; tap one to jump to it. Previous / next step through the parts of a song (markers) and continue with the next song in the setlist.

<br clear="right">

### Rearrange the setlist by dragging

<img src="docs/screenshots/setlist.png" alt="Rearrange the setlist by dragging" width="220" align="right">

Tap Edit and drag songs by their handle, or long-press anywhere on a row, into tonight's order; switch a song off to skip it. The order is stored in the REAPER project, compatible with reaper-remote-bandui, and every phone sees it right away.

<br clear="right">

### Input levels and gain

<img src="docs/screenshots/inputs.png" alt="Input levels and gain" width="220" align="right">

A live meter for every input with its peak level, and a clip indicator that stays lit until you tap it, so you see which input clipped during the last song. The fader sets the input's gain (its track volume) on a dB scale up to +12 dB.

<br clear="right">

### FOH outputs

<img src="docs/screenshots/foh.png" alt="FOH outputs" width="220" align="right">

The same meters and faders for the front of house: the outputs that go to the house desk or straight to the PA. Whoever looks after the sound for the audience can check and adjust them from the stage, next to the in-ear mixes.

<br clear="right">

### Personal in-ear mixes

<img src="docs/screenshots/iem.png" alt="Personal in-ear mixes" width="220" align="right">

Every band member picks their own mix and sets the level of each source: click, backtracks, other instruments, vocals. Faders only move when dragged sideways, relative to where they are, so a tap or a scroll never changes a level by accident. Mutes and the overall mix level are one tap away, and changes made in REAPER show up instantly.

<br clear="right">

### Pair once, share by QR code

<img src="docs/screenshots/share.png" alt="Pair once, share by QR code" width="220" align="right">

REAPER shows a QR code; scan it with the app (no Google services needed) or type the code. A paired phone shows the same QR code to the next one, so the whole band is set up in a minute without touching the computer again. Pair the rehearsal laptop and the stage PC alike: the app connects to whichever is in range and asks when both are.

<br clear="right">

### Connectivity

Phones connect to the computer directly over Bluetooth Low Energy, so it works wherever you play. Only paired phones can find the computer, and every session is encrypted. The app reconnects by itself and keeps the screen on while connected.

### A project layout that explains itself

BandControl relies on a simple naming convention instead of guessing: songs are regions, the tracks in a folder named "Inputs" fill the Inputs tab, those in a "FOH" (or "PA") folder the FOH tab, and the in-ear mixes (hardware outputs with receives) live in an "IEM" folder. No plugins and no special routing; start from the example project or rename three folders in yours. reaper-remote-bandui projects keep working, with the IEM tab only.

<img src="docs/screenshots/reaper-project.png" alt="A project layout that explains itself">

<!-- features:end -->

## Install

Download the files from the [latest release](https://github.com/nikarh/reaper-bandcontrol/releases/latest), or use
the [website](https://nikarh.github.io/reaper-bandcontrol/).

### Requirements

| | |
| --- | --- |
| REAPER | **5.93 or newer.** Every release from 5.93 (the oldest REAPER for Linux still available) to 7.82 passes the end-to-end tests ([`mise run e2e:reaper-versions`](#development)); CI tests the oldest and the newest on every change, and every week the newest REAPER release. On an older REAPER the extension says so in the console when it loads. Windows and macOS builds use the same REAPER API, they aren't run against old releases. |
| Windows | Windows 10 1803 or newer, x64 or ARM, with a Bluetooth adapter that supports the LE peripheral role (most built-in ones do). |
| macOS | macOS 10.12 or newer (11 on Apple silicon). REAPER must be allowed to use Bluetooth, see [quirks](#quirks-and-limitations). |
| Linux | x86_64 or aarch64 (e.g. a Raspberry Pi 5) with BlueZ (`bluetoothd`) and glibc 2.35 or newer: Ubuntu 22.04, Debian 12, Fedora 36 and newer. |
| Phones | Android 9 or newer with Bluetooth LE. |

### REAPER extension

Quit REAPER, run the installer, start REAPER again: the actions are under *Actions → Show action list*, starting
with "BandControl".

| Platform | Installer | |
| --- | --- | --- |
| Windows, x64 (most PCs) | `bandcontrol-windows-x64-setup.exe` | For your user, into `%APPDATA%\REAPER`; for a portable REAPER choose its folder. Uninstall from *Settings → Apps*. |
| Windows on ARM | `bandcontrol-windows-arm64-setup.exe` | For REAPER's ARM64EC build. An x64 REAPER running under emulation needs the x64 installer. |
| macOS, Apple silicon and Intel | `bandcontrol-macos.pkg` | Into `~/Library/Application Support/REAPER`, no administrator password. |
| Linux | – | Copy the library, see below. |

Until a release is signed with the project's certificates, Windows SmartScreen asks first (*More info → Run anyway*),
and macOS opens the package only from the context menu (*right click → Open*).

#### Manual install: the plain library

For a portable REAPER, Linux, or to see exactly what goes where: copy the library for your platform into REAPER's
`UserPlugins` folder (in REAPER: *Options → Show REAPER resource path*, then `UserPlugins`) and restart REAPER. The
file name doesn't matter as long as it starts with `reaper_`; an installer puts the same library in the same place.

| Platform | File |
| --- | --- |
| Windows x64 | `reaper_bandcontrol-windows-x86_64.dll` |
| Windows on ARM (ARM64EC REAPER) | `reaper_bandcontrol-windows-arm64ec.dll` |
| macOS | `reaper_bandcontrol-macos-universal.dylib`; downloaded with a browser it is quarantined and REAPER can't load it: run `xattr -d com.apple.quarantine reaper_bandcontrol-macos-universal.dylib` once |
| Linux x86_64 | `reaper_bandcontrol-linux-x86_64.so` (into `~/.config/REAPER/UserPlugins`) |
| Linux aarch64 | `reaper_bandcontrol-linux-aarch64.so` |

From source: `mise run install` builds the extension and copies it into REAPER's `UserPlugins` (Linux and macOS).

### Android app

Install `bandcontrol.apk` on every phone (Android 9 or newer with Bluetooth LE). Android asks to allow installing
apps from your browser or file manager the first time. The app needs the *Nearby devices* permission (on Android 11
and older: location, which those versions require for Bluetooth scanning) and the camera only to scan QR codes.
Google Play services are not needed; it runs on de-Googled phones like LineageOS.

<img src="docs/screenshots/get-app.png" alt="Get BandControl: a QR code for the latest app" width="220" align="right">

Once one phone has the app, the others don't need this page: menu (⋮) → **About & share the app** shows a QR code
that downloads the latest APK from GitHub when scanned with any phone camera, a **Share link** button for messengers
and a link to the project on GitHub.

<br clear="right">

### Pairing

<img src="docs/screenshots/reaper-pairing.png" alt="The pairing window in REAPER" width="300" align="right">

1. In REAPER, run the action **BandControl: Show pairing QR code** (*Actions → Show action list*, or give it a
   toolbar button). A window opens with the QR code, the code to type, the name phones will see and whether
   Bluetooth is up. Make the window bigger and the QR code grows with it, which helps phones on the far side of
   the room. Its size and text follow REAPER's scaling preferences (*Preferences → General → Advanced UI/system
   tweaks*).
2. In the app, tap **Scan QR code**, or type the code.
3. Pair the other phones from the first one: menu (⋮) → **Share pairing with another phone** shows the same QR code.

All phones share one pairing code. **Reset pairing key…** in the window (or the action **BandControl: Reset pairing
key**) creates a new one and unpairs every phone; **Rename…** sets the name phones show. **BandControl: Show status**
shows the details, including why Bluetooth doesn't work if it doesn't.

<br clear="right">

### Where pairings are saved, and which computer is which

- **On the computer** the pairing code belongs to the REAPER installation: it is saved in REAPER's settings
  (`reaper-extstate.ini` in the resource folder), so it is the same for every project and survives restarts. A
  second computer, or a second REAPER installation with its own resource folder, has its own code.
- **The name** phones show comes from **BandControl: Set the name shown on phones**, saved in the same place; until
  it is set it's the computer's hostname (`BANDCONTROL_NAME` in the environment overrides both). Give every computer
  a name the band recognizes, like "Rehearsal laptop" or "Stage PC". The app shows it at the top while connected and
  picks up renames by itself.
- **On the phone** the app remembers every computer it was paired with (menu → **Pair with another computer**
  adds one). Each installation has its own code, so the phone only ever finds computers it was paired with.
  - Only one of them in range: the app connects to it by itself.
  - Several in range when the app opens: it asks which one.
  - Another paired computer comes in range while one is in use: the app says so (**Stage PC is nearby**, with a
    **Switch** button) and stays connected to the current one.
  - The menu switches to any paired computer and **Forget** removes the current one.

## Organizing the REAPER project

**BandControl finds its way around the project by a naming convention**, not by guessing from the routing. Three
folders, named the way it expects, decide what shows up in which tab:

```text
INPUTS/               -> Inputs tab: one track per input (Drums, Bass, Guitar, Vocals)
FOH OUT/              -> FOH tab: front of house, the feeds to the house desk or the PA, each a hardware output
                         receiving its sources
IEM OUT/              -> IEM tab: one mix per musician, a hardware output receiving everything they want to hear
```

Everything else is up to you: the click, backtracks, buses, effect returns and any other tracks can live anywhere
and be organized however you like, as long as they don't land in these folders: a direct child of the Inputs or FOH
folder shows up in that tab, and a track with a hardware output and receives inside the IEM folder is listed as a
mix. Without these folders the Inputs and FOH tabs are hidden and the IEM tab lists every track with a
hardware output and receives. There is nothing to install in the project; the [example project](#example-project)
shows one way to lay it out.

The rules:

1. **Songs are regions.** They must not overlap but may follow each other directly. Markers inside a region are the
   song's parts, used by previous / next. Most bands need none: regions alone are enough.
2. **An IEM mix is a track with a hardware output and at least one receive**, one per band member, receiving every
   source they want to hear (click, backtracks, instruments, vocals). The IEM tab sets the volume and mute of those
   receives and the mix's own volume. Put the mixes in a folder with **IEM** or **Monitor** in its name: then other
   output tracks (spare outputs) are not listed. FOH outputs never are.
3. **Inputs** are the direct children of a top level folder with the word **Input** or **Inputs** in its name (any
   case: "INPUTS", "Band inputs"). Nested tracks (effect returns and the like) are not inputs.
4. **FOH outputs** (front of house: what the audience hears, through the house desk or straight to the PA) are the
   direct children of a top level folder with the word **FOH** or **PA** in its name ("FOH OUT", "To FOH",
   "PA OUT").
5. The **song order** and disabled songs are stored in the project (project state `BANDUI/regions`), shared with
   reaper-remote-bandui.

Inputs and FOH tabs are hidden when the project has no such folder; a reaper-remote-bandui `Mother Project.RPP`
works as it is.

### Example project

[**Download `BandControl-Example.RPP`**](https://raw.githubusercontent.com/nikarh/reaper-bandcontrol/main/examples/BandControl-Example.RPP)
(*right click → Save link as*): the smallest project laid out this way, to open and look at, or to start from. It
opens in any REAPER from 5.93 on and needs no plugins. The screenshots here are taken from it.

- **Click** (a click plays during every song) and **Backtrack**, at the top level in this example.
- **INPUTS**: Drums, Bass, Guitar and Vocals on hardware inputs 1 to 4.
- **FOH OUT**: FOH Backtrack, FOH Instruments and FOH Vocals on hardware outputs 9 to 11, each receiving its sources.
- **IEM OUT**: Drummer, Bassist, Guitarist and Singer on output pairs 1/2 to 7/8, each receiving all six sources.
- Four songs, Spring, Summer, Autumn and Winter, at REAPER's default 120 BPM, one empty bar apart.

![REAPER: the example project with its folders](docs/screenshots/reaper-project.png)

## How it works

```text
 REAPER ── extension (Rust) ─────────────────────────┐            Android app (Kotlin, Compose)
   timer callback (~30 Hz)                            │
   ├─ reads tracks, receives, regions, transport      │  Bluetooth LE GATT   ┌──────────────────────┐
   ├─ engine: what changed, who needs it              ├─────────────────────►│ session, state, UI   │
   └─ server thread: sessions, crypto, BLE/TCP ───────┘◄─────────────────────┤ commands from faders │
                                                                             └──────────────────────┘
```

- **Transport.** The extension registers a GATT service whose UUID is derived from the pairing secret, with one
  characteristic phones write to and one they subscribe to. Phones scan for that UUID, so only paired phones even
  see the computer. Messages are [protobuf](proto/bandcontrol.proto) in a small framing layer that splits them into
  packets of the negotiated Bluetooth MTU. Linux uses BlueZ, macOS CoreBluetooth, Windows WinRT.
- **Sync policy.** A phone that connects gets the full state (tracks, volumes, receives, regions, markers, song
  order) and the transport. After that: the play position about once a second (the app extrapolates in between),
  volume and mute changes as small deltas the moment they happen, and the full state again, debounced by a second,
  when the project structure changes (tracks, receives, regions, markers). Meters are opt-in: the app subscribes to
  the tracks on screen and gets their peaks ten times a second, one byte per track; silence is sent once.
- **Commands.** Faders send at most every 50 ms while dragged; the extension applies commands on REAPER's main
  thread and the result comes back as a delta to every phone, so all phones stay in sync. The last change wins,
  whether it came from a phone or the engineer at the computer.
- **Security.** The 128-bit pairing secret derives the service UUID and an authentication key (HKDF-SHA256). A
  session starts with a mutual challenge-response (HMAC-SHA256): the phone proves it knows the secret, and the
  computer proves it too, so a phone can't be lured to an impostor. All further messages are encrypted and
  authenticated with ChaCha20-Poly1305; broadcast messages use a group key handed out to authenticated phones.
- **Threads.** REAPER's API is only touched from REAPER's main thread (the timer callback); Bluetooth and crypto run
  on a separate thread and exchange messages with it, so a slow phone never stalls the audio engine's UI thread.

## Quirks and limitations

- **Range.** Bluetooth LE reaches roughly 10 m through a stage, more with line of sight. Keep the computer's
  adapter unobstructed; a USB adapter on a short extension cable helps with laptops in flight cases.
- **Number of phones.** Adapters handle a limited number of simultaneous connections, usually 5 to 10. A typical
  band fits; a big ensemble may hit the adapter's limit.
- **macOS.** macOS only lets an application use Bluetooth if its `Info.plist` declares it
  (`NSBluetoothAlwaysUsageDescription`). The extension checks this instead of letting macOS terminate REAPER; if
  REAPER's bundle lacks the key, **Show status** says so and Bluetooth stays off.
- **Windows.** Needs Windows 10 1803 or newer and an adapter that supports the peripheral role. Windows can't drop a
  connection from the computer's side, so a rejected phone stays connected at the Bluetooth level until it leaves.
- **Gain and meters.** "Gain" on the Inputs and FOH tabs is the track volume (the fader in REAPER), and meters show
  the level after it. Clip indicators light at 0 dBFS and stay lit until tapped.
- **Faders.** Faders move only with horizontal drags and relative to their position: a tap or a vertical scroll
  never changes a level. The range is -∞ to +12 dB with a detent at 0 dB.
- **Changing the pairing code.** After **Reset pairing key** every phone has to pair again; over Bluetooth a phone
  with an old code simply doesn't find the computer anymore, and the app says so.
- **Sideloading.** Until the app is on Google Play and F-Droid, Android shows the usual warnings for apps installed
  from an APK.

## Verifying downloads

Every release contains `SHA256SUMS` and its signature `SHA256SUMS.asc`, made with the project's key
([`docs/signing-key.asc`](docs/signing-key.asc)); the APK is additionally signed with the app's release key.

```sh
gpg --import signing-key.asc
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS
```

## Development

[mise](https://mise.jdx.dev) pins every toolchain (Rust, Java, Gradle, Android SDK, cargo-deny, git-cliff,
actionlint, shellcheck, taplo):

```sh
mise install
mise run android:sdk   # once: Android SDK packages into .android-sdk
```

| Layer | Command | Needs |
| --- | --- | --- |
| Rust unit and integration tests (protocol, engine, server over TCP, .RPP parser) | `mise run test` | – |
| Lints: clippy for Linux, macOS and Windows, cargo-deny, rustfmt, taplo, shellcheck, actionlint, actions pinned to commit SHAs and allowed, docs check | `mise run lint` | – |
| Extension in a real, headless REAPER (band fixture and Mother Project) | `mise run e2e:reaper` | REAPER, `cage` or Xvfb |
| The same against REAPER releases from 5.93 to 7.82, downloaded on demand | `mise run e2e:reaper-versions [VERSION...]` | `cage` or Xvfb |
| Kotlin protocol and state tests, Kotlin client against the Rust simulator | `mise run android:test` | – |
| Android lint and ktlint | `mise run android:lint` | – |
| App on a phone against the simulated example project | `mise run android:e2e [tcp\|ble]` | phone (adb) |
| App on a phone against headless REAPER with the example project | `mise run android:e2e:reaper [ble\|tcp]` | phone, REAPER |

Tools:

- `cargo run -p bandctl -- sim --project band --ble --activity` simulates REAPER as a Bluetooth peripheral, moving
  faders and the playhead; `--tcp 127.0.0.1:47011` serves TCP. `--project band` is the test fixture
  ([`tests/fixtures/Band Project.RPP`](tests/fixtures/Band%20Project.RPP): a real band's layout with markers, a custom
  song order and a skipped song, which the tests need), `--project example` the example project, `--project
  FILE.RPP` any project. `bandctl pair` prints a pairing code, `bandctl client` prints what a server sends.
- `mise run reaper` starts an isolated headless REAPER with the extension and the band fixture (`--project FILE` for
  another); lines typed on stdin go to REAPER's web API (`SET/TRACK/13/VOL/0.5`), `screenshot FILE.png` saves a
  screenshot.
- `mise run package:windows x64|arm64 [DLL]` and `mise run package:macos [DYLIB]` build the installers, signed when
  the signing variables are set (see [`docs/RELEASING.md`](docs/RELEASING.md)). The Windows one needs `makensis`
  (NSIS) and, without a DLL argument, Windows; the macOS one needs macOS.
- `scripts/make-example-project.py` and `scripts/make-band-fixture.py` regenerate the two projects.
- `mise run android:deploy [-s SERIAL] [pairing-url]` installs and starts the app; with several devices in
  `adb devices` it asks which one (or takes `-s` / `ANDROID_SERIAL`), like the other phone scripts. A pairing URL
  with `&t=127.0.0.1:PORT` makes debug builds connect over TCP through `adb reverse`. `scripts/android-emulator.sh` boots a headless
  emulator (after `mise run android:emulator-sdk`) when there's no phone.

Extension environment variables: `BANDCONTROL_SECRET` (pairing code), `BANDCONTROL_TCP` (also listen on TCP),
`BANDCONTROL_BLE=0`, `BANDCONTROL_NAME`, `BANDCONTROL_LOG` (tracing filter). It logs to `bandcontrol.log` in REAPER's
resource directory.

### Docs, screenshots and releases

- **Features** are described once in [`docs/features.toml`](docs/features.toml); `mise run docs` renders them into
  this README and the website (`site/`), and `mise run lint` fails when the README is out of date.
- **Screenshots** in `docs/screenshots` are generated: `mise run docs:screenshots` runs `ScreenshotTest` on a phone
  or emulator against the simulator, and takes REAPER screenshots in a headless REAPER. The *Docs* workflow does the
  same on GitHub and opens a pull request when they change.
- **Commits** follow [Conventional Commits](https://www.conventionalcommits.org); [git-cliff](cliff.toml) turns
  `feat`, `fix`, `perf` and `docs` commits into [CHANGELOG.md](CHANGELOG.md) and the release notes.
- **Releases** are made by the *Create a release PR* workflow (choose patch / minor / major), which bumps the
  version and changelog in a pull request; merging it tags the release, and the *Publish* workflow builds, signs and
  uploads every artifact. See [docs/RELEASING.md](docs/RELEASING.md) for the one-time setup.

Agents and contributors: [AGENTS.md](AGENTS.md) describes the conventions in one place.

## License

[MIT](LICENSE)
