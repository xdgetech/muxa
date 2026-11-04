package tech.xdge.muxa.app

import android.content.Intent
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** The New Task sheet's workspace repos, a branch from the title, and Ask (ADR 0031). */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class AskTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null

    /** [work1] plays a host from before ask sessions. */
    private fun launch(work1: Boolean = false) {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(MainActivity.EXTRA_DEMO, true)
            .putExtra(MainActivity.EXTRA_WORK1, work1)
        scenario = ActivityScenario.launch(intent)
        compose.leaveInitialInbox()
    }

    @After
    fun close() {
        scenario?.close()
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun waitForText(tag: String, text: String) =
        compose.waitUntilAtLeastOneExists(hasTestTag(tag) and hasText(text, substring = true), TIMEOUT)

    /** The New Task sheet from the Tasks screen, with a title, a goal and the repo fields shown. */
    private fun newRepoTask(title: String) {
        waitForTag("composer")
        compose.onNodeWithTag("more").performClick()
        waitForTag("menu-tasks")
        compose.onNodeWithTag("menu-tasks").performClick()
        // The Triage/List control is gone; All Tasks is the list page, as TasksTest opens it.
        waitForTag("inbox-mode-tasks")
        compose.onNodeWithTag("inbox-mode-tasks").performClick()
        waitForTag("new-task-record")
        compose.onNodeWithTag("new-task-record").performClick()
        waitForTag("task-title")
        compose.onNodeWithTag("task-title").performTextInput(title)
        compose.onNodeWithTag("task-goal").performTextInput("Index the docs for search.")
        compose.onNodeWithTag("task-with-repo").performScrollTo().performClick()
        waitForTag("record-repo-choice")
    }

    /** A repo picked by its name under the root; with no branch, the host names it after the title. */
    @Test
    fun aRepoPickedByNameGetsABranchNamedAfterTheTitle() {
        launch()
        newRepoTask("Index the docs")
        compose.onNodeWithTag("record-repo-choice").performClick()
        waitForTag("repo-search")
        waitForTag("repo-~/forks/tmux")
        compose.onNodeWithTag("repo-search").performTextInput("core")
        compose.waitUntilDoesNotExist(hasTestTag("repo-app"), TIMEOUT)
        compose.onNodeWithTag("repo-libs/core").performClick()
        waitForText("record-repo-choice", "libs/core")
        compose.onNodeWithTag("task-create").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("task-create"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasTestTag("task-detail") and hasAnyDescendant(hasText("index-the-docs")), TIMEOUT)
    }

    /** Ask opens a manual session in the repo's main checkout: no Task, listed under Sessions. */
    @Test
    fun askOpensAManualSessionInTheRepo() {
        launch()
        newRepoTask("Unused")
        compose.onNodeWithTag("record-repo-choice").performClick()
        waitForTag("repo-app")
        compose.onNodeWithTag("repo-app").performClick()
        compose.onNodeWithTag("record-ask").performScrollTo().performClick()
        compose.waitUntilDoesNotExist(hasTestTag("record-ask"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasText("ask-app-1"), TIMEOUT)
        waitForTag("pane")
        compose.onNodeWithTag("tasks-done").assertDoesNotExist()
        showList()
        // Its repo now has a manual session, so its header offers Ask too.
        waitForTag("ask-app")
        compose.onNodeWithTag("ask-app").performClick()
        compose.waitUntilAtLeastOneExists(hasText("ask-app-2"), TIMEOUT)
    }

    /** A host from before ask sessions needs the branch and offers no Ask. */
    @Test
    fun anOlderHostNeedsTheBranchAndHasNoAsk() {
        launch(work1 = true)
        newRepoTask("Index the docs")
        compose.onNodeWithTag("record-ask").assertDoesNotExist()
        compose.onNodeWithTag("task-create").assertIsNotEnabled()
        waitForText("task-record-error", "Name the branch.")
    }

    /** The list of hosts and sessions: shown beside the pane, or behind Back on a phone. */
    private fun showList() {
        for (tag in listOf("back", "show-list")) {
            if (compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()) {
                compose.onNodeWithTag(tag).performClick()
                break
            }
        }
    }

    private companion object {
        const val TIMEOUT = 10_000L
    }
}
