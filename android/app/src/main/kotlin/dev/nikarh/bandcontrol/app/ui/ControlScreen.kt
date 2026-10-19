package dev.nikarh.bandcontrol.app.ui

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.FiberManualRecord
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.nikarh.bandcontrol.core.Commands
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.core.state.NavTarget
import dev.nikarh.bandcontrol.core.state.OrderedRegion
import dev.nikarh.bandcontrol.core.state.currentMarker
import dev.nikarh.bandcontrol.core.state.currentRegion
import dev.nikarh.bandcontrol.core.state.enabledRegions
import dev.nikarh.bandcontrol.core.state.markersIn
import dev.nikarh.bandcontrol.core.state.moveRegion
import dev.nikarh.bandcontrol.core.state.nextTarget
import dev.nikarh.bandcontrol.core.state.previousTarget
import dev.nikarh.bandcontrol.core.state.sortedRegions
import dev.nikarh.bandcontrol.core.state.toOrder
import dev.nikarh.bandcontrol.core.state.toggleRegion
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.Marker
import dev.nikarh.bandcontrol.proto.PlayState
import dev.nikarh.bandcontrol.proto.Region
import dev.nikarh.bandcontrol.proto.RegionOrder
import kotlinx.coroutines.delay
import sh.calvin.reorderable.ReorderableCollectionItemScope
import sh.calvin.reorderable.ReorderableItem
import sh.calvin.reorderable.rememberReorderableLazyListState

/** Longer than the server's 1 s structure debounce plus Bluetooth latency. */
private const val LOCAL_ORDER_TIMEOUT_MS = 3_000L

private fun monotonicMs() = System.nanoTime() / 1_000_000

/** The extrapolated play position, refreshed while playing. */
@Composable
private fun rememberPosition(state: BandState): Long {
    var now by remember { mutableLongStateOf(monotonicMs()) }
    LaunchedEffect(state.transport, state.transportAt) {
        now = monotonicMs()
        while (state.playing) {
            delay(100)
            now = monotonicMs()
        }
    }
    return state.positionMs(now)
}

@Composable
fun ControlScreen(
    state: BandState,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    // Region order changes come back after the server's debounce: show them immediately. The server sends nothing
    // when the order is changed back in REAPER within the debounce, so an unconfirmed local order expires.
    var localOrder by remember(state.regionOrder) { mutableStateOf<List<RegionOrder>?>(null) }
    LaunchedEffect(localOrder) {
        if (localOrder != null) {
            delay(LOCAL_ORDER_TIMEOUT_MS)
            localOrder = null
        }
    }
    val view = localOrder?.let { state.copy(regionOrder = it) } ?: state
    val onOrder = { order: List<RegionOrder> -> if (send(Commands.regionOrder(order))) localOrder = order }
    val position = rememberPosition(state)
    var editing by rememberSaveable { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        Column(
            Modifier
                .fillMaxWidth()
                .background(Palette.gray800)
                .padding(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            TransportRow(state, enabled, send)
            NavigationRow(view, position, enabled, send)
        }
        Row(Modifier.padding(horizontal = 12.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                if (editing) "Edit setlist" else "Move to Region",
                color = Color.White,
                fontWeight = FontWeight.ExtraBold,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = { editing = !editing }, modifier = Modifier.testTag("edit-regions")) {
                if (!editing) {
                    Icon(Icons.Default.Edit, contentDescription = null, tint = Palette.blue500, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                }
                Text(if (editing) "Done" else "Edit", color = Palette.blue500, fontWeight = FontWeight.SemiBold)
            }
        }
        if (editing) {
            Text(
                "Drag to reorder, switch off to skip a song",
                color = Palette.gray400,
                fontSize = 13.sp,
                modifier = Modifier.padding(start = 12.dp, end = 12.dp, bottom = 4.dp),
            )
        }
        if (editing) RegionEditor(view, enabled, onOrder) else RegionList(view, position, enabled, send)
    }
}

@Composable
private fun TransportButton(
    icon: ImageVector,
    description: String,
    selected: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    shape: RoundedCornerShape,
    modifier: Modifier = Modifier,
    selectedColor: Color = Palette.gray500,
) {
    Surface(
        onClick = onClick,
        enabled = enabled,
        shape = shape,
        color =
            when {
                !selected -> Palette.gray700
                enabled -> selectedColor
                else -> Palette.gray600
            },
        modifier =
            modifier
                .height(44.dp)
                .semantics { this.selected = selected }
                .testTag("transport-$description"),
    ) {
        Box(contentAlignment = Alignment.Center) {
            Icon(icon, contentDescription = description, tint = if (enabled) Color.White else Palette.gray400)
        }
    }
}

@Composable
private fun TransportRow(
    state: BandState,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    val t = state.transport
    val round = RoundedCornerShape(8.dp)
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxWidth()) {
        TransportButton(
            Icons.Default.FiberManualRecord,
            "record",
            t.recording,
            enabled,
            { send(Commands.record) },
            round,
            Modifier.width(56.dp),
            selectedColor = Palette.red500,
        )
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            TransportButton(
                Icons.Default.PlayArrow,
                "play",
                t.state == PlayState.PLAY_STATE_PLAYING,
                enabled,
                { send(Commands.play) },
                RoundedCornerShape(topStart = 8.dp, bottomStart = 8.dp),
                Modifier.weight(1f),
            )
            TransportButton(
                Icons.Default.Pause,
                "pause",
                t.state == PlayState.PLAY_STATE_PAUSED,
                enabled,
                { send(Commands.pause) },
                RoundedCornerShape(0.dp),
                Modifier.weight(1f),
            )
            TransportButton(
                Icons.Default.Stop,
                "stop",
                t.state == PlayState.PLAY_STATE_STOPPED,
                enabled,
                { send(Commands.stop) },
                RoundedCornerShape(topEnd = 8.dp, bottomEnd = 8.dp),
                Modifier.weight(1f),
            )
        }
        TransportButton(
            Icons.Default.Repeat,
            "repeat",
            t.repeat,
            enabled,
            { send(Commands.toggleRepeat) },
            round,
            Modifier.width(56.dp),
        )
    }
}

@Composable
private fun NavigationRow(
    state: BandState,
    position: Long,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    val previous = state.previousTarget(position)
    val next = state.nextTarget(position)
    val region = state.currentRegion(position)
    val marker = state.currentMarker(position)
    Row(
        Modifier
            .fillMaxWidth()
            .height(52.dp)
            .clip(RoundedCornerShape(8.dp)),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NavButton(Icons.Default.SkipPrevious, "previous", previous, enabled, send)
        Column(
            Modifier
                .weight(1f)
                .fillMaxHeight()
                .background(Palette.gray900)
                .padding(horizontal = 8.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            if (region == null) {
                Text("-", color = Palette.gray400)
            } else {
                val bg = rgb(region.color)
                Text(
                    region.name,
                    color = textOn(bg),
                    fontSize = 15.sp,
                    lineHeight = 20.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier =
                        Modifier
                            .clip(RoundedCornerShape(4.dp))
                            .background(bg ?: Color.Transparent)
                            .padding(horizontal = 6.dp)
                            .testTag("current-region"),
                )
            }
            if (marker != null) {
                Spacer(Modifier.height(3.dp))
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.height(16.dp)) {
                    MarkerChip(marker)
                    if (marker.name.isNotEmpty()) {
                        Text(
                            marker.name,
                            color = Palette.gray300,
                            fontSize = 12.sp,
                            lineHeight = 16.sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.padding(start = 4.dp).testTag("current-marker"),
                        )
                    }
                }
            }
        }
        NavButton(Icons.Default.SkipNext, "next", next, enabled, send)
    }
}

@Composable
private fun NavButton(
    icon: ImageVector,
    description: String,
    target: NavTarget?,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    val active = enabled && target != null
    Surface(
        onClick = { target?.let { send(Commands.goto(it)) } },
        enabled = active,
        color = if (active) Palette.gray700 else Palette.gray700.copy(alpha = 0.4f),
        modifier =
            Modifier
                .width(56.dp)
                .fillMaxHeight()
                .testTag("nav-$description"),
    ) {
        Box(contentAlignment = Alignment.Center) {
            Icon(
                icon,
                contentDescription = description,
                tint = if (active) Color.White else Palette.gray500,
                modifier = Modifier.size(26.dp),
            )
        }
    }
}

/** Marker number; as a [flag] it is square on the pole side, like a "P". */
@Composable
private fun MarkerChip(
    marker: Marker,
    flag: Boolean = false,
) {
    val bg = rgb(marker.color) ?: Palette.red600
    Box(
        Modifier
            .height(16.dp)
            .defaultMinSize(minWidth = 16.dp)
            .clip(if (flag) RoundedCornerShape(topEnd = 4.dp, bottomEnd = 4.dp) else RoundedCornerShape(4.dp))
            .background(bg)
            .padding(horizontal = 3.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(marker.id.toString(), color = textOn(bg), fontSize = 10.sp, lineHeight = 10.sp, maxLines = 1)
    }
}

private fun formatTime(ms: Long): String {
    if (ms < 1000) return "0:00"
    val seconds = ms / 1000
    return "%d:%02d".format(seconds / 60, seconds % 60)
}

@Composable
private fun RegionList(
    state: BandState,
    position: Long,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    val current = state.currentRegion(position)
    LazyColumn(Modifier.fillMaxSize().padding(horizontal = 8.dp)) {
        itemsIndexed(state.enabledRegions(), key = { _, r -> r.id }) { _, region ->
            RegionRow(
                region = region,
                markers = state.markersIn(region),
                current = current?.id == region.id,
                playState = state.transport.state,
                position = position,
                enabled = enabled,
                onClick = { send(Commands.goto(region.id)) },
            )
        }
    }
}

@Composable
private fun RegionRow(
    region: Region,
    markers: List<Marker>,
    current: Boolean,
    playState: PlayState,
    position: Long,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    val length = (region.end_ms - region.start_ms).coerceAtLeast(1)
    val shape = RoundedCornerShape(8.dp)
    val border =
        when {
            !current -> Color.Transparent
            playState == PlayState.PLAY_STATE_PLAYING -> Palette.emerald300
            playState == PlayState.PLAY_STATE_PAUSED -> Palette.yellow600
            else -> Palette.slate200
        }
    BoxWithConstraints(
        Modifier
            .padding(vertical = 4.dp)
            .fillMaxWidth()
            .height(44.dp)
            .clip(shape)
            .background(rgb(region.color) ?: Palette.gray800)
            .border(2.dp, border, shape)
            .clickable(enabled = enabled, onClick = onClick)
            .testTag("region-${region.id}"),
    ) {
        if (!current) Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.3f)))
        if (current) {
            val progress = ((position - region.start_ms).toFloat() / length).coerceIn(0f, 1f)
            Box(Modifier.fillMaxHeight().fillMaxWidth(progress).background(Color.Black.copy(alpha = 0.4f)))
        }
        for (marker in markers) {
            val x = maxWidth * ((marker.pos_ms - region.start_ms).toFloat() / length)
            val color = rgb(marker.color) ?: Palette.red600
            Box(
                Modifier
                    .offset(x = x)
                    .width(1.dp)
                    .fillMaxHeight()
                    .background(color),
            )
            Box(Modifier.offset(x = x)) { MarkerChip(marker, flag = true) }
        }
        Text(
            "${region.id}. ${region.name}",
            color = Palette.gray200,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier =
                Modifier
                    .align(Alignment.CenterStart)
                    .padding(start = 4.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(Color(0x66171717))
                    .padding(horizontal = 6.dp, vertical = 2.dp),
        )
        if (current) {
            Text(
                "${formatTime(position - region.start_ms)} / ${formatTime(length.toLong())}",
                color = Palette.gray300,
                fontSize = 12.sp,
                fontFamily = FontFamily.Monospace,
                modifier =
                    Modifier
                        .align(Alignment.CenterEnd)
                        .padding(end = 4.dp)
                        .clip(RoundedCornerShape(8.dp))
                        .background(Color(0x66171717))
                        .padding(4.dp),
            )
        }
    }
}

@Composable
private fun RegionEditor(
    state: BandState,
    enabled: Boolean,
    onOrder: (List<RegionOrder>) -> Unit,
) {
    val sorted = state.sortedRegions()
    // The list as dragged; sent as a whole when the finger lifts, not on every step.
    var items by remember(sorted) { mutableStateOf(sorted) }
    val haptics = LocalHapticFeedback.current
    val listState = rememberLazyListState()
    val reorder =
        rememberReorderableLazyListState(listState) { from, to ->
            items = items.toMutableList().apply { add(to.index, removeAt(from.index)) }
            haptics.performHapticFeedback(HapticFeedbackType.SegmentFrequentTick)
        }
    val drop = { if (items != sorted) onOrder(items.toOrder()) }
    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxSize().testTag("region-editor"),
        contentPadding = PaddingValues(horizontal = 8.dp, vertical = 4.dp),
    ) {
        itemsIndexed(items, key = { _, r -> r.region.id }) { index, item ->
            ReorderableItem(reorder, key = item.region.id) { dragging ->
                val elevation by animateDpAsState(if (dragging) 8.dp else 0.dp, label = "elevation")
                val move = { delta: Int ->
                    onOrder(moveRegion(items, index, delta))
                    true
                }
                Surface(
                    shape = RoundedCornerShape(10.dp),
                    color = if (dragging) Palette.gray700 else Palette.gray800,
                    shadowElevation = elevation,
                    modifier =
                        Modifier
                            .padding(vertical = 3.dp)
                            .fillMaxWidth()
                            .longPressDraggableHandle(
                                enabled = enabled,
                                onDragStarted = { haptics.performHapticFeedback(HapticFeedbackType.GestureThresholdActivate) },
                                onDragStopped = { drop() },
                            ).semantics {
                                customActions =
                                    listOfNotNull(
                                        CustomAccessibilityAction("Move up") { move(-1) }.takeIf { index > 0 },
                                        CustomAccessibilityAction("Move down") { move(1) }.takeIf { index < items.lastIndex },
                                    )
                            }.testTag("region-item-${item.region.id}"),
                ) {
                    EditorRow(
                        item = item,
                        enabled = enabled,
                        onToggle = { onOrder(toggleRegion(items, index)) },
                        onDragStarted = { haptics.performHapticFeedback(HapticFeedbackType.GestureThresholdActivate) },
                        onDragStopped = { drop() },
                    )
                }
            }
        }
    }
}

@Composable
private fun ReorderableCollectionItemScope.EditorRow(
    item: OrderedRegion,
    enabled: Boolean,
    onToggle: () -> Unit,
    onDragStarted: () -> Unit,
    onDragStopped: () -> Unit,
) {
    val region = item.region
    val dim = item.disabled
    Row(Modifier.height(56.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(
            Modifier
                .draggableHandle(enabled = enabled, onDragStarted = { onDragStarted() }, onDragStopped = onDragStopped)
                .fillMaxHeight()
                .width(48.dp)
                .testTag("drag-${region.id}"),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Default.DragHandle, contentDescription = "Drag to reorder", tint = Palette.gray400)
        }
        Box(
            Modifier
                .width(4.dp)
                .height(32.dp)
                .clip(RoundedCornerShape(2.dp))
                .background((rgb(region.color) ?: Palette.gray500).copy(alpha = if (dim) 0.35f else 1f)),
        )
        Column(Modifier.weight(1f).padding(horizontal = 12.dp)) {
            Text(
                "${region.id}. ${region.name}",
                color = if (dim) Palette.gray500 else Color.White,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val length = formatTime((region.end_ms - region.start_ms).toLong())
            Text(
                if (dim) "$length · skipped" else length,
                color = Palette.gray500,
                fontSize = 12.sp,
            )
        }
        Switch(
            checked = !item.disabled,
            onCheckedChange = { onToggle() },
            enabled = enabled,
            colors =
                SwitchDefaults.colors(
                    checkedThumbColor = Color.White,
                    checkedTrackColor = Palette.blue500,
                    uncheckedThumbColor = Palette.gray400,
                    uncheckedTrackColor = Palette.gray700,
                    uncheckedBorderColor = Palette.gray600,
                ),
            modifier = Modifier.padding(end = 12.dp).testTag("region-toggle-${region.id}"),
        )
    }
}
