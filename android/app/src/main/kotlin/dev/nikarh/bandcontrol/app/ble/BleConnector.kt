package dev.nikarh.bandcontrol.app.ble

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.BluetoothStatusCodes
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.os.Build
import android.os.ParcelUuid
import android.util.Log
import dev.nikarh.bandcontrol.core.link.ChannelLink
import dev.nikarh.bandcontrol.core.link.Connector
import dev.nikarh.bandcontrol.core.link.LinkException
import dev.nikarh.bandcontrol.core.link.PacketLink
import dev.nikarh.bandcontrol.core.link.Problem
import dev.nikarh.bandcontrol.core.protocol.Gatt
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout
import java.io.IOException
import java.util.UUID
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

private const val TAG = "BandControlBle"
private const val SCAN_TIMEOUT_MS = 10_000L
private const val OP_TIMEOUT_MS = 10_000L

/** Finds the REAPER computer by its secret-derived service UUID and opens a GATT connection to it. */
@SuppressLint("MissingPermission") // Permissions are requested before a connection is started.
class BleConnector(
    private val context: Context,
    private val service: UUID,
) : Connector {
    override suspend fun connect(): PacketLink {
        val adapter =
            context.getSystemService(BluetoothManager::class.java)?.adapter
                ?: throw LinkException(Problem.BluetoothUnavailable, "no Bluetooth adapter")
        if (!adapter.isEnabled) throw LinkException(Problem.BluetoothOff, "adapter disabled")
        val device = scan(adapter.bluetoothLeScanner ?: throw LinkException(Problem.BluetoothOff, "no LE scanner"))
        return BleLink.open(context, device, service)
    }

    private suspend fun scan(scanner: android.bluetooth.le.BluetoothLeScanner): BluetoothDevice =
        try {
            withTimeout(SCAN_TIMEOUT_MS) {
                suspendCancellableCoroutine { cont ->
                    val callback =
                        object : ScanCallback() {
                            override fun onScanResult(
                                callbackType: Int,
                                result: ScanResult,
                            ) {
                                scanner.stopScan(this)
                                if (cont.isActive) cont.resume(result.device)
                            }

                            override fun onScanFailed(errorCode: Int) {
                                if (cont.isActive) cont.resumeWithException(LinkException(Problem.Unknown, "scan failed ($errorCode)"))
                            }
                        }
                    val filter = ScanFilter.Builder().setServiceUuid(ParcelUuid(service)).build()
                    val settings = ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY).build()
                    scanner.startScan(listOf(filter), settings, callback)
                    cont.invokeOnCancellation { runCatching { scanner.stopScan(callback) } }
                }
            }
        } catch (_: TimeoutCancellationException) {
            throw LinkException(Problem.NotFound, "no device advertises the paired service")
        }
}

@SuppressLint("MissingPermission")
private class BleLink private constructor(
    context: Context,
    device: BluetoothDevice,
) : ChannelLink() {
    private val ops = Mutex()
    private var pending: CompletableDeferred<Int>? = null
    private val connected = CompletableDeferred<Unit>()
    private var mtu = 23
    private lateinit var rx: BluetoothGattCharacteristic

    private val callback =
        object : BluetoothGattCallback() {
            override fun onConnectionStateChange(
                gatt: BluetoothGatt,
                status: Int,
                newState: Int,
            ) {
                Log.d(TAG, "connection state $newState status $status")
                if (newState == BluetoothProfile.STATE_CONNECTED && status == BluetoothGatt.GATT_SUCCESS) {
                    connected.complete(Unit)
                } else if (newState == BluetoothProfile.STATE_DISCONNECTED) {
                    val error = IOException("disconnected ($status)")
                    connected.completeExceptionally(error)
                    pending?.completeExceptionally(error)
                    closeIncoming(error)
                    gatt.close()
                }
            }

            override fun onMtuChanged(
                gatt: BluetoothGatt,
                mtu: Int,
                status: Int,
            ) {
                if (status == BluetoothGatt.GATT_SUCCESS) this@BleLink.mtu = mtu
                pending?.complete(status)
            }

            override fun onServicesDiscovered(
                gatt: BluetoothGatt,
                status: Int,
            ) {
                pending?.complete(status)
            }

            override fun onDescriptorWrite(
                gatt: BluetoothGatt,
                descriptor: BluetoothGattDescriptor,
                status: Int,
            ) {
                pending?.complete(status)
            }

            override fun onCharacteristicWrite(
                gatt: BluetoothGatt,
                characteristic: BluetoothGattCharacteristic,
                status: Int,
            ) {
                pending?.complete(status)
            }

            override fun onCharacteristicChanged(
                gatt: BluetoothGatt,
                characteristic: BluetoothGattCharacteristic,
                value: ByteArray,
            ) {
                if (characteristic.uuid == Gatt.TX) deliver(value)
            }

            @Deprecated("Used before Android 13")
            @Suppress("DEPRECATION")
            override fun onCharacteristicChanged(
                gatt: BluetoothGatt,
                characteristic: BluetoothGattCharacteristic,
            ) {
                if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU && characteristic.uuid == Gatt.TX) {
                    deliver(characteristic.value.copyOf())
                }
            }
        }

    // The settings based overload only exists on API 37+.
    @Suppress("DEPRECATION")
    private val gatt: BluetoothGatt =
        device.connectGatt(context, false, callback, BluetoothDevice.TRANSPORT_LE)
            ?: throw LinkException(Problem.Unreachable, "connectGatt returned null")

    override val maxPacket get() = mtu - 3

    /** Runs one GATT operation and waits for its callback. */
    private suspend fun op(
        name: String,
        start: () -> Boolean,
    ): Int =
        ops.withLock {
            val deferred = CompletableDeferred<Int>()
            pending = deferred
            if (!start()) throw IOException("$name could not be started")
            try {
                withTimeout(OP_TIMEOUT_MS) { deferred.await() }
            } catch (_: TimeoutCancellationException) {
                throw IOException("$name timed out")
            } finally {
                pending = null
            }
        }

    private suspend fun setup(service: UUID) {
        try {
            withTimeout(OP_TIMEOUT_MS) { connected.await() }
        } catch (_: TimeoutCancellationException) {
            throw IOException("connection timed out")
        }
        op("MTU request") { gatt.requestMtu(Gatt.MAX_MTU) }
        gatt.requestConnectionPriority(BluetoothGatt.CONNECTION_PRIORITY_HIGH)
        if (op("service discovery") { gatt.discoverServices() } != BluetoothGatt.GATT_SUCCESS) {
            throw IOException("service discovery failed")
        }
        val s = gatt.getService(service) ?: throw LinkException(Problem.Unreachable, "service not discovered")
        rx = s.getCharacteristic(Gatt.RX) ?: throw LinkException(Problem.Incompatible, "rx characteristic missing")
        val tx = s.getCharacteristic(Gatt.TX) ?: throw LinkException(Problem.Incompatible, "tx characteristic missing")
        gatt.setCharacteristicNotification(tx, true)
        val cccd =
            tx.getDescriptor(Gatt.CLIENT_CONFIG)
                ?: throw LinkException(Problem.Incompatible, "notification descriptor missing")
        val status =
            op("notification subscription") {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    gatt.writeDescriptor(cccd, BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE) == BluetoothStatusCodes.SUCCESS
                } else {
                    @Suppress("DEPRECATION")
                    cccd.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                    @Suppress("DEPRECATION")
                    gatt.writeDescriptor(cccd)
                }
            }
        if (status != BluetoothGatt.GATT_SUCCESS) throw IOException("notification subscription failed ($status)")
        Log.i(TAG, "connected, mtu $mtu")
    }

    override suspend fun send(packet: ByteArray) {
        val type = BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
        val status =
            op("write") {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    gatt.writeCharacteristic(rx, packet, type) == BluetoothStatusCodes.SUCCESS
                } else {
                    rx.writeType = type
                    @Suppress("DEPRECATION")
                    rx.value = packet
                    @Suppress("DEPRECATION")
                    gatt.writeCharacteristic(rx)
                }
            }
        if (status != BluetoothGatt.GATT_SUCCESS) throw IOException("write failed ($status)")
    }

    override fun close() {
        gatt.disconnect()
        gatt.close()
        closeIncoming()
    }

    companion object {
        suspend fun open(
            context: Context,
            device: BluetoothDevice,
            service: UUID,
        ): PacketLink {
            val link = BleLink(context, device)
            try {
                link.setup(service)
            } catch (e: LinkException) {
                link.close()
                throw e
            } catch (e: IOException) {
                link.close()
                throw LinkException(Problem.Unreachable, "setup failed: ${e.message}", e)
            } catch (e: Throwable) {
                link.close()
                throw e
            }
            return link
        }
    }
}
