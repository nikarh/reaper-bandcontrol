package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.Receive
import dev.nikarh.bandcontrol.proto.ServerMessage
import dev.nikarh.bandcontrol.proto.State
import dev.nikarh.bandcontrol.proto.Track
import org.junit.Assert.assertEquals
import org.junit.Test

class GroupsTest {
    private fun track(
        id: Int,
        name: String,
        parent: Int? = null,
        output: Boolean = false,
    ) = Track(
        id = id,
        number = id,
        name = name,
        parent = parent,
        output = output,
        receives = if (output) listOf(Receive(src = 2)) else emptyList(),
    )

    private fun groups(vararg tracks: Track) = BandState().apply(ServerMessage(state = State(tracks = tracks.toList())), 0).groups()

    private fun List<Track>.ids() = map { it.id }

    @Test
    fun bandLayout() {
        val g =
            groups(
                track(1, "Click"),
                track(2, "INPUTS"),
                track(3, "Drums input", parent = 2),
                track(4, "VOX input", parent = 2),
                track(5, "VOX effects", parent = 4),
                track(6, "IEM VOCALS"),
                track(7, "IEM VOX", parent = 6),
                track(8, "PA OUT"),
                track(9, "PA Bass", parent = 8, output = true),
                track(10, "PA R GTR", parent = 8, output = true),
                track(11, "PA R GTR effects", parent = 10),
                track(12, "IEM OUT"),
                track(13, "Drums", parent = 12, output = true),
                track(14, "Left guitar", parent = 12, output = true),
                track(15, "Spare mix", output = true),
            )
        // Nested tracks are not inputs or outputs, outputs outside the IEM folder are not mixes.
        assertEquals(listOf(3, 4), g.inputs.ids())
        assertEquals(listOf(9, 10), g.fohOutputs.ids())
        assertEquals(listOf(13, 14), g.iemOutputs.ids())
    }

    @Test
    fun fohFoldersByEitherName() {
        for (name in listOf("FOH OUT", "To FOH", "foh", "PA OUT", "To PA")) {
            val g = groups(track(1, name), track(2, "Feed", parent = 1, output = true))
            assertEquals(name, listOf(2), g.fohOutputs.ids())
            assertEquals(name, emptyList<Int>(), g.iemOutputs.ids())
        }
        // Whole words only.
        for (name in listOf("PAD", "Spare", "Paths")) {
            val g = groups(track(1, name), track(2, "Feed", parent = 1, output = true))
            assertEquals(name, emptyList<Int>(), g.fohOutputs.ids())
        }
    }

    @Test
    fun motherProjectLayout() {
        val g =
            groups(
                track(2, "Click"),
                track(6, "Inputs"),
                track(7, "Drum mic [1]", parent = 6),
                track(8, "Bass [3]", parent = 6),
                track(11, "Outputs"),
                track(12, "PA [1+2]", parent = 11, output = true),
                track(13, "Drums [3+4]", parent = 11, output = true),
            )
        assertEquals(listOf(7, 8), g.inputs.ids())
        assertEquals(emptyList<Int>(), g.fohOutputs.ids())
        assertEquals(listOf(12, 13), g.iemOutputs.ids())
    }

    @Test
    fun noFolders() {
        val g = groups(track(1, "Input"), track(2, "Mix", output = true))
        assertEquals(emptyList<Int>(), g.inputs.ids())
        assertEquals(listOf(2), g.iemOutputs.ids())
    }

    @Test
    fun peakHolds() {
        var p = PeakHolds().add(mapOf(1 to -6f, 2 to null))
        p = p.add(mapOf(1 to -12f, 2 to 0f))
        assertEquals(mapOf(1 to -6f, 2 to 0f), p.max)
        assertEquals(false, p.clipped(1))
        assertEquals(true, p.clipped(2))
        assertEquals(mapOf(1 to -6f), p.reset(2).max)
        assertEquals(emptyMap<Int, Float>(), p.reset(listOf(1, 2)).max)
    }
}
