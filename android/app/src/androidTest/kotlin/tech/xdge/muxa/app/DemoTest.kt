package tech.xdge.muxa.app

import android.content.Intent
import android.view.KeyEvent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Drives the app against its in-app demo bridge: no host needed. Ported from the iOS `DemoTests`. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class DemoTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null

    private fun launch(alerts: Boolean = false) {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(MainActivity.EXTRA_DEMO, true)
            .putExtra(MainActivity.EXTRA_DEMO_ALERTS, alerts)
        scenario = ActivityScenario.launch(intent)
    }

    @After
    fun close() {
        scenario?.close()
    }

    private fun paneShows(text: String) = SemanticsMatcher("pane shows \"$text\"") {
        it.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    private fun waitForPane(text: String) {
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows(text), TIMEOUT)
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    /** A hardware key press with modifiers, as a keyboard case sends it. */
    private fun key(code: Int, meta: Int = 0) {
        val now = android.os.SystemClock.uptimeMillis()
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0, meta))
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0, meta))
    }

    /** The tap reaches the pane view a frame later; typing before it has focus goes nowhere. */
    private fun waitForPaneFocus() {
        compose.waitUntil(TIMEOUT) {
            var focused = false
            scenario?.onActivity { focused = it.currentFocus is tech.xdge.muxa.app.pane.TerminalPaneView }
            focused
        }
    }

    private fun shortcut(code: Int) = key(code, KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON)

    private fun type(text: String) = InstrumentationRegistry.getInstrumentation().sendStringSync(text)

    private fun showAgentTerminal() {
        waitForTag("view-Agent")
        compose.onNodeWithTag("view-Agent").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("composer"), TIMEOUT)
    }

    @Test
    fun typingReachesThePaneAndShortcutsSwitch() {
        launch()
        // The agent pane opens in Pretty; Ctrl+Shift+R shows the terminal.
        waitForTag("composer")
        shortcut(KeyEvent.KEYCODE_R)
        waitForPane("pane %1 100x30")
        compose.onNodeWithTag("pane").performClick()
        waitForPaneFocus()
        type("hello")
        waitForPane("$ hello")
        key(KeyEvent.KEYCODE_C, KeyEvent.META_CTRL_ON)
        waitForPane("<C-c>")

        // Ctrl+Shift+J: the terminal side of the same session.
        shortcut(KeyEvent.KEYCODE_J)
        waitForPane("pane %2 100x30")
        // Ctrl+Shift+4: the fourth session in the list's repo order, a bigger pane.
        shortcut(KeyEvent.KEYCODE_4)
        waitForPane("pane %3 132x43")
    }

    @Test
    fun prettyViewSendsPromptsAndSwitchesToRaw() {
        launch()
        waitForTag("composer")
        // The chat opens at its newest message; the first one is a scroll up.
        compose.waitUntilAtLeastOneExists(hasTestTag("messages"), TIMEOUT)
        compose.onNodeWithTag("messages").performScrollToNode(hasText("Fix the failing test in the parser"))
        compose.onNodeWithTag("composer").performTextInput("hello")
        compose.onNodeWithTag("send").performClick()
        compose.waitUntilAtLeastOneExists(hasText("You said", substring = true), TIMEOUT)

        // The prompt went into the pane itself.
        compose.onNodeWithTag("view-Agent").performClick()
        waitForPane("$ hello")
        compose.onNodeWithTag("view-Chat").performClick()
        waitForTag("composer")

        // While the agent waits on a prompt, Send is off and the answer keys show.
        compose.onNodeWithTag("composer").performTextInput("/wait")
        compose.onNodeWithTag("send").performClick()
        waitForTag("waiting")
        compose.onNodeWithTag("composer").performTextInput("x")
        compose.onNodeWithTag("send").assertIsNotEnabled()
        compose.onNodeWithTag("quick-1").assertExists()

        // One switch for the session: Chat and Agent are the agent pane, Shell the other.
        compose.onNodeWithTag("view-Shell").performClick()
        waitForPane("pane %2 100x30")
        compose.onNodeWithTag("view-Chat").performClick()
        waitForTag("composer")
    }

    @Test
    fun approvalCardAnswersWithTheOptionsKeys() {
        launch()
        waitForTag("composer")
        compose.onNodeWithTag("composer").performTextInput("/allow")
        compose.onNodeWithTag("send").performClick()
        waitForTag("approval")
        compose.onNode(hasText("Allow Bash?") and hasAnyAncestor(hasTestTag("approval"))).assertExists()
        compose.onNode(hasText("rm -rf build") and hasAnyAncestor(hasTestTag("approval"))).assertExists()
        compose.onNodeWithTag("approval-Allow").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("approval"), TIMEOUT)

        compose.onNodeWithTag("composer").performTextInput("/ask")
        compose.onNodeWithTag("send").performClick()
        waitForTag("approval")
        compose.onNode(hasText("Which format should the config use?") and hasAnyAncestor(hasTestTag("approval"))).assertExists()
        compose.onNodeWithTag("approval-JSON").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("approval"), TIMEOUT)

        // The keys went to the pane itself.
        showAgentTerminal()
        waitForPane("<1>")
        waitForPane("<2>")
    }

    @Test
    fun theKeyBarSendsKeysTheKeyboardLacks() {
        launch()
        waitForTag("composer")
        showAgentTerminal()
        waitForPane("pane %1 100x30")
        compose.onNodeWithTag("pane").performClick()
        // The bar shows over the on-screen keyboard only; a hardware keyboard hides it.
        val shown = runCatching { compose.waitUntilAtLeastOneExists(hasTestTag("keybar-ESCAPE"), TIMEOUT) }.isSuccess
        assumeTrue("the emulator has a hardware keyboard, which hides the bar", shown)
        compose.onNodeWithTag("keybar-ESCAPE").performClick()
        waitForPane("<Escape>")
        compose.onNodeWithTag("keybar-UP").performClick()
        waitForPane("<Up>")
        // Ctrl is one-shot.
        compose.onNodeWithTag("keybar-CONTROL").performClick()
        compose.onNodeWithTag("keybar-INTERRUPT").performClick()
        waitForPane("<C-c>")
    }

    @Test
    fun theReaderShowsChangesAndImages() {
        launch()
        waitForTag("composer")
        shortcut(KeyEvent.KEYCODE_D)
        waitForTag("reader")
        // On a phone the Reader opens on the plan; the list is one Back away.
        if (compose.onAllNodes(hasTestTag("reader-list")).fetchSemanticsNodes().isNotEmpty()) {
            compose.onNodeWithTag("reader-list").performClick()
        }
        waitForTag("reader-tab-Changes")
        compose.onNodeWithTag("reader-tab-Changes").performClick()
        waitForTag("change-src/parser.rs")
        compose.onNodeWithTag("branch-header").assertExists()
        compose.onNodeWithText("Since origin/main · 3 commits").assertExists()
        compose.onNodeWithTag("change-assets/logo.png").performClick()
        compose.waitUntilAtLeastOneExists(hasText("Binary File"), TIMEOUT)

        if (compose.onAllNodes(hasTestTag("reader-list")).fetchSemanticsNodes().isNotEmpty()) {
            compose.onNodeWithTag("reader-list").performClick()
        }
        compose.onNodeWithTag("reader-tab-Images").performClick()
        waitForTag("image-build/screenshots/en/login.png")
        compose.onNodeWithTag("image-build/screenshots/en/login.png").performClick()
        waitForTag("image-view")
        compose.onNodeWithText("2 of 2").assertExists()
        compose.onNodeWithTag("image-previous").assertIsEnabled().performClick()
        compose.waitUntilAtLeastOneExists(hasText("1 of 2"), TIMEOUT)
    }

    @Test
    fun aWaitingAgentElsewhereIsAnnounced() {
        launch(alerts = true)
        waitForTag("agent-alert")
        compose.onNodeWithText("fix-login: needs your input", substring = true).assertExists()
        compose.onNodeWithTag("agent-alert").performClick()
        // The bar's title, not the banner's own text, which also names the session.
        compose.waitUntilDoesNotExist(hasTestTag("agent-alert"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasText("fix-login", substring = false), TIMEOUT)
    }

    @Test
    fun aNewTaskOpensItsSession() {
        launch()
        waitForTag("composer")
        shortcut(KeyEvent.KEYCODE_N)
        waitForTag("task-branch")
        compose.onNodeWithTag("task-branch").performTextInput("taken")
        compose.onNodeWithTag("task-start").performClick()
        waitForTag("task-error")
        compose.onNodeWithText("already exists", substring = true).assertExists()
    }

    @Test
    fun closingTheShownTaskOpensTheNextOne() {
        launch()
        waitForTag("composer")
        compose.waitUntilAtLeastOneExists(hasText("feature-auth"), TIMEOUT)
        compose.onNodeWithTag("more").performClick()
        compose.onNodeWithText("Close Task…").performClick()
        compose.onNodeWithText("Close").performClick()
        compose.waitUntilAtLeastOneExists(hasText("fix-login"), TIMEOUT)
    }

    private companion object {
        const val TIMEOUT = 10_000L
    }
}
