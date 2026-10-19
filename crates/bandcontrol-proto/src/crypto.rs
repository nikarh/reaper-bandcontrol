//! Key derivation, proofs and AEAD framing.
//!
//! ```text
//! auth_key     = HKDF-SHA256(ikm = secret, salt = "bandcontrol/v1", info = "auth")
//! service_uuid = HKDF-SHA256(ikm = secret, salt = "bandcontrol/v1", info = "service-uuid")[..16], as UUIDv8
//! server_proof = HMAC-SHA256(auth_key, "server" || client_nonce || server_nonce || session_id)
//! client_proof = HMAC-SHA256(auth_key, "client" || client_nonce || server_nonce || session_id)
//! s2c / c2s    = HKDF-SHA256(ikm = auth_key, salt = client_nonce || server_nonce, info = "s2c" / "c2s")
//! ```
//!
//! Messages are sealed with ChaCha20-Poly1305, nonce = `counter (u64 LE) || 0u32`. Unicast counters are implicit
//! (both sides count messages), broadcast frames carry the counter as a LEB128 prefix.

use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit as _,
    aead::{Aead as _, array::Array},
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac as _};
use sha2::Sha256;

use crate::pairing::PairingSecret;

pub const NONCE_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
pub const PROOF_LEN: usize = 32;
pub const TAG_LEN: usize = 16;

const SALT: &[u8] = b"bandcontrol/v1";

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("message authentication failed")]
    Decrypt,
    #[error("malformed broadcast frame")]
    Malformed,
    #[error("message counter exhausted")]
    CounterExhausted,
}

fn hkdf<const N: usize>(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; N] {
    let mut out = [0; N];
    // Expanding at most 32 bytes from SHA-256 never exceeds the 255 * 32 byte limit.
    let _ = Hkdf::<Sha256>::new(Some(salt), ikm).expand(info, &mut out);
    out
}

fn hmac(key: &[u8], parts: &[&[u8]]) -> HmacSha256 {
    #[allow(clippy::expect_used)]
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    for part in parts {
        mac.update(part);
    }
    mac
}

/// Service UUID (big endian bytes) derived from the pairing secret.
#[must_use]
pub fn service_uuid(secret: &PairingSecret) -> [u8; 16] {
    let mut uuid: [u8; 16] = hkdf(secret.as_bytes(), SALT, b"service-uuid");
    uuid[6] = (uuid[6] & 0x0f) | 0x80; // version 8 (custom)
    uuid[8] = (uuid[8] & 0x3f) | 0x80; // RFC 4122 variant
    uuid
}

/// Long term key derived from the pairing secret.
#[derive(Clone)]
pub struct AuthKey([u8; KEY_LEN]);

impl std::fmt::Debug for AuthKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthKey(..)")
    }
}

impl AuthKey {
    #[must_use]
    pub fn derive(secret: &PairingSecret) -> Self {
        Self(hkdf(secret.as_bytes(), SALT, b"auth"))
    }

    fn proof_mac(&self, role: &[u8], client_nonce: &[u8], server_nonce: &[u8], session_id: u8) -> HmacSha256 {
        hmac(&self.0, &[role, client_nonce, server_nonce, &[session_id]])
    }

    #[must_use]
    pub fn server_proof(&self, client_nonce: &[u8], server_nonce: &[u8], session_id: u8) -> [u8; PROOF_LEN] {
        self.proof_mac(b"server", client_nonce, server_nonce, session_id)
            .finalize()
            .into_bytes()
            .into()
    }

    #[must_use]
    pub fn client_proof(&self, client_nonce: &[u8], server_nonce: &[u8], session_id: u8) -> [u8; PROOF_LEN] {
        self.proof_mac(b"client", client_nonce, server_nonce, session_id)
            .finalize()
            .into_bytes()
            .into()
    }

    /// Constant time verification of a server proof.
    #[must_use]
    pub fn verify_server_proof(&self, client_nonce: &[u8], server_nonce: &[u8], session_id: u8, proof: &[u8]) -> bool {
        self.proof_mac(b"server", client_nonce, server_nonce, session_id)
            .verify_slice(proof)
            .is_ok()
    }

    /// Constant time verification of a client proof.
    #[must_use]
    pub fn verify_client_proof(&self, client_nonce: &[u8], server_nonce: &[u8], session_id: u8, proof: &[u8]) -> bool {
        self.proof_mac(b"client", client_nonce, server_nonce, session_id)
            .verify_slice(proof)
            .is_ok()
    }

    /// Directional session keys `(server_to_client, client_to_server)`.
    #[must_use]
    pub fn session_keys(&self, client_nonce: &[u8], server_nonce: &[u8]) -> ([u8; KEY_LEN], [u8; KEY_LEN]) {
        let salt = [client_nonce, server_nonce].concat();
        (hkdf(&self.0, &salt, b"s2c"), hkdf(&self.0, &salt, b"c2s"))
    }
}

fn nonce(counter: u64) -> Array<u8, chacha20poly1305::consts::U12> {
    let mut n = [0u8; 12];
    n[..8].copy_from_slice(&counter.to_le_bytes());
    Array::from(n)
}

fn cipher(key: &[u8; KEY_LEN]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(&Array::from(*key))
}

/// Seals unicast messages with an implicit counter.
pub struct Sealer {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

impl std::fmt::Debug for Sealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sealer")
            .field("counter", &self.counter)
            .finish_non_exhaustive()
    }
}

impl Sealer {
    #[must_use]
    pub fn new(key: &[u8; KEY_LEN]) -> Self {
        Self {
            cipher: cipher(key),
            counter: 0,
        }
    }

    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let counter = self.counter;
        self.counter = counter.checked_add(1).ok_or(CryptoError::CounterExhausted)?;
        self.cipher
            .encrypt(&nonce(counter), plaintext)
            .map_err(|_| CryptoError::CounterExhausted)
    }
}

/// Opens unicast messages with an implicit counter.
pub struct Opener {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

impl std::fmt::Debug for Opener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opener")
            .field("counter", &self.counter)
            .finish_non_exhaustive()
    }
}

impl Opener {
    #[must_use]
    pub fn new(key: &[u8; KEY_LEN]) -> Self {
        Self {
            cipher: cipher(key),
            counter: 0,
        }
    }

    pub fn open(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let plaintext = self
            .cipher
            .decrypt(&nonce(self.counter), ciphertext)
            .map_err(|_| CryptoError::Decrypt)?;
        self.counter = self.counter.checked_add(1).ok_or(CryptoError::CounterExhausted)?;
        Ok(plaintext)
    }
}

/// Seals broadcast frames: `leb128(counter) || ciphertext`.
pub struct BroadcastSealer {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

impl std::fmt::Debug for BroadcastSealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcastSealer")
            .field("counter", &self.counter)
            .finish_non_exhaustive()
    }
}

impl BroadcastSealer {
    #[must_use]
    pub fn new(key: &[u8; KEY_LEN]) -> Self {
        Self {
            cipher: cipher(key),
            counter: 0,
        }
    }

    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let counter = self.counter;
        self.counter = counter.checked_add(1).ok_or(CryptoError::CounterExhausted)?;
        let ciphertext = self
            .cipher
            .encrypt(&nonce(counter), plaintext)
            .map_err(|_| CryptoError::CounterExhausted)?;
        let mut out = Vec::with_capacity(10 + ciphertext.len());
        prost::encoding::encode_varint(counter, &mut out);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }
}

/// Opens broadcast frames. Rejects counters that are not strictly increasing (replay protection).
pub struct BroadcastOpener {
    cipher: ChaCha20Poly1305,
    next: u64,
}

impl std::fmt::Debug for BroadcastOpener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcastOpener")
            .field("next", &self.next)
            .finish_non_exhaustive()
    }
}

impl BroadcastOpener {
    #[must_use]
    pub fn new(key: &[u8; KEY_LEN]) -> Self {
        Self {
            cipher: cipher(key),
            next: 0,
        }
    }

    pub fn open(&mut self, frame: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut buf = frame;
        let counter = prost::encoding::decode_varint(&mut buf).map_err(|_| CryptoError::Malformed)?;
        if counter < self.next {
            return Err(CryptoError::Decrypt);
        }
        let plaintext = self
            .cipher
            .decrypt(&nonce(counter), buf)
            .map_err(|_| CryptoError::Decrypt)?;
        self.next = counter.saturating_add(1);
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::{assert_eq, assert_ne};

    use super::*;

    fn secret() -> PairingSecret {
        PairingSecret::from_bytes([7; 16])
    }

    #[test]
    fn proofs_verify_and_are_role_bound() {
        let key = AuthKey::derive(&secret());
        let (cn, sn) = ([1; 16], [2; 16]);
        let sp = key.server_proof(&cn, &sn, 3);
        let cp = key.client_proof(&cn, &sn, 3);
        assert_ne!(sp, cp);
        assert!(key.verify_server_proof(&cn, &sn, 3, &sp));
        assert!(!key.verify_server_proof(&cn, &sn, 4, &sp));
        assert!(!key.verify_client_proof(&cn, &sn, 3, &sp));
        assert!(key.verify_client_proof(&cn, &sn, 3, &cp));

        let other = AuthKey::derive(&PairingSecret::from_bytes([8; 16]));
        assert!(!other.verify_client_proof(&cn, &sn, 3, &cp));
    }

    #[test]
    fn unicast_roundtrip_and_replay_rejection() {
        let key = [9; KEY_LEN];
        let mut s = Sealer::new(&key);
        let mut o = Opener::new(&key);
        let a = s.seal(b"hello").unwrap();
        let b = s.seal(b"world").unwrap();
        assert_eq!(a.len(), 5 + TAG_LEN);
        assert_eq!(o.open(&a).unwrap(), b"hello");
        assert_eq!(o.open(&a), Err(CryptoError::Decrypt));
        // A failed open does not advance the counter.
        assert_eq!(o.open(&b).unwrap(), b"world");
    }

    #[test]
    fn broadcast_allows_gaps_but_not_replays() {
        let key = [3; KEY_LEN];
        let mut s = BroadcastSealer::new(&key);
        let mut o = BroadcastOpener::new(&key);
        let a = s.seal(b"a").unwrap();
        let _missed = s.seal(b"b").unwrap();
        let c = s.seal(b"c").unwrap();
        assert_eq!(o.open(&c).unwrap(), b"c");
        assert_eq!(o.open(&a), Err(CryptoError::Decrypt));
        assert_eq!(o.open(&c), Err(CryptoError::Decrypt));
    }

    #[test]
    fn tampering_is_detected() {
        let key = [3; KEY_LEN];
        let mut frame = BroadcastSealer::new(&key).seal(b"abc").unwrap();
        let last = frame.len() - 1;
        frame[last] ^= 1;
        assert_eq!(BroadcastOpener::new(&key).open(&frame), Err(CryptoError::Decrypt));
    }

    #[test]
    fn service_uuid_is_valid_v8() {
        let uuid = service_uuid(&secret());
        assert_eq!(uuid[6] >> 4, 8);
        assert_eq!(uuid[8] >> 6, 2);
        assert_ne!(uuid, service_uuid(&PairingSecret::from_bytes([8; 16])));
    }
}
