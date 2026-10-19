//! Server + TCP transport + engine + fake DAW, driven like REAPER drives the extension.

#![allow(clippy::float_cmp, clippy::unwrap_used)]

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use bandcontrol_core::{
    engine::{Engine, EngineConfig},
    fake::FakeDaw,
};
use bandcontrol_proto::{pairing::PairingSecret, pb};
use bandcontrol_server::{
    Server, ServerOptions, TransportStatus,
    client::{Mirror, TcpClient, TcpClientError},
};
use parking_lot::Mutex;
use pretty_assertions::assert_eq;

const TIMEOUT: Duration = Duration::from_secs(5);

struct Host {
    addr: SocketAddr,
    daw: Arc<Mutex<FakeDaw>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}

fn secret() -> PairingSecret {
    PairingSecret::from_bytes([42; 16])
}

/// Runs the engine on its own thread at 30 Hz, like REAPER's timer.
fn host() -> Host {
    let mut server = Server::start(ServerOptions {
        secret: secret(),
        name: "Test REAPER".into(),
        ble: false,
        tcp: Some("127.0.0.1:0".parse().unwrap()),
    })
    .unwrap();
    let addr = loop {
        if let TransportStatus::Running(a) = server.status().tcp {
            break a.parse().unwrap();
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let daw = Arc::new(Mutex::new(FakeDaw::mother_project()));
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let daw = daw.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut engine = Engine::new(EngineConfig::default());
            let mut last = Instant::now();
            while !stop.load(Ordering::SeqCst) {
                let now = Instant::now();
                let mut daw = daw.lock();
                daw.advance(now - last);
                last = now;
                let outputs = engine.tick(&mut *daw, now, server.poll_inputs());
                drop(daw);
                server.send(outputs);
                std::thread::sleep(Duration::from_millis(33));
            }
        })
    };
    Host {
        addr,
        daw,
        stop,
        thread: Some(thread),
    }
}

async fn connected(host: &Host, max_packet: usize) -> (TcpClient, Mirror) {
    let mut client = TcpClient::connect(host.addr, &secret(), max_packet).await.unwrap();
    let mut mirror = Mirror::default();
    client
        .wait_for(&mut mirror, TIMEOUT, |m| m.state.is_some() && m.transport.is_some())
        .await
        .unwrap();
    (client, mirror)
}

#[tokio::test]
async fn initial_state_commands_and_deltas() {
    let host = host();
    let (mut a, mut ma) = connected(&host, 512).await;
    let (mut b, mut mb) = connected(&host, 20).await;
    assert_eq!(a.server_name, "Test REAPER");
    assert_eq!(ma.state, mb.state);
    assert_eq!(ma.track("Guitar [7+8]").unwrap().volume, 0.9);

    a.send(pb::client_message::Msg::SetTrackVolume(pb::SetTrackVolume {
        track: 15,
        volume: 0.5,
    }))
    .await
    .unwrap();
    b.wait_for(&mut mb, TIMEOUT, |m| m.track("Guitar [7+8]").unwrap().volume == 0.5)
        .await
        .unwrap();
    a.wait_for(&mut ma, TIMEOUT, |m| m.track("Guitar [7+8]").unwrap().volume == 0.5)
        .await
        .unwrap();
    assert_eq!(host.daw.lock().track_mut(15).unwrap().volume, 0.5);

    // Engineer changes a send in the DAW.
    host.daw.lock().track_mut(13).unwrap().receives[0].mute = true;
    b.wait_for(&mut mb, TIMEOUT, |m| m.track("Drums [3+4]").unwrap().receives[0].mute)
        .await
        .unwrap();

    a.send(pb::client_message::Msg::Transport(pb::TransportAction::Play.into()))
        .await
        .unwrap();
    b.wait_for(&mut mb, TIMEOUT, |m| {
        m.transport
            .is_some_and(|t| t.state == i32::from(pb::PlayState::Playing) && t.pos_ms > 1000)
    })
    .await
    .unwrap();

    // Structure change arrives as a new full state.
    {
        let mut daw = host.daw.lock();
        daw.track_mut(2).unwrap().name = "Metronome".into();
        daw.touch();
    }
    b.wait_for(&mut mb, TIMEOUT, |m| m.track("Metronome").is_some())
        .await
        .unwrap();
    assert_eq!(mb.states_received, 2);
}

#[tokio::test]
async fn wrong_secret_is_rejected() {
    let host = host();
    let err = TcpClient::connect(host.addr, &PairingSecret::from_bytes([1; 16]), 512)
        .await
        .unwrap_err();
    assert!(matches!(err, TcpClientError::Session(_)), "{err:?}");
}
