package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PlayCircle
import androidx.compose.material.icons.filled.Speaker
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.nikarh.bandcontrol.app.AppModel
import dev.nikarh.bandcontrol.core.ConnectionStatus
import dev.nikarh.bandcontrol.core.link.Problem
import dev.nikarh.bandcontrol.core.state.groups

enum class Tab(
    val label: String,
    val icon: ImageVector,
) {
    Playback("Playback", Icons.Default.PlayCircle),
    Inputs("Inputs", Icons.Default.Mic),
    Pa("PA", Icons.Default.Speaker),
    Iem("IEM", Icons.Default.Tune),
}

@Composable
fun BandControlApp(
    model: AppModel,
    onRequestPermissions: () -> Unit,
    onEnableBluetooth: () -> Unit,
) {
    val pairing by model.pairing.collectAsStateWithLifecycle()
    val status by model.status.collectAsStateWithLifecycle()
    val state by model.state.collectAsStateWithLifecycle()
    val permitted by model.permissionsGranted.collectAsStateWithLifecycle()
    var tab by rememberSaveable { mutableStateOf(Tab.Playback) }
    var selectedOutput by rememberSaveable { mutableStateOf<Int?>(null) }
    var repairing by remember { mutableStateOf(false) }
    var sharing by rememberSaveable { mutableStateOf(false) }
    // Keeps each tab's scroll positions and selections while another tab is shown.
    val tabStates = rememberSaveableStateHolder()
    val groups = remember(state.tracks) { state.groups() }
    val tabs =
        Tab.entries.filter {
            when (it) {
                Tab.Inputs -> groups.inputs.isNotEmpty()
                Tab.Pa -> groups.paOutputs.isNotEmpty()
                else -> true
            }
        }
    val shown = if (tab in tabs || !state.loaded) tab else Tab.Playback
    val output = groups.iemOutputs.find { it.id == selectedOutput } ?: groups.iemOutputs.firstOrNull()
    val metered =
        when (shown) {
            Tab.Playback -> emptyList()
            Tab.Iem -> listOfNotNull(output?.id)
            Tab.Inputs -> groups.inputs.map { it.id }
            Tab.Pa -> groups.paOutputs.map { it.id }
        }
    LaunchedEffect(metered, pairing, repairing) { model.meter(if (pairing == null || repairing) emptyList() else metered) }

    val info = pairing
    if (info == null || repairing) {
        PairScreen(
            onPair = {
                repairing = false
                model.pair(it)
            },
            onCancel = if (info != null) ({ repairing = false }) else null,
        )
        return
    }

    if (sharing) {
        val name = info.name.ifEmpty { (status as? ConnectionStatus.Connected)?.serverName ?: "" }
        ShareScreen(info.copy(name = name), onClose = { sharing = false })
        return
    }

    val connected = status is ConnectionStatus.Connected
    Scaffold(
        containerColor = Palette.gray900,
        topBar = {
            StatusBar(
                status = status,
                pairedName = info.name,
                needsPermission = !permitted && info.tcp == null,
                onRequestPermissions = onRequestPermissions,
                onEnableBluetooth = onEnableBluetooth,
                onRepair = { repairing = true },
                onShare = { sharing = true },
                onUnpair = model::unpair,
            )
        },
        bottomBar = {
            CompactTabBar(tabs, shown) { tab = it }
        },
    ) { padding ->
        Box(Modifier.padding(padding).fillMaxSize()) {
            if (!state.loaded) {
                Text(
                    "Waiting for REAPER…",
                    color = Palette.gray400,
                    modifier = Modifier.align(Alignment.Center),
                )
            } else {
                tabStates.SaveableStateProvider(shown.name) {
                    when (shown) {
                        Tab.Playback -> {
                            ControlScreen(state, enabled = connected, send = model::send)
                        }

                        Tab.Iem -> {
                            MixScreen(
                                state = state,
                                outputs = groups.iemOutputs,
                                selected = output,
                                onSelect = { selectedOutput = it },
                                meters = model.meters,
                                enabled = connected,
                                send = model::send,
                            )
                        }

                        Tab.Inputs -> {
                            LevelsScreen("Inputs", groups.inputs, model.meters, model.peaks, model::resetPeaks, connected, model::send)
                        }

                        Tab.Pa -> {
                            LevelsScreen(
                                "PA outputs",
                                groups.paOutputs,
                                model.meters,
                                model.peaks,
                                model::resetPeaks,
                                connected,
                                model::send,
                            )
                        }
                    }
                }
            }
        }
    }
}

/** A low tab bar (icon over a small label, no indicator pill) instead of Material's 80 dp one. */
@Composable
private fun CompactTabBar(
    tabs: List<Tab>,
    selected: Tab,
    onSelect: (Tab) -> Unit,
) {
    Column(Modifier.fillMaxWidth().background(Palette.gray800)) {
        HorizontalDivider(color = Palette.gray700)
        Row(Modifier.fillMaxWidth().navigationBarsPadding().height(52.dp)) {
            for (t in tabs) {
                val color = if (t == selected) Color.White else Palette.gray400
                Column(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .selectable(selected = t == selected, role = Role.Tab) { onSelect(t) }
                        .testTag("tab-${t.name.lowercase()}"),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    Icon(t.icon, contentDescription = null, tint = color, modifier = Modifier.size(22.dp))
                    Text(
                        t.label,
                        color = color,
                        fontSize = 10.sp,
                        lineHeight = 12.sp,
                        fontWeight = if (t == selected) FontWeight.SemiBold else FontWeight.Normal,
                    )
                }
            }
        }
    }
}

/** What went wrong and what happens next, without technical terms. */
fun problemText(
    problem: Problem,
    name: String,
): String =
    when (problem) {
        Problem.BluetoothUnavailable -> "This phone has no Bluetooth"
        Problem.BluetoothOff -> "Bluetooth is off"
        Problem.PermissionMissing -> "Allow access to nearby devices to connect"
        Problem.NotFound -> "$name not found nearby. Is REAPER running? Still looking…"
        Problem.Unreachable -> "Can't connect to $name. Trying again…"
        Problem.NoAnswer -> "$name doesn't answer. Trying again…"
        Problem.Lost -> "Connection to $name lost. Reconnecting…"
        Problem.PairingRejected -> "$name doesn't accept this phone's pairing code"
        Problem.TooManyPhones -> "Too many phones are connected to $name"
        Problem.Incompatible -> "This app and the REAPER extension don't match. Update both."
        Problem.Unknown -> "Can't connect to $name. Trying again…"
    }

@Composable
private fun StatusBar(
    status: ConnectionStatus,
    pairedName: String,
    needsPermission: Boolean,
    onRequestPermissions: () -> Unit,
    onEnableBluetooth: () -> Unit,
    onRepair: () -> Unit,
    onShare: () -> Unit,
    onUnpair: () -> Unit,
) {
    val name = pairedName.ifEmpty { "REAPER" }
    val (color, text) =
        when (status) {
            is ConnectionStatus.Connected -> {
                Palette.emerald400 to status.serverName
            }

            ConnectionStatus.Connecting -> {
                Palette.yellow600 to "Looking for $name…"
            }

            ConnectionStatus.Authenticating -> {
                Palette.yellow600 to "Connecting to $name…"
            }

            is ConnectionStatus.Disconnected -> {
                Palette.red500 to problemText(status.problem, name)
            }

            ConnectionStatus.Idle -> {
                Palette.gray500 to
                    if (needsPermission) "Allow access to nearby devices to connect" else "Not connected"
            }
        }
    val problem = (status as? ConnectionStatus.Disconnected)?.problem
    var menu by remember { mutableStateOf(false) }
    Row(
        Modifier
            .fillMaxWidth()
            .background(Palette.gray800)
            .statusBarsPadding()
            .padding(start = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(10.dp).background(color, CircleShape))
        Spacer(Modifier.width(8.dp))
        Column(Modifier.weight(1f)) {
            Text(
                text,
                color = Palette.gray300,
                fontSize = 13.sp,
                lineHeight = 16.sp,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(vertical = 4.dp).testTag("status"),
            )
        }
        when {
            needsPermission || problem == Problem.PermissionMissing -> {
                TextButton(onClick = onRequestPermissions) { Text("Allow", color = Palette.blue500) }
            }

            problem == Problem.BluetoothOff -> {
                TextButton(onClick = onEnableBluetooth) { Text("Turn on", color = Palette.blue500) }
            }

            problem == Problem.PairingRejected -> {
                TextButton(onClick = onRepair) { Text("Pair again", color = Palette.blue500) }
            }
        }
        Box {
            IconButton(onClick = { menu = true }) {
                Icon(Icons.Default.MoreVert, contentDescription = "Menu", tint = Palette.gray400)
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(text = { Text("Share pairing with another phone") }, onClick = {
                    menu = false
                    onShare()
                })
                DropdownMenuItem(text = { Text("Pair with another computer") }, onClick = {
                    menu = false
                    onRepair()
                })
                DropdownMenuItem(text = { Text("Forget pairing", color = Color(0xFFF87171)) }, onClick = {
                    menu = false
                    onUnpair()
                })
            }
        }
    }
}
