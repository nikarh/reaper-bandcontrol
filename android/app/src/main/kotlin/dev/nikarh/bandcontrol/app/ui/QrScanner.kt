package dev.nikarh.bandcontrol.app.ui

import android.Manifest
import android.content.pm.PackageManager
import android.util.Log
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.nikarh.bandcontrol.core.protocol.QrDecoder
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

private fun QrDecoder.decode(image: ImageProxy): String? {
    val plane = image.planes.firstOrNull() ?: return null
    val buffer = plane.buffer.apply { rewind() }
    val luminance = ByteArray(plane.rowStride * image.height)
    buffer.get(luminance, 0, minOf(buffer.remaining(), luminance.size))
    return decode(luminance, image.width, image.height, plane.rowStride)
}

/**
 * Full screen camera preview that reports the first QR code it sees. Uses CameraX and ZXing, so it works without
 * Google Play services.
 */
@Composable
fun QrScanner(
    onResult: (String) -> Unit,
    onClose: () -> Unit,
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val currentOnResult by rememberUpdatedState(onResult)
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    var problem by remember { mutableStateOf<String?>(null) }
    val permission =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
            granted = it
            if (!it) problem = "Camera access was denied. Type the pairing code instead."
        }
    LaunchedEffect(Unit) { if (!granted) permission.launch(Manifest.permission.CAMERA) }
    BackHandler(onBack = onClose)

    val previewView = remember { PreviewView(context) }
    DisposableEffect(granted, lifecycleOwner) {
        if (!granted) return@DisposableEffect onDispose {}
        val analyzer = Executors.newSingleThreadExecutor()
        val decoder = QrDecoder()
        val found = AtomicBoolean(false)
        val main = ContextCompat.getMainExecutor(context)
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener({
            try {
                val provider = future.get()
                val preview = Preview.Builder().build().also { it.surfaceProvider = previewView.surfaceProvider }
                val analysis =
                    ImageAnalysis
                        .Builder()
                        .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                        .build()
                analysis.setAnalyzer(analyzer) { image ->
                    val text = image.use { decoder.decode(it) }
                    if (text != null && found.compareAndSet(false, true)) main.execute { currentOnResult(text) }
                }
                provider.unbindAll()
                provider.bindToLifecycle(lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
            } catch (e: Exception) {
                Log.w("BandControl", "camera failed", e)
                problem = "The camera couldn't be started. Type the pairing code instead."
            }
        }, main)
        onDispose {
            if (future.isDone) runCatching { future.get().unbindAll() }
            analyzer.shutdown()
        }
    }

    Box(Modifier.fillMaxSize().background(Color.Black).testTag("qr-scanner")) {
        if (granted && problem == null) {
            AndroidView(factory = { previewView }, modifier = Modifier.fillMaxSize())
            Box(
                Modifier
                    .align(Alignment.Center)
                    .size(240.dp)
                    .border(3.dp, Color.White.copy(alpha = 0.8f), RoundedCornerShape(16.dp)),
            )
        }
        Text(
            problem ?: "Point the camera at the QR code shown by REAPER",
            color = Color.White,
            textAlign = TextAlign.Center,
            modifier =
                Modifier
                    .align(if (problem == null) Alignment.BottomCenter else Alignment.Center)
                    .safeDrawingPadding()
                    .fillMaxWidth()
                    .padding(24.dp),
        )
        IconButton(onClick = onClose, modifier = Modifier.safeDrawingPadding().padding(8.dp).testTag("qr-close")) {
            Icon(Icons.Default.Close, contentDescription = "Close", tint = Color.White)
        }
    }
}
