package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.PlayState
import dev.nikarh.bandcontrol.proto.ServerMessage
import dev.nikarh.bandcontrol.proto.Transport
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

class SnapshotTest {
    private val playing =
        Fixtures.state().apply(
            ServerMessage(transport = Transport(state = PlayState.PLAY_STATE_PLAYING, pos_ms = 40_000, repeat = true, recording = true)),
            1_000,
        )

    @Test
    fun keepsTheProjectAndPositionButNotThePlayState() {
        val restored = snapshotOf(playing.toSnapshot())!!
        assertEquals(playing.tracks, restored.tracks)
        assertEquals(playing.regions, restored.regions)
        assertEquals(playing.markers, restored.markers)
        assertEquals(playing.regionOrder, restored.regionOrder)
        assertEquals(Transport(state = PlayState.PLAY_STATE_STOPPED, pos_ms = 40_000, repeat = true), restored.transport)
        assertEquals(true, restored.loaded)
    }

    @Test
    fun otherTextIsNoSnapshot() {
        assertNull(snapshotOf(null))
        assertNull(snapshotOf(""))
        assertNull(snapshotOf("2:AA:AA"))
        assertNull(snapshotOf("1:not base64:!"))
    }

    @Test
    fun stoppedFreezesThePositionWhereItWasLastSeen() {
        val stopped = playing.stopped(nowMs = 2_500)
        assertEquals(PlayState.PLAY_STATE_STOPPED, stopped.transport.state)
        assertEquals(false, stopped.transport.recording)
        assertEquals(41_500L, stopped.positionMs(10_000))
        val idle = Fixtures.state()
        assertSame(idle, idle.stopped(5_000))
    }
}
