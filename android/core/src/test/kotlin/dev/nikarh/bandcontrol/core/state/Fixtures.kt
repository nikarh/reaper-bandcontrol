package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.Marker
import dev.nikarh.bandcontrol.proto.Receive
import dev.nikarh.bandcontrol.proto.Region
import dev.nikarh.bandcontrol.proto.RegionOrder
import dev.nikarh.bandcontrol.proto.ServerMessage
import dev.nikarh.bandcontrol.proto.State
import dev.nikarh.bandcontrol.proto.Track

/** Regions and markers of `tests/fixtures/Mother Project.RPP`. */
object Fixtures {
    val regions =
        listOf(
            Region(id = 1, name = "Summer", start_ms = 0, end_ms = 250_435),
            Region(id = 2, name = "Autumn", start_ms = 333_913, end_ms = 550_956),
            Region(id = 3, name = "Winter", start_ms = 684_522, end_ms = 1_126_956),
            Region(id = 4, name = "Spring", start_ms = 1_268_869, end_ms = 1_736_347),
        )
    val markers =
        listOf(
            Marker(id = 1, name = "Intro", pos_ms = 0),
            Marker(id = 2, name = "Verse", pos_ms = 33_391),
            Marker(id = 3, name = "Chrous", pos_ms = 83_478),
            Marker(id = 4, name = "Verse", pos_ms = 367_304),
            Marker(id = 5, name = "Chorus", pos_ms = 434_087),
            Marker(id = 6, name = "Start", pos_ms = 684_522),
        )
    val order = listOf(1, 4, 3, 2).map { RegionOrder(id = it) }
    val tracks =
        listOf(
            Track(id = 2, number = 2, name = "Click", volume = 1f),
            Track(id = 7, number = 7, name = "Drum mic [1]", volume = 1f),
            Track(
                id = 13,
                number = 13,
                name = "Drums [3+4]",
                volume = 0.42f,
                output = true,
                receives = listOf(Receive(src = 2, volume = 0.99f), Receive(src = 7, volume = 0.36f, mute = true)),
            ),
        )

    fun state(order: List<RegionOrder> = this.order) =
        BandState().apply(
            ServerMessage(state = State(tracks = tracks, regions = regions, markers = markers, region_order = order)),
            0,
        )
}
