package dev.nikarh.bandcontrol.app.ui

import android.content.ActivityNotFoundException
import android.content.Intent
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.net.toUri
import dev.nikarh.bandcontrol.app.BuildConfig
import dev.nikarh.bandcontrol.app.R
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import dev.nikarh.bandcontrol.core.protocol.PairingSecret

/**
 * Pairing with a computer: the welcome screen of a fresh install (no [onCancel]), or "pair another computer"
 * from the menu, with Back.
 */
@Composable
fun PairScreen(
    onPair: (PairingInfo) -> Unit,
    onCancel: (() -> Unit)?,
    onAbout: () -> Unit,
) {
    var code by rememberSaveable { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf("") }
    var scanError by rememberSaveable { mutableStateOf<String?>(null) }
    var manual by rememberSaveable { mutableStateOf(false) }
    val secret = PairingSecret.fromCode(code)
    var scanning by rememberSaveable { mutableStateOf(false) }
    if (scanning) {
        QrScanner(
            onResult = { text ->
                scanning = false
                val info = PairingInfo.fromUrl(text)
                when {
                    info == null -> scanError = "This QR code is not a BandControl pairing code."
                    BuildConfig.DEBUG -> onPair(info)
                    else -> onPair(info.copy(tcp = null))
                }
            },
            onClose = { scanning = false },
        )
        return
    }
    if (onCancel != null) BackHandler(onBack = onCancel)
    val context = LocalContext.current
    val openWebsite = {
        try {
            context.startActivity(Intent(Intent.ACTION_VIEW, WEBSITE.toUri()))
        } catch (_: ActivityNotFoundException) {
            Toast.makeText(context, "No browser to open $WEBSITE", Toast.LENGTH_LONG).show()
        }
    }

    Scaffold(
        containerColor = Palette.gray900,
        topBar = {
            AppBar(if (onCancel == null) "BandControl" else "Pair with a computer", onBack = onCancel) {
                OverflowMenu(MenuItem("About & share the app", "about", onAbout))
            }
        },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize()
                .imePadding()
                .padding(horizontal = 24.dp, vertical = 16.dp),
        ) {
            // The middle takes the free space and centers its content; it only scrolls when it can't fit (the
            // keyboard is up, landscape).
            BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
                Column(
                    Modifier
                        .verticalScroll(rememberScrollState())
                        .heightIn(min = maxHeight)
                        .fillMaxWidth(),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    if (manual) {
                        ManualEntry(
                            code = code,
                            onCode = { code = it },
                            name = name,
                            onName = { name = it },
                            valid = secret != null,
                            onPair = { secret?.let { onPair(PairingInfo(it, name.trim())) } },
                        )
                    } else {
                        Intro(welcome = onCancel == null, onGetExtension = openWebsite)
                    }
                }
            }
            scanError?.let {
                Text(it, color = Palette.red500, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp))
            }
            Button(
                onClick = {
                    scanError = null
                    manual = false
                    scanning = true
                },
                colors = ButtonDefaults.buttonColors(containerColor = Palette.blue500, contentColor = Color.White),
                modifier = Modifier.fillMaxWidth().height(48.dp).testTag("scan-button"),
            ) {
                Icon(Icons.Default.QrCodeScanner, contentDescription = null, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(8.dp))
                Text("Scan QR code")
            }
            if (!manual) {
                TextButton(
                    onClick = { manual = true },
                    modifier = Modifier.align(Alignment.CenterHorizontally).testTag("enter-code"),
                ) { Text("Enter the code instead", color = Palette.blue500) }
            }
        }
    }
}

/** What the app is (on a fresh install) and the one thing to do: show the QR code in REAPER. */
@Composable
private fun Intro(
    welcome: Boolean,
    onGetExtension: () -> Unit,
) {
    if (welcome) {
        Image(
            painterResource(R.drawable.ic_launcher_foreground),
            contentDescription = null,
            modifier = Modifier.size(88.dp).clip(RoundedCornerShape(22.dp)).background(Palette.gray800),
        )
        Spacer(Modifier.height(20.dp))
        Text(
            "Control REAPER from your phone",
            color = Palette.gray200,
            fontSize = 22.sp,
            fontWeight = FontWeight.Bold,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(8.dp))
    }
    Text(
        "In REAPER, run \"BandControl: Show pairing QR code\" and scan it.",
        color = Palette.gray400,
        textAlign = TextAlign.Center,
    )
    TextButton(onClick = onGetExtension, modifier = Modifier.testTag("get-extension")) {
        Text("Get the REAPER extension", color = Palette.blue500)
    }
}

@Composable
private fun ManualEntry(
    code: String,
    onCode: (String) -> Unit,
    name: String,
    onName: (String) -> Unit,
    valid: Boolean,
    onPair: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Type the code shown below the QR code in REAPER.", color = Palette.gray400, fontSize = 14.sp)
        OutlinedTextField(
            value = code,
            onValueChange = onCode,
            label = { Text("Pairing code") },
            placeholder = { Text("XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XX") },
            singleLine = true,
            isError = code.isNotBlank() && !valid,
            textStyle = TextStyle(fontFamily = FontFamily.Monospace),
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Characters, autoCorrectEnabled = false),
            modifier = Modifier.fillMaxWidth().testTag("pair-code"),
        )
        OutlinedTextField(
            value = name,
            onValueChange = onName,
            label = { Text("Computer name (optional)") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Button(
            onClick = onPair,
            enabled = valid,
            colors = ButtonDefaults.buttonColors(containerColor = Palette.blue500, contentColor = Color.White),
            modifier = Modifier.fillMaxWidth().testTag("pair-button"),
        ) { Text("Pair") }
    }
}
