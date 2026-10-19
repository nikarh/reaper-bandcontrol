package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PlayCircle
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.nikarh.bandcontrol.app.AppModel
import dev.nikarh.bandcontrol.core.ConnectionStatus

enum class Tab { Mix, Control }

@Composable
fun BandControlApp(
    model: AppModel,
    onRequestPermissions: () -> Unit,
) {
    val pairing by model.pairing.collectAsStateWithLifecycle()
    val status by model.status.collectAsStateWithLifecycle()
    val state by model.state.collectAsStateWithLifecycle()
    val permitted by model.permissionsGranted.collectAsStateWithLifecycle()
    var tab by rememberSaveable { mutableStateOf(Tab.Control) }
    var repairing by remember { mutableStateOf(false) }

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

    val connected = status is ConnectionStatus.Connected
    Scaffold(
        containerColor = Palette.gray900,
        topBar = {
            StatusBar(
                status = status,
                pairedName = info.name,
                needsPermission = !permitted && info.tcp == null,
                onRequestPermissions = onRequestPermissions,
                onRepair = { repairing = true },
                onUnpair = model::unpair,
            )
        },
        bottomBar = {
            NavigationBar(containerColor = Palette.gray700) {
                val colors =
                    NavigationBarItemDefaults.colors(
                        selectedIconColor = Palette.blue500,
                        selectedTextColor = Palette.blue500,
                        indicatorColor = Palette.gray800,
                        unselectedIconColor = Palette.gray400,
                        unselectedTextColor = Palette.gray400,
                    )
                NavigationBarItem(
                    selected = tab == Tab.Mix,
                    onClick = { tab = Tab.Mix },
                    icon = { Icon(Icons.Default.Tune, contentDescription = null) },
                    label = { Text("Mix") },
                    colors = colors,
                    modifier = Modifier.testTag("tab-mix"),
                )
                NavigationBarItem(
                    selected = tab == Tab.Control,
                    onClick = { tab = Tab.Control },
                    icon = { Icon(Icons.Default.PlayCircle, contentDescription = null) },
                    label = { Text("Control") },
                    colors = colors,
                    modifier = Modifier.testTag("tab-control"),
                )
            }
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
                when (tab) {
                    Tab.Control -> ControlScreen(state, enabled = connected, send = model::send)
                    Tab.Mix -> MixScreen(state, enabled = connected, send = model::send)
                }
            }
        }
    }
}

@Composable
private fun StatusBar(
    status: ConnectionStatus,
    pairedName: String,
    needsPermission: Boolean,
    onRequestPermissions: () -> Unit,
    onRepair: () -> Unit,
    onUnpair: () -> Unit,
) {
    val (color, text) =
        when (status) {
            is ConnectionStatus.Connected -> {
                Palette.emerald400 to status.serverName
            }

            ConnectionStatus.Connecting -> {
                Palette.yellow600 to "Looking for ${pairedName.ifEmpty { "REAPER" }}…"
            }

            ConnectionStatus.Authenticating -> {
                Palette.yellow600 to "Authenticating…"
            }

            is ConnectionStatus.Disconnected -> {
                Palette.red500 to if (status.authRejected) "Pairing code rejected, pair again" else status.reason
            }

            ConnectionStatus.Idle -> {
                Palette.gray500 to if (needsPermission) "Bluetooth permission needed" else "Idle"
            }
        }
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
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag("status"),
            )
        }
        if (needsPermission) {
            TextButton(onClick = onRequestPermissions) { Text("Allow", color = Palette.blue500) }
        }
        Box {
            IconButton(onClick = { menu = true }) {
                Icon(Icons.Default.MoreVert, contentDescription = "Menu", tint = Palette.gray400)
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
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
