package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import dev.nikarh.bandcontrol.core.protocol.PairingSecret

@Composable
fun PairScreen(
    onPair: (PairingInfo) -> Unit,
    onCancel: (() -> Unit)?,
) {
    var code by rememberSaveable { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf("") }
    val secret = PairingSecret.fromCode(code)

    Column(
        Modifier
            .fillMaxSize()
            .background(Palette.gray900)
            .safeDrawingPadding()
            .imePadding()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("Pair with REAPER", color = Palette.gray200, fontSize = 24.sp, fontWeight = FontWeight.Bold)
        Text(
            "On the computer, run the REAPER action \"BandControl: Show pairing QR code\" and scan the QR code with " +
                "the phone camera. Or type the code shown below the QR code:",
            color = Palette.gray400,
        )
        OutlinedTextField(
            value = code,
            onValueChange = { code = it },
            label = { Text("Pairing code") },
            placeholder = { Text("XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XX") },
            singleLine = true,
            isError = code.isNotBlank() && secret == null,
            textStyle =
                androidx.compose.ui.text
                    .TextStyle(fontFamily = FontFamily.Monospace),
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Characters, autoCorrectEnabled = false),
            modifier = Modifier.fillMaxWidth().testTag("pair-code"),
        )
        OutlinedTextField(
            value = name,
            onValueChange = { name = it },
            label = { Text("Computer name (optional)") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                onClick = { secret?.let { onPair(PairingInfo(it, name.trim())) } },
                enabled = secret != null,
                modifier = Modifier.testTag("pair-button"),
            ) { Text("Pair") }
            if (onCancel != null) TextButton(onClick = onCancel) { Text("Cancel") }
        }
    }
}
