package dev.nikarh.bandcontrol.app.ui

import android.view.WindowManager
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Scaffold
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

    Scaffold(
        containerColor = Palette.gray900,
        topBar = { AppBar("Pair another phone", onBack = onClose, backTag = "share-close") },
    ) { padding ->
        QrPage(Modifier.padding(padding)) { qr ->
            Text(
                "On the other phone, open BandControl, tap \"Scan QR code\" and point it at this code.",
                color = Palette.gray400,
                textAlign = TextAlign.Center,
            )
            QrCode(matrix, qr.testTag("share-qr"))
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
    }
}

/** Below this height the QR code would get too small to scan, so the page scrolls instead of shrinking it. */
private val MIN_FITTING_HEIGHT = 480.dp

/**
 * A centered page around a QR code. The code shrinks so that the page fits the screen without scrolling; [content]
 * gets the modifier to give it. On a very short screen (landscape) the page scrolls with the code at full width.
 */
@Composable
fun QrPage(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.(qr: Modifier) -> Unit,
) {
    BoxWithConstraints(modifier.fillMaxSize()) {
        val fits = maxHeight >= MIN_FITTING_HEIGHT
        Column(
            Modifier
                .fillMaxSize()
                .then(if (fits) Modifier else Modifier.verticalScroll(rememberScrollState()))
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        ) {
            content(if (fits) Modifier.weight(1f, fill = false) else Modifier)
        }
    }
}

/** A QR code on white with its quiet zone, as large as the space it gets allows, up to 360 dp. */
@Composable
fun QrCode(
    matrix: QrMatrix,
    modifier: Modifier = Modifier,
) {
    BoxWithConstraints(modifier, contentAlignment = Alignment.Center) {
        val side = minOf(maxWidth, maxHeight, 360.dp)
        Canvas(
            Modifier
                .size(side)
                .clip(RoundedCornerShape(12.dp))
                .background(Color.White),
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
    }
}

/** Raises the screen brightness while shown, so cameras read the code easily. */
@Composable
fun FullBrightness() {
    val activity = LocalActivity.current ?: return
    DisposableEffect(activity) {
        val window = activity.window
        val before = window.attributes.screenBrightness
        window.attributes = window.attributes.apply { screenBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_FULL }
        onDispose { window.attributes = window.attributes.apply { screenBrightness = before } }
    }
}
