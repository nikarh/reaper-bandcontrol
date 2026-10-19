package dev.nikarh.bandcontrol.core.protocol

import java.nio.ByteBuffer
import java.security.GeneralSecurityException
import java.security.MessageDigest
import java.security.NoSuchAlgorithmException
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.Mac
import javax.crypto.spec.IvParameterSpec
import javax.crypto.spec.SecretKeySpec

/** Key derivation, proofs and AEAD, see `crates/bandcontrol-proto/src/crypto.rs`. */
object Crypto {
    const val NONCE_LEN = 16
    const val KEY_LEN = 32
    private val SALT = "bandcontrol/v1".toByteArray()

    fun hmac(
        key: ByteArray,
        vararg parts: ByteArray,
    ): ByteArray {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(key, "HmacSHA256"))
        parts.forEach(mac::update)
        return mac.doFinal()
    }

    /** HKDF-SHA256 with a single expand block (at most 32 bytes of output). */
    fun hkdf(
        ikm: ByteArray,
        salt: ByteArray,
        info: ByteArray,
        length: Int = KEY_LEN,
    ): ByteArray {
        require(length <= KEY_LEN)
        val prk = hmac(salt, ikm)
        return hmac(prk, info, byteArrayOf(1)).copyOf(length)
    }

    fun serviceUuid(secret: PairingSecret): UUID {
        val b = hkdf(secret.bytes, SALT, "service-uuid".toByteArray(), 16)
        b[6] = ((b[6].toInt() and 0x0f) or 0x80).toByte()
        b[8] = ((b[8].toInt() and 0x3f) or 0x80).toByte()
        val buffer = ByteBuffer.wrap(b)
        return UUID(buffer.long, buffer.long)
    }

    internal fun authKey(secret: PairingSecret): ByteArray = hkdf(secret.bytes, SALT, "auth".toByteArray())

    internal fun nonce(counter: Long): ByteArray {
        val n = ByteArray(12)
        for (i in 0 until 8) n[i] = (counter ushr (8 * i)).toByte()
        return n
    }

    internal fun aead(
        mode: Int,
        key: ByteArray,
        counter: Long,
        data: ByteArray,
    ): ByteArray {
        val cipher =
            try {
                Cipher.getInstance("ChaCha20-Poly1305")
            } catch (_: NoSuchAlgorithmException) {
                // Android (Conscrypt) name.
                Cipher.getInstance("ChaCha20/Poly1305/NoPadding")
            }
        cipher.init(mode, SecretKeySpec(key, "ChaCha20"), IvParameterSpec(nonce(counter)))
        return cipher.doFinal(data)
    }
}

class AuthKey(
    secret: PairingSecret,
) {
    private val key = Crypto.authKey(secret)

    private fun proof(
        role: String,
        clientNonce: ByteArray,
        serverNonce: ByteArray,
        session: Int,
    ) = Crypto.hmac(key, role.toByteArray(), clientNonce, serverNonce, byteArrayOf(session.toByte()))

    fun serverProof(
        clientNonce: ByteArray,
        serverNonce: ByteArray,
        session: Int,
    ) = proof("server", clientNonce, serverNonce, session)

    fun clientProof(
        clientNonce: ByteArray,
        serverNonce: ByteArray,
        session: Int,
    ) = proof("client", clientNonce, serverNonce, session)

    fun verifyServerProof(
        clientNonce: ByteArray,
        serverNonce: ByteArray,
        session: Int,
        proof: ByteArray,
    ) = MessageDigest.isEqual(serverProof(clientNonce, serverNonce, session), proof)

    /** Directional session keys `(serverToClient, clientToServer)`. */
    fun sessionKeys(
        clientNonce: ByteArray,
        serverNonce: ByteArray,
    ): Pair<ByteArray, ByteArray> {
        val salt = clientNonce + serverNonce
        return Crypto.hkdf(key, salt, "s2c".toByteArray()) to Crypto.hkdf(key, salt, "c2s".toByteArray())
    }
}

class DecryptException(
    message: String,
    cause: Throwable? = null,
) : Exception(message, cause)

/** Seals unicast messages with an implicit counter. */
class Sealer(
    private val key: ByteArray,
) {
    private var counter = 0L

    fun seal(plaintext: ByteArray): ByteArray = Crypto.aead(Cipher.ENCRYPT_MODE, key, counter++, plaintext)
}

/** Opens unicast messages with an implicit counter. A failed open does not advance the counter. */
class Opener(
    private val key: ByteArray,
) {
    private var counter = 0L

    fun open(ciphertext: ByteArray): ByteArray {
        val plaintext =
            try {
                Crypto.aead(Cipher.DECRYPT_MODE, key, counter, ciphertext)
            } catch (e: GeneralSecurityException) {
                throw DecryptException("unicast frame failed authentication", e)
            }
        counter++
        return plaintext
    }
}

/** Opens broadcast frames `leb128(counter) || ciphertext`, rejecting counters that don't increase. */
class BroadcastOpener(
    private val key: ByteArray,
) {
    private var next = 0L

    fun open(frame: ByteArray): ByteArray {
        var counter = 0L
        var shift = 0
        var offset = 0
        while (true) {
            if (offset >= frame.size || shift > 63) throw DecryptException("malformed broadcast frame")
            val b = frame[offset++].toInt() and 0xff
            counter = counter or ((b and 0x7f).toLong() shl shift)
            shift += 7
            if (b and 0x80 == 0) break
        }
        if (java.lang.Long.compareUnsigned(counter, next) < 0) throw DecryptException("replayed broadcast frame")
        val plaintext =
            try {
                Crypto.aead(Cipher.DECRYPT_MODE, key, counter, frame.copyOfRange(offset, frame.size))
            } catch (e: GeneralSecurityException) {
                throw DecryptException("broadcast frame failed authentication", e)
            }
        next = counter + 1
        return plaintext
    }
}
