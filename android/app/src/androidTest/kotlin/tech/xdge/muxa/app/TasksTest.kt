package tech.xdge.muxa.app

import android.content.Intent
import android.view.KeyEvent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.session.InboxGroup

/** The Tasks screen against the demo bridge's records: one Task in each Inbox group. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class TasksTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null

    private fun launch(leaveInbox: Boolean = true) {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        val intent = Intent(context, MainActivity::class.java).putExtra(MainActivity.EXTRA_DEMO, true)
        scenario = ActivityScenario.launch(intent)
        if (leaveInbox) compose.leaveInitialInbox()
    }

    @After
    fun close() {
        scenario?.close()
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun paneShows(text: String) = SemanticsMatcher("pane shows \"$text\"") {
        it.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    /** From the pane, through More: the same way on every layout. */
    private fun openTasks() {
        waitForTag("composer")
        compose.onNodeWithTag("more").performClick()
        waitForTag("menu-tasks")
        compose.onNodeWithTag("menu-tasks").performClick()
        waitForTag("inbox-mode-tasks")
        compose.onNodeWithTag("inbox-mode-tasks").performClick()
        waitForTag("tasks-done")
    }

    /** The Inbox list replaced Triage (ADR 0025): a phone opens on it at connect. */
    @Test
    fun phoneStartsInTheInboxWhenAttentionIsWaiting() {
        launch(leaveInbox = false)
        val model = ApplicationProvider.getApplicationContext<MuxaApplication>().model
        compose.waitUntil(TIMEOUT) { model.isConnected && !model.connecting }
        if (model.phoneInbox) {
            waitForTag("inbox-list")
            waitForTag("inbox-row-t-auth01")
            compose.onNodeWithTag("tasks-done").performClick()
            waitForTag("composer")
        } else {
            waitForTag("composer")
        }
    }

    /**
     * A host without `task-inbox` keeps the device-local Snooze: the Task leaves this device's
     * Inbox, and Show snoozed brings it back. It replaces the Triage test, because Triage with
     * Next and Previous is gone, as on iOS.
     */
    @Test
    fun anOlderHostKeepsTheLocalSnooze() {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(MainActivity.EXTRA_DEMO, true)
            .putExtra(MainActivity.EXTRA_NO_TASK_INBOX, true)
        scenario = ActivityScenario.launch(intent)
        compose.leaveInitialInbox()
        waitForTag("composer")
        compose.onNodeWithTag("more").performClick()
        waitForTag("menu-tasks")
        compose.onNodeWithTag("menu-tasks").performClick()
        waitForTag("inbox-row-t-auth01")
        compose.onNodeWithTag("inbox-row-e-000000a1").assertDoesNotExist()
        compose.onNodeWithTag("inbox-actions-t-auth01").performClick()
        waitForTag("inbox-snooze")
        compose.onNodeWithTag("inbox-move-down").assertDoesNotExist()
        compose.onNodeWithTag("inbox-snooze").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("inbox-row-t-auth01"), TIMEOUT)
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag("inbox-show-snoozed"))
        compose.onNodeWithTag("inbox-show-snoozed").performClick()
        waitForTag("inbox-row-t-auth01")
        compose.onNodeWithTag("inbox-mode-tasks").performClick()
        waitForTag("task-list")
        for (group in InboxGroup.entries) {
            compose.onNodeWithTag("task-list").performScrollToNode(hasTestTag("inbox-${group.name}"))
            waitForTag("inbox-${group.name}")
        }
    }

    /** Scrolls the list to the Task first: on a phone the lower groups are off screen. */
    private fun openTask(id: String) {
        waitForTag("task-list")
        compose.onNodeWithTag("task-list").performScrollToNode(hasTestTag("task-$id"))
        compose.onNodeWithTag("task-$id").performClick()
    }

    /** Clicks a control of the Task detail, which scrolls. */
    private fun tap(tag: String) {
        waitForTag(tag)
        compose.onNodeWithTag(tag).performScrollTo().performClick()
    }

    private fun waitForText(tag: String, text: String) =
        compose.waitUntilAtLeastOneExists(hasTestTag(tag) and hasText(text, substring = true), TIMEOUT)

    @Test
    fun everyGroupShowsAndATaskOpensItsPane() {
        launch()
        openTasks()
        // The Routines sit above the groups, so the last group can start below the fold.
        for (group in InboxGroup.entries) {
            waitForTag("task-list")
            compose.onNodeWithTag("task-list").performScrollToNode(hasTestTag("inbox-${group.name}"))
            waitForTag("inbox-${group.name}")
        }
        openTask("t-login1")
        waitForTag("task-open-pane")
        compose.onNodeWithTag("task-open-pane").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("tasks-done"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows("pane %5 100x30"), TIMEOUT)
    }

    @Test
    fun aTaskWithoutASessionShowsWhyItsLaunchFailed() {
        launch()
        openTasks()
        openTask("t-price1")
        waitForTag("task-note")
        compose.onNodeWithTag("task-open-pane").assertDoesNotExist()
    }

    @Test
    fun theBriefOpensInTheReader() {
        launch()
        openTasks()
        openTask("t-auth01")
        waitForTag("task-open-brief")
        compose.onNodeWithTag("task-open-brief").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("tasks-done"), TIMEOUT)
        // The Reader opens on the brief itself, on every layout.
        waitForTag("reader")
        compose.waitUntilAtLeastOneExists(hasText("TASK.md"), TIMEOUT)
    }

    /** An idle agent restarts without force: a new run, and the agent runs again. */
    @Test
    fun restartingAnIdleAgentAddsARun() {
        launch()
        openTasks()
        openTask("t-login1")
        waitForTag("run-1")
        compose.onNodeWithTag("run-2").assertDoesNotExist()
        compose.onNodeWithTag("task-open-handoff").assertDoesNotExist()
        compose.onNodeWithTag("task-restart-agent").performClick()
        waitForTag("restart-confirm")
        compose.onNodeWithTag("restart-force").assertDoesNotExist()
        compose.onNodeWithTag("restart-confirm").performClick()
        waitForTag("run-2")
        compose.waitUntilAtLeastOneExists(hasTestTag("run-2") and hasAnyDescendant(hasText("still running", substring = true)), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasTestTag("task-detail") and hasAnyDescendant(hasText(InboxGroup.RUNNING.title)), TIMEOUT)
        waitForTag("task-open-handoff")
    }

    /** A running agent only offers to stop and restart it. */
    @Test
    fun aRunningAgentAsksToStopIt() {
        launch()
        openTasks()
        openTask("t-docs01")
        waitForTag("task-restart-agent")
        compose.onNodeWithTag("task-restart-agent").performClick()
        waitForTag("restart-force")
        compose.onNodeWithTag("restart-confirm").assertDoesNotExist()
        compose.onNodeWithText("Stop and restart").assertExists()
        compose.onNodeWithTag("restart-force").performClick()
        waitForTag("run-2")
    }

    @Test
    fun theHandoffOpensInTheReader() {
        launch()
        openTasks()
        openTask("t-auth01")
        waitForTag("task-open-handoff")
        compose.onNodeWithTag("task-open-handoff").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("tasks-done"), TIMEOUT)
        waitForTag("reader")
        compose.waitUntilAtLeastOneExists(hasText("HANDOFF.md"), TIMEOUT)
    }

    /** Ctrl+Shift+N, as a keyboard case sends it. */
    private fun newTaskShortcut() {
        val now = android.os.SystemClock.uptimeMillis()
        val meta = KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_N, 0, meta))
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_UP, KeyEvent.KEYCODE_N, 0, meta))
    }

    /**
     * Ctrl+Shift+N opens the record-based sheet in the shown session's repo and the workspace
     * that lists it. The new Task shows in Tasks, and its session opens.
     */
    @Test
    fun ctrlShiftNCreatesATaskInTheShownRepo() {
        launch()
        waitForTag("composer")
        compose.waitUntilAtLeastOneExists(hasText("feature-auth"), TIMEOUT)
        newTaskShortcut()
        waitForTag("task-title")
        waitForText("task-workspace", "work")
        // A listed repo shows by its name under the workspace root (ADR 0031).
        waitForText("record-repo-choice", "app")
        compose.onNodeWithTag("task-title").performTextInput("Add search")
        compose.onNodeWithTag("task-goal").performTextInput("Search the docs.")
        compose.onNodeWithTag("task-branch").performScrollTo().performTextInput("feat/search")
        compose.onNodeWithTag("task-create").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("task-create"), TIMEOUT)
        // Its session, not the Tasks screen.
        compose.waitUntilAtLeastOneExists(hasText("add-search"), TIMEOUT)
        compose.onNodeWithTag("tasks-done").assertDoesNotExist()
        // The new agent's pane may show its terminal, so no composer to wait for.
        waitForTag("more")
        compose.onNodeWithTag("more").performClick()
        waitForTag("menu-tasks")
        compose.onNodeWithTag("menu-tasks").performClick()
        waitForTag("inbox-mode-tasks")
        compose.onNodeWithTag("inbox-mode-tasks").performClick()
        compose.onNodeWithTag("task-list").performScrollToNode(hasTestTag("task-t-new001"))
        waitForTag("task-t-new001")
    }

    /** The list offers Tasks and New Task above the hosts, so a phone's first screen has them. */
    @Test
    fun theListOffersTasksAndNewTask() {
        launch()
        waitForTag("composer")
        for (tag in listOf("back", "show-list")) {
            if (compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()) {
                compose.onNodeWithTag(tag).performClick()
                break
            }
        }
        waitForTag("open-tasks")
        compose.onNodeWithTag("sidebar-new-task").performClick()
        waitForTag("task-title")
        waitForText("task-workspace", "work")
    }

    private fun newTask(title: String) {
        compose.onNodeWithTag("new-task-record").performClick()
        waitForTag("task-title")
        compose.onNodeWithTag("task-title").performTextInput(title)
        compose.onNodeWithTag("task-goal").performTextInput("Book the flights and the hotel.")
    }

    /** Create a Task without a repo, see it run, mark it done, and find it under Closed. */
    @Test
    fun aNewTaskRunsAndIsMarkedDone() {
        launch()
        openTasks()
        newTask("Plan the trip")
        compose.onNodeWithTag("task-create").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("task-create"), TIMEOUT)
        waitForTag("task-detail")
        compose.waitUntilAtLeastOneExists(hasTestTag("task-detail") and hasAnyDescendant(hasText(InboxGroup.RUNNING.title)), TIMEOUT)
        compose.onNodeWithTag("task-status").performClick()
        waitForTag("status-done")
        compose.onNodeWithTag("status-done").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("task-detail") and hasAnyDescendant(hasText(InboxGroup.CLOSED.title)), TIMEOUT)
        // It opens its new session's pane.
        compose.onNodeWithTag("task-open-pane").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("tasks-done"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasText("plan-the-trip"), TIMEOUT)
    }

    @Test
    fun aFailedLaunchKeepsTheTaskAndShowsWhy() {
        launch()
        openTasks()
        newTask("Taken branch")
        compose.onNodeWithTag("task-with-repo").performClick()
        // A typed path, as for a repo the workspace does not list.
        compose.onNodeWithTag("record-repo-choice").performClick()
        waitForTag("repo-other")
        compose.onNodeWithTag("repo-other").performClick()
        waitForTag("task-repo")
        compose.onNodeWithTag("task-repo").performTextClearance()
        compose.onNodeWithTag("task-repo").performTextInput("/src/app")
        compose.onNodeWithTag("task-branch").performTextInput("taken")
        compose.onNodeWithTag("task-create").performClick()
        waitForTag("task-note")
        compose.onNodeWithText("already exists", substring = true).assertExists()
    }

    /** Start a stopped loop with the workspace's second listed check, then stop it (ADR 0019). */
    @Test
    fun aLoopStartsWithAListedCheckAndStops() {
        launch()
        openTasks()
        openTask("t-deps01")
        waitForText("loop-state", "Stopped")
        waitForText("loop-reason", "Stopped by the user.")
        tap("loop-start")
        // The sheet does not scroll: its fields are tapped where they are.
        waitForTag("loop-check")
        compose.onNodeWithTag("loop-check").performClick()
        waitForTag("check-option-1")
        compose.onNodeWithTag("check-option-1").performClick()
        waitForText("loop-check", "cargo test --workspace")
        compose.onNodeWithTag("loop-start-confirm").performClick()
        waitForText("loop-state", "Armed")
        waitForText("loop-check-command", "cargo test --workspace")
        tap("loop-stop")
        waitForText("loop-state", "Stopped")
        waitForTag("loop-start")
    }

    /** A loop paused for the user, or stopped at its limit, puts the Task in Needs attention. */
    @Test
    fun aPausedLoopNeedsAttention() {
        launch()
        openTasks()
        // The row merges its texts: its badge is part of it.
        waitForText("task-t-auth01", "Loop paused")
        openTask("t-auth01")
        compose.waitUntilAtLeastOneExists(hasTestTag("task-detail") and hasAnyDescendant(hasText(InboxGroup.NEEDS_ATTENTION.title)), TIMEOUT)
        waitForText("loop-reason", "The loop resumes after the agent finishes its turn.")
        compose.onNodeWithTag("loop-stop").assertExists()
        if (compose.onAllNodesWithTag("task-back").fetchSemanticsNodes().isNotEmpty()) compose.onNodeWithTag("task-back").performClick()
        // No agent runs this one: only its loop puts it there.
        openTask("t-lint01")
        waitForText("loop-reason", "Stopped: the check failed 3 times.")
        compose.waitUntilAtLeastOneExists(hasTestTag("task-detail") and hasAnyDescendant(hasText(InboxGroup.NEEDS_ATTENTION.title)), TIMEOUT)
    }

    @Test
    fun theCheckLogOpensInTheReader() {
        launch()
        openTasks()
        openTask("t-auth01")
        tap("loop-open-log")
        compose.waitUntilDoesNotExist(hasTestTag("tasks-done"), TIMEOUT)
        waitForTag("reader")
        compose.waitUntilAtLeastOneExists(hasText("CHECK.md"), TIMEOUT)
    }

    /** A workspace with listed checks offers its first one; the new Task's loop is armed. */
    @Test
    fun aNewTaskRunsACheckAfterEachTurn() {
        launch()
        openTasks()
        newTask("Lint the site")
        compose.onNodeWithTag("task-with-loop").performScrollTo().performClick()
        waitForText("loop-check", "make check")
        compose.onNodeWithTag("task-create").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("task-create"), TIMEOUT)
        waitForText("loop-state", "Armed")
        waitForText("loop-check-command", "make check")
    }

    private companion object {
        const val TIMEOUT = 10_000L
    }
}
