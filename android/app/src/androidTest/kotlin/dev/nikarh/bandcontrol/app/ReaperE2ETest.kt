package dev.nikarh.bandcontrol.app

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isOff
import androidx.compose.ui.test.isOn
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.net.InetSocketAddress
import java.net.Socket

/**
 * Drives the app against a real REAPER running the extension, observed and driven through REAPER's web interface
 * (reached through `adb reverse`). Run by `mise run android:e2e:reaper [tcp]`.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class ReaperE2ETest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val args = InstrumentationRegistry.getArguments()

    /** Null connects over Bluetooth. */
    private val tcp: String? = args.getString("tcp")
    private val webAddress = args.getString("reaperWeb") ?: "127.0.0.1:47031"
    private val secret = args.getString("secret") ?: "AAAAAAAAAAAAAAAAAAAAAAAAAA"
    private lateinit var scenario: ActivityScenario<MainActivity>

    /** Runs REAPER web interface commands, e.g. `SET/POS/10;TRANSPORT`. */
    private fun web(commands: String): String {
        val (host, port) = webAddress.split(':')
        return Socket().use { s ->
            s.connect(InetSocketAddress(host, port.toInt()), 5_000)
            s.soTimeout = 5_000
            s.getOutputStream().write("GET /_/$commands HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".toByteArray())
            s
                .getInputStream()
                .bufferedReader()
                .readText()
                .substringAfter("\r\n\r\n")
        }
    }

    /** Fields of the first response line starting with [kind], e.g. `TRANSPORT playstate position ...`. */
    private fun query(
        commands: String,
        kind: String,
    ) = web(commands).lineSequence().first { it.startsWith("$kind\t") }.split('\t')

    private fun playState() = query("TRANSPORT", "TRANSPORT")[1].toInt()

    private fun position() = query("TRANSPORT", "TRANSPORT")[2].toDouble()

    private fun trackVolume(id: Int) = query("TRACK/$id", "TRACK")[4].toDouble()

    private fun regionOrder() = query("GET/PROJEXTSTATE/BANDUI/regions", "PROJEXTSTATE")[3]

    private fun awaitPosition(range: ClosedFloatingPointRange<Double>) {
        eventually { position() in range }
        Thread.sleep(500)
    }

    private fun eventually(
        timeoutMs: Long = 10_000,
        condition: () -> Boolean,
    ) = compose.waitUntil(timeoutMs) { runCatching(condition).getOrDefault(false) }

    @Before
    fun launch() {
        web("$STOP;SET/POS/0;SET/PROJEXTSTATE/BANDUI/regions/$INITIAL_ORDER;SET/TRACK/13/VOL/0.42;SET/TRACK/7/VOL/1")
        val uri = Uri.parse("bandcontrol://pair?s=$secret&n=REAPER" + (tcp?.let { "&t=$it" } ?: ""))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        scenario = ActivityScenario.launch(Intent(Intent.ACTION_VIEW, uri).setPackage(context.packageName))
        compose.waitUntilAtLeastOneExists(hasTestTag("region-4"), 30_000)
    }

    @After
    fun close() {
        scenario.close()
    }

    @Test
    fun transportAndNavigation() {
        compose.onNodeWithTag("transport-play").performClick()
        eventually { playState() == 1 }
        compose.onNodeWithTag("transport-stop").performClick()
        eventually { playState() == 0 }

        compose.onNodeWithTag("region-3").performClick()
        awaitPosition(684.5..684.6)
        compose.waitUntilAtLeastOneExists(hasTestTag("current-region") and hasText("Winter"), 5_000)
        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(333.9..334.0)
        compose.onNodeWithTag("nav-next").performClick()
        awaitPosition(367.3..367.4)
        compose.onNodeWithTag("nav-previous").performClick()
        awaitPosition(333.9..334.0)

        // Seeking in REAPER moves the phone to the region under the cursor.
        web("SET/POS/700")
        compose.waitUntilAtLeastOneExists(hasTestTag("current-region") and hasText("Winter"), 5_000)
    }

    @Test
    fun regionOrderFollowsBothSides() {
        compose.onNodeWithTag("edit-regions").performClick()
        compose.onNodeWithTag("region-toggle-4").performClick()
        eventually { regionOrder() == "11,04,13,12" }
        compose.onNodeWithTag("edit-regions").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("region-4"), 5_000)

        // Usually within the server's debounce, so REAPER never publishes the phone's order.
        web("SET/PROJEXTSTATE/BANDUI/regions/$INITIAL_ORDER")
        compose.waitUntilAtLeastOneExists(hasTestTag("region-4"), 5_000)
    }

    @Test
    fun mixFollowsBothSides() {
        compose.onNodeWithTag("tab-mix").performClick()
        compose.onNodeWithTag("output-13").performClick()

        // The mute button only shows what REAPER reports back, so this round-trips through REAPER.
        compose.onNodeWithTag("mute-2").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("mute-2") and isOn(), 5_000)
        web("SET/TRACK/13/SEND/-1/MUTE/-1")
        compose.waitUntilAtLeastOneExists(hasTestTag("mute-2") and isOff(), 5_000)

        compose.onNodeWithTag("output-volume").performTouchInput { swipeLeft() }
        eventually { trackVolume(13) < 0.3 }

        web("SET/TRACK/13/VOL/0.25")
        compose.waitUntilAtLeastOneExists(hasTestTag("db") and hasText("-12.0 dB"), 5_000)
    }

    @Test
    fun inputGain() {
        compose.onNodeWithTag("tab-inputs").performClick()
        compose.onNodeWithTag("gain-7").performTouchInput { swipeLeft() }
        eventually { trackVolume(7) < 0.5 }
        web("SET/TRACK/7/VOL/1")
        compose.waitUntilAtLeastOneExists(
            hasTestTag("gain-7") and SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "0.0 dB"),
            5_000,
        )
    }

    private companion object {
        /** Region order stored in the Mother Project: Summer, Spring, Winter, Autumn, all enabled. */
        const val INITIAL_ORDER = "11,14,13,12"

        /** Transport: Stop. */
        const val STOP = "1016"
    }
}
