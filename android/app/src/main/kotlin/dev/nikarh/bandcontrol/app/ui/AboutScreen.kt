package dev.nikarh.bandcontrol.app.ui

import android.content.ActivityNotFoundException
import android.content.Intent
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.net.toUri
import dev.nikarh.bandcontrol.app.BuildConfig
import dev.nikarh.bandcontrol.core.protocol.QrMatrix

/** The project's website (GitHub Pages of the repository): the manual and the REAPER extension's downloads. */
val WEBSITE: String =
    BuildConfig.REPOSITORY
        .removeSuffix("/")
        .split('/')
        .takeLast(2)
        .let { (owner, repo) -> "https://$owner.github.io/$repo/" }

/** Always the newest release: GitHub redirects this to the asset of the latest one. */
val LATEST_APK = "${BuildConfig.REPOSITORY}/releases/latest/download/bandcontrol.apk"

/** The project page and a QR code to download the app, to hand it to the rest of the band. */
@Composable
fun AboutScreen(onClose: () -> Unit) {
    val context = LocalContext.current
    val matrix = remember { QrMatrix.encode(LATEST_APK) }
    BackHandler(onBack = onClose)
    FullBrightness()

    val open = { url: String ->
        try {
            context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri()))
        } catch (_: ActivityNotFoundException) {
            Toast.makeText(context, "No browser to open $url", Toast.LENGTH_LONG).show()
        }
    }
    val share = {
        val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, LATEST_APK)
        context.startActivity(Intent.createChooser(send, "Share the BandControl app"))
    }

    Scaffold(
        containerColor = Palette.gray900,
        topBar = { AppBar("About BandControl", onBack = onClose, backTag = "about-close") },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        ) {
            Text("Get the app", color = Palette.gray200, fontSize = 20.sp, fontWeight = FontWeight.Bold)
            Text(
                "Scan with the camera of another phone to download the latest app from GitHub.",
                color = Palette.gray400,
                textAlign = TextAlign.Center,
            )
            QrCode(matrix, Modifier.testTag("about-qr"))
            Text(
                "Android 9 or newer. Android asks to allow installing apps from the browser the first time.",
                color = Palette.gray500,
                fontSize = 13.sp,
                textAlign = TextAlign.Center,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                LinkButton("Share link", Icons.Default.Share, Modifier.testTag("about-share"), share)
                LinkButton("GitHub", Icons.Default.Code, Modifier.testTag("about-github")) { open(BuildConfig.REPOSITORY) }
            }
            LinkButton("Website and REAPER extension", Icons.Default.Language, Modifier.testTag("about-website")) { open(WEBSITE) }
            Text(
                "BandControl ${BuildConfig.VERSION_NAME} · free software (MIT)",
                color = Palette.gray500,
                fontSize = 12.sp,
                modifier = Modifier.testTag("about-version"),
            )
        }
    }
}

@Composable
private fun LinkButton(
    label: String,
    icon: ImageVector,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    OutlinedButton(
        onClick = onClick,
        colors = ButtonDefaults.outlinedButtonColors(contentColor = Palette.blue500),
        modifier = modifier,
    ) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(8.dp))
        Text(label)
    }
}
