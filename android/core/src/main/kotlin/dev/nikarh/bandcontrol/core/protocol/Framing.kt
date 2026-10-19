package dev.nikarh.bandcontrol.core.protocol

/**
 * Packet framing, see `crates/bandcontrol-proto/src/framing.rs`.
 *
 * A packet is `header (1 byte) || chunk`: the top bit of the header says more fragments follow, the low 7 bits are
 * the channel.
 */
object Framing {
    const val CHANNEL_BROADCAST = 0
    const val CHANNEL_HANDSHAKE = 127
    const val MAX_SESSION_CHANNEL = 126
    const val MIN_PACKET = 20
    const val MAX_MESSAGE = 64 * 1024

    internal const val MORE = 0x80
    internal const val CHANNEL_MASK = 0x7f

    fun fragment(
        channel: Int,
        message: ByteArray,
        maxPacket: Int,
    ): List<ByteArray> {
        val header = channel and CHANNEL_MASK
        val chunk = maxOf(maxPacket, MIN_PACKET) - 1
        if (message.isEmpty()) return listOf(byteArrayOf(header.toByte()))
        val count = (message.size + chunk - 1) / chunk
        return List(count) { i ->
            val start = i * chunk
            val end = minOf(start + chunk, message.size)
            val more = if (i + 1 < count) MORE else 0
            byteArrayOf((header or more).toByte()) + message.copyOfRange(start, end)
        }
    }

    fun channelOf(packet: ByteArray): Int? = packet.firstOrNull()?.let { it.toInt() and CHANNEL_MASK }
}

class FramingException(
    message: String,
) : Exception(message)

/** Reassembles messages from packets, independently per channel. */
class Reassembler {
    private val partial = HashMap<Int, java.io.ByteArrayOutputStream>()

    /** Returns `(channel, message)` when [packet] completes a message. */
    fun push(packet: ByteArray): Pair<Int, ByteArray>? {
        if (packet.isEmpty()) throw FramingException("empty packet")
        val header = packet[0].toInt() and 0xff
        val channel = header and Framing.CHANNEL_MASK
        val buffer = partial.getOrPut(channel) { java.io.ByteArrayOutputStream() }
        if (buffer.size() + packet.size - 1 > Framing.MAX_MESSAGE) {
            partial.remove(channel)
            throw FramingException("message on channel $channel is too large")
        }
        buffer.write(packet, 1, packet.size - 1)
        if (header and Framing.MORE != 0) return null
        partial.remove(channel)
        return channel to buffer.toByteArray()
    }

    fun reset(channel: Int) {
        partial.remove(channel)
    }
}
