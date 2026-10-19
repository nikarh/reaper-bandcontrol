package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.nikarh.bandcontrol.core.Commands
import dev.nikarh.bandcontrol.core.state.PeakHolds
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.Track
import kotlinx.coroutines.flow.StateFlow

/** Gain and live level of each track, with the highest level since the last reset. */
@Composable
fun LevelsScreen(
    title: String,
    tracks: List<Track>,
    meters: StateFlow<Map<Int, Float?>>,
    peaks: StateFlow<PeakHolds>,
    onResetPeaks: (Collection<Int>) -> Unit,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    val levels by meters.collectAsStateWithLifecycle()
    val holds by peaks.collectAsStateWithLifecycle()
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().background(Palette.gray800).padding(start = 12.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(title, color = Palette.gray200, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
            TextButton(
                onClick = { onResetPeaks(tracks.map { it.id }) },
                modifier = Modifier.testTag("reset-peaks"),
            ) { Text("Reset peaks", color = Palette.blue500) }
        }
        LazyColumn(Modifier.fillMaxSize().testTag("levels")) {
            items(tracks, key = { it.id }) { track ->
                LevelRow(
                    track = track,
                    level = levels[track.id],
                    max = holds.max[track.id],
                    clipped = holds.clipped(track.id),
                    onResetPeak = { onResetPeaks(listOf(track.id)) },
                    enabled = enabled,
                    send = send,
                )
                HorizontalDivider(color = Palette.gray800)
            }
        }
    }
}

@Composable
private fun LevelRow(
    track: Track,
    level: Float?,
    max: Float?,
    clipped: Boolean,
    onResetPeak: () -> Unit,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    Column(Modifier.padding(horizontal = 12.dp, vertical = 8.dp).testTag("level-${track.id}")) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(8.dp).background(rgb(track.color) ?: Palette.gray500, CircleShape))
            Spacer(Modifier.width(8.dp))
            Text(
                track.name,
                color = Palette.gray200,
                fontSize = 14.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            PeakHold(max, clipped, onResetPeak, Modifier.testTag("peak-${track.id}"))
        }
        Spacer(Modifier.height(6.dp))
        Fader(
            gain = track.volume,
            color = rgb(track.color),
            enabled = enabled,
            onChange = { send(Commands.trackVolume(track.id, it)) },
            height = 36.dp,
            modifier = Modifier.testTag("gain-${track.id}"),
        )
        Spacer(Modifier.height(4.dp))
        LevelMeter(level)
    }
}
