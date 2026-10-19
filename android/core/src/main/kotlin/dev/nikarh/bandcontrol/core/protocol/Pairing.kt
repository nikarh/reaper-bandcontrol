package dev.nikarh.bandcontrol.core.protocol

import java.io.ByteArrayOutputStream
import java.security.SecureRandom

/** The shared secret every paired phone knows, see `crates/bandcontrol-proto/src/pairing.rs`. */
class PairingSecret(
    bytes: ByteArray,
) {
    val bytes: ByteArray = bytes.copyOf()

    init {
        require(bytes.size == LEN) { "pairing secret must be $LEN bytes" }
    }

    /** 26 characters of RFC 4648 base32. */
    fun toCode(): String = Base32.encode(bytes)

    /** Groups of 4 characters separated by dashes. */
    fun toDisplayCode(): String = toCode().chunked(4).joinToString("-")

    override fun equals(other: Any?) = other is PairingSecret && other.bytes.contentEquals(bytes)

    override fun hashCode() = bytes.contentHashCode()

    override fun toString() = "PairingSecret(..)"

    companion object {
        const val LEN = 16

        /** Parses a code, ignoring case, whitespace and dashes. Returns null when invalid. */
        fun fromCode(code: String): PairingSecret? {
            val normalized = code.filter { it.isLetterOrDigit() && it.code < 128 }.uppercase()
            return Base32.decode(normalized)?.takeIf { it.size == LEN }?.let(::PairingSecret)
        }

        fun generate(random: SecureRandom = SecureRandom()) = PairingSecret(ByteArray(LEN).also(random::nextBytes))
    }
}

/**
 * Contents of the pairing QR code: `bandcontrol://pair?s=<code>&n=<name>`.
 *
 * [tcp] (`t=host:port`) is a development extension for connecting over TCP instead of Bluetooth.
 */
data class PairingInfo(
    val secret: PairingSecret,
    val name: String,
    val tcp: String? = null,
) {
    fun toUrl(): String =
        buildString {
            append(PREFIX).append("s=").append(secret.toCode()).append("&n=").append(percentEncode(name))
            if (tcp != null) append("&t=").append(percentEncode(tcp))
        }

    companion object {
        private const val PREFIX = "bandcontrol://pair?"

        fun fromUrl(url: String): PairingInfo? {
            if (!url.startsWith(PREFIX)) return null
            var secret: PairingSecret? = null
            var name = ""
            var tcp: String? = null
            for (pair in url.removePrefix(PREFIX).split('&')) {
                val key = pair.substringBefore('=')
                val value = pair.substringAfter('=', "")
                when (key) {
                    "s" -> secret = PairingSecret.fromCode(value) ?: return null
                    "n" -> name = percentDecode(value) ?: return null
                    "t" -> tcp = percentDecode(value) ?: return null
                }
            }
            return secret?.let { PairingInfo(it, name, tcp) }
        }

        private fun percentEncode(s: String): String =
            buildString {
                for (b in s.toByteArray()) {
                    val c = b.toInt() and 0xff
                    if (c < 128 && (c.toChar().isLetterOrDigit() || c.toChar() in "-._~")) {
                        append(c.toChar())
                    } else {
                        append('%').append("%02X".format(c))
                    }
                }
            }

        private fun percentDecode(s: String): String? {
            val out = ByteArrayOutputStream()
            var i = 0
            while (i < s.length) {
                when (val c = s[i]) {
                    '%' -> {
                        val hex = s.substring(i + 1, minOf(i + 3, s.length))
                        if (hex.length != 2) return null
                        out.write(hex.toIntOrNull(16) ?: return null)
                        i += 2
                    }
                    '+' -> out.write(' '.code)
                    else -> out.write(c.toString().toByteArray())
                }
                i++
            }
            return runCatching {
                Charsets.UTF_8
                    .newDecoder()
                    .decode(java.nio.ByteBuffer.wrap(out.toByteArray()))
                    .toString()
            }.getOrNull()
        }
    }
}

internal object Base32 {
    private const val ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"

    fun encode(data: ByteArray): String =
        buildString {
            var buffer = 0
            var bits = 0
            for (b in data) {
                buffer = (buffer shl 8) or (b.toInt() and 0xff)
                bits += 8
                while (bits >= 5) {
                    append(ALPHABET[(buffer shr (bits - 5)) and 31])
                    bits -= 5
                }
            }
            if (bits > 0) append(ALPHABET[(buffer shl (5 - bits)) and 31])
        }

    /** Strict decoding without padding: trailing bits must be zero. */
    fun decode(s: String): ByteArray? {
        val out = ByteArrayOutputStream()
        var buffer = 0
        var bits = 0
        for (c in s) {
            val v = ALPHABET.indexOf(c)
            if (v < 0) return null
            buffer = ((buffer shl 5) or v) and 0xfff
            bits += 5
            if (bits >= 8) {
                out.write((buffer shr (bits - 8)) and 0xff)
                bits -= 8
            }
        }
        if (bits >= 5 || (buffer and ((1 shl bits) - 1)) != 0) return null
        return out.toByteArray()
    }
}
