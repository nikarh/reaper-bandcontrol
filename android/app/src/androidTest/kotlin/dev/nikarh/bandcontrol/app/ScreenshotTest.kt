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

/** Width of the screenshots in docs/screenshots. */
private const val WIDTH = 540

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
        // The status and navigation bars are drawn by the system, so the app's capture would show empty bands there,
        // of a height that depends on the device: cut them off, exactly as far as the app pads its content.
        var top = 0
        var bottom = 0
        scenario.onActivity { activity ->
            val insets = ViewCompat.getRootWindowInsets(activity.window.decorView)
            top = insets?.getInsets(WindowInsetsCompat.Type.statusBars())?.top ?: 0
            bottom = insets?.getInsets(WindowInsetsCompat.Type.navigationBars())?.bottom ?: 0
        }
        val cropped = Bitmap.createBitmap(full, 0, top, full.width, full.height - top - bottom)
        // Scaled here rather than on the computer, so the result doesn't depend on its image tools.
        val bitmap = Bitmap.createScaledBitmap(cropped, WIDTH, (cropped.height * WIDTH + cropped.width / 2) / cropped.width, true)
        File(out, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    @Before
    fun launch() {
        out.mkdirs()
        control("reset")
        // In Summer, the second song of the example project (examples/BandControl-Example.RPP).
        control("seek 200")
        // Fixed levels instead of generated ones, so the screenshots don't change from run to run: the inputs
        // (the guitar clips), the PA feeds and the drummer's in-ear mix.
        val levels = mapOf(4 to -9, 5 to -6, 6 to 1, 7 to -4, 9 to -14, 10 to -8, 11 to -11, 13 to -10)
        for ((track, db) in levels) control("meter $track $db")
        val uri = Uri.parse("bandcontrol://pair?s=$secret&n=Band%20PC&t=$tcp")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        context
            .getSharedPreferences("bandcontrol", 0)
            .edit()
            .clear()
            .commit()
        scenario = ActivityScenario.launch(Intent(Intent.ACTION_VIEW, uri).setPackage(context.packageName))
        compose.waitUntilAtLeastOneExists(hasTestTag("current-region") and hasText("Summer"), 20_000)
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
        compose.waitUntilAtLeastOneExists(hasTestTag("peak-6") and hasText("CLIP +1.0"), 10_000)
        shot("inputs")
        compose.onNodeWithTag("tab-foh").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("level-9"), 5_000)
        shot("foh")
        compose.onNodeWithTag("tab-iem").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("output-volume"), 5_000)
        shot("iem")
        compose.onNodeWithContentDescription("Menu").performClick()
        compose.onNodeWithText("Share pairing with another phone").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("share-qr"), 5_000)
        shot("share")
        compose.onNodeWithTag("share-close").performClick()
        compose.onNodeWithContentDescription("Menu").performClick()
        compose.onNodeWithTag("about").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("about-qr"), 5_000)
        shot("get-app")
    }
}
