package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.Receive
import dev.nikarh.bandcontrol.proto.SetReceive
import dev.nikarh.bandcontrol.proto.SetTrackVolume
import dev.nikarh.bandcontrol.proto.Track
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MixesTest {
    private val state = Fixtures.state()
    private val drums = state.track(13)!!

    @Test
    fun levelsAreBySourceName() {
        assertEquals(
            MixLevels(0.42f, listOf(SourceLevel("Click", 0.99f, false), SourceLevel("Drum mic [1]", 0.36f, true))),
            state.levelsOf(drums),
        )
        assertEquals(null, state.levelsOf(drums, withVolume = false).volume)
    }

    @Test
    fun onlyDifferencesAreSent() {
        assertEquals(emptyList<Any>(), state.commandsFor(drums, state.levelsOf(drums)))
        val wanted = MixLevels(0.5f, listOf(SourceLevel("Drum mic [1]", 0.36f, false), SourceLevel("Gone", 1f, false)))
        val commands = state.commandsFor(drums, wanted)
        assertEquals(
            listOf(
                SetTrackVolume(track = 13, volume = 0.5f),
                SetReceive(track = 13, index = 1, src = 7, mute = false),
            ),
            commands.map { it.set_track_volume ?: it.set_receive },
        )
    }

    @Test
    fun missingSourcesAreKeptOrMuted() {
        val onlyDrums = MixLevels(null, listOf(SourceLevel("Drum mic [1]", 0.36f, true)))
        assertEquals(emptyList<Any>(), state.commandsFor(drums, onlyDrums, Missing.Keep))
        assertEquals(
            listOf(SetReceive(track = 13, index = 0, src = 2, mute = true)),
            state.commandsFor(drums, onlyDrums, Missing.Mute).map { it.set_receive },
        )
    }

    @Test
    fun sourcesWithTheSameNameMatchInOrder() {
        val twoMics =
            Fixtures.state().copy(
                tracks =
                    Fixtures.tracks.take(2) +
                        Track(id = 8, number = 8, name = "Drum mic [1]", volume = 1f) +
                        Track(
                            id = 13,
                            name = "Drums",
                            output = true,
                            volume = 1f,
                            receives = listOf(Receive(src = 7, volume = 1f), Receive(src = 8, volume = 1f)),
                        ),
            )
        val mix = twoMics.track(13)!!
        val wanted = MixLevels(null, listOf(SourceLevel("Drum mic [1]", 1f, false), SourceLevel("Drum mic [1]", 0.5f, false)))
        assertEquals(
            listOf(SetReceive(track = 13, index = 1, src = 8, volume = 0.5f)),
            twoMics.commandsFor(mix, wanted).map { it.set_receive },
        )
    }

    @Test
    fun roundingIsNoChange() {
        val a = MixLevels(1f, listOf(SourceLevel("Click", 0.5f, false)))
        assertTrue(a.sameAs(MixLevels(1.0001f, listOf(SourceLevel("Click", 0.50001f, false)))))
        assertFalse(a.sameAs(MixLevels(1.1f, listOf(SourceLevel("Click", 0.5f, false)))))
        assertFalse(a.sameAs(MixLevels(1f, listOf(SourceLevel("Click", 0.5f, true)))))
        // Without a mix level on one side, only the sources count; sources only one side has don't.
        assertTrue(a.sameAs(MixLevels(null, listOf(SourceLevel("Click", 0.5f, false), SourceLevel("Bass", 1f, true)))))
        // Silence is silence.
        assertTrue(
            MixLevels(null, listOf(SourceLevel("Click", 0f, false))).sameAs(MixLevels(null, listOf(SourceLevel("Click", 1e-7f, false)))),
        )
    }
}
