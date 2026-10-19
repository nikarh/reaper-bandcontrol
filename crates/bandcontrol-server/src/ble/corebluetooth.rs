//! BLE peripheral on macOS via `CoreBluetooth`.
//!
//! `CoreBluetooth` calls the delegate on a private serial dispatch queue, and every `CoreBluetooth` object is only used
//! on that queue. Centrals are identified by their identifier and count as connected while subscribed to TX.
//! `CoreBluetooth` can't drop a central, so `Disconnect` only forgets it.

#![allow(unsafe_code)]

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    time::Duration,
};

use bandcontrol_proto::server::Target;
use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use objc2::{
    AllocAnyThread, DefinedClass, Message as _, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_core_bluetooth::{
    CBATTError, CBATTRequest, CBAdvertisementDataLocalNameKey, CBAdvertisementDataServiceUUIDsKey,
    CBAttributePermissions, CBCentral, CBCharacteristic, CBCharacteristicProperties, CBManager, CBManagerAuthorization,
    CBManagerState, CBMutableCharacteristic, CBMutableService, CBPeripheralManager,
    CBPeripheralManagerConnectionLatency, CBPeripheralManagerDelegate, CBService, CBUUID,
};
use objc2_foundation::{NSArray, NSBundle, NSData, NSDictionary, NSError, NSString, ns_string};
use tokio::sync::oneshot;
use uuid::Uuid;

use super::{BleConfig, BleError};
use crate::transport::{CommandReceiver, EventSender, LocalId, TransportCommand, TransportEvent};

/// How long to wait for the adapter, the service and advertising to come up.
const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// A value that is only used on the `CoreBluetooth` queue, moved there through `exec_async`.
struct OnQueue<T>(T);

// SAFETY: The wrapped value is only dereferenced inside closures running on the serial `CoreBluetooth` queue, and
// Objective-C reference counting is thread safe.
unsafe impl<T> Send for OnQueue<T> {}

impl<T> OnQueue<T> {
    /// Closures must capture the wrapper, not its field, so access goes through methods.
    fn get(&self) -> &T {
        &self.0
    }

    fn into_inner(self) -> T {
        self.0
    }
}

struct State {
    config: BleConfig,
    events: EventSender,
    ready: Option<oneshot::Sender<Result<(), BleError>>>,
    manager: Option<Retained<CBPeripheralManager>>,
    tx: Option<Retained<CBMutableCharacteristic>>,
    service_added: bool,
    ids: HashMap<String, LocalId>,
    last_id: LocalId,
    centrals: HashMap<LocalId, Retained<CBCentral>>,
    /// Notifications waiting for room in `CoreBluetooth`'s transmit queue; `None` targets all centrals.
    outbox: VecDeque<(Option<LocalId>, Retained<NSData>)>,
}

impl State {
    fn id_of(&mut self, central: &CBCentral) -> LocalId {
        // SAFETY: `identifier` has no preconditions.
        let key = unsafe { central.identifier() }.UUIDString().to_string();
        let last = &mut self.last_id;
        *self.ids.entry(key).or_insert_with(|| {
            *last += 1;
            *last
        })
    }

    fn report(&mut self, result: Result<(), BleError>) {
        match (self.ready.take(), result) {
            (Some(ready), result) => {
                let _ = ready.send(result);
            }
            (None, Ok(())) => tracing::info!("BLE peripheral advertising again"),
            (None, Err(e)) => tracing::warn!("BLE peripheral: {e}"),
        }
    }

    fn drop_centrals(&mut self) {
        for id in self.centrals.keys() {
            let _ = self.events.send(TransportEvent::Disconnected(*id));
        }
        self.centrals.clear();
        self.outbox.clear();
    }
}

struct Ivars {
    state: RefCell<State>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and `Delegate` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "BandControlPeripheralManagerDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    // SAFETY: NSObjectProtocol has no requirements.
    unsafe impl NSObjectProtocol for Delegate {}

    // SAFETY: The signatures match `CBPeripheralManagerDelegate`.
    unsafe impl CBPeripheralManagerDelegate for Delegate {
        #[unsafe(method(peripheralManagerDidUpdateState:))]
        fn did_update_state(&self, peripheral: &CBPeripheralManager) {
            self.with_state(|s| state_changed(s, peripheral));
        }

        #[unsafe(method(peripheralManager:didAddService:error:))]
        fn did_add_service(&self, peripheral: &CBPeripheralManager, _service: &CBService, error: Option<&NSError>) {
            self.with_state(|s| match error {
                Some(e) => s.report(Err(backend("adding the GATT service", e))),
                None => advertise(s, peripheral),
            });
        }

        #[unsafe(method(peripheralManagerDidStartAdvertising:error:))]
        fn did_start_advertising(&self, _peripheral: &CBPeripheralManager, error: Option<&NSError>) {
            self.with_state(|s| s.report(error.map_or(Ok(()), |e| Err(backend("advertising", e)))));
        }

        #[unsafe(method(peripheralManager:central:didSubscribeToCharacteristic:))]
        fn did_subscribe(&self, peripheral: &CBPeripheralManager, central: &CBCentral, _: &CBCharacteristic) {
            // SAFETY: `central` is connected.
            unsafe {
                peripheral.setDesiredConnectionLatency_forCentral(CBPeripheralManagerConnectionLatency::Low, central);
            }
            self.with_state(|s| {
                let id = s.id_of(central);
                if s.centrals.insert(id, central.retain()).is_none() {
                    let _ = s.events.send(TransportEvent::Connected(id));
                }
            });
        }

        #[unsafe(method(peripheralManager:central:didUnsubscribeFromCharacteristic:))]
        fn did_unsubscribe(&self, _peripheral: &CBPeripheralManager, central: &CBCentral, _: &CBCharacteristic) {
            self.with_state(|s| {
                let id = s.id_of(central);
                if s.centrals.remove(&id).is_some() {
                    let _ = s.events.send(TransportEvent::Disconnected(id));
                }
                s.outbox.retain(|(target, _)| *target != Some(id));
            });
        }

        #[unsafe(method(peripheralManager:didReceiveWriteRequests:))]
        fn did_receive_writes(&self, peripheral: &CBPeripheralManager, requests: &NSArray<CBATTRequest>) {
            let requests = requests.to_vec();
            let Some(first) = requests.first() else {
                return;
            };
            // SAFETY: `offset` has no preconditions.
            if requests.iter().any(|r| unsafe { r.offset() } != 0) {
                // SAFETY: `first` is the first request of this callback and is answered exactly once.
                unsafe { peripheral.respondToRequest_withResult(first, CBATTError::InvalidOffset) };
                return;
            }
            self.with_state(|s| {
                for request in &requests {
                    // SAFETY: `central` and `value` have no preconditions.
                    let (central, value) = unsafe { (request.central(), request.value()) };
                    let packet = value.map(|v| v.to_vec()).unwrap_or_default();
                    if !packet.is_empty() {
                        let id = s.id_of(&central);
                        let _ = s.events.send(TransportEvent::Packet(id, packet));
                    }
                }
            });
            // SAFETY: `first` is the first request of this callback and is answered exactly once.
            unsafe { peripheral.respondToRequest_withResult(first, CBATTError::Success) };
        }

        #[unsafe(method(peripheralManagerIsReadyToUpdateSubscribers:))]
        fn ready_to_update(&self, _peripheral: &CBPeripheralManager) {
            self.with_state(flush);
        }
    }
);

impl Delegate {
    fn new(state: State) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars {
            state: RefCell::new(state),
        });
        // SAFETY: NSObject's `init` has no preconditions.
        unsafe { msg_send![super(this), init] }
    }

    /// Panicking inside a delegate callback would unwind into `CoreBluetooth`, so re-entrancy is only logged.
    fn with_state(&self, f: impl FnOnce(&mut State)) {
        if let Ok(mut state) = self.ivars().state.try_borrow_mut() {
            f(&mut state);
        } else {
            tracing::error!("re-entrant CoreBluetooth callback ignored");
        }
    }

    fn handle(&self, command: TransportCommand) {
        self.with_state(|s| match command {
            TransportCommand::Send { target, packets } => {
                let target = match target {
                    Target::All => None,
                    Target::Client(id) => Some(id),
                };
                s.outbox.extend(packets.iter().map(|p| (target, NSData::with_bytes(p))));
                flush(s);
            }
            TransportCommand::Disconnect(id) => {
                s.centrals.remove(&id);
                s.ids.retain(|_, v| *v != id);
                s.outbox.retain(|(target, _)| *target != Some(id));
            }
        });
    }

    fn stop(&self) {
        self.with_state(|s| {
            if let Some(manager) = s.manager.take() {
                // SAFETY: Called on the manager's queue.
                unsafe {
                    manager.stopAdvertising();
                    manager.removeAllServices();
                }
            }
            s.tx = None;
            s.centrals.clear();
            s.outbox.clear();
        });
    }
}

fn backend(what: &str, error: &NSError) -> BleError {
    BleError::Backend(format!("{what} failed: {}", error.localizedDescription()))
}

fn cb_uuid(uuid: Uuid) -> Retained<CBUUID> {
    // SAFETY: The string is a valid UUID.
    unsafe { CBUUID::UUIDWithString(&NSString::from_str(&uuid.to_string())) }
}

fn state_changed(s: &mut State, peripheral: &CBPeripheralManager) {
    // SAFETY: `state` has no preconditions.
    let state = unsafe { peripheral.state() };
    if state == CBManagerState::PoweredOn {
        if !s.service_added {
            s.service_added = true;
            add_service(s, peripheral);
        }
        return;
    }
    s.drop_centrals();
    // Below "powered off" `CoreBluetooth` clears its database, so the service has to be added again.
    if state != CBManagerState::PoweredOff {
        s.service_added = false;
    }
    let reason = match state {
        CBManagerState::PoweredOff => "Bluetooth is turned off",
        CBManagerState::Unauthorized => "Bluetooth access is not allowed (System Settings > Privacy & Security)",
        CBManagerState::Unsupported => "this Mac can't act as a Bluetooth LE peripheral",
        _ => return,
    };
    s.report(Err(BleError::Backend(reason.into())));
}

fn add_service(s: &mut State, peripheral: &CBPeripheralManager) {
    // SAFETY: Valid arguments: dynamic characteristics have no value, notify-only characteristics need no permissions.
    let (rx, tx, service) = unsafe {
        let rx = CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &cb_uuid(Uuid::from_u128(bandcontrol_proto::RX_CHARACTERISTIC_UUID)),
            CBCharacteristicProperties::Write | CBCharacteristicProperties::WriteWithoutResponse,
            None,
            CBAttributePermissions::Writeable,
        );
        let tx = CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &cb_uuid(Uuid::from_u128(bandcontrol_proto::TX_CHARACTERISTIC_UUID)),
            CBCharacteristicProperties::Notify,
            None,
            CBAttributePermissions::empty(),
        );
        let service =
            CBMutableService::initWithType_primary(CBMutableService::alloc(), &cb_uuid(s.config.service_uuid), true);
        (rx, tx, service)
    };
    let characteristics: Retained<NSArray<CBCharacteristic>> = NSArray::from_slice(&[&**rx, &**tx]);
    // SAFETY: The service is not published yet; `addService` is called on the manager's queue.
    unsafe {
        service.setCharacteristics(Some(&characteristics));
        peripheral.addService(&service);
    }
    s.tx = Some(tx);
}

fn advertise(s: &State, peripheral: &CBPeripheralManager) {
    let name = NSString::from_str(&s.config.advertised_name);
    let uuids = NSArray::from_retained_slice(&[cb_uuid(s.config.service_uuid)]);
    let objects: [&AnyObject; 2] = [&name, &uuids];
    // SAFETY: The keys are immutable framework constants.
    let keys = unsafe { [CBAdvertisementDataLocalNameKey, CBAdvertisementDataServiceUUIDsKey] };
    let data = NSDictionary::from_slices(&keys, &objects);
    // SAFETY: Only the local name and service UUIDs are advertised, which `startAdvertising` supports.
    unsafe { peripheral.startAdvertising(Some(&data)) };
}

/// Sends queued notifications until `CoreBluetooth`'s transmit queue is full.
fn flush(s: &mut State) {
    let (Some(manager), Some(tx)) = (s.manager.clone(), s.tx.clone()) else {
        s.outbox.clear();
        return;
    };
    while let Some((target, data)) = s.outbox.front().map(|(t, d)| (*t, d.clone())) {
        let centrals = match target.map(|id| s.centrals.get(&id)) {
            None => None,
            Some(Some(central)) => Some(NSArray::from_slice(&[&**central])),
            Some(None) => {
                s.outbox.pop_front();
                continue;
            }
        };
        // SAFETY: Called on the manager's queue with its own characteristic.
        let sent =
            unsafe { manager.updateValue_forCharacteristic_onSubscribedCentrals(&data, &tx, centrals.as_deref()) };
        if !sent {
            // Resumed by `peripheralManagerIsReadyToUpdateSubscribers:`.
            return;
        }
        s.outbox.pop_front();
    }
}

pub async fn start(config: BleConfig, events: EventSender, mut commands: CommandReceiver) -> Result<(), BleError> {
    // macOS terminates a process that touches Bluetooth without this key in its Info.plist.
    if NSBundle::mainBundle()
        .objectForInfoDictionaryKey(ns_string!("NSBluetoothAlwaysUsageDescription"))
        .is_none()
    {
        return Err(BleError::Backend(
            "the application has no NSBluetoothAlwaysUsageDescription, macOS doesn't allow it to use Bluetooth".into(),
        ));
    }
    // SAFETY: `authorization` has no preconditions.
    let authorization = unsafe { CBManager::authorization_class() };
    if authorization == CBManagerAuthorization::Denied || authorization == CBManagerAuthorization::Restricted {
        return Err(BleError::Backend(
            "Bluetooth access is not allowed (System Settings > Privacy & Security > Bluetooth)".into(),
        ));
    }

    let service_uuid = config.service_uuid;
    let (ready_tx, ready) = oneshot::channel();
    let queue: DispatchRetained<DispatchQueue> =
        DispatchQueue::new("dev.nikarh.bandcontrol.ble", DispatchQueueAttr::SERIAL);
    let mut delegate = None;
    let state = OnQueue(State {
        config,
        events,
        ready: Some(ready_tx),
        manager: None,
        tx: None,
        service_added: false,
        ids: HashMap::new(),
        last_id: 0,
        centrals: HashMap::new(),
        outbox: VecDeque::new(),
    });
    let manager_queue = OnQueue(queue.clone());
    // Created on the queue so no callback runs before the manager is stored.
    queue.exec_sync(|| {
        let manager_queue = manager_queue.into_inner();
        let d = Delegate::new(state.into_inner());
        // SAFETY: The delegate outlives the manager, which it owns and releases in `stop`.
        let manager = unsafe {
            CBPeripheralManager::initWithDelegate_queue(
                CBPeripheralManager::alloc(),
                Some(ProtocolObject::from_ref(&*d)),
                Some(&manager_queue),
            )
        };
        d.with_state(|s| s.manager = Some(manager));
        delegate = Some(OnQueue(d));
    });
    let Some(delegate) = delegate.map(OnQueue::into_inner) else {
        return Err(BleError::Backend("creating the peripheral manager failed".into()));
    };

    let result = match tokio::time::timeout(READY_TIMEOUT, ready).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(BleError::Backend("the peripheral manager stopped".into())),
        Err(_) => Err(BleError::Backend(
            "Bluetooth did not become ready, is access allowed?".into(),
        )),
    };
    if let Err(e) = result {
        let d = OnQueue(delegate);
        queue.exec_async(move || d.get().stop());
        return Err(e);
    }
    tracing::info!(service = %service_uuid, "BLE peripheral advertising");

    let delegate = OnQueue(delegate);
    tokio::spawn(async move {
        while let Some(command) = commands.recv().await {
            let d = OnQueue(delegate.get().clone());
            queue.exec_async(move || d.get().handle(command));
        }
        queue.exec_async(move || delegate.get().stop());
        tracing::info!("BLE peripheral stopped");
    });
    Ok(())
}
