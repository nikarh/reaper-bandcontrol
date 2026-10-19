package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.GainDelta
import dev.nikarh.bandcontrol.proto.Marker
import dev.nikarh.bandcontrol.proto.PlayState
import dev.nikarh.bandcontrol.proto.Region
import dev.nikarh.bandcontrol.proto.RegionOrder
import dev.nikarh.bandcontrol.proto.ServerMessage
import dev.nikarh.bandcontrol.proto.State
import dev.nikarh.bandcontrol.proto.Track
import dev.nikarh.bandcontrol.proto.Transport

/** Everything a phone knows about the project, built from server messages. */
data class BandState(
    val tracks: List<Track> = emptyList(),
    val regions: List<Region> = emptyList(),
    val markers: List<Marker> = emptyList(),
    val regionOrder: List<RegionOrder> = emptyList(),
    val transport: Transport = Transport(),
    /** Monotonic time (ms) when [transport] was received, to extrapolate the play position. */
    val transportAt: Long = 0,
    val loaded: Boolean = false,
) {
    val playing get() = transport.state == PlayState.PLAY_STATE_PLAYING

    /** Current play position, extrapolated between server updates while playing. */
    fun positionMs(nowMs: Long): Long =
        transport.pos_ms.toLong() + if (playing) (nowMs - transportAt).coerceIn(0, MAX_EXTRAPOLATION_MS) else 0

    fun track(id: Int) = tracks.find { it.id == id }

    fun apply(
        message: ServerMessage,
        nowMs: Long,
    ): BandState {
        message.state?.let { return withState(it) }
        message.transport?.let { return copy(transport = it, transportAt = nowMs) }
        message.gains?.let { return withGains(it) }
        return this
    }

    private fun withState(s: State) =
        copy(tracks = s.tracks, regions = s.regions, markers = s.markers, regionOrder = s.region_order, loaded = true)

    private fun withGains(g: GainDelta): BandState {
        if (g.tracks.isEmpty() && g.receives.isEmpty()) return this
        val volumes = g.tracks.associate { it.track to it.volume }
        val receives = g.receives.groupBy { it.track }
        return copy(
            tracks =
                tracks.map { t ->
                    val changes = receives[t.id]
                    if (t.id !in volumes && changes == null) return@map t
                    t.copy(
                        volume = volumes[t.id] ?: t.volume,
                        receives =
                            if (changes == null) {
                                t.receives
                            } else {
                                t.receives.mapIndexed { i, r ->
                                    changes.lastOrNull { it.index == i }?.let { r.copy(volume = it.volume, mute = it.mute) } ?: r
                                }
                            },
                    )
                },
        )
    }

    companion object {
        /** Stop extrapolating when updates stop arriving (they come every second while playing). */
        const val MAX_EXTRAPOLATION_MS = 3_000L
    }
}
