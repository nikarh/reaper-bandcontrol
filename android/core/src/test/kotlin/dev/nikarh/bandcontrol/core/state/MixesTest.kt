package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.core.Commands
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

class ListeningTest {
    // Two mixes of the same sources: Drums [3+4] (13) and Bass (14), which also hears a bass DI (8).
    private val state =
        Fixtures.state().copy(
            tracks =
                Fixtures.tracks +
                    Track(id = 8, number = 8, name = "Bass DI", volume = 1f) +
                    Track(
                        id = 14,
                        number = 14,
                        name = "Bass",
                        volume = 0.8f,
                        output = true,
                        receives = listOf(Receive(src = 8, volume = 1f), Receive(src = 7, volume = 0.5f), Receive(src = 2, volume = 0.5f)),
                    ),
        )
    private val listening = Listening(mine = "Bass", to = "Drums [3+4]", before = state.levelsOf(state.track(14)!!, withVolume = false))

    @Test
    fun myMixTakesTheirBalanceAndMutesWhatTheyDontHear() {
        assertEquals(
            listOf(
                SetReceive(track = 14, index = 0, src = 8, mute = true),
                SetReceive(track = 14, index = 1, src = 7, volume = 0.36f, mute = true),
                SetReceive(track = 14, index = 2, src = 2, volume = 0.99f),
            ),
            state.listenCommands(listening)!!.map { it.set_receive },
        )
        assertEquals(null, state.listenCommands(listening.copy(to = "Gone")))
    }

    @Test
    fun theirChangesAreMirroredToMyMix() {
        val change = Commands.receiveVolume(13, 1, 7, 0.7f)
        assertEquals(SetReceive(track = 14, index = 1, src = 7, volume = 0.7f), state.mirror(change, listening)?.set_receive)
        // Changes to other mixes, or stale ones, aren't.
        assertEquals(null, state.mirror(Commands.receiveVolume(14, 1, 7, 0.7f), listening))
        assertEquals(null, state.mirror(Commands.receiveVolume(13, 0, 7, 0.7f), listening))
        assertEquals(null, state.mirror(Commands.trackVolume(13, 0.7f), listening))
    }
}
