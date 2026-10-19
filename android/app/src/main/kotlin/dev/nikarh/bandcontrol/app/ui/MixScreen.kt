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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
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
import dev.nikarh.bandcontrol.core.Commands
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.core.state.ReceiveRow
import dev.nikarh.bandcontrol.core.state.outputs
import dev.nikarh.bandcontrol.core.state.receivesOf
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.Track

@Composable
fun MixScreen(
    state: BandState,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    val outputs = state.outputs()
    var selectedId by rememberSaveable { mutableStateOf<Int?>(null) }
    val output = outputs.find { it.id == selectedId } ?: outputs.firstOrNull()

    if (output == null) {
        Box(Modifier.fillMaxSize().padding(16.dp)) {
            Text(
                "No output tracks. An output is a track with a hardware output and at least one receive.",
                color = Palette.gray400,
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(12.dp))
                        .background(Palette.gray800)
                        .padding(16.dp),
            )
        }
        return
    }

    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier
                .fillMaxWidth()
                .background(Palette.gray800)
                .horizontalScroll(rememberScrollState())
                .padding(8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            for (o in outputs) {
                val selected = o.id == output.id
                Text(
                    o.name,
                    color = if (selected) Color.White else Palette.gray400,
                    fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
                    modifier =
                        Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(if (selected) Palette.gray600 else Palette.gray700)
                            .clickable { selectedId = o.id }
                            .padding(horizontal = 14.dp, vertical = 10.dp)
                            .testTag("output-${o.id}"),
                )
            }
        }
        LazyColumn(
            Modifier.fillMaxSize().padding(horizontal = 8.dp),
            contentPadding =
                androidx.compose.foundation.layout
                    .PaddingValues(vertical = 8.dp),
        ) {
            item(key = "output-${output.id}") { OutputCard(output, enabled, send) }
            items(state.receivesOf(output), key = { "${output.id}-${it.index}-${it.source.id}" }) { row ->
                ReceiveCard(output, row, enabled, send)
            }
        }
    }
}

@Composable
private fun OutputCard(
    output: Track,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    Column(
        Modifier
            .padding(bottom = 8.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(Palette.gray700)
            .padding(8.dp),
    ) {
        Row {
            Text("Output: ", color = Palette.gray400, fontWeight = FontWeight.Bold)
            Text(output.name, color = Palette.gray400)
            Text(" (${output.number})", color = Palette.gray500)
        }
        Spacer(Modifier.height(4.dp))
        VolumeSlider(
            value = output.volume,
            color = rgb(output.color),
            enabled = enabled,
            onChange = { send(Commands.trackVolume(output.id, it)) },
            modifier = Modifier.testTag("output-volume"),
        )
    }
}

@Composable
private fun ReceiveCard(
    output: Track,
    row: ReceiveRow,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    Row(
        Modifier
            .padding(vertical = 6.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(Palette.gray800)
            .padding(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        VolumeSlider(
            value = row.volume,
            color = rgb(row.source.color),
            enabled = enabled,
            onChange = { send(Commands.receiveVolume(output.id, row.index, row.source.id, it)) },
            modifier = Modifier.weight(1f).testTag("receive-${row.source.id}"),
            label = {
                Row(
                    Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(Color(0x66171717))
                        .padding(4.dp),
                ) {
                    Text(row.source.name, color = Palette.gray200, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(" (${row.source.number})", color = Palette.gray400)
                }
            },
        )
        Spacer(Modifier.width(16.dp))
        Text(
            "M",
            color = if (row.mute) Color.White else Palette.gray400,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            modifier =
                Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (row.mute) Palette.red600 else Palette.gray800)
                    .border(1.dp, Palette.gray600, RoundedCornerShape(8.dp))
                    .toggleable(value = row.mute, enabled = enabled, role = Role.Switch) {
                        send(Commands.receiveMute(output.id, row.index, row.source.id, it))
                    }.semantics { contentDescription = "Mute ${row.source.name}" }
                    .padding(horizontal = 24.dp, vertical = 10.dp)
                    .testTag("mute-${row.source.id}"),
        )
    }
}
