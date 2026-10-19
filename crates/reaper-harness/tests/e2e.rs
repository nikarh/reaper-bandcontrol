//! End-to-end: real headless REAPER with the extension, a protocol client over TCP, and the REAPER web
//! interface standing in for the sound engineer. Run with `mise run e2e:reaper`.

#![allow(clippy::unwrap_used, clippy::float_cmp, clippy::panic, clippy::too_many_lines)]

use std::{net::SocketAddr, time::Duration};

use bandcontrol_core::{convert, daw::Daw as _, fake::FakeDaw};
use bandcontrol_proto::{pairing::PairingSecret, pb};
use bandcontrol_server::client::{Mirror, TcpClient, TcpClientError};
use pretty_assertions::assert_eq;
use reaper_harness::{Options, Reaper, build_plugin, free_port, mother_project, wait_for_port, workspace_root};

const TIMEOUT: Duration = Duration::from_secs(5);

fn secret() -> PairingSecret {
    PairingSecret::from_bytes([7; 16])
}

struct Env {
    reaper: Reaper,
    tcp: SocketAddr,
}

fn start(name: &str) -> Env {
    let tcp = SocketAddr::from(([127, 0, 0, 1], free_port().unwrap()));
    let mut reaper = Reaper::launch(&Options {
        dir: workspace_root().join("target/reaper-e2e").join(name),
        project: mother_project(),
        plugin: build_plugin(false).unwrap(),
        env: vec![
            ("BANDCONTROL_TCP".into(), tcp.to_string()),
            ("BANDCONTROL_SECRET".into(), secret().to_code()),
            ("BANDCONTROL_BLE".into(), "0".into()),
            ("BANDCONTROL_NAME".into(), "E2E".into()),
            ("BANDCONTROL_LOG".into(), "debug".into()),
        ],
    })
    .unwrap();
    reaper.wait_ready(Duration::from_secs(60)).unwrap();
    wait_for_port(tcp, Duration::from_secs(10)).unwrap_or_else(|e| panic!("{e}\n{}", reaper.log()));
    Env { reaper, tcp }
}

async fn connect(env: &Env) -> (TcpClient, Mirror) {
    let mut client = TcpClient::connect(env.tcp, &secret(), 512).await.unwrap();
    let mut mirror = Mirror::default();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| m.state.is_some() && m.transport.is_some())
        .await
        .unwrap();
    (client, mirror)
}

fn expected_initial_state() -> pb::State {
    let mut fake = FakeDaw::mother_project();
    let structure = fake.structure().relevant();
    let mix = fake.mix(&structure);
    convert::state(&structure, &mix)
}

fn track(mirror: &Mirror, id: u32) -> &pb::Track {
    mirror
        .state
        .as_ref()
        .unwrap()
        .tracks
        .iter()
        .find(|t| t.id == id)
        .unwrap()
}

#[tokio::test]
#[ignore = "needs REAPER and cage, run with `mise run e2e:reaper`"]
async fn initial_state_and_phone_commands() {
    use pb::client_message::Msg;

    let env = start("commands");
    let (mut client, mut mirror) = connect(&env).await;
    assert_eq!(client.server_name, "E2E");
    assert_eq!(mirror.state.clone().unwrap(), expected_initial_state());
    assert_eq!(mirror.transport.unwrap().state(), pb::PlayState::Stopped);

    client
        .send(Msg::SetTrackVolume(pb::SetTrackVolume {
            track: 13,
            volume: 0.25,
        }))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| track(m, 13).volume == 0.25)
        .await
        .unwrap();
    assert!(env.reaper.web("TRACK/13").unwrap().contains("\t0.250000\t"));

    client
        .send(Msg::SetReceive(pb::SetReceive {
            track: 15,
            index: 1,
            src: 4,
            volume: Some(0.5),
            mute: Some(false),
        }))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| {
            let r = &track(m, 15).receives[1];
            r.volume == 0.5 && !r.mute
        })
        .await
        .unwrap();

    // A receive index that doesn't match the source is rejected instead of changing the wrong send.
    client
        .send(Msg::SetReceive(pb::SetReceive {
            track: 15,
            index: 1,
            src: 5,
            volume: Some(0.1),
            mute: None,
        }))
        .await
        .unwrap();

    client
        .send(Msg::Goto(pb::Goto {
            region: 3,
            marker: Some(6),
        }))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| m.transport.unwrap().pos_ms == 684_522)
        .await
        .unwrap();

    client
        .send(Msg::Transport(pb::TransportAction::Play.into()))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| {
            let t = m.transport.unwrap();
            t.state() == pb::PlayState::Playing && t.pos_ms > 685_000
        })
        .await
        .unwrap();

    client
        .send(Msg::Transport(pb::TransportAction::Stop.into()))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| {
            m.transport.unwrap().state() == pb::PlayState::Stopped
        })
        .await
        .unwrap();

    client
        .send(Msg::Transport(pb::TransportAction::ToggleRepeat.into()))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| m.transport.unwrap().repeat)
        .await
        .unwrap();

    let order = vec![
        pb::RegionOrder { id: 2, disabled: false },
        pb::RegionOrder { id: 1, disabled: true },
        pb::RegionOrder { id: 3, disabled: false },
        pb::RegionOrder { id: 4, disabled: false },
    ];
    client
        .send(Msg::SetRegionOrder(pb::SetRegionOrder { order: order.clone() }))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| {
            m.state.as_ref().unwrap().region_order == order
        })
        .await
        .unwrap();
    assert!(
        env.reaper
            .web("GET/PROJEXTSTATE/BANDUI/regions")
            .unwrap()
            .contains("12,01,13,14")
    );

    // The rejected receive change must not have been applied anywhere.
    assert_eq!(track(&mirror, 15).receives[1].volume, 0.5);
    assert!(env.reaper.log().contains("command failed"), "{}", env.reaper.log());
}

#[tokio::test]
#[ignore = "needs REAPER and cage, run with `mise run e2e:reaper`"]
async fn engineer_changes_reach_phones() {
    let env = start("engineer");
    let (mut client, mut mirror) = connect(&env).await;
    let states = mirror.states_received;

    env.reaper.web("SET/TRACK/14/VOL/0.5").unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| track(m, 14).volume == 0.5)
        .await
        .unwrap();

    env.reaper.web("SET/TRACK/16/SEND/-3/MUTE/-1").unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| track(m, 16).receives[2].mute)
        .await
        .unwrap();
    assert_eq!(mirror.states_received, states, "gain changes must be sent as deltas");

    // Seek while stopped.
    env.reaper.web("SET/POS/100").unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| m.transport.unwrap().pos_ms == 100_000)
        .await
        .unwrap();

    // Insert marker at the edit cursor: a structure change, sent as a new state.
    env.reaper.action(40157).unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| {
            m.state.as_ref().unwrap().markers.iter().any(|mk| mk.pos_ms == 100_000)
        })
        .await
        .unwrap();
    assert_eq!(mirror.states_received, states + 1);

    // Region order edited elsewhere (e.g. by the old web UI), not tracked by the project change counter.
    env.reaper.web("SET/PROJEXTSTATE/BANDUI/regions/14,13,12,11").unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| {
            m.state.as_ref().unwrap().region_order.first().map(|r| r.id) == Some(4)
        })
        .await
        .unwrap();

    // Wrong secret is rejected.
    let wrong = PairingSecret::from_bytes([8; 16]);
    let result = TcpClient::connect(env.tcp, &wrong, 512).await;
    assert!(
        matches!(result, Err(TcpClientError::Session(_) | TcpClientError::Io(_))),
        "{result:?}"
    );
}
