//! Packet framing.
//!
//! A packet is one BLE notification or write: `header (1 byte) || chunk`. The header's top bit says that more
//! fragments of the same message follow, the low 7 bits are the channel. A message is the concatenation of the chunks
//! of consecutive packets on one channel, terminated by a packet without the "more" bit.

use std::collections::HashMap;

/// Channel of broadcast frames (encrypted with the group key).
pub const CHANNEL_BROADCAST: u8 = 0;
/// Channel of plaintext handshake frames.
pub const CHANNEL_HANDSHAKE: u8 = 127;
/// Largest valid session id / unicast channel.
pub const MAX_SESSION_CHANNEL: u8 = 126;
/// Smallest packet size, the BLE default MTU (23) minus the ATT header (3).
pub const MIN_PACKET: usize = 20;
/// Upper bound of a reassembled message.
pub const MAX_MESSAGE: usize = 64 * 1024;

const MORE: u8 = 0x80;
const CHANNEL_MASK: u8 = 0x7f;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FramingError {
    #[error("empty packet")]
    Empty,
    #[error("message on channel {0} exceeds {MAX_MESSAGE} bytes")]
    TooLarge(u8),
}

/// Splits `message` into packets of at most `max_packet` bytes (header included) for `channel`.
#[must_use]
pub fn fragment(channel: u8, message: &[u8], max_packet: usize) -> Vec<Vec<u8>> {
    let channel = channel & CHANNEL_MASK;
    let chunk = max_packet.max(MIN_PACKET) - 1;
    if message.is_empty() {
        return vec![vec![channel]];
    }

    let count = message.len().div_ceil(chunk);
    message
        .chunks(chunk)
        .enumerate()
        .map(|(i, part)| {
            let more = if i + 1 < count { MORE } else { 0 };
            let mut packet = Vec::with_capacity(part.len() + 1);
            packet.push(channel | more);
            packet.extend_from_slice(part);
            packet
        })
        .collect()
}

/// Reassembles messages from packets, independently per channel.
#[derive(Debug, Default)]
pub struct Reassembler {
    partial: HashMap<u8, Vec<u8>>,
}

impl Reassembler {
    /// Feeds one packet. Returns the channel and the message when the packet completes one.
    pub fn push(&mut self, packet: &[u8]) -> Result<Option<(u8, Vec<u8>)>, FramingError> {
        let (&header, chunk) = packet.split_first().ok_or(FramingError::Empty)?;
        let channel = header & CHANNEL_MASK;
        let more = header & MORE != 0;

        let buffer = self.partial.entry(channel).or_default();
        if buffer.len() + chunk.len() > MAX_MESSAGE {
            self.partial.remove(&channel);
            return Err(FramingError::TooLarge(channel));
        }
        buffer.extend_from_slice(chunk);

        if more {
            return Ok(None);
        }
        Ok(self.partial.remove(&channel).map(|message| (channel, message)))
    }

    /// Drops a partially received message of `channel`.
    pub fn reset(&mut self, channel: u8) {
        self.partial.remove(&channel);
    }

    /// Peeks at the channel of a packet without consuming it.
    #[must_use]
    pub fn channel_of(packet: &[u8]) -> Option<u8> {
        packet.first().map(|header| header & CHANNEL_MASK)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn empty_message_is_a_single_header() {
        assert_eq!(fragment(5, &[], 100), vec![vec![5]]);
        let mut r = Reassembler::default();
        assert_eq!(r.push(&[5]).unwrap(), Some((5, vec![])));
    }

    #[test]
    fn splits_on_chunk_boundary() {
        let msg: Vec<u8> = (0..38).collect();
        let packets = fragment(1, &msg, 20);
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0][0], 0x81);
        assert_eq!(packets[0].len(), 20);
        assert_eq!(packets[1][0], 0x01);
        assert_eq!(packets[1].len(), 20);
    }

    #[test]
    fn small_max_packet_is_clamped() {
        let packets = fragment(1, &[0; 40], 2);
        assert!(packets.iter().all(|p| p.len() <= MIN_PACKET));
    }

    #[test]
    fn rejects_oversized_messages() {
        let mut r = Reassembler::default();
        let chunk = vec![0x80 | 3; 1001];
        let mut result = Ok(None);
        for _ in 0..=(MAX_MESSAGE / 1000) {
            result = r.push(&chunk);
            if result.is_err() {
                break;
            }
        }
        assert_eq!(result, Err(FramingError::TooLarge(3)));
        assert_eq!(r.push(&[3, 1]).unwrap(), Some((3, vec![1])));
    }

    #[test]
    fn rejects_empty_packet() {
        assert_eq!(Reassembler::default().push(&[]), Err(FramingError::Empty));
    }

    proptest! {
        #[test]
        fn roundtrip_interleaved_channels(
            a in proptest::collection::vec(any::<u8>(), 0..2000),
            b in proptest::collection::vec(any::<u8>(), 0..2000),
            max_packet in 20usize..600,
        ) {
            let pa = fragment(CHANNEL_BROADCAST, &a, max_packet);
            let pb = fragment(42, &b, max_packet);
            let mut r = Reassembler::default();
            let mut out = Vec::new();
            let mut ia = pa.iter();
            let mut ib = pb.iter();
            loop {
                let (x, y) = (ia.next(), ib.next());
                if x.is_none() && y.is_none() { break; }
                for p in [x, y].into_iter().flatten() {
                    prop_assert!(p.len() <= max_packet);
                    if let Some(m) = r.push(p).unwrap() { out.push(m); }
                }
            }
            out.sort_by_key(|(c, _)| *c);
            prop_assert_eq!(out, vec![(CHANNEL_BROADCAST, a), (42, b)]);
        }
    }
}
