package dev.nikarh.bandcontrol.app

import android.app.Application
import android.content.Context
import android.os.Build
import android.util.Log
import androidx.core.content.edit
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.nikarh.bandcontrol.app.ble.BleConnector
import dev.nikarh.bandcontrol.app.ble.NearbyScanner
import dev.nikarh.bandcontrol.core.ComputerChoice
import dev.nikarh.bandcontrol.core.Connection
import dev.nikarh.bandcontrol.core.ConnectionStatus
import dev.nikarh.bandcontrol.core.chooseComputer
import dev.nikarh.bandcontrol.core.link.Connector
import dev.nikarh.bandcontrol.core.link.TcpConnector
import dev.nikarh.bandcontrol.core.newlyNearby
import dev.nikarh.bandcontrol.core.protocol.Crypto
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import dev.nikarh.bandcontrol.core.protocol.PairingSecret
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.core.state.PeakHolds
import dev.nikarh.bandcontrol.proto.ClientMessage
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject

private val PairingInfo.id: String get() = secret.toCode()

@OptIn(ExperimentalCoroutinesApi::class)
class AppModel(
    app: Application,
) : AndroidViewModel(app) {
    private val prefs = app.getSharedPreferences("bandcontrol", Context.MODE_PRIVATE)
    private val scanner = NearbyScanner(app)

    private val mutableComputers = MutableStateFlow(loadComputers())

    /** Every paired computer, in pairing order. */
    val computers: StateFlow<List<PairingInfo>> = mutableComputers.asStateFlow()

    private val mutablePairing = MutableStateFlow<PairingInfo?>(null)

    /** The computer in use, null while none is chosen. */
    val pairing: StateFlow<PairingInfo?> = mutablePairing.asStateFlow()

    private val nearbyIds = MutableStateFlow<Set<String>>(emptySet())

    /** Paired computers in range; only watched while more than one is paired. */
    val nearby: StateFlow<List<PairingInfo>> =
        combine(mutableComputers, nearbyIds) { all, ids -> all.filter { it.id in ids } }
            .stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    private val mutableChoices = MutableStateFlow<List<PairingInfo>>(emptyList())

    /** Several paired computers are in range and none was chosen yet: ask which one. */
    val choices: StateFlow<List<PairingInfo>> = mutableChoices.asStateFlow()

    private val mutableNotice = MutableStateFlow<PairingInfo?>(null)

    /** Another paired computer came into range while one is in use. */
    val notice: StateFlow<PairingInfo?> = mutableNotice.asStateFlow()

    /** Chosen in this session; a fresh start chooses again by what is in range. */
    private var selected: String? = null

    /** The first scan had time to find every computer in range. */
    private var settled = false
    private var watching: Job? = null

    private var started = false
    private val mutablePermitted = MutableStateFlow(false)
    val permissionsGranted: StateFlow<Boolean> = mutablePermitted.asStateFlow()

    private val connection = MutableStateFlow<Connection?>(null)

    val status: StateFlow<ConnectionStatus> =
        connection
            .flatMapLatest { it?.status ?: flowOf(ConnectionStatus.Idle) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, ConnectionStatus.Idle)

    val state: StateFlow<BandState> =
        connection
            .flatMapLatest { it?.state ?: flowOf(BandState()) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, BandState())

    /** Latest peak per metered track in dBFS, null for silence. */
    val meters: StateFlow<Map<Int, Float?>> =
        connection
            .flatMapLatest { it?.meters ?: flowOf(emptyMap()) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, emptyMap())

    private val mutablePeaks = MutableStateFlow(PeakHolds())
    val peaks: StateFlow<PeakHolds> = mutablePeaks.asStateFlow()

    private var meterTracks: List<Int> = emptyList()

    init {
        status
            .onEach {
                when (it) {
                    is ConnectionStatus.Disconnected -> Log.w(TAG, "disconnected: ${it.problem} (${it.detail})")

                    // The computer may have been renamed since pairing; keep the saved name current.
                    is ConnectionStatus.Connected -> rememberName(it.serverName)

                    else -> Unit
                }
            }.launchIn(viewModelScope)
        meters.onEach { levels -> mutablePeaks.update { it.add(levels) } }.launchIn(viewModelScope)
        decide()
    }

    /** The tracks currently shown with meters, kept across reconnects. */
    fun meter(tracks: List<Int>) {
        meterTracks = tracks
        connection.value?.meter(tracks)
    }

    fun resetPeaks(tracks: Collection<Int>) = mutablePeaks.update { it.reset(tracks) }

    /** Adds (or, for [replacing], swaps in) a pairing and uses it. */
    fun pair(
        info: PairingInfo,
        replacing: PairingInfo? = null,
    ) {
        save(mutableComputers.value.filter { it.id != info.id && it.id != replacing?.id } + info)
        use(info)
    }

    /** Uses [info] from now on, e.g. picked from [choices] or the [notice]. */
    fun use(info: PairingInfo) {
        selected = info.id
        mutableNotice.value = null
        decide()
    }

    fun dismissNotice() {
        mutableNotice.value = null
    }

    fun forget(info: PairingInfo) {
        if (selected == info.id) selected = null
        save(mutableComputers.value.filter { it.id != info.id })
        decide()
    }

    private fun rememberName(name: String) {
        val info = mutablePairing.value ?: return
        if (name.isEmpty() || name == info.name) return
        val renamed = info.copy(name = name)
        mutablePairing.value = renamed
        save(mutableComputers.value.map { if (it.id == info.id) renamed else it }, rewatch = false)
    }

    /** Called when the UI becomes visible (and Bluetooth permissions are granted) or hidden. */
    fun setActive(
        active: Boolean,
        permissionsGranted: Boolean,
    ) {
        if (active == started && permissionsGranted == mutablePermitted.value) return
        started = active
        mutablePermitted.value = permissionsGranted
        watch()
        restart()
    }

    fun send(message: ClientMessage): Boolean = connection.value?.send(message) ?: false

    private fun decide() {
        val all = mutableComputers.value
        val choice = chooseComputer(all.map { it.id }, nearbyIds.value, selected, settled)
        mutableChoices.value = (choice as? ComputerChoice.Ask)?.ids?.mapNotNull { id -> all.find { it.id == id } }.orEmpty()
        val chosen = (choice as? ComputerChoice.Connect)?.let { c -> all.find { it.id == c.id } }
        if (chosen != null) selected = chosen.id
        val changed = chosen?.id != mutablePairing.value?.id
        mutablePairing.value = chosen
        if (changed) restart()
    }

    /** Watches which computers are in range while the UI is visible and there is more than one to pick from. */
    private fun watch() {
        watching?.cancel()
        watching = null
        val all = mutableComputers.value
        if (!started || all.size < 2) return
        watching =
            viewModelScope.launch {
                if (!settled) {
                    launch {
                        delay(SETTLE_MS)
                        settled = true
                        decide()
                    }
                }
                scanner.watch(all, mutablePermitted.value).collect { now ->
                    val fresh = newlyNearby(nearbyIds.value, now, mutablePairing.value?.id)
                    nearbyIds.value = now
                    val notice = mutableNotice.value
                    if (notice != null && notice.id !in now) mutableNotice.value = null
                    if (mutablePairing.value != null) {
                        fresh.firstNotNullOfOrNull { id -> all.find { it.id == id } }?.let { mutableNotice.value = it }
                    }
                    decide()
                }
            }
    }

    private fun restart() {
        connection.value?.stop()
        val info = mutablePairing.value
        val connector = info?.let(::connector)
        connection.value =
            if (started && connector != null) {
                Connection(viewModelScope, connector, info.secret, Build.MODEL).also {
                    it.meter(meterTracks)
                    it.start()
                }
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

    private fun save(
        computers: List<PairingInfo>,
        rewatch: Boolean = true,
    ) {
        val json = JSONArray()
        for (c in computers) json.put(JSONObject().put("s", c.id).put("n", c.name).putOpt("t", c.tcp))
        prefs.edit { putString(KEY_COMPUTERS, json.toString()) }
        mutableComputers.value = computers
        if (rewatch) watch()
    }

    private fun loadComputers(): List<PairingInfo> {
        val json = prefs.getString(KEY_COMPUTERS, null)
        if (json == null) {
            // Before several computers were supported, a single pairing was kept in separate keys.
            val secret = prefs.getString(KEY_SECRET, null)?.let(PairingSecret::fromCode) ?: return emptyList()
            return listOf(PairingInfo(secret, prefs.getString(KEY_NAME, null) ?: "", prefs.getString(KEY_TCP, null)))
        }
        val array = runCatching { JSONArray(json) }.getOrElse { return emptyList() }
        return (0 until array.length()).mapNotNull { i ->
            val o = array.optJSONObject(i) ?: return@mapNotNull null
            val secret = PairingSecret.fromCode(o.optString("s")) ?: return@mapNotNull null
            PairingInfo(secret, o.optString("n"), if (o.has("t")) o.getString("t") else null)
        }
    }

    override fun onCleared() {
        connection.value?.stop()
    }

    private companion object {
        const val TAG = "BandControl"
        const val KEY_COMPUTERS = "computers"
        const val KEY_SECRET = "secret"
        const val KEY_NAME = "name"
        const val KEY_TCP = "tcp"

        /** How long the first scan may take to find every computer in range before one is picked. */
        const val SETTLE_MS = 3_000L
    }
}
