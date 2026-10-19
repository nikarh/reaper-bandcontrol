package dev.nikarh.bandcontrol.core.link

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.net.InetSocketAddress
import java.net.Socket

/** TCP transport used for development and tests: packets are prefixed with a big endian u16 length. */
class TcpConnector(
    private val host: String,
    private val port: Int,
    private val maxPacket: Int = 512,
) : Connector {
    override suspend fun connect(): PacketLink =
        withContext(Dispatchers.IO) {
            val socket = Socket()
            try {
                socket.connect(InetSocketAddress(host, port), CONNECT_TIMEOUT_MS)
                socket.tcpNoDelay = true
                TcpLink(socket, maxPacket)
            } catch (e: IOException) {
                socket.close()
                throw e
            }
        }

    companion object {
        private const val CONNECT_TIMEOUT_MS = 5_000

        /** Parses `host:port`. */
        fun parse(
            address: String,
            maxPacket: Int = 512,
        ): TcpConnector? {
            val host = address.substringBeforeLast(':', "")
            val port = address.substringAfterLast(':').toIntOrNull() ?: return null
            return if (host.isEmpty()) null else TcpConnector(host, port, maxPacket)
        }
    }
}

class TcpLink(
    private val socket: Socket,
    override val maxPacket: Int,
) : ChannelLink() {
    private val output = DataOutputStream(socket.getOutputStream().buffered())

    init {
        val input = DataInputStream(socket.getInputStream().buffered())
        Thread({
            try {
                while (true) {
                    val packet = ByteArray(input.readUnsignedShort())
                    input.readFully(packet)
                    deliver(packet)
                }
            } catch (e: IOException) {
                closeIncoming(e)
            }
        }, "tcp-link-reader").apply { isDaemon = true }.start()
    }

    override suspend fun send(packet: ByteArray) =
        withContext(Dispatchers.IO) {
            synchronized(output) {
                output.writeShort(packet.size)
                output.write(packet)
                output.flush()
            }
        }

    override fun close() {
        socket.close()
    }
}
