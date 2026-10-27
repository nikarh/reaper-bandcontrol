//! Hosts sessions on a background thread and bridges them to an [`Engine`](bandcontrol_core::engine::Engine) running
//! on the caller's (DAW main) thread.
//!
//! ```text
//! main thread                       server thread (tokio, current thread)
//! Engine::tick(poll_inputs()) ──►   ServerSessions ◄──► transports (BLE, TCP)
//!        send(outputs)        ──►
//! ```

pub mod ble;
pub mod client;
pub mod helper;
pub mod tcp;
pub mod transport;

use std::{net::SocketAddr, sync::Arc, thread::JoinHandle, time::Instant};

use bandcontrol_core::engine::{Input, Output};
use bandcontrol_proto::{
    crypto,
    pairing::PairingSecret,
    random::OsRandom,
    server::{ClientId, ServerConfig, ServerEvent, ServerSessions, Target},
};
use helper::{HelperOptions, Phone, Request};
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot, watch};
use transport::{LocalId, TransportCommand, TransportEvent};

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub secret: PairingSecret,
    pub name: String,
    pub ble: bool,
    pub tcp: Option<SocketAddr>,
    /// Bluetooth through the BandControl app instead of directly (macOS, where REAPER may not use Bluetooth).
    pub helper: Option<HelperOptions>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TransportStatus {
    #[default]
    Disabled,
    Starting,
    Running(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Status {
    pub ble: TransportStatus,
    pub tcp: TransportStatus,
    pub sessions: usize,
}

/// Owns the server thread. Dropping it stops the thread.
#[derive(Debug)]
pub struct Server {
    inputs: mpsc::UnboundedReceiver<Input>,
    outputs: mpsc::UnboundedSender<Output>,
    status: Arc<Mutex<Status>>,
    requests: mpsc::UnboundedReceiver<Request>,
    show: mpsc::UnboundedSender<()>,
    goodbye: mpsc::UnboundedSender<()>,
    helper: bool,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn start(options: ServerOptions) -> std::io::Result<Self> {
        let (inputs_tx, inputs) = mpsc::unbounded_channel();
        let (outputs, outputs_rx) = mpsc::unbounded_channel();
        let (shutdown, shutdown_rx) = oneshot::channel();
        let (requests_tx, requests) = mpsc::unbounded_channel();
        let (show, show_rx) = mpsc::unbounded_channel();
        let (goodbye, goodbye_rx) = mpsc::unbounded_channel();
        let helper = options.ble && options.helper.is_some();
        let app = App {
            requests: requests_tx,
            show: show_rx,
            goodbye: goodbye_rx,
        };
        let status = Arc::new(Mutex::new(Status {
            ble: if options.ble {
                TransportStatus::Starting
            } else {
                TransportStatus::Disabled
            },
            tcp: if options.tcp.is_some() {
                TransportStatus::Starting
            } else {
                TransportStatus::Disabled
            },
            sessions: 0,
        }));

        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let thread_status = status.clone();
        let thread = std::thread::Builder::new().name("bandcontrol".into()).spawn(move || {
            runtime.block_on(run(options, inputs_tx, outputs_rx, shutdown_rx, thread_status, app));
        })?;

        Ok(Self {
            inputs,
            outputs,
            status,
            requests,
            show,
            goodbye,
            helper,
            shutdown: Some(shutdown),
            thread: Some(thread),
        })
    }

    /// Everything that happened since the last call. Never blocks.
    pub fn poll_inputs(&mut self) -> Vec<Input> {
        let mut inputs = Vec::new();
        while let Ok(input) = self.inputs.try_recv() {
            inputs.push(input);
        }
        inputs
    }

    pub fn send(&self, outputs: Vec<Output>) {
        for output in outputs {
            let _ = self.outputs.send(output);
        }
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.status.lock().clone()
    }

    /// What the BandControl app asked for since the last call. Never blocks.
    pub fn poll_requests(&mut self) -> Vec<Request> {
        let mut requests = Vec::new();
        while let Ok(r) = self.requests.try_recv() {
            requests.push(r);
        }
        requests
    }

    /// Ends every session with a `Goodbye`, as if the user disconnected every phone (for the simulator).
    pub fn goodbye_all(&self) {
        let _ = self.goodbye.send(());
    }

    /// Shows the pairing window of the BandControl app, starting the app if needed. False without the app.
    #[must_use]
    pub fn show_pairing(&self) -> bool {
        self.helper && self.show.send(()).is_ok()
    }
}

/// The server's side of the BandControl app link.
struct App {
    requests: mpsc::UnboundedSender<Request>,
    show: mpsc::UnboundedReceiver<()>,
    goodbye: mpsc::UnboundedReceiver<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

const TRANSPORT_SHIFT: u32 = 56;
const TCP: u64 = 1;
const BLE: u64 = 2;

fn client_id(transport: u64, local: LocalId) -> ClientId {
    (transport << TRANSPORT_SHIFT) | (local & ((1 << TRANSPORT_SHIFT) - 1))
}

fn split_id(client: ClientId) -> (u64, LocalId) {
    (client >> TRANSPORT_SHIFT, client & ((1 << TRANSPORT_SHIFT) - 1))
}

struct Transports {
    commands: Vec<(u64, mpsc::UnboundedSender<TransportCommand>)>,
}

impl Transports {
    fn send(&self, transport: u64, command: &TransportCommand) {
        for (t, tx) in &self.commands {
            if *t == transport {
                let _ = tx.send(command.clone());
            }
        }
    }

    fn broadcast(&self, command: &TransportCommand) {
        for (_, tx) in &self.commands {
            let _ = tx.send(command.clone());
        }
    }
}

async fn run(
    options: ServerOptions,
    inputs: mpsc::UnboundedSender<Input>,
    mut outputs: mpsc::UnboundedReceiver<Output>,
    mut shutdown: oneshot::Receiver<()>,
    status: Arc<Mutex<Status>>,
    mut app: App,
) {
    let (events_tx, mut events) = mpsc::unbounded_channel::<(u64, TransportEvent)>();
    let mut transports = Transports { commands: vec![] };

    if let Some(addr) = options.tcp {
        let (event_tx, cmd_rx) = forward(TCP, &events_tx, &mut transports);
        status.lock().tcp = match tcp::start(addr, event_tx, cmd_rx).await {
            Ok(local) => {
                tracing::info!(%local, "tcp transport listening");
                TransportStatus::Running(local.to_string())
            }
            Err(e) => {
                tracing::error!(%addr, "tcp transport failed: {e}");
                TransportStatus::Failed(e.to_string())
            }
        };
    }

    let (phones_tx, phones_rx) = watch::channel(Vec::<Phone>::new());
    let (ble_status_tx, mut ble_status) = watch::channel(TransportStatus::Starting);
    if let (true, Some(helper)) = (options.ble, options.helper.clone()) {
        let (event_tx, cmd_rx) = forward(BLE, &events_tx, &mut transports);
        let link = helper::Link {
            status: ble_status_tx,
            phones: phones_rx,
            requests: app.requests.clone(),
            show: std::mem::replace(&mut app.show, mpsc::unbounded_channel().1),
        };
        start_helper(helper, &options, event_tx, cmd_rx, link);
    } else if options.ble {
        let (event_tx, cmd_rx) = forward(BLE, &events_tx, &mut transports);
        let service_uuid = uuid::Uuid::from_bytes(crypto::service_uuid(&options.secret));
        let config = ble::BleConfig {
            service_uuid,
            advertised_name: "BandCtl".into(),
        };
        status.lock().ble = match ble::start(config, event_tx, cmd_rx).await {
            Ok(()) => TransportStatus::Running(service_uuid.to_string()),
            Err(e) => {
                tracing::error!("BLE transport failed: {e}");
                TransportStatus::Failed(e.to_string())
            }
        };
    }

    let config = ServerConfig {
        name: options.name.clone(),
        ..ServerConfig::default()
    };
    let mut sessions = ServerSessions::new(&options.secret, config, Box::new(OsRandom));
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));

    loop {
        let server_events = tokio::select! {
            _ = &mut shutdown => break,
            Some((transport, event)) = events.recv() => match event {
                TransportEvent::Connected(local) => {
                    sessions.connect(client_id(transport, local), Instant::now());
                    vec![]
                }
                TransportEvent::Packet(local, packet) => sessions.receive(client_id(transport, local), &packet, Instant::now()),
                TransportEvent::Disconnected(local) => sessions.disconnect(client_id(transport, local)),
                TransportEvent::Goodbye(local) => sessions.goodbye(client_id(transport, local)),
            },
            output = outputs.recv() => match output {
                Some(Output::Broadcast(msg)) => sessions.broadcast(&msg),
                Some(Output::Unicast(session, msg)) => sessions.unicast(session, &msg),
                None => break,
            },
            _ = tick.tick() => sessions.tick(Instant::now()),
            Some(()) = app.goodbye.recv() => {
                let peers = sessions.peers();
                peers.into_iter().flat_map(|p| sessions.goodbye(p.client)).collect()
            }
            Ok(()) = ble_status.changed() => {
                status.lock().ble = ble_status.borrow_and_update().clone();
                vec![]
            }
        };

        for event in server_events {
            dispatch(event, &transports, &inputs);
        }
        status.lock().sessions = sessions.session_count();
        let phones = ble_phones(&sessions);
        phones_tx.send_if_modified(|p| (*p != phones).then(|| *p = phones).is_some());
    }
}

/// The phones connected over Bluetooth with a session.
fn ble_phones(sessions: &ServerSessions) -> Vec<Phone> {
    sessions
        .peers()
        .into_iter()
        .filter_map(|p| match split_id(p.client) {
            (BLE, client) => Some(Phone { client, name: p.name }),
            _ => None,
        })
        .collect()
}

/// Carries out what the sessions decided.
fn dispatch(event: ServerEvent, transports: &Transports, inputs: &mpsc::UnboundedSender<Input>) {
    match event {
        ServerEvent::Send {
            target: Target::All,
            packets,
        } => transports.broadcast(&TransportCommand::Send {
            target: Target::All,
            packets,
        }),
        ServerEvent::Send {
            target: Target::Client(id),
            packets,
        } => {
            let (transport, local) = split_id(id);
            transports.send(
                transport,
                &TransportCommand::Send {
                    target: Target::Client(local),
                    packets,
                },
            );
        }
        ServerEvent::Disconnect(id) => {
            let (transport, local) = split_id(id);
            transports.send(transport, &TransportCommand::Disconnect(local));
        }
        ServerEvent::Joined(s) => {
            tracing::info!(session = s, "client authenticated");
            let _ = inputs.send(Input::Joined(s));
        }
        ServerEvent::Left(s) => {
            tracing::info!(session = s, "client left");
            let _ = inputs.send(Input::Left(s));
        }
        ServerEvent::Message(s, msg) => {
            let _ = inputs.send(Input::Command(s, msg));
        }
    }
}

/// Bluetooth through the BandControl app.
#[cfg_attr(not(unix), allow(clippy::needless_pass_by_value))]
fn start_helper(
    helper: HelperOptions,
    options: &ServerOptions,
    events: transport::EventSender,
    commands: transport::CommandReceiver,
    link: helper::Link,
) {
    let hello = helper::ToApp::Hello {
        version: helper::VERSION,
        code: options.secret.to_code(),
        name: options.name.clone(),
    };
    #[cfg(unix)]
    helper::spawn(helper, hello, events, commands, link);
    #[cfg(not(unix))]
    {
        let _ = (helper, hello, events, commands);
        link.status.send_replace(TransportStatus::Failed(
            "the BandControl app only exists for macOS".into(),
        ));
    }
}

/// Creates the channels of one transport and forwards its events tagged with the transport id.
fn forward(
    transport: u64,
    events: &mpsc::UnboundedSender<(u64, TransportEvent)>,
    transports: &mut Transports,
) -> (transport::EventSender, transport::CommandReceiver) {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    transports.commands.push((transport, cmd_tx));
    let events = events.clone();
    tokio::spawn(async move {
        while let Some(e) = event_rx.recv().await {
            if events.send((transport, e)).is_err() {
                break;
            }
        }
    });
    (event_tx, cmd_rx)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::{assert_eq, assert_ne};

    use super::*;

    #[test]
    fn client_ids_roundtrip() {
        assert_eq!(split_id(client_id(BLE, 0x1234_5678_9abc)), (BLE, 0x1234_5678_9abc));
        assert_ne!(client_id(BLE, 1), client_id(TCP, 1));
    }
}
