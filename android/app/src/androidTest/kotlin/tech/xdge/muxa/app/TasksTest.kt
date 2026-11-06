package tech.xdge.muxa.app

import android.content.Intent
import android.view.KeyEvent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.session.InboxGroup

/** Tasks mode (ADR 0035) against the demo bridge's records: one Task in each group. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class TasksTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null

    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    /** Sessions mode, which shows the default pane at connect. */
    private fun launch() {
        scenario = ActivityScenario.launch(demoIntent())
        compose.waitForInitialPane()
    }

    @After
    fun close() {
        scenario?.close()
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun paneShows(text: String) = SemanticsMatcher("pane shows \"$text\"") {
        it.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    /** The demo in Tasks mode: the sidebar is the Task list, on every layout (ADR 0035). */
    private fun openTasks() {
        scenario = ActivityScenario.launch(demoIntent(SidebarMode.TASKS))
        waitForTag("task-list")
    }

    /**
     * Sessions mode shows the default pane at connect, as before ADR 0035. It replaces the
     * test that a phone opened on the Inbox when an agent waited: the mode decides now, and
     * ItemListTest checks that Inbox mode stays on its list.
     */
    @Test
    fun sessionsModeShowsThePaneAtConnect() {
        launch()
        waitForTag("composer")
        compose.waitUntil(TIMEOUT) { model.paneShown }
        compose.waitUntilAtLeastOneExists(hasText("feature-auth"), TIMEOUT)
    }

    /** Tasks mode stays on its list at connect, also on a phone, and watches no pane. */
    @Test
    fun tasksModeStaysOnItsListAtConnect() {
        openTasks()
        compose.waitUntil(TIMEOUT) { model.isConnected && !model.connecting }
        compose.onNodeWithTag("composer").assertDoesNotExist()
        assertFalse(model.paneShown)
    }

    /** On a phone the pane returns to the Task it was opened from, and the Task to the list. */
    @Test
    fun backReturnsFromThePaneToTheTaskThenTheList() {
        openTasks()
        openTask("t-login1")
        tap("task-open-pane")
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows("pane %5 100x30"), TIMEOUT)
        assertEquals("t-login1", model.paneTask)
        if (!model.phoneLayout) return
        compose.onNodeWithTag("back").performClick()
        waitForTag("task-detail")
        InstrumentationRegistry.getInstrumentation().runOnMainSync { model.back() }
        waitForTag("task-list")
    }

    /**
     * A host below `work` level 3 has no Inbox items (ADR 0033): the Inbox says the host needs a
     * newer muxa-bridge, and All Tasks still lists every group. It replaces the local Snooze's
     * test, because I4 removed the older Inbox list.
     */
    @Test
    fun anOlderHostAsksForANewerBridge() {
        scenario = ActivityScenario.launch(demoIntent(SidebarMode.INBOX).putExtra(MainActivity.EXTRA_WORK2, true))
        waitForTag("inbox-empty")
        compose.waitUntilAtLeastOneExists(hasText("needs a newer muxa-bridge", substring = true), TIMEOUT)
        compose.onNodeWithTag("inbox-empty-tasks").performClick()
        waitForTag("task-list")
        for (group in InboxGroup.entries) {
            compose.onNodeWithTag("task-list").performScrollToNode(hasTestTag("inbox-${group.name}"))
            waitForTag("inbox-${group.name}")
        }
    }

    /** A Routine's detail shows its fields; Run now starts a run, and a disabled one cannot run. */
    @Test
    fun aRoutineShowsItsFieldsAndRunsNow() {
        openTasks()
        waitForTag("open-routines")
        compose.onNodeWithTag("open-routines").performClick()
        waitForTag("routine-work/slack-digest")
        compose.onNodeWithTag("routine-work/slack-digest").performClick()
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

        compose.onNodeWithTag("routine-back").performClick()
        waitForTag("routine-personal/weekly-memory")
        compose.onNodeWithTag("routine-personal/weekly-memory").performClick()
        waitForText("routine-trigger", "Run now only")
        compose.onNodeWithTag("routine-run-now").performScrollTo().assertIsNotEnabled()
    }

    /** The running run's Open Terminal shows its session. */
    @Test
    fun aRunningRoutineOpensItsTerminal() {
        openTasks()
        waitForTag("open-routines")
        compose.onNodeWithTag("open-routines").performClick()
        waitForTag("routine-work/linear-triage")
        compose.onNodeWithTag("routine-work/linear-triage").performClick()
        waitForTag("routine-open-terminal")
        compose.onNodeWithTag("routine-open-terminal").performScrollTo().performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows("pane %90 100x30"), TIMEOUT)
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
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows("pane %5 100x30"), TIMEOUT)
    }

    @Test
    fun aTaskWithoutASessionShowsWhyItsLaunchFailed() {
        openTasks()
        openTask("t-price1")
        waitForTag("task-note")
        compose.onNodeWithTag("task-open-pane").assertDoesNotExist()
    }

    @Test
    fun theBriefOpensInTheReader() {
        openTasks()
        openTask("t-auth01")
        waitForTag("task-open-brief")
        compose.onNodeWithTag("task-open-brief").performClick()
        // The Reader opens on the brief itself, on every layout.
        waitForTag("reader")
        compose.waitUntilAtLeastOneExists(hasText("TASK.md"), TIMEOUT)
    }

    /** An idle agent restarts without force: a new run, and the agent runs again. */
    @Test
    fun restartingAnIdleAgentAddsARun() {
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
        openTasks()
        openTask("t-auth01")
        waitForTag("task-open-handoff")
        compose.onNodeWithTag("task-open-handoff").performClick()
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
        // Its session, not the Task's detail: the sheet opened over a pane.
        compose.waitUntilAtLeastOneExists(hasText("add-search"), TIMEOUT)
        compose.waitUntil(TIMEOUT) { model.paneShown }
        compose.onNodeWithTag("task-detail").assertDoesNotExist()
        compose.pickMode(SidebarMode.TASKS)
        waitForTag("task-list")
        compose.onNodeWithTag("task-list").performScrollToNode(hasTestTag("task-t-new001"))
        waitForTag("task-t-new001")
    }

    /**
     * The sidebar offers the three modes, the status row and `+` for New Task, so a phone's
     * first screen has them (ADR 0035). The hosts are in Settings.
     */
    @Test
    fun theSidebarOffersTheModesAndNewTask() {
        launch()
        waitForTag("composer")
        compose.showList()
        for (mode in SidebarMode.entries) waitForTag("sidebar-mode-${mode.tag}")
        waitForTag("status-row")
        compose.onNodeWithTag("host-Demo").assertDoesNotExist()
        compose.onNodeWithTag("sidebar-new-task").performClick()
        waitForTag("task-title")
        waitForText("task-workspace", "work")
    }

    private fun newTask(title: String) {
        compose.onNodeWithTag("sidebar-new-task").performClick()
        waitForTag("task-title")
        compose.onNodeWithTag("task-title").performTextInput(title)
        compose.onNodeWithTag("task-goal").performTextInput("Book the flights and the hotel.")
    }

    /** Create a Task without a repo, see it run, mark it done, and find it under Closed. */
    @Test
    fun aNewTaskRunsAndIsMarkedDone() {
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
        tap("task-open-pane")
        compose.waitUntilAtLeastOneExists(hasText("plan-the-trip"), TIMEOUT)
    }

    private fun pickDone() {
        tap("task-status")
        waitForTag("status-done")
        compose.onNodeWithTag("status-done").performClick()
    }

    /**
     * ADR 0043: the demo's t-perf01 has a branch that is not merged. Done asks first with the
     * host's reason; Cancel keeps the Task open, and Remove ends it.
     */
    @Test
    fun doneAsksBeforeItRemovesUnmergedWork() {
        openTasks()
        openTask("t-perf01")
        pickDone()
        waitForText("lose-work-message", "Its branch is not merged. Remove its worktree and branch anyway?")
        compose.onNodeWithTag("lose-work-cancel").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("lose-work-message"), TIMEOUT)
        assertEquals(TaskStatus.OPEN, model.state.tasks?.record("t-perf01")?.status)
        pickDone()
        waitForTag("lose-work-remove")
        compose.onNodeWithTag("lose-work-remove").performClick()
        compose.waitUntil(TIMEOUT) { model.state.tasks?.record("t-perf01")?.status == TaskStatus.DONE }
        compose.onNodeWithTag("lose-work-message").assertDoesNotExist()
    }

    @Test
    fun aFailedLaunchKeepsTheTaskAndShowsWhy() {
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
        openTasks()
        openTask("t-auth01")
        tap("loop-open-log")
        waitForTag("reader")
        compose.waitUntilAtLeastOneExists(hasText("CHECK.md"), TIMEOUT)
    }

    /** A workspace with listed checks offers its first one; the new Task's loop is armed. */
    @Test
    fun aNewTaskRunsACheckAfterEachTurn() {
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
