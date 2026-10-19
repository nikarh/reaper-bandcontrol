package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.nikarh.bandcontrol.core.FaderLaw

private const val METER_FLOOR_DB = -60f
private const val METER_TOP_DB = 6f
private const val FALL_DB_PER_SECOND = 24f
private const val YELLOW_DB = -9f

private fun meterPosition(db: Float) = ((db - METER_FLOOR_DB) / (METER_TOP_DB - METER_FLOOR_DB)).coerceIn(0f, 1f)

/** Peak meter from -60 to +6 dBFS that rises instantly and falls smoothly. [level] is dBFS, null for silence. */
@Composable
fun LevelMeter(
    level: Float?,
    modifier: Modifier = Modifier,
) {
    val target = level ?: METER_FLOOR_DB
    var shown by remember { mutableFloatStateOf(METER_FLOOR_DB) }
    // Animates only while falling, so an idle meter doesn't keep requesting frames.
    LaunchedEffect(target) {
        var last = 0L
        while (shown > target) {
            withFrameNanos { now ->
                val dt = if (last == 0L) 0f else (now - last) / 1e9f
                last = now
                shown = maxOf(target, shown - FALL_DB_PER_SECOND * dt)
            }
        }
        shown = target
    }
    Canvas(
        modifier
            .fillMaxWidth()
            .height(6.dp)
            .clip(RoundedCornerShape(3.dp))
            // Lighter than whatever it sits on: the IEM mix header and the level rows have different backgrounds.
            .background(Color.White.copy(alpha = 0.1f)),
    ) {
        val x = size.width * meterPosition(shown)
        val yellowX = size.width * meterPosition(YELLOW_DB)
        val redX = size.width * meterPosition(0f)
        drawRect(Palette.green500, size = Size(minOf(x, yellowX), size.height))
        if (x > yellowX) drawRect(Palette.yellow400, Offset(yellowX, 0f), Size(minOf(x, redX) - yellowX, size.height))
        if (x > redX) drawRect(Palette.red500, Offset(redX, 0f), Size(x - redX, size.height))
        drawRect(Color.White.copy(alpha = 0.4f), Offset(redX, 0f), Size(1.dp.toPx(), size.height))
    }
}

/** Highest level since the last reset, red once it reached 0 dBFS. Tapping it resets it. */
@Composable
fun PeakHold(
    max: Float?,
    clipped: Boolean,
    onReset: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Text(
        when {
            clipped -> "CLIP ${FaderLaw.format(max ?: 0f)}"
            max == null -> "-"
            else -> FaderLaw.format(max)
        },
        color = if (clipped) Color.White else Palette.gray300,
        fontSize = 12.sp,
        fontFamily = FontFamily.Monospace,
        fontWeight = if (clipped) FontWeight.Bold else FontWeight.Normal,
        textAlign = TextAlign.Center,
        modifier =
            modifier
                .widthIn(min = 64.dp)
                .clip(RoundedCornerShape(4.dp))
                .background(if (clipped) Palette.red600 else Palette.gray800)
                .clickable(onClick = onReset)
                .padding(horizontal = 6.dp, vertical = 3.dp)
                .testTag("peak"),
    )
}
