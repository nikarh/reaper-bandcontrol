package dev.nikarh.bandcontrol.app

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.compose.ui.test.swipeDown
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeUp
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import dev.nikarh.bandcontrol.core.protocol.QrDecoder
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.net.InetSocketAddress
import java.net.Socket

/**
 * Drives the app against `bandctl sim` running on the development machine, reached through `adb reverse`.
 * Run by `mise run android:e2e [ble]`, which starts the simulator and passes its addresses as instrumentation arguments.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class SimulatorE2ETest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val args = InstrumentationRegistry.getArguments()

    /** Null connects over Bluetooth, the simulator must then run with `--ble`. */
    private val tcp = if (args.getString("ble") == "true") null else args.getString("simTcp") ?: "127.0.0.1:47021"
    private val controlAddress = args.getString("simControl") ?: "127.0.0.1:47022"
    private val secret = args.getString("secret") ?: "AAAAAAAAAAAAAAAAAAAAAAAAAA"
    private lateinit var scenario: ActivityScenario<MainActivity>

    private fun control(command: String): String {
        val (host, port) = controlAddress.split(':')
        return Socket().use { s ->
            s.connect(InetSocketAddress(host, port.toInt()), 5_000)
            s.getOutputStream().write("$command\n".toByteArray())
            s.getInputStream().bufferedReader().readLine()
        }
    }

    private fun dump() = JSONObject(control("dump"))

    private fun track(id: Int) =
        dump().getJSONArray("tracks").let { tracks ->
            (0 until tracks.length()).map(tracks::getJSONObject).single { it.getInt("id") == id }
        }

    /** Waits for [position] (seconds) on the simulator, then gives the transport update time to reach the phone. */
    private fun awaitPosition(position: ClosedFloatingPointRange<Double>) {
        eventually { dump().getJSONObject("transport").getDouble("position") in position }
        Thread.sleep(500)
    }

    private fun eventually(
        timeoutMs: Long = 10_000,
        condition: () -> Boolean,
    ) = compose.waitUntil(timeoutMs) { runCatching(condition).getOrDefault(false) }

    @Before
    fun launch() {
        control("reset")
        val uri = Uri.parse("bandcontrol://pair?s=$secret&n=Simulator" + (tcp?.let { "&t=$it" } ?: ""))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        scenario = ActivityScenario.launch(Intent(Intent.ACTION_VIEW, uri).setPackage(context.packageName))
        compose.waitUntilAtLeastOneExists(hasTestTag("region-1"), 20_000)
    }

    @After
    fun close() {
        scenario.close()
    }

    @Test
    fun transportAndNavigation() {
        compose.onNodeWithTag("transport-play").performClick()
        eventually { dump().getJSONObject("transport").getString("play") == "Playing" }
        compose.onNodeWithTag("transport-stop").performClick()
        eventually { dump().getJSONObject("transport").getString("play") == "Stopped" }

        compose.onNodeWithTag("region-3").performClick()
        awaitPosition(684.5..684.6)
        compose.waitUntilAtLeastOneExists(hasTestTag("current-region") and hasText("Winter"), 5_000)

        // Winter starts on marker 6, the next region in the user's order (1, 4, 3, 2) is Autumn.
        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(333.9..334.0)
        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(367.3..367.4)
        compose.onNodeWithTag("nav-previous").performClick()
        awaitPosition(333.9..334.0)
    }

    @Test
    fun regionEditing() {
        compose.onNodeWithTag("edit-regions").performClick()
        compose.onNodeWithTag("region-toggle-4").performClick()
        eventually { dump().getString("region_order") == "11,04,13,12" }
        compose.onNodeWithTag("edit-regions").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("region-4"), 5_000)
    }

    @Test
    fun mixFollowsBothSides() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-13").performClick()

        compose.onNodeWithTag("mute-2").performClick()
        eventually { track(13).getJSONArray("receives").getJSONObject(0).getBoolean("mute") }

        val before = track(13).getJSONArray("receives").getJSONObject(3).getDouble("volume")
        compose.onNodeWithTag("receive-7").performTouchInput { swipeLeft() }
        eventually { track(13).getJSONArray("receives").getJSONObject(3).getDouble("volume") < before - 0.1 }

        // Changes made in REAPER show up on the phone.
        control("volume 13 0.25")
        compose.waitUntilAtLeastOneExists(hasTestTag("db") and hasText("-12.0 dB"), 5_000)
        control("rename 2 Klick")
        compose.waitUntilAtLeastOneExists(hasText("Klick"), 5_000)
    }

    private fun receiveVolume(
        output: Int,
        index: Int,
    ) = track(output).getJSONArray("receives").getJSONObject(index).getDouble("volume")

    @Test
    fun faderIgnoresTapsAndVerticalSwipes() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-13").performClick()
        val before = receiveVolume(13, 3)
        compose.onNodeWithTag("receive-7").performClick()
        compose.onNodeWithTag("receive-7").performTouchInput { swipeUp() }
        compose.onNodeWithTag("receive-7").performTouchInput { swipeDown() }
        Thread.sleep(1_000)
        assertEquals(before, receiveVolume(13, 3), 1e-6)

        // A short horizontal drag nudges the level instead of jumping to the touched position.
        compose.onNodeWithTag("receive-7").performTouchInput {
            swipe(center, center + Offset(width * 0.05f, 0f), 300)
        }
        eventually { receiveVolume(13, 3) > before }
        val after = receiveVolume(13, 3)
        assertTrue("$before -> $after", after < before * 1.6)
    }

    @Test
    fun tabsKeepTheirState() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-15").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("mix-title") and hasText("Guitar [7+8]"), 5_000)
        compose.onNodeWithTag("tab-playback").performClick()
        compose.onNodeWithTag("region-3").assertExists()
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("mix-title").assertTextEquals("Guitar [7+8]")
    }

    @Test
    fun inputLevelsAndGain() {
        control("meter 7 1.5")
        control("meter 8 -6")
        compose.onNodeWithTag("tab-inputs").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-7") and hasText("CLIP +1.5"), 5_000)
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-8") and hasText("-6.0"), 5_000)

        // Peaks hold until reset.
        control("meter 7 off")
        Thread.sleep(500)
        compose.onNodeWithTag("peak-7").assertTextEquals("CLIP +1.5")
        compose.onNodeWithTag("peak-7").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-7") and hasText("-"), 5_000)

        compose.onNodeWithTag("gain-8").performTouchInput { swipeLeft() }
        eventually { track(8).getDouble("volume") < 0.5 }
        control("meter 8 off")
    }

    @Test
    fun paOutputsOfABandProject() {
        control("reset band")
        compose.waitUntilAtLeastOneExists(hasTestTag("tab-pa"), 10_000)
        compose.onNodeWithTag("tab-pa").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("level-30"), 10_000)
        control("meter 35 -9")
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-35") and hasText("-9.0"), 5_000)

        // The Mix tab only lists the monitor mixes, not the PA outputs or the debug mix.
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-40").assertExists()
        compose.onNodeWithTag("output-30").assertDoesNotExist()
        compose.onNodeWithTag("output-45").assertDoesNotExist()
    }

    @Test
    fun qrScannerWorksWithoutPlayServices() {
        compose.onNodeWithContentDescription("Menu").performClick()
        compose.onNodeWithText("Pair with another computer").performClick()
        compose.onNodeWithTag("scan-button").performClick()
        compose.waitUntilAtLeastOneExists(hasText("Point the camera at the QR code shown by REAPER"), 5_000)
        Thread.sleep(1_000)
        compose.onNodeWithText("camera couldn't be started", substring = true).assertDoesNotExist()
        compose.onNodeWithTag("qr-close").performClick()
        compose.onNodeWithTag("pair-code").assertExists()
    }

    @Test
    fun rejectedPairingIsExplained() {
        val wrong = "EEEEEEEEEEEEEEEEEEEEEEEEEE"
        val uri = Uri.parse("bandcontrol://pair?s=$wrong&n=Studio" + (tcp?.let { "&t=$it" } ?: ""))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        scenario.onActivity { it.startActivity(Intent(Intent.ACTION_VIEW, uri).setPackage(context.packageName)) }
        compose.waitUntilAtLeastOneExists(
            hasTestTag("status") and hasText("Studio doesn't accept this phone's pairing code"),
            20_000,
        )
        compose.onNodeWithText("Pair again").performClick()
        compose.onNodeWithTag("pair-code").assertExists()
    }

    @Test
    fun sharedPairingQrScans() {
        compose.onNodeWithContentDescription("Menu").performClick()
        compose.onNodeWithText("Share pairing with another phone").performClick()
        val image = compose.onNodeWithTag("share-qr").captureToImage().toPixelMap()
        val luminance =
            ByteArray(image.width * image.height) { i ->
                val c = image[i % image.width, i / image.width]
                ((c.red * 0.299f + c.green * 0.587f + c.blue * 0.114f) * 255).toInt().toByte()
            }
        val url = QrDecoder().decode(luminance, image.width, image.height)
        val info = PairingInfo.fromUrl(url ?: "")
        assertEquals(secret, info?.secret?.toCode())
        assertEquals("Simulator", info?.name)
        assertEquals(null, info?.tcp)
        compose.onNodeWithTag("share-close").performClick()
        compose.onNodeWithTag("region-1").assertExists()
    }
}
