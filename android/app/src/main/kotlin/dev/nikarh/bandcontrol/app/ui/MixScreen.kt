package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.nikarh.bandcontrol.core.Commands
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.core.state.ReceiveRow
import dev.nikarh.bandcontrol.core.state.receivesOf
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.Track
import kotlinx.coroutines.flow.StateFlow

/** One IEM mix: the output's own level on top, the receives feeding it below. */
@Composable
fun MixScreen(
    state: BandState,
    outputs: List<Track>,
    selected: Track?,
    onSelect: (Int) -> Unit,
    meters: StateFlow<Map<Int, Float?>>,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    if (selected == null) {
        Text(
            "No IEM mixes found. A mix is a track with a hardware output and receives, optionally in a folder named " +
                "\"IEM\" or \"Monitor\".",
            color = Palette.gray400,
            modifier = Modifier.padding(16.dp),
        )
        return
    }

    Column(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxWidth().background(Palette.gray800)) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (o in outputs) {
                    val isSelected = o.id == selected.id
                    Text(
                        o.name,
                        color = if (isSelected) Color.White else Palette.gray400,
                        fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                        modifier =
                            Modifier
                                .clip(RoundedCornerShape(8.dp))
                                .background(if (isSelected) Palette.gray600 else Palette.gray700)
                                .clickable { onSelect(o.id) }
                                .padding(horizontal = 14.dp, vertical = 10.dp)
                                .testTag("output-${o.id}"),
                    )
                }
            }
            MainMix(selected, meters, enabled, send)
        }
        LazyColumn(Modifier.fillMaxSize()) {
            items(state.receivesOf(selected), key = { "${selected.id}-${it.index}-${it.source.id}" }) { row ->
                ReceiveRow(selected, row, enabled, send)
                HorizontalDivider(color = Palette.gray800)
            }
        }
    }
}

@Composable
private fun MainMix(
    output: Track,
    meters: StateFlow<Map<Int, Float?>>,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    val levels by meters.collectAsStateWithLifecycle()
    Column(Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp)) {
        Row(verticalAlignment = Alignment.Bottom) {
            Text(
                output.name,
                color = Color.White,
                fontSize = 18.sp,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false).testTag("mix-title"),
            )
            Spacer(Modifier.width(6.dp))
            Text("mix level", color = Palette.gray400, fontSize = 12.sp, modifier = Modifier.padding(bottom = 3.dp))
        }
        Spacer(Modifier.height(6.dp))
        Fader(
            gain = output.volume,
            color = rgb(output.color),
            enabled = enabled,
            onChange = { send(Commands.trackVolume(output.id, it)) },
            height = 56.dp,
            modifier = Modifier.testTag("output-volume"),
        )
        Spacer(Modifier.height(4.dp))
        LevelMeter(levels[output.id])
    }
}

@Composable
private fun ReceiveRow(
    output: Track,
    row: ReceiveRow,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    Row(
        Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Fader(
            gain = row.volume,
            color = rgb(row.source.color),
            enabled = enabled,
            onChange = { send(Commands.receiveVolume(output.id, row.index, row.source.id, it)) },
            modifier = Modifier.weight(1f).testTag("receive-${row.source.id}"),
            label = {
                Text(
                    row.source.name,
                    color = Color.White,
                    fontSize = 14.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier =
                        Modifier
                            .clip(RoundedCornerShape(4.dp))
                            .background(Color(0x99171717))
                            .padding(horizontal = 4.dp, vertical = 1.dp),
                )
            },
        )
        Spacer(Modifier.width(10.dp))
        Box(
            Modifier
                .size(width = 48.dp, height = 40.dp)
                .clip(RoundedCornerShape(6.dp))
                .background(if (row.mute) Palette.red600 else Palette.gray800)
                .border(1.dp, if (row.mute) Palette.red600 else Palette.gray600, RoundedCornerShape(6.dp))
                .toggleable(value = row.mute, enabled = enabled, role = Role.Switch) {
                    send(Commands.receiveMute(output.id, row.index, row.source.id, it))
                }.semantics { contentDescription = "Mute ${row.source.name}" }
                .testTag("mute-${row.source.id}"),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                "M",
                color = if (row.mute) Color.White else Palette.gray400,
                fontSize = 13.sp,
                fontWeight = FontWeight.Bold,
            )
        }
    }
}
