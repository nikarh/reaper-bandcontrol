package dev.nikarh.bandcontrol.core

import dev.nikarh.bandcontrol.core.link.Connector
import dev.nikarh.bandcontrol.core.link.LinkException
import dev.nikarh.bandcontrol.core.link.PacketLink
import dev.nikarh.bandcontrol.core.link.Problem
import dev.nikarh.bandcontrol.core.link.problemOf
import dev.nikarh.bandcontrol.core.protocol.ClientEvent
import dev.nikarh.bandcontrol.core.protocol.ClientSession
import dev.nikarh.bandcontrol.core.protocol.PairingSecret
import dev.nikarh.bandcontrol.core.protocol.Peaks
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.Meters
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlin.coroutines.coroutineContext

sealed interface ConnectionStatus {
    data object Idle : ConnectionStatus

    data object Connecting : ConnectionStatus

    data object Authenticating : ConnectionStatus

    data class Connected(
        val serverName: String,
        val session: Int,
    ) : ConnectionStatus

    data class Disconnected(
        val problem: Problem,
        /** Technical description for logs, never shown to users. */
        val detail: String = "",
    ) : ConnectionStatus {
        val authRejected get() = problem == Problem.PairingRejected
    }
}

/**
 * Keeps a session to the server alive: connects, authenticates, reconnects, and folds messages into [state].
 * Meters of the tracks passed to [meter] arrive in [meters], also across reconnects.
 */
class Connection(
    private val scope: CoroutineScope,
    private val connector: Connector,
    private val secret: PairingSecret,
    private val clientName: String,
    private val clock: () -> Long = { System.nanoTime() / 1_000_000 },
    private val retryDelayMs: Long = 1_000,
    private val maxRetryDelayMs: Long = 5_000,
    private val handshakeTimeoutMs: Long = 10_000,
) {
    private val mutableStatus = MutableStateFlow<ConnectionStatus>(ConnectionStatus.Idle)
    val status: StateFlow<ConnectionStatus> = mutableStatus.asStateFlow()

    private val mutableState = MutableStateFlow(BandState())
    val state: StateFlow<BandState> = mutableState.asStateFlow()

    private val mutableMeters = MutableStateFlow<Map<Int, Float?>>(emptyMap())

    /** Latest peak per metered track in dBFS, null for silence. */
    val meters: StateFlow<Map<Int, Float?>> = mutableMeters.asStateFlow()

    @Volatile private var meterTracks: List<Int> = emptyList()

    @Volatile private var meterSubscription = 0

    private val outgoing = Channel<ClientMessage>(OUTGOING_CAPACITY, BufferOverflow.DROP_OLDEST)
    private var job: Job? = null

    fun start() {
        if (job?.isActive == true) return
        job = scope.launch { run() }
    }

    fun stop() {
        job?.cancel()
        job = null
        mutableStatus.value = ConnectionStatus.Idle
    }

    /** Queues a command. Returns false when not connected (commands are not kept across reconnects). */
    fun send(message: ClientMessage): Boolean {
        if (status.value !is ConnectionStatus.Connected) return false
        return outgoing.trySend(message).isSuccess
    }

    /** Replaces the metered tracks, empty stops metering. */
    fun meter(tracks: List<Int>) {
        if (tracks == meterTracks) return
        meterTracks = tracks
        meterSubscription++
        mutableMeters.value = emptyMap()
        send(Commands.subscribeMeters(meterSubscription, tracks))
    }

    private fun onMeters(m: Meters) {
        val tracks = meterTracks
        if (m.subscription != meterSubscription) return
        val peaks = m.peaks.toByteArray()
        mutableMeters.value = tracks.withIndex().associate { (i, t) -> t to peaks.getOrNull(i)?.let { Peaks.decode(it.toInt()) } }
    }

    private suspend fun run() {
        var failures = 0
        var wasConnected = false
        while (coroutineContext.isActive) {
            mutableStatus.value = ConnectionStatus.Connecting
            var connected = false
            val status =
                try {
                    connector.connect().use { link ->
                        session(link) {
                            connected = true
                            wasConnected = true
                            failures = 0
                        }
                    }
                    ConnectionStatus.Disconnected(Problem.Lost, "link closed")
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    val problem = problemOf(e, connected)
                    // After a working session, failing reconnects are still "connection lost" to the user.
                    val shown = if (wasConnected && problem in RECONNECT_FAILURES) Problem.Lost else problem
                    ConnectionStatus.Disconnected(shown, e.toString())
                }
            mutableStatus.value = status
            val wait = if (status.authRejected) maxRetryDelayMs else minOf(retryDelayMs shl minOf(failures, 8), maxRetryDelayMs)
            failures++
            delay(wait)
        }
    }

    private suspend fun session(
        link: PacketLink,
        onAuthenticated: () -> Unit,
    ) = coroutineScope {
        val session = ClientSession(secret, clientName, link.maxPacket)
        val lock = Mutex()
        while (outgoing.tryReceive().isSuccess) {
            // Drop commands queued for a previous session.
        }
        for (p in session.hello()) link.send(p)
        mutableStatus.value = ConnectionStatus.Authenticating

        val handshakeTimer =
            launch {
                delay(handshakeTimeoutMs)
                throw LinkException(Problem.NoAnswer, "handshake timed out")
            }
        launch {
            for (message in outgoing) {
                val packets = lock.withLock { if (session.isReady) session.send(message) else emptyList() }
                for (p in packets) link.send(p)
            }
        }
        while (true) {
            val packet = link.receive()
            for (event in lock.withLock { session.receive(packet) }) {
                when (event) {
                    is ClientEvent.Send -> {
                        event.packets.forEach { link.send(it) }
                    }

                    is ClientEvent.Authenticated -> {
                        handshakeTimer.cancel()
                        onAuthenticated()
                        mutableStatus.value = ConnectionStatus.Connected(event.serverName, event.session)
                        if (meterTracks.isNotEmpty()) send(Commands.subscribeMeters(meterSubscription, meterTracks))
                    }

                    is ClientEvent.Message -> {
                        val meters = event.message.meters
                        if (meters != null) {
                            onMeters(meters)
                        } else {
                            mutableState.update { it.apply(event.message, clock()) }
                        }
                    }
                }
            }
        }
    }

    private companion object {
        const val OUTGOING_CAPACITY = 64
        val RECONNECT_FAILURES = setOf(Problem.NotFound, Problem.Unreachable, Problem.NoAnswer, Problem.Unknown)
    }
}
