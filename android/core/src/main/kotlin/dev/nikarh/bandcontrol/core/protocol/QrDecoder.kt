package dev.nikarh.bandcontrol.core.protocol

import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader

/** Finds a QR code in a camera frame's luminance (Y) plane, without Google Play services. */
class QrDecoder {
    private val reader = QRCodeReader()
    private val hints = mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE), DecodeHintType.TRY_HARDER to true)

    /** [luminance] holds [height] rows of [rowStride] bytes, of which the first [width] are pixels. */
    fun decode(
        luminance: ByteArray,
        width: Int,
        height: Int,
        rowStride: Int = width,
    ): String? {
        val source = PlanarYUVLuminanceSource(luminance, rowStride, height, 0, 0, width, height, false)
        return try {
            reader.decode(BinaryBitmap(HybridBinarizer(source)), hints).text
        } catch (_: ReaderException) {
            null
        } finally {
            reader.reset()
        }
    }
}
