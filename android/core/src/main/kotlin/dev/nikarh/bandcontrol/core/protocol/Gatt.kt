package dev.nikarh.bandcontrol.core.protocol

import java.util.UUID

/** GATT layout, see `RX_CHARACTERISTIC_UUID` / `TX_CHARACTERISTIC_UUID` in `crates/bandcontrol-proto/src/lib.rs`. */
object Gatt {
    /** Client to server packets (write / write without response). */
    val RX: UUID = UUID.fromString("b4d00001-7e1c-4f2b-9a5e-3c1d2b6f8a01")

    /** Server to client packets (notify). */
    val TX: UUID = UUID.fromString("b4d00002-7e1c-4f2b-9a5e-3c1d2b6f8a01")

    val CLIENT_CONFIG: UUID = UUID.fromString("00002902-0000-1000-8000-00805f9b34fb")

    /** Largest ATT MTU, the packet size is the negotiated MTU minus the 3 byte ATT header. */
    const val MAX_MTU = 517
}
