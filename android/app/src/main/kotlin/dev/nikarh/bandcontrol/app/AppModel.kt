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
import dev.nikarh.bandcontrol.app.ui.PresetActions
import dev.nikarh.bandcontrol.core.ComputerChoice
import dev.nikarh.bandcontrol.core.Connection
import dev.nikarh.bandcontrol.core.ConnectionStatus
import dev.nikarh.bandcontrol.core.chooseComputer
import dev.nikarh.bandcontrol.core.link.Connector
import dev.nikarh.bandcontrol.core.link.Problem
import dev.nikarh.bandcontrol.core.link.TcpConnector
import dev.nikarh.bandcontrol.core.newlyNearby
import dev.nikarh.bandcontrol.core.protocol.Crypto
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import dev.nikarh.bandcontrol.core.protocol.PairingSecret
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.core.state.MixPresets
import dev.nikarh.bandcontrol.core.state.PeakHolds
import dev.nikarh.bandcontrol.core.state.Presets
import dev.nikarh.bandcontrol.core.state.RegionView
import dev.nikarh.bandcontrol.core.state.commandsFor
import dev.nikarh.bandcontrol.core.state.levelsOf
import dev.nikarh.bandcontrol.core.state.snapshotOf
import dev.nikarh.bandcontrol.core.state.stopped
import dev.nikarh.bandcontrol.core.state.toSnapshot
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.Track
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.flow.updateAndGet
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject

private val PairingInfo.id: String get() = secret.toCode()

@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
class AppModel(
    app: Application,
) : AndroidViewModel(app),
    PresetActions {
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

    /** Connecting is allowed: the app is visible, or was until less than [BACKGROUND_MINUTES] ago. */
    private var started = false

    /** The app is on screen. */
    private var visible = false

    /** Ends the connection after the app spent [BACKGROUND_MINUTES] in the background. */
    private var background: Job? = null

    /** [ConnectionService] runs. */
    private var keepingAlive = false
    private val mutablePermitted = MutableStateFlow(false)
    val permissionsGranted: StateFlow<Boolean> = mutablePermitted.asStateFlow()

    private val connection = MutableStateFlow<Connection?>(null)

    val status: StateFlow<ConnectionStatus> =
        connection
            .flatMapLatest { it?.status ?: flowOf(ConnectionStatus.Idle) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, ConnectionStatus.Idle)

    /** The state as the computer sends it in this connection, empty until it arrives. */
    private val live: StateFlow<BandState> =
        connection
            .flatMapLatest { it?.state ?: flowOf(BandState()) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, BandState())

    /** The computer's state as last seen, from this or an earlier connection (stored), empty if never. */
    private val lastKnown = MutableStateFlow(BandState())

    /** The live state, or the last known one while (re)connecting; see [stale]. */
    val state: StateFlow<BandState> =
        combine(live, lastKnown) { now, last -> if (now.loaded) now else last.stopped(monotonicMs()) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, BandState())

    /** [state] is the last known one, not live: shown read-only. */
    val stale: StateFlow<Boolean> =
        combine(status, live) { status, now -> status !is ConnectionStatus.Connected || !now.loaded }
            .stateIn(viewModelScope, SharingStarted.Eagerly, true)

    /** Latest peak per metered track in dBFS, null for silence. */
    val meters: StateFlow<Map<Int, Float?>> =
        connection
            .flatMapLatest { it?.meters ?: flowOf(emptyMap()) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, emptyMap())

    private val mutableRegionView =
        MutableStateFlow(prefs.getString(KEY_REGION_VIEW, null)?.let { v -> RegionView.entries.find { it.name == v } })

    /** How the Playback tab lists songs, null until the user picks (then it depends on the project). */
    val regionView: StateFlow<RegionView?> = mutableRegionView.asStateFlow()

    fun setRegionView(view: RegionView) {
        mutableRegionView.value = view
        prefs.edit { putString(KEY_REGION_VIEW, view.name) }
    }

    private val mutablePresets = MutableStateFlow(Presets())

    /** The mix presets of the computer in use, kept on this phone. */
    val presets: StateFlow<Presets> = mutablePresets.asStateFlow()

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
                keepAlive()
            }.launchIn(viewModelScope)
        KeepAlive.disconnect = ::disconnect
        meters.onEach { levels -> mutablePeaks.update { it.add(levels) } }.launchIn(viewModelScope)
        live.onEach { if (it.loaded) lastKnown.value = it }.launchIn(viewModelScope)
        // Stored once the project settles (not on every fader step), and when the app leaves the screen.
        live
            .filter { it.loaded }
            .debounce(SNAPSHOT_DEBOUNCE_MS)
            .onEach { storeSnapshot() }
            .launchIn(viewModelScope)
        mutablePairing
            .map { it?.id }
            .distinctUntilChanged()
            .onEach { id ->
                lastKnown.value = id?.let { snapshotOf(prefs.getString(snapshotKey(it), null)) } ?: BandState()
                mutablePresets.value = Presets.fromJson(id?.let { prefs.getString(presetsKey(it), null) })
            }.launchIn(viewModelScope)
        decide()
    }

    /** The tracks currently shown with meters, kept across reconnects. */
    fun meter(tracks: List<Int>) {
        meterTracks = tracks
        // Nobody looks at meters in the background: they would only cost Bluetooth bandwidth and battery.
        if (visible) connection.value?.meter(tracks)
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
        prefs.edit {
            remove(snapshotKey(info.id))
            remove(presetsKey(info.id))
        }
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

    /**
     * Called when the UI becomes visible (and Bluetooth permissions are granted) or hidden. A working connection
     * outlives the UI by [BACKGROUND_MINUTES], so a look at a message or the locked screen doesn't end it.
     */
    fun setActive(
        active: Boolean,
        permissionsGranted: Boolean,
    ) {
        val permissionsChanged = permissionsGranted != mutablePermitted.value
        if (active == visible && !permissionsChanged) return
        mutablePermitted.value = permissionsGranted
        visible = active
        if (!active) storeSnapshot()
        watch()
        background?.cancel()
        background = null
        val current = connection.value
        when {
            active && started && !permissionsChanged -> {
                current?.meter(meterTracks)
                keepAlive()
            }

            active -> {
                started = true
                restart()
            }

            current != null && keepingAlive && !permissionsChanged -> {
                current.meter(emptyList())
                background =
                    viewModelScope.launch {
                        delay(BACKGROUND_MINUTES * 60_000L)
                        disconnect()
                    }
            }

            else -> {
                started = false
                restart()
            }
        }
    }

    /** Saves the current levels of [output] as preset [name] (replacing one of that name). */
    override fun save(
        output: Track,
        name: String,
    ) {
        val levels = state.value.levelsOf(output)
        updatePresets { it.with(output.name, it.of(output.name).save(name, levels)) }
    }

    /** Applies preset [name] (or [MixPresets.UNSAVED]) to [output]; false when not connected. */
    override fun load(
        output: Track,
        name: String,
    ): Boolean {
        if (stale.value) return false
        val state = state.value
        val (next, levels) = presets.value.of(output.name).load(name, state.levelsOf(output)) ?: return false
        if (!state.commandsFor(output, levels).all(::send)) return false
        updatePresets { it.with(output.name, next) }
        return true
    }

    override fun rename(
        output: Track,
        from: String,
        to: String,
    ) = updatePresets { it.with(output.name, it.of(output.name).rename(from, to)) }

    override fun delete(
        output: Track,
        name: String,
    ) = updatePresets { it.with(output.name, it.of(output.name).delete(name)) }

    private fun updatePresets(change: (Presets) -> Presets) {
        val id = mutablePairing.value?.id ?: return
        val presets = mutablePresets.updateAndGet(change)
        prefs.edit { putString(presetsKey(id), presets.toJson()) }
    }

    /** Connects again after the computer disconnected this phone, or after [disconnect]. */
    fun reconnect() {
        if (visible) started = true
        restart()
    }

    /** Ends the connection until the app is opened again or [reconnect]. */
    fun disconnect() {
        background?.cancel()
        background = null
        started = false
        restart()
    }

    /** Runs [ConnectionService] while a connection is wanted, once it has worked (it can only start while visible). */
    private fun keepAlive() {
        val app = getApplication<Application>()
        val status = status.value
        val wanted = connection.value != null && !(status is ConnectionStatus.Disconnected && status.problem in FINAL_PROBLEMS)
        val name = mutablePairing.value?.name?.ifEmpty { null } ?: "REAPER"
        val text = if (status is ConnectionStatus.Connected) "Connected to ${status.serverName}" else "Reconnecting to $name…"
        when {
            !wanted && keepingAlive -> {
                keepingAlive = false
                ConnectionService.stop(app)
            }

            wanted && keepingAlive -> {
                ConnectionService.update(app, text)
            }

            wanted && visible && status is ConnectionStatus.Connected -> {
                keepingAlive = true
                ConnectionService.start(app, text)
            }
        }
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
        if (!visible || all.size < 2) return
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
                    it.meter(if (visible) meterTracks else emptyList())
                    it.start()
                }
            } else {
                null
            }
        keepAlive()
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

    private fun storeSnapshot() {
        val id = mutablePairing.value?.id ?: return
        val state = lastKnown.value
        if (state.loaded) prefs.edit { putString(snapshotKey(id), state.stopped(monotonicMs()).toSnapshot()) }
    }

    override fun onCleared() {
        storeSnapshot()
        connection.value?.stop()
        if (KeepAlive.disconnect == ::disconnect) KeepAlive.disconnect = null
        if (keepingAlive) ConnectionService.stop(getApplication())
    }

    companion object {
        /** How long a working connection stays up while the app is in the background. */
        const val BACKGROUND_MINUTES = 10

        /** Nothing to keep alive: the connection doesn't retry, or not without the user. */
        private val FINAL_PROBLEMS = setOf(Problem.DisconnectedByComputer, Problem.PairingRejected, Problem.Incompatible)

        private const val TAG = "BandControl"
        private const val KEY_COMPUTERS = "computers"
        private const val KEY_SECRET = "secret"
        private const val KEY_NAME = "name"
        private const val KEY_TCP = "tcp"
        private const val KEY_REGION_VIEW = "regionView"

        /** How long the first scan may take to find every computer in range before one is picked. */
        private const val SETTLE_MS = 3_000L

        private const val SNAPSHOT_DEBOUNCE_MS = 2_000L

        private fun snapshotKey(id: String) = "state.$id"

        private fun presetsKey(id: String) = "presets.$id"

        private fun monotonicMs() = System.nanoTime() / 1_000_000
    }
}
