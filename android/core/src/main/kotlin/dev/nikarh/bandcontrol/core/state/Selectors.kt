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

/** How songs are listed and stepped through. */
enum class RegionView {
    /** As they are in the project, by position; every region. */
    Timeline,

    /** The band's order, without the songs switched off. */
    Setlist,
}

/** The songs of [view], in its order. */
fun BandState.regionsIn(view: RegionView): List<Region> =
    when (view) {
        RegionView.Timeline -> regions.sortedWith(compareBy({ it.start_ms }, { it.id }))
        RegionView.Setlist -> enabledRegions()
    }

/** The project has a setlist: an order other than the regions' own, or songs switched off. */
fun BandState.hasSetlist(): Boolean {
    val sorted = sortedRegions()
    return sorted.any { it.disabled } || sorted.map { it.region.id } != regions.map { it.id }.sorted()
}

/** The view to start with when the user never picked one: the setlist when the band made one. */
fun BandState.defaultRegionView(): RegionView = if (hasSetlist()) RegionView.Setlist else RegionView.Timeline

/** Markers from the region start up to (not including) its end, where the next song may already start. */
fun BandState.markersIn(region: Region): List<Marker> = markers.filter { it.pos_ms >= region.start_ms && it.pos_ms < region.end_ms }

/**
 * The region containing [posMs] (with whole second tolerance like the web UI). Where songs follow each other directly
 * the tolerance overlaps, then the song that has already started wins.
 */
fun BandState.currentRegion(posMs: Long): Region? {
    val t = maxOf(posMs, 0)
    val candidates = regions.filter { t >= floorSecond(it.start_ms) && t <= ceilSecond(it.end_ms) }
    return candidates.filter { it.start_ms <= t }.maxByOrNull { it.start_ms } ?: candidates.firstOrNull()
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

/** While playing, "previous" this soon after a song or part started goes to the one before it, like music players. */
const val PREVIOUS_GRACE_MS = 2_000L

/**
 * Where "previous" goes from [posMs]: the start of the current song or part, or the one before it within [graceMs] of
 * that start.
 */
fun BandState.previousTarget(
    posMs: Long,
    graceMs: Long = 100,
    view: RegionView = RegionView.Setlist,
): NavTarget? {
    val current = currentRegion(posMs) ?: return null
    navigationTargets(current).lastOrNull { posMs - it.posMs > graceMs }?.let { return it }
    val regions = regionsIn(view)
    val previous = regions.getOrNull(regions.indexOfFirst { it.id == current.id } - 1) ?: return null
    return navigationTargets(previous).lastOrNull()
}

fun BandState.nextTarget(
    posMs: Long,
    view: RegionView = RegionView.Setlist,
): NavTarget? {
    val current = currentRegion(posMs) ?: return null
    navigationTargets(current).firstOrNull { it.posMs - posMs > 10 }?.let { return it }
    val regions = regionsIn(view)
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

/** The order to send for an edited list, e.g. after dragging. */
fun List<OrderedRegion>.toOrder(): List<RegionOrder> = map { RegionOrder(id = it.region.id, disabled = it.disabled) }

private const val UNORDERED = 10_000

private fun floorSecond(ms: Int): Long = ms / 1000 * 1000L

private fun ceilSecond(ms: Int): Long = (ms + 999L) / 1000 * 1000
