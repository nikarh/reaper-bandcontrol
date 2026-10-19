package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.GainDelta
import dev.nikarh.bandcontrol.proto.PlayState
import dev.nikarh.bandcontrol.proto.ReceiveGain
import dev.nikarh.bandcontrol.proto.ServerMessage
import dev.nikarh.bandcontrol.proto.TrackGain
import dev.nikarh.bandcontrol.proto.Transport
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class BandStateTest {
    @Test
    fun gainDeltasUpdateOnlyTheirTargets() {
        val before = Fixtures.state()
        val after =
            before.apply(
                ServerMessage(
                    gains =
                        GainDelta(
                            tracks = listOf(TrackGain(track = 13, volume = 0.5f)),
                            receives = listOf(ReceiveGain(track = 13, index = 1, volume = 0.2f, mute = false)),
                        ),
                ),
                0,
            )
        val drums = after.track(13)!!
        assertEquals(0.5f, drums.volume)
        assertEquals(0.99f, drums.receives[0].volume)
        assertEquals(0.2f, drums.receives[1].volume)
        assertFalse(drums.receives[1].mute)
        assertSame(before.track(2), after.track(2))
    }

    @Test
    fun gainsForUnknownTracksAreIgnored() {
        val before = Fixtures.state()
        val after =
            before.apply(
                ServerMessage(gains = GainDelta(receives = listOf(ReceiveGain(track = 99, index = 5, volume = 1f)))),
                0,
            )
        assertEquals(before, after)
    }

    @Test
    fun positionIsExtrapolatedWhilePlaying() {
        val playing =
            Fixtures.state().apply(
                ServerMessage(transport = Transport(state = PlayState.PLAY_STATE_PLAYING, pos_ms = 10_000)),
                1_000,
            )
        assertTrue(playing.loaded)
        assertEquals(10_500, playing.positionMs(1_500))
        assertEquals(10_000 + BandState.MAX_EXTRAPOLATION_MS, playing.positionMs(60_000))
        val stopped = playing.apply(ServerMessage(transport = Transport(pos_ms = 20_000)), 2_000)
        assertEquals(20_000, stopped.positionMs(9_000))
    }
}
