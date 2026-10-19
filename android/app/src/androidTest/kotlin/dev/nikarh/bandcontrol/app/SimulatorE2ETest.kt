package dev.nikarh.bandcontrol.app

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasStateDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
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

    private fun receive(
        output: Int,
        index: Int,
    ): JSONObject = track(output).getJSONArray("receives").getJSONObject(index)

    /** Scrolls a lazy list so the row with [tag] is composed. */
    private fun scrollTo(
        list: String,
        tag: String,
    ) {
        compose.onNodeWithTag(list).performScrollToNode(hasTestTag(tag))
    }

    @Test
    fun transportAndNavigation() {
        compose.onNodeWithTag("transport-play").performClick()
        eventually { dump().getJSONObject("transport").getString("play") == "Playing" }
        compose.onNodeWithTag("transport-stop").performClick()
        eventually { dump().getJSONObject("transport").getString("play") == "Stopped" }

        compose.onNodeWithTag("region-2").performClick()
        awaitPosition(158.84..158.85)
        compose.waitUntilAtLeastOneExists(hasTestTag("current-region") and hasText("Paper Lions"), 5_000)
        compose.waitUntilAtLeastOneExists(hasTestTag("current-marker") and hasText("Intro"), 5_000)

        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(174.07..174.09)
        compose.waitUntilAtLeastOneExists(hasTestTag("current-marker") and hasText("Verse 1"), 5_000)
        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(183.60..183.61)
        compose.onNodeWithTag("nav-previous").performClick()
        awaitPosition(174.07..174.09)

        // After the last part of Paper Lions comes Static Bloom, the next song in the band's order (1, 2, 6, ...).
        control("seek 260")
        compose.waitUntilAtLeastOneExists(hasTestTag("current-marker") and hasText("Outro"), 5_000)
        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(572.07..572.08)
        compose.waitUntilAtLeastOneExists(hasTestTag("current-region") and hasText("Static Bloom"), 5_000)
    }

    @Test
    fun regionEditing() {
        // Velvet Machine is disabled in the project.
        compose.onNodeWithTag("region-11").assertDoesNotExist()
        compose.onNodeWithTag("edit-regions").performClick()
        compose.onNodeWithTag("region-toggle-4").performClick()
        eventually { dump().getString("region_order") == "11,12,16,04,15,13,17,18,19,110,011,112" }
        compose.onNodeWithTag("edit-regions").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("region-4"), 5_000)
    }

    @Test
    fun mixFollowsBothSides() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-40").performClick()
        compose.onNodeWithTag("mix-title").assertTextEquals("Drums")

        // Drums input is receive 7 of the drummer's mix.
        scrollTo("receives", "mute-14")
        compose.onNodeWithTag("mute-14").performClick()
        eventually { receive(40, 7).getBoolean("mute") }

        scrollTo("receives", "receive-7")
        val before = receive(40, 2).getDouble("volume")
        compose.onNodeWithTag("receive-7").performTouchInput { swipeLeft() }
        eventually { receive(40, 2).getDouble("volume") < before - 0.1 }

        // Changes made in REAPER show up on the phone.
        control("volume 40 0.25")
        compose.waitUntilAtLeastOneExists(hasTestTag("output-volume") and hasStateDescription("-12.0 dB"), 5_000)
        control("rename 14 Kick and snare")
        scrollTo("receives", "receive-14")
        compose.waitUntilAtLeastOneExists(hasText("Kick and snare"), 5_000)
    }

    @Test
    fun faderIgnoresTapsAndVerticalSwipes() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-40").performClick()
        val before = receive(40, 2).getDouble("volume")
        compose.onNodeWithTag("receive-7").performClick()
        compose.onNodeWithTag("receive-7").performTouchInput { swipeUp() }
        compose.onNodeWithTag("receive-7").performTouchInput { swipeDown() }
        Thread.sleep(1_000)
        assertEquals(before, receive(40, 2).getDouble("volume"), 1e-6)

        // A short horizontal drag nudges the level instead of jumping to the touched position.
        compose.onNodeWithTag("receive-7").performTouchInput {
            swipe(center, center + Offset(width * 0.05f, 0f), 300)
        }
        eventually { receive(40, 2).getDouble("volume") > before }
        val after = receive(40, 2).getDouble("volume")
        assertTrue("$before -> $after", after < before * 1.6)
    }

    @Test
    fun tabsKeepTheirState() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-42").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("mix-title") and hasText("Right guitar"), 5_000)
        compose.onNodeWithTag("tab-playback").performClick()
        compose.onNodeWithTag("region-1").assertExists()
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("mix-title").assertTextEquals("Right guitar")
    }

    @Test
    fun iemTabListsOnlyMonitorMixes() {
        compose.onNodeWithTag("tab-iem").performClick()
        for (id in 40..44) compose.onNodeWithTag("output-$id").assertExists()
        // Not the PA outputs, not the debug mix.
        compose.onNodeWithTag("output-30").assertDoesNotExist()
        compose.onNodeWithTag("output-45").assertDoesNotExist()
    }

    @Test
    fun inputLevelsAndGain() {
        control("meter 16 1.5")
        control("meter 15 -6")
        compose.onNodeWithTag("tab-inputs").performClick()
        for (id in listOf(14, 15, 16, 17, 18, 21, 23)) scrollTo("levels", "level-$id")
        // Nested tracks of the vocal folders are not inputs.
        compose.onNodeWithTag("level-19").assertDoesNotExist()
        scrollTo("levels", "level-14")
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-16") and hasText("CLIP +1.5"), 5_000)
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-15") and hasText("-6.0"), 5_000)

        // Peaks hold until reset.
        control("meter 16 off")
        Thread.sleep(500)
        compose.onNodeWithTag("peak-16").assertTextEquals("CLIP +1.5")
        compose.onNodeWithTag("peak-16").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-16") and hasText("-"), 5_000)

        compose.onNodeWithTag("gain-15").performTouchInput { swipeLeft() }
        eventually { track(15).getDouble("volume") < 0.5 }
        control("meter 15 off")
    }

    @Test
    fun paOutputLevels() {
        compose.onNodeWithTag("tab-pa").performClick()
        compose.onNodeWithTag("level-30").assertExists()
        control("meter 35 -9")
        scrollTo("levels", "level-35")
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-35") and hasText("-9.0"), 5_000)
        compose.onNodeWithTag("reset-peaks").performClick()
        control("meter 35 off")
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
        if (tcp == null) {
            // Over Bluetooth the pairing code also selects which computer to look for.
            compose.waitUntilAtLeastOneExists(hasTestTag("status") and hasText("Studio not found nearby", substring = true), 30_000)
        } else {
            compose.waitUntilAtLeastOneExists(
                hasTestTag("status") and hasText("Studio doesn't accept this phone's pairing code"),
                20_000,
            )
            compose.onNodeWithText("Pair again").performClick()
            compose.onNodeWithTag("pair-code").assertExists()
        }
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
