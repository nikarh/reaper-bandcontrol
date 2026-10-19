package dev.nikarh.bandcontrol.core.link

import kotlinx.coroutines.channels.Channel
import java.io.Closeable
import java.io.IOException

/** A connected packet transport (one BLE connection, one TCP socket...). */
interface PacketLink : Closeable {
    /** Largest packet the link carries in the client to server direction. */
    val maxPacket: Int

    /** Suspends until a packet arrives. Throws when the link is closed. */
    suspend fun receive(): ByteArray

    suspend fun send(packet: ByteArray)
}

fun interface Connector {
    suspend fun connect(): PacketLink
}

/** Base for callback or thread driven links: packets are pushed into a channel. */
abstract class ChannelLink : PacketLink {
    private val incoming = Channel<ByteArray>(Channel.UNLIMITED)

    protected fun deliver(packet: ByteArray) {
        incoming.trySend(packet)
    }

    protected fun closeIncoming(cause: Throwable? = null) {
        incoming.close(cause ?: IOException("link closed"))
    }

    override suspend fun receive(): ByteArray = incoming.receive()
}
