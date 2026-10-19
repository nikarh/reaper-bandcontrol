//! Sans-IO client side of the session protocol. The Android app implements the same state machine in Kotlin.

use prost::Message as _;

use crate::{
    PROTOCOL_VERSION,
    crypto::{AuthKey, BroadcastOpener, KEY_LEN, NONCE_LEN, Opener, Sealer},
    framing::{CHANNEL_BROADCAST, CHANNEL_HANDSHAKE, MAX_SESSION_CHANNEL, Reassembler, fragment},
    pairing::PairingSecret,
    pb,
    random::{RandomSource, array},
    server::SessionId,
};

#[derive(Debug, PartialEq)]
pub enum ClientEvent {
    /// Packets to write to the server.
    Send(Vec<Vec<u8>>),
    Authenticated {
        session: SessionId,
        server_name: String,
    },
    Message(pb::ServerMessage),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ClientError {
    #[error("server rejected authentication: {0:?}")]
    AuthFailed(pb::AuthFailure),
    #[error("server failed to prove knowledge of the pairing secret")]
    BadServerProof,
    #[error("protocol violation: {0}")]
    Protocol(&'static str),
    #[error("not connected")]
    NotReady,
}

#[derive(Debug)]
enum Phase {
    New,
    AwaitChallenge,
    AwaitWelcome {
        session: SessionId,
        sealer: Sealer,
        opener: Opener,
        server_name: String,
    },
    Ready {
        session: SessionId,
        sealer: Sealer,
        opener: Opener,
        group: BroadcastOpener,
    },
    Failed,
}

#[derive(Debug)]
pub struct ClientSession {
    auth_key: AuthKey,
    name: String,
    client_nonce: [u8; NONCE_LEN],
    max_packet: usize,
    reassembler: Reassembler,
    phase: Phase,
}

impl ClientSession {
    pub fn new(secret: &PairingSecret, name: impl Into<String>, max_packet: usize, rng: &mut dyn RandomSource) -> Self {
        Self {
            auth_key: AuthKey::derive(secret),
            name: name.into(),
            client_nonce: array(rng),
            max_packet,
            reassembler: Reassembler::default(),
            phase: Phase::New,
        }
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self.phase, Phase::Ready { .. })
    }

    #[must_use]
    pub fn session(&self) -> Option<SessionId> {
        match &self.phase {
            Phase::AwaitWelcome { session, .. } | Phase::Ready { session, .. } => Some(*session),
            _ => None,
        }
    }

    /// Packets of the initial `Hello`.
    pub fn hello(&mut self) -> Vec<Vec<u8>> {
        self.phase = Phase::AwaitChallenge;
        let hello = pb::Handshake {
            msg: Some(pb::handshake::Msg::Hello(pb::Hello {
                version: PROTOCOL_VERSION,
                client_nonce: self.client_nonce.to_vec(),
                max_packet: u32::try_from(self.max_packet).unwrap_or(u32::MAX),
                client_name: self.name.clone(),
            })),
        };
        fragment(CHANNEL_HANDSHAKE, &hello.encode_to_vec(), self.max_packet)
    }

    /// Encrypts a command.
    pub fn send(&mut self, msg: &pb::ClientMessage) -> Result<Vec<Vec<u8>>, ClientError> {
        let Phase::Ready { session, sealer, .. } = &mut self.phase else {
            return Err(ClientError::NotReady);
        };
        let frame = sealer
            .seal(&msg.encode_to_vec())
            .map_err(|_| ClientError::Protocol("counter exhausted"))?;
        Ok(fragment(*session, &frame, self.max_packet))
    }

    /// Handles a packet notified by the server. Errors are fatal for the session.
    pub fn receive(&mut self, packet: &[u8]) -> Result<Vec<ClientEvent>, ClientError> {
        let Some(channel) = Reassembler::channel_of(packet) else {
            return Ok(vec![]);
        };
        let wanted = match &self.phase {
            Phase::AwaitChallenge => channel == CHANNEL_HANDSHAKE,
            Phase::AwaitWelcome { session, .. } => channel == *session,
            Phase::Ready { session, .. } => channel == *session || channel == CHANNEL_BROADCAST,
            Phase::New | Phase::Failed => false,
        };
        if !wanted {
            return Ok(vec![]);
        }

        let message = match self.reassembler.push(packet) {
            Ok(Some((_, m))) => m,
            Ok(None) => return Ok(vec![]),
            Err(_) if channel == CHANNEL_BROADCAST || channel == CHANNEL_HANDSHAKE => return Ok(vec![]),
            Err(_) => return self.fail(ClientError::Protocol("oversized message")),
        };

        match channel {
            CHANNEL_HANDSHAKE => self.on_handshake(&message),
            CHANNEL_BROADCAST => Ok(self.on_broadcast(&message)),
            1..=MAX_SESSION_CHANNEL => self.on_unicast(&message),
            _ => Ok(vec![]),
        }
    }

    fn fail<T>(&mut self, error: ClientError) -> Result<T, ClientError> {
        self.phase = Phase::Failed;
        Err(error)
    }

    fn on_handshake(&mut self, message: &[u8]) -> Result<Vec<ClientEvent>, ClientError> {
        // Other clients' handshakes may be visible on broadcast transports, ignore anything not addressed to us.
        let Ok(pb::Handshake { msg: Some(msg) }) = pb::Handshake::decode(message) else {
            return Ok(vec![]);
        };
        match msg {
            pb::handshake::Msg::Challenge(c) if c.client_nonce == self.client_nonce => self.on_challenge(c),
            pb::handshake::Msg::Failed(f) if f.client_nonce == self.client_nonce => {
                let reason = pb::AuthFailure::try_from(f.reason).unwrap_or(pb::AuthFailure::Unspecified);
                self.fail(ClientError::AuthFailed(reason))
            }
            _ => Ok(vec![]),
        }
    }

    fn on_challenge(&mut self, c: pb::Challenge) -> Result<Vec<ClientEvent>, ClientError> {
        let Ok(session @ 1..=MAX_SESSION_CHANNEL) = u8::try_from(c.session_id) else {
            return self.fail(ClientError::Protocol("invalid session id"));
        };
        if !self
            .auth_key
            .verify_server_proof(&self.client_nonce, &c.server_nonce, session, &c.server_proof)
        {
            return self.fail(ClientError::BadServerProof);
        }

        let proof = self.auth_key.client_proof(&self.client_nonce, &c.server_nonce, session);
        let (s2c, c2s) = self.auth_key.session_keys(&self.client_nonce, &c.server_nonce);
        self.reassembler.reset(session);
        self.phase = Phase::AwaitWelcome {
            session,
            sealer: Sealer::new(&c2s),
            opener: Opener::new(&s2c),
            server_name: c.server_name,
        };

        let auth = pb::Handshake {
            msg: Some(pb::handshake::Msg::Auth(pb::Auth {
                client_proof: proof.to_vec(),
            })),
        };
        Ok(vec![ClientEvent::Send(fragment(
            CHANNEL_HANDSHAKE,
            &auth.encode_to_vec(),
            self.max_packet,
        ))])
    }

    fn on_unicast(&mut self, message: &[u8]) -> Result<Vec<ClientEvent>, ClientError> {
        let (Phase::AwaitWelcome { opener, .. } | Phase::Ready { opener, .. }) = &mut self.phase else {
            return Ok(vec![]);
        };
        let Ok(plaintext) = opener.open(message) else {
            return self.fail(ClientError::Protocol("unicast frame failed authentication"));
        };
        let Ok(msg) = pb::ServerMessage::decode(plaintext.as_slice()) else {
            return self.fail(ClientError::Protocol("malformed server message"));
        };

        match std::mem::replace(&mut self.phase, Phase::Failed) {
            Phase::AwaitWelcome {
                session,
                sealer,
                opener,
                server_name,
            } => {
                let Some(pb::server_message::Msg::Welcome(welcome)) = msg.msg else {
                    return self.fail(ClientError::Protocol("expected welcome"));
                };
                let Ok(group_key) = <[u8; KEY_LEN]>::try_from(welcome.group_key.as_slice()) else {
                    return self.fail(ClientError::Protocol("invalid group key"));
                };
                self.reassembler.reset(CHANNEL_BROADCAST);
                self.phase = Phase::Ready {
                    session,
                    sealer,
                    opener,
                    group: BroadcastOpener::new(&group_key),
                };
                Ok(vec![ClientEvent::Authenticated { session, server_name }])
            }
            ready @ Phase::Ready { .. } => {
                self.phase = ready;
                Ok(Self::message_event(msg))
            }
            other => {
                self.phase = other;
                Ok(vec![])
            }
        }
    }

    fn on_broadcast(&mut self, message: &[u8]) -> Vec<ClientEvent> {
        let Phase::Ready { group, .. } = &mut self.phase else {
            return vec![];
        };
        // Broadcast frames can be truncated when we started listening mid-message, drop them silently.
        let Ok(plaintext) = group.open(message) else {
            return vec![];
        };
        pb::ServerMessage::decode(plaintext.as_slice())
            .map(Self::message_event)
            .unwrap_or_default()
    }

    fn message_event(msg: pb::ServerMessage) -> Vec<ClientEvent> {
        match msg.msg {
            None | Some(pb::server_message::Msg::Welcome(_)) => vec![],
            Some(_) => vec![ClientEvent::Message(msg)],
        }
    }
}

#[cfg(test)]
mod tests;
