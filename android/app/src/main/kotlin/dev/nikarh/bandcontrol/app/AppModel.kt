package dev.nikarh.bandcontrol.app

import android.app.Application
import android.content.Context
import android.os.Build
import androidx.core.content.edit
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.nikarh.bandcontrol.app.ble.BleConnector
import dev.nikarh.bandcontrol.core.Connection
import dev.nikarh.bandcontrol.core.ConnectionStatus
import dev.nikarh.bandcontrol.core.link.Connector
import dev.nikarh.bandcontrol.core.link.TcpConnector
import dev.nikarh.bandcontrol.core.protocol.Crypto
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import dev.nikarh.bandcontrol.core.protocol.PairingSecret
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.proto.ClientMessage
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn

@OptIn(ExperimentalCoroutinesApi::class)
class AppModel(
    app: Application,
) : AndroidViewModel(app) {
    private val prefs = app.getSharedPreferences("bandcontrol", Context.MODE_PRIVATE)

    private val mutablePairing = MutableStateFlow(loadPairing())
    val pairing: StateFlow<PairingInfo?> = mutablePairing.asStateFlow()

    private val connection = MutableStateFlow<Connection?>(null)

    val status: StateFlow<ConnectionStatus> =
        connection
            .flatMapLatest { it?.status ?: flowOf(ConnectionStatus.Idle) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, ConnectionStatus.Idle)

    val state: StateFlow<BandState> =
        connection
            .flatMapLatest { it?.state ?: flowOf(BandState()) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, BandState())

    private var started = false
    private val mutablePermitted = MutableStateFlow(false)
    val permissionsGranted: StateFlow<Boolean> = mutablePermitted.asStateFlow()

    fun pair(info: PairingInfo) {
        prefs.edit {
            putString(KEY_SECRET, info.secret.toCode())
            putString(KEY_NAME, info.name)
            putString(KEY_TCP, info.tcp)
        }
        mutablePairing.value = info
        restart()
    }

    fun unpair() {
        prefs.edit { clear() }
        mutablePairing.value = null
        restart()
    }

    /** Called when the UI becomes visible (and Bluetooth permissions are granted) or hidden. */
    fun setActive(
        active: Boolean,
        permissionsGranted: Boolean,
    ) {
        if (active == started && permissionsGranted == mutablePermitted.value) return
        started = active
        mutablePermitted.value = permissionsGranted
        restart()
    }

    fun send(message: ClientMessage): Boolean = connection.value?.send(message) ?: false

    private fun restart() {
        connection.value?.stop()
        val info = mutablePairing.value
        val connector = info?.let(::connector)
        connection.value =
            if (started && connector != null) {
                Connection(viewModelScope, connector, info.secret, Build.MODEL).also { it.start() }
            } else {
                null
            }
    }

    private fun connector(info: PairingInfo): Connector? {
        val tcp = info.tcp
        return when {
            tcp != null -> TcpConnector.parse(tcp)
            mutablePermitted.value -> BleConnector(getApplication(), Crypto.serviceUuid(info.secret))
            else -> null
        }
    }

    private fun loadPairing(): PairingInfo? {
        val secret = prefs.getString(KEY_SECRET, null)?.let(PairingSecret::fromCode) ?: return null
        return PairingInfo(secret, prefs.getString(KEY_NAME, null) ?: "", prefs.getString(KEY_TCP, null))
    }

    override fun onCleared() {
        connection.value?.stop()
    }

    private companion object {
        const val KEY_SECRET = "secret"
        const val KEY_NAME = "name"
        const val KEY_TCP = "tcp"
    }
}
