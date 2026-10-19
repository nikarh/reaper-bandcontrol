//! End-to-end: real headless REAPER with the extension, a protocol client over TCP, and the REAPER web
//! interface standing in for the sound engineer. Run with `mise run e2e:reaper`.

#![allow(clippy::unwrap_used, clippy::float_cmp, clippy::panic, clippy::too_many_lines)]

use std::{net::SocketAddr, time::Duration};

use bandcontrol_core::{convert, daw::Daw as _, fake::FakeDaw, rpp};
use bandcontrol_proto::{pairing::PairingSecret, pb};
use bandcontrol_server::client::{Mirror, TcpClient, TcpClientError};
use pretty_assertions::assert_eq;
use reaper_harness::{
    Options, Reaper, band_project, build_plugin, free_port, mother_project, wait_for_port, workspace_root,
};

const TIMEOUT: Duration = Duration::from_secs(5);

fn secret() -> PairingSecret {
    PairingSecret::from_bytes([7; 16])
}

struct Env {
    reaper: Reaper,
    tcp: SocketAddr,
}

fn start(name: &str) -> Env {
    start_project(name, mother_project())
}

fn start_project(name: &str, project: std::path::PathBuf) -> Env {
    let tcp = SocketAddr::from(([127, 0, 0, 1], free_port().unwrap()));
    let mut reaper = Reaper::launch(&Options {
        dir: workspace_root().join("target/reaper-e2e").join(name),
        project,
        plugin: build_plugin(false).unwrap(),
        env: vec![
            ("BANDCONTROL_TCP".into(), tcp.to_string()),
            ("BANDCONTROL_SECRET".into(), secret().to_code()),
            ("BANDCONTROL_BLE".into(), "0".into()),
            ("BANDCONTROL_NAME".into(), "E2E".into()),
            ("BANDCONTROL_LOG".into(), "debug".into()),
        ],
        web_port: None,
        ini: Vec::new(),
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

    // The click track plays a click source, a silent input track reads as silence.
    client
        .send(Msg::SubscribeMeters(pb::SubscribeMeters {
            subscription: 3,
            tracks: vec![2, 7],
        }))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| {
            m.meters
                .as_ref()
                .is_some_and(|f| f.subscription == 3 && f.peaks[0] > 0 && f.peaks[1] == 0)
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

/// Colors read from project files are rounded slightly differently from what REAPER reports.
fn without_colors(mut state: pb::State) -> (pb::State, Vec<Option<u32>>) {
    let mut colors = vec![];
    for t in &mut state.tracks {
        colors.push(t.color.take());
    }
    for r in &mut state.regions {
        colors.push(r.color.take());
    }
    for m in &mut state.markers {
        colors.push(m.color.take());
    }
    (state, colors)
}

fn close_colors(a: &[Option<u32>], b: &[Option<u32>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| match (a, b) {
            (Some(a), Some(b)) => (0..3).all(|i| ((a >> (i * 8)) & 0xff).abs_diff((b >> (i * 8)) & 0xff) <= 1),
            (a, b) => a == b,
        })
}

#[tokio::test]
#[ignore = "needs REAPER and cage, run with `mise run e2e:reaper`"]
async fn band_project_in_reaper() {
    use pb::client_message::Msg;

    let env = start_project("band", band_project());
    let (mut client, mut mirror) = connect(&env).await;

    // Everything the extension reads from REAPER matches what the project file says.
    let mut parsed = rpp::parse(&std::fs::read_to_string(band_project()).unwrap()).unwrap();
    let structure = parsed.structure().relevant();
    let mix = parsed.mix(&structure);
    let (expected, expected_colors) = without_colors(convert::state(&structure, &mix));
    let (actual, actual_colors) = without_colors(mirror.state.clone().unwrap());
    assert_eq!(actual, expected);
    assert!(
        close_colors(&actual_colors, &expected_colors),
        "{actual_colors:?}\n{expected_colors:?}"
    );

    // While playing, the click track and the drums IEM mix (which receives it) have levels, inputs are silent.
    client
        .send(Msg::Goto(pb::Goto {
            region: 2,
            marker: None,
        }))
        .await
        .unwrap();
    client
        .send(Msg::Transport(pb::TransportAction::Play.into()))
        .await
        .unwrap();
    client
        .send(Msg::SubscribeMeters(pb::SubscribeMeters {
            subscription: 1,
            tracks: vec![1, 27, 6],
        }))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| {
            m.meters
                .as_ref()
                .is_some_and(|f| f.peaks[0] > 0 && f.peaks[1] > 0 && f.peaks[2] == 0)
        })
        .await
        .unwrap();

    // Input gain from the phone.
    client
        .send(Msg::SetTrackVolume(pb::SetTrackVolume { track: 6, volume: 0.5 }))
        .await
        .unwrap();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| track(m, 6).volume == 0.5)
        .await
        .unwrap();
    assert!(env.reaper.web("TRACK/6").unwrap().contains("\t0.500000\t"));
}

/// The QR code on screen and the side length of its finder pattern square, in pixels.
fn find_qr(reaper: &Reaper) -> Option<(String, i32)> {
    let image = reaper.screenshot_gray().unwrap();
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(image.width, image.height, |x, y| {
        image.pixels.get(y * image.width + x).copied().unwrap_or(0)
    });
    prepared.detect_grids().into_iter().find_map(|grid| {
        let (_, text) = grid.decode().ok()?;
        let [a, b, ..] = grid.bounds;
        Some((text, (b.x - a.x).abs().max((b.y - a.y).abs())))
    })
}

fn eventually_qr(reaper: &Reaper, longer_than: i32) -> (String, i32) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(found) = find_qr(reaper).filter(|(_, size)| *size > longer_than) {
            return found;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no QR code larger than {longer_than} px on screen"
        );
        std::thread::sleep(Duration::from_millis(300));
    }
}

#[test]
#[ignore = "needs REAPER and cage, run with `mise run e2e:reaper`"]
fn pairing_window_shows_the_code_and_scales() {
    let env = start("pairing-window");
    env.reaper.web("_BANDCONTROL_SHOW_PAIRING").unwrap();
    let expected = bandcontrol_proto::pairing::PairingInfo {
        secret: secret(),
        name: "E2E".into(),
    }
    .to_url();
    let (url, size) = eventually_qr(&env.reaper, 0);
    assert_eq!(url, expected);

    // Running the action again keeps the one window.
    env.reaper.web("_BANDCONTROL_SHOW_PAIRING").unwrap();
    assert_eq!(eventually_qr(&env.reaper, 0).0, expected);

    if env.reaper.can_resize_windows() {
        env.reaper.resize_window("^BandControl pairing$", 900, 1050).unwrap();
        let (url, larger) = eventually_qr(&env.reaper, size * 3 / 2);
        assert_eq!(url, expected);
        assert!(larger > size, "{larger} > {size}");
    }
    assert!(!env.reaper.log().contains("pairing window is not available"));
}
