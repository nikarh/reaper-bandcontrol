//! TCP transport for tests and debugging: every packet is prefixed with its length as `u16` big endian.

use std::{collections::HashMap, io, net::SocketAddr};

use bandcontrol_proto::server::Target;
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use crate::transport::{CommandReceiver, EventSender, LocalId, TransportCommand, TransportEvent};

pub(crate) async fn read_packet(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<Vec<u8>> {
    let len = reader.read_u16().await?;
    let mut packet = vec![0; usize::from(len)];
    reader.read_exact(&mut packet).await?;
    Ok(packet)
}

pub(crate) async fn write_packet(writer: &mut (impl AsyncWrite + Unpin), packet: &[u8]) -> io::Result<()> {
    let len = u16::try_from(packet.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "packet too big"))?;
    writer.write_u16(len).await?;
    writer.write_all(packet).await
}

enum Registry {
    Add(LocalId, mpsc::UnboundedSender<Option<Vec<u8>>>),
    Remove(LocalId),
}

/// Binds `addr` and serves connections until the command channel closes. Returns the bound address.
pub async fn start(addr: SocketAddr, events: EventSender, commands: CommandReceiver) -> io::Result<SocketAddr> {
    let listener = TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    let (registry_tx, registry_rx) = mpsc::unbounded_channel();
    tokio::spawn(dispatch(commands, registry_rx));
    tokio::spawn(accept(listener, events, registry_tx));
    Ok(local)
}

async fn accept(listener: TcpListener, events: EventSender, registry: mpsc::UnboundedSender<Registry>) {
    let mut next_id: LocalId = 1;
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("tcp accept failed: {e}");
                continue;
            }
        };
        let id = next_id;
        next_id += 1;
        tracing::info!(id, %peer, "tcp client connected");
        let _ = stream.set_nodelay(true);
        let (writer_tx, writer_rx) = mpsc::unbounded_channel();
        if registry.send(Registry::Add(id, writer_tx)).is_err() {
            return;
        }
        let _ = events.send(TransportEvent::Connected(id));
        tokio::spawn(connection(id, stream, writer_rx, events.clone(), registry.clone()));
    }
}

async fn connection(
    id: LocalId,
    stream: TcpStream,
    mut outgoing: mpsc::UnboundedReceiver<Option<Vec<u8>>>,
    events: EventSender,
    registry: mpsc::UnboundedSender<Registry>,
) {
    let (mut reader, mut writer) = stream.into_split();
    let read = async {
        while let Ok(packet) = read_packet(&mut reader).await {
            if events.send(TransportEvent::Packet(id, packet)).is_err() {
                break;
            }
        }
    };
    let write = async {
        while let Some(Some(packet)) = outgoing.recv().await {
            if write_packet(&mut writer, &packet).await.is_err() {
                break;
            }
        }
    };
    tokio::select! {
        () = read => {}
        () = write => {}
    }
    tracing::info!(id, "tcp client disconnected");
    let _ = registry.send(Registry::Remove(id));
    let _ = events.send(TransportEvent::Disconnected(id));
}

async fn dispatch(mut commands: CommandReceiver, mut registry: mpsc::UnboundedReceiver<Registry>) {
    let mut clients: HashMap<LocalId, mpsc::UnboundedSender<Option<Vec<u8>>>> = HashMap::new();
    loop {
        tokio::select! {
            r = registry.recv() => match r {
                Some(Registry::Add(id, tx)) => { clients.insert(id, tx); }
                Some(Registry::Remove(id)) => { clients.remove(&id); }
                None => return,
            },
            c = commands.recv() => match c {
                Some(TransportCommand::Send { target, packets }) => {
                    let targets: Vec<_> = match target {
                        Target::All => clients.values().collect(),
                        Target::Client(id) => clients.get(&id).into_iter().collect(),
                    };
                    for tx in targets {
                        for p in &packets {
                            let _ = tx.send(Some(p.clone()));
                        }
                    }
                }
                Some(TransportCommand::Disconnect(id)) => {
                    if let Some(tx) = clients.remove(&id) {
                        let _ = tx.send(None);
                    }
                }
                None => return,
            },
        }
    }
}
