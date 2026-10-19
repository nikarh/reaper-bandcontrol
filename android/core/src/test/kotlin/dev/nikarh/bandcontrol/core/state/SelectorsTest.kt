package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.Marker
import dev.nikarh.bandcontrol.proto.Region
import dev.nikarh.bandcontrol.proto.RegionOrder
import dev.nikarh.bandcontrol.proto.ServerMessage
import dev.nikarh.bandcontrol.proto.State
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SelectorsTest {
    private val state = Fixtures.state()

    /** `(region id, marker id or null)` of a navigation target. */
    private fun NavTarget?.ids() = this?.let { it.region.id to it.marker?.id }

    @Test
    fun regionsFollowUserOrderAndUnorderedAreAppended() {
        assertEquals(listOf(1, 4, 3, 2), state.sortedRegions().map { it.region.id })
        val partial = Fixtures.state(listOf(RegionOrder(id = 3), RegionOrder(id = 1, disabled = true)))
        assertEquals(listOf(3, 1, 2, 4), partial.sortedRegions().map { it.region.id })
        assertEquals(listOf(3, 2, 4), partial.enabledRegions().map { it.id })
    }

    @Test
    fun currentRegionAndMarker() {
        assertEquals(1, state.currentRegion(50_000)?.id)
        assertEquals(2, state.currentMarker(50_000)?.id)
        assertEquals(2, state.currentMarker(33_391)?.id)
        // Whole second tolerance at region bounds, like the web UI.
        assertEquals(1, state.currentRegion(250_900)?.id)
        assertNull(state.currentRegion(300_000))
        assertNull(state.currentMarker(300_000))
    }

    @Test
    fun navigationInsideRegion() {
        assertEquals(1 to 2, state.previousTarget(50_000).ids())
        assertEquals(1 to 3, state.nextTarget(50_000).ids())
        // Standing on a marker goes to the one before it.
        assertEquals(1 to 1, state.previousTarget(33_391).ids())
        // A region without a marker at its start gets a synthetic start target.
        assertEquals(2 to null, state.previousTarget(340_000).ids())
        assertEquals(333_913, state.previousTarget(340_000)?.posMs)
        assertEquals(2 to 5, state.nextTarget(400_000).ids())
    }

    @Test
    fun navigationAcrossRegionsFollowsUserOrder() {
        // Summer -> Spring (order 1, 4, 3, 2), Spring has no markers: its start.
        assertEquals(4 to null, state.nextTarget(100_000).ids())
        // Winter starts with marker 6; previous region in order is Spring.
        assertEquals(4 to null, state.previousTarget(684_600).ids())
        // Autumn is last.
        assertNull(state.nextTarget(500_000))
        assertNull(state.nextTarget(300_000))
    }

    @Test
    fun disabledRegionsAreSkipped() {
        val s = Fixtures.state(listOf(1, 4, 3, 2).map { RegionOrder(id = it, disabled = it == 4) })
        assertEquals(3 to 6, s.nextTarget(100_000).ids())
    }

    @Test
    fun regionEditing() {
        val sorted = state.sortedRegions()
        assertEquals(listOf(4, 1, 3, 2), moveRegion(sorted, 1, -1).map { it.id })
        assertEquals(listOf(1, 3, 4, 2), moveRegion(sorted, 1, 1).map { it.id })
        assertEquals(listOf(1, 4, 3, 2), moveRegion(sorted, 0, -1).map { it.id })
        assertEquals(listOf(false, false, true, false), toggleRegion(sorted, 2).map { it.disabled })
    }

    @Test
    fun outputsAndReceives() {
        val output = state.outputs().single()
        assertEquals(13, output.id)
        val rows = state.receivesOf(output)
        assertEquals(listOf("Click", "Drum mic [1]"), rows.map { it.source.name })
        assertEquals(listOf(false, true), rows.map { it.mute })
    }

    @Test
    fun songsThatFollowEachOtherDirectly() {
        val regions =
            listOf(
                Region(id = 5, name = "Copper Sky", start_ms = 462_077, end_ms = 572_077),
                Region(id = 6, name = "Static Bloom", start_ms = 572_077, end_ms = 690_141),
            )
        val markers =
            listOf(
                Marker(id = 39, name = "Outro", pos_ms = 560_000),
                Marker(id = 40, name = "Intro", pos_ms = 572_077),
                Marker(id = 41, name = "Chorus", pos_ms = 587_560),
            )
        val s =
            BandState().apply(
                ServerMessage(
                    state = State(regions = regions, markers = markers, region_order = listOf(5, 6).map { RegionOrder(id = it) }),
                ),
                0,
            )
        assertEquals("Copper Sky", s.currentRegion(572_000)?.name)
        assertEquals("Static Bloom", s.currentRegion(572_077)?.name)
        assertEquals("Static Bloom", s.currentRegion(572_500)?.name)
        assertEquals(listOf(39), s.markersIn(regions[0]).map { it.id })
        assertEquals(listOf(40, 41), s.markersIn(regions[1]).map { it.id })
        assertEquals("Intro", s.currentMarker(572_077)?.name)
        // From the outro of the first song, next goes to the start of the second.
        assertEquals(572_077, s.nextTarget(565_000)?.posMs)
        assertEquals(6, s.nextTarget(565_000)?.region?.id)
    }
}
