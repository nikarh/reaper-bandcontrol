package dev.nikarh.bandcontrol.app

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasStateDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.compose.ui.test.swipeDown
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.nikarh.bandcontrol.core.protocol.PairingInfo
import dev.nikarh.bandcontrol.core.protocol.QrDecoder
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.net.InetSocketAddress
import java.net.Socket
import kotlin.math.abs
import kotlin.math.log10

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

    private fun control(
        command: String,
        address: String = controlAddress,
    ): String {
        val (host, port) = address.split(':')
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

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private fun pairingIntent(
        secret: String,
        name: String,
        tcp: String?,
    ) = Intent(Intent.ACTION_VIEW, Uri.parse("bandcontrol://pair?s=$secret&n=${Uri.encode(name)}" + (tcp?.let { "&t=$it" } ?: "")))
        .setPackage(context.packageName)

    @Before
    fun launch() {
        control("reset")
        // Every test starts with only the simulator paired.
        context
            .getSharedPreferences("bandcontrol", 0)
            .edit()
            .clear()
            .commit()
        scenario = ActivityScenario.launch(pairingIntent(secret, "Simulator", tcp))
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

        compose.onNodeWithTag("region-3").performClick()
        awaitPosition(176.74..176.75)
        compose.waitUntilAtLeastOneExists(hasTestTag("current-region") and hasText("Paper Lions"), 5_000)
        compose.onNodeWithTag("current-marker").assertDoesNotExist()

        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(191.74..191.75)
        compose.waitUntilAtLeastOneExists(hasTestTag("current-marker") and hasText("Verse"), 5_000)
        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(221.74..221.75)
        compose.onNodeWithTag("nav-previous").performClick()
        awaitPosition(191.74..191.75)

        // After the last part of Paper Lions comes Interlude B, next in the band's order (1, 2, 6, 4, 5, 3, 7, ...).
        control("seek 300")
        compose.waitUntilAtLeastOneExists(hasTestTag("current-marker") and hasText("Outro"), 5_000)
        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(728.92..728.93)
        compose.waitUntilAtLeastOneExists(hasTestTag("current-region") and hasText("Interlude B"), 5_000)
    }

    @Test
    fun regionEditing() {
        // Northbound is disabled in the project.
        compose.onNodeWithTag("region-16").assertDoesNotExist()
        compose.onNodeWithTag("edit-regions").performClick()
        compose.onNodeWithTag("region-toggle-4").performClick()
        eventually { dump().getString("region_order") == "11,12,16,04,15,13,17,18,19,110,111,112,113,114,115,016,117" }
        compose.onNodeWithTag("edit-regions").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("region-4"), 5_000)
    }

    @Test
    fun timelineListsEverySongByPosition() {
        // The band's setlist (1, 2, 6, 4, ...) skips Northbound (16); the timeline has every song where it is.
        compose.onNodeWithTag("region-view-setlist").assertIsSelected()
        compose.onNodeWithTag("region-view-timeline").performClick()
        compose.onNodeWithTag("region-view-timeline").assertIsSelected()
        scrollTo("regions", "region-16")
        compose.onNodeWithTag("region-16").assertExists()
        scrollTo("regions", "region-1")
        val tops =
            listOf(1, 2, 3, 4, 6).map {
                compose
                    .onNodeWithTag("region-$it")
                    .fetchSemanticsNode()
                    .boundsInRoot.top
            }
        assertEquals(tops.sorted(), tops)
        // Editing the setlist shows the setlist again.
        compose.onNodeWithTag("edit-regions").performClick()
        compose.onNodeWithTag("edit-regions").performClick()
        compose.onNodeWithTag("region-view-setlist").assertIsSelected()
        compose.onNodeWithTag("region-16").assertDoesNotExist()
    }

    @Test
    fun regionsReorderByDragging() {
        compose.onNodeWithTag("edit-regions").performClick()
        // Region 6 is third in the band's order (1, 2, 6, 4, ...); dragged by its handle to the top.
        compose.onNodeWithTag("drag-6").performTouchInput {
            down(center)
            // Past the touch slop first, then up by more than two rows, in steps like a finger.
            repeat(30) { moveBy(Offset(0f, -12.dp.toPx())) }
            up()
        }
        eventually { dump().getString("region_order").startsWith("16,11,12,14,") }
        compose.onNodeWithTag("edit-regions").performClick()
        val tops =
            listOf(6, 1, 2).map {
                compose
                    .onNodeWithTag("region-$it")
                    .fetchSemanticsNode()
                    .boundsInRoot.top
            }
        assertEquals(tops.sorted(), tops)
    }

    @Test
    fun mixFollowsBothSides() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-27").performClick()
        compose.onNodeWithTag("output-27").assertIsSelected().assertTextEquals("Drums")

        // Drums input is receive 3 of the drums mix.
        scrollTo("receives", "mute-6")
        compose.onNodeWithTag("mute-6").performClick()
        eventually { receive(27, 3).getBoolean("mute") }

        scrollTo("receives", "receive-3")
        val before = receive(27, 1).getDouble("volume")
        compose.onNodeWithTag("receive-3").performTouchInput { swipeLeft() }
        eventually { receive(27, 1).getDouble("volume") < before - 0.1 }

        // Changes made in REAPER show up on the phone.
        control("volume 27 0.25")
        compose.waitUntilAtLeastOneExists(hasTestTag("output-volume") and hasStateDescription("-12.0 dB"), 5_000)
        control("rename 6 Kick and snare")
        scrollTo("receives", "receive-6")
        compose.waitUntilAtLeastOneExists(hasText("Kick and snare"), 5_000)
    }

    @Test
    fun presetsSaveRevertAndCompare() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-27").performClick()
        compose.onNodeWithTag("preset-save").performClick()
        compose.onNodeWithTag("preset-name").performTextReplacement("Gig")
        compose.onNodeWithTag("preset-confirm").performClick()
        compose.onNodeWithTag("preset-Gig").assertIsSelected()
        compose.onNodeWithTag("preset-revert").assertDoesNotExist()

        // A change to the loaded preset: Revert undoes it, and the change stays one tap away.
        scrollTo("receives", "mute-6")
        compose.onNodeWithTag("mute-6").performClick()
        eventually { receive(27, 3).getBoolean("mute") }
        compose.waitUntilAtLeastOneExists(hasTestTag("preset-revert"), 5_000)
        compose.onNodeWithTag("preset-Gig").assertTextEquals("Gig •")
        compose.onNodeWithTag("preset-revert").performClick()
        eventually { !receive(27, 3).getBoolean("mute") }
        compose.onNodeWithTag("preset-unsaved").performClick()
        eventually { receive(27, 3).getBoolean("mute") }
        compose.onNodeWithTag("preset-unsaved").assertIsSelected()

        // Presets survive a restart of the app, and are per mix.
        scenario.close()
        scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntilAtLeastOneExists(hasTestTag("tab-iem"), 20_000)
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-27").performClick()
        compose.onNodeWithTag("preset-Gig").assertExists()
        compose.onNodeWithTag("output-28").performClick()
        compose.onNodeWithTag("preset-Gig").assertDoesNotExist()

        // Long-press to delete.
        compose.onNodeWithTag("output-27").performClick()
        compose.onNodeWithTag("preset-Gig").performTouchInput { longClick() }
        compose.onNodeWithTag("preset-delete").performClick()
        compose.onNodeWithTag("preset-delete-confirm").performClick()
        compose.onNodeWithTag("preset-Gig").assertDoesNotExist()
    }

    /** Receives of [output] by source: volume and mute. */
    private fun receivesBySource(output: Int): Map<Int, Pair<Double, Boolean>> {
        val receives = track(output).getJSONArray("receives")
        return (0 until receives.length()).map(receives::getJSONObject).associate {
            it.getInt("src") to (it.getDouble("volume") to it.getBoolean("mute"))
        }
    }

    /**
     * The [mine] mix sounds like [to]: the same levels (within 0.1 dB: the app leaves inaudible differences alone)
     * for the sources both have, the others muted.
     */
    private fun soundsLike(
        mine: Int,
        to: Int,
    ): Boolean {
        val theirs = receivesBySource(to)
        return receivesBySource(mine).all { (src, level) ->
            val their = theirs[src]
            if (their == null) level.second else sameLevel(level, their)
        }
    }

    private fun sameLevel(
        a: Pair<Double, Boolean>,
        b: Pair<Double, Boolean>,
    ) = a.second == b.second && abs(20 * log10(a.first / b.first)) < 0.1

    @Test
    fun listeningToAnotherMix() {
        val bassBefore = receivesBySource(28)
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-27").performClick()
        // The first time, the app asks which mix is the phone owner's.
        compose.onNodeWithTag("listen").performClick()
        compose.onNodeWithTag("mine-28").performClick()
        compose.onNodeWithTag("listening").assertExists()
        eventually { soundsLike(mine = 28, to = 27) }

        // Changes to the drums mix, here or in REAPER, are heard in the own mix too.
        scrollTo("receives", "mute-6")
        compose.onNodeWithTag("mute-6").performClick()
        eventually { receive(27, 3).getBoolean("mute") && soundsLike(mine = 28, to = 27) }
        control("receive 27 1 0.3 0")
        eventually { soundsLike(mine = 28, to = 27) }

        // Back: the own mix as it was, the drums mix keeps its changes.
        compose.onNodeWithTag("listen-stop").performClick()
        eventually {
            val bass = receivesBySource(28)
            bass.keys == bassBefore.keys && bass.all { (src, level) -> sameLevel(level, bassBefore.getValue(src)) }
        }
        compose.onNodeWithTag("listening").assertDoesNotExist()
        assertTrue(receive(27, 3).getBoolean("mute"))
        // The own mix is marked from now on, and listening asks no more.
        compose.onNodeWithTag("output-28").performClick()
        compose.onNodeWithTag("your-mix").assertExists()
        compose.onNodeWithTag("output-27").performClick()
        compose.onNodeWithTag("listen").performClick()
        compose.onNodeWithTag("mine-chooser").assertDoesNotExist()
        compose.onNodeWithTag("listening").assertExists()
    }

    @Test
    fun faderIgnoresTapsAndVerticalSwipes() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-27").performClick()
        val before = receive(27, 1).getDouble("volume")
        compose.onNodeWithTag("receive-3").performClick()
        compose.onNodeWithTag("receive-3").performTouchInput { swipeUp() }
        compose.onNodeWithTag("receive-3").performTouchInput { swipeDown() }
        Thread.sleep(1_000)
        assertEquals(before, receive(27, 1).getDouble("volume"), 1e-6)

        // A short horizontal drag nudges the level instead of jumping to the touched position.
        compose.onNodeWithTag("receive-3").performTouchInput {
            swipe(center, center + Offset(width * 0.05f, 0f), 300)
        }
        eventually { receive(27, 1).getDouble("volume") > before }
        val after = receive(27, 1).getDouble("volume")
        assertTrue("$before -> $after", after < before * 1.6)
    }

    @Test
    fun tabsKeepTheirState() {
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-29").performScrollTo().performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("output-29") and isSelected() and hasText("Right guitar"), 5_000)
        compose.onNodeWithTag("tab-playback").performClick()
        compose.onNodeWithTag("region-1").assertExists()
        compose.onNodeWithTag("tab-iem").performClick()
        compose.onNodeWithTag("output-29").assertIsSelected()
    }

    @Test
    fun iemTabListsOnlyMonitorMixes() {
        compose.onNodeWithTag("tab-iem").performClick()
        for (id in 27..31) compose.onNodeWithTag("output-$id").performScrollTo().assertExists()
        // Not the FOH outputs.
        compose.onNodeWithTag("output-18").assertDoesNotExist()
    }

    @Test
    fun inputLevelsAndGain() {
        control("meter 8 1.5")
        control("meter 7 -6")
        compose.onNodeWithTag("tab-inputs").performClick()
        for (id in 6..12) scrollTo("levels", "level-$id")
        // Folders are not inputs.
        compose.onNodeWithTag("level-5").assertDoesNotExist()
        scrollTo("levels", "level-6")
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-8") and hasText("CLIP +1.5"), 5_000)
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-7") and hasText("-6.0"), 5_000)

        // Peaks hold until reset.
        control("meter 8 off")
        Thread.sleep(500)
        compose.onNodeWithTag("peak-8").assertTextEquals("CLIP +1.5")
        compose.onNodeWithTag("peak-8").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-8") and hasText("-"), 5_000)

        compose.onNodeWithTag("gain-7").performTouchInput { swipeLeft() }
        eventually { track(7).getDouble("volume") < 0.5 }
        control("meter 7 off")
    }

    @Test
    fun fohOutputLevels() {
        compose.onNodeWithTag("tab-foh").performClick()
        compose.onNodeWithTag("level-18").assertExists()
        control("meter 23 -9")
        scrollTo("levels", "level-23")
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-23") and hasText("-9.0"), 5_000)
        compose.onNodeWithTag("reset-peaks").performClick()
        control("meter 23 off")
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

    private fun status(text: String) = hasTestTag("status") and hasText(text)

    /** A second simulator, "Stage PC", reached over TCP; it can go offline as if the computer left. */
    @Test
    fun severalComputers() {
        val stageTcp = args.getString("stageTcp")
        val stageControl = args.getString("stageControl")
        val stageSecret = args.getString("stageSecret")
        assumeTrue("needs the second simulator over TCP", tcp != null && stageTcp != null && stageControl != null)
        val stage = { command: String -> control(command, stageControl!!) }
        try {
            // A new pairing is used right away, and the computer paired before is announced as nearby.
            scenario.onActivity { it.startActivity(pairingIntent(stageSecret!!, "Stage PC", stageTcp)) }
            compose.waitUntilAtLeastOneExists(status("Stage PC"), 20_000)
            compose.waitUntilAtLeastOneExists(hasTestTag("notice") and hasText("Simulator is nearby"), 20_000)
            compose.onNodeWithTag("notice-switch").performClick()
            compose.waitUntilAtLeastOneExists(status("Simulator"), 20_000)
            compose.onNodeWithTag("notice").assertDoesNotExist()

            // Opening the app with both in range asks which one.
            scenario.close()
            scenario = ActivityScenario.launch(MainActivity::class.java)
            compose.waitUntilAtLeastOneExists(hasTestTag("choose-Simulator"), 20_000)
            compose.onNodeWithTag("choose-Stage PC").performClick()
            compose.waitUntilAtLeastOneExists(status("Stage PC"), 20_000)
            compose.onNodeWithTag("chooser").assertDoesNotExist()

            // With only one in range it connects without asking, and announces the other when it shows up.
            stage("offline")
            scenario.close()
            scenario = ActivityScenario.launch(MainActivity::class.java)
            compose.waitUntilAtLeastOneExists(status("Simulator"), 20_000)
            compose.onNodeWithTag("chooser").assertDoesNotExist()
            stage("online")
            compose.waitUntilAtLeastOneExists(hasTestTag("notice") and hasText("Stage PC is nearby"), 30_000)
            compose.onNodeWithTag("notice-dismiss").performClick()
            compose.onNodeWithTag("notice").assertDoesNotExist()

            // The menu switches between paired computers and forgets them.
            compose.onNodeWithTag("menu").performClick()
            compose.onNodeWithTag("switch-Stage PC").performClick()
            compose.waitUntilAtLeastOneExists(status("Stage PC"), 20_000)
            compose.onNodeWithTag("menu").performClick()
            compose.onNodeWithTag("forget").performClick()
            compose.waitUntilAtLeastOneExists(status("Simulator"), 20_000)
            compose.onNodeWithTag("menu").performClick()
            compose.onNodeWithTag("switch-Stage PC").assertDoesNotExist()
        } finally {
            stage("online")
        }
    }

    @Test
    fun rejectedPairingIsExplained() {
        val wrong = "EEEEEEEEEEEEEEEEEEEEEEEEEE"
        scenario.onActivity { it.startActivity(pairingIntent(wrong, "Studio", tcp)) }
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

/** Decodes the QR code shown in the node tagged [tag]. */
    private fun qrText(tag: String): String? {
        val image = compose.onNodeWithTag(tag).captureToImage().toPixelMap()
        val luminance =
            ByteArray(image.width * image.height) { i ->
                val c = image[i % image.width, i / image.width]
                ((c.red * 0.299f + c.green * 0.587f + c.blue * 0.114f) * 255).toInt().toByte()
            }
        return QrDecoder().decode(luminance, image.width, image.height)
    }

    @Test
    fun aboutShowsTheDownloadQr() {
        compose.onNodeWithTag("menu").performClick()
        compose.onNodeWithTag("about").performClick()
        assertEquals("https://github.com/nikarh/reaper-bandcontrol/releases/latest/download/bandcontrol.apk", qrText("about-qr"))
        compose.onNodeWithTag("about-version").assertTextEquals("BandControl ${BuildConfig.VERSION_NAME} · free software (MIT)")
        compose.onNodeWithTag("about-close").performClick()
        compose.onNodeWithTag("region-1").assertExists()
    }

    @Test
    fun sharedPairingQrScans() {
        compose.onNodeWithContentDescription("Menu").performClick()
        compose.onNodeWithText("Share pairing with another phone").performClick()
        val info = PairingInfo.fromUrl(qrText("share-qr") ?: "")
        assertEquals(secret, info?.secret?.toCode())
        assertEquals("Simulator", info?.name)
        assertEquals(null, info?.tcp)
        compose.onNodeWithTag("share-close").performClick()
        compose.onNodeWithTag("region-1").assertExists()
    }
}
