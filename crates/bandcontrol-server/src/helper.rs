//! Bluetooth through the BandControl app (macOS).
//!
//! macOS only lets an application use Bluetooth when its `Info.plist` says why, and REAPER's doesn't. So on macOS the
//! extension doesn't touch Bluetooth: the BandControl app (crate `bandcontrol-app`) runs the `CoreBluetooth`
//! peripheral, the menu bar icon and the pairing window, and relays packets over a Unix socket. Sessions and
//! encryption stay in the extension, the app only sees encrypted packets (and the pairing code, to show it).
//!
//! ```text
//! REAPER ── extension ── sessions, crypto ──► helper transport ◄── Unix socket ──► BandControl.app ◄── BLE ──► phones
//! ```
//!
//! The app listens, the extension connects (and starts the app through Launch Services when nobody listens). One JSON
//! message per line. A newer extension connecting replaces the older one.

use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};

#[cfg(unix)]
use crate::transport::{CommandReceiver, EventSender};
use crate::{TransportStatus, transport::LocalId};

/// Bumped when the messages change incompatibly; the app refuses an extension with another version.
pub const VERSION: u32 = 1;

/// Environment variable overriding [`default_socket`].
pub const SOCKET_ENV: &str = "BANDCONTROL_HELPER_SOCKET";

/// The app's bundle identifier.
pub const BUNDLE_ID: &str = "dev.nikarh.bandcontrol";

/// Where the app listens: `~/Library/Application Support/BandControl/helper.sock`, or [`SOCKET_ENV`].
#[must_use]
pub fn default_socket() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(SOCKET_ENV).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join("Library/Application Support/BandControl/helper.sock"))
}

/// A phone with an authenticated session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phone {
    pub client: LocalId,
    /// What the phone calls itself (its model), may be empty.
    pub name: String,
}

/// Extension → app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToApp {
    /// First message of a connection: advertise the service of this pairing code, under this name.
    Hello {
        version: u32,
        code: String,
        name: String,
    },
    /// Packets for one phone, or for every phone with `client: None`.
    Send {
        client: Option<LocalId>,
        #[serde(with = "base64_list")]
        packets: Vec<Vec<u8>>,
    },
    Disconnect {
        client: LocalId,
    },
    /// The phones with a session, sent whenever they change.
    Phones {
        phones: Vec<Phone>,
    },
    /// The user ran *Show pairing QR code* in REAPER.
    ShowPairing,
}

/// App → extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromApp {
    Bluetooth {
        status: TransportStatus,
    },
    Connected {
        client: LocalId,
    },
    Packet {
        client: LocalId,
        #[serde(with = "base64")]
        data: Vec<u8>,
    },
    Disconnected {
        client: LocalId,
    },
    /// The user disconnected this phone in the app: end its session, telling the phone (`Goodbye`).
    Goodbye {
        client: LocalId,
    },
    /// Set from the app's window: the name shown on phones.
    Rename {
        name: String,
    },
    /// From the app's window: a new pairing code, every phone has to pair again.
    ResetKey,
    /// Another REAPER connected to the app; this one should stop using it.
    Replaced,
}

/// What the app asks REAPER to do; REAPER owns the pairing code and the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Rename(String),
    ResetKey,
}

#[derive(Debug, Clone)]
pub struct HelperOptions {
    pub socket: PathBuf,
    /// Start the app when nobody listens on the socket.
    pub launch: bool,
}

/// The extension's end of the link to the app, besides the transport channels.
#[derive(Debug)]
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) struct Link {
    pub status: watch::Sender<TransportStatus>,
    pub phones: watch::Receiver<Vec<Phone>>,
    pub requests: mpsc::UnboundedSender<Request>,
    pub show: mpsc::UnboundedReceiver<()>,
}

mod base64 {
    use data_encoding::BASE64;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(data: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&BASE64.encode(data))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        BASE64.decode(text.as_bytes()).map_err(serde::de::Error::custom)
    }
}

mod base64_list {
    use data_encoding::BASE64;
    use serde::{Deserialize, Deserializer, Serializer, ser::SerializeSeq as _};

    pub fn serialize<S: Serializer>(packets: &[Vec<u8>], s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(packets.len()))?;
        for p in packets {
            seq.serialize_element(&BASE64.encode(p))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<u8>>, D::Error> {
        Vec::<String>::deserialize(d)?
            .iter()
            .map(|t| BASE64.decode(t.as_bytes()).map_err(serde::de::Error::custom))
            .collect()
    }
}

/// Reads and writes newline separated JSON messages.
#[cfg(unix)]
pub mod codec {
    use std::io;

    use serde::{Serialize, de::DeserializeOwned};
    use tokio::io::{AsyncBufReadExt as _, AsyncRead, AsyncWrite, AsyncWriteExt as _, BufReader};

    /// Longest line accepted, far above the biggest message (a full state is a few kB).
    const MAX_LINE: usize = 1 << 20;

    /// Reads messages; [`Reader::next`] is cancel safe, a partly read line is kept for the next call.
    #[derive(Debug)]
    pub struct Reader<R> {
        inner: BufReader<R>,
        line: Vec<u8>,
    }

    impl<R: AsyncRead + Unpin> Reader<R> {
        pub fn new(inner: R) -> Self {
            Self {
                inner: BufReader::new(inner),
                line: Vec::new(),
            }
        }

        /// The next message, `None` at the end of the stream. Lines that don't parse (from a newer peer) are skipped.
        pub async fn next<T: DeserializeOwned>(&mut self) -> io::Result<Option<T>> {
            loop {
                if self.inner.read_until(b'\n', &mut self.line).await? == 0 {
                    return Ok(None);
                }
                if !self.line.ends_with(b"\n") {
                    if self.line.len() > MAX_LINE {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too long"));
                    }
                    continue;
                }
                let parsed = serde_json::from_slice(&self.line);
                self.line.clear();
                match parsed {
                    Ok(msg) => return Ok(Some(msg)),
                    Err(e) => tracing::debug!("ignoring a message: {e}"),
                }
            }
        }
    }

    pub async fn write<T: Serialize>(writer: &mut (impl AsyncWrite + Unpin), msg: &T) -> io::Result<()> {
        let mut line = serde_json::to_vec(msg).map_err(io::Error::other)?;
        line.push(b'\n');
        writer.write_all(&line).await
    }
}

/// How long to keep trying after starting the app before reporting it missing.
#[cfg(unix)]
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(unix)]
const RETRY: Duration = Duration::from_millis(500);

/// Starts the app through Launch Services, so macOS treats it as an application of its own (with its own Bluetooth
/// permission) and not as part of REAPER, as a child process would be.
#[cfg(target_os = "macos")]
fn launch() -> std::io::Result<()> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let candidates = [
        std::env::var_os("BANDCONTROL_APP").map(PathBuf::from),
        home.map(|h| h.join("Applications/BandControl.app")),
        Some(PathBuf::from("/Applications/BandControl.app")),
    ];
    let mut open = std::process::Command::new("/usr/bin/open");
    // -g: don't take the focus from REAPER.
    open.arg("-g");
    match candidates.into_iter().flatten().find(|p| p.exists()) {
        Some(app) => open.arg(app),
        None => open.args(["-b", BUNDLE_ID]),
    };
    let status = open.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("open exited with {status}")))
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
fn launch() -> std::io::Result<()> {
    Err(std::io::Error::other("the BandControl app only exists for macOS"))
}

/// Connects to the app and relays until `commands` closes, reconnecting when the app goes away.
#[cfg(unix)]
pub(crate) fn spawn(options: HelperOptions, hello: ToApp, events: EventSender, commands: CommandReceiver, link: Link) {
    tokio::spawn(run(options, hello, events, commands, link));
}

#[cfg(unix)]
async fn run(options: HelperOptions, hello: ToApp, events: EventSender, mut commands: CommandReceiver, mut link: Link) {
    use tokio::net::UnixStream;

    let mut launched = !options.launch;
    let mut waiting_since = tokio::time::Instant::now();
    loop {
        let stream = tokio::select! {
            s = UnixStream::connect(&options.socket) => s,
            // The extension is shutting down.
            None = commands.recv() => return,
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                if !launched {
                    launched = true;
                    waiting_since = tokio::time::Instant::now();
                    tracing::info!("starting the BandControl app");
                    if let Err(e) = launch() {
                        tracing::warn!("starting the BandControl app failed: {e}");
                    }
                }
                if waiting_since.elapsed() > LAUNCH_TIMEOUT {
                    let failed = TransportStatus::Failed(format!(
                        "the BandControl app isn't running ({e}); install it with bandcontrol-macos.pkg, or start it"
                    ));
                    link.status
                        .send_if_modified(|s| (*s != failed).then(|| *s = failed).is_some());
                }
                // A request to show the pairing window starts the app again.
                if link.show.try_recv().is_ok() {
                    launched = false;
                }
                tokio::time::sleep(RETRY).await;
                continue;
            }
        };
        tracing::info!("connected to the BandControl app");
        let ended = connection(stream, &hello, &events, &mut commands, &mut link).await;
        if events.is_closed() || commands.is_closed() {
            return;
        }
        let reason = match ended {
            Ended::Replaced => {
                tracing::info!("another REAPER uses the BandControl app now");
                link.status.send_replace(TransportStatus::Failed(
                    "another REAPER is using the BandControl app; restart this one to take it back".into(),
                ));
                // Keep the transport alive, without Bluetooth, until the server stops.
                while commands.recv().await.is_some() {}
                return;
            }
            Ended::Closed => "the BandControl app quit".to_owned(),
            Ended::Failed(e) => format!("lost the BandControl app: {e}"),
        };
        tracing::warn!("{reason}");
        link.status.send_replace(TransportStatus::Failed(reason));
        // Reconnect when the app comes back, but don't start it again: it was probably quit on purpose.
        launched = true;
        waiting_since = tokio::time::Instant::now();
    }
}

#[cfg(unix)]
enum Ended {
    Closed,
    Replaced,
    Failed(std::io::Error),
}

/// Relays between one connection to the app and the server until either side ends.
#[cfg(unix)]
async fn connection(
    stream: tokio::net::UnixStream,
    hello: &ToApp,
    events: &EventSender,
    commands: &mut CommandReceiver,
    link: &mut Link,
) -> Ended {
    use bandcontrol_proto::server::Target;

    use crate::transport::{TransportCommand, TransportEvent};

    let (reader, mut writer) = stream.into_split();
    let mut reader = codec::Reader::new(reader);
    let mut clients = std::collections::HashSet::new();
    let mut phones = link.phones.clone();
    phones.mark_changed();

    let result: std::io::Result<Ended> = async {
        codec::write(&mut writer, hello).await?;
        // A request from before the connection, like the one that started the app.
        while link.show.try_recv().is_ok() {
            codec::write(&mut writer, &ToApp::ShowPairing).await?;
        }
        loop {
            tokio::select! {
                msg = reader.next::<FromApp>() => match msg? {
                    None => return Ok(Ended::Closed),
                    Some(FromApp::Bluetooth { status }) => { link.status.send_replace(status); }
                    Some(FromApp::Connected { client }) => {
                        clients.insert(client);
                        let _ = events.send(TransportEvent::Connected(client));
                    }
                    Some(FromApp::Packet { client, data }) => {
                        clients.insert(client);
                        let _ = events.send(TransportEvent::Packet(client, data));
                    }
                    Some(FromApp::Disconnected { client }) => {
                        clients.remove(&client);
                        let _ = events.send(TransportEvent::Disconnected(client));
                    }
                    Some(FromApp::Goodbye { client }) => {
                        clients.remove(&client);
                        let _ = events.send(TransportEvent::Goodbye(client));
                    }
                    Some(FromApp::Rename { name }) => { let _ = link.requests.send(Request::Rename(name)); }
                    Some(FromApp::ResetKey) => { let _ = link.requests.send(Request::ResetKey); }
                    Some(FromApp::Replaced) => return Ok(Ended::Replaced),
                },
                command = commands.recv() => match command {
                    Some(TransportCommand::Send { target, packets }) => {
                        let client = match target {
                            Target::All => None,
                            Target::Client(id) => Some(id),
                        };
                        codec::write(&mut writer, &ToApp::Send { client, packets }).await?;
                    }
                    Some(TransportCommand::Disconnect(client)) => {
                        codec::write(&mut writer, &ToApp::Disconnect { client }).await?;
                    }
                    None => return Ok(Ended::Closed),
                },
                Ok(()) = phones.changed() => {
                    let list = phones.borrow_and_update().clone();
                    codec::write(&mut writer, &ToApp::Phones { phones: list }).await?;
                }
                Some(()) = link.show.recv() => codec::write(&mut writer, &ToApp::ShowPairing).await?,
            }
        }
    }
    .await;

    for client in clients {
        let _ = events.send(TransportEvent::Disconnected(client));
    }
    result.unwrap_or_else(Ended::Failed)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn messages_are_one_json_line_each() {
        let msg = ToApp::Send {
            client: Some(3),
            packets: vec![vec![1, 2, 3], vec![]],
        };
        let json = serde_json::to_string(&msg).unwrap_or_default();
        assert_eq!(json, r#"{"type":"send","client":3,"packets":["AQID",""]}"#);
        assert_eq!(serde_json::from_str::<ToApp>(&json).ok(), Some(msg));

        let msg = FromApp::Bluetooth {
            status: TransportStatus::Failed("off".into()),
        };
        let json = serde_json::to_string(&msg).unwrap_or_default();
        assert_eq!(json, r#"{"type":"bluetooth","status":{"Failed":"off"}}"#);
        assert_eq!(serde_json::from_str::<FromApp>(&json).ok(), Some(msg));
    }
}
