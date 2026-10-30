package tech.xdge.muxa.app

import android.content.Intent
import android.view.KeyEvent
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Play Store screenshots, against the demo bridge's showcase data. Ported from the iOS
 * `ShowcaseTests`. Runs only with the instrumentation argument `muxaShowcase=true`
 * (`make screenshots-android`), and writes to [DIR] on the device.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class ShowcaseTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null

    @Before
    fun onlyWhenAsked() {
        assumeTrue("set muxaShowcase=true to take the screenshots", InstrumentationRegistry.getArguments().getString("muxaShowcase") == "true")
        shell("mkdir -p $DIR")
    }

    @After
    fun close() {
        scenario?.close()
    }

    @Test
    fun screenshots() {
        launch()
        waitForTag("waiting")
        snap("1-chat")

        click("view-Agent")
        waitForTag("pane")
        snap("2-agent")

        // A phone's session switcher is its list; a tablet has the keyboard's.
        if (exists("back")) {
            click("back")
            waitForTag("host-Mac mini")
            snap("3-switcher")
            click("session-stripe-webhooks")
            waitForTag("view-Agent")
        } else {
            shortcut(KeyEvent.KEYCODE_K)
            waitForTag("switcher-search")
            snap("3-switcher")
            InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(KeyEvent.KEYCODE_ESCAPE)
            compose.waitUntilDoesNotExist(hasTestTag("switcher-search"), TIMEOUT)
        }

        // By touch: a chord right after the switcher closes can reach the window before focus does.
        if (exists("documents")) click("documents") else { click("more"); click("menu-documents") }
        waitForTag("reader")
        // The page draws its diagram after it loads.
        Thread.sleep(RENDER_MS)
        snap("4-plan")

        // On a phone the Reader opens on the plan; the list is one Back away.
        if (exists("reader-list")) click("reader-list")
        click("reader-tab-Changes")
        click("change-src/webhooks/router.rs")
        // The list's search field had the keyboard; the shot must not catch it sliding away.
        hideKeyboard()
        Thread.sleep(RENDER_MS)
        snap("5-diff")
        click(if (exists("reader-done")) "reader-done" else "reader-close")

        click("view-Shell")
        waitForTag("pane")
        snap("6-shell")
    }

    /** Another agent starts waiting 10 seconds after launch. */
    @Test
    fun alertScreenshot() {
        launch(alerts = true)
        waitForTag("waiting")
        compose.waitUntil(ALERT_MS) { exists("agent-alert") }
        // Past the slide-in.
        Thread.sleep(600)
        snap("7-alert")
    }

    private fun launch(alerts: Boolean = false) {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(MainActivity.EXTRA_DEMO, true)
            .putExtra(MainActivity.EXTRA_SHOWCASE, true)
            .putExtra(MainActivity.EXTRA_DEMO_ALERTS, alerts)
        scenario = ActivityScenario.launch(intent)
    }

    private fun exists(tag: String) = compose.onAllNodes(hasTestTag(tag)).fetchSemanticsNodes().isNotEmpty()

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun click(tag: String) {
        waitForTag(tag)
        compose.onNodeWithTag(tag).performClick()
    }

    private fun shortcut(code: Int) {
        val now = android.os.SystemClock.uptimeMillis()
        val meta = KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0, meta))
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0, meta))
    }

    private fun hideKeyboard() {
        scenario?.onActivity { WindowCompat.getInsetsController(it.window, it.window.decorView).hide(WindowInsetsCompat.Type.ime()) }
        compose.waitForIdle()
    }

    /** Without the on-screen keyboard, which would cover half the shot. */
    private fun snap(name: String) {
        hideKeyboard()
        // The keyboard slides away and the cursor blink settles.
        Thread.sleep(SETTLE_MS)
        shell("screencap -p $DIR/$name.png")
    }

    /** Runs as the shell user, which can write [DIR]; waits until the command ends. */
    private fun shell(command: String) {
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command).use { fd ->
            android.os.ParcelFileDescriptor.AutoCloseInputStream(fd).use { it.readBytes() }
        }
    }

    private companion object {
        const val DIR = "/data/local/tmp/muxa-showcase"
        const val TIMEOUT = 10_000L
        const val ALERT_MS = 20_000L
        const val RENDER_MS = 3_000L
        const val SETTLE_MS = 1_500L
    }
}
