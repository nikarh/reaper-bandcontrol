# Changelog

All notable changes to this project are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versions follow [Semantic Versioning](https://semver.org).

## [0.1.2] - 2026-10-20

### Features

- **android:** Previous / next and the transport controls look like buttons by @nikarh
- **android:** The IEM mix level is labelled "Mix level", the mix is the highlighted button above by @nikarh
- **android:** Every source in an IEM mix has a level meter again, at its level in the mix by @nikarh
- **android:** Mute buttons in the IEM tab look like the other buttons by @nikarh

### Fixes

- **android:** Pressing previous twice while playing goes back twice by @nikarh

### Documentation

- Example project songs have colors and markers for their parts by @nikarh
- Refresh screenshots (#8) by @nikarh-release-bot[bot] in #8

## [0.1.1] - 2026-10-19

### Features

- **proto:** Protobuf schema, framing, crypto and sans-IO sessions by @nikarh
- **core:** Domain model, Daw trait, sync engine and fake DAW by @nikarh
- **server:** Session loop with TCP and BlueZ BLE transports, TCP test client by @nikarh
- **bandctl:** Simulator with control port, test client and pairing QR by @nikarh
- **reaper:** Add REAPER extension with FFI bindings, ReaperDaw and pairing actions by @nikarh
- **android:** Kotlin protocol core with Wire, session, state selectors and TCP link by @nikarh
- **android:** Compose app with BLE/TCP transports, Control and Mix tabs, phone e2e tests by @nikarh
- **android:** Expose receive mute as an accessible switch by @nikarh
- **harness:** Optional fixed web interface port by @nikarh
- **server:** BLE peripheral backends for macOS (CoreBluetooth) and Windows (WinRT) by @nikarh
- **android:** Scan the pairing QR code in the app by @nikarh
- Track folders and level meters in the protocol, simulate project files by @nikarh
- **android:** Track groups from folders, meter subscriptions and peak holds in core by @nikarh
- **android:** DB faders, flat mix layout, Inputs and PA level tabs by @nikarh
- **android:** Compact bottom tab bar by @nikarh
- **android:** QR scanner with CameraX and ZXing instead of Google Play services by @nikarh
- **android:** Tabs Playback, Inputs, PA, IEM by @nikarh
- **android:** Explain connection problems in plain words instead of exception names by @nikarh
- **android:** Share the pairing with another phone as a QR code by @nikarh
- **reaper:** Set the name phones show for this computer by @nikarh
- **android:** Keep the paired computer's name up to date after a rename by @nikarh
- **bandctl:** Offline and online control commands for the simulator by @nikarh
- **android:** Remember several computers and pick the one in range by @nikarh
- **reaper:** Native pairing window instead of the browser page by @nikarh
- **reaper:** Pairing window follows REAPER's UI and font scaling by @nikarh
- **android:** Drag and drop setlist editor by @nikarh
- **android:** Share the app by QR code and link to the project by @nikarh
- **scripts:** Choose the device when several are connected by @nikarh
- A minimal example project to download by @nikarh
- **release:** Installers for Windows (x64, ARM64EC) and macOS by @nikarh
- The PA tab is now the FOH tab by @nikarh
- **reaper:** The supported REAPER versions in one place, checked at load by @nikarh
- **android:** F-Droid listing and a draft build recipe by @nikarh

### Fixes

- **android:** Expire unconfirmed local region order by @nikarh
- **android:** Format dB values with a decimal point regardless of locale by @nikarh
- **android:** Locale independent percent-encoding in pairing URLs by @nikarh
- **android:** Transport icons stay visible while disconnected by @nikarh
- **android:** Use LocalActivity for the share screen brightness by @nikarh
- **harness:** Kill the instance's process group with killpg, Xvfb display and screenshots by @nikarh
- **docs:** Build the site before serving it by @nikarh
- Run multi-step mise tasks in order by @nikarh
- **android:** Level meter track visible on the IEM mix header by @nikarh
- **release:** MacOS packaging works with macOS's bash 3.2 by @nikarh

### Documentation

- README, extension install task by @nikarh
- README manual, landing page, generated screenshots, changelog and contributor conventions by @nikarh
- Where pairings are saved and how computers are named; split downloads by part by @nikarh
- Several paired computers on one phone by @nikarh
- The pairing window, with a screenshot by @nikarh
- The pairing window follows REAPER's scaling by @nikarh
- Refresh app screenshots by @nikarh
- Setlist editing with a screenshot by @nikarh
- Sharing the app from a phone by @nikarh
- Supported REAPER versions, installers, the example project and its naming convention by @nikarh
- **site:** Icons on buttons, download cards and platform sections by @nikarh
- Front of house (FOH) instead of PA by @nikarh
- REAPER versions from Cargo.toml; lint checks the README names the same ones by @nikarh
- **site:** Consistent blocks and headings by @nikarh
- Only the three folders are a convention, the rest of the project is free by @nikarh
- **site:** A download card for the example project by @nikarh
- **site:** Phones and tablets: a screenshot carousel and a full screen view by @nikarh
- GitHub stars on the website, badges in the README by @nikarh
- **site:** The app icon in the header and as favicon by @nikarh
- The release signing key by @nikarh
- Screenshots without the empty status bar band by @nikarh
- CI screenshots on the same screen as local ones, cropped by the status bar only by @nikarh
- Screenshots only change when their pixels do by @nikarh
- Plainer description of how phones connect by @nikarh
- Plainer wording in the README, website and app description by @nikarh
- Shorter example project box on the website by @nikarh

[0.1.2]: https://github.com/nikarh/reaper-bandcontrol/compare/v0.1.1..v0.1.2

<!-- generated by git-cliff -->
