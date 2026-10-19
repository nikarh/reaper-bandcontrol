package dev.nikarh.bandcontrol.app

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.net.InetSocketAddress
import java.net.Socket

/**
 * Saves the app screenshots of the README and the website, from a fixed state of the simulated example project.
 * Run by `scripts/docs-screenshots.sh app`; the names match `docs/features.toml`.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class ScreenshotTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val args = InstrumentationRegistry.getArguments()
    private val tcp = args.getString("simTcp") ?: "127.0.0.1:47021"
    private val controlAddress = args.getString("simControl") ?: "127.0.0.1:47022"
    private val secret = args.getString("secret") ?: "AAAAAAAAAAAAAAAAAAAAAAAAAA"
    private val out = File(InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(null), "screenshots")
    private lateinit var scenario: ActivityScenario<MainActivity>

    private fun control(command: String): String {
        val (host, port) = controlAddress.split(':')
        return Socket().use { s ->
            s.connect(InetSocketAddress(host, port.toInt()), 5_000)
            s.getOutputStream().write("$command\n".toByteArray())
            s.getInputStream().bufferedReader().readLine()
        }
    }

    private fun shot(name: String) {
        compose.waitForIdle()
        // Meter frames and animations settle.
        Thread.sleep(1_500)
        compose.waitForIdle()
        val full = compose.onRoot().captureToImage().asAndroidBitmap()
        // The navigation bar area is drawn by the system, not the app.
        var bottom = 0
        scenario.onActivity { activity ->
            bottom =
                ViewCompat
                    .getRootWindowInsets(activity.window.decorView)
                    ?.getInsets(WindowInsetsCompat.Type.navigationBars())
                    ?.bottom ?: 0
        }
        val bitmap = Bitmap.createBitmap(full, 0, 0, full.width, full.height - bottom)
        File(out, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    @Before
    fun launch() {
        out.mkdirs()
        control("reset")
        control("seek 200")
        // Fixed levels instead of generated ones, so the screenshots don't change from run to run.
        val levels =
            mapOf(6 to -9, 7 to -6, 8 to 1, 9 to -2, 10 to -12, 11 to -18, 12 to -15, 27 to -10) +
                (18..25).associateWith { -20 + it % 4 * 3 }
        for ((track, db) in levels) control("meter $track $db")
        val uri = Uri.parse("bandcontrol://pair?s=$secret&n=Band%20PC&t=$tcp")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        context
            .getSharedPreferences("bandcontrol", 0)
            .edit()
            .clear()
            .commit()
        scenario = ActivityScenario.launch(Intent(Intent.ACTION_VIEW, uri).setPackage(context.packageName))
        compose.waitUntilAtLeastOneExists(hasTestTag("current-region") and hasText("Paper Lions"), 20_000)
    }

    @After
    fun close() {
        scenario.close()
    }

    @Test
    fun screenshots() {
        shot("playback")
        compose.onNodeWithTag("edit-regions").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("region-editor"), 5_000)
        shot("setlist")
        compose.onNodeWithTag("edit-regions").performClick()
        compose.onNodeWithTag("tab-inputs").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-8") and hasText("CLIP +1.0"), 10_000)
        shot("inputs")
        compose.onNodeWithTag("tab-pa").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("level-18"), 5_000)
        shot("pa")
        compose.onNodeWithTag("tab-iem").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("mix-title"), 5_000)
        shot("iem")
        compose.onNodeWithContentDescription("Menu").performClick()
        compose.onNodeWithText("Share pairing with another phone").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("share-qr"), 5_000)
        shot("share")
    }
}
