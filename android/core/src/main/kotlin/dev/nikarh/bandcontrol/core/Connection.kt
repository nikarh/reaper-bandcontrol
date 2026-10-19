package dev.nikarh.bandcontrol.core

import dev.nikarh.bandcontrol.core.link.Connector
import dev.nikarh.bandcontrol.core.link.PacketLink
import dev.nikarh.bandcontrol.core.protocol.ClientEvent
import dev.nikarh.bandcontrol.core.protocol.ClientException
import dev.nikarh.bandcontrol.core.protocol.ClientSession
import dev.nikarh.bandcontrol.core.protocol.PairingSecret
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.proto.ClientMessage
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
import java.io.IOException
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
        val reason: String,
        val authRejected: Boolean = false,
    ) : ConnectionStatus
}

/** Keeps a session to the server alive: connects, authenticates, reconnects, and folds messages into [state]. */
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

    private suspend fun run() {
        var failures = 0
        while (coroutineContext.isActive) {
            mutableStatus.value = ConnectionStatus.Connecting
            var authRejected = false
            val reason =
                try {
                    connector.connect().use { link -> session(link) { failures = 0 } }
                    "disconnected"
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ClientException.AuthFailed) {
                    authRejected = true
                    e.message
                } catch (e: ClientException.BadServerProof) {
                    authRejected = true
                    e.message
                } catch (e: Exception) {
                    e.message ?: e.javaClass.simpleName
                }
            mutableStatus.value = ConnectionStatus.Disconnected(reason ?: "disconnected", authRejected)
            val wait = if (authRejected) maxRetryDelayMs else minOf(retryDelayMs shl minOf(failures, 8), maxRetryDelayMs)
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
                throw IOException("handshake timed out")
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
                    }

                    is ClientEvent.Message -> {
                        mutableState.update { it.apply(event.message, clock()) }
                    }
                }
            }
        }
    }

    private companion object {
        const val OUTGOING_CAPACITY = 64
    }
}
