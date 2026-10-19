package dev.nikarh.bandcontrol.core.state

/** The highest level (dBFS) seen per track since its last reset. */
data class PeakHolds(
    val max: Map<Int, Float> = emptyMap(),
) {
    /** Adds a meter frame: dBFS per track, null for silence. */
    fun add(levels: Map<Int, Float?>): PeakHolds {
        var changed = false
        val next = max.toMutableMap()
        for ((track, level) in levels) {
            if (level != null && level > (next[track] ?: Float.NEGATIVE_INFINITY)) {
                next[track] = level
                changed = true
            }
        }
        return if (changed) PeakHolds(next) else this
    }

    fun reset(track: Int) = PeakHolds(max - track)

    fun reset(tracks: Collection<Int>) = PeakHolds(max - tracks.toSet())

    /** Reached full scale. */
    fun clipped(track: Int) = (max[track] ?: Float.NEGATIVE_INFINITY) >= CLIP_DB

    companion object {
        const val CLIP_DB = 0f
    }
}
