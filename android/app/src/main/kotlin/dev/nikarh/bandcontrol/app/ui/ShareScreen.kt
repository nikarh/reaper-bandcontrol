package dev.nikarh.bandcontrol.app.ui

import android.app.Activity
import android.view.WindowManager
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import dev.nikarh.bandcontrol.core.protocol.QrMatrix

/** Light modules around the code, as the QR spec asks for. */
private const val QUIET_ZONE = 4

private const val CODE_GROUPS_PER_LINE = 4

/** Shows this phone's pairing as a QR code, so another phone can pair without the computer. */
@Composable
fun ShareScreen(
    info: PairingInfo,
    onClose: () -> Unit,
) {
    // The development TCP address only means something to this phone.
    val url = remember(info) { info.copy(tcp = null).toUrl() }
    val matrix = remember(url) { QrMatrix.encode(url) }
    BackHandler(onBack = onClose)
    FullBrightness()

    Box(Modifier.fillMaxSize().background(Palette.gray900).safeDrawingPadding()) {
        Column(
            Modifier.fillMaxSize().padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        ) {
            Text("Pair another phone", color = Palette.gray200, fontSize = 22.sp, fontWeight = FontWeight.Bold)
            Text(
                "On the other phone, open BandControl, tap \"Scan QR code\" and point it at this code.",
                color = Palette.gray400,
                textAlign = TextAlign.Center,
            )
            Canvas(
                Modifier
                    .fillMaxWidth()
                    .widthIn(max = 360.dp)
                    .aspectRatio(1f)
                    .clip(RoundedCornerShape(12.dp))
                    .background(Color.White)
                    .testTag("share-qr"),
            ) {
                val module = size.width / (matrix.size + 2 * QUIET_ZONE)
                for (y in 0 until matrix.size) {
                    for (x in 0 until matrix.size) {
                        if (matrix[x, y]) {
                            drawRect(
                                Color.Black,
                                Offset((x + QUIET_ZONE) * module, (y + QUIET_ZONE) * module),
                                // Slight overlap so no hairlines appear between modules.
                                Size(module + 0.5f, module + 0.5f),
                            )
                        }
                    }
                }
            }
            if (info.name.isNotEmpty()) Text(info.name, color = Palette.gray200, fontSize = 16.sp)
            val groups = info.secret.toDisplayCode().split('-')
            Text(
                groups.take(CODE_GROUPS_PER_LINE).joinToString("-") + "\n" + groups.drop(CODE_GROUPS_PER_LINE).joinToString("-"),
                color = Palette.gray300,
                fontSize = 18.sp,
                lineHeight = 26.sp,
                fontFamily = FontFamily.Monospace,
                textAlign = TextAlign.Center,
                modifier = Modifier.testTag("share-code"),
            )
        }
        IconButton(onClick = onClose, modifier = Modifier.padding(8.dp).testTag("share-close")) {
            Icon(Icons.Default.Close, contentDescription = "Close", tint = Palette.gray300)
        }
    }
}

/** Raises the screen brightness while shown, so cameras read the code easily. */
@Composable
private fun FullBrightness() {
    val activity = LocalContext.current as? Activity ?: return
    DisposableEffect(activity) {
        val window = activity.window
        val before = window.attributes.screenBrightness
        window.attributes = window.attributes.apply { screenBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_FULL }
        onDispose { window.attributes = window.attributes.apply { screenBrightness = before } }
    }
}
