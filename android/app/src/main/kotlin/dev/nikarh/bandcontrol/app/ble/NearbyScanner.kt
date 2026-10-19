package dev.nikarh.bandcontrol.app.ble

import android.annotation.SuppressLint
import android.bluetooth.BluetoothManager
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.os.ParcelUuid
import android.os.SystemClock
import android.util.Log
import dev.nikarh.bandcontrol.core.protocol.Crypto
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.withContext
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException
import java.util.concurrent.ConcurrentHashMap

private const val TAG = "BandControlNearby"
private const val TICK_MS = 1_000L
private const val PROBE_EVERY = 2

/** A computer counts as gone this long after its last advertisement. */
private const val FORGET_MS = 15_000L

/**
 * Which paired computers are in range, by pairing code: those whose secret-derived service UUID is advertised over
 * BLE, and (debug pairings over TCP) those whose port accepts connections.
 */
@SuppressLint("MissingPermission") // Bluetooth is only scanned once the permissions are granted.
class NearbyScanner(
    private val context: Context,
) {
    fun watch(
        computers: List<PairingInfo>,
        bluetooth: Boolean,
    ): Flow<Set<String>> =
        channelFlow {
            val seen = ConcurrentHashMap<String, Long>()
            val byService =
                computers.filter { it.tcp == null }.associate { Crypto.serviceUuid(it.secret) to it.secret.toCode() }
            val stop = if (bluetooth && byService.isNotEmpty()) scan(byService, seen) else null
            try {
                var tick = 0
                while (true) {
                    if (tick++ % PROBE_EVERY == 0) {
                        for (computer in computers) {
                            val tcp = computer.tcp ?: continue
                            if (probe(tcp)) seen[computer.secret.toCode()] = SystemClock.elapsedRealtime()
                        }
                    }
                    val now = SystemClock.elapsedRealtime()
                    send(seen.filterValues { now - it < FORGET_MS }.keys.toSet())
                    delay(TICK_MS)
                }
            } finally {
                stop?.invoke()
            }
        }.distinctUntilChanged()

    private fun scan(
        byService: Map<java.util.UUID, String>,
        seen: MutableMap<String, Long>,
    ): (() -> Unit)? {
        val adapter = context.getSystemService(BluetoothManager::class.java)?.adapter ?: return null
        val scanner = (if (adapter.isEnabled) adapter.bluetoothLeScanner else null) ?: return null
        val callback =
            object : ScanCallback() {
                override fun onScanResult(
                    callbackType: Int,
                    result: ScanResult,
                ) = record(result)

                override fun onBatchScanResults(results: List<ScanResult>) = results.forEach(::record)

                override fun onScanFailed(errorCode: Int) {
                    Log.w(TAG, "scan failed ($errorCode)")
                }

                private fun record(result: ScanResult) {
                    for (uuid in result.scanRecord?.serviceUuids.orEmpty()) {
                        byService[uuid.uuid]?.let { seen[it] = SystemClock.elapsedRealtime() }
                    }
                }
            }
        val filters = byService.keys.map { ScanFilter.Builder().setServiceUuid(ParcelUuid(it)).build() }
        val settings = ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_BALANCED).build()
        return try {
            scanner.startScan(filters, settings, callback)
            val stop = { runCatching { scanner.stopScan(callback) }.let {} }
            stop
        } catch (e: SecurityException) {
            Log.w(TAG, "no permission to scan", e)
            null
        }
    }

    private suspend fun probe(address: String): Boolean {
        val host = address.substringBeforeLast(':', "")
        val port = address.substringAfterLast(':').toIntOrNull() ?: return false
        return withContext(Dispatchers.IO) {
            runCatching {
                Socket().use {
                    it.connect(InetSocketAddress(host, port), 500)
                    // A forwarded port (adb reverse) accepts even with nobody behind it, then closes right away;
                    // a live server waits silently for the phone's hello.
                    it.soTimeout = 300
                    it.getInputStream().read()
                }
                false
            }.getOrElse { it is SocketTimeoutException }
        }
    }
}
