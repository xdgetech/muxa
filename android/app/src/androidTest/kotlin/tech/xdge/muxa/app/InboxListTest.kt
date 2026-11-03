package tech.xdge.muxa.app

import android.content.Intent
import android.view.KeyEvent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.protocol.TaskStatus

/**
 * The Inbox list against the demo's Task places, stored entries and Routines (ADR 0025
 * "List first", ADR 0026, ADR 0027 §4). Ported from the iOS `InboxListTests`.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class InboxListTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    /** Opens the demo on the Inbox list: a phone opens there by itself, a tablet through More. */
    private fun launchOnInbox() {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        scenario = ActivityScenario.launch(Intent(context, MainActivity::class.java).putExtra(MainActivity.EXTRA_DEMO, true))
        compose.waitUntil(TIMEOUT) { exists("inbox-list") || exists("composer") }
        if (!exists("inbox-list")) {
            click("more")
            click("menu-tasks")
        }
        waitForTag("inbox-list")
        waitForTag("inbox-row-t-auth01")
    }

    private fun exists(tag: String) = compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun click(tag: String) {
        waitForTag(tag)
        compose.onNodeWithTag(tag).performClick()
    }

    private fun row(id: String) = "inbox-row-$id"

    /** Scrolls the list to [tag]: the entries sit below the Tasks. */
    private fun reveal(tag: String) {
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag(tag))
        waitForTag(tag)
    }

    private fun act(id: String, action: String) {
        reveal("inbox-actions-$id")
        click("inbox-actions-$id")
        click(action)
    }

    private val rowTag = SemanticsMatcher("an Inbox row") {
        it.config.getOrNull(SemanticsProperties.TestTag)?.startsWith("inbox-row-") == true
    }

    /** The rows' ids, top to bottom, as far as the list has them on screen. */
    private fun rowIds(): List<String> = compose.onAllNodes(rowTag).fetchSemanticsNodes()
        .sortedBy { it.boundsInRoot.top }
        .map { it.config[SemanticsProperties.TestTag].removePrefix("inbox-row-") }

    private fun paneShows(text: String) = SemanticsMatcher("pane shows \"$text\"") {
        it.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    private fun waitForText(tag: String, text: String) =
        compose.waitUntilAtLeastOneExists(hasTestTag(tag) and hasText(text, substring = true), TIMEOUT)

    /** Task needs come first, then the waiting Routine run, then briefings and proposals. */
    @Test
    fun theListHoldsTasksRoutineRunsAndEntries() {
        launchOnInbox()
        assertEquals("t-auth01", rowIds().first())
        reveal(row("t-r00002"))
        waitForText(row("t-r00002"), "Routine run waits for your answer")
        reveal(row("e-000000a1"))
        reveal(row("t-r00002"))
        click(row("t-r00002"))
        // Act opens the run's terminal.
        compose.waitUntilDoesNotExist(hasTestTag("tasks-done"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows("pane %90 100x30"), TIMEOUT)
    }

    /** Move down puts a Task at the end of the list, below every entry; its status stays. */
    @Test
    fun moveDownPutsATaskLast() {
        launchOnInbox()
        act("t-auth01", "inbox-move-down")
        compose.waitUntil(TIMEOUT) { rowIds().firstOrNull() != "t-auth01" }
        reveal(row("t-auth01"))
        assertEquals("t-auth01", rowIds().last())
        assertEquals("t-auth01", model.inboxRows.last().id)
        assertEquals(TaskStatus.OPEN, model.state.tasks?.record("t-auth01")?.status)
        click("inbox-mode-tasks")
        // Move down does not change the group.
        compose.onNodeWithTag("task-list").performScrollToNode(hasTestTag("task-t-auth01"))
        waitForTag("inbox-NEEDS_ATTENTION")
    }

    /** Later hides a Task until the chosen time. */
    @Test
    fun laterHidesATask() {
        launchOnInbox()
        act("t-auth01", "inbox-later")
        click("later-in_an_hour")
        compose.waitUntilDoesNotExist(hasTestTag(row("t-auth01")), TIMEOUT)
    }

    /** Approve shows the proposal as the Task gets it, creates exactly one Task and opens it. */
    @Test
    fun approvingAProposalCreatesOneTaskAndOpensIt() {
        launchOnInbox()
        val title = "Close the stale issue ENG-12"
        act("e-000000a2", "inbox-approve")
        waitForText("approve-action", "Close the issue")
        waitForText("approve-target", "linear:ENG-12")
        waitForText("approve-content", "no activity for 30 days")
        click("approve-confirm")
        // The new Task's detail opens; its goal carries the proposal unchanged.
        compose.waitUntilAtLeastOneExists(
            hasText("muxa-proposal: e-000000a2", substring = true) and hasAnyAncestor(hasTestTag("task-detail")), TIMEOUT,
        )
        waitForTag("task-open-pane")
        if (exists("task-back")) click("task-back")
        compose.waitUntilDoesNotExist(hasTestTag(row("e-000000a2")), TIMEOUT)
        click("inbox-mode-tasks")
        waitForTag("task-list")
        val task = SemanticsMatcher("a Task row") { it.config.getOrNull(SemanticsProperties.TestTag)?.startsWith("task-t-") == true }
        compose.onNodeWithTag("task-list").performScrollToNode(task and hasText(title))
        assertEquals(1, compose.onAllNodes(task and hasText(title)).fetchSemanticsNodes().size)
        assertEquals(1, model.state.tasks?.records?.count { it.title == title })
    }

    /** Dismiss removes a proposal and creates nothing. */
    @Test
    fun dismissRemovesAProposal() {
        launchOnInbox()
        val tasks = model.state.tasks?.records?.size
        act("e-000000a3", "inbox-dismiss")
        compose.waitUntilDoesNotExist(hasTestTag(row("e-000000a3")), TIMEOUT)
        assertEquals(tasks, model.state.tasks?.records?.size)
    }

    /** A briefing opens with its text and source; Done acknowledges it. */
    @Test
    fun aBriefingIsReadThenDone() {
        launchOnInbox()
        reveal(row("e-000000a1"))
        click(row("e-000000a1"))
        waitForText("entry-body", "ship on Thursday")
        waitForText("entry-link", "Release thread")
        waitForTag("entry-done")
        compose.onNodeWithTag("entry-done").performScrollTo().performClick()
        compose.waitUntilDoesNotExist(hasTestTag(row("e-000000a1")), TIMEOUT)
        compose.waitUntilDoesNotExist(hasTestTag("entry-body"), TIMEOUT)
    }

    /** A Routine's detail shows its fields; Run now starts a run, and a disabled one cannot run. */
    @Test
    fun aRoutineShowsItsFieldsAndRunsNow() {
        launchOnInbox()
        click("inbox-mode-tasks")
        click("open-routines")
        click("routine-work/slack-digest")
        waitForText("routine-workspace", "work")
        waitForText("routine-trigger", "08:00 Europe/Berlin")
        waitForText("routine-enabled", "Yes")
        waitForText("routine-last-success", "3 new")
        waitForTag("routine-next-due")
        waitForText("routine-failure-reason", "output.json")
        compose.onNodeWithTag("routine-running").assertDoesNotExist()
        compose.onNodeWithTag("routine-run-now").performScrollTo().performClick()
        // One run at a time.
        compose.waitUntil(TIMEOUT) { compose.onAllNodes(hasTestTag("routine-run-now") and isEnabled()).fetchSemanticsNodes().isEmpty() }
        waitForTag("routine-running")
        waitForTag("routine-open-terminal")

        if (exists("routine-back")) click("routine-back")
        click("routine-personal/weekly-memory")
        waitForText("routine-trigger", "Run now only")
        compose.onNodeWithTag("routine-run-now").performScrollTo().assertIsNotEnabled()
    }

    /** The running run's Open Terminal shows its session. */
    @Test
    fun aRunningRoutineOpensItsTerminal() {
        launchOnInbox()
        click("inbox-mode-tasks")
        click("open-routines")
        click("routine-work/linear-triage")
        waitForTag("routine-open-terminal")
        compose.onNodeWithTag("routine-open-terminal").performScrollTo().performClick()
        compose.waitUntilDoesNotExist(hasTestTag("tasks-done"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows("pane %90 100x30"), TIMEOUT)
    }

    /** Swiping a Task's row left moves it down, as its menu does. */
    @Test
    fun swipingLeftMovesATaskDown() {
        launchOnInbox()
        compose.onNodeWithTag(row("t-auth01")).performTouchInput { swipeLeft() }
        compose.waitUntil(TIMEOUT) { model.inboxRows.lastOrNull()?.id == "t-auth01" }
    }

    /** Ctrl+Shift+Down focuses the next row, Ctrl+Shift+Alt+Down moves it down, Ctrl+Shift+L shows All Tasks. */
    @Test
    fun keysFocusMoveDownAndSwitchToAllTasks() {
        launchOnInbox()
        compose.waitUntil(TIMEOUT) { model.inboxFocus == "t-auth01" }
        val second = model.inboxRows[1].id
        key(KeyEvent.KEYCODE_DPAD_DOWN)
        compose.waitUntil(TIMEOUT) { model.inboxFocus == second }
        key(KeyEvent.KEYCODE_DPAD_DOWN, alt = true)
        compose.waitUntil(TIMEOUT) { model.inboxRows.lastOrNull()?.id == second }
        key(KeyEvent.KEYCODE_L)
        waitForTag("task-list")
    }

    private fun key(code: Int, alt: Boolean = false) {
        val now = android.os.SystemClock.uptimeMillis()
        val meta = KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON or (if (alt) KeyEvent.META_ALT_ON else 0)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0, meta))
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0, meta))
    }

    private companion object {
        const val TIMEOUT = 10_000L
    }
}
