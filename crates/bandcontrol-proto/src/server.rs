//! Sans-IO server side of the session protocol.
//!
//! The transport reports connections, disconnections and received packets, and executes the returned
//! [`ServerEvent`]s. Broadcast-only transports (`BlueZ`) may deliver [`Target::Client`] packets to every client, the
//! protocol is designed for that.

use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use prost::Message as _;

use crate::{
    PROTOCOL_VERSION,
    crypto::{AuthKey, BroadcastSealer, KEY_LEN, NONCE_LEN, Opener, Sealer},
    framing::{CHANNEL_BROADCAST, CHANNEL_HANDSHAKE, MAX_SESSION_CHANNEL, MIN_PACKET, Reassembler, fragment},
    pairing::PairingSecret,
    pb,
    random::{RandomSource, array},
};

/// Transport level connection id.
pub type ClientId = u64;
/// Authenticated session id, also the unicast channel.
pub type SessionId = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    All,
    Client(ClientId),
}

#[derive(Debug, PartialEq)]
pub enum ServerEvent {
    Send { target: Target, packets: Vec<Vec<u8>> },
    Disconnect(ClientId),
    Joined(SessionId),
    Left(SessionId),
    Message(SessionId, pb::ClientMessage),
}

/// Longest client name kept, in characters.
const MAX_CLIENT_NAME: usize = 64;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub name: String,
    pub max_sessions: usize,
    pub handshake_timeout: Duration,
    /// Largest packet the transport can carry.
    pub max_packet: usize,
    /// Failed authentications tolerated per `failure_window` before new handshakes are refused.
    pub max_failures: usize,
    pub failure_window: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: "REAPER".into(),
            max_sessions: 16,
            handshake_timeout: Duration::from_secs(10),
            max_packet: 512,
            max_failures: 10,
            failure_window: Duration::from_secs(60),
        }
    }
}

#[derive(Debug)]
enum Phase {
    Connected,
    Challenged {
        client_nonce: Vec<u8>,
        server_nonce: [u8; NONCE_LEN],
        session: SessionId,
    },
    Ready {
        session: SessionId,
        sealer: Sealer,
        opener: Opener,
    },
}

impl Phase {
    fn session(&self) -> Option<SessionId> {
        match self {
            Self::Connected => None,
            Self::Challenged { session, .. } | Self::Ready { session, .. } => Some(*session),
        }
    }
}

/// An authenticated client, as shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub client: ClientId,
    pub session: SessionId,
    /// What the phone calls itself (its model), may be empty.
    pub name: String,
}

#[derive(Debug)]
struct Client {
    connected_at: Instant,
    name: String,
    reassembler: Reassembler,
    max_packet: Option<usize>,
    phase: Phase,
}

pub struct ServerSessions {
    config: ServerConfig,
    auth_key: AuthKey,
    rng: Box<dyn RandomSource>,
    group_key: [u8; KEY_LEN],
    group: BroadcastSealer,
    clients: HashMap<ClientId, Client>,
    failures: VecDeque<Instant>,
}

impl std::fmt::Debug for ServerSessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerSessions")
            .field("config", &self.config)
            .field("clients", &self.clients)
            .finish_non_exhaustive()
    }
}

impl ServerSessions {
    #[must_use]
    pub fn new(secret: &PairingSecret, config: ServerConfig, mut rng: Box<dyn RandomSource>) -> Self {
        let group_key = array(rng.as_mut());
        Self {
            config,
            auth_key: AuthKey::derive(secret),
            rng,
            group_key,
            group: BroadcastSealer::new(&group_key),
            clients: HashMap::new(),
            failures: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Number of authenticated sessions.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.clients
            .values()
            .filter(|c| matches!(c.phase, Phase::Ready { .. }))
            .count()
    }

    /// Authenticated clients, ordered by session id.
    #[must_use]
    pub fn peers(&self) -> Vec<Peer> {
        let mut peers: Vec<Peer> = self
            .clients
            .iter()
            .filter_map(|(id, c)| match c.phase {
                Phase::Ready { session, .. } => Some(Peer {
                    client: *id,
                    session,
                    name: c.name.clone(),
                }),
                _ => None,
            })
            .collect();
        peers.sort_by_key(|p| p.session);
        peers
    }

    /// Largest packet every known client can receive (broadcast safe).
    #[must_use]
    pub fn packet_size(&self) -> usize {
        self.clients
            .values()
            .filter_map(|c| c.max_packet)
            .chain(std::iter::once(self.config.max_packet))
            .min()
            .unwrap_or(self.config.max_packet)
            .max(MIN_PACKET)
    }

    pub fn connect(&mut self, client: ClientId, now: Instant) {
        self.clients.entry(client).or_insert_with(|| Client {
            connected_at: now,
            name: String::new(),
            reassembler: Reassembler::default(),
            max_packet: None,
            phase: Phase::Connected,
        });
    }

    pub fn disconnect(&mut self, client: ClientId) -> Vec<ServerEvent> {
        match self.clients.remove(&client) {
            Some(Client {
                phase: Phase::Ready { session, .. },
                ..
            }) => vec![ServerEvent::Left(session)],
            _ => vec![],
        }
    }

    /// Handles a packet written by `client`.
    pub fn receive(&mut self, client: ClientId, packet: &[u8], now: Instant) -> Vec<ServerEvent> {
        self.connect(client, now);
        let Some(c) = self.clients.get_mut(&client) else {
            return vec![];
        };
        let (channel, message) = match c.reassembler.push(packet) {
            Ok(Some(m)) => m,
            Ok(None) => return vec![],
            Err(_) => return self.drop_client(client),
        };

        if channel == CHANNEL_HANDSHAKE {
            return self.handle_handshake(client, &message, now);
        }

        let Some(Client {
            phase: Phase::Ready { session, opener, .. },
            ..
        }) = self.clients.get_mut(&client)
        else {
            return self.drop_client(client);
        };
        let session = *session;
        if channel != session {
            return self.drop_client(client);
        }
        let Ok(plaintext) = opener.open(&message) else {
            return self.drop_client(client);
        };
        match pb::ClientMessage::decode(plaintext.as_slice()) {
            Ok(msg) if msg.msg.is_some() => vec![ServerEvent::Message(session, msg)],
            // Unknown (newer) command or garbage inside an authentic frame: ignore.
            _ => vec![],
        }
    }

    /// Encrypts `msg` with the group key for every authenticated client.
    pub fn broadcast(&mut self, msg: &pb::ServerMessage) -> Vec<ServerEvent> {
        if self.session_count() == 0 {
            return vec![];
        }
        let Ok(frame) = self.group.seal(&msg.encode_to_vec()) else {
            return vec![];
        };
        vec![ServerEvent::Send {
            target: Target::All,
            packets: fragment(CHANNEL_BROADCAST, &frame, self.packet_size()),
        }]
    }

    /// Encrypts `msg` for a single session.
    pub fn unicast(&mut self, session: SessionId, msg: &pb::ServerMessage) -> Vec<ServerEvent> {
        let packet_size = self.packet_size();
        let Some((&id, sealer)) = self.clients.iter_mut().find_map(|(id, c)| match &mut c.phase {
            Phase::Ready { session: s, sealer, .. } if *s == session => Some((id, sealer)),
            _ => None,
        }) else {
            return vec![];
        };
        let Ok(frame) = sealer.seal(&msg.encode_to_vec()) else {
            return vec![ServerEvent::Disconnect(id)];
        };
        vec![ServerEvent::Send {
            target: Target::Client(id),
            packets: fragment(session, &frame, packet_size),
        }]
    }

    /// Ends a client's session because the user disconnected it, telling the client first.
    pub fn goodbye(&mut self, client: ClientId) -> Vec<ServerEvent> {
        let session = self.clients.get(&client).and_then(|c| match c.phase {
            Phase::Ready { session, .. } => Some(session),
            _ => None,
        });
        let goodbye = pb::ServerMessage {
            msg: Some(pb::server_message::Msg::Goodbye(pb::Goodbye {})),
        };
        let mut events = session.map(|s| self.unicast(s, &goodbye)).unwrap_or_default();
        events.extend(self.drop_client(client));
        events
    }

    /// Expires handshakes that take too long.
    pub fn tick(&mut self, now: Instant) -> Vec<ServerEvent> {
        while self
            .failures
            .front()
            .is_some_and(|t| now.duration_since(*t) > self.config.failure_window)
        {
            self.failures.pop_front();
        }

        let expired: Vec<ClientId> = self
            .clients
            .iter()
            .filter(|(_, c)| {
                !matches!(c.phase, Phase::Ready { .. })
                    && now.duration_since(c.connected_at) > self.config.handshake_timeout
            })
            .map(|(id, _)| *id)
            .collect();
        expired.into_iter().flat_map(|id| self.drop_client(id)).collect()
    }

    fn drop_client(&mut self, client: ClientId) -> Vec<ServerEvent> {
        let mut events = self.disconnect(client);
        events.push(ServerEvent::Disconnect(client));
        events
    }

    fn handshake_reply(&self, client: ClientId, msg: pb::handshake::Msg) -> ServerEvent {
        let bytes = pb::Handshake { msg: Some(msg) }.encode_to_vec();
        ServerEvent::Send {
            target: Target::Client(client),
            packets: fragment(CHANNEL_HANDSHAKE, &bytes, self.packet_size()),
        }
    }

    fn reject(&mut self, client: ClientId, client_nonce: Vec<u8>, reason: pb::AuthFailure) -> Vec<ServerEvent> {
        let reply = self.handshake_reply(
            client,
            pb::handshake::Msg::Failed(pb::AuthFailed {
                client_nonce,
                reason: reason.into(),
            }),
        );
        let mut events = vec![reply];
        events.extend(self.drop_client(client));
        events
    }

    fn free_session_id(&self) -> Option<SessionId> {
        (1..=MAX_SESSION_CHANNEL).find(|id| !self.clients.values().any(|c| c.phase.session() == Some(*id)))
    }

    fn handle_handshake(&mut self, client: ClientId, message: &[u8], now: Instant) -> Vec<ServerEvent> {
        let Ok(pb::Handshake { msg: Some(msg) }) = pb::Handshake::decode(message) else {
            return self.drop_client(client);
        };
        let phase = self.clients.get(&client).map(|c| &c.phase);
        match (msg, phase) {
            (pb::handshake::Msg::Hello(hello), Some(Phase::Connected)) => self.on_hello(client, hello),
            // A transport that reuses ids (BLE device address) may miss a disconnect: treat it as a reconnect.
            (pb::handshake::Msg::Hello(hello), Some(_)) => {
                let mut events = self.disconnect(client);
                self.connect(client, now);
                events.extend(self.on_hello(client, hello));
                events
            }
            (pb::handshake::Msg::Auth(auth), Some(Phase::Challenged { .. })) => self.on_auth(client, &auth, now),
            _ => self.drop_client(client),
        }
    }

    fn on_hello(&mut self, client: ClientId, hello: pb::Hello) -> Vec<ServerEvent> {
        if hello.client_nonce.len() != NONCE_LEN {
            return self.drop_client(client);
        }
        if hello.version != PROTOCOL_VERSION {
            return self.reject(client, hello.client_nonce, pb::AuthFailure::UnsupportedVersion);
        }
        if self.failures.len() >= self.config.max_failures {
            return self.reject(client, hello.client_nonce, pb::AuthFailure::RateLimited);
        }
        let in_use = self.clients.values().filter(|c| c.phase.session().is_some()).count();
        let session = match self.free_session_id() {
            Some(id) if in_use < self.config.max_sessions => id,
            _ => return self.reject(client, hello.client_nonce, pb::AuthFailure::TooManyClients),
        };

        let server_nonce: [u8; NONCE_LEN] = array(self.rng.as_mut());
        let server_proof = self.auth_key.server_proof(&hello.client_nonce, &server_nonce, session);
        let max_packet = usize::try_from(hello.max_packet)
            .unwrap_or(usize::MAX)
            .clamp(MIN_PACKET, self.config.max_packet);
        if let Some(c) = self.clients.get_mut(&client) {
            c.max_packet = Some(max_packet);
            c.name = hello.client_name.chars().take(MAX_CLIENT_NAME).collect();
            c.phase = Phase::Challenged {
                client_nonce: hello.client_nonce.clone(),
                server_nonce,
                session,
            };
        }

        vec![self.handshake_reply(
            client,
            pb::handshake::Msg::Challenge(pb::Challenge {
                client_nonce: hello.client_nonce,
                session_id: session.into(),
                server_nonce: server_nonce.to_vec(),
                server_proof: server_proof.to_vec(),
                server_name: self.config.name.clone(),
            }),
        )]
    }

    fn on_auth(&mut self, client: ClientId, auth: &pb::Auth, now: Instant) -> Vec<ServerEvent> {
        let Some(Client {
            phase:
                Phase::Challenged {
                    client_nonce,
                    server_nonce,
                    session,
                },
            ..
        }) = self.clients.get(&client)
        else {
            return self.drop_client(client);
        };
        let (client_nonce, server_nonce, session) = (client_nonce.clone(), *server_nonce, *session);

        if !self
            .auth_key
            .verify_client_proof(&client_nonce, &server_nonce, session, &auth.client_proof)
        {
            self.failures.push_back(now);
            return self.reject(client, client_nonce, pb::AuthFailure::BadProof);
        }

        let (s2c, c2s) = self.auth_key.session_keys(&client_nonce, &server_nonce);
        if let Some(c) = self.clients.get_mut(&client) {
            c.phase = Phase::Ready {
                session,
                sealer: Sealer::new(&s2c),
                opener: Opener::new(&c2s),
            };
        }

        let welcome = pb::ServerMessage {
            msg: Some(pb::server_message::Msg::Welcome(pb::Welcome {
                group_key: self.group_key.to_vec(),
            })),
        };
        let mut events = self.unicast(session, &welcome);
        events.push(ServerEvent::Joined(session));
        events
    }
}
