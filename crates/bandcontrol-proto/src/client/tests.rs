use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use pretty_assertions::assert_eq;

use super::*;
use crate::{
    random::CountingRandom,
    server::{ClientId, ServerConfig, ServerEvent, ServerSessions, Target},
};

/// In-memory network: delivers server packets either per client (TCP, macOS, Windows) or to everybody (`BlueZ`).
struct Net {
    server: ServerSessions,
    clients: HashMap<ClientId, ClientSession>,
    broadcast_only: bool,
    now: Instant,
    joined: Vec<SessionId>,
    left: Vec<SessionId>,
    commands: Vec<(SessionId, pb::ClientMessage)>,
    events: HashMap<ClientId, Vec<ClientEvent>>,
    errors: HashMap<ClientId, ClientError>,
    disconnected: Vec<ClientId>,
}

impl Net {
    fn new(broadcast_only: bool, config: ServerConfig) -> Self {
        Self {
            server: ServerSessions::new(&secret(), config, Box::new(CountingRandom::starting_at(100))),
            clients: HashMap::new(),
            broadcast_only,
            now: Instant::now(),
            joined: vec![],
            left: vec![],
            commands: vec![],
            events: HashMap::new(),
            errors: HashMap::new(),
            disconnected: vec![],
        }
    }

    fn add_client(&mut self, id: ClientId, secret: &PairingSecret, max_packet: usize) -> Vec<Vec<u8>> {
        let mut rng = CountingRandom::starting_at(u8::try_from(id * 16).unwrap());
        let mut client = ClientSession::new(secret, format!("phone{id}"), max_packet, &mut rng);
        let hello = client.hello();
        self.clients.insert(id, client);
        self.server.connect(id, self.now);
        hello
    }

    fn client_writes(&mut self, id: ClientId, packets: Vec<Vec<u8>>) {
        let mut queue = VecDeque::new();
        for p in packets {
            queue.extend(self.server.receive(id, &p, self.now));
        }
        self.run(queue);
    }

    fn server_events(&mut self, events: Vec<ServerEvent>) {
        self.run(events.into());
    }

    fn run(&mut self, mut queue: VecDeque<ServerEvent>) {
        while let Some(event) = queue.pop_front() {
            match event {
                ServerEvent::Send { target, packets } => {
                    let recipients: Vec<ClientId> = match target {
                        Target::Client(id) if !self.broadcast_only => vec![id],
                        _ => self.clients.keys().copied().collect(),
                    };
                    for id in recipients {
                        for p in &packets {
                            let Some(client) = self.clients.get_mut(&id) else {
                                continue;
                            };
                            match client.receive(p) {
                                Ok(events) => {
                                    for e in events {
                                        if let ClientEvent::Send(out) = e {
                                            for o in out {
                                                queue.extend(self.server.receive(id, &o, self.now));
                                            }
                                        } else {
                                            self.events.entry(id).or_default().push(e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    self.errors.insert(id, e);
                                }
                            }
                        }
                    }
                }
                ServerEvent::Disconnect(id) => {
                    self.clients.remove(&id);
                    queue.extend(self.server.disconnect(id));
                    self.disconnected.push(id);
                }
                ServerEvent::Joined(s) => self.joined.push(s),
                ServerEvent::Left(s) => self.left.push(s),
                ServerEvent::Message(s, m) => self.commands.push((s, m)),
            }
        }
    }

    fn messages(&self, id: ClientId) -> Vec<pb::ServerMessage> {
        self.events
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|e| match e {
                ClientEvent::Message(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }
}

fn secret() -> PairingSecret {
    PairingSecret::from_bytes(*b"0123456789abcdef")
}

fn transport(pos_ms: u32) -> pb::ServerMessage {
    pb::ServerMessage {
        msg: Some(pb::server_message::Msg::Transport(pb::Transport {
            state: pb::PlayState::Playing.into(),
            pos_ms,
            repeat: false,
            recording: false,
        })),
    }
}

fn big_state() -> pb::ServerMessage {
    pb::ServerMessage {
        msg: Some(pb::server_message::Msg::State(pb::State {
            tracks: (0..20)
                .map(|i| pb::Track {
                    id: i,
                    number: i + 1,
                    name: format!("Track number {i}"),
                    color: Some(0x00ff_00ff),
                    volume: 0.5,
                    output: i > 15,
                    receives: vec![],
                })
                .collect(),
            ..Default::default()
        })),
    }
}

fn play() -> pb::ClientMessage {
    pb::ClientMessage {
        msg: Some(pb::client_message::Msg::Transport(pb::TransportAction::Play.into())),
    }
}

#[test]
fn handshake_then_messages_both_ways() {
    let mut net = Net::new(false, ServerConfig::default());
    let hello = net.add_client(1, &secret(), 512);
    net.client_writes(1, hello);

    assert_eq!(net.joined, vec![1]);
    assert_eq!(
        net.events[&1],
        vec![ClientEvent::Authenticated {
            session: 1,
            server_name: "REAPER".into()
        }]
    );

    let events = net.server.broadcast(&transport(1000));
    net.server_events(events);
    let events = net.server.unicast(1, &big_state());
    net.server_events(events);
    assert_eq!(net.messages(1), vec![transport(1000), big_state()]);

    let packets = net.clients.get_mut(&1).unwrap().send(&play()).unwrap();
    net.client_writes(1, packets);
    assert_eq!(net.commands, vec![(1, play())]);
}

#[test]
fn concurrent_handshakes_on_broadcast_transport_with_tiny_mtu() {
    let mut net = Net::new(true, ServerConfig::default());
    let hellos: Vec<_> = (1..=3).map(|id| (id, net.add_client(id, &secret(), 20))).collect();
    for (id, hello) in hellos {
        net.client_writes(id, hello);
    }
    net.joined.sort_unstable();
    assert_eq!(net.joined, vec![1, 2, 3]);
    assert_eq!(net.server.packet_size(), 20);

    let events = net.server.unicast(2, &big_state());
    net.server_events(events);
    let events = net.server.broadcast(&transport(5));
    net.server_events(events);

    assert_eq!(net.messages(1), vec![transport(5)]);
    assert_eq!(net.messages(2), vec![big_state(), transport(5)]);
    assert_eq!(net.messages(3), vec![transport(5)]);
    assert_eq!(net.errors, HashMap::new());
}

#[test]
fn wrong_secret_is_detected_by_client() {
    let mut net = Net::new(false, ServerConfig::default());
    let hello = net.add_client(1, &PairingSecret::from_bytes([0; 16]), 512);
    net.client_writes(1, hello);
    assert_eq!(net.errors[&1], ClientError::BadServerProof);
    assert_eq!(net.joined, Vec::<SessionId>::new());
}

#[test]
fn forged_client_proof_is_rejected_and_rate_limited() {
    let config = ServerConfig {
        max_failures: 2,
        ..ServerConfig::default()
    };
    let mut net = Net::new(false, config);
    for id in 1..=2 {
        let hello = net.add_client(id, &secret(), 512);
        // Attacker: answer the challenge with a bogus proof.
        let replies = net.server.receive(id, &hello[0], net.now);
        assert!(matches!(&replies[..], [ServerEvent::Send { .. }]));
        let forged = pb::Handshake {
            msg: Some(pb::handshake::Msg::Auth(pb::Auth {
                client_proof: vec![0; 32],
            })),
        };
        let packets = fragment(CHANNEL_HANDSHAKE, &forged.encode_to_vec(), 512);
        let events = net.server.receive(id, &packets[0], net.now);
        assert!(events.contains(&ServerEvent::Disconnect(id)), "{events:?}");
    }
    assert_eq!(net.joined, Vec::<SessionId>::new());

    // Even the right secret is refused until the failure window passes.
    let hello = net.add_client(4, &secret(), 512);
    net.client_writes(4, hello);
    assert_eq!(net.errors[&4], ClientError::AuthFailed(pb::AuthFailure::RateLimited));

    net.now += Duration::from_secs(61);
    let events = net.server.tick(net.now);
    net.server_events(events);
    let hello = net.add_client(5, &secret(), 512);
    net.client_writes(5, hello);
    assert_eq!(net.joined, vec![1]);
}

#[test]
fn handshake_timeout_disconnects() {
    let mut net = Net::new(false, ServerConfig::default());
    net.add_client(1, &secret(), 512);
    net.now += Duration::from_secs(11);
    let events = net.server.tick(net.now);
    assert_eq!(events, vec![ServerEvent::Disconnect(1)]);
}

#[test]
fn session_ids_are_reused_and_limited() {
    let config = ServerConfig {
        max_sessions: 2,
        ..ServerConfig::default()
    };
    let mut net = Net::new(false, config);
    for id in 1..=3 {
        let hello = net.add_client(id, &secret(), 512);
        net.client_writes(id, hello);
    }
    assert_eq!(net.joined, vec![1, 2]);
    assert_eq!(net.errors[&3], ClientError::AuthFailed(pb::AuthFailure::TooManyClients));

    let events = net.server.disconnect(1);
    net.server_events(events);
    assert_eq!(net.left, vec![1]);
    let hello = net.add_client(4, &secret(), 512);
    net.client_writes(4, hello);
    assert_eq!(net.joined, vec![1, 2, 1]);
}

#[test]
fn tampered_command_disconnects() {
    let mut net = Net::new(false, ServerConfig::default());
    let hello = net.add_client(1, &secret(), 512);
    net.client_writes(1, hello);
    let mut packets = net.clients.get_mut(&1).unwrap().send(&play()).unwrap();
    packets[0][3] ^= 0xff;
    net.client_writes(1, packets);
    assert_eq!(net.commands, vec![]);
    assert_eq!(net.disconnected, vec![1]);
    assert_eq!(net.left, vec![1]);
}

#[test]
fn broadcast_without_sessions_is_dropped() {
    let mut net = Net::new(false, ServerConfig::default());
    assert_eq!(net.server.broadcast(&transport(1)), vec![]);
}

#[test]
fn truncated_broadcast_is_ignored() {
    let mut net = Net::new(false, ServerConfig::default());
    let hello = net.add_client(1, &secret(), 20);
    net.client_writes(1, hello);
    let ServerEvent::Send { packets, .. } = net.server.broadcast(&big_state()).remove(0) else {
        panic!("expected send");
    };
    let client = net.clients.get_mut(&1).unwrap();
    for p in &packets[1..] {
        assert_eq!(client.receive(p).unwrap(), vec![]);
    }
    let ServerEvent::Send { packets, .. } = net.server.broadcast(&transport(7)).remove(0) else {
        panic!("expected send");
    };
    let client = net.clients.get_mut(&1).unwrap();
    let events: Vec<_> = packets.iter().flat_map(|p| client.receive(p).unwrap()).collect();
    assert_eq!(events, vec![ClientEvent::Message(transport(7))]);
}

#[test]
fn commands_before_auth_are_rejected() {
    let mut net = Net::new(false, ServerConfig::default());
    net.add_client(1, &secret(), 512);
    assert_eq!(
        net.clients.get_mut(&1).unwrap().send(&play()),
        Err(ClientError::NotReady)
    );
    let events = net.server.receive(1, &[1, 2, 3], net.now);
    assert_eq!(events, vec![ServerEvent::Disconnect(1)]);
}

#[test]
fn hello_on_existing_session_is_a_reconnect() {
    let mut net = Net::new(false, ServerConfig::default());
    let hello = net.add_client(1, &secret(), 512);
    net.client_writes(1, hello);
    // Same transport id, new client session (e.g. BLE device reconnected without us seeing the disconnect).
    let hello = net.add_client(1, &secret(), 512);
    net.client_writes(1, hello);
    assert_eq!(net.left, vec![1]);
    assert_eq!(net.joined, vec![1, 1]);
    assert_eq!(net.server.session_count(), 1);
}
