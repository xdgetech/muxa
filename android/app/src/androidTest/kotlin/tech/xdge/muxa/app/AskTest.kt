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
import androidx.compose.ui.test.onNodeWithText
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

    /** [work1] plays a host from before ask sessions, [work3] one from before closing them. */
    private fun launch(work1: Boolean = false, work3: Boolean = false) {
        scenario = ActivityScenario.launch(
            demoIntent().putExtra(MainActivity.EXTRA_WORK1, work1).putExtra(MainActivity.EXTRA_WORK3, work3),
        )
        compose.waitForInitialPane()
    }

    @After
    fun close() {
        scenario?.close()
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun waitForText(tag: String, text: String) =
        compose.waitUntilAtLeastOneExists(hasTestTag(tag) and hasText(text, substring = true), TIMEOUT)

    /** The New Task sheet from Tasks mode's `+` (ADR 0035), with a title, a goal and the repo fields shown. */
    private fun newRepoTask(title: String) {
        waitForTag("composer")
        compose.pickMode(SidebarMode.TASKS)
        waitForTag("task-list")
        compose.click("sidebar-new-task")
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
        // An ask session is a manual session: Sessions mode lists it (ADR 0031, 0035).
        compose.pickMode(SidebarMode.SESSIONS)
        // Its repo now has a manual session, so its header offers Ask too.
        waitForTag("ask-app")
        compose.onNodeWithTag("ask-app").performClick()
        compose.waitUntilAtLeastOneExists(hasText("ask-app-2"), TIMEOUT)
    }

    /** Asks about `app` from the New Task sheet; the ask session's pane shows. */
    private fun askAboutApp() {
        newRepoTask("Unused")
        compose.onNodeWithTag("record-repo-choice").performClick()
        waitForTag("repo-app")
        compose.onNodeWithTag("repo-app").performClick()
        compose.onNodeWithTag("record-ask").performScrollTo().performClick()
        compose.waitUntilDoesNotExist(hasTestTag("record-ask"), TIMEOUT)
        compose.waitUntilAtLeastOneExists(hasText("ask-app-1"), TIMEOUT)
        waitForTag("pane")
    }

    /** Close Session closes the shown ask session after a confirmation (ADR 0039). It is no task: no Close Task. */
    @Test
    fun anAskSessionClosesFromTheMoreMenu() {
        launch()
        askAboutApp()
        compose.onNodeWithTag("more").performClick()
        compose.waitUntilAtLeastOneExists(hasText("Close Session…"), TIMEOUT)
        compose.onNodeWithText("Close Task…").assertDoesNotExist()
        compose.onNodeWithText("Close Session…").performClick()
        compose.waitUntilAtLeastOneExists(hasText("The agent stops", substring = true), TIMEOUT)
        compose.onNodeWithText("Close Session").performClick()
        compose.waitUntilDoesNotExist(hasText("ask-app-1"), TIMEOUT)
        compose.pickMode(SidebarMode.SESSIONS)
        compose.waitUntilAtLeastOneExists(hasTestTag("session-scratch"), TIMEOUT)
        compose.onNodeWithTag("session-ask-app-1").assertDoesNotExist()
    }

    /** A host from before closing ask sessions offers no Close Session. */
    @Test
    fun anOlderHostHasNoCloseSession() {
        launch(work3 = true)
        askAboutApp()
        compose.onNodeWithTag("more").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("menu-sessions"), TIMEOUT)
        compose.onNodeWithText("Close Session…").assertDoesNotExist()
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

    private companion object {
        const val TIMEOUT = 10_000L
    }
}
