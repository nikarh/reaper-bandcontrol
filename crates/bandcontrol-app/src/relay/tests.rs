//! The relay between the real server (as in the extension) and a fake Bluetooth, with a phone's client session.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant as StdInstant,
};

use bandcontrol_core::{
    engine::{Engine, EngineConfig},
    fake::FakeDaw,
};
use bandcontrol_proto::{
    client::{ClientEvent, ClientSession},
    pb,
    random::OsRandom,
};
use bandcontrol_server::{
    Server, ServerOptions,
    helper::{HelperOptions, Request},
};
use parking_lot::Mutex;
use pretty_assertions::assert_eq;

use super::*;

const TIMEOUT: Duration = Duration::from_secs(5);

fn secret() -> PairingSecret {
    PairingSecret::from_bytes([7; 16])
}

fn socket() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "bandcontrol-test-{}-{}.sock",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ))
}

/// The extension: server and engine on their own thread, like REAPER's timer.
struct Extension {
    requests: Arc<Mutex<Vec<Request>>>,
    show: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Extension {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}

fn extension(socket: &Path) -> Extension {
    let mut server = Server::start(ServerOptions {
        secret: secret(),
        name: "Test REAPER".into(),
        ble: true,
        tcp: None,
        helper: Some(HelperOptions {
            socket: socket.to_owned(),
            launch: false,
        }),
    })
    .unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let show = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let (requests, show, stop) = (requests.clone(), show.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut daw = FakeDaw::mother_project();
            let mut engine = Engine::new(EngineConfig::default());
            while !stop.load(Ordering::SeqCst) {
                let outputs = engine.tick(&mut daw, StdInstant::now(), server.poll_inputs());
                server.send(outputs);
                requests.lock().extend(server.poll_requests());
                if show.swap(false, Ordering::SeqCst) {
                    assert!(server.show_pairing());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })
    };
    Extension {
        requests,
        show,
        stop,
        thread: Some(thread),
    }
}

type FakeBluetooth = (EventSender, CommandReceiver);

struct App {
    bluetooth: mpsc::UnboundedReceiver<FakeBluetooth>,
    ui: mpsc::UnboundedReceiver<UiUpdate>,
    actions: mpsc::UnboundedSender<Action>,
    last: Snapshot,
}

impl App {
    async fn start(socket: &Path, options: impl FnOnce(&mut Options), fail_bluetooth: usize) -> Self {
        let (bluetooth_tx, bluetooth) = mpsc::unbounded_channel();
        let failures = Arc::new(AtomicUsize::new(fail_bluetooth));
        let start: StartBluetooth = Box::new(move |_config, events, commands| {
            let bluetooth_tx = bluetooth_tx.clone();
            let failures = failures.clone();
            Box::pin(async move {
                if failures.load(Ordering::SeqCst) > 0 {
                    failures.fetch_sub(1, Ordering::SeqCst);
                    return Err(BleError::Backend("Bluetooth is turned off".into()));
                }
                let _ = bluetooth_tx.send((events, commands));
                Ok(())
            })
        });
        let (ui_tx, ui) = mpsc::unbounded_channel();
        let (actions, actions_rx) = mpsc::unbounded_channel();
        let mut opts = Options {
            bluetooth_retry: Duration::from_millis(100),
            ..Options::default()
        };
        options(&mut opts);
        let listener = listen(socket).await.unwrap();
        let relay = Relay::new(
            opts,
            start,
            Box::new(move |u| {
                let _ = ui_tx.send(u);
            }),
        );
        tokio::task::spawn_local(relay.run(listener, actions_rx));
        Self {
            bluetooth,
            ui,
            actions,
            last: Snapshot::default(),
        }
    }

    async fn wait(&mut self, what: &str, mut until: impl FnMut(&UiUpdate) -> bool) -> UiUpdate {
        tokio::time::timeout(TIMEOUT, async {
            loop {
                let update = self.ui.recv().await.unwrap();
                if let UiUpdate::State(s) = &update {
                    self.last = s.clone();
                }
                if until(&update) {
                    return update;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}, last state {:?}", self.last))
    }

    async fn wait_state(&mut self, what: &str, mut until: impl FnMut(&Snapshot) -> bool) -> Snapshot {
        match self.wait(what, |u| matches!(u, UiUpdate::State(s) if until(s))).await {
            UiUpdate::State(s) => s,
            _ => unreachable!(),
        }
    }

    async fn bluetooth(&mut self) -> FakeBluetooth {
        tokio::time::timeout(TIMEOUT, self.bluetooth.recv())
            .await
            .unwrap()
            .unwrap()
    }
}

/// A phone with a session over the fake Bluetooth.
struct Phone {
    id: LocalId,
    session: ClientSession,
}

impl Phone {
    /// Connects and authenticates; returns the server name.
    async fn connect(id: LocalId, name: &str, bluetooth: &mut FakeBluetooth) -> (Self, String) {
        let mut session = ClientSession::new(&secret(), name, 180, &mut OsRandom);
        bluetooth.0.send(TransportEvent::Connected(id)).unwrap();
        for p in session.hello() {
            bluetooth.0.send(TransportEvent::Packet(id, p)).unwrap();
        }
        let mut phone = Self { id, session };
        let mut server_name = None;
        let mut state = false;
        while server_name.is_none() || !state {
            for event in phone.receive(bluetooth).await {
                match event {
                    ClientEvent::Authenticated { server_name: n, .. } => server_name = Some(n),
                    ClientEvent::Message(m) => state |= matches!(m.msg, Some(pb::server_message::Msg::State(_))),
                    ClientEvent::Send(_) => {}
                }
            }
        }
        (phone, server_name.unwrap())
    }

    /// Handles the next packets the server sends to this phone.
    async fn receive(&mut self, bluetooth: &mut FakeBluetooth) -> Vec<ClientEvent> {
        let command = tokio::time::timeout(TIMEOUT, bluetooth.1.recv())
            .await
            .expect("no packets from the server")
            .unwrap();
        let TransportCommand::Send { target, packets } = command else {
            panic!("unexpected {command:?}");
        };
        if !matches!(target, Target::All) && target != Target::Client(self.id) {
            return vec![];
        }
        let mut events = vec![];
        for p in packets {
            for event in self.session.receive(&p).unwrap() {
                if let ClientEvent::Send(packets) = &event {
                    for p in packets {
                        bluetooth.0.send(TransportEvent::Packet(self.id, p.clone())).unwrap();
                    }
                }
                events.push(event);
            }
        }
        events
    }
}

fn local() -> tokio::task::LocalSet {
    tokio::task::LocalSet::new()
}

#[tokio::test]
async fn phone_talks_to_reaper_through_the_app() {
    local()
        .run_until(async {
            let socket = socket();
            let mut app = App::start(&socket, |_| {}, 0).await;
            app.wait_state("waiting for REAPER", |s| s.pairing.is_none()).await;

            let extension = extension(&socket);
            let s = app
                .wait_state("Bluetooth running", |s| {
                    matches!(s.bluetooth, TransportStatus::Running(_))
                })
                .await;
            let pairing = s.pairing.unwrap();
            assert_eq!(pairing.name, "Test REAPER");
            assert_eq!(pairing.secret, secret());

            let mut bluetooth = app.bluetooth().await;
            let (_phone, server_name) = Phone::connect(7, "Pixel Test", &mut bluetooth).await;
            assert_eq!(server_name, "Test REAPER");
            app.wait_state("the phone in the list", |s| {
                s.phones
                    == vec![bandcontrol_server::helper::Phone {
                        client: 7,
                        name: "Pixel Test".into(),
                    }]
            })
            .await;

            // Window buttons reach REAPER, which owns the name and the pairing code.
            app.actions.send(Action::Rename("Stage PC".into())).unwrap();
            app.actions.send(Action::ResetKey).unwrap();
            tokio::time::timeout(TIMEOUT, async {
                while extension.requests.lock().len() < 2 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                *extension.requests.lock(),
                vec![Request::Rename("Stage PC".into()), Request::ResetKey]
            );

            // REAPER's action shows the window.
            extension.show.store(true, Ordering::SeqCst);
            app.wait("the pairing window", |u| *u == UiUpdate::ShowPairing).await;

            // REAPER quits: the app goes back to waiting.
            drop(extension);
            app.wait_state("REAPER gone", |s| s.pairing.is_none() && s.phones.is_empty())
                .await;
            let _ = std::fs::remove_file(socket);
        })
        .await;
}

#[tokio::test]
async fn a_disconnected_phone_stays_out_until_allowed() {
    local()
        .run_until(async {
            let socket = socket();
            let mut app = App::start(&socket, |_| {}, 0).await;
            let _extension = extension(&socket);
            let mut bluetooth = app.bluetooth().await;
            let (mut phone, _) = Phone::connect(3, "Galaxy", &mut bluetooth).await;
            app.wait_state("the phone in the list", |s| s.phones.len() == 1).await;

            app.actions.send(Action::Block(3)).unwrap();
            // The phone is told before its session ends.
            let goodbye = loop {
                let events = phone.receive(&mut bluetooth).await;
                if let Some(g) = events.iter().find_map(|e| match e {
                    ClientEvent::Message(m) => match &m.msg {
                        Some(pb::server_message::Msg::Goodbye(g)) => Some(*g),
                        _ => None,
                    },
                    _ => None,
                }) {
                    break g;
                }
            };
            assert_eq!(goodbye, pb::Goodbye {});
            let s = app
                .wait_state("the phone blocked", |s| s.phones.is_empty() && s.blocked.len() == 1)
                .await;
            assert_eq!(s.blocked[0].name, "Galaxy");
            // What the session sent before.
            tokio::time::sleep(Duration::from_millis(100)).await;
            while bluetooth.1.try_recv().is_ok() {}

            // Its packets are dropped: a new handshake gets no answer, and it gets no broadcasts.
            let mut again = ClientSession::new(&secret(), "Galaxy", 180, &mut OsRandom);
            for p in again.hello() {
                bluetooth.0.send(TransportEvent::Packet(3, p)).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
            while let Ok(c) = bluetooth.1.try_recv() {
                // Nor broadcasts, which Bluetooth would deliver to every subscribed phone.
                assert!(
                    !matches!(
                        c,
                        TransportCommand::Send {
                            target: Target::Client(3) | Target::All,
                            ..
                        }
                    ),
                    "sent {c:?} to a blocked phone"
                );
            }

            app.actions.send(Action::Unblock(3)).unwrap();
            app.wait_state("the phone allowed", |s| s.blocked.is_empty()).await;
            let _ = Phone::connect(3, "Galaxy", &mut bluetooth).await;
            app.wait_state("the phone back", |s| s.phones.len() == 1).await;
            let _ = std::fs::remove_file(socket);
        })
        .await;
}

#[tokio::test]
async fn bluetooth_is_retried_until_it_works() {
    local()
        .run_until(async {
            let socket = socket();
            let mut app = App::start(&socket, |_| {}, 2).await;
            let _extension = extension(&socket);
            app.wait_state(
                "Bluetooth failed",
                |s| matches!(&s.bluetooth, TransportStatus::Failed(e) if e.contains("turned off")),
            )
            .await;
            // Retries keep the last error on screen: no "starting" in between, which made the window flicker.
            let mut seen = vec![];
            app.wait_state("Bluetooth running", |s| {
                seen.push(s.bluetooth.clone());
                matches!(s.bluetooth, TransportStatus::Running(_))
            })
            .await;
            assert!(!seen.contains(&TransportStatus::Starting), "{seen:?}");
            let _ = std::fs::remove_file(socket);
        })
        .await;
}

#[tokio::test]
async fn quits_after_reaper_and_refuses_a_second_instance() {
    local()
        .run_until(async {
            let socket = socket();
            let mut app = App::start(&socket, |o| o.idle_quit = Duration::from_millis(200), 0).await;
            assert_eq!(
                listen(&socket).await.err().map(|e| e.kind()),
                Some(io::ErrorKind::AddrInUse)
            );
            let extension = extension(&socket);
            app.wait_state("REAPER connected", |s| s.pairing.is_some()).await;
            drop(extension);
            app.wait("quit", |u| *u == UiUpdate::Quit).await;
            let _ = std::fs::remove_file(socket);
        })
        .await;
}

#[tokio::test]
async fn a_second_reaper_replaces_the_first() {
    local()
        .run_until(async {
            let socket = socket();
            let mut app = App::start(&socket, |_| {}, 0).await;
            let _first = extension(&socket);
            app.wait_state("first REAPER", |s| s.pairing.is_some()).await;
            let _ = app.bluetooth().await;
            let _second = extension(&socket);
            // Bluetooth restarts for the new REAPER, the first one doesn't reconnect.
            let _ = app.bluetooth().await;
            tokio::time::sleep(Duration::from_millis(1500)).await;
            assert!(
                app.bluetooth.try_recv().is_err(),
                "the REAPERs keep replacing each other"
            );
            let _ = std::fs::remove_file(socket);
        })
        .await;
}
