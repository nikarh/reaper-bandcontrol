package dev.nikarh.bandcontrol.core.protocol

import dev.nikarh.bandcontrol.proto.Auth
import dev.nikarh.bandcontrol.proto.AuthFailure
import dev.nikarh.bandcontrol.proto.Challenge
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.Handshake
import dev.nikarh.bandcontrol.proto.Hello
import dev.nikarh.bandcontrol.proto.ServerMessage
import okio.ByteString.Companion.toByteString
import java.io.IOException
import java.security.SecureRandom

const val PROTOCOL_VERSION = 1

sealed interface ClientEvent {
    /** Packets to write to the server. */
    class Send(
        val packets: List<ByteArray>,
    ) : ClientEvent

    data class Authenticated(
        val session: Int,
        val serverName: String,
    ) : ClientEvent

    data class Message(
        val message: ServerMessage,
    ) : ClientEvent
}

sealed class ClientException(
    message: String,
) : Exception(message) {
    class AuthFailed(
        val reason: AuthFailure,
    ) : ClientException("server rejected authentication: $reason")

    class BadServerProof : ClientException("server failed to prove knowledge of the pairing secret")

    class Protocol(
        message: String,
    ) : ClientException("protocol violation: $message")

    class NotReady : ClientException("not connected")
}

/** Sans-IO client side of the session protocol, the same state machine as `crates/bandcontrol-proto/src/client.rs`. */
class ClientSession(
    secret: PairingSecret,
    private val name: String,
    private val maxPacket: Int,
    private val clientNonce: ByteArray = ByteArray(Crypto.NONCE_LEN).also(SecureRandom()::nextBytes),
) {
    private sealed interface Phase {
        data object New : Phase

        data object AwaitChallenge : Phase

        class AwaitWelcome(
            val session: Int,
            val sealer: Sealer,
            val opener: Opener,
            val serverName: String,
        ) : Phase

        class Ready(
            val session: Int,
            val sealer: Sealer,
            val opener: Opener,
            val group: BroadcastOpener,
        ) : Phase

        data object Failed : Phase
    }

    private val authKey = AuthKey(secret)
    private val reassembler = Reassembler()
    private var phase: Phase = Phase.New

    val isReady get() = phase is Phase.Ready

    val session: Int?
        get() =
            when (val p = phase) {
                is Phase.AwaitWelcome -> p.session
                is Phase.Ready -> p.session
                else -> null
            }

    fun hello(): List<ByteArray> {
        phase = Phase.AwaitChallenge
        val hello =
            Handshake(
                hello =
                    Hello(
                        version = PROTOCOL_VERSION,
                        client_nonce = clientNonce.toByteString(),
                        max_packet = maxPacket,
                        client_name = name,
                    ),
            )
        return Framing.fragment(Framing.CHANNEL_HANDSHAKE, hello.encode(), maxPacket)
    }

    fun send(message: ClientMessage): List<ByteArray> {
        val ready = phase as? Phase.Ready ?: throw ClientException.NotReady()
        return Framing.fragment(ready.session, ready.sealer.seal(message.encode()), maxPacket)
    }

    /** Handles a packet from the server. Exceptions are fatal for the session. */
    fun receive(packet: ByteArray): List<ClientEvent> {
        val channel = Framing.channelOf(packet) ?: return emptyList()
        val wanted =
            when (val p = phase) {
                Phase.AwaitChallenge -> channel == Framing.CHANNEL_HANDSHAKE
                is Phase.AwaitWelcome -> channel == p.session
                is Phase.Ready -> channel == p.session || channel == Framing.CHANNEL_BROADCAST
                Phase.New, Phase.Failed -> false
            }
        if (!wanted) return emptyList()

        val message =
            try {
                reassembler.push(packet)?.second ?: return emptyList()
            } catch (e: FramingException) {
                if (channel == Framing.CHANNEL_BROADCAST || channel == Framing.CHANNEL_HANDSHAKE) return emptyList()
                fail(ClientException.Protocol(e.message ?: "framing"))
            }

        return when (channel) {
            Framing.CHANNEL_HANDSHAKE -> onHandshake(message)
            Framing.CHANNEL_BROADCAST -> onBroadcast(message)
            else -> onUnicast(message)
        }
    }

    private fun fail(e: ClientException): Nothing {
        phase = Phase.Failed
        throw e
    }

    private fun onHandshake(message: ByteArray): List<ClientEvent> {
        // Other clients' handshakes may be visible on broadcast transports, ignore anything not addressed to us.
        val handshake =
            try {
                Handshake.ADAPTER.decode(message)
            } catch (_: IOException) {
                return emptyList()
            }
        val nonce = clientNonce.toByteString()
        handshake.challenge?.takeIf { it.client_nonce == nonce }?.let { return onChallenge(it) }
        handshake.failed?.takeIf { it.client_nonce == nonce }?.let { fail(ClientException.AuthFailed(it.reason)) }
        return emptyList()
    }

    private fun onChallenge(c: Challenge): List<ClientEvent> {
        val session = c.session_id
        if (session !in 1..Framing.MAX_SESSION_CHANNEL) fail(ClientException.Protocol("invalid session id"))
        val serverNonce = c.server_nonce.toByteArray()
        if (!authKey.verifyServerProof(clientNonce, serverNonce, session, c.server_proof.toByteArray())) {
            fail(ClientException.BadServerProof())
        }
        val proof = authKey.clientProof(clientNonce, serverNonce, session)
        val (s2c, c2s) = authKey.sessionKeys(clientNonce, serverNonce)
        reassembler.reset(session)
        phase = Phase.AwaitWelcome(session, Sealer(c2s), Opener(s2c), c.server_name)
        val auth = Handshake(auth = Auth(client_proof = proof.toByteString()))
        return listOf(ClientEvent.Send(Framing.fragment(Framing.CHANNEL_HANDSHAKE, auth.encode(), maxPacket)))
    }

    private fun onUnicast(message: ByteArray): List<ClientEvent> {
        val opener =
            when (val p = phase) {
                is Phase.AwaitWelcome -> p.opener
                is Phase.Ready -> p.opener
                else -> return emptyList()
            }
        val plaintext =
            try {
                opener.open(message)
            } catch (_: DecryptException) {
                fail(ClientException.Protocol("unicast frame failed authentication"))
            }
        val msg =
            try {
                ServerMessage.ADAPTER.decode(plaintext)
            } catch (_: IOException) {
                fail(ClientException.Protocol("malformed server message"))
            }

        return when (val p = phase) {
            is Phase.AwaitWelcome -> {
                val groupKey = msg.welcome?.group_key ?: fail(ClientException.Protocol("expected welcome"))
                if (groupKey.size != Crypto.KEY_LEN) fail(ClientException.Protocol("invalid group key"))
                reassembler.reset(Framing.CHANNEL_BROADCAST)
                phase = Phase.Ready(p.session, p.sealer, p.opener, BroadcastOpener(groupKey.toByteArray()))
                listOf(ClientEvent.Authenticated(p.session, p.serverName))
            }

            is Phase.Ready -> {
                messageEvent(msg)
            }

            else -> {
                emptyList()
            }
        }
    }

    private fun onBroadcast(message: ByteArray): List<ClientEvent> {
        val ready = phase as? Phase.Ready ?: return emptyList()
        // Broadcast frames can be truncated when we started listening mid-message, drop them silently.
        val plaintext =
            try {
                ready.group.open(message)
            } catch (_: DecryptException) {
                return emptyList()
            }
        return try {
            messageEvent(ServerMessage.ADAPTER.decode(plaintext))
        } catch (_: IOException) {
            emptyList()
        }
    }

    private fun messageEvent(msg: ServerMessage): List<ClientEvent> =
        if (msg.state != null || msg.transport != null || msg.gains != null || msg.meters != null || msg.goodbye != null) {
            listOf(ClientEvent.Message(msg))
        } else {
            emptyList()
        }
}
