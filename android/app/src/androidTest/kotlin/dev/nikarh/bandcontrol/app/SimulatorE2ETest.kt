package dev.nikarh.bandcontrol.app

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.net.InetSocketAddress
import java.net.Socket

/**
 * Drives the app against `bandctl sim` running on the development machine, reached through `adb reverse`.
 * Run by `mise run android:e2e`, which starts the simulator and passes its addresses as instrumentation arguments.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class SimulatorE2ETest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val args = InstrumentationRegistry.getArguments()
    private val tcp = args.getString("simTcp") ?: "127.0.0.1:47021"
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
        val uri = Uri.parse("bandcontrol://pair?s=$secret&n=Simulator&t=$tcp")
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
        compose.onNodeWithTag("tab-mix").performClick()
        compose.onNodeWithTag("output-13").performClick()

        compose.onNodeWithTag("mute-2").performClick()
        eventually { track(13).getJSONArray("receives").getJSONObject(0).getBoolean("mute") }

        val before = track(13).getJSONArray("receives").getJSONObject(3).getDouble("volume")
        compose.onNodeWithTag("receive-7").performTouchInput { swipeLeft() }
        eventually { track(13).getJSONArray("receives").getJSONObject(3).getDouble("volume") < before - 0.1 }

        // Changes made in REAPER show up on the phone.
        control("volume 13 0.25")
        compose.waitUntilAtLeastOneExists(hasTestTag("db") and hasText("-12.04 dB"), 5_000)
        control("rename 2 Klick")
        compose.waitUntilAtLeastOneExists(hasText("Klick"), 5_000)
    }
}
