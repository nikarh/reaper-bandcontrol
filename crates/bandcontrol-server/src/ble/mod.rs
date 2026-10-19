//! BLE GATT peripheral transport.

#[cfg(target_os = "linux")]
mod bluez;

use uuid::Uuid;

use crate::transport::{CommandReceiver, EventSender};

#[derive(Debug, Clone)]
pub struct BleConfig {
    pub service_uuid: Uuid,
    /// Short name put into advertisements (the phone filters by service UUID, the name is informational).
    pub advertised_name: String,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum BleError {
    #[error("BLE peripheral mode is not supported on this platform yet")]
    Unsupported,
    #[error("{0}")]
    Backend(String),
}

/// Registers the GATT service and starts advertising. The transport runs until `commands` closes.
#[cfg(target_os = "linux")]
pub async fn start(config: BleConfig, events: EventSender, commands: CommandReceiver) -> Result<(), BleError> {
    bluez::start(config, events, commands).await
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::unused_async)]
pub async fn start(_config: BleConfig, _events: EventSender, _commands: CommandReceiver) -> Result<(), BleError> {
    Err(BleError::Unsupported)
}
