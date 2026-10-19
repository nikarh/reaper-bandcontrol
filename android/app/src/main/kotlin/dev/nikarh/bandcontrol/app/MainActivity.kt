package dev.nikarh.bandcontrol.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import dev.nikarh.bandcontrol.app.ui.BandControlApp
import dev.nikarh.bandcontrol.app.ui.BandControlTheme
import dev.nikarh.bandcontrol.core.ConnectionStatus
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val model: AppModel by viewModels()

    private val bluetoothPermissions =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT)
        } else {
            arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
        }

    private val permissionRequest =
        registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
            model.setActive(lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED), hasPermissions())
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        handleIntent(intent)
        setContent {
            BandControlTheme {
                // Test tags double as resource ids so UiAutomator / adb scripts can find elements.
                Box(Modifier.semantics { testTagsAsResourceId = true }) {
                    BandControlApp(model, onRequestPermissions = { permissionRequest.launch(bluetoothPermissions) })
                }
            }
        }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                model.status.collect { status ->
                    // On stage the screen must stay on while connected.
                    if (status is ConnectionStatus.Connected) {
                        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    } else {
                        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    }
                }
            }
        }
    }

    override fun onStart() {
        super.onStart()
        if (!hasPermissions() && model.pairing.value?.tcp == null) permissionRequest.launch(bluetoothPermissions)
        model.setActive(true, hasPermissions())
    }

    override fun onStop() {
        super.onStop()
        model.setActive(false, hasPermissions())
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent)
    }

    private fun hasPermissions() = bluetoothPermissions.all { checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }

    private fun handleIntent(intent: Intent?) {
        val url = intent?.dataString ?: return
        val info = PairingInfo.fromUrl(url) ?: return
        // TCP is a development feature: release builds always use Bluetooth.
        model.pair(if (BuildConfig.DEBUG) info else info.copy(tcp = null))
    }
}
