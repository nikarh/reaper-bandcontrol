//! BLE peripheral on Windows via the `WinRT` `GattServiceProvider` (Windows 10 1803 or later).
//!
//! Centrals are identified by their Bluetooth device id and count as connected while subscribed to TX. Windows can't
//! drop a central from the peripheral side, so `Disconnect` only forgets it.

use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use bandcontrol_proto::server::Target;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use uuid::Uuid;
use windows::{
    Devices::Bluetooth::{
        BluetoothError,
        GenericAttributeProfile::{
            GattCharacteristicProperties, GattCommunicationStatus, GattLocalCharacteristic,
            GattLocalCharacteristicParameters, GattLocalService, GattProtectionLevel, GattServiceProvider,
            GattServiceProviderAdvertisingParameters, GattSession, GattSubscribedClient, GattWriteOption,
            GattWriteRequestedEventArgs,
        },
    },
    Foundation::TypedEventHandler,
    Storage::Streams::{DataReader, DataWriter, IBuffer},
    core::{GUID, IInspectable, Ref},
};

use super::{BleConfig, BleError};
use crate::transport::{CommandReceiver, EventSender, LocalId, TransportCommand, TransportEvent};

impl From<windows::core::Error> for BleError {
    fn from(e: windows::core::Error) -> Self {
        Self::Backend(e.to_string())
    }
}

fn guid(uuid: Uuid) -> GUID {
    GUID::from_u128(uuid.as_u128())
}

fn check(error: BluetoothError, what: &str) -> Result<(), BleError> {
    let reason = match error {
        BluetoothError::Success => return Ok(()),
        BluetoothError::RadioNotAvailable => "no Bluetooth radio, or it is turned off",
        BluetoothError::ResourceInUse => "the Bluetooth radio is in use",
        BluetoothError::DisabledByPolicy => "Bluetooth is disabled by policy",
        BluetoothError::DisabledByUser => "Bluetooth is disabled",
        BluetoothError::NotSupported => "the Bluetooth adapter can't act as a peripheral",
        BluetoothError::ConsentRequired => "Bluetooth access needs consent",
        _ => "unknown Bluetooth error",
    };
    Err(BleError::Backend(format!("{what} failed: {reason} ({})", error.0)))
}

fn buffer(bytes: &[u8]) -> windows::core::Result<IBuffer> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(bytes)?;
    writer.DetachBuffer()
}

fn bytes(buffer: &IBuffer) -> windows::core::Result<Vec<u8>> {
    let reader = DataReader::FromBuffer(buffer)?;
    let mut bytes = vec![0; usize::try_from(reader.UnconsumedBufferLength()?).unwrap_or(0)];
    reader.ReadBytes(&mut bytes)?;
    Ok(bytes)
}

fn device_of(session: &GattSession) -> windows::core::Result<String> {
    Ok(session.DeviceId()?.Id()?.to_string())
}

/// Transport-local ids of Bluetooth device ids.
#[derive(Debug, Default)]
struct Ids {
    by_device: HashMap<String, LocalId>,
    last: LocalId,
}

impl Ids {
    fn get(&mut self, device: String) -> LocalId {
        let last = &mut self.last;
        *self.by_device.entry(device).or_insert_with(|| {
            *last += 1;
            *last
        })
    }

    fn forget(&mut self, id: LocalId) {
        self.by_device.retain(|_, v| *v != id);
    }
}

async fn characteristic(
    service: &GattLocalService,
    uuid: u128,
    properties: GattCharacteristicProperties,
) -> Result<GattLocalCharacteristic, BleError> {
    let parameters = GattLocalCharacteristicParameters::new()?;
    parameters.SetCharacteristicProperties(properties)?;
    parameters.SetReadProtectionLevel(GattProtectionLevel::Plain)?;
    parameters.SetWriteProtectionLevel(GattProtectionLevel::Plain)?;
    let result = service
        .CreateCharacteristicAsync(GUID::from_u128(uuid), &parameters)?
        .await?;
    check(result.Error()?, "creating a GATT characteristic")?;
    Ok(result.Characteristic()?)
}

pub async fn start(config: BleConfig, events: EventSender, commands: CommandReceiver) -> Result<(), BleError> {
    let result = GattServiceProvider::CreateAsync(guid(config.service_uuid))?.await?;
    check(result.Error()?, "creating the GATT service")?;
    let provider = result.ServiceProvider()?;
    let service = provider.Service()?;
    let rx = characteristic(
        &service,
        bandcontrol_proto::RX_CHARACTERISTIC_UUID,
        GattCharacteristicProperties::Write | GattCharacteristicProperties::WriteWithoutResponse,
    )
    .await?;
    let tx = characteristic(
        &service,
        bandcontrol_proto::TX_CHARACTERISTIC_UUID,
        GattCharacteristicProperties::Notify,
    )
    .await?;

    let ids = Arc::new(Mutex::new(Ids::default()));
    let write_ids = ids.clone();
    let write_events = events.clone();
    rx.WriteRequested(&TypedEventHandler::new(
        move |_, args: Ref<GattWriteRequestedEventArgs>| {
            let args = args.ok()?;
            let deferral = args.GetDeferral()?;
            // Blocking keeps a central's packets ordered; the phone waits for each write response anyway.
            let received = (|| {
                let request = args.GetRequestAsync()?.join()?;
                let packet = bytes(&request.Value()?)?;
                if request.Option()? == GattWriteOption::WriteWithResponse {
                    request.Respond()?;
                }
                let id = write_ids.lock().get(device_of(&args.Session()?)?);
                if !packet.is_empty() {
                    let _ = write_events.send(TransportEvent::Packet(id, packet));
                }
                Ok(())
            })();
            deferral.Complete()?;
            received
        },
    ))?;

    let (subscribers_tx, subscribers) = mpsc::unbounded_channel();
    tx.SubscribedClientsChanged(&TypedEventHandler::new(move |_, _: Ref<IInspectable>| {
        let _ = subscribers_tx.send(());
        Ok(())
    }))?;
    provider.AdvertisementStatusChanged(&TypedEventHandler::new(move |provider: Ref<GattServiceProvider>, _| {
        tracing::info!(status = ?provider.ok()?.AdvertisementStatus()?, "BLE advertisement status");
        Ok(())
    }))?;

    let parameters = GattServiceProviderAdvertisingParameters::new()?;
    parameters.SetIsConnectable(true)?;
    parameters.SetIsDiscoverable(true)?;
    provider.StartAdvertisingWithParameters(&parameters)?;
    tracing::info!(service = %config.service_uuid, "BLE peripheral advertising");

    tokio::spawn(async move {
        run(&tx, &ids, &events, commands, subscribers).await;
        let _ = provider.StopAdvertising();
        tracing::info!("BLE peripheral stopped");
    });
    Ok(())
}

struct Peer {
    queue: mpsc::UnboundedSender<Vec<u8>>,
}

async fn run(
    tx: &GattLocalCharacteristic,
    ids: &Mutex<Ids>,
    events: &EventSender,
    mut commands: CommandReceiver,
    mut subscribers: mpsc::UnboundedReceiver<()>,
) {
    let mut peers: HashMap<LocalId, Peer> = HashMap::new();
    loop {
        tokio::select! {
            Some(()) = subscribers.recv() => {
                if let Err(e) = sync_subscribers(tx, ids, events, &mut peers) {
                    tracing::warn!("reading BLE subscribers failed: {e}");
                }
            }
            command = commands.recv() => match command {
                Some(TransportCommand::Send { target: Target::All, packets }) => {
                    for peer in peers.values() {
                        for p in &packets {
                            let _ = peer.queue.send(p.clone());
                        }
                    }
                }
                Some(TransportCommand::Send { target: Target::Client(id), packets }) => {
                    // A forgotten central that stays subscribed gets a new id on its next write.
                    if !peers.contains_key(&id) && let Err(e) = sync_subscribers(tx, ids, events, &mut peers) {
                        tracing::warn!("reading BLE subscribers failed: {e}");
                    }
                    if let Some(peer) = peers.get(&id) {
                        for p in packets {
                            let _ = peer.queue.send(p);
                        }
                    } else {
                        tracing::debug!(id, "dropping packets for a central that is not subscribed");
                    }
                }
                Some(TransportCommand::Disconnect(id)) => {
                    peers.remove(&id);
                    ids.lock().forget(id);
                }
                None => return,
            },
        }
    }
}

/// Connects newly subscribed centrals and disconnects the ones that unsubscribed.
fn sync_subscribers(
    tx: &GattLocalCharacteristic,
    ids: &Mutex<Ids>,
    events: &EventSender,
    peers: &mut HashMap<LocalId, Peer>,
) -> windows::core::Result<()> {
    let mut current = HashMap::new();
    for client in tx.SubscribedClients()? {
        let id = ids.lock().get(device_of(&client.Session()?)?);
        current.insert(id, client);
    }
    peers.retain(|id, _| {
        let subscribed = current.contains_key(id);
        if !subscribed {
            let _ = events.send(TransportEvent::Disconnected(*id));
        }
        subscribed
    });
    for (id, client) in current {
        if let Entry::Vacant(entry) = peers.entry(id) {
            let (queue, packets) = mpsc::unbounded_channel();
            tokio::spawn(write_loop(id, tx.clone(), client, packets));
            entry.insert(Peer { queue });
            let _ = events.send(TransportEvent::Connected(id));
        }
    }
    Ok(())
}

async fn write_loop(
    id: LocalId,
    tx: GattLocalCharacteristic,
    client: GattSubscribedClient,
    mut packets: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    while let Some(packet) = packets.recv().await {
        let notify = buffer(&packet).and_then(|b| tx.NotifyValueForSubscribedClientAsync(&b, &client));
        let sent = match notify {
            Ok(operation) => operation.await,
            Err(e) => Err(e),
        };
        match sent.and_then(|r| r.Status()) {
            Ok(GattCommunicationStatus::Success) => {}
            Ok(status) => {
                tracing::debug!(id, ?status, "notify failed");
                break;
            }
            Err(e) => {
                tracing::debug!(id, "notify failed: {e}");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn ids_are_stable_until_forgotten() {
        let mut ids = Ids::default();
        let a = ids.get("a".into());
        let b = ids.get("b".into());
        assert_eq!(ids.get("a".into()), a);
        ids.forget(a);
        assert_eq!(ids.get("b".into()), b);
        assert_eq!(ids.get("a".into()), b + 1);
    }
}
