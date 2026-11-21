package tech.xdge.muxa.app

import android.content.Intent
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** The app against its in-app v2 demo server (`--ez demo true`), as the iOS `NextTests`. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class NextTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null

    @After
    fun close() {
        scenario?.close()
    }

    private fun launch() {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        scenario = ActivityScenario.launch(Intent(context, MainActivity::class.java).putExtra(MainActivity.EXTRA_DEMO, true))
        compose.waitUntilAtLeastOneExists(hasTestTag("item.d1"), TIMEOUT)
    }

    @Test
    fun homeShowsTheInboxAndWhoNeedsYou() {
        launch()
        for (id in listOf("d1", "d2", "d3", "d4", "d5", "d6")) compose.waitUntilAtLeastOneExists(hasTestTag("item.$id"), TIMEOUT)
        compose.onNodeWithTag("needsyou.t2").assertExists()
    }

    @Test
    fun aCaptureShowsItsFieldsAndGoesOnTop() {
        launch()
        compose.onNodeWithTag("capture.field").performTextInput("reply Sam about the launch #personal due today")
        compose.waitUntilAtLeastOneExists(hasContentDescription("#Personal, remove"), TIMEOUT)
        compose.onNodeWithTag("capture.add").performClick()
        compose.waitUntilAtLeastOneExists(hasText("Sam about the launch"), TIMEOUT)
        // The new item is first in the list.
        compose.waitUntil(TIMEOUT) { compose.nodeText(compose.onAllNodes(isItem).onFirst().fetchSemanticsNode().id).contains("Sam about the launch") }
    }

    @Test
    fun approveTakesTheReviewOutOfTheList() {
        launch()
        compose.onNodeWithTag("item.d1").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.title"), TIMEOUT)
        compose.onNodeWithTag("detail.title").assertTextEquals("Review fix-login")
        compose.onNodeWithTag("detail.main").performClick()
        compose.backToList()
        compose.waitUntilDoesNotExist(hasTestTag("item.d1"), TIMEOUT)
    }

    @Test
    fun aRoutineRunsNow() {
        launch()
        compose.onNodeWithTag("home.part.Routines").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("routine.release-notes"), TIMEOUT)
        compose.onNodeWithTag("routine.release-notes").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("routine.run"), TIMEOUT)
        compose.onNodeWithTag("routine.run").performClick()
        compose.waitUntilAtLeastOneExists(hasText("Running since"), TIMEOUT)
        compose.onNodeWithTag("routine.run").assertIsNotEnabled()
    }

    @Test
    fun needsYouOpensTheWaitingAgentInFocus() {
        launch()
        compose.onNodeWithTag("needsyou.t2").performClick()
        compose.waitUntilAtLeastOneExists(screenContains("Allow edits to README.md"), TIMEOUT)
    }

    @Test
    fun typingInFocusReachesTheTerminal() {
        launch()
        compose.onNodeWithTag("mode.focus").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("terminal.t3"), TIMEOUT)
        compose.onNodeWithTag("terminal.t3").performClick()
        compose.waitUntilAtLeastOneExists(screenContains("$"), TIMEOUT)
        // The view takes keys as the input method sends them.
        scenario?.onActivity { activity ->
            val view = activity.window.decorView.findTerminal() ?: error("no terminal view")
            view.requestFocus()
            view.onCreateInputConnection(android.view.inputmethod.EditorInfo()).commitText("echo hi", 1)
        }
        compose.waitUntilAtLeastOneExists(screenContains("echo hi"), TIMEOUT)
        // The Inbox panel opens beside the terminal.
        compose.onNodeWithTag("focus.inbox").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("item.d1"), TIMEOUT)
    }

    @Test
    fun disconnectReturnsToTheHostsAndAHostCanBeAdded() {
        launch()
        compose.onNodeWithTag("home.host").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("home.disconnect"), TIMEOUT)
        compose.onNodeWithTag("home.disconnect").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("next.addhost"), TIMEOUT)
        compose.onNodeWithTag("next.addhost").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("host-address"), TIMEOUT)
        compose.onNodeWithTag("host-name").performTextInput("Nowhere")
        compose.onNodeWithTag("host-address").performTextInput("127.0.0.1")
        compose.onNodeWithTag("host-user").performTextInput("me")
        // Nothing answers on the emulator's own port 22: the connection fails at once.
        compose.onNodeWithTag("host-connect").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("host.Nowhere"), TIMEOUT)
        compose.waitUntil(CONNECT_TIMEOUT) {
            compose.nodeText(compose.onNodeWithTag("next.link").fetchSemanticsNode().id) !in listOf("Connecting…", "Not connected")
        }
        compose.onNodeWithText("Try the Demo").assertExists()
    }

    private companion object {
        const val TIMEOUT = 10_000L
        const val CONNECT_TIMEOUT = 20_000L
    }
}

private val isItem = SemanticsMatcher("an Inbox item") { it.config.getOrNull(SemanticsProperties.TestTag)?.startsWith("item.") == true }

/** The terminal's screen, which the pane reports as its state. */
private fun screenContains(text: String) = SemanticsMatcher("screen contains \"$text\"") { node ->
    node.config.getOrNull(SemanticsProperties.TestTag) == "focus.pane" &&
        node.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
}

private fun ComposeTestRule.nodeText(id: Int): String {
    val node = onAllNodes(SemanticsMatcher("id $id") { it.id == id }, useUnmergedTree = true).onFirst().fetchSemanticsNode()
    val texts = mutableListOf<String>()
    fun walk(n: SemanticsNode) {
        n.config.getOrNull(SemanticsProperties.Text)?.forEach { texts += it.text }
        n.children.forEach(::walk)
    }
    walk(node)
    return texts.joinToString(" ")
}

/** A phone shows the detail over the list; go back to it. A tablet shows both. */
@OptIn(ExperimentalTestApi::class)
private fun ComposeTestRule.backToList() {
    if (onAllNodesWithTag("detail.back").fetchSemanticsNodes().isNotEmpty()) onNodeWithTag("detail.back").performClick()
}

private fun android.view.View.findTerminal(): tech.xdge.muxa.app.pane.TerminalPaneView? {
    if (this is tech.xdge.muxa.app.pane.TerminalPaneView) return this
    if (this is android.view.ViewGroup) for (i in 0 until childCount) getChildAt(i).findTerminal()?.let { return it }
    return null
}
