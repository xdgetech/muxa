package tech.xdge.muxa.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToNode
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.app.ui.wire
import tech.xdge.muxa.protocol.InboxAction

/**
 * The assistant proposes Tasks (ADR 0060 §8) against the demo at `work` level 18: the Proposed Task card,
 * Start Task and Open its Task, Start Task beside a hand-off, and a plan item's answers.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class ProposalTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    private fun launchOnInbox() {
        scenario = ActivityScenario.launch(proposalsDemoIntent())
        compose.waitUntilAtLeastOneExists(hasTestTag("inbox-section-today"), TIMEOUT)
    }

    private fun waitForText(tag: String, text: String) = compose.waitForTextIn(tag, text, TIMEOUT)

    private fun openItem(id: String) {
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag("item-row-$id"))
        compose.click("item-row-$id")
        compose.waitUntilAtLeastOneExists(hasTestTag("item-detail"), TIMEOUT)
    }

    private fun item(id: String) = model.state.inboxItems.first { it.id == id }

    /** The secretary's proposal: its card says it starts in plan mode; Start Task stays on the item, which opens its Task. */
    @Test
    fun startTaskOpensItsTask() {
        launchOnInbox()
        openItem(NEW_TASK)
        waitForText("item-proposed-task-title", "Fix the flaky upload test")
        waitForText("item-proposed-task-where", "work · repo app")
        waitForText("item-proposed-task-plan", "Starts in plan mode")
        waitForText("item-reply", "Tell the secretary what to change…")
        waitForText("item-${InboxAction.APPROVE.wire}", "Start Task")
        compose.click("item-${InboxAction.APPROVE.wire}")
        compose.waitUntil(TIMEOUT) { item(NEW_TASK).task != null }
        waitForText("item-started-task", "Task started")
        compose.click("item-started-task-session")
        compose.waitUntil(TIMEOUT) { model.paneShown && model.currentSession?.taskId == item(NEW_TASK).task }
    }

    /** Beside a hand-off, Hand off and Start Task are both there; Start Task answers with `start-task`. */
    @Test
    fun startTaskBesideAHandoff() {
        launchOnInbox()
        openItem(CHAIN_ITEM)
        waitForText("item-${InboxAction.APPROVE.wire}", "Hand off to work-review")
        waitForText("item-${InboxAction.START_TASK.wire}", "Start Task")
        waitForText("item-proposed-task-title", "Add the error tracker SDK")
        compose.click("item-${InboxAction.START_TASK.wire}")
        compose.waitUntil(TIMEOUT) { item(CHAIN_ITEM).answer == InboxAction.START_TASK }
    }

    /** A plan item approves the plan with edits asked one by one, or keeps planning. */
    @Test
    fun aPlanItemApprovesThePlan() {
        launchOnInbox()
        openItem(PLAN)
        waitForText("item-${InboxAction.APPROVE.wire}", "Approve plan")
        waitForText("item-${InboxAction.REJECT.wire}", "Keep planning")
        waitForText("item-reply", "Keep planning & send…")
        compose.waitUntilAtLeastOneExists(hasTestTag("item-plan-note"), TIMEOUT)
        compose.click("item-${InboxAction.APPROVE.wire}")
        compose.waitUntil(TIMEOUT) { item(PLAN).answer == InboxAction.APPROVE }
    }

    /** No manual option on the screen: approve in the terminal, which Open Session shows. */
    @Test
    fun aStuckPlanOpensTheTerminal() {
        launchOnInbox()
        openItem(STUCK_PLAN)
        compose.click("item-${InboxAction.APPROVE.wire}")
        waitForText("approve-in-terminal-message", "Approve the plan in the Task's terminal")
        assertFalse(item(STUCK_PLAN).answer == InboxAction.APPROVE)
        compose.click("approve-in-terminal-open")
        compose.waitUntil(TIMEOUT) { model.paneShown && model.state.watchedPane == "%7" }
    }

    private companion object {
        const val TIMEOUT = 15_000L
        const val NEW_TASK = "i-0000f301"
        const val CHAIN_ITEM = "i-0000f001"
        const val PLAN = "i-0000f302"
        const val STUCK_PLAN = "i-0000f303"
    }
}
