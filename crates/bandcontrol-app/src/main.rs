//! BandControl for macOS: the app that gives the REAPER extension Bluetooth, since macOS doesn't let REAPER use it.
//! It lives in the menu bar, shows the pairing QR code and the connected phones, and relays packets between the
//! phones and the extension (see `bandcontrol_server::helper`).

// Unix only (Unix sockets); built and tested on Linux too, used on macOS.
#[cfg(unix)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod relay;
#[cfg(unix)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod view;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    macos::main()
}

/// Only macOS needs the app; elsewhere the extension uses Bluetooth itself.
#[cfg(not(target_os = "macos"))]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::FAILURE
}
