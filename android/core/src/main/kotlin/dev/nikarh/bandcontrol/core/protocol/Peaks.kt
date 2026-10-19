package dev.nikarh.bandcontrol.core.protocol

/** One byte peak levels of `Meters`: 0 is silence (below -60 dBFS), otherwise half dB steps from -60 dBFS. */
object Peaks {
    const val FLOOR_DB = -60f

    /** dBFS of an encoded peak, null for silence. */
    fun decode(byte: Int): Float? = if (byte and 0xff == 0) null else ((byte and 0xff) - 1) / 2f + FLOOR_DB
}
