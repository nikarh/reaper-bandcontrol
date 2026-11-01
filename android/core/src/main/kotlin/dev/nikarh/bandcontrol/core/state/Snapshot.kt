package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.PlayState
import dev.nikarh.bandcontrol.proto.State
import dev.nikarh.bandcontrol.proto.Transport
import java.util.Base64

private const val VERSION = "1"

/**
 * The project as last seen, as text for storage: what the app shows while it (re)connects. Holds the structure,
 * levels and play position; meters and the play state are live only.
 */
fun BandState.toSnapshot(): String {
    val encoder = Base64.getEncoder()
    val state = State(tracks = tracks, regions = regions, markers = markers, region_order = regionOrder)
    val transport = Transport(pos_ms = transport.pos_ms, repeat = transport.repeat)
    return listOf(VERSION, encoder.encodeToString(State.ADAPTER.encode(state)), encoder.encodeToString(Transport.ADAPTER.encode(transport)))
        .joinToString(":")
}

/** The state stored by [toSnapshot], stopped, or null when [text] isn't one. */
fun snapshotOf(text: String?): BandState? {
    val parts = text?.split(':') ?: return null
    if (parts.size != 3 || parts[0] != VERSION) return null
    return runCatching {
        val decoder = Base64.getDecoder()
        val state = State.ADAPTER.decode(decoder.decode(parts[1]))
        val transport = Transport.ADAPTER.decode(decoder.decode(parts[2]))
        BandState(
            tracks = state.tracks,
            regions = state.regions,
            markers = state.markers,
            regionOrder = state.region_order,
            transport = transport.copy(state = PlayState.PLAY_STATE_STOPPED, recording = false),
            loaded = true,
        )
    }.getOrNull()
}

/** As shown while not connected: nothing plays or records, the position stays where it was last seen. */
fun BandState.stopped(nowMs: Long): BandState =
    if (transport.state == PlayState.PLAY_STATE_STOPPED && !transport.recording) {
        this
    } else {
        copy(
            transport =
                transport.copy(
                    state = PlayState.PLAY_STATE_STOPPED,
                    pos_ms = positionMs(nowMs).toInt(),
                    recording = false,
                ),
            transportAt = nowMs,
        )
    }
