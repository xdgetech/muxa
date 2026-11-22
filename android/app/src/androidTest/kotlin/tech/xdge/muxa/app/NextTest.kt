package tech.xdge.muxa.app

import android.content.Intent
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
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
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
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
    fun removeAnywayClosesAReviewThatWouldLoseWork() {
        launch()
        compose.onNodeWithTag("inbox.list").performScrollToNode(hasTestTag("item.d8"))
        compose.onNodeWithTag("item.d8").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.title"), TIMEOUT)
        compose.onNodeWithTag("detail.title").assertTextEquals("Review docs-audit")
        compose.onNodeWithTag("detail.main").performClick()
        // The demo refuses the first Approve with would-lose-work; the dialog shows its message.
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.loss"), TIMEOUT)
        compose.onNodeWithTag("detail.loss").assert(hasText("has uncommitted changes", substring = true))
        compose.onNodeWithTag("detail.force").assert(hasText("Remove Anyway")).performClick()
        compose.waitUntilDoesNotExist(hasTestTag("detail.loss"), TIMEOUT)
        compose.backToList()
        compose.waitUntilDoesNotExist(hasTestTag("item.d8"), TIMEOUT)
    }

    // Settings reads the host's settings since ADR 0074, and ADR 0075 gives every agent all its
    // Workspace's connections: the demo's settings have no linear connection, and nothing is "Default".
    @Test
    fun settingsShowsWhoUsesEachConnectionAndMcpServer() {
        launch()
        compose.openWorkspaceSettings("app")
        compose.onNodeWithTag("settings.tab.CONNECTIONS").performClick()
        compose.onNodeWithTag("settings.tools.app").assert(hasText("gets all its connections and MCP servers", substring = true))
        compose.onNodeWithTag("settings.connection.app.github").performScrollTo()
        compose.waitUntil(TIMEOUT) {
            val text = compose.nodeText(compose.onNodeWithTag("settings.connection.app.github").fetchSemanticsNode().id)
            // A Task by its title, from the terminal list; a Routine by its name.
            "Tasks: fix-login, docs-audit" in text && "dependency-digest" in text && "last used" in text
        }
        compose.onNodeWithTag("settings.connection.app.github.secret", useUnmergedTree = true).assert(hasText("set"))
        compose.onNodeWithTag("settings.tab.MCP").performScrollTo().performClick()
        compose.onNodeWithTag("settings.mcp.app.docs").performScrollTo()
        compose.onNodeWithTag("settings.strict.app").performScrollTo().assert(hasText("only these MCP servers", substring = true))
        compose.onNodeWithTag("settings.back").performScrollTo().performClick()
        compose.openWorkspaceSettings("Personal")
        compose.onNodeWithTag("settings.tab.MCP").performClick()
        compose.onNodeWithTag("settings.strict.Personal").assertDoesNotExist()
    }

    @Test
    fun aWorkspaceIsAdded() {
        launch()
        compose.openSettings()
        compose.onNodeWithTag("settings.workspace.add").performScrollTo().performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("workspace.name"), TIMEOUT)
        compose.onNodeWithTag("workspace.name").performTextInput("Lab")
        compose.onNodeWithTag("workspace.root").performTextInput("/demo/lab")
        compose.onNodeWithTag("settings.form.save").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("settings.workspace.Lab"), TIMEOUT)
        compose.onNodeWithTag("settings.workspace.Lab").assert(hasText("/demo/lab", substring = true))
    }

    @Test
    fun aConnectionWithASecretShowsSet() {
        launch()
        compose.openWorkspaceSettings("app")
        compose.onNodeWithTag("settings.tab.CONNECTIONS").performClick()
        compose.onNodeWithTag("settings.connection.add").performScrollTo().performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("connection.name"), TIMEOUT)
        compose.onNodeWithTag("connection.name").performTextInput("linear")
        compose.onNodeWithTag("connection.env").performTextInput("LINEAR_API_KEY")
        compose.onNodeWithTag("connection.secret").performScrollTo().performTextInput("s3cret-value")
        compose.onNodeWithTag("settings.form.save").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("settings.connection.app.linear"), TIMEOUT)
        compose.onNodeWithTag("settings.connection.app.linear.secret", useUnmergedTree = true).assert(hasText("set"))
        // The secret is write-only: nothing shows it.
        check(compose.onAllNodes(hasText("s3cret-value", substring = true), useUnmergedTree = true).fetchSemanticsNodes().isEmpty())
    }

    @Test
    fun anMcpServerIsAdded() {
        launch()
        compose.openWorkspaceSettings("app")
        compose.onNodeWithTag("settings.tab.MCP").performClick()
        compose.onNodeWithTag("settings.mcp.add").performScrollTo().performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("mcp.name"), TIMEOUT)
        compose.onNodeWithTag("mcp.name").performTextInput("search")
        compose.onNodeWithTag("mcp.command").performTextInput("npx")
        compose.onNodeWithTag("mcp.args").performTextInput("search-mcp")
        compose.onNodeWithTag("settings.form.save").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("settings.mcp.app.search"), TIMEOUT)
        compose.onNodeWithTag("settings.mcp.app.search").assert(hasText("npx search-mcp", substring = true))
    }

    @Test
    fun aRoutinesInstructionsAreEdited() {
        launch()
        compose.openWorkspaceSettings("app")
        compose.onNodeWithTag("settings.tab.ROUTINES").performClick()
        compose.onNodeWithTag("settings.routine.app.dependency-digest").performScrollTo().performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("routine.instructions"), TIMEOUT)
        compose.onNodeWithTag("routine.instructions").assert(hasText("List the dependency updates", substring = true))
        compose.onNodeWithTag("routine.instructions").performScrollTo().performTextReplacement("Only the security fixes.")
        compose.onNodeWithTag("settings.form.save").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("routine.instructions"), TIMEOUT)
        compose.onNodeWithTag("settings.routine.app.dependency-digest").performScrollTo().performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("routine.instructions"), TIMEOUT)
        compose.onNodeWithTag("routine.instructions").assert(hasText("Only the security fixes."))
    }

    @Test
    fun requestChangesTypesTheCommentToTheAgent() {
        launch()
        compose.onNodeWithTag("item.d1").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.changes"), TIMEOUT)
        compose.onNodeWithTag("detail.changes").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.comment"), TIMEOUT)
        compose.onNodeWithTag("detail.comment").performTextInput("Use a 30 s margin")
        compose.onNodeWithTag("detail.comment.send").performClick()
        compose.backToList()
        compose.waitUntilDoesNotExist(hasTestTag("item.d1"), TIMEOUT)
        compose.onNodeWithTag("mode.focus").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("terminal.t1"), TIMEOUT)
        compose.onNodeWithTag("terminal.t1").performClick()
        compose.waitUntilAtLeastOneExists(screenContains("Use a 30 s margin"), TIMEOUT)
    }

    @Test
    fun retryRunsTheFailedRoutineAgain() {
        launch()
        compose.onNodeWithTag("item.d5").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.main"), TIMEOUT)
        compose.onNodeWithTag("detail.main").assertTextEquals("Retry")
        compose.onNodeWithTag("detail.main").performClick()
        compose.backToList()
        compose.waitUntilDoesNotExist(hasTestTag("item.d5"), TIMEOUT)
        compose.onNodeWithTag("home.part.Routines").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("routine.nightly-backup"), TIMEOUT)
        compose.onNodeWithTag("routine.nightly-backup").performClick()
        compose.waitUntilAtLeastOneExists(hasText("Running since"), TIMEOUT)
    }

    @Test
    fun aTaskStartsFromTheSheetWithALoop() {
        launch()
        // New Task sits with the Tasks: the phone's Workspaces tab, the tablet's sidebar.
        compose.onNodeWithTag("mode.focus").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("home.newtask"), TIMEOUT)
        compose.onNodeWithTag("home.newtask").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("task.repo"), TIMEOUT)
        compose.onNodeWithTag("task.title").performTextInput("Add rate limits")
        compose.onNodeWithTag("task.repo").performScrollTo().performClick()
        compose.onNodeWithTag("task.repo.app").performClick()
        compose.onNodeWithTag("task.branch").performScrollTo().performTextInput("rate-limits")
        compose.onNodeWithTag("task.loop").performScrollTo().performClick()
        compose.onNodeWithTag("task.check.make lint").performScrollTo().performClick()
        compose.onNodeWithTag("task.start").performClick()
        compose.waitUntilAtLeastOneExists(screenContains("Starting the agent for Add rate limits"), TIMEOUT)
        compose.onNodeWithTag("task.sheet").assertDoesNotExist()
        compose.waitUntilAtLeastOneExists(hasText("Loop 0/3"), TIMEOUT)
    }

    @Test
    fun startTaskSaysWhatTheWorkspacesAgentsGet() {
        launch()
        compose.onNodeWithTag("mode.focus").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("home.newtask"), TIMEOUT)
        compose.onNodeWithTag("home.newtask").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("task.tools"), TIMEOUT)
        compose.onNodeWithTag("task.tools").performScrollTo().assert(hasText("This Workspace's agents get: github, linear · MCP: docs"))
        compose.onNodeWithTag("task.connection.github").assertDoesNotExist()
    }

    @Test
    fun startTaskOnAnItemTakesItsTitle() {
        launch()
        compose.onNodeWithTag("item.d6").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.starttask"), TIMEOUT)
        compose.onNodeWithTag("detail.starttask").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("task.title"), TIMEOUT)
        compose.onNodeWithTag("task.title").assert(hasText("Renew the domain"))
        compose.waitUntilAtLeastOneExists(hasTestTag("task.workspace") and hasText("Workspace: Personal"), TIMEOUT)
    }

    @Test
    fun resumeTaskOpensTheAgentAndTakesTheItemOutOfTheInbox() {
        launch()
        compose.onNodeWithTag("inbox.list").performScrollToNode(hasTestTag("item.d9"))
        compose.onNodeWithTag("item.d9").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.resume"), TIMEOUT)
        compose.onNodeWithTag("detail.title").assertTextEquals("Resume: Tidy the release notes")
        compose.onNodeWithTag("detail.starttask").assertDoesNotExist()
        compose.onNodeWithTag("detail.resume").performClick()
        compose.waitUntilAtLeastOneExists(screenContains("Starting the agent for Tidy the release notes"), TIMEOUT)
        // A phone shows the terminal alone: Back leads to the tabs.
        if (compose.onAllNodesWithTag("focus.back").fetchSemanticsNodes().isNotEmpty()) compose.onNodeWithTag("focus.back").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("home.part.Inbox"), TIMEOUT)
        compose.onNodeWithTag("home.part.Inbox").performClick()
        compose.backToList()
        compose.waitUntilAtLeastOneExists(hasTestTag("item.d1"), TIMEOUT)
        // Scrolling the whole list finds no d9.
        compose.waitUntil(TIMEOUT) {
            runCatching { compose.onNodeWithTag("inbox.list").performScrollToNode(hasTestTag("item.d9")) }.isFailure
        }
    }

    @Test
    fun anItemsNoteShowsAsMarkdown() {
        launch()
        compose.onNodeWithTag("item.d3").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.note"), TIMEOUT)
        compose.onNode(hasText("pgvector", substring = true), useUnmergedTree = true).assertExists()
        compose.onNode(hasText("Plan: upgrade them", substring = true), useUnmergedTree = true).assertExists()
        compose.onAllNodes(hasText("**", substring = true), useUnmergedTree = true).assertCountEquals(0)
        compose.onAllNodes(hasText("- `", substring = true), useUnmergedTree = true).assertCountEquals(0)
    }

    @Test
    fun makeItATaskOnAReadItemTakesItsTitleAndNote() {
        launch()
        compose.onNodeWithTag("item.d4").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("detail.maketask"), TIMEOUT)
        compose.onNodeWithTag("detail.starttask").assertDoesNotExist()
        compose.onNodeWithTag("detail.maketask").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("task.title"), TIMEOUT)
        compose.onNodeWithTag("task.title").assert(hasText("Weekly dependency digest"))
        compose.onNodeWithTag("task.goal").assert(hasText("3 updates, 1 security fix."))
        compose.waitUntilAtLeastOneExists(hasTestTag("task.workspace") and hasText("Workspace: app"), TIMEOUT)
    }

    @Test
    fun otherTerminalsSitApartFromTheTasksAndStartClosed() {
        launch()
        compose.onNodeWithTag("mode.focus").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("terminal.t1"), TIMEOUT)
        compose.onNodeWithTag("terminal.t2").assertExists()
        compose.onNodeWithTag("terminal.t3").assertDoesNotExist()
        compose.onNodeWithTag("terminal.t4").assertDoesNotExist()
        compose.onNodeWithTag("terminals.other").assert(hasText("Other terminals", ignoreCase = true)).performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("terminal.t4"), TIMEOUT)
        // Without a name or a label, a terminal is called by its id.
        compose.onNodeWithTag("terminal.t4").assert(hasText("Terminal t4"))
        compose.onNodeWithTag("terminal.t3").assert(hasText("shell"))
        compose.onNodeWithTag("terminals.other").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("terminal.t4"), TIMEOUT)
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
        // The shell has no muxa labels: it is under Other terminals, closed by default.
        compose.waitUntilAtLeastOneExists(hasTestTag("terminals.other"), TIMEOUT)
        compose.onNodeWithTag("terminals.other").performClick()
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
    fun theReaderShowsATasksPlanAndChanges() {
        launch()
        compose.onNodeWithTag("needsyou.t2").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("focus.reader"), TIMEOUT)
        compose.onNodeWithTag("focus.reader").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("reader.file.PLAN.md"), TIMEOUT)
        compose.onNodeWithTag("reader.file.PLAN.md").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("reader"), TIMEOUT)
        compose.onNodeWithTag("reader.back").performClick()
        compose.onNodeWithTag("reader.kind.changes").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("reader.file.src/auth/session.ts"), TIMEOUT)
        compose.onNodeWithTag("reader.kind.images").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("reader.file.screenshots/login.png"), TIMEOUT)
        compose.onNodeWithTag("reader.file.screenshots/login.png").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("reader.image"), TIMEOUT)
    }

    @Test
    fun aLinkInADocumentOpensTheLinkedDocument() {
        launch()
        compose.onNodeWithTag("needsyou.t2").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("focus.reader"), TIMEOUT)
        compose.onNodeWithTag("focus.reader").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("reader.file.PLAN.md"), TIMEOUT)
        compose.onNodeWithTag("reader.file.PLAN.md").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("reader"), TIMEOUT)
        // Click the link in the page as a tap would, until the page has rendered it.
        compose.waitUntil(TIMEOUT) {
            scenario?.onActivity { activity ->
                activity.window.decorView.findWebView()?.evaluateJavascript(
                    "document.querySelector('a[href=\".muxa/TASK.md\"]')?.click()", null,
                )
            }
            compose.onAllNodes(hasTestTag("reader.path") and hasText("TASK.md")).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag("reader.back").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("reader.path") and hasText("PLAN.md"), TIMEOUT)
    }

    @Test
    fun aTasksLoopStartsAndStops() {
        launch()
        compose.onNodeWithTag("needsyou.t2").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("focus.loop"), TIMEOUT)
        compose.onNodeWithTag("focus.loop").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("loop.start"), TIMEOUT)
        compose.onNodeWithTag("loop.start").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("loop.start"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasText("Loop 0/3"), TIMEOUT)
        compose.onNodeWithTag("focus.loop").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("loop.now"), TIMEOUT)
        compose.onNodeWithTag("loop.stop").performClick()
        compose.waitUntilAtLeastOneExists(hasText("Stopped"), TIMEOUT)
    }

    @Test
    fun chatShowsTheConversationAndAnswersThePrompt() {
        launch()
        compose.onNodeWithTag("needsyou.t2").performClick()
        compose.waitUntilAtLeastOneExists(screenContains("Allow edits to README.md"), TIMEOUT)
        compose.onNodeWithTag("focus.chat").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("chat.prompt"), TIMEOUT)
        compose.onNodeWithText("Fix the README links.").assertExists()
        compose.onNodeWithTag("chat.option.Allow").performClick()
        // The demo's program echoes what the option sends.
        compose.waitUntilAtLeastOneExists(screenContains("(y/n) y"), TIMEOUT)
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

/** A phone has a Settings tab; a tablet opens Settings from the host in the sidebar. */
@OptIn(ExperimentalTestApi::class)
private fun ComposeTestRule.openSettings() {
    if (onAllNodesWithTag("home.settings").fetchSemanticsNodes().isNotEmpty()) {
        onNodeWithTag("home.settings").performClick()
    } else {
        onNodeWithTag("home.host").performClick()
        waitUntilAtLeastOneExists(hasTestTag("home.host.settings"), WAIT)
        onNodeWithTag("home.host.settings").performClick()
    }
    waitUntilAtLeastOneExists(hasTestTag("settings.workspaces"), WAIT)
    onNodeWithTag("settings.secrets").assert(hasText("Keychain", substring = true))
}

@OptIn(ExperimentalTestApi::class)
private fun ComposeTestRule.openWorkspaceSettings(name: String) {
    if (onAllNodesWithTag("settings.workspaces").fetchSemanticsNodes().isEmpty()) openSettings()
    onNodeWithTag("settings.workspace.$name").performScrollTo().performClick()
    waitUntilAtLeastOneExists(hasTestTag("settings.detail.$name"), WAIT)
}

private const val WAIT = 10_000L

/** A phone shows the detail over the list; go back to it. A tablet shows both. */
@OptIn(ExperimentalTestApi::class)
private fun ComposeTestRule.backToList() {
    if (onAllNodesWithTag("detail.back").fetchSemanticsNodes().isNotEmpty()) onNodeWithTag("detail.back").performClick()
}

private fun android.view.View.findWebView(): android.webkit.WebView? {
    if (this is android.webkit.WebView) return this
    if (this is android.view.ViewGroup) for (i in 0 until childCount) getChildAt(i).findWebView()?.let { return it }
    return null
}

private fun android.view.View.findTerminal(): tech.xdge.muxa.app.pane.TerminalPaneView? {
    if (this is tech.xdge.muxa.app.pane.TerminalPaneView) return this
    if (this is android.view.ViewGroup) for (i in 0 until childCount) getChildAt(i).findTerminal()?.let { return it }
    return null
}
