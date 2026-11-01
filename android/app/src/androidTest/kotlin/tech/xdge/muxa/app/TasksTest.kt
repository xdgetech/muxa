package tech.xdge.muxa.app

import android.content.Intent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
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

    private companion object {
        const val TIMEOUT = 10_000L
    }
}
