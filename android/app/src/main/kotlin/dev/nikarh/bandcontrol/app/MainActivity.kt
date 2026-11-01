package dev.nikarh.bandcontrol.app

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.provider.Settings
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
import androidx.core.content.edit
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

    private val notificationRequest = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    /**
     * Asks once to show notifications: the one that says the app stays connected in the background, with its
     * Disconnect button (Android 13+ hides it otherwise; staying connected works either way).
     */
    private fun askForNotifications() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        val prefs = getSharedPreferences("bandcontrol", MODE_PRIVATE)
        if (prefs.getBoolean(KEY_ASKED_NOTIFICATIONS, false)) return
        prefs.edit { putBoolean(KEY_ASKED_NOTIFICATIONS, true) }
        notificationRequest.launch(Manifest.permission.POST_NOTIFICATIONS)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        handleIntent(intent)
        setContent {
            BandControlTheme {
                // Test tags double as resource ids so UiAutomator / adb scripts can find elements.
                Box(Modifier.semantics { testTagsAsResourceId = true }) {
                    BandControlApp(
                        model,
                        onRequestPermissions = { permissionRequest.launch(bluetoothPermissions) },
                        onEnableBluetooth = ::enableBluetooth,
                    )
                }
            }
        }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                model.status.collect { status ->
                    // On stage the screen must stay on while connected.
                    if (status is ConnectionStatus.Connected) {
                        askForNotifications()
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

    // Without BLUETOOTH_CONNECT the request throws, then the Bluetooth settings open instead.
    @SuppressLint("MissingPermission")
    private fun enableBluetooth() {
        try {
            startActivity(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE))
        } catch (_: SecurityException) {
            startActivity(Intent(Settings.ACTION_BLUETOOTH_SETTINGS))
        }
    }

    private fun hasPermissions() = bluetoothPermissions.all { checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }

    private companion object {
        const val KEY_ASKED_NOTIFICATIONS = "askedNotifications"
    }

    private fun handleIntent(intent: Intent?) {
        val url = intent?.dataString ?: return
        val info = PairingInfo.fromUrl(url) ?: return
        // TCP is a development feature: release builds always use Bluetooth.
        model.pair(if (BuildConfig.DEBUG) info else info.copy(tcp = null))
    }
}
