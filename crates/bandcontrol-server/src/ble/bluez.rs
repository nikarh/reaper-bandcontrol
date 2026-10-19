//! BLE peripheral on Linux via `BlueZ`.
//!
//! Both characteristics use socket based IO (`AcquireWrite` / `AcquireNotify`), which recent `BlueZ` versions
//! provide per connected device, so packets are ordered and attributable to a device.

use std::{
    collections::{BTreeSet, HashMap},
    time::{Duration, Instant},
};

use bandcontrol_proto::server::Target;
use bluer::{
    Address,
    adv::{Advertisement, Type},
    gatt::{
        CharacteristicReader, CharacteristicWriter,
        local::{
            Application, Characteristic, CharacteristicControlEvent, CharacteristicNotify, CharacteristicNotifyMethod,
            CharacteristicWrite, CharacteristicWriteMethod, Service, characteristic_control,
        },
    },
};
use futures::StreamExt as _;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{BleConfig, BleError};
use crate::transport::{CommandReceiver, EventSender, LocalId, TransportCommand, TransportEvent};

/// Packets for a device are kept this long while its notification socket is not acquired yet.
const PENDING_TTL: Duration = Duration::from_secs(3);

fn id_of(address: Address) -> LocalId {
    address.0.iter().fold(0, |acc, b| (acc << 8) | LocalId::from(*b))
}

fn address_of(id: LocalId) -> Address {
    let b = id.to_be_bytes();
    Address([b[2], b[3], b[4], b[5], b[6], b[7]])
}

impl From<bluer::Error> for BleError {
    fn from(e: bluer::Error) -> Self {
        Self::Backend(e.to_string())
    }
}

pub async fn start(config: BleConfig, events: EventSender, commands: CommandReceiver) -> Result<(), BleError> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    if !adapter.is_powered().await? {
        adapter.set_powered(true).await?;
    }

    let (rx_control, rx_handle) = characteristic_control();
    let (tx_control, tx_handle) = characteristic_control();
    let app = Application {
        services: vec![Service {
            uuid: config.service_uuid,
            primary: true,
            characteristics: vec![
                Characteristic {
                    uuid: Uuid::from_u128(bandcontrol_proto::RX_CHARACTERISTIC_UUID),
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: true,
                        method: CharacteristicWriteMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: rx_handle,
                    ..Default::default()
                },
                Characteristic {
                    uuid: Uuid::from_u128(bandcontrol_proto::TX_CHARACTERISTIC_UUID),
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: tx_handle,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let app_handle = adapter.serve_gatt_application(app).await?;
    let adv_handle = adapter
        .advertise(Advertisement {
            advertisement_type: Type::Peripheral,
            service_uuids: BTreeSet::from([config.service_uuid]),
            discoverable: Some(true),
            local_name: Some(config.advertised_name.clone()),
            ..Default::default()
        })
        .await?;
    tracing::info!(
        adapter = adapter.name(),
        service = %config.service_uuid,
        "BLE peripheral advertising"
    );

    tokio::spawn(async move {
        // Keep registrations alive for the lifetime of the loop.
        let _handles = (app_handle, adv_handle, session);
        run(adapter, rx_control, tx_control, events, commands).await;
        tracing::info!("BLE peripheral stopped");
    });
    Ok(())
}

struct Peer {
    queue: mpsc::UnboundedSender<Vec<u8>>,
}

async fn run(
    adapter: bluer::Adapter,
    mut rx_control: bluer::gatt::local::CharacteristicControl,
    mut tx_control: bluer::gatt::local::CharacteristicControl,
    events: EventSender,
    mut commands: CommandReceiver,
) {
    let mut peers: HashMap<LocalId, Peer> = HashMap::new();
    let mut pending: HashMap<LocalId, (Instant, Vec<Vec<u8>>)> = HashMap::new();
    let (closed_tx, mut closed_rx) = mpsc::unbounded_channel::<LocalId>();
    let mut cleanup = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            event = rx_control.next() => match event {
                Some(CharacteristicControlEvent::Write(req)) => {
                    let id = id_of(req.device_address());
                    tracing::debug!(device = %req.device_address(), mtu = req.mtu(), "write socket acquired");
                    match req.accept() {
                        Ok(reader) => { tokio::spawn(read_loop(id, reader, events.clone())); }
                        Err(e) => tracing::warn!("failed to accept write socket: {e}"),
                    }
                }
                Some(CharacteristicControlEvent::Notify(_)) => {}
                None => return,
            },
            event = tx_control.next() => match event {
                Some(CharacteristicControlEvent::Notify(writer)) => {
                    let id = id_of(writer.device_address());
                    tracing::debug!(device = %writer.device_address(), mtu = writer.mtu(), "notify socket acquired");
                    let (queue, rx) = mpsc::unbounded_channel();
                    if let Some((_, packets)) = pending.remove(&id) {
                        for p in packets {
                            let _ = queue.send(p);
                        }
                    }
                    tokio::spawn(write_loop(id, writer, rx, closed_tx.clone()));
                    peers.insert(id, Peer { queue });
                }
                Some(CharacteristicControlEvent::Write(_)) => {}
                None => return,
            },
            Some(id) = closed_rx.recv() => {
                peers.remove(&id);
            }
            command = commands.recv() => match command {
                Some(TransportCommand::Send { target, packets }) => match target {
                    Target::All => {
                        for peer in peers.values() {
                            for p in &packets {
                                let _ = peer.queue.send(p.clone());
                            }
                        }
                    }
                    Target::Client(id) => match peers.get(&id) {
                        Some(peer) => {
                            for p in packets {
                                let _ = peer.queue.send(p);
                            }
                        }
                        None => pending.entry(id).or_insert_with(|| (Instant::now(), vec![])).1.extend(packets),
                    },
                },
                Some(TransportCommand::Disconnect(id)) => {
                    peers.remove(&id);
                    pending.remove(&id);
                    let adapter = adapter.clone();
                    tokio::spawn(async move {
                        let address = address_of(id);
                        match adapter.device(address) {
                            Ok(device) => {
                                if let Err(e) = device.disconnect().await {
                                    tracing::debug!(%address, "disconnect failed: {e}");
                                }
                            }
                            Err(e) => tracing::debug!(%address, "unknown device: {e}"),
                        }
                    });
                }
                None => return,
            },
            _ = cleanup.tick() => {
                pending.retain(|_, (since, _)| since.elapsed() < PENDING_TTL);
            }
        }
    }
}

async fn read_loop(id: LocalId, reader: CharacteristicReader, events: EventSender) {
    let _ = events.send(TransportEvent::Connected(id));
    loop {
        match reader.recv().await {
            Ok(packet) if !packet.is_empty() => {
                if events.send(TransportEvent::Packet(id, packet)).is_err() {
                    return;
                }
            }
            Ok(_) | Err(_) => break,
        }
    }
    tracing::debug!(device = %address_of(id), "write socket closed");
    let _ = events.send(TransportEvent::Disconnected(id));
}

async fn write_loop(
    id: LocalId,
    writer: CharacteristicWriter,
    mut queue: mpsc::UnboundedReceiver<Vec<u8>>,
    closed: mpsc::UnboundedSender<LocalId>,
) {
    loop {
        tokio::select! {
            packet = queue.recv() => match packet {
                Some(p) => {
                    if let Err(e) = writer.send(&p).await {
                        tracing::debug!(device = %address_of(id), "notify failed: {e}");
                        break;
                    }
                }
                None => break,
            },
            _ = writer.closed() => break,
        }
    }
    let _ = closed.send(id);
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn address_id_roundtrip() {
        let a = Address([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
        assert_eq!(id_of(a), 0x1234_5678_9abc);
        assert_eq!(address_of(id_of(a)), a);
    }
}
