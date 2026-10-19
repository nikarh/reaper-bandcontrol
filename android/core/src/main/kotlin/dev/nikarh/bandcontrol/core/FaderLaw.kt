package dev.nikarh.bandcontrol.core

import java.util.Locale
import kotlin.math.log10
import kotlin.math.pow

/**
 * Maps fader positions (0..1) to gain. Silence at 0, -60..-20 dB over the first quarter and -20..+12 dB over the
 * rest, so most of the travel is where levels are actually set.
 */
object FaderLaw {
    const val MAX_DB = 12f
    const val MIN_DB = -60f
    private const val KNEE_DB = -20f
    private const val KNEE_POSITION = 0.25f

    /** Highest gain a fader sets, REAPER's +12 dB. */
    val maxGain = dbToGain(MAX_DB)

    /** Where 0 dB sits. */
    val unityPosition = dbToPosition(0f)

    fun gainToDb(gain: Float): Float = if (gain <= 0f) Float.NEGATIVE_INFINITY else 20f * log10(gain)

    fun dbToGain(db: Float): Float = if (db == Float.NEGATIVE_INFINITY) 0f else 10f.pow(db / 20f)

    fun dbToPosition(db: Float): Float =
        when {
            db == Float.NEGATIVE_INFINITY -> 0f
            db < KNEE_DB -> KNEE_POSITION * (db - MIN_DB) / (KNEE_DB - MIN_DB)
            else -> KNEE_POSITION + (1 - KNEE_POSITION) * (db - KNEE_DB) / (MAX_DB - KNEE_DB)
        }.coerceIn(0f, 1f)

    fun positionToDb(position: Float): Float {
        val p = position.coerceIn(0f, 1f)
        return when {
            p <= 0f -> Float.NEGATIVE_INFINITY
            p < KNEE_POSITION -> MIN_DB + p / KNEE_POSITION * (KNEE_DB - MIN_DB)
            else -> KNEE_DB + (p - KNEE_POSITION) / (1 - KNEE_POSITION) * (MAX_DB - KNEE_DB)
        }
    }

    fun gainToPosition(gain: Float) = dbToPosition(gainToDb(gain))

    fun positionToGain(position: Float) = dbToGain(positionToDb(position))

    /** `-3.2`, `+6.0`, `0.0`, `-inf`, with a decimal point in every locale. */
    fun format(db: Float): String =
        when {
            db == Float.NEGATIVE_INFINITY || db < -150f -> "-inf"
            db >= 0.05f -> "+%.1f".format(Locale.ROOT, db)
            db > -0.05f -> "0.0"
            else -> "%.1f".format(Locale.ROOT, db)
        }
}
