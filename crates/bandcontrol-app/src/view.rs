//! What the menu and the window say, without `AppKit`.

use bandcontrol_server::{TransportStatus, helper::Phone};

use crate::relay::Snapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Pending,
    Error,
}

/// One line about REAPER, Bluetooth and the phones.
#[must_use]
pub fn status(snapshot: &Snapshot) -> (Level, String) {
    if snapshot.pairing.is_none() {
        return (
            Level::Pending,
            "Waiting for REAPER: start REAPER with the BandControl extension".into(),
        );
    }
    match &snapshot.bluetooth {
        TransportStatus::Running(_) => (Level::Ok, format!("Bluetooth on · {}", phones(snapshot.phones.len()))),
        TransportStatus::Starting => (Level::Pending, "Bluetooth is starting…".into()),
        TransportStatus::Failed(e) => (Level::Error, format!("Bluetooth isn't working: {e}")),
        TransportStatus::Disabled => (Level::Error, "Bluetooth is off".into()),
    }
}

fn phones(n: usize) -> String {
    match n {
        0 => "no phones connected".into(),
        1 => "1 phone connected".into(),
        n => format!("{n} phones connected"),
    }
}

/// How a phone is listed: its name, or a placeholder for phones that send none.
#[must_use]
pub fn phone_name(phone: &Phone) -> String {
    let name = phone.name.trim();
    if name.is_empty() {
        format!("Phone {}", phone.client)
    } else {
        name.to_owned()
    }
}

/// The QR code's modules, row by row, without quiet zone, and its width.
#[must_use]
pub fn qr(url: &str) -> Option<(Vec<bool>, usize)> {
    let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
    let width = code.width();
    Some((
        code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect(),
        width,
    ))
}

#[cfg(test)]
mod tests {
    use bandcontrol_proto::pairing::{PairingInfo, PairingSecret};
    use pretty_assertions::assert_eq;

    use super::*;

    fn connected(bluetooth: TransportStatus, phones: usize) -> Snapshot {
        Snapshot {
            pairing: Some(PairingInfo {
                secret: PairingSecret::from_bytes([1; 16]),
                name: "Stage PC".into(),
            }),
            bluetooth,
            phones: (0..phones)
                .map(|i| Phone {
                    client: i as u64,
                    name: format!("Phone {i}"),
                })
                .collect(),
            blocked: vec![],
        }
    }

    #[test]
    fn status_lines() {
        assert_eq!(status(&Snapshot::default()).0, Level::Pending);
        assert_eq!(
            status(&connected(TransportStatus::Running(String::new()), 2)),
            (Level::Ok, "Bluetooth on · 2 phones connected".into())
        );
        assert_eq!(
            status(&connected(TransportStatus::Running(String::new()), 1)).1,
            "Bluetooth on · 1 phone connected"
        );
        assert_eq!(
            status(&connected(TransportStatus::Failed("Bluetooth is turned off".into()), 0)),
            (Level::Error, "Bluetooth isn't working: Bluetooth is turned off".into())
        );
    }

    #[test]
    fn unnamed_phones_get_a_name() {
        assert_eq!(
            phone_name(&Phone {
                client: 4,
                name: " ".into()
            }),
            "Phone 4"
        );
    }

    #[test]
    fn qr_is_square() {
        let (modules, width) = qr("bandcontrol://pair?s=ABC").unwrap();
        assert_eq!(modules.len(), width * width);
    }
}
