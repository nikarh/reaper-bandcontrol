//! Wire protocol of reaper-bandcontrol.
//!
//! * [`pb`] — protobuf messages generated from `proto/bandcontrol.proto`.
//! * [`framing`] — splitting messages into transport packets (BLE notifications / writes) and reassembling them.
//! * [`crypto`] — pairing secret, key derivation, proofs and AEAD.
//! * [`server`] / [`client`] — sans-IO session state machines, transports only move packets around.

pub mod client;
pub mod crypto;
pub mod framing;
pub mod pairing;
pub mod random;
pub mod server;

/// Protobuf messages.
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::cargo,
    missing_debug_implementations,
    unused_qualifications
)]
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/bandcontrol.v1.rs"));
}

/// Protocol version announced in `Hello`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Fixed GATT characteristic UUID for client → server packets.
pub const RX_CHARACTERISTIC_UUID: u128 = 0xb4d0_0001_7e1c_4f2b_9a5e_3c1d_2b6f_8a01;
/// Fixed GATT characteristic UUID for server → client packets (notify).
pub const TX_CHARACTERISTIC_UUID: u128 = 0xb4d0_0002_7e1c_4f2b_9a5e_3c1d_2b6f_8a01;
