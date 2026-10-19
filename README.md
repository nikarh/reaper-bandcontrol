# reaper-bandcontrol

Control REAPER from the band's phones over Bluetooth LE, without Wi-Fi, tethering or typing IP addresses.

A REAPER extension (Rust) turns the computer into a Bluetooth LE peripheral. An Android app connects to it and
offers the same two tabs as [reaper-remote-bandui](https://github.com/nikarh/reaper-remote-bandui):

- **Control**: transport, previous/next marker, song (region) list with markers and progress, song order editor
- **Mix**: each band member's IEM mix: the mix level on top, the receive levels and mutes of their output track below
- **Inputs** and **PA**: gain and live level of every input and PA output, with peak hold and clip indicator

## Install

1. Install the extension: `mise run install` builds it and copies it to REAPER's `UserPlugins` (Linux and macOS;
   on Windows copy `target/release/reaper_bandcontrol.dll` there). Restart REAPER.
2. Install the app on each phone: `mise run android:build`, then install
   `android/app/build/outputs/apk/release/app-release.apk`.
3. In REAPER, run the action **BandControl: Show pairing QR code**. It opens a page with a QR code and the pairing
   code. Scan the QR code with the phone or type the code into the app.

All phones use the same pairing code. **BandControl: Reset pairing key** creates a new one and unpairs every phone.
**BandControl: Show status** shows whether Bluetooth is up and how many phones are connected.

### Platform notes

- Linux: needs BlueZ (`bluetoothd`); the adapter is powered on automatically.
- macOS: macOS only lets an application use Bluetooth if its `Info.plist` has `NSBluetoothAlwaysUsageDescription`.
  The extension checks this and shows an error in the status instead of letting macOS terminate REAPER.
- Windows: Windows 10 1803 or later with an adapter that supports the peripheral role.
- The macOS and Windows backends have not been tested on real hardware yet.

## REAPER project layout

The same rules as for reaper-remote-bandui (see `Mother Project.RPP` there):

1. Songs are named regions that don't overlap. Markers inside a region are the song's parts.
2. An output (a band member's IEM mix) is a track with a hardware output and at least one receive. The Mix tab
   changes the volume and mute of those receives. If the mixes are in a folder named like "IEM" or "Monitor", only
   those are listed.
3. Optional: direct children of a top level folder named like "Inputs" show in the Inputs tab, direct children of a
   top level folder with "PA" in its name in the PA tab. Gain there is the track volume, levels are post-fader.

The song order and disabled songs are stored in the project (`BANDUI/regions` project state), compatible with
reaper-remote-bandui.

## How it works

- On connect a phone gets the full state: tracks, volumes, receives, regions, markers and transport. After that:
  transport once a second, volume and mute changes as deltas, and the full state again (debounced to 1 s) when
  tracks, receives, regions or markers change.
- Messages are protobuf (`proto/bandcontrol.proto`) in a small framing layer that splits them into BLE-sized
  packets.
- Security: the pairing code is a 128-bit secret. It derives the Bluetooth service UUID, so only paired phones even
  find the computer, and the key for a mutual challenge-response handshake. Each session is then encrypted and
  authenticated with ChaCha20-Poly1305. Unpaired devices can connect at the Bluetooth level but can't read or
  change anything.

## Development

[mise](https://mise.jdx.dev) pins every toolchain (Rust, Java, Gradle, Android SDK, cargo-deny, shellcheck, taplo):

```sh
mise install
mise run android:sdk   # once: Android SDK packages into .android-sdk
```

| Layer | Command | Needs |
| --- | --- | --- |
| Rust unit and integration tests (protocol, engine, server over TCP) | `mise run test` | – |
| Lints: clippy for Linux, macOS and Windows, cargo-deny, rustfmt, taplo, shellcheck | `mise run lint` | – |
| Extension in a real, headless REAPER, driven over TCP and REAPER's web API | `mise run e2e:reaper` | REAPER, `cage` |
| Kotlin protocol and state tests, Kotlin client against the Rust simulator | `mise run android:test` | – |
| Android lint and ktlint | `mise run android:lint` | – |
| App on the phone against the simulator | `mise run android:e2e [tcp\|ble]` | phone (adb) |
| App on the phone against headless REAPER | `mise run android:e2e:reaper [ble\|tcp]` | phone, REAPER |

Manual tools:

- `cargo run -p bandctl -- sim --ble --activity` runs a simulated REAPER (the Mother Project) as a Bluetooth peripheral
  and moves faders and the playhead; `bandctl sim --tcp 127.0.0.1:47011` serves TCP instead. `--project FILE.RPP`
  simulates a real project (tracks, folders, receives, regions), `--project band` a bundled band layout. `bandctl pair` prints a
  pairing code and QR, `bandctl client` connects and prints what it receives.
- `mise run reaper` starts an isolated headless REAPER with the extension (fresh resource directory under
  `target/reaper-harness`) and sends lines typed on stdin to REAPER's web API, e.g. `SET/TRACK/13/VOL/0.5`.
  `BANDCONTROL_*` variables are passed through; `REAPER_HEADLESS=0` shows the window.
- `mise run android:deploy [pairing-url]` installs and starts the app on the phone; `scripts/android-tap.sh <tag>`
  taps an element by test tag, `scripts/android-screenshot.sh` saves a screenshot. A pairing URL with
  `&t=127.0.0.1:PORT` makes debug builds connect over TCP through `adb reverse` instead of Bluetooth.

Extension environment variables: `BANDCONTROL_SECRET` (pairing code), `BANDCONTROL_TCP` (also listen on TCP),
`BANDCONTROL_BLE=0`, `BANDCONTROL_NAME`, `BANDCONTROL_LOG` (tracing filter). The extension logs to
`bandcontrol.log` in REAPER's resource directory.

`PLAN.md` tracks progress and design notes.
