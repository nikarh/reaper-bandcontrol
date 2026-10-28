//! The app without its user interface: accepts the REAPER extension's connection, runs Bluetooth for it and relays
//! packets between the two. Platform independent, so it is tested on Linux with a fake Bluetooth.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io,
    path::Path,
    pin::Pin,
    time::Duration,
};

use bandcontrol_proto::{
    crypto,
    pairing::{PairingInfo, PairingSecret},
    server::Target,
};
use bandcontrol_server::{
    TransportStatus,
    ble::{BleConfig, BleError},
    helper::{FromApp, Phone, ToApp, VERSION, codec},
    transport::{CommandReceiver, EventSender, LocalId, TransportCommand, TransportEvent},
};
use tokio::{
    net::{UnixListener, UnixStream, unix::OwnedWriteHalf},
    sync::mpsc,
    time::Instant,
};

/// Starts Bluetooth like [`bandcontrol_server::ble::start`]; it runs until the command channel closes.
pub type StartBluetooth =
    Box<dyn Fn(BleConfig, EventSender, CommandReceiver) -> Pin<Box<dyn Future<Output = Result<(), BleError>>>>>;

/// What the window and the menu show.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// The pairing of the connected REAPER, `None` while no REAPER is connected.
    pub pairing: Option<PairingInfo>,
    pub bluetooth: TransportStatus,
    pub phones: Vec<Phone>,
    /// Phones disconnected from the app, kept out until Bluetooth restarts.
    pub blocked: Vec<Phone>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiUpdate {
    State(Snapshot),
    ShowPairing,
    Quit,
}

/// What the user did in the window or the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Rename(String),
    ResetKey,
    Block(LocalId),
    Unblock(LocalId),
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Quit this long after REAPER disconnected, so the app goes away with REAPER. Not before REAPER connected once.
    pub idle_quit: Duration,
    /// Wait this long before starting Bluetooth again after it failed (turned off, permission not given yet).
    pub bluetooth_retry: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            idle_quit: Duration::from_secs(5),
            bluetooth_retry: Duration::from_secs(3),
        }
    }
}

/// Listens on the socket, creating its folder. Fails when another instance of the app is listening already.
pub async fn listen(socket: &Path) -> io::Result<UnixListener> {
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if UnixStream::connect(socket).await.is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "BandControl is running already",
        ));
    }
    // A socket left behind by an app that didn't quit cleanly.
    let _ = std::fs::remove_file(socket);
    UnixListener::bind(socket)
}

struct Reaper {
    reader: codec::Reader<tokio::net::unix::OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

/// Ends the connection of a replaced REAPER without losing what it was told: only the app's direction is closed and
/// what it still sends is read, until it has read [`FromApp::Replaced`] and hangs up. Dropping the connection instead
/// could make its next write fail, or reset the connection, before it reads why, and it would reconnect and replace
/// the other REAPER in turn.
async fn hang_up(mut reaper: Reaper) {
    let _ = tokio::io::AsyncWriteExt::shutdown(&mut reaper.writer).await;
    let drain = async { while let Ok(Some(_)) = reaper.reader.next::<ToApp>().await {} };
    let _ = tokio::time::timeout(Duration::from_secs(5), drain).await;
}

struct Bluetooth {
    commands: mpsc::UnboundedSender<TransportCommand>,
    events: mpsc::UnboundedReceiver<TransportEvent>,
}

enum Event {
    Accepted(io::Result<UnixStream>),
    Reaper(io::Result<Option<ToApp>>),
    Bluetooth(Option<TransportEvent>),
    Action(Option<Action>),
    Tick,
}

pub struct Relay {
    options: Options,
    start_bluetooth: StartBluetooth,
    ui: Box<dyn Fn(UiUpdate)>,
    reaper: Option<Reaper>,
    bluetooth: Option<Bluetooth>,
    /// When to try starting Bluetooth again.
    retry_at: Option<Instant>,
    /// Since when no REAPER is connected, after one was.
    idle_since: Option<Instant>,
    blocked: BTreeMap<LocalId, String>,
    /// What the user interface shows, to only send it changes.
    published: Option<Snapshot>,
    /// Phones Bluetooth reported and that aren't blocked.
    allowed: BTreeSet<LocalId>,
    /// Blocked phones that still get the extension's `Goodbye`, until it disconnects them.
    farewell: BTreeSet<LocalId>,
    snapshot: Snapshot,
}

impl std::fmt::Debug for Relay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relay")
            .field("snapshot", &self.snapshot)
            .finish_non_exhaustive()
    }
}

impl Relay {
    #[must_use]
    pub fn new(options: Options, start_bluetooth: StartBluetooth, ui: Box<dyn Fn(UiUpdate)>) -> Self {
        Self {
            options,
            start_bluetooth,
            ui,
            reaper: None,
            bluetooth: None,
            retry_at: None,
            idle_since: None,
            blocked: BTreeMap::new(),
            published: None,
            allowed: BTreeSet::new(),
            farewell: BTreeSet::new(),
            snapshot: Snapshot {
                bluetooth: TransportStatus::Disabled,
                ..Snapshot::default()
            },
        }
    }

    /// Serves REAPER until the user quits (`actions` closes) or REAPER is gone for [`Options::idle_quit`].
    pub async fn run(mut self, listener: UnixListener, mut actions: mpsc::UnboundedReceiver<Action>) {
        self.publish();
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            let event = tokio::select! {
                accepted = listener.accept() => Event::Accepted(accepted.map(|(s, _)| s)),
                msg = async {
                    match &mut self.reaper {
                        Some(r) => r.reader.next::<ToApp>().await,
                        None => std::future::pending().await,
                    }
                } => Event::Reaper(msg),
                event = async {
                    match &mut self.bluetooth {
                        Some(b) => b.events.recv().await,
                        None => std::future::pending().await,
                    }
                } => Event::Bluetooth(event),
                action = actions.recv() => Event::Action(action),
                _ = tick.tick() => Event::Tick,
            };
            match event {
                Event::Accepted(Ok(stream)) => self.accept(stream).await,
                Event::Accepted(Err(e)) => tracing::warn!("accepting a connection failed: {e}"),
                Event::Reaper(Ok(Some(msg))) => self.on_reaper(msg).await,
                Event::Reaper(Ok(None)) => self.reaper_gone("REAPER disconnected"),
                Event::Reaper(Err(e)) => self.reaper_gone(&format!("lost REAPER: {e}")),
                Event::Bluetooth(Some(event)) => self.on_bluetooth(event).await,
                Event::Bluetooth(None) => {
                    tracing::warn!("Bluetooth stopped");
                    self.bluetooth = None;
                }
                Event::Action(Some(action)) => self.action(action).await,
                Event::Action(None) => break,
                Event::Tick => {
                    if self.tick().await {
                        (self.ui)(UiUpdate::Quit);
                        break;
                    }
                }
            }
        }
        if let Some(mut reaper) = self.reaper.take() {
            // Tells the extension the app is gone, so it doesn't wait for a reply.
            let _ = tokio::io::AsyncWriteExt::shutdown(&mut reaper.writer).await;
        }
        self.bluetooth = None;
    }

    fn publish(&mut self) {
        if self.published.as_ref() != Some(&self.snapshot) {
            self.published = Some(self.snapshot.clone());
            (self.ui)(UiUpdate::State(self.snapshot.clone()));
        }
    }

    async fn send(&mut self, msg: &FromApp) {
        let Some(reaper) = &mut self.reaper else { return };
        if let Err(e) = codec::write(&mut reaper.writer, msg).await {
            self.reaper_gone(&format!("lost REAPER: {e}"));
        }
    }

    async fn accept(&mut self, stream: UnixStream) {
        if self.reaper.is_some() {
            tracing::info!("another REAPER connected, it replaces the current one");
            self.send(&FromApp::Replaced).await;
            if let Some(old) = self.reaper.take() {
                tokio::spawn(hang_up(old));
            }
            self.reaper_gone("replaced");
        }
        tracing::info!("REAPER connected");
        let (reader, writer) = stream.into_split();
        self.reaper = Some(Reaper {
            reader: codec::Reader::new(reader),
            writer,
        });
        self.idle_since = None;
    }

    fn reaper_gone(&mut self, why: &str) {
        tracing::info!("{why}");
        self.reaper = None;
        self.stop_bluetooth();
        self.snapshot.pairing = None;
        self.snapshot.bluetooth = TransportStatus::Disabled;
        self.idle_since = Some(Instant::now());
        self.publish();
    }

    fn stop_bluetooth(&mut self) {
        // Dropping the command channel stops the transport.
        self.bluetooth = None;
        self.retry_at = None;
        self.blocked.clear();
        self.allowed.clear();
        self.farewell.clear();
        self.snapshot.phones.clear();
        self.snapshot.blocked.clear();
    }

    async fn on_reaper(&mut self, msg: ToApp) {
        match msg {
            ToApp::Hello { version, code, name } => self.hello(version, &code, name).await,
            ToApp::Send { client, packets } => {
                let Some(b) = &self.bluetooth else { return };
                let targets = match client {
                    // Bluetooth sends broadcasts to every subscribed phone: address the allowed ones instead.
                    None if !self.blocked.is_empty() => self.allowed.iter().copied().map(Target::Client).collect(),
                    None => vec![Target::All],
                    Some(c) if self.blocked.contains_key(&c) && !self.farewell.contains(&c) => vec![],
                    Some(c) => vec![Target::Client(c)],
                };
                for target in targets {
                    let packets = packets.clone();
                    let _ = b.commands.send(TransportCommand::Send { target, packets });
                }
            }
            // A blocked phone keeps its id as long as Bluetooth doesn't forget it.
            ToApp::Disconnect { client } if self.blocked.contains_key(&client) => {
                self.farewell.remove(&client);
            }
            ToApp::Disconnect { client } => {
                if let Some(b) = &self.bluetooth {
                    let _ = b.commands.send(TransportCommand::Disconnect(client));
                }
            }
            ToApp::Phones { phones } => {
                self.snapshot.phones = phones;
                self.publish();
            }
            ToApp::ShowPairing => (self.ui)(UiUpdate::ShowPairing),
        }
    }

    async fn hello(&mut self, version: u32, code: &str, name: String) {
        self.stop_bluetooth();
        let secret = match (version, PairingSecret::from_code(code)) {
            (VERSION, Ok(secret)) => secret,
            (VERSION, Err(e)) => return self.fail(format!("REAPER sent an invalid pairing code: {e}")).await,
            _ => {
                return self
                    .fail(
                        "the BandControl app and the REAPER extension are from different releases, install both again"
                            .into(),
                    )
                    .await;
            }
        };
        self.snapshot.pairing = Some(PairingInfo { secret, name });
        self.set_status(TransportStatus::Starting).await;
        self.start_bluetooth().await;
    }

    async fn fail(&mut self, reason: String) {
        tracing::error!("{reason}");
        self.set_status(TransportStatus::Failed(reason)).await;
    }

    async fn set_status(&mut self, status: TransportStatus) {
        self.snapshot.bluetooth = status.clone();
        self.publish();
        self.send(&FromApp::Bluetooth { status }).await;
    }

    async fn start_bluetooth(&mut self) {
        let Some(pairing) = &self.snapshot.pairing else { return };
        let service_uuid = uuid::Uuid::from_bytes(crypto::service_uuid(&pairing.secret));
        let config = BleConfig {
            service_uuid,
            advertised_name: "BandCtl".into(),
        };
        // Retries keep showing why the last attempt failed, instead of flashing "starting" every few seconds.
        let (events_tx, events) = mpsc::unbounded_channel();
        let (commands, commands_rx) = mpsc::unbounded_channel();
        match (self.start_bluetooth)(config, events_tx, commands_rx).await {
            Ok(()) => {
                tracing::info!(%service_uuid, "Bluetooth advertising");
                self.bluetooth = Some(Bluetooth { commands, events });
                self.retry_at = None;
                self.set_status(TransportStatus::Running(service_uuid.to_string()))
                    .await;
            }
            Err(e) => {
                tracing::warn!("Bluetooth failed: {e}");
                self.retry_at = Some(Instant::now() + self.options.bluetooth_retry);
                self.set_status(TransportStatus::Failed(e.to_string())).await;
            }
        }
    }

    async fn on_bluetooth(&mut self, event: TransportEvent) {
        let msg = match event {
            TransportEvent::Connected(c) | TransportEvent::Packet(c, _) | TransportEvent::Disconnected(c)
                if self.blocked.contains_key(&c) =>
            {
                return;
            }
            // Bluetooth backends don't produce it.
            TransportEvent::Goodbye(_) => return,
            TransportEvent::Connected(client) => {
                self.allowed.insert(client);
                FromApp::Connected { client }
            }
            TransportEvent::Packet(client, data) => {
                self.allowed.insert(client);
                FromApp::Packet { client, data }
            }
            TransportEvent::Disconnected(client) => {
                self.allowed.remove(&client);
                FromApp::Disconnected { client }
            }
        };
        self.send(&msg).await;
    }

    async fn action(&mut self, action: Action) {
        match action {
            Action::Rename(name) => self.send(&FromApp::Rename { name }).await,
            Action::ResetKey => self.send(&FromApp::ResetKey).await,
            Action::Block(client) => {
                let name = self
                    .snapshot
                    .phones
                    .iter()
                    .find(|p| p.client == client)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                tracing::info!(client, name, "phone disconnected by the user");
                self.blocked.insert(client, name);
                self.allowed.remove(&client);
                self.farewell.insert(client);
                self.snapshot.phones.retain(|p| p.client != client);
                self.update_blocked();
                // The extension tells the phone and ends the session; the phone's packets don't reach it anymore.
                self.send(&FromApp::Goodbye { client }).await;
            }
            Action::Unblock(client) => {
                self.blocked.remove(&client);
                self.farewell.remove(&client);
                self.update_blocked();
            }
        }
    }

    fn update_blocked(&mut self) {
        self.snapshot.blocked = self
            .blocked
            .iter()
            .map(|(client, name)| Phone {
                client: *client,
                name: name.clone(),
            })
            .collect();
        self.publish();
    }

    /// Retries Bluetooth when due. True when it's time to quit.
    async fn tick(&mut self) -> bool {
        if self.retry_at.is_some_and(|at| Instant::now() >= at) && self.bluetooth.is_none() {
            self.retry_at = None;
            self.start_bluetooth().await;
        }
        self.reaper.is_none()
            && self
                .idle_since
                .is_some_and(|since| since.elapsed() >= self.options.idle_quit)
    }
}

#[cfg(test)]
mod tests;
