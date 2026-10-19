package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlin.math.ln

/** Linear gain to dB, as displayed by the web UI. */
fun gainToDb(gain: Float): String {
    if (gain < 0.00000002980232f) return "-inf"
    return "%.2f".format(ln(gain.toDouble()) * 8.68588963806)
}

private const val SEND_INTERVAL_MS = 50L
private const val SETTLE_MS = 800L

/**
 * Horizontal fader for a linear gain in 0..1. Dragging is relative to the current value, so an accidental tap does
 * not jump the level. While dragging, changes are sent at most every [SEND_INTERVAL_MS].
 */
@Composable
fun VolumeSlider(
    value: Float,
    color: Color?,
    enabled: Boolean,
    onChange: (Float) -> Unit,
    modifier: Modifier = Modifier,
    label: @Composable (() -> Unit)? = null,
) {
    // Local value while the finger is down, and for a moment after so the echo from the server doesn't jitter.
    var local by remember { mutableStateOf<Float?>(null) }
    var releases by remember { mutableIntStateOf(0) }
    var lastSent by remember { mutableLongStateOf(0L) }
    var width by remember { mutableFloatStateOf(1f) }
    val currentOnChange by rememberUpdatedState(onChange)
    val shown = (local ?: value).coerceIn(0f, 1f)

    LaunchedEffect(releases) {
        if (releases > 0) {
            delay(SETTLE_MS)
            local = null
        }
    }

    fun update(
        v: Float,
        force: Boolean,
    ) {
        local = v
        val now = System.nanoTime() / 1_000_000
        if (force || now - lastSent >= SEND_INTERVAL_MS) {
            lastSent = now
            currentOnChange(v)
        }
    }

    Box(
        modifier
            .fillMaxWidth()
            .height(40.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(Palette.gray600)
            .semantics {
                progressBarRangeInfo = ProgressBarRangeInfo(shown, 0f..1f)
                if (enabled) {
                    setProgress { v ->
                        update(v.coerceIn(0f, 1f), force = true)
                        releases++
                        true
                    }
                }
            }.pointerInput(enabled) {
                if (!enabled) return@pointerInput
                width = size.width.toFloat().coerceAtLeast(1f)
                detectHorizontalDragGestures(
                    onDragStart = { local = local ?: value },
                    onDragEnd = {
                        local?.let { update(it, force = true) }
                        releases++
                    },
                    onDragCancel = { releases++ },
                ) { change, dx ->
                    change.consume()
                    update(((local ?: value) + dx / width).coerceIn(0f, 1f), force = false)
                }
            },
    ) {
        Box(
            Modifier
                .fillMaxHeight()
                .fillMaxWidth(shown)
                .background((color ?: Palette.blue500).copy(alpha = if (enabled) 1f else 0.4f)),
        )
        if (label != null) {
            Box(Modifier.align(Alignment.CenterStart).padding(start = 4.dp)) { label() }
        }
        androidx.compose.material3.Text(
            "${gainToDb(local ?: value)} dB",
            color = if (local != null) Palette.gray400 else Color.White,
            fontSize = 12.sp,
            fontFamily = FontFamily.Monospace,
            modifier =
                Modifier
                    .align(Alignment.CenterEnd)
                    .padding(end = 8.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(Color(0x66171717))
                    .padding(4.dp)
                    .testTag("db"),
        )
    }
}
