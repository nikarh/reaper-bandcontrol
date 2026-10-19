package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.Marker
import dev.nikarh.bandcontrol.proto.Region
import dev.nikarh.bandcontrol.proto.RegionOrder
import dev.nikarh.bandcontrol.proto.Track
import kotlin.math.abs

/** A region with its user defined order and enabled flag. */
data class OrderedRegion(
    val region: Region,
    val disabled: Boolean,
)

/** Where prev/next buttons go: a marker inside a region, or the region start when [marker] is null. */
data class NavTarget(
    val region: Region,
    val marker: Marker?,
    val posMs: Int,
)

data class ReceiveRow(
    val index: Int,
    val source: Track,
    val volume: Float,
    val mute: Boolean,
)

/** Regions in the user defined order; regions missing from the order are appended by id. */
fun BandState.sortedRegions(): List<OrderedRegion> {
    val order = regionOrder.withIndex().associate { (i, o) -> o.id to (i to o.disabled) }
    return regions
        .sortedBy { order[it.id]?.first ?: (UNORDERED + it.id) }
        .map { OrderedRegion(it, order[it.id]?.second ?: false) }
}

fun BandState.enabledRegions(): List<Region> = sortedRegions().filterNot { it.disabled }.map { it.region }

fun BandState.markersIn(region: Region): List<Marker> = markers.filter { it.pos_ms in region.start_ms..region.end_ms }

/** The region containing [posMs] (with whole second tolerance like the web UI). */
fun BandState.currentRegion(posMs: Long): Region? {
    val t = maxOf(posMs, 0)
    return regions.find { t >= floorSecond(it.start_ms) && t <= ceilSecond(it.end_ms) }
}

/** The last marker of the current region at or before [posMs]. */
fun BandState.currentMarker(posMs: Long): Marker? {
    val region = currentRegion(posMs) ?: return null
    return markersIn(region).lastOrNull { posMs - it.pos_ms > -1 }
}

private fun BandState.navigationTargets(region: Region): List<NavTarget> {
    val markers = markersIn(region).map { NavTarget(region, it, it.pos_ms) }
    val startsWithMarker = markers.firstOrNull()?.let { abs(it.posMs - region.start_ms) < 10 } ?: false
    return if (startsWithMarker) markers else listOf(NavTarget(region, null, region.start_ms)) + markers
}

fun BandState.previousTarget(posMs: Long): NavTarget? {
    val current = currentRegion(posMs) ?: return null
    navigationTargets(current).lastOrNull { posMs - it.posMs > 100 }?.let { return it }
    val regions = enabledRegions()
    val previous = regions.getOrNull(regions.indexOfFirst { it.id == current.id } - 1) ?: return null
    return navigationTargets(previous).lastOrNull()
}

fun BandState.nextTarget(posMs: Long): NavTarget? {
    val current = currentRegion(posMs) ?: return null
    navigationTargets(current).firstOrNull { it.posMs - posMs > 10 }?.let { return it }
    val regions = enabledRegions()
    val index = regions.indexOfFirst { it.id == current.id }
    if (index < 0) return null
    val next = regions.getOrNull(index + 1) ?: return null
    return navigationTargets(next).firstOrNull()
}

fun BandState.outputs(): List<Track> = tracks.filter { it.output }

fun BandState.receivesOf(output: Track): List<ReceiveRow> =
    output.receives.mapIndexedNotNull { i, r -> track(r.src)?.let { ReceiveRow(i, it, r.volume, r.mute) } }

/** New region order after moving the region at [index] (in [sorted]) by [delta] positions. */
fun moveRegion(
    sorted: List<OrderedRegion>,
    index: Int,
    delta: Int,
): List<RegionOrder> {
    val target = index + delta
    if (index !in sorted.indices || target !in sorted.indices) return sorted.toOrder()
    val list = sorted.toMutableList()
    list.add(target, list.removeAt(index))
    return list.toOrder()
}

fun toggleRegion(
    sorted: List<OrderedRegion>,
    index: Int,
): List<RegionOrder> = sorted.mapIndexed { i, r -> if (i == index) r.copy(disabled = !r.disabled) else r }.toOrder()

private fun List<OrderedRegion>.toOrder() = map { RegionOrder(id = it.region.id, disabled = it.disabled) }

private const val UNORDERED = 10_000

private fun floorSecond(ms: Int): Long = ms / 1000 * 1000L

private fun ceilSecond(ms: Int): Long = (ms + 999L) / 1000 * 1000
