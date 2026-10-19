package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.nikarh.bandcontrol.core.protocol.PairingInfo

fun PairingInfo.label(): String = name.ifEmpty { "REAPER" }

/** Several paired computers are in range: which one to control. */
@Composable
fun ComputerChooser(
    choices: List<PairingInfo>,
    onPick: (PairingInfo) -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = Palette.gray800,
        title = { Text("Which computer?", color = Palette.gray200) },
        text = {
            Column(Modifier.testTag("chooser")) {
                Text("Several paired computers are nearby.", color = Palette.gray400, fontSize = 14.sp)
                Spacer(Modifier.size(8.dp))
                for (computer in choices) {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clickable { onPick(computer) }
                            .padding(vertical = 12.dp)
                            .testTag("choose-${computer.label()}"),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(Icons.Default.Computer, contentDescription = null, tint = Palette.gray400)
                        Spacer(Modifier.width(12.dp))
                        Text(computer.label(), color = Palette.gray200, fontSize = 16.sp)
                    }
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text("Not now", color = Palette.blue500) } },
    )
}

/** Another paired computer came into range. */
@Composable
fun NearbyNotice(
    computer: PairingInfo,
    onSwitch: () -> Unit,
    onDismiss: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().background(Palette.gray700).padding(start = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "${computer.label()} is nearby",
            color = Palette.gray200,
            fontSize = 13.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f).testTag("notice"),
        )
        TextButton(onClick = onSwitch, modifier = Modifier.testTag("notice-switch")) {
            Text("Switch", color = Palette.blue500)
        }
        IconButton(onClick = onDismiss, modifier = Modifier.testTag("notice-dismiss")) {
            Icon(Icons.Default.Close, contentDescription = "Dismiss", tint = Palette.gray400)
        }
    }
}
