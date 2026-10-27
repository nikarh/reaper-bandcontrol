//! Pairing page: an HTML file with the pairing QR code, opened in the default browser.

use std::{io, path::Path, process::Command};

use bandcontrol_proto::pairing::PairingInfo;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[must_use]
pub fn page(info: &PairingInfo) -> String {
    let url = info.to_url();
    let svg = qrcode::QrCode::new(url.as_bytes()).map_or_else(
        |e| format!("<p>QR code generation failed: {e}</p>"),
        |code| {
            code.render::<qrcode::render::svg::Color<'_>>()
                .min_dimensions(320, 320)
                .quiet_zone(true)
                .build()
        },
    );
    let name = escape(&info.name);
    let code = escape(&info.secret.to_display_code());
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>BandControl pairing — {name}</title>
<style>
body {{ font-family: sans-serif; background: #111827; color: #e5e7eb; display: flex; flex-direction: column;
       align-items: center; gap: 1rem; padding: 2rem; }}
.qr {{ background: white; padding: 1rem; border-radius: 1rem; }}
code {{ font-size: 1.5rem; letter-spacing: 0.1em; }}
</style></head>
<body>
<h1>Pair a phone with {name}</h1>
<p>Scan with the phone camera (or from the BandControl app).</p>
<div class="qr">{svg}</div>
<p>Or enter this code in the app:</p>
<code>{code}</code>
<p>Anyone with this code can control playback and mixes. Use "BandControl: Reset pairing key" to revoke all phones.</p>
</body></html>
"#
    )
}

pub fn open(path: &Path) -> io::Result<()> {
    let mut command = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else if cfg!(target_os = "macos") {
        Command::new("open")
    } else {
        Command::new("xdg-open")
    };
    command.arg(path).spawn().map(drop)
}

/// Best effort machine name for the server name shown on phones.
#[must_use]
pub fn hostname() -> String {
    computer_name()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "REAPER".into())
}

/// macOS: the name from *System Settings → General → Sharing*, like "Alex's Mac". Applications started from
/// the Finder have no `HOSTNAME`, and the host name is a mangled version of this one.
#[cfg(target_os = "macos")]
fn computer_name() -> Option<String> {
    let out = Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(not(target_os = "macos"))]
fn computer_name() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use bandcontrol_proto::pairing::PairingSecret;

    use super::*;

    #[test]
    fn page_contains_qr_and_escaped_name() {
        let html = page(&PairingInfo {
            secret: PairingSecret::from_bytes([1; 16]),
            name: "<Studio>".into(),
        });
        assert!(html.contains("<svg"));
        assert!(html.contains("&lt;Studio&gt;"));
        assert!(!html.contains("<Studio>"));
    }
}
