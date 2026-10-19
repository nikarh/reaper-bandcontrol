package dev.nikarh.bandcontrol.core.protocol

import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel

/** QR code modules, `true` is dark. Has no quiet zone, draw it with a light border of a few modules. */
class QrMatrix(
    val size: Int,
    private val modules: BooleanArray,
) {
    operator fun get(
        x: Int,
        y: Int,
    ) = modules[y * size + x]

    companion object {
        fun encode(text: String): QrMatrix {
            val hints = mapOf(EncodeHintType.MARGIN to 0, EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M)
            val bits = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 0, 0, hints)
            return QrMatrix(bits.width, BooleanArray(bits.width * bits.height) { bits[it % bits.width, it / bits.width] })
        }
    }
}
