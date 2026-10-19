//! Golden vectors shared with the Android implementation: `proto/test-vectors.json`.
//!
//! The file is checked against the current implementation; regenerate it with
//! `UPDATE_VECTORS=1 cargo test -p bandcontrol-proto --test vectors`.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::too_many_lines)]

use std::time::Instant;

use bandcontrol_proto::{
    client::{ClientEvent, ClientSession},
    crypto::{self, AuthKey, BroadcastSealer, Sealer},
    framing::fragment,
    pairing::{PairingInfo, PairingSecret},
    pb,
    random::CountingRandom,
    server::{ServerConfig, ServerEvent, ServerSessions, Target},
};
use pretty_assertions::assert_eq;
use prost::Message as _;
use serde_json::{Value, json};

fn h(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

fn hs(packets: &[Vec<u8>]) -> Vec<String> {
    packets.iter().map(|p| h(p)).collect()
}

fn secret() -> PairingSecret {
    PairingSecret::from_bytes(core::array::from_fn(|i| u8::try_from((i * 17 + 3) % 256).unwrap()))
}

fn pairing() -> Value {
    let cases = [
        (PairingSecret::from_bytes([0; 16]), "REAPER"),
        (secret(), "Studio PC"),
        (PairingSecret::from_bytes([0xff; 16]), "Кухня & co/✓"),
    ];
    cases
        .into_iter()
        .map(|(secret, name)| {
            let info = PairingInfo {
                secret: secret.clone(),
                name: name.into(),
            };
            json!({
                "secret": h(secret.as_bytes()),
                "code": secret.to_code(),
                "display_code": secret.to_display_code(),
                "name": name,
                "url": info.to_url(),
                "service_uuid": uuid::Uuid::from_bytes(crypto::service_uuid(&secret)).to_string(),
            })
        })
        .collect()
}

fn proofs() -> Value {
    let key = AuthKey::derive(&secret());
    let (cn, sn, session) = ([0x11; 16], [0x22; 16], 5);
    let (s2c, c2s) = key.session_keys(&cn, &sn);
    json!({
        "secret": h(secret().as_bytes()),
        "client_nonce": h(&cn),
        "server_nonce": h(&sn),
        "session_id": session,
        "server_proof": h(&key.server_proof(&cn, &sn, session)),
        "client_proof": h(&key.client_proof(&cn, &sn, session)),
        "s2c": h(&s2c),
        "c2s": h(&c2s),
    })
}

fn aead() -> Value {
    let key = [0x42; 32];
    let mut sealer = Sealer::new(&key);
    let unicast: Vec<Value> = [&b""[..], b"hello", &[0xab; 300]]
        .into_iter()
        .map(|p| json!({"plaintext": h(p), "ciphertext": h(&sealer.seal(p).unwrap())}))
        .collect();

    let mut sealer = BroadcastSealer::new(&key);
    let broadcast: Vec<Value> = (0..130u32)
        .filter_map(|counter| {
            let plaintext = counter.to_le_bytes();
            let frame = sealer.seal(&plaintext).unwrap();
            [0, 1, 127, 128, 129]
                .contains(&counter)
                .then(|| json!({"counter": counter, "plaintext": h(&plaintext), "frame": h(&frame)}))
        })
        .collect();
    json!({"key": h(&key), "unicast": unicast, "broadcast": broadcast})
}

fn framing() -> Value {
    let message: Vec<u8> = (0..=255).collect();
    [(1, 0, 20), (3, 19, 20), (126, 256, 100), (0, 38, 20), (127, 255, 512)]
        .into_iter()
        .map(|(channel, len, max_packet)| {
            let m = &message[..len];
            json!({
                "channel": channel,
                "max_packet": max_packet,
                "message": h(m),
                "packets": hs(&fragment(channel, m, max_packet)),
            })
        })
        .collect()
}

/// Peak levels: linear amplitude to byte, and byte to dBFS.
fn meters() -> Value {
    let encode: Vec<Value> = [0.0_f32, 0.0005, 0.001, 0.5, 1.0, 1.5, 2.0, 1e9]
        .into_iter()
        .map(|peak| json!({"peak": peak, "byte": bandcontrol_proto::meters::encode(peak)}))
        .collect();
    let decode: Vec<Value> = [0_u8, 1, 2, 120, 121, 133, 255]
        .into_iter()
        .map(|byte| json!({"byte": byte, "db": bandcontrol_proto::meters::decode(byte)}))
        .collect();
    json!({"encode": encode, "decode": decode})
}

fn sample_state() -> pb::ServerMessage {
    pb::ServerMessage {
        msg: Some(pb::server_message::Msg::State(pb::State {
            tracks: vec![
                pb::Track {
                    id: 2,
                    number: 2,
                    name: "Click".into(),
                    color: Some(0x00fb_02ff),
                    volume: 1.0,
                    output: false,
                    receives: vec![],
                    parent: None,
                },
                pb::Track {
                    id: 13,
                    number: 13,
                    name: "Drums [3+4]".into(),
                    color: None,
                    volume: 0.42,
                    output: true,
                    receives: vec![
                        pb::Receive {
                            src: 2,
                            volume: 0.99,
                            mute: false,
                        },
                        pb::Receive {
                            src: 5,
                            volume: 0.0,
                            mute: true,
                        },
                    ],
                    parent: Some(11),
                },
            ],
            regions: vec![pb::Region {
                id: 1,
                name: "Summer".into(),
                start_ms: 0,
                end_ms: 250_435,
                color: Some(0x8b_8727),
            }],
            markers: vec![pb::Marker {
                id: 3,
                name: "Chrous".into(),
                pos_ms: 83_478,
                color: None,
            }],
            region_order: vec![
                pb::RegionOrder { id: 1, disabled: false },
                pb::RegionOrder { id: 4, disabled: true },
            ],
        })),
    }
}

/// A recorded session: the Kotlin client, given the same client nonce, must answer the server packets with exactly
/// the recorded client packets and decode the recorded messages.
fn session() -> Value {
    let max_packet = 40;
    let now = Instant::now();
    let mut server = ServerSessions::new(
        &secret(),
        ServerConfig {
            name: "Studio ✓".into(),
            max_packet,
            ..ServerConfig::default()
        },
        Box::new(CountingRandom::starting_at(100)),
    );
    let mut rng = CountingRandom::starting_at(1);
    let mut client = ClientSession::new(&secret(), "Phone", max_packet, &mut rng);
    let client_nonce: Vec<u8> = (1..=16).collect();
    server.connect(1, now);

    let mut steps = vec![];
    let to_client = |events: Vec<ServerEvent>| -> Vec<Vec<u8>> {
        events
            .into_iter()
            .filter_map(|e| match e {
                ServerEvent::Send {
                    target: Target::All | Target::Client(1),
                    packets,
                } => Some(packets),
                _ => None,
            })
            .flatten()
            .collect()
    };

    let mut client_packets = client.hello();
    let mut expected_messages: Vec<Value> = vec![];
    // Handshake: client packets go to the server, its answers go back to the client.
    while !client_packets.is_empty() {
        let mut server_packets = vec![];
        for p in &client_packets {
            server_packets.extend(to_client(server.receive(1, p, now)));
        }
        let mut answer = vec![];
        let mut events = vec![];
        for p in &server_packets {
            for e in client.receive(p).unwrap() {
                match e {
                    ClientEvent::Send(packets) => answer.extend(packets),
                    ClientEvent::Authenticated { session, server_name } => {
                        events.push(json!({"authenticated": {"session": session, "server_name": server_name}}));
                    }
                    ClientEvent::Message(m) => events.push(json!({"message": h(&m.encode_to_vec())})),
                }
            }
        }
        steps.push(json!({"client": hs(&client_packets), "server": hs(&server_packets), "events": events}));
        client_packets = answer;
    }
    assert!(client.is_ready());
    let session = client.session().unwrap();

    // Server messages after authentication.
    let gains = pb::ServerMessage {
        msg: Some(pb::server_message::Msg::Gains(pb::GainDelta {
            tracks: vec![pb::TrackGain { track: 13, volume: 0.5 }],
            receives: vec![pb::ReceiveGain {
                track: 13,
                index: 1,
                volume: 0.25,
                mute: false,
            }],
        })),
    };
    let transport = pb::ServerMessage {
        msg: Some(pb::server_message::Msg::Transport(pb::Transport {
            state: pb::PlayState::Playing.into(),
            pos_ms: 367_304,
            repeat: true,
            recording: false,
        })),
    };
    let meters = pb::ServerMessage {
        msg: Some(pb::server_message::Msg::Meters(pb::Meters {
            subscription: 7,
            peaks: vec![0, 1, 121, 255],
        })),
    };
    let mut server_packets = to_client(server.broadcast(&sample_state()));
    server_packets.extend(to_client(server.unicast(session, &transport)));
    server_packets.extend(to_client(server.broadcast(&gains)));
    server_packets.extend(to_client(server.unicast(session, &meters)));
    for p in &server_packets {
        for e in client.receive(p).unwrap() {
            if let ClientEvent::Message(m) = e {
                expected_messages.push(json!(h(&m.encode_to_vec())));
            }
        }
    }
    assert_eq!(expected_messages.len(), 4);

    // Client commands.
    let commands: Vec<Value> = [
        pb::client_message::Msg::SetTrackVolume(pb::SetTrackVolume {
            track: 13,
            volume: 0.75,
        }),
        pb::client_message::Msg::Transport(pb::TransportAction::Play.into()),
        pb::client_message::Msg::SetReceive(pb::SetReceive {
            track: 13,
            index: 1,
            src: 5,
            volume: None,
            mute: Some(true),
        }),
        pb::client_message::Msg::Goto(pb::Goto {
            region: 3,
            marker: Some(6),
        }),
        pb::client_message::Msg::SubscribeMeters(pb::SubscribeMeters {
            subscription: 7,
            tracks: vec![7, 8, 12],
        }),
    ]
    .into_iter()
    .map(|msg| {
        let msg = pb::ClientMessage { msg: Some(msg) };
        let packets = client.send(&msg).unwrap();
        let mut received = vec![];
        for p in &packets {
            for e in server.receive(1, p, now) {
                if let ServerEvent::Message(_, m) = e {
                    received.push(m);
                }
            }
        }
        assert_eq!(received, vec![msg.clone()]);
        json!({"message": h(&msg.encode_to_vec()), "packets": hs(&packets)})
    })
    .collect();

    json!({
        "secret": h(secret().as_bytes()),
        "client_name": "Phone",
        "client_nonce": h(&client_nonce),
        "max_packet": max_packet,
        "handshake": steps,
        "server_packets": hs(&server_packets),
        "messages": expected_messages,
        "commands": commands,
    })
}

#[test]
fn vectors_are_up_to_date() {
    let vectors = json!({
        "pairing": pairing(),
        "proofs": proofs(),
        "aead": aead(),
        "framing": framing(),
        "session": session(),
        "meters": meters(),
    });
    let text = serde_json::to_string_pretty(&vectors).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proto/test-vectors.json");
    if std::env::var_os("UPDATE_VECTORS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == text,
        "{} is outdated, run with UPDATE_VECTORS=1",
        path.display()
    );
}
