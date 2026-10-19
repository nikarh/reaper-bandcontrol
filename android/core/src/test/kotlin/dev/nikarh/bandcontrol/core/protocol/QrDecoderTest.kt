package dev.nikarh.bandcontrol.core.protocol

import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class QrDecoderTest {
    private val url = PairingInfo(PairingSecret(ByteArray(16) { it.toByte() }), "Studio PC").toUrl()

    /** A camera-like frame: the code on a gray background, rows padded to [rowStride]. */
    private fun frame(
        width: Int,
        height: Int,
        rowStride: Int,
    ): ByteArray {
        val code = QRCodeWriter().encode(url, BarcodeFormat.QR_CODE, 300, 300)
        val frame = ByteArray(rowStride * height) { 0x80.toByte() }
        val left = (width - code.width) / 2
        val top = (height - code.height) / 2
        for (y in 0 until code.height) {
            for (x in 0 until code.width) {
                frame[(top + y) * rowStride + left + x] = if (code[x, y]) 20 else 230.toByte()
            }
        }
        return frame
    }

    @Test
    fun decodesPairingUrls() {
        assertEquals(url, QrDecoder().decode(frame(640, 480, 640), 640, 480))
        assertEquals(url, QrDecoder().decode(frame(640, 480, 704), 640, 480, rowStride = 704))
        assertEquals(PairingInfo.fromUrl(url)?.name, "Studio PC")
    }

    @Test
    fun noCode() {
        assertNull(QrDecoder().decode(ByteArray(640 * 480) { (it % 251).toByte() }, 640, 480))
    }
}
