//! Pairing secret and pairing URL (`bandcontrol://pair?s=<code>&n=<name>`), encoded into a QR code.

use data_encoding::BASE32_NOPAD;

use crate::random::{RandomSource, array};

pub const SECRET_LEN: usize = 16;
const URL_PREFIX: &str = "bandcontrol://pair?";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PairingError {
    #[error("pairing code must be {SECRET_LEN} bytes of base32")]
    InvalidCode,
    #[error("not a bandcontrol pairing URL")]
    InvalidUrl,
}

/// The shared secret every paired phone knows.
#[derive(Clone, PartialEq, Eq)]
pub struct PairingSecret([u8; SECRET_LEN]);

impl std::fmt::Debug for PairingSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairingSecret(..)")
    }
}

impl PairingSecret {
    pub fn generate(rng: &mut dyn RandomSource) -> Self {
        Self(array(rng))
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; SECRET_LEN]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; SECRET_LEN] {
        &self.0
    }

    /// 26 characters of RFC 4648 base32.
    #[must_use]
    pub fn to_code(&self) -> String {
        BASE32_NOPAD.encode(&self.0)
    }

    /// Human friendly code: groups of 4 characters separated by dashes.
    #[must_use]
    pub fn to_display_code(&self) -> String {
        let code = self.to_code();
        let groups: Vec<String> = code
            .as_bytes()
            .chunks(4)
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect();
        groups.join("-")
    }

    /// Parses a code, ignoring case, whitespace and dashes.
    pub fn from_code(code: &str) -> Result<Self, PairingError> {
        let normalized: String = code
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let bytes = BASE32_NOPAD
            .decode(normalized.as_bytes())
            .map_err(|_| PairingError::InvalidCode)?;
        let bytes: [u8; SECRET_LEN] = bytes.try_into().map_err(|_| PairingError::InvalidCode)?;
        Ok(Self(bytes))
    }
}

/// Contents of the pairing QR code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingInfo {
    pub secret: PairingSecret,
    pub name: String,
}

impl PairingInfo {
    #[must_use]
    pub fn to_url(&self) -> String {
        format!(
            "{URL_PREFIX}s={}&n={}",
            self.secret.to_code(),
            percent_encode(&self.name)
        )
    }

    pub fn from_url(url: &str) -> Result<Self, PairingError> {
        let query = url.strip_prefix(URL_PREFIX).ok_or(PairingError::InvalidUrl)?;
        let mut secret = None;
        let mut name = String::new();
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "s" => secret = Some(PairingSecret::from_code(value)?),
                "n" => name = percent_decode(value).ok_or(PairingError::InvalidUrl)?,
                _ => {}
            }
        }
        Ok(Self {
            secret: secret.ok_or(PairingError::InvalidUrl)?,
            name,
        })
    }
}

fn percent_encode(s: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        match b {
            b'%' => {
                let hi = char::from(bytes.next()?).to_digit(16)?;
                let lo = char::from(bytes.next()?).to_digit(16)?;
                out.push(u8::try_from(hi * 16 + lo).ok()?);
            }
            b'+' => out.push(b' '),
            _ => out.push(b),
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::random::CountingRandom;

    #[test]
    fn code_roundtrip_is_lenient() {
        let secret = PairingSecret::generate(&mut CountingRandom::default());
        let code = secret.to_code();
        assert_eq!(code.len(), 26);
        assert_eq!(PairingSecret::from_code(&code).unwrap(), secret);
        let display = secret.to_display_code();
        assert_eq!(PairingSecret::from_code(&display.to_lowercase()).unwrap(), secret);
        assert_eq!(PairingSecret::from_code("abc"), Err(PairingError::InvalidCode));
    }

    #[test]
    fn url_roundtrip() {
        let info = PairingInfo {
            secret: PairingSecret::from_bytes([0xab; 16]),
            name: "Studio PC #1 / ünïcode".into(),
        };
        let url = info.to_url();
        assert!(url.starts_with("bandcontrol://pair?s="));
        assert!(!url.contains(' '));
        assert_eq!(PairingInfo::from_url(&url).unwrap(), info);
        assert_eq!(PairingInfo::from_url("https://x"), Err(PairingError::InvalidUrl));
        assert_eq!(
            PairingInfo::from_url("bandcontrol://pair?n=x"),
            Err(PairingError::InvalidUrl)
        );
    }
}
