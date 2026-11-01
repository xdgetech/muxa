package tech.xdge.muxa.app

import android.content.Intent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
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

    private fun launch() {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        val intent = Intent(context, MainActivity::class.java).putExtra(MainActivity.EXTRA_DEMO, true)
        scenario = ActivityScenario.launch(intent)
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
        waitForTag("tasks-done")
    }

    @Test
    fun everyGroupShowsAndATaskOpensItsPane() {
        launch()
        openTasks()
        for (group in InboxGroup.entries) waitForTag("inbox-${group.name}")
        compose.onNodeWithTag("task-t-login1").performClick()
        waitForTag("task-open-pane")
        compose.onNodeWithTag("task-open-pane").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("tasks-done"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows("pane %5 100x30"), TIMEOUT)
    }

    @Test
    fun aTaskWithoutASessionShowsWhyItsLaunchFailed() {
        launch()
        openTasks()
        compose.onNodeWithTag("task-t-price1").performClick()
        waitForTag("task-note")
        compose.onNodeWithTag("task-open-pane").assertDoesNotExist()
    }

    @Test
    fun theBriefOpensInTheReader() {
        launch()
        openTasks()
        compose.onNodeWithTag("task-t-auth01").performClick()
        waitForTag("task-open-brief")
        compose.onNodeWithTag("task-open-brief").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("tasks-done"), TIMEOUT)
        // The Reader opens on the brief itself, on every layout.
        waitForTag("reader")
        compose.waitUntilAtLeastOneExists(hasText("TASK.md"), TIMEOUT)
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
        compose.onNodeWithTag("task-repo").performTextClearance()
        compose.onNodeWithTag("task-repo").performTextInput("/src/app")
        compose.onNodeWithTag("task-branch").performTextInput("taken")
        compose.onNodeWithTag("task-create").performClick()
        waitForTag("task-note")
        compose.onNodeWithText("already exists", substring = true).assertExists()
    }

    private companion object {
        const val TIMEOUT = 10_000L
    }
}
