package tech.xdge.muxa.app

import android.content.Intent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.window.layout.FoldingFeature
import androidx.window.testing.layout.FoldingFeature
import androidx.window.testing.layout.TestWindowLayoutInfo
import androidx.window.testing.layout.WindowLayoutInfoPublisherRule
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith
import tech.xdge.muxa.app.pane.TerminalPaneView

/**
 * A fold, an unfold or a window resize changes nothing but the layout: the connection,
 * the pane, its screen and the keyboard stay (ADR 0012, 0013). The Android version of
 * the iOS `FoldTests`.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class FoldTest {
    private val compose = createEmptyComposeRule()
    private val hinges = WindowLayoutInfoPublisherRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(hinges).around(compose)
    private var scenario: ActivityScenario<MainActivity>? = null

    @After
    fun close() {
        shell("wm size reset")
        scenario?.close()
    }

    private fun launch() {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        scenario = ActivityScenario.launch(Intent(context, MainActivity::class.java).putExtra(MainActivity.EXTRA_DEMO, true))
    }

    /** Runs [command] to its end: the output is read until the command closes it. */
    private fun shell(command: String) {
        val out = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
        android.os.ParcelFileDescriptor.AutoCloseInputStream(out).use { it.readBytes() }
    }

    /** Types through the input method, as a user does; injected key events can bypass it. */
    private fun type(text: String) = shell("input text $text")

    private fun paneShows(text: String) = SemanticsMatcher("pane shows \"$text\"") {
        it.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    private fun waitForPane(text: String) = compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows(text), TIMEOUT)

    private fun paneFocused(): Boolean {
        var focused = false
        scenario?.onActivity { focused = it.currentFocus is TerminalPaneView }
        return focused
    }

    /**
     * Focus comes back at once, but the input method reconnects to the moved view a
     * moment later; keys sent before that are lost.
     */
    private fun waitForKeyboard() {
        compose.waitUntil(TIMEOUT) { paneFocused() }
        compose.waitForIdle()
        Thread.sleep(INPUT_RECONNECT_MS)
    }

    private fun recreations(): Int {
        var id = 0
        scenario?.onActivity { id = System.identityHashCode(it) }
        return id
    }

    private fun openAgentTerminal() {
        compose.waitUntilAtLeastOneExists(hasTestTag("view-Agent"), TIMEOUT)
        compose.onNodeWithTag("view-Agent").performClick()
        waitForPane("pane %1 100x30")
        compose.onNodeWithTag("pane").performClick()
        compose.waitUntil(TIMEOUT) { paneFocused() }
    }

    @Test
    fun resizingKeepsThePaneItsScreenAndTheKeyboard() {
        launch()
        openAgentTerminal()
        type("one")
        waitForPane("$ one")
        val activity = recreations()

        // The other layout from the one the device starts in: a phone-sized window on a
        // tablet or an unfolded foldable (Back), a tablet-sized one on a phone (Show List).
        val startsWide = compose.onAllNodes(hasTestTag("show-list")).fetchSemanticsNodes().isNotEmpty()
        val (other, start) = if (startsWide) "back" to "show-list" else "show-list" to "back"
        shell(if (startsWide) "wm size 1080x2400" else "wm size 2560x1600")
        compose.waitUntilAtLeastOneExists(hasTestTag(other), TIMEOUT)
        waitForPane("$ one")
        waitForKeyboard()
        type("two")
        waitForPane("$ onetwo")

        // Back to the device's own size and layout, with the same pane and screen.
        shell("wm size reset")
        compose.waitUntilAtLeastOneExists(hasTestTag(start), TIMEOUT)
        waitForPane("$ onetwo")
        waitForKeyboard()
        type("three")
        waitForPane("$ onetwothree")
        assertTrue("the Activity was recreated", activity == recreations())
    }

    @Test
    fun halfOpenLikeABookShowsTheListBesideThePane() {
        launch()
        openAgentTerminal()
        scenario?.onActivity { activity ->
            val book = FoldingFeature(activity, state = FoldingFeature.State.HALF_OPENED, orientation = FoldingFeature.Orientation.VERTICAL)
            hinges.overrideWindowLayoutInfo(TestWindowLayoutInfo(listOf(book)))
        }
        // Both sides at once: the hosts on the left of the hinge, the pane on the right.
        compose.waitUntilAtLeastOneExists(hasTestTag("host-Demo"), TIMEOUT)
        waitForPane("$")
    }

    private companion object {
        const val TIMEOUT = 10_000L
        const val INPUT_RECONNECT_MS = 1_000L
    }
}
