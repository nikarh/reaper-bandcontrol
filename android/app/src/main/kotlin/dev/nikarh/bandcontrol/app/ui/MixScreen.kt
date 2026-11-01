package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.background
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Headphones
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.nikarh.bandcontrol.core.Commands
import dev.nikarh.bandcontrol.core.FaderLaw
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.core.state.Listening
import dev.nikarh.bandcontrol.core.state.Presets
import dev.nikarh.bandcontrol.core.state.ReceiveRow
import dev.nikarh.bandcontrol.core.state.levelsOf
import dev.nikarh.bandcontrol.core.state.mix
import dev.nikarh.bandcontrol.core.state.receivesOf
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.Track
import kotlinx.coroutines.flow.StateFlow

/** What can be done with mixes beyond their faders: presets ([PresetBar]) and listening to another mix. */
interface MixActions {
    fun load(
        output: Track,
        name: String,
    ): Boolean

    fun save(
        output: Track,
        name: String,
    )

    fun rename(
        output: Track,
        from: String,
        to: String,
    )

    fun delete(
        output: Track,
        name: String,
    )

    /** Marks the phone owner's own mix, null for none. */
    fun setMine(mix: String?)

    /** Hear [to] in the own mix; false when not possible now. */
    fun listen(to: Track): Boolean

    /** Back to the own mix as it was; false when not possible now. */
    fun stopListening(): Boolean
}

/** One IEM mix: the output's own level on top, its presets, the receives feeding it below. */
@Composable
fun MixScreen(
    state: BandState,
    outputs: List<Track>,
    selected: Track?,
    onSelect: (Int) -> Unit,
    meters: StateFlow<Map<Int, Float?>>,
    presets: Presets,
    actions: MixActions,
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

    val levels by meters.collectAsStateWithLifecycle()
    val listening = presets.listening
    val mine = presets.mine
    // The own mix follows the one listened to: hands off until back.
    val editable = enabled && listening?.mine != selected.name
    var choosingMine by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxWidth().background(Palette.gray800)) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(8.dp)
                    .testTag("tab-iem-outputs"),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (o in outputs) {
                    val isSelected = o.id == selected.id
                    val color = if (isSelected) Color.White else Palette.gray400
                    Row(
                        Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(if (isSelected) Palette.gray600 else Palette.gray700)
                            .clickable { onSelect(o.id) }
                            .padding(horizontal = 14.dp, vertical = 10.dp)
                            .semantics { this.selected = isSelected }
                            .testTag("output-${o.id}"),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        if (o.name == mine) {
                            Icon(Icons.Default.Person, contentDescription = "Your mix", tint = color, modifier = Modifier.size(16.dp))
                            Spacer(Modifier.width(4.dp))
                        }
                        Text(o.name, color = color, fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal)
                    }
                }
            }
            if (listening != null) {
                ListeningBanner(listening, enabled, onStop = actions::stopListening)
            }
            PresetBar(
                presets = presets.of(selected.name),
                current = state.levelsOf(selected),
                enabled = editable,
                onLoad = { actions.load(selected, it) },
                onSave = { actions.save(selected, it) },
                onRename = { from, to -> actions.rename(selected, from, to) },
                onDelete = { actions.delete(selected, it) },
                modifier = Modifier.padding(bottom = 8.dp),
            )
            MainMix(selected, levels[selected.id], editable, send) {
                when {
                    selected.name == mine -> {
                        YourMix(enabled = listening == null, onNotMine = { actions.setMine(null) })
                    }

                    listening == null -> {
                        TextButton(
                            onClick = {
                                if (mine != null && state.mix(mine) != null) actions.listen(selected) else choosingMine = true
                            },
                            enabled = enabled,
                            modifier = Modifier.testTag("listen"),
                        ) {
                            Icon(Icons.Default.Headphones, contentDescription = null, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.width(6.dp))
                            Text("Listen")
                        }
                    }
                }
            }
        }
        LazyColumn(Modifier.fillMaxSize().testTag("receives")) {
            items(state.receivesOf(selected), key = { "${selected.id}-${it.index}-${it.source.id}" }) { row ->
                ReceiveRow(selected, row, levels[row.source.id], editable, send)
                HorizontalDivider(color = Palette.gray800)
            }
        }
    }

    if (choosingMine) {
        MineChooser(
            outputs = outputs.filter { it.id != selected.id },
            listeningTo = selected.name,
            onPick = {
                choosingMine = false
                actions.setMine(it.name)
                actions.listen(selected)
            },
            onDismiss = { choosingMine = false },
        )
    }
}

/** The own mix sounds like [listening]'s other mix: say so, and the way back. */
@Composable
private fun ListeningBanner(
    listening: Listening,
    enabled: Boolean,
    onStop: () -> Boolean,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = 8.dp, end = 8.dp, bottom = 8.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(Palette.blue500.copy(alpha = 0.25f))
            .padding(start = 12.dp)
            .testTag("listening"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Default.Headphones, contentDescription = null, tint = Color.White, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f).padding(vertical = 6.dp)) {
            Text("You hear the ${listening.to} mix", color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
            Text("Changes to it are what you hear", color = Palette.gray300, fontSize = 12.sp, lineHeight = 14.sp)
        }
        TextButton(onClick = { onStop() }, enabled = enabled, modifier = Modifier.testTag("listen-stop")) {
            Text("Back to my mix", color = if (enabled) Color.White else Palette.gray400, fontWeight = FontWeight.SemiBold)
        }
    }
}

/** On the own mix: says so, with a way to unmark it. */
@Composable
private fun YourMix(
    enabled: Boolean,
    onNotMine: () -> Unit,
) {
    Box {
        var menu by remember { mutableStateOf(false) }
        TextButton(onClick = { menu = true }, enabled = enabled, modifier = Modifier.testTag("your-mix")) {
            Icon(Icons.Default.Person, contentDescription = null, tint = Palette.gray300, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(6.dp))
            Text("Your mix", color = Palette.gray300)
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text("Not my mix") }, onClick = {
                menu = false
                onNotMine()
            })
        }
    }
}

/** Listening needs to know which mix is the phone owner's: asked once. */
@Composable
private fun MineChooser(
    outputs: List<Track>,
    listeningTo: String,
    onPick: (Track) -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = Palette.gray800,
        title = { Text("Which mix is yours?", color = Palette.gray200) },
        text = {
            Column(Modifier.testTag("mine-chooser")) {
                Text(
                    "You'll hear the $listeningTo mix in it until you go back. Your own balance is kept and comes back.",
                    color = Palette.gray400,
                    fontSize = 14.sp,
                )
                Spacer(Modifier.height(8.dp))
                for (output in outputs) {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clickable { onPick(output) }
                            .padding(vertical = 12.dp)
                            .testTag("mine-${output.id}"),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(Icons.Default.Person, contentDescription = null, tint = Palette.gray400)
                        Spacer(Modifier.width(12.dp))
                        Text(output.name, color = Palette.gray200, fontSize = 16.sp)
                    }
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel", color = Palette.blue500) } },
    )
}

@Composable
private fun MainMix(
    output: Track,
    level: Float?,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
    trailing: @Composable () -> Unit,
) {
    Column(Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp)) {
        Row(Modifier.fillMaxWidth().height(36.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Mix level", color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
            trailing()
        }
        Fader(
            gain = output.volume,
            color = rgb(output.color),
            enabled = enabled,
            onChange = { send(Commands.trackVolume(output.id, it)) },
            height = 56.dp,
            modifier = Modifier.testTag("output-volume"),
        )
        Spacer(Modifier.height(4.dp))
        LevelMeter(level)
    }
}

/** [sourceLevel]: the source track's level; its meter shows it through this receive (fader and mute). */
@Composable
private fun ReceiveRow(
    output: Track,
    row: ReceiveRow,
    sourceLevel: Float?,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    val level = sourceLevel?.takeUnless { row.mute }?.plus(FaderLaw.gainToDb(row.volume))?.takeIf { it.isFinite() }
    Row(
        Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            ReceiveFader(output, row, enabled, send)
            Spacer(Modifier.height(4.dp))
            LevelMeter(level, Modifier.testTag("receive-level-${row.source.id}"))
        }
        Spacer(Modifier.width(10.dp))
        MuteButton(output, row, enabled, send)
    }
}

@Composable
private fun ReceiveFader(
    output: Track,
    row: ReceiveRow,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    Fader(
        gain = row.volume,
        color = rgb(row.source.color),
        enabled = enabled,
        onChange = { send(Commands.receiveVolume(output.id, row.index, row.source.id, it)) },
        modifier = Modifier.testTag("receive-${row.source.id}"),
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
}

@Composable
private fun MuteButton(
    output: Track,
    row: ReceiveRow,
    enabled: Boolean,
    send: (ClientMessage) -> Boolean,
) {
    Box(
        Modifier
            .size(width = 48.dp, height = 40.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(if (row.mute) Palette.red600 else Palette.gray700)
            .toggleable(value = row.mute, enabled = enabled, role = Role.Switch) {
                send(Commands.receiveMute(output.id, row.index, row.source.id, it))
            }.semantics { contentDescription = "Mute ${row.source.name}" }
            .testTag("mute-${row.source.id}"),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            "M",
            color = if (row.mute) Color.White else Palette.gray300,
            fontSize = 13.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}
