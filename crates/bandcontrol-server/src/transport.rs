//! Interface between the session loop and concrete transports (TCP, BLE).

use bandcontrol_proto::server::Target;
use tokio::sync::mpsc;

/// Connection id local to one transport.
pub type LocalId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportEvent {
    Connected(LocalId),
    Packet(LocalId, Vec<u8>),
    Disconnected(LocalId),
    /// The user disconnected this phone: end its session, telling the phone.
    Goodbye(LocalId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportCommand {
    /// `Target::Client` carries a [`LocalId`].
    Send {
        target: Target,
        packets: Vec<Vec<u8>>,
    },
    Disconnect(LocalId),
}

pub type EventSender = mpsc::UnboundedSender<TransportEvent>;
pub type CommandReceiver = mpsc::UnboundedReceiver<TransportCommand>;
