package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.nikarh.bandcontrol.core.FaderLaw
import kotlinx.coroutines.delay

private const val SEND_INTERVAL_MS = 50L
private const val SETTLE_MS = 800L

/** Around 0 dB the fader sticks to exactly 0 dB, as a fraction of its travel. */
private const val DETENT = 0.008f

/**
 * Horizontal fader for a gain on a dB scale (see [FaderLaw]). Only horizontal drags move it, relative to where it is:
 * taps never change the level and vertical swipes scroll the list. Changes are sent at most every
 * [SEND_INTERVAL_MS] while dragging; the local value is kept briefly after release so server echoes don't jitter.
 */
@Composable
fun Fader(
    gain: Float,
    color: Color?,
    enabled: Boolean,
    onChange: (Float) -> Unit,
    modifier: Modifier = Modifier,
    height: Dp = 40.dp,
    label: @Composable (() -> Unit)? = null,
) {
    // Position while the finger is down, without the detent, and for a moment after release.
    var dragged by remember { mutableStateOf<Float?>(null) }
    var releases by remember { mutableIntStateOf(0) }
    var lastSent by remember { mutableLongStateOf(0L) }
    var width by remember { mutableFloatStateOf(1f) }
    val currentOnChange by rememberUpdatedState(onChange)
    val haptics = LocalHapticFeedback.current

    fun detent(raw: Float) = if (kotlin.math.abs(raw - FaderLaw.unityPosition) < DETENT) FaderLaw.unityPosition else raw.coerceIn(0f, 1f)

    val position = dragged?.let(::detent) ?: FaderLaw.gainToPosition(gain)
    val db = if (dragged != null) FaderLaw.positionToDb(position) else FaderLaw.gainToDb(gain)

    LaunchedEffect(releases) {
        if (releases > 0) {
            delay(SETTLE_MS)
            dragged = null
        }
    }

    fun send(force: Boolean) {
        val p = dragged?.let(::detent) ?: return
        val now = System.nanoTime() / 1_000_000
        if (force || now - lastSent >= SEND_INTERVAL_MS) {
            lastSent = now
            currentOnChange(FaderLaw.positionToGain(p))
        }
    }

    val drag =
        rememberDraggableState { dx ->
            val before = dragged ?: return@rememberDraggableState
            val after = (before + dx / width).coerceIn(-DETENT, 1f + DETENT)
            if ((detent(before) == FaderLaw.unityPosition) != (detent(after) == FaderLaw.unityPosition) &&
                detent(after) == FaderLaw.unityPosition
            ) {
                haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
            }
            dragged = after
            send(force = false)
        }

    val fill = (color ?: Palette.blue500).copy(alpha = if (enabled) 1f else 0.4f)
    BoxWithConstraints(
        modifier
            .fillMaxWidth()
            .height(height)
            .clip(RoundedCornerShape(6.dp))
            .background(Palette.gray700)
            .onSizeChanged { width = it.width.toFloat().coerceAtLeast(1f) }
            .semantics {
                stateDescription = "${FaderLaw.format(db)} dB"
                progressBarRangeInfo = ProgressBarRangeInfo(position, 0f..1f)
                if (enabled) {
                    setProgress { p ->
                        dragged = p.coerceIn(0f, 1f)
                        send(force = true)
                        releases++
                        true
                    }
                }
            }.draggable(
                state = drag,
                orientation = Orientation.Horizontal,
                enabled = enabled,
                onDragStarted = { dragged = dragged ?: FaderLaw.gainToPosition(gain) },
                onDragStopped = {
                    send(force = true)
                    releases++
                },
            ),
    ) {
        Box(Modifier.fillMaxHeight().fillMaxWidth(position).background(fill))
        // 0 dB mark.
        Box(
            Modifier
                .offset(x = maxWidth * FaderLaw.unityPosition)
                .width(1.dp)
                .fillMaxHeight()
                .background(Color.White.copy(alpha = 0.35f)),
        )
        if (label != null) {
            Box(Modifier.align(Alignment.CenterStart).padding(start = 8.dp)) { label() }
        }
        Text(
            "${FaderLaw.format(db)} dB",
            color = if (dragged != null) Palette.yellow300 else Color.White,
            fontSize = 12.sp,
            fontFamily = FontFamily.Monospace,
            modifier =
                Modifier
                    .align(Alignment.CenterEnd)
                    .padding(end = 6.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .background(Color(0x99171717))
                    .padding(horizontal = 4.dp, vertical = 2.dp)
                    .testTag("db"),
        )
    }
}
